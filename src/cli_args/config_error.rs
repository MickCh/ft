use core::fmt;
use std::fmt::Formatter;

#[derive(Debug)]
pub enum ConfigError {
    MissingFindForReplace,
    ReplaceWithDelete,
    DeleteWithoutRange,
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
                    "Delete requires a row range (--rows) or a column range (--cols)"
                )
            }
            ConfigError::InvalidRegex(error) => write!(f, "Invalid regular expression: {error}"),
        }
    }
}

impl std::error::Error for ConfigError {}
