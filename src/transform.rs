//! Per-line operations composed into a processing pipeline.
//!
//! Each operation implements [`LineTransform`] and works on line content
//! without its terminator. [`build_pipeline`] derives the pipeline from
//! the configuration once, so adding a new operation means adding a new
//! transform here instead of branching inside the processing loop.

use std::ops::RangeInclusive;

use regex::{NoExpand, Regex, RegexBuilder};

use crate::cli_args::{Config, FindPattern};
use crate::text;

/// A single per-line operation in the processing pipeline.
pub trait LineTransform {
    /// Transform line content (without its terminator).
    fn apply(&self, line: &str) -> String;
}

/// Keeps only the characters within a column range (like `cut`).
pub struct SelectColumns {
    cols: RangeInclusive<usize>,
}

impl SelectColumns {
    pub fn new(cols: RangeInclusive<usize>) -> SelectColumns {
        SelectColumns { cols }
    }
}

impl LineTransform for SelectColumns {
    fn apply(&self, line: &str) -> String {
        text::select_columns(line, &self.cols)
    }
}

/// Removes the characters within a column range.
pub struct DeleteColumns {
    cols: RangeInclusive<usize>,
}

impl DeleteColumns {
    pub fn new(cols: RangeInclusive<usize>) -> DeleteColumns {
        DeleteColumns { cols }
    }
}

impl LineTransform for DeleteColumns {
    fn apply(&self, line: &str) -> String {
        text::remove_columns(line, &self.cols)
    }
}

/// Uppercases the characters within a column range.
pub struct UppercaseColumns {
    cols: RangeInclusive<usize>,
}

impl UppercaseColumns {
    pub fn new(cols: RangeInclusive<usize>) -> UppercaseColumns {
        UppercaseColumns { cols }
    }
}

impl LineTransform for UppercaseColumns {
    fn apply(&self, line: &str) -> String {
        text::map_columns(line, &self.cols, |within| within.to_uppercase())
    }
}

/// Lowercases the characters within a column range.
pub struct LowercaseColumns {
    cols: RangeInclusive<usize>,
}

impl LowercaseColumns {
    pub fn new(cols: RangeInclusive<usize>) -> LowercaseColumns {
        LowercaseColumns { cols }
    }
}

impl LineTransform for LowercaseColumns {
    fn apply(&self, line: &str) -> String {
        text::map_columns(line, &self.cols, |within| within.to_lowercase())
    }
}

/// Trims whitespace at both ends of a column range (with the full
/// range, this trims the whole line).
pub struct TrimColumns {
    cols: RangeInclusive<usize>,
}

impl TrimColumns {
    pub fn new(cols: RangeInclusive<usize>) -> TrimColumns {
        TrimColumns { cols }
    }
}

impl LineTransform for TrimColumns {
    fn apply(&self, line: &str) -> String {
        text::map_columns(line, &self.cols, |within| within.trim().to_owned())
    }
}

/// Replaces `find` with `replace` within a column range.
pub struct ReplaceInColumns {
    find: String,
    replace: String,
    cols: RangeInclusive<usize>,
}

impl ReplaceInColumns {
    pub fn new(find: String, replace: String, cols: RangeInclusive<usize>) -> ReplaceInColumns {
        ReplaceInColumns {
            find,
            replace,
            cols,
        }
    }
}

impl LineTransform for ReplaceInColumns {
    fn apply(&self, line: &str) -> String {
        text::replace_in_columns(line, &self.find, &self.replace, &self.cols)
    }
}

/// Replaces case-insensitive occurrences of a literal within a column
/// range. The literal is matched through an escaped regex so Unicode
/// case folding applies; the replacement is inserted verbatim.
pub struct ReplaceInColumnsIgnoreCase {
    pattern: Regex,
    replace: String,
    cols: RangeInclusive<usize>,
}

impl ReplaceInColumnsIgnoreCase {
    pub fn new(
        find: &str,
        replace: String,
        cols: RangeInclusive<usize>,
    ) -> ReplaceInColumnsIgnoreCase {
        let pattern = RegexBuilder::new(&regex::escape(find))
            .case_insensitive(true)
            .build()
            .expect("an escaped literal is always a valid regex");
        ReplaceInColumnsIgnoreCase {
            pattern,
            replace,
            cols,
        }
    }
}

impl LineTransform for ReplaceInColumnsIgnoreCase {
    fn apply(&self, line: &str) -> String {
        text::map_columns(line, &self.cols, |within| {
            self.pattern
                .replace_all(within, NoExpand(&self.replace))
                .into_owned()
        })
    }
}

/// Replaces every regex match with the replacement (which may use
/// capture group references like `$1`) within a column range.
pub struct RegexReplaceInColumns {
    pattern: Regex,
    replacement: String,
    cols: RangeInclusive<usize>,
}

impl RegexReplaceInColumns {
    pub fn new(
        pattern: Regex,
        replacement: String,
        cols: RangeInclusive<usize>,
    ) -> RegexReplaceInColumns {
        RegexReplaceInColumns {
            pattern,
            replacement,
            cols,
        }
    }
}

impl LineTransform for RegexReplaceInColumns {
    fn apply(&self, line: &str) -> String {
        text::map_columns(line, &self.cols, |within| {
            self.pattern
                .replace_all(within, self.replacement.as_str())
                .into_owned()
        })
    }
}

