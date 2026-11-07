//a one-element slice of ranges (`&[2..=2]`) is how tests spell a column
//list with a single part, not a mistaken `Vec` of the range's values
#![cfg_attr(test, allow(clippy::single_range_in_vec_init))]

pub mod cli_args;
pub mod codec;
pub mod columns;
pub mod compose;
pub mod encoding;
pub mod error;
pub mod file_processor;
pub mod number;
pub mod pattern;
pub mod pattern_range;
pub mod predicate;
pub mod ranges;
pub mod records;
pub mod reduce;
pub mod text;
pub mod transform;
