//! Pure text helpers operating on 1-based, inclusive, char-indexed
//! column ranges. A range lying entirely beyond the line covers
//! nothing: selecting through it yields an empty string, removing or
//! mapping through it leaves the line unchanged.
//!
//! The `_ranges` helpers take several ranges at once. Selecting reads
//! them in the order given, so the caller can permute the parts;
//! removing and mapping rewrite the line in one pass and therefore
//! expect ascending, non-overlapping ranges (a part reaching back into
//! one already rewritten is skipped).

use std::borrow::Cow;
use std::convert::Infallible;
use std::ops::{Range, RangeInclusive};
use unicode_segmentation::UnicodeSegmentation;

/// Split a line into its content and line terminator ("\r\n", "\n" or none).
pub fn split_line_terminator(line: &str) -> (&str, &'static str) {
    if let Some(content) = line.strip_suffix("\r\n") {
        return (content, "\r\n");
    }
    if let Some(content) = line.strip_suffix('\n') {
        return (content, "\n");
    }
    (line, "")
}

/// The content of a raw line, its terminator ("\r\n" or "\n") cut off:
/// [`split_line_terminator`] for bytes that need not be UTF-8.
pub fn without_line_terminator(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\r\n")
        .or_else(|| line.strip_suffix(b"\n"))
        .unwrap_or(line)
}

/// Cut the line into consecutive chunks of at most `width` characters
/// (like `fold -w`), counting chars rather than bytes. A line that
/// already fits yields itself, so the result is never empty; a `width`
/// of 0 would never advance and is rejected before it gets here.
pub fn wrap_chars(line: &str, width: usize) -> Vec<&str> {
    debug_assert!(width > 0, "wrapping needs a width of at least one char");

    let mut chunks = Vec::new();
    let mut start = 0usize;
    let mut chars = 0usize;

    for (offset, _) in line.char_indices() {
        if chars == width {
            chunks.push(&line[start..offset]);
            start = offset;
            chars = 0;
        }
        chars += 1;
    }
    chunks.push(&line[start..]);

    chunks
}

/// [`wrap_chars`] counting grapheme clusters instead (`--graphemes`), so
/// a chunk boundary never falls inside a user-perceived character.
pub fn wrap_graphemes(line: &str, width: usize) -> Vec<&str> {
    debug_assert!(width > 0, "wrapping needs a width of at least one");

    let mut chunks = Vec::new();
    let mut start = 0usize;
    let mut count = 0usize;

    for (offset, _) in line.grapheme_indices(true) {
        if count == width {
            chunks.push(&line[start..offset]);
            start = offset;
            count = 0;
        }
        count += 1;
    }
    chunks.push(&line[start..]);

    chunks
}

/// Map a 1-based inclusive char range onto the byte range it occupies
/// on `line`, clamping the end to the line length. `None` means the
/// range lies entirely beyond the line (or is inverted) and covers
/// no characters.
fn byte_range(line: &str, cols: &RangeInclusive<usize>) -> Option<(usize, usize)> {
    //column numbering is 1-based; treat a 0 start as column 1
    let start = (*cols.start()).max(1);
    let end = *cols.end();
    if start > end {
        return None;
    }

    //in ASCII text a column is a byte, so no walk is needed — the common
    //case, and `is_ascii` checks a word at a time
    if line.is_ascii() {
        return (start <= line.len()).then(|| (start - 1, end.min(line.len())));
    }

    let mut from = None;
    let mut to = line.len();
    for (position, (offset, _)) in line.char_indices().enumerate() {
        let column = position + 1;
        if column == start {
            from = Some(offset);
        }
        if column > end {
            to = offset;
            break;
        }
    }
    from.map(|from| (from, to))
}

