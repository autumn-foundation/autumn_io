//! Per-request docs-API/MCP harness.
//!
//! `/api/docs`, `/api/docs/{slug}`, and the MCP tools built from the same
//! handlers (`content/guide/mcp.md`) are thin `axum` wrappers — `Query`,
//! `Path`, and `Json` — around `DocRegistry`/`DocPage` methods and
//! `site::doc_group_label`. This harness calls those directly, the same way
//! `profile_docs_search` calls `SearchIndex::search` rather than going
//! through `search_autumn_docs`: the extractors and JSON serialization are
//! framework code, not logic this site controls, and `/api/search` is
//! already covered by `profile_docs_search` since it shares
//! `SearchIndex::search` with the HTML search box.
//!
//! Two request shapes, matching the two remaining handlers in `api.rs`:
//!
//! - **List** (`list_autumn_docs`): one unfiltered request plus one filtered
//!   request per group actually present in the corpus, each simulated as its
//!   own independent call via [`simulate_list_request`] — every call redoes
//!   the handler's full unfiltered tally pass from scratch (there is no
//!   cross-request cache), so amortizing the tally across the 14 simulated
//!   calls would undercount the path the same way running it once would.
//! - **Read + narrow** (`get_autumn_doc`): for every guide, the whole-guide
//!   fetch — `page.markdown.clone()` and `page.preamble().to_owned()`,
//!   matching the handler's own `(page.markdown.clone(), page.toc.as_slice(),
//!   page.preamble().to_owned())` for the no-`section` case, so the clones'
//!   allocations count rather than just their lengths — then
//!   `DocPage::section` for every heading in `page.toc`: every id the
//!   whole-guide response's `sections` field lists as narrowable. That is
//!   not a hypothetical path: `get_autumn_doc`'s own docs describe it as the
//!   required next call for `deployment.md` (over 150 KB) and
//!   `generators.md`, both of which upstream ships today, so an agent
//!   reading either guide triggers `DocPage::section` on its very first
//!   follow-up call.
//!
//! ```bash
//! cargo build --release --bin profile_docs_api
//!
//! # Instructions. Attribution needs symbols, so do not add `strip` to
//! # `[profile.release]` without expecting hex addresses here.
//! valgrind --tool=callgrind --callgrind-out-file=callgrind.out \
//!     ./target/release/profile_docs_api
//! callgrind_annotate --threshold=99.9 callgrind.out
//!
//! # The build-only baseline to subtract, same method as issue #19/#41:
//! # subtract Total(profile_docs_render) from Total(profile_docs_api) to
//! # isolate this harness's own loop from the one-time registry build both
//! # binaries pay.
//! valgrind --tool=callgrind --callgrind-out-file=callgrind.build.out \
//!     ./target/release/profile_docs_render
//!
//! # Allocations and memory traffic.
//! valgrind --tool=dhat --dhat-out-file=dhat.out.json \
//!     ./target/release/profile_docs_api
//! ```

use autumn_io::docs::DocRegistry;
use autumn_io::site;

/// One `list_autumn_docs` call: the handler's own unfiltered tally pass
/// (always runs, filtered or not), then its `.filter().map(guide_summary)`
/// pass — `Option::is_none_or` short-circuits the filter closure's own
/// `doc_group_label` call when `group` is `None`, exactly as the handler's
/// `filter.as_deref().is_none_or(...)` does, so an unfiltered call pays for
/// the filter pass only in the (skipped) closure calls it would have made.
fn simulate_list_request(registry: &DocRegistry, group: Option<&str>) -> usize {
    let mut total = 0usize;

    for page in registry.pages() {
        total += site::doc_group_label(&page.slug).len();
    }

    let matching: Vec<_> = registry
        .pages()
        .iter()
        .filter(|page| {
            group.is_none_or(|name| {
                total += 1; // the filter closure's own doc_group_label call
                site::doc_group_label(&page.slug) == name
            })
        })
        .collect();

    for page in matching {
        total += site::doc_group_label(&page.slug).len();
    }

    total
}

fn main() {
    let registry = autumn_io::site_docs().expect("embedded guides render");

    let mut group_names: Vec<&'static str> = Vec::new();
    for page in registry.pages() {
        let name = site::doc_group_label(&page.slug);
        if !group_names.contains(&name) {
            group_names.push(name);
        }
    }

    // One unfiltered list_autumn_docs call, plus one filtered call per group
    // actually present in the corpus — each its own independent request.
    let mut list_total = simulate_list_request(registry, None);
    for name in &group_names {
        list_total += simulate_list_request(registry, Some(name));
    }

    // get_autumn_doc: whole-guide fetch plus narrowing into every listed
    // section, for every guide.
    let mut read_total = 0usize;
    let mut sections_visited = 0usize;
    for page in registry.pages() {
        let markdown = page.markdown.clone();
        let preamble = page.preamble().to_owned();
        read_total += markdown.len();
        read_total += preamble.len();
        for item in &page.toc {
            if let Some(section) = page.section(&item.id) {
                read_total += section.markdown.len();
                read_total += section.preamble.len();
                read_total += page.subsections(&item.id).len();
                sections_visited += 1;
            }
        }
    }

    println!(
        "pages={} groups={} list_total={list_total} read_total={read_total} sections_visited={sections_visited}",
        registry.pages().len(),
        group_names.len()
    );
}
