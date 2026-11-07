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
- ✅ **Quoted CSV values** — `ColumnSpan::select_value` reads a field's value without its quotes; numeric operations use it, so `"10"` sums and sorts as 10. Lexical ones still read the field as written, which keeps projections valid CSV. *(2026-11-06 comparison)*
- ✅ **Multi-line CSV boundary** — under `--quoted`, a line that leaves a quoted field open fails with exit code 2 instead of returning records cut in half. *(2026-11-06 comparison)*
- ✅ **`--grep-key`** — `--grep` gets a column range of its own, so `--cols` is free to select: `-F , -C 2 --grep-key 1 -g ERROR`. Unlike the sort and unique keys it addresses the line as read, since the predicate runs before the transforms. *(2026-11-06 comparison)*
- ✅ **Quoted `--group-by` keys** — `ColumnSpan::select_canonical` reduces each field to its value and quotes it again only where needed, so `"a"` and `a` are one group, printed as `a`, and the key stays valid CSV. Being unambiguous, it keeps `a,"b,c"` and `"a,b",c` apart. *(2026-11-06 comparison)*
- ✅ **UTF-8 BOM** — `bom::strip_bom` takes the mark off every input before they are joined, so it no longer hides the first field; `--in-place` (and so `--dry-run`) writes it back, other output carries none. *(2026-11-06 comparison)*
- ✅ **Order of operations documented** — README section on the fixed pipeline and the shared `--cols` scope, with examples. *(2026-11-06 comparison)*
- ✅ **Renamed to `fxt`** — PowerShell aliases `ft` to `Format-Table`, which shadowed the binary.
- ✅ **Quoted `--unique` keys** — `--unique` keys on `select_canonical` too, so under `--quoted` `"a"` and `a` are duplicates, as they are one `--group-by` group.
- ✅ **Quoted text-sort keys** — the text sort reads `select_value` like the numeric one, so under `--quoted` `"b"` sorts between `a` and `c` instead of first, on its quote. With `--group-by`, `--unique` and the numeric sort, every key under `--quoted` now reads field values.
- ✅ **Pattern-addressed rows** — `--from`/`--to`, `sed '/A/,/B/'` semantics (inclusive, end looked for from the next line, every block selected; `--to` alone like `sed 0,/B/`). Not a `RangeBound` variant after all: `RangeSpec` resolves up front, this cannot, so it is its own stateful `PatternRange`, told every line in order and intersected with `--rows`. Matched as bytes, so rows outside a block need not be UTF-8.
- ✅ **Property tests** — `proptest` modules in `columns.rs` and `text.rs` check the field splitter against the char-by-char one it replaced and against `str::split`, quoted values round-tripping through CSV, and the char helpers against a `Vec<char>` reference (`select(R)` put back into `delete(R)` rebuilds the line), on lines of multi-byte chars, quotes and multi-char delimiters. Deliberately broken versions of the byte walk and of the ASCII fast path were caught at once.
- ✅ **Benchmarks** — `benches/engine.rs` (`criterion`): the engine on 100k generated log and CSV rows, in memory, per operation; baseline under *Performance* below.
- ✅ **Completions, man page, help examples** — `--completions <shell>` (`clap_complete`) and `--man` (`clap_mangen`) are exclusive flags of the binary itself, so they work after a plain `cargo install`; `after_help` lists examples of the newer features.

## 1. Free slots: a new `LineTransform` / `LinePredicate` (no engine changes)

Each is one new type plus one branch in `compose::build_pipeline`:

- `--pad-left N` / `--pad-right N` — pad the column range to a fixed width
- `--expand` / `--unexpand` — tabs ↔ spaces
- `--encode base64|url|hex` / `--decode …`, scoped to the column range
- `--reverse-chars` (like `rev`)
- New predicates: `--min-fields N`, `--max-len N`, a repeatable `--grep` with AND/OR

## 2. Still open

- **Encodings** — non-UTF-8 input is a hard error today. Add `--encoding latin1|utf16`, or a lossy/byte-oriented mode.
- **More reducers** — `--median`, `--distinct` (count of distinct keys), `--first`/`--last` per group. Now just implementations of an abstraction that exists.

## 3. From the 2026-11-06 comparison with grep/sed/sort/cut/awk

Open findings of an external black-box comparison (the fixed ones are under *Done*):

- **Multi-line CSV records** — a newline inside quotes is now rejected; actually reading such a record needs the engine to iterate over *records* rather than lines when `--quoted` is set (a record reader in front of `process_line`, with row numbers counting records). The largest item here.
- **Grapheme columns** — columns count code points, so `e\u0301` and ZWJ emoji sequences can be split. An opt-in `--graphemes` (`unicode-segmentation`) would count user-perceived characters; it touches every char-indexed helper in `text.rs`.
- **Regex backreferences** — `(.)\1` is rejected by the `regex` crate. `fancy-regex` behind a flag would allow it, at the cost of linear-time matching guarantees.
- **Partial output before an encoding error** — a streaming run writes the valid lines before failing on invalid UTF-8 (exit 2). Inherent to streaming; the *Encodings* item above (a lossy/byte mode) is the real answer.
- **Performance** — `fxt` beat `sed` on replacement and `awk`/pipelines on combined operations, but trailed `grep` on plain filtering and `sort` on whole-line sorting (300k rows, Windows). `cargo bench` now measures the engine in memory; the baseline (100k rows, release build, Windows, 2026-11-06) points at field mode rather than at `--grep` or the sort:

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

  A summary used to split the same line once per aggregate plus once for the key; it now splits it once (`SplitLine`), shared by the key and every aggregate: `--group-by 1 --sum 3` 555 → 341 ms, `--quoted` 629 → 395 ms, four aggregates 660 → 512 ms; summaries without a key, with nothing to share, lost ~10–16 ns a row (`--count` 88 → 98 ms). Still open: aggregates over the same span still parse its number once each; every field selection allocates the resolved ranges and the joined result; a literal `--find` was slower than `--grep` by more than its writing explained — fixed: a line without the literal is now kept uncopied, and a match is written straight into the rebuilt line (1M rows: no match 374 → 243 ms, which is pass-through's 228 ms; 1 row in 10 382 → 259 ms; every row 396 → 366 ms). What remains against `grep` is output volume: a replacement writes every row, a filter only the matches. In-process `cargo bench` numbers swing by ±20% on this machine between runs, so compare binaries (best of several interleaved runs) before trusting a change.

## 4. Tool maturity (cheap, highly visible)

- Fuzzing the range parser
