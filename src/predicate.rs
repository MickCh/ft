//! Row-level predicates: content-based tests deciding whether a line
//! takes part in processing. Unlike a [`crate::transform::LineTransform`],
//! a predicate does not change a line — it selects lines, complementing
//! the positional row range with a content filter.

use std::ops::RangeInclusive;

use regex::Regex;

use crate::cli_args::Config;
use crate::text;

/// A content-based test applied to each line within the row range.
/// In selection mode failing lines are dropped; in delete mode they
/// are kept unchanged (only matching lines are deleted).
pub trait LinePredicate {
    /// Test line content (without its terminator).
    fn matches(&self, line: &str) -> bool;
}

/// Matches lines whose column range contains a regex match (`--grep`),
/// optionally inverted (`--invert`).
pub struct GrepPredicate {
    pattern: Regex,
    cols: RangeInclusive<usize>,
    invert: bool,
}

impl GrepPredicate {
    pub fn new(pattern: Regex, cols: RangeInclusive<usize>, invert: bool) -> GrepPredicate {
        GrepPredicate {
            pattern,
            cols,
            invert,
        }
    }
}

impl LinePredicate for GrepPredicate {
    fn matches(&self, line: &str) -> bool {
        let within = text::select_columns(line, &self.cols);
        self.pattern.is_match(&within) != self.invert
    }
}

/// Build the row filter implied by the configuration, if any.
pub fn build_predicate(config: &Config) -> Option<Box<dyn LinePredicate>> {
    config.grep.as_ref().map(|pattern| {
        Box::new(GrepPredicate::new(
            pattern.clone(),
            config.cols_or_full(),
            config.invert,
        )) as Box<dyn LinePredicate>
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grep_matches_lines_containing_pattern() {
        let predicate = GrepPredicate::new(Regex::new("ERROR").unwrap(), 1..=usize::MAX, false);

        assert!(predicate.matches("2024 ERROR disk full"));
        assert!(!predicate.matches("2024 INFO all good"));
    }

    #[test]
    fn invert_flips_the_match() {
        let predicate = GrepPredicate::new(Regex::new("ERROR").unwrap(), 1..=usize::MAX, true);

        assert!(!predicate.matches("2024 ERROR disk full"));
        assert!(predicate.matches("2024 INFO all good"));
    }

    #[test]
    fn grep_is_scoped_to_the_column_range() {
        let predicate = GrepPredicate::new(Regex::new("foo").unwrap(), 1..=3, false);

        assert!(predicate.matches("foo bar"));
        assert!(!predicate.matches("bar foo"));
        //a line shorter than the range start has nothing to match
        assert!(!predicate.matches(""));
    }
}
