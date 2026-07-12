//! Per-line operations composed into a processing pipeline.
//!
//! Each operation implements [`LineTransform`] and works on line content
//! without its terminator. The pipeline itself is derived from the
//! configuration by [`crate::compose`], so adding a new operation means
//! adding a new transform here instead of branching inside the
//! processing loop.

use regex::{NoExpand, Regex, RegexBuilder};

use crate::columns::ColumnSpan;
use crate::text;

/// A single per-line operation in the processing pipeline.
pub trait LineTransform {
    /// Transform line content (without its terminator).
    fn apply(&self, line: &str) -> String;
}

/// Keeps only the characters within a column span (like `cut`). The
/// parts are read in the order written, so a permuted list reorders
/// them; in field mode they are joined by the output delimiter.
pub struct SelectColumns {
    span: ColumnSpan,
}

impl SelectColumns {
    pub fn new(span: impl Into<ColumnSpan>) -> SelectColumns {
        SelectColumns { span: span.into() }
    }
}

impl LineTransform for SelectColumns {
    fn apply(&self, line: &str) -> String {
        text::select_ranges(line, &self.span.read_ranges(line), self.span.joiner()).into_owned()
    }
}

/// Removes the characters within a column span. In field mode one
/// adjacent delimiter is removed too, like `cut`.
pub struct DeleteColumns {
    span: ColumnSpan,
}

impl DeleteColumns {
    pub fn new(span: impl Into<ColumnSpan>) -> DeleteColumns {
        DeleteColumns { span: span.into() }
    }
}

impl LineTransform for DeleteColumns {
    fn apply(&self, line: &str) -> String {
        text::remove_ranges(line, &self.span.delete_ranges(line))
    }
}

/// Applies a text mapping to the characters within a column span,
/// leaving the rest of the line untouched. One constructor per
/// mapping keeps `build_pipeline` readable.
pub struct MapColumns {
    span: ColumnSpan,
    map: fn(&str) -> String,
}

impl MapColumns {
    /// Uppercases the characters within the span.
    pub fn uppercase(span: impl Into<ColumnSpan>) -> MapColumns {
        MapColumns {
            span: span.into(),
            map: str::to_uppercase,
        }
    }

    /// Lowercases the characters within the span.
    pub fn lowercase(span: impl Into<ColumnSpan>) -> MapColumns {
        MapColumns {
            span: span.into(),
            map: str::to_lowercase,
        }
    }

    /// Trims whitespace at both ends of the span (with the full span,
    /// this trims the whole line).
    pub fn trim(span: impl Into<ColumnSpan>) -> MapColumns {
        MapColumns {
            span: span.into(),
            map: |within| within.trim().to_owned(),
        }
    }
}

impl LineTransform for MapColumns {
    fn apply(&self, line: &str) -> String {
        text::map_ranges(line, &self.span.write_ranges(line), self.map)
    }
}

/// Replaces `find` with `replace` within a column span.
pub struct ReplaceInColumns {
    find: String,
    replace: String,
    span: ColumnSpan,
}

impl ReplaceInColumns {
    pub fn new(find: String, replace: String, span: impl Into<ColumnSpan>) -> ReplaceInColumns {
        ReplaceInColumns {
            find,
            replace,
            span: span.into(),
        }
    }
}

impl LineTransform for ReplaceInColumns {
    fn apply(&self, line: &str) -> String {
        text::replace_in_ranges(
            line,
            &self.find,
            &self.replace,
            &self.span.write_ranges(line),
        )
    }
}

/// Replaces case-insensitive occurrences of a literal within a column
/// span. The literal is matched through an escaped regex so Unicode
/// case folding applies; the replacement is inserted verbatim.
pub struct ReplaceInColumnsIgnoreCase {
    pattern: Regex,
    replace: String,
    span: ColumnSpan,
}

impl ReplaceInColumnsIgnoreCase {
    pub fn new(
        find: &str,
        replace: String,
        span: impl Into<ColumnSpan>,
    ) -> ReplaceInColumnsIgnoreCase {
        let pattern = RegexBuilder::new(&regex::escape(find))
            .case_insensitive(true)
            .build()
            .expect("an escaped literal is always a valid regex");
        ReplaceInColumnsIgnoreCase {
            pattern,
            replace,
            span: span.into(),
        }
    }
}

impl LineTransform for ReplaceInColumnsIgnoreCase {
    fn apply(&self, line: &str) -> String {
        text::map_ranges(line, &self.span.write_ranges(line), |within| {
            self.pattern
                .replace_all(within, NoExpand(&self.replace))
                .into_owned()
        })
    }
}

