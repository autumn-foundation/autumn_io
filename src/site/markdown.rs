//! The Markdown twin of every page [`crate::site`] renders as HTML.
//!
//! Each function here answers the same request as the `render_*` function it
//! sits beside in the parent module, for a client that asked for
//! `text/markdown` (see [`crate::negotiate`]). A guide's body needs no
//! conversion — the site holds the Markdown the HTML was rendered *from*, so
//! the agent gets the author's source, not a lossy round trip through
//! syntax-highlighted markup.
//!
//! What is written here is the *page around* that body: the title, the
//! description, the navigation, and, for the home page, the prose that exists
//! only inside `html!` macros. Those strings are shared constants in the parent
//! module rather than copies, so the two representations cannot drift on the
//! facts. The framing differs deliberately — an agent gets a linked guide index
//! where a reader gets a sidebar, and no skip link, header, or footer chrome at
//! all.

use std::error::Error;
use std::fmt::Write as _;

use crate::docs::{DocPage, DocRegistry, SearchHit};
use crate::seo;
use crate::{DOCS_SEARCH_PATH, DOCS_START_SLUG};

use super::{
    DOCS_NAV_GROUPS, HARVEST_DOC_PATH, HARVEST_GUIDE_START_PATH, HARVEST_LEDE,
    HOME_FEATURED_DOC_SLUGS, HOME_HEADLINE, HOME_LEDE, HOME_ROUTE_EXAMPLE, UNGROUPED_DOCS_LABEL,
    docs_navigation_neighbors, home_mcp_example, home_secondary_pages, is_grouped_doc_slug,
};

/// How the site describes its own Markdown negotiation to an agent reading the
/// home page.
///
/// The one piece of documentation an agent cannot get anywhere else: it is
/// already holding a Markdown response, so it knows the feature exists, but not
/// that it covers every page.
const MARKDOWN_NEGOTIATION_NOTE: &str = "Every page on this site answers `Accept: text/markdown` \
     with its Markdown source, at the same URL a browser uses. The response carries \
     `x-markdown-tokens`, an estimate of what the body costs to read.";

/// The home page: what Autumn is, where to start, and how an agent reads the
/// rest of the site.
#[must_use]
pub fn render_home_page(registry: &DocRegistry) -> String {
    let mut out = String::new();

    let _ = writeln!(out, "# {} {}\n", seo::SITE_NAME, seo::AUTUMN_VERSION);
    let _ = writeln!(out, "{HOME_HEADLINE}\n");
    let _ = writeln!(out, "{HOME_LEDE}\n");
    let _ = writeln!(out, "```rust\n{HOME_ROUTE_EXAMPLE}\n```\n");

    let _ = writeln!(
        out,
        "## Autumn Harvest {}\n\n{HARVEST_LEDE}\n",
        seo::HARVEST_VERSION,
    );
    let _ = writeln!(
        out,
        "- [Harvest overview]({})",
        seo::absolute_url(HARVEST_DOC_PATH),
    );
    let _ = writeln!(
        out,
        "- [Harvest guide]({})",
        seo::absolute_url(HARVEST_GUIDE_START_PATH),
    );
    let _ = writeln!(out, "- [Harvest API docs]({})", seo::HARVEST_RUSTDOC_URL);
    let _ = writeln!(out, "- [Harvest crate]({})\n", seo::HARVEST_CRATES_IO_URL);

    let _ = writeln!(out, "## Start here\n");
    for page in HOME_FEATURED_DOC_SLUGS
        .iter()
        .filter_map(|slug| registry.page(slug))
        .chain(home_secondary_pages(registry))
    {
        let _ = writeln!(out, "{}", guide_list_item(page));
    }

    let _ = writeln!(out, "\n## For coding agents\n");
    let _ = writeln!(
        out,
        "This site serves its own guides over the [Model Context \
         Protocol](https://modelcontextprotocol.io) at {}. It is public, \
         unauthenticated, and read-only, and it answers from the {} {} guides that are \
         deployed rather than from whatever an agent remembers.\n",
        seo::absolute_url(crate::MCP_MOUNT_PATH),
        seo::SITE_NAME,
        seo::AUTUMN_VERSION,
    );
    let _ = writeln!(out, "```bash\n{}\n```\n", home_mcp_example());
    let _ = writeln!(out, "{MARKDOWN_NEGOTIATION_NOTE}\n");

    let _ = write!(out, "{}", guide_index(registry));
    let _ = write!(out, "{}", project_links());

    out
}

