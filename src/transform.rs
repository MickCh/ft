//! Per-line operations composed into a processing pipeline.
//!
//! Each operation implements [`LineTransform`] and works on line content
//! without its terminator. A transform is not bound to produce exactly
//! one line: it reports a [`LineOutcome`], so it may leave the line
//! alone, rewrite it, expand it into several lines or drop it. The
//! pipeline itself is derived from the configuration by
//! [`crate::compose`], so adding a new operation means adding a new
//! transform here instead of branching inside the processing loop.

use std::borrow::Cow;

use regex::{NoExpand, Regex, RegexBuilder};

use crate::columns::ColumnSpan;
use crate::text;

/// What a transform made of a line.
#[derive(Debug, PartialEq, Eq)]
pub enum LineOutcome {
    /// The line is left as it is (no allocation needed).
    Keep,
    /// The line is rewritten.
    Replace(String),
    /// The line becomes several lines.
    Expand(Vec<String>),
    /// The line disappears.
    Drop,
}

/// A single per-line operation in the processing pipeline.
pub trait LineTransform {
    /// Transform line content (without its terminator).
    fn apply(&self, line: &str) -> LineOutcome;
}

/// What a whole pipeline made of one input line: still a single line —
/// borrowed while no transform has touched it — or several of them, and
/// none at all once a transform dropped it.
#[derive(Debug, PartialEq, Eq)]
pub enum Lines<'a> {
    One(Cow<'a, str>),
    Several(Vec<String>),
}

impl<'a> Lines<'a> {
    /// Run every line through one more transform, flattening whatever
    /// each of them expands into.
    fn through(self, transform: &dyn LineTransform) -> Lines<'a> {
        match self {
            Lines::One(content) => match transform.apply(&content) {
                LineOutcome::Keep => Lines::One(content),
                LineOutcome::Replace(rewritten) => Lines::One(Cow::Owned(rewritten)),
                LineOutcome::Expand(lines) => Lines::Several(lines),
                LineOutcome::Drop => Lines::Several(Vec::new()),
            },
            Lines::Several(contents) => {
                let mut lines = Vec::with_capacity(contents.len());
                for content in contents {
                    match transform.apply(&content) {
                        LineOutcome::Keep => lines.push(content),
                        LineOutcome::Replace(rewritten) => lines.push(rewritten),
                        LineOutcome::Expand(expanded) => lines.extend(expanded),
                        LineOutcome::Drop => {}
                    }
                }
                Lines::Several(lines)
            }
        }
    }
}

/// The transforms applied to every processed line, in order.
#[derive(Default)]
pub struct Pipeline {
    transforms: Vec<Box<dyn LineTransform>>,
}

impl Pipeline {
    pub fn new(transforms: Vec<Box<dyn LineTransform>>) -> Pipeline {
        Pipeline { transforms }
    }

    pub fn len(&self) -> usize {
        self.transforms.len()
    }

    pub fn is_empty(&self) -> bool {
        self.transforms.is_empty()
    }

