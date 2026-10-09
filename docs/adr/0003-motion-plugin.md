# ADR-0003: Animate with `autumn-plugin-motion`, ahead of the 0.8 docs refresh

- **Status:** accepted
- **Date:** 2026-10-08
- **Related:** ADR-0001 (edge caching), `src/site.rs`, `src/lib.rs` (`apply_cache_control`), `src/export.rs`

## Context

The site is entirely static from the browser's point of view: server-rendered
Maud, a little htmx for docs search, and two small scripts of its own. That is
deliberate and stays that way. The landing page still feels flat, though, and
long guides give no sense of how far through them a reader is.

[`autumn-plugin-motion`](https://crates.io/crates/autumn-plugin-motion) gives an
Autumn app [Motion](https://motion.dev) animations through `data-motion`
attributes. It vendors Motion into the crate and serves it from memory, so
there is no npm, no bundler and no third-party CDN. Its init script re-scans
after every `htmx:afterSwap`, so htmx partials animate without extra code.

Version 0.2.0 serves its bundle through autumn-web 0.8's `plugin_assets` seam,
so it needs autumn-web 0.8. The guides this site serves document 0.7.0, and the
plan is to refresh them for 0.8 alongside Harvest 0.7.0.

## Decision

- **The runtime moves to autumn-web 0.8.0 now. The docs line stays on 0.7.0.**
  `tests/fly_deploy_config.rs` already separates the two:
  `runtime_versions_reflect_current_published_autumn_dependency` (Cargo.toml,
  `export.rs`) now pins 0.8.0, and `site_copy_targets_the_upcoming_autumn_docs_line`
  (`VERSION_LABEL`, `seo::AUTUMN_VERSION`; since renamed
  `site_copy_targets_the_published_autumn_docs_line`) still pins 0.7.0. The upgrade itself
  was small: `#[serde(default)]` beside `skip_serializing_if` for 0.8's stricter
  `OpenApiSchema` derive, and `ManifestEntry::new` / `StaticManifest::new` for the
  structs 0.8 made `#[non_exhaustive]`.
- **Restrained, purposeful motion only:**
  - home: the hero copy rises in a stagger, the code sample blurs in, the CTAs
    pop in on a spring and react to hover and press, the featured cards scale
    in one after another with hover and press, and the remaining bands fade up
    as they scroll into view;
  - docs: a reading-progress bar driven by scroll, repainted in the site's
    copper and rust;
  - search: htmx results cascade in, through the plugin's swap re-scan.
- **`prefers-reduced-motion` is honoured.** The plugin skips every animation
  and leaves the content fully visible. Without JavaScript nothing is hidden.
- **Plugin assets keep the framework's cache policy.** `apply_cache_control`
  marks a `/static/` response `immutable` only when it has a `?v=` query. The
  plugin's content-hashed URLs don't carry one, so that rule would have cut
  them to an hour. Requests under `/static/_plugins/` now keep the framework's
  header: `immutable` on the hashed URL, `must-revalidate` on the plain one.
- **The static export writes the bundle.** It is served from memory, not from
  `static/`, so `export_site` writes each file to its hashed URL. Otherwise the
  dead-link check fails, and so would the exported pages' SRI.

## Consequences

- CSP needs no change. The scripts are same-origin under `script-src 'self'`
  and carry SRI hashes, and Motion writes styles through the CSSOM, which
  `style-src` does not restrict.
- A typo in a `data-motion` value falls back to `fade-up` without an error.
  `tests/docs_site.rs` checks every value the site renders against the
  plugin's preset list.
- When the docs move to 0.8, only the docs-line constants and the guides
  change. The runtime is already there.

## Follow-up (2026-10-09)

The docs line moved to Autumn 0.8.0 / Harvest 0.7.0, as the last consequence
above anticipated: the docs-line constants and the guides changed, and nothing
else in the runtime did. Runtime and docs line now pin the same version.
