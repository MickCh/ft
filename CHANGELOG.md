# Changelog

All notable changes to `fxt`. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [Semantic Versioning](https://semver.org/) — before 1.0, a minor version may change behaviour.

## [Unreleased]

### Added

- `--lossy`: input that is not valid in its encoding no longer fails the run; each malformed sequence becomes U+FFFD. Invalid UTF-8 is repaired only in the rows `fxt` actually rewrites — a row passed through, filtered in, or left as it was by every transform keeps its bytes, so `--lossy -i` changes no more of a file than the edit itself.
- `--reverse-chars`: reverse the column range, like `rev` (grapheme clusters under `--graphemes`).
- `--pad-left <width>` / `--pad-right <width>`: pad the column range with spaces to a width, right- or left-aligning it; in field mode each field on its own, empty ones too.

### Performance

- Rows held back for reordering, and the input buffered for `~N` row bounds, are stored back to back in one allocation instead of one per row; reordering moves small records, not the text. `-R ~1000-~1` 1.4× faster, `--shuffle` 1.2×, `--tac`, `--sort` and a sort by a CSV field 1.1×.

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

[Unreleased]: https://github.com/MickCh/ft/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/MickCh/ft/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/MickCh/ft/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/MickCh/ft/compare/5d06433...v0.2.0
