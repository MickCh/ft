use core::fmt;
use std::fmt::Formatter;

#[derive(Debug)]
pub enum ConfigError {
    MissingFindForReplace,
    ReplaceWithDelete,
    RowEndGTStart,
    ColEndGTStart,
}

impl ConfigError {
    const MISSING_FIND_MESSAGE: &str = "Find string not specified for replace operation";
    const REPLACE_WITH_DELETE_MESSAGE: &str = "Replace cannot be used with delete option";
    const ROW_END_GT_START_MESSAGE: &str = "Start row cannot be greater than end row";
    const COL_END_GT_START_MESSAGE: &str = "Start column cannot be greater than end column";
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let message = match self {
            ConfigError::MissingFindForReplace => Self::MISSING_FIND_MESSAGE,
            ConfigError::ReplaceWithDelete => Self::REPLACE_WITH_DELETE_MESSAGE,
            ConfigError::RowEndGTStart => Self::ROW_END_GT_START_MESSAGE,
            ConfigError::ColEndGTStart => Self::COL_END_GT_START_MESSAGE,
        };

        write!(f, "{message}")
    }
}

//TODO: check it
impl std::error::Error for ConfigError {}
