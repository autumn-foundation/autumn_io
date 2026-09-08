use std::sync::LazyLock;

use autumn_web::prelude::*;
use autumn_web::reexports::axum::extract::Request;
use autumn_web::reexports::axum::middleware::{self, Next};
use autumn_web::reexports::axum::response::{IntoResponse, Redirect, Response};
use autumn_web::reexports::http::{HeaderValue, StatusCode, header};

pub mod api;
pub mod docs;
pub mod export;
pub mod metrics;
pub mod negotiate;
pub mod seo;
pub mod site;

use serde::Deserialize;

use docs::{DocRegistry, DocSource, DocsError, SearchHit, SearchIndex};
use negotiate::{MarkdownNegotiate, MarkdownPage};

pub const DOCS_START_SLUG: &str = "getting-started";
pub const DOCS_START_PATH: &str = "/docs/getting-started";

/// Path of the docs-search UI.
///
/// Deliberately outside the `/docs/{slug}` namespace: an exact route there
/// silently shadows the guide of the same slug, and guide slugs come from
/// upstream file names we do not control — upstream 0.7.0 added `search.md`,
/// which would have been unreachable behind a `/docs/search` endpoint.
pub const DOCS_SEARCH_PATH: &str = "/search";

/// Where the MCP server is mounted.
///
/// `/mcp` is the convention every MCP client defaults to, and Autumn panics at
/// startup if the mount path collides with a real route — no site route uses it.
pub const MCP_MOUNT_PATH: &str = "/mcp";

/// Maximum number of guide results returned by the docs search handler.
const DOCS_SEARCH_RESULT_LIMIT: usize = 20;

macro_rules! guide_doc {
    ($slug:literal) => {
        DocSource::new(
            $slug,
            include_str!(concat!("../content/guide/", $slug, ".md")),
        )
    };
}

