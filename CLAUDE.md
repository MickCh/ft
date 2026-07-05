# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

`ft` (File Transformer) is a CLI tool for transforming text files: selecting row/column ranges, deleting them, sorting lines by a column range, and find/replace within a column range. It is also a learning project for idiomatic Rust — code quality and design matter as much as features.

## Commands

```bash
cargo build                     # build
cargo test                      # run all tests
cargo test test_line_replace    # run a single test by name
cargo clippy --all-targets -- -D warnings   # lint (CI fails on any warning)
cargo fmt                       # format (rustfmt.toml: chain_width 40)
cargo run -- --help             # run the CLI
cargo run -- -R 1-5 -C 2-4 -f foo -r bar input.txt   # example invocation
```

CI (`.github/workflows/rust.yml`) runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo build` and `cargo test` on pushes/PRs to `main`.

## Architecture

Library crate (`src/lib.rs`) plus a thin binary (`src/main.rs`). Flow: argv → validated `Config` → transform pipeline → streaming processor. Only `main.rs` touches the filesystem; the processor works on injected `BufRead`/`Write`, so tests run against in-memory `Cursor`/`Vec<u8>`. Input comes from a file, or stdin when the filename is omitted or `-` (`Config.filename: Option<PathBuf>`).

- `src/cli_args/` — everything between argv and a valid `Config`:
  - `cli.rs` — clap `Command` definition (name/version come from `crate_name!`/`crate_version!`) and range parsing; single-value validity (format `<start>-<end>`, 1-based, start ≤ end) is enforced here in the value parser.
  - `config.rs` — immutable `Config` built via `TryFrom<ArgMatches>`, which validates cross-argument rules (e.g. `--replace` requires `--find`, replace and delete are mutually exclusive). Ranges are `Option<RangeInclusive<usize>>` (`None` = not provided); `rows_or_full()`/`cols_or_full()` supply the `1..=usize::MAX` fallback. Paths are `PathBuf`.
  - `config_error.rs` — `ConfigError` enum with `Display` for user-facing validation messages.
- `src/transform.rs` — `LineTransform` trait and its implementations (`DeleteColumns`, `ReplaceInColumns`, `RegexReplaceInColumns`); `build_pipeline(&Config)` derives the pipeline once. A new per-line operation is a new transform here, not a new branch in the processing loop. `--find` is a `FindPattern` enum (literal or pre-compiled `Regex`).
- `src/text.rs` — pure char-indexed helpers (`substring`, `remove_columns`, `replace_in_columns`, `split_line_terminator`); all UTF-8 edge-case tests live here.
- `src/file_processor.rs` — `FileProcessor::run<R: BufRead, W: Write>` streams line by line (`bstr::for_byte_line_with_terminator`), applies row selection (delete mode keeps out-of-range lines; selection mode drops them), runs the transform pipeline on content with the terminator split off and re-attached (original terminators, including CRLF, are preserved).
- `src/constants.rs` — platform-dependent `NEW_LINE` (only used when sorting forces a terminator onto an unterminated line).

Key processing concept: transforms stream (each line written immediately), but sort is *sequence-breaking* — lines inside the row range are buffered in `sort_buffer` and flushed sorted once the row range ends or at EOF. Sorting is described by `SortSpec` (key columns, `--numeric` via `NumericKey`/`total_cmp`, `--reverse` via `cmp::Reverse`), always through `sort_by_cached_key` so it stays stable.

All column/row semantics are 1-based inclusive, and column indexing is by `char`, not by byte.

Tests: unit tests live next to the code; end-to-end CLI tests in `tests/cli.rs` run the real binary via `env!("CARGO_BIN_EXE_ft")` — extend those whenever CLI behavior changes.

## Conventions

- Code, comments, docs, commit messages: English only.
- Design goals for this codebase: SOLID principles, the stairway pattern (depend on traits/abstractions, not concrete types, with dependencies pointing "down the stairs"), and idiomatic Rust (prefer `Result` propagation over `unwrap`, iterators over index loops, borrowed types in signatures).
- Tests live in `#[cfg(test)]` modules next to the code and follow a Given/When/Then comment style.
