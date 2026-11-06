# fxt — File Transformer

[![Rust](https://github.com/MickCh/ft/actions/workflows/rust.yml/badge.svg)](https://github.com/MickCh/ft/actions/workflows/rust.yml)

A small command-line tool for transforming text files: select or delete row and column ranges, reorder columns, sort/reverse/shuffle lines, filter by content, find/replace within a column range, wrap/split/join rows, summarize them (count, sum, avg, min, max, per group), and edit files in place — with CSV support (one record per line).

It was called `ft` until 2026-11; the binary was renamed because PowerShell aliases `ft` to `Format-Table`, which shadowed it.

## Usage

```
fxt [OPTIONS] [filename]...
```

When no filename is given (or a filename is `-`), `fxt` reads from standard input, so it works in pipelines. Several files are read as one stream, like `cat`.

| Option | Description |
|---|---|
| `-R, --rows <ranges>` | Rows to process: `3`, `2-5`, `10-`, `-5`, `~10-~1` or a list `1-5,10-20` (default: all rows) |
| `-C, --cols <ranges>` | Columns to process: `3`, `2-5`, `10-`, `-5` or a list `1,3,5-7` (default: all columns) |
| `-F, --fields <delim>` | Treat the column ranges as fields separated by `<delim>` (requires a column range) |
| `--quoted` | Respect `"quoted"` fields: a delimiter inside quotes does not split (requires `--fields`) |
| `--output-delimiter <s>` | Join the selected fields with `<s>` instead of the input delimiter (requires `--fields`) |
| `-s, --sort` | Sort the selected rows, using the column range as the sort key |
| `--sort-key <range>` | Columns keying `--sort`, instead of `--cols` (requires `--sort`) |
| `-n, --numeric` | Sort numerically instead of lexicographically (requires `--sort`) |
| `--reverse` | Sort in descending order (requires `--sort`) |
| `--tac` | Reverse the order of the selected rows (like `tac`) |
| `--shuffle` | Shuffle the selected rows into a random order |
| `-d, --delete` | Delete the selected rows, or the column range within them |
| `-u, --unique` | Drop duplicate rows, comparing the column range (first wins) |
| `--unique-key <range>` | Columns keying `--unique`, instead of `--cols` (requires `--unique`) |
| `--grep-key <range>` | Columns `--grep` matches in, instead of `--cols` (requires `--grep`) |
| `-g, --grep <regex>` | Keep only rows matching the regex (with `--delete`: delete them) |
| `--invert` | Invert the `--grep` match, like `grep -v` (requires `--grep`) |
| `-q, --quiet` | Write nothing; say with the exit code whether anything matched (requires `--grep`) |
| `-f, --find <text>` | Substring to find (repeatable, paired with `--replace`) |
| `-r, --replace <text>` | Replacement text (repeatable, one per `--find`) |
| `-e, --regex` | Treat the find pattern as a regular expression (requires `--find`) |
| `--ignore-case` | Match the find pattern case-insensitively (requires `--find`) |
| `--upper` | Convert the column range to uppercase |
| `--lower` | Convert the column range to lowercase |
| `--trim` | Trim whitespace at both ends of the column range |
| `--title-case` | Capitalize the first letter of every word in the column range |
| `--squeeze` | Collapse runs of whitespace in the column range into single spaces |
| `--number` | Number the output rows, like `nl` |
| `--split-on <sep>` | Split every line at each occurrence of `<sep>`, one row per piece |
| `--wrap <width>` | Wrap every line into chunks of at most `<width>` characters (like `fold -w`) |
| `--drop-empty` | Drop lines that are empty after the other transforms ran |
| `--join <sep>` | Join every row into a single row, separated by `<sep>` (like `paste -s`) |
| `--count` | Summarize: how many rows |
| `--sum <cols>` | Summarize: the total of the numbers in those columns |
| `--avg <cols>` | Summarize: the mean of the numbers in those columns |
| `--min <cols>` / `--max <cols>` | Summarize: the smallest / largest number in those columns |
| `--group-by <cols>` | Summarize once per distinct value of those columns (requires a summary) |
| `-o, --output <file>` | Write to a file instead of stdout |
| `-i, --in-place` | Edit the input files in place (needs files, conflicts with `-o`) |
| `--backup <suffix>` | Keep a copy of each edited file, with this suffix (requires `--in-place`) |
| `--dry-run` | Report which files the edit would change, without writing (requires `--in-place`) |

### Semantics

- Row and column ranges are **1-based and inclusive**; columns are counted in characters (Unicode code points), not bytes, so multi-byte UTF-8 text is never cut inside a character. They are not grapheme clusters, though: a letter followed by a combining accent (`e\u0301`) or an emoji joined with ZWJ (`👩\u200d💻`) counts as several columns, and a range may split it.
- A range can be a single number (`3`), open-ended (`10-` to the end, `-5` from the start) or closed (`2-5`). Rows and columns both accept a comma-separated list of ranges (`1-5,10-20`).
- A **column list keeps the order written**, so operations that *read* the columns — selecting them, the `--sort`/`--unique` key, `--grep` — read the parts in that order: `-F , -C 3,1,2` is an `awk`-style projection that reorders the fields, and `--output-delimiter` chooses what rejoins them. Operations that *write into* the line — `--delete`, `--upper`/`--lower`/`--trim`, find/replace — work on the same columns as a set (sorted, overlaps merged), where order carries no meaning; each part is written on its own, so a match straddling two parts is not replaced.
- A row bound prefixed with `~` counts from the **end** of the input: `~1` is the last row, `~10-~1` the last ten, `2-~2` everything but the first and last row. Because the total line count must be known first, end-relative ranges buffer the whole input instead of streaming; columns do not accept `~`.
- Without `--delete`, the row range **selects** lines: only rows inside the range are output (and transformed). Without a row range, the whole file is processed.
- A column range with no other operation **selects** columns (like `cut`): only the characters inside the range are output. With `--sort` it is the sort key, with `--find` it scopes the replacement, and with `--delete` it is removed — in those cases the rest of the line is kept.
- `--sort-key`, `--unique-key` and `--grep-key` give those operations a column range of their own, so `--cols` is free for another one: `-C 5 -f a -r b --sort --sort-key 3` replaces inside column 5 but sorts by column 3. An operation with its own key no longer claims `--cols`, so a `--cols` left over with no other operation goes back to **selecting** columns. The sort and unique keys then address that selected result, since they are read from the transformed line; the grep key addresses the line *as read*, since the filter runs before any transform — so `-F , -C 2 --grep-key 1 -g ERROR` keeps the rows whose field 1 matches and prints their field 2.
- With `--fields`, the column ranges count **delimited fields** instead of characters: `-F , -C 2` addresses the second comma-separated field, per line. All column-based operations (select, delete, sort key, find/replace scope, `--grep`, `--unique`, case/trim) work on fields the same way; deleting fields also removes one adjacent delimiter, like `cut`, and a field list is merged before that, so `-C 2,3` deletes exactly what `-C 2-3` does. Selected fields are rejoined by the delimiter (or `--output-delimiter`), and a field the line does not have is skipped rather than joining a stray delimiter. The delimiter may be more than one character.
- `--quoted` makes field mode read CSV rather than a plain split: a delimiter inside a `"…"` field no longer splits it, and a doubled `""` is an escaped quote (RFC 4180). A field keeps its quotes — they are part of the text it occupies — so selecting or reordering fields re-joins them into valid CSV, and deleting one takes its quotes with it. Operations that read a field as a *number* (`--sort --numeric` and the `--sum`/`--avg`/`--min`/`--max` summaries) read its value instead, without the quotes, so `"10"` counts as 10. Every record must fit on one line: a line that leaves a quoted field open (a newline inside quotes) is rejected with exit code 2 rather than read as two broken records.
- With `--delete`, the row range is **removed**: rows outside the range pass through unchanged. Adding a column range deletes only those columns inside the selected rows.
- Find/replace only replaces occurrences that lie entirely inside the column range.
- `--find` and `--replace` may be repeated to run several substitutions: the *n*-th `--find` pairs with the *n*-th `--replace`, and the pairs apply left to right, so a later pair can rewrite what an earlier one produced. Every `--find` must have its own `--replace` and vice versa (a lone `--find` is rejected — use `--grep` to filter rows by content instead).
- `--grep` filters rows by content, complementing the positional row range: only rows inside `--rows` *and* matching the pattern are processed. With `--delete`, matching rows are deleted instead. The match is scoped to the column range.
- `--upper`, `--lower`, `--title-case`, `--squeeze` and `--trim` apply to the column range (the whole line without one) and run after find/replace, so replaced text is transformed too. They cannot be combined with `--delete`. `--squeeze` runs before `--trim`, so `--squeeze --trim` normalizes the whitespace of a line completely.
- `--number` prefixes each **output** row with its number (separated by `--output-delimiter`, else `--fields`, else a tab). It counts the rows it actually emits — after the filters, after `--split-on`/`--wrap` expanded them and after `--drop-empty` removed some — so the numbers are always contiguous. It cannot be combined with a reordering, which would shuffle the numbers along with the rows.
- `--wrap` cuts every processed line into chunks of at most `<width>` **characters** — one row in, several rows out. It runs after the other transforms, so the chunks are cut from the finished line. If the input's last line had no terminator, neither does the last chunk.
- `--split-on` cuts every processed line at each occurrence of the separator, turning one row into one row per piece (`tr , '\n'`, but only on the rows being processed). It runs after the column-scoped transforms and before `--wrap`.
- `--join` folds every processed row into a single row — the inverse of `--split-on`, and the two compose: `fxt --split-on , --join ,` returns what it was given. Like a summary, it consumes the rows, so it cannot be combined with one (or with `--delete` or `--number`).
- **Summaries** (`--count`, `--sum`, `--avg`, `--min`, `--max`) *replace* the rows they summarize: the rows are consumed and only the summary is printed. They see exactly the rows that survive `--rows`, `--grep` and `--unique`, so `--unique --count` counts the distinct rows. Add `--group-by <cols>` for one summary row per distinct key, printed in the order the keys first appear. Under `--quoted` the key is each field's *value*, quoted again only where CSV needs it (the value holds the delimiter, a quote or a line break), so `"a"` and `a` are one group, printed as `a`, while `"x,y"` stays `"x,y"`. The output columns are the key (if any), then the count, sum, avg, min and max that were asked for, separated by `--output-delimiter`, else `--fields`, else a tab. A value that is not a number takes no part in the statistics (it is not a zero), so a group with no numbers at all shows an empty average, minimum and maximum. Without `--group-by` the summary always appears, even over no rows at all — `fxt --count -g X` prints `0` when nothing matches, like `grep -c` (and still exits 1); with `--group-by`, no rows means no groups and no output. Summaries cannot be combined with `--delete` or `--number`, which would have nothing left to act on; a reordering is fine — the reducer takes the rows in the order they come out, so `--sort --group-by` reports the groups in sorted order and `--sort --join` folds them sorted.
- `--drop-empty` removes lines that are empty *after* the transforms ran — which `--grep --invert` cannot do, since a predicate sees the line as it was read. `fxt -C 3 --drop-empty` drops the rows too short to reach column 3, and `--trim --drop-empty` drops whitespace-only lines.
- Numeric sort parses the sort key as a number (integer or decimal); lines whose key is not a number sort before all numeric lines. Integers compare exactly at any size up to 128 bits, so keys beyond 2^53 — where a floating-point key would round neighbours together — still sort correctly, and so do integers against decimals. Sums stay exact integers too, for as long as every value is one.
- `--unique` keeps the first row per key (the column range, or the whole line without one) and drops later duplicates; combined with `--sort`, "first" means first in sorted order, like `sort -u`.
- `--sort`, `--tac` and `--shuffle` are mutually exclusive reordering operations; each buffers the selected rows before writing them out. They cannot be combined with `--delete` on whole rows (the rows would be removed, not reordered); combining them with `--delete --cols` is fine, since there `--delete` removes columns.
- Original line endings (LF or CRLF) are preserved.
- A UTF-8 byte order mark (BOM) at the start of an input is taken off before processing — of every input file, not just the first — so it does not count as a column or keep the first field from matching. The output carries none, except with `--in-place`, which writes it back: the edited file stays encoded as it was, and `--dry-run` does not report the mark as a change.
- Several input files are read **as one stream**, in the order given (like `cat a b | fxt`), so a row range addresses the concatenation. `--in-place` is the exception: it edits each file on its own, so row 1 means row 1 *of each file* — which is what makes `fxt -i -f foo -r bar *.txt` a batch edit.
- `--in-place` rewrites the input file itself: the result is written to a temporary file in the same directory and then atomically renamed over the original, so an interrupted run never truncates the input. The original file's permissions are preserved. It needs real input files (not stdin) and cannot be combined with `--output`.
- `--backup .bak` keeps the original as `<file>.bak` before the swap; `--dry-run` writes nothing at all and instead reports, per file, whether the edit *would* change it — so a batch edit can be checked before it happens.
- `--replace` cannot be combined with `--delete`, and `--delete` requires a row or column range.

### Order of operations

The flags describe *what* to do, not in which order: whatever order they are written in, `fxt` always runs the same pipeline.

1. **Row range** (`--rows`), then the **content filter** (`--grep`/`--invert`) — on the line *as it was read*, before any transform.
2. **Transforms**, in this fixed order: column delete → find/replace pairs → `--upper` → `--lower` → `--title-case` → `--squeeze` → `--trim` → `--split-on` → `--wrap` → `--drop-empty` → `--number`.
3. **`--unique`**, on the transformed line.
4. **Reordering** (`--sort`/`--tac`/`--shuffle`), then a **reducer** (`--join` or a summary) instead of writing the rows.

So `--upper -g error` finds nothing in a line reading `ERROR` — the filter sees the original text. Use `-i` (ignore case) or a pattern matching the input as it is.

`--cols` is **one shared scope**: every operation that accepts a column range uses it, and only a `--cols` that no operation claimed selects (cuts) columns. `-F , -C 2 -g ERROR` therefore *filters* on field 2 and prints the whole row; it does not also cut field 2 out. Where an operation has a key option of its own (`--sort-key`, `--unique-key`, `--grep-key`), use it to free `--cols` for the selection; otherwise chain two runs:

```bash
# Filter on field 2 and print only field 2 (the grep key counts in the line as read)
fxt -F , -C 2 --grep-key 2 -g ERROR data.csv

# Filter on field 1, print field 2
fxt -F , -C 2 --grep-key 1 -g ERROR data.csv

# Uppercase field 2, then filter on the result: a second run sees what the first wrote
fxt -F , -C 2 --upper data.csv | fxt -F , -C 2 -g ERROR

# Print fields 3 and 1, sorted by field 1 — keys are read from the selected
# result, so the original field 1 is field 2 there
fxt -F , -C 3,1 --sort --sort-key 2 data.csv
```

### Exit codes

Like `grep`:

| Code | Meaning |
|---|---|
| `0` | Rows matched (a run without `--grep` has nothing that could fail to match, so it always succeeds) |
| `1` | `--grep` was given and no row matched |
| `2` | The run failed (bad arguments, unreadable file, …) |

A row counts as matched when it lies inside `--rows` *and* satisfies the filter — including the rows a `--delete` removed, since matching is why they went. `--quiet` writes nothing at all and leaves only the exit code, stopping at the first match (`grep -q`), which makes `fxt` usable as a condition:

```bash
if fxt -q -g ERROR app.log; then echo "there were errors"; fi
```

### Examples

```bash
# Print rows 10-20
fxt -R 10-20 input.txt

# Print the first 5 rows, rows 1, 3 and 10-20, or everything from row 100 on
fxt -R -5 input.txt
fxt -R 1,3,10-20 input.txt
fxt -R 100- input.txt

# Print the last 10 rows (like tail); drop the last row
fxt -R '~10-~1' input.txt
fxt -d -R '~1' input.txt

# Delete rows 2-5, keep everything else
fxt -d -R 2-5 input.txt

# Delete columns 1-8 in every line (e.g. strip a fixed-width prefix)
fxt -d -C 1-8 input.txt

# Keep only columns 1-8 of every line (like cut)
fxt -C 1-8 input.txt

# Replace "foo" with "bar", but only in columns 10-20
fxt -C 10-20 -f foo -r bar input.txt

# CSV fields: keep fields 2-3, drop field 2, sort numerically by field 3
fxt -F , -C 2-3 data.csv
fxt -d -F , -C 2 data.csv
fxt -s -n -F , -C 3 data.csv

# Project fields in a new order (like awk '{print $3,$1,$2}'), retabulating them
fxt -F , -C 3,1,2 data.csv
fxt -F , -C 3,1 --output-delimiter ';' data.csv

# Real CSV: a comma inside "quoted, fields" no longer splits them
fxt -F , --quoted -C 2 data.csv
fxt -F , --quoted -C 3,1 data.csv

# Keep characters 1, 3 and 5-7; drop fields 1 and 3 at once
fxt -C 1,3,5-7 input.txt
fxt -d -F , -C 1,3 data.csv

# Sort by field 1 while replacing inside field 2 only
fxt -F , -C 2 -f x -r y -s --sort-key 1 data.csv

# Deduplicate on field 1 (the whole row need not repeat), like sort -u -k1,1
fxt -s -u --unique-key 1 -F , data.csv

# Regex replace: collapse every number to "N"; $1-style capture references work too
fxt -e -f '[0-9]+' -r N input.txt
fxt -e -f '(\w+)@(\w+)' -r '$2.$1' input.txt

# Case-insensitive replace: rewrites foo, FOO, Foo, ...
fxt --ignore-case -f foo -r bar input.txt

# Several substitutions in one pass (applied left to right)
fxt -f cat -r dog -f red -r blue input.txt

# Uppercase columns 1-3, trim whitespace around every line
fxt --upper -C 1-3 input.txt
fxt --trim input.txt

# Hard-wrap every line at 80 characters (like fold -w 80)
fxt --wrap 80 input.txt

# Explode a comma-separated row into one row per value, and fold rows back into one
fxt --split-on , input.txt
fxt --join , input.txt

# Summaries: count the rows, count only the errors, count the distinct ones
fxt --count input.txt
fxt --count -g ERROR app.log
fxt --unique --count input.txt

# Per-key statistics (like a small datamash): total and mean of field 2 per field 1
fxt -F , --group-by 1 --count --sum 2 --avg 2 data.csv

# Drop blank lines; drop rows left empty by cutting a column
fxt --trim --drop-empty input.txt
fxt -C 3 --drop-empty input.txt

# Keep only rows containing ERROR; delete rows that match a pattern
fxt -g ERROR app.log
fxt -d -g '^#' config.txt

# Sort and deduplicate by the key in columns 1-8 (like sort -u)
fxt -s -u -C 1-8 input.txt

# Reverse the file; shuffle rows 2-100
fxt --tac input.txt
fxt --shuffle -R 2-100 input.txt

# Sort the whole file by columns 5-12, write the result to out.txt
fxt -s -C 5-12 -o out.txt input.txt

# Edit a file in place (atomic replace)
fxt -i -f foo -r bar input.txt

# Batch edit: every .txt at once, keeping a .bak of each
fxt -i --backup .bak -f foo -r bar *.txt

# Check first what that batch edit would touch, without writing anything
fxt -i --dry-run -f foo -r bar *.txt

# Concatenate several files and process them as one stream
fxt -s a.txt b.txt c.txt

# Use in a pipeline: sort the output of another command
grep ERROR app.log | fxt -s -C 1-19

# Numeric, descending sort by the value in columns 20-26
fxt -s -n --reverse -C 20-26 input.txt

# Sort only rows 2-100 (e.g. keep a header line first... and select just that block)
fxt -s -R 2-100 input.txt
```

## Building

Requires a stable [Rust](https://rustup.rs/) toolchain (edition 2024).

```bash
cargo build --release    # binary in target/release/fxt
cargo test               # unit + CLI integration tests
cargo clippy --all-targets -- -D warnings
cargo install --path .   # put fxt on the PATH (~/.cargo/bin)
```

### Shell completions and man page

`fxt --completions <shell>` prints a completion script (`bash`, `zsh`, `fish`, `powershell`, `elvish`); `fxt --man` prints the man page.

```bash
# bash: load in ~/.bashrc
source <(fxt --completions bash)

# zsh: a file on $fpath
fxt --completions zsh > ~/.zfunc/_fxt

# fish
fxt --completions fish > ~/.config/fish/completions/fxt.fish

# man page
fxt --man > ~/.local/share/man/man1/fxt.1
```

```powershell
# PowerShell: add to $PROFILE
fxt --completions powershell | Out-String | Invoke-Expression
```

## Project structure

- `src/cli_args/` — clap definition and validated `Config` (`TryFrom<ArgMatches>`)
- `src/transform.rs` — `LineTransform` trait and per-line operations; the pipeline is derived from the configuration once
- `src/text.rs` — pure, char-indexed text helpers
- `src/file_processor.rs` — streaming orchestrator, generic over `BufRead`/`Write`
- `tests/cli.rs` — end-to-end tests running the real binary
