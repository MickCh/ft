use std::path::{Path, PathBuf};

use clap::ArgMatches;
use regex::{Regex, RegexBuilder};

use super::ConfigError;
use crate::columns::{ColumnList, ColumnSpan, FieldSpan};
use crate::ranges::RangeSpec;

/// What `--find` matches: a literal substring, or a regular expression
/// when `--regex` is given. The regex is compiled (and therefore
/// validated) while building the `Config`.
#[derive(Debug)]
pub enum FindPattern {
    Literal(String),
    Regex(Regex),
}

/// One `--find`/`--replace` pair. Pairing the two arguments up while
/// building the `Config` makes their positional correspondence
/// structural instead of an invariant to re-validate downstream.
#[derive(Debug)]
pub struct Replacement {
    pub find: FindPattern,
    pub replace: String,
}

/// How the selected rows are reordered (`--sort`, `--tac` or
/// `--shuffle`). The flags are mutually exclusive on the command line;
/// the enum makes that exclusivity structural.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReorderMode {
    Sort { numeric: bool, reverse: bool },
    Tac,
    Shuffle,
}

/// Where input comes from. Standard input is written `-` (or simply
/// omitted), and is a valid member of a list of inputs — but not one
/// `--in-place` can edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    Stdin,
    File(PathBuf),
}

impl Input {
    /// The file this input reads, if it is one.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Input::Stdin => None,
            Input::File(path) => Some(path),
        }
    }
}

//the derived Default (no ranges, no operations, stdin to stdout) is a
//test-only base configuration; real configs always come from TryFrom
#[cfg_attr(test, derive(Default))]
#[derive(Debug)]
pub struct Config {
    pub rows: Option<RangeSpec>,
    pub cols: Option<ColumnList>,
    //per-operation column ranges: `None` falls back to `cols`, so an
    //operation only needs its own range when it must differ from it
    pub sort_key: Option<ColumnList>,
    pub unique_key: Option<ColumnList>,
    //`Some` switches the column ranges from counting chars to
    //delimited fields
    pub field_delimiter: Option<String>,
    //`Some` joins selected fields with something else than the input
    //delimiter (`--output-delimiter`)
    pub output_delimiter: Option<String>,
    pub reorder: Option<ReorderMode>,
    pub delete: bool,
    pub ignore_case: bool,
    pub upper: bool,
    pub lower: bool,
    pub trim: bool,
    pub title_case: bool,
    pub squeeze: bool,
    //number the output rows, like `nl`
    pub number: bool,
    //--fields splits on the delimiter even inside quotes unless set
    pub quoted: bool,
    //`Some` splits every line at each occurrence of the separator
    pub split_on: Option<String>,
    //`Some` wraps every line into chunks of that many chars
    pub wrap: Option<usize>,
    pub drop_empty: bool,
    //`Some` joins every processed row into one, separated by this
    pub join: Option<String>,
    //the summary to write instead of the processed rows: how many rows
    //(`--count`) and statistics over a column, optionally per group
    pub count: bool,
    pub sum: Option<ColumnList>,
    pub avg: Option<ColumnList>,
    pub min: Option<ColumnList>,
    pub max: Option<ColumnList>,
    pub group_by: Option<ColumnList>,
    pub grep: Option<Regex>,
    pub invert: bool,
    //answer only whether anything matched: write nothing, and say it
    //with the exit code
    pub quiet: bool,
    pub unique: bool,
    //the inputs, read one after another as a single stream (`--in-place`
    //edits each file on its own); never empty, as no argument means stdin
    pub inputs: Vec<Input>,
    //find/replace pairs are applied in the order given
    pub replacements: Vec<Replacement>,
    pub output_filename: Option<PathBuf>,
    pub in_place: bool,
    //`Some` keeps a copy of each edited file, named after it plus this
    //suffix (`--backup`)
    pub backup: Option<String>,
    //report what `--in-place` would change instead of writing it
    pub dry_run: bool,
}

impl Config {
    /// Rows to process; no range provided means every row.
    pub fn rows_or_full(&self) -> RangeSpec {
        self.rows
            .clone()
            .unwrap_or_else(RangeSpec::full)
    }

