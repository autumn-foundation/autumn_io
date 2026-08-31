//! A read-only JSON API over the bundled guides, and through it the site's MCP
//! server.
//!
//! The site already holds every Autumn and Harvest guide in memory, already
//! indexes them for search, and is already redeployed whenever upstream docs
//! move. That makes it the natural place to answer the question a coding agent
//! keeps getting wrong: *what does this version of Autumn actually do?* An
//! agent that can call [`search_autumn_docs`] and [`get_autumn_doc`] reads the
//! guides for the release that is deployed, instead of recalling whatever it
//! saw during training.
//!
//! Every handler here is an ordinary Autumn route returning `Json<T>`. Tagging
//! them `#[api_doc(mcp)]` and calling `mount_mcp("/mcp")` in `main` is the
//! whole MCP server: Autumn derives the tool catalog from the same `ApiDoc`
//! metadata that drives its OpenAPI document, so the tool schemas cannot drift
//! from these signatures. See `content/guide/mcp.md` for the mechanism.
//!
//! ## Why the tools are shaped this way
//!
//! The guides are not uniformly sized. Most are a few kilobytes, but
//! `deployment.md` is over 150 KB — roughly forty thousand tokens, more than an
//! agent should ever receive from one tool call. So [`get_autumn_doc`] returns
//! a whole guide only while it fits [`MAX_INLINE_DOC_BYTES`]; past that it
//! withholds the body and returns the guide's section list instead, letting the
//! agent ask again for the one section it wants. Section ids are the anchors
//! the rendered page already uses, so a section reference is also a working
//! deep link a human can open.

use autumn_web::openapi::OpenApiSchema;
use autumn_web::prelude::*;
use autumn_web::reexports::axum::response::{IntoResponse, Response};
use autumn_web::reexports::http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::docs::{DocPage, DocRegistry, DocsError};
use crate::{seo, site};

/// Largest guide, in bytes of Markdown, that [`get_autumn_doc`] will return in
/// one piece.
///
/// Three of the guides sit above this line. For those, an unrequested full body
/// would dominate an agent's context window, so the response carries the
/// section list and a [`GuideDocument::notice`] instead — a cheap extra
/// round-trip in exchange for never blowing up the caller.
pub const MAX_INLINE_DOC_BYTES: usize = 60_000;

/// Default and maximum number of hits [`search_autumn_docs`] returns.
const DEFAULT_SEARCH_LIMIT: usize = 10;
const MAX_SEARCH_LIMIT: usize = 50;

/// How much of a guide's frontmatter description [`list_autumn_docs`] carries.
///
/// The guides write long descriptions — a median of 276 characters and a
/// maximum near a thousand — and 140 of those unabridged make the index alone a
/// five-figure token cost. A first sentence is enough to choose between guides,
/// and [`get_autumn_doc`] still returns the description in full.
const MAX_INDEX_DESCRIPTION_BYTES: usize = 180;

// ─────────────────────────────────────────────────────────────────────────
// Response types
// ─────────────────────────────────────────────────────────────────────────

/// One guide as it appears in the index: enough to choose a guide, not enough
/// to read one.
#[derive(Debug, Serialize, OpenApiSchema)]
pub struct GuideSummary {
    /// Identifier accepted by `get_autumn_doc`, and the last segment of the
    /// guide's URL under `https://autumn-web.app/docs/`.
    pub slug: String,
    pub title: String,
    /// The guide's own summary, from its frontmatter, abridged to a sentence.
    /// `get_autumn_doc` returns it in full.
    pub description: String,
    /// Sidebar section this guide belongs to on the website.
    pub group: String,
    /// Size of the guide's Markdown, so a caller can tell up front whether
    /// `get_autumn_doc` will return it whole.
    pub bytes: usize,
}

/// One sidebar section and how many guides it holds.
#[derive(Debug, Serialize, OpenApiSchema)]
pub struct GuideGroup {
    pub name: String,
    pub count: usize,
}

/// The guide index returned by [`list_autumn_docs`].
#[derive(Debug, Serialize, OpenApiSchema)]
pub struct GuideIndex {
    /// Version of `autumn-web` these guides document.
    pub autumn_version: String,
    /// Version of `autumn-harvest` the `harvest-*` guides document.
    pub harvest_version: String,
    /// Every group, with its guide count, regardless of any `group` filter —
    /// so one call is enough to learn how to narrow the next one.
    pub groups: Vec<GuideGroup>,
    /// The group filter this response was narrowed to, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Number of guides in `guides`, after any filtering.
    pub count: usize,
    pub guides: Vec<GuideSummary>,
}

