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
use crate::text;

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

/// Field mode: which delimited fields to address, and how they are
/// delimited on the way in and on the way out.
#[derive(Debug, Clone)]
pub struct FieldSpan {
    pub delimiter: String,
    /// What joins the selected fields on output; `None` reuses the
    /// input delimiter (`--output-delimiter`).
    pub output_delimiter: Option<String>,
    /// Whether a delimiter inside a `"quoted"` field splits it
    /// (`--quoted`, i.e. CSV rather than a plain split).
    pub quoted: bool,
    pub fields: ColumnList,
}

impl FieldSpan {
    /// Field mode with the input delimiter reused on output and no
    /// quoting, which is what a plain `--fields` asks for.
    pub fn new(delimiter: impl Into<String>, fields: ColumnList) -> FieldSpan {
        FieldSpan {
            delimiter: delimiter.into(),
            output_delimiter: None,
            quoted: false,
            fields,
        }
    }
}

/// How a column list addresses a line: by character positions, or by
/// delimited fields.
#[derive(Debug, Clone)]
pub enum ColumnSpan {
    Chars(ColumnList),
    Fields(FieldSpan),
}

impl ColumnSpan {
    /// Field mode with the input delimiter reused on output.
    pub fn fields(delimiter: impl Into<String>, fields: ColumnList) -> ColumnSpan {
        ColumnSpan::Fields(FieldSpan::new(delimiter, fields))
    }

