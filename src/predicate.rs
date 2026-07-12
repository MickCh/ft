//! Row-level predicates: content-based tests deciding whether a line
//! takes part in processing. Unlike a [`crate::transform::LineTransform`],
//! a predicate does not change a line — it selects lines, complementing
//! the positional row range with a content filter.

use regex::Regex;

use crate::columns::ColumnSpan;
use crate::text;

/// A content-based test applied to each line within the row range.
/// In selection mode failing lines are dropped; in delete mode they
/// are kept unchanged (only matching lines are deleted).
pub trait LinePredicate {
    /// Test line content (without its terminator).
    fn matches(&self, line: &str) -> bool;
}

/// Matches lines whose column span contains a regex match (`--grep`),
/// optionally inverted (`--invert`).
pub struct GrepPredicate {
    pattern: Regex,
    span: ColumnSpan,
    invert: bool,
}

impl GrepPredicate {
    pub fn new(pattern: Regex, span: impl Into<ColumnSpan>, invert: bool) -> GrepPredicate {
        GrepPredicate {
            pattern,
            span: span.into(),
            invert,
        }
    }
}

impl LinePredicate for GrepPredicate {
    fn matches(&self, line: &str) -> bool {
        let within = text::select_columns(line, &self.span.char_range(line));
        self.pattern.is_match(within) != self.invert
    }
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

    #[test]
    fn grep_is_scoped_to_the_field_range() {
        let span = ColumnSpan::Fields {
            delimiter: ",".to_owned(),
            fields: 2..=2,
        };
        let predicate = GrepPredicate::new(Regex::new("foo").unwrap(), span, false);

        assert!(predicate.matches("bar,foo"));
        assert!(!predicate.matches("foo,bar"));
    }
}
