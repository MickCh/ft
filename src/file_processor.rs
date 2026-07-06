//! Streaming orchestrator: applies row selection, the per-line transform
//! pipeline and optional sorting to any `BufRead` source and `Write` sink.

use crate::cli_args::Config;
use crate::columns::ColumnSpan;
use crate::constants::NEW_LINE;
use crate::predicate::{self, LinePredicate};
use crate::ranges::{RangeSet, RangeSpec};
use crate::text;
use crate::transform::{self, LineTransform};

use bstr::io::BufReadExt;
use std::collections::HashSet;
use std::io;
use std::io::prelude::*;
use std::str::from_utf8;

/// A buffered line split into content and its original terminator.
struct Line {
    content: String,
    terminator: String,
}

/// Mutable state threaded through one `run` call.
#[derive(Default)]
struct RunState {
    //lines held back for reordering
    reorder_buffer: Vec<Line>,
    buffer_flushed: bool,
    //unique keys already written (`--unique`)
    seen_keys: HashSet<String>,
}

/// How to order the buffered lines: by which columns, compared
/// lexicographically or numerically, ascending or descending.
struct SortSpec {
    key_span: ColumnSpan,
    numeric: bool,
    reverse: bool,
}

/// A sequence-breaking operation: unlike per-line transforms, it needs
/// the whole row range buffered before anything can be written.
enum Reorder {
    Sort(SortSpec),
    //reverse the order of the buffered lines, like `tac`
    Tac,
    //write the buffered lines in random order
    Shuffle,
}

/// An `Ord` wrapper around the parsed numeric sort key.
/// Lines that do not parse as a number sort before all numbers.
struct NumericKey(f64);

impl NumericKey {
    fn parse(text: &str) -> NumericKey {
        NumericKey(
            text.trim()
                .parse()
                .unwrap_or(f64::NEG_INFINITY),
        )
    }
}

impl PartialEq for NumericKey {
    fn eq(&self, other: &NumericKey) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}

impl Eq for NumericKey {}

impl PartialOrd for NumericKey {
    fn partial_cmp(&self, other: &NumericKey) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for NumericKey {
    fn cmp(&self, other: &NumericKey) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

pub struct FileProcessor {
    rows: RangeSpec,
    keep_lines_outside_rows: bool,
    delete_lines_in_rows: bool,
    //`None` means lines stream straight to the writer
    reorder: Option<Reorder>,
    //content filter applied to lines within the row range
    predicate: Option<Box<dyn LinePredicate>>,
    //key columns for `--unique`; `None` means duplicates are kept
    unique_key_span: Option<ColumnSpan>,
    transforms: Vec<Box<dyn LineTransform>>,
}

impl FileProcessor {
    pub fn new(config: &Config) -> FileProcessor {
        FileProcessor {
            rows: config.rows_or_full(),
            //delete mode keeps lines outside the row range,
            //selection mode (no delete) drops them
            keep_lines_outside_rows: config.delete,
            delete_lines_in_rows: config.delete && config.cols.is_none(),
            reorder: if config.sort {
                Some(Reorder::Sort(SortSpec {
                    key_span: config.col_span(),
                    numeric: config.numeric_sort,
                    reverse: config.reverse_sort,
                }))
            } else if config.tac {
                Some(Reorder::Tac)
            } else if config.shuffle {
                Some(Reorder::Shuffle)
            } else {
                None
            },
            predicate: predicate::build_predicate(config),
            unique_key_span: config.unique.then(|| config.col_span()),
            transforms: transform::build_pipeline(config),
        }
    }

    /// Stream `reader` line by line into `writer`, applying the configured
    /// row selection, per-line transforms and optional sorting.
    pub fn run<R: BufRead, W: Write>(&self, mut reader: R, writer: &mut W) -> io::Result<()> {
        let mut state = RunState::default();

        if self.rows.is_absolute() {
            //the total line count is irrelevant, so lines can stream through
            let rows = self.rows.resolve(usize::MAX);
            let mut line_number = 0usize;
            reader.for_byte_line_with_terminator(|raw_line| {
                line_number += 1;
                self.process_line(raw_line, line_number, &rows, &mut state, writer)
                    .map(|_| true)
            })?;
        } else {
            //end-relative bounds (`~N`) only resolve once the total
            //line count is known: the whole input must be buffered
            let mut lines: Vec<Vec<u8>> = Vec::new();
            reader.for_byte_line_with_terminator(|raw_line| {
                lines.push(raw_line.to_vec());
                Ok(true)
            })?;
            let rows = self.rows.resolve(lines.len());
            for (index, raw_line) in lines.iter().enumerate() {
                self.process_line(raw_line, index + 1, &rows, &mut state, writer)?;
            }
        }

        if !state.buffer_flushed {
            self.flush_reordered(&mut state, writer)?;
        }
        writer.flush()
    }

    fn process_line<W: Write>(
        &self,
        raw_line: &[u8],
        line_number: usize,
        rows: &RangeSet,
        state: &mut RunState,
        writer: &mut W,
    ) -> io::Result<()> {
        if !rows.contains(line_number) {
            if self.keep_lines_outside_rows {
                writer.write_all(raw_line)?;
            }
            return Ok(());
        }

        //without a content filter, deleting whole rows needs no UTF-8 look
        if self.delete_lines_in_rows && self.predicate.is_none() {
            return Ok(());
        }

        let utf8_line = from_utf8(raw_line).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("line {line_number} is not valid UTF-8: {e}"),
            )
        })?;

