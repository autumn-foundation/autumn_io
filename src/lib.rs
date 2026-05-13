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

use docs::{DocRegistry, DocSource, DocsError};

pub const DOCS_START_SLUG: &str = "getting-started";
pub const DOCS_START_PATH: &str = "/docs/getting-started";

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
    ])
});

pub fn site_docs() -> Result<&'static DocRegistry, &'static DocsError> {
    match &*SITE_DOCS {
        Ok(registry) => Ok(registry),
        Err(error) => Err(error),
    }
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
    routes![index, docs_index, docs_page, robots_txt, sitemap_xml]
}

fn docs_load_error_response(error: &DocsError) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        site::render_docs_load_error(error),
    )
        .into_response()
}
