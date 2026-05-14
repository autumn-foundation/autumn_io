use serde_json::json;

use crate::docs::{DocPage, DocRegistry};

pub const SITE_BASE_URL: &str = "https://autumn-web.app";
pub const SITE_NAME: &str = "Autumn";
pub const SITE_DESCRIPTION: &str = "Autumn is a Rust web framework for fast server-rendered apps, typed routes, Maud templates, static assets, and production defaults.";
pub const SITE_IMAGE_PATH: &str = "/static/img/autumn-social.png";
pub const GITHUB_REPOSITORY_URL: &str = "https://github.com/madmax983/autumn";
pub const WEBSITE_REPOSITORY_URL: &str = "https://github.com/madmax983/autumn_io";
pub const CRATES_IO_URL: &str = "https://crates.io/crates/autumn-web";
pub const RUSTDOC_URL: &str = "https://docs.rs/autumn-web";
pub const AUTUMN_VERSION: &str = "0.4.0";
pub const HARVEST_REPOSITORY_URL: &str = "https://github.com/madmax983/autumn-harvest";
pub const HARVEST_GUIDE_URL: &str =
    "https://github.com/madmax983/autumn-harvest/tree/trunk/docs/getting-started";
pub const HARVEST_CRATES_IO_URL: &str = "https://crates.io/crates/autumn-harvest";
pub const HARVEST_RUSTDOC_URL: &str = "https://docs.rs/autumn-harvest";
pub const HARVEST_VERSION: &str = "0.3.0";

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

#[must_use]
pub fn docs_path(slug: &str) -> String {
    format!("/docs/{slug}")
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

#[must_use]
pub fn robots_txt() -> String {
    format!("User-agent: *\nAllow: /\n\nSitemap: {SITE_BASE_URL}/sitemap.xml\n")
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