/// The byte range `part` occupies in `line`, if it is a slice of it —
/// as a borrowed selection usually is, though an empty one may be a
/// `""` from anywhere. Plain address arithmetic, no `unsafe`.
pub fn offset_in(line: &str, part: &str) -> Option<Range<usize>> {
    let start = (part.as_ptr() as usize).wrapping_sub(line.as_ptr() as usize);
    (start <= line.len() && part.len() <= line.len() - start).then(|| start..start + part.len())
}

/// Return only the characters within the column range, like `cut`:
/// a range that lies entirely beyond the line yields an empty string.
pub fn select_columns<'a>(line: &'a str, cols: &RangeInclusive<usize>) -> &'a str {
    match byte_range(line, cols) {
        Some((from, to)) => &line[from..to],
        None => "",
    }
}

/// Return the characters within every column range, read in the order
/// given (so the ranges may permute the line) and joined by `joiner`.
/// A single range borrows straight from the line.
pub fn select_ranges<'a>(
    line: &'a str,
    cols: &[RangeInclusive<usize>],
    joiner: &str,
) -> Cow<'a, str> {
    match cols {
        [] => Cow::Borrowed(""),
        [only] => Cow::Borrowed(select_columns(line, only)),
        parts => {
            let selected: Vec<&str> = parts
                .iter()
                .map(|part| select_columns(line, part))
                .collect();
            Cow::Owned(selected.join(joiner))
        }
    }
}

/// Return the line with the characters within the column ranges removed.
pub fn remove_ranges(line: &str, cols: &[RangeInclusive<usize>]) -> String {
    rewrite_ranges(line, cols, |_, _| {})
}

/// Apply `map_within` to the parts of the line inside the column ranges,
/// leaving the rest of the line untouched.
pub fn map_ranges<F>(line: &str, cols: &[RangeInclusive<usize>], map_within: F) -> String
where
    F: Fn(&str) -> String,
{
    rewrite_ranges(line, cols, |within, result| {
        result.push_str(&map_within(within))
    })
}

/// Replace each tab with spaces up to the next tab stop, one every `tab`
/// columns counted from the start of `text`, like `expand -t`. A line
/// break inside the text (a multi-line CSV record) starts the count over.
pub fn expand_tabs(text: &str, tab: usize) -> String {
    let mut result = String::with_capacity(text.len());
    let mut column = 0;
    for character in text.chars() {
        match character {
            '\t' => {
                let spaces = tab - column % tab;
                result.extend(std::iter::repeat_n(' ', spaces));
                column += spaces;
            }
            '\n' => {
                result.push(character);
                column = 0;
            }
            _ => {
                result.push(character);
                column += 1;
            }
        }
    }
    result
}

/// Turn the blanks that reach a tab stop into tabs, like `unexpand -a`:
/// a run of two or more spaces ending at a stop becomes one tab, spaces
/// right before a tab go into it, and a lone space stays a space, so the
/// gaps between words are not turned into tabs by accident. Whatever it
/// writes [`expand_tabs`] expands to what the text expands to.
pub fn unexpand_tabs(text: &str, tab: usize) -> String {
    let mut result = String::with_capacity(text.len());
    let mut column = 0;
    //spaces read but not yet written: a stop may turn them into a tab
    let mut pending = 0;
    for character in text.chars() {
        match character {
            ' ' => {
                pending += 1;
                column += 1;
                if column % tab == 0 {
                    result.push(if pending > 1 { '\t' } else { ' ' });
                    pending = 0;
                }
            }
            '\t' => {
                pending = 0;
                result.push('\t');
                column += tab - column % tab;
            }
            _ => {
                result.extend(std::iter::repeat_n(' ', pending));
                pending = 0;
                result.push(character);
                column = if character == '\n' { 0 } else { column + 1 };
            }
        }
    }
    result.extend(std::iter::repeat_n(' ', pending));
    result
}

