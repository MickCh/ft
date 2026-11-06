# Roadmap — `ft`

Date: 2026-07-12. Ideas for growing `ft` into a general-purpose file transformation tool, grouped by what each one costs in the current architecture.

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

## 1. Free slots: a new `LineTransform` / `LinePredicate` (no engine changes)

Each is one new type plus one branch in `compose::build_pipeline`:

- `--pad-left N` / `--pad-right N` — pad the column range to a fixed width
- `--expand` / `--unexpand` — tabs ↔ spaces
- `--encode base64|url|hex` / `--decode …`, scoped to the column range
- `--reverse-chars` (like `rev`)
- New predicates: `--min-fields N`, `--max-len N`, a repeatable `--grep` with AND/OR

## 2. Still open

- **Pattern-addressed rows** — `--from /START/ --to /END/` (a range like `sed`'s). Fits as a new `RangeBound` variant, but must be resolved while streaming rather than via `resolve(total)`. The largest remaining design item.
- **Encodings** — non-UTF-8 input is a hard error today. Add `--encoding latin1|utf16`, or a lossy/byte-oriented mode.
- **More reducers** — `--median`, `--distinct` (count of distinct keys), `--first`/`--last` per group. Now just implementations of an abstraction that exists.

## 3. From the 2026-11-06 comparison with grep/sed/sort/cut/awk

Open findings of an external black-box comparison (the fixed ones are under *Done*):

- **Multi-line CSV records** — a newline inside quotes is now rejected; actually reading such a record needs the engine to iterate over *records* rather than lines when `--quoted` is set (a record reader in front of `process_line`, with row numbers counting records). The largest item here.
- **Quoted `--unique`/`--sort` keys** — `--group-by` now compares field values, but `--unique` and the text sort still compare fields as written, so `"a"` and `a` are two rows to `--unique`. `select_canonical` is ready for them; it is a question of whether that surprises more than it helps.
- **Grapheme columns** — columns count code points, so `e\u0301` and ZWJ emoji sequences can be split. An opt-in `--graphemes` (`unicode-segmentation`) would count user-perceived characters; it touches every char-indexed helper in `text.rs`.
- **Regex backreferences** — `(.)\1` is rejected by the `regex` crate. `fancy-regex` behind a flag would allow it, at the cost of linear-time matching guarantees.
- **Partial output before an encoding error** — a streaming run writes the valid lines before failing on invalid UTF-8 (exit 2). Inherent to streaming; the *Encodings* item above (a lossy/byte mode) is the real answer.
- **Performance** — `ft` beat `sed` on replacement and `awk`/pipelines on combined operations, but trailed `grep` on plain filtering and `sort` on whole-line sorting (300k rows, Windows). Worth a `criterion` look at the predicate path and at `sort_by_cached_key` cloning every key.

## 4. Tool maturity (cheap, highly visible)

- `clap_complete` — shell completions
- `clap_mangen` — a man page
- Examples in `after_help`
- Benchmarks (`criterion`) over large files
- Property tests (`proptest`), e.g. `select(R) + delete(R) == input`
- Fuzzing the range parser
