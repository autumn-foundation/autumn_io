use std::sync::LazyLock;

use autumn_web::prelude::*;
use autumn_web::reexports::axum::extract::Request;
use autumn_web::reexports::axum::middleware::{self, Next};
use autumn_web::reexports::axum::response::{IntoResponse, Redirect, Response};
use autumn_web::reexports::http::{HeaderValue, StatusCode, header};

pub mod docs;
pub mod export;
pub mod seo;
pub mod site;

use serde::Deserialize;

use docs::{DocRegistry, DocSource, DocsError, SearchIndex};

pub const DOCS_START_SLUG: &str = "getting-started";
pub const DOCS_START_PATH: &str = "/docs/getting-started";

/// Maximum number of guide results returned by the docs search handler.
const DOCS_SEARCH_RESULT_LIMIT: usize = 20;

macro_rules! guide_doc {
    ($slug:literal) => {
        DocSource::new(
            $slug,
            include_str!(concat!("../content/guide/", $slug, ".md")),
        )
    };
}

static SITE_DOCS: LazyLock<Result<DocRegistry, DocsError>> = LazyLock::new(|| {
    DocRegistry::from_sources([
        guide_doc!("getting-started"),
        guide_doc!("what-happens-when"),
        guide_doc!("autumn-harvest"),
        guide_doc!("coming-from-other-frameworks"),
        guide_doc!("generators"),
        guide_doc!("accessibility"),
        guide_doc!("middleware"),
        guide_doc!("path-helpers"),
        guide_doc!("routes-cli"),
        guide_doc!("macro-transparency"),
        guide_doc!("testing"),
        guide_doc!("transactions"),
        guide_doc!("seeding"),
        guide_doc!("storage"),
        guide_doc!("mail"),
        guide_doc!("authorization"),
        guide_doc!("signed-webhooks"),
        guide_doc!("signing-secrets"),
        guide_doc!("realtime"),
        guide_doc!("websockets"),
        guide_doc!("jobs"),
        guide_doc!("tasks"),
        guide_doc!("operating-background-jobs"),
        guide_doc!("scheduled-multi-replica"),
        guide_doc!("admin"),
        guide_doc!("custom-subsystems"),
        guide_doc!("extensibility"),
        guide_doc!("cloud-native"),
        guide_doc!("i18n"),
        guide_doc!("deployment"),
        // New in Autumn 0.5.0.
        guide_doc!("compression"),
        guide_doc!("conditional-get"),
        guide_doc!("pagination"),
        guide_doc!("active-search-and-autocomplete"),
        guide_doc!("wizards"),
        guide_doc!("hooks-and-transactions"),
        guide_doc!("repositories"),
        guide_doc!("migrations"),
        guide_doc!("soft-delete"),
        guide_doc!("state-machines"),
        guide_doc!("version-history"),
        guide_doc!("full-text-search"),
        guide_doc!("storage-variants"),
        guide_doc!("attribute-encryption"),
        guide_doc!("oauth"),
        guide_doc!("step-up-authentication"),
        guide_doc!("credentials"),
        guide_doc!("bot-protection"),
        guide_doc!("idempotency"),
        guide_doc!("logging-pii"),
        guide_doc!("presence"),
        guide_doc!("api-versioning"),
        guide_doc!("outbound-http"),
        guide_doc!("outbound-webhooks"),
        guide_doc!("mcp"),
        guide_doc!("feature-flags"),
        guide_doc!("experiments"),
        guide_doc!("runtime-config"),
        guide_doc!("resilience"),
        guide_doc!("health-indicators"),
        guide_doc!("metrics-sources"),
        guide_doc!("error-reporting"),
        guide_doc!("maintenance-mode"),
        guide_doc!("staged-deploys"),
        guide_doc!("dev-error-overlay"),
        guide_doc!("dev-inspector"),
        guide_doc!("dev-loop-latency"),
        guide_doc!("system-tests"),
        // New in Autumn 0.6.0.
        guide_doc!("flash"),
        guide_doc!("tabs"),
        guide_doc!("declarative-schema"),
        guide_doc!("events"),
        guide_doc!("lifecycle"),
        guide_doc!("mail-compliance"),
        guide_doc!("cache-stampede"),
        guide_doc!("daemon"),
        guide_doc!("distributed-locks"),
        guide_doc!("fragment-caching"),
        guide_doc!("operator-alerts"),
        guide_doc!("rate-limiting"),
        guide_doc!("security-posture-manifest"),
        guide_doc!("tls"),
        guide_doc!("format-helpers"),
        guide_doc!("stories"),
        guide_doc!("time-zones"),
        guide_doc!("transition-effects"),
        guide_doc!("wasm-islands"),
        guide_doc!("widget-styling"),
        guide_doc!("sharding"),
        guide_doc!("sqlite-in-production"),
        guide_doc!("tenant-cells"),
        guide_doc!("tauri"),
        guide_doc!("tauri-mobile-in-process"),
        guide_doc!("tauri-mobile-offline-sync"),
        guide_doc!("tauri-mobile-thin-client"),
        guide_doc!("starters"),
    ])
});

