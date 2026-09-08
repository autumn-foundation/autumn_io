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
    // without `_headers` every statically served page would carry four fewer
    // protections than the same page from the origin.
    //
    // Asserted in both directions. A header the origin adds and the file omits
    // is a protection silently lost; a header the file has and the origin does
    // not send is the export inventing policy of its own.
    let workspace = temp_dir("export-headers");
    let headers_file = read(&export_to(&workspace), "_headers");

    let app = TestApp::new().routes(autumn_io::app_routes()).build();
    let response = app.get("/docs/getting-started").send().await;

    // Headers the origin sends that are not part of the rendered response
    // itself — i.e. the ones its middleware stamps on.
    let body_headers = ["content-type", "content-length", "etag", "vary"];
    let volatile = [
        "date",
        "x-request-id",
        "server-timing",
        "content-security-policy",
    ];

    let mut unaccounted = Vec::new();
    for (name, value) in &response.headers {
        let name = name.to_ascii_lowercase();
        if body_headers.contains(&name.as_str()) || volatile.contains(&name.as_str()) {
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
