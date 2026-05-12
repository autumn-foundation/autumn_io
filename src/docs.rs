use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::LazyLock;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd, html};
use serde::Deserialize;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::html::{IncludeBackground, styled_line_to_highlighted_html};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);
const CODE_THEME: &str = "base16-ocean.dark";

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
}

/// In-page table of contents item generated from Markdown headings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TocItem {
    pub level: u8,
    pub id: String,
    pub title: String,
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
    pub fn from_sources<'a>(
        sources: impl IntoIterator<Item = DocSource<'a>>,
    ) -> Result<Self, DocsError> {
        let mut pages = Vec::new();
        let mut seen_slugs = HashSet::new();

        for source in sources {
            if !seen_slugs.insert(source.slug.to_owned()) {
                return Err(DocsError::DuplicateSlug(source.slug.to_owned()));
            }
            pages.push(parse_doc(source)?);
        }

        pages.sort_by(|left, right| {
            left.order
                .cmp(&right.order)
                .then_with(|| left.title.cmp(&right.title))
        });

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

#[derive(Debug)]
pub enum DocsError {
    MissingFrontmatter {
        slug: String,
    },
    InvalidFrontmatter {
        slug: String,
        source: serde_yaml::Error,
    },
    DuplicateSlug(String),
}

impl Display for DocsError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingFrontmatter { slug } => {
                write!(f, "docs page `{slug}` is missing frontmatter")
            }
            Self::InvalidFrontmatter { slug, source } => {
                write!(f, "docs page `{slug}` has invalid frontmatter: {source}")
            }
            Self::DuplicateSlug(slug) => write!(f, "duplicate docs slug `{slug}`"),
        }
    }
}

impl Error for DocsError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidFrontmatter { source, .. } => Some(source),
            Self::MissingFrontmatter { .. } | Self::DuplicateSlug(_) => None,
        }
    }
}

#[derive(Deserialize)]
struct Frontmatter {
    title: String,
    description: String,
    order: u32,
}

struct RenderedMarkdown {
    html: String,
    toc: Vec<TocItem>,
}

fn parse_doc(source: DocSource<'_>) -> Result<DocPage, DocsError> {
    let normalized = source.markdown.replace("\r\n", "\n");
    let Some(rest) = normalized.strip_prefix("---\n") else {
        return Err(DocsError::MissingFrontmatter {
            slug: source.slug.to_owned(),
        });
    };
    let Some((frontmatter, markdown)) = rest.split_once("\n---\n") else {
        return Err(DocsError::MissingFrontmatter {
            slug: source.slug.to_owned(),
        });
    };

    let frontmatter: Frontmatter = serde_yaml::from_str(frontmatter).map_err(|source_error| {
        DocsError::InvalidFrontmatter {
            slug: source.slug.to_owned(),
            source: source_error,
        }
    })?;
    let markdown = strip_redundant_title_heading(markdown, &frontmatter.title);
    let rendered = render_markdown(&markdown);

    Ok(DocPage {
        slug: source.slug.to_owned(),
        title: frontmatter.title,
        description: frontmatter.description,
        order: frontmatter.order,
        html: rendered.html,
        toc: rendered.toc,
    })
}

fn render_markdown(markdown: &str) -> RenderedMarkdown {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);

    let headings = add_heading_ids(markdown);
    let parser = Parser::new_ext(&headings.markdown, options);
    let mut rendered = String::new();
    html::push_html(
        &mut rendered,
        highlight_code_block_events(parser).into_iter(),
    );

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

fn highlight_code_block_events<'a>(events: impl IntoIterator<Item = Event<'a>>) -> Vec<Event<'a>> {
    let mut highlighted = Vec::new();
    let mut events = events.into_iter();

    while let Some(event) = events.next() {
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                let code = collect_code_block_text(&mut events);
                highlighted.push(Event::Html(render_code_block(&kind, &code).into()));
            }
            event => highlighted.push(event),
        }
    }

    highlighted
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
    let language_label = language.map_or_else(|| "Code".to_owned(), code_language_label);
    let mut output = String::with_capacity(code.len() + 256);

    push_code_block_header(&mut output, &language_label);
    output.push_str("<pre><code");
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
        CodeBlockKind::Fenced(info) => info
            .split_whitespace()
            .next()
            .filter(|lang| !lang.is_empty()),
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
    output.push_str(language);
    output.push_str(r#"</span>"#);
    output.push_str(r#"<button class="copy-code-button" type="button" data-copy-button "#);
    output.push_str(r#"aria-label="Copy code to clipboard" aria-live="polite">Copy</button>"#);
    output.push_str("</div>");
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
}
