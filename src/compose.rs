//! Composition layer: derives the processing engine from a validated
//! [`Config`]. All knowledge of how CLI options map onto engine parts
//! (row mode, reordering, row predicate, transform pipeline) lives
//! here, so the engine modules never depend on the CLI layer and the
//! CLI layer never constructs engine internals itself.

use crate::cli_args::{Config, FindPattern, ReorderMode, Replacement};
use crate::columns::{ColumnList, ColumnSpan};
use crate::file_processor::{FileProcessor, Reorder, RowMode, SortSpec};
use crate::predicate::{GrepPredicate, LinePredicate};
use crate::reduce::{Aggregate, LineReducer, Summarize};
use crate::transform::{
    DeleteColumns, DropEmpty, LineTransform, MapColumns, Pipeline, RegexReplaceInColumns,
    ReplaceInColumns, ReplaceInColumnsIgnoreCase, SelectColumns, SplitLines, WrapLines,
};

/// Assemble the streaming processor implied by the configuration.
pub fn build_processor(config: &Config) -> FileProcessor {
    FileProcessor {
        rows: config.rows_or_full(),
        row_mode: row_mode(config),
        reorder: build_reorder(config),
        predicate: build_predicate(config),
        unique_key_span: config
            .unique
            .then(|| config.unique_key_span()),
        transforms: build_pipeline(config),
        reducer: build_reducer(config),
    }
}

/// Build the summary implied by the configuration, if any: the
/// aggregates asked for, keyed by `--group-by` when it is given.
fn build_reducer(config: &Config) -> Option<Box<dyn LineReducer>> {
    let aggregates = build_aggregates(config);
    if aggregates.is_empty() {
        return None;
    }

    let key_span = config
        .group_by
        .clone()
        .map(|columns| config.span_for(columns));

    Some(Box::new(Summarize::new(
        key_span,
        aggregates,
        config.summary_separator(),
    )))
}

/// One statistic flag: the columns it reads, and what it computes.
type Statistic<'a> = (&'a Option<ColumnList>, fn(ColumnSpan) -> Aggregate);

/// The summary columns, in the order they are printed: the row count
/// first, then each statistic in the order the flags are declared.
fn build_aggregates(config: &Config) -> Vec<Aggregate> {
    let mut aggregates = Vec::new();

    if config.count {
        aggregates.push(Aggregate::Count);
    }
    let statistics: [Statistic; 4] = [
        (&config.sum, Aggregate::Sum),
        (&config.avg, Aggregate::Avg),
        (&config.min, Aggregate::Min),
        (&config.max, Aggregate::Max),
    ];
    for (columns, aggregate) in statistics {
        if let Some(columns) = columns {
            aggregates.push(aggregate(config.span_for(columns.clone())));
        }
    }

    aggregates
}

/// How rows inside vs outside the row range are treated.
fn row_mode(config: &Config) -> RowMode {
    if !config.delete {
        RowMode::Select
    } else if config.cols.is_none() {
        RowMode::DeleteSelected
    } else {
        //with a column range, `--delete` removes columns (a transform
        //in the pipeline); the selected rows themselves survive
        RowMode::EditSelected
    }
}

/// Attach the column key to the configured reordering, if any.
fn build_reorder(config: &Config) -> Option<Reorder> {
    config.reorder.map(|mode| match mode {
        ReorderMode::Sort { numeric, reverse } => Reorder::Sort(SortSpec {
            key_span: config.sort_key_span(),
            numeric,
            reverse,
        }),
        ReorderMode::Tac => Reorder::Tac,
        ReorderMode::Shuffle => Reorder::Shuffle,
    })
}

/// Build the row filter implied by the configuration, if any.
fn build_predicate(config: &Config) -> Option<Box<dyn LinePredicate>> {
    config.grep.as_ref().map(|pattern| {
        Box::new(GrepPredicate::new(
            pattern.clone(),
            config.col_span(),
            config.invert,
        )) as Box<dyn LinePredicate>
    })
}

