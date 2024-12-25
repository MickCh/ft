use crate::config::config::Config;

use bstr::io::BufReadExt;
use std::cmp;
use std::io;
use std::io::prelude::*;
use std::io::BufReader;
use std::str::from_utf8;

pub struct FileProcessor {
    config: Config,
}

impl FileProcessor {
    pub fn new(config: Config) -> FileProcessor {
        FileProcessor { config }
    }

    pub fn process(&self) -> std::io::Result<()> {
        let is_sequence_breaking = self.config.is_sequence_breaking();

        let filename = &self.config.filename;
        let reader = std::fs::File::open(&filename)?;
        let mut buffer_reader = BufReader::new(reader);

        //rethink how to store data
        //I tried to do everything is single loop but maybe need to change that approach
        //I have 3 section now
        //1. Before row range
        //2. Insize of row range (modified rows)
        //3. After row range
        let mut writer = std::fs::File::create(format!("{}{}", filename, ".out"))?;

        let mut non_sequence_vec: Vec<String> = Vec::new();
        let mut current_line_number = 0usize;
        let mut sequence_stored = false;

        buffer_reader.for_byte_record_with_terminator(b'\n', |line| {
            current_line_number += 1;
            self.process_single_line(
                &line,
                current_line_number,
                is_sequence_breaking,
                &mut non_sequence_vec,
                &mut writer,
                &mut sequence_stored,
            )
        })?;

        if !sequence_stored {
            self.write_all_modifiable_lines(&non_sequence_vec, &mut writer)?;
        }

        writer.flush()
    }

    fn process_single_line(
        &self,
        line: &[u8],
        current_line_number: usize,
        is_sequence_breaking: bool,
        non_sequence_vec: &mut Vec<String>,
        writer: &mut std::fs::File,
        sequence_stored: &mut bool,
    ) -> io::Result<bool> {
        let (row_start, row_end) = self.config.rows.clone().into_inner();

        if row_start > current_line_number || current_line_number > row_end {
            //if line_number is outside of the row range (before & after)
            writer.write_all(line)?;
        } else if self.config.delete {
            //do nothing (delete)
        } else if is_sequence_breaking {
            let utf8_line = from_utf8(line).unwrap(); //TODO: unwrap! - improve error handling
            non_sequence_vec.push(utf8_line.to_owned());

            if current_line_number >= row_end {
                *sequence_stored = self.write_all_modifiable_lines(&non_sequence_vec, writer)?;
            }
        } else {
            //write modified lines in sequence
            let utf8_line = from_utf8(line).unwrap(); //TODO: unwrap! - improve error handling

            writer.write_all(
                self.modify_line(&utf8_line.to_owned())
                    .as_bytes(),
            )?;
        }
        Ok(true)
    }

    fn write_all_modifiable_lines(
        &self,
        lines: &Vec<String>,
        writer: &mut std::fs::File,
    ) -> io::Result<bool> {
        let (col_start, col_end) = self.config.cols.clone().into_inner();

        for i in &self.process_all_modifiable_lines(&lines, col_start, col_end) {
            writer.write_all(i.as_bytes())?;
        }
        Ok(true)
    }

    fn process_all_modifiable_lines(
        &self,
        lines: &Vec<String>,
        col_start: usize,
        col_end: usize,
    ) -> Vec<String> {
        let mut result: Vec<String> = lines
            .iter()
            .map(|line| self.modify_line(&line))
            .collect();

        if self.config.sort {
            result.sort_by(|line1, line2| {
                let line1_slice = &line1[(col_start - 1)..cmp::min(col_end, line1.len())];
                let line2_slice = &line2[(col_start - 1)..cmp::min(col_end, line2.len())];

                line1_slice.cmp(line2_slice)
            });
        }
        result
    }

    fn modify_line(&self, utf8_line: &String) -> String {
        if let Some(find) = self.config.find_string.clone() {
            if let Some(replace) = self.config.replace_string.clone() {
                let (col_start, col_end) = self.config.cols.clone().into_inner();

                return self.line_replace(
                    utf8_line.as_str(),
                    find.as_str(),
                    replace.as_str(),
                    col_start,
                    col_end,
                );
            }
        };

        utf8_line.clone()
    }

    fn line_replace(
        &self,
        line_str: &str,
        find: &str,
        replace: &str,
        start: usize,
        stop: usize,
    ) -> String {
        let mut prefix: Vec<char> = vec![];
        let mut main: Vec<char> = vec![];
        let mut suffix: Vec<char> = vec![];

        for (index, c) in line_str.chars().enumerate() {
            match index {
                i if i + 1 < start => prefix.push(c),
                i if i >= stop => suffix.push(c),
                _ => main.push(c),
            }
        }

        let main: String = main.into_iter().collect();
        let replaced = str::replace(&main, find, replace);

        let capacity = prefix.len() + replaced.len() + suffix.len();

        let mut result = String::with_capacity(capacity);
        result.extend(prefix);
        result.push_str(&replaced);
        result.extend(suffix);
        result
    }
}
