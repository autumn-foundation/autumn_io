//! What an agent gets when it asks a page for Markdown instead of HTML.
//!
//! The unit tests in `src/negotiate.rs` cover the `Accept` grammar itself; this
//! file is about the site: that every page in the read path has a Markdown
//! representation, that the representation is *actually* Markdown rather than
//! stripped-looking HTML, that a browser is unaffected, and that the response
//! carries the headers a cache and an agent read.
//!
//! Every test goes through the app as `main.rs` builds it, for the reason
//! `cache_and_metrics.rs` gives: the cache policy and the `ETag` live in the
//! layer stack, not in `app_routes()`, and a Markdown response is now one of
//! the things that policy decides about.

use autumn_web::test::{TestApp, TestClient};

/// What Chrome sends. Nothing here names `text/markdown`, so nothing here may
/// receive it.
const BROWSER_ACCEPT: &str =
    "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8";

const MARKDOWN_CONTENT_TYPE: &str = "text/markdown; charset=utf-8";

fn app() -> TestClient {
    TestApp::new()
        .routes(autumn_io::app_routes())
        .layer(autumn_io::response_compression_layer())
        .mount_mcp(autumn_io::MCP_MOUNT_PATH)
        .build()
}

/// Every read-path page an agent would fetch.
const NEGOTIATED_PATHS: [&str; 4] = [
    "/",
    "/docs/getting-started",
    "/docs/metrics",
    "/search?q=jobs",
];

/// Whether a `Vary` header names `Accept` as its own field.
///
/// A substring check would pass on the `accept-encoding` the compression layer
/// adds to every response, which is a different question entirely.
fn varies_on_accept(vary: &str) -> bool {
    vary.split(',').any(|field| field.trim() == "accept")
}

#[tokio::test]
async fn every_read_path_page_answers_the_markdown_accept_header() {
    let app = app();

    for path in NEGOTIATED_PATHS {
        let response = app.get(path).header("accept", "text/markdown").send().await;

        response.assert_status(200);
        assert_eq!(
            response.header("content-type"),
            Some(MARKDOWN_CONTENT_TYPE),
            "{path} should answer the convention's Accept header with Markdown",
        );
        assert!(
            response.text().starts_with("# "),
            "{path} should open with a Markdown heading, not a document head",
        );
    }
}

#[tokio::test]
async fn a_markdown_response_is_markdown_and_not_stripped_html() {
    // The point of the feature: an agent that asks for Markdown must not have
    // to unpick a layout to find the words.
    let response = app()
        .get("/docs/getting-started")
        .header("accept", "text/markdown")
        .send()
        .await;

    let body = response.text();

    for markup in [
        "<!DOCTYPE",
        "<html",
        "<nav",
        "<aside",
        "class=\"docs-",
        "<span style=",
    ] {
        assert!(
            !body.contains(markup),
            "the Markdown representation should carry no {markup} from the HTML page",
        );
    }

    assert!(
        body.starts_with("# Getting Started with Autumn\n"),
        "the page's title should be its one top-level heading",
    );
    assert!(
        body.contains("```rust"),
        "code should arrive as fenced Markdown, not as highlighted markup",
    );
    assert!(
        body.contains("https://autumn-web.app/docs/getting-started"),
        "the body should say where it came from, in a link that survives being \
         copied out of the response",
    );
}

#[tokio::test]
async fn a_guide_serves_the_markdown_the_html_was_rendered_from() {
    // Not a conversion of the rendered page but the author's own source: the
    // registry already holds it, so the two representations cannot disagree.
    let registry = autumn_io::site_docs().expect("the bundled docs should load");
    let page = registry
        .page("metrics")
        .expect("the metrics guide is bundled");

    let body = app()
        .get("/docs/metrics")
        .header("accept", "text/markdown")
        .send()
        .await
        .text();

    assert!(
        body.contains(page.markdown.trim_end()),
        "the guide's Markdown source should be present verbatim",
    );
}

#[tokio::test]
async fn a_markdown_response_reports_its_token_cost() {
    let response = app()
        .get("/docs/getting-started")
        .header("accept", "text/markdown")
        .send()
        .await;

    let tokens: usize = response
        .header("x-markdown-tokens")
        .expect("a Markdown response should estimate what it costs to read")
        .parse()
        .expect("the estimate should be a bare number a caller can compare");

    let body_length = response.text().len();
    assert!(
        tokens > 0 && tokens <= body_length,
        "an estimate of {tokens} tokens is not plausible for a {body_length}-byte body",
    );
}

