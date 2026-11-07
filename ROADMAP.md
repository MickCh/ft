# Roadmap — `fxt`

Date: 2026-07-12. Ideas for growing `fxt` into a general-purpose file transformation tool, grouped by what each one costs in the current architecture.

## Done

Implemented in the order suggested below, one commit per item:

- ✅ **Per-operation column keys** — `--sort-key`, `--unique-key`. An operation with its own key no longer claims `--cols`, so a left-over `--cols` goes back to selecting columns.
- ✅ **Column lists and permutation** — `-C 1,3,5-7`; `-F , -C 3,1,2` is an `awk`-style projection. Reading honours the written order, writing normalizes. Plus `--output-delimiter`.
- ✅ **`LineOutcome`** — `LineTransform` is no longer 1:1 (`Keep` / `Replace` / `Expand` / `Drop`); `Pipeline` folds a line into `Lines`. Delivered `--wrap` and `--drop-empty`.
- ✅ **Quoted CSV** — `--quoted` (RFC 4180): a delimiter inside `"…"` no longer splits, a doubled quote escapes one, fields keep their quotes so a projection stays valid CSV.
- ✅ **Multiple input files** — `Vec<Input>`; streaming concatenates like `cat`, `--in-place` edits each file on its own (the batch edit).
- ✅ **`--backup` and `--dry-run`** — a copy kept before the swap, and a per-file "would change / unchanged" report that streams (`CompareWriter`) instead of buffering.
- ✅ **`--split-on`** — one row in, one row per piece out (the other half of `Expand`).
- ✅ **Aggregations** — `LineReducer`, the third engine abstraction: `--count`, `--sum`, `--avg`, `--min`, `--max`, optionally per `--group-by`. A summary replaces the rows it summarizes.
- ✅ **Stateful transforms** — `LineTransform::apply` takes `&mut self`, so `--number` can count the rows it emits. `--title-case` and `--squeeze` came free as `MapColumns` constructors.
- ✅ **`grep`-like exit codes and `--quiet`** — 0 matched / 1 nothing matched / 2 failed; `--quiet` stops at the first match and answers with the exit code alone.
- ✅ **`--join`** — the N→1 direction, a streaming `LineReducer` (`accept` takes the writer). The inverse of `--split-on`.
- ✅ **Reordering + reducers** — the reorder buffer drains through the same output path as streamed lines, so `--sort --join` folds sorted and `--sort --group-by` reports groups in sorted order.
- ✅ **Exact numbers** — `Number` (`Int(i128)` exact, `Float(f64)` otherwise, compared exactly across the two) replaces the bare `f64` in the numeric sort key and the summaries, so keys beyond 2^53 sort correctly and integer sums stay exact. *(2026-11-06 comparison)*
- ✅ **Multi-line CSV records** — under `--quoted` the engine reads records, not lines (`records::Records` joins lines while a quote is open), so a newline inside quotes stays in its field and `--rows`/`--number`/`--count` count records; only a quote never closed fails.
- ✅ **Quoted CSV values** — `ColumnSpan::select_value` reads a field's value without its quotes; numeric operations use it, so `"10"` sums and sorts as 10. Lexical ones still read the field as written, which keeps projections valid CSV. *(2026-11-06 comparison)*
- ✅ **Multi-line CSV boundary** — under `--quoted`, a line that leaves a quoted field open fails with exit code 2 instead of returning records cut in half. *(2026-11-06 comparison)*
- ✅ **`--grep-key`** — `--grep` gets a column range of its own, so `--cols` is free to select: `-F , -C 2 --grep-key 1 -g ERROR`. Unlike the sort and unique keys it addresses the line as read, since the predicate runs before the transforms. *(2026-11-06 comparison)*
- ✅ **Quoted `--group-by` keys** — `ColumnSpan::select_canonical` reduces each field to its value and quotes it again only where needed, so `"a"` and `a` are one group, printed as `a`, and the key stays valid CSV. Being unambiguous, it keeps `a,"b,c"` and `"a,b",c` apart. *(2026-11-06 comparison)*
- ✅ **UTF-8 BOM** — `encoding::open` takes the mark off every input before they are joined, so it no longer hides the first field; `--in-place` (and so `--dry-run`) writes it back, other output carries none. *(2026-11-06 comparison)*
- ✅ **Order of operations documented** — README section on the fixed pipeline and the shared `--cols` scope, with examples. *(2026-11-06 comparison)*
- ✅ **Renamed to `fxt`** — PowerShell aliases `ft` to `Format-Table`, which shadowed the binary.
- ✅ **Quoted `--unique` keys** — `--unique` keys on `select_canonical` too, so under `--quoted` `"a"` and `a` are duplicates, as they are one `--group-by` group.
- ✅ **Quoted text-sort keys** — the text sort reads `select_value` like the numeric one, so under `--quoted` `"b"` sorts between `a` and `c` instead of first, on its quote. With `--group-by`, `--unique` and the numeric sort, every key under `--quoted` now reads field values.
- ✅ **Regex backreferences** — `-P`/`--perl-regexp` compiles every regex with `fancy-regex` (backreferences, lookaround); the default stays the linear-time `regex`. Behind `pattern::Pattern`/`BytePattern`, so `--grep`, `--find`, `--from`/`--to` share it. A backtracking match can fail at run time, so `LinePredicate::matches`, `LineTransform::apply` and `PatternRange::admits` now return `io::Result` — the failure ends the run instead of reading as a miss. *(2026-11-06 comparison)*
- ✅ **Grapheme columns** — `--graphemes` counts char columns and `--wrap` widths in grapheme clusters (`unicode-segmentation`). A `ColumnSpan::Graphemes` resolves its columns to char ranges per line, as field mode does, so not one char helper in `text.rs` changed; ASCII lines without a CRLF borrow the ranges as they are. *(2026-11-06 comparison)*
- ✅ **More summaries** — `--median` (exact for integers: halves added back rather than a sum that may overflow), `--distinct`, `--first`, `--last`, per group too: four more `Aggregate`/`Accumulator` variants, no engine change. The text ones read `select_canonical`, like `--group-by`.
- ✅ **`--reverse-chars`** — one more `MapColumns` constructor (two: by chars, by grapheme clusters).
- ✅ **Padding** — `--pad-left`/`--pad-right` (`PadColumns`). Padding writes into each field separately, which neither the write ranges (adjacent fields merge into one stretch) nor the char helpers (an empty field resolves to nothing) could do, so a span now also resolves to *cells* (`ColumnSpan::cell_ranges`: every field of the span, empty ones as inverted ranges) and `text::map_cells` writes into those, empty ones included.
- ✅ **Lossy mode** — `--lossy` replaces what is not valid in the input encoding with U+FFFD instead of failing (`encoding::Malformed`). Other encodings are decoded with replacement; UTF-8 passes through unchecked and the engine repairs a row only when it processes it — and writes the original bytes when the pipeline left the row as it was and it streams straight out, so a pass-through, a filter or a missed replacement keeps them (`--lossy -i --dry-run` reports such a file unchanged). The output side stays strict: a character the output encoding lacks is still an error.
- ✅ **Encodings** — `--encoding <label>` decodes the inputs from any WHATWG encoding (`encoding_rs`), a UTF-16/UTF-8 BOM settles it by itself, and `--output-encoding` writes the output in one. `--in-place` writes each file back as it was read (BOM included) unless `--output-encoding` converts it. Undecodable input and unmappable output are errors, never replacements. Done at the edges (`encoding::open`, `EncodeWriter`), so the engine stays UTF-8 only.
- ✅ **Pattern-addressed rows** — `--from`/`--to`, `sed '/A/,/B/'` semantics (inclusive, end looked for from the next line, every block selected; `--to` alone like `sed 0,/B/`). Not a `RangeBound` variant after all: `RangeSpec` resolves up front, this cannot, so it is its own stateful `PatternRange`, told every line in order and intersected with `--rows`. Matched as bytes, so rows outside a block need not be UTF-8.
- ✅ **Property tests** — `proptest` modules in `columns.rs` and `text.rs` check the field splitter against the char-by-char one it replaced and against `str::split`, quoted values round-tripping through CSV, and the char helpers against a `Vec<char>` reference (`select(R)` put back into `delete(R)` rebuilds the line), on lines of multi-byte chars, quotes and multi-char delimiters. Deliberately broken versions of the byte walk and of the ASCII fast path were caught at once.
- ✅ **Benchmarks** — `benches/engine.rs` (`criterion`): the engine on 100k generated log and CSV rows, in memory, per operation; baseline under *Performance* below.
- ✅ **Completions, man page, help examples** — `--completions <shell>` (`clap_complete`) and `--man` (`clap_mangen`) are exclusive flags of the binary itself, so they work after a plain `cargo install`; `after_help` lists examples of the newer features.