/// Rebuild the line with each cell's text put through `map`. A cell is a
/// char range as [`crate::columns::ColumnSpan::cell_ranges`] gives them:
/// ascending, and an empty one (`n..=n-1`) is the point before column
/// `n`, so something can be written into an empty field. A cell lying
/// past the end of the line is skipped.
pub fn map_cells<F>(line: &str, cells: &[RangeInclusive<usize>], map: F) -> String
where
    F: Fn(&str) -> String,
{
    let mut result = String::with_capacity(line.len());
    let mut cursor = 0usize;

    for cell in cells {
        let bounds = match cell.start() > cell.end() {
            true => column_offset(line, *cell.start()).map(|at| (at, at)),
            false => byte_range(line, cell),
        };
        let Some((from, to)) = bounds else {
            continue;
        };
        if from < cursor {
            continue;
        }
        result.push_str(&line[cursor..from]);
        result.push_str(&map(&line[from..to]));
        cursor = to;
    }
    result.push_str(&line[cursor..]);
    result
}

/// The byte offset at which the 1-based char `column` starts — the
/// line's length for the column just past its end, `None` beyond that.
fn column_offset(line: &str, column: usize) -> Option<usize> {
    let index = column.max(1) - 1;
    if line.is_ascii() {
        return (index <= line.len()).then_some(index);
    }
    line.char_indices()
        .map(|(offset, _)| offset)
        .chain(std::iter::once(line.len()))
        .nth(index)
}

/// Replace every `find` occurrence with `replace`, but only within the
/// column ranges; the rest of the line is left untouched. Each range is
/// searched on its own, so a match straddling two of them is not
/// replaced. The pieces go straight into the rebuilt line, with no
/// intermediate string per range.
pub fn replace_in_ranges(
    line: &str,
    find: &str,
    replace: &str,
    cols: &[RangeInclusive<usize>],
) -> String {
    rewrite_ranges(line, cols, |within, result| {
        let mut last = 0;
        for (at, _) in within.match_indices(find) {
            result.push_str(&within[last..at]);
            result.push_str(replace);
            last = at + find.len();
        }
        result.push_str(&within[last..]);
    })
}

/// [`map_ranges`] with a mapping that may fail: the first error ends the
/// rewrite and is returned.
pub fn try_map_ranges<F, E>(
    line: &str,
    cols: &[RangeInclusive<usize>],
    mut map_within: F,
) -> Result<String, E>
where
    F: FnMut(&str) -> Result<Cow<'_, str>, E>,
{
    try_rewrite_ranges(line, cols, |within, result| {
        result.push_str(&map_within(within)?);
        Ok(())
    })
}

/// Rebuild the line, letting `write_within` write what replaces the
/// content of each column range (writing nothing removes it).
fn rewrite_ranges<F>(line: &str, cols: &[RangeInclusive<usize>], mut write_within: F) -> String
where
    F: FnMut(&str, &mut String),
{
    let rewritten = try_rewrite_ranges(line, cols, |within, result| {
        write_within(within, result);
        Ok::<(), Infallible>(())
    });
    match rewritten {
        Ok(line) => line,
        Err(never) => match never {},
    }
}