    /// The char ranges to read, in the order written, so a permuted list
    /// (`3,1,2`) reads the parts in that order. Field parts that lie past
    /// the last field of `line` are dropped: they address nothing, and
    /// keeping them would join a stray delimiter into the result.
    pub fn read_ranges(&self, line: &str) -> Cow<'_, [RangeInclusive<usize>]> {
        match self {
            ColumnSpan::Chars(list) => Cow::Borrowed(list.written()),
            ColumnSpan::Fields(spec) => {
                Cow::Owned(resolve_fields(line, spec, spec.fields.written(), false))
            }
        }
    }

    /// The char ranges to write into: normalized, so they are ascending
    /// and never overlap and the line can be rebuilt in one pass.
    pub fn write_ranges(&self, line: &str) -> Cow<'_, [RangeInclusive<usize>]> {
        match self {
            ColumnSpan::Chars(list) => Cow::Borrowed(list.normalized()),
            ColumnSpan::Fields(spec) => {
                Cow::Owned(resolve_fields(line, spec, spec.fields.normalized(), false))
            }
        }
    }

    /// Like [`ColumnSpan::write_ranges`], but in field mode each part
    /// swallows one adjacent delimiter (like `cut`), so deleting fields
    /// does not leave a dangling delimiter behind. Normalizing first is
    /// what makes `-C 2,3` delete the same columns as `-C 2-3`.
    pub fn delete_ranges(&self, line: &str) -> Cow<'_, [RangeInclusive<usize>]> {
        match self {
            ColumnSpan::Chars(list) => Cow::Borrowed(list.normalized()),
            ColumnSpan::Fields(spec) => {
                Cow::Owned(resolve_fields(line, spec, spec.fields.normalized(), true))
            }
        }
    }

    /// The content this span selects from the line: its parts read in
    /// the order written and joined. A span reaching past the line
    /// selects nothing, like `cut` — which is what makes it a usable
    /// sort, unique or grouping key on a short line.
    pub fn select<'a>(&self, line: &'a str) -> Cow<'a, str> {
        text::select_ranges(line, &self.read_ranges(line), self.joiner())
    }

    /// What the selection *means* rather than the text it occupies: under
    /// `--quoted` each field loses its surrounding quotes and a doubled
    /// quote becomes one, so `"10"` reads as the number 10. Lexical
    /// operations keep using [`ColumnSpan::select`], whose quotes make a
    /// projection valid CSV; numeric ones read this.
    pub fn select_value<'a>(&self, line: &'a str) -> Cow<'a, str> {
        self.select_value_in(&SplitLine::new(line, [self]))
    }

    /// [`ColumnSpan::select_value`] from a line already split, which
    /// several spans may share.
    pub fn select_value_in<'a>(&self, split: &SplitLine<'a, '_>) -> Cow<'a, str> {
        match self {
            ColumnSpan::Fields(spec) if spec.quoted => {
                join_parts(self.quoted_values(spec, split), self.joiner())
            }
            _ => self.select_in(split),
        }
    }

    /// [`ColumnSpan::select`] from a line already split.
    fn select_in<'a>(&self, split: &SplitLine<'a, '_>) -> Cow<'a, str> {
        match self {
            ColumnSpan::Chars(_) => self.select(split.line),
            ColumnSpan::Fields(spec) => {
                let parts = spec.fields.written();
                let positions = split.positions_for(spec, parts);
                let ranges = resolve_parts(&positions, spec, parts, false);
                text::select_ranges(split.line, &ranges, self.joiner())
            }
        }
    }

    /// The selection in one canonical form, so that a key does not depend
    /// on how the input happened to quote it: under `--quoted` each field
    /// is reduced to its value and quoted again only where it has to be —
    /// when it holds the joiner, a quote or a line break — so `"a"` and
    /// `a` give the same key, while `"x,y"` stays one quoted field. That
    /// minimal quoting is unambiguous, so distinct field lists never meet
    /// in one key (`a,"b,c"` is not `"a,b",c`), and the key is valid CSV
    /// to print. Without `--quoted` it is the selection as written.
    pub fn select_canonical<'a>(&self, line: &'a str) -> Cow<'a, str> {
        self.select_canonical_in(&SplitLine::new(line, [self]))
    }

    /// [`ColumnSpan::select_canonical`] from a line already split, which
    /// several spans may share.
    pub fn select_canonical_in<'a>(&self, split: &SplitLine<'a, '_>) -> Cow<'a, str> {
        match self {
            ColumnSpan::Fields(spec) if spec.quoted => {
                let joiner = self.joiner();
                let fields = self
                    .quoted_values(spec, split)
                    .into_iter()
                    .map(|value| quote_if_needed(value, joiner))
                    .collect();
                join_parts(fields, joiner)
            }
            _ => self.select_in(split),
        }
    }

    /// The CSV value of every selected field, one by one — a part like
    /// `1-2` spans two fields, each with quotes of its own — in the order
    /// written, skipping fields the line does not have.
    fn quoted_values<'a>(&self, spec: &FieldSpan, split: &SplitLine<'a, '_>) -> Vec<Cow<'a, str>> {
        let parts = spec.fields.written();
        let positions = split.positions_for(spec, parts);
        parts
            .iter()
            .flat_map(|part| (*part.start()).max(1)..=(*part.end()).min(positions.len()))
            .map(|field| unquote(text::select_columns(split.line, &positions[field - 1])))
            .collect()
    }

    /// What joins the parts of a selection: the output delimiter in
    /// field mode (the input delimiter unless overridden), nothing in
    /// char mode, where columns are adjacent by definition.
    pub fn joiner(&self) -> &str {
        match self {
            ColumnSpan::Chars(_) => "",
            ColumnSpan::Fields(spec) => spec
                .output_delimiter
                .as_deref()
                .unwrap_or(&spec.delimiter),
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
/// parts that address no field at all. The line is split into fields
/// once, however many parts ask about it.
fn resolve_fields(
    line: &str,
    spec: &FieldSpan,
    parts: &[RangeInclusive<usize>],
    swallow_delimiter: bool,
) -> Vec<RangeInclusive<usize>> {
    let positions = field_positions(line, spec, fields_needed(parts));
    resolve_parts(&positions, spec, parts, swallow_delimiter)
}

/// Resolve every part against fields already split out of the line.
fn resolve_parts(
    positions: &[RangeInclusive<usize>],
    spec: &FieldSpan,
    parts: &[RangeInclusive<usize>],
    swallow_delimiter: bool,
) -> Vec<RangeInclusive<usize>> {
    let delimiter_len = spec.delimiter.chars().count();
    parts
        .iter()
        .filter_map(|part| resolve_part(positions, part, delimiter_len, swallow_delimiter))
        .collect()
}

/// A line split into fields once, for several spans to read from — a
/// summary's key and each of its aggregates — instead of each splitting
/// it again. It is split as far as the furthest of them needs.
///
/// Spans of one run share their field settings, but a span whose
/// delimiter or quoting differs, or that reaches further than the split
/// went, is not given a wrong answer: it splits the line for itself.
pub struct SplitLine<'line, 'spec> {
    line: &'line str,
    fields: Option<SplitFields<'spec>>,
}

struct SplitFields<'spec> {
    delimiter: &'spec str,
    quoted: bool,
    /// How many fields were asked for (see `fields_needed`): a split cut
    /// short there answers for no span reaching further.
    limit: usize,
    positions: Vec<RangeInclusive<usize>>,
}

impl<'line, 'spec> SplitLine<'line, 'spec> {
    /// Split `line` for `spans` to read: as far as the furthest field
    /// span with the settings of the first one needs. A line read only
    /// by char spans is not split at all.
    pub fn new(line: &'line str, spans: impl IntoIterator<Item = &'spec ColumnSpan>) -> Self {
        let mut shared: Option<(&'spec FieldSpan, usize)> = None;
        for span in spans {
            let ColumnSpan::Fields(spec) = span else {
                continue;
            };
            let needed = fields_needed(spec.fields.written());
            match &mut shared {
                None => shared = Some((spec, needed)),
                Some((first, limit)) if same_split(first, spec) => *limit = (*limit).max(needed),
                Some(_) => {}
            }
        }

        let fields = shared.map(|(spec, limit)| SplitFields {
            delimiter: &spec.delimiter,
            quoted: spec.quoted,
            limit,
            positions: field_positions(line, spec, limit),
        });
        SplitLine { line, fields }
    }

    /// The fields of the line as `spec` splits them, as far as `parts`
    /// need: the shared split when it fits, else a split of its own.
    fn positions_for(
        &self,
        spec: &FieldSpan,
        parts: &[RangeInclusive<usize>],
    ) -> Cow<'_, [RangeInclusive<usize>]> {
        let needed = fields_needed(parts);
        match &self.fields {
            Some(fields)
                if fields.delimiter == spec.delimiter
                    && fields.quoted == spec.quoted
                    && fields.limit >= needed =>
            {
                Cow::Borrowed(&fields.positions)
            }
            _ => Cow::Owned(field_positions(self.line, spec, needed)),
        }
    }
}

/// Whether two field spans split a line the same way.
fn same_split(this: &FieldSpan, that: &FieldSpan) -> bool {
    this.delimiter == that.delimiter && this.quoted == that.quoted
}

/// How many fields a line must be split into to resolve `parts`: up to
/// the highest one asked for, plus one more, which tells whether the
/// last field asked for is the last of the line — what clamping a range
/// and swallowing a trailing delimiter need to know. An open range
/// (`3-`) needs them all.
fn fields_needed(parts: &[RangeInclusive<usize>]) -> usize {
    parts
        .iter()
        .map(|part| *part.end())
        .max()
        .unwrap_or(0)
        .saturating_add(1)
}

/// Map a 1-based field range onto the char range those fields occupy.
/// `None` means the range starts past the last field, so it covers no
/// characters at all.
fn resolve_part(
    positions: &[RangeInclusive<usize>],
    fields: &RangeInclusive<usize>,
    delimiter_len: usize,
    swallow_delimiter: bool,
) -> Option<RangeInclusive<usize>> {
    let first = (*fields.start()).max(1);
    let wanted_last = (*fields.end()).min(positions.len());
    if first > positions.len() {
        return None;
    }

    //fields are 1-based, the positions are indexed from 0
    let mut start = *positions[first - 1].start();
    let mut end = *positions[wanted_last - 1].end();

    if swallow_delimiter {
        //take one bordering delimiter with the fields, like `cut`:
        //the trailing one when a field follows, else the leading one
        if wanted_last < positions.len() {
            end += delimiter_len;
        } else if first > 1 {
            start -= delimiter_len;
        }
    }

    Some(start..=end)
}

/// The 1-based char range each field of the line occupies. An empty
/// field ends one char before it starts, an inverted range the text
/// helpers read as nothing.
///
/// In quoted mode a delimiter inside a `"…"` field does not split it,
/// so `a,"b,c"` is two fields rather than three. A doubled quote is the
/// RFC 4180 way of escaping one inside a quoted field; toggling on every
/// quote handles it, since the pair leaves the state where it found it.
/// A field keeps its quotes: they are part of the text it occupies, so
/// selected fields re-join into valid CSV.
///
/// The walk stops once `limit` fields are known: `-C 1` of a long line
/// reads only as far as its first two fields.
fn field_positions(line: &str, spec: &FieldSpan, limit: usize) -> Vec<RangeInclusive<usize>> {
    let bytes = line.as_bytes();
    let delimiter_bytes = spec.delimiter.as_bytes();
    let delimiter_chars = spec.delimiter.chars().count();
    let quoted = spec.quoted;
    //a few fields cover most lines; an open range must not reserve
    //`usize::MAX` of them
    let mut positions = Vec::with_capacity(limit.min(16));

    //walked byte by byte, which needs no decoding: a delimiter or a quote
    //can only start on a char boundary, since no byte of a multi-byte
    //char is a valid first byte. Columns stay 1-based char positions —
    //a char is counted at its first byte, never at a continuation byte
    let mut start = 1usize;
    let mut column = 1usize;
    let mut inside_quotes = false;
    let mut offset = 0usize;

    while let Some(&byte) = bytes.get(offset) {
        if quoted && byte == b'"' {
            inside_quotes = !inside_quotes;
        } else if !inside_quotes
            && byte == delimiter_bytes[0]
            && bytes[offset..].starts_with(delimiter_bytes)
        {
            positions.push(start..=column - 1);
            if positions.len() == limit {
                return positions;
            }
            column += delimiter_chars;
            start = column;
            offset += delimiter_bytes.len();
            continue;
        }
        if !is_continuation_byte(byte) {
            column += 1;
        }
        offset += 1;
    }
    positions.push(start..=column - 1);

    positions
}

/// Whether `byte` continues a multi-byte UTF-8 char rather than starting one.
fn is_continuation_byte(byte: u8) -> bool {
    byte & 0b1100_0000 == 0b1000_0000
}

/// The value of one CSV field: its surrounding quotes removed and each
/// doubled quote inside turned back into one. A field that is not
/// quoted is its own value.
fn unquote(field: &str) -> Cow<'_, str> {
    match field
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    {
        Some(inner) if inner.contains("\"\"") => Cow::Owned(inner.replace("\"\"", "\"")),
        Some(inner) => Cow::Borrowed(inner),
        None => Cow::Borrowed(field),
    }
}

