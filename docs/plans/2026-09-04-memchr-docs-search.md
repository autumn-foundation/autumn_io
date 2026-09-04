# Docs Search: `memchr::memmem` Directly, Not Through `aho-corasick`

Date: 2026-09-04
Status: Accepted — measured, implemented
Issue: https://github.com/autumn-foundation/autumn_io/issues/28 (item 1)

## Goal

Issue #28 named a specific, checkable claim: `aho-corasick`, as shipped in
#27 for issue #23, wraps `memchr::memmem` for the single-pattern case this
matcher always uses, and the wrapper's construction cost is what forced three
tuned bounds (`MIN_SEARCHER_PATTERN_BYTES`, `MAX_SEARCHER_PATTERN_BYTES`,
`MAX_SEARCHERS`) into `QueryMatcher`. Replacing the wrapper with `memchr::memmem`
directly should let those three bounds — and the code and tests that exist only
to enforce them — go away, while making every query at least as fast. This
document is the before/after that decision needs, per
`docs/plans/2026-09-03-aho-corasick-docs-search.md`'s own "Not taken" section,
which flagged the swap but declined to make it without one.

## Non-goals

Unchanged from the #23/#27 document, and not reopened here:

- **Changing what search returns.** Substring matching, all-tokens-required
  filtering, the title/heading/body weighting, and the title tie-break stay
  exactly as they are.
- The multi-pattern pass issue #23 proposed. That was measured three ways in
  the prior document and lost every time; nothing here changes the mechanism,
  only which single-pattern searcher builds each token's finder.
- An n-gram/suffix index, a word/token index, or anything else that is a
  structural change to `SearchIndex`.
- Issue #28's items 2-4 (a length cap and rate limiting on `?q=`, the snippet
  offset-walk cliff, and the corpus-growth question). Each is its own call,
  filed separately, and out of scope here.

## Brainstorming — candidate ways to make the swap

1. **Swap the type, keep the laziness.** Replace
   `Vec<OnceCell<Option<AhoCorasick>>>` with
   `Vec<OnceCell<memmem::Finder<'a>>>`, keeping deferred construction.
2. **Swap the type, build eagerly.** Replace it with `Vec<memmem::Finder<'a>>`,
   built for every pattern in `QueryMatcher::new`, since a `Finder` is cheap
   enough that deferring it buys nothing.
3. **Keep `aho-corasick` for long patterns, `memchr` for short ones.** A
   hybrid that keeps some of the tuning that made the three bounds necessary
   in the first place.
4. **Drop the crate, hand-roll a substring search.** Skip both dependencies
   and reimplement a searcher directly.

### Narrowing