## 1. Free slots: a new `LineTransform` / `LinePredicate` (no engine changes)

Each is one new type plus one branch in `compose::build_pipeline`:

- `--expand` / `--unexpand` — tabs ↔ spaces
- `--encode base64|url|hex` / `--decode …`, scoped to the column range
- New predicates: `--min-fields N`, `--max-len N`, a repeatable `--grep` with AND/OR

## 2. Still open

Nothing at the moment; see *Free slots* above for small additions.

## 3. From the 2026-11-06 comparison with grep/sed/sort/cut/awk

Open findings of an external black-box comparison (the fixed ones are under *Done*):

- ✅ **Partial output before an encoding error** — a streaming run writes the valid lines before failing on invalid UTF-8 (exit 2). Inherent to streaming; `--encoding` answers it for files in a known encoding, `--lossy` for the rest.
- ✅ **Performance** — `fxt` beat `sed` on replacement and `awk`/pipelines on combined operations, but trailed `grep` on plain filtering and `sort` on whole-line sorting (300k rows, Windows). Re-measured 2026-11-07 with 0.4.0 (same 300k rows, median of 11): `fxt` now leads all five — filter 38 ms vs `grep` 40, replace 135 vs `sed` 539, whole-line sort 182 vs `sort -s` 207, filter + replace + uppercase 48 vs `awk` 384, the same as a pipeline 49 vs 110. `cargo bench` now measures the engine in memory; the baseline (100k rows, release build, Windows, 2026-11-06) points at field mode rather than at `--grep` or the sort:

  | Case | Throughput |
  |---|---:|
  | pass-through | 1.58 GiB/s |
  | `-g ERROR` | 420 MiB/s |
  | `-C 10-20` | 366 MiB/s |
  | `-g -f -r --upper` | 279 MiB/s |
  | `-s` | 174 MiB/s |
  | `-f foo -r bar` | 159 MiB/s |
  | `-e -f … -r …` | 128 MiB/s |
  | `-F , --quoted -C 3,1` | 79 MiB/s |
  | `-u --unique-key 7` (fields) | 76 MiB/s |
  | `-F , -C 3,1` | 73 MiB/s |
  | `-s -n --sort-key 9` (fields) | 62 MiB/s |
  | `--group-by 1 --count --sum 3` | 38 MiB/s |

  First round of field-mode work (2026-11-06): `field_positions` walks bytes instead of decoding chars and stops once the fields asked for (plus one) are known, `byte_range` skips the char walk on ASCII lines, and `--group-by` allocates a key only for a new group. Release binaries on 1M rows, best of 7 interleaved runs, identical output:

  | Case | Before | After |
  |---|---:|---:|
  | `-F , -C 1` | 386 ms | 267 ms |
  | `-F , -C 3,1` | 457 ms | 365 ms |
  | `-d -F , -C 2` | 494 ms | 345 ms |
  | `-F , --sum 3` | 406 ms | 265 ms |
  | `--group-by 1 --count --sum 3` | 725 ms | 395 ms |
  | `-u --unique-key 7` | 703 ms | 327 ms |
  | `-s -n --sort-key 9` | 1156 ms | 796 ms |
  | `--count` (control) | 91 ms | 92 ms |

  A summary used to split the same line once per aggregate plus once for the key; it now splits it once (`SplitLine`), shared by the key and every aggregate: `--group-by 1 --sum 3` 555 → 341 ms, `--quoted` 629 → 395 ms, four aggregates 660 → 512 ms; summaries without a key, with nothing to share, lost ~10–16 ns a row (`--count` 88 → 98 ms). Aggregates over the same span used to read and parse its number once each; now once per distinct span (`NumberReadings`): `--sum 3 --avg 3 --min 3 --max 3` 546 → 350 ms, with `--group-by 1` 645 → 428 ms, single aggregates unchanged. Every field selection used to allocate its field positions, its resolved ranges and a copy of the result; now the ranges live inline (`SmallVec`, `SpanRanges`) and a contiguous selection only narrows the line (`LineOutcome::Slice`). 1M rows: `-F , -C 1` 258 → 138 ms, `--quoted -C 2` 342 → 207 ms, `--sum 3` 283 → 214 ms, `--group-by 1 --count --sum 3` 364 → 260 ms, `-C 1-6` 128 → 86 ms (pass-through speed); a permutation still joins new text. a literal `--find` was slower than `--grep` by more than its writing explained — fixed: a line without the literal is now kept uncopied, and a match is written straight into the rebuilt line (1M rows: no match 374 → 243 ms, which is pass-through's 228 ms; 1 row in 10 382 → 259 ms; every row 396 → 366 ms). What remains against `grep` is output volume: a replacement writes every row, a filter only the matches. A text sort used to copy every line into its key (`sort_by_cached_key`) and compare strings that live all over the heap; now a key is borrowed from its line, carries its first eight bytes as a number, and keys are sorted with their positions (ties broken by position, so the unstable sort stays stable), the lines moved once at the end. 300k rows: `-s` 250 → 178 ms, `-s --reverse` 240 → 174 ms, by a quoted CSV field (1M rows) 500 → 439 ms; numeric sorts unchanged. Borrowing alone gained nothing; the time went to comparisons missing the cache. Every buffered row then still had a `String` of its own; now a reorder buffer (`LineBuffer`) keeps all their text in one, with a small record per row that reordering moves instead, and the `~N` buffer is one `Vec<u8>` with row ends. 300k rows, best of 9 interleaved: `--tac` 126 → 111 ms, `--shuffle` 148 → 125, `-s` 229 → 209, `-R ~1000-~1` 46 → 33, a quoted CSV field sort (1M rows) 566 → 499; streaming unchanged. In-process `cargo bench` numbers swing by ±20% on this machine between runs, so compare binaries (best of several interleaved runs) before trusting a change.

## 4. Tool maturity (cheap, highly visible)

- ✅ Fuzzing the range parser — property tests in `cli_args::cli` (`properties`): arbitrary and hostile input never panics and only spec characters get through (`+2` and `~+1` used to be accepted, via `usize::from_str`; this property now holds that out); generated specs, written out, select exactly the rows their definition names; column lists normalize to what they cover. Property tests rather than `cargo fuzz`, so they run in CI on every platform.
