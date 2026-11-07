# Roadmap — `fxt`

Date: 2026-07-12. Ideas for growing `fxt` into a general-purpose file transformation tool, grouped by what each one costs in the current architecture.

## Done

Implemented in the order suggested below, one commit per item:

- ✅ **Per-operation column keys** — `--sort-key`, `--unique-key`. An operation with its own key no longer claims `--cols`, so a left-over `--cols` goes back to selecting columns.
- ✅ **Column lists and permutation** — `-C 1,3,5-7`; `-F , -C 3,1,2` is an `awk`-style projection. Reading honours the written order, writing normalizes. Plus `--output-delimiter`.
- ✅ **`LineOutcome`** — `LineTransform` is no longer 1:1 (`Keep` / `Replace` / `Expand` / `Drop`); `Pipeline` folds a line into `Lines`. Delivered `--wrap` and `--drop-empty`.
- ✅ **Quoted CSV** — `--quoted` (RFC 4180): a delimiter inside `"…"` no longer splits, a doubled quote escapes one, fields keep their quotes so a projection stays valid CSV.
- ✅ **Multiple input files** — `Vec<Input>`; streaming concatenates like `cat`, `--in-place` edits each file on its own (the batch edit).
- ✅ **`--backup` and `--dry-run`** — a copy kept before the swap, and a per-file "would change / unchanged" report that streams (`CompareWriter`) instead of buffering.
- ✅ **`--split-on`** — one row in, one row per piece out (the other half of `Expand`).
- ✅ **Aggregations** — `LineReducer`, the third engine abstraction: `--count`, `--sum`, `--avg`, `--min`, `--max`, optionally per `--group-by`. A summary replaces the rows it summarizes.
- ✅ **Stateful transforms** — `LineTransform::apply` takes `&mut self`, so `--number` can count the rows it emits. `--title-case` and `--squeeze` came free as `MapColumns` constructors.
- ✅ **`grep`-like exit codes and `--quiet`** — 0 matched / 1 nothing matched / 2 failed; `--quiet` stops at the first match and answers with the exit code alone.
- ✅ **`--join`** — the N→1 direction, a streaming `LineReducer` (`accept` takes the writer). The inverse of `--split-on`.
- ✅ **Reordering + reducers** — the reorder buffer drains through the same output path as streamed lines, so `--sort --join` folds sorted and `--sort --group-by` reports groups in sorted order.
- ✅ **Exact numbers** — `Number` (`Int(i128)` exact, `Float(f64)` otherwise, compared exactly across the two) replaces the bare `f64` in the numeric sort key and the summaries, so keys beyond 2^53 sort correctly and integer sums stay exact. *(2026-11-06 comparison)*
- ✅ **Multi-line CSV records** — under `--quoted` the engine reads records, not lines (`records::Records` joins lines while a quote is open), so a newline inside quotes stays in its field and `--rows`/`--number`/`--count` count records; only a quote never closed fails.
- ✅ **Quoted CSV values** — `ColumnSpan::select_value` reads a field's value without its quotes; numeric operations use it, so `"10"` sums and sorts as 10. Lexical ones still read the field as written, which keeps projections valid CSV. *(2026-11-06 comparison)*
- ✅ **Multi-line CSV boundary** — under `--quoted`, a line that leaves a quoted field open fails with exit code 2 instead of returning records cut in half. *(2026-11-06 comparison)*
- ✅ **`--grep-key`** — `--grep` gets a column range of its own, so `--cols` is free to select: `-F , -C 2 --grep-key 1 -g ERROR`. Unlike the sort and unique keys it addresses the line as read, since the predicate runs before the transforms. *(2026-11-06 comparison)*
- ✅ **Quoted `--group-by` keys** — `ColumnSpan::select_canonical` reduces each field to its value and quotes it again only where needed, so `"a"` and `a` are one group, printed as `a`, and the key stays valid CSV. Being unambiguous, it keeps `a,"b,c"` and `"a,b",c` apart. *(2026-11-06 comparison)*
- ✅ **UTF-8 BOM** — `encoding::open` takes the mark off every input before they are joined, so it no longer hides the first field; `--in-place` (and so `--dry-run`) writes it back, other output carries none. *(2026-11-06 comparison)*
- ✅ **Order of operations documented** — README section on the fixed pipeline and the shared `--cols` scope, with examples. *(2026-11-06 comparison)*
- ✅ **Renamed to `fxt`** — PowerShell aliases `ft` to `Format-Table`, which shadowed the binary.
- ✅ **Quoted `--unique` keys** — `--unique` keys on `select_canonical` too, so under `--quoted` `"a"` and `a` are duplicates, as they are one `--group-by` group.
- ✅ **Quoted text-sort keys** — the text sort reads `select_value` like the numeric one, so under `--quoted` `"b"` sorts between `a` and `c` instead of first, on its quote. With `--group-by`, `--unique` and the numeric sort, every key under `--quoted` now reads field values.
- ✅ **Regex backreferences** — `-P`/`--perl-regexp` compiles every regex with `fancy-regex` (backreferences, lookaround); the default stays the linear-time `regex`. Behind `pattern::Pattern`/`BytePattern`, so `--grep`, `--find`, `--from`/`--to` share it. A backtracking match can fail at run time, so `LinePredicate::matches`, `LineTransform::apply` and `PatternRange::admits` now return `io::Result` — the failure ends the run instead of reading as a miss. *(2026-11-06 comparison)*
- ✅ **Grapheme columns** — `--graphemes` counts char columns and `--wrap` widths in grapheme clusters (`unicode-segmentation`). A `ColumnSpan::Graphemes` resolves its columns to char ranges per line, as field mode does, so not one char helper in `text.rs` changed; ASCII lines without a CRLF borrow the ranges as they are. *(2026-11-06 comparison)*
- ✅ **More summaries** — `--median` (exact for integers: halves added back rather than a sum that may overflow), `--distinct`, `--first`, `--last`, per group too: four more `Aggregate`/`Accumulator` variants, no engine change. The text ones read `select_canonical`, like `--group-by`.
- ✅ **More predicates** — `--grep` repeatable (`GrepPredicate::any_of`/`all_of`, `--all`), `MinFields` (`--min-fields`, counting no further than it needs: `FieldSpan::count_fields`), `MaxLength` (`--max-len`); `AllOf` joins whatever filters are given. They count as content filters for the exit code and `--quiet`.
- ✅ **`--encode` / `--decode`** — `codec::Codec` (base64, hex, url, by hand: a few lines each, held by a round-trip property), applied per cell by `CodecColumns` through `text::try_map_cells`. Decoding is fallible twice over: the codec, then UTF-8, which `--lossy` repairs.
- ✅ **`--expand` / `--unexpand`** — two more `MapColumns` constructors; the mapping became a boxed closure so it can carry the tab size. A property holds `unexpand` to changing nothing `expand` would show.
- ✅ **`--reverse-chars`** — one more `MapColumns` constructor (two: by chars, by grapheme clusters).
- ✅ **Padding** — `--pad-left`/`--pad-right` (`PadColumns`). Padding writes into each field separately, which neither the write ranges (adjacent fields merge into one stretch) nor the char helpers (an empty field resolves to nothing) could do, so a span now also resolves to *cells* (`ColumnSpan::cell_ranges`: every field of the span, empty ones as inverted ranges) and `text::map_cells` writes into those, empty ones included.
- ✅ **Lossy mode** — `--lossy` replaces what is not valid in the input encoding with U+FFFD instead of failing (`encoding::Malformed`). Other encodings are decoded with replacement; UTF-8 passes through unchecked and the engine repairs a row only when it processes it — and writes the original bytes when the pipeline left the row as it was and it streams straight out, so a pass-through, a filter or a missed replacement keeps them (`--lossy -i --dry-run` reports such a file unchanged). The output side stays strict: a character the output encoding lacks is still an error.
- ✅ **Encodings** — `--encoding <label>` decodes the inputs from any WHATWG encoding (`encoding_rs`), a UTF-16/UTF-8 BOM settles it by itself, and `--output-encoding` writes the output in one. `--in-place` writes each file back as it was read (BOM included) unless `--output-encoding` converts it. Undecodable input and unmappable output are errors, never replacements. Done at the edges (`encoding::open`, `EncodeWriter`), so the engine stays UTF-8 only.
- ✅ **Pattern-addressed rows** — `--from`/`--to`, `sed '/A/,/B/'` semantics (inclusive, end looked for from the next line, every block selected; `--to` alone like `sed 0,/B/`). Not a `RangeBound` variant after all: `RangeSpec` resolves up front, this cannot, so it is its own stateful `PatternRange`, told every line in order and intersected with `--rows`. Matched as bytes, so rows outside a block need not be UTF-8.
- ✅ **Property tests** — `proptest` modules in `columns.rs` and `text.rs` check the field splitter against the char-by-char one it replaced and against `str::split`, quoted values round-tripping through CSV, and the char helpers against a `Vec<char>` reference (`select(R)` put back into `delete(R)` rebuilds the line), on lines of multi-byte chars, quotes and multi-char delimiters. Deliberately broken versions of the byte walk and of the ASCII fast path were caught at once.
- ✅ **Benchmarks** — `benches/engine.rs` (`criterion`): the engine on 100k generated log and CSV rows, in memory, per operation; baseline under *Performance* below.
- ✅ **Completions, man page, help examples** — `--completions <shell>` (`clap_complete`) and `--man` (`clap_mangen`) are exclusive flags of the binary itself, so they work after a plain `cargo install`; `after_help` lists examples of the newer features.

