use std::ops::RangeInclusive;
use std::path::PathBuf;

use clap::ArgMatches;
use regex::{Regex, RegexBuilder};

use super::ConfigError;
use crate::columns::ColumnSpan;
use crate::ranges::RangeSpec;

/// What `--find` matches: a literal substring, or a regular expression
/// when `--regex` is given. The regex is compiled (and therefore
/// validated) while building the `Config`.
#[derive(Debug)]
pub enum FindPattern {
    Literal(String),
    Regex(Regex),
}

//the derived Default (no ranges, no operations, stdin to stdout) is a
//test-only base configuration; real configs always come from TryFrom
#[cfg_attr(test, derive(Default))]
#[derive(Debug)]
pub struct Config {
    pub rows: Option<RangeSpec>,
    pub cols: Option<RangeInclusive<usize>>,
    //`Some` switches `cols` from counting chars to delimited fields
    pub field_delimiter: Option<String>,
    pub sort: bool,
    pub numeric_sort: bool,
    pub reverse_sort: bool,
    pub tac: bool,
    pub shuffle: bool,
    pub delete: bool,
    pub ignore_case: bool,
    pub upper: bool,
    pub lower: bool,
    pub trim: bool,
    pub grep: Option<Regex>,
    pub invert: bool,
    pub unique: bool,
    //`None` means the input comes from stdin
    pub filename: Option<PathBuf>,
    //find/replace pairs are zipped by position and applied in order
    pub finds: Vec<FindPattern>,
    pub replace_strings: Vec<String>,
    pub output_filename: Option<PathBuf>,
    pub in_place: bool,
}

impl Config {
    /// Rows to process; no range provided means every row.
    pub fn rows_or_full(&self) -> RangeSpec {
        self.rows
            .clone()
            .unwrap_or_else(RangeSpec::full)
    }

    /// Columns to process; no range provided means every column.
    pub fn cols_or_full(&self) -> RangeInclusive<usize> {
        self.cols
            .clone()
            .unwrap_or(1..=usize::MAX)
    }

    /// How the column range addresses lines: char positions, or fields
    /// separated by a delimiter when `--fields` was given.
    pub fn col_span(&self) -> ColumnSpan {
        match &self.field_delimiter {
            Some(delimiter) => ColumnSpan::Fields {
                delimiter: delimiter.clone(),
                fields: self.cols_or_full(),
            },
            None => ColumnSpan::Chars(self.cols_or_full()),
        }
    }

    /// Whether some operation claims the column range as its scope or
    /// key (as opposed to a bare `--cols`, which selects the columns).
    pub fn has_column_operation(&self) -> bool {
        self.delete
            || self.sort
            || !self.finds.is_empty()
            || self.upper
            || self.lower
            || self.trim
            || self.grep.is_some()
            || self.unique
    }
}

impl TryFrom<ArgMatches> for Config {
    type Error = ConfigError;

