use clap::ArgMatches;
use std::ops::RangeInclusive;

use crate::config::cli::cli;
use crate::config::config_error::ConfigError;

#[derive(Debug)]
pub struct Config {
    pub rows: RangeInclusive<usize>,
    pub cols: RangeInclusive<usize>,
    pub sort: bool,
    pub delete: bool,
    pub filename: String,
    pub find_string: Option<String>,
    pub replace_string: Option<String>,
}

impl Config {
    pub fn new() -> ConfigBuilder {
        ConfigBuilder {
            matches: cli().get_matches(),
            rows: RangeInclusive::new(1usize, usize::MAX),
            cols: RangeInclusive::new(1usize, usize::MAX),
            sort: false,
            delete: false,
            filename: String::new(),
            find_string: None,
            replace_string: None,
        }
    }

    pub fn is_sequence_breaking(&self) -> bool {
        self.sort || self.delete
    }
}

pub struct ConfigBuilder {
    matches: ArgMatches,
    rows: RangeInclusive<usize>,
    cols: RangeInclusive<usize>,
    sort: bool,
    delete: bool,
    filename: String,
    find_string: Option<String>,
    replace_string: Option<String>,
}

impl ConfigBuilder {
    pub fn rows(&mut self) -> &mut Self {
        self.rows = match self
            .matches
            .get_one::<RangeInclusive<usize>>("rows")
        {
            Some(rows) => rows.clone(),
            None => RangeInclusive::new(1usize, usize::MAX),
        };
        self
    }

    pub fn cols(&mut self) -> &mut Self {
        self.cols = match self
            .matches
            .get_one::<RangeInclusive<usize>>("columns")
        {
            Some(cols) => cols.clone(),
            None => RangeInclusive::new(1usize, usize::MAX),
        };
        self
    }

    pub fn sort(&mut self) -> &mut Self {
        self.sort = match self.matches.get_one::<bool>("sort") {
            Some(sort) => *sort,
            None => false,
        };
        self
    }

    pub fn delete(&mut self) -> &mut Self {
        self.delete = match self.matches.get_one::<bool>("delete") {
            Some(delete) => *delete,
            None => false,
        };
        self
    }

    pub fn filename(&mut self) -> &mut Self {
        self.filename = match self
            .matches
            .get_one::<String>("filename")
        {
            Some(filename) => filename.to_owned(),
            None => String::new(),
        };
        self
    }

    pub fn replace(&mut self) -> &mut Self {
        self.find_string = self
            .matches
            .get_one::<String>("find")
            .map(|f| f.to_owned());

        self.replace_string = self
            .matches
            .get_one::<String>("replace")
            .map(|r| r.to_owned());

        self
    }

    pub fn build(&mut self) -> Result<Config, ConfigError> {
        if self.replace_string.is_some() && self.find_string.is_none() {
            return Err(ConfigError::MissingFindForReplace);
        }

        if self.replace_string.is_some() && self.delete {
            return Err(ConfigError::ReplaceWithDelete);
        }

        if self.rows.start() > self.rows.end() {
            return Err(ConfigError::RowEndGTStart);
        }

        if self.cols.start() > self.cols.end() {
            return Err(ConfigError::ColEndGTStart);
        }

        Ok(Config {
            rows: self.rows.clone(),
            cols: self.cols.clone(),
            sort: self.sort,
            delete: self.delete,
            filename: self.filename.clone(),
            find_string: self.find_string.clone(),
            replace_string: self.replace_string.clone(),
        })
    }
}
