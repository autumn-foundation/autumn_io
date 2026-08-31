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
//! Markdown only while it fits [`MAX_INLINE_DOC_BYTES`]; past that it withholds
//! the body and returns the headings nested inside what was asked for, letting
//! the agent narrow and ask again. Section ids are the anchors the rendered
//! page already uses, so a section reference is also a working deep link a
//! human can open.
//!
//! The gate applies to a requested *section* just as it does to a whole guide.
//! A `##` section of the deployment guide is 76 KB on its own, so exempting the
//! section path would have reopened the hole on the very call the notice tells
//! an agent to make. See [`gate_by_size`].

use autumn_web::openapi::OpenApiSchema;
use autumn_web::prelude::*;
use autumn_web::reexports::axum::response::{IntoResponse, Response};
use autumn_web::reexports::http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::docs::{DocPage, DocRegistry, DocsError, TocItem};
use crate::{seo, site};

/// Largest guide, in bytes of Markdown, that [`get_autumn_doc`] will return in
/// one piece.
///
/// Three whole guides and two individual sections sit above this line. For
/// those, returning the full body would dominate an agent's context window, so
/// the response carries the headings to narrow to and a
/// [`GuideDocument::notice`] instead — a cheap extra round-trip in exchange for
/// never blowing up the caller.
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
    /// The headings the caller can narrow to from here, in document order:
    /// every heading in the guide, or — when a `section` was requested — the
    /// headings nested inside that section.
    pub sections: Vec<GuideSectionRef>,
    /// The requested section's id, when the response is one section rather
    /// than the whole guide.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    /// The Markdown itself, `null` when it was too large to inline and there
    /// are `sections` to narrow to instead, or a truncated prefix when it was
    /// too large with nothing nested inside it. Whenever it is not the
    /// complete text, [`notice`](GuideDocument::notice) says so.
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
    description = "Return a guide's Markdown source by slug, along with its \
                   section headings. Anything over 60 KB is not returned whole: \
                   `markdown` comes back null, and you should pick an id from \
                   `sections` and call again with the `section` argument to read \
                   that part. A requested section is subject to the same limit, \
                   and `sections` then lists the headings inside it, so keep \
                   narrowing until you get a body. Slugs come from \
                   list_autumn_docs or search_autumn_docs."
)]
pub async fn get_autumn_doc(
    Path(slug): Path<String>,
    Query(query): Query<GetDocQuery>,
) -> Result<Json<GuideDocument>, DocsApiError> {
    let registry = registry()?;
    let page = registry
        .page(&slug)
        .ok_or_else(|| DocsApiError::UnknownGuide(slug.clone()))?;

    let requested = query
        .section
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty());

    // Resolve what was asked for — the whole guide, or one section of it —
    // and the headings the caller can narrow to from there. Both go through
    // the same size gate below: a `##` section of a 150 KB guide can itself be
    // 75 KB, which is the result the gate exists to prevent, so exempting the
    // section path would reopen the hole on the very call the notice tells an
    // agent to make.
    let (markdown, sections) = match requested {
        Some(id) => {
            let section = page
                .section(id)
                .ok_or_else(|| DocsApiError::UnknownSection {
                    slug: page.slug.clone(),
                    section: id.to_owned(),
                })?;

            (section.markdown, page.subsections(id))
        }
        None => (page.markdown.clone(), page.toc.as_slice()),
    };

    let url = seo::absolute_url(&seo::docs_path(&page.slug));
    let (markdown, notice) = gate_by_size(markdown, sections, requested, &url);

    Ok(Json(GuideDocument {
        slug: page.slug.clone(),
        title: page.title.clone(),
        description: page.description.clone(),
        group: site::doc_group_label(&page.slug).to_owned(),
        url,
        autumn_version: seo::AUTUMN_VERSION.to_owned(),
        sections: sections
            .iter()
            .map(|item| GuideSectionRef {
                id: item.id.clone(),
                title: item.title.clone(),
                level: item.level,
            })
            .collect(),
        section: requested.map(str::to_owned),
        markdown,
        notice,
    }))
}

