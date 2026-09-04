//! Guards for the docs-search matcher.
//!
//! Issue #23 profiled `SearchIndex::search` — the per-request path behind
//! `/api/search`, `/search` and the MCP `search_autumn_docs` tool — and found
//! that ~96% of the marginal cost of a request was `str::contains`, called
//! once per query token per field per page over the whole embedded corpus.
//! Searching is now `memchr::memmem`'s, with one finder per query token built
//! once for the query — see
//! `docs/plans/2026-09-03-aho-corasick-docs-search.md` for why the
//! multi-pattern pass the issue proposed was measured and left out, and
//! `docs/plans/2026-09-04-memchr-docs-search.md` for why the matcher talks to
//! `memchr` directly rather than through the `aho-corasick` wrapper #23's own
//! recommendation shipped with (issue #28).
//!
//! The load-bearing constraint is that this is a *performance* change. The
//! issue rejected a word/token index precisely because it would change which
//! pages match; nothing here may. So two kinds of test live in this file:
//!
//! 1. Pins on the build input the change depends on (the dependency itself),
//!    mirroring `tests/syntax_highlighting_backend.rs`.
//! 2. Behavioural pins on the search semantics that must survive the rewrite —
//!    substring (not whole-word) matching, all-tokens-required filtering, the
//!    title/heading/body weighting, and the awkward corners a matcher swap
//!    can silently get wrong (overlapping tokens, repeated tokens, case
//!    folding, queries with far more tokens than a search box ever sends).
//!
//! The differential test that compares the matcher against the naive scan over
//! the real corpus lives in `src/docs.rs`, where the private scoring internals
//! are reachable.

use autumn_io::docs::{DocRegistry, DocSource, SearchIndex};

const CARGO_TOML: &str = include_str!("../Cargo.toml");
const CARGO_LOCK: &str = include_str!("../Cargo.lock");

