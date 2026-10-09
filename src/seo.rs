use serde_json::json;

use crate::docs::{DocPage, DocRegistry};

pub const SITE_BASE_URL: &str = "https://autumn-web.app";
pub const SITE_NAME: &str = "Autumn";
pub const SITE_DESCRIPTION: &str = "Autumn is a Rust web framework for fast server-rendered apps, typed routes, Maud templates, static assets, and production defaults.";
const MCP_PATH: &str = "/mcp";
pub const SITE_IMAGE_PATH: &str = "/static/img/autumn-social.png";
pub const GITHUB_REPOSITORY_URL: &str = "https://github.com/autumn-foundation/autumn";
pub const WEBSITE_REPOSITORY_URL: &str = "https://github.com/autumn-foundation/autumn_io";
pub const CRATES_IO_URL: &str = "https://crates.io/crates/autumn-web";
pub const RUSTDOC_URL: &str = "https://docs.rs/autumn-web";
pub const AUTUMN_VERSION: &str = "0.7.0";
pub const HARVEST_REPOSITORY_URL: &str = "https://github.com/autumn-foundation/autumn-harvest";
pub const HARVEST_CRATES_IO_URL: &str = "https://crates.io/crates/autumn-harvest";
pub const HARVEST_RUSTDOC_URL: &str = "https://docs.rs/autumn-harvest";
pub const HARVEST_VERSION: &str = "0.6.0";

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
/// `/api/` serves the same guides as JSON for agents (and `/mcp` is the
/// JSON-RPC envelope over those same handlers). Letting a crawler index them
/// would put a second, uglier copy of every guide in the index competing with
/// the HTML page that should rank — so they are disallowed here while staying
/// fully open to the clients they exist for, which do not read `robots.txt`.
#[must_use]
pub fn robots_txt() -> String {
    format!(
        "User-agent: *\nAllow: /\nDisallow: /api/\nDisallow: /mcp\n\nSitemap: {SITE_BASE_URL}/sitemap.xml\n"
    )
}

/// Path of the Auth.md document agents read to learn how to get access.
pub const AUTH_MD_PATH: &str = "/auth.md";
/// Path of the OAuth Protected Resource Metadata (RFC 9728).
pub const OAUTH_PROTECTED_RESOURCE_PATH: &str = "/.well-known/oauth-protected-resource";
/// Path of the OAuth Authorization Server Metadata (RFC 8414).
pub const OAUTH_AUTHORIZATION_SERVER_PATH: &str = "/.well-known/oauth-authorization-server";

/// `/auth.md`: how an agent gets access to this site.
///
/// The honest answer is that it needs nothing. The site has no accounts,
/// sessions or tokens, and everything it serves is public documentation, so
/// the only "registration" is anonymous and no credential is ever issued.
/// Publishing that explicitly lets an agent stop looking for a sign-up flow.
#[must_use]
pub fn auth_md() -> String {
    format!(
        "# auth.md\n\n\
Agent registration instructions for {SITE_NAME} ({SITE_BASE_URL}).\n\n\
## Audience\n\n\
Software agents, crawlers and coding assistants reading the {SITE_NAME} documentation.\n\n\
## Summary\n\n\
**No registration is required.** This site has no user accounts, sessions, API keys or\n\
OAuth tokens. Every read-only endpoint is public and anonymous. No credential is issued,\n\
so there is nothing to claim and nothing to revoke.\n\n\
## Identity types\n\n\
- `anonymous`: the only supported type. Send requests without an `Authorization` header.\n\n\
## Endpoints\n\n\
- Docs as Markdown: `GET {SITE_BASE_URL}/docs/{{slug}}` with `Accept: text/markdown`\n\
- Docs JSON API: `GET {SITE_BASE_URL}/api/docs`\n\
- MCP server (read-only tools): `POST {SITE_BASE_URL}{MCP_PATH}`\n\n\
## Credentials\n\n\
None. Credential types supported: `none`. `Authorization` headers are ignored.\n\n\
## Discovery\n\n\
- Protected resource metadata: {SITE_BASE_URL}{OAUTH_PROTECTED_RESOURCE_PATH}\n\
- Authorization server metadata: {SITE_BASE_URL}{OAUTH_AUTHORIZATION_SERVER_PATH}\n\n\
## Etiquette\n\n\
Responses are cached at the edge; honour `Cache-Control` and `ETag` and keep request rates\n\
reasonable.\n"
    )
}

/// `/.well-known/oauth-protected-resource` (RFC 9728).
///
/// The resource is the site itself and its own origin is the (anonymous)
/// authorization server, because there is no separate issuer to point at.
#[must_use]
pub fn oauth_protected_resource() -> String {
    json!({
        "resource": SITE_BASE_URL,
        "authorization_servers": [SITE_BASE_URL],
        "scopes_supported": ["docs:read"],
        "bearer_methods_supported": ["header"],
        "resource_name": SITE_NAME,
        "resource_documentation": absolute_url(AUTH_MD_PATH)
    })
    .to_string()
}

/// `/.well-known/oauth-authorization-server` (RFC 8414) with an `agent_auth`
/// block describing anonymous registration.
///
/// No `authorization_endpoint`, `token_endpoint`, `revocation_uri` or empty
/// grant/response-type lists (RFC 8414 §3.2 forbids zero-element claims) are
/// advertised: this site issues no credentials, so there is nothing to
/// exchange, claim or revoke. `register_uri` points at `/auth.md`, which
/// states that registration is unnecessary.
#[must_use]
pub fn oauth_authorization_server() -> String {
    json!({
        "issuer": SITE_BASE_URL,
        "scopes_supported": ["docs:read"],
        "service_documentation": absolute_url(AUTH_MD_PATH),
        "agent_auth": {
            "skill": absolute_url(AUTH_MD_PATH),
            "register_uri": absolute_url(AUTH_MD_PATH),
            "identity_types_supported": ["anonymous"],
            "anonymous": {
                "credential_types_supported": ["none"]
            }
        }
    })
    .to_string()
}

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
