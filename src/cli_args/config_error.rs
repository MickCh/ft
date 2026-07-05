use core::fmt;
use std::fmt::Formatter;

#[derive(Debug)]
pub enum ConfigError {
    MissingFindForReplace,
    ReplaceWithDelete,
    DeleteWithoutRange,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let message = match self {
            ConfigError::MissingFindForReplace => "Find string not specified for replace operation",
            ConfigError::ReplaceWithDelete => "Replace cannot be used with delete option",
            ConfigError::DeleteWithoutRange => {
                "Delete requires a row range (--rows) or a column range (--cols)"
            }
        };

        write!(f, "{message}")
    }
}

impl std::error::Error for ConfigError {}