#[tokio::test]
async fn an_html_response_carries_no_token_header() {
    // The header describes a Markdown body; on an HTML page it would be a
    // measurement of something the caller did not receive.
    let response = app().get("/docs/getting-started").send().await;

    assert_eq!(response.header("x-markdown-tokens"), None);
}

#[tokio::test]
async fn browsers_and_bare_clients_still_get_html() {
    let app = app();

    for (label, accept) in [
        ("a browser", Some(BROWSER_ACCEPT)),
        ("a bare curl", Some("*/*")),
        (
            "the text wildcard, which names both representations",
            Some("text/*"),
        ),
        ("a client sending no preference at all", None),
    ] {
        for path in ["/", "/docs/getting-started"] {
            let mut request = app.get(path);
            if let Some(accept) = accept {
                request = request.header("accept", accept);
            }

            let response = request.send().await;
            response.assert_status(200);
            assert_eq!(
                response.header("content-type"),
                Some("text/html; charset=utf-8"),
                "{label} should still get the page at {path}",
            );
        }
    }
}

#[tokio::test]
async fn the_home_page_hands_an_agent_the_whole_guide_index() {
    // An agent has no sidebar to browse: without the index it would have to
    // guess slugs or crawl.
    let registry = autumn_io::site_docs().expect("the bundled docs should load");

    let body = app()
        .get("/")
        .header("accept", "text/markdown")
        .send()
        .await
        .text();

    assert!(body.starts_with("# Autumn 0.7.0\n"));
    assert!(
        body.contains("## All guides"),
        "the home page should carry the site's navigation as a list",
    );
    for page in registry.pages() {
        let url = format!("https://autumn-web.app/docs/{}", page.slug);
        assert!(
            body.contains(&url),
            "every guide should be linked from the index; {url} is missing",
        );
    }
    assert!(
        body.contains("Accept: text/markdown"),
        "the page an agent lands on should say the rest of the site works this way too",
    );
    assert!(
        body.contains("https://autumn-web.app/mcp"),
        "and should still point at the MCP endpoint",
    );
}

#[tokio::test]
async fn a_missing_guide_is_a_markdown_404_that_lists_the_real_ones() {
    let response = app()
        .get("/docs/not-a-guide")
        .header("accept", "text/markdown")
        .send()
        .await;

    response.assert_status(404);
    assert_eq!(response.header("content-type"), Some(MARKDOWN_CONTENT_TYPE));

    let body = response.text();
    assert!(
        body.contains("`not-a-guide`"),
        "the wrong slug should be quoted back"
    );
    assert!(
        body.contains("https://autumn-web.app/docs/getting-started"),
        "a 404 should hand back the list of pages that do exist",
    );
}

#[tokio::test]
async fn search_answers_in_markdown() {
    let response = app()
        .get("/search?q=durable+workflows")
        .header("accept", "text/markdown")
        .send()
        .await;

    response.assert_status(200);
    assert_eq!(response.header("content-type"), Some(MARKDOWN_CONTENT_TYPE));

    let body = response.text();
    assert!(body.starts_with("# Search the guides\n"));
    assert!(
        body.contains("result") && body.contains("](https://autumn-web.app/docs/"),
        "results should be a linked list of guides: {body}",
    );

    // The empty-query and no-match arms are answers too, not errors.
    let empty = app()
        .get("/search?q=")
        .header("accept", "text/markdown")
        .send()
        .await;
    empty.assert_status(200);
    assert!(empty.text().contains("Enter a search term"));

    let no_hits = app()
        .get("/search?q=zzzzzzzznotaword")
        .header("accept", "text/markdown")
        .send()
        .await;
    no_hits.assert_status(200);
    assert!(no_hits.text().contains("No guides match"));
}

#[tokio::test]
async fn negotiated_pages_declare_that_they_vary_on_accept() {
    let app = app();

    for path in NEGOTIATED_PATHS {
        for accept in ["text/markdown", BROWSER_ACCEPT] {
            let response = app.get(path).header("accept", accept).send().await;
            let vary = response
                .header("vary")
                .expect("a negotiated response should declare its variance")
                .to_ascii_lowercase();

            assert!(
                varies_on_accept(&vary),
                "{path} should vary on Accept itself, not just on accept-encoding, \
                 for {accept}; got {vary}",
            );
        }
    }
}

