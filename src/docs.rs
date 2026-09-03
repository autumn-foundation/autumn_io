use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::LazyLock;

use autumn_web::markdown::{MarkdownError, MarkdownPage, MarkdownRegistry, MarkdownSource};
use pulldown_cmark::{CodeBlockKind, CowStr, Event, Options, Parser, Tag, TagEnd, html};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::html::{IncludeBackground, styled_line_to_highlighted_html};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);
const CODE_THEME: &str = "base16-ocean.dark";
const AUTUMN_REPOSITORY_URL: &str = "https://github.com/autumn-foundation/autumn";
const AUTUMN_REPOSITORY_BRANCH: &str = "trunk-dev";
const GUIDE_SOURCE_ROOT: [&str; 2] = ["docs", "guide"];

/// A Markdown document bundled into the Autumn website.
#[derive(Clone, Copy, Debug)]
pub struct DocSource<'a> {
    pub slug: &'a str,
    pub markdown: &'a str,
}

impl<'a> DocSource<'a> {
    #[must_use]
    pub const fn new(slug: &'a str, markdown: &'a str) -> Self {
        Self { slug, markdown }
    }
}

/// Rendered docs page with metadata and generated navigation data.
#[derive(Clone, Debug)]
pub struct DocPage {
    pub slug: String,
    pub title: String,
    pub description: String,
    pub order: u32,
    pub html: String,
    pub toc: Vec<TocItem>,
    /// The page's Markdown source, as the renderer and [`DocPage::toc`] saw it:
    /// frontmatter stripped by the framework registry, then the redundant
    /// leading `# Title` removed by [`strip_redundant_title_heading`].
    ///
    /// Kept alongside the rendered `html` so the JSON docs API — and through it
    /// the MCP server — can hand an agent the Markdown an LLM actually wants to
    /// read, rather than syntax-highlighted HTML. Retaining it costs roughly the
    /// size of `content/guide` in resident memory; the rendered HTML already
    /// costs several times that, so this is the cheaper half of the pair.
    ///
    /// Because it is the *same* string [`add_heading_ids`] walks, the section
    /// ids [`DocPage::section`] derives from it match the `#anchor` fragments
    /// the site puts on the rendered page.
    pub markdown: String,
}

/// In-page table of contents item generated from Markdown headings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TocItem {
    pub level: u8,
    pub id: String,
    pub title: String,
}

/// One heading's slice of a guide: the heading line itself plus everything
/// beneath it, up to the next heading at the same or a shallower level.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocSection {
    pub id: String,
    pub level: u8,
    pub title: String,
    pub markdown: String,
    /// The heading line plus whatever prose follows it, up to (not including)
    /// the first heading nested inside this section. Equal to `markdown` when
    /// this section has no nested headings.
    ///
    /// This is what a size-gated API response falls back to instead of
    /// withholding the section outright: narrowing to a nested heading only
    /// ever returns *that* heading onward, so without this, the section's own
    /// introduction — between its heading and the first subheading — could
    /// never be retrieved by any request.
    pub preamble: String,
}

impl DocPage {
    /// Extract the subtree of the heading whose anchor id is `id`, or [`None`]
    /// when the page has no such heading.
    ///
    /// The ids are exactly those in [`DocPage::toc`] — the anchors the rendered
    /// page carries — because both walks run over the same
    /// [`markdown`](DocPage::markdown) with the same fence tracking and
    /// duplicate-id numbering.
    ///
    /// This is what keeps the largest guides usable over MCP: `deployment.md`
    /// is 150 KB of Markdown, far more than an agent wants in one tool result,
    /// but a single section of it is a few kilobytes.
    #[must_use]
    pub fn section(&self, id: &str) -> Option<DocSection> {
        let mut used_ids = HashMap::<String, usize>::new();
        let mut in_fence = false;
        let mut open: Option<(u8, String)> = None;
        let mut markdown = String::new();
        let mut preamble_len: Option<usize> = None;

        for line in self.markdown.lines() {
            let is_fence = line.trim_start().starts_with("```");
            if is_fence {
                in_fence = !in_fence;
            }

            // A `#` only opens a heading outside a fenced block — otherwise a
            // shell comment in a code sample would split the section.
            if !in_fence
                && !is_fence
                && let Some((level, title)) = parse_heading_line(line)
            {
                // Consume an id for every heading in document order, so the
                // duplicate-suffix counters match the ones `add_heading_ids`
                // assigned when it built the toc.
                let heading_id = unique_heading_id(&title, &mut used_ids);
                match &open {
                    Some((open_level, _)) if level <= *open_level => break,
                    // The first heading nested inside the open section: mark
                    // where its preamble ends, before this line is appended.
                    Some(_) if preamble_len.is_none() => preamble_len = Some(markdown.len()),
                    None if heading_id == id => open = Some((level, title)),
                    _ => {}
                }
            }

            if open.is_some() {
                markdown.push_str(line);
                markdown.push('\n');
            }
        }

        let (level, title) = open?;
        markdown.truncate(markdown.trim_end().len());
        let preamble_len = preamble_len.unwrap_or(markdown.len()).min(markdown.len());
        let preamble = markdown[..preamble_len].trim_end().to_owned();

        Some(DocSection {
            id: id.to_owned(),
            level,
            title,
            markdown,
            preamble,
        })
    }