static SITE_DOCS: LazyLock<Result<DocRegistry, DocsError>> = LazyLock::new(|| {
    DocRegistry::from_sources([
        guide_doc!("getting-started"),
        guide_doc!("what-happens-when"),
        guide_doc!("autumn-harvest"),
        guide_doc!("coming-from-other-frameworks"),
        guide_doc!("generators"),
        guide_doc!("accessibility"),
        guide_doc!("middleware"),
        guide_doc!("path-helpers"),
        guide_doc!("routes-cli"),
        guide_doc!("macro-transparency"),
        guide_doc!("testing"),
        guide_doc!("transactions"),
        guide_doc!("seeding"),
        guide_doc!("storage"),
        guide_doc!("mail"),
        guide_doc!("authorization"),
        guide_doc!("signed-webhooks"),
        guide_doc!("signing-secrets"),
        guide_doc!("realtime"),
        guide_doc!("websockets"),
        guide_doc!("jobs"),
        guide_doc!("tasks"),
        guide_doc!("operating-background-jobs"),
        guide_doc!("scheduled-multi-replica"),
        guide_doc!("admin"),
        guide_doc!("custom-subsystems"),
        guide_doc!("extensibility"),
        guide_doc!("cloud-native"),
        guide_doc!("i18n"),
        guide_doc!("deployment"),
        // New in Autumn 0.5.0.
        guide_doc!("compression"),
        guide_doc!("conditional-get"),
        guide_doc!("pagination"),
        guide_doc!("active-search-and-autocomplete"),
        guide_doc!("wizards"),
        guide_doc!("hooks-and-transactions"),
        guide_doc!("repositories"),
        guide_doc!("migrations"),
        guide_doc!("soft-delete"),
        guide_doc!("state-machines"),
        guide_doc!("version-history"),
        guide_doc!("full-text-search"),
        guide_doc!("storage-variants"),
        guide_doc!("attribute-encryption"),
        guide_doc!("oauth"),
        guide_doc!("step-up-authentication"),
        guide_doc!("credentials"),
        guide_doc!("bot-protection"),
        guide_doc!("idempotency"),
        guide_doc!("logging-pii"),
        guide_doc!("presence"),
        guide_doc!("api-versioning"),
        guide_doc!("outbound-http"),
        guide_doc!("outbound-webhooks"),
        guide_doc!("mcp"),
        guide_doc!("feature-flags"),
        guide_doc!("experiments"),
        guide_doc!("runtime-config"),
        guide_doc!("resilience"),
        guide_doc!("health-indicators"),
        guide_doc!("metrics-sources"),
        guide_doc!("error-reporting"),
        guide_doc!("maintenance-mode"),
        guide_doc!("staged-deploys"),
        guide_doc!("dev-error-overlay"),
        guide_doc!("dev-inspector"),
        guide_doc!("dev-loop-latency"),
        guide_doc!("system-tests"),
        // New in Autumn 0.6.0.
        guide_doc!("flash"),
        guide_doc!("tabs"),
        guide_doc!("declarative-schema"),
        guide_doc!("events"),
        guide_doc!("lifecycle"),
        guide_doc!("mail-compliance"),
        guide_doc!("cache-stampede"),
        guide_doc!("daemon"),
        guide_doc!("distributed-locks"),
        guide_doc!("fragment-caching"),
        guide_doc!("operator-alerts"),
        guide_doc!("rate-limiting"),
        guide_doc!("security-posture-manifest"),
        guide_doc!("tls"),
        guide_doc!("format-helpers"),
        guide_doc!("stories"),
        guide_doc!("time-zones"),
        guide_doc!("transition-effects"),
        guide_doc!("wasm-islands"),
        guide_doc!("widget-styling"),
        guide_doc!("sharding"),
        guide_doc!("sqlite-in-production"),
        guide_doc!("tenant-cells"),
        guide_doc!("tauri"),
        guide_doc!("tauri-mobile-in-process"),
        guide_doc!("tauri-mobile-offline-sync"),
        guide_doc!("tauri-mobile-thin-client"),
        guide_doc!("starters"),
        // New guides folded in after the 0.6.0 sync.
        guide_doc!("submit-tokens"),
        guide_doc!("downloads"),
        guide_doc!("media"),
        // Two newer upstream framework guides.
        guide_doc!("content-negotiation"),
        guide_doc!("nested-forms"),
        // Autumn Harvest 0.5 guide — the upstream getting-started chapter
        // sequence, vendored under `harvest-*` slugs and grouped as "Harvest"
        // in the sidebar, anchored by the `autumn-harvest` intro above.
        guide_doc!("harvest-project-skeleton"),
        guide_doc!("harvest-first-workflow"),
        guide_doc!("harvest-durable-timers"),
        guide_doc!("harvest-signals"),
        guide_doc!("harvest-child-workflows"),
        guide_doc!("harvest-idempotency"),
        guide_doc!("harvest-reliability-knobs"),
        guide_doc!("harvest-dags-and-schedules"),
        guide_doc!("harvest-worker-routing"),
        guide_doc!("harvest-operations"),
        guide_doc!("harvest-testing"),
        guide_doc!("harvest-webhooks"),
        // New in Harvest 0.6.0.
        guide_doc!("harvest-broker-connectors"),
        // New in Autumn 0.7.0.
        guide_doc!("seo"),
        guide_doc!("pdf-downloads"),
        guide_doc!("rich-text"),
        guide_doc!("commentable"),
        guide_doc!("votable"),
        guide_doc!("feeds"),
        guide_doc!("notifications"),
        guide_doc!("search"),
        guide_doc!("openapi"),
        guide_doc!("authentication"),
        guide_doc!("route-auth-coverage"),
        guide_doc!("aggregates"),
        guide_doc!("counter-cache"),
        guide_doc!("ledgered-entities"),
        guide_doc!("audit-logging"),
        guide_doc!("retention-sweeps"),
        guide_doc!("query-budgets"),
        guide_doc!("metrics"),
        guide_doc!("server-timing"),
        guide_doc!("failure-capsules"),
        guide_doc!("console"),
        guide_doc!("simulation-testing"),
        guide_doc!("clustering"),
        guide_doc!("upgrading"),
        guide_doc!("edge"),
        guide_doc!("fleet-deploys"),
    ])
});