/// [`rewrite_ranges`] for a `write_within` that may fail. Walks the line
/// once, left to right, so the ranges must ascend; one starting inside
/// an already rewritten range is skipped rather than applied twice.
fn try_rewrite_ranges<F, E>(
    line: &str,
    cols: &[RangeInclusive<usize>],
    mut write_within: F,
) -> Result<String, E>
where
    F: FnMut(&str, &mut String) -> Result<(), E>,
{
    let mut result = String::with_capacity(line.len());
    let mut cursor = 0usize;

    for part in cols {
        let Some((from, to)) = byte_range(line, part) else {
            continue;
        };
        if from < cursor {
            continue;
        }
        result.push_str(&line[cursor..from]);
        write_within(&line[from..to], &mut result)?;
        cursor = to;
    }
    result.push_str(&line[cursor..]);

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_line_terminator() {
        assert_eq!(split_line_terminator("abc\n"), ("abc", "\n"));
        assert_eq!(split_line_terminator("abc\r\n"), ("abc", "\r\n"));
        assert_eq!(split_line_terminator("abc"), ("abc", ""));
        assert_eq!(split_line_terminator("\n"), ("", "\n"));
        assert_eq!(split_line_terminator(""), ("", ""));
    }

    #[test]
    fn test_select_columns() {
        //pos:           123456789012345678901
        let line_utf8 = "ABCabcąłć😊😍😁123śę";

        assert_eq!(select_columns(line_utf8, &(1..=3)), "ABC");
        assert_eq!(select_columns(line_utf8, &(1..=6)), "ABCabc");
        assert_eq!(select_columns(line_utf8, &(1..=9)), "ABCabcąłć");
        assert_eq!(select_columns(line_utf8, &(1..=11)), "ABCabcąłć😊😍");
        assert_eq!(select_columns(line_utf8, &(1..=16)), "ABCabcąłć😊😍😁123ś");
        assert_eq!(select_columns(line_utf8, &(1..=17)), "ABCabcąłć😊😍😁123śę");
        assert_eq!(select_columns(line_utf8, &(4..=30)), "abcąłć😊😍😁123śę");
        assert_eq!(select_columns(line_utf8, &(9..=30)), "ć😊😍😁123śę");
        assert_eq!(select_columns(line_utf8, &(10..=12)), "😊😍😁");
        assert_eq!(select_columns(line_utf8, &(11..=11)), "😍");
        //end beyond line length is clamped
        assert_eq!(select_columns(line_utf8, &(16..=30)), "śę");
        //a range beyond the line selects nothing
        assert_eq!(select_columns(line_utf8, &(20..=30)), "");
        //an inverted range selects nothing (`10..=5` literal trips clippy)
        assert_eq!(select_columns(line_utf8, &RangeInclusive::new(10, 5)), "");
        assert_eq!(select_columns("", &(1..=5)), "");
    }

    #[test]
    fn test_remove_columns() {
        //pos:           123456789012345678901
        let line_utf8 = "ABCabcąłć😊😍😁123śę";
        let remove = |cols| remove_ranges(line_utf8, &[cols]);

        assert_eq!(remove(1..=3), "abcąłć😊😍😁123śę");
        assert_eq!(remove(1..=6), "ąłć😊😍😁123śę");
        assert_eq!(remove(1..=9), "😊😍😁123śę");
        assert_eq!(remove(1..=11), "😁123śę");
        assert_eq!(remove(1..=16), "ę");
        assert_eq!(remove(1..=18), "");
        assert_eq!(remove(4..=30), "ABC");
        assert_eq!(remove(9..=30), "ABCabcął");
        assert_eq!(remove(10..=12), "ABCabcąłć123śę");
        assert_eq!(remove(11..=11), "ABCabcąłć😊😁123śę");
        //start beyond line length leaves the line unchanged
        assert_eq!(remove(20..=30), line_utf8);
    }

    #[test]
    fn test_replace_in_columns() {
        //pos:          123456789012345678901234
        let line_str = "Test01234567891231234567";
        let replace = |find, to, cols| replace_in_ranges(line_str, find, to, &[cols]);

        assert_eq!(
            replace("Test", "Passed", 1..=usize::MAX),
            "Passed01234567891231234567"
        );
        assert_eq!(
            replace("123", "ABC", 1..=usize::MAX),
            "Test0ABC456789ABCABC4567"
        );
        assert_eq!(
            replace("123", "ABCDEF", 1..=usize::MAX),
            "Test0ABCDEF456789ABCDEFABCDEF4567"
        );
        assert_eq!(replace("123", "", 1..=usize::MAX), "Test04567894567");
        assert_eq!(replace("123", "ABCD", 6..=8), "Test0ABCD4567891231234567");

        //match only partially inside the range is not replaced
        assert_eq!(replace("123", "ABCD", 7..=8), line_str);
        assert_eq!(replace("123", "ABCD", 6..=7), line_str);

        assert_eq!(
            replace("123", "ABCD", 15..=20),
            "Test0123456789ABCDABCD4567"
        );

        //range outside the line or inverted leaves the line unchanged
        assert_eq!(replace("123", "ABCD", 21..=40), line_str);
        assert_eq!(replace("123", "ABCD", 30..=40), line_str);
        //inverted range built explicitly, `10..=5` literal trips clippy
        assert_eq!(replace("123", "ABCD", RangeInclusive::new(10, 5)), line_str);
    }

    #[test]
    fn wrap_chars_cuts_the_line_into_chunks() {
        assert_eq!(wrap_chars("abcdefg", 3), ["abc", "def", "g"]);
        //an exact multiple of the width leaves no remainder chunk
        assert_eq!(wrap_chars("abcdef", 3), ["abc", "def"]);
        //a line that already fits yields itself
        assert_eq!(wrap_chars("abc", 3), ["abc"]);
        assert_eq!(wrap_chars("", 3), [""]);
    }

    #[test]
    fn wrap_chars_counts_chars_not_bytes() {
        //each of these is multi-byte, so a byte-based split would tear
        //them apart mid-character
        assert_eq!(wrap_chars("ąłć😊😍😁", 2), ["ął", "ć😊", "😍😁"]);
    }

    #[test]
    fn wrap_graphemes_never_splits_a_cluster() {
        // Given / When / Then a ZWJ sequence and a decomposed accent stay whole
        let line = "\u{1F469}\u{200D}\u{1F4BB}e\u{301}xy";
        assert_eq!(
            wrap_graphemes(line, 2),
            ["\u{1F469}\u{200D}\u{1F4BB}e\u{301}", "xy"]
        );
        assert_eq!(wrap_graphemes("", 3), [""]);
    }

    #[test]
    fn offset_in_finds_a_slice_of_the_line() {
        // Given / When / Then
        let line = "zażółć gęślą";
        assert_eq!(offset_in(line, &line[2..8]), Some(2..8));
        assert_eq!(
            offset_in(line, &line[line.len()..]),
            Some(line.len()..line.len())
        );
        assert_eq!(offset_in(line, line), Some(0..line.len()));
    }

    #[test]
    fn offset_in_rejects_text_from_elsewhere() {
        // Given / When / Then
        let elsewhere = String::from("elsewhere");
        assert_eq!(offset_in("abc", &elsewhere), None);
    }

    /// The empty cell before column `n`, as field positions write it.
    fn empty_at(n: usize) -> RangeInclusive<usize> {
        n..=n - 1
    }

    #[test]
    fn expand_tabs_fills_to_the_next_stop() {
        // Given / When / Then
        assert_eq!(expand_tabs("a\tbc\td", 4), "a   bc  d");
        assert_eq!(expand_tabs("abcd\te", 4), "abcd    e");
        assert_eq!(expand_tabs("\t\tx", 2), "    x");
        assert_eq!(expand_tabs("ż\tx\n\ty", 4), "ż   x\n    y");
    }

    #[test]
    fn unexpand_tabs_turns_blanks_at_a_stop_into_tabs() {
        // Given / When / Then
        assert_eq!(unexpand_tabs("a   bc  d", 4), "a\tbc\td");
        assert_eq!(unexpand_tabs("        x", 4), "\t\tx");
        //a lone space at a stop stays a space; spaces before a tab go into it
        assert_eq!(unexpand_tabs("abc d", 4), "abc d");
        assert_eq!(unexpand_tabs("a  \tb", 4), "a\tb");
        //trailing spaces that reach no stop stay as they are
        assert_eq!(unexpand_tabs("ab  ", 8), "ab  ");
    }

    #[test]
    fn map_cells_writes_into_empty_cells_too() {
        // Given "a,,ż" with its fields as cells: 1..=1, an empty 3..=2, 4..=4
        let line = "a,,ż";
        let cells = [1..=1, empty_at(3), 4..=4];

        // When each cell is bracketed
        let result = map_cells(line, &cells, |cell| format!("[{cell}]"));

        // Then the empty field gets its brackets as well
        assert_eq!(result, "[a],[],[ż]");
    }

    #[test]
    fn map_cells_skips_cells_past_the_line() {
        // Given / When / Then: the point just past the end is still on
        // the line, anything further is not
        assert_eq!(map_cells("ab", &[empty_at(3)], |_| "!".to_owned()), "ab!");
        assert_eq!(
            map_cells("ab", &[empty_at(4), 5..=9], |_| "!".to_owned()),
            "ab"
        );
        assert_eq!(map_cells("żb", &[empty_at(3)], |_| "!".to_owned()), "żb!");
    }

    #[test]
    fn select_ranges_reads_the_parts_in_order() {
        //an empty list selects nothing, one part borrows from the line
        assert_eq!(select_ranges("abcdef", &[], ""), "");
        assert_eq!(select_ranges("abcdef", &[2..=3], ""), "bc");

        //the parts are read as given, so they may permute or repeat
        assert_eq!(select_ranges("abcdef", &[5..=6, 1..=2], ""), "efab");
        assert_eq!(select_ranges("abcdef", &[1..=1, 1..=1], ""), "aa");
        //a part beyond the line contributes an empty string
        assert_eq!(select_ranges("abc", &[1..=1, 9..=9], ""), "a");
    }

    #[test]
    fn select_ranges_joins_the_parts() {
        assert_eq!(select_ranges("a,b,c", &[5..=5, 1..=1], ","), "c,a");
        assert_eq!(select_ranges("a,b,c", &[5..=5, 1..=1], " | "), "c | a");
    }

    #[test]
    fn remove_ranges_removes_every_part() {
        //pos:        123456
        let line = "abcdef";
        assert_eq!(remove_ranges(line, &[1..=2, 5..=6]), "cd");
        assert_eq!(remove_ranges(line, &[]), line);
        //a part reaching into one already removed is skipped, not
        //applied twice
        assert_eq!(remove_ranges(line, &[1..=3, 2..=4]), "def");
    }

    #[test]
    fn map_ranges_maps_every_part_on_its_own() {
        let upper = |within: &str| within.to_uppercase();
        assert_eq!(map_ranges("abcdef", &[1..=2, 5..=6], upper), "ABcdEF");
        assert_eq!(map_ranges("abcdef", &[], upper), "abcdef");

        //each part is mapped separately, so a length change in one does
        //not shift the next
        let double = |within: &str| within.repeat(2);
        assert_eq!(map_ranges("abcdef", &[1..=1, 3..=3], double), "aabccdef");
    }

    #[test]
    fn replace_in_ranges_searches_each_part_on_its_own() {
        //"ab" straddles the two parts and is not replaced
        assert_eq!(
            replace_in_ranges("xabx", "ab", "-", &[1..=2, 3..=4]),
            "xabx"
        );
        assert_eq!(
            replace_in_ranges("abxab", "ab", "-", &[1..=2, 4..=5]),
            "-x-"
        );
    }
}