#[tokio::test]
async fn markdown_is_never_stored_by_a_shared_cache() {
    // Cloudflare keys HTML by URL and ignores a custom `Vary`, so a stored
    // Markdown entry would be handed to browsers. The HTML representation of
    // the same URL stays cacheable — that is the whole point of the edge cache.
    let app = app();

    for path in ["/", "/docs/getting-started"] {
        let markdown = app.get(path).header("accept", "text/markdown").send().await;
        assert_eq!(
            markdown.header("cache-control"),
            Some("no-store"),
            "the Markdown representation of {path} must not enter a shared cache",
        );

        let html = app.get(path).header("accept", BROWSER_ACCEPT).send().await;
        assert_eq!(
            html.header("cache-control"),
            Some("public, max-age=0, s-maxage=3600, must-revalidate"),
            "the HTML representation of {path} should still be cacheable",
        );
    }
}

#[tokio::test]
async fn only_a_named_markdown_media_type_produces_markdown() {
    // The condition the origin serves Markdown on has to be the condition
    // Cloudflare's cache rule can match as a string, or the shapes it cannot
    // match get answered from an HTML cache entry that ignores `Accept`. A
    // wildcard covering Markdown is therefore not an election, and refusing
    // HTML without naming Markdown is a 406 rather than Markdown by
    // elimination.
    let app = app();

    for accept in ["text/html;q=0.1, */*;q=1", "text/*;q=1"] {
        let response = app
            .get("/docs/getting-started")
            .header("accept", accept)
            .send()
            .await;

        response.assert_status(200);
        assert_eq!(
            response.header("content-type"),
            Some("text/html; charset=utf-8"),
            "{accept} does not name text/markdown, so it is not a request for it",
        );
    }

    app.get("/docs/getting-started")
        .header("accept", "text/html;q=0")
        .send()
        .await
        .assert_status(406);

    let named = app
        .get("/docs/getting-started")
        .header("accept", "text/html;q=0, text/markdown")
        .send()
        .await;
    named.assert_status(200);
    assert_eq!(named.header("content-type"), Some(MARKDOWN_CONTENT_TYPE));
}

#[tokio::test]
async fn a_revalidated_markdown_response_is_still_uncacheable() {
    // `EtagLayer` answers a repeat visit with a `304` built from scratch,
    // carrying `ETag` and nothing else — no `Content-Type` to recognise Markdown
    // by — and a `304` is a `3xx`, which the page policy would otherwise treat
    // as a cacheable redirect. A shared cache would then hold `s-maxage=3600`
    // against a Markdown revalidation of a URL whose stored entry is HTML.
    let app = app();

    let first = app
        .get("/docs/getting-started")
        .header("accept", "text/markdown")
        .send()
        .await;
    first.assert_status(200);
    let etag = first
        .header("etag")
        .expect("the layer stack derives a weak ETag from the body")
        .to_owned();

    let revalidated = app
        .get("/docs/getting-started")
        .header("accept", "text/markdown")
        .header("if-none-match", &etag)
        .send()
        .await;

    revalidated.assert_status(304);
    assert_eq!(
        revalidated.header("cache-control"),
        Some("no-store"),
        "a revalidated Markdown response must not become cacheable by losing \
         its Content-Type",
    );

    // The HTML representation of the same URL revalidates too, and keeps the
    // policy that makes the edge cache worth having.
    let html = app.get("/docs/getting-started").send().await;
    let html_etag = html
        .header("etag")
        .expect("HTML carries an ETag too")
        .to_owned();
    assert_ne!(
        html_etag, etag,
        "two representations of one URL must not share a validator",
    );

    let html_revalidated = app
        .get("/docs/getting-started")
        .header("if-none-match", &html_etag)
        .send()
        .await;
    html_revalidated.assert_status(304);
    assert_eq!(
        html_revalidated.header("cache-control"),
        Some("public, max-age=0, s-maxage=3600, must-revalidate"),
    );
}

#[tokio::test]
async fn forbidding_both_representations_is_a_406() {
    let response = app()
        .get("/docs/getting-started")
        .header("accept", "text/html;q=0, text/markdown;q=0")
        .send()
        .await;

    response.assert_status(406);
    assert!(
        response
            .header("vary")
            .is_some_and(|vary| varies_on_accept(&vary.to_ascii_lowercase())),
        "even a 406 varies on Accept — it is the header that produced it",
    );
}

#[tokio::test]
async fn the_json_api_is_untouched_by_page_negotiation() {
    // `/api/*` and `/mcp` have their own contract; negotiation applies to the
    // pages a browser and an agent share, not to them.
    let response = app()
        .get("/api/docs")
        .header("accept", "text/markdown")
        .send()
        .await;

    response.assert_status(200);
    assert!(
        response
            .header("content-type")
            .is_some_and(|value| value.starts_with("application/json")),
        "the JSON API should keep answering JSON",
    );
}