pub fn site_docs() -> Result<&'static DocRegistry, &'static DocsError> {
    match &*SITE_DOCS {
        Ok(registry) => Ok(registry),
        Err(error) => Err(error),
    }
}

/// In-memory search index over the embedded guides, built once from
/// [`site_docs`]. `None` when the docs failed to load.
static SITE_SEARCH_INDEX: LazyLock<Option<SearchIndex>> =
    LazyLock::new(|| site_docs().ok().map(SearchIndex::from_registry));

#[must_use]
pub fn site_search_index() -> Option<&'static SearchIndex> {
    SITE_SEARCH_INDEX.as_ref()
}

/// Optimize HTTP responses for repeat visitors.
///
/// Autumn 0.6.0 (issue #752) now applies user layers to static-first responses,
/// so wiring the framework's `dist/` static HTML serving is possible. We
/// deliberately do not: the docs are served dynamically from the in-memory
/// registry (content is already resident, so a request costs a HashMap lookup +
/// template wrap + on-the-fly compression). Serving `dist/` would duplicate the
/// embedded content on disk for no latency or bandwidth win on the 256 MB
/// scale-to-zero VM. The `build_site`/`dist` exporter is kept only as a
/// CDN/static-hosting bundle generator.
///
/// The stack also provides weak-ETag conditional-GET: [`EtagLayer`] is the
/// innermost layer, so on the response path it runs first and derives a weak
/// `ETag` from the raw uncompressed handler body, returning `304 Not Modified`
/// when a repeat visit's `If-None-Match` matches. `CompressionLayer` then
/// encodes the body and adds `Vary: Accept-Encoding`, keeping the ETag computed
/// over the unencoded bytes (framework-blessed ordering — see `router.rs`).
///
/// [`EtagLayer`]: autumn_web::etag::EtagLayer
pub fn response_compression_layer() -> impl autumn_web::app::IntoAppLayer {
    metrics::describe();
    tower::ServiceBuilder::new()
        .layer(middleware::from_fn(apply_cache_control))
        .layer(tower_http::map_response_body::MapResponseBodyLayer::new(
            autumn_web::reexports::axum::body::Body::new,
        ))
        .layer(tower_http::compression::CompressionLayer::new())
        .layer(autumn_web::etag::EtagLayer::new())
}

/// `Cache-Control` for a static asset whose URL carries the build's asset
/// version: the URL changes whenever the bytes do, so the response can be
/// cached permanently and never revalidated.
const IMMUTABLE_CACHE_CONTROL: &str = "public, max-age=31536000, immutable";

/// `Cache-Control` for a static asset served at a stable, unversioned URL.
///
/// These cannot be cached immutably: the URL stays the same across deploys, so
/// a year-long `immutable` entry would pin a visitor to a stale copy with no
/// way to bust it. A short freshness window plus revalidation keeps them cheap
/// — [`EtagLayer`] answers the revalidation with a `304`.
///
/// [`EtagLayer`]: autumn_web::etag::EtagLayer
const REVALIDATED_CACHE_CONTROL: &str = "public, max-age=3600, must-revalidate";

/// `Cache-Control` for a rendered page.
///
/// The origin is one scale-to-zero machine in `ord`; a reader far from it pays
/// a trans-Pacific round trip, and a cold start on top if the machine is
/// asleep. These pages are the same bytes for every visitor asking for HTML —
/// no `Set-Cookie`, and no `Vary` a CDN ignores — so a shared cache in front of
/// the origin can serve them, and that is what this header is for.
///
/// The Markdown representation of the same URL (see [`negotiate`]) is
/// deliberately *not* covered: it carries [`UNCACHEABLE`] instead, because
/// Cloudflare would store it under a cache key that ignores `Accept` and then
/// hand Markdown to a browser.
///
/// `s-maxage` addresses the shared cache only; `max-age=0, must-revalidate`
/// keeps browsers asking, which is cheap because [`EtagLayer`] answers with a
/// `304` and never re-renders. An hour bounds how long a missed purge can
/// serve stale docs — a deploy should purge, but this self-heals if it does
/// not.
///
/// [`EtagLayer`]: autumn_web::etag::EtagLayer
const PAGE_CACHE_CONTROL: &str = "public, max-age=0, s-maxage=3600, must-revalidate";

