pub mod cli;
pub mod config;
pub mod config_error;

pub use cli::cli;
pub use config::{Config, FindPattern, InPlace, Input, ReorderMode, Replacement, SummarySpec};
pub use config_error::ConfigError;
