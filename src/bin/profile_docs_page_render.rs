//! Per-request docs-page render harness.
//!
//! Unlike `profile_docs_render`, which measures the one-time cold-start cost
//! of building the embedded [`autumn_io::site_docs`] registry, this measures
//! the *steady-state* per-request cost every `/docs/{slug}` hit pays: the
//! registry is already built (as it is in the running server, since
//! `SITE_DOCS` is a [`std::sync::LazyLock`]), and only the public entry point
//! the `docs_page` handler calls — [`autumn_io::site::render_docs_page`] — is
//! exercised, once per embedded guide, simulating a visitor reading every
//! guide in the site once. That is a realistic distribution: a real crawl or
//! documentation read-through touches every page, in real page sizes, through
//! the exact function the deployed handler runs on every request.
//!
//! ```bash
//! cargo build --release --bin profile_docs_page_render
//!
//! valgrind --tool=callgrind --callgrind-out-file=callgrind.out \
//!     ./target/release/profile_docs_page_render
//! callgrind_annotate --threshold=99.9 callgrind.out
//!
//! valgrind --tool=dhat --dhat-out-file=dhat.out.json \
//!     ./target/release/profile_docs_page_render
//! ```

fn main() {
    let registry = autumn_io::site_docs().expect("embedded guides render");

    let mut rendered_bytes: usize = 0;
    for page in registry.pages() {
        let html = autumn_io::site::render_docs_page(registry, page).into_string();
        rendered_bytes += html.len();
    }

    // A home-page render too: it is the other page every visitor hits, and it
    // shares `document_head`'s per-request structured-data allocation.
    rendered_bytes += autumn_io::site::render_home_page(registry)
        .into_string()
        .len();

    println!(
        "pages={} rendered_bytes={rendered_bytes}",
        registry.pages().len()
    );
}
