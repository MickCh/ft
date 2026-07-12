//! Column addressing: a column list counts characters by default, or
//! delimited fields in field mode (`--fields`). Either way a span
//! resolves to the 1-based, inclusive char ranges it occupies on a
//! concrete line, so the helpers in [`crate::text`] stay purely
//! char-indexed.
//!
//! A list may name several parts (`1,3,5-7`). Reading operations
//! (selecting columns, sort/unique keys, `--grep`) honour the order as
//! written, so `3,1,2` permutes; operations that write into the line
//! (delete, case/trim, find/replace) work on the normalized set, where
//! the parts are sorted and merged and order carries no meaning.

use std::borrow::Cow;
use std::ops::RangeInclusive;

use crate::ranges::RangeSet;

/// Column parts as written on the command line, together with their
/// normalized form. Both are needed: reading honours the written order,
/// writing needs sorted, non-overlapping parts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnList {
    written: Vec<RangeInclusive<usize>>,
    normalized: RangeSet,
}

impl ColumnList {
    pub fn new(written: Vec<RangeInclusive<usize>>) -> ColumnList {
        let normalized = RangeSet::new(written.clone());
        ColumnList {
            written,
            normalized,
        }
    }

    /// The list covering every column.
    pub fn full() -> ColumnList {
        ColumnList::from(1..=usize::MAX)
    }

    /// The parts in the order written; a part may repeat or overlap.
    pub fn written(&self) -> &[RangeInclusive<usize>] {
        &self.written
    }

    /// The same columns as a normalized set: ascending, merged, disjoint.
    pub fn normalized(&self) -> &[RangeInclusive<usize>] {
        self.normalized.parts()
    }
}

impl From<RangeInclusive<usize>> for ColumnList {
    fn from(range: RangeInclusive<usize>) -> ColumnList {
        ColumnList::new(vec![range])
    }
}

/// How a column list addresses a line: by character positions, or by
/// fields separated by a delimiter.
#[derive(Debug, Clone)]
pub enum ColumnSpan {
    Chars(ColumnList),
    Fields {
        delimiter: String,
        /// What joins the selected fields on output; `None` reuses the
        /// input delimiter (`--output-delimiter`).
        output_delimiter: Option<String>,
        fields: ColumnList,
    },
}

impl ColumnSpan {
    /// Field mode with the input delimiter reused on output.
    pub fn fields(delimiter: impl Into<String>, fields: ColumnList) -> ColumnSpan {
        ColumnSpan::Fields {
            delimiter: delimiter.into(),
            output_delimiter: None,
            fields,
        }
    }

    /// The char ranges to read, in the order written, so a permuted list
    /// (`3,1,2`) reads the parts in that order. Field parts that lie past
    /// the last field of `line` are dropped: they address nothing, and
    /// keeping them would join a stray delimiter into the result.
    pub fn read_ranges(&self, line: &str) -> Cow<'_, [RangeInclusive<usize>]> {
        match self {
            ColumnSpan::Chars(list) => Cow::Borrowed(list.written()),
            ColumnSpan::Fields {
                delimiter, fields, ..
            } => Cow::Owned(resolve_all(line, delimiter, fields.written(), false)),
        }
    }

    /// The char ranges to write into: normalized, so they are ascending
    /// and never overlap and the line can be rebuilt in one pass.
    pub fn write_ranges(&self, line: &str) -> Cow<'_, [RangeInclusive<usize>]> {
        match self {
            ColumnSpan::Chars(list) => Cow::Borrowed(list.normalized()),
            ColumnSpan::Fields {
                delimiter, fields, ..
            } => Cow::Owned(resolve_all(line, delimiter, fields.normalized(), false)),
        }
    }

    /// Like [`ColumnSpan::write_ranges`], but in field mode each part
    /// swallows one adjacent delimiter (like `cut`), so deleting fields
    /// does not leave a dangling delimiter behind. Normalizing first is
    /// what makes `-C 2,3` delete the same columns as `-C 2-3`.
    pub fn delete_ranges(&self, line: &str) -> Cow<'_, [RangeInclusive<usize>]> {
        match self {
            ColumnSpan::Chars(list) => Cow::Borrowed(list.normalized()),
            ColumnSpan::Fields {
                delimiter, fields, ..
            } => Cow::Owned(resolve_all(line, delimiter, fields.normalized(), true)),
        }
    }

    /// What joins the parts of a selection: the output delimiter in
    /// field mode (the input delimiter unless overridden), nothing in
    /// char mode, where columns are adjacent by definition.
    pub fn joiner(&self) -> &str {
        match self {
            ColumnSpan::Chars(_) => "",
            ColumnSpan::Fields {
                delimiter,
                output_delimiter,
                ..
            } => output_delimiter
                .as_deref()
                .unwrap_or(delimiter),
        }
    }
}

