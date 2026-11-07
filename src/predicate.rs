//! Row-level predicates: content-based tests deciding whether a line
//! takes part in processing. Unlike a [`crate::transform::LineTransform`],
//! a predicate does not change a line — it selects lines, complementing
//! the positional row range with a content filter.

use std::io;

use unicode_segmentation::UnicodeSegmentation;

use crate::columns::{ColumnSpan, FieldSpan};
use crate::pattern::Pattern;

/// A content-based test applied to each line within the row range.
/// In selection mode failing lines are dropped; in delete mode they
/// are kept unchanged (only matching lines are deleted).
pub trait LinePredicate {
    /// Test line content (without its terminator). Fails only when a
    /// pattern cannot be matched (a `-P` regex that backtracks out).
    fn matches(&self, line: &str) -> io::Result<bool>;
}

/// Matches lines whose column span contains a regex match (`--grep`),
/// optionally inverted (`--invert`). With several patterns a line must
/// match any of them, like `grep -e A -e B`, or every one (`--all`).
pub struct GrepPredicate {
    patterns: Vec<Pattern>,
    span: ColumnSpan,
    invert: bool,
    all: bool,
}

impl GrepPredicate {
    pub fn new(pattern: Pattern, span: impl Into<ColumnSpan>, invert: bool) -> GrepPredicate {
        GrepPredicate::any_of(vec![pattern], span, invert)
    }

    /// A line matches when any of the patterns does.
    pub fn any_of(
        patterns: Vec<Pattern>,
        span: impl Into<ColumnSpan>,
        invert: bool,
    ) -> GrepPredicate {
        GrepPredicate {
            patterns,
            span: span.into(),
            invert,
            all: false,
        }
    }

    /// A line matches only when every pattern does (`--all`).
    pub fn all_of(
        patterns: Vec<Pattern>,
        span: impl Into<ColumnSpan>,
        invert: bool,
    ) -> GrepPredicate {
        GrepPredicate {
            all: true,
            ..GrepPredicate::any_of(patterns, span, invert)
        }
    }
}

impl LinePredicate for GrepPredicate {
    fn matches(&self, line: &str) -> io::Result<bool> {
        let within = self.span.select(line);
        //every pattern matching is the answer under `--all` until one
        //misses, none matching is the answer otherwise until one hits
        let mut matched = self.all;
        for pattern in &self.patterns {
            if pattern.is_match(&within)? != self.all {
                matched = !self.all;
                break;
            }
        }
        Ok(matched != self.invert)
    }
}

/// Matches lines with at least `min` fields (`--min-fields`), split the
/// way `--fields` splits them (`--quoted` included). An empty line is
/// one empty field.
pub struct MinFields {
    spec: FieldSpan,
    min: usize,
}

impl MinFields {
    /// `spec` says how fields are split; its own field list is ignored.
    pub fn new(spec: FieldSpan, min: usize) -> MinFields {
        MinFields { spec, min }
    }
}

impl LinePredicate for MinFields {
    fn matches(&self, line: &str) -> io::Result<bool> {
        Ok(self.spec.count_fields(line, self.min) >= self.min)
    }
}

/// Matches lines at most `max` characters long (`--max-len`), or grapheme
/// clusters under `--graphemes`.
pub struct MaxLength {
    max: usize,
    graphemes: bool,
}

impl MaxLength {
    pub fn new(max: usize, graphemes: bool) -> MaxLength {
        MaxLength { max, graphemes }
    }
}

impl LinePredicate for MaxLength {
    fn matches(&self, line: &str) -> io::Result<bool> {
        //no line has more chars than bytes, so a short one needs no count
        if line.len() <= self.max {
            return Ok(true);
        }
        let length = match self.graphemes {
            true => line
                .graphemes(true)
                .take(self.max + 1)
                .count(),
            false => line.chars().take(self.max + 1).count(),
        };
        Ok(length <= self.max)
    }
}

/// Matches lines every one of its predicates matches: the filters given
/// together narrow each other down.
pub struct AllOf(pub Vec<Box<dyn LinePredicate>>);

