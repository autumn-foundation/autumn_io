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
//! - **List** (`list_autumn_docs`): one unfiltered tally of every page's
//!   group via `site::doc_group_label`, plus one filtered tally per group
//!   actually present in the corpus — the handler's filter closure calls
//!   `doc_group_label` again for *every* page regardless of match, so a
//!   filtered call cost the same tally pass twice.
//! - **Read + narrow** (`get_autumn_doc`): for every guide, the whole-guide
//!   fetch (`page.markdown.clone()`, `page.preamble()`), then
//!   `DocPage::section` for every heading in `page.toc` — every id the
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

use autumn_io::site;

fn main() {
    let registry = autumn_io::site_docs().expect("embedded guides render");

    // list_autumn_docs: one unfiltered tally, plus one filtered tally per
    // group actually present in the corpus — the same two passes the
    // handler runs per request, filtered or not.
    let mut group_names: Vec<&'static str> = Vec::new();
    let mut list_total = 0usize;
    for page in registry.pages() {
        let name = site::doc_group_label(&page.slug);
        if !group_names.contains(&name) {
            group_names.push(name);
        }
        list_total += name.len();
    }
    for name in &group_names {
        for page in registry.pages() {
            if site::doc_group_label(&page.slug) == *name {
                list_total += 1;
            }
        }
    }

    // get_autumn_doc: whole-guide fetch plus narrowing into every listed
    // section, for every guide.
    let mut read_total = 0usize;
    let mut sections_visited = 0usize;
    for page in registry.pages() {
        read_total += page.markdown.len();
        read_total += page.preamble().len();
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
