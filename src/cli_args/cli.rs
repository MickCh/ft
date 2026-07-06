use clap::{Arg, ArgAction, Command, crate_name, crate_version};
use std::ops::RangeInclusive;

use crate::ranges::RangeSet;

pub fn cli() -> Command {
    Command::new(crate_name!())
        .version(crate_version!())
        .about("File Transformer")
        .arg(
            Arg::new("rows")
                .short('R')
                .long("rows")
                .required(false)
                .allow_hyphen_values(true)
                .value_parser(parse_row_ranges)
                .help("Rows to process: e.g. 3, 2-5, 10-, -5, or a list like 1-5,10-20"),
        )
        .arg(
            Arg::new("columns")
                .short('C')
                .long("cols")
                .required(false)
                .allow_hyphen_values(true)
                .value_parser(parse_column_range)
                .help("Columns to process: e.g. 3, 2-5, 10- or -5"),
        )
        .arg(
            Arg::new("sort")
                .short('s')
                .long("sort")
                .required(false)
                .action(ArgAction::SetTrue)
                .help("Sort specified region (rows/columns)"),
        )
        .arg(
            Arg::new("numeric")
                .short('n')
                .long("numeric")
                .required(false)
                .action(ArgAction::SetTrue)
                .requires("sort")
                .help("Sort numerically instead of lexicographically (requires --sort)"),
        )
        .arg(
            Arg::new("reverse")
                .long("reverse")
                .required(false)
                .action(ArgAction::SetTrue)
                .requires("sort")
                .help("Sort in descending order (requires --sort)"),
        )
        .arg(
            Arg::new("tac")
                .long("tac")
                .required(false)
                .action(ArgAction::SetTrue)
                .conflicts_with_all(["sort", "shuffle"])
                .help("Reverse the order of the selected rows (like tac)"),
        )
        .arg(
            Arg::new("shuffle")
                .long("shuffle")
                .required(false)
                .action(ArgAction::SetTrue)
                .conflicts_with("sort")
                .help("Shuffle the selected rows into a random order"),
        )
        .arg(
            Arg::new("delete")
                .short('d')
                .long("delete")
                .required(false)
                .action(ArgAction::SetTrue)
                .help("Delete specified region (rows)"),
        )
        .arg(
            Arg::new("unique")
                .short('u')
                .long("unique")
                .required(false)
                .action(ArgAction::SetTrue)
                .conflicts_with("delete")
                .help("Drop duplicate rows, comparing the column range (first wins)"),
        )
        .arg(
            Arg::new("grep")
                .short('g')
                .long("grep")
                .required(false)
                .help("Keep only rows matching this regex (with --delete: delete them)"),
        )
        .arg(
            Arg::new("invert")
                .long("invert")
                .required(false)
                .action(ArgAction::SetTrue)
                .requires("grep")
                .help("Invert the --grep match, like grep -v (requires --grep)"),
        )
        .arg(
            Arg::new("upper")
                .long("upper")
                .required(false)
                .action(ArgAction::SetTrue)
                .conflicts_with_all(["lower", "delete"])
                .help("Convert the column range to uppercase"),
        )
        .arg(
            Arg::new("lower")
                .long("lower")
                .required(false)
                .action(ArgAction::SetTrue)
                .conflicts_with("delete")
                .help("Convert the column range to lowercase"),
        )
        .arg(
            Arg::new("trim")
                .long("trim")
                .required(false)
                .action(ArgAction::SetTrue)
                .conflicts_with("delete")
                .help("Trim whitespace at both ends of the column range"),
        )
        .arg(
            Arg::new("find")
                .short('f')
                .long("find")
                .required(false)
                .help("Set substring to find"),
        )
        .arg(
            Arg::new("replace")
                .short('r')
                .long("replace")
                .required(false)
                .help("Set substring to replace (find is required)"),
        )
        .arg(
            Arg::new("regex")
                .short('e')
                .long("regex")
                .required(false)
                .action(ArgAction::SetTrue)
                .requires("find")
                .help("Treat the find pattern as a regular expression"),
        )
        .arg(
            Arg::new("ignore-case")
                .long("ignore-case")
                .required(false)
                .action(ArgAction::SetTrue)
                .help("Match the find/grep pattern case-insensitively"),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .required(false)
                .help("Output filename"),
        )
        .arg(
            Arg::new("filename")
                .required(false)
                .help("Input file; reads standard input when omitted or `-`"),
        )
}

