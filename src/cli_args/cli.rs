use clap::{Arg, ArgAction, ArgGroup, Command, crate_name, crate_version};
use std::ops::RangeInclusive;

use crate::columns::ColumnList;
use crate::ranges::{RangeBound, RangeSpec};

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
                .help("Rows to process: e.g. 3, 2-5, 10-, -5, ~10-~1, or a list like 1-5,10-20"),
        )
        .arg(
            Arg::new("columns")
                .short('C')
                .long("cols")
                .required(false)
                .allow_hyphen_values(true)
                .value_parser(parse_column_list)
                .help("Columns to process: e.g. 3, 2-5, 10-, -5, or a list like 1,3,5-7 (order kept: 3,1,2 permutes)"),
        )
        .arg(
            Arg::new("fields")
                .short('F')
                .long("fields")
                .required(false)
                .requires("column-ranges")
                .value_parser(parse_delimiter)
                .help("Treat the column ranges as fields separated by this delimiter (requires a column range)"),
        )
        //--fields needs some column range to interpret, but any of the
        //three will do, so they form one group it can require
        .group(
            ArgGroup::new("column-ranges")
                .args([
                    "columns",
                    "sort-key",
                    "unique-key",
                    "sum",
                    "avg",
                    "min",
                    "max",
                    "group-by",
                ])
                .multiple(true),
        )
        .arg(
            Arg::new("quoted")
                .long("quoted")
                .required(false)
                .action(ArgAction::SetTrue)
                .requires("fields")
                .help("Respect \"quoted\" fields: a delimiter inside quotes does not split (requires --fields)"),
        )
        .arg(
            Arg::new("output-delimiter")
                .long("output-delimiter")
                .required(false)
                .requires("fields")
                .value_parser(parse_delimiter)
                .help("Join the selected fields with this delimiter instead of the input one (requires --fields)"),
        )
        .arg(
            Arg::new("sort-key")
                .long("sort-key")
                .required(false)
                .allow_hyphen_values(true)
                .requires("sort")
                .value_parser(parse_column_list)
                .help("Columns keying --sort, instead of --cols (requires --sort)"),
        )
        .arg(
            Arg::new("unique-key")
                .long("unique-key")
                .required(false)
                .allow_hyphen_values(true)
                .requires("unique")
                .value_parser(parse_column_list)
                .help("Columns keying --unique, instead of --cols (requires --unique)"),
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
            Arg::new("title-case")
                .long("title-case")
                .required(false)
                .action(ArgAction::SetTrue)
                .conflicts_with_all(["upper", "lower", "delete"])
                .help("Capitalize the first letter of every word in the column range"),
        )
        .arg(
            Arg::new("squeeze")
                .long("squeeze")
                .required(false)
                .action(ArgAction::SetTrue)
                .conflicts_with("delete")
                .help("Collapse runs of whitespace in the column range into single spaces"),
        )
        .arg(
            Arg::new("number")
                .long("number")
                .required(false)
                .action(ArgAction::SetTrue)
                //numbering the rows and then reordering them would shuffle
                //the numbers along with the rows
                .conflicts_with_all(["delete", "sort", "tac", "shuffle"])
                .help("Number the output rows, like nl"),
        )
        .arg(
            Arg::new("split-on")
                .long("split-on")
                .required(false)
                .conflicts_with("delete")
                .value_parser(parse_delimiter)
                .help("Split every line at each occurrence of this separator, one row per piece"),
        )
        .arg(
            Arg::new("count")
                .long("count")
                .required(false)
                .action(ArgAction::SetTrue)
                .help("Summarize: how many rows (per --group-by key, if given)"),
        )
        .arg(
            Arg::new("sum")
                .long("sum")
                .required(false)
                .allow_hyphen_values(true)
                .value_parser(parse_column_list)
                .help("Summarize: the total of the numbers in these columns"),
        )
        .arg(
            Arg::new("avg")
                .long("avg")
                .required(false)
                .allow_hyphen_values(true)
                .value_parser(parse_column_list)
                .help("Summarize: the mean of the numbers in these columns"),
        )
        .arg(
            Arg::new("min")
                .long("min")
                .required(false)
                .allow_hyphen_values(true)
                .value_parser(parse_column_list)
                .help("Summarize: the smallest number in these columns"),
        )
        .arg(
            Arg::new("max")
                .long("max")
                .required(false)
                .allow_hyphen_values(true)
                .value_parser(parse_column_list)
                .help("Summarize: the largest number in these columns"),
        )
        .arg(
            Arg::new("group-by")
                .long("group-by")
                .required(false)
                .allow_hyphen_values(true)
                .requires("summary")
                .value_parser(parse_column_list)
                .help("Summarize once per distinct value of these columns (requires a summary)"),
        )
        //a summary replaces the rows it summarizes, so there is nothing
        //left for --delete to remove or for a reordering to reorder
        .group(
            ArgGroup::new("summary")
                .args(["count", "sum", "avg", "min", "max"])
                .multiple(true)
                .conflicts_with_all(["delete", "sort", "tac", "shuffle"]),
        )
        .arg(
            Arg::new("wrap")
                .long("wrap")
                .required(false)
                .conflicts_with("delete")
                .value_parser(parse_width)
                .help("Wrap every line into chunks of at most this many characters (like fold -w)"),
        )
        .arg(
            Arg::new("drop-empty")
                .long("drop-empty")
                .required(false)
                .action(ArgAction::SetTrue)
                .conflicts_with("delete")
                .help("Drop lines that are empty after the other transforms ran"),
        )
        .arg(
            Arg::new("find")
                .short('f')
                .long("find")
                .required(false)
                .action(ArgAction::Append)
                .help("Set substring to find (repeatable, pairing up with --replace)"),
        )
        .arg(
            Arg::new("replace")
                .short('r')
                .long("replace")
                .required(false)
                .action(ArgAction::Append)
                .help("Set substring to replace (repeatable, one per --find)"),
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
            Arg::new("quiet")
                .short('q')
                .long("quiet")
                .required(false)
                .action(ArgAction::SetTrue)
                .requires("grep")
                .conflicts_with_all(["output", "in-place"])
                .help("Write nothing; exit 0 if any row matched, 1 if none did (requires --grep)"),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .required(false)
                .help("Output filename"),
        )
        .arg(
            Arg::new("in-place")
                .short('i')
                .long("in-place")
                .required(false)
                .action(ArgAction::SetTrue)
                .conflicts_with("output")
                .help("Edit the input file in place instead of writing to stdout"),
        )
        .arg(
            Arg::new("backup")
                .long("backup")
                .required(false)
                .requires("in-place")
                .value_parser(parse_suffix)
                .help("Keep a copy of each edited file, with this suffix (requires --in-place)"),
        )
        .arg(
            Arg::new("dry-run")
                .long("dry-run")
                .required(false)
                .action(ArgAction::SetTrue)
                .requires("in-place")
                .help("Report which files the edit would change, without writing (requires --in-place)"),
        )
        .arg(
            Arg::new("filename")
                .required(false)
                .num_args(1..)
                .help(
                    "Input files, read as one stream (--in-place edits each on its own); \
                     reads standard input when omitted or `-`",
                ),
        )
}

