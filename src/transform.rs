//! Per-line operations composed into a processing pipeline.
//!
//! Each operation implements [`LineTransform`] and works on line content
//! without its terminator. [`build_pipeline`] derives the pipeline from
//! the configuration once, so adding a new operation means adding a new
//! transform here instead of branching inside the processing loop.

use std::ops::RangeInclusive;

use regex::Regex;

use crate::cli_args::{Config, FindPattern};
use crate::text;

/// A single per-line operation in the processing pipeline.
pub trait LineTransform {
    /// Transform line content (without its terminator).
    fn apply(&self, line: &str) -> String;
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

    if let (Some(find), Some(replace)) = (&config.find, &config.replace_string) {
        match find {
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
    fn build_pipeline_is_empty_by_default() {
        assert!(build_pipeline(&config()).is_empty());
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
