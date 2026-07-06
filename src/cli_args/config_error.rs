use core::fmt;
use std::fmt::Formatter;

#[derive(Debug)]
pub enum ConfigError {
    MissingFindForReplace,
    ReplaceWithDelete,
    DeleteWithoutRange,
    IgnoreCaseWithoutPattern,
    InvalidRegex(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::MissingFindForReplace => {
                write!(f, "Find string not specified for replace operation")
            }
            ConfigError::ReplaceWithDelete => {
                write!(f, "Replace cannot be used with delete option")
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
            ConfigError::InvalidRegex(error) => write!(f, "Invalid regular expression: {error}"),
        }
    }
}

impl std::error::Error for ConfigError {}