impl LinePredicate for AllOf {
    fn matches(&self, line: &str) -> io::Result<bool> {
        for predicate in &self.0 {
            if !predicate.matches(line)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::columns::ColumnList;
    use crate::pattern::Pattern;
    use crate::pattern::Syntax;

    #[test]
    fn grep_matches_lines_containing_pattern() {
        let predicate = GrepPredicate::new(
            Pattern::new("ERROR", false, Syntax::Linear).unwrap(),
            1..=usize::MAX,
            false,
        );

        assert!(
            predicate
                .matches("2024 ERROR disk full")
                .unwrap()
        );
        assert!(
            !predicate
                .matches("2024 INFO all good")
                .unwrap()
        );
    }

    #[test]
    fn invert_flips_the_match() {
        let predicate = GrepPredicate::new(
            Pattern::new("ERROR", false, Syntax::Linear).unwrap(),
            1..=usize::MAX,
            true,
        );

        assert!(
            !predicate
                .matches("2024 ERROR disk full")
                .unwrap()
        );
        assert!(
            predicate
                .matches("2024 INFO all good")
                .unwrap()
        );
    }

    #[test]
    fn grep_is_scoped_to_the_column_range() {
        let predicate = GrepPredicate::new(
            Pattern::new("foo", false, Syntax::Linear).unwrap(),
            1..=3,
            false,
        );

        assert!(predicate.matches("foo bar").unwrap());
        assert!(!predicate.matches("bar foo").unwrap());
        //a line shorter than the range start has nothing to match
        assert!(!predicate.matches("").unwrap());
    }

    #[test]
    fn grep_is_scoped_to_the_field_range() {
        let span = ColumnSpan::fields(",", ColumnList::from(2..=2));
        let predicate = GrepPredicate::new(
            Pattern::new("foo", false, Syntax::Linear).unwrap(),
            span,
            false,
        );

        assert!(predicate.matches("bar,foo").unwrap());
        assert!(!predicate.matches("foo,bar").unwrap());
    }

    #[test]
    fn grep_matches_the_selected_fields_joined() {
        //fields 1 and 3, read in that order and joined by the delimiter
        let span = ColumnSpan::fields(",", ColumnList::new(vec![1..=1, 3..=3]));
        let predicate = GrepPredicate::new(
            Pattern::new("^a,c$", false, Syntax::Linear).unwrap(),
            span,
            false,
        );

        assert!(predicate.matches("a,b,c").unwrap());
        assert!(!predicate.matches("a,b,x").unwrap());
    }

    fn pattern(source: &str) -> Pattern {
        Pattern::new(source, false, Syntax::Linear).unwrap()
    }

    #[test]
    fn several_patterns_match_any_or_all() {
        // Given two patterns
        let any = GrepPredicate::any_of(vec![pattern("a"), pattern("b")], 1..=usize::MAX, false);
        let all = GrepPredicate::all_of(vec![pattern("a"), pattern("b")], 1..=usize::MAX, false);
        let none = GrepPredicate::all_of(vec![pattern("a"), pattern("b")], 1..=usize::MAX, true);

        // When / Then
        for (line, matches_any, matches_all) in [
            ("ab", true, true),
            ("a", true, false),
            ("b", true, false),
            ("c", false, false),
        ] {
            assert_eq!(any.matches(line).unwrap(), matches_any, "{line}");
            assert_eq!(all.matches(line).unwrap(), matches_all, "{line}");
            assert_eq!(none.matches(line).unwrap(), !matches_all, "{line}");
        }
    }

    #[test]
    fn min_fields_counts_the_way_fields_split() {
        // Given
        let plain = MinFields::new(FieldSpan::new(",", ColumnList::from(1..=1)), 3);
        let mut quoted_spec = FieldSpan::new(",", ColumnList::from(1..=1));
        quoted_spec.quoted = true;
        let quoted = MinFields::new(quoted_spec, 3);

        // When / Then
        assert!(plain.matches("a,b,c").unwrap());
        assert!(plain.matches("a,,").unwrap());
        assert!(!plain.matches("a,b").unwrap());
        assert!(plain.matches("a,\"b,c\"").unwrap());
        assert!(!quoted.matches("a,\"b,c\"").unwrap());
    }

    #[test]
    fn max_length_counts_chars_or_clusters() {
        // Given a decomposed accent: two chars, one cluster
        let line = "e\u{301}";

        // When / Then
        assert!(
            MaxLength::new(2, false)
                .matches(line)
                .unwrap()
        );
        assert!(
            !MaxLength::new(1, false)
                .matches(line)
                .unwrap()
        );
        assert!(
            MaxLength::new(1, true)
                .matches(line)
                .unwrap()
        );
        assert!(
            MaxLength::new(3, false)
                .matches("żół")
                .unwrap()
        );
        assert!(
            !MaxLength::new(2, false)
                .matches("żół")
                .unwrap()
        );
    }

    #[test]
    fn all_of_needs_every_predicate() {
        // Given
        let both = AllOf(vec![
            Box::new(GrepPredicate::new(pattern("a"), 1..=usize::MAX, false)),
            Box::new(MaxLength::new(3, false)),
        ]);

        // When / Then
        assert!(both.matches("abc").unwrap());
        assert!(!both.matches("abcd").unwrap());
        assert!(!both.matches("bc").unwrap());
    }
}
