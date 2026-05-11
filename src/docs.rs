use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use pulldown_cmark::{Options, Parser, html};
use serde::Deserialize;

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
    let rendered = render_markdown(markdown);

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
    html::push_html(&mut rendered, parser);

    RenderedMarkdown {
        html: add_code_copy_controls(rendered),
        toc: headings.toc,
    }
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

fn add_code_copy_controls(html: String) -> String {
    html.replace(
        "<pre><code",
        r#"<div class="code-block" data-copy-code><button class="copy-code-button" type="button" data-copy-button>Copy</button><pre><code"#,
    )
    .replace("</code></pre>", "</code></pre></div>")
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
