//! A set of 1-based, inclusive ranges, possibly non-contiguous
//! (e.g. rows `1-5,10-20`). A [`RangeSpec`] may contain bounds counted
//! from the end of the input (`~N`); it resolves into an absolute
//! [`RangeSet`] once the total number of lines is known.

use std::ops::RangeInclusive;

/// One end of a range: an absolute 1-based position, or a position
/// counted from the end of the input (`~1` is the last line).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeBound {
    FromStart(usize),
    FromEnd(usize),
}

impl RangeBound {
    /// The absolute position, given the total number of lines.
    /// A `FromEnd` bound pointing before the first line yields 0.
    fn resolve(self, total: usize) -> usize {
        match self {
            RangeBound::FromStart(value) => value,
            RangeBound::FromEnd(value) => (total + 1).saturating_sub(value),
        }
    }
}

/// Row ranges as written on the command line, possibly end-relative.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeSpec {
    parts: Vec<(RangeBound, RangeBound)>,
}

impl RangeSpec {
    pub fn new(parts: Vec<(RangeBound, RangeBound)>) -> RangeSpec {
        RangeSpec { parts }
    }

    /// The spec covering everything (row 1 onwards).
    pub fn full() -> RangeSpec {
        RangeSpec::from(1..=usize::MAX)
    }

    /// Whether no bound is counted from the end — if so, the spec can
    /// be resolved without knowing the total number of lines.
    pub fn is_absolute(&self) -> bool {
        self.parts.iter().all(|(from, to)| {
            matches!(from, RangeBound::FromStart(_)) && matches!(to, RangeBound::FromStart(_))
        })
    }

    /// Resolve every bound against the total number of lines. Parts
    /// that fall entirely before the first line disappear.
    pub fn resolve(&self, total: usize) -> RangeSet {
        let parts = self
            .parts
            .iter()
            .filter_map(|(from, to)| {
                let from = from.resolve(total).max(1);
                let to = to.resolve(total);
                (from <= to).then_some(from..=to)
            })
            .collect();
        RangeSet::new(parts)
    }
}

impl From<RangeInclusive<usize>> for RangeSpec {
    fn from(range: RangeInclusive<usize>) -> RangeSpec {
        RangeSpec {
            parts: vec![(
                RangeBound::FromStart(*range.start()),
                RangeBound::FromStart(*range.end()),
            )],
        }
    }
}

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

    /// The normalized parts, ascending and non-overlapping.
    pub fn parts(&self) -> &[RangeInclusive<usize>] {
        &self.parts
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
    fn full_contains_everything() {
        let set = RangeSet::full();
        assert!(set.contains(1));
        assert!(set.contains(usize::MAX));
    }

    #[test]
    fn spec_from_absolute_range_is_absolute() {
        assert!(RangeSpec::from(2..=4).is_absolute());
        assert!(RangeSpec::full().is_absolute());

        let relative = RangeSpec::new(vec![(RangeBound::FromEnd(2), RangeBound::FromEnd(1))]);
        assert!(!relative.is_absolute());
    }

    #[test]
    fn resolve_maps_end_relative_bounds_to_absolute_rows() {
        //~2-~1 on a 5-line input means rows 4-5
        let spec = RangeSpec::new(vec![(RangeBound::FromEnd(2), RangeBound::FromEnd(1))]);
        assert_eq!(spec.resolve(5), RangeSet::new(vec![4..=5]));
    }

    #[test]
    fn resolve_supports_mixed_bounds() {
        //2-~2 on a 5-line input means rows 2-4
        let spec = RangeSpec::new(vec![(RangeBound::FromStart(2), RangeBound::FromEnd(2))]);
        assert_eq!(spec.resolve(5), RangeSet::new(vec![2..=4]));
    }

    #[test]
    fn resolve_clamps_a_start_reaching_before_the_input() {
        //the last 10 lines of a 3-line input are all of it
        let spec = RangeSpec::new(vec![(RangeBound::FromEnd(10), RangeBound::FromEnd(1))]);
        assert_eq!(spec.resolve(3), RangeSet::new(vec![1..=3]));
    }

    #[test]
    fn resolve_drops_parts_that_become_empty() {
        //5-~3 on a 5-line input would be rows 5-3: empty
        let spec = RangeSpec::new(vec![(RangeBound::FromStart(5), RangeBound::FromEnd(3))]);
        assert_eq!(spec.resolve(5), RangeSet::new(vec![]));
    }
}
