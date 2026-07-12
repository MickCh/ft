//! Top-level application error: everything `main` can fail with.

use std::fmt;
use std::io;
use std::path::PathBuf;

use crate::cli_args::ConfigError;

#[derive(Debug)]
pub enum AppError {
    InvalidArguments(ConfigError),
    OpenInput { path: PathBuf, source: io::Error },
    CreateOutput { path: PathBuf, source: io::Error },
    OutputIsInput { path: PathBuf },
    ReplaceInput { path: PathBuf, source: io::Error },
    Backup { path: PathBuf, source: io::Error },
    Processing(io::Error),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::InvalidArguments(error) => write!(f, "User input: {error}"),
            AppError::OpenInput { path, source } => {
                write!(f, "Cannot open input file `{}`: {source}", path.display())
            }
            AppError::CreateOutput { path, source } => {
                write!(
                    f,
                    "Cannot create output file `{}`: {source}",
                    path.display()
                )
            }
            AppError::OutputIsInput { path } => {
                write!(
                    f,
                    "Output file `{}` is the input file; refusing to truncate it (use --in-place)",
                    path.display()
                )
            }
            AppError::ReplaceInput { path, source } => {
                write!(
                    f,
                    "Cannot replace input file `{}` in place: {source}",
                    path.display()
                )
            }
            AppError::Backup { path, source } => {
                write!(f, "Cannot write backup file `{}`: {source}", path.display())
            }
            AppError::Processing(error) => write!(f, "Processing error: {error}"),
        }
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AppError::InvalidArguments(error) => Some(error),
            AppError::OpenInput { source, .. }
            | AppError::CreateOutput { source, .. }
            | AppError::ReplaceInput { source, .. }
            | AppError::Backup { source, .. } => Some(source),
            AppError::OutputIsInput { .. } => None,
            AppError::Processing(error) => Some(error),
        }
    }
}

impl From<ConfigError> for AppError {
    fn from(error: ConfigError) -> AppError {
        AppError::InvalidArguments(error)
    }
}