        let (content, terminator) = text::split_line_terminator(utf8_line);

        if let Some(predicate) = &self.predicate
            && !predicate.matches(content)
        {
            //a non-matching line is treated like one outside the row range
            if self.keep_lines_outside_rows {
                writer.write_all(raw_line)?;
            }
            return Ok(());
        }

        if self.delete_lines_in_rows {
            return Ok(());
        }

        let content = self.apply_transforms(content);

        if self.reorder.is_some() {
            state.reorder_buffer.push(Line {
                content,
                terminator: terminator.to_owned(),
            });
            if line_number >= rows.end() {
                self.flush_reordered(state, writer)?;
                state.buffer_flushed = true;
            }
        } else {
            if !self.passes_unique(&content, &mut state.seen_keys) {
                return Ok(());
            }
            writer.write_all(content.as_bytes())?;
            writer.write_all(terminator.as_bytes())?;
        }

        Ok(())
    }

    /// Whether the line survives `--unique`: its key columns have not
    /// been written yet. Without `--unique` every line passes.
    fn passes_unique(&self, content: &str, seen_keys: &mut HashSet<String>) -> bool {
        match &self.unique_key_span {
            None => true,
            Some(span) => seen_keys.insert(text::substring(content, &span.char_range(content))),
        }
    }

    fn apply_transforms(&self, content: &str) -> String {
        self.transforms
            .iter()
            .fold(content.to_owned(), |line, transform| transform.apply(&line))
    }

    fn flush_reordered<W: Write>(&self, state: &mut RunState, writer: &mut W) -> io::Result<()> {
        let Some(reorder) = &self.reorder else {
            return Ok(());
        };
        let RunState {
            reorder_buffer,
            seen_keys,
            ..
        } = state;

        match reorder {
            Reorder::Sort(spec) => Self::sort_lines(reorder_buffer, spec),
            Reorder::Tac => reorder_buffer.reverse(),
            Reorder::Shuffle => {
                use rand::seq::SliceRandom;
                reorder_buffer.shuffle(&mut rand::rng());
            }
        }

        //`--unique` keeps the first line per key in reordered order
        let lines: Vec<&Line> = reorder_buffer
            .iter()
            .filter(|line| self.passes_unique(&line.content, seen_keys))
            .collect();

        let last_index = lines.len().saturating_sub(1);
        for (index, line) in lines.iter().enumerate() {
            writer.write_all(line.content.as_bytes())?;
            if !line.terminator.is_empty() {
                writer.write_all(line.terminator.as_bytes())?;
            } else if index < last_index {
                //a line missing its terminator must not glue to the next one
                writer.write_all(NEW_LINE.as_bytes())?;
            }
        }
        state.reorder_buffer.clear();
        Ok(())
    }

    fn sort_lines(buffer: &mut [Line], spec: &SortSpec) {
        //`Reverse` keeps the sort stable in descending order too
        use std::cmp::Reverse;
        let text_key =
            |line: &Line| text::substring(&line.content, &spec.key_span.char_range(&line.content));
        match (spec.numeric, spec.reverse) {
            (false, false) => buffer.sort_by_cached_key(|line| text_key(line)),
            (false, true) => buffer.sort_by_cached_key(|line| Reverse(text_key(line))),
            (true, false) => buffer.sort_by_cached_key(|line| NumericKey::parse(&text_key(line))),
            (true, true) => {
                buffer.sort_by_cached_key(|line| Reverse(NumericKey::parse(&text_key(line))))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli_args::FindPattern;
    use std::io::Cursor;

    fn config() -> Config {
        Config {
            rows: None,
            cols: None,
            field_delimiter: None,
            sort: false,
            numeric_sort: false,
            reverse_sort: false,
            tac: false,
            shuffle: false,
            delete: false,
            ignore_case: false,
            upper: false,
            lower: false,
            trim: false,
            grep: None,
            invert: false,
            unique: false,
            filename: None,
            finds: Vec::new(),
            replace_strings: Vec::new(),
            output_filename: None,
        }
    }

    fn run(config: Config, input: &str) -> String {
        let processor = FileProcessor::new(&config);
        let mut output = Vec::new();
        processor
            .run(Cursor::new(input.as_bytes()), &mut output)
            .expect("processing failed");
        String::from_utf8(output).expect("output is not valid UTF-8")
    }

    #[test]
    fn passes_input_through_by_default() {
        let input = "line1\nline2\nline3\n";
        assert_eq!(run(config(), input), input);
    }

    #[test]
    fn streams_replace_without_buffering() {
        let mut config = config();
        config.finds = vec![FindPattern::Literal("foo".to_owned())];
        config.replace_strings = vec!["BAR".to_owned()];

        let result = run(config, "a foo\nb foo\n");
        assert_eq!(result, "a BAR\nb BAR\n");
    }

    #[test]
    fn applies_multiple_find_replace_pairs_per_line() {
        let mut config = config();
        config.finds = vec![
            FindPattern::Literal("cat".to_owned()),
            FindPattern::Literal("dog".to_owned()),
        ];
        config.replace_strings = vec!["dog".to_owned(), "wolf".to_owned()];

        //cat->dog runs first, then dog->wolf rewrites both
        let result = run(config, "cat and dog\n");
        assert_eq!(result, "wolf and wolf\n");
    }

    #[test]
    fn sorts_whole_input() {
        let mut config = config();
        config.sort = true;

        let result = run(config, "delta\nalpha\ncharlie\nbravo\n");
        assert_eq!(result, "alpha\nbravo\ncharlie\ndelta\n");
    }

    #[test]
    fn sorts_only_selected_rows() {
        let mut config = config();
        config.sort = true;
        config.rows = Some((2..=4).into());

        let result = run(config, "header\nc\na\nb\n");
        //row 1 is dropped in selection mode, rows 2-4 are sorted
        assert_eq!(result, "a\nb\nc\n");
    }

    #[test]
    fn numeric_sort_orders_by_value_not_lexicographically() {
        let mut config = config();
        config.sort = true;
        config.numeric_sort = true;

        //lexicographic order would be 10, 2, 9
        let result = run(config, "10\n9\n2\n");
        assert_eq!(result, "2\n9\n10\n");
    }

    #[test]
    fn numeric_sort_puts_non_numeric_lines_first() {
        let mut config = config();
        config.sort = true;
        config.numeric_sort = true;

        let result = run(config, "7\nabc\n-1.5\n");
        assert_eq!(result, "abc\n-1.5\n7\n");
    }

    #[test]
    fn reverse_sort_orders_descending() {
        let mut config = config();
        config.sort = true;
        config.reverse_sort = true;

        let result = run(config, "alpha\ncharlie\nbravo\n");
        assert_eq!(result, "charlie\nbravo\nalpha\n");
    }

    #[test]
    fn numeric_reverse_sort_with_column_key() {
        let mut config = config();
        config.sort = true;
        config.numeric_sort = true;
        config.reverse_sort = true;
        config.cols = Some(3..=4);

        let result = run(config, "a  2\nb 10\nc  9\n");
        assert_eq!(result, "b 10\nc  9\na  2\n");
    }

    #[test]
    fn sort_preserves_crlf_terminators() {
        let mut config = config();
        config.sort = true;

        let result = run(config, "b\r\na\r\n");
        assert_eq!(result, "a\r\nb\r\n");
    }

    #[test]
    fn sort_adds_terminator_when_unterminated_line_moves_up() {
        let mut config = config();
        config.sort = true;

        //"a" has no trailing newline and sorts before "b"
        let result = run(config, "b\na");
        assert_eq!(result, format!("a{}b\n", NEW_LINE));
    }

    #[test]
    fn replace_respects_column_boundaries_per_line() {
        let mut config = config();
        config.cols = Some(7..=9);
        config.finds = vec![FindPattern::Literal("foo".to_owned())];
        config.replace_strings = vec!["BAR".to_owned()];

        //"foo" starts at column 7 in the first line and column 9 in the second
        let result = run(config, "delta foo\ncharlie foo\n");
        assert_eq!(result, "delta BAR\ncharlie foo\n");
    }

    #[test]
    fn delete_keeps_lines_outside_row_range() {
        let mut config = config();
        config.delete = true;
        config.rows = Some((2..=3).into());

        let result = run(config, "one\ntwo\nthree\nfour\n");
        assert_eq!(result, "one\nfour\n");
    }

    #[test]
    fn delete_columns_applies_only_to_selected_rows() {
        let mut config = config();
        config.delete = true;
        config.rows = Some((1..=1).into());
        config.cols = Some(1..=4);

        let result = run(config, "one one\ntwo two\n");
        assert_eq!(result, "one\ntwo two\n");
    }

    #[test]
    fn end_relative_rows_select_from_the_end() {
        use crate::ranges::RangeBound::FromEnd;
        let mut config = config();
        config.rows = Some(RangeSpec::new(vec![(FromEnd(2), FromEnd(1))]));

        //~2-~1 means the last two lines
        let result = run(config, "one\ntwo\nthree\nfour\n");
        assert_eq!(result, "three\nfour\n");
    }

    #[test]
    fn end_relative_rows_combine_with_delete() {
        use crate::ranges::RangeBound::FromEnd;
        let mut config = config();
        config.delete = true;
        config.rows = Some(RangeSpec::new(vec![(FromEnd(1), FromEnd(1))]));

        let result = run(config, "one\ntwo\nthree\n");
        assert_eq!(result, "one\ntwo\n");
    }

    #[test]
    fn end_relative_rows_combine_with_sort() {
        use crate::ranges::RangeBound::FromEnd;
        let mut config = config();
        config.sort = true;
        config.rows = Some(RangeSpec::new(vec![(FromEnd(3), FromEnd(1))]));

        let result = run(config, "header\nc\na\nb\n");
        assert_eq!(result, "a\nb\nc\n");
    }

    #[test]
    fn grep_keeps_only_matching_lines() {
        let mut config = config();
        config.grep = Some(regex::Regex::new("ERROR").unwrap());

        let result = run(config, "a ERROR\nb INFO\nc ERROR\n");
        assert_eq!(result, "a ERROR\nc ERROR\n");
    }

    #[test]
    fn grep_with_delete_removes_matching_lines() {
        let mut config = config();
        config.delete = true;
        config.grep = Some(regex::Regex::new("ERROR").unwrap());

        let result = run(config, "a ERROR\nb INFO\nc ERROR\n");
        assert_eq!(result, "b INFO\n");
    }

    #[test]
    fn grep_filters_within_row_range_only() {
        let mut config = config();
        config.rows = Some((1..=2).into());
        config.grep = Some(regex::Regex::new("keep").unwrap());

        //row 3 matches but lies outside the selected rows
        let result = run(config, "keep a\ndrop b\nkeep c\n");
        assert_eq!(result, "keep a\n");
    }

    #[test]
    fn grep_combines_with_sort() {
        let mut config = config();
        config.sort = true;
        config.grep = Some(regex::Regex::new("x").unwrap());

        let result = run(config, "bx\nc\nax\n");
        assert_eq!(result, "ax\nbx\n");
    }

    #[test]
    fn tac_reverses_line_order() {
        let mut config = config();
        config.tac = true;

        let result = run(config, "one\ntwo\nthree\n");
        assert_eq!(result, "three\ntwo\none\n");
    }

    #[test]
    fn tac_reverses_only_selected_rows() {
        let mut config = config();
        config.tac = true;
        config.rows = Some((2..=3).into());

        let result = run(config, "header\nb\na\ntail\n");
        //selection mode keeps only rows 2-3, reversed
        assert_eq!(result, "a\nb\n");
    }

    #[test]
    fn tac_adds_terminator_when_unterminated_line_moves_up() {
        let mut config = config();
        config.tac = true;

        let result = run(config, "b\na");
        assert_eq!(result, format!("a{}b\n", NEW_LINE));
    }

    #[test]
    fn shuffle_preserves_the_set_of_lines() {
        let mut config = config();
        config.shuffle = true;

        let result = run(config, "one\ntwo\nthree\nfour\n");
        let mut lines: Vec<&str> = result.lines().collect();
        lines.sort_unstable();
        assert_eq!(lines, ["four", "one", "three", "two"]);
    }

    #[test]
    fn unique_drops_duplicate_lines_keeping_first() {
        let mut config = config();
        config.unique = true;

        let result = run(config, "b\na\nb\nc\na\n");
        assert_eq!(result, "b\na\nc\n");
    }

    #[test]
    fn unique_compares_only_key_columns() {
        let mut config = config();
        config.unique = true;
        config.cols = Some(1..=1);

        //"a1" and "a2" share the key "a", the first one wins
        let result = run(config, "a1\na2\nb1\n");
        assert_eq!(result, "a1\nb1\n");
    }

    #[test]
    fn unique_after_sort_keeps_first_in_sorted_order() {
        let mut config = config();
        config.sort = true;
        config.unique = true;

        let result = run(config, "b\na\nb\na\n");
        assert_eq!(result, "a\nb\n");
    }

    #[test]
    fn field_mode_selects_delimited_fields() {
        let mut config = config();
        config.cols = Some(2..=2);
        config.field_delimiter = Some(",".to_owned());

        let result = run(config, "a,bb,c\nx,yy,z\n");
        assert_eq!(result, "bb\nyy\n");
    }

    #[test]
    fn field_mode_delete_removes_field_and_delimiter() {
        let mut config = config();
        config.delete = true;
        config.cols = Some(2..=2);
        config.field_delimiter = Some(",".to_owned());

        let result = run(config, "a,b,c\nx,y\n");
        assert_eq!(result, "a,c\nx\n");
    }

    #[test]
    fn field_mode_sorts_by_field_key() {
        let mut config = config();
        config.sort = true;
        config.cols = Some(2..=2);
        config.field_delimiter = Some(",".to_owned());

        let result = run(config, "x,c\ny,a\nz,b\n");
        assert_eq!(result, "y,a\nz,b\nx,c\n");
    }

    #[test]
    fn field_mode_unique_keys_on_field() {
        let mut config = config();
        config.unique = true;
        config.cols = Some(1..=1);
        config.field_delimiter = Some(",".to_owned());

        let result = run(config, "a,1\na,2\nb,1\n");
        assert_eq!(result, "a,1\nb,1\n");
    }

    #[test]
    fn invalid_utf8_reports_line_number() {
        let processor = FileProcessor::new(&config());
        let mut output = Vec::new();
        let input: &[u8] = b"ok\n\xFF\xFE\n";

        let error = processor
            .run(Cursor::new(input), &mut output)
            .expect_err("invalid UTF-8 must fail");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("line 2"));
    }
}
