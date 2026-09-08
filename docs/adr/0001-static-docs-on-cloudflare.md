# ADR-0001: Serve the docs read path as static files from Cloudflare

- **Status:** accepted
- **Date:** 2026-09-08
- **Supersedes:** nothing
- **Related:** [`edge/README.md`](../../edge/README.md) (the operational half),
  [`content/guide/edge.md`](../../content/guide/edge.md) (the capsule mechanism
  this ADR rejects, for a site it does not fit)

## Context

The docs site is one 256 MB scale-to-zero machine in `ord`. Every page it serves
is a pure function of the request: the guides are `include_str!`'d into the
binary, so a reader in Sydney waits on a trans-Pacific round trip — and, if the
machine has scaled to zero, on a cold start — to receive bytes that were fixed
at build time.

Every URL on the read path is known at build time. There are 140 guides, a home
page, `robots.txt` and `sitemap.xml`, and nothing about any of them varies by
request. `src/export.rs` has been able to render the whole set to `dist/` since
before this change; it was kept only as "a CDN/static-hosting bundle generator"
that nothing generated bundles for.

## Decision

Pre-render the read path and serve it from Cloudflare. Route only those paths to
the CDN; leave `/search`, the JSON API, `/mcp`, `/_stories` and `/health` on the
Fly origin, which they reach directly.

`export_site` gained the three things a static host needs that a running app did
in code: `404.html`, `_headers`, and `_redirects`. All three are generated from
the same sources the origin uses and asserted against the origin's own behaviour
in `tests/static_export.rs`, so none of them is hand-maintained.

### What the CDN has to be told, because no middleware and no router run

This is the part worth writing down, because getting it wrong is silent.

`autumn-web`'s middleware stamps `x-frame-options`, `x-content-type-options`,
`referrer-policy`, `x-xss-protection` and a `content-security-policy` on every
origin response. A CDN serving files runs none of it. Left alone, **every
statically served page would carry five fewer protections than the same page
from the origin** — a security regression, not a cosmetic difference.

The CSP nearly did get dropped, and how is worth recording. It was excused as
"volatile", an exception copied from
`autumn_edge::conformance::VOLATILE_HEADERS` — where it belongs, because a wasm
capsule structurally cannot emit one. A static bundle has no such constraint and
autumn-web's value is a constant with no per-request nonce, so the exception was
inherited reasoning that had stopped applying the moment the capsule was
dropped.

`edge/security-headers.json` is the single source of truth. The exporter writes
it into `_headers`, and the test asserts it is exactly what the origin emits: no
more (a sixth header from a framework upgrade would go unrestored) and no fewer
(the bundle would be inventing policy of its own). The same reasoning covers
`/docs`, which the origin answers with a 307 from a handler and the CDN answers
from `_redirects` — matched on target *and* status.

### And the assets should come too — but not yet (#51)

`/static/*` is **not** routed to the CDN in this decision, and the route in
`edge/wrangler.toml` is commented with a warning rather than merely commented
out. This section records why it is wanted and why it is deferred, because the
two are easy to conflate at cutover time.

Wanted: every page links `/static/css/site.css`, which is render-blocking. With
the pages on the CDN and the assets on Fly, a docs visit still wakes the
scale-to-zero origin and still blocks the render on its cold start — which is
most of what this decision exists to avoid.

Deferred: `/static/` is a namespace with two owners. This repo's `static/` tree
is one; `autumn-web` is the other, serving seven paths from memory with no file
on disk anywhere here. The bundle contains exactly one of those seven — htmx,
and only because a docs page links it, so
`no_exported_page_references_a_missing_asset` caught it. Handing the whole
prefix to that bundle turns the other six into CDN 404s and breaks `/_stories`,
which `autumn.toml` deliberately exposes publicly.

Exporting the missing six would close the instance and leave the cause: the set
is owned by a dependency and moves with its feature flags and version, so a
hand-list is a future outage rather than a fix. **#51** carries the options; the
likely answer is a small fallback script on the assets deployment, so a bundled
path is served from the CDN and anything else proxies to the origin — safe by
construction rather than by enumeration, and it demotes the missing-asset guard
from a correctness check to a performance one.

Until then the four page routes stand on their own, and the assets keep being
served by Fly exactly as they are today. The deferral costs the asset half of
the latency win, not correctness.

## What we tried first, and why it lost

