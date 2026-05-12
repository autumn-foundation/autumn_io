use autumn_web::test::TestApp;

use autumn_io::docs::{DocRegistry, DocSource, slugify_heading};
use autumn_io::site::{render_docs_page, render_home_page};

const SITE_CSS: &str = include_str!("../static/css/site.css");
const COPY_CODE_JS: &str = include_str!("../static/js/copy-code.js");

const QUICKSTART_SOURCE: &str = r#"---
title: Quickstart
description: Build and run your first Autumn app.
order: 10
---

# Quickstart

## Create an app

```rust
use autumn_web::prelude::*;
```
"#;

const ROUTING_SOURCE: &str = r#"---
title: Routing
description: Define routes and path parameters.
order: 20
---

# Routing

## Path parameters
"#;

#[test]
fn parses_frontmatter_and_generates_article_metadata() {
    let registry = DocRegistry::from_sources([DocSource::new("quickstart", QUICKSTART_SOURCE)])
        .expect("valid docs source should parse");
    let page = registry
        .page("quickstart")
        .expect("quickstart should be registered");

    assert_eq!(page.title, "Quickstart");
    assert_eq!(page.description, "Build and run your first Autumn app.");
    assert_eq!(page.order, 10);
    assert_eq!(page.slug, "quickstart");
    assert!(
        page.html.contains("<h2 id=\"create-an-app\">"),
        "heading IDs should be generated from Markdown headings"
    );
    assert!(
        page.html.contains("<pre><code class=\"language-rust\">"),
        "Rust code blocks should keep language metadata for copy controls"
    );
    assert!(
        !page.html.contains("<h1 id=\"quickstart\">"),
        "page title should be owned by the document template, not duplicated from Markdown"
    );
    assert_eq!(page.toc[0].id, "create-an-app");
}

#[test]
fn registry_orders_pages_and_calculates_previous_next_links() {
    let registry = DocRegistry::from_sources([
        DocSource::new("routing", ROUTING_SOURCE),
        DocSource::new("quickstart", QUICKSTART_SOURCE),
    ])
    .expect("valid docs source should parse");

    let pages = registry.pages();
    assert_eq!(pages[0].slug, "quickstart");
    assert_eq!(pages[1].slug, "routing");

    let neighbors = registry.neighbors("routing");
    assert_eq!(
        neighbors.previous.expect("previous page").slug,
        "quickstart"
    );
    assert!(neighbors.next.is_none());
}

#[test]
fn slugifies_headings_for_stable_search_ready_anchors() {
    assert_eq!(
        slugify_heading("Autumn 0.4.0: Templates & Static Assets!"),
        "autumn-0-4-0-templates-static-assets"
    );
}

#[test]
fn rendered_docs_page_contains_nav_toc_and_copyable_code_block() {
    let registry = DocRegistry::from_sources([
        DocSource::new("quickstart", QUICKSTART_SOURCE),
        DocSource::new("routing", ROUTING_SOURCE),
    ])
    .expect("valid docs source should parse");
    let page = registry.page("quickstart").expect("quickstart exists");
    let html = render_docs_page(&registry, page).into_string();

    assert!(html.contains("docs-sidebar"));
    assert!(html.contains("href=\"/docs/routing\""));
    assert!(html.contains("docs-toc"));
    assert!(html.contains("href=\"#create-an-app\""));
    assert!(html.contains("data-copy-code"));
    assert!(html.contains("Next"));
}

