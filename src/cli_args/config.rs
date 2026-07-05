use std::ops::RangeInclusive;
use std::path::PathBuf;

use clap::ArgMatches;

use super::ConfigError;

#[derive(Debug)]
pub struct Config {
    pub rows: Option<RangeInclusive<usize>>,
    pub cols: Option<RangeInclusive<usize>>,
    pub sort: bool,
    pub delete: bool,
    //`None` means the input comes from stdin
    pub filename: Option<PathBuf>,
    pub find_string: Option<String>,
    pub replace_string: Option<String>,
    pub output_filename: Option<PathBuf>,
}

impl Config {
    /// Rows to process; no range provided means every row.
    pub fn rows_or_full(&self) -> RangeInclusive<usize> {
        self.rows
            .clone()
            .unwrap_or(1..=usize::MAX)
    }

    /// Columns to process; no range provided means every column.
    pub fn cols_or_full(&self) -> RangeInclusive<usize> {
        self.cols
            .clone()
            .unwrap_or(1..=usize::MAX)
    }
}

impl TryFrom<ArgMatches> for Config {
    type Error = ConfigError;

    /// Build a `Config` from parsed arguments, validating the rules that
    /// span multiple arguments. Single-argument validity (range format,
    /// 1-based bounds) is enforced earlier, by the clap value parsers.
    fn try_from(matches: ArgMatches) -> Result<Config, ConfigError> {
        let config = Config {
            rows: matches
                .get_one::<RangeInclusive<usize>>("rows")
                .cloned(),
            cols: matches
                .get_one::<RangeInclusive<usize>>("columns")
                .cloned(),
            sort: matches.get_flag("sort"),
            delete: matches.get_flag("delete"),
            filename: matches
                .get_one::<String>("filename")
                .filter(|name| name.as_str() != "-")
                .map(PathBuf::from),
            find_string: matches
                .get_one::<String>("find")
                .cloned(),
            replace_string: matches
                .get_one::<String>("replace")
                .cloned(),
            output_filename: matches
                .get_one::<String>("output")
                .map(PathBuf::from),
        };

        if config.replace_string.is_some() && config.find_string.is_none() {
            return Err(ConfigError::MissingFindForReplace);
        }

        if config.replace_string.is_some() && config.delete {
            return Err(ConfigError::ReplaceWithDelete);
        }

        if config.delete && config.rows.is_none() && config.cols.is_none() {
            return Err(ConfigError::DeleteWithoutRange);
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
        assert!(config.find_string.is_none());
        assert!(config.replace_string.is_none());
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

        assert_eq!(config.rows, Some(2..=4));
        assert_eq!(config.cols, Some(1..=10));
        assert!(config.sort);
        assert_eq!(config.find_string.as_deref(), Some("a"));
        assert_eq!(config.replace_string.as_deref(), Some("b"));
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

        assert_eq!(config.rows_or_full(), 1..=usize::MAX);
        assert_eq!(config.cols_or_full(), 1..=usize::MAX);
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
    fn accepts_delete_with_row_or_column_range() {
        assert!(config_from(&["ft", "-d", "-R", "2-3", "input.txt"]).is_ok());
        assert!(config_from(&["ft", "-d", "-C", "2-3", "input.txt"]).is_ok());
    }
}
