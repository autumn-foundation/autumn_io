//! Guards for the syntax-highlighting regex backend.
//!
//! Issue #19 profiled the cold-start docs render — `autumn_io::site_docs()`,
//! the one-time pass over all embedded guides that both `build_site` and the
//! first request after a scale-to-zero cold boot pay. The regex bucket behind
//! `syntect` (`fancy_regex` + `regex_automata` + `regex_syntax`) was 76.81% of
//! instructions, with `fancy_regex`'s backtracking VM alone at 42.24%, and it
//! held 71.5% of allocation blocks. Our own code was 0.15% of instructions.
//!
//! The fix was a dependency-feature choice, not code: run syntect on its own
//! upstream default backend, Oniguruma, instead of the pure-Rust `fancy-regex`
//! fallback. Because it is a single line in `Cargo.toml`, it is also a single
//! line to lose.
//!
//! Two kinds of test live here. The first three pin the build inputs the swap
//! depends on. The rest pin rendered output exactly, so that a backend change —
//! this one, a `syntect` bump, an `onig` bump — cannot quietly alter how 140
//! guides look. They are golden strings on purpose: when highlighting moves,
//! the right outcome is a loud failure and a human looking at the diff.
//!
//! `tests/fly_deploy_config.rs` holds the other build-configuration guards.

use autumn_io::docs::{DocRegistry, DocSource};

const CARGO_TOML: &str = include_str!("../Cargo.toml");
const CARGO_LOCK: &str = include_str!("../Cargo.lock");
const DOCKERFILE: &str = include_str!("../Dockerfile");

/// `base16-ocean.dark`'s default foreground. A code block rendered entirely in
/// this one colour got no scopes from any grammar — it is plain text, however
/// the fence was labelled.
const PLAIN_TEXT_COLOR: &str = "#c0c5ce";

fn syntect_dependency_line() -> &'static str {
    // Assumes the single-line inline-table form. If the dependency is ever
    // rewritten as a `[dependencies.syntect]` table this stops finding it, and
    // the expect message below is the first thing you will see.
    CARGO_TOML
        .lines()
        .find(|line| line.trim_start().starts_with("syntect"))
        .expect("Cargo.toml should declare syntect as a single-line dependency")
}

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
                .filter_map(|line| line.trim().trim_end_matches(',').trim_matches('"').into())
                .filter(|line: &&str| !line.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn render(markdown: &str) -> String {
    // `DocRegistry::from_sources` takes `DocSource<'static>` because the real
    // sources are `include_str!`ed guides. Leaking one fixture per call is the
    // cheapest way to hand it an owned document from a test.
    let source: &'static str = format!(
        "+++\ntitle = \"Fixture\"\ndescription = \"Highlighting fixture.\"\norder = 10\n+++\n\n{markdown}"
    )
    .leak();
    let registry = DocRegistry::from_sources([DocSource::new("fixture", source)])
        .expect("fixture should render");

    registry
        .page("fixture")
        .expect("fixture should be registered")
        .html
        .clone()
}

/// The `<pre>…</pre>` of the first code block, which is what the highlighter
/// produced. Everything around it belongs to the page template.
fn code_block(html: &str) -> String {
    let start = html.find("<pre>").expect("a rendered code block");
    let end = html.find("</pre>").expect("a closed code block");
    html[start..end + "</pre>".len()].to_owned()
}

/// Every distinct `color:#rrggbb` in the block.
///
/// One colour means the highlighter emitted a single style run — plain text.
/// More than one means a grammar actually scoped the code, which is the
/// property "is this still highlighted?" really asks about; the mere presence
/// of a `<span style="color:` proves nothing, because syntect wraps plain text
/// in one too.
fn distinct_colors(block: &str) -> Vec<&str> {
    let mut colors: Vec<&str> = block
        .match_indices("color:")
        .map(|(index, _)| &block[index + "color:".len()..index + "color:".len() + 7])
        .collect();
    colors.sort_unstable();
    colors.dedup();
    colors
}