/// A value written as a CSV field: quoted, with its quotes doubled,
/// only when it holds the delimiter, a quote or a line break — the
/// minimal quoting RFC 4180 needs (and Python's `csv` writes).
fn quote_if_needed<'a>(value: Cow<'a, str>, delimiter: &str) -> Cow<'a, str> {
    let needs_quotes = value.contains(delimiter) || value.contains(['"', '\n', '\r']);
    if needs_quotes {
        Cow::Owned(format!("\"{}\"", value.replace('"', "\"\"")))
    } else {
        value
    }
}

/// Join selected parts, borrowing the only one rather than copying it.
fn join_parts<'a>(parts: Vec<Cow<'a, str>>, joiner: &str) -> Cow<'a, str> {
    match <[_; 1]>::try_from(parts) {
        Ok([only]) => only,
        Err(parts) => Cow::Owned(parts.join(joiner)),
    }
}

/// Whether a quoted field is still open at the end of the line — i.e.
/// the CSV record goes on in the next one. `fxt` works line by line, so
/// such a record cannot be read correctly; the caller rejects it rather
/// than return fields cut in half. Counts quotes the way
/// `field_positions` toggles on them.
pub fn leaves_quote_open(line: &str) -> bool {
    line.bytes()
        .filter(|&byte| byte == b'"')
        .count()
        % 2
        == 1
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
        let span = ColumnSpan::Fields(FieldSpan {
            output_delimiter: Some(";".to_owned()),
            ..FieldSpan::new(",", list(&[1..=2]))
        });
        assert_eq!(span.joiner(), ";");
    }

    fn quoted_fields(parts: &[RangeInclusive<usize>]) -> ColumnSpan {
        ColumnSpan::Fields(FieldSpan {
            quoted: true,
            ..FieldSpan::new(",", list(parts))
        })
    }

    #[test]
    fn quoted_span_ignores_a_delimiter_inside_quotes() {
        //pos: `a,"b,c",d` -> a=1, "b,c"=3-7, d=9
        //without --quoted this line would look like four fields
        assert_eq!(
            quoted_fields(&[2..=2])
                .read_ranges(r#"a,"b,c",d"#)
                .as_ref(),
            [3..=7]
        );
        assert_eq!(
            quoted_fields(&[3..=3])
                .read_ranges(r#"a,"b,c",d"#)
                .as_ref(),
            [9..=9]
        );
        //a plain field split sees the comma inside the quotes, so its
        //field 2 is the torn-off `"b`
        assert_eq!(
            fields(",", &[2..=2])
                .read_ranges(r#"a,"b,c",d"#)
                .as_ref(),
            [3..=4]
        );
    }

    #[test]
    fn quoted_span_keeps_the_quotes_with_the_field() {
        //the quotes are part of the field's text, so selected fields
        //re-join into valid CSV
        let span = quoted_fields(&[1..=1]);
        assert_eq!(span.read_ranges(r#""a,b",c"#).as_ref(), [1..=5]);
    }

    #[test]
    fn quoted_span_handles_a_doubled_quote_inside_a_field() {
        //RFC 4180 escapes a quote by doubling it: `"a""b"` is one field
        //pos: `"a""b",c` -> field 1 = 1-6, field 2 = 8
        assert_eq!(
            quoted_fields(&[2..=2])
                .read_ranges(r#""a""b",c"#)
                .as_ref(),
            [8..=8]
        );
    }

    #[test]
    fn quoted_span_deletes_a_whole_quoted_field() {
        //deleting field 2 takes its quotes and one delimiter with it
        assert_eq!(
            quoted_fields(&[2..=2])
                .delete_ranges(r#"a,"b,c",d"#)
                .as_ref(),
            [3..=8]
        );
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

    #[test]
    fn select_value_unquotes_quoted_fields() {
        // Given a quoted number and a field with an escaped quote
        let line = r#"a,"10","say ""hi""",7"#;

        // When / Then each field is read as its value
        assert_eq!(quoted_fields(&[2..=2]).select_value(line), "10");
        assert_eq!(quoted_fields(&[3..=3]).select_value(line), r#"say "hi""#);
        assert_eq!(quoted_fields(&[4..=4]).select_value(line), "7");
        assert_eq!(quoted_fields(&[4..=4, 2..=2]).select_value(line), "7,10");
    }

    #[test]
    fn select_value_keeps_quotes_without_quoted_mode() {
        // Given a plain field split, where quotes are ordinary text
        let span = fields(",", &[2..=2]);

        // When / Then
        assert_eq!(span.select_value(r#"a,"10""#), r#""10""#);
    }

    #[test]
    fn an_odd_quote_count_leaves_a_quote_open() {
        // Given / When / Then
        assert!(leaves_quote_open(r#"1,"hello"#));
        assert!(!leaves_quote_open(r#"1,"hello",2"#));
        assert!(!leaves_quote_open(r#"1,"say ""hi""",2"#));
        assert!(!leaves_quote_open("plain"));
    }

    #[test]
    fn select_canonical_quotes_only_where_needed() {
        // Given the same values, quoted and not
        let span = quoted_fields(&[1..=1]);

        // When / Then a needless quote goes, a needed one stays
        assert_eq!(span.select_canonical(r#""a",1"#), "a");
        assert_eq!(span.select_canonical("a,1"), "a");
        assert_eq!(span.select_canonical(r#""x,y",1"#), r#""x,y""#);
        assert_eq!(
            span.select_canonical(r#""say ""hi""",1"#),
            r#""say ""hi""""#
        );
        assert_eq!(span.select_canonical(r#""",1"#), "");
    }

    #[test]
    fn select_value_unquotes_each_field_of_a_range() {
        // Given one part spanning two quoted fields
        let span = quoted_fields(&[1..=2]);

        // When / Then each field loses its own quotes
        assert_eq!(span.select_value(r#""1","2",3"#), "1,2");
    }

    #[test]
    fn select_canonical_keeps_distinct_field_lists_apart() {
        // Given two lines whose unquoted values join into the same text
        let span = quoted_fields(&[1..=2]);

        // When
        let first = span.select_canonical(r#"a,"b,c""#);
        let second = span.select_canonical(r#""a,b",c"#);

        // Then the quoting tells them apart
        assert_eq!(first, r#"a,"b,c""#);
        assert_eq!(second, r#""a,b",c"#);
    }

    #[test]
    fn select_canonical_quotes_for_the_output_delimiter() {
        // Given fields rejoined by ';' instead of ','
        let span = ColumnSpan::Fields(FieldSpan {
            quoted: true,
            output_delimiter: Some(";".to_owned()),
            ..FieldSpan::new(",", list(&[1..=2]))
        });

        // When / Then a ';' in a value is what needs quoting now
        assert_eq!(span.select_canonical(r#""a;b","c,d""#), r#""a;b";c,d"#);
    }

    #[test]
    fn a_split_that_stops_early_still_sees_the_next_field() {
        // Given a line with more fields than the span reaches
        let line = "a,b,c,d,e";
        let span = fields(",", &[2..=3]);

        // When / Then the selection is right, and deleting it swallows
        // the trailing delimiter — which needs to know field 4 exists
        assert_eq!(span.select(line), "b,c");
        assert_eq!(
            text::remove_ranges(line, &span.delete_ranges(line)),
            "a,d,e"
        );
        // while an open range still reaches the end of the line
        assert_eq!(fields(",", &[4..=usize::MAX]).select(line), "d,e");
    }

    #[test]
    fn select_canonical_is_the_selection_without_quoted_mode() {
        // Given / When / Then
        assert_eq!(fields(",", &[1..=1]).select_canonical(r#""a",1"#), r#""a""#);
    }
}

/// Property tests: the field splitting is checked against references
/// simple enough to be obviously right, on generated lines full of what
/// breaks splitters — multi-byte chars, quotes, empty fields, delimiters
/// that are several chars long or multi-byte themselves.
#[cfg(test)]
mod properties {
    use super::*;
    use proptest::prelude::*;

    /// The char-by-char splitter `field_positions` replaced: slow, but
    /// a direct reading of the rules, so the byte walk must agree with it.
    fn reference_positions(
        line: &str,
        delimiter: &str,
        quoted: bool,
    ) -> Vec<RangeInclusive<usize>> {
        let delimiter_len = delimiter.chars().count();
        let mut positions = Vec::new();
        let (mut start, mut column, mut inside_quotes, mut skip) = (1, 1, false, 0);
        for (offset, character) in line.char_indices() {
            if skip > 0 {
                skip -= 1;
            } else if quoted && character == '"' {
                inside_quotes = !inside_quotes;
            } else if !inside_quotes && line[offset..].starts_with(delimiter) {
                positions.push(start..=column - 1);
                start = column + delimiter_len;
                skip = delimiter_len - 1;
            }
            column += 1;
        }
        positions.push(start..=column - 1);
        positions
    }

    fn delimiters() -> impl Strategy<Value = &'static str> {
        prop::sample::select(vec![",", ";", "::", "ż", "🙂"])
    }

    /// Lines built from the delimiters' own pieces and other trouble.
    fn lines() -> impl Strategy<Value = String> {
        let pieces = prop::sample::select(vec![
            "a", "b", "xyz", " ", ",", ";", ":", "::", "\"", "\"\"", "ż", "🙂", "e\u{301}", "",
        ]);
        prop::collection::vec(pieces, 0..24).prop_map(|pieces| pieces.concat())
    }

    /// A field range: a closed one, or open towards the end.
    fn field_ranges() -> impl Strategy<Value = RangeInclusive<usize>> {
        (1usize..8, 0usize..6, any::<bool>()).prop_map(|(start, length, open)| {
            start..=if open { usize::MAX } else { start + length }
        })
    }

    fn span(delimiter: &str, quoted: bool, parts: Vec<RangeInclusive<usize>>) -> FieldSpan {
        FieldSpan {
            quoted,
            ..FieldSpan::new(delimiter, ColumnList::new(parts))
        }
    }

    proptest! {
        #[test]
        fn byte_walk_matches_the_char_walk(
            line in lines(),
            delimiter in delimiters(),
            quoted in any::<bool>(),
            limit in 1usize..10,
        ) {
            let spec = span(delimiter, quoted, vec![1..=1]);
            let reference = reference_positions(&line, delimiter, quoted);

            // the full split agrees, and an early stop yields its prefix
            prop_assert_eq!(field_positions(&line, &spec, usize::MAX), reference.clone());
            let early = field_positions(&line, &spec, limit);
            prop_assert_eq!(&early[..], &reference[..limit.min(reference.len())]);
        }

        #[test]
        fn selecting_fields_is_split_and_join(
            line in lines(),
            delimiter in delimiters(),
            range in field_ranges(),
        ) {
            //without --quoted a field split is exactly `str::split`
            let pieces: Vec<&str> = line.split(delimiter).collect();
            let first = *range.start();
            let expected = match first <= pieces.len() {
                true => pieces[first - 1..(*range.end()).min(pieces.len())].join(delimiter),
                false => String::new(),
            };
            let span = ColumnSpan::fields(delimiter, ColumnList::new(vec![range]));
            prop_assert_eq!(span.select(&line), expected);
        }

        #[test]
        fn deleting_fields_keeps_the_others_joined(
            line in lines(),
            delimiter in delimiters(),
            range in field_ranges(),
        ) {
            //like `cut --complement`: what is left is the other fields,
            //joined by the delimiter — no stray one on either side
            let kept: Vec<&str> = line
                .split(delimiter)
                .enumerate()
                .filter(|(index, _)| !range.contains(&(index + 1)))
                .map(|(_, field)| field)
                .collect();
            let span = ColumnSpan::fields(delimiter, ColumnList::new(vec![range]));
            let deleted = text::remove_ranges(&line, &span.delete_ranges(&line));
            prop_assert_eq!(deleted, kept.join(delimiter));
        }

        #[test]
        fn a_shared_split_reads_like_a_split_of_ones_own(
            line in lines(),
            delimiter in delimiters(),
            other in delimiters(),
            quoted in any::<bool>(),
            parts in prop::collection::vec(prop::collection::vec(field_ranges(), 1..3), 1..4),
        ) {
            // Given several spans sharing one split — and one that splits
            // differently, which must fall back to a split of its own
            let mut spans: Vec<ColumnSpan> = parts
                .into_iter()
                .map(|parts| ColumnSpan::Fields(span(delimiter, quoted, parts)))
                .collect();
            spans.push(ColumnSpan::Fields(span(other, !quoted, vec![2..=3])));
            let split = SplitLine::new(&line, &spans);

            // When / Then each reads what it would read alone
            for span in &spans {
                prop_assert_eq!(span.select_value_in(&split), span.select_value(&line));
                prop_assert_eq!(span.select_canonical_in(&split), span.select_canonical(&line));
                prop_assert_eq!(span.select_in(&split), span.select(&line));
            }
        }

        #[test]
        fn quoted_values_round_trip(
            values in prop::collection::vec("[a-c,\" żé🙂]{0,6}", 1..6),
            always_quote in any::<bool>(),
        ) {
            // Given values written as CSV, quoted where needed (or always)
            let line = values
                .iter()
                .map(|value| match always_quote {
                    true => format!("\"{}\"", value.replace('"', "\"\"")),
                    false => quote_if_needed(Cow::Borrowed(value), ",").into_owned(),
                })
                .collect::<Vec<_>>()
                .join(",");

            for (index, value) in values.iter().enumerate() {
                let field = ColumnSpan::Fields(span(",", true, vec![index + 1..=index + 1]));
                // When / Then each field reads back as the value written
                prop_assert_eq!(field.select_value(&line), value.as_str());
                // and the canonical form does not depend on the quoting
                prop_assert_eq!(
                    field.select_canonical(&line),
                    quote_if_needed(Cow::Borrowed(value), ",")
                );
            }
        }
    }
}
