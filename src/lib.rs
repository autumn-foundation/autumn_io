//! The Autumn documentation site.
//!
//! # Two lanes, one source
//!
//! Everything on the public read path lives in [`edge`], which compiles for
//! `wasm32-wasip1` as well as for the host. The origin binary mounts those
//! handlers like any other route; `autumn build` also compiles them into an
//! edge capsule a CDN can run in front of the origin. See
//! `content/guide/edge.md` and `docs/adr/0001-cloudflare-edge-capsule.md`.
//!
//! That split is why this module is careful about what it names. `lib.rs`,
//! [`docs`], [`site`], [`seo`], [`frontmatter`] and [`widgets`] are all
//! **edge-safe**: they compile with `autumn-web` absent from the dependency
//! graph entirely. Anything that needs the framework — the htmx search route,
//! the JSON docs API, the response layer stack, the static-site exporter — is
//! behind a `cfg(not(target_arch = "wasm32"))` gate below.

use std::sync::LazyLock;

pub mod docs;
pub mod edge;
pub mod frontmatter;
pub mod seo;
pub mod site;
pub mod widgets;

// ── origin-only (these name `autumn-web`) ──
#[cfg(not(target_arch = "wasm32"))]
pub mod api;
#[cfg(not(target_arch = "wasm32"))]
pub mod export;
#[cfg(not(target_arch = "wasm32"))]
pub mod origin;

#[cfg(not(target_arch = "wasm32"))]
pub use origin::{app_routes, response_compression_layer, site_search_index};

use docs::{DocRegistry, DocSource, DocsError};

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
///
/// Origin-only, like the route that reads it: `/search` needs the framework's
/// `HxRequest` extractor, so it is the one read-path route the capsule does not
/// carry.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const DOCS_SEARCH_RESULT_LIMIT: usize = 20;

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