    /// Columns to process; no range provided means every column.
    pub fn cols_or_full(&self) -> ColumnList {
        self.cols
            .clone()
            .unwrap_or_else(ColumnList::full)
    }

    /// How the column range addresses lines: char positions, or fields
    /// separated by a delimiter when `--fields` was given.
    pub fn col_span(&self) -> ColumnSpan {
        self.span_of(None)
    }

    /// Column span keying `--sort`: `--sort-key` when given, `--cols`
    /// otherwise.
    pub fn sort_key_span(&self) -> ColumnSpan {
        self.span_of(self.sort_key.clone())
    }

    /// Column span keying `--unique`: `--unique-key` when given,
    /// `--cols` otherwise.
    pub fn unique_key_span(&self) -> ColumnSpan {
        self.span_of(self.unique_key.clone())
    }

    /// The span of an explicit column list (a `--sum` column, a
    /// `--group-by` key), in the same char or field mode as every other
    /// column range.
    pub fn span_for(&self, columns: ColumnList) -> ColumnSpan {
        self.span_of(Some(columns))
    }

    /// What separates the columns of a summary row: the output
    /// delimiter, then the input one, and a tab when neither says
    /// otherwise (in char mode there is no delimiter to inherit).
    pub fn summary_separator(&self) -> String {
        self.output_delimiter
            .clone()
            .or_else(|| self.field_delimiter.clone())
            .unwrap_or_else(|| "\t".to_owned())
    }

    /// Turn a column list into a span, falling back to `--cols` (and
    /// then to every column) when the operation has no list of its own.
    /// `--fields` applies to every column list alike.
    fn span_of(&self, columns: Option<ColumnList>) -> ColumnSpan {
        let columns = columns.unwrap_or_else(|| self.cols_or_full());
        match &self.field_delimiter {
            Some(delimiter) => ColumnSpan::Fields(FieldSpan {
                delimiter: delimiter.clone(),
                output_delimiter: self.output_delimiter.clone(),
                quoted: self.quoted,
                fields: columns,
            }),
            None => ColumnSpan::Chars(columns),
        }
    }

    /// Whether some operation claims `--cols` as its scope or key (as
    /// opposed to a bare `--cols`, which selects the columns). An
    /// operation given its own key range no longer claims `--cols`, so
    /// `--cols 2-3 --sort --sort-key 1` still cuts columns 2-3.
    pub fn has_column_operation(&self) -> bool {
        self.delete
            || self.sort_keys_on_cols()
            || !self.replacements.is_empty()
            || self.upper
            || self.lower
            || self.trim
            || self.title_case
            || self.squeeze
            || self.grep.is_some()
            || (self.unique && self.unique_key.is_none())
    }

    /// Whether `--sort` takes its key from `--cols` (no `--sort-key`).
    fn sort_keys_on_cols(&self) -> bool {
        matches!(self.reorder, Some(ReorderMode::Sort { .. })) && self.sort_key.is_none()
    }

