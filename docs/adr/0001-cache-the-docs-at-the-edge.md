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

**Until the Cache Rule below exists, everything above is inert.** Cloudflare's
default cache level decides eligibility by file extension and bypasses HTML no
matter what the origin sends, so the `s-maxage` this change adds is read by
nobody. The code is necessary and not sufficient; this section is the rest.

Neither step can be committed here, which is exactly why they are written down
in this much detail.

### 1. The Cache Rule

**Caching → Cache Rules → Create rule.** Build a custom rule rather than
starting from a template — the templates are shaped around bypassing cache for
admin panels and carts, and none of them fits "cache this handful of paths on
the origin's terms".

Name it something a stranger can act on: `Docs read path — cacheable`.

**Expression.** Use *Edit expression* and paste this rather than assembling it
in the visual builder:

```
(http.request.uri.path eq "/")
or (http.request.uri.path eq "/docs")
or (starts_with(http.request.uri.path, "/docs/"))
or (http.request.uri.path eq "/robots.txt")
or (http.request.uri.path eq "/sitemap.xml")
```

It is an **allow-list of exact paths plus one prefix**, deliberately. `/docs`
appears on its own line because it is the 307 to the first guide, which is a
different response from anything under `/docs/`. And it is `starts_with(…,
"/docs/")` rather than `matches "^/docs"` because the regex form would also
match a future `/docsearch` — there is no such route today, but an allow-list
that can accidentally widen is not an allow-list.

**Then the settings.** The form offers a long list of them. **Three decide
whether this works. Leave every other one at its default.**

| Setting | Set it to | Why |
| --- | --- | --- |
| Cache eligibility | **Eligible for cache** | The whole point. This is what overrides the default "bypass HTML" behaviour. |
| Edge TTL | **Use cache-control header if present, bypass cache if not** | Makes the origin's `s-maxage=3600` the source of truth, so the policy stays in this repo. The *bypass if not* half is a fail-safe: if a response ever arrives with no policy, Cloudflare declines to cache rather than inventing a TTL. |
| Browser TTL | **Respect origin TTL** | Keeps `max-age=0, must-revalidate` intact so browsers revalidate against the `ETag`. An override here would pin pages in readers' browsers, where no purge can reach them — the one failure mode with no remedy. |

Dashboard labels shift between Cloudflare UI revisions; match on meaning rather
than exact wording. Anything not named above — Cache Reserve, Cache Deception
Armor, cache-key components, origin error handling — is already correct at its
default for this site, and changing one is more likely to break the rule than
improve it. Two worth understanding rather than touching:

- **Cache key / query string.** The default includes the query string, which is
  what makes `?v=` asset fingerprinting work. This rule does not match
  `/static/*` anyway (see below), but do not switch on "ignore query string" at
  the zone level for the same reason.
- **Serve stale while revalidating.** Optional, and defensible here: it would
  keep serving a guide during an origin cold start. The trade is that a reader
  can get a page one revalidation-cycle out of date. The default (off) is the
  conservative choice; turning it on is a preference, not a fix.

### 2. What the rule must not match

`/search`, `/api/*`, `/mcp`, `/actuator/*`, `/health` and `/_stories` all have
to keep reaching the origin. The expression above cannot match them, which is
the reason it is written as an allow-list.

The specific thing not to do is a **"Cache Everything" rule scoped to the whole
zone**. Two of those paths break loudly under one:

- `/search` returns a different body for the same URL depending on the
  `HX-Request` header, which is not in any cache key. It sends `no-store` to
  say so, but an **"Ignore cache-control header and use this TTL"** Edge TTL
  setting overrides exactly that, and a cached htmx fragment then gets served
  into a full page load.
- `/actuator/prometheus` would report a frozen snapshot as though it were
  current.

### 3. `/static/*` needs no rule at all

It is already cached, and was before this change. Cloudflare caches those
extensions by default, and the origin marks versioned asset URLs
`public, max-age=31536000, immutable`. Adding a rule for them is not
necessary and gives you a second place to get it wrong.

### 4. The zone-level setting that can silently defeat this