/// A guide: its own Markdown, framed by the metadata and navigation the HTML
/// page puts around it.
///
/// The body is [`DocPage::markdown`] verbatim — the same string the HTML render
/// starts from, with the frontmatter and the redundant `# Title` heading
/// already stripped — so the `# {title}` written here restores exactly one
/// top-level heading.
#[must_use]
pub fn render_docs_page(registry: &DocRegistry, page: &DocPage) -> String {
    let mut out = String::new();

    let _ = writeln!(out, "# {}\n", page.title);
    if !page.description.is_empty() && !opens_with(&page.markdown, &page.description) {
        let _ = writeln!(out, "{}\n", page.description);
    }

    let _ = writeln!(out, "{}", page.markdown.trim_end());

    // The sidebar's order, not the registry's: an agent walking `Next` should
    // traverse the guides in the same sequence the site paginates them.
    let (previous, next) = docs_navigation_neighbors(registry, &page.slug);
    let _ = writeln!(out, "\n---\n");
    let _ = writeln!(
        out,
        "- This page on the web: {}",
        seo::absolute_url(&seo::docs_path(&page.slug)),
    );
    if let Some(previous) = previous {
        let _ = writeln!(out, "- Previous guide: {}", guide_link(previous));
    }
    if let Some(next) = next {
        let _ = writeln!(out, "- Next guide: {}", guide_link(next));
    }
    let _ = writeln!(out, "- All guides: {}", seo::absolute_url("/"));

    out
}

/// The `404` an agent gets for a slug that is not a guide.
///
/// Carries the full guide index for the same reason the HTML page renders the
/// whole sidebar: the useful answer to a wrong slug is the list of right ones.
#[must_use]
pub fn render_missing_docs_page(registry: &DocRegistry, slug: &str) -> String {
    let mut out = String::new();

    let _ = writeln!(out, "# That docs page is not in the stack\n");
    let _ = writeln!(
        out,
        "No {} docs page exists for `{slug}`. The route is valid; the page is not.\n",
        seo::SITE_NAME,
    );
    let _ = writeln!(
        out,
        "Start from [Getting Started]({}).",
        seo::absolute_url(&seo::docs_path(DOCS_START_SLUG)),
    );

    let _ = write!(out, "{}", guide_index(registry));

    out
}

/// Search results, as the list of guides they are.
///
/// Takes the hits rather than rendered markup so the handler's one search runs
/// once and feeds whichever representation was asked for.
#[must_use]
pub fn render_docs_search_page(query: &str, hits: Option<&[SearchHit]>) -> String {
    let mut out = String::new();

    let _ = writeln!(out, "# Search the guides\n");

    let Some(hits) = hits else {
        // The index is absent only because the bundled content failed to parse,
        // so this is a server failure and the handler answers it with a `500`.
        // Say which, rather than leaving a caller to infer an empty corpus.
        let _ = writeln!(
            out,
            "Search is unavailable: the bundled Markdown content could not be parsed.",
        );
        return out;
    };

    if query.is_empty() {
        let _ = writeln!(
            out,
            "Enter a search term to find matching guides: `{}?q=your+terms`.",
            seo::absolute_url(DOCS_SEARCH_PATH),
        );
        return out;
    }

    if hits.is_empty() {
        let _ = writeln!(out, "No guides match “{query}”.");
        return out;
    }

    let _ = writeln!(
        out,
        "{} result{} for “{query}”.\n",
        hits.len(),
        if hits.len() == 1 { "" } else { "s" },
    );

    for hit in hits {
        let link = format!(
            "[{}]({})",
            hit.title,
            seo::absolute_url(&seo::docs_path(&hit.slug)),
        );
        if hit.snippet.is_empty() {
            let _ = writeln!(out, "- {link}");
        } else {
            let _ = writeln!(out, "- {link} — {}", hit.snippet);
        }
    }

    out
}

