//! TOML frontmatter parsing for the bundled guides.
//!
//! This is the site's own copy of what `autumn_web::markdown::MarkdownRegistry`
//! did for us until the edge lane arrived, and it exists for exactly one
//! reason: an `#[edge]` handler module compiles for `wasm32-wasip1`, where
//! `autumn-web` is not in the dependency graph at all (see
//! `content/guide/edge.md` § "The edge-safe module rule"). The guide registry
//! is on the read path both lanes serve, so its frontmatter parser had to come
//! with it.
//!
//! It is a deliberate line-for-line port, not a reimplementation. The framework
//! parser's exact behaviour is load bearing — the `+++` delimiter handling, the
//! CRLF normalisation, the `trim_start` on the body, and the `(order, slug)`
//! sort all decide what the rendered site looks like — and
//! `tests/port_parity.rs` pins this module against the framework's on the
//! native target, where both are in the graph, so a divergence is a test
//! failure rather than a silently different page.

use serde::Deserialize;

/// Parsed frontmatter from a guide.
#[derive(Clone, Debug, Deserialize)]
pub struct Frontmatter {
    /// Display title of the page.
    pub title: String,
    /// Short description used in listings and meta tags.
    #[serde(default)]
    pub description: String,
    /// Sort order for navigation listings (lower numbers appear first).
    #[serde(default)]
    pub order: u32,
}

/// A parsed guide: frontmatter plus the Markdown body it preceded.
#[derive(Clone, Debug)]
pub struct ParsedPage {
    /// URL-safe identifier, e.g. `"getting-started"`.
    pub slug: String,
    /// Parsed frontmatter metadata.
    pub frontmatter: Frontmatter,
    /// Raw Markdown body, frontmatter stripped.
    pub body: String,
}

/// Why a guide could not be parsed.
#[derive(Debug)]
pub enum FrontmatterError {
    /// The document does not open with a `+++` block, or never closes it.
    Missing {
        /// The guide that could not be parsed.
        slug: String,
    },
    /// The document has a `+++` block whose TOML does not parse, or whose
    /// fields do not match [`Frontmatter`].
    Invalid {
        /// The guide that could not be parsed.
        slug: String,
        /// The TOML parser's own message.
        message: String,
    },
}

/// Parse one source into a page.
///
/// # Errors
///
/// Returns [`FrontmatterError`] when the `+++` block is absent, unterminated,
/// or not valid TOML for [`Frontmatter`].
pub fn parse_page(slug: &str, content: &str) -> Result<ParsedPage, FrontmatterError> {
    let (frontmatter, body) = split_frontmatter(slug, content)?;
    Ok(ParsedPage {
        slug: slug.to_owned(),
        frontmatter,
        body: body.trim_start().to_owned(),
    })
}

/// Order pages the way the sidebar, the sitemap, and the JSON API all expect:
/// `order` ascending, then slug as the tiebreaker.
pub fn sort_pages<T>(pages: &mut [T], key: impl Fn(&T) -> (u32, &str)) {
    pages.sort_by(|left, right| key(left).cmp(&key(right)));
}

/// Split a document into its TOML frontmatter and raw Markdown body.
///
/// Expects the content to start with `+++\n`, followed by TOML, followed by
/// `\n+++` on its own line. The body is everything after the closing delimiter.
fn split_frontmatter<'a>(
    slug: &str,
    content: &'a str,
) -> Result<(Frontmatter, &'a str), FrontmatterError> {
    let content = content.trim_start();

    let after_open = content
        .strip_prefix("+++\n")
        .or_else(|| content.strip_prefix("+++\r\n"))
        .ok_or_else(|| FrontmatterError::Missing {
            slug: slug.to_owned(),
        })?;

    let close_pos = after_open
        .find("\n+++")
        .ok_or_else(|| FrontmatterError::Missing {
            slug: slug.to_owned(),
        })?;

    let toml_str = &after_open[..close_pos];
    // Skip the closing `\n+++`.
    let after_close = &after_open[close_pos + 4..];

    // Skip the optional newline immediately after the closing `+++`.
    let body = after_close
        .strip_prefix("\r\n")
        .or_else(|| after_close.strip_prefix('\n'))
        .unwrap_or(after_close);

    // Normalize CRLF → LF before TOML parsing so that embedded files checked
    // out on Windows (where git autocrlf converts line endings) parse correctly.
    //
    // Every `\r` goes, not just the ones paired with a `\n`. That is not
    // over-eager: `find("\n+++")` lands on the `\n` *of* a `\r\n`, which leaves
    // a lone trailing `\r` in the TOML slice, and the toml crate rejects a bare
    // carriage return outright ("carriage return must be followed by newline").
    // Replacing only the pairs leaves exactly that one behind and fails every
    // CRLF checkout — which is how this was written first, and what
    // `crlf_frontmatter_parses` caught.
    let normalized;
    let for_parse: &str = if toml_str.contains('\r') {
        normalized = toml_str.replace('\r', "");
        &normalized
    } else {
        toml_str
    };

    let frontmatter = toml::from_str(for_parse).map_err(|error| FrontmatterError::Invalid {
        slug: slug.to_owned(),
        message: error.to_string(),
    })?;

    Ok((frontmatter, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_well_formed_document() {
        let page = parse_page(
            "greeting",
            "+++\ntitle = \"Greeting\"\ndescription = \"Hi\"\norder = 7\n+++\n\n# Greeting\n\nBody.\n",
        )
        .expect("parses");

        assert_eq!(page.slug, "greeting");
        assert_eq!(page.frontmatter.title, "Greeting");
        assert_eq!(page.frontmatter.description, "Hi");
        assert_eq!(page.frontmatter.order, 7);
        assert_eq!(page.body, "# Greeting\n\nBody.\n");
    }

    #[test]
    fn description_and_order_default_when_absent() {
        let page = parse_page("t", "+++\ntitle = \"T\"\n+++\nBody\n").expect("parses");

        assert_eq!(page.frontmatter.description, "");
        assert_eq!(page.frontmatter.order, 0);
    }

    #[test]
    fn crlf_frontmatter_parses() {
        let page = parse_page("t", "+++\r\ntitle = \"T\"\r\norder = 3\r\n+++\r\nBody\r\n")
            .expect("parses");

        assert_eq!(page.frontmatter.title, "T");
        assert_eq!(page.frontmatter.order, 3);
    }

    #[test]
    fn a_document_without_frontmatter_is_an_error() {
        assert!(matches!(
            parse_page("t", "# No frontmatter\n"),
            Err(FrontmatterError::Missing { .. })
        ));
    }

    #[test]
    fn an_unterminated_block_is_an_error() {
        assert!(matches!(
            parse_page("t", "+++\ntitle = \"T\"\n"),
            Err(FrontmatterError::Missing { .. })
        ));
    }

    #[test]
    fn a_missing_title_is_invalid_not_missing() {
        assert!(matches!(
            parse_page("t", "+++\norder = 1\n+++\nBody\n"),
            Err(FrontmatterError::Invalid { .. })
        ));
    }

    #[test]
    fn sorting_is_by_order_then_slug() {
        let mut pages = vec![("b", 1_u32), ("a", 2), ("c", 1)];
        sort_pages(&mut pages, |(slug, order)| (*order, slug));

        assert_eq!(
            pages.iter().map(|(slug, _)| *slug).collect::<Vec<_>>(),
            ["b", "c", "a"]
        );
    }
}
