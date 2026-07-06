# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

`ft` (File Transformer) is a CLI tool for transforming text files: selecting or deleting row/column ranges, reordering rows (sort/tac/shuffle), content filtering (`--grep`), deduplication (`--unique`), case/whitespace transforms (`--upper`/`--lower`/`--trim`), find/replace within a column range, and in-place editing. Row ranges support lists, open-ended and end-relative (`~N`) bounds; column ranges can address delimited fields (`--fields`). It is also a learning project for idiomatic Rust — code quality and design matter as much as features.

## Commands

```bash
cargo build                     # build
cargo test                      # run all tests
cargo test test_line_replace    # run a single test by name
cargo clippy --all-targets -- -D warnings   # lint (CI fails on any warning)
cargo fmt                       # format (rustfmt.toml: chain_width 40)
cargo run -- --help             # run the CLI
cargo run -- -R 1-5 -C 2-4 -f foo -r bar input.txt   # example invocation
cargo run -- -F , -C 2-3 data.csv                    # field (delimiter) mode
cargo run -- -R '~10-~1' input.txt                   # end-relative rows (tail)
```

CI (`.github/workflows/rust.yml`) runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo build` and `cargo test` on pushes/PRs to `main`.

## Architecture

Library crate (`src/lib.rs`) plus a thin binary (`src/main.rs`). Flow: argv → validated `Config` → transform pipeline + row predicate → streaming processor. Only `main.rs` touches the filesystem; the processor works on injected `BufRead`/`Write`, so tests run against in-memory `Cursor`/`Vec<u8>`. Input comes from a file, or stdin when the filename is omitted or `-` (`Config.filename: Option<PathBuf>`).

- `src/main.rs` — two output paths: `run_streaming` (file or stdout) and `run_in_place` (`--in-place`, writes to a `.<name>.ft-<pid>.tmp` sibling then atomically `rename`s it over the input, cleaning up the temp file on any error). `--in-place` requires a file input and conflicts with `--output`.
- `src/cli_args/` — everything between argv and a valid `Config`:
  - `cli.rs` — clap `Command` definition (name/version come from `crate_name!`/`crate_version!`) and range/delimiter parsing. Rows parse into a `RangeSpec` (comma-separated list of parts; each bound is `FromStart`/`FromEnd`, the latter written `~N`); columns parse into a single `RangeInclusive<usize>` and reject lists and `~`. Same-side inverted ranges are rejected here; mixed-bound inversion is only detectable after resolution.
  - `config.rs` — immutable `Config` built via `TryFrom<ArgMatches>`, which validates cross-argument rules (`--replace` requires `--find` and their counts must match; replace/delete are mutually exclusive; delete needs a row/column range or `--grep`; `--ignore-case` needs a pattern; `--in-place` needs a file). `rows: Option<RangeSpec>`, `cols: Option<RangeInclusive<usize>>`, `finds: Vec<FindPattern>` paired positionally with `replace_strings: Vec<String>`. `rows_or_full()`/`cols_or_full()` supply the `1..=usize::MAX` fallback; `col_span()` returns a `ColumnSpan` (chars, or fields when `--fields` is set); `has_column_operation()` decides whether a bare `--cols` selects (cuts) columns.
  - `config_error.rs` — `ConfigError` enum with `Display` for user-facing validation messages.
- `src/columns.rs` — `ColumnSpan` (`Chars` or `Fields { delimiter, fields }`); `char_range(line)` / `char_range_for_delete(line)` resolve a span to the 1-based char range it occupies on a concrete line, so field mode reuses the char-indexed helpers. Delete resolves to swallow one adjacent delimiter, like `cut`.
- `src/ranges.rs` — `RangeSet` (normalized, merged, possibly non-contiguous absolute ranges: `contains`, `end`) and `RangeSpec` (row ranges as written, possibly end-relative). `RangeSpec::is_absolute()` says whether it can resolve without the line count; `resolve(total)` maps every `RangeBound` to an absolute `RangeSet`, dropping empty parts.
- `src/transform.rs` — `LineTransform` trait and its implementations (`SelectColumns`, `DeleteColumns`, `UppercaseColumns`, `LowercaseColumns`, `TrimColumns`, `ReplaceInColumns`, `ReplaceInColumnsIgnoreCase`, `RegexReplaceInColumns`); `build_pipeline(&Config)` derives the pipeline once (delete-cols → bare-select → find/replace pairs in order → upper → lower → trim). A new per-line operation is a new transform here, not a new branch in the processing loop. Transforms take an `impl Into<ColumnSpan>`. `--find` is a `FindPattern` enum (literal or pre-compiled `Regex`).
- `src/predicate.rs` — `LinePredicate` trait and `GrepPredicate` (`--grep`/`--invert`, scoped to the column span); `build_predicate(&Config)` returns the row filter, if any. Selection mode drops non-matching lines; delete mode deletes matching ones.
- `src/text.rs` — pure char-indexed helpers (`substring`, `select_columns`, `remove_columns`, `map_columns`, `replace_in_columns`, `split_line_terminator`); all UTF-8 edge-case tests live here.
- `src/file_processor.rs` — `FileProcessor::run<R: BufRead, W: Write>`. When `rows.is_absolute()` it streams line by line (`bstr::for_byte_line_with_terminator`) resolving rows against `usize::MAX`; otherwise (end-relative `~N` bounds) it buffers the whole input first, then resolves rows against the real line count. Applies row selection (delete mode keeps out-of-range lines; selection mode drops them), the row predicate, the transform pipeline and `--unique`, with the terminator split off and re-attached (original terminators, including CRLF, preserved).
- `src/constants.rs` — platform-dependent `NEW_LINE` (only used when a reordered/unterminated line needs a terminator).

Key processing concept: transforms stream (each line written immediately), but reordering is *sequence-breaking* — lines inside the row range are buffered in `RunState::reorder_buffer` and flushed once the row range ends or at EOF. `Reorder` unifies the three reordering ops: `Sort` (via `SortSpec`: key `ColumnSpan`, `--numeric` via `NumericKey`/`total_cmp`, `--reverse` via `cmp::Reverse`, always `sort_by_cached_key` so it stays stable), `Tac` (reverse) and `Shuffle` (rand 0.9). `--unique` keys on the column span of the transformed content via a shared `seen_keys` `HashSet` (first wins; post-sort order with `--sort`).

All column/row semantics are 1-based inclusive, and column indexing is by `char`, not by byte. End-relative rows count `~1` as the last line.

Tests: unit tests live next to the code; end-to-end CLI tests in `tests/cli.rs` run the real binary via `env!("CARGO_BIN_EXE_ft")` — extend those whenever CLI behavior changes.

## Conventions

- Code, comments, docs, commit messages: English only.
- Design goals for this codebase: SOLID principles, the stairway pattern (depend on traits/abstractions, not concrete types, with dependencies pointing "down the stairs"), and idiomatic Rust (prefer `Result` propagation over `unwrap`, iterators over index loops, borrowed types in signatures).
- Tests live in `#[cfg(test)]` modules next to the code and follow a Given/When/Then comment style.
