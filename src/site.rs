use autumn_web::{Markup, PreEscaped, html};

use crate::docs::{DocPage, DocRegistry};

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
            (document_head("Autumn - Rust web apps without the ritual", "Docs-first home for the Autumn Rust web framework."))
            body class="site-shell home-shell" {
                (site_header("home"))
                main class="home-main" {
                    section class="home-hero" {
                        div class="hero-copy" {
                            p class="eyebrow" { (VERSION_LABEL) }
                            h1 { "Fast, simple Rust web apps" }
                            p class="hero-lede" {
                                "Autumn gives Rust developers a direct path from route handler to production-ready web app: typed routing, clear configuration, static assets, health checks, and deployment defaults."
                            }
                            div class="hero-actions" {
                                a class="button button-primary" href="/docs/quickstart" { "Get started" }
                                a class="button button-secondary" href="/docs/routing" { "Read the docs" }
                            }
                        }
                        div class="hero-code code-block" data-copy-code {
                            button class="copy-code-button" type="button" data-copy-button { "Copy" }
                            pre { code class="language-rust" { (HOME_ROUTE_EXAMPLE) } }
                        }
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
            (document_head(&format!("{} - Autumn Docs", page.title), &page.description))
            body class="site-shell docs-shell" {
                (site_header("docs"))
                div class="docs-layout" {
                    aside class="docs-sidebar" aria-label="Docs navigation" {
                        nav {
                            p class="sidebar-label" { "Start here" }
                            @for item in registry.pages() {
                                a
                                    class=(if item.slug == page.slug { "docs-nav-link active" } else { "docs-nav-link" })
                                    href=(format!("/docs/{}", item.slug))
                                {
                                    span { (&item.title) }
                                }
                            }
                        }
                    }
                    main class="docs-main" {
                        article class="docs-article" {
                            header class="article-header" {
                                p class="eyebrow" { (VERSION_LABEL) }
                                h1 { (&page.title) }
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
            (document_head("Docs page not found - Autumn", "The requested Autumn documentation page was not found."))
            body class="site-shell docs-shell" {
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
                    main class="docs-main" {
                        article class="docs-article missing-page" {
                            p class="eyebrow" { "404" }
                            h1 { "That docs page is not in the stack" }
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
            (document_head("Docs failed to load - Autumn", "The Autumn documentation content failed to load."))
            body class="site-shell docs-shell" {
                (site_header("docs"))
                main class="centered-error" {
                    p class="eyebrow" { "500" }
                    h1 { "Docs failed to load" }
                    p { "The bundled Markdown content could not be parsed." }
                    pre class="error-detail" { code { (error.to_string()) } }
                }
            }
        }
    }
}

fn document_head(title: &str, description: &str) -> Markup {
    html! {
        head {
            meta charset="utf-8";
            meta name="viewport" content="width=device-width, initial-scale=1";
            meta name="description" content=(description);
            title { (title) }
            link rel="icon" href="/static/img/autumn-leaf.svg" type="image/svg+xml";
            link rel="stylesheet" href="/static/css/site.css";
            script src="/static/js/copy-code.js" defer {}
        }
    }
}

fn doctype() -> Markup {
    PreEscaped("<!doctype html>".to_owned())
}

fn site_header(active: &str) -> Markup {
    html! {
        header class="site-header" {
            a class="brand" href="/" aria-label="Autumn home" {
                img src="/static/img/autumn-leaf.svg" alt="" width="34" height="34";
                span { "Autumn" }
            }
            nav class="site-nav" aria-label="Primary navigation" {
                a class=(if active == "docs" { "active" } else { "" }) href="/docs/quickstart" { "Docs" }
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
