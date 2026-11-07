//! Rows addressed by content rather than by number: `--from /START/`
//! `--to /END/`, the `sed '/START/,/END/'` range.
//!
//! A numeric range resolves up front (see [`crate::ranges`]); this one
//! cannot, since where it starts and ends depends on lines not yet read.
//! It is resolved while streaming instead: [`PatternRange::admits`] is
//! told every line in order and answers for that line, carrying its
//! state — inside a block or not — from one to the next.

use std::io;

use crate::pattern::BytePattern;

/// A row range bounded by patterns. Like `sed`, it is inclusive at both
/// ends, the end is looked for from the line *after* the start (so one
/// line matching both opens a block rather than being one), and once a
/// block ends the next start opens another: every block is selected.
///
/// Without a start it begins at the first line and the end may match
/// that very line (`sed 0,/END/`); it is then one block, up to the first
/// end. Without an end a block runs to the end of the input.
///
/// Lines are matched as bytes, without their terminator, so a line
/// outside every block is never required to be valid UTF-8 — it passes
/// through untouched, as it would outside a numeric range.
#[derive(Debug)]
pub struct PatternRange {
    from: Option<BytePattern>,
    to: Option<BytePattern>,
    inside: bool,
    /// Whether the implicit block of a range without a start has been
    /// opened: it is opened once, at the first line.
    opened_at_start: bool,
}

impl PatternRange {
    pub fn new(from: Option<BytePattern>, to: Option<BytePattern>) -> PatternRange {
        PatternRange {
            from,
            to,
            inside: false,
            opened_at_start: false,
        }
    }

    /// Whether `line` (its content, without the terminator) lies in a
    /// block. Must be told every line of the input, in order — also the
    /// ones another filter will reject — since each one may open or
    /// close a block.
    pub fn admits(&mut self, line: &[u8]) -> io::Result<bool> {
        if self.inside {
            if self.ends_at(line)? {
                self.inside = false;
            }
            return Ok(true);
        }

        match &self.from {
            Some(start) if start.is_match(line)? => {
                //the end is only looked for from the next line on
                self.inside = true;
                Ok(true)
            }
            Some(_) => Ok(false),
            None if !self.opened_at_start => {
                self.opened_at_start = true;
                //no start pattern, so nothing else could claim this line:
                //the end may close the block right here
                self.inside = !self.ends_at(line)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    fn ends_at(&self, line: &[u8]) -> io::Result<bool> {
        match &self.to {
            Some(end) => end.is_match(line),
            None => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::Syntax;

    fn range(from: Option<&str>, to: Option<&str>) -> PatternRange {
        let compile = |pattern: &str| BytePattern::new(pattern, false, Syntax::Linear).unwrap();
        PatternRange::new(from.map(compile), to.map(compile))
    }

    /// The lines of `input` the range admits.
    fn admitted<'a>(range: &mut PatternRange, input: &[&'a str]) -> Vec<&'a str> {
        input
            .iter()
            .copied()
            .filter(|line| range.admits(line.as_bytes()).unwrap())
            .collect()
    }

    #[test]
    fn a_block_runs_from_start_to_end_inclusive() {
        // Given / When / Then
        let lines = ["a", "START", "b", "END", "c"];
        assert_eq!(
            admitted(&mut range(Some("START"), Some("END")), &lines),
            ["START", "b", "END"]
        );
    }

    #[test]
    fn every_block_is_selected() {
        // Given two blocks
        let lines = ["x", "[a]", "1", "[/]", "y", "[b]", "2", "[/]", "z"];

        // When / Then both come out, like sed
        assert_eq!(
            admitted(&mut range(Some(r"^\[[a-z]"), Some(r"^\[/")), &lines),
            ["[a]", "1", "[/]", "[b]", "2", "[/]"]
        );
    }

    #[test]
    fn the_end_is_looked_for_after_the_start_line() {
        // Given section headers, where a header is both start and end
        let lines = ["intro", "[one]", "a", "[two]", "b"];

        // When / Then the start line does not close its own block, as in
        // sed; the next header does, and is part of the block
        assert_eq!(
            admitted(&mut range(Some(r"^\[one"), Some(r"^\[")), &lines),
            ["[one]", "a", "[two]"]
        );
    }

    #[test]
    fn without_an_end_a_block_runs_to_the_end_of_the_input() {
        let lines = ["a", "START", "b", "c"];
        assert_eq!(
            admitted(&mut range(Some("START"), None), &lines),
            ["START", "b", "c"]
        );
    }

    #[test]
    fn without_a_start_the_range_runs_from_the_first_line_once() {
        // Given / When / Then up to the first end, and not again
        let lines = ["a", "END", "b", "END", "c"];
        assert_eq!(
            admitted(&mut range(None, Some("END")), &lines),
            ["a", "END"]
        );
    }

    #[test]
    fn without_a_start_the_end_may_be_the_first_line() {
        // Given / When / Then like `sed 0,/END/`
        let lines = ["END", "a", "END"];
        assert_eq!(admitted(&mut range(None, Some("END")), &lines), ["END"]);
    }

    #[test]
    fn a_start_that_never_matches_selects_nothing() {
        let lines = ["a", "b"];
        assert!(admitted(&mut range(Some("START"), Some("END")), &lines).is_empty());
    }

    #[test]
    fn lines_are_matched_as_bytes() {
        // Given a line that is not valid UTF-8 outside the block
        let mut range = range(Some("START"), None);

        // When / Then it is matched without complaint, and not admitted
        assert!(!range.admits(b"\xff\xfe").unwrap());
        assert!(range.admits(b"START").unwrap());
    }
}