pub fn site_docs() -> Result<&'static DocRegistry, &'static DocsError> {
    match &*SITE_DOCS {
        Ok(registry) => Ok(registry),
        Err(error) => Err(error),
    }
}

/// In-memory search index over the embedded guides, built once from
/// [`site_docs`]. `None` when the docs failed to load.
static SITE_SEARCH_INDEX: LazyLock<Option<SearchIndex>> =
    LazyLock::new(|| site_docs().ok().map(SearchIndex::from_registry));

#[must_use]
pub fn site_search_index() -> Option<&'static SearchIndex> {
    SITE_SEARCH_INDEX.as_ref()
}

/// Optimize HTTP responses for repeat visitors.
///
/// Framework static HTML routing is intentionally not registered for this app
/// until Autumn applies user layers to static-first responses.
pub fn response_compression_layer() -> impl autumn_web::app::IntoAppLayer {
    tower::ServiceBuilder::new()
        .layer(middleware::from_fn(cache_static_assets))
        .layer(tower_http::map_response_body::MapResponseBodyLayer::new(
            autumn_web::reexports::axum::body::Body::new,
        ))
        .layer(tower_http::compression::CompressionLayer::new())
}

async fn cache_static_assets(request: Request, next: Next) -> Response {
    let cacheable = request.uri().path().starts_with("/static/");
    let mut response = next.run(request).await;

    if cacheable && response.status().is_success() {
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, immutable"),
        );
    }

    response
}

#[get("/")]
pub async fn index() -> Response {
    match site_docs() {
        Ok(registry) => site::render_home_page(registry).into_response(),
        Err(error) => docs_load_error_response(error),
    }
}

#[get("/docs")]
pub async fn docs_index() -> Redirect {
    Redirect::temporary(DOCS_START_PATH)
}

#[get("/docs/{slug}")]
pub async fn docs_page(Path(slug): Path<String>) -> Response {
    let registry = match site_docs() {
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

#[derive(Debug, Deserialize)]
pub struct DocsSearchQuery {
    #[serde(default)]
    q: String,
}

/// Search the embedded guides and return an htmx results partial, or a full
/// docs page when reached directly (e.g. the widget's `<noscript>` GET form).
#[get("/docs/search")]
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

#[get("/robots.txt")]
pub async fn robots_txt() -> Response {
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        seo::robots_txt(),
    )
        .into_response()
}

#[get("/sitemap.xml")]
pub async fn sitemap_xml() -> Response {
    let registry = match site_docs() {
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
        seo::sitemap_xml(registry),
    )
        .into_response()
}

#[must_use]
pub fn app_routes() -> Vec<autumn_web::Route> {
    routes![
        index,
        docs_index,
        docs_search,
        docs_page,
        robots_txt,
        sitemap_xml
    ]
}

fn docs_load_error_response(error: &DocsError) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        site::render_docs_load_error(error),
    )
        .into_response()
}