    /// The headings nested inside the heading with this id, in document order,
    /// or an empty slice when it has none (or does not exist).
    ///
    /// Applies the same boundary rule [`DocPage::section`] slices by — every
    /// heading up to the next one at the same or a shallower level — so the
    /// headings listed here are exactly the ones whose text lies within that
    /// section. That is what makes them the narrower requests a caller can
    /// make when a section is itself too large to return.
    #[must_use]
    pub fn subsections(&self, id: &str) -> &[TocItem] {
        let Some(start) = self.toc.iter().position(|item| item.id == id) else {
            return &[];
        };

        let level = self.toc[start].level;
        let nested = &self.toc[start + 1..];
        let end = nested
            .iter()
            .position(|item| item.level <= level)
            .unwrap_or(nested.len());

        &nested[..end]
    }

    /// The page's Markdown up to (not including) its first heading — the
    /// framing prose above `## First Heading` that [`DocPage::toc`] does not
    /// cover. Empty when the page has no such prose, or no headings at all.
    ///
    /// Mirrors [`DocSection::preamble`] one level up: narrowing to a top-level
    /// heading never returns what precedes it, so an oversized guide's own
    /// introduction needs the same fallback the size gate gives a section.
    #[must_use]
    pub fn preamble(&self) -> &str {
        let mut in_fence = false;

        for line in self.markdown.lines() {
            let is_fence = line.trim_start().starts_with("```");
            if !in_fence && !is_fence && parse_heading_line(line).is_some() {
                let offset = line.as_ptr() as usize - self.markdown.as_ptr() as usize;
                return self.markdown[..offset].trim_end();
            }
            if is_fence {
                in_fence = !in_fence;
            }
        }

        self.markdown.trim_end()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DocNeighbors<'a> {
    pub previous: Option<&'a DocPage>,
    pub next: Option<&'a DocPage>,
}

#[derive(Clone, Debug)]
pub struct DocRegistry {
    pages: Vec<DocPage>,
    index_by_slug: HashMap<String, usize>,
}

impl DocRegistry {
    pub fn from_sources(
        sources: impl IntoIterator<Item = DocSource<'static>>,
    ) -> Result<Self, DocsError> {
        // Path-safety validation stays on this side: `MarkdownRegistry` does not
        // guard against slugs that could escape routes or export paths.
        let mut markdown_sources = Vec::new();
        for source in sources {
            if !is_valid_doc_slug(source.slug) {
                return Err(DocsError::InvalidSlug(source.slug.to_owned()));
            }
            markdown_sources.push(MarkdownSource {
                slug: source.slug,
                content: source.markdown,
            });
        }

        // The framework registry owns frontmatter parsing, deduplication, and
        // ordering (by `order`, then `slug`).
        let registry =
            MarkdownRegistry::from_embedded(&markdown_sources).map_err(map_markdown_error)?;

        let pages: Vec<DocPage> = registry
            .all_sorted()
            .into_iter()
            .map(render_doc_page)
            .collect();

        let index_by_slug = pages
            .iter()
            .enumerate()
            .map(|(index, page)| (page.slug.clone(), index))
            .collect();

        Ok(Self {
            pages,
            index_by_slug,
        })
    }

    #[must_use]
    pub fn pages(&self) -> &[DocPage] {
        &self.pages
    }

    #[must_use]
    pub fn page(&self, slug: &str) -> Option<&DocPage> {
        self.index_by_slug
            .get(slug)
            .and_then(|index| self.pages.get(*index))
    }