/// The `dependencies = [...]` block of one `Cargo.lock` package entry.
fn locked_dependencies_of(crate_name: &str) -> Vec<&'static str> {
    let entry = CARGO_LOCK
        .split("[[package]]")
        .find(|entry| entry.contains(&format!("name = \"{crate_name}\"")))
        .unwrap_or_else(|| panic!("Cargo.lock should contain a {crate_name} package entry"));

    entry
        .split_once("dependencies = [")
        .map(|(_, rest)| {
            rest.split_once(']')
                .expect("a dependencies block should be closed")
                .0
                .lines()
                .map(|line| line.trim().trim_end_matches(',').trim_matches('"'))
                .filter(|line: &&str| !line.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Build a one-page index. `body` is Markdown appended after the H1, so the
/// fixture goes through the same render → plain-text path the real guides do.
///
/// `DocRegistry::from_sources` takes `DocSource<'static>` because the real
/// sources are `include_str!`ed guides; leaking one fixture per call is the
/// cheapest way to hand it an owned document from a test.
fn index_of(pages: &[(&str, &str, &str)]) -> SearchIndex {
    let sources = pages
        .iter()
        .enumerate()
        .map(|(order, (slug, title, body))| {
            let source: &'static str = format!(
                "+++\ntitle = \"{title}\"\ndescription = \"Fixture description for {slug}.\"\norder = {}\n+++\n\n# {title}\n\n{body}\n",
                order + 1
            )
            .leak();
            let slug: &'static str = slug.to_string().leak();
            DocSource::new(slug, source)
        })
        .collect::<Vec<_>>();

    let registry = DocRegistry::from_sources(sources).expect("fixture registry should build");
    SearchIndex::from_registry(&registry)
}

fn slugs(hits: &[autumn_io::docs::SearchHit]) -> Vec<&str> {
    hits.iter().map(|hit| hit.slug.as_str()).collect()
}

/// The load-bearing dependency of issue #23, as issue #28 shipped it.
///
/// `str::contains` was ~96% of the marginal cost of a request. `memchr::memmem`
/// answers the same question — does this token appear in this text? — for
/// about half the instructions, and does so directly rather than through the
/// `aho-corasick` automaton wrapper #23's own recommendation used: a wrapper
/// whose per-query construction cost was 500-1000x `memmem::Finder::new`'s
/// (see `docs/plans/2026-09-04-memchr-docs-search.md`).
///
/// This pins the manifest only: it fails if someone removes the dependency,
/// not if someone stops using it. The unit tests in `src/docs.rs` are what
/// hold the matcher itself to actually building finders.
#[test]
fn docs_search_declares_the_memchr_matcher() {
    let declared = CARGO_TOML
        .split("[dependencies]")
        .nth(1)
        .expect("Cargo.toml should have a [dependencies] section")
        .lines()
        .take_while(|line| !line.starts_with('['))
        .any(|line| line.trim_start().starts_with("memchr"));

    assert!(
        declared,
        "docs search needs memchr as a direct dependency (issue #28); \
         without it the per-token substring scan comes back"
    );
}

/// `memchr` reaches us as a true leaf — no dependencies of its own, pure Rust,
/// no build script — one level flatter than the `aho-corasick` wrapper it
/// replaced (issue #28). That property is most of why either was an
/// acceptable answer to #23 at all: this repo has been bitten twice by
/// builder OOM (#9, #16), and the alternative structural fix was a whole new
/// index.
///
/// Checked against the resolved lockfile rather than the declaration, so a
/// future version that grows a dependency surface fails here rather than in
/// Docker.
#[test]
fn the_matcher_dependency_stays_a_leaf() {
    assert_eq!(
        locked_dependencies_of("memchr"),
        Vec::<&str>::new(),
        "memchr should stay a dependency-free leaf; anything else changes the \
         build graph this change was accepted on"
    );
}

/// `aho-corasick`'s automaton wrapper is exactly what issue #28 removed:
/// `memchr::memmem::Finder` costs 11-266 ns to build against ~12.7 µs for a
/// single-pattern automaton, which was the reason the matcher needed a
/// minimum-length floor, a maximum-length ceiling and a token-count cap at
/// all. Pinned so it does not silently come back.
#[test]
fn docs_search_no_longer_needs_the_aho_corasick_wrapper() {
    let declared = CARGO_TOML
        .split("[dependencies]")
        .nth(1)
        .expect("Cargo.toml should have a [dependencies] section")
        .lines()
        .take_while(|line| !line.starts_with('['))
        .any(|line| line.trim_start().starts_with("aho-corasick"));

    assert!(
        !declared,
        "docs search no longer needs aho-corasick as a direct dependency \
         (issue #28); memchr::memmem::Finder replaces it"
    );
}

/// The semantics the issue refused to trade away. A token matches anywhere in
/// the text, including inside a longer word — which is exactly what a
/// whole-word token index would have broken, and what a multi-pattern matcher
/// must keep.
#[test]
fn search_matches_substrings_inside_longer_words() {
    let index = index_of(&[("auth", "Auth Guide", "Requests are authenticated per user.")]);

    // "cate" is also a substring of "authenticated" — the point of the rule.
    assert_eq!(slugs(&index.search("authent", 10)), ["auth"]);
    assert_eq!(slugs(&index.search("cate", 10)), ["auth"]);
    assert_eq!(slugs(&index.search("authentz", 10)), Vec::<&str>::new());
}

/// The trap a non-overlapping multi-pattern search falls into, and the reason
/// this file pins it even though the shipped matcher searches token by token.
///
/// A matcher that reports non-overlapping matches consumes `categor` and then
/// resumes *after* it, so `ego` inside `category` is never reported and the
/// page silently stops matching. Both tokens are present as substrings, so
/// both must count, whatever the matcher underneath is.
#[test]
fn search_counts_tokens_that_overlap_another_tokens_match() {
    let index = index_of(&[("taxonomy", "Taxonomy", "Every widget has a category.")]);

    // "categor" and "ego" overlap inside "category"; "cat" is nested in both.
    assert_eq!(slugs(&index.search("categor ego", 10)), ["taxonomy"]);
    assert_eq!(slugs(&index.search("cat categor ego", 10)), ["taxonomy"]);
    assert_eq!(slugs(&index.search("ego categor", 10)), ["taxonomy"]);
}

/// Every token must match somewhere on the page, across any field.
#[test]
fn search_requires_every_token_to_match() {
    let index = index_of(&[
        (
            "widgets",
            "Widget Guide",
            "## Zebra handling\n\nProduction zebras.",
        ),
        (
            "jobs",
            "Background Jobs",
            "Jobs discuss giraffes and queues.",
        ),
    ]);

    assert_eq!(slugs(&index.search("zebra handling", 10)), ["widgets"]);
    assert!(index.search("zebra giraffes", 10).is_empty());
    assert!(index.search("zebra handling giraffes", 10).is_empty());
}

/// A token found in a title outranks the same token found only in a heading,
/// which outranks one found only in the body. The weights are additive across
/// fields (title 8, heading 4, body 1), so this pins both the per-field
/// weights and the fact that they accumulate rather than short-circuit on the
/// heaviest field that matched.
#[test]
fn search_ranks_title_above_heading_above_body() {
    let index = index_of(&[
        ("body-only", "Alpha", "The zebra appears only in the body."),
        (
            "heading-only",
            "Beta",
            "## Zebra section\n\nNothing else here.",
        ),
        (
            "title-and-body",
            "Zebra Handbook",
            "A zebra in the body too.",
        ),
    ]);

    assert_eq!(
        slugs(&index.search("zebra", 10)),
        ["title-and-body", "heading-only", "body-only"]
    );
}

/// Ties break on title so that ordering is stable rather than dependent on
/// registry order or, now, on the order a matcher happens to report matches in.
#[test]
fn search_breaks_score_ties_on_title() {
    let index = index_of(&[
        ("zeta", "Zeta", "A zebra lives here."),
        ("alpha", "Alpha", "A zebra lives here."),
        ("mid", "Mid", "A zebra lives here."),
    ]);

    assert_eq!(slugs(&index.search("zebra", 10)), ["alpha", "mid", "zeta"]);
}

/// A repeated query token is scored once per occurrence *in the query*. A
/// matcher has to deduplicate its patterns to address them by index, and the
/// easy mistake is to let that deduplication reach the score.
///
/// `zebra zebra alpha` scores the zebra-titled page 8 + 8 + 1 = 17 and the
/// alpha-titled page 1 + 1 + 8 = 10. Deduplicated, both score 9, and the
/// title tie-break would put `Alpha Handbook` first — so the two behaviours
/// are distinguishable by the result order alone.
#[test]
fn search_scores_a_repeated_token_once_per_occurrence() {
    let index = index_of(&[
        (
            "zebra-titled",
            "Zebra Handbook",
            "The alpha appears in the body.",
        ),
        (
            "alpha-titled",
            "Alpha Handbook",
            "The zebra appears in the body.",
        ),
    ]);

    assert_eq!(
        slugs(&index.search("zebra zebra alpha", 10)),
        ["zebra-titled", "alpha-titled"]
    );
}

/// Matching is case-insensitive on both sides, including beyond ASCII: the
/// index lowercases page text with `str::to_lowercase`, so the query has to be
/// folded the same way. An ASCII-only shortcut (`ascii_case_insensitive` on
/// the matcher, say) would leave the page's `CAFÉ` unfolded and stop matching
/// a lowercase `café` query.
///
/// `to_lowercase` is a lowercase mapping, not full case folding, so `STRASSE`
/// still does not find `Straße`. That is the behaviour today and this change
/// does not set out to alter it; the assertion is here so a matcher swap that
/// *would* alter it fails loudly rather than silently.
#[test]
fn search_folds_case_beyond_ascii() {
    // The only occurrence is the uppercase one in the body, so an ASCII-only
    // fold on the page side leaves it as `CAFÉ` and the lowercase query stops
    // matching. A fixture with `Café` in the title would pass either way.
    let index = index_of(&[("cafe", "Beverage Guide", "Straße service is CAFÉ-side.")]);

    assert_eq!(slugs(&index.search("café", 10)), ["cafe"]);
    assert_eq!(slugs(&index.search("CAFÉ", 10)), ["cafe"]);
    assert_eq!(slugs(&index.search("STRAßE", 10)), ["cafe"]);
    assert!(index.search("STRASSE", 10).is_empty());
}

/// A two-character token — the shortest the search box ever sends
/// (`min_length(2)` in `site.rs`) — still cuts a snippet around its match, on
/// the snippet path as well as the scoring one. This is the case the
/// `aho-corasick` version's minimum-length floor used to route around the
/// searcher for (issue #28); a `memchr::memmem::Finder` handles it directly.
#[test]
fn short_tokens_still_cut_the_snippet_around_the_match() {
    let index = index_of(&[(
        "zed",
        "Zed",
        "A preamble long enough to push the match well clear of the start of \
         the page, so the snippet has to be cut around it rather than merely \
         starting at the beginning: a zebra grazes beyond the fence.",
    )]);

    let hits = index.search("br", 10);

    assert_eq!(slugs(&hits), ["zed"]);
    assert!(
        hits[0].snippet.contains("zebra"),
        "got: {}",
        hits[0].snippet
    );
}

/// A query that only matched the title or a heading has no body offset to cut
/// around, so the snippet falls back to the page description.
#[test]
fn a_match_outside_the_body_falls_back_to_the_description() {
    let index = index_of(&[("widgets", "Zebra Handbook", "Nothing relevant here.")]);

    let hits = index.search("handbook", 10);

    assert_eq!(slugs(&hits), ["widgets"]);
    assert_eq!(hits[0].snippet, "Fixture description for widgets.");
}

/// A query far longer than a search box would ever send — seventy distinct
/// tokens, past `MAX_FINDERS` (32, unchanged by issue #28's matcher swap) —
/// still filters and ranks rather than truncating, dropping tokens, or
/// panicking.
#[test]
fn search_handles_far_more_tokens_than_a_search_box_sends() {
    let words: Vec<String> = (0..70).map(|n| format!("token{n}")).collect();
    let body: &'static str = words.join(" ").leak();
    let index = index_of(&[
        ("many", "Many Tokens", body),
        ("few", "Few", "token0 only."),
    ]);

    let query = words.join(" ");
    assert_eq!(slugs(&index.search(&query, 10)), ["many"]);

    // One token that is absent still filters the page out, 70 tokens deep.
    let missing = format!("{query} absentium");
    assert!(index.search(&missing, 10).is_empty());

    // The 70th token is what decides the result, so it is really being
    // matched rather than waved through past any cap.
    let mut altered = words.clone();
    altered[69] = "token69x".to_string();
    assert!(index.search(&altered.join(" "), 10).is_empty());
}

/// A single enormous token still has to match exactly. The `aho-corasick`
/// version used to fall back to a plain scan here — an automaton cost about
/// thirty bytes per pattern byte, and this runs on a 256 MB machine against an
/// uncapped query string (issue #28) — but a `memchr::memmem::Finder` over a
/// 4 KB pattern allocates nothing beyond the pattern itself.
#[test]
fn search_matches_a_very_long_token() {
    let long: &'static str = "z".repeat(4096).leak();
    let body: &'static str = format!("A page containing {long} and nothing else.").leak();
    let index = index_of(&[("long", "Long", body), ("short", "Short", "No z run here.")]);

    assert_eq!(slugs(&index.search(long, 10)), ["long"]);
    assert!(index.search(&format!("{long}z"), 10).is_empty());

    // Mixed with ordinary tokens, both still have to agree.
    assert_eq!(slugs(&index.search(&format!("page {long}"), 10)), ["long"]);
    assert!(index.search(&format!("absent {long}"), 10).is_empty());
}

/// An empty or whitespace-only query matches nothing rather than everything.
#[test]
fn search_returns_nothing_for_an_empty_query() {
    let index = index_of(&[("widgets", "Widget Guide", "Anything at all.")]);

    assert!(index.search("", 10).is_empty());
    assert!(index.search("   \t ", 10).is_empty());
}

/// `limit` caps the result count without disturbing the ranking.
#[test]
fn search_applies_the_result_limit_after_ranking() {
    let index = index_of(&[
        ("body-only", "Alpha", "The zebra appears only in the body."),
        ("heading-only", "Beta", "## Zebra section\n\nNothing else."),
        (
            "title-and-body",
            "Zebra Handbook",
            "A zebra in the body too.",
        ),
    ]);

    assert_eq!(
        slugs(&index.search("zebra", 2)),
        ["title-and-body", "heading-only"]
    );
    assert!(index.search("zebra", 0).is_empty());
}

/// Snippets are cut from the original-cased text using offsets found in its
/// lowercased copy. `str::to_lowercase` is not length-preserving — `İ`
/// (U+0130, 2 bytes) folds to `i` plus a combining dot (3 bytes) — so every
/// such character earlier in the page shifts every later offset, and a snippet
/// taken at the unadjusted offset points past the match it is supposed to show.
///
/// 120 of them puts the drift past `SNIPPET_RADIUS`, so the window misses the
/// match entirely. Nothing in the corpus does this today; the guides describe
/// real-world deployments, and one `İstanbul` section would be enough.
#[test]
fn snippets_survive_case_folding_that_changes_byte_offsets() {
    let body: &'static str =
        format!("{} and then the zebra appears.", "İstanbul ".repeat(120)).leak();
    let index = index_of(&[("cities", "Cities", body)]);

    let hits = index.search("zebra", 10);

    assert_eq!(slugs(&hits), ["cities"]);
    assert!(
        hits[0].snippet.contains("zebra"),
        "the snippet should show the match it was cut around, got: {}",
        hits[0].snippet
    );
}

/// The same shift, in the other direction: the fold must not run off the end
/// of the original text or split a character, whatever the offset drift is.
#[test]
fn snippets_stay_valid_when_the_match_is_the_last_thing_on_the_page() {
    let body: &'static str = format!("{}zebra", "İ".repeat(200)).leak();
    let index = index_of(&[("edge", "Edge", body)]);

    let hits = index.search("zebra", 10);

    assert_eq!(slugs(&hits), ["edge"]);
    assert!(
        hits[0].snippet.contains("zebra"),
        "got: {}",
        hits[0].snippet
    );
}
