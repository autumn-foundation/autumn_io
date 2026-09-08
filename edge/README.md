# Serving the docs from Cloudflare

The docs site's read path is pre-rendered at build time and served as static
files from Cloudflare. The origin — the Rust binary on Fly — keeps serving
everything that is not a fixed page.

```
                    ┌─────────── Cloudflare (static) ──────────┐
  reader ─────────► │  /  /docs/{slug}  /robots.txt  /sitemap  │
                    └──────────────────────────────────────────┘

                    ┌─────────────── Fly (origin) ─────────────┐
  reader ─────────► │  /search  /api/*  /mcp  /_stories  /health│
                    └──────────────────────────────────────────┘
```

Not a fallthrough — a split. Only the read path is routed to Cloudflare, so the
dynamic routes never arrive there and there is nothing to forward.

## Layout

| Path | What it is |
| --- | --- |
| `wrangler.toml` | the static-assets deploy config |
| `security-headers.json` | the headers the origin's middleware adds, which a CDN must add itself |

Everything else is in the Rust crate: `src/export.rs` renders the bundle and
`src/bin/build_site.rs` is the entry point.

## Deploying

```sh
cargo run --release --bin build_site        # renders dist/
cargo test --test static_export             # checks the bundle against the origin
npx wrangler deploy --config edge/wrangler.toml
```

The render is deliberately not a wrangler `[build]` command: it needs the Rust
toolchain, and having `wrangler deploy` die three minutes in with a cargo error
is worse than being told to run it first.

Routes are commented out in `wrangler.toml` until the DNS cutover. Until then a
`workers.dev` subdomain serves the same bundle for checking.

## What the bundle contains

`export_site` writes all of it, and `tests/static_export.rs` asserts each part
against what the origin actually does — so none of it can drift into
hand-maintenance:

| File | Replaces |
| --- | --- |
| `index.html`, `docs/{slug}/index.html` | the `index` and `docs_page` handlers |
| `robots.txt`, `sitemap.xml` | their handlers |
| `404.html` | `docs_page`'s not-found arm |
| `_headers` | `autumn-web`'s security middleware |
| `_redirects` | the `docs_index` handler's 307 to the first guide |
| `manifest.json` | route → file map, for anything that wants to inspect the bundle |

### Why `_headers` exists

`autumn-web`'s middleware stamps `x-frame-options`, `x-content-type-options`,
`referrer-policy` and `x-xss-protection` on every origin response. **A CDN
serving files runs none of that.** Without `_headers`, every statically served
page would carry four fewer protections than the same page from the origin — a
security regression, not a cosmetic difference.

`security-headers.json` is the single source of truth. `export_site` writes it
into `_headers`, and `the_headers_file_carries_exactly_what_the_origin_adds`
asserts it is exactly what the origin sends: no more (a fifth header added by a
framework upgrade would go unrestored) and no fewer (the bundle would be
inventing policy of its own).

### Why the bodies cannot drift

`export_site` and the route handlers call the same `site::render_*` functions in
the same binary, so an exported page is the origin's bytes by construction, not
by convention. `every_exported_page_is_what_the_origin_renders` pins that for
all 140 guides plus the home page, so a change that gave the exporter its own
rendering path would fail rather than ship a quietly different site.

## What this replaced

An earlier version of this directory ran the read path as an Autumn **edge
capsule** — the same handlers compiled to `wasm32-wasip1`, driven by a
hand-written Cloudflare Worker speaking Autumn's NDJSON wire protocol. It
worked, and it was measurably the wrong tool for this site.
[ADR-0001](../docs/adr/0001-static-docs-on-cloudflare.md) has the measurements
and the reasoning. The short version: static is zero CPU per request, needs no
`wasm32-wasip1` port of the framework's internals, and carries no risk of the
two lanes rendering differently, because there is only one renderer.
