use autumn_web::{Markup, PreEscaped, html};

use crate::docs::{DocPage, DocRegistry, render_highlighted_code_block};
use crate::{DOCS_START_PATH, seo};

const VERSION_LABEL: &str = "Autumn 0.4.0";
const HARVEST_DOC_PATH: &str = "/docs/autumn-harvest";
const ASSET_VERSION: &str = env!("AUTUMN_IO_ASSET_VERSION");
const BRAND_MARK_1X_PATH: &str = "/static/img/autumn-mark-68.png";
const BRAND_MARK_2X_PATH: &str = "/static/img/autumn-mark-136.png";
const HOME_FEATURED_DOC_SLUGS: &[&str] = &["getting-started", "coming-from-other-frameworks"];
const HOME_SECONDARY_DOC_SLUGS: &[&str] = &[
    "what-happens-when",
    "testing",
    "authorization",
    "jobs",
    "cloud-native",
    "deployment",
];

struct DocsNavGroup {
    label: &'static str,
    slugs: &'static [&'static str],
}

const DOCS_NAV_GROUPS: &[DocsNavGroup] = &[
    DocsNavGroup {
        label: "Start here",
        slugs: &[
            "getting-started",
            "what-happens-when",
            "autumn-harvest",
            "coming-from-other-frameworks",
            "generators",
        ],
    },
    DocsNavGroup {
        label: "Request surface",
        slugs: &[
            "accessibility",
            "middleware",
            "path-helpers",
            "routes-cli",
            "macro-transparency",
        ],
    },
    DocsNavGroup {
        label: "Data and auth",
        slugs: &[
            "testing",
            "transactions",
            "seeding",
            "storage",
            "mail",
            "authorization",
            "signed-webhooks",
            "signing-secrets",
        ],
    },
    DocsNavGroup {
        label: "Realtime and jobs",
        slugs: &[
            "realtime",
            "websockets",
            "jobs",
            "tasks",
            "operating-background-jobs",
            "scheduled-multi-replica",
            "admin",
        ],
    },
    DocsNavGroup {
        label: "Extending and shipping",
        slugs: &[
            "custom-subsystems",
            "extensibility",
            "cloud-native",
            "i18n",
            "deployment",
        ],
    },
];

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
                            h1 id="page-title" { "Ship the app, not the plumbing." }
                            p class="hero-lede" {
                                "Autumn gives Rust teams the batteries they expect from mature app frameworks: typed routes, Maud views, Postgres persistence, background work, health checks, and production defaults in one server-rendered path."
                            }
                            div class="hero-actions" {
                                a class="button button-primary" href=(DOCS_START_PATH) { "Get started" }
                                a class="button button-secondary" href="/docs/what-happens-when" { "Read the docs" }
                            }
                        }
                        (PreEscaped(render_highlighted_code_block(Some("rust"), HOME_ROUTE_EXAMPLE)))
                    }
                    (home_harvest_release())
                    section class="home-featured" aria-labelledby="featured-guides-title" {
                        div class="home-section-header" {
                            p class="eyebrow" { "Start with intent" }
                            h2 id="featured-guides-title" { "Pick your entry point" }
                        }
                        div class="home-featured-grid" {
                            @for slug in HOME_FEATURED_DOC_SLUGS {
                                @if let Some(page) = registry.page(slug) {
                                    (home_feature_card(page))
                                }
                            }
                        }
                    }
                    section class="home-secondary" aria-labelledby="common-paths-title" {
                        div class="home-section-header" {
                            p class="eyebrow" { "Core workflows" }
                            h2 id="common-paths-title" { "Build, test, secure, and deploy" }
                        }
                        div class="home-secondary-grid" {
                            @for page in home_secondary_pages(registry) {
                                a class="home-secondary-link" href=(seo::docs_path(&page.slug)) {
                                    span class="feature-title" { (&page.title) }
                                    span class="feature-description" { (&page.description) }
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

fn home_harvest_release() -> Markup {
    html! {
        section class="home-harvest" aria-labelledby="harvest-release-title" {
            div class="home-harvest-copy" {
                p class="eyebrow" { "Companion release" }
                h2 id="harvest-release-title" { "Autumn Harvest " (seo::HARVEST_VERSION) }
                p {
                    "Harvest adds Postgres-backed durable workflows to Autumn: activities, timers, signals, child workflows, DAG schedules, replay, dead letters, and a management API without operating a separate workflow server."
                }
            }
            div class="home-harvest-actions" {
                a class="button button-primary" href=(HARVEST_DOC_PATH) { "Read Harvest overview" }
                a class="button button-secondary" href=(seo::HARVEST_GUIDE_URL) { "Guide" }
                a class="button button-secondary" href=(seo::HARVEST_RUSTDOC_URL) { "API docs" }
                a class="button button-secondary" href=(seo::HARVEST_CRATES_IO_URL) { "Crate" }
            }
        }
    }
}

fn home_feature_card(page: &DocPage) -> Markup {
    let kicker = match page.slug.as_str() {
        "getting-started" => "Build first",
        "coming-from-other-frameworks" => "Map what you know",
        _ => "Guide",
    };

    html! {
        a class="home-feature-card" href=(seo::docs_path(&page.slug)) {
            span class="home-card-kicker" { (kicker) }
            h2 class="home-card-title" { (&page.title) }
            p { (&page.description) }
            span class="home-card-action" { "Read guide" }
        }
    }
}

fn home_secondary_pages(registry: &DocRegistry) -> Vec<&DocPage> {
    let pages = HOME_SECONDARY_DOC_SLUGS
        .iter()
        .filter_map(|slug| registry.page(slug))
        .collect::<Vec<_>>();

    if pages.is_empty() {
        registry
            .pages()
            .iter()
            .filter(|page| !HOME_FEATURED_DOC_SLUGS.contains(&page.slug.as_str()))
            .take(6)
            .collect()
    } else {
        pages
    }
}

pub fn render_docs_page(registry: &DocRegistry, page: &DocPage) -> Markup {
    let (previous_page, next_page) = docs_navigation_neighbors(registry, &page.slug);

    html! {
        (doctype())
        html lang="en" {
            (document_head(&PageMeta::docs(page)))
            body class="site-shell docs-shell" {
                (skip_link())
                (site_header("docs"))
                div class="docs-layout" {
                    (docs_sidebar(registry, Some(&page.slug)))
                    main id="main-content" class="docs-main" tabindex="-1" aria-labelledby="page-title" {
                        article class="docs-article" aria-labelledby="page-title" {
                            header class="article-header" {
                                p class="eyebrow" { (VERSION_LABEL) }
                                h1 id="page-title" { (&page.title) }
                                p { (&page.description) }
                                a class="docs-mobile-nav-link" href="#docs-navigation" {
                                    "Browse docs"
                                }
                            }
                            div class="article-body" {
                                (PreEscaped(page.html.clone()))
                            }
                        }
                        nav class="docs-pagination" aria-label="Docs pagination" {
                            @if let Some(previous) = previous_page {
                                a class="pagination-link previous" href=(format!("/docs/{}", previous.slug)) {
                                    span { "Previous" }
                                    strong { (&previous.title) }
                                }
                            } @else {
                                span class="pagination-placeholder" {}
                            }
                            @if let Some(next) = next_page {
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

fn docs_navigation_neighbors<'a>(
    registry: &'a DocRegistry,
    slug: &str,
) -> (Option<&'a DocPage>, Option<&'a DocPage>) {
    let pages = docs_navigation_pages(registry);
    let Some(index) = pages.iter().position(|page| page.slug == slug) else {
        return (None, None);
    };

    let previous = index
        .checked_sub(1)
        .and_then(|previous| pages.get(previous).copied());
    let next = pages.get(index + 1).copied();

    (previous, next)
}

fn docs_navigation_pages(registry: &DocRegistry) -> Vec<&DocPage> {
    let mut pages = Vec::new();
    let mut seen_slugs = Vec::new();

    for group in DOCS_NAV_GROUPS {
        for slug in group.slugs {
            if seen_slugs.contains(slug) {
                continue;
            }

            if let Some(page) = registry.page(slug) {
                seen_slugs.push(page.slug.as_str());
                pages.push(page);
            }
        }
    }

    for page in registry.pages() {
        if !seen_slugs.contains(&page.slug.as_str()) {
            seen_slugs.push(page.slug.as_str());
            pages.push(page);
        }
    }

    pages
}

fn docs_sidebar(registry: &DocRegistry, active_slug: Option<&str>) -> Markup {
    html! {
        aside id="docs-navigation" class="docs-sidebar" aria-label="Docs navigation" {
            nav {
                p class="sidebar-label" { "Docs" }
                @for group in DOCS_NAV_GROUPS {
                    @if docs_nav_group_has_pages(registry, group) {
                        section class="docs-nav-section" {
                            p class="docs-nav-section-title" { (group.label) }
                            @for slug in group.slugs {
                                @if let Some(page) = registry.page(slug) {
                                    (docs_nav_link(page, active_slug))
                                }
                            }
                        }
                    }
                }
                @if registry.pages().iter().any(|page| !is_grouped_doc_slug(&page.slug)) {
                    section class="docs-nav-section" {
                        p class="docs-nav-section-title" { "Reference" }
                        @for page in registry.pages() {
                            @if !is_grouped_doc_slug(&page.slug) {
                                (docs_nav_link(page, active_slug))
                            }
                        }
                    }
                }
            }
        }
    }
}

fn docs_nav_link(page: &DocPage, active_slug: Option<&str>) -> Markup {
    if active_slug == Some(page.slug.as_str()) {
        html! {
            a
                class="docs-nav-link active"
                aria-current="page"
                href=(seo::docs_path(&page.slug))
            {
                span { (&page.title) }
            }
        }
    } else {
        html! {
            a
                class="docs-nav-link"
                href=(seo::docs_path(&page.slug))
            {
                span { (&page.title) }
            }
        }
    }
}

fn docs_nav_group_has_pages(registry: &DocRegistry, group: &DocsNavGroup) -> bool {
    group.slugs.iter().any(|slug| registry.page(slug).is_some())
}

fn is_grouped_doc_slug(slug: &str) -> bool {
    DOCS_NAV_GROUPS
        .iter()
        .any(|group| group.slugs.contains(&slug))
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
                    (docs_sidebar(registry, None))
                    main id="main-content" class="docs-main" tabindex="-1" aria-labelledby="page-title" {
                        article class="docs-article missing-page" aria-labelledby="page-title" {
                            p class="eyebrow" { "404" }
                            h1 id="page-title" { "That docs page is not in the stack" }
                            p {
                                "No Autumn docs page exists for "
                                code { (slug) }
                                ". The route is valid; the page is not."
                            }
                            a class="button button-primary" href=(DOCS_START_PATH) { "Back to Getting Started" }
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
    let image_url = seo::site_image_url(ASSET_VERSION);
    let icon_path = versioned_asset_path(BRAND_MARK_1X_PATH);
    let stylesheet_path = versioned_asset_path("/static/css/site.css");
    let copy_code_script_path = versioned_asset_path("/static/js/copy-code.js");

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
            link rel="icon" href=(icon_path) type="image/png";
            link rel="sitemap" type="application/xml" href="/sitemap.xml";
            link rel="stylesheet" href=(stylesheet_path);
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
            script src=(copy_code_script_path) defer {}
        }
    }
}

fn versioned_asset_path(path: &str) -> String {
    format!("{path}?v={ASSET_VERSION}")
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
    let brand_mark_1x = versioned_asset_path(BRAND_MARK_1X_PATH);
    let brand_mark_2x = versioned_asset_path(BRAND_MARK_2X_PATH);
    let brand_srcset = format!("{brand_mark_1x} 1x, {brand_mark_2x} 2x");

    html! {
        header class="site-header" {
            a class="brand" href="/" aria-label="Autumn home" {
                img
                    src=(brand_mark_1x)
                    srcset=(brand_srcset)
                    alt=""
                    width="34"
                    height="34";
                span { "Autumn" }
            }
            nav class="site-nav" aria-label="Primary navigation" {
                @if active == "docs" {
                    a class="active" aria-current="location" href=(DOCS_START_PATH) { "Docs" }
                } @else {
                    a href=(DOCS_START_PATH) { "Docs" }
                }
                a href=(HARVEST_DOC_PATH) { "Harvest" }
                a href=(DOCS_START_PATH) { "0.4.0" }
                a href=(seo::GITHUB_REPOSITORY_URL) { "GitHub" }
                a href=(seo::CRATES_IO_URL) { "crates.io" }
                a href="/docs/deployment" { "Deploy" }
            }
        }
    }
}

fn site_footer() -> Markup {
    html! {
        footer class="site-footer" {
            span {
                "Built with "
                a href=(seo::GITHUB_REPOSITORY_URL) { "Autumn" }
                "."
            }
            a href=(seo::WEBSITE_REPOSITORY_URL) { "Site source" }
            a href=(DOCS_START_PATH) { "Getting Started" }
            a href=(HARVEST_DOC_PATH) { "Harvest" }
            a href=(seo::HARVEST_REPOSITORY_URL) { "Harvest source" }
            a href="/docs/deployment" { "Deployment" }
        }
    }
}
