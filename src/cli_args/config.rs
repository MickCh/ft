use std::ops::RangeInclusive;

#[derive(Debug)]
pub struct Config {
    pub rows: RangeInclusive<usize>,
    pub cols: RangeInclusive<usize>,
    pub sort: bool,
    pub delete: bool,
    pub filename: String,
    pub find_string: Option<String>,
    pub replace_string: Option<String>,
    pub output_filename: Option<String>,
}

impl Config {
    pub fn is_sequence_breaking(&self) -> bool {
        self.sort
    }

    pub fn is_rows_range_provided(&self) -> bool {
        *self.rows.start() != 1usize || *self.rows.end() != usize::MAX
    }

    pub fn is_cols_range_provided(&self) -> bool {
        *self.cols.start() != 1usize || *self.cols.end() != usize::MAX
    }
}
