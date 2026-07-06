use core::fmt;
use std::fmt::Formatter;

#[derive(Debug)]
pub enum ConfigError {
    MissingFindForReplace,
    MissingReplaceForFind,
    FindReplaceCountMismatch { finds: usize, replaces: usize },
    ReplaceWithDelete,
    DeleteWithReorder,
    DeleteWithoutRange,
    IgnoreCaseWithoutPattern,
    InPlaceWithoutFile,
    InvalidRegex(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::MissingFindForReplace => {
                write!(f, "Find string not specified for replace operation")
            }
            ConfigError::MissingReplaceForFind => {
                write!(
                    f,
                    "--find needs a matching --replace (use --grep to filter rows by content)"
                )
            }
            ConfigError::FindReplaceCountMismatch { finds, replaces } => {
                write!(
                    f,
                    "Each --find needs its own --replace (got {finds} find and {replaces} replace values)"
                )
            }
            ConfigError::ReplaceWithDelete => {
                write!(f, "Replace cannot be used with delete option")
            }
            ConfigError::DeleteWithReorder => {
                write!(
                    f,
                    "Delete removes the selected rows, so they cannot also be reordered (--sort, --tac or --shuffle)"
                )
            }
            ConfigError::DeleteWithoutRange => {
                write!(
                    f,
                    "Delete requires a row range (--rows), a column range (--cols) or a filter (--grep)"
                )
            }
            ConfigError::IgnoreCaseWithoutPattern => {
                write!(f, "Ignore-case requires a pattern (--find or --grep)")
            }
            ConfigError::InPlaceWithoutFile => {
                write!(
                    f,
                    "In-place editing needs an input file, not standard input"
                )
            }
            ConfigError::InvalidRegex(error) => write!(f, "Invalid regular expression: {error}"),
        }
    }
}

impl std::error::Error for ConfigError {}
