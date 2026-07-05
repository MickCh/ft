//! Per-line operations composed into a processing pipeline.
//!
//! Each operation implements [`LineTransform`] and works on line content
//! without its terminator. [`build_pipeline`] derives the pipeline from
//! the configuration once, so adding a new operation means adding a new
//! transform here instead of branching inside the processing loop.

use std::ops::RangeInclusive;

use crate::cli_args::Config;
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

/// Build the per-line transform pipeline implied by the configuration.
pub fn build_pipeline(config: &Config) -> Vec<Box<dyn LineTransform>> {
    let mut pipeline: Vec<Box<dyn LineTransform>> = Vec::new();

    if config.delete
        && let Some(cols) = &config.cols
    {
        pipeline.push(Box::new(DeleteColumns::new(cols.clone())));
    }

    if let (Some(find), Some(replace)) = (&config.find_string, &config.replace_string) {
        pipeline.push(Box::new(ReplaceInColumns::new(
            find.clone(),
            replace.clone(),
            config.cols_or_full(),
        )));
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
            delete: false,
            filename: None,
            find_string: None,
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
        config.find_string = Some("foo".to_owned());
        assert!(build_pipeline(&config).is_empty());

        config.replace_string = Some("bar".to_owned());
        assert_eq!(build_pipeline(&config).len(), 1);
    }
}