/// Build the per-line transform pipeline implied by the configuration.
fn build_pipeline(config: &Config) -> Pipeline {
    let mut pipeline: Vec<Box<dyn LineTransform>> = Vec::new();

    if config.delete && config.cols.is_some() {
        pipeline.push(Box::new(DeleteColumns::new(config.col_span())));
    }

    //with no operation claiming the column range, `--cols` alone
    //selects the range, mirroring how `--rows` alone selects lines
    if !config.has_column_operation() && config.cols.is_some() {
        pipeline.push(Box::new(SelectColumns::new(config.col_span())));
    }

    //the find/replace pairs run in order, so a later one can rewrite
    //an earlier result
    for Replacement { find, replace } in &config.replacements {
        match find {
            FindPattern::Literal(text) if config.ignore_case => pipeline.push(Box::new(
                ReplaceInColumnsIgnoreCase::new(text, replace.clone(), config.col_span()),
            )),
            FindPattern::Literal(text) => pipeline.push(Box::new(ReplaceInColumns::new(
                text.clone(),
                replace.clone(),
                config.col_span(),
            ))),
            FindPattern::Regex(pattern) => pipeline.push(Box::new(RegexReplaceInColumns::new(
                pattern.clone(),
                replace.clone(),
                config.col_span(),
            ))),
        }
    }

    if config.upper {
        pipeline.push(Box::new(MapColumns::uppercase(config.col_span())));
    }
    if config.lower {
        pipeline.push(Box::new(MapColumns::lowercase(config.col_span())));
    }
    if config.trim {
        pipeline.push(Box::new(MapColumns::trim(config.col_span())));
    }

    //splitting comes after the rewriting transforms (they are scoped to
    //columns of the original line) and before wrapping, which then cuts
    //each piece to width
    if let Some(separator) = &config.split_on {
        pipeline.push(Box::new(SplitLines::new(separator.clone())));
    }
    //wrapping comes last of the rewriting transforms, so the chunks are
    //cut from the finished line rather than from the raw one
    if let Some(width) = config.wrap {
        pipeline.push(Box::new(WrapLines::new(width)));
    }
    //and dropping empties comes last of all, so it sees what the line
    //became — `--trim --drop-empty` drops whitespace-only lines
    if config.drop_empty {
        pipeline.push(Box::new(DropEmpty));
    }

    Pipeline::new(pipeline)
}

