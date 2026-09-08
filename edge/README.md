# The edge lane

The docs site's read path runs in two places. The origin — the Rust binary on
Fly — serves everything. A Cloudflare Worker in front of it runs the same
handlers, compiled to `wasm32-wasip1`, and forwards anything it cannot answer
to the origin unchanged.

```
                    ┌───────────────── Cloudflare ─────────────────┐
  reader ─────────► │  cache hit? ──► served                       │
                    │      │                                       │
                    │      └─► edge-capsule.wasm ──► served + cached│
                    └──────────────────┬───────────────────────────┘
                                       │ fallthrough
                                       ▼
                    ┌───────────────── Fly (origin) ───────────────┐
                    │  the whole app: /search, /api, /mcp, /static  │
                    └──────────────────────────────────────────────┘
```

Autumn ships the artifact and the protocol, not a vendor binding
([`content/guide/edge.md`][guide] § "Deploying"), so `worker/` is the vendor
half: about 700 lines of dependency-free JavaScript that loads the `.wasm`,
speaks the NDJSON dialogue on its stdio, and forwards declines upstream.

[guide]: ../content/guide/edge.md

## Layout

| Path | What it is |
| --- | --- |
| `build-capsule.sh` | builds the capsule and stages it for wrangler |
| `compute-version.sh` | the deployment version: a hash of everything that decides a served byte |
| `worker/src/wire.js` | wire protocol v1: frames, base64, header canonicalization |
| `worker/src/wasi.js` | a minimal `wasi_snapshot_preview1` host over in-memory stdio |
| `worker/src/capsule.js` | the dialogue driver — request in, outcome out, runtime-agnostic |
| `worker/src/cache.js` | colo cache policy: the versioned key, what may be stored, HEAD handling |
| `worker/src/index.js` | the Worker: cache, capsule, origin fallthrough |
| `worker/test/` | `node --test`, driving the real artifact |
| `worker/wrangler.toml` | deploy configuration |

## Deploying

```sh
./edge/build-capsule.sh                 # cargo build --target wasm32-wasip1 --release
cd edge/worker && npm test              # drives the artifact you just built
npx wrangler deploy
```

The build step is deliberately not a wrangler `[build]` command: it needs the
Rust toolchain and the `wasm32-wasip1` target, and having `wrangler deploy` die
three minutes in with a cargo error is worse than being told to run it first.

Routes are commented out in `wrangler.toml` until the DNS cutover. Until then
`wrangler dev` and a `workers.dev` subdomain exercise the whole path against the
real origin.

## What the edge serves

Five routes, all of them `#[edge]` in [`src/edge.rs`](../src/edge.rs):

| Route | Notes |
| --- | --- |
| `/` | home page |
| `/docs` | 307 to the first guide |
| `/docs/{slug}` | one guide; an unknown slug is the site's own rendered 404 |
| `/robots.txt` | |
| `/sitemap.xml` | |

Everything else — `/search`, `/api/*`, `/mcp`, `/_stories`, `/static/*`,
`/health` — is not routed to the Worker at all and reaches the origin directly.
The capsule would decline them correctly; a hop that can only ever forward is a
hop worth not making.

## What a request costs

Measured on the staged artifact under Node 22, median of five, one
instantiation per request:

| Route | Median | Body |
| --- | --- | --- |
| `/robots.txt` | 3 ms | — |
| `/sitemap.xml` | 12 ms | 10 KB |
| `/` | 102 ms | 13 KB |
| `/docs/jobs` | 143 ms | 108 KB |
| `/docs/deployment` | 170 ms | 329 KB |
| `/docs/getting-started` | 232 ms | 156 KB |

Compiling the module costs 13 ms and happens once per isolate, not per request.

Two things dominate the rest, and both are consequences of one instantiation per
request:

- **The syntax set.** `SyntaxSet::load_defaults_newlines` decodes syntect's
  bundled grammars, which is roughly the 90 ms separating `/sitemap.xml` (no
  code blocks) from `/` (two). It is paid on every page carrying a code fence.
  Trimming the dump to the handful of languages the guides actually use would
  cut most of it, and is the obvious next optimisation — but it changes what an
  unrecognised language renders as, so it needs its own change and its own
  conformance run.
- **The page render.** Markdown plus highlighting for one guide.

This is why [`DocPage`](../src/docs.rs) renders lazily. Rendering the whole
corpus — which is what the origin does at startup, and what the code did before
this — costs about 3.4 billion instructions; at the rates above that is tens of
seconds per request, and the reference host's fuel budget is 10⁹ instructions.
The edge lane is not viable at all without the lazy render.

The Cache API is what makes the numbers above a per-page-per-colo cost rather
than a per-request one. The content changes only on deploy.

### Cache keys are versioned by the deployment

`caches.default` survives Worker deployments. An entry keyed on the request URL
alone would let a warmed colo serve the previous deploy's response indefinitely
— and a cached 404 would hide a newly published guide from every colo that had
already answered for that slug.

So every entry is keyed on a deployment version, which `compute-version.sh`
computes and `build-capsule.sh` writes to `worker/build/capsule-version.js`. It
hashes **everything that decides a served byte**:

