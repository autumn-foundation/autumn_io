use serde_json::json;

use crate::docs::{DocPage, DocRegistry};

pub const SITE_BASE_URL: &str = "https://autumn-web.app";
pub const SITE_NAME: &str = "Autumn";
pub const SITE_DESCRIPTION: &str = "Autumn is a Rust web framework for fast server-rendered apps, typed routes, Maud templates, static assets, and production defaults.";
pub const SITE_IMAGE_PATH: &str = "/static/img/autumn-social.png";
pub const GITHUB_REPOSITORY_URL: &str = "https://github.com/autumn-foundation/autumn";
pub const WEBSITE_REPOSITORY_URL: &str = "https://github.com/autumn-foundation/autumn_io";
pub const CRATES_IO_URL: &str = "https://crates.io/crates/autumn-web";
pub const RUSTDOC_URL: &str = "https://docs.rs/autumn-web";
pub const AUTUMN_VERSION: &str = "0.8.0";
pub const HARVEST_REPOSITORY_URL: &str = "https://github.com/autumn-foundation/autumn-harvest";
pub const HARVEST_CRATES_IO_URL: &str = "https://crates.io/crates/autumn-harvest";
pub const HARVEST_RUSTDOC_URL: &str = "https://docs.rs/autumn-harvest";
pub const HARVEST_VERSION: &str = "0.7.0";

#[must_use]
pub fn absolute_url(path: &str) -> String {
    if path == "/" {
        format!("{SITE_BASE_URL}/")
    } else if path.starts_with('/') {
        format!("{SITE_BASE_URL}{path}")
    } else {
        format!("{SITE_BASE_URL}/{path}")
    }
}

/// Path prefix for a guide page, shared with [`docs_path`] so the two never
/// drift: [`crate::site::docs_nav_link`] interpolates this directly into a
/// `format_args!` rather than calling `docs_path`, since `format_args!`
/// can't be returned from a function (its argument array is a temporary
/// tied to the creating expression).
pub const DOCS_PATH_PREFIX: &str = "/docs/";

#[must_use]
pub fn docs_path(slug: &str) -> String {
    format!("{DOCS_PATH_PREFIX}{slug}")
}

#[must_use]
pub fn site_image_url(asset_version: &str) -> String {
    absolute_url(&format!("{SITE_IMAGE_PATH}?v={asset_version}"))
}

#[must_use]
pub fn home_structured_data() -> String {
    json!({
        "@context": "https://schema.org",
        "@graph": [
            {
                "@type": "WebSite",
                "@id": format!("{SITE_BASE_URL}/#website"),
                "url": absolute_url("/"),
                "name": SITE_NAME,
                "alternateName": "Autumn Web",
                "description": SITE_DESCRIPTION,
                "inLanguage": "en-US",
                "publisher": {
                    "@type": "Organization",
                    "name": SITE_NAME,
                    "url": absolute_url("/")
                }
            },
            {
                "@type": "SoftwareSourceCode",
                "@id": format!("{SITE_BASE_URL}/#source"),
                "name": SITE_NAME,
                "description": SITE_DESCRIPTION,
                "codeRepository": GITHUB_REPOSITORY_URL,
                "programmingLanguage": "Rust",
                "runtimePlatform": "Rust",
                "softwareVersion": AUTUMN_VERSION,
                "url": absolute_url("/"),
                "sameAs": [
                    GITHUB_REPOSITORY_URL,
                    CRATES_IO_URL,
                    RUSTDOC_URL,
                    HARVEST_REPOSITORY_URL,
                    HARVEST_CRATES_IO_URL,
                    HARVEST_RUSTDOC_URL
                ]
            }
        ]
    })
    .to_string()
}

#[must_use]
pub fn docs_structured_data(page: &DocPage) -> String {
    let page_path = docs_path(&page.slug);
    let page_url = absolute_url(&page_path);

    json!({
        "@context": "https://schema.org",
        "@graph": [
            {
                "@type": "TechArticle",
                "@id": format!("{page_url}#article"),
                "headline": page.title,
                "description": page.description,
                "url": page_url,
                "mainEntityOfPage": page_url,
                "inLanguage": "en-US",
                "isPartOf": {
                    "@id": format!("{SITE_BASE_URL}/#website")
                },
                "about": {
                    "@id": format!("{SITE_BASE_URL}/#source"),
                    "name": SITE_NAME
                }
            },
            {
                "@type": "BreadcrumbList",
                "@id": format!("{page_url}#breadcrumb"),
                "itemListElement": [
                    {
                        "@type": "ListItem",
                        "position": 1,
                        "name": SITE_NAME,
                        "item": absolute_url("/")
                    },
                    {
                        "@type": "ListItem",
                        "position": 2,
                        "name": page.title,
                        "item": page_url
                    }
                ]
            }
        ]
    })
    .to_string()
}

