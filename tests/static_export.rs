//! The exported bundle, checked against what the origin actually serves.
//!
//! When Cloudflare serves `dist/`, the bytes a reader gets are the exporter's,
//! not the origin's. Those two must not be able to drift — and the good news is
//! that they structurally cannot drift *in the body*, because `export_site` and
//! the route handlers call the same `site::render_*` functions in the same
//! binary. This file pins that, cheaply, so a future change that gives the
//! exporter its own rendering path fails here.
//!
//! What can drift is everything *around* the body, because a static host runs
//! none of the origin's middleware and none of its router:
//!
//! | The origin does it with | A CDN needs |
//! | --- | --- |
//! | `autumn-web`'s security middleware | a `_headers` file |
//! | the `docs_index` handler's 307 | a `_redirects` file |
//! | `docs_page`'s not-found arm | a pre-rendered `404.html` |
//! | the framework serving htmx from memory | that file written into the bundle |
//!
//! Each of those is asserted below against the origin's own behaviour, so the
//! bundle cannot quietly lose a protection or a route that the origin has.

use std::path::{Path, PathBuf};

use autumn_web::test::TestApp;

use autumn_io::export::{ExportConfig, MISSING_PAGE_FILE, export_site};
use autumn_io::site;

/// A unique scratch directory, removed at the end of each test.
fn temp_dir(label: &str) -> PathBuf {
    let unique = format!(
        "autumn-io-{label}-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock is after the epoch")
            .as_nanos(),
    );
    std::env::temp_dir().join(unique)
}

/// Export the site into `workspace/dist` and hand back that path.
fn export_to(workspace: &Path) -> PathBuf {
    let registry = autumn_io::site_docs().expect("the bundled guides parse");
    let config = ExportConfig::new(workspace.join("dist")).with_static_dir(PathBuf::from("static"));
    export_site(registry, &config).expect("export succeeds");
    workspace.join("dist")
}

fn read(dist: &Path, relative: &str) -> String {
    std::fs::read_to_string(dist.join(relative))
        .unwrap_or_else(|error| panic!("{relative} should exist in the export: {error}"))
}

#[test]
fn every_exported_page_is_what_the_origin_renders() {
    // The bodies come from the same `site::render_*` calls in the same binary,
    // so this is exact rather than approximate. It is here to stay exact.
    let workspace = temp_dir("export-parity");
    let dist = export_to(&workspace);
    let registry = autumn_io::site_docs().expect("the bundled guides parse");

    assert_eq!(
        read(&dist, "index.html"),
        site::render_home_page(registry).into_string(),
        "the exported home page differs from what the origin renders",
    );

    for page in registry.pages() {
        let exported = read(&dist, &format!("docs/{}/index.html", page.slug));
        assert_eq!(
            exported,
            site::render_docs_page(registry, page).into_string(),
            "the exported /docs/{} differs from what the origin renders",
            page.slug,
        );
    }

    assert_eq!(read(&dist, "robots.txt"), autumn_io::seo::robots_txt());
    assert_eq!(
        read(&dist, "sitemap.xml"),
        autumn_io::seo::sitemap_xml(registry)
    );

    std::fs::remove_dir_all(&workspace).expect("cleanup");
}

#[tokio::test]
async fn the_headers_file_carries_exactly_what_the_origin_adds() {
    // The replacement for the edge lane's conformance check, and the reason it
    // still exists after the capsule was dropped: a CDN runs no middleware, so
    // without `_headers` every statically served page would carry five fewer
    // protections than the same page from the origin.
    //
    // Asserted in both directions. A header the origin adds and the file omits
    // is a protection silently lost; a header the file has and the origin does
    // not send is the export inventing policy of its own.
    let workspace = temp_dir("export-headers");
    let headers_file = read(&export_to(&workspace), "_headers");

    let app = TestApp::new().routes(autumn_io::app_routes()).build();
    let response = app.get("/docs/getting-started").send().await;

    // Per-response headers, which a static host computes for itself from the
    // file it is serving, and genuinely volatile ones the origin stamps per
    // request. Everything else is policy and must survive into `_headers`.
    //
    // `content-security-policy` is deliberately NOT excused here. It looks
    // volatile — it is on `autumn_edge::conformance::VOLATILE_HEADERS`, which
    // this list was first copied from — but that list exists for a wasm capsule
    // that structurally cannot emit it. autumn-web's CSP is a constant, and a
    // static bundle can carry it verbatim, so excusing it would have dropped
    // the strongest header in the set for a reason that does not apply.
    let per_response = ["content-type", "content-length", "etag", "vary"];
    let volatile = ["date", "x-request-id", "server-timing"];

    let mut unaccounted = Vec::new();
    for (name, value) in &response.headers {
        let name = name.to_ascii_lowercase();
        if per_response.contains(&name.as_str()) || volatile.contains(&name.as_str()) {
            continue;
        }
        if !headers_file.contains(&format!("{name}: {value}")) {
            unaccounted.push(format!("{name}: {value}"));
        }
    }
    assert!(
        unaccounted.is_empty(),
        "the origin sends {unaccounted:?}, which the static bundle would not. \
         Add them to edge/security-headers.json.",
    );

    // And nothing invented: every entry in the file must be a header the origin
    // actually sends.
    for line in headers_file.lines().filter(|line| line.starts_with("  ")) {
        let (name, value) = line.trim().split_once(": ").expect("`name: value`");
        assert_eq!(
            response.header(name),
            Some(value),
            "_headers claims `{name}: {value}` but the origin does not send it",
        );
    }

    std::fs::remove_dir_all(&workspace).expect("cleanup");
}