    #[must_use]
    pub fn neighbors(&self, slug: &str) -> DocNeighbors<'_> {
        let Some(index) = self.index_by_slug.get(slug).copied() else {
            return DocNeighbors {
                previous: None,
                next: None,
            };
        };

        DocNeighbors {
            previous: index.checked_sub(1).and_then(|i| self.pages.get(i)),
            next: self.pages.get(index + 1),
        }
    }
}

/// A single search result over the bundled guide pages.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchHit {
    pub slug: String,
    pub title: String,
    pub snippet: String,
}

/// Case-insensitive in-memory search index over the rendered guide pages.
///
/// Built once at startup from a [`DocRegistry`]; the bundled content only
/// changes on deploy, so a plain tokenized substring match over the embedded
/// docs needs no database or external index.
#[derive(Clone, Debug)]
pub struct SearchIndex {
    entries: Vec<SearchEntry>,
}

/// Relative weights for where a query token matches within a page. Title
/// matches rank above heading matches, which rank above body matches.
const TITLE_MATCH_WEIGHT: u32 = 8;
const HEADING_MATCH_WEIGHT: u32 = 4;
const BODY_MATCH_WEIGHT: u32 = 1;

/// Number of characters of context shown on either side of a snippet match.
const SNIPPET_RADIUS: usize = 90;

impl SearchIndex {
    /// Build a search index over every page in `registry`.
    #[must_use]
    pub fn from_registry(registry: &DocRegistry) -> Self {
        let entries = registry
            .pages()
            .iter()
            .map(SearchEntry::from_page)
            .collect();
        Self { entries }
    }

    /// Return up to `limit` pages matching every whitespace-separated token in
    /// `query`, ranked by weighted match score (title, then heading, then body).
    #[must_use]
    pub fn search(&self, query: &str, limit: usize) -> Vec<SearchHit> {
        let tokens: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        if tokens.is_empty() {
            return Vec::new();
        }

        let mut scored: Vec<(u32, &SearchEntry)> = self
            .entries
            .iter()
            .filter_map(|entry| entry.score(&tokens).map(|score| (score, entry)))
            .collect();

        // Highest score first; ties broken by title so ordering stays stable.
        scored.sort_by(|(left_score, left), (right_score, right)| {
            right_score
                .cmp(left_score)
                .then_with(|| left.title.cmp(&right.title))
        });

        scored
            .into_iter()
            .take(limit)
            .map(|(_, entry)| entry.to_hit(&tokens))
            .collect()
    }
}

#[derive(Clone, Debug)]
struct SearchEntry {
    slug: String,
    title: String,
    description: String,
    title_lower: String,
    headings_lower: String,
    text: String,
    text_lower: String,
}

impl SearchEntry {
    fn from_page(page: &DocPage) -> Self {
        let headings = page
            .toc
            .iter()
            .map(|item| item.title.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let text = html_to_plain_text(&page.html);

        Self {
            slug: page.slug.clone(),
            title: page.title.clone(),
            description: page.description.clone(),
            title_lower: page.title.to_lowercase(),
            headings_lower: headings.to_lowercase(),
            text_lower: text.to_lowercase(),
            text,
        }
    }

    /// Sum the match weights for every token, or return [`None`] when any token
    /// is missing from the page (all tokens must match for a page to appear).
    fn score(&self, tokens: &[String]) -> Option<u32> {
        let mut total = 0;
        for token in tokens {
            let mut token_score = 0;
            if self.title_lower.contains(token) {
                token_score += TITLE_MATCH_WEIGHT;
            }
            if self.headings_lower.contains(token) {
                token_score += HEADING_MATCH_WEIGHT;
            }
            if self.text_lower.contains(token) {
                token_score += BODY_MATCH_WEIGHT;
            }
            if token_score == 0 {
                return None;
            }
            total += token_score;
        }
        Some(total)
    }

    fn to_hit(&self, tokens: &[String]) -> SearchHit {
        SearchHit {
            slug: self.slug.clone(),
            title: self.title.clone(),
            snippet: self.snippet(tokens),
        }
    }

    /// Build a context snippet around the earliest body match, falling back to
    /// the page description when the query only matched the title or headings.
    fn snippet(&self, tokens: &[String]) -> String {
        match tokens
            .iter()
            .filter_map(|token| self.text_lower.find(token))
            .min()
        {
            Some(index) => build_snippet(&self.text, index, SNIPPET_RADIUS),
            None => self.description.clone(),
        }
    }
}

/// Strip HTML tags and decode the handful of entities the renderer emits,
/// collapsing runs of whitespace so matching and snippets stay clean.
fn html_to_plain_text(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut pending_space = false;

    for char in html.chars() {
        match char {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                pending_space = true;
            }
            _ if in_tag => {}
            char if char.is_whitespace() => pending_space = true,
            char => {
                if pending_space && !text.is_empty() {
                    text.push(' ');
                }
                pending_space = false;
                text.push(char);
            }
        }
    }

