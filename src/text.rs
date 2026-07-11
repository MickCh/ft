//! Pure text helpers operating on 1-based, inclusive, char-indexed
//! column ranges. A range lying entirely beyond the line covers
//! nothing: selecting through it yields an empty string, removing or
//! mapping through it leaves the line unchanged.

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

/// Return the line with the characters within the column range removed.
pub fn remove_columns(line: &str, cols: &RangeInclusive<usize>) -> String {
    match byte_range(line, cols) {
        Some((from, to)) => {
            let mut result = String::with_capacity(line.len() - (to - from));
            result.push_str(&line[..from]);
            result.push_str(&line[to..]);
            result
        }
        None => line.to_owned(),
    }
}

/// Apply `map_within` to the part of the line inside the column range,
/// leaving the rest of the line untouched.
pub fn map_columns<F>(line: &str, cols: &RangeInclusive<usize>, map_within: F) -> String
where
    F: FnOnce(&str) -> String,
{
    match byte_range(line, cols) {
        Some((from, to)) => {
            let mapped = map_within(&line[from..to]);
            let mut result = String::with_capacity(from + mapped.len() + (line.len() - to));
            result.push_str(&line[..from]);
            result.push_str(&mapped);
            result.push_str(&line[to..]);
            result
        }
        None => line.to_owned(),
    }
}

/// Replace every `find` occurrence with `replace`, but only within
/// the column range; the rest of the line is left untouched.
pub fn replace_in_columns(
    line: &str,
    find: &str,
    replace: &str,
    cols: &RangeInclusive<usize>,
) -> String {
    map_columns(line, cols, |within| within.replace(find, replace))
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

        assert_eq!(remove_columns(line_utf8, &(1..=3)), "abcąłć😊😍😁123śę");
        assert_eq!(remove_columns(line_utf8, &(1..=6)), "ąłć😊😍😁123śę");
        assert_eq!(remove_columns(line_utf8, &(1..=9)), "😊😍😁123śę");
        assert_eq!(remove_columns(line_utf8, &(1..=11)), "😁123śę");
        assert_eq!(remove_columns(line_utf8, &(1..=16)), "ę");
        assert_eq!(remove_columns(line_utf8, &(1..=18)), "");
        assert_eq!(remove_columns(line_utf8, &(4..=30)), "ABC");
        assert_eq!(remove_columns(line_utf8, &(9..=30)), "ABCabcął");
        assert_eq!(remove_columns(line_utf8, &(10..=12)), "ABCabcąłć123śę");
        assert_eq!(remove_columns(line_utf8, &(11..=11)), "ABCabcąłć😊😁123śę");
        //start beyond line length leaves the line unchanged
        assert_eq!(remove_columns(line_utf8, &(20..=30)), line_utf8);
    }

    #[test]
    fn test_replace_in_columns() {
        //pos:          123456789012345678901234
        let line_str = "Test01234567891231234567";

        let result = replace_in_columns(line_str, "Test", "Passed", &(1..=usize::MAX));
        assert_eq!(result, "Passed01234567891231234567");

        let result = replace_in_columns(line_str, "123", "ABC", &(1..=usize::MAX));
        assert_eq!(result, "Test0ABC456789ABCABC4567");

        let result = replace_in_columns(line_str, "123", "ABCDEF", &(1..=usize::MAX));
        assert_eq!(result, "Test0ABCDEF456789ABCDEFABCDEF4567");

        let result = replace_in_columns(line_str, "123", "", &(1..=usize::MAX));
        assert_eq!(result, "Test04567894567");

        let result = replace_in_columns(line_str, "123", "ABCD", &(6..=8));
        assert_eq!(result, "Test0ABCD4567891231234567");

        //match only partially inside the range is not replaced
        let result = replace_in_columns(line_str, "123", "ABCD", &(7..=8));
        assert_eq!(result, line_str);
        let result = replace_in_columns(line_str, "123", "ABCD", &(6..=7));
        assert_eq!(result, line_str);

        let result = replace_in_columns(line_str, "123", "ABCD", &(15..=20));
        assert_eq!(result, "Test0123456789ABCDABCD4567");

        //range outside the line or inverted leaves the line unchanged
        let result = replace_in_columns(line_str, "123", "ABCD", &(21..=40));
        assert_eq!(result, line_str);
        let result = replace_in_columns(line_str, "123", "ABCD", &(30..=40));
        assert_eq!(result, line_str);
        //inverted range built explicitly, `10..=5` literal trips clippy
        let result = replace_in_columns(line_str, "123", "ABCD", &RangeInclusive::new(10, 5));
        assert_eq!(result, line_str);
    }
}
