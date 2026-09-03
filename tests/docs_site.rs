use autumn_web::test::TestApp;

use autumn_io::docs::{DocRegistry, DocSource, DocsError, slugify_heading};
use autumn_io::export::{ExportConfig, ExportError, export_site};
use autumn_io::site::{render_docs_page, render_home_page};

const GUIDE_START_SLUG: &str = "getting-started";
const SITE_CSS: &str = include_str!("../static/css/site.css");
const COPY_CODE_JS: &str = include_str!("../static/js/copy-code.js");
const DOCS_NAV_DISCLOSURE_JS: &str = include_str!("../static/js/docs-nav-disclosure.js");

const QUICKSTART_SOURCE: &str = r#"+++
title = "Quickstart"
description = "Build and run your first Autumn app."
order = 10
+++

# Quickstart

## Create an app

```rust
use autumn_web::prelude::*;
```

```toml
autumn-web = "0.4"
```
"#;

const ROUTING_SOURCE: &str = r#"+++
title = "Routing"
description = "Define routes and path parameters."
order = 20
+++

# Routing

## Path parameters
"#;

const RAW_HTML_SOURCE: &str = r#"+++
title = "Raw HTML"
description = "Raw HTML should not pass through docs rendering."
order = 30
+++

# Raw HTML

<script>alert("xss")</script>