/// `Cache-Control` for a page that does not exist.
///
/// Worth caching — a bad slug should not wake the origin repeatedly — but for
/// far less time than a real page: a cached `404` outlives the deploy that
/// adds the guide it denies, and the shorter window bounds how long a newly
/// published guide can appear missing.
const MISSING_PAGE_CACHE_CONTROL: &str = "public, max-age=0, s-maxage=60, must-revalidate";

/// `Cache-Control` for a response that must never be held by a shared cache.
///
/// `/search` renders two different bodies at one URL — an htmx fragment for
/// `HX-Request`, a full page otherwise — and the request header that picks
/// between them is not part of any CDN cache key. A shared cache holding one
/// variant would serve a bare fragment to a normal navigation, or a whole page
/// into a `<div>`. Cloudflare does not honour a custom `Vary` for HTML, so
/// declaring the variance is not a fix; not storing it is.
///
/// A Markdown response is the same hazard with a worse failure: every page in
/// the read path now answers one URL with either HTML or Markdown depending on
/// `Accept`, and a stored Markdown entry would be served to browsers until it
/// expired. The responses carry `Vary: Accept` for caches that honour it, and
/// this for the one in front of us that does not. See
/// `docs/adr/0002-markdown-for-agents.md` for the Cloudflare rule that keeps
/// the *HTML* entry from being served to agents in turn.
const UNCACHEABLE: &str = "no-store";

/// Whether a static-asset request carries this build's asset-version query
/// (`?v=…`), which is what makes a URL safe to cache immutably.
///
/// Only `site::versioned_asset_path` adds it, and it covers just the assets
/// this site authors. The framework serves its own assets under `/static/`
/// too — `autumn-widgets.css`, `autumn-widgets.js`, `htmx.min.js` — and the
/// pages that link them (the `/_stories` gallery is rendered by the framework,
/// not by us) reference them at bare, unversioned URLs. Marking those
/// `immutable` pinned every returning visitor to the previous release's copy
/// for a year across an `autumn-web` upgrade.
fn has_asset_version_query(query: Option<&str>) -> bool {
    query.is_some_and(|query| {
        query
            .split('&')
            .any(|pair| pair.split_once('=').is_some_and(|(key, _)| key == "v"))
    })
}

/// Whether a path renders a page whose bytes are identical for every visitor.
///
/// The read path only. `/search` is excluded deliberately (see [`UNCACHEABLE`]),
/// and so is everything not listed: `/api/*`, `/mcp`, `/health` and the
/// framework's own `/actuator/*` and `/_stories` are either request-specific or
/// nobody's business to cache.
fn is_cacheable_page(path: &str) -> bool {
    path == "/"
        || path == "/robots.txt"
        || path == "/sitemap.xml"
        || (path.starts_with("/docs") && path != DOCS_SEARCH_PATH)
}

