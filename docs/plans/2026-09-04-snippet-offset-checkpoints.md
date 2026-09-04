# Docs Search: Checkpoint Table for the Snippet Offset Walk

Date: 2026-09-04
Status: Accepted — implemented
Issue: https://github.com/autumn-foundation/autumn_io/issues/34 (filed as item 3 of #28)

## Goal

Issue #34 named a specific, checkable latent cost: `SearchEntry::original_offset`
walks `text` character by character, from the start of the page, whenever
`lowercasing_preserves_offsets` found that `str::to_lowercase` does not map
`text` to `text_lower` byte-for-byte. No guide trips this today — the corpus
has 66 distinct non-ASCII characters and none whose lowercase changes byte
length — but the walk runs on the request path (once per hit with a body
match, on every search), so the day a guide adds an `İstanbul` or a `ẞ`, every
search that returns that page pays for a full-page scan, per hit, silently:
the issue measured 0.022 ms → 1.81 ms per search on a 162 KB page.

The fix the issue asks for: precompute a checkpoint table (or the full offset
map) at `SearchEntry::from_page` time — which already runs once at startup,
not per request — for the pages that need it, so the request-path walk is
bounded rather than proportional to page length.

## Non-goals

- **Changing what search returns.** Every existing behavioural test in
  `tests/docs_search_matcher.rs` and `src/docs.rs` must keep passing
  unmodified; this is a request-path performance fix, not a semantics change.
- **The matcher itself** (`QueryMatcher`, `memchr::memmem::Finder`,
  `MAX_FINDERS`). Untouched — issue #28's items 1 and 2 already have their own
  documents and are not reopened here.
