use autumn_web::test::TestApp;

use autumn_io::docs::{DocRegistry, DocSource, slugify_heading};
use autumn_io::export::{ExportConfig, export_site};
use autumn_io::site::{render_docs_page, render_home_page};

const GUIDE_START_SLUG: &str = "getting-started";
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

```toml
autumn-web = "0.4"
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
        page.html.contains("<span style=\"color:"),
        "code blocks should include server-rendered syntax highlight spans"
    );
    assert!(
        page.html.contains("autumn_web::prelude")
            && page.html.contains("autumn-web")
            && page.html.contains("language-toml"),
        "highlighting should preserve Rust and TOML code text and language classes"
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
    assert!(html.contains(r#"<div class="code-block-header">"#));
    assert!(html.contains(r#"<span class="code-language">Rust</span>"#));
    assert!(html.contains(r#"<span class="code-window-dots" aria-hidden="true">"#));
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
    assert!(html.contains("use"));
    assert!(html.contains("autumn_web"));
    assert!(html.contains("prelude"));
}

#[test]
fn bundled_site_docs_use_vendored_autumn_guide_snapshot() {
    let registry = autumn_io::site_docs().expect("bundled guide docs should load");
    let page = registry
        .page(GUIDE_START_SLUG)
        .expect("getting-started guide should be bundled");

    assert!(registry.pages().len() >= 20);
    assert_eq!(registry.pages()[0].slug, GUIDE_START_SLUG);
    assert_eq!(page.title, "Getting Started with Autumn");
    assert!(page.html.contains("autumn doctor"));
    assert!(page.html.contains("autumn_web::prelude"));
    assert!(
        registry.page("quickstart").is_none(),
        "old hand-written quickstart should not shadow the upstream guide snapshot"
    );
}

#[test]
fn bundled_guide_rustdoc_fence_modifiers_render_as_rust() {
    let registry = autumn_io::site_docs().expect("bundled guide docs should load");
    let all_docs_html = registry
        .pages()
        .iter()
        .map(|page| page.html.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(all_docs_html.contains(r#"class="language-rust""#));
    assert!(all_docs_html.contains(r#"<span class="code-language">Rust</span>"#));
    assert!(!all_docs_html.contains("language-rust,ignore"));
    assert!(!all_docs_html.contains("language-rust,no_run"));
    assert!(!all_docs_html.contains("Rust,ignore"));
    assert!(!all_docs_html.contains("Rust,no Run"));
}

#[test]
fn vendored_guide_snapshot_and_sync_tool_do_not_commit_local_source_paths() {
    let guide_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("content/guide");
    let sync_tool =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/bin/sync_guide_docs.rs");
    let mut scanned = 0;

    for entry in std::fs::read_dir(&guide_dir).expect("vendored guide directory should exist") {
        let entry = entry.expect("guide entry should be readable");
        if entry.path().extension().and_then(|ext| ext.to_str()) != Some("md") {
            continue;
        }

        let content =
            std::fs::read_to_string(entry.path()).expect("vendored guide file should be readable");
        assert!(
            !content.contains(r"C:\Users\markm"),
            "vendored guide content must not leak the local Autumn checkout path"
        );
        scanned += 1;
    }

    assert!(
        scanned >= 20,
        "expected the Autumn guide snapshot, not a token placeholder"
    );

    let sync_tool = std::fs::read_to_string(sync_tool).expect("sync tool should be committed");
    assert!(sync_tool.contains("AUTUMN_REPO_DIR"));
    assert!(
        !sync_tool.contains(r"C:\Users\markm"),
        "sync tooling should use env/CLI paths, not a machine-specific source path"
    );
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
    assert!(html.contains(r#"<link rel="icon" href="/static/img/autumn-mark-68.png""#));
    assert!(html.contains(r#"src="/static/img/autumn-mark-68.png""#));
    assert!(html.contains("<span style=\"color:"));
    assert!(html.contains(r#"<span class="code-language">Rust</span>"#));
    assert!(html.contains(
        r#"srcset="/static/img/autumn-mark-68.png 1x, /static/img/autumn-mark-136.png 2x""#
    ));
    assert!(html.contains(r#"<meta property="og:site_name" content="Autumn">"#));
    assert!(html.contains(
        r#"<meta property="og:image" content="https://autumn.io/static/img/autumn-social.png">"#
    ));
    assert!(html.contains(r#"<meta name="twitter:card" content="summary">"#));
    assert!(html.contains(r#""@type":"WebSite""#));
    assert!(html.contains(r#""@type":"SoftwareSourceCode""#));
}

#[test]
fn rendered_site_chrome_links_to_autumn_sources_and_crate() {
    let registry = DocRegistry::from_sources([DocSource::new("quickstart", QUICKSTART_SOURCE)])
        .expect("valid docs source should parse");
    let html = render_home_page(&registry).into_string();

    assert!(html.contains(r#"href="https://github.com/madmax983/autumn""#));
    assert!(html.contains(r#"href="https://crates.io/crates/autumn-web""#));
    assert!(html.contains(r#"href="https://github.com/madmax983/autumn_io""#));
}

#[test]
fn bundled_guide_links_are_rewritten_for_site_routes_and_upstream_source() {
    let registry = autumn_io::site_docs().expect("bundled guide docs should load");

    let deployment = registry
        .page("deployment")
        .expect("deployment guide should be bundled");
    assert!(
        deployment
            .html
            .contains(r#"href="/docs/getting-started#configuration""#)
    );
    assert!(deployment.html.contains(r#"href="/docs/signing-secrets""#));
    assert!(!deployment.html.contains(r#"href="getting-started.md"#));
    assert!(!deployment.html.contains(r#"href="signing-secrets.md"#));

    let getting_started = registry
        .page("getting-started")
        .expect("getting started guide should be bundled");
    assert!(getting_started.html.contains(
        r#"href="https://github.com/madmax983/autumn/tree/trunk-dev/examples/todo-app""#
    ));
    assert!(
        !getting_started
            .html
            .contains(r#"href="../../examples/todo-app""#)
    );

    let custom_subsystems = registry
        .page("custom-subsystems")
        .expect("custom subsystems guide should be bundled");
    assert!(
        custom_subsystems
            .html
            .contains(r#"href="/docs/extensibility""#)
    );
    assert!(custom_subsystems.html.contains(
        r#"href="https://github.com/madmax983/autumn/tree/trunk-dev/examples/custom_config_loader""#
    ));
    assert!(custom_subsystems.html.contains(
        r#"href="https://github.com/madmax983/autumn/blob/trunk-dev/autumn/src/plugin.rs""#
    ));
    assert!(
        !custom_subsystems
            .html
            .contains(r#"href="../../examples/custom_config_loader""#)
    );

    for page in registry.pages() {
        for href in html_hrefs(&page.html) {
            assert!(
                !href.starts_with("../") && !href.starts_with("./"),
                "{} should not render repo-relative href {href}",
                page.slug
            );
            assert!(
                !href.ends_with(".md")
                    || href.starts_with("https://github.com/madmax983/autumn/blob/trunk-dev/"),
                "{} should not render site-local Markdown href {href}",
                page.slug
            );
            assert!(
                !href.contains(".md#")
                    || href.starts_with("https://github.com/madmax983/autumn/blob/trunk-dev/"),
                "{} should not render site-local Markdown anchor href {href}",
                page.slug
            );
        }
    }
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
fn css_makes_code_samples_visually_distinct() {
    assert!(SITE_CSS.contains(".code-block::before"));
    assert!(SITE_CSS.contains(".code-block-header"));
    assert!(SITE_CSS.contains(".code-window-dot"));
    assert!(SITE_CSS.contains(".code-language"));
    assert!(SITE_CSS.contains("box-shadow: 0 18px 42px"));
    assert!(SITE_CSS.contains("background: linear-gradient"));
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
        .assert_header("location", "/docs/getting-started");

    app.get("/docs/getting-started")
        .send()
        .await
        .assert_status(200)
        .assert_body_contains("Getting Started with Autumn")
        .assert_body_contains("docs-sidebar")
        .assert_body_contains("data-copy-code");

    app.get("/docs/no-such-page")
        .send()
        .await
        .assert_status(404)
        .assert_body_contains("That docs page is not in the stack");
}

#[tokio::test]
async fn autumn_routes_compress_html_when_client_accepts_gzip() {
    let app = TestApp::new()
        .routes(autumn_io::app_routes())
        .layer(autumn_io::response_compression_layer())
        .build();

    app.get("/")
        .header("accept-encoding", "gzip")
        .send()
        .await
        .assert_status(200)
        .assert_header("content-encoding", "gzip");
}

#[tokio::test]
async fn autumn_routes_cache_static_assets_for_repeat_visits() {
    let app = TestApp::new()
        .routes(autumn_io::app_routes())
        .layer(autumn_io::response_compression_layer())
        .build();

    let home = app.get("/").send().await;
    home.assert_status(200);
    assert_eq!(home.header("cache-control"), None);

    for path in [
        "/static/css/site.css",
        "/static/js/copy-code.js",
        "/static/img/autumn-social.png",
        "/static/img/autumn-mark-68.png",
    ] {
        app.get(path)
            .send()
            .await
            .assert_status(200)
            .assert_header("cache-control", "public, max-age=31536000, immutable");
    }
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
        .assert_body_contains("<loc>https://autumn.io/docs/getting-started</loc>")
        .assert_body_contains("<loc>https://autumn.io/docs/deployment</loc>")
        .text();

    assert!(
        !sitemap.contains("<loc>https://autumn.io/docs</loc>"),
        "sitemap should not advertise the redirect-only docs index"
    );
}

#[test]
fn export_site_writes_static_dist_tree_from_shared_renderers() {
    let workspace = unique_temp_dir("autumn-io-export");
    let dist = workspace.join("dist");
    let registry = autumn_io::site_docs().expect("bundled docs should load");

    let summary = export_site(registry, &ExportConfig::new(&dist)).expect("site should export");

    assert_eq!(summary.html_pages, registry.pages().len() + 1);
    assert!(summary.static_assets >= 4);
    assert_eq!(summary.routes, registry.pages().len() + 3);

    let home = std::fs::read_to_string(dist.join("index.html")).expect("home html");
    assert!(home.contains("Rust web framework for server-rendered apps"));
    assert!(home.contains(r#"<link rel="canonical" href="https://autumn.io/">"#));

    let getting_started =
        std::fs::read_to_string(dist.join("docs/getting-started/index.html")).expect("docs html");
    assert!(getting_started.contains(r#"<h1 id="page-title">Getting Started with Autumn</h1>"#));
    assert!(getting_started.contains(r#"aria-current="page" href="/docs/getting-started""#));
    assert!(
        getting_started
            .contains(r#"<link rel="canonical" href="https://autumn.io/docs/getting-started">"#)
    );

    let robots = std::fs::read_to_string(dist.join("robots.txt")).expect("robots file");
    assert!(robots.contains("Sitemap: https://autumn.io/sitemap.xml"));

    let sitemap = std::fs::read_to_string(dist.join("sitemap.xml")).expect("sitemap file");
    assert!(sitemap.contains("<loc>https://autumn.io/docs/getting-started</loc>"));

    assert!(dist.join("static/css/site.css").exists());
    assert!(dist.join("static/js/copy-code.js").exists());
    assert!(dist.join("static/img/autumn-social.png").exists());
    assert!(dist.join("static/img/autumn-mark-68.png").exists());
    assert!(dist.join("static/img/autumn-mark-136.png").exists());

    let manifest = std::fs::read_to_string(dist.join("manifest.json")).expect("manifest");
    assert!(manifest.contains(r#""/docs/getting-started""#));
    assert!(manifest.contains(r#""docs/getting-started/index.html""#));

    std::fs::remove_dir_all(workspace).expect("cleanup export test");
}

fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "{prefix}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("temp dir should be created");
    dir
}

fn html_hrefs(html: &str) -> impl Iterator<Item = &str> {
    html.split("href=\"")
        .skip(1)
        .filter_map(|segment| segment.split('"').next())
}