/// Decide whether `markdown` can be returned whole, and what to say if not.
///
/// Three outcomes, in the order a caller can act on them:
///
/// 1. **It fits.** Return it.
/// 2. **Too large, but `sections` offers somewhere narrower.** Withhold the
///    body and point at those headings. This is the common case, and it
///    recurses safely: each narrowing is strictly smaller than the last.
/// 3. **Too large with nothing nested inside it.** There is no narrower request
///    left to make, so returning nothing would strand the caller — truncate
///    instead and say so. No guide hits this today; it is the floor that keeps
///    the recursion in (2) from ever bottoming out at a dead end, since guide
///    content is synced from upstream and can grow a large leaf section at any
///    time.
fn gate_by_size(
    markdown: String,
    sections: &[TocItem],
    requested: Option<&str>,
    url: &str,
) -> (Option<String>, Option<String>) {
    if markdown.len() <= MAX_INLINE_DOC_BYTES {
        return (Some(markdown), None);
    }

    let subject = match requested {
        Some(id) => format!("Section {id:?}"),
        None => "This guide".to_owned(),
    };
    let kilobytes = markdown.len() / 1024;

    if sections.is_empty() {
        let cut = crate::docs::floor_char_boundary(&markdown, MAX_INLINE_DOC_BYTES);
        // Cut at a line break so the result is not a half-line of Markdown.
        let cut = markdown[..cut].rfind('\n').map_or(cut, |line| line + 1);
        let mut truncated = markdown;
        truncated.truncate(cut);

        return (
            Some(truncated),
            Some(format!(
                "{subject} is {kilobytes} KB of Markdown and has no headings inside \
                 it to narrow to, so this is the first {} KB only. Read the rest at \
                 {url}.",
                cut / 1024,
            )),
        );
    }

    (
        None,
        Some(format!(
            "{subject} is {kilobytes} KB of Markdown, too large to return in one tool \
             result. Pick an id from `sections` and call get_autumn_doc again with the \
             `section` argument to read that part, or read it at {url}."
        )),
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    fn heading(id: &str, level: u8) -> TocItem {
        TocItem {
            level,
            id: id.to_owned(),
            title: id.to_owned(),
        }
    }

    fn oversized(lines: usize) -> String {
        // Each line is 100 bytes, so `lines` controls the size directly.
        "x".repeat(99)
            .lines()
            .cycle()
            .take(lines)
            .map(|line| format!("{line}\n"))
            .collect()
    }

    #[test]
    fn a_body_within_the_cap_is_returned_untouched() {
        let body = "## Small\n\nJust a little text.".to_owned();

        let (markdown, notice) = gate_by_size(body.clone(), &[], None, "https://example.test");

        assert_eq!(markdown, Some(body));
        assert_eq!(notice, None);
    }

    #[test]
    fn an_oversized_body_with_headings_is_withheld_in_favour_of_narrowing() {
        let sections = [heading("first", 3), heading("second", 3)];

        let (markdown, notice) = gate_by_size(
            oversized(1_000),
            &sections,
            Some("outer"),
            "https://example.test/docs/guide",
        );

        assert_eq!(markdown, None, "the body should be withheld");
        let notice = notice.expect("a notice explaining why");
        assert!(
            notice.contains("\"outer\""),
            "name what was too large: {notice}"
        );
        assert!(
            notice.contains("`sections`"),
            "point at the way out: {notice}"
        );
    }

    /// The floor under the narrowing recursion: a heading with nothing nested
    /// inside it has no narrower request left, so returning nothing would
    /// strand the caller. No guide hits this today — upstream content can grow
    /// into it at any time.
    #[test]
    fn an_oversized_body_with_no_headings_is_truncated_rather_than_withheld() {
        let (markdown, notice) = gate_by_size(
            oversized(1_000),
            &[],
            Some("leaf"),
            "https://example.test/docs/guide",
        );

        let markdown = markdown.expect("a truncated body, not nothing");
        assert!(markdown.len() <= MAX_INLINE_DOC_BYTES);
        assert!(
            markdown.ends_with('\n'),
            "the cut should land on a line break, not mid-line"
        );

        let notice = notice.expect("a notice saying it was cut");
        assert!(notice.contains("https://example.test/docs/guide"));
    }

    #[test]
    fn abridging_cuts_at_a_word_break_without_splitting_a_character() {
        // Every one of these is a real guide description shape: an em dash, a
        // multibyte character straddling the budget, and a run with no spaces.
        assert_eq!(abridge("short enough", 64), "short enough");
        assert_eq!(abridge("one two three four", 11), "one two…");
        assert_eq!(abridge("alpha — beta gamma", 9), "alpha…");

        let multibyte = "café☕ ".repeat(40);
        for budget in 1..40 {
            // The only requirement is that it does not panic on a byte index
            // that falls inside a character.
            let _ = abridge(&multibyte, budget);
        }

        // With no word break to cut at, keep the byte-cut prefix: a clipped
        // word still tells the reader more than a bare ellipsis.
        assert_eq!(abridge("supercalifragilistic", 8), "supercal…");
    }
}
