# ADR-0001: Cache the docs at the edge, on the origin's own terms

- **Status:** accepted
- **Date:** 2026-09-08
- **Supersedes:** an earlier draft of this file that proposed serving `dist/`
  from Cloudflare Workers Static Assets, and before that an Autumn edge capsule.

## Context

The site is one scale-to-zero Fly machine in `ord`. Every reader anywhere pays
a round trip to Chicago, and a cold start on top if the machine is asleep, for
pages whose bytes were fixed at build time: 140 guides, a home page,
`robots.txt`, `sitemap.xml`. None of them vary by request.

The domain is already proxied through Cloudflare, so there is already a CDN in
front of the origin. The question was never "how do we get a CDN" — it was why
the CDN was not helping.

Measuring the origin's actual response headers answered it:

| Response | `Cache-Control` | `ETag` |
| --- | --- | --- |
| `/static/*?v=…` | `public, max-age=31536000, immutable` | yes |
| `/static/*` (unversioned) | `public, max-age=3600, must-revalidate` | yes |
| `/`, `/docs/{slug}` | **none** | yes (weak) |

The assets were already right. `build.rs` hashes the whole static tree into
`ASSET_VERSION`, `site::versioned_asset_path` stamps it as `?v=`, and
`cache_static_assets` marks those URLs `immutable` — query-string fingerprinting,
which Cloudflare keys on. That work predates this change and needed nothing.

The pages carried no cache policy at all. A CDN with no TTL and no instruction
does not cache HTML, so every page view — every reader, every navigation —
travelled to `ord` and woke the machine to re-render bytes identical for
everyone.

## Decision

**Give the pages a shared-cache policy and let the existing proxy do its job.**

```
Cache-Control: public, max-age=0, s-maxage=3600, must-revalidate
```

- `s-maxage=3600` is for the shared cache: Cloudflare holds the page for an
  hour and serves it without touching the origin.
- `max-age=0, must-revalidate` keeps browsers asking, which costs nothing
  because `EtagLayer` already answers with a `304` and never re-renders.
- An hour bounds a missed purge. A deploy should purge; if one does not, the
  site self-heals rather than serving yesterday's docs indefinitely.

A missing guide gets the same shape with `s-maxage=60`: worth caching so a bad
link does not wake the origin repeatedly, but briefly, because a cached `404`
outlives the deploy that adds the guide it denies.

`/search` is marked `no-store`. It renders two different bodies at one URL — an
htmx fragment for `HX-Request`, a full page otherwise — and no CDN puts a
request header in its cache key. Declaring `Vary: HX-Request` is not a fix
(Cloudflare does not honour a custom `Vary` for HTML); not storing it is.

## Alternatives rejected

**Cloudflare Workers Static Assets serving a pre-rendered `dist/`.** Built and
working before this decision. Its one real advantage over proxy caching is zero
origin contact on a cache *miss*, which with tiered caching is a handful of
upper-tier fetches per purge cycle — bounded, and not worth what it cost:

- a `_headers` file to restore the five security headers, because a static host
  runs none of the origin's middleware, plus a both-directions test to keep it
  honest;
- a `_redirects` file for a 307 the router already performs;
- a pre-rendered `404.html` for an arm the handler already renders;
- htmx written into the bundle, because the framework serves it from memory;
- `/static/*` left unroutable, because that prefix has two owners — this repo's
  tree and seven assets `autumn-web` serves from memory — so routing it would
  have 404'd the story gallery's dependencies;
- a compile-time `include_str!` of the header list, which broke the production
  Docker build in a way no CI job could see.

Every item is machinery to reconstruct, in a static bundle, something the origin
already does correctly. Deleting it removed ~700 lines and one open issue.

**An Autumn edge capsule** (`wasm32-wasip1` + a hand-written Worker). Measured
at ~150 ms CPU per guide, ~90 ms of it syntect decoding grammars on every
instantiation, against an origin already answering in under 100 ms near `ord`.
It also forced porting framework internals into this repo and highlighted with a
different regex engine than the origin, creating a divergence risk that existed
only because of it.

## Consequences

- A docs page is served from the reader's nearest colo, and the origin sees one
  request per colo per hour instead of one per reader.
- **A deploy should purge the Cloudflare cache** or wait up to an hour. This is
  the one operational obligation the change adds.
- The origin still serves `/search`, `/api/*`, `/mcp`, `/_stories` and
  `/health`, so the machine still wakes — just not for reading documentation.
- Nothing about the deployment artifact changes. There is no second thing to
  build, deploy, or keep in sync with the origin.

## Configuring Cloudflare

Two settings, both in the dashboard; neither can be committed here.

1. **A Cache Rule making the read path eligible for cache.** Cloudflare's
   default cache level caches by file extension and bypasses HTML no matter
   what the origin sends, so `s-maxage` alone does nothing. Match
   `http.request.uri.path eq "/" or http.request.uri.path matches "^/docs"`
   plus `/robots.txt` and `/sitemap.xml`, set *Cache eligibility: Eligible for
   cache*, and *Edge TTL: Use cache-control header from origin* so the policy
   stays in this repo rather than splitting across two places.

   Do **not** write a rule that caches everything: `/search`, `/api/*`, `/mcp`
   and `/actuator/*` must keep reaching the origin. `/search` sends `no-store`,
   but an "ignore origin cache-control" TTL override would defeat that.

2. **A purge on deploy.** One authenticated `POST` to the zone's
   `purge_cache` endpoint after a successful release, so new guides appear
   immediately instead of within the hour.

Verify with `curl -sI https://autumn-web.app/docs/getting-started` and look for
`cf-cache-status: HIT` on the second request.