/// One search hit: where the match is and enough context to judge it.
#[derive(Debug, Serialize, OpenApiSchema)]
pub struct GuideSearchHit {
    pub slug: String,
    pub title: String,
    /// Plain-text excerpt around the match.
    pub snippet: String,
    pub url: String,
}

/// The result set returned by [`search_autumn_docs`].
#[derive(Debug, Serialize, OpenApiSchema)]
pub struct GuideSearchResults {
    /// The query as it was searched, after trimming.
    pub query: String,
    pub count: usize,
    pub results: Vec<GuideSearchHit>,
}

/// One heading in a guide, and the handle for fetching just that part of it.
#[derive(Debug, Serialize, OpenApiSchema)]
pub struct GuideSectionRef {
    /// Pass this back as `get_autumn_doc`'s `section` argument; it is also the
    /// `#fragment` of the heading on the website.
    pub id: String,
    pub title: String,
    /// Markdown heading depth: 2 for `##`, 3 for `###`, and so on.
    pub level: u8,
}

/// A guide as returned by [`get_autumn_doc`] — whole, or one section of it.
#[derive(Debug, Serialize, OpenApiSchema)]
pub struct GuideDocument {
    pub slug: String,
    pub title: String,
    pub description: String,
    pub group: String,
    pub url: String,
    pub autumn_version: String,
    /// Every heading in the guide, in document order.
    pub sections: Vec<GuideSectionRef>,
    /// The requested section's id, when the response is one section rather
    /// than the whole guide.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    /// The Markdown itself, or `null` when the guide was too large to inline
    /// and no section was requested — see [`notice`](GuideDocument::notice).
    pub markdown: Option<String>,
    /// Present only when something about the response needs explaining, so an
    /// agent that gets a `null` body is told what to do next.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────
// Query types
// ─────────────────────────────────────────────────────────────────────────

/// Query arguments for [`list_autumn_docs`].
#[derive(Debug, Deserialize, OpenApiSchema)]
pub struct ListDocsQuery {
    /// Return only the guides in this sidebar group, matched case-insensitively
    /// against a name from a previous response's `groups`. Omit it for all of
    /// them.
    pub group: Option<String>,
}

/// Query arguments for [`search_autumn_docs`].
#[derive(Debug, Deserialize, OpenApiSchema)]
pub struct SearchDocsQuery {
    /// Words to search for. All of them must appear in a guide for it to match,
    /// so prefer two or three distinctive terms over a whole question.
    pub q: String,
    /// Maximum number of hits, 1–50. Defaults to 10.
    pub limit: Option<usize>,
}

/// Query arguments for [`get_autumn_doc`].
#[derive(Debug, Deserialize, OpenApiSchema)]
pub struct GetDocQuery {
    /// Return only this section of the guide, identified by an `id` from a
    /// previous response's `sections`. Omit it for the whole guide.
    pub section: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────
// Handlers
// ─────────────────────────────────────────────────────────────────────────

/// List the bundled guides, optionally narrowed to one sidebar group.
#[get("/api/docs")]
#[api_doc(
    mcp,
    operation_id = "list_autumn_docs",
    tag = "docs",
    summary = "List the Autumn and Autumn Harvest guides",
    description = "Return the slug, title, abridged description, sidebar group, \
                   and Markdown size of every guide bundled with this site, plus \
                   the autumn-web and autumn-harvest versions they document. \
                   There are around 140 guides, so pass the optional `group` \
                   argument — a name from the `groups` list every response \
                   carries — to list one section at a time. Use this to browse \
                   what documentation exists; use search_autumn_docs when you \
                   already know what you are looking for, and get_autumn_doc to \
                   read a guide by slug."
)]
pub async fn list_autumn_docs(
    Query(query): Query<ListDocsQuery>,
) -> Result<Json<GuideIndex>, DocsApiError> {
    let registry = registry()?;

    let mut groups: Vec<GuideGroup> = Vec::new();
    for page in registry.pages() {
        let name = site::doc_group_label(&page.slug);
        match groups.iter_mut().find(|group| group.name == name) {
            Some(group) => group.count += 1,
            None => groups.push(GuideGroup {
                name: name.to_owned(),
                count: 1,
            }),
        }
    }

    let filter = query
        .group
        .as_deref()
        .map(str::trim)
        .filter(|group| !group.is_empty());

    // Resolve the filter to the canonical label so the echoed `group` matches
    // the `groups` list even when the agent guessed the casing.
    let filter = match filter {
        Some(requested) => Some(
            groups
                .iter()
                .find(|group| group.name.eq_ignore_ascii_case(requested))
                .map(|group| group.name.clone())
                .ok_or_else(|| DocsApiError::UnknownGroup(requested.to_owned()))?,
        ),
        None => None,
    };

    let guides: Vec<GuideSummary> = registry
        .pages()
        .iter()
        .filter(|page| {
            filter
                .as_deref()
                .is_none_or(|group| site::doc_group_label(&page.slug) == group)
        })
        .map(guide_summary)
        .collect();

    Ok(Json(GuideIndex {
        autumn_version: seo::AUTUMN_VERSION.to_owned(),
        harvest_version: seo::HARVEST_VERSION.to_owned(),
        groups,
        group: filter,
        count: guides.len(),
        guides,
    }))
}