/// The load-bearing assertion of issue #19.
///
/// `default-fancy` selects `fancy-regex`, whose backtracking VM dominated the
/// profile; `default-onig` selects Oniguruma, syntect's own default and the
/// engine TextMate grammars are written against. If this fails, someone has
/// put the backtracking VM back on the cold-start path.
#[test]
fn syntect_uses_the_oniguruma_regex_backend() {
    let syntect = syntect_dependency_line();

    assert!(
        syntect.contains("default-onig"),
        "syntect should select the Oniguruma backend (issue #19), got: {syntect}"
    );
    assert!(
        !syntect.contains("default-fancy"),
        "the fancy-regex backend was 76.81% of the cold-start render, got: {syntect}"
    );
}

/// `onig_sys` compiles bundled Oniguruma C. It reaches us with default features
/// off — through `syntect` → `onig` → `onig_sys`, each with
/// `default-features = false` — which is the only reason the build needs just a
/// C compiler. Its own default feature, `generate`, pulls in `bindgen` and with
/// it a libclang requirement that `rust:1.88-bookworm` does not satisfy.
///
/// This checks the resolved shape rather than the declaration, because that
/// chain is three crates deep and invisible from our `Cargo.toml`. Anything
/// that re-enables `generate` — a new dependency on `syntect` with default
/// features, a change to `onig` — shows up here as `bindgen` appearing in
/// `onig_sys`'s dependency list, and would otherwise only fail inside Docker.
///
/// It does not, and cannot, catch a *stale* lockfile: Cargo rewrites
/// `Cargo.lock` before compiling this test. Only `cargo build --locked` (which
/// the Dockerfile uses) catches that.
#[test]
fn the_oniguruma_build_stays_free_of_bindgen() {
    assert_eq!(
        locked_dependencies_of("onig_sys"),
        vec!["cc", "pkg-config"],
        "onig_sys must build from its checked-in bindings; bindgen needs libclang, \
         which the builder image does not have"
    );
    assert!(
        locked_dependencies_of("onig").contains(&"onig_sys"),
        "the onig binding should still be the thing pulling onig_sys in"
    );
}

/// `onig_sys` compiles bundled Oniguruma C at build time, which the pure-Rust
/// backend did not need. The builder stage is what supplies that compiler;
/// swapping it for a slimmer base would break the build in Docker only.
///
/// The assertion is deliberately on the *property* — a full `rust:` image
/// rather than a `-slim` or `-alpine` one — so that a routine Rust version bump
/// does not fail with a message about C toolchains.
#[test]
fn dockerfile_builder_keeps_the_c_toolchain_oniguruma_needs() {
    let (builder, _runtime) = DOCKERFILE
        .split_once("FROM debian:bookworm-slim AS runtime")
        .expect("Dockerfile should have a builder stage and a runtime stage");

    // Instructions only — the surrounding comments talk about `-slim` bases in
    // order to warn against them, and would otherwise trip the check below.
    let builder_base = builder
        .lines()
        .find(|line| line.starts_with("FROM "))
        .expect("the builder stage should declare a base image");

    assert!(
        builder_base.contains("rust:"),
        "the builder base must stay a full rust image, which ships gcc: {builder_base}"
    );
    assert!(
        !builder_base.contains("-slim") && !builder_base.contains("-alpine"),
        "a slim or alpine builder has no C compiler for onig_sys: {builder_base}"
    );
    assert!(
        builder.to_lowercase().contains("oniguruma"),
        "record why the builder needs a C compiler, so nobody slims it away"
    );
    assert!(
        builder.contains("RUSTONIG_STATIC_LIBONIG=1"),
        "pin static linking: a discoverable system libonig would produce a binary \
         the slim runtime stage cannot start"
    );
}

/// Golden output for a Rust block.
///
/// Pinned byte-for-byte, and pinned on purpose. The engines under this markup
/// slice a line into scopes their own way, so this string is the difference
/// between "the corpus still renders" and "the corpus still renders the same".
/// It held identical across the `default-fancy` → `default-onig` swap; if a
/// future backend or grammar bump moves it, that is a visual change to 140
/// guides and wants a human eye, not a re-recorded constant.
#[test]
fn rust_blocks_highlight_into_the_same_scopes_on_either_backend() {
    let block = code_block(&render("```rust\nfn main() {}\n```\n"));

    assert_eq!(
        block,
        concat!(
            r#"<pre><code class="language-rust">"#,
            r#"<span style="color:#b48ead;">fn </span>"#,
            r#"<span style="color:#8fa1b3;">main</span>"#,
            "<span style=\"color:#c0c5ce;\">() {}\n</span>",
            "</code></pre>",
        )
    );
    assert!(
        distinct_colors(&block).len() >= 3,
        "keyword, function name and punctuation should land in different scopes"
    );
}

