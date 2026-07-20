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

## 3. Tool maturity (cheap, highly visible)

- `clap_complete` — shell completions
- `clap_mangen` — a man page
- Examples in `after_help`
- Benchmarks (`criterion`) over large files
- Property tests (`proptest`), e.g. `select(R) + delete(R) == input`
- Fuzzing the range parser
