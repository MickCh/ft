# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

`ft` (File Transformer) is a CLI tool for transforming text files: selecting row/column ranges, deleting them, sorting lines by a column range, and find/replace within a column range. It is also a learning project for idiomatic Rust — code quality and design matter as much as features.

## Commands

```bash
cargo build                     # build
cargo test                      # run all tests
cargo test test_line_replace    # run a single test by name
cargo clippy --all-targets      # lint (keep warning-free)
cargo fmt                       # format (rustfmt.toml: Block indent, chain_width 40)
cargo run -- --help             # run the CLI
cargo run -- -R 1-5 -C 2-4 -f foo -r bar input.txt   # example invocation
```

CI (`.github/workflows/rust.yml`) runs `cargo build` and `cargo test` on pushes/PRs to `main`.

## Architecture

Two-stage pipeline: parse CLI input into a validated `Config`, then stream the file through `FileProcessor`.

- `src/cli_args/` — everything between argv and a valid `Config`:
  - `cli.rs` — clap `Command` definition and range parsing (`<start>-<end>`, 1-based inclusive).
  - `config_builder.rs` — `ConfigBuilder` reads clap matches, applies defaults, and validates cross-argument rules (e.g. `--replace` requires `--find`, replace and delete are mutually exclusive) in `build()`.
  - `config.rs` — immutable `Config` consumed by the processor. "Range not provided" is encoded as the full range `1..=usize::MAX`; the `is_*_provided()` helpers detect this.
  - `config_error.rs` — `ConfigError` enum with `Display` for user-facing validation messages.
- `src/file_processor.rs` — `FileProcessor` owns the `Config` and streams the input line by line (`bstr::for_byte_line_with_terminator`, so line terminators stay attached to lines). Output goes to `--output` file or stdout via `Box<dyn Write>`.
- `src/constants.rs` — platform-dependent `NEW_LINE`.

Key processing concept: operations are either streamable (delete, find/replace — each line written immediately) or *sequence-breaking* (sort — lines inside the row range must be buffered and only flushed once the range ends; see `Config::is_sequence_breaking` and the `non_sequence_vec` buffering in `process_single_line`).

All column/row semantics are 1-based inclusive, and column indexing is by `char`, not by byte (see the UTF-8 tests in `file_processor.rs`).

## Conventions

- Code, comments, docs, commit messages: English only.
- Design goals for this codebase: SOLID principles, the stairway pattern (depend on traits/abstractions, not concrete types, with dependencies pointing "down the stairs"), and idiomatic Rust (prefer `Result` propagation over `unwrap`, iterators over index loops, borrowed types in signatures).
- Tests live in `#[cfg(test)]` modules next to the code and follow a Given/When/Then comment style.
