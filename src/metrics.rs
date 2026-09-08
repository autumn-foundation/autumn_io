//! Application metrics for the docs site.
//!
//! `autumn-web` already exposes a lot on `/actuator/prometheus`, and most of
//! what an application wants is there: `autumn_http_requests_total` for volume,
//! and `autumn_http_route_requests_total{method,route}` for a per-endpoint
//! breakdown. Check that list before adding anything here — the families below
//! are only the ones the framework genuinely cannot express.
//!
//! There are two such gaps. Neither built-in family carries a *status* or
//! outcome dimension, so "how many people searched and found nothing" is not
//! derivable from either. And nothing at all observes `/mcp`.
//!
//! ## These count origin work, not reader traffic
//!
//! The read path is cached at the edge, so a handler on it runs only for a
//! cache miss or a revalidation. Anything counted in a page handler is
//! therefore a measure of what the origin still does, never of how many people
//! visited — and the better the cache works, the wider that gap grows. The
//! names here say `renders` rather than `views` for that reason. `/search` is
//! the exception, because it is `no-store` and every search reaches the
//! origin.
//!
//! Everything here goes through `autumn_web::metrics`, so it lands on the same
//! scrape endpoint as the framework's own families with no registration step.
//!
//! ## What is deliberately *not* here
//!
//! **Per-tool API counters.** `autumn_http_route_requests_total{method,route}`
//! already counts every registered route, giving one series per MCP tool for
//! free, so counting those again here would be duplication.
//!
//! **An MCP request counter.** There is no way to write one in 0.7.0. The MCP
//! endpoint is a *mount*, not a route: app layers do not wrap it (in either
//! builder order), it registers no route series, and `autumn-web` emits no MCP
//! metrics of its own, so nothing an application can install ever observes a
//! request to `/mcp`.
//!
//! What *is* observable is the tool call itself. A `tools/call` is replayed
//! into the ordinary router as `GET /api/…`, so it lands in the route family
//! alongside direct API traffic:
//!
//! ```promql
//! sum(rate(autumn_http_route_requests_total{route=~"/api/.*"}[5m])) by (route)
//! ```
//!
//! That over-counts by whatever direct `/api` traffic exists — nothing on the
//! site links those endpoints, so in practice it is close to MCP tool usage —
//! and under-counts by `initialize` and `tools/list`, which never reach a tool.
//! Attributing a call to its transport is not possible either: the replay
//! carries no marker, no distinguishing header, and no extension that a direct
//! request lacks.
//!
//! ## Naming
//!
//! The `autumn_` prefix is reserved: `counter("autumn_…")` returns an inert
//! handle that records nothing, with no error and no panic. These names use a
//! `docs_site_` prefix for that reason, and every family below is asserted to
//! actually reach a scrape — an inert counter is indistinguishable from a
//! counter whose value is zero, so only a test can tell the difference.
//!
//! ## Cardinality
//!
//! Both families carry exactly one label, `outcome`, drawn from a fixed set of
//! five. Nothing is labelled with a slug, a search term or anything else a
//! visitor controls: a Prometheus label with unbounded values is a memory leak
//! with a metrics API in front of it, and one crawler is enough to fill it.

use autumn_web::metrics;

/// Guide searches, by outcome — the site's own search box and the MCP search
/// tool both land here.
///
/// `outcome="empty"` is the interesting series: a reader searched and the
/// corpus had nothing. A rising empty rate is a documentation gap, which is
/// why this counter exists at all.
pub const DOCS_SEARCHES: &str = "docs_site_searches_total";

/// Guide pages **rendered by the origin**, split by whether the slug existed.
///
/// Not page views, and deliberately not named as though it were. Pages are
/// cached at the edge, so this handler runs only on a cache miss or a
/// revalidation; every hit is served without the origin hearing about it. The
/// gap between this and real traffic is the cache hit rate, which is the whole
/// point of the caching — so the better this site performs, the further this
/// counter falls below the number of readers. Real page views would have to
/// come from CDN analytics, which this process cannot see.
///
/// What it does measure is worth having on its own: how much rendering the
/// origin is still doing. That is the number this PR set out to reduce, and it
/// is what says whether the cache is working.
///
/// `outcome="missing"` is a floor rather than a count. A 404 is cached for only
/// a minute, so a bad inbound link still surfaces here — just under-counted by
/// however many colos absorbed it. Rising is meaningful; the magnitude is not.
///
/// [`DOCS_SEARCHES`] has no such caveat: `/search` is `no-store`, so every
/// search reaches the origin and that counter is exact.
pub const DOCS_PAGE_RENDERS: &str = "docs_site_page_renders_total";

/// Registers the `# HELP` text for every family this crate records.
///
/// Describing does not create the instrument — the description is held until
/// the first record call — so this is safe to call before any traffic, and the
/// families stay absent from a scrape until something actually happens.
///
/// Called from [`crate::response_compression_layer`] so that installing the
/// app's layer stack is the single wiring point; a binary or test that builds
/// the app gets the descriptions without a separate startup call to forget.
pub fn describe() {
    metrics::describe_counter(
        DOCS_SEARCHES,
        "Guide searches by outcome; 'empty' means the corpus had no match",
    );
    metrics::describe_counter(
        DOCS_PAGE_RENDERS,
        "Guide pages rendered by the origin (cache misses and revalidations, \
         not page views), by whether the requested slug exists",
    );
}

/// Records one guide search.
pub fn record_search(outcome: &'static str) {
    metrics::counter(DOCS_SEARCHES)
        .with_label("outcome", outcome)
        .increment(1);
}

/// Records one guide page render at the origin.
pub fn record_page_render(outcome: &'static str) {
    metrics::counter(DOCS_PAGE_RENDERS)
        .with_label("outcome", outcome)
        .increment(1);
}

/// Outcome labels, named once so a handler and its test cannot disagree.
pub mod outcome {
    /// The request found what it asked for.
    pub const FOUND: &str = "found";
    /// The requested slug does not exist.
    pub const MISSING: &str = "missing";
    /// A search matched at least one guide.
    pub const HIT: &str = "hit";
    /// A search ran and matched nothing.
    pub const EMPTY: &str = "empty";
    /// A search could not run — the index failed to build.
    pub const UNAVAILABLE: &str = "unavailable";
}