#[test]
fn shell_blocks_highlight_into_the_same_scopes_on_either_backend() {
    let block = code_block(&render("```bash\ncargo run --bin autumn_io\n```\n"));

    assert_eq!(
        block,
        concat!(
            r#"<pre><code class="language-bash">"#,
            r#"<span style="color:#8fa1b3;">cargo</span>"#,
            r#"<span style="color:#c0c5ce;"> run</span>"#,
            r#"<span style="color:#bf616a;"> --bin</span>"#,
            "<span style=\"color:#c0c5ce;\"> autumn_io\n</span>",
            "</code></pre>",
        )
    );
}

/// Not every fenced language is highlighted, and TOML — which the guides use
/// heavily — is one that is not: syntect's default syntax set carries no TOML
/// grammar, so `syntax_for_language` finds nothing and the block falls through
/// to plain text in the default foreground colour.
///
/// Worth pinning because the obvious assertion (`contains("<span style=\"color:")`)
/// passes here too, and would have let a claim that TOML is highlighted stand.
#[test]
fn toml_blocks_fall_through_to_plain_text_and_keep_their_label() {
    let block = code_block(&render("```toml\nautumn-web = \"0.7.0\"\n```\n"));

    assert!(block.contains(r#"class="language-toml""#));
    assert_eq!(
        distinct_colors(&block),
        vec![PLAIN_TEXT_COLOR],
        "the default syntax set has no TOML grammar, so this is plain text: {block}"
    );
    assert!(block.contains("autumn-web = &quot;0.7.0&quot;"));
}

#[test]
fn unknown_languages_fall_through_to_plain_text_with_their_content_intact() {
    let block = code_block(&render("```klingon\nnuqneH\n```\n"));

    assert_eq!(
        distinct_colors(&block),
        vec![PLAIN_TEXT_COLOR],
        "an unrecognised language must not be scoped by some other grammar: {block}"
    );
    assert!(block.contains("nuqneH"), "content must survive: {block}");
    assert!(
        block.contains(r#"class="language-klingon""#),
        "the declared language is still echoed for copy controls: {block}"
    );
}

/// Regex engines differ in where they cut a line into scopes, and the escaping
/// happens per slice — so a scope boundary landing inside `Vec<&str>` is what
/// decides whether the `<` gets escaped as part of one run or another. Pinned
/// whole, because asserting `!block.contains("Vec<&str>")` would pass even with
/// escaping removed entirely: the highlighter puts a span boundary in the
/// middle of that text either way.
#[test]
fn html_metacharacters_in_code_stay_escaped_across_scope_boundaries() {
    let block = code_block(&render(
        "```rust\nlet generic: Vec<&str> = vec![\"a & b\"];\n```\n",
    ));

    assert_eq!(
        block,
        concat!(
            r#"<pre><code class="language-rust">"#,
            r#"<span style="color:#b48ead;">let</span>"#,
            r#"<span style="color:#c0c5ce;"> generic: Vec&lt;&amp;</span>"#,
            r#"<span style="color:#b48ead;">str</span>"#,
            r#"<span style="color:#c0c5ce;">&gt; = vec![&quot;</span>"#,
            r#"<span style="color:#a3be8c;">a &amp; b</span>"#,
            "<span style=\"color:#c0c5ce;\">&quot;];\n</span>",
            "</code></pre>",
        )
    );
}

/// The corpus this change exists for.
#[test]
fn the_full_embedded_corpus_renders_and_is_still_highlighted() {
    let registry = autumn_io::site_docs().expect("bundled guide docs should load");

    // `content/guide` holds 141 Markdown files, but `docs-smoke` is a
    // release-rehearsal checklist deliberately kept off the site, so the
    // rendered corpus is 140 pages. Issue #19 counted files, not pages.
    assert!(
        registry.pages().len() >= 140,
        "the profiled corpus was 140 rendered guides, found {}",
        registry.pages().len()
    );

    let scoped_pages = registry
        .pages()
        .iter()
        .filter(|page| distinct_colors(&page.html).len() > 1)
        .count();
    assert!(
        scoped_pages > 100,
        "most guides carry code blocks that grammars actually scope; only \
         {scoped_pages} did, which is what a silently failing backend looks like"
    );
}