#[cfg(test)]
//tests tweak single flags on a default config; mutating it reads
//better here than struct-update syntax
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::columns::ColumnList;
    use crate::constants::NEW_LINE;
    use crate::ranges::RangeSpec;
    use crate::transform::Lines;
    use std::io::{self, Cursor};

    fn run(config: Config, input: &str) -> String {
        let processor = build_processor(&config);
        let mut output = Vec::new();
        processor
            .run(Cursor::new(input.as_bytes()), &mut output)
            .expect("processing failed");
        String::from_utf8(output).expect("output is not valid UTF-8")
    }

    fn literal(find: &str, replace: &str) -> Replacement {
        Replacement {
            find: FindPattern::Literal(find.to_owned()),
            replace: replace.to_owned(),
        }
    }

    fn sorted(numeric: bool, reverse: bool) -> Option<ReorderMode> {
        Some(ReorderMode::Sort { numeric, reverse })
    }

    /// The single line a pipeline turns `line` into.
    fn piped(pipeline: &Pipeline, line: &str) -> String {
        match pipeline.apply(line) {
            Lines::One(content) => content.into_owned(),
            Lines::Several(contents) => panic!("expected a single line, got {contents:?}"),
        }
    }

    #[test]
    fn passes_input_through_by_default() {
        let input = "line1\nline2\nline3\n";
        assert_eq!(run(Config::default(), input), input);
    }

    #[test]
    fn row_mode_maps_delete_and_columns() {
        //selection is the default
        assert_eq!(row_mode(&Config::default()), RowMode::Select);

        //--delete without columns removes whole rows
        let mut delete_rows = Config::default();
        delete_rows.delete = true;
        assert_eq!(row_mode(&delete_rows), RowMode::DeleteSelected);

        //--delete with columns edits the selected rows in place
        let mut delete_cols = Config::default();
        delete_cols.delete = true;
        delete_cols.cols = Some((1..=2).into());
        assert_eq!(row_mode(&delete_cols), RowMode::EditSelected);
    }

    #[test]
    fn streams_replace_without_buffering() {
        let mut config = Config::default();
        config.replacements = vec![literal("foo", "BAR")];

        let result = run(config, "a foo\nb foo\n");
        assert_eq!(result, "a BAR\nb BAR\n");
    }

    #[test]
    fn applies_multiple_find_replace_pairs_per_line() {
        let mut config = Config::default();
        config.replacements = vec![literal("cat", "dog"), literal("dog", "wolf")];

        //cat->dog runs first, then dog->wolf rewrites both
        let result = run(config, "cat and dog\n");
        assert_eq!(result, "wolf and wolf\n");
    }

    #[test]
    fn sorts_whole_input() {
        let mut config = Config::default();
        config.reorder = sorted(false, false);

        let result = run(config, "delta\nalpha\ncharlie\nbravo\n");
        assert_eq!(result, "alpha\nbravo\ncharlie\ndelta\n");
    }

    #[test]
    fn sorts_only_selected_rows() {
        let mut config = Config::default();
        config.reorder = sorted(false, false);
        config.rows = Some((2..=4).into());

        let result = run(config, "header\nc\na\nb\n");
        //row 1 is dropped in selection mode, rows 2-4 are sorted
        assert_eq!(result, "a\nb\nc\n");
    }

    #[test]
    fn numeric_sort_orders_by_value_not_lexicographically() {
        let mut config = Config::default();
        config.reorder = sorted(true, false);

        //lexicographic order would be 10, 2, 9
        let result = run(config, "10\n9\n2\n");
        assert_eq!(result, "2\n9\n10\n");
    }

    #[test]
    fn numeric_sort_puts_non_numeric_lines_first() {
        let mut config = Config::default();
        config.reorder = sorted(true, false);

        let result = run(config, "7\nabc\n-1.5\n");
        assert_eq!(result, "abc\n-1.5\n7\n");
    }

    #[test]
    fn numeric_sort_treats_negative_infinity_as_a_number() {
        let mut config = Config::default();
        config.reorder = sorted(true, false);

        //-inf parses as a number, so it sorts after non-numeric lines;
        //NaN does not order meaningfully and counts as non-numeric
        let result = run(config, "7\n-inf\nNaN\nabc\n");
        assert_eq!(result, "NaN\nabc\n-inf\n7\n");
    }

    #[test]
    fn sort_key_beyond_short_lines_is_empty() {
        let mut config = Config::default();
        config.reorder = sorted(false, false);
        config.cols = Some((5..=6).into());

        //no line reaches column 5: all keys are empty, so the stable
        //sort keeps the input order instead of comparing whole lines
        let result = run(config, "zz\nabc\nxy\n");
        assert_eq!(result, "zz\nabc\nxy\n");
    }

    #[test]
    fn reverse_sort_orders_descending() {
        let mut config = Config::default();
        config.reorder = sorted(false, true);

        let result = run(config, "alpha\ncharlie\nbravo\n");
        assert_eq!(result, "charlie\nbravo\nalpha\n");
    }

    #[test]
    fn numeric_reverse_sort_with_column_key() {
        let mut config = Config::default();
        config.reorder = sorted(true, true);
        config.cols = Some((3..=4).into());

        let result = run(config, "a  2\nb 10\nc  9\n");
        assert_eq!(result, "b 10\nc  9\na  2\n");
    }

    #[test]
    fn sort_preserves_crlf_terminators() {
        let mut config = Config::default();
        config.reorder = sorted(false, false);

        let result = run(config, "b\r\na\r\n");
        assert_eq!(result, "a\r\nb\r\n");
    }

    #[test]
    fn sort_adds_terminator_when_unterminated_line_moves_up() {
        let mut config = Config::default();
        config.reorder = sorted(false, false);

        //"a" has no trailing newline and sorts before "b"
        let result = run(config, "b\na");
        assert_eq!(result, format!("a{}b\n", NEW_LINE));
    }

    #[test]
    fn replace_respects_column_boundaries_per_line() {
        let mut config = Config::default();
        config.cols = Some((7..=9).into());
        config.replacements = vec![literal("foo", "BAR")];

        //"foo" starts at column 7 in the first line and column 9 in the second
        let result = run(config, "delta foo\ncharlie foo\n");
        assert_eq!(result, "delta BAR\ncharlie foo\n");
    }

    #[test]
    fn delete_keeps_lines_outside_row_range() {
        let mut config = Config::default();
        config.delete = true;
        config.rows = Some((2..=3).into());

        let result = run(config, "one\ntwo\nthree\nfour\n");
        assert_eq!(result, "one\nfour\n");
    }

    #[test]
    fn delete_columns_applies_only_to_selected_rows() {
        let mut config = Config::default();
        config.delete = true;
        config.rows = Some((1..=1).into());
        config.cols = Some((1..=4).into());

        let result = run(config, "one one\ntwo two\n");
        assert_eq!(result, "one\ntwo two\n");
    }

    #[test]
    fn end_relative_rows_select_from_the_end() {
        use crate::ranges::RangeBound::FromEnd;
        let mut config = Config::default();
        config.rows = Some(RangeSpec::new(vec![(FromEnd(2), FromEnd(1))]));

        //~2-~1 means the last two lines
        let result = run(config, "one\ntwo\nthree\nfour\n");
        assert_eq!(result, "three\nfour\n");
    }

    #[test]
    fn end_relative_rows_combine_with_delete() {
        use crate::ranges::RangeBound::FromEnd;
        let mut config = Config::default();
        config.delete = true;
        config.rows = Some(RangeSpec::new(vec![(FromEnd(1), FromEnd(1))]));

        let result = run(config, "one\ntwo\nthree\n");
        assert_eq!(result, "one\ntwo\n");
    }

    #[test]
    fn end_relative_rows_combine_with_sort() {
        use crate::ranges::RangeBound::FromEnd;
        let mut config = Config::default();
        config.reorder = sorted(false, false);
        config.rows = Some(RangeSpec::new(vec![(FromEnd(3), FromEnd(1))]));

        let result = run(config, "header\nc\na\nb\n");
        assert_eq!(result, "a\nb\nc\n");
    }

    #[test]
    fn grep_keeps_only_matching_lines() {
        let mut config = Config::default();
        config.grep = Some(regex::Regex::new("ERROR").unwrap());

        let result = run(config, "a ERROR\nb INFO\nc ERROR\n");
        assert_eq!(result, "a ERROR\nc ERROR\n");
    }

    #[test]
    fn grep_with_delete_removes_matching_lines() {
        let mut config = Config::default();
        config.delete = true;
        config.grep = Some(regex::Regex::new("ERROR").unwrap());

        let result = run(config, "a ERROR\nb INFO\nc ERROR\n");
        assert_eq!(result, "b INFO\n");
    }

    #[test]
    fn grep_filters_within_row_range_only() {
        let mut config = Config::default();
        config.rows = Some((1..=2).into());
        config.grep = Some(regex::Regex::new("keep").unwrap());

        //row 3 matches but lies outside the selected rows
        let result = run(config, "keep a\ndrop b\nkeep c\n");
        assert_eq!(result, "keep a\n");
    }

    #[test]
    fn grep_combines_with_sort() {
        let mut config = Config::default();
        config.reorder = sorted(false, false);
        config.grep = Some(regex::Regex::new("x").unwrap());

        let result = run(config, "bx\nc\nax\n");
        assert_eq!(result, "ax\nbx\n");
    }

    #[test]
    fn tac_reverses_line_order() {
        let mut config = Config::default();
        config.reorder = Some(ReorderMode::Tac);

        let result = run(config, "one\ntwo\nthree\n");
        assert_eq!(result, "three\ntwo\none\n");
    }

    #[test]
    fn tac_reverses_only_selected_rows() {
        let mut config = Config::default();
        config.reorder = Some(ReorderMode::Tac);
        config.rows = Some((2..=3).into());

        let result = run(config, "header\nb\na\ntail\n");
        //selection mode keeps only rows 2-3, reversed
        assert_eq!(result, "a\nb\n");
    }

    #[test]
    fn tac_adds_terminator_when_unterminated_line_moves_up() {
        let mut config = Config::default();
        config.reorder = Some(ReorderMode::Tac);

        let result = run(config, "b\na");
        assert_eq!(result, format!("a{}b\n", NEW_LINE));
    }

    #[test]
    fn reorder_keeps_noncontiguous_segments_in_place() {
        use crate::ranges::RangeBound::FromStart;
        //deleting a column keeps the lines outside the row range, so
        //each selected segment must reorder in place instead of
        //drifting past the kept lines in between
        let mut config = Config::default();
        config.delete = true;
        config.cols = Some((1..=1).into());
        config.reorder = Some(ReorderMode::Tac);
        config.rows = Some(RangeSpec::new(vec![
            (FromStart(2), FromStart(3)),
            (FromStart(6), FromStart(7)),
        ]));

        let result = run(config, "Xa\nXb\nXc\nXd\nXe\nXf\nXg\n");
        assert_eq!(result, "Xa\nc\nb\nXd\nXe\ng\nf\n");
    }

    #[test]
    fn shuffle_preserves_the_set_of_lines() {
        let mut config = Config::default();
        config.reorder = Some(ReorderMode::Shuffle);

        let result = run(config, "one\ntwo\nthree\nfour\n");
        let mut lines: Vec<&str> = result.lines().collect();
        lines.sort_unstable();
        assert_eq!(lines, ["four", "one", "three", "two"]);
    }

    #[test]
    fn unique_drops_duplicate_lines_keeping_first() {
        let mut config = Config::default();
        config.unique = true;

        let result = run(config, "b\na\nb\nc\na\n");
        assert_eq!(result, "b\na\nc\n");
    }

    #[test]
    fn unique_compares_only_key_columns() {
        let mut config = Config::default();
        config.unique = true;
        config.cols = Some((1..=1).into());

        //"a1" and "a2" share the key "a", the first one wins
        let result = run(config, "a1\na2\nb1\n");
        assert_eq!(result, "a1\nb1\n");
    }

    #[test]
    fn unique_dedupes_empty_fields() {
        let mut config = Config::default();
        config.unique = true;
        config.cols = Some((2..=2).into());
        config.field_delimiter = Some(",".to_owned());

        //"b," and "c," share the empty field 2 as their key
        let result = run(config, "a,1\nb,\nc,\nd,1\n");
        assert_eq!(result, "a,1\nb,\n");
    }

    #[test]
    fn unique_after_sort_keeps_first_in_sorted_order() {
        let mut config = Config::default();
        config.reorder = sorted(false, false);
        config.unique = true;

        let result = run(config, "b\na\nb\na\n");
        assert_eq!(result, "a\nb\n");
    }

    #[test]
    fn sort_key_frees_cols_for_another_operation() {
        //the motivating case: sort by field 1, replace inside field 2
        let mut config = Config::default();
        config.reorder = sorted(false, false);
        config.field_delimiter = Some(",".to_owned());
        config.cols = Some((2..=2).into());
        config.sort_key = Some((1..=1).into());
        config.replacements = vec![literal("x", "X")];

        let result = run(config, "b,x\na,x\n");
        //"x" is replaced only in field 2, and the rows sort by field 1
        assert_eq!(result, "a,X\nb,X\n");
    }

    #[test]
    fn sort_key_applies_to_the_transformed_line() {
        //a bare --cols still cuts, because --sort-key claims no columns;
        //the sort key then addresses the cut result
        let mut config = Config::default();
        config.reorder = sorted(false, false);
        config.cols = Some((3..=5).into());
        config.sort_key = Some((1..=1).into());

        let result = run(config, "xxb..\nxxa..\n");
        assert_eq!(result, "a..\nb..\n");
    }

    #[test]
    fn unique_key_is_independent_of_cols() {
        let mut config = Config::default();
        config.unique = true;
        config.field_delimiter = Some(",".to_owned());
        config.cols = Some((2..=2).into());
        config.unique_key = Some((1..=1).into());
        config.upper = true;

        //rows dedupe on field 1, while --upper still works on field 2
        let result = run(config, "a,x\na,y\nb,z\n");
        assert_eq!(result, "a,X\nb,Z\n");
    }

    #[test]
    fn unique_falls_back_to_cols_without_its_own_key() {
        let mut config = Config::default();
        config.unique = true;
        config.cols = Some((1..=1).into());

        let result = run(config, "a1\na2\nb1\n");
        assert_eq!(result, "a1\nb1\n");
    }

    #[test]
    fn wrap_expands_long_lines_into_several() {
        let mut config = Config::default();
        config.wrap = Some(3);

        let result = run(config, "abcdefg\nhi\n");
        assert_eq!(result, "abc\ndef\ng\nhi\n");
    }

    #[test]
    fn wrap_keeps_the_terminator_off_the_last_line() {
        let mut config = Config::default();
        config.wrap = Some(3);

        //the input's last line carries no terminator, so neither does
        //the last chunk it expands into — but the chunks before it must
        //still be separated
        let result = run(config, "abcdef");
        assert_eq!(result, format!("abc{NEW_LINE}def"));
    }

    #[test]
    fn wrap_runs_after_the_other_transforms() {
        let mut config = Config::default();
        config.wrap = Some(2);
        config.upper = true;

        //the chunks are cut from the finished line, and each is uppercased
        let result = run(config, "abcde\n");
        assert_eq!(result, "AB\nCD\nE\n");
    }

    #[test]
    fn wrap_combines_with_sort() {
        let mut config = Config::default();
        config.wrap = Some(2);
        config.reorder = sorted(false, false);

        //every chunk is a line of its own by the time the rows reorder
        let result = run(config, "cd\nab\n");
        assert_eq!(result, "ab\ncd\n");
    }

    #[test]
    fn drop_empty_removes_lines_left_empty_by_a_transform() {
        let mut config = Config::default();
        config.cols = Some((3..=3).into());
        config.drop_empty = true;

        //only the rows that reach column 3 survive the cut
        let result = run(config, "abc\nx\ndef\n");
        assert_eq!(result, "c\nf\n");
    }

    #[test]
    fn drop_empty_after_trim_removes_whitespace_only_lines() {
        let mut config = Config::default();
        config.trim = true;
        config.drop_empty = true;

        let result = run(config, "a\n   \nb\n\n");
        assert_eq!(result, "a\nb\n");
    }

    #[test]
    fn split_on_expands_a_line_into_one_row_per_piece() {
        let mut config = Config::default();
        config.split_on = Some(",".to_owned());

        let result = run(config, "a,b,c\nd\n");
        assert_eq!(result, "a\nb\nc\nd\n");
    }

    #[test]
    fn split_on_combines_with_a_row_range() {
        let mut config = Config::default();
        config.split_on = Some(",".to_owned());
        config.rows = Some((1..=1).into());

        //only the selected row is split; row 2 is not even output
        let result = run(config, "a,b\nc,d\n");
        assert_eq!(result, "a\nb\n");
    }

    #[test]
    fn count_replaces_the_rows_with_their_number() {
        let mut config = Config::default();
        config.count = true;

        let result = run(config, "a\nb\nc\n");
        assert_eq!(result, "3\n");
    }

    #[test]
    fn count_counts_only_the_rows_that_survive_the_filters() {
        let mut config = Config::default();
        config.count = true;
        config.grep = Some(regex::Regex::new("ERROR").unwrap());

        //a reducer sees the rows the pipeline let through, no others
        let result = run(config, "a ERROR\nb INFO\nc ERROR\n");
        assert_eq!(result, "2\n");
    }

    #[test]
    fn group_by_summarizes_each_key() {
        let mut config = Config::default();
        config.field_delimiter = Some(",".to_owned());
        config.group_by = Some((1..=1).into());
        config.count = true;
        config.sum = Some((2..=2).into());

        let result = run(config, "a,1\nb,10\na,3\n");
        //one row per key, in first-seen order: key, count, sum
        assert_eq!(result, "a,2,4\nb,1,10\n");
    }

    #[test]
    fn summary_columns_are_separated_by_a_tab_in_char_mode() {
        let mut config = Config::default();
        config.count = true;
        config.sum = Some((1..=2).into());

        //no delimiter to inherit, so the summary falls back to a tab
        let result = run(config, "10\n20\n");
        assert_eq!(result, "2\t30\n");
    }

    #[test]
    fn unique_and_count_together_count_the_distinct_rows() {
        let mut config = Config::default();
        config.unique = true;
        config.count = true;

        //--unique drops the duplicates before the reducer sees them
        let result = run(config, "a\nb\na\nc\n");
        assert_eq!(result, "3\n");
    }

    #[test]
    fn quoted_fields_survive_a_delimiter_inside_them() {
        let mut config = Config::default();
        config.field_delimiter = Some(",".to_owned());
        config.quoted = true;
        config.cols = Some((2..=2).into());

        //field 2 is the quoted one, comma and all
        let result = run(config, "a,\"b,c\",d\n");
        assert_eq!(result, "\"b,c\"\n");
    }

    #[test]
    fn quoted_fields_permute_and_rejoin_as_csv() {
        let mut config = Config::default();
        config.field_delimiter = Some(",".to_owned());
        config.quoted = true;
        config.cols = Some(ColumnList::new(vec![3..=3, 1..=1]));

        //a field keeps its quotes, so the projection stays valid CSV
        let result = run(config, "a,\"b,c\",d\n");
        assert_eq!(result, "d,a\n");

        let mut config = Config::default();
        config.field_delimiter = Some(",".to_owned());
        config.quoted = true;
        config.cols = Some(ColumnList::new(vec![2..=2, 1..=1]));
        let result = run(config, "a,\"b,c\",d\n");
        assert_eq!(result, "\"b,c\",a\n");
    }

    #[test]
    fn column_list_selects_the_parts_in_the_written_order() {
        let mut config = Config::default();
        config.cols = Some(ColumnList::new(vec![3..=3, 1..=1, 2..=2]));
        config.field_delimiter = Some(",".to_owned());

        //an awk-style projection: fields reordered, rejoined by the delimiter
        let result = run(config, "a,b,c\nx,y,z\n");
        assert_eq!(result, "c,a,b\nz,x,y\n");
    }

    #[test]
    fn column_list_joins_on_the_output_delimiter() {
        let mut config = Config::default();
        config.cols = Some(ColumnList::new(vec![2..=2, 1..=1]));
        config.field_delimiter = Some(",".to_owned());
        config.output_delimiter = Some(";".to_owned());

        let result = run(config, "a,b\n");
        assert_eq!(result, "b;a\n");
    }

    #[test]
    fn column_list_deletes_every_part() {
        let mut config = Config::default();
        config.delete = true;
        config.cols = Some(ColumnList::new(vec![1..=1, 3..=3]));
        config.field_delimiter = Some(",".to_owned());

        let result = run(config, "a,b,c\n");
        assert_eq!(result, "b\n");
    }

    #[test]
    fn column_list_scopes_a_write_to_every_part() {
        let mut config = Config::default();
        config.cols = Some(ColumnList::new(vec![1..=1, 3..=3]));
        config.field_delimiter = Some(",".to_owned());
        config.upper = true;

        //writing works on the normalized set, so the order is irrelevant
        let result = run(config, "a,b,c\n");
        assert_eq!(result, "A,b,C\n");
    }

    #[test]
    fn field_mode_selects_delimited_fields() {
        let mut config = Config::default();
        config.cols = Some((2..=2).into());
        config.field_delimiter = Some(",".to_owned());

        let result = run(config, "a,bb,c\nx,yy,z\n");
        assert_eq!(result, "bb\nyy\n");
    }

    #[test]
    fn field_mode_delete_removes_field_and_delimiter() {
        let mut config = Config::default();
        config.delete = true;
        config.cols = Some((2..=2).into());
        config.field_delimiter = Some(",".to_owned());

        let result = run(config, "a,b,c\nx,y\n");
        assert_eq!(result, "a,c\nx\n");
    }

    #[test]
    fn field_mode_sorts_by_field_key() {
        let mut config = Config::default();
        config.reorder = sorted(false, false);
        config.cols = Some((2..=2).into());
        config.field_delimiter = Some(",".to_owned());

        let result = run(config, "x,c\ny,a\nz,b\n");
        assert_eq!(result, "y,a\nz,b\nx,c\n");
    }

    #[test]
    fn field_mode_unique_keys_on_field() {
        let mut config = Config::default();
        config.unique = true;
        config.cols = Some((1..=1).into());
        config.field_delimiter = Some(",".to_owned());

        let result = run(config, "a,1\na,2\nb,1\n");
        assert_eq!(result, "a,1\nb,1\n");
    }

    #[test]
    fn invalid_utf8_reports_line_number() {
        let processor = build_processor(&Config::default());
        let mut output = Vec::new();
        let input: &[u8] = b"ok\n\xFF\xFE\n";

        let error = processor
            .run(Cursor::new(input), &mut output)
            .expect_err("invalid UTF-8 must fail");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("line 2"));
    }

    #[test]
    fn build_pipeline_is_empty_by_default() {
        assert!(build_pipeline(&Config::default()).is_empty());
    }

    #[test]
    fn build_pipeline_selects_columns_when_no_other_operation() {
        let mut config = Config::default();
        config.cols = Some((5..=10).into());
        assert_eq!(build_pipeline(&config).len(), 1);
    }

    #[test]
    fn build_pipeline_does_not_select_columns_when_they_key_another_operation() {
        //sort uses the column range as its key
        let mut sort_config = Config::default();
        sort_config.cols = Some((5..=10).into());
        sort_config.reorder = sorted(false, false);
        assert!(build_pipeline(&sort_config).is_empty());

        //find/replace is scoped by the column range
        let mut replace_config = Config::default();
        replace_config.cols = Some((5..=10).into());
        replace_config.replacements = vec![literal("a", "b")];
        let pipeline = build_pipeline(&replace_config);
        assert_eq!(pipeline.len(), 1);
        assert_eq!(piped(&pipeline, "aaaa aaaa"), "aaaa bbbb");
    }

    #[test]
    fn build_pipeline_adds_delete_columns() {
        let mut config = Config::default();
        config.delete = true;
        config.cols = Some((5..=10).into());
        assert_eq!(build_pipeline(&config).len(), 1);
    }

    #[test]
    fn build_pipeline_ignores_delete_without_columns() {
        let mut config = Config::default();
        config.delete = true;
        assert!(build_pipeline(&config).is_empty());
    }

    #[test]
    fn build_pipeline_orders_replace_before_case_transforms() {
        let mut config = Config::default();
        config.upper = true;
        config.replacements = vec![literal("foo", "bar")];

        let pipeline = build_pipeline(&config);
        assert_eq!(pipeline.len(), 2);
        //replace runs first, so the replacement is uppercased too
        assert_eq!(piped(&pipeline, "x foo y"), "X BAR Y");
    }

    #[test]
    fn build_pipeline_adds_one_transform_per_replacement_pair() {
        let mut config = Config::default();
        config.replacements = vec![literal("a", "b"), literal("b", "c")];

        let pipeline = build_pipeline(&config);
        assert_eq!(pipeline.len(), 2);
        //pairs run in order: a->b then b->c turns "a" into "c"
        assert_eq!(piped(&pipeline, "a"), "c");
    }
}
