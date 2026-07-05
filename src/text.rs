//! Pure text helpers operating on 1-based, inclusive, char-indexed
//! column ranges. Out-of-bounds ranges leave the line unchanged.

use std::ops::RangeInclusive;

/// Split a line into its content and line terminator ("\r\n", "\n" or none).
pub fn split_line_terminator(line: &str) -> (&str, &str) {
    if let Some(content) = line.strip_suffix("\r\n") {
        return (content, "\r\n");
    }
    if let Some(content) = line.strip_suffix('\n') {
        return (content, "\n");
    }
    (line, "")
}

/// Return the characters within the column range.
pub fn substring(line: &str, cols: &RangeInclusive<usize>) -> String {
    let (start, end) = clamp_start(cols);
    let chars: Vec<char> = line.chars().collect();

    if start > chars.len() || start > end {
        return line.to_owned();
    }
    let end = end.min(chars.len());

    chars[(start - 1)..end].iter().collect()
}

/// Return the line with the characters within the column range removed.
pub fn remove_columns(line: &str, cols: &RangeInclusive<usize>) -> String {
    let (start, end) = clamp_start(cols);
    let chars: Vec<char> = line.chars().collect();

    if start > chars.len() || start > end {
        return line.to_owned();
    }
    let end = end.min(chars.len());

    let mut result = String::new();
    result.extend(&chars[..(start - 1)]);
    result.extend(&chars[end..]);
    result
}

/// Apply `map_within` to the part of the line inside the column range,
/// leaving the rest of the line untouched.
pub fn map_columns<F>(line: &str, cols: &RangeInclusive<usize>, map_within: F) -> String
where
    F: FnOnce(&str) -> String,
{
    let (start, end) = clamp_start(cols);
    let chars: Vec<char> = line.chars().collect();

    if start > chars.len() || start > end {
        return line.to_owned();
    }
    let end = end.min(chars.len());

    let before = &chars[..(start - 1)];
    let within: String = chars[(start - 1)..end].iter().collect();
    let after = &chars[end..];

    let mapped = map_within(&within);

    let mut result = String::with_capacity(before.len() + mapped.len() + after.len());
    result.extend(before);
    result.push_str(&mapped);
    result.extend(after);
    result
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

//column numbering is 1-based; treat a 0 start as column 1
fn clamp_start(cols: &RangeInclusive<usize>) -> (usize, usize) {
    ((*cols.start()).max(1), *cols.end())
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
    fn test_substring() {
        //pos:           123456789012345678901
        let line_utf8 = "ABCabcąłć😊😍😁123śę";

        assert_eq!(substring(line_utf8, &(1..=3)), "ABC");
        assert_eq!(substring(line_utf8, &(1..=6)), "ABCabc");
        assert_eq!(substring(line_utf8, &(1..=9)), "ABCabcąłć");
        assert_eq!(substring(line_utf8, &(1..=11)), "ABCabcąłć😊😍");
        assert_eq!(substring(line_utf8, &(1..=16)), "ABCabcąłć😊😍😁123ś");
        assert_eq!(substring(line_utf8, &(1..=17)), "ABCabcąłć😊😍😁123śę");
        assert_eq!(substring(line_utf8, &(1..=18)), "ABCabcąłć😊😍😁123śę");
        assert_eq!(substring(line_utf8, &(1..=30)), "ABCabcąłć😊😍😁123śę");
        assert_eq!(substring(line_utf8, &(4..=30)), "abcąłć😊😍😁123śę");
        assert_eq!(substring(line_utf8, &(9..=30)), "ć😊😍😁123śę");
        assert_eq!(substring(line_utf8, &(10..=12)), "😊😍😁");
        assert_eq!(substring(line_utf8, &(11..=11)), "😍");
        //start beyond line length leaves the line unchanged
        assert_eq!(substring(line_utf8, &(20..=30)), line_utf8);
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
