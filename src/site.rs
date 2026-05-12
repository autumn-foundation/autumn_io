use autumn_web::{Markup, PreEscaped, html};

use crate::docs::{DocPage, DocRegistry, render_highlighted_code_block};
use crate::seo;

const VERSION_LABEL: &str = "Autumn 0.4.0";

const HOME_ROUTE_EXAMPLE: &str = r#"use autumn_web::prelude::*;

#[get("/")]
async fn index() -> Markup {
    html! { h1 { "Hello, Autumn." } }
}

#[autumn_web::main]
async fn main() {
    autumn_web::app()
        .routes(routes![index])
        .run()
        .await;
}"#;

pub fn render_home_page(registry: &DocRegistry) -> Markup {
    html! {
        (doctype())
        html lang="en" {
            (document_head(&PageMeta::home()))
            body class="site-shell home-shell" {
                (skip_link())
                (site_header("home"))
                main id="main-content" class="home-main" tabindex="-1" aria-labelledby="page-title" {
                    section class="home-hero" {
                        div class="hero-copy" {
                            p class="eyebrow" { (VERSION_LABEL) }
                            h1 id="page-title" { "Rust web framework for server-rendered apps" }
                            p class="hero-lede" {
                                "Autumn gives Rust developers a direct path from route handler to production-ready web app: typed routing, Maud templates, static assets, health checks, and deployment defaults."
                            }
                            div class="hero-actions" {
                                a class="button button-primary" href="/docs/quickstart" { "Get started" }
                                a class="button button-secondary" href="/docs/routing" { "Read the docs" }
                            }
                        }
                        (PreEscaped(render_highlighted_code_block(Some("rust"), HOME_ROUTE_EXAMPLE)))
                    }
                    section class="home-grid" aria-label="Core docs" {
                        @for page in registry.pages() {
                            a class="feature-link" href=(format!("/docs/{}", page.slug)) {
                                span class="feature-title" { (&page.title) }
                                span class="feature-description" { (&page.description) }
                            }
                        }
                    }
                }
                (site_footer())
            }
        }
    }
}

pub fn render_docs_page(registry: &DocRegistry, page: &DocPage) -> Markup {
    let neighbors = registry.neighbors(&page.slug);

    html! {
        (doctype())
        html lang="en" {
            (document_head(&PageMeta::docs(page)))
            body class="site-shell docs-shell" {
                (skip_link())
                (site_header("docs"))
                div class="docs-layout" {
                    aside class="docs-sidebar" aria-label="Docs navigation" {
                        nav {
                            p class="sidebar-label" { "Start here" }
                            @for item in registry.pages() {
                                @if item.slug == page.slug {
                                    a
                                        class="docs-nav-link active"
                                        aria-current="page"
                                        href=(format!("/docs/{}", item.slug))
                                    {
                                        span { (&item.title) }
                                    }
                                } @else {
                                    a
                                        class="docs-nav-link"
                                        href=(format!("/docs/{}", item.slug))
                                    {
                                        span { (&item.title) }
                                    }
                                }
                            }
                        }
                    }
                    main id="main-content" class="docs-main" tabindex="-1" aria-labelledby="page-title" {
                        article class="docs-article" aria-labelledby="page-title" {
                            header class="article-header" {
                                p class="eyebrow" { (VERSION_LABEL) }
                                h1 id="page-title" { (&page.title) }
                                p { (&page.description) }
                            }
                            div class="article-body" {
                                (PreEscaped(page.html.clone()))
                            }
                        }
                        nav class="docs-pagination" aria-label="Docs pagination" {
                            @if let Some(previous) = neighbors.previous {
                                a class="pagination-link previous" href=(format!("/docs/{}", previous.slug)) {
                                    span { "Previous" }
                                    strong { (&previous.title) }
                                }
                            } @else {
                                span class="pagination-placeholder" {}
                            }
                            @if let Some(next) = neighbors.next {
                                a class="pagination-link next" href=(format!("/docs/{}", next.slug)) {
                                    span { "Next" }
                                    strong { (&next.title) }
                                }
                            } @else {
                                span class="pagination-placeholder" {}
                            }
                        }
                    }
                    aside class="docs-toc" aria-label="On this page" {
                        p class="toc-label" { "On this page" }
                        nav {
                            @for item in &page.toc {
                                a class=(format!("toc-link depth-{}", item.level)) href=(format!("#{}", item.id)) {
                                    (&item.title)
                                }
                            }
                        }
                    }
                }
                (site_footer())
            }
        }
    }
}

pub fn render_missing_docs_page(registry: &DocRegistry, slug: &str) -> Markup {
    html! {
        (doctype())
        html lang="en" {
            (document_head(&PageMeta::noindex(
                "Docs page not found | Autumn",
                "The requested Autumn documentation page was not found.",
                &seo::docs_path(slug),
            )))
            body class="site-shell docs-shell" {
                (skip_link())
                (site_header("docs"))
                div class="docs-layout missing-layout" {
                    aside class="docs-sidebar" aria-label="Docs navigation" {
                        nav {
                            p class="sidebar-label" { "Start here" }
                            @for page in registry.pages() {
                                a class="docs-nav-link" href=(format!("/docs/{}", page.slug)) {
                                    span { (&page.title) }
                                }
                            }
                        }
                    }
                    main id="main-content" class="docs-main" tabindex="-1" aria-labelledby="page-title" {
                        article class="docs-article missing-page" aria-labelledby="page-title" {
                            p class="eyebrow" { "404" }
                            h1 id="page-title" { "That docs page is not in the stack" }
                            p {
                                "No Autumn docs page exists for "
                                code { (slug) }
                                ". The route is valid; the page is not."
                            }
                            a class="button button-primary" href="/docs/quickstart" { "Back to Quickstart" }
                        }
                    }
                }
                (site_footer())
            }
        }
    }
}