The read path initially ran as an Autumn **edge capsule**: the same handlers
compiled to a `wasm32-wasip1` artifact, driven by a hand-written Cloudflare
Worker speaking Autumn's NDJSON wire protocol on the capsule's stdio. It worked
— every route, every fallthrough channel, credential stripping and determinism
all verified against the real artifact. It was still the wrong tool here, for
three reasons that only became clear once it was built and measured.

### It was not faster where it mattered

Measured on the artifact under Node 22, median of five, one instantiation per
request:

| Route | Median CPU |
| --- | --- |
| `/robots.txt` | 3 ms |
| `/sitemap.xml` | 12 ms |
| `/` | 102 ms |
| `/docs/jobs` | 143 ms |
| `/docs/deployment` | 170 ms |
| `/docs/getting-started` | 232 ms |

About 90 ms of that is `SyntaxSet::load_defaults_newlines` decoding syntect's
bundled grammars, paid on every instantiation because a Worker cannot park a
returned `_start` between requests.

Against an origin already answering in under 100 ms for a reader near `ord`, a
cache *miss* on the capsule is a regression. The capsule only wins on a cache
hit — where it does not run at all — which is to say the win belonged to the
cache, not the capsule. Static is zero CPU per request on hit *and* miss.

### It forced a port of the framework's internals

An `#[edge]` module compiles for `wasm32-wasip1`, where `autumn-web` is absent
from the dependency graph. That meant reimplementing its Markdown frontmatter
parser and its active-search widget inside this repo — 181 lines of logic plus a
196-line parity test whose whole job was to prove the copies had not drifted from
the originals.

Neither port needed anything a capsule lacks. The parser is TOML and string
slicing; the widget is maud markup. They were unavailable only because they live
in a crate that also contains tokio and diesel. That is a framework packaging
gap, not a cost inherent to edge capsules — but it was a real cost here, and
static needs none of it, because the exporter runs *inside* the origin binary.

### It created a divergence risk that only existed because of it

Oniguruma is a C library with no `wasm32-wasip1` sysroot, so the capsule had to
highlight with `fancy-regex` while the origin kept `onig`. The two engines
render all 140 guides identically today — that was verified before committing to
it — but keeping it true needed a conformance suite comparing both lanes on
every build.

The exporter renders in the origin binary with `onig`. There is one renderer, so
there is nothing to diverge, and the whole apparatus is unnecessary.

## Consequences

**Good.**

- Zero CPU per request, so the pages are faster everywhere — including for
  readers near `ord`, where the capsule was not. Their assets still come from
  Fly until #51; see above.
- The origin sees only dynamic routes *and static assets*, so it still scales to
  zero more often than before, though not as often as it will once #51 moves
  `/static/*` across.
- Substantially less to own: no 5.2 MB wasm artifact, no ~700-line WASI shim and
  Worker, no ports, no parity or conformance suites, no dependency on an upstream
  surface that is explicitly outside the stability policy.
- Exported bytes are the origin's bytes by construction, not by convention.

**Costs.**

- A content change means re-rendering and re-uploading the bundle, not restarting
  a server. That is a deploy step to remember; CI renders it on every PR so a
  broken export fails there first.
- A new *route type* needs a line in the exporter. New guides do not — they come
  from the registry automatically.
- Two places serve autumn-web.app, so the routing split has to stay correct. It
  is four route patterns today — five once #51 lands — and they are in one file.
- The bundle must contain every asset its pages link, including ones the
  framework serves from memory rather than from disk. That is a test rather
  than a convention.

## Also in this change, and independent of it

**Guide rendering became lazy.** `DocRegistry` used to render all 140 guides the
first time anything touched it — which, on a scale-to-zero machine, is during the
first request after a cold start. A reader arriving on a cold machine paid ~3.4
billion instructions to be shown one page. Frontmatter is still parsed for the
whole corpus, but a page's HTML is now rendered on first use and memoized.

This was originally forced by the capsule's fuel budget. It outlived the capsule
because it is a straightforward improvement to the origin, and the static export
is unaffected either way — it walks the whole registry and pays the same total.

## Alternatives considered

**The edge capsule.** Built, measured, rejected; see above.

**Cloudflare as a plain caching proxy, no pre-render.** Simplest possible change
and it would deliver most of the latency win. Rejected because a cache miss still
crosses the Pacific and can still hit a cold start, which is the case that hurts
most — and because pre-rendering costs nothing extra once the exporter exists.

**`#[static_get]`.** The framework's own pre-render attribute, which is the same
idea expressed in the router. Worth revisiting: it would move the URL list out of
`export.rs` and into the routes themselves. Not done here because `export_site`
already worked and this change was large enough.