    decode_html_entities(&text)
}

fn decode_html_entities(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        // `&amp;` is decoded last so an escaped entity is only decoded once.
        .replace("&amp;", "&")
}

/// Rounds `index` down to the nearest UTF-8 char boundary, clamped to
/// `[0, s.len()]`. Mirrors the unstable `str::floor_char_boundary` using only
/// stable APIs so the crate compiles on stable Rust.
pub(crate) fn floor_char_boundary(s: &str, index: usize) -> usize {
    let mut i = index.min(s.len());
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Rounds `index` up to the nearest UTF-8 char boundary, clamped to
/// `[0, s.len()]`. Mirrors the unstable `str::ceil_char_boundary` using only
/// stable APIs so the crate compiles on stable Rust.
fn ceil_char_boundary(s: &str, index: usize) -> usize {
    let mut i = index.min(s.len());
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

fn build_snippet(text: &str, match_index: usize, radius: usize) -> String {
    let start = floor_char_boundary(text, match_index.saturating_sub(radius));
    let end = ceil_char_boundary(text, match_index.saturating_add(radius));

    let mut snippet = String::new();
    if start > 0 {
        snippet.push('…');
    }
    snippet.push_str(text[start..end].trim());
    if end < text.len() {
        snippet.push('…');
    }
    snippet
}

#[derive(Debug)]
pub enum DocsError {
    MissingFrontmatter { slug: String },
    InvalidFrontmatter { slug: String, message: String },
    InvalidSlug(String),
    DuplicateSlug(String),
}

impl Display for DocsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingFrontmatter { slug } => {
                write!(f, "docs page `{slug}` is missing frontmatter")
            }
            Self::InvalidFrontmatter { slug, message } => {
                write!(f, "docs page `{slug}` has invalid frontmatter: {message}")
            }
            Self::InvalidSlug(slug) => write!(f, "docs page slug `{slug}` is not safe"),
            Self::DuplicateSlug(slug) => write!(f, "duplicate docs slug `{slug}`"),
        }
    }
}

impl Error for DocsError {}

/// Translate a framework [`MarkdownError`] into this site's [`DocsError`].
fn map_markdown_error(error: MarkdownError) -> DocsError {
    match error {
        MarkdownError::FrontmatterMissing { slug } => DocsError::MissingFrontmatter { slug },
        MarkdownError::FrontmatterInvalid { slug, source } => DocsError::InvalidFrontmatter {
            slug,
            message: source.to_string(),
        },
        MarkdownError::DuplicateSlug { slug } => DocsError::DuplicateSlug(slug),
        // `Io` and `InvalidFileName` only arise from `MarkdownRegistry::from_dir`,
        // which this site never calls; map defensively so the enum stays covered.
        other => DocsError::InvalidFrontmatter {
            slug: String::new(),
            message: other.to_string(),
        },
    }
}

struct RenderedMarkdown {
    html: String,
    toc: Vec<TocItem>,
}

/// Render a framework-parsed [`MarkdownPage`] into a site [`DocPage`], keeping
/// the syntect highlighting, link-rewriting, and redundant-title-stripping
/// pipeline that the framework renderer does not provide.
fn render_doc_page(page: &MarkdownPage) -> DocPage {
    let title = page.frontmatter.title.clone();
    let markdown = strip_redundant_title_heading(&page.body, &title);
    let rendered = render_markdown(&markdown);

    DocPage {
        slug: page.slug.clone(),
        title,
        description: page.frontmatter.description.clone(),
        order: page.frontmatter.order,
        html: rendered.html,
        toc: rendered.toc,
        markdown,
    }
}

fn is_valid_doc_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.split('-').all(|segment| !segment.is_empty())
        && slug
            .chars()
            .all(|char| char.is_ascii_lowercase() || char.is_ascii_digit() || char == '-')
}