impl From<RangeInclusive<usize>> for ColumnSpan {
    fn from(range: RangeInclusive<usize>) -> ColumnSpan {
        ColumnSpan::Chars(ColumnList::from(range))
    }
}

impl From<ColumnList> for ColumnSpan {
    fn from(list: ColumnList) -> ColumnSpan {
        ColumnSpan::Chars(list)
    }
}

/// Resolve every field part to the char range it occupies, dropping the
/// parts that address no field at all.
fn resolve_all(
    line: &str,
    delimiter: &str,
    parts: &[RangeInclusive<usize>],
    swallow_delimiter: bool,
) -> Vec<RangeInclusive<usize>> {
    parts
        .iter()
        .filter_map(|part| resolve_fields(line, delimiter, part, swallow_delimiter))
        .collect()
}

/// Map a 1-based field range onto the char range those fields occupy.
/// `None` means the range starts past the last field, so it covers no
/// characters at all.
///
/// Walks the fields once, tracking only the start of the first selected
/// field, the end of the last, and the total field count — enough to
/// resolve the span without materializing every field's position.
fn resolve_fields(
    line: &str,
    delimiter: &str,
    fields: &RangeInclusive<usize>,
    swallow_delimiter: bool,
) -> Option<RangeInclusive<usize>> {
    let delimiter_len = delimiter.chars().count();
    let first = (*fields.start()).max(1);
    let wanted_last = *fields.end();

    //1-based char position where the current field starts
    let mut field_start = 1usize;
    let mut span_start = None;
    let mut span_end = 0usize;
    let mut count = 0usize;

    for (index, field) in line.split(delimiter).enumerate() {
        let field_index = index + 1;
        let len = field.chars().count();
        //an empty field has its end one before its start
        let field_end = field_start + len - 1;

        if field_index == first {
            span_start = Some(field_start);
        }
        if (first..=wanted_last).contains(&field_index) {
            span_end = field_end;
        }

        field_start += len + delimiter_len;
        count = field_index;
    }

    //the first wanted field lies past the last one: nothing to address
    let mut start = span_start?;
    let mut end = span_end;

    if swallow_delimiter {
        //take one bordering delimiter with the fields, like `cut`:
        //the trailing one when a field follows, else the leading one
        if wanted_last < count {
            end += delimiter_len;
        } else if first > 1 {
            start -= delimiter_len;
        }
    }

    Some(start..=end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(parts: &[RangeInclusive<usize>]) -> ColumnList {
        ColumnList::new(parts.to_vec())
    }

    fn fields(delimiter: &str, parts: &[RangeInclusive<usize>]) -> ColumnSpan {
        ColumnSpan::fields(delimiter, list(parts))
    }

    fn chars(parts: &[RangeInclusive<usize>]) -> ColumnSpan {
        ColumnSpan::Chars(list(parts))
    }

    #[test]
    fn column_list_keeps_the_written_order_and_normalizes_separately() {
        let list = list(&[3..=4, 1..=1, 2..=2]);

        assert_eq!(list.written(), [3..=4, 1..=1, 2..=2]);
        //1, 2 and 3-4 are adjacent, so the normalized set merges them
        assert_eq!(list.normalized(), [1..=4]);
    }

    #[test]
    fn char_span_reads_the_parts_in_order() {
        let span = chars(&[3..=4, 1..=2]);
        assert_eq!(span.read_ranges("abcd").as_ref(), [3..=4, 1..=2]);
        //writing normalizes, so the order carries no meaning
        assert_eq!(span.write_ranges("abcd").as_ref(), [1..=4]);
        assert_eq!(span.joiner(), "");
    }

    #[test]
    fn char_span_returns_the_range_unchanged() {
        let span = ColumnSpan::from(2..=5);
        assert_eq!(span.read_ranges("whatever").as_ref(), [2..=5]);
        assert_eq!(span.delete_ranges("whatever").as_ref(), [2..=5]);
    }

    #[test]
    fn field_span_maps_fields_to_char_positions() {
        //pos: "aa,b,ccc" -> aa=1-2, b=4, ccc=6-8
        assert_eq!(
            fields(",", &[2..=2])
                .read_ranges("aa,b,ccc")
                .as_ref(),
            [4..=4]
        );
        assert_eq!(
            fields(",", &[2..=3])
                .read_ranges("aa,b,ccc")
                .as_ref(),
            [4..=8]
        );
        assert_eq!(
            fields(",", &[1..=1])
                .read_ranges("aa,b,ccc")
                .as_ref(),
            [1..=2]
        );
    }

    #[test]
    fn field_span_reads_parts_in_the_written_order() {
        //pos: "a,bb,c" -> a=1, bb=3-4, c=6
        let span = fields(",", &[3..=3, 1..=1]);
        assert_eq!(span.read_ranges("a,bb,c").as_ref(), [6..=6, 1..=1]);
        assert_eq!(span.joiner(), ",");
    }

    #[test]
    fn field_span_joins_on_the_output_delimiter_when_given() {
        let span = ColumnSpan::Fields {
            delimiter: ",".to_owned(),
            output_delimiter: Some(";".to_owned()),
            fields: list(&[1..=2]),
        };
        assert_eq!(span.joiner(), ";");
    }

    #[test]
    fn field_span_clamps_open_ended_ranges() {
        assert_eq!(
            fields(",", &[2..=usize::MAX])
                .read_ranges("a,b,c")
                .as_ref(),
            [3..=5]
        );
    }

    #[test]
    fn field_span_beyond_the_line_drops_the_part() {
        //"a,b" has two fields, so field 5 addresses nothing at all
        assert!(
            fields(",", &[5..=6])
                .read_ranges("a,b")
                .is_empty()
        );
        //the parts that do exist survive
        assert_eq!(
            fields(",", &[1..=1, 5..=6])
                .read_ranges("a,b")
                .as_ref(),
            [1..=1]
        );
    }

    #[test]
    fn field_span_of_an_empty_field_is_inverted() {
        //field 2 of "a,,c" is empty: start 3, end 2 — an inverted
        //range, which the text helpers treat as nothing selected
        //(`3..=2` as a literal trips clippy::reversed_empty_ranges)
        assert_eq!(
            fields(",", &[2..=2])
                .read_ranges("a,,c")
                .as_ref(),
            [RangeInclusive::new(3, 2)]
        );
    }

    #[test]
    fn field_span_counts_unicode_chars() {
        //pos: "łą,śż" -> łą=1-2, śż=4-5
        assert_eq!(
            fields(",", &[2..=2])
                .read_ranges("łą,śż")
                .as_ref(),
            [4..=5]
        );
    }

    #[test]
    fn field_span_supports_multi_char_delimiters() {
        //pos: "a::b::c" -> a=1, b=4, c=7
        assert_eq!(
            fields("::", &[2..=2])
                .read_ranges("a::b::c")
                .as_ref(),
            [4..=4]
        );
        assert_eq!(
            fields("::", &[2..=2])
                .delete_ranges("a::b::c")
                .as_ref(),
            [4..=6]
        );
    }

    #[test]
    fn delete_span_swallows_the_trailing_delimiter() {
        assert_eq!(
            fields(",", &[2..=2])
                .delete_ranges("a,b,c")
                .as_ref(),
            [3..=4]
        );
        assert_eq!(
            fields(",", &[1..=1])
                .delete_ranges("a,b,c")
                .as_ref(),
            [1..=2]
        );
    }

    #[test]
    fn delete_span_of_the_last_fields_swallows_the_leading_delimiter() {
        assert_eq!(
            fields(",", &[3..=3])
                .delete_ranges("a,b,c")
                .as_ref(),
            [4..=5]
        );
        assert_eq!(
            fields(",", &[2..=3])
                .delete_ranges("a,b,c")
                .as_ref(),
            [2..=5]
        );
    }

    #[test]
    fn delete_span_of_the_only_field_swallows_nothing() {
        assert_eq!(
            fields(",", &[1..=1])
                .delete_ranges("abc")
                .as_ref(),
            [1..=3]
        );
    }

    #[test]
    fn delete_span_normalizes_a_list_before_swallowing() {
        //fields 2 and 3 are adjacent: normalized to 2-3, they swallow one
        //delimiter between them, exactly like the range `2-3` would
        assert_eq!(
            fields(",", &[2..=2, 3..=3])
                .delete_ranges("a,b,c")
                .as_ref(),
            [2..=5]
        );
        //disjoint parts each swallow their own delimiter
        assert_eq!(
            fields(",", &[1..=1, 3..=3])
                .delete_ranges("a,b,c")
                .as_ref(),
            [1..=2, 4..=5]
        );
    }
}
