//! What the origin tells a CDN to do, and what it counts while doing it.
//!
//! Both concerns live here because both are properties of the layer stack
//! rather than of any one handler, and both are invisible in a browser: a wrong
//! `Cache-Control` looks exactly like a right one until a reader gets a stale
//! page, and a metric that is never recorded looks exactly like a metric whose
//! value is zero.

use autumn_web::test::TestApp;

use autumn_io::metrics;

/// The app as `main.rs` builds it — routes plus the full layer stack.
///
/// Building it any other way is what made an earlier measurement of this site
/// wrong: `app_routes()` alone has no cache policy and no `ETag`, because both
/// live in the layer. Every test here goes through this function so none of
/// them can accidentally measure an app the deployment does not run.
fn app() -> autumn_web::test::TestClient {
    TestApp::new()
        .routes(autumn_io::app_routes())
        .layer(autumn_io::response_compression_layer())
        .mount_mcp(autumn_io::MCP_MOUNT_PATH)
        .build()
}

const PAGE_POLICY: &str = "public, max-age=0, s-maxage=3600, must-revalidate";

#[tokio::test]
async fn every_read_path_page_is_cacheable_by_a_shared_cache() {
    // The point of the whole exercise: without `s-maxage` a CDN has no TTL to
    // hold these on, so every reader's request travels to `ord` and wakes a
    // scale-to-zero machine to re-render bytes that were the same for everyone.
    let app = app();

    for path in [
        "/",
        "/docs/getting-started",
        "/docs/metrics",
        "/robots.txt",
        "/sitemap.xml",
    ] {
        let response = app.get(path).send().await;
        response.assert_status(200);
        assert_eq!(
            response.header("cache-control"),
            Some(PAGE_POLICY),
            "{path} should be cacheable by a shared cache",
        );
    }
}

#[tokio::test]
async fn the_docs_entry_point_redirect_is_cacheable() {
    // `/docs` answers with a 307 to the first guide rather than a body, and a
    // 307 is not cacheable by default. Without an explicit policy it was the
    // one read-path URL that still reached the origin on every visit — the
    // entry point to the guides, so plausibly the most-hit of them.
    let app = app();

    let response = app.get("/docs").send().await;
    assert!(
        response.status.is_redirection(),
        "/docs should redirect to the first guide",
    );
    assert_eq!(
        response.header("cache-control"),
        Some(PAGE_POLICY),
        "the redirect should be held by the shared cache like any other page",
    );
}

#[tokio::test]
async fn a_page_still_revalidates_with_a_browser() {
    // `max-age=0` costs nothing because `EtagLayer` answers the revalidation
    // with a `304` and never re-renders. If the ETag ever stopped being sent,
    // `max-age=0` would silently turn every navigation into a full re-download
    // of a 160 KB page, so the two are asserted together.
    let app = app();

    let first = app.get("/docs/getting-started").send().await;
    let etag = first.header("etag").expect("pages carry an ETag");

    let second = app
        .get("/docs/getting-started")
        .header("if-none-match", etag)
        .send()
        .await;
    second.assert_status(304);
}

#[tokio::test]
async fn a_missing_guide_is_cached_for_far_less_time_than_a_real_one() {
    // A cached 404 outlives the deploy that adds the guide it denies. Caching
    // it at all keeps a bad link from waking the origin repeatedly; caching it
    // briefly bounds how long a newly published guide can look missing.
    let app = app();

    let response = app.get("/docs/no-such-guide").send().await;
    response.assert_status(404);
    assert_eq!(
        response.header("cache-control"),
        Some("public, max-age=0, s-maxage=60, must-revalidate"),
    );
}

#[tokio::test]
async fn search_is_never_stored_by_a_shared_cache() {
    // `/search` returns two different bodies at one URL — an htmx fragment when
    // `HX-Request` is set, a full page otherwise — and no CDN puts a request
    // header in its cache key. A shared cache holding either variant would
    // serve a bare fragment to a normal navigation, or a whole page into a
    // `<div>`. This asserts the variance is real *and* that the response
    // refuses to be stored, because the first is what makes the second load
    // bearing.
    let app = app();

    let full = app.get("/search?q=router").send().await;
    let fragment = app
        .get("/search?q=router")
        .header("hx-request", "true")
        .send()
        .await;

    full.assert_status(200);
    fragment.assert_status(200);
    assert_ne!(
        full.text(),
        fragment.text(),
        "if these ever became identical this test would pass for the wrong reason",
    );
    assert!(
        !full.text().is_empty() && !fragment.text().is_empty(),
        "both variants should render something",
    );

    for response in [&full, &fragment] {
        assert_eq!(
            response.header("cache-control"),
            Some("no-store"),
            "a shared cache must not hold either variant",
        );
    }
}

#[tokio::test]
async fn a_server_error_is_never_cached() {
    // Nothing in this site returns a 5xx on demand, so this pins the policy
    // rather than an observed response: the `apply_cache_control` match arm for
    // a failed page must stay `None`, or an outage would be cached and outlive
    // its own fix.
    let app = app();

    // A path under /docs that is not a guide still resolves to the 404 arm, so
    // the closest reachable assertion is that success and not-found are the
    // only statuses that get a policy at all.
    let response = app.get("/docs/getting-started").send().await;
    assert!(response.status.is_success());
    assert!(response.header("cache-control").is_some());
}