/// Full-text search across the bundled guides.
///
/// Served at `/api/search` rather than `/api/docs/search`: an exact route under
/// `/api/docs/` would shadow the guide of the same slug, and upstream ships a
/// guide called `search`. The site's HTML search is at `/search` for the same
/// reason.
#[get("/api/search")]
#[api_doc(
    mcp,
    operation_id = "search_autumn_docs",
    tag = "docs",
    summary = "Search the Autumn and Autumn Harvest guides",
    description = "Find guides matching a set of terms, ranked by where the terms \
                   appear (title, then heading, then body). Every term must appear \
                   in a guide for it to match, so two or three distinctive words \
                   work better than a sentence. Returns each hit's slug and a \
                   text snippet around the match; pass a slug to get_autumn_doc to \
                   read the guide itself. This is the right first call when \
                   answering a question about how Autumn works."
)]
pub async fn search_autumn_docs(
    Query(query): Query<SearchDocsQuery>,
) -> Result<Json<GuideSearchResults>, DocsUnavailable> {
    let term = query.q.trim();
    let limit = query
        .limit
        .unwrap_or(DEFAULT_SEARCH_LIMIT)
        .clamp(1, MAX_SEARCH_LIMIT);

    let hits = match crate::site_search_index() {
        Some(index) if !term.is_empty() => index.search(term, limit),
        // An empty query is a caller mistake rather than a server fault, and an
        // empty result set says so without costing a round-trip to an error.
        Some(_) => Vec::new(),
        None => return Err(DocsUnavailable),
    };

    let results: Vec<GuideSearchHit> = hits
        .into_iter()
        .map(|hit| GuideSearchHit {
            url: seo::absolute_url(&seo::docs_path(&hit.slug)),
            slug: hit.slug,
            title: hit.title,
            snippet: hit.snippet,
        })
        .collect();

    Ok(Json(GuideSearchResults {
        query: term.to_owned(),
        count: results.len(),
        results,
    }))
}

/// Read one guide, whole or by section.
#[get("/api/docs/{slug}")]
#[api_doc(
    mcp,
    operation_id = "get_autumn_doc",
    tag = "docs",
    summary = "Read one Autumn guide as Markdown",
    description = "Return a guide's Markdown source by slug, along with its list \
                   of section headings. Guides over 60 KB are not returned whole: \
                   for those, `markdown` is null and you should pick an id from \
                   `sections` and call again with the `section` argument to read \
                   that part. Slugs come from list_autumn_docs or \
                   search_autumn_docs."
)]
pub async fn get_autumn_doc(
    Path(slug): Path<String>,
    Query(query): Query<GetDocQuery>,
) -> Result<Json<GuideDocument>, DocsApiError> {
    let registry = registry()?;
    let page = registry
        .page(&slug)
        .ok_or_else(|| DocsApiError::UnknownGuide(slug.clone()))?;

    let sections = page
        .toc
        .iter()
        .map(|item| GuideSectionRef {
            id: item.id.clone(),
            title: item.title.clone(),
            level: item.level,
        })
        .collect();

    let document = |section, markdown, notice| GuideDocument {
        slug: page.slug.clone(),
        title: page.title.clone(),
        description: page.description.clone(),
        group: site::doc_group_label(&page.slug).to_owned(),
        url: seo::absolute_url(&seo::docs_path(&page.slug)),
        autumn_version: seo::AUTUMN_VERSION.to_owned(),
        sections,
        section,
        markdown,
        notice,
    };

    if let Some(id) = query
        .section
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        let section = page
            .section(id)
            .ok_or_else(|| DocsApiError::UnknownSection {
                slug: page.slug.clone(),
                section: id.to_owned(),
            })?;

        return Ok(Json(document(
            Some(section.id),
            Some(section.markdown),
            None,
        )));
    }

    if page.markdown.len() > MAX_INLINE_DOC_BYTES {
        let notice = format!(
            "This guide is {} KB of Markdown, too large to return in one tool result. \
             Pick an id from `sections` and call get_autumn_doc again with the \
             `section` argument to read that part, or read the whole guide at {}.",
            page.markdown.len() / 1024,
            seo::absolute_url(&seo::docs_path(&page.slug)),
        );

        return Ok(Json(document(None, None, Some(notice))));
    }

    Ok(Json(document(None, Some(page.markdown.clone()), None)))
}

