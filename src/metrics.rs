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
//! Every label below is drawn from a fixed, small set — a tool name, an
//! outcome, a status class. Nothing is labelled with a slug, a search term or
//! anything else a visitor controls: a Prometheus label with unbounded values
//! is a memory leak with a metrics API in front of it.

use autumn_web::metrics;
/// Guide searches, by outcome — the site's own search box and the MCP search
/// tool both land here.
///
/// `outcome="empty"` is the interesting series: a reader searched and the
/// corpus had nothing. A rising empty rate is a documentation gap, which is
/// why this counter exists at all.
pub const DOCS_SEARCHES: &str = "docs_site_searches_total";

/// Rendered guide pages, split by whether the slug existed.
///
/// `outcome="missing"` rising means something links a guide that does not
/// exist — a stale external link, or a rename that did not update the sidebar.
pub const DOCS_PAGE_VIEWS: &str = "docs_site_page_views_total";

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
        DOCS_PAGE_VIEWS,
        "Guide page renders, by whether the requested slug exists",
    );
}

/// Records one guide search.
pub fn record_search(outcome: &'static str) {
    metrics::counter(DOCS_SEARCHES)
        .with_label("outcome", outcome)
        .increment(1);
}

/// Records one guide page render.
pub fn record_page_view(outcome: &'static str) {
    metrics::counter(DOCS_PAGE_VIEWS)
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
