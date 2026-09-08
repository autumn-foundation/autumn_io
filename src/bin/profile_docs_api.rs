//! Per-request docs-API/MCP harness.
//!
//! `/api/search` (and the MCP `search_autumn_docs` tool built from the same
//! handler) is already covered by `profile_docs_search`, since it shares
//! `SearchIndex::search` with the HTML search box. `list_autumn_docs` and
//! `get_autumn_doc` — and the MCP tools autumn-web derives from them — had no
//! profiling coverage at all.
//!
//! This calls those two handlers directly rather than reimplementing their
//! logic: an earlier version of this harness summed `doc_group_label` and
//! `DocPage::section` results by hand, and two rounds of review kept finding
//! real per-request work (the group-tally allocations, `GuideSummary`'s
//! description abridging, `gate_by_size`, `GuideSectionRef`/`GuideDocument`
//! construction) it left out. Calling the handlers themselves closes that gap
//! for good instead of chasing it function by function. `list_autumn_docs`
//! and `get_autumn_doc` are `async fn` only because Autumn's route macros
//! require the signature — neither contains an `.await` — so [`block_on`]
//! polls each call once against [`Waker::noop`] rather than pulling in a
//! runtime.
//!
//! Two request shapes, matching the two remaining handlers in `api.rs`:
//!
//! - **List**: one unfiltered `list_autumn_docs` call, plus one filtered call
//!   per group its own response lists — the same discovery path an agent
//!   would follow.
//! - **Read + narrow**: for every guide, the whole-guide `get_autumn_doc`
//!   call, then one more call per heading in the whole-guide response's own
//!   `sections` list — every id it lists as narrowable. That is not a
//!   hypothetical path: `get_autumn_doc`'s own docs describe it as the
//!   required next call for `deployment.md` (over 150 KB) and
//!   `generators.md`, both of which upstream ships today, so an agent
//!   reading either guide triggers it on its very first follow-up call.
//!
//! ```bash
//! cargo build --profile profiling --bin profile_docs_api
//!
//! # Instructions. Attribution needs symbols, which `[profile.release]` now
//! # strips — build through `[profile.profiling]`, which inherits it and puts
//! # the symbol table and debug info back, or read hex addresses here.
//! valgrind --tool=callgrind --callgrind-out-file=callgrind.out \
//!     ./target/profiling/profile_docs_api
//! callgrind_annotate --threshold=99.9 callgrind.out
//!
//! # The build-only baseline to subtract, same method as issue #19/#41:
//! # subtract Total(profile_docs_render) from Total(profile_docs_api) to
//! # isolate this harness's own loop from the one-time registry build both
//! # binaries pay.
//! valgrind --tool=callgrind --callgrind-out-file=callgrind.build.out \
//!     ./target/profiling/profile_docs_render
//!
//! # Allocations and memory traffic.
//! valgrind --tool=dhat --dhat-out-file=dhat.out.json \
//!     ./target/profiling/profile_docs_api
//! ```

use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use autumn_io::api::{GetDocQuery, ListDocsQuery, get_autumn_doc, list_autumn_docs};
use autumn_web::extract::{Path, Query};

/// Poll `future` once and return its output, panicking if it is not ready
/// immediately.
///
/// Sound here specifically because `list_autumn_docs`/`get_autumn_doc`
/// contain no `.await` (confirmed by grep, not just by reading): they are
/// `async fn` only because Autumn's route macros require it, so the first
/// poll always resolves and nothing ever calls the waker it was given.
fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut cx) {
        Poll::Ready(value) => value,
        Poll::Pending => unreachable!("docs API handlers never suspend"),
    }
}

fn main() {
    let registry = autumn_io::site_docs().expect("embedded guides render");

    let index = block_on(list_autumn_docs(Query(ListDocsQuery { group: None })))
        .expect("unfiltered list_autumn_docs succeeds")
        .0;
    let mut list_total = index.guides.len();
    for group in &index.groups {
        let filtered = block_on(list_autumn_docs(Query(ListDocsQuery {
            group: Some(group.name.clone()),
        })))
        .expect("filtered list_autumn_docs succeeds")
        .0;
        list_total += filtered.guides.len();
    }

    let mut read_total = 0usize;
    let mut sections_visited = 0usize;
    for page in registry.pages() {
        let whole = block_on(get_autumn_doc(
            Path(page.slug.clone()),
            Query(GetDocQuery { section: None }),
        ))
        .expect("whole-guide get_autumn_doc succeeds")
        .0;
        read_total += whole.markdown.as_deref().map_or(0, str::len);
        read_total += whole.sections.len();

        for item in page.toc() {
            let section = block_on(get_autumn_doc(
                Path(page.slug.clone()),
                Query(GetDocQuery {
                    section: Some(item.id.clone()),
                }),
            ))
            .expect("section get_autumn_doc succeeds")
            .0;
            read_total += section.markdown.as_deref().map_or(0, str::len);
            read_total += section.sections.len();
            sections_visited += 1;
        }
    }

    println!(
        "pages={} groups={} list_total={list_total} read_total={read_total} sections_visited={sections_visited}",
        registry.pages().len(),
        index.groups.len()
    );
}