#[tokio::test]
async fn the_redirects_file_matches_the_origin_route_it_replaces() {
    let workspace = temp_dir("export-redirects");
    let redirects = read(&export_to(&workspace), "_redirects");

    let app = TestApp::new().routes(autumn_io::app_routes()).build();
    let response = app.get("/docs").send().await;

    let target = response
        .header("location")
        .expect("the origin redirects /docs");
    assert_eq!(
        redirects.trim(),
        format!("/docs {target} {}", response.status.as_u16()),
        "the redirect rule must match the status and target the origin uses",
    );

    std::fs::remove_dir_all(&workspace).expect("cleanup");
}

#[test]
fn the_missing_page_is_the_site_own_404_not_a_bare_one() {
    // A reader who mistypes a slug should land on a page that still looks like
    // the docs — sidebar, nav, a way back — rather than the CDN's generic 404.
    let workspace = temp_dir("export-404");
    let missing = read(&export_to(&workspace), MISSING_PAGE_FILE);

    assert!(missing.contains("<!doctype html>"), "should be a full page");
    assert!(
        missing.contains("/docs/getting-started"),
        "should offer a way back into the guides",
    );

    std::fs::remove_dir_all(&workspace).expect("cleanup");
}

#[test]
fn no_exported_page_references_a_missing_asset() {
    // The general form of a bug Codex caught in review: every docs page has a
    // `<script src="/static/js/htmx.min.js">`, but htmx is not in this repo's
    // `static/` tree — the framework embeds it and serves it from memory. The
    // bundle therefore shipped without it, and the moment `/static/*` is served
    // by the CDN rather than the origin, the docs search box would have been
    // dead on every page.
    //
    // Checking references rather than a hand-listed set is what makes this
    // durable: any future page that links something the export does not write
    // fails here, whatever it is.
    let workspace = temp_dir("export-assets");
    let dist = export_to(&workspace);

    let mut missing: Vec<String> = Vec::new();
    let mut checked = 0;

    for page in ["index.html", "docs/getting-started/index.html", "404.html"] {
        let html = read(&dist, page);
        for reference in asset_references(&html) {
            checked += 1;
            // Strip the cache-busting `?v=…` that `site::versioned_asset_path`
            // adds; the file on disk has no query.
            let relative = reference.trim_start_matches('/');
            if !dist.join(relative).exists() {
                missing.push(format!("{page} references /{relative}"));
            }
        }
    }

    assert!(
        checked > 0,
        "the extractor found no asset references at all"
    );
    assert!(
        missing.is_empty(),
        "the bundle is missing files its own pages link to:\n  {}",
        missing.join("\n  "),
    );

    std::fs::remove_dir_all(&workspace).expect("cleanup");
}

/// Every same-origin `href`/`src` an exported page points at, query stripped.
///
/// Attribute values only — a `/static/...` path mentioned inside a `<code>`
/// block is prose, not a reference, and the guides are full of those.
fn asset_references(html: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (attribute, _) in [("href=\"", ()), ("src=\"", ())] {
        let mut rest = html;
        while let Some(start) = rest.find(attribute) {
            rest = &rest[start + attribute.len()..];
            let Some(end) = rest.find('"') else { break };
            let value = &rest[..end];
            rest = &rest[end..];

            // Same-origin asset paths only: skip absolute URLs, fragments, and
            // the rendered pages themselves (which are directories, not files).
            if !value.starts_with("/static/") {
                continue;
            }
            let path = value.split('?').next().unwrap_or(value);
            found.push(path.to_owned());
        }
    }
    found.sort();
    found.dedup();
    found
}

#[test]
fn exported_urls_are_slashless() {
    // What `html_handling = "drop-trailing-slash"` in `edge/wrangler.toml` is
    // built on. Pages are stored as `docs/{slug}/index.html`, so a static host
    // has to be told which URL shape to serve them at — and Cloudflare's
    // default is the other one, which would redirect every guide navigation to
    // a trailing-slash URL that the page's own canonical tag contradicts.
    //
    // If this site ever starts generating trailing-slash URLs, that setting
    // becomes wrong, and this fails rather than the redirect quietly reversing.
    let workspace = temp_dir("export-urls");
    let dist = export_to(&workspace);
    let registry = autumn_io::site_docs().expect("the bundled guides parse");

    let page = read(&dist, "docs/getting-started/index.html");
    assert!(
        page.contains(
            r#"<link rel="canonical" href="https://autumn-web.app/docs/getting-started">"#
        ),
        "the canonical URL should be slashless",
    );

    let sitemap = read(&dist, "sitemap.xml");
    let mut trailing: Vec<&str> = Vec::new();
    for entry in sitemap.split("<loc>").skip(1) {
        let url = entry.split("</loc>").next().unwrap_or_default();
        // The site root is the one URL that legitimately ends in a slash.
        if url.ends_with('/') && url != "https://autumn-web.app/" {
            trailing.push(url);
        }
    }
    assert!(
        trailing.is_empty(),
        "sitemap entries should be slashless, found {trailing:?}",
    );

    for page in registry.pages().iter().take(20) {
        let html = read(&dist, &format!("docs/{}/index.html", page.slug));
        assert!(
            !html.contains(&format!(r#"href="/docs/{}/""#, page.slug)),
            "/docs/{} links to itself with a trailing slash",
            page.slug,
        );
    }

    std::fs::remove_dir_all(&workspace).expect("cleanup");
}
