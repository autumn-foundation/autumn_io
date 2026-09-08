//! The edge lane's ports, checked against what they were ported from.
//!
//! Two pieces of `autumn-web` had to come across when the read path started
//! compiling for `wasm32-wasip1`, because `autumn-web` is not in a capsule's
//! dependency graph at all:
//!
//! - `autumn_web::markdown` → [`autumn_io::frontmatter`]
//! - `autumn_web::widgets`' active-search widget → [`autumn_io::widgets`]
//!
//! Neither is supposed to be an improvement on the original. The frontmatter
//! parser decides what every page's title, description and ordering are; the
//! widget decides what the docs sidebar looks like on every page. A port that
//! drifted would change the site quietly, and — because the origin binary
//! renders through the *same* ports — it would not even show up as an
//! origin/edge divergence in `tests/edge_conformance.rs`. It would just be
//! wrong in both lanes at once.
//!
//! So the check has to be against the framework itself, which is only possible
//! on the native target, which is exactly where this test runs.

use autumn_io::frontmatter;
use autumn_io::widgets::{self, ActiveSearchConfig};
use autumn_web::markdown::{MarkdownRegistry, MarkdownSource};

/// Every guide the site bundles, as the registry sees them.
fn guide_sources() -> Vec<(&'static str, &'static str)> {
    let mut sources = Vec::new();
    for entry in std::fs::read_dir("content/guide").expect("guide directory") {
        let path = entry.expect("directory entry").path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("md") {
            continue;
        }
        let slug = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("utf-8 file name")
            .to_owned();
        let content = std::fs::read_to_string(&path).expect("guide is readable");
        sources.push((
            Box::leak(slug.into_boxed_str()) as &'static str,
            Box::leak(content.into_boxed_str()) as &'static str,
        ));
    }
    sources.sort_by_key(|(slug, _)| *slug);
    sources
}

#[test]
fn frontmatter_port_agrees_with_the_framework_on_every_bundled_guide() {
    let sources = guide_sources();
    assert!(
        sources.len() > 100,
        "expected the whole guide corpus, found {}",
        sources.len()
    );

    let framework_sources: Vec<MarkdownSource> = sources
        .iter()
        .map(|(slug, content)| MarkdownSource { slug, content })
        .collect();
    let framework =
        MarkdownRegistry::from_embedded(&framework_sources).expect("framework parses the corpus");

    for (slug, content) in &sources {
        let ours = frontmatter::parse_page(slug, content)
            .unwrap_or_else(|error| panic!("{slug}: port failed to parse: {error:?}"));
        let theirs = framework.get(slug).expect("framework parsed this slug");

        assert_eq!(ours.slug, theirs.slug, "{slug}: slug");
        assert_eq!(
            ours.frontmatter.title, theirs.frontmatter.title,
            "{slug}: title"
        );
        assert_eq!(
            ours.frontmatter.description, theirs.frontmatter.description,
            "{slug}: description"
        );
        assert_eq!(
            ours.frontmatter.order, theirs.frontmatter.order,
            "{slug}: order"
        );
        assert_eq!(ours.body, theirs.body, "{slug}: body");
    }
}

#[test]
fn frontmatter_port_orders_pages_the_way_the_framework_did() {
    let sources = guide_sources();
    let framework_sources: Vec<MarkdownSource> = sources
        .iter()
        .map(|(slug, content)| MarkdownSource { slug, content })
        .collect();
    let framework =
        MarkdownRegistry::from_embedded(&framework_sources).expect("framework parses the corpus");

    let mut ours: Vec<_> = sources
        .iter()
        .map(|(slug, content)| frontmatter::parse_page(slug, content).expect("parses"))
        .collect();
    frontmatter::sort_pages(&mut ours, |page| {
        (page.frontmatter.order, page.slug.as_str())
    });

    let theirs: Vec<&str> = framework
        .all_sorted()
        .into_iter()
        .map(|page| page.slug.as_str())
        .collect();
    let ours: Vec<&str> = ours.iter().map(|page| page.slug.as_str()).collect();

    assert_eq!(ours, theirs);
}

/// Line endings the port must handle the same way the framework does.
///
/// Not hypothetical: `find("\n+++")` lands on the `\n` of a `\r\n`, leaving a
/// lone `\r` at the end of the TOML slice that the toml crate rejects outright.
/// A port that normalised only `\r\n` pairs would parse the whole LF corpus
/// perfectly and fail every single guide on a Windows checkout — invisible to
/// every other test here, all of which read the repository's own LF files.
#[test]
fn frontmatter_port_handles_line_endings_the_way_the_framework_does() {
    const CRLF: &str =
        "+++\r\ntitle = \"T\"\r\ndescription = \"D\"\r\norder = 3\r\n+++\r\nBody\r\n";
    const LF: &str = "+++\ntitle = \"T\"\ndescription = \"D\"\norder = 3\n+++\nBody\n";

    for (label, content) in [("crlf", CRLF), ("lf", LF)] {
        let ours = frontmatter::parse_page("t", content)
            .unwrap_or_else(|error| panic!("{label}: port failed to parse: {error:?}"));
        let framework = MarkdownRegistry::from_embedded(&[MarkdownSource { slug: "t", content }])
            .unwrap_or_else(|error| panic!("{label}: framework failed to parse: {error:?}"));
        let theirs = framework.get("t").expect("framework parsed it");

        assert_eq!(ours.frontmatter.title, theirs.frontmatter.title, "{label}");
        assert_eq!(
            ours.frontmatter.description, theirs.frontmatter.description,
            "{label}"
        );
        assert_eq!(ours.frontmatter.order, theirs.frontmatter.order, "{label}");
        assert_eq!(ours.body, theirs.body, "{label}");
    }
}

/// The exact configuration `site::docs_search_box` builds.
fn docs_search_config() -> (
    ActiveSearchConfig<'static>,
    autumn_web::widgets::ActiveSearchConfig<'static>,
) {
    const ACTION: &str = "/search";
    const TARGET: &str = "#docs-search-results";
    const INDICATOR: &str = "#docs-search-indicator";
    const PLACEHOLDER: &str = "Search the guides…";

    (
        ActiveSearchConfig::new(ACTION, TARGET)
            .placeholder(PLACEHOLDER)
            .min_length(2)
            .indicator(INDICATOR),
        autumn_web::widgets::ActiveSearchConfig::new(ACTION, TARGET)
            .placeholder(PLACEHOLDER)
            .min_length(2)
            .indicator(INDICATOR),
    )
}

#[test]
fn active_search_port_renders_the_framework_widget_byte_for_byte() {
    let (ours, theirs) = docs_search_config();

    assert_eq!(
        widgets::active_search("docs-search", "Search docs", &ours).into_string(),
        autumn_web::widgets::active_search("docs-search", "Search docs", &theirs).into_string(),
    );
}

#[test]
fn active_search_empty_state_port_matches_the_framework() {
    for message in [
        "Type to search the guides.",
        "Search is unavailable right now.",
        "No guides match “tokio”.",
        // The framework escapes the message; so must the port.
        "<script>alert(1)</script>",
    ] {
        assert_eq!(
            widgets::active_search_empty_state(message).into_string(),
            autumn_web::widgets::active_search_empty_state(message).into_string(),
            "empty state for {message:?}",
        );
    }
}

#[test]
fn htmx_asset_path_matches_the_framework() {
    assert_eq!(widgets::HTMX_JS_PATH, autumn_web::prelude::HTMX_JS_PATH);
}
