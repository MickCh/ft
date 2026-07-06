//! Column addressing: a column range counts characters by default, or
//! delimited fields in field mode (`--fields`). Either way a span
//! resolves to a 1-based, inclusive char range on a concrete line, so
//! the helpers in [`crate::text`] stay purely char-indexed.

use std::ops::RangeInclusive;

/// How a column range addresses a line: by character positions, or by
/// fields separated by a delimiter.
#[derive(Debug, Clone)]
pub enum ColumnSpan {
    Chars(RangeInclusive<usize>),
    Fields {
        delimiter: String,
        fields: RangeInclusive<usize>,
    },
}

impl ColumnSpan {
    /// The char range this span covers on `line`. A span lying beyond
    /// the line resolves to a range starting past its last character,
    /// which the char-indexed helpers treat as out of bounds.
    pub fn char_range(&self, line: &str) -> RangeInclusive<usize> {
        match self {
            ColumnSpan::Chars(range) => range.clone(),
            ColumnSpan::Fields { delimiter, fields } => {
                resolve_fields(line, delimiter, fields, false)
            }
        }
    }

    /// Like [`ColumnSpan::char_range`], but in field mode the range
    /// swallows one delimiter adjacent to the fields (like `cut`), so
    /// deleting fields does not leave a dangling delimiter behind.
    pub fn char_range_for_delete(&self, line: &str) -> RangeInclusive<usize> {
        match self {
            ColumnSpan::Chars(range) => range.clone(),
            ColumnSpan::Fields { delimiter, fields } => {
                resolve_fields(line, delimiter, fields, true)
            }
        }
    }
}

impl From<RangeInclusive<usize>> for ColumnSpan {
    fn from(range: RangeInclusive<usize>) -> ColumnSpan {
        ColumnSpan::Chars(range)
    }
}

/// Map a 1-based field range onto the char range those fields occupy.
fn resolve_fields(
    line: &str,
    delimiter: &str,
    fields: &RangeInclusive<usize>,
    swallow_delimiter: bool,
) -> RangeInclusive<usize> {
    let positions = field_positions(line, delimiter);
    let first = (*fields.start()).max(1);
    if first > positions.len() {
        let beyond = line.chars().count() + 1;
        return beyond..=beyond;
    }
    let last = (*fields.end()).min(positions.len());

    let (mut start, _) = positions[first - 1];
    let (_, mut end) = positions[last - 1];

    if swallow_delimiter {
        let delimiter_len = delimiter.chars().count();
        if last < positions.len() {
            end += delimiter_len;
        } else if first > 1 {
            start -= delimiter_len;
        }
    }

    start..=end
}

/// 1-based `(start, end)` char positions of every delimited field;
/// an empty field has `end = start - 1`.
fn field_positions(line: &str, delimiter: &str) -> Vec<(usize, usize)> {
    let delimiter_len = delimiter.chars().count();
    let mut start = 1usize;
    line.split(delimiter)
        .map(|field| {
            let len = field.chars().count();
            let position = (start, start + len - 1);
            start += len + delimiter_len;
            position
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(delimiter: &str, fields: RangeInclusive<usize>) -> ColumnSpan {
        ColumnSpan::Fields {
            delimiter: delimiter.to_owned(),
            fields,
        }
    }

    #[test]
    fn char_span_returns_the_range_unchanged() {
        let span = ColumnSpan::from(2..=5);
        assert_eq!(span.char_range("whatever"), 2..=5);
        assert_eq!(span.char_range_for_delete("whatever"), 2..=5);
    }

    #[test]
    fn field_span_maps_fields_to_char_positions() {
        //pos: "aa,b,ccc" -> aa=1-2, b=4, ccc=6-8
        assert_eq!(fields(",", 2..=2).char_range("aa,b,ccc"), 4..=4);
        assert_eq!(fields(",", 2..=3).char_range("aa,b,ccc"), 4..=8);
        assert_eq!(fields(",", 1..=1).char_range("aa,b,ccc"), 1..=2);
    }

    #[test]
    fn field_span_clamps_open_ended_ranges() {
        assert_eq!(fields(",", 2..=usize::MAX).char_range("a,b,c"), 3..=5);
    }

    #[test]
    fn field_span_beyond_the_line_resolves_past_it() {
        //"a,b" has 3 chars, so the span starts at 4
        assert_eq!(fields(",", 5..=6).char_range("a,b"), 4..=4);
    }

    #[test]
    fn field_span_of_an_empty_field_is_inverted() {
        //field 2 of "a,,c" is empty: start 3, end 2 — an inverted
        //range, which the text helpers treat as nothing selected
        //(`3..=2` as a literal trips clippy::reversed_empty_ranges)
        assert_eq!(
            fields(",", 2..=2).char_range("a,,c"),
            RangeInclusive::new(3, 2)
        );
    }

    #[test]
    fn field_span_counts_unicode_chars() {
        //pos: "łą,śż" -> łą=1-2, śż=4-5
        assert_eq!(fields(",", 2..=2).char_range("łą,śż"), 4..=5);
    }

    #[test]
    fn field_span_supports_multi_char_delimiters() {
        //pos: "a::b::c" -> a=1, b=4, c=7
        assert_eq!(fields("::", 2..=2).char_range("a::b::c"), 4..=4);
        assert_eq!(fields("::", 2..=2).char_range_for_delete("a::b::c"), 4..=6);
    }

    #[test]
    fn delete_span_swallows_the_trailing_delimiter() {
        assert_eq!(fields(",", 2..=2).char_range_for_delete("a,b,c"), 3..=4);
        assert_eq!(fields(",", 1..=1).char_range_for_delete("a,b,c"), 1..=2);
    }

    #[test]
    fn delete_span_of_the_last_fields_swallows_the_leading_delimiter() {
        assert_eq!(fields(",", 3..=3).char_range_for_delete("a,b,c"), 4..=5);
        assert_eq!(fields(",", 2..=3).char_range_for_delete("a,b,c"), 2..=5);
    }

    #[test]
    fn delete_span_of_the_only_field_swallows_nothing() {
        assert_eq!(fields(",", 1..=1).char_range_for_delete("abc"), 1..=3);
    }
}