    /// Run one line through every transform. With no transforms
    /// configured the line passes through without an allocation.
    pub fn apply<'a>(&self, line: &'a str) -> Lines<'a> {
        self.transforms
            .iter()
            .fold(Lines::One(Cow::Borrowed(line)), |lines, transform| {
                lines.through(transform.as_ref())
            })
    }
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
    fn apply(&self, line: &str) -> LineOutcome {
        LineOutcome::Replace(
            text::select_ranges(line, &self.span.read_ranges(line), self.span.joiner())
                .into_owned(),
        )
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
    fn apply(&self, line: &str) -> LineOutcome {
        LineOutcome::Replace(text::remove_ranges(line, &self.span.delete_ranges(line)))
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
    fn apply(&self, line: &str) -> LineOutcome {
        LineOutcome::Replace(text::map_ranges(
            line,
            &self.span.write_ranges(line),
            self.map,
        ))
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
    fn apply(&self, line: &str) -> LineOutcome {
        LineOutcome::Replace(text::replace_in_ranges(
            line,
            &self.find,
            &self.replace,
            &self.span.write_ranges(line),
        ))
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
    fn apply(&self, line: &str) -> LineOutcome {
        LineOutcome::Replace(text::map_ranges(
            line,
            &self.span.write_ranges(line),
            |within| {
                self.pattern
                    .replace_all(within, NoExpand(&self.replace))
                    .into_owned()
            },
        ))
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
    fn apply(&self, line: &str) -> LineOutcome {
        LineOutcome::Replace(text::map_ranges(
            line,
            &self.span.write_ranges(line),
            |within| {
                self.pattern
                    .replace_all(within, self.replacement.as_str())
                    .into_owned()
            },
        ))
    }
}

/// Hard-wraps the line into chunks of at most `width` characters, like
/// `fold -w`: the first transform to turn one line into several.
pub struct WrapLines {
    width: usize,
}

impl WrapLines {
    pub fn new(width: usize) -> WrapLines {
        WrapLines { width }
    }
}

impl LineTransform for WrapLines {
    fn apply(&self, line: &str) -> LineOutcome {
        let chunks = text::wrap_chars(line, self.width);
        match chunks.as_slice() {
            //a line that already fits is left alone, allocating nothing
            [_] => LineOutcome::Keep,
            chunks => LineOutcome::Expand(
                chunks
                    .iter()
                    .map(|chunk| (*chunk).to_owned())
                    .collect(),
            ),
        }
    }
}

/// Drops lines that are empty *after* the transforms before it ran —
/// which is what a predicate cannot do, since it runs on the line as it
/// was read. Put `--trim` in front of it to drop whitespace-only lines.
pub struct DropEmpty;

impl LineTransform for DropEmpty {
    fn apply(&self, line: &str) -> LineOutcome {
        if line.is_empty() {
            LineOutcome::Drop
        } else {
            LineOutcome::Keep
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::columns::{ColumnList, FieldSpan};

    /// The one line a transform leaves behind, for the transforms that
    /// rewrite a line rather than expand or drop it.
    fn applied(transform: &dyn LineTransform, line: &str) -> String {
        match transform.apply(line) {
            LineOutcome::Keep => line.to_owned(),
            LineOutcome::Replace(rewritten) => rewritten,
            outcome => panic!("expected a single line, got {outcome:?}"),
        }
    }

    #[test]
    fn delete_columns_removes_range() {
        let transform = DeleteColumns::new(5..=10);
        assert_eq!(
            applied(&transform, "Test01234567891231234567"),
            "Test67891231234567"
        );
    }

    #[test]
    fn replace_in_columns_replaces_within_range() {
        let transform = ReplaceInColumns::new("123".to_owned(), "ABCD".to_owned(), 5..=10);
        assert_eq!(
            applied(&transform, "Test01234567891231234567"),
            "Test0ABCD4567891231234567"
        );
    }

    #[test]
    fn uppercase_columns_respects_range_and_unicode() {
        let transform = MapColumns::uppercase(1..=4);
        assert_eq!(applied(&transform, "ąbcdefgh"), "ĄBCDefgh");
    }

    #[test]
    fn lowercase_columns_respects_range_and_unicode() {
        let transform = MapColumns::lowercase(1..=4);
        assert_eq!(applied(&transform, "ĄBCDEFGH"), "ąbcdEFGH");
    }

    #[test]
    fn trim_columns_trims_whole_line_with_full_range() {
        let transform = MapColumns::trim(1..=usize::MAX);
        assert_eq!(applied(&transform, "  padded  "), "padded");
    }

    #[test]
    fn trim_columns_trims_only_inside_range() {
        //columns 4-9 are " mid  ", trimmed to "mid"
        let transform = MapColumns::trim(4..=9);
        assert_eq!(applied(&transform, "ab  mid   cd"), "ab mid cd");
    }

    #[test]
    fn ignore_case_replace_matches_any_case() {
        let transform = ReplaceInColumnsIgnoreCase::new("foo", "BAR".to_owned(), 1..=usize::MAX);
        assert_eq!(applied(&transform, "foo FOO Foo fOO"), "BAR BAR BAR BAR");
    }

    #[test]
    fn ignore_case_replace_folds_unicode_case() {
        let transform = ReplaceInColumnsIgnoreCase::new("łódź", "X".to_owned(), 1..=usize::MAX);
        assert_eq!(applied(&transform, "ŁÓDŹ łódź Łódź"), "X X X");
    }

    #[test]
    fn ignore_case_replace_does_not_expand_dollar_references() {
        //the literal find "a$1b" must not be treated as a regex, and the
        //replacement "$0" must be inserted verbatim
        let transform = ReplaceInColumnsIgnoreCase::new("a$1b", "$0".to_owned(), 1..=usize::MAX);
        assert_eq!(applied(&transform, "xA$1Bx"), "x$0x");
    }

    #[test]
    fn ignore_case_replace_respects_column_range() {
        let transform = ReplaceInColumnsIgnoreCase::new("ab", "X".to_owned(), 1..=4);
        assert_eq!(applied(&transform, "ABababAB"), "XXabAB");
    }

    #[test]
    fn regex_replace_in_columns_replaces_matches() {
        let transform =
            RegexReplaceInColumns::new(Regex::new(r"\d+").unwrap(), "N".to_owned(), 1..=usize::MAX);
        assert_eq!(applied(&transform, "a1 bb22 ccc333"), "aN bbN cccN");
    }

    #[test]
    fn regex_replace_supports_capture_groups() {
        let transform = RegexReplaceInColumns::new(
            Regex::new(r"(\w+)@(\w+)").unwrap(),
            "$2.$1".to_owned(),
            1..=usize::MAX,
        );
        assert_eq!(applied(&transform, "user@host"), "host.user");
    }

    #[test]
    fn regex_replace_respects_column_range() {
        let transform =
            RegexReplaceInColumns::new(Regex::new(r"\d").unwrap(), "X".to_owned(), 1..=4);
        assert_eq!(applied(&transform, "1234567890"), "XXXX567890");
    }

    #[test]
    fn select_columns_keeps_only_range() {
        let transform = SelectColumns::new(5..=10);
        assert_eq!(applied(&transform, "Test01234567891231234567"), "012345");
        //a line shorter than the range start selects nothing
        assert_eq!(applied(&transform, "abc"), "");
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
        assert_eq!(applied(&transform, "a,bb,ccc,d"), "bb,ccc");
        //a line with fewer fields than the range start selects nothing
        assert_eq!(applied(&transform, "a"), "");
    }

    #[test]
    fn delete_fields_removes_an_adjacent_delimiter_too() {
        let transform = DeleteColumns::new(field_span(",", 2..=2));
        assert_eq!(applied(&transform, "a,b,c"), "a,c");
        //deleting the last field removes the delimiter before it
        assert_eq!(applied(&transform, "a,b"), "a");
    }

    #[test]
    fn uppercase_fields_transforms_only_the_field_range() {
        let transform = MapColumns::uppercase(field_span(",", 2..=2));
        assert_eq!(applied(&transform, "ab,cd,ef"), "ab,CD,ef");
    }

    #[test]
    fn replace_in_fields_is_scoped_to_the_field_range() {
        let transform =
            ReplaceInColumns::new("x".to_owned(), "Y".to_owned(), field_span(",", 2..=2));
        assert_eq!(applied(&transform, "x,x,x"), "x,Y,x");
    }

    #[test]
    fn select_columns_reads_a_list_in_the_written_order() {
        let transform = SelectColumns::new(char_list(&[5..=6, 1..=2]));
        assert_eq!(applied(&transform, "abcdef"), "efab");
    }

    #[test]
    fn select_fields_permutes_and_rejoins_them() {
        let transform = SelectColumns::new(field_list(",", &[3..=3, 1..=1]));
        assert_eq!(applied(&transform, "a,b,c"), "c,a");

        //a missing field is skipped instead of joining a stray delimiter
        assert_eq!(applied(&transform, "a,b"), "a");
    }

    #[test]
    fn select_fields_joins_on_the_output_delimiter() {
        let span = ColumnSpan::Fields(FieldSpan {
            output_delimiter: Some(" | ".to_owned()),
            ..FieldSpan::new(",", ColumnList::new(vec![2..=2, 1..=1]))
        });
        let transform = SelectColumns::new(span);
        assert_eq!(applied(&transform, "a,b"), "b | a");
    }

    #[test]
    fn delete_columns_removes_every_part_of_a_list() {
        let transform = DeleteColumns::new(char_list(&[1..=2, 5..=6]));
        assert_eq!(applied(&transform, "abcdef"), "cd");
    }

    #[test]
    fn delete_fields_removes_every_part_of_a_list() {
        let transform = DeleteColumns::new(field_list(",", &[1..=1, 3..=3]));
        assert_eq!(applied(&transform, "a,b,c"), "b");

        //adjacent parts normalize, so `2,3` deletes just like `2-3`
        let transform = DeleteColumns::new(field_list(",", &[2..=2, 3..=3]));
        assert_eq!(applied(&transform, "a,b,c"), "a");
    }

    #[test]
    fn map_columns_maps_every_part_of_a_list() {
        let transform = MapColumns::uppercase(char_list(&[1..=1, 3..=3]));
        assert_eq!(applied(&transform, "abc"), "AbC");
    }

    #[test]
    fn replace_in_columns_covers_every_part_of_a_list() {
        let transform = ReplaceInColumns::new(
            "x".to_owned(),
            "Y".to_owned(),
            field_list(",", &[1..=1, 3..=3]),
        );
        assert_eq!(applied(&transform, "x,x,x"), "Y,x,Y");
    }

    #[test]
    fn wrap_expands_a_long_line_into_several() {
        let transform = WrapLines::new(3);
        assert_eq!(
            transform.apply("abcdefg"),
            LineOutcome::Expand(vec!["abc".to_owned(), "def".to_owned(), "g".to_owned()])
        );
    }

    #[test]
    fn wrap_keeps_a_line_that_already_fits() {
        let transform = WrapLines::new(3);
        assert_eq!(transform.apply("abc"), LineOutcome::Keep);
        assert_eq!(transform.apply(""), LineOutcome::Keep);
    }

    #[test]
    fn drop_empty_drops_only_empty_lines() {
        let transform = DropEmpty;
        assert_eq!(transform.apply(""), LineOutcome::Drop);
        assert_eq!(transform.apply("a"), LineOutcome::Keep);
        //whitespace is content; --trim in front of it makes it empty
        assert_eq!(transform.apply("  "), LineOutcome::Keep);
    }

    #[test]
    fn empty_pipeline_borrows_the_line() {
        let pipeline = Pipeline::default();
        assert!(pipeline.is_empty());
        assert_eq!(pipeline.apply("abc"), Lines::One(Cow::Borrowed("abc")));
    }

    #[test]
    fn pipeline_applies_transforms_in_order() {
        let pipeline = Pipeline::new(vec![
            Box::new(ReplaceInColumns::new(
                "a".to_owned(),
                "b".to_owned(),
                1..=usize::MAX,
            )),
            Box::new(MapColumns::uppercase(1..=usize::MAX)),
        ]);
        assert_eq!(pipeline.apply("a"), Lines::One(Cow::Owned("B".to_owned())));
    }

    #[test]
    fn pipeline_runs_later_transforms_on_every_expanded_line() {
        //wrapping first, so --upper must reach each of the three chunks
        let pipeline = Pipeline::new(vec![
            Box::new(WrapLines::new(2)),
            Box::new(MapColumns::uppercase(1..=usize::MAX)),
        ]);
        assert_eq!(
            pipeline.apply("abcde"),
            Lines::Several(vec!["AB".to_owned(), "CD".to_owned(), "E".to_owned()])
        );
    }

    #[test]
    fn pipeline_drops_expanded_lines_one_by_one() {
        //splitting "a,,b" into fields would leave an empty middle line
        let pipeline = Pipeline::new(vec![Box::new(WrapLines::new(1)), Box::new(DropEmpty)]);
        assert_eq!(
            pipeline.apply("ab"),
            Lines::Several(vec!["a".to_owned(), "b".to_owned()])
        );

        //a dropped single line leaves nothing behind
        let pipeline = Pipeline::new(vec![Box::new(DropEmpty)]);
        assert_eq!(pipeline.apply(""), Lines::Several(Vec::new()));
    }
}