/// `robots.txt`, allowing the whole site except the machine-readable mirror of
/// it.
///
/// `/api/` serves the same guides as JSON for agents. Letting a crawler index
/// it would put a second, uglier copy of every guide in the index competing
/// with the HTML page that should rank — so it is disallowed here while staying
/// fully open to the clients it exists for, which do not read `robots.txt`.
/// `/mcp` (the JSON-RPC envelope over those same handlers) is deliberately left
/// crawlable so agents that honor `robots.txt` can still discover and use it.
///
/// The `Content-Signal` line declares AI-usage preferences (see
/// <https://contentsignals.org/>): these are public framework docs, so search
/// indexing, AI grounding/input, and AI training are all welcome.
#[must_use]
pub fn robots_txt() -> String {
    format!(
        "User-agent: *\nContent-Signal: ai-train=yes, search=yes, ai-input=yes\nAllow: /\nDisallow: /api/\n\nSitemap: {SITE_BASE_URL}/sitemap.xml\n"
    )
}

/// Identity the mounted MCP server reports in `initialize.serverInfo`.
///
/// `autumn-web` hard-codes both (its own package name and version) and offers
/// no accessor, so they are mirrored here; `tests/mcp_docs_api.rs` compares
/// them with a live `initialize` response, so an `autumn-web` upgrade that
/// changes either fails CI instead of leaving the card stale.
pub const MCP_SERVER_NAME: &str = "autumn-mcp";
pub const MCP_SERVER_VERSION: &str = AUTUMN_VERSION;

/// MCP Server Card (SEP-1649), served at `/.well-known/mcp/server-card.json`
/// so an agent can discover the `/mcp` server without being told about it.
///
/// Tools-only server: the catalog is derived from the `#[api_doc(mcp)]` routes
/// in `src/api.rs`, so the card marks it `"dynamic"` rather than duplicating
/// the descriptors; `tools/list` is the source of truth.
#[must_use]
pub fn mcp_server_card() -> String {
    serde_json::json!({
        "$schema": "https://static.modelcontextprotocol.io/schemas/mcp-server-card/v1.json",
        "version": "1.0",
        "protocolVersion": "2025-06-18",
        "serverInfo": {
            "name": MCP_SERVER_NAME,
            "title": "Autumn Docs",
            "version": MCP_SERVER_VERSION
        },
        "description": "Search and read the Autumn and Autumn Harvest guides as Markdown.",
        "documentationUrl": absolute_url("/docs/mcp"),
        "transport": {
            "type": "streamable-http",
            "endpoint": crate::MCP_MOUNT_PATH
        },
        "endpoint": absolute_url(crate::MCP_MOUNT_PATH),
        "authentication": { "required": false, "schemes": [] },
        "capabilities": {
            "tools": { "listChanged": false }
        },
        "tools": ["dynamic"]
    })
    .to_string()
}

/// The RFC 9727 API catalog served at `/.well-known/api-catalog`.
///
/// A linkset (RFC 9264) with one entry for the public docs API under `/api/`:
/// `service-desc` points at the generated OpenAPI document, `service-doc` at
/// the human-readable guides, and `status` at the health endpoint.
#[must_use]
pub fn api_catalog() -> String {
    serde_json::json!({
        "linkset": [
            {
                "anchor": absolute_url("/api/"),
                "service-desc": [
                    {
                        "href": absolute_url("/openapi.json"),
                        "type": "application/json"
                    }
                ],
                "service-doc": [
                    {
                        "href": absolute_url("/docs"),
                        "type": "text/html"
                    }
                ],
                "status": [
                    {
                        "href": absolute_url("/health"),
                        "type": "application/json"
                    }
                ]
            }
        ]
    })
    .to_string()
}