    /// The files among the inputs, in order. With `--in-place` these are
    /// all of them (validated), and each is edited on its own.
    pub fn input_files(&self) -> impl Iterator<Item = &Path> {
        self.inputs
            .iter()
            .filter_map(Input::path)
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
        let find_patterns = matches
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
        let replace_strings: Vec<String> = matches
            .get_many::<String>("replace")
            .into_iter()
            .flatten()
            .cloned()
            .collect();

        //--find and --replace pair up positionally, so their counts must
        //match; a lone --find (or --replace) has no partner and is rejected
        //rather than silently ignored
        match (find_patterns.len(), replace_strings.len()) {
            (0, 0) => {}
            (0, _) => return Err(ConfigError::MissingFindForReplace),
            (_, 0) => return Err(ConfigError::MissingReplaceForFind),
            (finds, replaces) if finds != replaces => {
                return Err(ConfigError::FindReplaceCountMismatch { finds, replaces });
            }
            _ => {}
        }
        let replacements = find_patterns
            .into_iter()
            .zip(replace_strings)
            .map(|(find, replace)| Replacement { find, replace })
            .collect();

        //clap conflict rules keep the three reorder flags mutually exclusive
        let reorder = if matches.get_flag("sort") {
            Some(ReorderMode::Sort {
                numeric: matches.get_flag("numeric"),
                reverse: matches.get_flag("reverse"),
            })
        } else if matches.get_flag("tac") {
            Some(ReorderMode::Tac)
        } else if matches.get_flag("shuffle") {
            Some(ReorderMode::Shuffle)
        } else {
            None
        };

        let config = Config {
            rows: matches
                .get_one::<RangeSpec>("rows")
                .cloned(),
            cols: matches
                .get_one::<ColumnList>("columns")
                .cloned(),
            sort_key: matches
                .get_one::<ColumnList>("sort-key")
                .cloned(),
            unique_key: matches
                .get_one::<ColumnList>("unique-key")
                .cloned(),
            field_delimiter: matches
                .get_one::<String>("fields")
                .cloned(),
            output_delimiter: matches
                .get_one::<String>("output-delimiter")
                .cloned(),
            reorder,
            delete: matches.get_flag("delete"),
            ignore_case,
            upper: matches.get_flag("upper"),
            lower: matches.get_flag("lower"),
            trim: matches.get_flag("trim"),
            title_case: matches.get_flag("title-case"),
            squeeze: matches.get_flag("squeeze"),
            number: matches.get_flag("number"),
            quoted: matches.get_flag("quoted"),
            split_on: matches
                .get_one::<String>("split-on")
                .cloned(),
            wrap: matches
                .get_one::<usize>("wrap")
                .copied(),
            drop_empty: matches.get_flag("drop-empty"),
            join: matches
                .get_one::<String>("join")
                .cloned(),
            count: matches.get_flag("count"),
            sum: matches
                .get_one::<ColumnList>("sum")
                .cloned(),
            avg: matches
                .get_one::<ColumnList>("avg")
                .cloned(),
            min: matches
                .get_one::<ColumnList>("min")
                .cloned(),
            max: matches
                .get_one::<ColumnList>("max")
                .cloned(),
            group_by: matches
                .get_one::<ColumnList>("group-by")
                .cloned(),
            grep,
            invert: matches.get_flag("invert"),
            quiet: matches.get_flag("quiet"),
            unique: matches.get_flag("unique"),
            inputs: inputs(&matches),
            replacements,
            output_filename: matches
                .get_one::<String>("output")
                .map(PathBuf::from),
            in_place: matches.get_flag("in-place"),
            backup: matches
                .get_one::<String>("backup")
                .cloned(),
            dry_run: matches.get_flag("dry-run"),
        };

        if !config.replacements.is_empty() && config.delete {
            return Err(ConfigError::ReplaceWithDelete);
        }

        if config.delete && config.rows.is_none() && config.cols.is_none() && config.grep.is_none()
        {
            return Err(ConfigError::DeleteWithoutRange);
        }

        //deleting whole rows and reordering them is contradictory; with a
        //column range `--delete` removes columns, so reordering is fine
        if config.delete && config.cols.is_none() && config.reorder.is_some() {
            return Err(ConfigError::DeleteWithReorder);
        }

        if config.ignore_case && config.replacements.is_empty() && config.grep.is_none() {
            return Err(ConfigError::IgnoreCaseWithoutPattern);
        }

        //--in-place rewrites its inputs, and standard input cannot be
        //rewritten, so every input has to be a file
        if config.in_place
            && config
                .inputs
                .iter()
                .any(|input| input.path().is_none())
        {
            return Err(ConfigError::InPlaceWithoutFile);
        }

        Ok(config)
    }
}

