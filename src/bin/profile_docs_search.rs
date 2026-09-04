//! Per-request docs-search harness from issue #23.
//!
//! Builds the real search index once — `autumn_io::site_docs()` feeding
//! `SearchIndex::from_registry`, exactly what `site_search_index()` does at
//! startup — then issues 500 searches per query through `SearchIndex::search`,
//! the same public entry point `/api/search`, `/search` and the MCP
//! `search_autumn_docs` tool call on every request. That is 10,000 requests
//! for `default` and `typeahead`, 5,000 for `multi` — so subtract and divide
//! against a build-only run of the *same* set, never a different one. Every
//! run prints the set it used and its request count.
//!
//! The queries are drawn from real guide topics and filenames, plus a common
//! stop word (`the`) and one query chosen to match nothing, so the mix covers
//! the cheap and expensive ends of the workload rather than one shape of it.
//!
//! `SEARCH_REQUESTS_PER_QUERY=1` runs the same setup with a single search per
//! query, which isolates the one-time index build from the per-request
//! marginal cost: subtract the two runs and divide by the request delta.
//!
//! `SEARCH_QUERY_SET` swaps the mix. `multi` is 3-5 token queries: token count
//! is the axis a multi-pattern matcher is sensitive to, and the default mix is
//! mostly one and two token queries. `typeahead` is the prefixes a
//! search-as-you-type box actually sends — the widget fires from two
//! characters (`min_length(2)` in `site.rs`) — which is the axis *token
//! length* sits on, and where the default mix is silent: its shortest queries
//! are three characters. An unrecognised value is an error rather than a
//! silent fall-through to the default, because default-versus-default is an
//! easy and invisible way to measure nothing.
//!
//! ```bash
//! cargo build --release --bin profile_docs_search
//!
//! # Instructions. Attribution needs symbols, so do not add `strip` to
//! # `[profile.release]` without expecting hex addresses here.
//! valgrind --tool=callgrind --callgrind-out-file=callgrind.out \
//!     ./target/release/profile_docs_search
//! callgrind_annotate --threshold=95 callgrind.out
//!
//! # The build-only baseline to subtract.
//! SEARCH_REQUESTS_PER_QUERY=1 valgrind --tool=callgrind \
//!     --callgrind-out-file=callgrind.build.out \
//!     ./target/release/profile_docs_search
//!
//! # Allocations and memory traffic.
//! valgrind --tool=dhat --dhat-out-file=dhat.out.json \
//!     ./target/release/profile_docs_search
//! ```
//!
//! Figures quoted in `docs/plans/2026-09-03-aho-corasick-docs-search.md` were
//! taken this way on rustc 1.94.1, x86_64-linux. Instruction counts are stable
//! per binary but move with toolchain and dependency versions, which this repo
//! does not pin.

use autumn_io::docs::SearchIndex;

/// Realistic query mix: guide topics and filenames, a stop word that matches
/// nearly every page, and one query that matches nothing.
const QUERIES: &[&str] = &[
    "authentication",
    "authorization",
    "database",
    "migration",
    "caching",
    "deploy",
    "routing middleware",
    "attribute encryption",
    "bot protection",
    "content negotiation",
    "background jobs",
    "rate limiting",
    "websockets",
    "cli",
    "accessibility",
    "clustering",
    "audit logging",
    "conditional get",
    "the",
    "zzzznonexistentzzzz",
];

/// Longer queries, same corpus and same topics. Selected with
/// `SEARCH_QUERY_SET=multi`.
const MULTI_TOKEN_QUERIES: &[&str] = &[
    "routing middleware authentication",
    "attribute encryption audit logging",
    "background jobs rate limiting",
    "content negotiation conditional get",
    "database migration rollback",
    "deploy clustering edge fleet",
    "bot protection rate limiting middleware",
    "active search autocomplete htmx",
    "session cookie signing key rotation",
    "zzzznonexistentzzzz routing middleware",
];

/// Requests per query. Override with `SEARCH_REQUESTS_PER_QUERY` to isolate
/// the one-time index build (`=1`) from the per-request marginal cost.
const REQUESTS_PER_QUERY: usize = 500;

/// One keystroke sequence, as the docs-search box would send it: `min_length(2)`
/// means the first request goes out at two characters. Selected with
/// `SEARCH_QUERY_SET=typeahead`.
const TYPEAHEAD_QUERIES: &[&str] = &[
    "de", "dep", "depl", "deplo", "deploy", "au", "aut", "auth", "authe", "authen", "da", "dat",
    "data", "datab", "cl", "cli", "clu", "clus", "ro", "rou",
];

/// Result page size. `/api/search` asks for 10; the HTML box at `/search` asks
/// for 20, so snippet building costs it about twice this.
const LIMIT: usize = 10;

fn main() {
    let requests_per_query = std::env::var("SEARCH_REQUESTS_PER_QUERY")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(REQUESTS_PER_QUERY);

    let set = std::env::var("SEARCH_QUERY_SET").unwrap_or_else(|_| "default".to_string());
    let queries = match set.as_str() {
        "default" => QUERIES,
        "multi" => MULTI_TOKEN_QUERIES,
        "typeahead" => TYPEAHEAD_QUERIES,
        other => panic!("unknown SEARCH_QUERY_SET {other:?} (default, multi, typeahead)"),
    };

    let registry = autumn_io::site_docs().expect("embedded guides render");
    let index = SearchIndex::from_registry(registry);

    let mut total_hits = 0usize;
    let mut total_snippet_bytes = 0usize;
    for query in queries {
        for _ in 0..requests_per_query {
            let hits = index.search(query, LIMIT);
            total_hits += hits.len();
            total_snippet_bytes += hits.iter().map(|hit| hit.snippet.len()).sum::<usize>();
        }
    }

    println!(
        "set={set} requests={} total_hits={total_hits} total_snippet_bytes={total_snippet_bytes}",
        queries.len() * requests_per_query
    );
}