## 1. Free slots: a new `LineTransform` / `LinePredicate` (no engine changes)

Each is one new type plus one branch in `compose::build_pipeline` (or `build_predicate`). None are queued at the moment; candidates that would fit: `--min-len`/`--max-fields` (the other bounds), `--center`, `--rot13`.

## 2. Still open

- The bugs and usability findings of the 2026-11-07 functional tests — see section 3a.
- **`~N` ranges at the end only** — `-R '~1000-~1'` buffers the whole input to learn the line count (38 ms on 300k rows, where `tail -n 1000` takes 17). A row spec whose every part lies in the last N rows needs only a ring of the last N rows (and rows outside it, which are dropped, need no buffering at all); `tail` also seeks from the end of a regular file, which a stream reader cannot.
- **Text mappings that build a `String` per line** — `--reverse-chars` (269 ms on 300k rows), `--pad-left` on a field (214), `--expand` (192) and `--encode base64` on a field (191) are the slowest transforms: each mapping returns a new `String` per range, which `map_ranges` then copies into the rebuilt line. Writing straight into the rebuilt line, as the literal `--find` does since its rework, and keeping a line no mapping changes (`Keep`), should bring them near the 140 ms of a replacement.

See *Free slots* above for small additions.

## 3. From the 2026-11-06 comparison with grep/sed/sort/cut/awk