/// Applies the site's `Cache-Control` policy.
///
/// Everything the origin serves falls into one of four buckets — versioned
/// asset, unversioned asset, cacheable page, or must-not-be-cached — and this
/// is the single place that decides which. Resolved from the request path
/// before the response exists, then stamped afterwards once the status *and the
/// negotiated representation* are known: a `404` is cached differently from a
/// `200`, and a Markdown body must not be cached at all.
async fn apply_cache_control(request: Request, next: Next) -> Response {
    let path = request.uri().path();
    let is_static = path.starts_with("/static/");
    let is_page = is_cacheable_page(path);
    let is_search = path == DOCS_SEARCH_PATH;
    let versioned = has_asset_version_query(request.uri().query());

    // Which representation this is has to be read from the *request*. A repeat
    // visit that revalidates is answered by `EtagLayer` with a `304` built from
    // scratch — `ETag` and nothing else — so the response no longer says
    // `text/markdown` anywhere, and a `304` is a `3xx`, so the page policy below
    // would claim it and mark a revalidated Markdown response cacheable for an
    // hour. The `Accept` header still says what was asked for.
    let wants_markdown = negotiate::prefers_markdown(request.headers());

    let mut response = next.run(request).await;
    let status = response.status();

    // The response's own type is still consulted, as a backstop for any
    // Markdown this site might serve outside the negotiated read path.
    let is_markdown = wants_markdown
        || response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with(negotiate::MARKDOWN_MEDIA_TYPE));

    let cache_control = if is_markdown {
        Some(UNCACHEABLE)
    } else if is_static && status.is_success() {
        Some(if versioned {
            IMMUTABLE_CACHE_CONTROL
        } else {
            REVALIDATED_CACHE_CONTROL
        })
    } else if is_search {
        Some(UNCACHEABLE)
    } else if is_page && (status.is_success() || status.is_redirection()) {
        // Redirects included, for `/docs` — the one read-path URL that answers
        // with a 307 rather than a body. A 307 is not cacheable by default, so
        // without an explicit policy the entry point to the guides would be the
        // single page in the read path that still woke the origin every time.
        // Its target is a compile-time constant, so it only changes on a deploy,
        // which is exactly what the purge covers.
        Some(PAGE_CACHE_CONTROL)
    } else if is_page && status == StatusCode::NOT_FOUND {
        Some(MISSING_PAGE_CACHE_CONTROL)
    } else {
        // A 5xx must never be cached: the next reader would inherit an outage
        // that has already been fixed.
        None
    };

    if let Some(cache_control) = cache_control {
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static(cache_control),
        );
    }

    response
}

#[get("/")]
pub async fn index(negotiate: MarkdownNegotiate) -> Response {
    let registry = match site_docs() {
        Ok(registry) => registry,
        Err(error) => return docs_load_error_response(negotiate, error),
    };

    negotiate.respond(
        || site::render_home_page(registry).into_response(),
        || MarkdownPage::new(site::markdown::render_home_page(registry)),
    )
}

#[get("/docs")]
pub async fn docs_index() -> Redirect {
    Redirect::temporary(DOCS_START_PATH)
}

#[get("/docs/{slug}")]
pub async fn docs_page(negotiate: MarkdownNegotiate, Path(slug): Path<String>) -> Response {
    let registry = match site_docs() {
        Ok(registry) => registry,
        Err(error) => return docs_load_error_response(negotiate, error),
    };

    // Counted inside the arm that renders, not before it: the two
    // representations are different populations. An HTML render is a cache miss
    // or a revalidation, while a Markdown one deliberately bypasses the cache
    // and always reaches the origin — summed together, an agent crawling the
    // guides would read as the edge cache degrading. A `406` renders neither and
    // counts as neither.
    match registry.page(&slug) {
        Some(page) => negotiate.respond(
            || {
                metrics::record_page_render(metrics::outcome::FOUND, metrics::representation::HTML);
                site::render_docs_page(registry, page).into_response()
            },
            || {
                metrics::record_page_render(
                    metrics::outcome::FOUND,
                    metrics::representation::MARKDOWN,
                );
                MarkdownPage::new(site::markdown::render_docs_page(registry, page))
            },
        ),
        None => negotiate.respond(
            || {
                metrics::record_page_render(
                    metrics::outcome::MISSING,
                    metrics::representation::HTML,
                );
                (
                    StatusCode::NOT_FOUND,
                    site::render_missing_docs_page(registry, &slug),
                )
                    .into_response()
            },
            || {
                metrics::record_page_render(
                    metrics::outcome::MISSING,
                    metrics::representation::MARKDOWN,
                );
                MarkdownPage::with_status(
                    StatusCode::NOT_FOUND,
                    site::markdown::render_missing_docs_page(registry, &slug),
                )
            },
        ),
    }
}

#[derive(Debug, Deserialize)]
pub struct DocsSearchQuery {
    #[serde(default)]
    q: String,
}

