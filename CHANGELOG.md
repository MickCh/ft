# Changelog

All notable changes to `fxt`. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [Semantic Versioning](https://semver.org/) — before 1.0, a minor version may change behaviour.

## [Unreleased]

### Changed

- Under `--quoted`, a CSV record may span several lines: a newline inside quotes stays in its field, and `--rows`, `--number`, `--count` and error messages count records. Only a quoted field left open at the end of the input fails (exit code 2).

## [0.2.0] — 2026-11-07

### Changed

- **Renamed from `ft` to `fxt`.** PowerShell aliases `ft` to `Format-Table`, which shadowed the binary. In-place temp files are now `.<name>.fxt-<pid>.tmp`.
- Under `--quoted`, every key compares field *values*: the text and numeric sort, `--unique` and `--group-by`. `"a"` and `a` are the same key, and `"b"` sorts between `a` and `c`. Group keys are printed quoted only where CSV needs it.
- Under `--quoted`, `--sum`/`--avg`/`--min`/`--max` read `"10"` as 10 (it was ignored).
- A line that leaves a quoted field open under `--quoted` — a CSV record spanning several lines — fails with exit code 2 instead of returning fields cut in half.
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

[Unreleased]: https://github.com/MickCh/ft/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/MickCh/ft/compare/5d06433...v0.2.0