/// Property tests: the char-indexed helpers against a `Vec<char>`
/// reference, on ASCII lines (where columns map straight to bytes) and
/// on mixed ones (where they are walked), with ranges reaching past the
/// line, inverted and starting at 0.
#[cfg(test)]
mod properties {
    use super::*;
    use proptest::prelude::*;

    fn lines() -> impl Strategy<Value = String> {
        prop_oneof!["[a-z ,]{0,20}", "[a-zżé🙂 ,]{0,20}"]
    }

    fn ranges() -> impl Strategy<Value = RangeInclusive<usize>> {
        (0usize..25, 0usize..25).prop_map(|(start, end)| start..=end)
    }

    /// The columns of `range` as char indices into the line, 0-based.
    fn indices(range: &RangeInclusive<usize>, len: usize) -> std::ops::Range<usize> {
        let start = (*range.start()).max(1) - 1;
        let end = (*range.end()).min(len);
        start.min(end)..end
    }

    proptest! {
        #[test]
        fn unexpanding_keeps_what_the_text_expands_to(text in "[a-cż \t]{0,30}", tab in 1usize..9) {
            prop_assert_eq!(expand_tabs(&unexpand_tabs(&text, tab), tab), expand_tabs(&text, tab));
        }

        #[test]
        fn expanding_leaves_no_tab_and_unexpanding_adds_no_space(text in "[a-c \t]{0,30}", tab in 1usize..9) {
            prop_assert!(!expand_tabs(&text, tab).contains('\t'));
            let spaces = |text: &str| text.matches(' ').count();
            prop_assert!(spaces(&unexpand_tabs(&text, tab)) <= spaces(&text));
        }

        #[test]
        fn map_cells_marks_each_field_where_it_lies(line in "[a-zż]{0,3}(,[a-zż]{0,3}){0,4}") {
            //the fields of a comma-split line as cells (empty ones
            //inverted, as field positions write them), each wrapped in
            //brackets: the result is the line with every field bracketed
            let mut cells = Vec::new();
            let mut column = 1;
            for field in line.split(',') {
                let width = field.chars().count();
                cells.push(column..=column + width - 1);
                column += width + 1;
            }
            let expected: Vec<String> = line.split(',').map(|field| format!("[{field}]")).collect();
            prop_assert_eq!(map_cells(&line, &cells, |cell| format!("[{cell}]")), expected.join(","));
        }

        #[test]
        fn select_columns_matches_a_char_slice(line in lines(), range in ranges()) {
            let chars: Vec<char> = line.chars().collect();
            let expected: String = chars[indices(&range, chars.len())].iter().collect();
            prop_assert_eq!(select_columns(&line, &range), expected);
        }

        #[test]
        fn select_and_delete_rebuild_the_line(line in lines(), range in ranges()) {
            //what a range selects, put back where it was deleted from,
            //is the line again
            let chars: Vec<char> = line.chars().collect();
            let at = indices(&range, chars.len()).start;
            let selected = select_columns(&line, &range);
            let rest: Vec<char> = remove_ranges(&line, std::slice::from_ref(&range)).chars().collect();
            let rebuilt: String = rest[..at]
                .iter()
                .copied()
                .chain(selected.chars())
                .chain(rest[at..].iter().copied())
                .collect();
            prop_assert_eq!(rebuilt, line);
        }

        #[test]
        fn replace_in_ranges_is_str_replace_within_each_range(
            line in lines(),
            range in ranges(),
            find in prop::sample::select(vec!["a", " ", ",", "ż", "aa", ""]),
            replace in prop::sample::select(vec!["", "X", "🙂🙂"]),
        ) {
            let expected = map_ranges(&line, std::slice::from_ref(&range), |within| {
                within.replace(find, replace)
            });
            let replaced = replace_in_ranges(&line, find, replace, std::slice::from_ref(&range));
            prop_assert_eq!(replaced, expected);
        }

        #[test]
        fn map_ranges_touches_only_the_range(line in lines(), range in ranges()) {
            let chars: Vec<char> = line.chars().collect();
            let inside = indices(&range, chars.len());
            let expected: String = chars
                .iter()
                .enumerate()
                .map(|(index, &c)| if inside.contains(&index) { c.to_ascii_uppercase() } else { c })
                .collect();
            let mapped = map_ranges(&line, std::slice::from_ref(&range), |text| text.to_ascii_uppercase());
            prop_assert_eq!(mapped, expected);
        }
    }
}