fn render_markdown(markdown: &str) -> RenderedMarkdown {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);

    let headings = add_heading_ids(markdown);
    let parser = Parser::new_ext(&headings.markdown, options);
    let mut rendered = String::new();
    html::push_html(&mut rendered, render_markdown_events(parser).into_iter());

    // `.article-body table` scrolls horizontally (`overflow-x: auto`) on wide
    // tables but pulldown-cmark's `<table>` has no focusable content, so
    // without tabindex="0" a keyboard-only reader can never reach or scroll it
    // (axe: scrollable-region-focusable). pulldown-cmark always writes the
    // bare literal `<table>` for `Tag::Table`, so this replace is exact.
    let rendered = rendered.replace("<table>", r#"<table tabindex="0">"#);

    RenderedMarkdown {
        html: rendered,
        toc: headings.toc,
    }
}

fn strip_redundant_title_heading(markdown: &str, title: &str) -> String {
    let mut output = String::with_capacity(markdown.len());
    let mut checked_first_content_line = false;

    for line in markdown.lines() {
        if !checked_first_content_line {
            if line.trim().is_empty() {
                output.push_str(line);
                output.push('\n');
                continue;
            }

            checked_first_content_line = true;
            if let Some((1, heading_title)) = parse_heading_line(line)
                && heading_title == title
            {
                continue;
            }
        }

        output.push_str(line);
        output.push('\n');
    }

    output
}

struct MarkdownWithHeadings {
    markdown: String,
    toc: Vec<TocItem>,
}

fn add_heading_ids(markdown: &str) -> MarkdownWithHeadings {
    let mut output = String::with_capacity(markdown.len());
    let mut toc = Vec::new();
    let mut used_ids = HashMap::<String, usize>::new();
    let mut in_fence = false;

    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            output.push_str(line);
            output.push('\n');
            continue;
        }

        if !in_fence && let Some((level, title)) = parse_heading_line(line) {
            let id = unique_heading_id(&title, &mut used_ids);
            toc.push(TocItem {
                level,
                id: id.clone(),
                title: title.clone(),
            });
            output.push_str(&"#".repeat(level.into()));
            output.push(' ');
            output.push_str(&title);
            output.push_str(" {#");
            output.push_str(&id);
            output.push_str("}\n");
            continue;
        }

        output.push_str(line);
        output.push('\n');
    }

    MarkdownWithHeadings {
        markdown: output,
        toc,
    }
}

fn parse_heading_line(line: &str) -> Option<(u8, String)> {
    let trimmed = line.trim_start();
    let level = trimmed.chars().take_while(|char| *char == '#').count();
    if !(1..=6).contains(&level) {
        return None;
    }

    let after_hashes = trimmed.get(level..)?;
    if !after_hashes.starts_with(' ') {
        return None;
    }

    let title = after_hashes.trim().trim_end_matches('#').trim().to_owned();
    if title.is_empty() {
        return None;
    }

    Some((level as u8, title))
}

fn unique_heading_id(title: &str, used_ids: &mut HashMap<String, usize>) -> String {
    let base = slugify_heading(title);
    let count = used_ids.entry(base.clone()).or_insert(0);
    *count += 1;

    if *count == 1 {
        base
    } else {
        format!("{base}-{count}")
    }
}

#[must_use]
pub fn slugify_heading(heading: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;

    for char in heading.chars().flat_map(char::to_lowercase) {
        if char.is_ascii_alphanumeric() {
            slug.push(char);
            last_was_dash = false;
        } else if !last_was_dash && !slug.is_empty() {
            slug.push('-');
            last_was_dash = true;
        }
    }

    while slug.ends_with('-') {
        slug.pop();
    }

    if slug.is_empty() {
        "section".to_owned()
    } else {
        slug
    }
}

fn render_markdown_events<'a>(events: impl IntoIterator<Item = Event<'a>>) -> Vec<Event<'a>> {
    let mut rendered = Vec::new();
    let mut events = events.into_iter();

    while let Some(event) = events.next() {
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                let code = collect_code_block_text(&mut events);
                rendered.push(Event::Html(render_code_block(&kind, &code).into()));
            }
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                let dest_url = rewrite_link_destination(dest_url.as_ref())
                    .map(CowStr::from)
                    .unwrap_or(dest_url);
                rendered.push(Event::Start(Tag::Link {
                    link_type,
                    dest_url,
                    title,
                    id,
                }));
            }
            Event::Html(html) | Event::InlineHtml(html) => {
                rendered.push(Event::Html(html_escaped_text(html.as_ref()).into()));
            }
            event => rendered.push(event),
        }
    }

    rendered
}

