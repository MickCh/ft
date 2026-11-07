# Changelog

All notable changes to `fxt`. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [Semantic Versioning](https://semver.org/) — before 1.0, a minor version may change behaviour.

## [Unreleased]

## [0.5.0] — 2026-11-07

### Changed

- `--output-delimiter` also rewrites rows nothing selected fields from: `fxt -F , --output-delimiter ';'` converts a CSV to semicolons (it printed the rows unchanged), and `-F , -d -C 2 --output-delimiter ';'` writes `a;c`. Keys are still read with the input delimiter; rows passed through unchanged keep it.
- `--in-place` leaves a file untouched, with a note, whenever the selection takes no row of it — also for `--rows` past the end of a short file or a `--from` that never matches, not only for a content filter.
- `--output-delimiter ''` is accepted and runs the fields together; an empty `--split-on` is refused with a message about the separator, not about a field delimiter.

### Fixed

- `--title-case` capitalized all of a ligature or a digraph (`ﬁsh` → `FIsh`, `ǆ` → `Ǆ`) and turned a word-initial `ß` into `SS`; the first letter now gets its titlecase (`Fish`, `ǅ`, `Ss`).

## [0.4.1] — 2026-11-07

### Added

- `--lossy`: input that is not valid in its encoding no longer fails the run; each malformed sequence becomes U+FFFD. Invalid UTF-8 is repaired only in the rows `fxt` actually rewrites — a row passed through, filtered in, or left as it was by every transform keeps its bytes, so `--lossy -i` changes no more of a file than the edit itself.
- `--keep-others`: rows the selection leaves out pass through unchanged instead of being dropped, so `fxt -i --keep-others -R 2-5 -f a -r b` edits rows 2-5 of a file and keeps the rest, like `sed -i '2,5s/a/b/'`.
- `--grep` is repeatable: a row matches any of the patterns, or every one with `--all`.
- `--min-fields <n>` and `--max-len <n>`: keep the rows with at least `n` fields, or at most `n` characters; content filters like `--grep`, with its exit codes.
- `--encode <codec>` / `--decode <codec>`: base64, hex and URL percent encoding of the column range, each field on its own; invalid input fails the run (base64 padding is all or nothing: `YQ==` or `YQ`, never `YQ=`), decoded bytes that are not UTF-8 too unless `--lossy`.
- `--expand[=<tabsize>]` / `--unexpand[=<tabsize>]`: tabs to spaces and back within the column range, like `expand` and `unexpand -a` (with the same output as GNU's).
- `--reverse-chars`: reverse the column range, like `rev` (grapheme clusters under `--graphemes`).
- `--pad-left <width>` / `--pad-right <width>`: pad the column range with spaces to a width, right- or left-aligning it; in field mode each field on its own, empty ones too, and an empty line as well.

### Changed

- `--fields` no longer requires a column option: without one every field is in the span, so `-F , --quoted --count` counts CSV records and `-F , --number` separates the number with a comma.
- `--join ''` is accepted and runs the rows together, like `paste -s -d ''`.
- `--min-fields 0` is refused with a message about the field count, not about a width.
- An empty `--find ''` is refused: it matched between every two characters, so `-f '' -r x` turned `abc` into `xaxbxcx`.
- `--in-place` no longer writes a file back empty when a filter matched nothing in it: the file is left as it was (exit code 1, with a note on stderr), and `--dry-run` reports it unchanged.

### Fixed

- `--output-delimiter` stood only between the parts of a column list, not between the fields of one range: `-F , -C 1-3 --output-delimiter ;` printed `a,b,c`. It now rejoins every field.
- In field mode, the transforms that rewrite text worked on a range of fields as one stretch, delimiters included: `-F , -C 1- --reverse-chars` turned `ab,cd` into `dc,ba`, `--trim` trimmed only the ends of the stretch, `--expand` counted tab stops from its start, and `--find` could match across a delimiter. Each field is now rewritten on its own (deleting fields still takes a stretch).
- On Windows, a terminator `fxt` adds of its own was always `\r\n`, whatever the input used: `--join` ended in `\r\n` and an unterminated last line moved by `--tac` put a `\r\n` among `\n`s, while summaries wrote `\n` even into a CRLF file. The terminator now follows the input's lines, on every platform, and a `--join`ed row ends the way the input did (unterminated, if its last line was).
- `--number --unique` kept every duplicate: the number was added before `--unique` compared the rows, so it made every row distinct. Rows are now numbered on their way out, after `--unique`.
- Windows paths in messages and in the names of the files `--in-place` makes next to an input (its temporary file, `--backup`) no longer carry the `\\?\` prefix `canonicalize` puts in front of them; "nothing matched, left unchanged" names the file as it was given.

### Performance

- Rows held back for reordering, and the input buffered for `~N` row bounds, are stored back to back in one allocation instead of one per row; reordering moves small records, not the text. `--shuffle` 1.2× faster, `--tac`, `--sort` and a sort by a CSV field 1.1×.
- A row range whose every part starts from the end (`-R ~1000-`) holds only that many rows, in a ring, instead of the whole input: 1.65× faster on 300k rows (35 → 21 ms; `tail` takes 15), and memory no longer grows with the input.
- Text mappings write straight into the rebuilt line instead of building a string per range, with shortcuts for ASCII: `--reverse-chars` 1.75× faster, `--title-case` 1.45×, `--pad-left` on a field 1.2×, `--expand` and `--trim` 1.1×. A line a mapping leaves as it was goes on untouched.

## [0.4.0] — 2026-11-07

### Added

- `-P`, `--perl-regexp`: every regex may use backreferences (`(.)\1`) and lookaround, through a backtracking engine (`fancy-regex`). A pattern that backtracks without end fails the run (exit code 2).
- `--graphemes`: columns and `--wrap` widths count user-perceived characters (grapheme clusters), so `e` with a combining accent or a ZWJ emoji sequence is one column and is never cut in half.

### Fixed

- A row or column bound with a `+` sign (`-R +2`, `-R ~+1`, `-C 1-+3`) is rejected; it used to be read as the plain number.

### Performance

- Selecting columns copies nothing when the selection is one piece of the line, and field mode no longer allocates per line for field positions: `-F , -C 1` 1.9× faster, `-C 1-6` 1.5×, a summary over a field 1.3–1.4×.
- Several statistics over the same column read and parse it once: `--sum 3 --avg 3 --min 3 --max 3` 1.6× faster.
- A text sort borrows its keys instead of copying every line, and settles most comparisons on the keys' first bytes: a whole-line `--sort` 1.4× faster, now ahead of `sort -s`; by a field 1.15×.

## [0.3.0] — 2026-11-07

### Added

- `--encoding <name>`: inputs in another encoding than UTF-8 (`windows-1250`, `latin1`, `utf-16le`, `shift_jis`, … — the WHATWG labels). A UTF-16 file with a byte order mark is recognized without it.
- `--output-encoding <name>`: the output in another encoding. `--in-place` writes each file back in its own encoding and BOM, unless `--output-encoding` converts it.
- `--median`, `--distinct`, `--first` and `--last` summaries, per `--group-by` group like the others.
- Undecodable input and characters the output encoding cannot hold are errors (exit code 2), never silent replacements.

## [0.2.0] — 2026-11-07

### Changed

- **Renamed from `ft` to `fxt`.** PowerShell aliases `ft` to `Format-Table`, which shadowed the binary. In-place temp files are now `.<name>.fxt-<pid>.tmp`.
- Under `--quoted`, every key compares field *values*: the text and numeric sort, `--unique` and `--group-by`. `"a"` and `a` are the same key, and `"b"` sorts between `a` and `c`. Group keys are printed quoted only where CSV needs it.
- Under `--quoted`, `--sum`/`--avg`/`--min`/`--max` read `"10"` as 10 (it was ignored).
- Under `--quoted`, a CSV record may span several lines (it used to be cut in half at each line): a newline inside quotes stays in its field, and `--rows`, `--number`, `--count` and error messages count records. Only a quoted field left open at the end of the input fails (exit code 2).
- A UTF-8 byte order mark is taken off every input. `--in-place` writes it back; other output carries none.
- `--sort --numeric` and the summaries keep integers exact up to 128 bits; keys beyond 2^53 used to be rounded together. Integer sums print exactly.

### Added

- `--from <regex>` / `--to <regex>`: rows addressed by content, like `sed '/A/,/B/'`. They count as a filter for `--delete`, `--quiet` and the exit code.
- `--grep-key <cols>`: a column range for `--grep` alone, leaving `--cols` free to select.
- `--completions <shell>` (bash, zsh, fish, powershell, elvish) and `--man`.
- More examples in `--help`; a README section on the fixed order of operations and the shared `--cols` scope.
- Benchmarks (`cargo bench`) and property tests (`proptest`).

### Fixed

- `-d --from …` without `--grep` would have exited 1 after deleting rows.

### Performance

- Field mode splits lines byte-wise and only as far as needed: `-F , -C 1` 1.45× faster, `--unique-key` on a field 2.1×, a field-keyed numeric sort 1.45×.
- A summary splits each line once for its key and every aggregate: `--group-by 1 --sum 3` 1.6× faster.
- A literal `--find` no longer copies lines it leaves alone: 1.5× faster when few lines match.

## [0.1.0]

The first version, as `ft`: row and column ranges (lists, open and end-relative bounds, field mode with `--quoted` CSV), sort/tac/shuffle, `--grep` with grep-like exit codes and `--quiet`, `--unique`, case and whitespace transforms, `--number`, `--wrap`, `--split-on`/`--join`, `--drop-empty`, summaries per `--group-by`, find/replace, and in-place editing of many files with `--backup` and `--dry-run`.

[Unreleased]: https://github.com/MickCh/ft/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/MickCh/ft/compare/v0.4.1...v0.5.0
[0.4.1]: https://github.com/MickCh/ft/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/MickCh/ft/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/MickCh/ft/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/MickCh/ft/compare/5d06433...v0.2.0
