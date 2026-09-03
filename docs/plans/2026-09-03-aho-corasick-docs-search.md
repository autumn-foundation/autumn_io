# Docs Search: Multi-Pattern Matching for the Per-Request Scan

Date: 2026-09-03
Status: Accepted — measured, implemented
Issue: https://github.com/autumn-foundation/autumn_io/issues/23

## Goal

Decide, with evidence, how to cut the per-request cost of
`SearchIndex::search` — the path behind `/api/search`, the `/search` box and
the MCP `search_autumn_docs` tool — without changing which pages match or in
what order.

Issue #23 profiled it and found:

- ~96% of the **marginal cost of one request** is substring search
  (`<&str as Pattern>::is_contained_in` 50.91%, `TwoWaySearcher::next` 25.34%,
  `simd_contains` 3.44% — 79.7% of a run whose other 20% is the one-time
  index build).
- The cost is `O(pages × tokens × corpus bytes)` per request: `SearchEntry::score`
  calls `str::contains` once per query token, per field, per page, over all
  140 guides.
- dhat confirms the shape: 37.2 GB read across 10,000 requests, ~3.7 MB per
  request, with allocations flat. It is a scan-bound cost.

The issue deliberately stopped short of a PR because both fixes were human
calls: a word/token index would change search semantics, and the
semantics-preserving option needed a new dependency. This document is the
call on the second one, made against measurements.

## Non-goals

- **Changing what search returns.** Substring matching (a token matches inside
  a longer word), all-tokens-required filtering, the title/heading/body
  weighting and the title tie-break all stay exactly as they are. That
  constraint is what ruled the token index out in #23, and it rules here too.
- Adding an index structure (suffix array, n-gram index) or a search engine.
- Changing the docs-search UI or its result limit.

## Brainstorming — candidate levers

Everything considered, before narrowing:

1. **One automaton per query, run once per field per page** — issue #23's own
   recommendation: fold the per-token passes over the same text into one pass.
2. **A word/token index at startup** (`HashMap<word, Vec<page>>`) — O(1) per
   token, but whole-word semantics.
3. **An n-gram or suffix index at startup** — preserves substring semantics,
   turns the scan into a lookup, but is a new data structure on a public type.
4. **A cheap per-page rejection filter** — a bloom filter over each page's
   3-grams, tested before scanning. Exact (no false negatives), skips whole
   pages for rare tokens.
5. **Concatenate the corpus into one haystack** and search it once per query,
   mapping match offsets back to pages — removes 140x the per-call overhead.
6. **Replace the searcher, not the algorithm** — `str::contains` is not the
   fastest substring search available; `memchr::memmem` is the same algorithm
   class with better SIMD.
7. **Cache recent queries.** A search-as-you-type box repeats prefixes.
8. **Cap the corpus scanned** — search titles and headings first, only fall
   through to bodies when there are too few hits.
9. **Score lazily** — filter with a cheap pass, score only survivors.
10. **Parallelise across pages** (rayon).

### Narrowing

- (10) is dead on arrival: the Fly VM is `cpus = 1`, `cpu_kind = 'shared'`.
- (2) changes which pages match. #23 rejected it for that reason and this
  change is not the place to revisit it — it is a product decision about
  search quality, not a profiler's.
- (8) silently degrades results; (7) is a cache, which trades correctness
  windows and memory for a hit rate we have no traffic data to predict.
- (3) and (4) are real and larger. They attack *whether* the scan happens at
  all rather than how fast it is, and they are the right answer if the corpus
  grows another order of magnitude. Both are structural changes to
  `SearchIndex`; recorded here as the fallback path, not attempted.
- (9) is already what the code does — `score` bails on the first token that
  matches nothing.
- (5) is a variant of (1) and was measured with it.
- (1) and (6) are the two mechanical, semantics-preserving levers. Measure
  both; (1) is what the issue asked for, and (6) is the honest control that
  says how much of any win is actually attributable to multi-pattern matching.

## Reverse brainstorming — how would this change hurt us?

Asking "how do we make this fail?" turns directly into the test list. The
Guard column distinguishes a *standing test* (fails in CI or `cargo test`
forever after) from something *verified once* during this evaluation. The
table was written before the measurements, when a multi-pattern pass was still
the expected shape of the change; the rows about it are left as written, with
the Guard column updated to say what actually protects us now.

