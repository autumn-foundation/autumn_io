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

static SITE_DOCS: LazyLock<Result<DocRegistry, DocsError>> = LazyLock::new(|| {
    DocRegistry::from_sources([
        DocSource::new("quickstart", include_str!("../content/docs/quickstart.md")),
        DocSource::new("routing", include_str!("../content/docs/routing.md")),
        DocSource::new(
            "configuration",
            include_str!("../content/docs/configuration.md"),
        ),
        DocSource::new(
            "templates-static-assets",
            include_str!("../content/docs/templates-static-assets.md"),
        ),
        DocSource::new("deployment", include_str!("../content/docs/deployment.md")),
        DocSource::new(
            "upgrade-0-4",
            include_str!("../content/docs/upgrade-0-4.md"),
        ),
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
    Redirect::temporary("/docs/quickstart")
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
