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
use std::ops::RangeInclusive;

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
    rewrite_ranges(line, cols, |_| String::new())
}

/// Apply `map_within` to the parts of the line inside the column ranges,
/// leaving the rest of the line untouched.
pub fn map_ranges<F>(line: &str, cols: &[RangeInclusive<usize>], map_within: F) -> String
where
    F: Fn(&str) -> String,
{
    rewrite_ranges(line, cols, map_within)
}

/// Replace every `find` occurrence with `replace`, but only within the
/// column ranges; the rest of the line is left untouched. Each range is
/// searched on its own, so a match straddling two of them is not
/// replaced.
pub fn replace_in_ranges(
    line: &str,
    find: &str,
    replace: &str,
    cols: &[RangeInclusive<usize>],
) -> String {
    map_ranges(line, cols, |within| within.replace(find, replace))
}

/// Rebuild the line, replacing the content of each column range with
/// the result of `map_within` (the empty string removes it). Walks the
/// line once, left to right, so the ranges must ascend; one starting
/// inside an already rewritten range is skipped rather than applied
/// twice.
fn rewrite_ranges<F>(line: &str, cols: &[RangeInclusive<usize>], map_within: F) -> String
where
    F: Fn(&str) -> String,
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
        result.push_str(&map_within(&line[from..to]));
        cursor = to;
    }
    result.push_str(&line[cursor..]);

    result
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
