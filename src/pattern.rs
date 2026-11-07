//! Regular expressions behind one type, in either of two engines. The
//! default is the `regex` crate: matching in time linear in the input,
//! whatever the pattern. `-P` (`--perl-regexp`) switches to `fancy-regex`,
//! which adds what only backtracking can match — backreferences (`(.)\1`)
//! and lookaround (`(?=…)`, `(?<!…)`) — at the cost of that guarantee: a
//! pathological pattern may backtrack until it gives up, which is a
//! runtime error here, never a silent "no match".

use std::borrow::Cow;
use std::io;

use fancy_regex::BytesMode;

/// Which engine compiles the patterns.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Syntax {
    /// `regex`: linear time, no backreferences or lookaround.
    #[default]
    Linear,
    /// `fancy-regex`: backreferences and lookaround, by backtracking.
    Backtracking,
}

/// A pattern matched against text.
#[derive(Debug, Clone)]
pub enum Pattern {
    Linear(regex::Regex),
    Backtracking(fancy_regex::Regex),
}

impl Pattern {
    /// Compile `source`; the error is the engine's own message.
    pub fn new(source: &str, ignore_case: bool, syntax: Syntax) -> Result<Pattern, String> {
        match syntax {
            Syntax::Linear => regex::RegexBuilder::new(source)
                .case_insensitive(ignore_case)
                .build()
                .map(Pattern::Linear)
                .map_err(|error| error.to_string()),
            Syntax::Backtracking => fancy_regex::RegexBuilder::new(source)
                .case_insensitive(ignore_case)
                .build()
                .map(Pattern::Backtracking)
                .map_err(|error| error.to_string()),
        }
    }

    pub fn is_match(&self, text: &str) -> io::Result<bool> {
        match self {
            Pattern::Linear(pattern) => Ok(pattern.is_match(text)),
            Pattern::Backtracking(pattern) => pattern
                .is_match(text)
                .map_err(|error| runtime_error(pattern.as_str(), error)),
        }
    }

    /// Replace every match in `text`; `$1` and `${name}` in `replacement`
    /// refer to capture groups, in either engine. Borrows when nothing
    /// matched.
    pub fn replace_all<'t>(&self, text: &'t str, replacement: &str) -> io::Result<Cow<'t, str>> {
        match self {
            Pattern::Linear(pattern) => Ok(pattern.replace_all(text, replacement)),
            Pattern::Backtracking(pattern) => pattern
                .try_replacen(text, 0, replacement)
                .map_err(|error| runtime_error(pattern.as_str(), error)),
        }
    }
}

/// A pattern matched against raw bytes, which need not be UTF-8
/// (`--from`/`--to`, so rows outside a block are never decoded).
#[derive(Debug, Clone)]
pub enum BytePattern {
    Linear(regex::bytes::Regex),
    Backtracking(fancy_regex::Regex),
}

impl BytePattern {
    /// Compile `source`; the error is the engine's own message.
    pub fn new(source: &str, ignore_case: bool, syntax: Syntax) -> Result<BytePattern, String> {
        match syntax {
            Syntax::Linear => regex::bytes::RegexBuilder::new(source)
                .case_insensitive(ignore_case)
                .build()
                .map(BytePattern::Linear)
                .map_err(|error| error.to_string()),
            //Unicode-aware like the text patterns, but on bytes that may
            //not be UTF-8 — what `regex::bytes` does by default
            Syntax::Backtracking => fancy_regex::RegexBuilder::new(source)
                .case_insensitive(ignore_case)
                .bytes_mode(BytesMode::UnicodeBytes)
                .build()
                .map(BytePattern::Backtracking)
                .map_err(|error| error.to_string()),
        }
    }

    pub fn is_match(&self, bytes: &[u8]) -> io::Result<bool> {
        match self {
            BytePattern::Linear(pattern) => Ok(pattern.is_match(bytes)),
            BytePattern::Backtracking(pattern) => pattern
                .is_match(bytes)
                .map_err(|error| runtime_error(pattern.as_str(), error)),
        }
    }
}

/// A pattern that failed while matching (backtracking ran out), as an
/// error the run reports and fails with.
fn runtime_error(source: &str, error: fancy_regex::Error) -> io::Error {
    io::Error::other(format!("matching the pattern `{source}` failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pattern(source: &str, syntax: Syntax) -> Pattern {
        Pattern::new(source, false, syntax).unwrap()
    }

    #[test]
    fn backreferences_need_the_backtracking_engine() {
        // Given / When
        let linear = Pattern::new(r"(.)\1", false, Syntax::Linear);
        let backtracking = pattern(r"(.)\1", Syntax::Backtracking);

        // Then
        assert!(
            linear
                .unwrap_err()
                .contains("backreferences are not supported")
        );
        assert!(backtracking.is_match("book").unwrap());
        assert!(!backtracking.is_match("bok").unwrap());
    }

    #[test]
    fn lookaround_matches_without_consuming() {
        // Given a lookbehind and a negative lookahead
        let price = pattern(r"(?<=\$)\d+(?!\d*%)", Syntax::Backtracking);

        // When / Then
        assert_eq!(
            price
                .replace_all("$15 and 15% and $7", "N")
                .unwrap(),
            "$N and 15% and $N"
        );
    }

    #[test]
    fn both_engines_expand_capture_groups_alike() {
        // Given / When / Then
        for syntax in [Syntax::Linear, Syntax::Backtracking] {
            let swap = pattern(r"(\w+)@(?P<host>\w+)", syntax);
            assert_eq!(
                swap.replace_all("me@home", "${host}:$1")
                    .unwrap(),
                "home:me"
            );
        }
    }

    #[test]
    fn nothing_to_replace_borrows() {
        // Given / When
        let replaced = pattern(r"(x)\1", Syntax::Backtracking)
            .replace_all("abc", "y")
            .unwrap();

        // Then
        assert!(matches!(replaced, Cow::Borrowed("abc")));
    }

    #[test]
    fn ignore_case_applies_to_backtracking_too() {
        // Given / When
        let doubled = Pattern::new(r"(a)\1", true, Syntax::Backtracking).unwrap();

        // Then
        assert!(doubled.is_match("xAAx").unwrap());
    }

    #[test]
    fn byte_patterns_match_input_that_is_not_utf8() {
        // Given / When
        let doubled = BytePattern::new(r"^(\w)\1", false, Syntax::Backtracking).unwrap();

        // Then
        assert!(doubled.is_match(b"zz\xff").unwrap());
        assert!(!doubled.is_match(b"\xff\xff").unwrap());
    }

    #[test]
    fn runaway_backtracking_is_an_error_not_a_miss() {
        // Given a pattern that backtracks exponentially without a match
        let runaway = fancy_regex::RegexBuilder::new(r"^(a|aa)+\1$")
            .backtrack_limit(1_000)
            .build()
            .unwrap();

        // When
        let error = Pattern::Backtracking(runaway)
            .is_match(&format!("{}b", "a".repeat(40)))
            .unwrap_err();

        // Then
        assert!(error.to_string().contains("failed"), "{error}");
    }
}