pub fn render_docs_load_error(error: &dyn std::error::Error) -> Markup {
    html! {
        (doctype())
        html lang="en" {
            (document_head(&PageMeta::noindex(
                "Docs failed to load | Autumn",
                "The Autumn documentation content failed to load.",
                "/",
            )))
            body class="site-shell docs-shell" {
                (skip_link())
                (site_header("docs"))
                main id="main-content" class="centered-error" tabindex="-1" aria-labelledby="page-title" {
                    p class="eyebrow" { "500" }
                    h1 id="page-title" { "Docs failed to load" }
                    p { "The bundled Markdown content could not be parsed." }
                    pre class="error-detail" { code { (error.to_string()) } }
                }
            }
        }
    }
}

struct PageMeta {
    title: String,
    description: String,
    canonical_path: String,
    robots: &'static str,
    og_type: &'static str,
    structured_data: Option<String>,
}

impl PageMeta {
    fn home() -> Self {
        Self {
            title: "Autumn: Rust Web Framework for Server-Rendered Apps".to_owned(),
            description: seo::SITE_DESCRIPTION.to_owned(),
            canonical_path: "/".to_owned(),
            robots: "index,follow,max-snippet:-1,max-image-preview:large,max-video-preview:-1",
            og_type: "website",
            structured_data: Some(seo::home_structured_data()),
        }
    }

    fn docs(page: &DocPage) -> Self {
        Self {
            title: format!("{} | Autumn Rust Web Framework Docs", page.title),
            description: page.description.clone(),
            canonical_path: seo::docs_path(&page.slug),
            robots: "index,follow,max-snippet:-1,max-image-preview:large,max-video-preview:-1",
            og_type: "article",
            structured_data: Some(seo::docs_structured_data(page)),
        }
    }

    fn noindex(title: &str, description: &str, canonical_path: &str) -> Self {
        Self {
            title: title.to_owned(),
            description: description.to_owned(),
            canonical_path: canonical_path.to_owned(),
            robots: "noindex,follow",
            og_type: "website",
            structured_data: None,
        }
    }

    fn canonical_url(&self) -> String {
        seo::absolute_url(&self.canonical_path)
    }
}

fn document_head(meta: &PageMeta) -> Markup {
    let canonical_url = meta.canonical_url();
    let image_url = seo::site_image_url();

    html! {
        head {
            meta charset="utf-8";
            meta name="viewport" content="width=device-width, initial-scale=1";
            meta name="description" content=(&meta.description);
            meta name="robots" content=(meta.robots);
            meta name="theme-color" content="#b94722";
            meta name="application-name" content=(seo::SITE_NAME);
            title { (&meta.title) }
            link rel="canonical" href=(&canonical_url);
            link rel="icon" href="/static/img/autumn-mark-68.png" type="image/png";
            link rel="sitemap" type="application/xml" href="/sitemap.xml";
            link rel="stylesheet" href="/static/css/site.css";
            meta property="og:site_name" content=(seo::SITE_NAME);
            meta property="og:type" content=(meta.og_type);
            meta property="og:title" content=(&meta.title);
            meta property="og:description" content=(&meta.description);
            meta property="og:url" content=(&canonical_url);
            meta property="og:image" content=(&image_url);
            meta name="twitter:card" content="summary";
            meta name="twitter:title" content=(&meta.title);
            meta name="twitter:description" content=(&meta.description);
            meta name="twitter:image" content=(&image_url);
            @if let Some(structured_data) = &meta.structured_data {
                script type="application/ld+json" { (PreEscaped(structured_data.clone())) }
            }
            script src="/static/js/copy-code.js" defer {}
        }
    }
}

fn doctype() -> Markup {
    PreEscaped("<!doctype html>".to_owned())
}

fn skip_link() -> Markup {
    html! {
        a class="skip-link" href="#main-content" { "Skip to main content" }
    }
}

fn site_header(active: &str) -> Markup {
    html! {
        header class="site-header" {
            a class="brand" href="/" aria-label="Autumn home" {
                img
                    src="/static/img/autumn-mark-68.png"
                    srcset="/static/img/autumn-mark-68.png 1x, /static/img/autumn-mark-136.png 2x"
                    alt=""
                    width="34"
                    height="34";
                span { "Autumn" }
            }
            nav class="site-nav" aria-label="Primary navigation" {
                @if active == "docs" {
                    a class="active" aria-current="location" href="/docs/quickstart" { "Docs" }
                } @else {
                    a href="/docs/quickstart" { "Docs" }
                }
                a href="/docs/upgrade-0-4" { "0.4.0" }
                a href="/docs/deployment" { "Deploy" }
            }
        }
    }
}

fn site_footer() -> Markup {
    html! {
        footer class="site-footer" {
            span { "Built with Autumn." }
            a href="/docs/quickstart" { "Quickstart" }
            a href="/docs/deployment" { "Deployment" }
        }
    }
}