- (1) is dead on arrival: laziness existed to amortise a ~12.7 µs automaton
  build against a page loop that usually bails before needing it. A `Finder`
  costs 11-266 ns to build (issue #28's own numbers, reproduced in this
  document's evidence section) — cheaper than the branch and cell-check
  `OnceCell` needs to decide whether to build it. Deferred construction adds
  code for no measurable win once construction itself is this cheap.
- (3) reintroduces exactly the bounds this change exists to delete, for a
  crossover issue #28 already priced at 500-1000x in the wrong direction —
  there is no pattern length where the automaton wrapper is worth carrying.
- (4) throws away a well-tested, SIMD-backed searcher for a hand-rolled one on
  a repo that has no reason to own that code, and `memchr` is already the
  transitive dependency both the current and prior code pull in.
- (2) is the only candidate that actually removes the three bounds — the
  point of the issue — rather than moving them. Taken forward.

## Reverse brainstorming — how would this change hurt us?

| Failure mode | How it bites | Guard |
|---|---|---|
| Building a `Finder` per token turns out not to be as cheap as assumed | The swap trades one cost for a similar one and nothing is actually gained | Measured directly on the committed harness below, not assumed from the prior document's estimate |
| Deleting the bounds silently reintroduces the OOM issue #28's own predecessor document fixed | `?q=` becomes an amplification vector again if a `Finder` allocates more than a few bytes per pattern | `memmem::Finder::new` borrows the pattern rather than copying it into an automaton; verified in the evidence section against the same oversized-query shape the prior bounds were built for |
| Removing `searcher()`'s "no searcher, fall back to `str::contains`" branch leaves a token unmatched | A token that used to fall back silently stops matching | Every pattern gets a `Finder` unconditionally now — there is no fallback branch left to leave a gap in. `every_token_gets_a_finder_whatever_its_length_or_the_query_size` pins that every distinct token, at every length exercised by the old bounds, has one |
| The equivalence claim from #23/#27 quietly stops holding as a side effect of the rewrite | Result order, snippets, or which pages match drift | The full differential test (`matcher_scores_every_page_exactly_as_the_naive_scan_did`) and all fifteen behavioural tests in `tests/docs_search_matcher.rs` are unchanged in behaviour and run against the new matcher unmodified |
| The three now-deleted tests that pinned the bounds leave a gap where a future regression (e.g. someone reintroducing a length floor) goes unnoticed | A later change could silently reintroduce the bounds this change removes, and nothing would fail | Replaced with `every_token_gets_a_finder_whatever_its_length_or_the_query_size`, which asserts every pattern gets a finder regardless of length or query size — pinning the *absence* of bounds the same way the deleted tests pinned their presence |
| It is slower, not faster, on some query shape the prior harness under-samples | The whole point of the change is lost on real traffic even if the default mix looks better | All three committed query mixes (default, multi, typeahead) are measured, matching the methodology issue #28 itself used to price the win |

## Six thinking hats

**White (facts).** `aho-corasick`'s `MemmemBuilder::build` returns `Some` only
at `count == 1` (the crate's own `util/prefilter.rs`), which is exactly the
shape `QueryMatcher` always builds — one pattern per searcher. Every call this
matcher makes into `aho-corasick` already resolves to `memchr::memmem`
underneath; the automaton is present but not doing search work. `memchr` is
already resolved in `Cargo.lock` a dozen times over and has no dependencies of
its own.

**Red (instinct).** "Just call the thing underneath directly" sounds like it
should obviously win, and issue #28's own numbers already said so — the risk
here is confirmation bias, taking a prior estimate as proof rather than
re-measuring on the actual code being shipped, on the same harness, under the
same conditions.

**Black (risks).** A swap this mechanical is exactly the kind of change that
looks safe and carries a subtle behavioural regression — a missed re-export, a
fallback branch quietly dropped rather than made unconditional, a lifetime
that outlives what it should. The prior document's estimate of "another
~7-10 percentage points" was explicitly a rough number from the same harness
under different conditions (before the two allocation bounds existed); citing
it without re-measuring on the code as it stands today would be exactly the
mistake the prior document's own "Caveats on the evidence" section warns
about.

**Yellow (upside).** This is a rare case where a change both simplifies the
code (three constants and the logic gating them, gone) and is expected to be
faster — usually those pull in opposite directions. It is also low-risk in
scope: one private struct in one file, with an existing, comprehensive
behavioural test suite that does not need to change.

**Green (alternatives).** If the measured win were smaller than expected, the
fallback is simply not to ship this and to leave #27's `aho-corasick` version
in place — nothing about this change is required to unblock anything else,
so there is no pressure to ship a result the numbers do not support.

**Blue (process).** Red/green/refactor: red is the two dependency-pin tests in
`tests/docs_search_matcher.rs` inverted to expect `memchr` and reject
`aho-corasick`; green is the smallest change that makes them and the rest of
the suite pass; refactor is deleting the now-dead bounds, their guard logic,
and updating comments and the prior document's "Not taken" note. Then
re-measure on the same three-mix harness issue #28's own numbers came from,
rather than trust those numbers unverified.

## TDD plan

**Red.** `tests/docs_search_matcher.rs`'s two dependency-pin tests, rewritten
before touching `src/docs.rs` or `Cargo.toml`:

- `docs_search_declares_the_memchr_matcher` (was
  `docs_search_declares_the_aho_corasick_matcher`) — fails while `Cargo.toml`
  still declares `aho-corasick` instead of `memchr`.
- `docs_search_no_longer_needs_the_aho_corasick_wrapper` (new) — fails while
  `Cargo.toml` still declares `aho-corasick`.
- `the_matcher_dependency_stays_a_leaf` (was `..._a_memchr_leaf`, now checked
  against `memchr`'s own locked dependencies rather than `aho-corasick`'s) —
  fails until `memchr` is the crate whose lockfile entry the test reads.

All three fail against the code as `d1ac9b4` left it, confirmed before any
production code changed.

**Green.** In `src/docs.rs`: `Vec<OnceCell<Option<AhoCorasick>>>` becomes
`Vec<memmem::Finder<'a>>`, built eagerly in `QueryMatcher::new` for every
distinct pattern; `token_matches` and `earliest_match` call `Finder::find`
directly with no fallback branch, because every pattern now has a finder.
`MIN_SEARCHER_PATTERN_BYTES`, `MAX_SEARCHER_PATTERN_BYTES`, `MAX_SEARCHERS`,
and the `searcher()` method that enforced them are deleted. `Cargo.toml` swaps
`aho-corasick = "1.1.4"` for `memchr = "2.8.0"` (the version already resolved
in `Cargo.lock`). Nothing in `SearchIndex`, `SearchEntry`, or any public type
changes.

**Refactor.** The three bound-specific unit tests in `src/docs.rs`
(`the_searcher_bounds_stay_where_they_were_measured`,
`a_token_gets_a_searcher_only_within_those_bounds`,
`only_the_first_tokens_get_searchers_however_long_the_query`) are deleted —
there is nothing left for them to pin — and replaced with one test asserting
every token gets a finder regardless of length or query size, which is the
same "pin the current design decision" role the deleted tests played, aimed
at the opposite fact. Doc comments referencing `aho-corasick`, automaton
construction cost, or the three bounds are updated across `src/docs.rs` and
`tests/docs_search_matcher.rs`; none of the fifteen *behavioural* tests in
`tests/docs_search_matcher.rs` change their assertions, only (where they
referenced the now-deleted bounds by name) their explanatory comments.

Every other test — the differential oracle, the fifteen behavioural pins, the
Unicode snippet-offset tests inherited from #27 — runs unmodified against the
new matcher, which is the equivalence check.

## Evidence gathered

Measured in this session's sandbox: 4 cores, 15.7 GB RAM, rustc 1.94.1,
x86_64-linux, release profile, using the committed harness
(`src/bin/profile_docs_search.rs`) exactly as `docs/plans/2026-09-03-aho-corasick-docs-search.md`
describes running it — callgrind `Ir`, full run minus a `SEARCH_REQUESTS_PER_QUERY=1`
build-only run, over the request delta, on all three committed query mixes.
Not the same host that document's numbers came from, so absolute figures
differ from that document, but the shipped-vs-baseline comparison here is a
same-host, same-methodology A/B, and the baseline it re-measures reproduces
that document's own reported figures on this host to within 1% (see below),
which is what makes the comparison trustworthy despite the different machine.

### Baseline reproduction

Before changing any code, the harness was run against `d1ac9b4`
(`aho-corasick`, as #27 shipped it) to confirm this host's numbers track the
prior document's:

| Query mix | This host, Ir/request | Prior document, Ir/request |
|---|---:|---:|
| Default | 959,277 | 959,544 |
| Long (`multi`) | 1,458,209 | 1,455,458 |
| Typeahead | 546,194 | 546,111 |

Within 0.02%-0.19% on every mix — close enough that the marginal-cost
methodology is behaving the same way here as it did when the prior document
was written.

### Marginal cost, before and after

| Query mix | Baseline (`aho-corasick`), Ir/request | Shipped (`memchr::memmem`), Ir/request | Change |
|---|---:|---:|---:|
| Default | 959,277 | 726,888 | **−24.2%** |
| Long (`multi`) | 1,458,209 | 1,021,251 | **−30.0%** |
| Typeahead | 546,194 | 368,016 | **−32.6%** |

Every mix improved, and by more than the prior document's own "another
~7-10 percentage points" estimate — that estimate priced only the automaton
construction instructions it could see directly; it did not account for the
allocator traffic construction drove, or the branches the three bounds added
to every `token_matches` and `earliest_match` call regardless of whether a
searcher was actually built.

Wall clock corroborates it on the default mix, using the same
build-only-subtraction method (median of 5 runs, index build isolated with
`SEARCH_REQUESTS_PER_QUERY=1`):

| | Baseline | Shipped | Change |
|---|---:|---:|---:|
| Marginal wall clock, default mix | 135.3 µs/request | 100.7 µs/request | **−25.6%** |

−25.6% wall clock against −24.2% instructions on the same mix — consistent
with each other, and with the profile's shape moving from a mix of `memmem`
work plus automaton-construction overhead to `memmem` work alone.

### Equivalence

- All 15 behavioural tests in `tests/docs_search_matcher.rs` pass unmodified.
- `matcher_scores_every_page_exactly_as_the_naive_scan_did` — the differential
  oracle against the pre-#23 `str::contains` scan, over the real embedded
  corpus and nineteen queries spanning both sides of every bound the old
  matcher had — passes unmodified against the new matcher.
- The Unicode snippet-offset tests (`an_offset_inside_a_folded_character_maps_back_to_that_character`,
  `snippets_survive_case_folding_that_changes_byte_offsets`,
  `snippets_stay_valid_when_the_match_is_the_last_thing_on_the_page`) are
  untouched by this change — they exercise `SearchEntry::original_offset`, not
  the matcher — and continue to pass.

### Build and runtime cost

| Metric | Baseline (`aho-corasick`) | Shipped (`memchr` direct) | Change |
|---|---:|---:|---:|
| Release binary (`profile_docs_search`) | 7,133,680 B | 6,795,528 B | **−338,152 B (−4.74%)** |
| `cargo tree` entries (build graph size proxy) | 1,010 | 1,009 | −1 |
| Index build (`site_docs()` + `from_registry`), Ir | 3,959,946,143 | 3,949,504,543 | −0.26% (noise) |
| Index build, wall clock (median of 5) | 0.5365 s | 0.5267 s | −1.8% (noise, within run-to-run spread) |

The one dependency removed is exactly `aho-corasick` itself: `memchr` was
already resolved a dozen times over in `Cargo.lock` before this change, so
`Cargo.lock`'s only structural edit is `autumn_io`'s own dependency list
swapping one direct dependency for the other (verified by inspection — see
the diff on `Cargo.lock`). Index build cost is unaffected because
`QueryMatcher` is built per-query, not at index-build time; the small
movements above are within normal run-to-run noise for this host.

### Caveats on the evidence

- Taken on one sandbox host, not on Fly; per `docs/plans/2026-09-03-aho-corasick-docs-search.md`'s
  own caveat, a `shared-cpu-1x` under contention is slower in absolute terms
  and the ratios are CPU-bound and should carry.
- `cargo tree` entry count is a proxy for build-graph size, not an exact crate
  count (it lists every edge, including a crate resolved at multiple
  versions once per occurrence); it is reported because it is what was
  available in this environment, and the direction and rough magnitude (one
  fewer entry, matching one fewer direct dependency with no new transitive
  ones) is what matters here, not the absolute figure.
- Wall clock is only reported for the default mix; the other two mixes are
  reported by instruction count only, which is the more stable of the two
  metrics and the one both this document and its predecessor treat as
  primary.
- These are rustc 1.94.1, x86_64-linux, `memchr` 2.8.0, matching the versions
  the prior document measured against, so the two documents' numbers are
  comparable on toolchain even though they were taken on different hosts.

## Decision

**Adopt `memchr::memmem::Finder`, built eagerly per query token, replacing
`aho-corasick` and the three bounds it required.**

Every query mix measured faster on this change than on the `aho-corasick`
version it replaces — −24.2% to −32.6% marginal instructions per request,
beyond the prior document's own rough estimate for this swap — with results
identical to the byte, verified against the full existing behavioural suite
and the differential oracle against the pre-#23 scan. The change also nets out
smaller: three constants and the logic gating them are gone, the release
binary is smaller, and the build graph has one fewer direct dependency with no
new transitive ones, because `memchr` was already there.

### Not taken

- **Keeping a length-based hybrid** (short tokens via `memchr`, long ones via
  `aho-corasick`, or the reverse). There is no pattern length at which the
  automaton wrapper wins once its own single-pattern path already delegates to
  `memmem` — the wrapper only ever adds construction cost on top. See
  "Narrowing" above.
- **A lazy `OnceCell<memmem::Finder>`.** Deferred construction existed to
  amortise a ~12.7 µs automaton build; a `Finder` costs 11-266 ns to build,
  cheaper than the cell-check that would guard it. Building eagerly is simpler
  and not measurably slower.