    /// Build a `Config` from parsed arguments, validating the rules that
    /// span multiple arguments. Single-argument validity (range format,
    /// 1-based bounds) is enforced earlier, by the clap value parsers.
    fn try_from(matches: ArgMatches) -> Result<Config, ConfigError> {
        let ignore_case = matches.get_flag("ignore-case");
        let grep = matches
            .get_one::<String>("grep")
            .map(|pattern| {
                RegexBuilder::new(pattern)
                    .case_insensitive(ignore_case)
                    .build()
                    .map_err(|e| ConfigError::InvalidRegex(e.to_string()))
            })
            .transpose()?;
        let regex_mode = matches.get_flag("regex");
        let finds = matches
            .get_many::<String>("find")
            .into_iter()
            .flatten()
            .map(|pattern| {
                if regex_mode {
                    Ok(FindPattern::Regex(
                        RegexBuilder::new(pattern)
                            .case_insensitive(ignore_case)
                            .build()
                            .map_err(|e| ConfigError::InvalidRegex(e.to_string()))?,
                    ))
                } else {
                    Ok(FindPattern::Literal(pattern.clone()))
                }
            })
            .collect::<Result<Vec<FindPattern>, ConfigError>>()?;

        let config = Config {
            rows: matches
                .get_one::<RangeSpec>("rows")
                .cloned(),
            cols: matches
                .get_one::<RangeInclusive<usize>>("columns")
                .cloned(),
            field_delimiter: matches
                .get_one::<String>("fields")
                .cloned(),
            sort: matches.get_flag("sort"),
            numeric_sort: matches.get_flag("numeric"),
            reverse_sort: matches.get_flag("reverse"),
            tac: matches.get_flag("tac"),
            shuffle: matches.get_flag("shuffle"),
            delete: matches.get_flag("delete"),
            ignore_case,
            upper: matches.get_flag("upper"),
            lower: matches.get_flag("lower"),
            trim: matches.get_flag("trim"),
            grep,
            invert: matches.get_flag("invert"),
            unique: matches.get_flag("unique"),
            filename: matches
                .get_one::<String>("filename")
                .filter(|name| name.as_str() != "-")
                .map(PathBuf::from),
            finds,
            replace_strings: matches
                .get_many::<String>("replace")
                .into_iter()
                .flatten()
                .cloned()
                .collect(),
            output_filename: matches
                .get_one::<String>("output")
                .map(PathBuf::from),
            in_place: matches.get_flag("in-place"),
        };

        //--find and --replace pair up positionally, so their counts must
        //match; a lone --find (or --replace) has no partner and is rejected
        //rather than silently ignored
        match (config.finds.len(), config.replace_strings.len()) {
            (0, 0) => {}
            (0, _) => return Err(ConfigError::MissingFindForReplace),
            (_, 0) => return Err(ConfigError::MissingReplaceForFind),
            (finds, replaces) if finds != replaces => {
                return Err(ConfigError::FindReplaceCountMismatch { finds, replaces });
            }
            _ => {}
        }

        if !config.finds.is_empty() && config.delete {
            return Err(ConfigError::ReplaceWithDelete);
        }

        if config.delete && config.rows.is_none() && config.cols.is_none() && config.grep.is_none()
        {
            return Err(ConfigError::DeleteWithoutRange);
        }

        //deleting whole rows and reordering them is contradictory; with a
        //column range `--delete` removes columns, so reordering is fine
        if config.delete && config.cols.is_none() && (config.sort || config.tac || config.shuffle) {
            return Err(ConfigError::DeleteWithReorder);
        }

        if config.ignore_case && config.finds.is_empty() && config.grep.is_none() {
            return Err(ConfigError::IgnoreCaseWithoutPattern);
        }

        if config.in_place && config.filename.is_none() {
            return Err(ConfigError::InPlaceWithoutFile);
        }

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli_args::cli;

    fn config_from(args: &[&str]) -> Result<Config, ConfigError> {
        let matches = cli()
            .try_get_matches_from(args)
            .expect("clap parsing failed");
        Config::try_from(matches)
    }

    #[test]
    fn defaults_when_only_filename_given() {
        let config = config_from(&["ft", "input.txt"]).unwrap();

        assert_eq!(config.filename, Some(PathBuf::from("input.txt")));
        assert!(config.rows.is_none());
        assert!(config.cols.is_none());
        assert!(!config.sort);
        assert!(!config.delete);
        assert!(config.finds.is_empty());
        assert!(config.replace_strings.is_empty());
        assert!(config.output_filename.is_none());
    }

    #[test]
    fn reads_all_arguments() {
        let config = config_from(&[
            "ft",
            "-R",
            "2-4",
            "-C",
            "1-10",
            "-s",
            "-f",
            "a",
            "-r",
            "b",
            "-o",
            "out.txt",
            "input.txt",
        ])
        .unwrap();

        assert_eq!(config.rows, Some(RangeSpec::from(2..=4)));
        assert_eq!(config.cols, Some(1..=10));
        assert!(config.sort);
        assert!(matches!(config.finds.as_slice(), [FindPattern::Literal(f)] if f == "a"));
        assert_eq!(config.replace_strings, ["b"]);
        assert_eq!(config.output_filename, Some(PathBuf::from("out.txt")));
    }

    #[test]
    fn omitted_or_dash_filename_means_stdin() {
        let config = config_from(&["ft"]).unwrap();
        assert!(config.filename.is_none());

        let config = config_from(&["ft", "-"]).unwrap();
        assert!(config.filename.is_none());
    }

    #[test]
    fn missing_ranges_fall_back_to_full_range() {
        let config = config_from(&["ft", "input.txt"]).unwrap();

        assert_eq!(config.rows_or_full(), RangeSpec::full());
        assert_eq!(config.cols_or_full(), 1..=usize::MAX);
    }

    #[test]
    fn fields_delimiter_switches_the_span_to_field_mode() {
        let config = config_from(&["ft", "-F", ",", "-C", "2-3", "input.txt"]).unwrap();

        assert_eq!(config.field_delimiter.as_deref(), Some(","));
        assert!(matches!(config.col_span(), ColumnSpan::Fields { .. }));

        let config = config_from(&["ft", "-C", "2-3", "input.txt"]).unwrap();
        assert!(matches!(config.col_span(), ColumnSpan::Chars(_)));
    }

    #[test]
    fn fields_requires_columns() {
        assert!(
            cli()
                .try_get_matches_from(["ft", "-F", ",", "input.txt"])
                .is_err()
        );
    }

    #[test]
    fn fields_rejects_an_empty_delimiter() {
        assert!(
            cli()
                .try_get_matches_from(["ft", "-F", "", "-C", "2", "input.txt"])
                .is_err()
        );
    }

    #[test]
    fn numeric_and_reverse_require_sort() {
        assert!(
            cli()
                .try_get_matches_from(["ft", "-n", "input.txt"])
                .is_err()
        );
        assert!(
            cli()
                .try_get_matches_from(["ft", "--reverse", "input.txt"])
                .is_err()
        );

        let config = config_from(&["ft", "-s", "-n", "--reverse", "input.txt"]).unwrap();
        assert!(config.sort && config.numeric_sort && config.reverse_sort);
    }

    #[test]
    fn regex_flag_compiles_find_as_regex() {
        let config = config_from(&["ft", "-e", "-f", r"\d+", "-r", "N", "input.txt"]).unwrap();
        assert!(matches!(config.finds.as_slice(), [FindPattern::Regex(_)]));
    }

    #[test]
    fn collects_multiple_find_replace_pairs_in_order() {
        let config = config_from(&[
            "ft",
            "-f",
            "a",
            "-r",
            "1",
            "-f",
            "b",
            "-r",
            "2",
            "input.txt",
        ])
        .unwrap();

        assert!(
            matches!(config.finds.as_slice(), [FindPattern::Literal(x), FindPattern::Literal(y)] if x == "a" && y == "b")
        );
        assert_eq!(config.replace_strings, ["1", "2"]);
    }

    #[test]
    fn rejects_more_finds_than_replaces() {
        let error = config_from(&["ft", "-f", "a", "-r", "1", "-f", "b", "input.txt"]).unwrap_err();
        assert!(matches!(
            error,
            ConfigError::FindReplaceCountMismatch {
                finds: 2,
                replaces: 1
            }
        ));
    }

    #[test]
    fn rejects_find_without_replace() {
        //a lone --find does nothing on its own; --grep is for filtering
        let error = config_from(&["ft", "-f", "a", "input.txt"]).unwrap_err();
        assert!(matches!(error, ConfigError::MissingReplaceForFind));
    }

    #[test]
    fn ignore_case_flag_is_read() {
        let config =
            config_from(&["ft", "--ignore-case", "-f", "a", "-r", "b", "input.txt"]).unwrap();
        assert!(config.ignore_case);
    }

    #[test]
    fn ignore_case_makes_regex_case_insensitive() {
        let config = config_from(&[
            "ft",
            "-e",
            "--ignore-case",
            "-f",
            "abc",
            "-r",
            "x",
            "input.txt",
        ])
        .unwrap();
        let [FindPattern::Regex(pattern)] = config.finds.as_slice() else {
            panic!("expected a regex find pattern");
        };
        assert!(pattern.is_match("ABC"));
    }

    #[test]
    fn ignore_case_requires_find_or_grep() {
        let error = config_from(&["ft", "--ignore-case", "input.txt"]).unwrap_err();
        assert!(matches!(error, ConfigError::IgnoreCaseWithoutPattern));

        assert!(config_from(&["ft", "--ignore-case", "-g", "a", "input.txt"]).is_ok());
    }

    #[test]
    fn grep_compiles_as_regex() {
        let config = config_from(&["ft", "-g", "a+b", "input.txt"]).unwrap();
        assert!(config.grep.unwrap().is_match("aaab"));
    }

    #[test]
    fn grep_honors_ignore_case() {
        let config = config_from(&["ft", "--ignore-case", "-g", "abc", "input.txt"]).unwrap();
        assert!(config.grep.unwrap().is_match("ABC"));
    }

    #[test]
    fn rejects_invalid_grep_regex() {
        let error = config_from(&["ft", "-g", "[unclosed", "input.txt"]).unwrap_err();
        assert!(matches!(error, ConfigError::InvalidRegex(_)));
    }

    #[test]
    fn invert_requires_grep() {
        assert!(
            cli()
                .try_get_matches_from(["ft", "--invert", "input.txt"])
                .is_err()
        );
    }

    #[test]
    fn accepts_delete_with_grep_only() {
        assert!(config_from(&["ft", "-d", "-g", "foo", "input.txt"]).is_ok());
    }

    #[test]
    fn rejects_invalid_regex() {
        let error =
            config_from(&["ft", "-e", "-f", "[unclosed", "-r", "N", "input.txt"]).unwrap_err();
        assert!(matches!(error, ConfigError::InvalidRegex(_)));
    }

    #[test]
    fn regex_flag_requires_find() {
        assert!(
            cli()
                .try_get_matches_from(["ft", "-e", "input.txt"])
                .is_err()
        );
    }

    #[test]
    fn rejects_replace_without_find() {
        let error = config_from(&["ft", "-r", "b", "input.txt"]).unwrap_err();
        assert!(matches!(error, ConfigError::MissingFindForReplace));
    }

    #[test]
    fn rejects_replace_with_delete() {
        let error = config_from(&["ft", "-d", "-f", "a", "-r", "b", "input.txt"]).unwrap_err();
        assert!(matches!(error, ConfigError::ReplaceWithDelete));
    }

    #[test]
    fn rejects_delete_without_any_range() {
        let error = config_from(&["ft", "-d", "input.txt"]).unwrap_err();
        assert!(matches!(error, ConfigError::DeleteWithoutRange));
    }

    #[test]
    fn rejects_delete_combined_with_reorder() {
        for reorder in ["-s", "--tac", "--shuffle"] {
            let error = config_from(&["ft", "-d", reorder, "-R", "2", "input.txt"]).unwrap_err();
            assert!(
                matches!(error, ConfigError::DeleteWithReorder),
                "expected DeleteWithReorder for {reorder}, got {error:?}"
            );
        }
    }

    #[test]
    fn allows_delete_with_reorder_when_deleting_columns() {
        //with --cols, --delete removes columns, so sorting the rows is fine
        assert!(config_from(&["ft", "-d", "-s", "-C", "1", "-R", "2", "input.txt"]).is_ok());
    }

    #[test]
    fn accepts_delete_with_row_or_column_range() {
        assert!(config_from(&["ft", "-d", "-R", "2-3", "input.txt"]).is_ok());
        assert!(config_from(&["ft", "-d", "-C", "2-3", "input.txt"]).is_ok());
    }

    #[test]
    fn in_place_flag_is_read() {
        let config = config_from(&["ft", "-i", "input.txt"]).unwrap();
        assert!(config.in_place);
    }

    #[test]
    fn in_place_requires_a_file() {
        let error = config_from(&["ft", "-i"]).unwrap_err();
        assert!(matches!(error, ConfigError::InPlaceWithoutFile));
    }

    #[test]
    fn in_place_conflicts_with_output() {
        assert!(
            cli()
                .try_get_matches_from(["ft", "-i", "-o", "out.txt", "input.txt"])
                .is_err()
        );
    }
}