#[must_use]
pub fn api_routes() -> Vec<autumn_web::Route> {
    routes![list_autumn_docs, search_autumn_docs, get_autumn_doc]
}

// ─────────────────────────────────────────────────────────────────────────
// Errors
// ─────────────────────────────────────────────────────────────────────────

/// The bundled guides failed to load at startup, so no docs endpoint can
/// answer. Distinct from [`DocsApiError`] so the handlers that cannot fail any
/// other way say so in their signature.
#[derive(Debug)]
pub struct DocsUnavailable;

impl IntoResponse for DocsUnavailable {
    fn into_response(self) -> Response {
        let detail = crate::site_docs()
            .err()
            .map_or_else(|| "docs unavailable".to_owned(), DocsError::to_string);
        json_error(StatusCode::INTERNAL_SERVER_ERROR, &detail)
    }
}

#[derive(Debug)]
pub enum DocsApiError {
    Unavailable,
    UnknownGuide(String),
    UnknownGroup(String),
    UnknownSection { slug: String, section: String },
}

impl From<DocsUnavailable> for DocsApiError {
    fn from(_: DocsUnavailable) -> Self {
        Self::Unavailable
    }
}

impl IntoResponse for DocsApiError {
    fn into_response(self) -> Response {
        match self {
            Self::Unavailable => DocsUnavailable.into_response(),
            // Both of these are the agent guessing an identifier, so the body
            // names the tool that returns real ones. MCP surfaces a non-2xx as
            // `isError` with the body attached, which is what the agent reads.
            Self::UnknownGuide(slug) => json_error(
                StatusCode::NOT_FOUND,
                &format!(
                    "No guide with slug {slug:?}. Call list_autumn_docs or \
                     search_autumn_docs for valid slugs."
                ),
            ),
            Self::UnknownGroup(group) => json_error(
                StatusCode::NOT_FOUND,
                &format!(
                    "No guide group named {group:?}. Call list_autumn_docs without \
                     a `group` argument; every response lists the valid names."
                ),
            ),
            Self::UnknownSection { slug, section } => json_error(
                StatusCode::NOT_FOUND,
                &format!(
                    "Guide {slug:?} has no section {section:?}. Use an id from this \
                     guide's `sections` list, which you get by calling \
                     get_autumn_doc without a `section` argument."
                ),
            ),
        }
    }
}

fn json_error(status: StatusCode, detail: &str) -> Response {
    (status, Json(serde_json::json!({ "error": detail }))).into_response()
}

// ─────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────

fn registry() -> Result<&'static DocRegistry, DocsUnavailable> {
    crate::site_docs().map_err(|_| DocsUnavailable)
}

fn guide_summary(page: &DocPage) -> GuideSummary {
    GuideSummary {
        group: site::doc_group_label(&page.slug).to_owned(),
        slug: page.slug.clone(),
        title: page.title.clone(),
        description: abridge(&page.description, MAX_INDEX_DESCRIPTION_BYTES),
        bytes: page.markdown.len(),
    }
}

/// Shorten `text` to at most `budget` bytes, cutting at the last word break so
/// the result reads as a clause rather than a severed word, and marking the cut
/// with an ellipsis.
///
/// The byte index is walked back to a UTF-8 boundary first: guide descriptions
/// are full of em dashes and typographic quotes, and slicing one in half panics.
fn abridge(text: &str, budget: usize) -> String {
    if text.len() <= budget {
        return text.to_owned();
    }

    let cut = crate::docs::floor_char_boundary(text, budget);
    let head = text[..cut].trim_end();
    let head = head
        .rsplit_once(char::is_whitespace)
        .map_or(head, |(before, _)| before);

    format!(
        "{}…",
        head.trim_end_matches([',', ';', ':', '—', '-']).trim_end()
    )
}