fn rewrite_link_destination(destination: &str) -> Option<String> {
    if destination.is_empty() || is_external_or_special_link(destination) {
        return None;
    }

    let (path, fragment) = split_link_fragment(destination);
    if path.is_empty() || path.starts_with('/') {
        return None;
    }

    let path = path.replace('\\', "/");
    let fragment = fragment.unwrap_or_default();
    if let Some(slug) = docs_route_slug(&path) {
        return Some(format!("/docs/{slug}{fragment}"));
    }

    if should_link_to_upstream_source(&path) {
        let path = normalize_upstream_path(&path);
        let mode = upstream_link_mode(&path);
        return Some(format!(
            "{AUTUMN_REPOSITORY_URL}/{mode}/{AUTUMN_REPOSITORY_BRANCH}/{path}{fragment}"
        ));
    }

    None
}

fn is_external_or_special_link(destination: &str) -> bool {
    let lower = destination.to_ascii_lowercase();
    destination.starts_with('#')
        || lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mailto:")
        || lower.starts_with("tel:")
}

fn split_link_fragment(destination: &str) -> (&str, Option<&str>) {
    destination
        .split_once('#')
        .map_or((destination, None), |(path, fragment)| {
            (
                path,
                Some(destination.get(path.len()..).unwrap_or(fragment)),
            )
        })
}

fn docs_route_slug(path: &str) -> Option<&str> {
    let relative_path = path.strip_prefix("./").unwrap_or(path);
    let relative_path = relative_path
        .strip_prefix("docs/guide/")
        .unwrap_or(relative_path);
    let slug = relative_path.strip_suffix(".md")?;

    if slug.is_empty() || slug.contains('/') {
        None
    } else {
        Some(slug)
    }
}

fn should_link_to_upstream_source(path: &str) -> bool {
    let relative_path = path.strip_prefix("./").unwrap_or(path);
    relative_path.starts_with("../") || relative_path.contains('/')
}

fn normalize_upstream_path(path: &str) -> String {
    let mut segments = GUIDE_SOURCE_ROOT.to_vec();

    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            segment => segments.push(segment),
        }
    }

    segments.join("/")
}

fn upstream_link_mode(path: &str) -> &str {
    let Some(last_segment) = path.rsplit('/').next() else {
        return "tree";
    };

    if last_segment.contains('.') {
        "blob"
    } else {
        "tree"
    }
}

fn collect_code_block_text<'a>(events: &mut impl Iterator<Item = Event<'a>>) -> String {
    let mut code = String::new();

    for event in events.by_ref() {
        match event {
            Event::End(TagEnd::CodeBlock) => break,
            Event::Text(text) | Event::Code(text) | Event::Html(text) | Event::InlineHtml(text) => {
                code.push_str(&text);
            }
            Event::SoftBreak | Event::HardBreak => code.push('\n'),
            _ => {}
        }
    }

    code
}

fn render_code_block(kind: &CodeBlockKind<'_>, code: &str) -> String {
    render_highlighted_code_block(code_block_language(kind), code)
}

pub(crate) fn render_highlighted_code_block(language: Option<&str>, code: &str) -> String {
    let language = language.and_then(normalize_code_language);
    let language_label = language.map_or_else(|| "Code".to_owned(), code_language_label);
    let mut output = String::with_capacity(code.len() + 256);

    push_code_block_header(&mut output, &language_label);
    // Code samples scroll horizontally (`.code-block pre { overflow-x: auto }`)
    // but have no focusable content of their own, so without tabindex="0" a
    // keyboard-only reader can never reach or scroll them (axe: scrollable-region-focusable).
    output.push_str(r#"<pre tabindex="0"><code"#);
    if let Some(language) = language {
        output.push_str(r#" class="language-"#);
        push_html_attr_escaped(&mut output, language);
        output.push('"');
    }
    output.push('>');
    output.push_str(&highlight_code(language, code));
    output.push_str("</code></pre></div>");
    output
}

fn code_block_language<'a>(kind: &'a CodeBlockKind<'a>) -> Option<&'a str> {
    match kind {
        CodeBlockKind::Indented => None,
        CodeBlockKind::Fenced(info) => normalize_code_language(info),
    }
}

fn normalize_code_language(info: &str) -> Option<&str> {
    let token = info.split_whitespace().next()?;
    let language = token
        .trim_start_matches('{')
        .trim_start_matches('.')
        .split([',', '}'])
        .next()?
        .trim_start_matches('.');

    if language.is_empty() {
        None
    } else {
        Some(language)
    }
}

fn highlight_code(language: Option<&str>, code: &str) -> String {
    let syntax = language
        .and_then(syntax_for_language)
        .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text());
    let mut highlighter = HighlightLines::new(syntax, code_theme());
    let mut output = String::with_capacity(code.len() * 2);

    for line in LinesWithEndings::from(code) {
        let Ok(ranges) = highlighter.highlight_line(line, &SYNTAX_SET) else {
            push_html_escaped(&mut output, line);
            continue;
        };
        match styled_line_to_highlighted_html(&ranges, IncludeBackground::No) {
            Ok(line_html) => output.push_str(&line_html),
            Err(_) => push_html_escaped(&mut output, line),
        }
    }

    output
}

fn syntax_for_language(language: &str) -> Option<&'static SyntaxReference> {
    let lowercase = language.to_ascii_lowercase();
    let token = match lowercase.as_str() {
        "bash" | "shell" => "sh",
        "dockerfile" => "Dockerfile",
        "powershell" => "ps1",
        "rust" => "rs",
        "text" | "txt" => "txt",
        other => other,
    };

    SYNTAX_SET
        .find_syntax_by_token(token)
        .or_else(|| SYNTAX_SET.find_syntax_by_extension(token))
}