/// The inputs named on the command line, in order. No argument (or a
/// lone `-`) means standard input, so the list is never empty.
fn inputs(matches: &ArgMatches) -> Vec<Input> {
    let inputs: Vec<Input> = matches
        .get_many::<String>("filename")
        .into_iter()
        .flatten()
        .map(|name| match name.as_str() {
            "-" => Input::Stdin,
            path => Input::File(PathBuf::from(path)),
        })
        .collect();

    if inputs.is_empty() {
        vec![Input::Stdin]
    } else {
        inputs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli_args::cli;
    use std::ops::RangeInclusive;

    fn config_from(args: &[&str]) -> Result<Config, ConfigError> {
        let matches = cli()
            .try_get_matches_from(args)
            .expect("clap parsing failed");
        Config::try_from(matches)
    }

    /// The column parts a span addresses, in the order written.
    fn written(span: &ColumnSpan) -> Vec<RangeInclusive<usize>> {
        match span {
            ColumnSpan::Chars(list) => list.written().to_vec(),
            ColumnSpan::Fields(spec) => spec.fields.written().to_vec(),
        }
    }

    #[test]
    fn defaults_when_only_filename_given() {
        let config = config_from(&["ft", "input.txt"]).unwrap();

        assert_eq!(config.inputs, [Input::File(PathBuf::from("input.txt"))]);
        assert!(config.rows.is_none());
        assert!(config.cols.is_none());
        assert!(config.reorder.is_none());
        assert!(!config.delete);
        assert!(config.replacements.is_empty());
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
        assert_eq!(config.cols, Some(ColumnList::from(1..=10)));
        assert_eq!(
            config.reorder,
            Some(ReorderMode::Sort {
                numeric: false,
                reverse: false
            })
        );
        assert!(matches!(
            config.replacements.as_slice(),
            [Replacement { find: FindPattern::Literal(f), replace }] if f == "a" && replace == "b"
        ));
        assert_eq!(config.output_filename, Some(PathBuf::from("out.txt")));
    }

    #[test]
    fn omitted_or_dash_filename_means_stdin() {
        let config = config_from(&["ft"]).unwrap();
        assert_eq!(config.inputs, [Input::Stdin]);

        let config = config_from(&["ft", "-"]).unwrap();
        assert_eq!(config.inputs, [Input::Stdin]);
    }

    #[test]
    fn several_filenames_become_several_inputs() {
        let config = config_from(&["ft", "a.txt", "b.txt"]).unwrap();
        assert_eq!(
            config.inputs,
            [
                Input::File(PathBuf::from("a.txt")),
                Input::File(PathBuf::from("b.txt")),
            ]
        );
        assert_eq!(
            config.input_files().collect::<Vec<_>>(),
            [Path::new("a.txt"), Path::new("b.txt")]
        );

        //stdin may sit among the files, and is not one of them
        let config = config_from(&["ft", "a.txt", "-"]).unwrap();
        assert_eq!(
            config.inputs,
            [Input::File(PathBuf::from("a.txt")), Input::Stdin]
        );
        assert_eq!(
            config.input_files().collect::<Vec<_>>(),
            [Path::new("a.txt")]
        );
    }

    #[test]
    fn in_place_rejects_stdin_among_the_inputs() {
        let error = config_from(&["ft", "-i", "a.txt", "-"]).unwrap_err();
        assert!(matches!(error, ConfigError::InPlaceWithoutFile));

        assert!(config_from(&["ft", "-i", "a.txt", "b.txt"]).is_ok());
    }

    #[test]
    fn missing_ranges_fall_back_to_full_range() {
        let config = config_from(&["ft", "input.txt"]).unwrap();

        assert_eq!(config.rows_or_full(), RangeSpec::full());
        assert_eq!(config.cols_or_full(), ColumnList::full());
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
    fn key_ranges_override_cols_per_operation() {
        let config = config_from(&[
            "ft",
            "-C",
            "5-6",
            "-s",
            "--sort-key",
            "1-2",
            "-u",
            "--unique-key",
            "3-4",
            "input.txt",
        ])
        .unwrap();

        assert_eq!(config.sort_key, Some(ColumnList::from(1..=2)));
        assert_eq!(config.unique_key, Some(ColumnList::from(3..=4)));
        assert_eq!(written(&config.sort_key_span()), [1..=2]);
        assert_eq!(written(&config.unique_key_span()), [3..=4]);
        assert_eq!(written(&config.col_span()), [5..=6]);
    }

    #[test]
    fn key_ranges_fall_back_to_cols() {
        let config = config_from(&["ft", "-C", "2-3", "-s", "-u", "input.txt"]).unwrap();

        assert!(config.sort_key.is_none());
        assert!(config.unique_key.is_none());
        assert_eq!(written(&config.sort_key_span()), [2..=3]);
        assert_eq!(written(&config.unique_key_span()), [2..=3]);
    }

    #[test]
    fn column_lists_reach_every_operation_key() {
        let config =
            config_from(&["ft", "-C", "3,1", "-s", "--sort-key", "2,4-5", "input.txt"]).unwrap();

        //the parts keep the order written, so reading them permutes
        assert_eq!(written(&config.col_span()), [3..=3, 1..=1]);
        assert_eq!(written(&config.sort_key_span()), [2..=2, 4..=5]);
    }

    #[test]
    fn output_delimiter_joins_selected_fields() {
        let config = config_from(&[
            "ft",
            "-F",
            ",",
            "-C",
            "2,1",
            "--output-delimiter",
            ";",
            "input.txt",
        ])
        .unwrap();

        assert_eq!(config.output_delimiter.as_deref(), Some(";"));
        assert_eq!(config.col_span().joiner(), ";");

        //without it, the input delimiter joins the fields back together
        let config = config_from(&["ft", "-F", ",", "-C", "2,1", "input.txt"]).unwrap();
        assert_eq!(config.col_span().joiner(), ",");
    }

    #[test]
    fn output_delimiter_requires_fields() {
        assert!(
            cli()
                .try_get_matches_from(["ft", "-C", "1", "--output-delimiter", ";", "input.txt"])
                .is_err()
        );
    }

    #[test]
    fn an_operation_with_its_own_key_leaves_cols_to_select() {
        //--sort keys on --cols, so --cols is not a bare selection
        let config = config_from(&["ft", "-C", "2-3", "-s", "input.txt"]).unwrap();
        assert!(config.has_column_operation());

        //with its own key, --sort no longer claims --cols: it selects
        let config =
            config_from(&["ft", "-C", "2-3", "-s", "--sort-key", "1", "input.txt"]).unwrap();
        assert!(!config.has_column_operation());

        //the same holds for --unique
        let config =
            config_from(&["ft", "-C", "2-3", "-u", "--unique-key", "1", "input.txt"]).unwrap();
        assert!(!config.has_column_operation());
    }

    #[test]
    fn key_ranges_use_field_mode_too() {
        let config = config_from(&["ft", "-F", ",", "-s", "--sort-key", "2", "input.txt"]).unwrap();
        assert!(matches!(config.sort_key_span(), ColumnSpan::Fields { .. }));
    }

    #[test]
    fn key_ranges_require_their_operation() {
        assert!(
            cli()
                .try_get_matches_from(["ft", "--sort-key", "1", "input.txt"])
                .is_err()
        );
        assert!(
            cli()
                .try_get_matches_from(["ft", "--unique-key", "1", "input.txt"])
                .is_err()
        );
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
        assert_eq!(
            config.reorder,
            Some(ReorderMode::Sort {
                numeric: true,
                reverse: true
            })
        );
    }

    #[test]
    fn tac_and_shuffle_map_to_reorder_modes() {
        let config = config_from(&["ft", "--tac", "input.txt"]).unwrap();
        assert_eq!(config.reorder, Some(ReorderMode::Tac));

        let config = config_from(&["ft", "--shuffle", "input.txt"]).unwrap();
        assert_eq!(config.reorder, Some(ReorderMode::Shuffle));
    }

    #[test]
    fn regex_flag_compiles_find_as_regex() {
        let config = config_from(&["ft", "-e", "-f", r"\d+", "-r", "N", "input.txt"]).unwrap();
        assert!(matches!(
            config.replacements.as_slice(),
            [Replacement {
                find: FindPattern::Regex(_),
                ..
            }]
        ));
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

        assert!(matches!(
            config.replacements.as_slice(),
            [
                Replacement { find: FindPattern::Literal(x), replace: r1 },
                Replacement { find: FindPattern::Literal(y), replace: r2 },
            ] if x == "a" && r1 == "1" && y == "b" && r2 == "2"
        ));
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
        let [
            Replacement {
                find: FindPattern::Regex(pattern),
                ..
            },
        ] = config.replacements.as_slice()
        else {
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
