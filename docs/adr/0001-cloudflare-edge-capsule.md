# ADR-0001: Run the docs read path on a Cloudflare edge capsule

- **Status:** accepted
- **Date:** 2026-09-08
- **Supersedes:** nothing
- **Related:** [`content/guide/edge.md`][guide] (upstream design, issue #1790),
  [`edge/README.md`](../../edge/README.md) (the operational half)

[guide]: ../../content/guide/edge.md

## Context

The docs site is one 256 MB scale-to-zero machine in `ord`. Every page it serves
is a pure function of the request: the guides are `include_str!`'d into the
binary, so a reader in Sydney waits on a trans-Pacific round trip — and, if the
machine has scaled to zero, on a cold start — to receive bytes that were fixed
at build time.

Autumn 0.7.0 shipped **edge capsules**: the same handler source, compiled to a
portable `wasm32-wasip1` artifact a CDN can run, with the origin staying the
authority and the fallback. That is a good fit for a site whose entire read path
is deterministic rendering of embedded content.

Autumn deliberately ships no CDN binding — an artifact and a documented NDJSON
protocol, so no framework release is coupled to a vendor's SDK cadence. The
vendor half is ours to write.

## Decision

Run five routes — `/`, `/docs`, `/docs/{slug}`, `/robots.txt`, `/sitemap.xml` —
in a capsule behind a Cloudflare Worker, with everything else going straight to
the origin.

Four things had to be true first, and each one shaped the design.

### 1. The render pipeline had to stop naming `autumn-web`

An `#[edge]` module compiles for `wasm32-wasip1`, where `autumn-web` is not in
the dependency graph at all. The pipeline — `docs`, `site`, `seo` — used it in
three places: the Markdown registry, the active-search widget, and the maud
re-exports.

The maud re-exports were free (`maud` was already a direct dependency, and it is
the same crate). The other two are now ports: `src/frontmatter.rs` and
`src/widgets.rs`.

**A port is only as good as its parity check.** These decide what every page's
title and ordering are, and what the docs sidebar looks like on every page — and
because *both* lanes render through the ports, a drift would be wrong in both at
once and invisible to any origin/edge comparison. So `tests/port_parity.rs`
checks them against `autumn-web` itself on the native target: the whole 140-guide
corpus through both frontmatter parsers, and the widget markup byte-for-byte.
That test immediately earned its place by catching a CRLF bug in the port that
the entire LF corpus could never have exposed.

### 2. Rendering had to become lazy

`DocRegistry::from_sources` rendered all 140 guides eagerly. At the origin that
is a startup cost amortised over the machine's lifetime, and paying it up front
is what makes a request a HashMap lookup.

A capsule has no lifetime to amortise over. It is instantiated, serves one
request, and is dropped, and the reference host meters each request against
10⁹ instructions of fuel. Rendering the corpus costs ~3.4 billion. **Every single
request would have exhausted the budget, trapped, and fallen through** — the edge
lane would have served nothing, ever.

So frontmatter is parsed for every guide (the sidebar, the sitemap and the
ordering need it, and it is cheap) and a page's HTML is rendered on first use and
memoized. The origin pays the same total, spread across the first request to each
guide instead of startup.

### 3. The two lanes use different regex engines

The origin highlights with syntect's Oniguruma backend, which is 3.8× faster on
this corpus than the pure-Rust alternative and is why [issue #19] chose it.
Oniguruma is C, and building it for `wasm32-wasip1` needs a WASI sysroot no Rust
toolchain ships. The capsule therefore uses `fancy-regex`.

Two highlighters is exactly the kind of thing that quietly produces
*almost*-identical pages. Before committing to it, all 140 guides were rendered
under both engines and hashed: **byte-identical, every page**. That made it a
sound decision rather than a hopeful one — and `tests/edge_conformance.rs` now
re-checks it on every build, so a syntect release or a new code fence that made
them disagree shows up as a named divergence rather than a subtly wrong colour in
production.

[issue #19]: ../plans/2026-09-02-syntect-regex-backend.md

### 4. The origin's security headers had to be restored

This one the conformance test found, and it is the reason to have written it.

`autumn-web`'s middleware stamps `x-frame-options`, `x-content-type-options`,
`referrer-policy` and `x-xss-protection` on every origin response. **A capsule
runs no middleware — it serves exactly what the handler produced.** Left alone,
every page served from the edge would have carried four fewer protections than
the same page from the origin. That is a security regression, not a cosmetic
difference.

`edge/security-headers.json` is now the single source of truth: the Worker stamps
it on every response it serves, and the conformance test asserts the origin emits
*exactly* that list — no more (a fifth header would go unrestored) and no fewer
(the shim would be inventing headers). A framework upgrade that changes the set
fails the test and names the file to edit.

## Cache entries are keyed by the artifact, and `HEAD` is never stored

Both of these came out of review, and both are invisible from any single
request.

`caches.default` survives Worker deployments. Keyed on the request URL alone, a
warmed colo would go on serving the previous deploy's response — and a cached
404 would hide a newly published guide. Every entry is therefore keyed on a
deployment version.

Getting the *scope* of that version right took a second round. Hashing the
capsule alone still left a warmed colo serving stale headers after a deploy that
changed only `security-headers.json`, because that produces a byte-identical
`.wasm`. The version now covers everything that decides a served byte — the
capsule, the Worker sources, the header list and `wrangler.toml`
(`edge/compute-version.sh`). Hashing inputs rather than stamping a timestamp
keeps it exact in both directions: a rebuild that changes nothing keeps its warm
cache.

The stored copy carries a one-day `s-maxage`, stripped again on the way out so a
hit serves exactly what a miss serves — `cache.match()` returns the response as
stored, so without that a cache hit would have told browsers to hold the page,
cached 404s included, for a day no purge could shorten.

`HEAD` is not stored at all. Cloudflare's Cache API is `GET`-only, but the real
hazard is upstream of that: axum routes `HEAD` to the `GET` handler and strips
the body, so the capsule answers with zero body bytes and the `GET`'s
`content-length`. Cached under a key a `GET` would later read, that is a
truncated page claiming to be the whole one. A `HEAD` still *reads* the cache —
the key is a synthetic `GET` — with the body stripped on the way out.

## The KV seam, and why a snapshot

`EdgeCache::get` is synchronous inside a running handler. Every CDN key/value API
is asynchronous, and a synchronous WASI `fd_read` cannot await a promise, so
answering `kv_get` on demand inside a Worker is not possible.

The shim resolves the whole replica snapshot *before* the guest starts and
answers from it inline. This is not a weakening of the contract: `EdgeKv`
documents a replica-local, opportunistic, possibly-stale read where a miss is
always legal, which is precisely what a snapshot is.

It is also unused. This site declares no `needs(...)` capability, and a test
asserts that — adding `#[edge(needs(kv))]` to a route would silently make every
request to it fall through in production.

## Consequences

**Good.**

- Reads are served from the reader's nearest colo, and the Cache API means the
  capsule usually does not run at all.
- The origin machine sees only `/search`, the JSON API, `/mcp` and static
  assets, so it scales to zero more often, not less.
- One handler source. There is no second implementation of the docs site to keep
  honest, and the compiler refuses a handler the edge could not serve.
- The whole edge path is testable here: `node --test` drives the real artifact
  with no wrangler, no miniflare and no network.

**Costs, honestly.**

- **An uncached page costs 100–230 ms of CPU**, most of it decoding syntect's
  bundled grammars on every instantiation (see `edge/README.md` for the
  measurements). The Cache API makes that a per-page-per-colo cost, but it is
  real, and it means this needs a paid Workers plan. Trimming the syntax-set dump
  to the languages the guides actually use is the obvious next step; it changes
  what an unrecognised language renders as, so it wants its own change and its
  own conformance run.
- **A 5.2 MB artifact** is bundled into the Worker. Most of it is the syntax and
  theme dumps.
- **One instantiation per request.** `autumn_edge::serve` loops until stdin EOF,
  so a host *may* pipe many requests through one process — but the dialogue is
  synchronous and a returned `_start` cannot be parked between requests. Only the
  compiled module is reused across the isolate's lifetime.
- **The edge lane is experimental upstream.** `#[edge]`, `autumn-edge`, and wire
  version 1 are outside the stability policy. A version mismatch degrades to
  origin-serving rather than guessing, which is the right failure mode, but an
  Autumn upgrade may require reworking this.

## Alternatives considered

**Pre-render to `dist/` and serve static files from the CDN.** The exporter
already exists and this is strictly cheaper per request — no wasm, no CPU. It is
also the thing `#[static_get]` is for, and the guide says outright that it beats
a capsule where it applies. Rejected because it makes the CDN a separate
deployment artifact with its own staleness story, and because the capsule keeps
one code path: a route added to `src/edge.rs` is live in both lanes with no
export step to remember. Worth revisiting if the CPU cost above becomes a
problem.

**Put Cloudflare in front as a plain caching proxy, no capsule.** Simplest
option, and it would deliver most of the latency win. Rejected because a cache
miss still crosses the Pacific and can still hit a cold start, which is the case
that hurts most.

**Use `@cloudflare/workers-wasi` instead of a hand-rolled WASI host.** Rejected:
the capsule needs eight syscalls and stubs for the rest, the reference host in
`autumn-edge` is a readable specification to implement against, and a
dependency-free shim is one fewer thing between a security claim and its proof.
`wasi.js` is ~300 lines including the comments.

**Keep rendering eager and raise the fuel budget.** Not available — the budget is
the *host's*, and on Cloudflare the equivalent limit is CPU time we do not
control. Lazy rendering was not an optimisation here; it was the precondition.