**Caching → Configuration → Browser Cache TTL.** If this is set to a fixed
duration it overrides origin headers for browsers zone-wide, regardless of what
the Cache Rule says. Set it to **Respect Existing Headers**. Otherwise readers
hold pages for that duration and a purge cannot reach them.

### 5. Leave Web Analytics off

**Analytics & Logs → Web Analytics.** Cloudflare's automatic setup injects a
beacon script into every HTML response as it passes through the proxy. The
origin sends `script-src 'self'`, so the browser refuses it and every page load
logs a console error:

```
Loading the script 'https://static.cloudflareinsights.com/beacon.min.js/…'
violates the following Content Security Policy directive: "script-src 'self'".
```

This is the CSP working, not failing — an injected third-party script is exactly
what it exists to stop. It is left disabled, because the numbers that matter for
*this* decision do not come from the beacon: requests, cache hit ratio and
bandwidth are **zone analytics**, measured server-side by the proxy, and they
arrive whether or not any script runs in the browser. The beacon adds Core Web
Vitals and page views on top of that.

Enabling it means overriding `security.headers.content_security_policy` in
`autumn.toml` to allow `static.cloudflareinsights.com` in `script-src`, plus
whatever host the beacon reports to in `connect-src` — widening only `script-src`
silences the console error while the beacon still collects nothing. The real cost
is not the trust (Cloudflare terminates TLS for this zone and can already inject
anything into the page) but the pinning: `autumn.toml` has no "append a host", so
overriding means writing the whole policy here and no longer inheriting
improvements to the framework's default.

If that trade is ever taken, add a test asserting the policy still denies
everything else. Nothing guards the CSP today, because it is the framework
default and there was nothing to guard.

### 6. Purge on deploy

Without this, a new guide takes up to an hour to appear. One call after a
successful release:

```sh
curl -fsS -X POST \
  "https://api.cloudflare.com/client/v4/zones/$CLOUDFLARE_ZONE_ID/purge_cache" \
  -H "Authorization: Bearer $CLOUDFLARE_PURGE_TOKEN" \
  -H "Content-Type: application/json" \
  --data '{"purge_everything":true}'
```

The token needs exactly one permission — **Zone → Cache Purge → Purge** — and
should be scoped to this zone alone. Store it as a deploy secret; it is not a
build input and must not be committed.

`purge_everything` also drops the `immutable` asset entries, which is harmless:
their URLs carry `?v=<hash>` and change on any deploy that changes their bytes,
so they would have been re-fetched anyway.

### 7. Verify

```sh
# A guide: MISS on the first request, HIT on the second.
curl -sI https://autumn-web.app/docs/getting-started | grep -iE 'cf-cache-status|cache-control'
curl -sI https://autumn-web.app/docs/getting-started | grep -i cf-cache-status

# The entry-point redirect — 307, with a policy, and cacheable.
curl -sI https://autumn-web.app/docs | grep -iE 'cf-cache-status|location|cache-control'

# These must stay out of the cache entirely. Ask twice, and read the second
# answer: the first request for a URL Cloudflare has not seen reports MISS even
# when a rule has wrongly made it cacheable, so one probe cannot tell "never
# cached" apart from "about to be cached".
for path in '/search?q=router' /api/docs /actuator/prometheus; do
  curl -sI "https://autumn-web.app$path" > /dev/null   # warm, ignore
  printf '%s: ' "$path"
  curl -sI "https://autumn-web.app$path" | grep -i cf-cache-status
done
```

On that second request each must report `DYNAMIC` (never eligible) or `BYPASS`
(a rule declined it). **`MISS` is already a failure** — it means Cloudflare
judged the response cacheable and stored it — and a `HIT` on the next request
merely confirms what the `MISS` already said.

Either result means some rule is matching a path this one does not. The
allow-list above cannot match any of these three, so look for **another rule
that does — anywhere in the list, not just above this one.** Rule order is
irrelevant to this particular diagnosis: with no competing match on these paths
there is nothing for precedence to protect, so a "Cache Everything" rule takes
effect from wherever it sits.