/// Parse a row specification: a comma-separated list of range parts.
fn parse_row_ranges(input: &str) -> Result<RangeSpec, String> {
    let parts = input
        .split(',')
        .map(parse_range_part)
        .collect::<Result<Vec<_>, String>>()?;
    Ok(RangeSpec::new(parts))
}

/// Parse a column specification: a comma-separated list of range parts,
/// kept in the order written so that reading operations can permute the
/// columns. Columns have no end-relative (`~`) bounds: a line's length
/// is known as it is read, but the parts must be comparable before that.
fn parse_column_list(input: &str) -> Result<ColumnList, String> {
    let parts = input
        .split(',')
        .map(parse_column_part)
        .collect::<Result<Vec<_>, String>>()?;
    Ok(ColumnList::new(parts))
}

fn parse_column_part(part: &str) -> Result<RangeInclusive<usize>, String> {
    match parse_range_part(part)? {
        (RangeBound::FromStart(from), RangeBound::FromStart(to)) => Ok(from..=to),
        _ => Err("Columns do not support end-relative (~) values".to_owned()),
    }
}

/// Parse one range part: `<from>-<to>`, `<from>-` (to the end),
/// `-<to>` (from 1) or a single number. A bound prefixed with `~`
/// counts from the end of the input (`~1` is the last row).
fn parse_range_part(part: &str) -> Result<(RangeBound, RangeBound), String> {
    let (from, to) = match *part
        .split('-')
        .collect::<Vec<&str>>()
        .as_slice()
    {
        [single] => {
            let value = parse_bound(single)?;
            (value, value)
        }
        [from, ""] => (parse_bound(from)?, RangeBound::FromStart(usize::MAX)),
        ["", to] => (RangeBound::FromStart(1), parse_bound(to)?),
        [from, to] => (parse_bound(from)?, parse_bound(to)?),
        _ => return Err(format!("Invalid range `{part}`, expected <from>-<to>")),
    };

    //an inverted range is only detectable when both bounds count from
    //the same side; mixed bounds are checked once the input length is known
    let inverted = match (from, to) {
        (RangeBound::FromStart(a), RangeBound::FromStart(b)) => a > b,
        (RangeBound::FromEnd(a), RangeBound::FromEnd(b)) => a < b,
        _ => false,
    };
    if inverted {
        return Err("Range start cannot be greater than its end".to_owned());
    }

    Ok((from, to))
}

fn parse_delimiter(input: &str) -> Result<String, String> {
    if input.is_empty() {
        return Err("The field delimiter cannot be empty".to_owned());
    }
    Ok(input.to_owned())
}