/// Canonical path of the ARD (Agentic Resource Discovery) manifest.
pub const ARD_PATH: &str = "/.well-known/ard.json";

/// Predecessor path, still served so consumers that only check it find the
/// same manifest.
pub const AI_CATALOG_PATH: &str = "/.well-known/ai-catalog.json";

/// The ARD manifest served at [`ARD_PATH`] (and [`AI_CATALOG_PATH`]), so agents can discover the
/// site's MCP server and JSON docs API without parsing HTML.
///
/// Each entry carries exactly one of `url` or `data`: the MCP server is
/// described inline in the current `ext-server-card` vocabulary (the
/// SEP-1649-layout card at `/.well-known/mcp/server-card.json` uses a different
/// `version` meaning, so the two cannot be one document), and the JSON API is
/// linked by URL.
#[must_use]
pub fn ai_catalog_json() -> String {
    let domain = SITE_BASE_URL.trim_start_matches("https://");
    serde_json::to_string_pretty(&json!({
        "specVersion": "1.0",
        "host": {
            "displayName": SITE_NAME,
            "identifier": format!("did:web:{domain}")
        },
        "entries": [
            {
                "identifier": format!("urn:air:{domain}:mcp:docs"),
                "displayName": "Autumn docs MCP server",
                "type": "application/mcp-server-card+json",
                "data": {
                    "$schema": "https://static.modelcontextprotocol.io/schemas/v1/server-card.schema.json",
                    "name": "app.autumn-web/docs",
                    "title": "Autumn docs MCP server",
                    "description": "Read-only MCP server over the Autumn and Harvest guides for the deployed release.",
                    "version": AUTUMN_VERSION,
                    "remotes": [
                        { "type": "streamable-http", "url": absolute_url("/mcp") }
                    ]
                },
                "representativeQueries": [
                    "search the Autumn Rust web framework documentation",
                    "how do I define typed routes in Autumn",
                    "read the Autumn deployment guide",
                    "which Autumn version does this documentation describe"
                ]
            },
            {
                "identifier": format!("urn:air:{domain}:docs:json-api"),
                "displayName": "Autumn docs JSON API",
                "type": "application/json",
                "url": absolute_url("/api/docs"),
                "representativeQueries": [
                    "list all Autumn framework guides as JSON",
                    "fetch an Autumn guide as Markdown by slug",
                    "full-text search of Autumn and Harvest docs"
                ]
            }
        ]
    }))
    .expect("static JSON value always serializes")
        + "\n"
}

/// Path of the Web Bot Auth key directory.
pub const WEB_BOT_AUTH_PATH: &str = "/.well-known/http-message-signatures-directory";

/// Media type the Web Bot Auth draft assigns to the key directory.
pub const WEB_BOT_AUTH_CONTENT_TYPE: &str = "application/http-message-signatures-directory+json";

/// The site's Web Bot Auth key directory: a JWKS holding the Ed25519 public
/// key (`kid` is its RFC 7638 thumbprint) that receiving sites use to verify
/// requests this site signs as a bot or agent.
///
/// Only the public half lives here. To rotate, generate a new Ed25519 key,
/// replace the entry, and keep the old one listed until signed traffic drains.
pub const WEB_BOT_AUTH_DIRECTORY: &str = r#"{"keys":[{"kty":"OKP","crv":"Ed25519","x":"54vmrf8D78z2CRDIIjBEsd0Zzer3JgcjU8yIi6y5JuY","kid":"L8BOcML4IEa9Bcz6_cmkOlrDESF06ZmJqfKfkp2LA1E"}]}"#;

#[must_use]
pub fn sitemap_xml(registry: &DocRegistry) -> String {
    let mut sitemap = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );

    push_sitemap_url(&mut sitemap, &absolute_url("/"));
    for page in registry.pages() {
        push_sitemap_url(&mut sitemap, &absolute_url(&docs_path(&page.slug)));
    }

    sitemap.push_str("</urlset>\n");
    sitemap
}

fn push_sitemap_url(sitemap: &mut String, url: &str) {
    sitemap.push_str("  <url>\n    <loc>");
    sitemap.push_str(&xml_escape(url));
    sitemap.push_str("</loc>\n  </url>\n");
}

fn xml_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for char in value.chars() {
        match char {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(char),
        }
    }
    escaped
}
