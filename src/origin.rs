//! Everything that only the origin binary can do.
//!
//! The origin is the authority: it serves every route the site has, including
//! all five that [`crate::edge`] also compiles into the capsule. What lives
//! here is the remainder — the routes and layers that need `autumn-web` itself,
//! and therefore cannot exist at the edge:
//!
//! | Here | Why not at the edge |
//! | --- | --- |
//! | `/search` | takes the `HxRequest` extractor to choose between an htmx partial and a full page |
//! | the JSON docs API and `/mcp` | `#[api_doc(mcp)]` routes, projected into an MCP server by the framework |
//! | the response layer stack | compression, ETag and `Cache-Control` are origin middleware; the capsule serves exactly what the handler produced |
//!
//! This module is `cfg`-gated off for `wasm32`, which is what keeps
//! `::autumn_web` out of the capsule's dependency graph.

use std::sync::LazyLock;

use autumn_web::prelude::*;
use autumn_web::reexports::axum::extract::Request;
use autumn_web::reexports::axum::middleware::{self, Next};
use autumn_web::reexports::axum::response::{IntoResponse, Response};
use autumn_web::reexports::http::{HeaderValue, header};
use serde::Deserialize;

use crate::docs::SearchIndex;
use crate::edge::docs_load_error_response;
use crate::widgets::active_search_empty_state;
use crate::{DOCS_SEARCH_RESULT_LIMIT, api, site, site_docs};

/// In-memory search index over the embedded guides, built once from
/// [`site_docs`]. `None` when the docs failed to load.
///
/// Origin-only: `/search` is the one read-path route the edge lane does not
/// carry, so a capsule never builds this — which matters, because building it
/// renders every guide (see [`crate::docs::DocPage`]).
static SITE_SEARCH_INDEX: LazyLock<Option<SearchIndex>> =
    LazyLock::new(|| site_docs().ok().map(SearchIndex::from_registry));

#[must_use]
pub fn site_search_index() -> Option<&'static SearchIndex> {
    SITE_SEARCH_INDEX.as_ref()
}

/// Optimize HTTP responses for repeat visitors.
///
/// Autumn 0.6.0 (issue #752) now applies user layers to static-first responses,
/// so wiring the framework's `dist/` static HTML serving is possible. We
/// deliberately do not: the docs are served dynamically from the in-memory
/// registry (content is already resident, so a request costs a HashMap lookup +
/// template wrap + on-the-fly compression). Serving `dist/` would duplicate the
/// embedded content on disk for no latency or bandwidth win on the 256 MB
/// scale-to-zero VM. The `build_site`/`dist` exporter is kept only as a
/// CDN/static-hosting bundle generator.
///
/// The stack also provides weak-ETag conditional-GET: [`EtagLayer`] is the
/// innermost layer, so on the response path it runs first and derives a weak
/// `ETag` from the raw uncompressed handler body, returning `304 Not Modified`
/// when a repeat visit's `If-None-Match` matches. `CompressionLayer` then
/// encodes the body and adds `Vary: Accept-Encoding`, keeping the ETag computed
/// over the unencoded bytes (framework-blessed ordering — see `router.rs`).
///
/// None of this runs in the edge capsule, which serves exactly the bytes the
/// handler produced. That is deliberate and is why the conformance projection
/// excuses the headers this stack adds; see `tests/edge_conformance.rs`.
///
/// [`EtagLayer`]: autumn_web::etag::EtagLayer
pub fn response_compression_layer() -> impl autumn_web::app::IntoAppLayer {
    tower::ServiceBuilder::new()
        .layer(middleware::from_fn(cache_static_assets))
        .layer(tower_http::map_response_body::MapResponseBodyLayer::new(
            autumn_web::reexports::axum::body::Body::new,
        ))
        .layer(tower_http::compression::CompressionLayer::new())
        .layer(autumn_web::etag::EtagLayer::new())
}