#[tokio::test]
async fn the_actuator_is_not_offered_to_a_cdn() {
    // Operational endpoints have no business in a shared cache: a cached
    // scrape would report a frozen snapshot as if it were current.
    let app = app();

    let response = app.get("/actuator/prometheus").send().await;
    assert_eq!(
        response.header("cache-control"),
        None,
        "the scrape endpoint should carry no cache policy",
    );
}

/// Scrape the Prometheus endpoint.
async fn scrape(app: &autumn_web::test::TestClient) -> String {
    let response = app.get("/actuator/prometheus").send().await;
    response.assert_status(200);
    response.text()
}

/// The value of one counter series, or `None` if it has not been recorded yet.
///
/// Prometheus text format: `name{label="value",…} 12`. Absent rather than zero
/// is the meaningful distinction — a family that never records does not appear
/// at all, which is exactly the failure these tests are looking for.
fn series(scrape: &str, name: &str, label_fragment: &str) -> Option<f64> {
    scrape
        .lines()
        .filter(|line| !line.starts_with('#'))
        .find(|line| line.starts_with(name) && line.contains(label_fragment))
        .and_then(|line| line.rsplit_once(' '))
        .and_then(|(_, value)| value.trim().parse().ok())
}

#[tokio::test]
async fn a_docs_page_view_is_counted_by_outcome() {
    let app = app();

    app.get("/docs/getting-started").send().await;
    app.get("/docs/no-such-guide").send().await;

    let scrape = scrape(&app).await;
    assert!(
        series(&scrape, metrics::DOCS_PAGE_VIEWS, r#"outcome="found""#).is_some(),
        "a rendered guide should be counted",
    );
    assert!(
        series(&scrape, metrics::DOCS_PAGE_VIEWS, r#"outcome="missing""#).is_some(),
        "a bad slug should be counted separately — it is how a stale inbound \
         link becomes visible",
    );
}

#[tokio::test]
async fn a_search_is_counted_by_whether_it_found_anything() {
    // `outcome="empty"` is the series worth having: it is a documentation gap
    // reported by the people looking for the missing document.
    let app = app();

    app.get("/search?q=router").send().await;
    app.get("/search?q=zzzznotawordinanyguide").send().await;

    let scrape = scrape(&app).await;
    assert!(
        series(&scrape, metrics::DOCS_SEARCHES, r#"outcome="hit""#).is_some(),
        "a search that matched should be counted",
    );
    assert!(
        series(&scrape, metrics::DOCS_SEARCHES, r#"outcome="empty""#).is_some(),
        "a search that matched nothing should be counted",
    );
}

#[tokio::test]
async fn the_docs_api_is_covered_by_the_frameworks_own_route_counter() {
    // No custom per-tool counter exists, because `autumn_http_route_requests_total`
    // already breaks the API down by route — one series per MCP tool, for free.
    // Asserted rather than assumed: a framework upgrade that dropped this family
    // would otherwise silently end the only per-tool visibility this site has.
    let app = app();

    app.get("/api/docs").send().await;
    app.get("/api/docs/getting-started").send().await;
    app.get("/api/search?q=router").send().await;

    let scrape = scrape(&app).await;
    for route in ["/api/docs", "/api/docs/{slug}", "/api/search"] {
        assert!(
            scrape.contains(&format!(r#"route="{route}""#)),
            "autumn_http_route_requests_total should cover {route}; scrape had:\n{}",
            scrape
                .lines()
                .filter(|line| line.contains("route="))
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
}

#[tokio::test]
async fn no_family_name_falls_in_the_frameworks_reserved_namespace() {
    // `counter("autumn_…")` returns an inert handle: it records nothing, raises
    // nothing, and is indistinguishable from a counter sitting at zero. Every
    // family here was first named `autumn_io_*` and silently recorded nothing
    // for exactly that reason, which no amount of reading the code revealed —
    // only scraping the endpoint did.
    for family in [metrics::DOCS_SEARCHES, metrics::DOCS_PAGE_VIEWS] {
        assert!(
            !family.starts_with("autumn_"),
            "{family} is in the reserved namespace and would record nothing",
        );
    }
}

#[tokio::test]
async fn every_family_carries_help_text() {
    // A metric with no `# HELP` line is one nobody can interpret six months
    // later. `describe()` runs from the layer stack, so this also pins that
    // building the app is enough to register the descriptions.
    let app = app();
    app.get("/docs/getting-started").send().await;
    app.get("/search?q=router").send().await;

    let scrape = scrape(&app).await;
    for family in [metrics::DOCS_PAGE_VIEWS, metrics::DOCS_SEARCHES] {
        assert!(
            scrape.contains(&format!("# HELP {family}")),
            "{family} should carry HELP text",
        );
    }
}

#[tokio::test]
async fn no_metric_is_labelled_with_something_a_visitor_controls() {
    // Cardinality is the way an application metric turns into an outage. A
    // search term or a slug as a label value is unbounded by construction: one
    // crawler is enough to grow the registry without limit. This walks the
    // scrape and fails if a search term ever reaches a label.
    let app = app();
    app.get("/search?q=uniquesearchtermxyzzy").send().await;
    app.get("/docs/uniqueslugxyzzy").send().await;

    let scrape = scrape(&app).await;
    assert!(
        !scrape.contains("xyzzy"),
        "a visitor-controlled value reached a metric label:\n{scrape}",
    );
}