/// Build the per-line transform pipeline implied by the configuration.
pub fn build_pipeline(config: &Config) -> Vec<Box<dyn LineTransform>> {
    let mut pipeline: Vec<Box<dyn LineTransform>> = Vec::new();

    if config.delete
        && let Some(cols) = &config.cols
    {
        pipeline.push(Box::new(DeleteColumns::new(cols.clone())));
    }

    //with no operation claiming the column range, `--cols` alone
    //selects the range, mirroring how `--rows` alone selects lines
    if !config.has_column_operation()
        && let Some(cols) = &config.cols
    {
        pipeline.push(Box::new(SelectColumns::new(cols.clone())));
    }

    if let (Some(find), Some(replace)) = (&config.find, &config.replace_string) {
        match find {
            FindPattern::Literal(text) if config.ignore_case => pipeline.push(Box::new(
                ReplaceInColumnsIgnoreCase::new(text, replace.clone(), config.cols_or_full()),
            )),
            FindPattern::Literal(text) => pipeline.push(Box::new(ReplaceInColumns::new(
                text.clone(),
                replace.clone(),
                config.cols_or_full(),
            ))),
            FindPattern::Regex(pattern) => pipeline.push(Box::new(RegexReplaceInColumns::new(
                pattern.clone(),
                replace.clone(),
                config.cols_or_full(),
            ))),
        }
    }

    if config.upper {
        pipeline.push(Box::new(UppercaseColumns::new(config.cols_or_full())));
    }
    if config.lower {
        pipeline.push(Box::new(LowercaseColumns::new(config.cols_or_full())));
    }
    if config.trim {
        pipeline.push(Box::new(TrimColumns::new(config.cols_or_full())));
    }

    pipeline
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        Config {
            rows: None,
            cols: None,
            sort: false,
            numeric_sort: false,
            reverse_sort: false,
            delete: false,
            ignore_case: false,
            upper: false,
            lower: false,
            trim: false,
            grep: None,
            invert: false,
            unique: false,
            filename: None,
            find: None,
            replace_string: None,
            output_filename: None,
        }
    }

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
        let transform = UppercaseColumns::new(1..=4);
        assert_eq!(transform.apply("ąbcdefgh"), "ĄBCDefgh");
    }

    #[test]
    fn lowercase_columns_respects_range_and_unicode() {
        let transform = LowercaseColumns::new(1..=4);
        assert_eq!(transform.apply("ĄBCDEFGH"), "ąbcdEFGH");
    }

    #[test]
    fn trim_columns_trims_whole_line_with_full_range() {
        let transform = TrimColumns::new(1..=usize::MAX);
        assert_eq!(transform.apply("  padded  "), "padded");
    }

    #[test]
    fn trim_columns_trims_only_inside_range() {
        //columns 4-9 are " mid  ", trimmed to "mid"
        let transform = TrimColumns::new(4..=9);
        assert_eq!(transform.apply("ab  mid   cd"), "ab mid cd");
    }

    #[test]
    fn build_pipeline_orders_replace_before_case_transforms() {
        let mut config = config();
        config.upper = true;
        config.find = Some(FindPattern::Literal("foo".to_owned()));
        config.replace_string = Some("bar".to_owned());

        let pipeline = build_pipeline(&config);
        assert_eq!(pipeline.len(), 2);
        //replace runs first, so the replacement is uppercased too
        let result = pipeline
            .iter()
            .fold("x foo y".to_owned(), |line, transform| {
                transform.apply(&line)
            });
        assert_eq!(result, "X BAR Y");
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

    #[test]
    fn build_pipeline_is_empty_by_default() {
        assert!(build_pipeline(&config()).is_empty());
    }

    #[test]
    fn build_pipeline_selects_columns_when_no_other_operation() {
        let mut config = config();
        config.cols = Some(5..=10);
        assert_eq!(build_pipeline(&config).len(), 1);
    }

    #[test]
    fn build_pipeline_does_not_select_columns_when_they_key_another_operation() {
        //sort uses the column range as its key
        let mut sort_config = config();
        sort_config.cols = Some(5..=10);
        sort_config.sort = true;
        assert!(build_pipeline(&sort_config).is_empty());

        //find/replace is scoped by the column range
        let mut replace_config = config();
        replace_config.cols = Some(5..=10);
        replace_config.find = Some(FindPattern::Literal("a".to_owned()));
        replace_config.replace_string = Some("b".to_owned());
        let pipeline = build_pipeline(&replace_config);
        assert_eq!(pipeline.len(), 1);
        assert_eq!(pipeline[0].apply("aaaa aaaa"), "aaaa bbbb");
    }

    #[test]
    fn build_pipeline_adds_delete_columns() {
        let mut config = config();
        config.delete = true;
        config.cols = Some(5..=10);
        assert_eq!(build_pipeline(&config).len(), 1);
    }

    #[test]
    fn build_pipeline_ignores_delete_without_columns() {
        let mut config = config();
        config.delete = true;
        assert!(build_pipeline(&config).is_empty());
    }

    #[test]
    fn build_pipeline_adds_replace_only_when_find_and_replace_present() {
        let mut config = config();
        config.find = Some(FindPattern::Literal("foo".to_owned()));
        assert!(build_pipeline(&config).is_empty());

        config.replace_string = Some("bar".to_owned());
        assert_eq!(build_pipeline(&config).len(), 1);
    }
}