/// `Cache-Control` for a static asset whose URL carries the build's asset
/// version: the URL changes whenever the bytes do, so the response can be
/// cached permanently and never revalidated.
const IMMUTABLE_CACHE_CONTROL: &str = "public, max-age=31536000, immutable";

/// `Cache-Control` for a static asset served at a stable, unversioned URL.
///
/// These cannot be cached immutably: the URL stays the same across deploys, so
/// a year-long `immutable` entry would pin a visitor to a stale copy with no
/// way to bust it. A short freshness window plus revalidation keeps them cheap
/// — [`EtagLayer`] answers the revalidation with a `304`.
///
/// [`EtagLayer`]: autumn_web::etag::EtagLayer
const REVALIDATED_CACHE_CONTROL: &str = "public, max-age=3600, must-revalidate";

/// Whether a static-asset request carries this build's asset-version query
/// (`?v=…`), which is what makes a URL safe to cache immutably.
///
/// Only `site::versioned_asset_path` adds it, and it covers just the assets
/// this site authors. The framework serves its own assets under `/static/`
/// too — `autumn-widgets.css`, `autumn-widgets.js`, `htmx.min.js` — and the
/// pages that link them (the `/_stories` gallery is rendered by the framework,
/// not by us) reference them at bare, unversioned URLs. Marking those
/// `immutable` pinned every returning visitor to the previous release's copy
/// for a year across an `autumn-web` upgrade.
fn has_asset_version_query(query: Option<&str>) -> bool {
    query.is_some_and(|query| {
        query
            .split('&')
            .any(|pair| pair.split_once('=').is_some_and(|(key, _)| key == "v"))
    })
}

async fn cache_static_assets(request: Request, next: Next) -> Response {
    let is_static = request.uri().path().starts_with("/static/");
    let versioned = has_asset_version_query(request.uri().query());
    let mut response = next.run(request).await;

    if is_static && response.status().is_success() {
        let cache_control = if versioned {
            IMMUTABLE_CACHE_CONTROL
        } else {
            REVALIDATED_CACHE_CONTROL
        };
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static(cache_control),
        );
    }

    response
}

#[derive(Debug, Deserialize)]
pub struct DocsSearchQuery {
    #[serde(default)]
    q: String,
}

/// Search the embedded guides and return an htmx results partial, or a full
/// docs page when reached directly (e.g. the widget's `<noscript>` GET form).
///
/// Served at [`crate::DOCS_SEARCH_PATH`], outside the `/docs/{slug}` namespace.
#[get("/search")]
pub async fn docs_search(hx: HxRequest, Query(query): Query<DocsSearchQuery>) -> Response {
    let term = query.q.trim();

    let results = match site_search_index() {
        Some(index) if !term.is_empty() => {
            let hits = index.search(term, DOCS_SEARCH_RESULT_LIMIT);
            site::render_docs_search_results(term, &hits)
        }
        Some(_) => active_search_empty_state("Type to search the guides."),
        None => active_search_empty_state("Search is unavailable right now."),
    };

    if hx.is_htmx {
        return results.into_response();
    }

    // Non-htmx request (no-JS fallback): wrap the results in the docs layout.
    match site_docs() {
        Ok(registry) => site::render_docs_search_page(registry, term, results).into_response(),
        Err(error) => docs_load_error_response(error),
    }
}

/// Every route the origin serves.
///
/// The five `#[edge]` routes are mounted here as ordinary routes — that is what
/// makes fallthrough free. A request the capsule declines arrives at the origin
/// and lands on a route that was always there, with no glue in between.
#[must_use]
pub fn app_routes() -> Vec<autumn_web::Route> {
    let mut routes = routes![
        crate::edge::index,
        crate::edge::docs_index,
        docs_search,
        crate::edge::docs_page,
        crate::edge::robots_txt,
        crate::edge::sitemap_xml
    ];
    // The JSON docs API, which `main` projects into the `/mcp` MCP server.
    // Registered here rather than only in `main` so the test harness exercises
    // the same route set the deployed app serves.
    routes.extend(api::api_routes());
    routes
}