/// Search the embedded guides.
///
/// Answers an htmx results partial, a full docs page when reached directly
/// (e.g. the widget's `<noscript>` GET form), or the hits as Markdown for a
/// client that asked for `text/markdown` — one search either way.
///
/// Served at [`DOCS_SEARCH_PATH`], outside the `/docs/{slug}` namespace.
#[get("/search")]
pub async fn docs_search(
    negotiate: MarkdownNegotiate,
    hx: HxRequest,
    Query(query): Query<DocsSearchQuery>,
) -> Response {
    let term = query.q.trim();

    // The search runs once and both representations are built from its hits:
    // `None` means the index itself is unavailable, which is a different answer
    // from "no guide matched". Recording the outcome here, before the
    // representations diverge, keeps one request counted once however it is
    // served.
    let hits = match site_search_index() {
        Some(index) if !term.is_empty() => {
            let hits = index.search(term, DOCS_SEARCH_RESULT_LIMIT);
            metrics::record_search(if hits.is_empty() {
                metrics::outcome::EMPTY
            } else {
                metrics::outcome::HIT
            });
            Some(hits)
        }
        // An empty box is not a search; counting it would drown the signal the
        // `empty` series exists to carry.
        Some(_) => Some(Vec::new()),
        None => {
            metrics::record_search(metrics::outcome::UNAVAILABLE);
            None
        }
    };

    negotiate.respond(
        || search_html_response(hx.is_htmx, term, hits.as_deref()),
        || {
            // `hits: None` has one cause — `site_search_index()` is built from
            // `site_docs()`, so an absent index means the bundled content failed
            // to parse. That is the same `500` the other pages answer with, and
            // the full-page HTML arm below reaches it through `site_docs()` for
            // the same reason. A `200` whose body says "unavailable" would tell
            // an agent the server is fine and the corpus is empty.
            let status = if hits.is_some() {
                StatusCode::OK
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };

            MarkdownPage::with_status(
                status,
                site::markdown::render_docs_search_page(term, hits.as_deref()),
            )
        },
    )
}

/// The HTML arm of [`docs_search`]: an htmx results partial, or a full docs
/// page when reached directly (e.g. the widget's `<noscript>` GET form).
fn search_html_response(is_htmx: bool, term: &str, hits: Option<&[SearchHit]>) -> Response {
    let results = match hits {
        Some(_) if term.is_empty() => active_search_empty_state("Type to search the guides."),
        Some(hits) => site::render_docs_search_results(term, hits),
        None => active_search_empty_state("Search is unavailable right now."),
    };

    if is_htmx {
        return results.into_response();
    }

    match site_docs() {
        Ok(registry) => site::render_docs_search_page(registry, term, results).into_response(),
        // Only the docs *layout* is unavailable here, and only this arm needs
        // it — the same 500 the other pages answer with, without re-entering
        // negotiation for a representation already chosen.
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            site::render_docs_load_error(error),
        )
            .into_response(),
    }
}

#[get("/robots.txt")]
pub async fn robots_txt() -> Response {
    (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        seo::robots_txt(),
    )
        .into_response()
}

#[get("/sitemap.xml")]
pub async fn sitemap_xml() -> Response {
    let registry = match site_docs() {
        Ok(registry) => registry,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                error.to_string(),
            )
                .into_response();
        }
    };

    (
        [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
        seo::sitemap_xml(registry),
    )
        .into_response()
}

#[must_use]
pub fn app_routes() -> Vec<autumn_web::Route> {
    let mut routes = routes![
        index,
        docs_index,
        docs_search,
        docs_page,
        robots_txt,
        sitemap_xml
    ];
    // The JSON docs API, which `main` projects into the `/mcp` MCP server.
    // Registered here rather than only in `main` so the test harness exercises
    // the same route set the deployed app serves.
    routes.extend(api::api_routes());
    routes
}

fn docs_load_error_response(negotiate: MarkdownNegotiate, error: &DocsError) -> Response {
    negotiate.respond(
        || {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                site::render_docs_load_error(error),
            )
                .into_response()
        },
        || {
            MarkdownPage::with_status(
                StatusCode::INTERNAL_SERVER_ERROR,
                site::markdown::render_docs_load_error(error),
            )
        },
    )
}
