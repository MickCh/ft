use clap::{Arg, ArgAction, Command, crate_name, crate_version};
use std::ops::RangeInclusive;

pub fn cli() -> Command {
    Command::new(crate_name!())
        .version(crate_version!())
        .about("File Transformer")
        .arg(
            Arg::new("rows")
                .short('R')
                .long("rows")
                .required(false)
                .value_parser(parse_range_lines)
                .help("Set range of processed rows"),
        )
        .arg(
            Arg::new("columns")
                .short('C')
                .long("cols")
                .required(false)
                .value_parser(parse_range_lines)
                .help("Set range of processed columns"),
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

fn parse_range_lines(input: &str) -> Result<RangeInclusive<usize>, String> {
    let parts: Vec<&str> = input.split('-').collect();

    if parts.len() != 2 {
        return Err("Invalid range format, expected: <value1>-<value2>".to_owned());
    }

    let from: usize = parts[0]
        .parse()
        .map_err(|_| format!("First value `{input}` isn't a number"))?;

    let to: usize = parts[1]
        .parse()
        .map_err(|_| format!("Second value `{input}` isn't a number"))?;

    if from < 1 {
        return Err("Range is 1-based, start must be at least 1".to_owned());
    }

    if from > to {
        return Err("Range start cannot be greater than its end".to_owned());
    }

    Ok(RangeInclusive::new(from, to))
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
        assert_eq!(parse_range_lines("2-5").unwrap(), 2..=5);
        assert_eq!(parse_range_lines("7-7").unwrap(), 7..=7);
    }

    #[test]
    fn rejects_inverted_range() {
        assert!(parse_range_lines("5-2").is_err());
    }

    #[test]
    fn rejects_zero_start() {
        assert!(parse_range_lines("0-5").is_err());
    }

    #[test]
    fn rejects_non_numeric_values() {
        assert!(parse_range_lines("a-5").is_err());
        assert!(parse_range_lines("1-b").is_err());
    }

    #[test]
    fn rejects_missing_separator() {
        assert!(parse_range_lines("15").is_err());
    }
}