| Input | Why |
| --- | --- |
| the capsule | the guides are embedded in it, so it decides the body |
| `worker/src/*.js` | the shim stamps headers, filters them, and decides what may be cached |
| `security-headers.json` | the list of headers the shim restores on every response |
| `wrangler.toml` | the origin a fallthrough goes to, and the routes bound |

The capsule alone was the first version of this, and it left a hole worth
naming: a deploy changing only `security-headers.json` produces a
byte-identical `.wasm`, so every warmed colo would have kept serving the
previous headers with the new Worker logic never running. The headers are the
case that matters most here, which is exactly why the version cannot be blind to
them.

Hashing inputs rather than stamping a timestamp is what makes it exact in both
directions: a rebuild that changes nothing keeps its warm cache, and any change
that can alter a served byte invalidates it. Contents are hashed, never paths,
so a laptop and CI agree. `test/version.test.js` drives each of those four
inputs.

The stored copy also carries a one-day `s-maxage`, so unreachable entries age
out rather than sitting in the cache forever. Keeping that off the reader's
response takes two things, because `cache.match()` returns what was *stored*,
header included: `withoutStorageTtl` strips it on the way out so a hit and a
miss serve identical headers, and `s-maxage` rather than `max-age` means that
even if some future path forgot to strip it, browsers would ignore it. The
origin sends no `Cache-Control` on a docs page — it revalidates with an `ETag` —
and the edge lane must not quietly start pinning pages in browsers, where no
purge can reach them.

### `HEAD` is served from the cache but never stored

Cloudflare's Cache API is `GET`-only for `match` as well as `put`, so the key is
always a synthetic `GET` and a `HEAD` reads the `GET` representation with the
body stripped on the way out.

Storing a `HEAD` is refused for a sharper reason than the API restriction:
axum's `MethodRouter` routes `HEAD` to the `GET` handler and strips the body, so
the capsule answers a `HEAD` with **zero body bytes and the `GET`'s
`content-length`**. Cached under a key a `GET` would later read, that is a
truncated page claiming to be the whole one.

## The KV seam

The site declares no `needs(...)` capability: every guide is embedded in the
artifact, so no route needs anything the host has to mediate.
`tests/edge_conformance.rs` asserts that, because adding `#[edge(needs(kv))]` to
a route would silently make every request to it fall through in production.

The shim supports `kv` anyway, and the way it does so is worth writing down.
`EdgeCache::get` is **synchronous** inside a running handler; every CDN KV API is
asynchronous. There is no way to suspend a synchronous WASI `fd_read` to await a
promise, so answering `kv_get` on demand is not possible in a Worker. Instead
the shim resolves the whole replica snapshot *before* the guest starts and
answers from it inline.

That is not a weakening of `EdgeKv`: the trait documents a replica-local,
opportunistic, possibly-stale read where a miss is always legal, and a snapshot
is exactly that. To use it, bind a namespace as `EDGE_KV` holding a JSON object
of key → base64 bytes under `EDGE_KV_SNAPSHOT_KEY` (default `edge-snapshot`).
Without the binding the Worker advertises no capabilities and routes needing
`kv` fall through to the origin, which has the real cache.

## Testing

`worker/test/` runs under plain `node --test` — no wrangler, no network, no
miniflare — because the protocol is NDJSON over stdio and the WASI shim is
in-memory:

- **`wire.test.js`** pins the frame encoding against the JSON `autumn_edge`'s
  own tests assert.
- **`capsule.test.js`** drives the real artifact: every route, every fallthrough
  channel, the import allowlist, credential stripping, and two runs of the same
  request compared byte-for-byte.
- **`routes.test.js`** checks the Worker's path pre-filter against the capsule's
  actual router, one path at a time. It has already caught one bug — `/docs/`
  with an empty slug, which the filter accepted and `matchit` does not match.
- **`cache.test.js`** pins the cache policy. None of what it covers is visible
  from a single request: the versioned key needs two deployments to go wrong,
  the `HEAD` corruption needs a `HEAD` followed by a `GET`, and the leaked
  storage TTL needs a cache hit.
- **`version.test.js`** drives `compute-version.sh` over a throwaway copy of
  `edge/`, asserting the version moves for a change to any of its four inputs
  and stays put when nothing changes.

On the Rust side, [`tests/edge_conformance.rs`](../tests/edge_conformance.rs) is
the byte-identity proof, and [`tests/port_parity.rs`](../tests/port_parity.rs)
pins the two `autumn-web` pieces the edge lane had to port.

## Security

The claims in the guide's § "Security posture" hold here, and two of them are
asserted rather than assumed:

- **No ambient authority.** `capsule.test.js` checks the artifact's import list
  against what `wasi.js` provides, and separately that it imports nothing
  outside `wasi_snapshot_preview1`. There is no filesystem, no sockets, no host
  functions — the only way out is the dialogue.
- **Credentials never reach the capsule.** `wire.js` strips `cookie`,
  `authorization` and `proxy-authorization` before framing, the guest strips
  them again, and a test asserts a request carrying them serves identical bytes
  to one that does not. A fallthrough still forwards the *original* request, so
  the origin sees what the client sent.
- **No cookies from the edge.** The runtime declines any capsule response
  carrying `set-cookie`; the Worker drops the header too, as defence in depth.

The artifact is a program, not an asset. It lives in `edge/worker/build/`, which
is gitignored, and it must never be published under `static/` or `dist/` — that
would serve the capsule to browsers instead of running it at the edge.
