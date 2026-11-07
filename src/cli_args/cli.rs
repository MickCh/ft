use crate::codec::Codec;
use clap::{Arg, ArgAction, ArgGroup, Command, crate_name, crate_version, value_parser};
use clap_complete::Shell;
use std::ops::RangeInclusive;

use crate::columns::ColumnList;
use crate::encoding;
use crate::ranges::{RangeBound, RangeSpec};
use encoding_rs::Encoding;

pub fn cli() -> Command {
    Command::new(crate_name!())
        .version(crate_version!())
        .about("File Transformer")
        .after_help(
            "Examples:\n\
             \x20 fxt -R 1-5 -C 2-4 input.txt             keep rows 1-5, columns 2-4\n\
             \x20 fxt -R '~10-~1' input.txt               the last ten rows, like tail\n\
             \x20 fxt -F , -C 3,1 data.csv                CSV fields 3 and 1, in that order\n\
             \x20 fxt -F , --quoted -C 2 data.csv         real CSV: commas inside \"quotes\" stay\n\
             \x20 fxt -F , -C 2 --grep-key 1 -g X a.csv   field 2 of the rows whose field 1 has X\n\
             \x20 fxt -F , --group-by 1 --sum 2 data.csv  sum field 2 per distinct field 1\n\
             \x20 fxt -s -n --reverse -F , -C 3 data.csv  sort by field 3, largest first\n\
             \x20 fxt --from '^\\[db' --to '^\\[' a.ini     the [db] section, through the next header\n\
             \x20 fxt -g ERROR -d -i --backup .bak a.log  delete matching rows in place,\n\
             \x20                                        keeping a backup\n\
             \n\
             Exit codes (like grep):\n\
             \x20 0  rows matched (a run without a content filter always matches)\n\
             \x20 1  a content filter (--grep, --from, --to, --min-fields,\n\
             \x20    --max-len) matched nothing\n\
             \x20 2  the run failed",
        )
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
        //--fields needs something that counts in fields — a column range,
        //or --min-fields — but any of them will do, so they form one
        //group it can require
        .group(
            ArgGroup::new("column-ranges")
                .args([
                    "columns",
                    "sort-key",
                    "unique-key",
                    "grep-key",
                    "sum",
                    "avg",
                    "min",
                    "max",
                    "median",
                    "distinct",
                    "first",
                    "last",
                    "group-by",
                    "min-fields",
                ])
                .multiple(true),
        )
        .arg(
            Arg::new("quoted")
                .long("quoted")
                .required(false)
                .action(ArgAction::SetTrue)
                .requires("fields")
                .help("Respect \"quoted\" fields: a delimiter inside quotes does not split, a newline does not end the record (requires --fields)"),
        )
        .arg(
            Arg::new("graphemes")
                .long("graphemes")
                .required(false)
                .action(ArgAction::SetTrue)
                .help(
                    "Count columns and --wrap widths in user-perceived characters \
                     (grapheme clusters): an accented letter or a joined emoji is one column",
                ),
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
            Arg::new("grep-key")
                .long("grep-key")
                .required(false)
                .allow_hyphen_values(true)
                .requires("grep")
                .value_parser(parse_column_list)
                .help("Columns --grep matches in, instead of --cols (requires --grep)"),
        )
        .arg(
            Arg::new("sort")
                .short('s')
                .long("sort")
                .required(false)
                .action(ArgAction::SetTrue)
                .help("Sort the selected rows, using the column range as the sort key"),
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
                .help("Delete the selected rows, or the column range within them"),
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
                .action(ArgAction::Append)
                .help("Keep only rows matching this regex (with --delete: delete them); repeatable, any matches unless --all"),
        )
        .arg(
            Arg::new("keep-others")
                .long("keep-others")
                .required(false)
                .action(ArgAction::SetTrue)
                .conflicts_with_all(["delete", "number", "join", "summary"])
                .help("Keep the rows the selection leaves out, unchanged, instead of dropping them (like sed '2,5s/a/b/')"),
        )
        .arg(
            Arg::new("all")
                .long("all")
                .required(false)
                .action(ArgAction::SetTrue)
                .requires("grep")
                .help("With several --grep, a row must match every one of them (requires --grep)"),
        )
        .arg(
            Arg::new("min-fields")
                .long("min-fields")
                .required(false)
                .value_name("N")
                .requires("fields")
                .value_parser(parse_width)
                .help("Keep only rows with at least N fields (requires --fields)"),
        )
        .arg(
            Arg::new("max-len")
                .long("max-len")
                .required(false)
                .value_name("N")
                .value_parser(value_parser!(usize))
                .help("Keep only rows at most N characters long"),
        )
        .arg(
            Arg::new("from")
                .long("from")
                .required(false)
                .allow_hyphen_values(true)
                .value_name("REGEX")
                .help("Rows from each line matching REGEX (inclusive; like sed '/A/,/B/')"),
        )
        .arg(
            Arg::new("to")
                .long("to")
                .required(false)
                .allow_hyphen_values(true)
                .value_name("REGEX")
                .help("...through the next line matching REGEX (inclusive); alone: from line 1"),
        )
        //what --quiet can ask about: a filter by content
        .group(
            ArgGroup::new("content-filters")
                .args(["grep", "from", "to", "min-fields", "max-len"])
                .multiple(true),
        )
        .arg(
            Arg::new("invert")
                .short('v')
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
                .help("Capitalize every word in the column range and lowercase the rest of it (wORLD -> World)"),
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
            Arg::new("encode")
                .long("encode")
                .required(false)
                .value_name("CODEC")
                .conflicts_with("delete")
                .value_parser(parse_codec)
                .help("Encode the column range as base64, hex or url (percent encoding); each field on its own"),
        )
        .arg(
            Arg::new("decode")
                .long("decode")
                .required(false)
                .value_name("CODEC")
                .conflicts_with("delete")
                .value_parser(parse_codec)
                .help("Decode the column range from base64, hex or url; each field on its own"),
        )
        .arg(
            Arg::new("expand")
                .long("expand")
                .required(false)
                .value_name("TABSIZE")
                .num_args(0..=1)
                .require_equals(true)
                .default_missing_value("8")
                .conflicts_with_all(["delete", "unexpand"])
                .value_parser(parse_width)
                .help("Turn tabs in the column range into spaces, with a tab stop every TABSIZE columns (default 8; like expand)"),
        )
        .arg(
            Arg::new("unexpand")
                .long("unexpand")
                .required(false)
                .value_name("TABSIZE")
                .num_args(0..=1)
                .require_equals(true)
                .default_missing_value("8")
                .conflicts_with("delete")
                .value_parser(parse_width)
                .help("Turn runs of spaces in the column range that reach a tab stop into tabs (default 8; like unexpand -a)"),
        )
        .arg(
            Arg::new("reverse-chars")
                .long("reverse-chars")
                .required(false)
                .action(ArgAction::SetTrue)
                .conflicts_with("delete")
                .help("Reverse the characters in the column range, like rev"),
        )
        .arg(
            Arg::new("pad-left")
                .long("pad-left")
                .required(false)
                .value_name("WIDTH")
                .conflicts_with_all(["delete", "pad-right"])
                .value_parser(parse_width)
                .help("Pad the column range with spaces on the left to this width (right-aligns it; each field on its own)"),
        )
        .arg(
            Arg::new("pad-right")
                .long("pad-right")
                .required(false)
                .value_name("WIDTH")
                .conflicts_with("delete")
                .value_parser(parse_width)
                .help("Pad the column range with spaces on the right to this width (left-aligns it; each field on its own)"),
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
            Arg::new("join")
                .long("join")
                .required(false)
                //a reducer consumes the rows, and there is only one set of
                //them: --join and a summary cannot both have it. Numbering
                //them would number rows nobody ever sees
                .conflicts_with_all([
                    "count", "sum", "avg", "min", "max", "median", "distinct", "first", "last",
                    "delete", "number",
                ])
                .value_parser(parse_delimiter)
                .help("Join every row into a single row, separated by this (like paste -s)"),
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
            Arg::new("median")
                .long("median")
                .required(false)
                .allow_hyphen_values(true)
                .value_parser(parse_column_list)
                .help("Summarize: the middle number in these columns (the mean of the two middle ones)"),
        )
        .arg(
            Arg::new("distinct")
                .long("distinct")
                .required(false)
                .allow_hyphen_values(true)
                .value_parser(parse_column_list)
                .help("Summarize: how many different values these columns hold"),
        )
        .arg(
            Arg::new("first")
                .long("first")
                .required(false)
                .allow_hyphen_values(true)
                .value_parser(parse_column_list)
                .help("Summarize: these columns in the first row (of each --group-by group)"),
        )
        .arg(
            Arg::new("last")
                .long("last")
                .required(false)
                .allow_hyphen_values(true)
                .value_parser(parse_column_list)
                .help("Summarize: these columns in the last row (of each --group-by group)"),
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
        //left for --delete to remove, nor any rows to number. Reordering
        //is fine: a reducer takes the rows in the order they come out, so
        //`--sort --group-by` reports the groups in sorted order
        .group(
            ArgGroup::new("summary")
                .args([
                    "count", "sum", "avg", "min", "max", "median", "distinct", "first", "last",
                ])
                .multiple(true)
                .conflicts_with_all(["delete", "number"]),
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
                .help("Substring to find (repeatable, paired with --replace)"),
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
            Arg::new("perl-regexp")
                .short('P')
                .long("perl-regexp")
                .required(false)
                .action(ArgAction::SetTrue)
                .help(
                    "Let every regex (--grep, --regex --find, --from, --to) use backreferences \
                     like (.)\\1 and lookaround like (?<=x); matched by backtracking, \
                     without the default engine's linear-time guarantee",
                ),
        )
        .arg(
            Arg::new("ignore-case")
                .long("ignore-case")
                .required(false)
                .action(ArgAction::SetTrue)
                .help("Match the --find, --grep, --from and --to patterns case-insensitively"),
        )
        .arg(
            Arg::new("quiet")
                .short('q')
                .long("quiet")
                .required(false)
                .action(ArgAction::SetTrue)
                .requires("content-filters")
                .conflicts_with_all(["output", "in-place"])
                .help(
                    "Write nothing; exit 0 if any row matched, 1 if none did \
                     (requires a content filter: --grep, --from, --to, --min-fields or --max-len)",
                ),
        )
        .arg(
            Arg::new("output")
                .short('o')
                .long("output")
                .required(false)
                .help("Output filename"),
        )
        .arg(
            Arg::new("encoding")
                .long("encoding")
                .required(false)
                .value_name("ENCODING")
                .value_parser(parse_encoding)
                .help(
                    "Encoding of the inputs, e.g. windows-1250, latin1, utf-16le \
                     (default utf-8; a UTF-16 or UTF-8 byte order mark overrides it)",
                ),
        )
        .arg(
            Arg::new("output-encoding")
                .long("output-encoding")
                .required(false)
                .value_name("ENCODING")
                .value_parser(parse_output_encoding)
                .help(
                    "Encoding of the output (default utf-8; with --in-place, \
                     each file's own encoding)",
                ),
        )
        .arg(
            Arg::new("lossy")
                .long("lossy")
                .required(false)
                .action(ArgAction::SetTrue)
                .help(
                    "Replace bytes that are not valid in the input encoding with U+FFFD \
                     instead of failing",
                ),
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
        //output about the tool itself rather than a transformation, so
        //neither combines with anything else
        .arg(
            Arg::new("completions")
                .long("completions")
                .value_name("SHELL")
                .exclusive(true)
                .value_parser(value_parser!(Shell))
                .help("Print a completion script for SHELL (bash, zsh, fish, powershell, elvish) and exit"),
        )
        .arg(
            Arg::new("man")
                .long("man")
                .exclusive(true)
                .action(ArgAction::SetTrue)
                .help("Print the man page (roff) and exit"),
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
/// Parse an encoding label, as the WHATWG Encoding Standard names them
/// (`windows-1250`, `latin1`, `utf-16le`, `shift_jis`, …).
fn parse_encoding(input: &str) -> Result<&'static Encoding, String> {
    Encoding::for_label(input.as_bytes()).ok_or_else(|| format!("Unknown encoding `{input}`"))
}

/// Parse an encoding the output can be written in.
fn parse_output_encoding(input: &str) -> Result<&'static Encoding, String> {
    let encoding = parse_encoding(input)?;
    match encoding::can_encode(encoding) {
        true => Ok(encoding),
        false => Err(format!("Text cannot be written in `{input}`")),
    }
}

fn parse_suffix(input: &str) -> Result<String, String> {
    if input.is_empty() {
        return Err("The backup suffix cannot be empty".to_owned());
    }
    Ok(input.to_owned())
}

/// Parse a codec name (`--encode`, `--decode`).
fn parse_codec(input: &str) -> Result<Codec, String> {
    [Codec::Base64, Codec::Hex, Codec::Url]
        .into_iter()
        .find(|codec| codec.name().eq_ignore_ascii_case(input))
        .ok_or_else(|| format!("Unknown codec `{input}`: expected base64, hex or url"))
}

/// Parse a width (`--wrap`, `--pad-left`, `--pad-right`, a tab size,
/// `--min-fields`): a wrapping
/// width of 0 would cut the line into infinitely many empty chunks, and
/// padding to 0 would do nothing, so at least one char is required.
fn parse_width(input: &str) -> Result<usize, String> {
    let width: usize = input
        .parse()
        .map_err(|_| format!("Width `{input}` isn't a number"))?;
    if width < 1 {
        return Err("The width must be at least 1".to_owned());
    }
    Ok(width)
}

fn parse_bound(value: &str) -> Result<RangeBound, String> {
    let (make_bound, digits): (fn(usize) -> RangeBound, &str) = match value.strip_prefix('~') {
        Some(rest) => (RangeBound::FromEnd, rest),
        None => (RangeBound::FromStart, value),
    };
    //digits only: `usize::from_str` would take a leading `+` as well
    let bound: usize = digits
        .bytes()
        .all(|byte| byte.is_ascii_digit())
        .then(|| digits.parse().ok())
        .flatten()
        .ok_or_else(|| format!("Range value `{value}` isn't a number"))?;
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
    fn a_sign_is_no_part_of_a_number() {
        // Given / When / Then
        assert!(parse_row_ranges("+2").is_err());
        assert!(parse_row_ranges("~+1").is_err());
        assert!(parse_column_list("1-+3").is_err());
    }

    #[test]
    fn column_list_rejects_an_empty_part() {
        assert!(parse_column_list("1,").is_err());
    }
}

/// The range parsers fuzzed: whatever the input, they answer rather than
/// panic, and a spec they accept means what it says.
#[cfg(test)]
mod properties {
    use super::*;
    use proptest::prelude::*;

    /// One bound as a user writes it: a row number, or `~k` from the end.
    #[derive(Debug, Clone, Copy)]
    enum Bound {
        Start(usize),
        End(usize),
    }

    impl Bound {
        fn text(self) -> String {
            match self {
                Bound::Start(n) => n.to_string(),
                Bound::End(k) => format!("~{k}"),
            }
        }

        /// The row it names, by definition: `~1` is the last of `total`.
        fn row(self, total: usize) -> i64 {
            match self {
                Bound::Start(n) => n as i64,
                Bound::End(k) => total as i64 + 1 - k as i64,
            }
        }
    }

    /// One part of a row spec, in each of the forms it may take.
    #[derive(Debug, Clone, Copy)]
    enum Part {
        Single(Bound),
        Between(Bound, Bound),
        From(Bound),
        To(Bound),
    }

    impl Part {
        fn text(self) -> String {
            match self {
                Part::Single(b) => b.text(),
                Part::Between(a, b) => format!("{}-{}", a.text(), b.text()),
                Part::From(a) => format!("{}-", a.text()),
                Part::To(b) => format!("-{}", b.text()),
            }
        }

        /// Whether `row` of `total` lies in the part.
        fn holds(self, row: usize, total: usize) -> bool {
            let row = row as i64;
            let (from, to) = match self {
                Part::Single(b) => (b.row(total), b.row(total)),
                Part::Between(a, b) => (a.row(total), b.row(total)),
                Part::From(a) => (a.row(total), i64::MAX),
                Part::To(b) => (1, b.row(total)),
            };
            from <= row && row <= to
        }

        /// Inverted with both bounds on the same side, which the parser
        /// rejects; mixed sides only invert once the length is known.
        fn inverted(self) -> bool {
            match self {
                Part::Between(Bound::Start(a), Bound::Start(b)) => a > b,
                Part::Between(Bound::End(a), Bound::End(b)) => a < b,
                _ => false,
            }
        }
    }

    fn bounds() -> impl Strategy<Value = Bound> {
        prop_oneof![
            (1usize..30).prop_map(Bound::Start),
            (1usize..30).prop_map(Bound::End)
        ]
    }

    fn parts() -> impl Strategy<Value = Part> {
        prop_oneof![
            bounds().prop_map(Part::Single),
            (bounds(), bounds()).prop_map(|(a, b)| Part::Between(a, b)),
            bounds().prop_map(Part::From),
            bounds().prop_map(Part::To),
        ]
    }

    /// Text made of what range specs are made of, and of what they are not.
    fn nasty() -> impl Strategy<Value = String> {
        let pieces = prop::sample::select(vec![
            "0",
            "1",
            "7",
            "42",
            "-",
            "~",
            ",",
            " ",
            "+",
            "~0",
            "--",
            ",,",
            "ż",
            "18446744073709551616",
            "99999999999999999999999",
        ]);
        prop::collection::vec(pieces, 0..12).prop_map(|pieces| pieces.concat())
    }

    proptest! {
        #[test]
        fn row_parsing_never_panics(input in prop_oneof![nasty(), any::<String>()], total in 0usize..50) {
            // Given / When anything at all is parsed
            if let Ok(spec) = parse_row_ranges(&input) {
                // Then only spec characters got through, and whatever was
                // accepted resolves for any length
                prop_assert!(input.chars().all(|c| c.is_ascii_digit() || "~-,".contains(c)), "{:?}", input);
                let rows = spec.resolve(total);
                prop_assert!(rows.parts().iter().all(|part| *part.start() >= 1));
                if spec.is_absolute() {
                    let _ = spec.resolve(usize::MAX);
                }
            }
        }

        #[test]
        fn column_parsing_never_panics(input in prop_oneof![nasty(), any::<String>()]) {
            // Given / When / Then it answers, and never with a `~`
            if let Ok(columns) = parse_column_list(&input) {
                prop_assert!(input.chars().all(|c| c.is_ascii_digit() || "-,".contains(c)), "{:?}", input);
                prop_assert!(columns.written().iter().all(|part| part.start() <= part.end()));
            }
        }

        #[test]
        fn a_row_spec_selects_the_rows_it_names(
            spec in prop::collection::vec(parts(), 1..5),
            total in 0usize..40,
        ) {
            // Given a spec written out the way a user would
            let text: Vec<String> = spec.iter().map(|part| part.text()).collect();
            let text = text.join(",");

            // When
            let parsed = parse_row_ranges(&text);

            // Then an inversion on one side is refused, all else means
            // exactly the rows the definition names
            if spec.iter().any(|part| part.inverted()) {
                prop_assert!(parsed.is_err(), "{text} should be refused");
            } else {
                let rows = parsed.map_err(|e| TestCaseError::fail(format!("{text}: {e}")))?.resolve(total);
                for row in 1..=total + 2 {
                    let expected = spec.iter().any(|part| part.holds(row, total));
                    prop_assert_eq!(rows.contains(row), expected, "{} row {} of {}", text, row, total);
                }
            }
        }

        #[test]
        fn a_column_list_normalizes_to_what_it_covers(
            parts in prop::collection::vec((1usize..40, 0usize..10), 1..6),
        ) {
            // Given parts written in any order, overlapping or not
            let text: Vec<String> = parts.iter().map(|(from, length)| format!("{from}-{}", from + length)).collect();
            let columns = parse_column_list(&text.join(",")).unwrap();

            // When / Then the written order is kept, and the normalized
            // parts ascend, never touch, and cover the same columns
            prop_assert_eq!(columns.written().len(), parts.len());
            let normalized = columns.normalized();
            prop_assert!(normalized.windows(2).all(|pair| pair[0].end() + 1 < *pair[1].start()));
            for column in 1..60 {
                let written = parts.iter().any(|(from, length)| (*from..=from + length).contains(&column));
                let covered = normalized.iter().any(|part| part.contains(&column));
                prop_assert_eq!(written, covered, "column {}", column);
            }
        }
    }
}
