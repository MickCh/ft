# ft — File Transformer

[![Rust](https://github.com/MickCh/ft/actions/workflows/rust.yml/badge.svg)](https://github.com/MickCh/ft/actions/workflows/rust.yml)

A small command-line tool for transforming text files: select row ranges, delete rows or columns, sort lines by a column key, and find/replace restricted to a column range.

## Usage

```
ft [OPTIONS] [filename]
```

When `filename` is omitted (or given as `-`), `ft` reads from standard input, so it works in pipelines.

| Option | Description |
|---|---|
| `-R, --rows <from>-<to>` | Row range to process (default: all rows) |
| `-C, --cols <from>-<to>` | Column range to process (default: all columns) |
| `-s, --sort` | Sort the selected rows, using the column range as the sort key |
| `-n, --numeric` | Sort numerically instead of lexicographically (requires `--sort`) |
| `--reverse` | Sort in descending order (requires `--sort`) |
| `-d, --delete` | Delete the selected rows, or the column range within them |
| `-f, --find <text>` | Substring to find |
| `-r, --replace <text>` | Replacement text (requires `--find`) |
| `-e, --regex` | Treat the find pattern as a regular expression (requires `--find`) |
| `--ignore-case` | Match the find pattern case-insensitively (requires `--find`) |
| `-o, --output <file>` | Write to a file instead of stdout |

### Semantics

- Row and column ranges are **1-based and inclusive**; columns are counted in characters, not bytes, so multi-byte UTF-8 text (including emoji) is handled correctly.
- Without `--delete`, the row range **selects** lines: only rows inside the range are output (and transformed). Without a row range, the whole file is processed.
- A column range with no other operation **selects** columns (like `cut`): only the characters inside the range are output. With `--sort` it is the sort key, with `--find` it scopes the replacement, and with `--delete` it is removed — in those cases the rest of the line is kept.
- With `--delete`, the row range is **removed**: rows outside the range pass through unchanged. Adding a column range deletes only those columns inside the selected rows.
- Find/replace only replaces occurrences that lie entirely inside the column range.
- Numeric sort parses the sort key as a number (integer or decimal); lines whose key is not a number sort before all numeric lines.
- Original line endings (LF or CRLF) are preserved.
- `--replace` cannot be combined with `--delete`, and `--delete` requires a row or column range.

### Examples

```bash
# Print rows 10-20
ft -R 10-20 input.txt

# Delete rows 2-5, keep everything else
ft -d -R 2-5 input.txt

# Delete columns 1-8 in every line (e.g. strip a fixed-width prefix)
ft -d -C 1-8 input.txt

# Keep only columns 1-8 of every line (like cut)
ft -C 1-8 input.txt

# Replace "foo" with "bar", but only in columns 10-20
ft -C 10-20 -f foo -r bar input.txt

# Regex replace: collapse every number to "N"; $1-style capture references work too
ft -e -f '[0-9]+' -r N input.txt
ft -e -f '(\w+)@(\w+)' -r '$2.$1' input.txt

# Case-insensitive replace: rewrites foo, FOO, Foo, ...
ft --ignore-case -f foo -r bar input.txt

# Sort the whole file by columns 5-12, write the result to out.txt
ft -s -C 5-12 -o out.txt input.txt

# Use in a pipeline: sort the output of another command
grep ERROR app.log | ft -s -C 1-19

# Numeric, descending sort by the value in columns 20-26
ft -s -n --reverse -C 20-26 input.txt

# Sort only rows 2-100 (e.g. keep a header line first... and select just that block)
ft -s -R 2-100 input.txt
```

## Building

Requires a stable [Rust](https://rustup.rs/) toolchain (edition 2024).

```bash
cargo build --release    # binary in target/release/ft
cargo test               # unit + CLI integration tests
cargo clippy --all-targets -- -D warnings
```

## Project structure

- `src/cli_args/` — clap definition and validated `Config` (`TryFrom<ArgMatches>`)
- `src/transform.rs` — `LineTransform` trait and per-line operations; the pipeline is derived from the configuration once
- `src/text.rs` — pure, char-indexed text helpers
- `src/file_processor.rs` — streaming orchestrator, generic over `BufRead`/`Write`
- `tests/cli.rs` — end-to-end tests running the real binary
