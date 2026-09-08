//! The read-path routes, and the only module in this crate that compiles for
//! `wasm32-wasip1`.
//!
//! Everything here is an `#[edge]` route: one handler source that the origin
//! binary mounts like any other route *and* that `autumn build` compiles into
//! `target/wasm32-wasip1/release/edge-capsule.wasm`, the artifact a CDN runs.
//! See `content/guide/edge.md`.
//!
//! # The rules this module lives under
//!
//! `autumn-web` is not in the dependency graph for the wasm target, so:
//!
//! 1. **Only `#[edge]` routes belong here.** A plain `#[get]` emits a native
//!    companion naming `::autumn_web` and the wasm build stops on it. Routes
//!    that need the framework — the htmx search partial, the JSON docs API —
//!    live in [`crate::origin`], which is `cfg`-gated off for wasm.
//! 2. **Imports come from `autumn_edge`.** Its prelude carries the route
//!    macros and exactly the extractors the edge lane can mediate. On the
//!    native target these resolve to the same `axum` and `http` crates
//!    `autumn-web` uses, so the origin sees no difference at all.
//!
//! # Why these four routes
//!
//! They are the whole public read surface of the docs site, and every one of
//! them is a pure function of its request: the guides are embedded in the
//! binary at compile time, so a page's bytes depend on nothing but the path
//! and the build. There is no clock, no randomness, no database, and no KV —
//! this site declares no `needs(...)` capability at all, so a host that
//! mediates nothing can still serve every route here.
//!
//! `/search` is the one read-path route that stays at the origin: it takes the
//! `HxRequest` extractor to decide between an htmx partial and a full page, and
//! that extractor is `autumn-web`'s.

use autumn_edge::prelude::*;
use autumn_edge::reexports::axum::response::{IntoResponse, Redirect, Response};
use autumn_edge::reexports::http::header;

use crate::docs::DocsError;
use crate::{DOCS_START_PATH, site};

/// The site's home page.
#[get("/")]
#[edge]
pub async fn index() -> Response {
    match crate::site_docs() {
        Ok(registry) => site::render_home_page(registry).into_response(),
        Err(error) => docs_load_error_response(error),
    }
}

/// `/docs` is not a page; it opens the guides at the first one.
#[get("/docs")]
#[edge]
pub async fn docs_index() -> Redirect {
    Redirect::temporary(DOCS_START_PATH)
}

/// One guide.
#[get("/docs/{slug}")]
#[edge]
pub async fn docs_page(Path(slug): Path<String>) -> Response {
    let registry = match crate::site_docs() {
        Ok(registry) => registry,
        Err(error) => return docs_load_error_response(error),
    };

    match registry.page(&slug) {
        Some(page) => site::render_docs_page(registry, page).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            site::render_missing_docs_page(registry, &slug),
        )
            .into_response(),
    }
}

#[get("/robots.txt")]
#[edge]
pub async fn robots_txt() -> Response {
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        crate::seo::robots_txt(),
    )
        .into_response()
}

#[get("/sitemap.xml")]
#[edge]
pub async fn sitemap_xml() -> Response {
    let registry = match crate::site_docs() {
        Ok(registry) => registry,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                error.to_string(),
            )
                .into_response();
        }
    };

    (
        [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
        crate::seo::sitemap_xml(registry),
    )
        .into_response()
}

/// The edge lane's route table, handed to `autumn_edge::serve` by
/// `src/bin/edge-capsule.rs`.
///
/// A handler that is `#[edge]` but missing from this list compiles fine and
/// silently never reaches the capsule, so `autumn doctor` warns about it and
/// `tests/edge_conformance.rs` asserts this list against the origin's own
/// route table.
#[must_use]
pub fn edge_routes() -> Vec<autumn_edge::EdgeRoute> {
    edge_routes![index, docs_index, docs_page, robots_txt, sitemap_xml]
}

/// The 500 page for a registry that failed to load.
///
/// Shared with [`crate::origin`], whose routes render the same failure the same
/// way — a docs site whose docs did not parse is broken identically in both
/// lanes.
pub(crate) fn docs_load_error_response(error: &DocsError) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        site::render_docs_load_error(error),
    )
        .into_response()
}