/// The `500` an agent gets when the bundled content failed to parse.
///
/// Reports the failure rather than an empty page, for the same reason the HTML
/// twin prints the error: a caller that cannot tell "broken" from "empty"
/// caches the wrong conclusion.
#[must_use]
pub fn render_docs_load_error(error: &dyn Error) -> String {
    format!(
        "# Docs failed to load\n\nThe bundled Markdown content could not be parsed.\n\n\
         ```text\n{error}\n```\n",
    )
}

/// Every guide, under the sidebar's own headings and in the sidebar's order.
///
/// An agent has no sidebar to browse, so this is the whole navigational
/// structure of the site in one list. Built from [`DOCS_NAV_GROUPS`] and the
/// same ungrouped fallback the sidebar uses, so the two cannot drift.
fn guide_index(registry: &DocRegistry) -> String {
    let mut out = String::from("\n## All guides\n");

    for group in DOCS_NAV_GROUPS {
        let pages: Vec<&DocPage> = group
            .slugs
            .iter()
            .filter_map(|slug| registry.page(slug))
            .collect();
        if pages.is_empty() {
            continue;
        }

        let _ = writeln!(out, "\n### {}\n", group.label);
        for page in pages {
            let _ = writeln!(out, "{}", guide_list_item(page));
        }
    }

    let ungrouped: Vec<&DocPage> = registry
        .pages()
        .iter()
        .filter(|page| !is_grouped_doc_slug(&page.slug))
        .collect();
    if !ungrouped.is_empty() {
        let _ = writeln!(out, "\n### {UNGROUPED_DOCS_LABEL}\n");
        for page in ungrouped {
            let _ = writeln!(out, "{}", guide_list_item(page));
        }
    }

    out
}

/// Where the project itself lives — the footer's links, minus the chrome.
fn project_links() -> String {
    format!(
        "\n## Project\n\n\
         - [Autumn source]({})\n\
         - [Autumn crate]({})\n\
         - [Autumn API docs]({})\n\
         - [Harvest source]({})\n\
         - [Website source]({})\n",
        seo::GITHUB_REPOSITORY_URL,
        seo::CRATES_IO_URL,
        seo::RUSTDOC_URL,
        seo::HARVEST_REPOSITORY_URL,
        seo::WEBSITE_REPOSITORY_URL,
    )
}

/// Whether `markdown` already opens with `description`, ignoring how the
/// paragraph is wrapped.
///
/// Most guides use their frontmatter description as their own opening
/// paragraph. The HTML page can print it twice without anyone noticing — once
/// as the `<meta name="description">` a reader never sees, once as the lede —
/// but in Markdown both land in the body, and an agent would read the same
/// sentences twice and pay for them twice. The source is hard-wrapped and the
/// frontmatter is not, so the comparison collapses whitespace on both sides
/// rather than matching byte for byte.
fn opens_with(markdown: &str, description: &str) -> bool {
    let mut body = markdown.split_whitespace();
    description
        .split_whitespace()
        .all(|word| body.next() == Some(word))
}

/// One guide as a list item: a link, and its own one-line summary.
fn guide_list_item(page: &DocPage) -> String {
    if page.description.is_empty() {
        format!("- {}", guide_link(page))
    } else {
        format!("- {} — {}", guide_link(page), page.description)
    }
}

/// A guide's title linked to its absolute URL, so a link survives being read
/// outside the response it arrived in.
fn guide_link(page: &DocPage) -> String {
    format!(
        "[{}]({})",
        page.title,
        seo::absolute_url(&seo::docs_path(&page.slug)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape almost every bundled guide has: a one-sentence frontmatter
    /// description, then the same sentence hard-wrapped as the opening
    /// paragraph.
    #[test]
    fn a_description_the_body_repeats_is_not_printed_twice() {
        assert!(opens_with(
            "This guide takes you from an empty directory\nto a running app.\n\n## Prerequisites",
            "This guide takes you from an empty directory to a running app.",
        ));
    }

    #[test]
    fn a_description_the_body_does_not_repeat_is_kept() {
        assert!(!opens_with(
            "Autumn's job runner is Postgres-backed.\n",
            "Everything you need to know about background work.",
        ));
        assert!(
            !opens_with(
                "Short body.",
                "Short body that goes on for longer than the body does."
            ),
            "a body that merely starts the description is not the description",
        );
    }
}