/// Replaces every regex match with the replacement (which may use
/// capture group references like `$1`) within a column span.
pub struct RegexReplaceInColumns {
    pattern: Regex,
    replacement: String,
    span: ColumnSpan,
}

impl RegexReplaceInColumns {
    pub fn new(
        pattern: Regex,
        replacement: String,
        span: impl Into<ColumnSpan>,
    ) -> RegexReplaceInColumns {
        RegexReplaceInColumns {
            pattern,
            replacement,
            span: span.into(),
        }
    }
}

impl LineTransform for RegexReplaceInColumns {
    fn apply(&self, line: &str) -> String {
        text::map_ranges(line, &self.span.write_ranges(line), |within| {
            self.pattern
                .replace_all(within, self.replacement.as_str())
                .into_owned()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::columns::ColumnList;

    #[test]
    fn delete_columns_removes_range() {
        let transform = DeleteColumns::new(5..=10);
        assert_eq!(
            transform.apply("Test01234567891231234567"),
            "Test67891231234567"
        );
    }

    #[test]
    fn replace_in_columns_replaces_within_range() {
        let transform = ReplaceInColumns::new("123".to_owned(), "ABCD".to_owned(), 5..=10);
        assert_eq!(
            transform.apply("Test01234567891231234567"),
            "Test0ABCD4567891231234567"
        );
    }

    #[test]
    fn uppercase_columns_respects_range_and_unicode() {
        let transform = MapColumns::uppercase(1..=4);
        assert_eq!(transform.apply("ąbcdefgh"), "ĄBCDefgh");
    }

    #[test]
    fn lowercase_columns_respects_range_and_unicode() {
        let transform = MapColumns::lowercase(1..=4);
        assert_eq!(transform.apply("ĄBCDEFGH"), "ąbcdEFGH");
    }

    #[test]
    fn trim_columns_trims_whole_line_with_full_range() {
        let transform = MapColumns::trim(1..=usize::MAX);
        assert_eq!(transform.apply("  padded  "), "padded");
    }

    #[test]
    fn trim_columns_trims_only_inside_range() {
        //columns 4-9 are " mid  ", trimmed to "mid"
        let transform = MapColumns::trim(4..=9);
        assert_eq!(transform.apply("ab  mid   cd"), "ab mid cd");
    }

    #[test]
    fn ignore_case_replace_matches_any_case() {
        let transform = ReplaceInColumnsIgnoreCase::new("foo", "BAR".to_owned(), 1..=usize::MAX);
        assert_eq!(transform.apply("foo FOO Foo fOO"), "BAR BAR BAR BAR");
    }

    #[test]
    fn ignore_case_replace_folds_unicode_case() {
        let transform = ReplaceInColumnsIgnoreCase::new("łódź", "X".to_owned(), 1..=usize::MAX);
        assert_eq!(transform.apply("ŁÓDŹ łódź Łódź"), "X X X");
    }

    #[test]
    fn ignore_case_replace_does_not_expand_dollar_references() {
        //the literal find "a$1b" must not be treated as a regex, and the
        //replacement "$0" must be inserted verbatim
        let transform = ReplaceInColumnsIgnoreCase::new("a$1b", "$0".to_owned(), 1..=usize::MAX);
        assert_eq!(transform.apply("xA$1Bx"), "x$0x");
    }

    #[test]
    fn ignore_case_replace_respects_column_range() {
        let transform = ReplaceInColumnsIgnoreCase::new("ab", "X".to_owned(), 1..=4);
        assert_eq!(transform.apply("ABababAB"), "XXabAB");
    }

    #[test]
    fn regex_replace_in_columns_replaces_matches() {
        let transform =
            RegexReplaceInColumns::new(Regex::new(r"\d+").unwrap(), "N".to_owned(), 1..=usize::MAX);
        assert_eq!(transform.apply("a1 bb22 ccc333"), "aN bbN cccN");
    }

    #[test]
    fn regex_replace_supports_capture_groups() {
        let transform = RegexReplaceInColumns::new(
            Regex::new(r"(\w+)@(\w+)").unwrap(),
            "$2.$1".to_owned(),
            1..=usize::MAX,
        );
        assert_eq!(transform.apply("user@host"), "host.user");
    }

    #[test]
    fn regex_replace_respects_column_range() {
        let transform =
            RegexReplaceInColumns::new(Regex::new(r"\d").unwrap(), "X".to_owned(), 1..=4);
        assert_eq!(transform.apply("1234567890"), "XXXX567890");
    }

    #[test]
    fn select_columns_keeps_only_range() {
        let transform = SelectColumns::new(5..=10);
        assert_eq!(transform.apply("Test01234567891231234567"), "012345");
        //a line shorter than the range start selects nothing
        assert_eq!(transform.apply("abc"), "");
    }

    fn field_span(delimiter: &str, fields: std::ops::RangeInclusive<usize>) -> ColumnSpan {
        ColumnSpan::fields(delimiter, ColumnList::from(fields))
    }

    fn field_list(delimiter: &str, parts: &[std::ops::RangeInclusive<usize>]) -> ColumnSpan {
        ColumnSpan::fields(delimiter, ColumnList::new(parts.to_vec()))
    }

    fn char_list(parts: &[std::ops::RangeInclusive<usize>]) -> ColumnSpan {
        ColumnSpan::Chars(ColumnList::new(parts.to_vec()))
    }

    #[test]
    fn select_fields_keeps_only_the_field_range() {
        let transform = SelectColumns::new(field_span(",", 2..=3));
        assert_eq!(transform.apply("a,bb,ccc,d"), "bb,ccc");
        //a line with fewer fields than the range start selects nothing
        assert_eq!(transform.apply("a"), "");
    }

    #[test]
    fn delete_fields_removes_an_adjacent_delimiter_too() {
        let transform = DeleteColumns::new(field_span(",", 2..=2));
        assert_eq!(transform.apply("a,b,c"), "a,c");
        //deleting the last field removes the delimiter before it
        assert_eq!(transform.apply("a,b"), "a");
    }

    #[test]
    fn uppercase_fields_transforms_only_the_field_range() {
        let transform = MapColumns::uppercase(field_span(",", 2..=2));
        assert_eq!(transform.apply("ab,cd,ef"), "ab,CD,ef");
    }

    #[test]
    fn replace_in_fields_is_scoped_to_the_field_range() {
        let transform =
            ReplaceInColumns::new("x".to_owned(), "Y".to_owned(), field_span(",", 2..=2));
        assert_eq!(transform.apply("x,x,x"), "x,Y,x");
    }

    #[test]
    fn select_columns_reads_a_list_in_the_written_order() {
        let transform = SelectColumns::new(char_list(&[5..=6, 1..=2]));
        assert_eq!(transform.apply("abcdef"), "efab");
    }

    #[test]
    fn select_fields_permutes_and_rejoins_them() {
        let transform = SelectColumns::new(field_list(",", &[3..=3, 1..=1]));
        assert_eq!(transform.apply("a,b,c"), "c,a");

        //a missing field is skipped instead of joining a stray delimiter
        assert_eq!(transform.apply("a,b"), "a");
    }

    #[test]
    fn select_fields_joins_on_the_output_delimiter() {
        let span = ColumnSpan::Fields {
            delimiter: ",".to_owned(),
            output_delimiter: Some(" | ".to_owned()),
            fields: ColumnList::new(vec![2..=2, 1..=1]),
        };
        let transform = SelectColumns::new(span);
        assert_eq!(transform.apply("a,b"), "b | a");
    }

    #[test]
    fn delete_columns_removes_every_part_of_a_list() {
        let transform = DeleteColumns::new(char_list(&[1..=2, 5..=6]));
        assert_eq!(transform.apply("abcdef"), "cd");
    }

    #[test]
    fn delete_fields_removes_every_part_of_a_list() {
        let transform = DeleteColumns::new(field_list(",", &[1..=1, 3..=3]));
        assert_eq!(transform.apply("a,b,c"), "b");

        //adjacent parts normalize, so `2,3` deletes just like `2-3`
        let transform = DeleteColumns::new(field_list(",", &[2..=2, 3..=3]));
        assert_eq!(transform.apply("a,b,c"), "a");
    }

    #[test]
    fn map_columns_maps_every_part_of_a_list() {
        let transform = MapColumns::uppercase(char_list(&[1..=1, 3..=3]));
        assert_eq!(transform.apply("abc"), "AbC");
    }

    #[test]
    fn replace_in_columns_covers_every_part_of_a_list() {
        let transform = ReplaceInColumns::new(
            "x".to_owned(),
            "Y".to_owned(),
            field_list(",", &[1..=1, 3..=3]),
        );
        assert_eq!(transform.apply("x,x,x"), "Y,x,Y");
    }
}