/// Parse a backup suffix: an empty one would name the file itself, so
/// the backup would be the very file about to be overwritten.
fn parse_suffix(input: &str) -> Result<String, String> {
    if input.is_empty() {
        return Err("The backup suffix cannot be empty".to_owned());
    }
    Ok(input.to_owned())
}

/// Parse a wrapping width: a width of 0 would cut the line into
/// infinitely many empty chunks, so at least one char is required.
fn parse_width(input: &str) -> Result<usize, String> {
    let width: usize = input
        .parse()
        .map_err(|_| format!("Width `{input}` isn't a number"))?;
    if width < 1 {
        return Err("The wrapping width must be at least 1".to_owned());
    }
    Ok(width)
}

fn parse_bound(value: &str) -> Result<RangeBound, String> {
    let (make_bound, digits): (fn(usize) -> RangeBound, &str) = match value.strip_prefix('~') {
        Some(rest) => (RangeBound::FromEnd, rest),
        None => (RangeBound::FromStart, value),
    };
    let bound: usize = digits
        .parse()
        .map_err(|_| format!("Range value `{value}` isn't a number"))?;
    if bound < 1 {
        return Err("Ranges are 1-based, values must be at least 1".to_owned());
    }
    Ok(make_bound(bound))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ranges::RangeSet;
    use RangeBound::{FromEnd, FromStart};

    #[test]
    fn cli_definition_is_consistent() {
        cli().debug_assert();
    }

    #[test]
    fn parses_valid_range() {
        assert_eq!(
            parse_range_part("2-5").unwrap(),
            (FromStart(2), FromStart(5))
        );
        assert_eq!(
            parse_range_part("7-7").unwrap(),
            (FromStart(7), FromStart(7))
        );
    }

    #[test]
    fn parses_single_number_as_one_element_range() {
        assert_eq!(
            parse_range_part("15").unwrap(),
            (FromStart(15), FromStart(15))
        );
    }

    #[test]
    fn parses_open_ended_ranges() {
        assert_eq!(
            parse_range_part("10-").unwrap(),
            (FromStart(10), FromStart(usize::MAX))
        );
        assert_eq!(
            parse_range_part("-5").unwrap(),
            (FromStart(1), FromStart(5))
        );
    }

    #[test]
    fn parses_end_relative_bounds() {
        assert_eq!(
            parse_range_part("~10-~1").unwrap(),
            (FromEnd(10), FromEnd(1))
        );
        assert_eq!(parse_range_part("~5").unwrap(), (FromEnd(5), FromEnd(5)));
        assert_eq!(
            parse_range_part("2-~2").unwrap(),
            (FromStart(2), FromEnd(2))
        );
    }

    #[test]
    fn parses_range_list_into_a_spec() {
        let spec = parse_row_ranges("1-2,5-6,4").unwrap();
        assert_eq!(spec.resolve(100), RangeSet::new(vec![1..=2, 4..=6]));
    }

    #[test]
    fn rejects_inverted_range() {
        assert!(parse_range_part("5-2").is_err());
        //~1 is the last line, so ~1-~5 runs backwards too
        assert!(parse_range_part("~1-~5").is_err());
    }

    #[test]
    fn accepts_mixed_bounds_that_may_invert() {
        //whether 5-~5 is inverted depends on the input length
        assert!(parse_range_part("5-~5").is_ok());
    }

    #[test]
    fn rejects_zero_values() {
        assert!(parse_range_part("0-5").is_err());
        assert!(parse_range_part("0").is_err());
        assert!(parse_range_part("~0").is_err());
    }

    #[test]
    fn rejects_non_numeric_values() {
        assert!(parse_range_part("a-5").is_err());
        assert!(parse_range_part("1-b").is_err());
        assert!(parse_range_part("-").is_err());
        assert!(parse_range_part("").is_err());
        assert!(parse_range_part("~").is_err());
    }

    #[test]
    fn rejects_list_with_empty_part() {
        assert!(parse_row_ranges("1-2,").is_err());
    }

    #[test]
    fn column_list_parses_a_single_range() {
        assert_eq!(
            parse_column_list("3-").unwrap(),
            ColumnList::from(3..=usize::MAX)
        );
    }

    #[test]
    fn column_list_keeps_the_parts_in_the_written_order() {
        let list = parse_column_list("3,1,2").unwrap();
        assert_eq!(list.written(), [3..=3, 1..=1, 2..=2]);

        let list = parse_column_list("1,3,5-7").unwrap();
        assert_eq!(list.written(), [1..=1, 3..=3, 5..=7]);
    }

    #[test]
    fn column_list_rejects_end_relative_bounds() {
        assert!(parse_column_list("~5").is_err());
        assert!(parse_column_list("1-~2").is_err());
        assert!(parse_column_list("1,~2").is_err());
    }

    #[test]
    fn column_list_rejects_an_empty_part() {
        assert!(parse_column_list("1,").is_err());
    }
}