#[test]
fn rendered_home_page_has_keyboard_bypass_and_named_main_region() {
    let registry = DocRegistry::from_sources([
        DocSource::new("quickstart", QUICKSTART_SOURCE),
        DocSource::new("routing", ROUTING_SOURCE),
    ])
    .expect("valid docs source should parse");
    let html = render_home_page(&registry).into_string();

    assert!(
        html.contains(r##"<a class="skip-link" href="#main-content">Skip to main content</a>"##)
    );
    assert!(html.contains(r#"<main id="main-content""#));
    assert!(html.contains(r#"tabindex="-1""#));
    assert!(html.contains(r#"aria-labelledby="page-title""#));
    assert!(
        html.contains(r#"<h1 id="page-title">Rust web framework for server-rendered apps</h1>"#)
    );
}

#[test]
fn rendered_docs_page_marks_current_location_and_labels_article() {
    let registry = DocRegistry::from_sources([
        DocSource::new("quickstart", QUICKSTART_SOURCE),
        DocSource::new("routing", ROUTING_SOURCE),
    ])
    .expect("valid docs source should parse");
    let page = registry.page("quickstart").expect("quickstart exists");
    let html = render_docs_page(&registry, page).into_string();

    assert!(
        html.contains(r##"<a class="skip-link" href="#main-content">Skip to main content</a>"##)
    );
    assert!(html.contains(r#"<main id="main-content""#));
    assert!(html.contains(r#"<article class="docs-article" aria-labelledby="page-title">"#));
    assert!(html.contains(r#"<h1 id="page-title">Quickstart</h1>"#));
    assert!(html.contains(r#"aria-current="page" href="/docs/quickstart""#));
    assert!(html.contains(r#"aria-label="Copy code to clipboard""#));
    assert!(html.contains(r#"aria-live="polite""#));
}

#[test]
fn rendered_docs_page_has_one_page_level_heading() {
    let registry = DocRegistry::from_sources([DocSource::new("quickstart", QUICKSTART_SOURCE)])
        .expect("valid docs source should parse");
    let page = registry.page("quickstart").expect("quickstart exists");
    let html = render_docs_page(&registry, page).into_string();

    assert_eq!(html.matches("<h1").count(), 1);
    assert!(html.contains(r#"<h1 id="page-title">Quickstart</h1>"#));
}

#[test]
fn rendered_docs_page_contains_search_specific_metadata() {
    let registry = DocRegistry::from_sources([
        DocSource::new("quickstart", QUICKSTART_SOURCE),
        DocSource::new("routing", ROUTING_SOURCE),
    ])
    .expect("valid docs source should parse");
    let page = registry.page("quickstart").expect("quickstart exists");
    let html = render_docs_page(&registry, page).into_string();

    assert!(html.contains("Quickstart | Autumn Rust Web Framework Docs"));
    assert!(html.contains(r#"<link rel="canonical" href="https://autumn.io/docs/quickstart">"#));
    assert!(html.contains(r#"<meta property="og:type" content="article">"#));
    assert!(
        html.contains(r#"<meta property="og:url" content="https://autumn.io/docs/quickstart">"#)
    );
    assert!(html.contains(r#""@type":"TechArticle""#));
    assert!(html.contains(r#""headline":"Quickstart""#));
}

#[test]
fn rendered_home_page_links_into_core_docs_path() {
    let registry = DocRegistry::from_sources([
        DocSource::new("quickstart", QUICKSTART_SOURCE),
        DocSource::new("routing", ROUTING_SOURCE),
    ])
    .expect("valid docs source should parse");
    let html = render_home_page(&registry).into_string();

    assert!(html.contains("Rust web framework for server-rendered apps"));
    assert!(html.contains("href=\"/docs/quickstart\""));
    assert!(html.contains("href=\"/docs/routing\""));
    assert!(html.contains("use autumn_web::prelude::*;"));
}

#[test]
fn rendered_home_page_contains_search_social_and_site_schema_metadata() {
    let registry = DocRegistry::from_sources([
        DocSource::new("quickstart", QUICKSTART_SOURCE),
        DocSource::new("routing", ROUTING_SOURCE),
    ])
    .expect("valid docs source should parse");
    let html = render_home_page(&registry).into_string();

    assert!(html.contains("Autumn: Rust Web Framework for Server-Rendered Apps"));
    assert!(html.contains(r#"<link rel="canonical" href="https://autumn.io/">"#));
    assert!(html.contains(r#"<meta property="og:site_name" content="Autumn">"#));
    assert!(html.contains(
        r#"<meta property="og:image" content="https://autumn.io/static/img/autumn.png">"#
    ));
    assert!(html.contains(r#"<meta name="twitter:card" content="summary">"#));
    assert!(html.contains(r#""@type":"WebSite""#));
    assert!(html.contains(r#""@type":"SoftwareSourceCode""#));
}

#[test]
fn css_exposes_visible_focus_skip_link_and_reduced_motion_rules() {
    assert!(SITE_CSS.contains(".skip-link"));
    assert!(SITE_CSS.contains(":focus-visible"));
    assert!(SITE_CSS.contains("outline: 3px solid"));
    assert!(SITE_CSS.contains("@media (prefers-reduced-motion: reduce)"));
    assert!(SITE_CSS.contains("scroll-behavior: auto"));
}

#[test]
fn copy_code_script_updates_accessible_status_text() {
    assert!(COPY_CODE_JS.contains("aria-label"));
    assert!(COPY_CODE_JS.contains("Copied code to clipboard"));
    assert!(COPY_CODE_JS.contains("Select code manually"));
}

#[tokio::test]
async fn autumn_routes_render_home_docs_redirect_and_missing_docs_page() {
    let app = TestApp::new().routes(autumn_io::app_routes()).build();

    app.get("/")
        .send()
        .await
        .assert_status(200)
        .assert_body_contains("Rust web framework for server-rendered apps")
        .assert_body_contains("/static/css/site.css");

    app.get("/docs")
        .send()
        .await
        .assert_status(307)
        .assert_header("location", "/docs/quickstart");

    app.get("/docs/quickstart")
        .send()
        .await
        .assert_status(200)
        .assert_body_contains("Quickstart")
        .assert_body_contains("docs-sidebar")
        .assert_body_contains("data-copy-code");

    app.get("/docs/no-such-page")
        .send()
        .await
        .assert_status(404)
        .assert_body_contains("That docs page is not in the stack");
}

#[tokio::test]
async fn autumn_routes_expose_crawl_discovery_files() {
    let app = TestApp::new().routes(autumn_io::app_routes()).build();

    app.get("/robots.txt")
        .send()
        .await
        .assert_status(200)
        .assert_header_contains("content-type", "text/plain")
        .assert_body_contains("User-agent: *")
        .assert_body_contains("Allow: /")
        .assert_body_contains("Sitemap: https://autumn.io/sitemap.xml");

    let sitemap = app
        .get("/sitemap.xml")
        .send()
        .await
        .assert_status(200)
        .assert_header_contains("content-type", "application/xml")
        .assert_body_contains("<loc>https://autumn.io/</loc>")
        .assert_body_contains("<loc>https://autumn.io/docs/quickstart</loc>")
        .assert_body_contains("<loc>https://autumn.io/docs/routing</loc>")
        .text();

    assert!(
        !sitemap.contains("<loc>https://autumn.io/docs</loc>"),
        "sitemap should not advertise the redirect-only docs index"
    );
}
