use std::sync::LazyLock;

use autumn_web::prelude::*;
use autumn_web::reexports::axum::response::{IntoResponse, Redirect, Response};
use autumn_web::reexports::http::StatusCode;

pub mod docs;
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

#[must_use]
pub fn app_routes() -> Vec<autumn_web::Route> {
    routes![index, docs_index, docs_page]
}

fn docs_load_error_response(error: &DocsError) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        site::render_docs_load_error(error),
    )
        .into_response()
}
