//! A set of 1-based, inclusive ranges, possibly non-contiguous
//! (e.g. rows `1-5,10-20`). Kept normalized: sorted and merged.

use std::ops::RangeInclusive;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeSet {
    parts: Vec<RangeInclusive<usize>>,
}

impl RangeSet {
    /// Build a normalized set: parts are sorted by start and
    /// overlapping or adjacent parts are merged into one.
    pub fn new(mut parts: Vec<RangeInclusive<usize>>) -> RangeSet {
        parts.sort_by_key(|part| *part.start());

        let mut merged: Vec<RangeInclusive<usize>> = Vec::new();
        for part in parts {
            match merged.last_mut() {
                Some(last) if *part.start() <= last.end().saturating_add(1) => {
                    if part.end() > last.end() {
                        *last = *last.start()..=*part.end();
                    }
                }
                _ => merged.push(part),
            }
        }

        RangeSet { parts: merged }
    }

    /// The set covering everything (row/column 1 onwards).
    pub fn full() -> RangeSet {
        RangeSet::from(1..=usize::MAX)
    }

    pub fn contains(&self, value: usize) -> bool {
        self.parts
            .iter()
            .any(|part| part.contains(&value))
    }

    /// The largest value in the set (`usize::MAX` for open-ended sets).
    pub fn end(&self) -> usize {
        self.parts
            .last()
            .map_or(usize::MAX, |part| *part.end())
    }
}

impl From<RangeInclusive<usize>> for RangeSet {
    fn from(range: RangeInclusive<usize>) -> RangeSet {
        RangeSet { parts: vec![range] }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_checks_every_part() {
        let set = RangeSet::new(vec![1..=2, 5..=6]);

        assert!(set.contains(1));
        assert!(set.contains(2));
        assert!(!set.contains(3));
        assert!(!set.contains(4));
        assert!(set.contains(5));
        assert!(set.contains(6));
        assert!(!set.contains(7));
    }

    #[test]
    fn parts_are_sorted_and_merged() {
        //5-8 overlaps 7-10, and 11 is adjacent to 10
        let set = RangeSet::new(vec![7..=10, 1..=2, 5..=8, 11..=11]);
        assert_eq!(set, RangeSet::new(vec![1..=2, 5..=11]));
    }

    #[test]
    fn end_is_the_largest_value() {
        assert_eq!(RangeSet::new(vec![1..=2, 5..=6]).end(), 6);
        assert_eq!(RangeSet::full().end(), usize::MAX);
    }

    #[test]
    fn full_contains_everything() {
        let set = RangeSet::full();
        assert!(set.contains(1));
        assert!(set.contains(usize::MAX));
    }
}