fn code_theme() -> &'static Theme {
    THEME_SET
        .themes
        .get(CODE_THEME)
        .or_else(|| THEME_SET.themes.values().next())
        .expect("syntect ships at least one default theme")
}

fn push_code_block_header(output: &mut String, language: &str) {
    output.push_str(r#"<div class="code-block" data-copy-code>"#);
    output.push_str(r#"<div class="code-block-header">"#);
    output.push_str(r#"<span class="code-window-dots" aria-hidden="true">"#);
    output.push_str(r#"<span class="code-window-dot"></span>"#);
    output.push_str(r#"<span class="code-window-dot"></span>"#);
    output.push_str(r#"<span class="code-window-dot"></span>"#);
    output.push_str(r#"</span><span class="code-language">"#);
    push_html_escaped(output, language);
    output.push_str(r#"</span>"#);
    output.push_str(r#"<button class="copy-code-button" type="button" data-copy-button "#);
    output.push_str(r#"aria-label="Copy code to clipboard" aria-live="polite">Copy</button>"#);
    output.push_str("</div>");
}

fn html_escaped_text(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    push_html_escaped(&mut escaped, value);
    escaped
}

fn code_language_label(language: &str) -> String {
    match language.to_ascii_lowercase().as_str() {
        "bash" | "sh" | "shell" => "Shell".to_owned(),
        "css" => "CSS".to_owned(),
        "dockerfile" => "Dockerfile".to_owned(),
        "powershell" | "ps1" => "PowerShell".to_owned(),
        "rust" | "rs" => "Rust".to_owned(),
        "text" | "txt" => "Text".to_owned(),
        "toml" => "TOML".to_owned(),
        other => title_case_language(other),
    }
}

fn title_case_language(language: &str) -> String {
    let mut label = String::with_capacity(language.len());
    let mut uppercase_next = true;

    for char in language.chars() {
        if char == '-' || char == '_' {
            label.push(' ');
            uppercase_next = true;
        } else if uppercase_next {
            label.extend(char.to_uppercase());
            uppercase_next = false;
        } else {
            label.push(char);
        }
    }

    if label.is_empty() {
        "Code".to_owned()
    } else {
        label
    }
}

fn push_html_attr_escaped(output: &mut String, value: &str) {
    push_html_escaped(output, value);
}

fn push_html_escaped(output: &mut String, value: &str) {
    for char in value.chars() {
        match char {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#39;"),
            _ => output.push(char),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_heading_ids_get_stable_suffixes() {
        let rendered = render_markdown("# Install\n\n## Install\n");

        assert_eq!(rendered.toc[0].id, "install");
        assert_eq!(rendered.toc[1].id, "install-2");
    }

    #[test]
    fn rustdoc_fence_modifiers_render_as_rust_code_blocks() {
        let rendered = render_markdown(
            "```rust,ignore\nuse autumn_web::prelude::*;\n```\n\n```rust,no_run\nfn main() {}\n```\n",
        );

        assert_eq!(rendered.html.matches(r#"class="language-rust""#).count(), 2);
        assert_eq!(
            rendered
                .html
                .matches(r#"<span class="code-language">Rust</span>"#)
                .count(),
            2
        );
        assert!(rendered.html.contains("<span style=\"color:"));
        assert!(!rendered.html.contains("language-rust,ignore"));
        assert!(!rendered.html.contains("language-rust,no_run"));
        assert!(!rendered.html.contains("Rust,ignore"));
        assert!(!rendered.html.contains("Rust,no Run"));
    }

    #[test]
    fn code_block_language_labels_are_html_escaped() {
        let rendered = render_highlighted_code_block(Some("evil<script>"), "hello");

        assert!(rendered.contains(r#"<span class="code-language">Evil&lt;script&gt;</span>"#));
        assert!(!rendered.contains(r#"<span class="code-language">Evil<script></span>"#));
    }

    fn sample_registry() -> DocRegistry {
        DocRegistry::from_sources([
            DocSource::new(
                "widgets",
                "+++\ntitle = \"Widget Guide\"\ndescription = \"Working with widgets\"\norder = 1\n+++\n\n# Widget Guide\n\n## Zebra handling\n\nThe widget guide explains zebra handling in production.\n",
            ),
            DocSource::new(
                "jobs",
                "+++\ntitle = \"Background Jobs\"\ndescription = \"Queue and run work\"\norder = 2\n+++\n\n# Background Jobs\n\nJobs discuss giraffes and queues at length.\n",
            ),
        ])
        .expect("sample registry builds")
    }

    #[test]
    fn search_finds_page_by_body_term() {
        let index = SearchIndex::from_registry(&sample_registry());

        let hits = index.search("giraffes", 20);

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].slug, "jobs");
        assert!(hits[0].snippet.contains("giraffes"));
    }

    #[test]
    fn search_ranks_title_matches_above_body_matches() {
        let index = SearchIndex::from_registry(&sample_registry());

        // "widget" appears in the "Widget Guide" title and inside the jobs body
        // only via unrelated words, so the titled page must come first.
        let hits = index.search("widget", 20);

        assert_eq!(hits.first().map(|hit| hit.slug.as_str()), Some("widgets"));
    }

    #[test]
    fn search_requires_every_token_to_match() {
        let index = SearchIndex::from_registry(&sample_registry());

        assert!(index.search("zebra giraffes", 20).is_empty());
        assert_eq!(index.search("zebra handling", 20).len(), 1);
    }

    #[test]
    fn empty_query_returns_no_hits() {
        let index = SearchIndex::from_registry(&sample_registry());

        assert!(index.search("", 20).is_empty());
        assert!(index.search("   ", 20).is_empty());
    }

    #[test]
    fn html_to_plain_text_strips_tags_and_decodes_entities() {
        let text = html_to_plain_text("<p>Tom &amp; Jerry &lt;code&gt;</p>");

        assert_eq!(text, "Tom & Jerry <code>");
    }

    #[test]
    fn char_boundary_helpers_round_within_multibyte_string() {
        // "café☕" — 'é' is 2 bytes (indices 3..5) and '☕' is 3 bytes
        // (indices 5..8), so several byte indices fall mid-character.
        let text = "café☕";
        assert!(!text.is_char_boundary(4)); // middle of 'é'
        assert!(!text.is_char_boundary(6)); // middle of '☕'

        // floor rounds down to the nearest boundary, ceil rounds up.
        assert_eq!(floor_char_boundary(text, 4), 3);
        assert_eq!(ceil_char_boundary(text, 4), 5);
        assert_eq!(floor_char_boundary(text, 6), 5);
        assert_eq!(ceil_char_boundary(text, 6), 8);

        // Already-on-boundary indices are returned unchanged.
        assert_eq!(floor_char_boundary(text, 3), 3);
        assert_eq!(ceil_char_boundary(text, 5), 5);

        // Out-of-range indices clamp to the string length.
        assert_eq!(floor_char_boundary(text, 999), text.len());
        assert_eq!(ceil_char_boundary(text, 999), text.len());
    }

    #[test]
    fn build_snippet_never_splits_multibyte_chars() {
        // A match whose radius window lands inside multi-byte characters must
        // still yield a valid UTF-8 slice (no panic) rather than slicing mid-char.
        let text = "aéé☕bcdéé☕z";
        let match_index = text.find('b').expect("match present");

        // Small radius forces the window edges onto non-boundary byte offsets.
        let snippet = build_snippet(text, match_index, 3);

        assert!(snippet.contains('b'));
    }
}
