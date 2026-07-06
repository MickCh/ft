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
| `-R, --rows <ranges>` | Rows to process: `3`, `2-5`, `10-`, `-5`, `~10-~1` or a list `1-5,10-20` (default: all rows) |
| `-C, --cols <range>` | Column range to process: `3`, `2-5`, `10-` or `-5` (default: all columns) |
| `-s, --sort` | Sort the selected rows, using the column range as the sort key |
| `-n, --numeric` | Sort numerically instead of lexicographically (requires `--sort`) |
| `--reverse` | Sort in descending order (requires `--sort`) |
| `--tac` | Reverse the order of the selected rows (like `tac`) |
| `--shuffle` | Shuffle the selected rows into a random order |
| `-d, --delete` | Delete the selected rows, or the column range within them |
| `-u, --unique` | Drop duplicate rows, comparing the column range (first wins) |
| `-g, --grep <regex>` | Keep only rows matching the regex (with `--delete`: delete them) |
| `--invert` | Invert the `--grep` match, like `grep -v` (requires `--grep`) |
| `-f, --find <text>` | Substring to find |
| `-r, --replace <text>` | Replacement text (requires `--find`) |
| `-e, --regex` | Treat the find pattern as a regular expression (requires `--find`) |
| `--ignore-case` | Match the find pattern case-insensitively (requires `--find`) |
| `--upper` | Convert the column range to uppercase |
| `--lower` | Convert the column range to lowercase |
| `--trim` | Trim whitespace at both ends of the column range |
| `-o, --output <file>` | Write to a file instead of stdout |

### Semantics

- Row and column ranges are **1-based and inclusive**; columns are counted in characters, not bytes, so multi-byte UTF-8 text (including emoji) is handled correctly.
- A range can be a single number (`3`), open-ended (`10-` to the end, `-5` from the start) or closed (`2-5`). Rows additionally accept a comma-separated list of ranges (`1-5,10-20`); overlapping parts are merged.
- A row bound prefixed with `~` counts from the **end** of the input: `~1` is the last row, `~10-~1` the last ten, `2-~2` everything but the first and last row. Because the total line count must be known first, end-relative ranges buffer the whole input instead of streaming; columns do not accept `~`.
- Without `--delete`, the row range **selects** lines: only rows inside the range are output (and transformed). Without a row range, the whole file is processed.
- A column range with no other operation **selects** columns (like `cut`): only the characters inside the range are output. With `--sort` it is the sort key, with `--find` it scopes the replacement, and with `--delete` it is removed — in those cases the rest of the line is kept.
- With `--delete`, the row range is **removed**: rows outside the range pass through unchanged. Adding a column range deletes only those columns inside the selected rows.
- Find/replace only replaces occurrences that lie entirely inside the column range.
- `--grep` filters rows by content, complementing the positional row range: only rows inside `--rows` *and* matching the pattern are processed. With `--delete`, matching rows are deleted instead. The match is scoped to the column range.
- `--upper`, `--lower` and `--trim` apply to the column range (the whole line without one) and run after find/replace, so replaced text is transformed too. They cannot be combined with `--delete`.
- Numeric sort parses the sort key as a number (integer or decimal); lines whose key is not a number sort before all numeric lines.
- `--unique` keeps the first row per key (the column range, or the whole line without one) and drops later duplicates; combined with `--sort`, "first" means first in sorted order, like `sort -u`.
- `--sort`, `--tac` and `--shuffle` are mutually exclusive reordering operations; each buffers the selected rows before writing them out.
- Original line endings (LF or CRLF) are preserved.
- `--replace` cannot be combined with `--delete`, and `--delete` requires a row or column range.

### Examples

```bash
# Print rows 10-20
ft -R 10-20 input.txt

# Print the first 5 rows, rows 1, 3 and 10-20, or everything from row 100 on
ft -R -5 input.txt
ft -R 1,3,10-20 input.txt
ft -R 100- input.txt

# Print the last 10 rows (like tail); drop the last row
ft -R '~10-~1' input.txt
ft -d -R '~1' input.txt

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

# Uppercase columns 1-3, trim whitespace around every line
ft --upper -C 1-3 input.txt
ft --trim input.txt

# Keep only rows containing ERROR; delete rows that match a pattern
ft -g ERROR app.log
ft -d -g '^#' config.txt

# Sort and deduplicate by the key in columns 1-8 (like sort -u)
ft -s -u -C 1-8 input.txt

# Reverse the file; shuffle rows 2-100
ft --tac input.txt
ft --shuffle -R 2-100 input.txt

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
