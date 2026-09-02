//! Cold-start docs-render harness from issue #19.
//!
//! Renders every embedded guide once through the real pipeline —
//! `autumn_io::site_docs()`, the same public entry point `build_site` and the
//! server's first `/docs/{slug}` request use — so the one-time cost a
//! scale-to-zero cold boot pays can be profiled directly.
//!
//! ```bash
//! cargo build --release --bin profile_docs_render
//!
//! # Instructions. Attribution needs symbols, so do not add `strip` to
//! # `[profile.release]` without expecting hex addresses here.
//! valgrind --tool=callgrind --callgrind-out-file=callgrind.out \
//!     ./target/release/profile_docs_render
//! callgrind_annotate --threshold=99.9 callgrind.out
//!
//! # Allocations. Read the totals off valgrind's own summary, or sum `tb`
//! # (bytes), `tbk` (blocks) and `gb` (live at peak) over `pps` in the JSON;
//! # `ftbl` resolves the frames. `dh_view.html` from the valgrind install
//! # loads the same file.
//! valgrind --tool=dhat --dhat-out-file=dhat.out.json \
//!     ./target/release/profile_docs_render
//!
//! # Wall clock and peak RSS, five runs, take the median. `/usr/bin/time -v`
//! # does the same job where it exists.
//! for i in $(seq 5); do python3 -c "import subprocess,resource,time; \
//!     t=time.perf_counter(); \
//!     subprocess.run(['./target/release/profile_docs_render'],check=True); \
//!     print(time.perf_counter()-t, \
//!           resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss)"; done
//! ```
//!
//! To measure the other backend, set `syntect`'s feature back to
//! `default-fancy` in `Cargo.toml` and rebuild. Note that
//! `syntect_uses_the_oniguruma_regex_backend` in
//! `tests/syntax_highlighting_backend.rs` fails while that flip is in place —
//! it is pinning exactly this choice, and the flip is expected to be temporary.
//!
//! Figures quoted in `docs/plans/2026-09-02-syntect-regex-backend.md` were
//! taken this way on rustc 1.94.1, x86_64-linux. Instruction counts are stable
//! per binary but move with toolchain and dependency versions, which this repo
//! does not pin.

fn main() {
    let registry = autumn_io::site_docs().expect("embedded guides render");
    let pages = registry.pages().len();
    let html_bytes: usize = registry.pages().iter().map(|p| p.html.len()).sum();
    let markdown_bytes: usize = registry.pages().iter().map(|p| p.markdown.len()).sum();
    println!("pages={pages} html_bytes={html_bytes} markdown_bytes={markdown_bytes}");
}
