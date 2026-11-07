//! Row-level predicates: content-based tests deciding whether a line
//! takes part in processing. Unlike a [`crate::transform::LineTransform`],
//! a predicate does not change a line — it selects lines, complementing
//! the positional row range with a content filter.

use std::io;
use std::ops::RangeInclusive;

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

/// How far a count within `bounds` must go to answer: one past the upper
/// bound, or, without one, up to the lower bound.
fn count_limit(bounds: &RangeInclusive<usize>) -> usize {
    bounds
        .end()
        .checked_add(1)
        .unwrap_or(*bounds.start())
}

/// Matches lines whose number of fields lies within `bounds`
/// (`--min-fields`, `--max-fields`), split the way `--fields` splits
/// them (`--quoted` included). An empty line is one empty field.
pub struct FieldCount {
    spec: FieldSpan,
    bounds: RangeInclusive<usize>,
}

impl FieldCount {
    /// `spec` says how fields are split; its own field list is ignored.
    pub fn new(spec: FieldSpan, bounds: RangeInclusive<usize>) -> FieldCount {
        FieldCount { spec, bounds }
    }
}

impl LinePredicate for FieldCount {
    fn matches(&self, line: &str) -> io::Result<bool> {
        let count = self
            .spec
            .count_fields(line, count_limit(&self.bounds));
        Ok(self.bounds.contains(&count))
    }
}

/// Matches lines whose length lies within `bounds` (`--min-len`,
/// `--max-len`), in characters, or grapheme clusters under `--graphemes`.
pub struct Length {
    bounds: RangeInclusive<usize>,
    graphemes: bool,
}

impl Length {
    pub fn new(bounds: RangeInclusive<usize>, graphemes: bool) -> Length {
        Length { bounds, graphemes }
    }
}

impl LinePredicate for Length {
    fn matches(&self, line: &str) -> io::Result<bool> {
        let (min, max) = (*self.bounds.start(), *self.bounds.end());
        //no line has more chars (or clusters) than bytes, so a line with
        //few bytes needs no count
        if line.len() < min {
            return Ok(false);
        }
        if line.len() <= max && min == 0 {
            return Ok(true);
        }
        let limit = count_limit(&self.bounds);
        let length = match self.graphemes {
            true => line.graphemes(true).take(limit).count(),
            false => line.chars().take(limit).count(),
        };
        Ok(self.bounds.contains(&length))
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
    fn field_count_counts_the_way_fields_split() {
        // Given
        let plain = FieldCount::new(FieldSpan::new(",", ColumnList::from(1..=1)), 3..=usize::MAX);
        let mut quoted_spec = FieldSpan::new(",", ColumnList::from(1..=1));
        quoted_spec.quoted = true;
        let quoted = FieldCount::new(quoted_spec, 3..=usize::MAX);

        // When / Then
        assert!(plain.matches("a,b,c").unwrap());
        assert!(plain.matches("a,,").unwrap());
        assert!(!plain.matches("a,b").unwrap());
        assert!(plain.matches("a,\"b,c\"").unwrap());
        assert!(!quoted.matches("a,\"b,c\"").unwrap());
    }

    #[test]
    fn length_counts_chars_or_clusters() {
        // Given a decomposed accent: two chars, one cluster
        let line = "e\u{301}";

        // When / Then
        assert!(
            Length::new(0..=2, false)
                .matches(line)
                .unwrap()
        );
        assert!(
            !Length::new(0..=1, false)
                .matches(line)
                .unwrap()
        );
        assert!(
            Length::new(0..=1, true)
                .matches(line)
                .unwrap()
        );
        assert!(
            Length::new(0..=3, false)
                .matches("żół")
                .unwrap()
        );
        assert!(
            !Length::new(0..=2, false)
                .matches("żół")
                .unwrap()
        );
    }

    #[test]
    fn field_count_holds_both_bounds() {
        // Given rows of one to four fields, kept with two or three
        let between = FieldCount::new(FieldSpan::new(",", ColumnList::from(1..=1)), 2..=3);

        // When / Then
        let kept: Vec<bool> = ["a", "a,b", "a,b,c", "a,b,c,d", ""]
            .iter()
            .map(|line| between.matches(line).unwrap())
            .collect();
        assert_eq!(kept, [false, true, true, false, false]);
    }

    #[test]
    fn length_holds_both_bounds() {
        // Given a lower bound of two chars, measured in chars, not bytes
        let at_least_two = Length::new(2..=usize::MAX, false);
        let two_to_three = Length::new(2..=3, false);

        // When / Then
        assert!(!at_least_two.matches("").unwrap());
        assert!(!at_least_two.matches("ż").unwrap());
        assert!(at_least_two.matches("żó").unwrap());
        assert!(two_to_three.matches("abc").unwrap());
        assert!(!two_to_three.matches("abcd").unwrap());
        assert!(!two_to_three.matches("a").unwrap());
    }

    #[test]
    fn all_of_needs_every_predicate() {
        // Given
        let both = AllOf(vec![
            Box::new(GrepPredicate::new(pattern("a"), 1..=usize::MAX, false)),
            Box::new(Length::new(0..=3, false)),
        ]);

        // When / Then
        assert!(both.matches("abc").unwrap());
        assert!(!both.matches("abcd").unwrap());
        assert!(!both.matches("bc").unwrap());
    }
}