Open findings of an external black-box comparison (the fixed ones are under *Done*):

- ✅ **Partial output before an encoding error** — a streaming run writes the valid lines before failing on invalid UTF-8 (exit 2). Inherent to streaming; `--encoding` answers it for files in a known encoding, `--lossy` for the rest.
- ✅ **Performance** — `fxt` beat `sed` on replacement and `awk`/pipelines on combined operations, but trailed `grep` on plain filtering and `sort` on whole-line sorting (300k rows, Windows). Re-measured 2026-11-07 with 0.4.0 (same 300k rows, median of 11): `fxt` now leads all five — filter 38 ms vs `grep` 40, replace 135 vs `sed` 539, whole-line sort 182 vs `sort -s` 207, filter + replace + uppercase 48 vs `awk` 384, the same as a pipeline 49 vs 110. `cargo bench` now measures the engine in memory; the baseline (100k rows, release build, Windows, 2026-11-06) points at field mode rather than at `--grep` or the sort:

  | Case | Throughput |
  |---|---:|
  | pass-through | 1.58 GiB/s |
  | `-g ERROR` | 420 MiB/s |
  | `-C 10-20` | 366 MiB/s |
  | `-g -f -r --upper` | 279 MiB/s |
  | `-s` | 174 MiB/s |
  | `-f foo -r bar` | 159 MiB/s |
  | `-e -f … -r …` | 128 MiB/s |
  | `-F , --quoted -C 3,1` | 79 MiB/s |
  | `-u --unique-key 7` (fields) | 76 MiB/s |
  | `-F , -C 3,1` | 73 MiB/s |
  | `-s -n --sort-key 9` (fields) | 62 MiB/s |
  | `--group-by 1 --count --sum 3` | 38 MiB/s |

  First round of field-mode work (2026-11-06): `field_positions` walks bytes instead of decoding chars and stops once the fields asked for (plus one) are known, `byte_range` skips the char walk on ASCII lines, and `--group-by` allocates a key only for a new group. Release binaries on 1M rows, best of 7 interleaved runs, identical output:

  | Case | Before | After |
  |---|---:|---:|
  | `-F , -C 1` | 386 ms | 267 ms |
  | `-F , -C 3,1` | 457 ms | 365 ms |
  | `-d -F , -C 2` | 494 ms | 345 ms |
  | `-F , --sum 3` | 406 ms | 265 ms |
  | `--group-by 1 --count --sum 3` | 725 ms | 395 ms |
  | `-u --unique-key 7` | 703 ms | 327 ms |
  | `-s -n --sort-key 9` | 1156 ms | 796 ms |
  | `--count` (control) | 91 ms | 92 ms |

  A summary used to split the same line once per aggregate plus once for the key; it now splits it once (`SplitLine`), shared by the key and every aggregate: `--group-by 1 --sum 3` 555 → 341 ms, `--quoted` 629 → 395 ms, four aggregates 660 → 512 ms; summaries without a key, with nothing to share, lost ~10–16 ns a row (`--count` 88 → 98 ms). Aggregates over the same span used to read and parse its number once each; now once per distinct span (`NumberReadings`): `--sum 3 --avg 3 --min 3 --max 3` 546 → 350 ms, with `--group-by 1` 645 → 428 ms, single aggregates unchanged. Every field selection used to allocate its field positions, its resolved ranges and a copy of the result; now the ranges live inline (`SmallVec`, `SpanRanges`) and a contiguous selection only narrows the line (`LineOutcome::Slice`). 1M rows: `-F , -C 1` 258 → 138 ms, `--quoted -C 2` 342 → 207 ms, `--sum 3` 283 → 214 ms, `--group-by 1 --count --sum 3` 364 → 260 ms, `-C 1-6` 128 → 86 ms (pass-through speed); a permutation still joins new text. a literal `--find` was slower than `--grep` by more than its writing explained — fixed: a line without the literal is now kept uncopied, and a match is written straight into the rebuilt line (1M rows: no match 374 → 243 ms, which is pass-through's 228 ms; 1 row in 10 382 → 259 ms; every row 396 → 366 ms). What remains against `grep` is output volume: a replacement writes every row, a filter only the matches. A text sort used to copy every line into its key (`sort_by_cached_key`) and compare strings that live all over the heap; now a key is borrowed from its line, carries its first eight bytes as a number, and keys are sorted with their positions (ties broken by position, so the unstable sort stays stable), the lines moved once at the end. 300k rows: `-s` 250 → 178 ms, `-s --reverse` 240 → 174 ms, by a quoted CSV field (1M rows) 500 → 439 ms; numeric sorts unchanged. Borrowing alone gained nothing; the time went to comparisons missing the cache. Every buffered row then still had a `String` of its own; now a reorder buffer (`LineBuffer`) keeps all their text in one, with a small record per row that reordering moves instead, and the `~N` buffer is one `Vec<u8>` with row ends. 300k rows, best of 9 interleaved: `--tac` 126 → 111 ms, `--shuffle` 148 → 125, `-s` 229 → 209, `-R ~1000-~1` 46 → 33, a quoted CSV field sort (1M rows) 566 → 499; streaming unchanged. In-process `cargo bench` numbers swing by ±20% on this machine between runs, so compare binaries (best of several interleaved runs) before trusting a change.

  Measured again on 2026-11-07, after the 0.4.0 release and the Unreleased work (300k rows, median of 11 rotated runs, stdout piped, every tool's output identical; ms):

  | Case | `fxt` HEAD | `fxt` 0.4.0 | `ft` 0.1.0 | Reference |
  |---|---:|---:|---:|---:|
  | `-g ERROR` | 40 | 39 | 61 | `grep` 44 |
  | `-f foo -r bar` | 138 | 138 | 169 | `sed` 546 |
  | `-s` | 163 | 160 | 283 | `sort -s` 242 |
  | `-g ERROR -f foo -r bar --upper` | 52 | 51 | 80 | `awk` 423 |
  | the same, against a pipeline | 59 | 54 | 86 | `grep \| sed \| tr` 127 |
  | `--tac` | 120 | 122 | 140 | `tac` 122 |
  | `-R '~1000-~1'` | 38 | 37 | 51 | `tail -n 1000` 17 |

  HEAD and 0.4.0 differ by noise only (the two "uppercase" rows run the same `fxt` command). The features added since, against their nearest MSYS counterpart:

  | Case | `fxt` HEAD | Reference |
  |---|---:|---:|
  | `-g ERROR -g 99999` | 49 | `grep -e … -e …` 52 |
  | `-g ERROR -g '^00' --all` | 41 | `awk '/ERROR/ && /^00/'` 305 |
  | `-F , --min-fields 3` | 66 | `awk -F, 'NF >= 3'` 121 |
  | `--max-len 20` | 35 | `awk 'length($0) <= 20'` 95 |
  | `--expand` | 192 | `expand` 356 |
  | `--unexpand` | 161 | `unexpand -a` 431 |
  | `-F , -C 2 --pad-left 8` | 214 | `awk` with `sprintf("%8s")` 245 |
  | `-F , -C 3 --encode base64` | 191 | — |
  | `--reverse-chars` | 269 | — (`rev` is not in Git for Windows) |

  `fxt` leads every case but `tail`; the two items under *Still open* are what these numbers point at.

## 3a. From the 2026-11-07 functional tests

A black-box run of 525 cases against the documentation (report `fxt-testy-funkcjonalne-2026-11-07.md`, scripts in `fxt-testy-funkcjonalne/`, both next to the repository; `FXT=<binary> bash t3_transforms.sh` runs one file against a given build). On HEAD: 505 pass, 20 fail, every failure one of the items below. When fixing one, carry its cases over into `tests/cli.rs`.

Bugs, roughly in the order to fix them:

- ✅ **`--output-delimiter` is ignored inside a field range** — fixed: a span whose output delimiter differs reads every field of a part on its own (`read_parts`, `FieldSpan::rejoins`), held by a property. Was: `-F , -C 1-3 --output-delimiter ';'` prints `a,b,c`; only the joins *between* list parts use it (`-C 1,2,3` works, `-C 1-2,3` gives `a,b;c`). A field range resolves to one char range with the delimiters inside it. With an output delimiter that differs from the input one, read ranges must resolve per field (as `cell_ranges` does), so every field boundary is rejoined.
- ✅ **CRLF written into LF files on Windows** — fixed: the engine tracks how the input ends its lines (`reduce::InputEnding`, updated on every row read) and `constants::NEW_LINE` is gone; a `--join`ed row ends as the input did, summaries are terminated in its style, and CI also runs on `windows-latest`, with the CLI tests asserting bytes. Was: wherever `fxt` adds a terminator of its own it writes `constants::NEW_LINE`, `\r\n` on Windows, whatever the input uses: `--join` always ends in `\r\n`, and a reordered, split or wrapped unterminated last line puts `\r\n` among `\n`s (`printf 'b\na' | fxt --tac` → `a\r\nb`). Summaries write `\n`, so the program is not even consistent with itself. Use the terminator seen in the input (the last one read, `\n` when there was none) instead of a platform constant, and decide (and document) what `--join` ends with — probably the last joined row's terminator, none if it had none. CI runs on Linux only, where the constant is `\n`, so it never saw this: add a `windows-latest` job, and assert bytes in the tests.
- **`--number -u` keeps duplicates** — numbering is the last transform and `--unique` runs after the transforms, so the number makes every key distinct (and would leave gaps if it did not). Number at emission, after `--unique` (and after a reducer would have taken the row), not as a transform — or make the two conflict, as numbering and reordering do.
- ✅ **Text mappings in field mode work on the whole stretch, not per field** — fixed: in field mode `write_ranges` resolves every field of the span on its own (the cells padding already used), so every rewriting transform works per field; deleting keeps its stretches. Was: `-F , -C 1- --reverse-chars` turns `ab,cd` into `dc,ba` (the README promises each field reversed in place), `--trim` on `-C 1-` trims only the ends of the stretch, `--expand` counts tab stops from its start, a `--find` can match across a delimiter (`-f a,a`). Adjacent list parts merge first, so `-C 1,2` does the same. Padding and the codecs already work per cell (`cell_ranges`, `text::map_cells`); move `MapColumns` and the replacements onto cells too in field mode, and say in the README that field mode works per field. Deleting keeps the merged stretch, which is documented and right.
- **Padding skips an empty line in char mode** — `printf '\n' | fxt --pad-left 3` prints an empty line (field mode pads the empty field). The full span resolves to no char range on an empty line; a cell at `1..=0` should stand for it, as an empty field does.

Usability and validation:

- ✅ **`--in-place` with a row selection drops the other rows** — `fxt -i -R 2 -f b -r X f` left only row 2 in the file, and `-i -g zz` emptied it. Now: `--keep-others` (`RowMode::EditSelected`, everywhere, not only in place) keeps the rows a selection leaves out, as `sed -i '2s/b/X/'` does; an in-place edit whose filter matched nothing leaves the file as it was; the README warns at `--in-place`. The selection semantics themselves stay the same with and without `-i`.
- **`-F` requires a column option** — so `-F , --quoted --count` (count records), `-F , --number` (number separator), `-F , --quoted -s|-u|--tac|--join` and `-F , --max-len` are all refused, with an error listing fourteen options. Let `--fields` stand alone (it still says how rows split and what separates `fxt`'s own columns), or at least say "`--fields` needs something to apply to, e.g. `-C 1-`".
- **Misleading validation messages** — `--min-fields 0` reports "The width must be at least 1" (its parser is the width's); `--join ''` is refused as "The field delimiter cannot be empty", though joining with nothing (`paste -s -d ''`) is a reasonable thing to ask for — accept it.
- **`--decode base64` accepts `YQ=`** — neither the padded `YQ==` nor the unpadded `YQ`. Padding should be all or nothing: two `=` after two digits, one after three.
- **`\\?\` paths in Windows error messages** — `canonicalize` leaks into messages (`Cannot replace input file \\?\C:\…`). Report the path as given, or strip the prefix (`dunce::simplified`, or by hand).
- **An empty `--find ''`** inserts the replacement between every char (`abc` → `xaxbxcx`), as `str::replace` does; refuse it.
- **`--unexpand` and a lone space before a tab** — `abcdefg \tb` keeps the space where GNU `unexpand -a` makes it a tab; the same on screen, but matching GNU costs nothing.

Documentation: ✅ all nine fixed — `-i` advised as "ignore case" (it is `--in-place`), what `-q` and `--ignore-case` apply to, what `--title-case` does, which neighbours in the order of operations exclude each other, where non-numbers sort under `--reverse`, the header example, `-v` in the table, and the old-style `--help` texts of `-s`, `-d` and `-f`.

Observed and consistent with the documentation, noted for awareness: files without a final newline run into the next one in a concatenation (like `cat`); mixed-bound row ranges that resolve inverted (`~1-2`) select nothing, quietly; a `--to` that never matches selects everything (as `sed 0,/re/`); `-P` sends patterns without backtracking features to the linear engine; numbers accept `+5`, ` 7`, `1e3`, `inf` but not `0x10` or `NaN`, and floats print raw (`0.30000000000000004`).

## 4. Tool maturity (cheap, highly visible)

- ✅ Fuzzing the range parser — property tests in `cli_args::cli` (`properties`): arbitrary and hostile input never panics and only spec characters get through (`+2` and `~+1` used to be accepted, via `usize::from_str`; this property now holds that out); generated specs, written out, select exactly the rows their definition names; column lists normalize to what they cover. Property tests rather than `cargo fuzz`, so they run in CI on every platform.
- **Release binaries** — a GitHub Actions workflow that, on a `v*` tag, builds `fxt` for Windows, Linux and macOS and attaches the binaries to a GitHub Release, so installing no longer needs `cargo install`. *Deferred (2026-11-07): first a period of using and testing the 0.4 features.*