/// Parse a row specification: a comma-separated list of range parts.
fn parse_row_ranges(input: &str) -> Result<RangeSet, String> {
    let parts = input
        .split(',')
        .map(parse_range_part)
        .collect::<Result<Vec<_>, String>>()?;
    Ok(RangeSet::new(parts))
}

/// Parse a column specification: a single range part (columns address
/// one contiguous slice of the line, so lists are not supported).
fn parse_column_range(input: &str) -> Result<RangeInclusive<usize>, String> {
    if input.contains(',') {
        return Err("Columns accept a single range, not a list".to_owned());
    }
    parse_range_part(input)
}

/// Parse one range part: `<from>-<to>`, `<from>-` (to the end),
/// `-<to>` (from 1) or a single number.
fn parse_range_part(part: &str) -> Result<RangeInclusive<usize>, String> {
    let (from, to) = match *part
        .split('-')
        .collect::<Vec<&str>>()
        .as_slice()
    {
        [single] => {
            let value = parse_bound(single)?;
            (value, value)
        }
        [from, ""] => (parse_bound(from)?, usize::MAX),
        ["", to] => (1, parse_bound(to)?),
        [from, to] => (parse_bound(from)?, parse_bound(to)?),
        _ => return Err(format!("Invalid range `{part}`, expected <from>-<to>")),
    };

    if from > to {
        return Err("Range start cannot be greater than its end".to_owned());
    }

    Ok(from..=to)
}

fn parse_bound(value: &str) -> Result<usize, String> {
    let bound: usize = value
        .parse()
        .map_err(|_| format!("Range value `{value}` isn't a number"))?;
    if bound < 1 {
        return Err("Ranges are 1-based, values must be at least 1".to_owned());
    }
    Ok(bound)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_definition_is_consistent() {
        cli().debug_assert();
    }

    #[test]
    fn parses_valid_range() {
        assert_eq!(parse_range_part("2-5").unwrap(), 2..=5);
        assert_eq!(parse_range_part("7-7").unwrap(), 7..=7);
    }

    #[test]
    fn parses_single_number_as_one_element_range() {
        assert_eq!(parse_range_part("15").unwrap(), 15..=15);
    }

    #[test]
    fn parses_open_ended_ranges() {
        assert_eq!(parse_range_part("10-").unwrap(), 10..=usize::MAX);
        assert_eq!(parse_range_part("-5").unwrap(), 1..=5);
    }

    #[test]
    fn parses_range_list_into_a_set() {
        let set = parse_row_ranges("1-2,5-6,4").unwrap();
        assert_eq!(set, RangeSet::new(vec![1..=2, 4..=6]));
    }

    #[test]
    fn rejects_inverted_range() {
        assert!(parse_range_part("5-2").is_err());
    }

    #[test]
    fn rejects_zero_values() {
        assert!(parse_range_part("0-5").is_err());
        assert!(parse_range_part("0").is_err());
    }

    #[test]
    fn rejects_non_numeric_values() {
        assert!(parse_range_part("a-5").is_err());
        assert!(parse_range_part("1-b").is_err());
        assert!(parse_range_part("-").is_err());
        assert!(parse_range_part("").is_err());
    }

    #[test]
    fn rejects_list_with_empty_part() {
        assert!(parse_row_ranges("1-2,").is_err());
    }

    #[test]
    fn column_range_rejects_lists() {
        assert!(parse_column_range("1-2,4-5").is_err());
        assert_eq!(parse_column_range("3-").unwrap(), 3..=usize::MAX);
    }
}
