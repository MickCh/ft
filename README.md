# fxt — File Transformer

[![Rust](https://github.com/MickCh/ft/actions/workflows/rust.yml/badge.svg)](https://github.com/MickCh/ft/actions/workflows/rust.yml)

A small command-line tool for transforming text files: select or delete row and column ranges, reorder columns, sort/reverse/shuffle lines, filter by content or take blocks between two patterns (like `sed '/A/,/B/'`), find/replace within a column range, wrap/split/join rows, summarize them (count, sum, avg, min, max, median, distinct, first, last, per group), and edit files in place — with CSV support (quoted fields, records spanning several lines), any text encoding in and out, and optional backreferences and lookaround (`-P`).

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
| `-F, --fields <delim>` | Split rows into fields at `<delim>`: the column ranges count fields (without one, every field is in the span) |
| `--quoted` | Respect `"quoted"` fields: a delimiter inside quotes does not split (requires `--fields`) |
| `--output-delimiter <s>` | Write fields separated by `<s>` instead of the input delimiter: the selected fields, or every field of a row nothing selected from (requires `--fields`; may be empty) |
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
| `-g, --grep <regex>` | Keep only rows matching the regex (with `--delete`: delete them); repeatable: rows matching any of them |
| `--keep-others` | Keep the rows the selection leaves out, unchanged, instead of dropping them (like `sed '2,5s/a/b/'`) |
| `--all` | With several `--grep`, keep only rows matching every one (requires `--grep`) |
| `--min-fields <n>` | Keep only rows with at least `<n>` fields (requires `--fields`) |
| `--max-len <n>` | Keep only rows at most `<n>` characters long |
| `--from <regex>` | Rows from each line matching the regex… (like `sed '/A/,/B/'`) |
| `--to <regex>` | …through the next line matching this one; alone, from the first line |
| `-v, --invert` | Invert the `--grep` match, like `grep -v` (requires `--grep`) |
| `-q, --quiet` | Write nothing; say with the exit code whether anything matched (requires a content filter: `--grep`, `--from`, `--to`, `--min-fields` or `--max-len`) |
| `-f, --find <text>` | Substring to find (repeatable, paired with `--replace`) |
| `-r, --replace <text>` | Replacement text (repeatable, one per `--find`) |
| `-e, --regex` | Treat the find pattern as a regular expression (requires `--find`) |
| `--ignore-case` | Match the find, grep and `--from`/`--to` patterns case-insensitively (requires one of them) |
| `-P`, `--perl-regexp` | Let every regex use backreferences (`(.)\1`) and lookaround (`(?<=x)`, `(?!y)`), matched by backtracking |
| `--upper` | Convert the column range to uppercase |
| `--lower` | Convert the column range to lowercase |
| `--trim` | Trim whitespace at both ends of the column range |
| `--title-case` | Capitalize every word in the column range and lowercase the rest of it (`wORLD` → `World`) |
| `--squeeze` | Collapse runs of whitespace in the column range into single spaces |
| `--encode <codec>` | Encode the column range as `base64`, `hex` or `url` (percent encoding); each field on its own |
| `--decode <codec>` | Decode the column range from `base64`, `hex` or `url`; each field on its own |
| `--expand[=<tabsize>]` | Turn tabs in the column range into spaces, a tab stop every `<tabsize>` columns (default 8; like `expand`) |
| `--unexpand[=<tabsize>]` | Turn runs of spaces in the column range that reach a tab stop into tabs (default 8; like `unexpand -a`) |
| `--reverse-chars` | Reverse the characters in the column range, like `rev` |
| `--pad-left <width>` | Pad the column range with spaces on the left to `<width>`, right-aligning it (each field on its own) |
| `--pad-right <width>` | Pad the column range with spaces on the right to `<width>`, left-aligning it (each field on its own) |
| `--number` | Number the output rows, like `nl` |
| `--split-on <sep>` | Split every line at each occurrence of `<sep>`, one row per piece |
| `--wrap <width>` | Wrap every line into chunks of at most `<width>` characters (like `fold -w`) |
| `--graphemes` | Count columns and `--wrap` widths in user-perceived characters (grapheme clusters) |
| `--drop-empty` | Drop lines that are empty after the other transforms ran |
| `--join <sep>` | Join every row into a single row, separated by `<sep>` (like `paste -s`; `--join ''` runs them together) |
| `--count` | Summarize: how many rows |
| `--sum <cols>` | Summarize: the total of the numbers in those columns |
| `--avg <cols>` | Summarize: the mean of the numbers in those columns |
| `--min <cols>` / `--max <cols>` | Summarize: the smallest / largest number in those columns |
| `--median <cols>` | Summarize: the middle number in those columns (the mean of the two middle ones for an even count) |
| `--distinct <cols>` | Summarize: how many different values those columns hold |
| `--first <cols>` / `--last <cols>` | Summarize: those columns in the first / last row (of each group) |
| `--group-by <cols>` | Summarize once per distinct value of those columns (requires a summary) |
| `--encoding <name>` | Encoding of the inputs, e.g. `windows-1250`, `latin1`, `utf-16le` (default UTF-8; a byte order mark overrides it) |
| `--output-encoding <name>` | Encoding of the output (default UTF-8; with `--in-place`, each file's own) |
| `--lossy` | Replace bytes that are not valid in the input encoding with U+FFFD instead of failing |
| `-o, --output <file>` | Write to a file instead of stdout |
| `-i, --in-place` | Edit the input files in place (needs files, conflicts with `-o`) |
| `--backup <suffix>` | Keep a copy of each edited file, with this suffix (requires `--in-place`) |
| `--dry-run` | Report which files the edit would change, without writing (requires `--in-place`) |
| `--completions <shell>` | Print a completion script (`bash`, `zsh`, `fish`, `powershell`, `elvish`) and exit |
| `--man` | Print the man page and exit |

### Semantics

- Row and column ranges are **1-based and inclusive**; columns are counted in characters (Unicode code points), not bytes, so multi-byte UTF-8 text is never cut inside a character. They are not grapheme clusters, though: a letter followed by a combining accent (`e\u0301`) or an emoji joined with ZWJ (`👩\u200d💻`) counts as several columns, and a range may split it — unless `--graphemes` is given, which counts grapheme clusters instead: an accented letter, a flag or a ZWJ emoji sequence is then one column, and `--wrap` never cuts inside one. It costs a pass over each non-ASCII line, so it is opt-in; field mode is unaffected (fields are found by their delimiter), apart from `--wrap`.
- A range can be a single number (`3`), open-ended (`10-` to the end, `-5` from the start) or closed (`2-5`). Rows and columns both accept a comma-separated list of ranges (`1-5,10-20`).
- A **column list keeps the order written**, so operations that *read* the columns — selecting them, the `--sort`/`--unique` key, `--grep` — read the parts in that order: `-F , -C 3,1,2` is an `awk`-style projection that reorders the fields, and `--output-delimiter` chooses what rejoins them. Operations that *write into* the line — `--delete`, `--upper`/`--lower`/`--trim`, find/replace — work on the same columns as a set (sorted, overlaps merged), where order carries no meaning; each part is written on its own, so a match straddling two parts is not replaced.
- A row bound prefixed with `~` counts from the **end** of the input: `~1` is the last row, `~10-~1` the last ten, `2-~2` everything but the first and last row. Because the total line count must be known first, end-relative ranges cannot simply stream: when every part starts from the end (`~10-`, `~5-~2`) only the last rows are held, as many as the furthest bound reaches, like `tail`; a range starting from the beginning but ending from the end (`2-~2`) buffers the whole input. Columns do not accept `~`.
- Without `--delete`, the row range **selects** lines: only rows inside the range are output (and transformed). Without a row range, the whole file is processed. With `--keep-others` the rows outside the range are output too, unchanged — the same goes for rows a filter (`--grep`, `--from`/`--to`, `--min-fields`, `--max-len`) leaves out. It cannot be combined with `--delete` (which keeps them already), nor with a summary, `--join` or `--number`.
- A column range with no other operation **selects** columns (like `cut`): only the characters inside the range are output. With `--sort` it is the sort key, with `--find` it scopes the replacement, and with `--delete` it is removed — in those cases the rest of the line is kept.
- `--sort-key`, `--unique-key` and `--grep-key` give those operations a column range of their own, so `--cols` is free for another one: `-C 5 -f a -r b --sort --sort-key 3` replaces inside column 5 but sorts by column 3. An operation with its own key no longer claims `--cols`, so a `--cols` left over with no other operation goes back to **selecting** columns. The sort and unique keys then address that selected result, since they are read from the transformed line; the grep key addresses the line *as read*, since the filter runs before any transform — so `-F , -C 2 --grep-key 1 -g ERROR` keeps the rows whose field 1 matches and prints their field 2.
- With `--fields`, the column ranges count **delimited fields** instead of characters: `-F , -C 2` addresses the second comma-separated field, per line. `--fields` needs no column option: without one, every field is in the span, and the delimiter still says how a row splits (`-F , --quoted --count` counts CSV records, `-F , --max-len 80` measures them) and what separates `fxt`'s own columns (`-F , --number`). All column-based operations (select, delete, sort key, find/replace scope, `--grep`, `--unique`, case/trim) work on fields the same way, and those that rewrite text — case, `--trim`, `--squeeze`, `--expand`, `--reverse-chars`, find/replace, padding, the codecs — rewrite each field on its own, so `-C 1-` trims every field and a match never reaches across a delimiter; deleting fields also removes one adjacent delimiter, like `cut`, and a field list is merged before that, so `-C 2,3` deletes exactly what `-C 2-3` does. Selected fields are rejoined by the delimiter (or `--output-delimiter`, which then stands between *every* two fields, `-C 1-3` included), and a field the line does not have is skipped rather than joining a stray delimiter. A row nothing was selected from — no `--cols`, or `--cols` taken by an operation like `--upper` or `-d` — gets the output delimiter between all its fields once it is otherwise done, so `fxt -F , --output-delimiter ';'` converts a CSV to semicolons and `-F , -d -C 2 --output-delimiter ';'` writes `a;c`; sort, unique and grep keys are read with the input delimiter, whatever the output's, so `a,b` and `a;b` stay two different rows. Under `--quoted` a field that holds the new delimiter is quoted (`a;b,c` → `"a;b";c`), so the output is still CSV, and a delimiter containing `"` is refused; without it fields are plain text, written as they are. `--group-by` and `--distinct` also compare fields as read: `a;b`+`1` and `a`+`b;1` are two groups even where both print as `a;b;1`. Rows passed through unchanged (`--keep-others`, `--delete` of whole rows) keep theirs; `--join` gets the rows rejoined, and a summary uses it only between its own columns. The delimiter may be more than one character.
- `--quoted` makes field mode read CSV rather than a plain split: a delimiter inside a `"…"` field no longer splits it, and a doubled `""` is an escaped quote (RFC 4180). A field keeps its quotes — they are part of the text it occupies — so selecting or reordering fields re-joins them into valid CSV, and deleting one takes its quotes with it. Operations that read a field as a *number* (`--sort --numeric` and the `--sum`/`--avg`/`--min`/`--max`/`--median` summaries) read its value instead, without the quotes, so `"10"` counts as 10. A record may span several lines: a newline inside quotes belongs to the field, so under `--quoted` a row is a whole *record* — `--rows`, `--number`, `--count` and error messages count records, not lines, and the newline comes out with the field unchanged. A quoted field still open at the end of the input fails with exit code 2 rather than swallow the rest of the file.
- With `--delete`, the row range is **removed**: rows outside the range pass through unchanged. Adding a column range deletes only those columns inside the selected rows.
- Find/replace only replaces occurrences that lie entirely inside the column range — in field mode, inside one field.
- `--find` and `--replace` may be repeated to run several substitutions: the *n*-th `--find` pairs with the *n*-th `--replace`, and the pairs apply left to right, so a later pair can rewrite what an earlier one produced. Every `--find` must have its own `--replace` and vice versa (a lone `--find` is rejected — use `--grep` to filter rows by content instead).
- `--grep` filters rows by content, complementing the positional row range: only rows inside `--rows` *and* matching the pattern are processed. With `--delete`, matching rows are deleted instead. The match is scoped to the column range.
- `--grep` may be given several times: a row then matches when any of the patterns does, like `grep -e A -e B`, or — with `--all` — only when every one does, in any order (`-g ERROR -g disk --all`). `--invert` turns the combined answer around, so `-g a -g b --all -v` keeps the rows that lack at least one of them.
- `--min-fields <n>` keeps the rows that have at least `<n>` fields, split as `--fields` splits them (`--quoted` included; an empty line is one empty field), and `--max-len <n>` the rows at most `<n>` characters long (grapheme clusters under `--graphemes`), measured without the line terminator. They are content filters like `--grep`: they combine with it and with each other (a row must pass all of them), with `--delete` they pick the rows to delete, and when nothing passes the exit code is 1, and `fxt -d --max-len 120 notes.md` prints the lines that are too long.
- Regular expressions (`--grep`, `--regex --find`, `--from`, `--to`) use the `regex` crate by default, which matches in time linear in the input whatever the pattern — and so has no backreferences or lookaround. `-P` switches all of them to a backtracking engine (`fancy-regex`) that has both: `(.)\1`, `(?<=\$)\d+`, `(?!draft)`. Capture groups in a replacement are written `$1` or `${name}` either way. A pattern that backtracks without end gives up after a million steps; that is an error (exit code 2), never a silent miss.
- `--from`/`--to` address rows by content, like `sed '/START/,/END/'`: a block runs from a line matching `--from` through the next line matching `--to`, both included. The end is looked for from the line *after* the start, so `--from '^\[db' --to '^\['` takes the `[db]` section up to and including the next header, and once a block ends the next `--from` match opens another — every block is selected. `--from` alone runs to the end of the input; `--to` alone runs from the first line through the first match (which may be the first line itself, like `sed 0,/END/`). Blocks combine with the other filters: a row is processed when it lies in `--rows` *and* in a block *and* matches `--grep`, so `--rows` and `--grep` narrow a block rather than extend it. The patterns see whole lines (not the column range), honour `--ignore-case`, and are matched as bytes, so rows outside every block pass through even if they are not valid UTF-8.
- `--upper`, `--lower`, `--title-case`, `--squeeze` and `--trim` apply to the column range (the whole line without one) and run after find/replace, so replaced text is transformed too. They cannot be combined with `--delete`. `--squeeze` runs before `--trim`, so `--squeeze --trim` normalizes the whitespace of a line completely. `--title-case` starts a word at every letter or digit that follows anything else — a space, but also `-` or `'` — so `foo-bar o'neil` becomes `Foo-Bar O'Neil`. A letter is a whole grapheme cluster, so an accent combined with it (`e` + U+0301, as macOS writes file names) does not end the word, and the rest of a word is lowercased as a whole, so a Greek final sigma comes out as `ς`. The start of a column range counts as the start of a word: `-C 3- --title-case` on `abcd` gives `abCd`. The first letter gets its *titlecase*, which is not always its uppercase: a ligature or a digraph capitalizes only its first part (`ﬁsh` → `Fish`, `ǆungla` → `ǅungla`), and a word-initial `ß` becomes `Ss`.
- `--encode <codec>` writes the column range (the whole line without one) as `base64` (RFC 4648, padded), `hex` (lowercase) or `url` (RFC 3986 percent encoding: everything but `A-Z a-z 0-9 - . _ ~` as `%XX`), taking the text as its UTF-8 bytes; `--decode <codec>` reads it back. In field mode each field is converted on its own, empty ones too. Decoding accepts base64 with its padding or without it — but not half of it: `YQ==` and `YQ` are `a`, `YQ=` is an error — and hex in either case, and leaves a `+` alone (only HTML forms mean a space by it); anything else that is not valid in the codec fails the run with exit code 2, and so do decoded bytes that are not UTF-8 — unless `--lossy`, which repairs those. Decoding runs first, before find/replace, and encoding last of the column transforms, so `--decode base64 --encode hex` converts from one to the other.
- `--expand` replaces every tab in the column range with spaces up to the next tab stop, and `--unexpand` turns the blanks that reach a stop back into tabs: a run of two or more spaces ending at one, or spaces right before a tab (a lone space stays a space, so the gaps between words survive — unless another blank follows it; the output matches GNU `unexpand -a`). Stops come every 8 columns, or every `<tabsize>` given as `--expand=4` — with the `=`, so a bare `--expand` never takes the file name after it. The stops are counted from the start of the column range (of each field, in field mode), which is the line without one; columns are characters.
- `--reverse-chars` reverses the column range (the whole line without one), like `rev`; in field mode each field is reversed in place, so `-F , -C 2 --reverse-chars` turns `abc,żółw` into `abc,włóż`. It counts characters, so a combining accent ends up on the letter before it; under `--graphemes` it reverses grapheme clusters and the accent stays where it belongs. It runs after `--trim`, before padding.
- `--pad-left <width>` and `--pad-right <width>` pad the column range with spaces to at least `<width>` characters (grapheme clusters under `--graphemes`), on the left (right-aligning the text) or on the right; text already that wide is left whole, never cut. An empty line is padded too, as one empty cell (unless the column range starts further in). In field mode every field of the range is padded on its own, empty ones included, so `fxt -F , -C 2-3 --pad-left 8` right-aligns two columns of numbers; under `--quoted` the quotes count as part of the field. Padding runs after `--trim`, so `--trim --pad-left 8` aligns the trimmed text.
- `--number` prefixes each **output** row with its number (separated by `--output-delimiter`, else `--fields`, else a tab). It counts the rows it actually emits — after the filters, after `--split-on`/`--wrap` expanded them, after `--drop-empty` removed some and after `--unique` dropped the duplicates (the number is no part of the key) — so the numbers are always contiguous. It cannot be combined with a reordering, which would shuffle the numbers along with the rows.
- `--wrap` cuts every processed line into chunks of at most `<width>` **characters** — one row in, several rows out. It runs after the other transforms, so the chunks are cut from the finished line. If the input's last line had no terminator, neither does the last chunk.
- `--split-on` cuts every processed line at each occurrence of the separator, turning one row into one row per piece (`tr , '\n'`, but only on the rows being processed). It runs after the column-scoped transforms and before `--wrap`.
- `--join` folds every processed row into a single row — the inverse of `--split-on`, and the two compose: `fxt --split-on , --join ,` returns what it was given. Like a summary, it consumes the rows, so it cannot be combined with one (or with `--delete` or `--number`).
- **Summaries** (`--count`, `--sum`, `--avg`, `--min`, `--max`, `--median`, `--distinct`, `--first`, `--last`) *replace* the rows they summarize: the rows are consumed and only the summary is printed. They see exactly the rows that survive `--rows`, `--grep` and `--unique`, so `--unique --count` counts the distinct rows. Add `--group-by <cols>` for one summary row per distinct key, printed in the order the keys first appear. Under `--quoted` the key is each field's *value*, quoted again only where CSV needs it (the value holds the delimiter, a quote or a line break), so `"a"` and `a` are one group, printed as `a`, while `"x,y"` stays `"x,y"`. The output columns are the key (if any), then the count, sum, avg, min, max, median, distinct, first and last that were asked for, separated by `--output-delimiter`, else `--fields`, else a tab. A value that is not a number takes no part in the statistics (it is not a zero), so a group with no numbers at all shows an empty average, minimum, maximum and median. A median keeps the integers exact — the median of 2 and 3 is `2.5`, of 2 and 4 `3` — but has to hold every value of a group in memory until the end. `--distinct`, `--first` and `--last` read text rather than numbers: every row counts, an empty value too, and under `--quoted` they compare and print values the way `--group-by` does. Without `--group-by` the summary always appears, even over no rows at all — `fxt --count -g X` prints `0` when nothing matches, like `grep -c` (and still exits 1); with `--group-by`, no rows means no groups and no output. Summaries cannot be combined with `--delete` or `--number`, which would have nothing left to act on; a reordering is fine — the reducer takes the rows in the order they come out, so `--sort --group-by` reports the groups in sorted order and `--sort --join` folds them sorted.
- `--drop-empty` removes lines that are empty *after* the transforms ran — which `--grep --invert` cannot do, since a predicate sees the line as it was read. `fxt -C 3 --drop-empty` drops the rows too short to reach column 3, and `--trim --drop-empty` drops whitespace-only lines.
- Under `--quoted`, every key reads field *values*: the text and numeric sort, `--unique` and `--group-by` alike, so `"b"` sorts between `a` and `c` rather than first on its quote. The rows themselves are written as they were read.
- Numeric sort parses the sort key as a number (integer or decimal); lines whose key is not a number sort before all numeric lines (after them, with `--reverse`, as in `sort -rn`). Integers compare exactly at any size up to 128 bits, so keys beyond 2^53 — where a floating-point key would round neighbours together — still sort correctly, and so do integers against decimals. Sums stay exact integers too, for as long as every value is one.
- `--unique` keeps the first row per key (the column range, or the whole line without one) and drops later duplicates; combined with `--sort`, "first" means first in sorted order, like `sort -u`. Under `--quoted` it compares field *values*, like `--group-by`: `"a"` and `a` are duplicates (the row kept is written as it was read).
- `--sort`, `--tac` and `--shuffle` are mutually exclusive reordering operations; each buffers the selected rows before writing them out. They cannot be combined with `--delete` on whole rows (the rows would be removed, not reordered); combining them with `--delete --cols` is fine, since there `--delete` removes columns.
- Original line endings (LF or CRLF) are preserved. Where `fxt` writes a terminator of its own — after an unterminated last line that a reordering, `--wrap` or `--split-on` puts something behind, at the end of a `--join`ed row, after each summary row — it uses the one the input's lines end with (the last one read; `\n` if there was none), on every platform. A `--join`ed row ends the way the input did, so it is unterminated when the input's last line was; a summary row is always terminated.
- Text is UTF-8 unless said otherwise. `--encoding <name>` decodes the inputs from another encoding — any [WHATWG label](https://encoding.spec.whatwg.org/#names-and-labels): `windows-1250`, `latin1`, `iso-8859-2`, `shift_jis`, `utf-16le`, … — and `--output-encoding <name>` writes the output in one; by default the output is UTF-8, so `fxt --encoding windows-1250 -R 1- old.csv > new.csv` converts a file. A byte order mark settles the encoding by itself: a UTF-16 file with its BOM is read as UTF-16 without any flag, whatever `--encoding` says.
- `--in-place` writes each file back in the encoding it was read in, BOM included, so editing never converts a file — unless `--output-encoding` asks for it (`fxt -i --encoding cp1250 --output-encoding utf-8 -R 1- *.csv` converts them where they are). Nothing is replaced silently: bytes that are not valid in the input encoding, or a character the output encoding has no byte for (`'日' (U+65E5) cannot be written in windows-1250`), fail the run with exit code 2 (the first, unless `--lossy`), and an in-place edit that fails leaves the file as it was.
- `--lossy` makes invalid input a repair instead of an error: each malformed sequence becomes U+FFFD (`�`). In another encoding the whole input is decoded that way. In UTF-8 a row is repaired only when `fxt` rewrites it — a row outside `--rows`, one only filtered by `--grep` or one no transform changed goes out with its original bytes, like `grep -a` — so `fxt --lossy -i -f a -r b *.txt` changes nothing in the files but the replacements (and `--dry-run` reports the files it would not touch as unchanged). Sorting, summaries and `--join` see the repaired text. The output side stays strict: a character the output encoding has no byte for is still an error.
- A byte order mark (BOM) at the start of an input is taken off before processing — of every input file, not just the first — so it does not count as a column or keep the first field from matching. Streaming output carries none; `--in-place` writes it back, in the form of the encoding the file is written in, and `--dry-run` does not report the mark as a change.
- Several input files are read **as one stream**, in the order given (like `cat a b | fxt`), so a row range addresses the concatenation. `--in-place` is the exception: it edits each file on its own, so row 1 means row 1 *of each file* — which is what makes `fxt -i -f foo -r bar *.txt` a batch edit.
- **`--in-place` writes back what `fxt` would print**, which is why it refuses a summary and `--join` (each file would be replaced by it), so a selection keeps only the rows it selects there too: `fxt -i -R 2 -f b -r X notes.txt` leaves a file of one row, and `fxt -i -g ERROR app.log` keeps only the errors. To edit some rows and keep the others as they are — what `sed -i '2s/b/X/'` does — add `--keep-others`: rows outside `--rows`, `--from`/`--to` or a filter then pass through unchanged, and only the selected ones are transformed or reordered (`fxt -i --keep-others -s -R 2- data.csv` sorts all but the header). A selection that takes no row of a file leaves that file untouched instead of writing it back empty, with a note on stderr: a filter that matches nothing (exit code 1), but also a `--rows` past the end of a short file or a `--from` never found; `--dry-run` shows what an edit would touch before it happens.
- `--in-place` rewrites the input file itself: the result is written to a temporary file in the same directory and then atomically renamed over the original, so an interrupted run never truncates the input. The original file's permissions are preserved. It needs real input files (not stdin) and cannot be combined with `--output`.
- `--backup .bak` keeps the original as `<file>.bak` before the swap; `--dry-run` writes nothing at all and instead reports, per file, whether the edit *would* change it — so a batch edit can be checked before it happens.
- `--replace` cannot be combined with `--delete`, and `--delete` requires a row or column range.

### Order of operations

The flags describe *what* to do, not in which order: whatever order they are written in, `fxt` always runs the same pipeline.

1. **Row range** (`--rows` and the `--from`/`--to` blocks), then the **content filters** (`--grep`/`--all`/`--invert`, `--min-fields`, `--max-len`) — on the line *as it was read*, before any transform.
2. **Transforms**, in this fixed order: column delete → `--decode` → find/replace pairs → `--upper` → `--lower` → `--title-case` → `--expand`/`--unexpand` → `--squeeze` → `--trim` → `--reverse-chars` → `--pad-left`/`--pad-right` → `--encode` → `--split-on` → `--wrap` → `--drop-empty`.
   Some neighbours in this list exclude each other rather than run in turn: `--upper`/`--lower`, `--expand`/`--unexpand` and `--pad-left`/`--pad-right`. `--number` cannot be combined with `--delete`, a reordering, a summary or `--join`.
3. **`--unique`**, on the transformed line, then **`--number`**, on the rows `--unique` let through.
4. **Reordering** (`--sort`/`--tac`/`--shuffle`), then a **reducer** (`--join` or a summary) instead of writing the rows.

So `--upper -g error` finds nothing in a line reading `ERROR` — the filter sees the original text. Use `--ignore-case` or a pattern matching the input as it is. (Not `-i`: that is `--in-place`.)

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
| `0` | Rows matched (a run without a content filter — `--grep`, `--from`, `--to`, `--min-fields`, `--max-len` — has nothing that could fail to match, so it always succeeds) |
| `1` | A content filter was given and no row matched |
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

# Backreferences and lookaround (-P)
fxt -P -g '(.)\1' words.txt                       # lines with a doubled letter
fxt -P -e -f '\b(\w+) \1\b' -r '$1' notes.txt    # "the the" -> "the"

# Several substitutions in one pass (applied left to right)
fxt -f cat -r dog -f red -r blue input.txt

# Uppercase columns 1-3, trim whitespace around every line
fxt --upper -C 1-3 input.txt
fxt --trim input.txt

# Decode a base64 field; percent-encode a URL path
fxt -F , -C 3 --decode base64 data.csv
fxt --encode url paths.txt

# Right-align the numbers in fields 2-3 to a width of 8
fxt -F , -C 2-3 --pad-left 8 data.csv

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
fxt -F , --group-by 1 --median 3 --distinct 2 --last 4 data.csv

# Drop blank lines; drop rows left empty by cutting a column
fxt --trim --drop-empty input.txt
fxt -C 3 --drop-empty input.txt

# Rows with ERROR or WARN; rows with both "disk" and "full"; CSV rows with all 5 fields
fxt -g ERROR -g WARN app.log
fxt -g disk -g full --all app.log
fxt -F , --quoted --min-fields 5 data.csv

# Lines longer than 100 characters (delete the ones that fit)
fxt -d --max-len 100 notes.md

# Keep only rows containing ERROR; delete rows that match a pattern
fxt -g ERROR app.log
fxt -d -g '^#' config.txt

# Blocks addressed by content, like sed '/A/,/B/'
fxt --from '^\[db\]' --to '^\[' app.ini       # the [db] section and the header after it
fxt --from BEGIN --to END dump.sql             # every BEGIN…END block
fxt -d --from '^<!--' --to '-->$' page.html    # delete comment blocks
fxt --to '^$' mail.txt                         # the header lines, through the first blank one
fxt --from '^\[db\]' --to '^\[' -g = -s app.ini  # the section's key=value lines, sorted

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

# Sort rows 2-100 and print just them; with --keep-others the rest stays around them
fxt -s -R 2-100 input.txt
fxt -s -R 2- --keep-others data.csv          # sort everything below the header
```

## Building

Requires a stable [Rust](https://rustup.rs/) toolchain (edition 2024).

```bash
cargo build --release    # binary in target/release/fxt
cargo test               # unit + CLI integration tests
cargo clippy --all-targets -- -D warnings
cargo install --path .   # put fxt on the PATH (~/.cargo/bin)
cargo bench              # engine benchmarks (criterion), reports in target/criterion
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

- `src/main.rs` — the binary: files, stdin/stdout, `--in-place`, `--dry-run`, exit codes
- `src/cli_args/` — clap definition and validated `Config` (`TryFrom<ArgMatches>`)
- `src/compose.rs` — turns a `Config` into a processor; the only place that knows how options map onto engine parts
- `src/file_processor.rs` — the streaming engine, generic over `BufRead`/`Write`: row selection, filtering, reordering
- `src/transform.rs` — `LineTransform` trait and the per-line operations (one line in, zero or more out)
- `src/predicate.rs`, `src/pattern_range.rs` — the row filters (`--grep`, `--min-fields`, `--max-len`), and `--from`/`--to` blocks
- `src/reduce.rs` — `LineReducer`: `--join` and the summaries
- `src/columns.rs`, `src/ranges.rs`, `src/records.rs` — column spans (chars, graphemes, fields, quoted CSV), row ranges, multi-line CSV records
- `src/text.rs`, `src/number.rs`, `src/pattern.rs`, `src/encoding.rs`, `src/codec.rs` — char-indexed text helpers, exact numbers, the two regex engines, text encodings, base64/hex/url
- `tests/cli.rs` — end-to-end tests running the real binary; property tests (`proptest`) sit next to the parsers and text helpers
- `benches/engine.rs` — `criterion` benchmarks of the engine