Inline <em>HTML</em> should render as text.
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
        page.html
            .contains("<pre tabindex=\"0\"><code class=\"language-rust\">"),
        "Rust code blocks should keep language metadata for copy controls, \
         and stay keyboard-focusable so their horizontal scroll is reachable"
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
fn registry_rejects_slugs_that_can_escape_routes_or_export_paths() {
    for slug in [
        "",
        ".",
        "..",
        "../escape",
        "nested/page",
        r"nested\page",
        "C:drive",
    ] {
        let error = DocRegistry::from_sources([DocSource::new(slug, QUICKSTART_SOURCE)])
            .expect_err("unsafe docs slugs should be rejected");

        assert!(
            matches!(error, DocsError::InvalidSlug(ref invalid) if invalid == slug),
            "expected InvalidSlug for {slug:?}, got {error:?}"
        );
    }
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
    assert!(html.contains(r#"<aside id="docs-navigation" class="docs-sidebar""#));
    // The search box wires an htmx loading indicator: the input targets the
    // indicator via hx-indicator and the indicator element renders in-place.
    assert!(html.contains(r##"hx-indicator="#docs-search-indicator""##));
    assert!(html.contains(r#"id="docs-search-indicator""#));
    assert!(html.contains("docs-search-spinner"));
    assert!(html.contains(r##"href="#docs-navigation""##));
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
fn docs_sidebar_nav_links_render_inside_a_native_disclosure() {
    // On narrow viewports the 140-link sidebar nav sat first in DOM order,
    // forcing keyboard/AT users to tab through every link before reaching
    // the article (issue #25). Wrapping the link list in <details>/<summary>
    // lets a small script collapse it by default on mobile, without
    // reordering the DOM (which would break screen-reader source order).
    let registry = DocRegistry::from_sources([
        DocSource::new("quickstart", QUICKSTART_SOURCE),
        DocSource::new("routing", ROUTING_SOURCE),
    ])
    .expect("valid docs source should parse");
    let page = registry.page("quickstart").expect("quickstart exists");
    let html = render_docs_page(&registry, page).into_string();

    // Server-rendered markup defaults to `open` so the nav stays fully
    // usable with JavaScript disabled or before the script runs.
    assert!(html.contains(r#"<details class="docs-nav-disclosure" open>"#));
    assert!(html.contains(r#"<summary class="docs-nav-summary">Docs</summary>"#));

    let sidebar = html
        .split_once(r#"<aside id="docs-navigation" class="docs-sidebar""#)
        .and_then(|(_, rest)| rest.split_once("</aside>"))
        .map(|(sidebar, _)| sidebar)
        .expect("docs sidebar should render");

    let details_open = sidebar
        .find("<details")
        .expect("disclosure should wrap the nav links");
    let first_link = sidebar
        .find(r#"href="/docs/routing""#)
        .expect("nav link should render");
    let details_close = sidebar.find("</details>").expect("disclosure should close");
    assert!(
        details_open < first_link && first_link < details_close,
        "nav links must render inside the <details> disclosure"
    );

    // The disclosure script only makes sense on pages that render the
    // sidebar, so it ships alongside it rather than in the shared <head>.
    assert!(html.contains("/static/js/docs-nav-disclosure.js?v="));
}

#[test]
fn docs_sidebar_and_toc_navs_have_distinct_accessible_names() {
    // axe-core flagged `landmark-unique`: both inner <nav> elements were
    // unnamed and collided once both were visible (desktop widths).
    let registry = DocRegistry::from_sources([DocSource::new("quickstart", QUICKSTART_SOURCE)])
        .expect("valid docs source should parse");
    let page = registry.page("quickstart").expect("quickstart exists");
    let html = render_docs_page(&registry, page).into_string();

    assert!(html.contains(r#"<nav aria-label="Docs sections">"#));
    assert!(html.contains(r#"<nav aria-label="On this page">"#));
}

#[test]
fn markdown_raw_html_is_escaped_before_preescaped_page_rendering() {
    let registry = DocRegistry::from_sources([DocSource::new("raw-html", RAW_HTML_SOURCE)])
        .expect("valid docs source should parse");
    let page = registry.page("raw-html").expect("raw html page exists");

    assert!(!page.html.contains("<script>"));
    assert!(!page.html.contains("<em>HTML</em>"));
    assert!(
        page.html
            .contains("&lt;script&gt;alert(&quot;xss&quot;)&lt;/script&gt;")
    );
    assert!(
        page.html
            .contains("Inline &lt;em&gt;HTML&lt;/em&gt; should render as text.")
    );
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
    assert!(html.contains(r#"<h1 id="page-title">Ship the app, not the plumbing.</h1>"#));
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
    assert!(
        html.contains(r#"<link rel="canonical" href="https://autumn-web.app/docs/quickstart">"#)
    );
    assert!(html.contains(r#"<meta property="og:type" content="article">"#));
    assert!(
        html.contains(
            r#"<meta property="og:url" content="https://autumn-web.app/docs/quickstart">"#
        )
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

    assert!(html.contains("Ship the app, not the plumbing."));
    assert!(html.contains("href=\"/docs/quickstart\""));
    assert!(html.contains("href=\"/docs/routing\""));
    assert!(html.contains("use"));
    assert!(html.contains("autumn_web"));
    assert!(html.contains("prelude"));
}

#[test]
fn bundled_home_page_prioritizes_two_entry_paths_without_rendering_every_doc_card() {
    let registry = autumn_io::site_docs().expect("bundled guide docs should load");
    let html = render_home_page(registry).into_string();

    assert!(html.contains(r#"<h1 id="page-title">Ship the app, not the plumbing.</h1>"#));
    assert!(
        html.contains(
            "Autumn gives Rust teams the batteries they expect from mature app frameworks"
        )
    );
    assert!(html.contains("Core workflows"));
    assert!(html.contains("Build, test, secure, and deploy"));
    assert!(!html.contains("The docs people actually reach for"));
    assert_eq!(html.matches("home-feature-card").count(), 2);
    assert!(html.contains(r#"href="/docs/getting-started""#));
    assert!(html.contains("Getting Started with Autumn"));
    assert!(html.contains(r#"href="/docs/coming-from-other-frameworks""#));
    assert!(html.contains("Coming From Other Frameworks"));
    assert_eq!(html.matches("home-secondary-link").count(), 6);
    assert!(
        !html.contains(r#"href="/docs/docs-smoke""#),
        "front page should not render every vendored guide as a card"
    );
}

#[test]
fn bundled_home_page_represents_harvest_release_and_docs() {
    let registry = autumn_io::site_docs().expect("bundled guide docs should load");
    let html = render_home_page(registry).into_string();

    assert!(html.contains("Autumn Harvest 0.6.0"));
    assert!(html.contains("durable workflows"));
    assert!(html.contains(r#"href="/docs/autumn-harvest""#));
    assert!(html.contains(r#"href="https://github.com/autumn-foundation/autumn-harvest""#));
    assert!(html.contains(r#"href="https://crates.io/crates/autumn-harvest""#));
    assert!(html.contains(r#"href="https://docs.rs/autumn-harvest""#));
    // The home "Guide" button now leads into the on-site Harvest guide.
    assert!(html.contains(r#"href="/docs/harvest-project-skeleton""#));
}

#[test]
fn bundled_docs_sidebar_groups_guides_by_workflow() {
    let registry = autumn_io::site_docs().expect("bundled guide docs should load");
    let page = registry
        .page("transactions")
        .expect("transactions guide should be bundled");
    let html = render_docs_page(registry, page).into_string();

    for label in [
        "Start here",
        "Harvest",
        "Request surface",
        "Content and community",
        "Data and auth",
        "Realtime and jobs",
        "Extending and shipping",
    ] {
        assert!(html.contains(&format!(r#"<p class="docs-nav-section-title">{label}</p>"#)));
    }

    assert!(html.contains(r#"aria-current="page" href="/docs/transactions""#));

    let sidebar = html
        .split_once(r#"<aside id="docs-navigation" class="docs-sidebar""#)
        .and_then(|(_, rest)| rest.split_once("</aside>"))
        .map(|(sidebar, _)| sidebar)
        .expect("docs sidebar should render");
    let start_position = sidebar.find("Start here").expect("start group label");
    let generators_position = sidebar
        .find(r#"href="/docs/generators""#)
        .expect("generators link");
    let harvest_label_position = sidebar
        .find(r#"<p class="docs-nav-section-title">Harvest</p>"#)
        .expect("Harvest group label");
    let harvest_intro_position = sidebar
        .find(r#"href="/docs/autumn-harvest""#)
        .expect("Harvest intro docs link");
    let harvest_chapter_position = sidebar
        .find(r#"href="/docs/harvest-project-skeleton""#)
        .expect("Harvest chapter link");
    let request_surface_position = sidebar
        .find("Request surface")
        .expect("request surface group label");
    // generators stays in "Start here"; the dedicated "Harvest" group follows,
    // anchored by the intro and then its chapters, ahead of "Request surface".
    assert!(
        start_position < generators_position
            && generators_position < harvest_label_position
            && harvest_label_position < harvest_intro_position
            && harvest_intro_position < harvest_chapter_position
            && harvest_chapter_position < request_surface_position,
        "the Harvest group should sit after Start here and before Request surface"
    );

    let deployment_position = sidebar
        .find(r#"href="/docs/deployment""#)
        .expect("deployment link");
    assert!(
        !sidebar[deployment_position + r#"href="/docs/deployment""#.len()..]
            .contains(r#"href="/docs/"#),
        "deployment should be the final docs navigation link"
    );
    assert!(!sidebar.contains(r#"href="/docs/docs-smoke""#));
}

#[test]
fn bundled_docs_pagination_follows_grouped_sidebar_order() {
    let registry = autumn_io::site_docs().expect("bundled guide docs should load");

    let framework_page = registry
        .page("coming-from-other-frameworks")
        .expect("framework migration guide should be bundled");
    let framework_html = render_docs_page(registry, framework_page).into_string();
    assert!(framework_html.contains(
        r#"<a class="pagination-link next" href="/docs/generators"><span>Next</span><strong>Code Generators</strong></a>"#
    ));

    let generators_page = registry
        .page("generators")
        .expect("generators guide should be bundled");
    let generators_html = render_docs_page(registry, generators_page).into_string();
    assert!(generators_html.contains(
        r#"<a class="pagination-link previous" href="/docs/coming-from-other-frameworks"><span>Previous</span><strong>Coming From Other Frameworks</strong></a>"#
    ));

    let deployment_page = registry
        .page("deployment")
        .expect("deployment guide should be bundled");
    let deployment_html = render_docs_page(registry, deployment_page).into_string();
    assert!(deployment_html.contains(
        r#"<a class="pagination-link previous" href="/docs/fleet-deploys"><span>Previous</span><strong>Fleet Deploys</strong></a>"#
    ));
    assert!(
        !deployment_html.contains(r#"<a class="pagination-link next""#),
        "deployment should be the terminal docs page"
    );
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
    let harvest = registry
        .page("autumn-harvest")
        .expect("Harvest release page should be bundled");
    assert_eq!(harvest.title, "Autumn Harvest");
    assert!(harvest.html.contains("autumn_harvest::prelude"));
    assert!(harvest.html.contains("HarvestPlugin"));
    // The intro now points into the on-site Harvest guide rather than upstream.
    assert!(
        harvest
            .html
            .contains(r#"href="/docs/harvest-project-skeleton""#)
    );
    assert!(
        registry.page("docs-smoke").is_none(),
        "internal release smoke procedure should not ship as public docs"
    );
    assert!(
        registry.page("quickstart").is_none(),
        "old hand-written quickstart should not shadow the upstream guide snapshot"
    );
}

/// Upstream guides are authored for GitHub, so their in-page and cross-page
/// link fragments use GitHub's anchor convention (`#securedrole`, `#api_doc`) —
/// which is not what this site's renderer emits (`secured-role`, `api-doc`).
/// The sync tool rewrites those onto real heading IDs; this asserts none is
/// left dangling, so a reader who follows one actually lands on the heading.
#[test]
fn vendored_link_fragments_resolve_to_real_heading_ids() {
    let registry = autumn_io::site_docs().expect("bundled guide docs should load");

    // Fragments that are broken upstream too: hand-written stubs matching no
    // heading under either convention, so there is nothing to rewrite them to.
    // The sync tool leaves an unresolvable fragment exactly as authored rather
    // than guessing at a heading and sending the reader somewhere wrong.
    let dangling_upstream = [
        ("mail-compliance", "/docs/mail#deferred-delivery"),
        ("authorization", "/docs/macro-transparency#authorize"),
    ];

    let mut unresolved = Vec::new();
    for page in registry.pages() {
        // Code spans can contain literal markup (`<a href="#panel-id">` is prose
        // in the tabs guide, not a link), so scan only real anchors.
        let prose = strip_code_spans(&page.html);
        for href in html_hrefs(&prose) {
            let Some((path, fragment)) = href.split_once('#') else {
                continue;
            };
            if fragment.is_empty() {
                continue;
            }
            let target_slug = if path.is_empty() {
                page.slug.as_str()
            } else {
                match path.strip_prefix("/docs/") {
                    Some(slug) if !slug.contains('/') => slug,
                    _ => continue,
                }
            };
            let Some(target) = registry.page(target_slug) else {
                continue;
            };
            if target.toc.iter().any(|item| item.id == fragment) {
                continue;
            }
            if dangling_upstream
                .iter()
                .any(|(slug, link)| *slug == page.slug && *link == href)
            {
                continue;
            }
            unresolved.push(format!("{} -> {href}", page.slug));
        }
    }

    assert!(
        unresolved.is_empty(),
        "vendored links point at heading IDs that do not exist: {unresolved:#?}"
    );
}

/// Guide slugs come from upstream file names, so an exact route under
/// `/docs/…` silently shadows any guide that later takes that slug — Axum
/// matches the literal path first and the guide becomes unreachable. Upstream
/// 0.7.0 added `search.md`, which is why the docs-search UI lives at `/search`.
#[test]
fn no_exact_route_shadows_a_bundled_guide_slug() {
    let registry = autumn_io::site_docs().expect("bundled guide docs should load");

    for route in autumn_io::app_routes() {
        let Some(rest) = route.path.strip_prefix("/docs/") else {
            continue;
        };
        assert!(
            rest.starts_with('{'),
            "route `{}` is an exact path under /docs/ and would shadow a guide slug; \
             serve it outside the /docs/{{slug}} namespace",
            route.path
        );
    }

    // The specific collision that motivated this guard: the search guide is
    // reachable, and the search UI has moved off the guide namespace.
    assert!(registry.page("search").is_some());
    assert_eq!(autumn_io::DOCS_SEARCH_PATH, "/search");
}

#[test]
fn bundled_site_docs_include_the_autumn_070_and_harvest_060_guides() {
    let registry = autumn_io::site_docs().expect("bundled guide docs should load");

    // Every guide vendored in the 0.7.0 sync, plus Harvest's new chapter 13.
    for slug in [
        "seo",
        "pdf-downloads",
        "rich-text",
        "commentable",
        "votable",
        "feeds",
        "notifications",
        "search",
        "openapi",
        "authentication",
        "route-auth-coverage",
        "aggregates",
        "counter-cache",
        "ledgered-entities",
        "audit-logging",
        "retention-sweeps",
        "query-budgets",
        "metrics",
        "server-timing",
        "failure-capsules",
        "console",
        "simulation-testing",
        "clustering",
        "upgrading",
        "edge",
        "fleet-deploys",
        "harvest-broker-connectors",
    ] {
        let page = registry
            .page(slug)
            .unwrap_or_else(|| panic!("{slug} guide should be bundled"));
        assert!(!page.title.is_empty(), "{slug} should carry a title");
        assert!(!page.html.is_empty(), "{slug} should render a body");
        assert!(
            is_grouped_docs_nav_slug(slug),
            "{slug} should be slotted into a docs sidebar group"
        );
    }

    // `observability/server-timing.md` is nested upstream; the site's guide
    // namespace is flat, so it is served at its file stem.
    assert_eq!(
        registry
            .page("server-timing")
            .expect("server-timing guide should be bundled")
            .title,
        "Server-Timing response header"
    );

    // The Harvest chapter keeps its cleaned nav title and resolves its sibling
    // cross-links to on-site routes.
    let broker_connectors = registry
        .page("harvest-broker-connectors")
        .expect("harvest broker-connectors chapter should be bundled");
    assert_eq!(broker_connectors.title, "Broker connectors (Kafka, SQS)");
    assert!(
        broker_connectors
            .html
            .contains(r#"href="/docs/harvest-webhooks""#)
    );
}

/// Whether a slug is reachable from the rendered docs sidebar, which only
/// renders pages that a `DOCS_NAV_GROUPS` entry claims (anything else falls
/// into the ungrouped "Reference" catch-all).
fn is_grouped_docs_nav_slug(slug: &str) -> bool {
    let registry = autumn_io::site_docs().expect("bundled guide docs should load");
    let page = registry
        .page("transactions")
        .expect("transactions guide should be bundled");
    let html = render_docs_page(registry, page).into_string();
    let sidebar = html
        .split_once(r#"<aside id="docs-navigation" class="docs-sidebar""#)
        .and_then(|(_, rest)| rest.split_once("</aside>"))
        .map(|(sidebar, _)| sidebar)
        .expect("docs sidebar should render");
    let reference = sidebar
        .find(r#"<p class="docs-nav-section-title">Reference</p>"#)
        .unwrap_or(sidebar.len());

    sidebar[..reference].contains(&format!(r#"href="/docs/{slug}""#))
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
    assert!(html.contains(r#"<link rel="canonical" href="https://autumn-web.app/">"#));
    assert!(html.contains(r#"<link rel="icon" href="/static/img/autumn-mark-68.png?v="#));
    assert!(html.contains(r#"<link rel="stylesheet" href="/static/css/site.css?v="#));
    assert!(html.contains(r#"<script src="/static/js/copy-code.js?v="#));
    assert!(html.contains(r#"src="/static/img/autumn-mark-68.png?v="#));
    assert!(html.contains("<span style=\"color:"));
    assert!(html.contains(r#"<span class="code-language">Rust</span>"#));
    assert!(html.contains(r#"srcset="/static/img/autumn-mark-68.png?v="#));
    assert!(html.contains(r#"/static/img/autumn-mark-136.png?v="#));
    assert!(html.contains(r#"<meta property="og:site_name" content="Autumn">"#));
    assert!(html.contains(
        r#"<meta property="og:image" content="https://autumn-web.app/static/img/autumn-social.png?v="#
    ));
    assert!(html.contains(
        r#"<meta name="twitter:image" content="https://autumn-web.app/static/img/autumn-social.png?v="#
    ));
    assert!(html.contains(r#"<meta name="twitter:card" content="summary">"#));
    assert!(html.contains(r#""@type":"WebSite""#));
    assert!(html.contains(r#""@type":"SoftwareSourceCode""#));
}

#[test]
fn rendered_site_chrome_cache_busts_mutable_static_assets() {
    let registry = DocRegistry::from_sources([DocSource::new("quickstart", QUICKSTART_SOURCE)])
        .expect("valid docs source should parse");
    let html = render_home_page(&registry).into_string();

    assert!(html.contains(r#"<link rel="stylesheet" href="/static/css/site.css?v="#));
    assert!(html.contains(r#"<script src="/static/js/copy-code.js?v="#));
    assert!(html.contains(r#"<link rel="icon" href="/static/img/autumn-mark-68.png?v="#));
    assert!(html.contains(r#"src="/static/img/autumn-mark-68.png?v="#));
    assert!(html.contains(r#"/static/img/autumn-mark-136.png?v="#));
    assert!(html.contains(
        r#"<meta property="og:image" content="https://autumn-web.app/static/img/autumn-social.png?v="#
    ));
    assert!(
        !html.contains(r#"<link rel="stylesheet" href="/static/css/site.css">"#),
        "CSS uses immutable cache headers, so HTML must version the asset URL"
    );
    assert!(
        !html.contains(r#"<script src="/static/js/copy-code.js" defer>"#),
        "JS uses immutable cache headers, so HTML must version the asset URL"
    );
    assert!(
        !html.contains(
            r#"<link rel="icon" href="/static/img/autumn-mark-68.png" type="image/png">"#
        ),
        "image URLs use immutable cache headers, so HTML must version them too"
    );
    assert!(
        !html.contains(r#"<meta property="og:image" content="https://autumn-web.app/static/img/autumn-social.png">"#),
        "social image URLs use immutable cache headers, so metadata must version them too"
    );
}

#[test]
fn rendered_site_chrome_links_to_autumn_sources_and_crate() {
    let registry = DocRegistry::from_sources([DocSource::new("quickstart", QUICKSTART_SOURCE)])
        .expect("valid docs source should parse");
    let html = render_home_page(&registry).into_string();

    assert!(html.contains(r#"href="https://github.com/autumn-foundation/autumn""#));
    assert!(html.contains(r#"href="https://crates.io/crates/autumn-web""#));
    assert!(html.contains(r#"href="https://github.com/autumn-foundation/autumn_io""#));
    assert!(html.contains(r#"href="/docs/autumn-harvest""#));
    assert!(html.contains(r#"href="https://github.com/autumn-foundation/autumn-harvest""#));
    assert!(html.contains(r#"href="https://crates.io/crates/autumn-harvest""#));
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
        r#"href="https://github.com/autumn-foundation/autumn/tree/trunk-dev/examples/todo-app""#
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
        r#"href="https://github.com/autumn-foundation/autumn/tree/trunk-dev/examples/custom_config_loader""#
    ));
    assert!(custom_subsystems.html.contains(
        r#"href="https://github.com/autumn-foundation/autumn/blob/trunk-dev/autumn/src/plugin.rs""#
    ));
    assert!(
        !custom_subsystems
            .html
            .contains(r#"href="../../examples/custom_config_loader""#)
    );

    // The vendored Harvest chapters cross-link back into the autumn-harvest
    // repo for files this site does not vendor; those links are resolved to
    // absolute upstream URLs at sync time.
    let harvest_signals = registry
        .page("harvest-signals")
        .expect("harvest signals chapter should be bundled");
    assert!(
        harvest_signals
            .html
            .contains(r#"href="/docs/harvest-idempotency#idempotent-signal-delivery""#)
    );
    assert!(harvest_signals.html.contains(
        r#"href="https://github.com/autumn-foundation/autumn-harvest/blob/trunk-dev/docs/management-api.md""#
    ));
    // A sibling-chapter link rendered as a bare site route.
    let harvest_first_workflow = registry
        .page("harvest-first-workflow")
        .expect("harvest first-workflow chapter should be bundled");
    assert!(
        harvest_first_workflow
            .html
            .contains(r#"href="/docs/harvest-idempotency""#)
    );
    // The upstream guide index (`README.md`) resolves to the Harvest intro.
    let harvest_testing = registry
        .page("harvest-testing")
        .expect("harvest testing chapter should be bundled");
    assert!(
        harvest_testing
            .html
            .contains(r#"href="/docs/autumn-harvest""#)
    );

    // Upstream `.md`/`.md#` links are only ever rendered as absolute upstream
    // source URLs — into the framework repo for framework guides, or the
    // autumn-harvest repo for the Harvest guide — never as repo-relative or
    // site-local Markdown paths.
    let upstream_markdown_prefixes = [
        "https://github.com/autumn-foundation/autumn/blob/trunk-dev/",
        "https://github.com/autumn-foundation/autumn-harvest/blob/trunk-dev/",
    ];
    for page in registry.pages() {
        for href in html_hrefs(&page.html) {
            assert!(
                !href.starts_with("../") && !href.starts_with("./"),
                "{} should not render repo-relative href {href}",
                page.slug
            );
            let is_upstream_markdown = upstream_markdown_prefixes
                .iter()
                .any(|prefix| href.starts_with(prefix));
            assert!(
                !href.ends_with(".md") || is_upstream_markdown,
                "{} should not render site-local Markdown href {href}",
                page.slug
            );
            assert!(
                !href.contains(".md#") || is_upstream_markdown,
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
fn css_supports_featured_home_cards_and_grouped_docs_nav() {
    assert!(SITE_CSS.contains(".home-harvest"));
    assert!(SITE_CSS.contains(".home-harvest-actions"));
    assert!(SITE_CSS.contains(".home-mcp"));
    assert!(SITE_CSS.contains(".home-mcp-example"));
    assert!(SITE_CSS.contains(".home-featured-grid"));
    assert!(SITE_CSS.contains(".home-feature-card"));
    assert!(SITE_CSS.contains(".home-secondary-grid"));
    assert!(SITE_CSS.contains(".docs-nav-section-title"));
}

#[test]
fn css_places_docs_article_before_large_navigation_on_mobile() {
    assert!(SITE_CSS.contains(".docs-mobile-nav-link"));
    assert!(SITE_CSS.contains(".docs-main {\n    order: 1;"));
    assert!(SITE_CSS.contains(".docs-sidebar {\n    order: 2;"));
    assert!(SITE_CSS.contains("border-top: 1px solid var(--line);"));
}

#[test]
fn css_collapses_docs_nav_disclosure_on_mobile_and_stays_open_on_desktop() {
    // Desktop keeps the current always-open, unmarked label look…
    assert!(SITE_CSS.contains(".docs-nav-summary {"));
    assert!(SITE_CSS.contains("list-style: none;"));
    assert!(SITE_CSS.contains("::-webkit-details-marker"));
    // …the focus outline rule already used for links/buttons also covers
    // the summary, since <summary> isn't matched by either selector.
    assert!(SITE_CSS.contains(".docs-nav-summary:focus-visible,"));
    // …while narrow viewports turn it into a visible, tappable toggle.
    assert!(SITE_CSS.contains(".docs-nav-disclosure[open] .docs-nav-summary::after"));
}

#[test]
fn docs_nav_disclosure_script_defaults_open_state_from_the_1080px_breakpoint() {
    assert!(DOCS_NAV_DISCLOSURE_JS.contains(".docs-nav-disclosure"));
    assert!(DOCS_NAV_DISCLOSURE_JS.contains("(max-width: 1080px)"));
    assert!(DOCS_NAV_DISCLOSURE_JS.contains("matchMedia"));
    // Desktop must stay pixel-identical to today: a user (or assistive tech)
    // toggling the <summary> at desktop widths must not be able to hide the
    // nav, since nothing today lets that happen.
    assert!(DOCS_NAV_DISCLOSURE_JS.contains("toggle"));
    // The in-article "Browse docs" jump link must still reveal the nav.
    assert!(DOCS_NAV_DISCLOSURE_JS.contains("docs-mobile-nav-link"));
}

#[test]
fn css_keeps_markdown_tables_inside_the_article_column() {
    assert!(SITE_CSS.contains(".article-body table"));
    assert!(SITE_CSS.contains("overflow-x: auto;"));
    assert!(SITE_CSS.contains("max-width: 100%;"));
    assert!(SITE_CSS.contains("white-space: nowrap;"));
}

#[test]
fn copy_code_script_updates_accessible_status_text() {
    assert!(COPY_CODE_JS.contains("aria-label"));
    assert!(COPY_CODE_JS.contains("Copied code to clipboard"));
    assert!(COPY_CODE_JS.contains("Select code manually"));
}

#[test]
fn copy_code_script_resets_rapid_clicks_to_the_fixed_ready_label() {
    assert!(COPY_CODE_JS.contains(r#"READY_LABEL = "Copy""#));
    assert!(COPY_CODE_JS.contains("window.clearTimeout"));
    assert!(COPY_CODE_JS.contains("copyResetTimer"));
    assert!(
        !COPY_CODE_JS.contains("const previous = button.textContent"),
        "rapid clicks must not capture the transient Copied label as the restore target"
    );
}

#[tokio::test]
async fn autumn_routes_render_home_docs_redirect_and_missing_docs_page() {
    let app = TestApp::new().routes(autumn_io::app_routes()).build();

    app.get("/")
        .send()
        .await
        .assert_status(200)
        .assert_body_contains("Ship the app, not the plumbing.")
        .assert_body_contains("/static/css/site.css?v=");

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

    // The vendored Autumn Harvest 0.5 guide renders, is grouped under the
    // "Harvest" sidebar heading, and its cleaned chapter title (not the raw
    // "Chapter 1 — …" upstream heading) is what the page shows.
    app.get("/docs/harvest-project-skeleton")
        .send()
        .await
        .assert_status(200)
        .assert_body_contains("<h1 id=\"page-title\">Project skeleton</h1>")
        .assert_body_contains("Harvest")
        .assert_body_contains(r#"href="/docs/harvest-first-workflow""#);

    // A guide vendored from a nested upstream path (`observability/…`) is
    // served at its flat site slug, and Harvest's new chapter 13 closes the
    // chapter sequence.
    app.get("/docs/server-timing")
        .send()
        .await
        .assert_status(200)
        .assert_body_contains("Server-Timing response header");

    app.get("/docs/harvest-broker-connectors")
        .send()
        .await
        .assert_status(200)
        .assert_body_contains("<h1 id=\"page-title\">Broker connectors (Kafka, SQS)</h1>");

    // The search guide is served at its own slug; the docs-search UI lives
    // outside the guide namespace and no longer shadows it.
    app.get("/docs/search")
        .send()
        .await
        .assert_status(200)
        .assert_body_contains("Search: keyword and vector")
        .assert_body_contains("autumn-search");

    app.get("/search")
        .send()
        .await
        .assert_status(200)
        .assert_body_contains("Search the guides");

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

    // Versioned URLs change whenever the bytes do, so they cache permanently.
    for path in [
        "/static/css/site.css?v=test",
        "/static/js/copy-code.js?v=test",
        "/static/img/autumn-social.png?v=test",
        "/static/img/autumn-mark-68.png?v=test",
    ] {
        app.get(path)
            .send()
            .await
            .assert_status(200)
            .assert_header("cache-control", "public, max-age=31536000, immutable");
    }

    // The framework serves its own assets under /static/ at stable, unversioned
    // URLs, and the pages linking them — the framework-rendered /_stories
    // gallery — cannot add a version query. Caching those immutably pinned a
    // returning visitor to the previous release's copy for a year across an
    // autumn-web upgrade, with no way to bust it.
    for path in [
        "/static/css/autumn-widgets.css",
        "/static/js/autumn-widgets.js",
        "/static/css/site.css",
    ] {
        app.get(path)
            .send()
            .await
            .assert_status(200)
            .assert_header("cache-control", "public, max-age=3600, must-revalidate");
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
        // The JSON docs API and its MCP envelope serve the same guides as the
        // HTML pages; indexing them would compete with the pages that rank.
        .assert_body_contains("Disallow: /api/")
        .assert_body_contains("Disallow: /mcp")
        .assert_body_contains("Sitemap: https://autumn-web.app/sitemap.xml");

    let sitemap = app
        .get("/sitemap.xml")
        .send()
        .await
        .assert_status(200)
        .assert_header_contains("content-type", "application/xml")
        .assert_body_contains("<loc>https://autumn-web.app/</loc>")
        .assert_body_contains("<loc>https://autumn-web.app/docs/getting-started</loc>")
        .assert_body_contains("<loc>https://autumn-web.app/docs/autumn-harvest</loc>")
        .assert_body_contains("<loc>https://autumn-web.app/docs/harvest-project-skeleton</loc>")
        .assert_body_contains("<loc>https://autumn-web.app/docs/deployment</loc>")
        .assert_body_contains("<loc>https://autumn-web.app/docs/server-timing</loc>")
        .assert_body_contains("<loc>https://autumn-web.app/docs/fleet-deploys</loc>")
        .assert_body_contains("<loc>https://autumn-web.app/docs/harvest-broker-connectors</loc>")
        .text();

    assert!(
        !sitemap.contains("<loc>https://autumn-web.app/docs</loc>"),
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
    assert!(home.contains("Ship the app, not the plumbing."));
    assert!(home.contains(r#"<link rel="canonical" href="https://autumn-web.app/">"#));

    let getting_started =
        std::fs::read_to_string(dist.join("docs/getting-started/index.html")).expect("docs html");
    assert!(getting_started.contains(r#"<h1 id="page-title">Getting Started with Autumn</h1>"#));
    assert!(getting_started.contains(r#"aria-current="page" href="/docs/getting-started""#));
    assert!(
        getting_started.contains(
            r#"<link rel="canonical" href="https://autumn-web.app/docs/getting-started">"#
        )
    );

    let robots = std::fs::read_to_string(dist.join("robots.txt")).expect("robots file");
    assert!(robots.contains("Sitemap: https://autumn-web.app/sitemap.xml"));

    let sitemap = std::fs::read_to_string(dist.join("sitemap.xml")).expect("sitemap file");
    assert!(sitemap.contains("<loc>https://autumn-web.app/docs/getting-started</loc>"));

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

#[test]
fn export_site_refuses_to_remove_source_like_output_dirs() {
    let workspace = unique_temp_dir("autumn-io-unsafe-export");
    let content_dir = workspace.join("content");
    let static_dir = workspace.join("static");
    std::fs::create_dir_all(&content_dir).expect("content dir");
    std::fs::create_dir_all(&static_dir).expect("static dir");
    std::fs::write(content_dir.join("keep.txt"), "do not delete").expect("content marker");
    std::fs::write(static_dir.join("site.css"), "body {}").expect("static asset");

    let registry = DocRegistry::from_sources([DocSource::new("quickstart", QUICKSTART_SOURCE)])
        .expect("valid docs source should parse");
    let result = export_site(
        &registry,
        &ExportConfig::new(&content_dir).with_static_dir(&static_dir),
    );
    let marker_preserved = content_dir.join("keep.txt").exists();

    std::fs::remove_dir_all(workspace).expect("cleanup unsafe export test");

    assert!(
        matches!(result, Err(ExportError::UnsafeOutputDir(ref path)) if path == &content_dir),
        "export should reject source-like output dirs before deleting them; got {result:?}"
    );
    assert!(
        marker_preserved,
        "unsafe export must not delete existing content"
    );
}

#[test]
fn export_site_refuses_output_dirs_inside_the_static_source_tree() {
    let workspace = unique_temp_dir("autumn-io-static-trap-export");
    let static_dir = workspace.join("static");
    let dist = static_dir.join("dist");
    std::fs::create_dir_all(&static_dir).expect("static dir");
    std::fs::write(static_dir.join("site.css"), "body {}").expect("static asset");

    let registry = DocRegistry::from_sources([DocSource::new("quickstart", QUICKSTART_SOURCE)])
        .expect("valid docs source should parse");
    let result = export_site(
        &registry,
        &ExportConfig::new(&dist).with_static_dir(&static_dir),
    );
    let source_asset_preserved = static_dir.join("site.css").exists();

    std::fs::remove_dir_all(workspace).expect("cleanup static trap export test");

    assert!(
        matches!(result, Err(ExportError::UnsafeOutputDir(ref path)) if path == &dist),
        "export should reject output dirs nested inside static source; got {result:?}"
    );
    assert!(
        source_asset_preserved,
        "unsafe export must not recurse through or delete static assets"
    );
}

#[test]
fn export_site_refuses_output_dirs_that_contain_the_static_source_tree() {
    let workspace = unique_temp_dir("autumn-io-static-contained-export");
    let dist = workspace.join("dist");
    let static_dir = dist.join("static");
    std::fs::create_dir_all(&static_dir).expect("static dir");
    std::fs::write(static_dir.join("site.css"), "body {}").expect("static asset");

    let registry = DocRegistry::from_sources([DocSource::new("quickstart", QUICKSTART_SOURCE)])
        .expect("valid docs source should parse");
    let result = export_site(
        &registry,
        &ExportConfig::new(&dist).with_static_dir(&static_dir),
    );
    let source_asset_preserved = static_dir.join("site.css").exists();

    std::fs::remove_dir_all(workspace).expect("cleanup contained static export test");

    assert!(
        matches!(result, Err(ExportError::UnsafeOutputDir(ref path)) if path == &dist),
        "export should reject output dirs that contain the static source; got {result:?}"
    );
    assert!(
        source_asset_preserved,
        "unsafe export must not delete static assets contained by the output dir"
    );
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

/// Drop the contents of `<code>` elements. Inline code keeps its quotes
/// unescaped, so markup quoted as prose would otherwise read as real markup.
fn strip_code_spans(html: &str) -> String {
    let mut output = String::with_capacity(html.len());
    let mut rest = html;

    while let Some(open) = rest.find("<code") {
        output.push_str(&rest[..open]);
        let after = &rest[open..];
        match after.find("</code>") {
            Some(close) => rest = &after[close + "</code>".len()..],
            None => return output,
        }
    }

    output.push_str(rest);
    output
}

fn html_hrefs(html: &str) -> impl Iterator<Item = &str> {
    html.split("href=\"")
        .skip(1)
        .filter_map(|segment| segment.split('"').next())
}

/// The MCP endpoint is only useful to someone who knows it exists, and the
/// people who would point an agent at it are reading the home page. The snippet
/// is generated from the mount path, so this also catches the two drifting.
#[test]
fn home_page_advertises_the_docs_mcp_endpoint() {
    let registry = autumn_io::site_docs().expect("bundled guide docs should load");
    let html = render_home_page(registry).into_string();
    let endpoint = format!("https://autumn-web.app{}", autumn_io::MCP_MOUNT_PATH);

    assert!(html.contains("Point your agent at these docs"));
    assert!(
        html.matches(&endpoint).count() >= 2,
        "the prose and the copyable snippet should both name {endpoint}"
    );
    // Code blocks are syntax-highlighted, so the command is split across
    // `<span>`s; compare against the text a visitor would actually copy.
    let code_text = html.replace("</span>", "");
    let code_text = code_text
        .split("<span")
        .map(|chunk| chunk.split_once('>').map_or(chunk, |(_, rest)| rest))
        .collect::<String>();
    assert!(code_text.contains("claude mcp add --transport http autumn-docs"));
    // Quotes survive as `&quot;` entities, so match the bare key name.
    assert!(code_text.contains("mcpServers"));
    assert!(
        html.contains(r#"href="/docs/mcp""#),
        "the band should link to the guide explaining the mechanism"
    );
}