| Failure mode | How it bites | Guard |
|---|---|---|
| A non-overlapping multi-pattern search hides one token inside another token's match | `categor ego` silently stops matching a page containing `category` — a wrong result, not a slow one | Standing test `search_counts_tokens_that_overlap_another_tokens_match`. The shipped matcher searches one token at a time and cannot hit this at all; the test stays because the next person to reach for a multi-pattern pass will |
| The matcher deduplicates patterns and the score inherits the dedup | Repeated query tokens quietly stop double-counting, changing ranking | Standing test `search_scores_a_repeated_token_once_per_occurrence`, built so dedup flips the result order |
| Case folding narrows to ASCII (`ascii_case_insensitive` is the obvious knob) | `café` stops finding `CAFÉ` | Standing test `search_folds_case_beyond_ascii` |
| A long query overflows a fixed-width pattern set | Tokens silently dropped, or a panic on a user-supplied query | Standing test with 70 distinct tokens. The shipped matcher has one searcher per token and no fixed width, and builds them lazily so a pathological query cannot pay for searchers nothing consults |
| A searcher cannot be built at runtime | An `expect` on a user-supplied query, or a fallback that changes results | `token_matches` falls back to the `str::contains` it replaced, which is exactly right and merely slower; no `unwrap` on the query path |
| Ranking or limit changes as a side effect | Result order drifts across a release | Standing tests on weights, tie-break and limit; differential test against a naive reference over the real corpus |
| It is slower, not faster | The whole point of the change is lost | Instruction counts on the committed harness, before and after, on two query mixes |
| It is faster on the harness only because the harness is unrepresentative | We optimise a benchmark, not the product | Two query mixes (1-2 token and 3-5 token) measured separately, and the change made every individual scan cheaper rather than trading one query shape for another |
| A new dependency inflates the build | This repo has been bitten twice by builder OOM (#9, #16) | `aho-corasick` is pure Rust with one dependency (`memchr`) and no build script; standing lockfile test pins that, and build cost measured below |
| Snippet offsets drift from match offsets | The snippet shows the wrong part of the page | Standing tests on case folding that changes byte length (this was a live bug — see below) |

## Six thinking hats

**White (facts).** Substring search is ~96% of the marginal per-request cost.
The corpus is 140 guides, ~2.3 MB of Markdown, ~2.8 MB of extracted body text,
rebuilt only on deploy. Queries come from a search-as-you-type box, so they are
short: the issue's own harness is 13 single-token and 7 two-token queries out
of 20. `aho-corasick` is already resolved in `Cargo.lock` (via `regex`, an
optional path not currently compiled) and depends only on `memchr`.

**Red (instinct).** "Aho-Corasick is faster" is a reflex, not a measurement.
Multi-pattern automata earn their keep on *many* patterns; a two-token query
is not many, and a SIMD single-substring search is very hard to beat. The
suspicion going in was that the issue's recommendation would lose — and every
measurement below says it does.

**Black (risks).** Silent result changes are worse than slowness, and the
overlapping-match trap produces exactly that. A per-query automaton moves
construction cost onto every request. `packed`'s Teddy searcher is
SIMD-dependent and may not build. And a dependency added for a win that turns
out to be noise is a straight loss.

**Yellow (upside).** The lever is 96% of the per-request cost, and unlike the
cold-start render it scales with traffic *and* with the corpus. The change is
contained to one private type in `src/docs.rs`. The dependency is a pure-Rust
leaf, and it is the crate the issue named.

**Green (alternatives).** If the automaton loses, the same crate still offers
the SIMD packed searcher, and the same measurement harness can price a
single-pattern searcher swap as a control. Whatever wins, keep the harness
committed and the semantics pinned by tests so the next person can re-run the
comparison in minutes rather than rebuilding it. (This is what happened: the
control won, and the harness plus this document are what make that
re-checkable.)

**Blue (process).** Red/green/refactor, with the numbers as the arbiter of
design rather than the shape of the recommendation. Red: tests that fail
today — the dependency pin, and the snippet-offset bug found while reading the
scan path. Green: the smallest implementation that passes. Refactor: tidy
without moving the numbers. Then re-measure on the committed harness and put
both query mixes in the PR.

## TDD plan

**Red.** `tests/docs_search_matcher.rs` (dependency pins and semantics) and
one differential test in `src/docs.rs` (private internals). Three tests fail
before the change:

1. `docs_search_declares_the_aho_corasick_matcher`.
2. `snippets_survive_case_folding_that_changes_byte_offsets`.
3. `snippets_stay_valid_when_the_match_is_the_last_thing_on_the_page`.

(2) and (3) are a live bug, not a hypothetical: snippets are cut from the
original-cased text at offsets found in its lowercased copy, and
`str::to_lowercase` is not length-preserving (`İ` is 2 bytes and folds to 3).
It is in the function this change rewrites, so it is fixed here rather than
left in a rewritten routine.

The other eleven tests pass before *and* after. They are the equivalence
claim — substring matching, overlapping tokens, repeated tokens, all-tokens
filtering, weights, tie-break, limit, Unicode folding, oversized queries — and
are the actual protection against a "performance" change that quietly moves
results.

**Green.** One matcher per query, plus the offset fix. Nothing else. What the
matcher does inside is left to the measurements rather than decided here.

**Refactor.** Comments and structure only; re-run the suite and the harness.

## Evidence gathered

All numbers from one sandbox host (4 cores, 15.7 GB), rustc 1.94.1,
x86_64-linux, release profile. Instruction counts are callgrind `Ir` on
`src/bin/profile_docs_search.rs`, the issue's harness, now committed.

The baseline reproduces issue #23 to within 0.003%: 22,664,268,503
instructions against its 22,663,760,507, with the same three substring frames
at 79.69% of the run.

### Step 1 — the recommendation, measured as written

Issue #23 recommended building one `AhoCorasick` per query over its tokens and
running it once per field per page. Two forms of it were measured against the
default query mix, as *marginal cost per request* — the 10,000-request run
minus the build-only run, over the request delta:

| Variant | Ir per request | vs baseline |
|---|---:|---:|
| Baseline (`str::contains` per token) | 1,874,089 | — |
| `MatchKind::Standard` + `find_overlapping_iter` | 10,438,734 | **5.6x worse** |
| `MatchKind::LeftmostFirst` + `find_iter` + exact re-check | ~9,868,000 | **5.3x worse** |

(Both rows are prototypes carrying a variant switch, so treat them as "several
times worse" rather than to five figures. The second row's build-only baseline
is the shared index build rather than its own run.)

The profile says why. With two patterns, `aho-corasick`'s prefilter selection
falls to its rare-byte/start-byte heuristic — the Teddy SIMD searcher is only
considered from three distinct start bytes up — and on English prose those
"rare" bytes are common enough that the automaton is re-entered constantly:
`try_find_fwd` at 27.28% and the memchr2-based prefilters another 45.85% of
the run.

### Step 2 — where multi-pattern matching actually pays

Wall-clock cost of one scan of the 2.3 MB corpus, by token count. The
single-pattern rows are per token, so the whole-query cost is that figure
times the number of tokens actually reached; the one-automaton rows cover the
whole token set in one pass. Source in the appendix.

| Searcher | 1 token | 2 tokens | 4 tokens |
|---|---:|---:|---:|
| `str::contains`, per token | 0.199 ms | — | — |
| `AhoCorasick` (auto), one automaton | 0.152 ms | 3.776 ms | 0.228 ms |
| `AhoCorasick` (`NoncontiguousNFA`), one automaton | 0.113 ms | — | — |
| `packed::Searcher` (Teddy), one pass | 0.334 ms | 0.258 ms | 0.178 ms |
| `memchr::memmem`, per token | 0.094 ms | — | — |

Two things fall out. A single-pattern `AhoCorasick` defers internally to
`memmem` and beats `str::contains` outright — 0.113 ms against 0.199 ms — and
that is where the win is for the queries a search box actually sends. And the
crossover for the multi-pattern pass is around three or four distinct tokens
*if every token were scanned on every page*, which is the assumption step 3
goes on to test against the real filter.

### Step 3 — where the multi-pattern pass was given its best shot

The obvious rescue is the SIMD (Teddy) searcher, used directly rather than as
a prefilter the automaton may decline to pick. It was implemented properly:
`packed::Searcher` with leftmost-first semantics, one pass per field, gated on
a token set whose matches provably cannot overlap each other — so no
re-check pass is needed and the single non-overlapping pass is exact — and
only from three distinct tokens up, where the microbenchmark said one pass
beats three.

Measured on the long-query mix, A/B on otherwise identical shipped code:

| Variant (3-5 token queries) | Ir per request | vs baseline |
|---|---:|---:|
| Baseline (`str::contains` per token) | 2,824,408 | — |
| Per-token searchers **plus** the multi-pattern pass | 2,466,700 | −12.7% |
| Per-token searchers alone (shipped) | 1,422,423 | **−49.6%** |

The multi-pattern pass is **73% more expensive** than not having it, on the
query shape it exists for. The mechanism is in the code it was meant to
replace: `score` drops a page the moment one token is missing, so on most
pages only the *first* token is ever scanned, and the second and third are
never looked for at all. One pass that always reads the whole body for every
token therefore replaces, on average, rather less than one pass — and it reads
at a higher cost per byte than a single-pattern SIMD search does.

That is the answer to the question issue #23 asked. The multi-pattern
mechanism it proposed does not pay here, and it was left out rather than
shipped as a slower path nobody would want taken.

### Step 4 — what shipped

One `aho-corasick` searcher per distinct query token, built once for the query
and consulted token by token, keeping the existing bail. Marginal cost per
request, against the same baseline:

| Query mix | Baseline | Shipped | Change |
|---|---:|---:|---:|
| Default (1-2 tokens, the issue's own mix) | 1,874,089 | 916,101 | **−51.1%** |
| Long (3-5 tokens) | 2,824,408 | 1,422,423 | **−49.6%** |

Against issue #23's impact floor of 5%, both clear it by an order of
magnitude.

Wall clock says the same thing, and slightly more of it. Median of five runs
of the whole harness process, with the index build (0.556 s before, 0.542 s
after — noise) subtracted to leave the request time:

| Query mix | Baseline | Shipped | Change |
|---|---:|---:|---:|
| Default | 292 µs/request | 130 µs/request | **−55.3%** |
| Long | 474 µs/request | 206 µs/request | **−56.5%** |

Time falls a little further than instructions do, which is the opposite of
what #19 found for Oniguruma and for the same reason inverted: the
instructions that replaced the old ones are AVX2 substring-search
instructions, which retire more work each.

The shape of the profile changes accordingly. `str::contains` and its two-way
searcher are gone; the top frame is now `memchr`'s AVX2 `packedpair::Finder`
at 50.27% of the run. Substring search is still the dominant per-request cost
— it is a linear scan, and this change makes the scan cheaper rather than
removing it — but it is 79.1% of a marginal cost that is half what it was,
against 96.4% before.

One line item is worth naming because it is the price of this particular
crate: `NFA::init_full_state`, the per-query automaton construction, is
345.8 M instructions across the run, ~34.6 K per request, or 3.8% of the new
marginal cost. `memchr::memmem` builds its finder for a fraction of that (see
"Not taken" below).

### Results, unchanged

Equivalence was checked three ways rather than asserted:

- The harness reports identical totals on both query mixes before and after —
  78,500 hits and 13,958,500 bytes of snippet on the default mix, 20,000 and
  3,512,000 on the long one. Snippet bytes matching to the byte is a stronger
  statement than hit counts matching: it says the same pages came back in the
  same order with the same text cut from the same offsets.
- `matcher_scores_every_page_exactly_as_the_naive_scan_did` scores all 140
  real pages against the replaced `str::contains` loop, kept in the test
  module as an oracle, over fifteen queries chosen to cover both engines,
  overlapping tokens, repeated tokens, empty results and non-ASCII input.
- Fourteen behavioural tests in `tests/docs_search_matcher.rs` pin the
  semantics themselves, and passed before the change as well as after.

### Build and runtime cost

| Metric | Before | After | Change |
|---|---:|---:|---:|
| Crates in the build graph | 367 | 368 | +1 |
| Release binary | 37,144,616 B | 37,494,424 B | +349,808 B (+0.94%) |
| Index build (`site_docs()` + `from_registry`), Ir | 3,923,378,303 | 3,944,466,740 | +0.54% |
| Index build, wall clock (median of 5) | 0.556 s | 0.542 s | noise |
| Harness peak RSS | 30,456 KB | 30,672 KB | +216 KB |

`memchr` was already in the graph five times over (`regex-automata`, `syntect`
and others), so `aho-corasick` adds exactly one crate, with no build script
and no C toolchain — which is what makes it a different proposition from #19's
`onig_sys`, and why the builder-OOM history (#9, #16) does not apply here.

The index build pays 21 M more instructions for the new per-page check of
whether lowercasing preserves byte offsets. That is 0.54% of a one-time cost
this repo has already spent a release optimising, in exchange for a snippet
bug that was previously live.

## Decision

**Adopt `aho-corasick`, as one single-pattern searcher per query token. Do not
adopt the multi-pattern pass issue #23 proposed.**

The dependency question the issue raised is answered yes: it is a pure-Rust
leaf, it adds one crate and ~350 KB of binary, no build script, and it halves
the per-request cost of the workload the issue profiled — 2.05x on its own
query mix and 1.99x on a longer one, with results identical to the byte.

The mechanism question is answered no, and that is the more interesting half.
Multi-pattern matching was implemented three ways and measured on both query
mixes, and each was slower than the per-token scan it was meant to replace —
between 12.7% and 5.6x, worst where the automaton had to fall back on
`aho-corasick`'s byte-frequency prefilter heuristics. The reason is a property
of *this* search, not of the crate: a page is dropped as soon as one token is
missing, so most pages are only ever scanned for the first token, and there is
far less repeated scanning to fold away than the nested loop suggests. Folding
it costs a higher per-byte scan on every page in exchange for passes that were
mostly never happening.

The snippet-offset fix rides along because it is in the routine this change
rewrites, and because leaving a known wrong-offset bug inside newly rewritten
code is worse than the small extra diff.

### Not taken

- **`memchr::memmem` directly.** It is what `aho-corasick` calls for a single
  pattern, without the automaton wrapper or its per-query construction. The
  same harness put it at another ~7-10 percentage points beyond what shipped.
  It is a second dependency decision, not this one, and #23 named
  `aho-corasick`; recorded here with numbers so whoever wants it has them.
- **An n-gram or suffix index** (options 3 and 4 in the brainstorm). These stop
  the scan happening at all rather than making it cheaper, and are the right
  answer if the corpus grows an order of magnitude. They are a structural
  change to `SearchIndex` and want their own issue.
- **A whole-word token index.** Still a product decision about search quality,
  exactly as #23 said. Unchanged by any of this.

## Caveats on the evidence

- Every number was taken on one sandbox host (4 cores, 15.7 GB), not on Fly.
  A `shared-cpu-1x` under contention is slower in absolute terms; the ratios
  are CPU-bound and should carry.
- Instruction counts are stable per binary but move with toolchain and
  dependency versions, which this repo does not pin. These are rustc 1.94.1,
  x86_64-linux, `aho-corasick` 1.1.4, `memchr` 2.8.0.
- The two rejected `AhoCorasick`-automaton rows in step 1 were measured on
  prototypes carrying a variant switch, so their exact figures are worth
  slightly less than the rest. The multi-pattern row in step 3 is a clean A/B
  on the shipped code, changing one constant.
- Wall clock is median of five runs per side on an otherwise idle host; the
  spread within each set of five is ±5%, which is smaller than every
  difference reported but not smaller than the index-build difference, which
  is why that one is called noise rather than an improvement.
- The query mixes are constructed, not sampled from traffic — there is no
  traffic instrumentation on `/api/search` to sample. They are drawn from real
  guide topics, and the long mix exists precisely because the default one
  cannot speak for token counts a search box rarely produces. A real query
  distribution could shift the size of the win; it would have to be very
  strange indeed to shift its sign, since the change makes every individual
  scan cheaper rather than trading one shape of query for another.
- `SearchEntry::original_offset`'s walk is exercised by tests, not by the
  corpus: every guide today is offset-stable, so the fast path is what
  production takes. That is the point of the tests, and of the harness
  reporting identical snippet bytes.

## Appendix: the microbenchmark behind step 2

Not committed — no fix follows from it directly, and it exists to price
searchers rather than to guard anything, so it is recorded here in the shape
issue #23 used for its own repro. It lowercases every guide's Markdown into a
`Vec<String>`, then times 2,000 passes of each searcher over that corpus,
counting the pages that match:

```rust
// src/bin/bench_scan.rs
let pages: Vec<String> = registry.pages().iter().map(|p| p.markdown.to_lowercase()).collect();

// str::contains, one token
pages.iter().filter(|page| page.contains(token)).count()

// aho-corasick, one automaton over `tokens`
let ac = AhoCorasick::builder().match_kind(MatchKind::LeftmostFirst).build(tokens)?;
pages.iter().filter(|page| ac.is_match(page.as_str())).count()

// aho-corasick, cheapest automaton kind, one token
let ac = AhoCorasick::builder().kind(Some(AhoCorasickKind::NoncontiguousNFA)).build([token])?;
pages.iter().filter(|page| ac.is_match(page.as_str())).count()

// packed (Teddy) SIMD searcher over `tokens`
let mut builder = packed::Config::new().match_kind(packed::MatchKind::LeftmostFirst).builder();
for token in tokens { builder.add(token.as_bytes()); }
let searcher = builder.build()?;
pages.iter().filter(|page| searcher.find(page.as_bytes()).is_some()).count()

// memchr::memmem, one token
let finder = memmem::Finder::new(token.as_bytes());
pages.iter().filter(|page| finder.find(page.as_bytes()).is_some()).count()
```