- **A length cap or rate limit on `?q=`** (issue #28 item 2) and **a
  structural index for corpus growth** (issue #28 item 4). Separate calls,
  filed separately, out of scope here.
- **Changing how `text_lower` is computed** (still plain `str::to_lowercase`).
  A lowering scheme engineered to always preserve offsets would change match
  semantics (`STRASSE` vs `Straße`, already pinned by
  `search_folds_case_beyond_ascii`) for a request-path win this fix gets
  without touching it.

## Brainstorming — candidate ways to bound the walk

1. **A checkpoint table.** Precompute pairs of (offset into `text_lower`,
   offset into `text`) every *N* characters, at `from_page` time, only for
   pages where `lowercasing_preserves_offsets` is false. `original_offset`
   binary-searches the table for the closest checkpoint at or before the
   target, then walks forward at most *N* characters from there.
2. **The full offset map.** One (lower, original) pair per character, for the
   same pages. Simpler — no bounded walk needed, only the binary search (or a
   direct index if keyed by character count) — at the cost of one entry per
   character rather than one per *N*.
3. **Memoize per request.** Cache the walk's result the first time a given
   `lower_index` is asked for. Doesn't help: each hit asks for a different
   offset (wherever its own match landed), so nothing is ever asked for twice
   within a request, and the cache would need to persist across requests to
   pay for itself, which reintroduces the "when does it get invalidated"
   question for no benefit over precomputing at build time instead.
4. **Carry offsets through from the start.** Restructure `html_to_plain_text`
   and `SearchEntry::from_page` so `text_lower` is never a separate
   `String` needing a walk back at all — build the offset mapping as a
   side effect of the single pass that already exists. Structurally larger:
   touches the HTML-stripping pass and the two-string representation
   `SearchEntry` has had since before issue #23, for a narrow issue that does
   not ask for it.
5. **Do nothing until a guide needs it**, per the issue's own "or if someone
   would rather not leave a trapdoor" alternative. Rejected: the whole point
   of filing #34 rather than leaving it in #28 is that leaving a
   silently-tripped cliff in is exactly the kind of assumed-safe corner #28's
   own review (the `Finder`-size correction in
   `docs/plans/2026-09-04-memchr-docs-search.md`) already found costly once.
   The fix is cheap; there is no reason to wait for an incident to justify it.

### Narrowing

- (3) is dead on arrival: it doesn't bound anything, since a request's hits
  each ask for their own distinct offset once.
- (4) is out of scope: a bigger, riskier diff than the issue calls for, and
  nothing about it is required to close #34 — worth reconsidering only if
  issue #28 item 4 (structural index for corpus growth) is ever picked up.
- (5) is declined for the reason above — cheap to fix now, and "later" is
  exactly the failure mode the issue is warning about.
- (2) bounds the *lookup* cost (binary search) but not the *memory* cost: a
  page that is pathologically all length-changing characters would carry one
  entry per character — for the corpus's largest real page (`deployment.md`,
  153 KB raw) that is tens of thousands of 16-byte pairs if every character
  qualified, dozens of KB in the realistic case where only a handful do.
  Bounded, but proportional to page length in the worst case, which is the
  exact shape of cost this issue exists to remove.
- (1) bounds both: the table itself is `page length / N` entries, and the
  walk any single lookup does is at most `N` characters, whatever the page's
  total length. Taken forward, with `N` as `OFFSET_CHECKPOINT_STRIDE`.

## Reverse brainstorming — how would this change hurt us?

| Failure mode | How it bites | Guard |
|---|---|---|
| An off-by-one in the checkpoint-then-walk math gives a wrong offset at a stride boundary | A snippet silently starts one character early/late, or a search on a page that needs the walk returns a subtly wrong snippet | `original_offset_matches_a_brute_force_walk_at_every_checkpoint_boundary` checks the checkpoint-based result against an independent, un-optimized walk-from-zero oracle at *every* byte offset across a fixture engineered to span several strides, not just the offsets a sample query happens to hit |
| Checkpoints get built for every page, not only the ones that need the walk, quietly reintroducing per-page cost at startup for the corpus's 140-odd pages that never use them | Index build slows down and every `SearchEntry` carries a table it never reads | `offset_checkpoints_are_precomputed_only_for_pages_that_need_the_walk` pins that the table is empty whenever `lower_offsets_match` is true, over both the real embedded corpus and a fixture that needs the walk |
| `partition_point` is called against a checkpoint vector that isn't actually sorted/partitioned (a future edit reorders how checkpoints are pushed) | Binary search returns a checkpoint that isn't actually the closest one at or before the target, corrupting every offset past that point | Checkpoints are only ever appended in one forward pass over `text.chars()` with monotonically increasing `lower`/`original`, and the same brute-force differential test above would fail immediately if this ever went wrong |
| The first checkpoint is missing or `offset_checkpoints` is empty on a page where `lower_offsets_match` is false, and `partition_point(..) - 1` underflows | A panic on every hit against that page, worse than the perf cliff this change removes | `build_offset_checkpoints` unconditionally pushes `(0, 0)` first, and `0 <= lower_index` holds for every `usize`, so `partition_point` can never return `0`; both new tests exercise pages that take this branch and would panic immediately if it were wrong |
| The stride is picked too small (table nearly as big as the full map) or too large (walk barely bounded, most of the old cost survives) | Either the memory saving or the request-path saving this change is for is mostly given back | `OFFSET_CHECKPOINT_STRIDE` is documented with the reasoning for its value; not hyper-tuned, since no guide exercises this path today, but chosen so neither the table nor the walk it leaves behind is unbounded |
| The fast path (`lower_offsets_match == true`, every guide today) picks up cost it didn't have before | Search on the entire real corpus gets slower to fix a case nothing in it hits | The fast-path branch (`if self.lower_offsets_match { return lower_index; }`) and the skip in `from_page` that leaves `offset_checkpoints` empty are both unchanged in shape from before this fix; the existing profiling harness re-run below on the real corpus is the check |

## Six thinking hats

**White (facts).** `SearchEntry::original_offset` already short-circuits for
every page in the current corpus (`lower_offsets_match` is `true` everywhere
real). The slow branch exists, is exercised only by three fixture-built tests
today, and walks `self.text.chars()` from index zero on every call. The
corpus's largest real page (`deployment.md`) is 153 KB of raw Markdown,
consistent with the issue's own 162 KB rendered-page test case.

**Red (instinct).** A cost that "no guide trips today" but would silently
regress a real request path the day someone writes about İstanbul is the
kind of thing that's easy to defer indefinitely — right up until it isn't,
at which point it looks like an incident rather than a known, filed,
already-fixable issue. Worth closing now precisely because it's cheap and
nobody is blocked on not doing it.

**Black (risks).** The main way to make this worse rather than better is a
checkpoint/walk arithmetic bug that is *wrong* rather than merely *slow* — and
because the table would only ever be exercised by the same handful of tests
that exercise the walk today, a subtle bug here could sit uncaught the same
way the cost itself did. That is exactly why the new test is a brute-force
differential oracle over every offset in range, not an assertion on a couple
of hand-picked positions.

**Yellow (upside).** The fix is small (one field, one builder function, one
call-site rewrite), touches nothing about scoring or matching, and only adds
cost to the pages that would otherwise pay the O(page length) walk — which is
none of them today. It closes a documented trapdoor for the cost of a
few dozen lines and a differential test.

**Green (alternatives).** If a checkpoint table had turned out to be
meaningfully harder to get right than the full offset map, the fallback was
the full map (candidate 2) — simpler, still bounded, just not as tightly on
memory. Not needed: the checkpoint table's extra complexity is one
`partition_point` call and a bounded loop, not a structural difference.

**Blue (process).** Red/green/refactor, same as the prior two docs in this
directory: red is `entry.offset_checkpoints` failing to compile (the field
doesn't exist yet) plus the brute-force oracle test written against the
intended behaviour; green is the smallest struct field, builder function, and
`original_offset` rewrite that makes both new tests and every existing test
pass unmodified; refactor is doc comments explaining the stride and the
checkpoint invariant, and a look at whether `lowercase_len` and the walk body
can be shared between the builder and the lookup rather than duplicated (see
"Refactor" below for the conclusion).

## TDD plan

**Red.** Two new tests in `src/docs.rs`'s test module, written against the
field and behaviour this change adds before touching production code:

- `offset_checkpoints_are_precomputed_only_for_pages_that_need_the_walk` —
  fails to compile (`offset_checkpoints` does not exist on `SearchEntry`).
- `original_offset_matches_a_brute_force_walk_at_every_checkpoint_boundary` —
  same compile failure, plus a from-scratch `naive_original_offset` oracle
  (a copy of the pre-fix algorithm) checked against `entry.original_offset`
  at every byte offset across a fixture long enough to span several
  checkpoint strides.

Both fail (to compile) against the code as this session found it, confirmed
before any production code changed.

**Green.** In `src/docs.rs`:

- `SearchEntry` gains `offset_checkpoints: Vec<(usize, usize)>`.
- `SearchEntry::from_page` builds it via a new `build_offset_checkpoints`
  function when `lowercasing_preserves_offsets` is false, and leaves it
  `Vec::new()` (no allocation) when true.
- `original_offset` keeps its fast-path return unchanged, and replaces the
  from-zero walk with: binary search the checkpoint table via
  `Vec::partition_point` for the closest checkpoint at or before the target,
  then run the same bounded walk forward from there instead of from zero.

**Refactor.** Doc comments on the new field, the new constant
(`OFFSET_CHECKPOINT_STRIDE`), and `original_offset` itself explain the
invariant (checkpoints are monotonically increasing, the first is always
`(0, 0)`, so the binary search can never underflow) and why the stride is
what it is. No existing test's assertions change.

On sharing the walk body: `build_offset_checkpoints`'s per-character update
(`lower += lowercase_len(character); original += character.len_utf8();`) and
`original_offset`'s walk share that same two-line update, but not the loop
around it — the builder always runs to the end of the page and periodically
records a checkpoint, while the lookup stops as soon as it passes
`lower_index`. Factoring the shared two lines into a closure or helper would
trade two short, obviously-correct loops for one more indirection to save
duplicating an update that cannot drift out of sync unnoticed (the brute-force
oracle test would catch it immediately if it ever did). Left as two loops;
the test module's independent third copy (`naive_original_offset`) is
deliberately *not* shared with either, since its entire purpose is to be an
oracle that doesn't share a bug with the code it checks.

## Evidence gathered

Every pre-existing test in `src/docs.rs` and `tests/docs_search_matcher.rs`
passes unmodified, including the three that already exercised the slow branch
(`an_offset_inside_a_folded_character_maps_back_to_that_character`,
`snippets_survive_case_folding_that_changes_byte_offsets`,
`snippets_stay_valid_when_the_match_is_the_last_thing_on_the_page`) — none of
their assertions changed, and all still pass against the checkpoint-based
implementation.

The real corpus is unaffected either way: `lowercasing_preserves_offsets` is
`true` for every guide today, so `from_page` never calls
`build_offset_checkpoints` and `original_offset`'s fast path never changes.
`cargo run --release --bin profile_docs_search` (all three query mixes)
before and after this change is therefore a no-op comparison by construction;
what changes is the previously-untested-for-performance branch, measured
directly below with a synthetic fixture the same shape as the issue's own
(a large page where lowering does not preserve offsets), since nothing in the
committed harness's corpus can exercise it.

### Synthetic before/after, matching the issue's own repro shape

Measured with a purpose-built, throwaway harness (`src/bin/bench_offset_walk.rs`,
not committed — deleted again once these numbers were recorded, since unlike
`profile_docs_search.rs` there is no committed corpus page for it to run
against). A single synthetic page close to the issue's own 162 KB test case
turned out too cheap in isolation on this host to show the effect clearly (a
from-zero walk over ~150,000 mostly-ASCII characters is itself only tens of
microseconds here, swamped by the rest of a search's cost) — so, matching the
issue's own note that the cost "scal[es] with `limit` (up to 50 on
`/search`)", the harness instead builds **50** such pages (all `İstanbul`,
mixed with ordinary words, ~162 KB of body each) through the same
`SearchIndex::from_registry` path the real corpus uses, and searches for a
token that only appears once per page, right at the end — the worst case for
a from-zero walk, and enough matches to fill `limit = 50` and force a
snippet (and therefore an `original_offset` call) on every page. Median of 11
runs, release profile, same sandbox host as
`docs/plans/2026-09-04-memchr-docs-search.md`'s measurements; the "before"
number was taken by stashing `src/docs.rs` back to the pre-fix version
(this harness only calls the public `SearchIndex` API, so it runs unmodified
against either side) and restored immediately after:

| | Before (from-zero walk) | After (checkpoint table, stride 64) |
|---|---:|---:|
| Per-search wall clock, 50×162 KB pages | 22.14 ms | 1.08 ms |

A **~20.5x** reduction in the whole search, most of which is the walk itself:
the "after" figure is consistent with matching alone (`memchr` scanning
~8.1 MB of lowercased text across 50 pages, independent of this change and
unaffected by it), which puts the walk's own marginal cost at roughly
21 ms ÷ 50 pages ≈ 420 µs per page before this fix, against a bound of
`O(log(page length / 64) + 64)` — next to nothing — after it. That per-page
figure is the same order of magnitude as the issue's own 1.81 ms ÷ up to 50
hits ≈ tens of µs to low ms per page, on different hardware and a different
exact fixture; both point at the same shape of cost removed.

### Equivalence

- `original_offset_matches_a_brute_force_walk_at_every_checkpoint_boundary`
  checks the new code against a from-scratch, unoptimized oracle at every
  byte offset (not just the ones a sample query lands on) across a fixture
  spanning multiple checkpoint strides — this is the test that would catch a
  boundary bug, and it does so structurally rather than by picking positions
  by hand.
- All other Unicode/offset tests listed above are unmodified and pass.

### Caveats on the evidence

- The synthetic-fixture wall-clock numbers are from one sandbox host, not
  Fly, and are a single-fixture measurement rather than the three-mix
  committed harness — there is no committed corpus page for the harness to
  measure this branch against, which is the entire premise of the issue
  (nothing in the real corpus trips it). The order-of-magnitude match to the
  issue's own figure is what this evidence is for, not a precise number.
- The "before" and "after" numbers both include the same base per-search
  matching cost (`memchr` scanning every page's lowercased text, unrelated to
  this change), so the ~20.5x figure understates the walk's own improvement;
  the per-page walk-cost estimate above (~420 µs → near-zero) isolates it
  more directly, but is itself derived rather than measured in isolation.
- `OFFSET_CHECKPOINT_STRIDE = 64` is a reasonable, documented choice, not a
  swept-and-tuned one — there is no live traffic on this path to tune
  against, and the issue's ask is "bounded", not "optimal".

## Decision

**Precompute a checkpoint table in `SearchEntry::from_page`, only for pages
where lowering does not preserve offsets, and have `original_offset`
binary-search it instead of walking from the start of the page.**

This removes the O(page length) term from the snippet-offset request path for
the pages that would otherwise pay it, at zero added cost to every page in
today's corpus (the table is never built, and the walk never runs, when
`lowercasing_preserves_offsets` is true). Verified against every existing
correctness test unmodified, plus a new brute-force differential oracle at
every offset across a fixture spanning multiple checkpoint strides, and
against a synthetic reproduction of the issue's own before/after shape.

### Not taken

- **The full offset map** (one entry per character). Bounds the lookup but
  not the memory, which scales with page length in the same way the original
  bug's *time* cost did. See "Narrowing" above.
- **Carrying offsets through from `html_to_plain_text`.** A larger,
  structural change to how `SearchEntry` represents text, out of scope for
  this issue and not needed to close it.
- **Deferring the fix "until a guide needs it."** The issue's own alternative,
  declined because the fix is cheap and the whole point of filing it
  separately was to close the trapdoor rather than wait for it to be tripped.
