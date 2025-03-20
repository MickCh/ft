use crate::cli_args::Config;

use bstr::io::BufReadExt;
use std::cmp;
use std::io;
use std::io::BufReader;
use std::io::prelude::*;
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
        let reader = std::fs::File::open(filename)?;
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

        buffer_reader.for_byte_line_with_terminator(|line| {
            current_line_number += 1;
            self.process_single_line(
                line,
                current_line_number,
                is_sequence_breaking,
                &mut non_sequence_vec,
                &mut writer,
                &mut sequence_stored,
            )
        })?;

        if !sequence_stored {
            self.write_modified_lines(&non_sequence_vec, &mut writer)?;
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
        if !self
            .config
            .rows
            .contains(&current_line_number)
        {
            //if line_number is outside of the row range (before & after)
            writer.write_all(line)?;
            return Ok(true);
        }

        if self.config.delete
            && self.config.is_rows_range_provided()
            && !self.config.is_cols_range_provided()
        {
            //don't add lines when rows range is given without cols range
            //do nothing - it will remove a line
            return Ok(true);
        }

        let utf8_line = from_utf8(line).unwrap(); //TODO: unwrap! - improve error handling

        if is_sequence_breaking {
            non_sequence_vec.push(utf8_line.to_owned());

            if current_line_number >= *self.config.rows.end() {
                *sequence_stored = self.write_modified_lines(non_sequence_vec, writer)?;
            }
        } else {
            writer.write_all(self.modify_line(utf8_line).as_bytes())?;
        }

        Ok(true)
    }

    fn write_modified_lines(
        &self,
        lines: &[String],
        writer: &mut std::fs::File,
    ) -> io::Result<bool> {
        for i in &self.modify_lines(lines) {
            writer.write_all(i.as_bytes())?;
        }
        Ok(true)
    }

    fn modify_lines(&self, lines: &[String]) -> Vec<String> {
        let (col_start, col_end) = self.config.cols.clone().into_inner();

        let mut result: Vec<String> = lines
            .iter()
            .map(|line| self.modify_line(line))
            .collect();

        if self.config.sort {
            result.sort_by(|line1, line2| {
                let line1_sub = self.get_substring(line1, col_start, col_end, false);
                let line2_sub = self.get_substring(line2, col_start, col_end, false);

                line1_sub.cmp(&line2_sub)
            });
        }
        result
    }

    fn modify_line(&self, utf8_line: &str) -> String {
        let (col_start, col_end) = self.config.cols.clone().into_inner();

        if self.config.delete && self.config.is_cols_range_provided() {
            let line = self.remove_new_line(utf8_line.to_owned());
            let result = self.get_substring(line.as_str(), col_start, col_end, true);
            return self.append_new_line(result);
        }

        if let Some(find) = self.config.find_string.clone() {
            if let Some(replace) = self.config.replace_string.clone() {
                let line = self.remove_new_line(utf8_line.to_owned());
                let result = self.line_replace(
                    line.as_str(),
                    find.as_str(),
                    replace.as_str(),
                    col_start,
                    col_end,
                );
                return self.append_new_line(result);
            }
        };

        utf8_line.to_owned()
    }

    fn remove_new_line(&self, line: String) -> String {
        #[cfg(windows)]
        const NEW_LINE: &str = "\r\n";
        #[cfg(not(windows))]
        const NEW_LINE: &str = "\n";

        line[..(line.len() - NEW_LINE.len())].to_owned()
    }

    fn append_new_line(&self, line: String) -> String {
        #[cfg(windows)]
        const NEW_LINE: &str = "\r\n";
        #[cfg(not(windows))]
        const NEW_LINE: &str = "\n";

        format!("{}{}", line, NEW_LINE)
    }

    fn get_substring(
        &self,
        line_str: &str,
        start: usize,
        end: usize,
        exclude_range: bool,
    ) -> String {
        let vec: Vec<char> = line_str.chars().collect();
        let vec_length = vec.len();

        if start > vec_length || start > end {
            return line_str.to_owned();
        }

        let end = cmp::min(end, vec_length);

        match exclude_range {
            true => {
                //unknown real capacity (possible UTF8 chars)
                let chunk1: String = vec[0..(start - 1)].iter().collect();
                let chunk2: String = vec[end..vec_length].iter().collect();
                format!("{}{}", chunk1, chunk2)
            }
            false => vec[(start - 1)..end].iter().collect(),
        }
    }

    fn line_replace(
        &self,
        line_str: &str,
        find: &str,
        replace: &str,
        start: usize,
        end: usize,
    ) -> String {
        let vec: Vec<char> = line_str.chars().collect();
        let vec_length = vec.len();

        if start > vec_length || start > end {
            return line_str.to_owned();
        }

        let end = cmp::min(end, vec_length);

        let chunk1 = &vec[0..(start - 1)];
        let chunk2 = &vec[(start - 1)..end];
        let chunk3 = &vec[end..vec_length];

        let chunk2_str: String = chunk2.iter().collect();
        let replaced = str::replace(&chunk2_str, find, replace);

        let capacity = chunk1.len() + replaced.len() + chunk3.len();

        let mut result = String::with_capacity(capacity);
        result.extend(chunk1);
        result.push_str(&replaced);
        result.extend(chunk3);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ops::RangeInclusive;

    #[test]
    fn test_get_substring() {
        let file_processor = create_file_processor();

        //pos:           123456789012345678901
        let line_utf8 = "ABCabcąłć😊😍😁123śę";

        let result = file_processor.get_substring(line_utf8, 1, 3, false);
        assert_eq!(result, "ABC");
        let result = file_processor.get_substring(line_utf8, 1, 3, true);
        assert_eq!(result, "abcąłć😊😍😁123śę");

        let result = file_processor.get_substring(line_utf8, 1, 6, false);
        assert_eq!(result, "ABCabc");
        let result = file_processor.get_substring(line_utf8, 1, 6, true);
        assert_eq!(result, "ąłć😊😍😁123śę");

        let result = file_processor.get_substring(line_utf8, 1, 9, false);
        assert_eq!(result, "ABCabcąłć");
        let result = file_processor.get_substring(line_utf8, 1, 9, true);
        assert_eq!(result, "😊😍😁123śę");

        let result = file_processor.get_substring(line_utf8, 1, 11, false);
        assert_eq!(result, "ABCabcąłć😊😍");
        let result = file_processor.get_substring(line_utf8, 1, 11, true);
        assert_eq!(result, "😁123śę");

        let result = file_processor.get_substring(line_utf8, 1, 16, false);
        assert_eq!(result, "ABCabcąłć😊😍😁123ś");
        let result = file_processor.get_substring(line_utf8, 1, 16, true);
        assert_eq!(result, "ę");

        let result = file_processor.get_substring(line_utf8, 1, 17, false);
        assert_eq!(result, "ABCabcąłć😊😍😁123śę");

        let result = file_processor.get_substring(line_utf8, 1, 18, false);
        assert_eq!(result, "ABCabcąłć😊😍😁123śę");
        let result = file_processor.get_substring(line_utf8, 1, 18, true);
        assert_eq!(result, "");

        let result = file_processor.get_substring(line_utf8, 1, 30, false);
        assert_eq!(result, "ABCabcąłć😊😍😁123śę");

        let result = file_processor.get_substring(line_utf8, 4, 30, false);
        assert_eq!(result, "abcąłć😊😍😁123śę");
        let result = file_processor.get_substring(line_utf8, 4, 30, true);
        assert_eq!(result, "ABC");

        let result = file_processor.get_substring(line_utf8, 9, 30, false);
        assert_eq!(result, "ć😊😍😁123śę");
        let result = file_processor.get_substring(line_utf8, 9, 30, true);
        assert_eq!(result, "ABCabcął");

        let result = file_processor.get_substring(line_utf8, 10, 12, false);
        assert_eq!(result, "😊😍😁");
        let result = file_processor.get_substring(line_utf8, 10, 12, true);
        assert_eq!(result, "ABCabcąłć123śę");

        let result = file_processor.get_substring(line_utf8, 11, 11, false);
        assert_eq!(result, "😍");
        let result = file_processor.get_substring(line_utf8, 11, 11, true);
        assert_eq!(result, "ABCabcąłć😊😁123śę");
    }

    #[test]
    fn test_line_replace() {
        let file_processor = create_file_processor();

        //pos:          123456789012345678901234
        let line_str = "Test01234567891231234567";

        let result = file_processor.line_replace(line_str, "Test", "Passed", 1, usize::MAX);
        assert_eq!(result, "Passed01234567891231234567");

        let result = file_processor.line_replace(line_str, "123", "ABC", 1, usize::MAX);
        assert_eq!(result, "Test0ABC456789ABCABC4567");

        let result = file_processor.line_replace(line_str, "123", "ABCDEF", 1, usize::MAX);
        assert_eq!(result, "Test0ABCDEF456789ABCDEFABCDEF4567");

        let result = file_processor.line_replace(line_str, "123", "", 1, usize::MAX);
        assert_eq!(result, "Test04567894567");

        let result = file_processor.line_replace(line_str, "123", "ABCD", 6, 8);
        assert_eq!(result, "Test0ABCD4567891231234567");

        let result = file_processor.line_replace(line_str, "123", "ABCD", 7, 8);
        assert_eq!(result, "Test01234567891231234567");

        let result = file_processor.line_replace(line_str, "123", "ABCD", 6, 7);
        assert_eq!(result, "Test01234567891231234567");

        let result = file_processor.line_replace(line_str, "123", "ABCD", 15, 20);
        assert_eq!(result, "Test0123456789ABCDABCD4567");

        let result = file_processor.line_replace(line_str, "123", "ABCD", 21, 40);
        assert_eq!(result, "Test01234567891231234567");

        let result = file_processor.line_replace(line_str, "123", "ABCD", 30, 40);
        assert_eq!(result, "Test01234567891231234567");

        let result = file_processor.line_replace(line_str, "123", "ABCD", 10, 5);
        assert_eq!(result, "Test01234567891231234567");
    }

    #[test]
    fn test_modify_line() {
        //pos:          123456789012345678901234
        let line_str = "Test01234567891231234567";

        //1
        //Given - delete & replace disabled
        //When - call function
        //Then - the same string
        let file_processor = FileProcessor::new(Config {
            cols: RangeInclusive::new(1usize, usize::MAX),
            rows: RangeInclusive::new(1usize, usize::MAX),
            delete: false,
            filename: "".to_owned(),
            find_string: None,
            replace_string: None,
            sort: false,
        });
        let result = file_processor.modify_line(line_str);
        assert_eq!(result, line_str);

        //2
        //Given - delete enabled with column range covered string length provided
        //When - call function
        //Then - empty string
        let file_processor = FileProcessor::new(Config {
            cols: RangeInclusive::new(1usize, 50), //provided range of columns
            rows: RangeInclusive::new(1usize, usize::MAX),
            delete: true,
            filename: "".to_owned(),
            find_string: None,
            replace_string: None,
            sort: false,
        });
        let result = file_processor.modify_line(line_str);
        assert_eq!(result, "\r\n");

        //3
        //Given - delete enabled with column range 5-10 provided
        //When - call function
        //Then - input string without chars 5-10
        let file_processor = FileProcessor::new(Config {
            cols: RangeInclusive::new(5, 10), //provided range of columns
            rows: RangeInclusive::new(1usize, usize::MAX),
            delete: true,
            filename: "".to_owned(),
            find_string: None,
            replace_string: None,
            sort: false,
        });
        let result = file_processor.modify_line(line_str);
        assert_eq!(result, "Test67891231234567");

        //4
        //Given - delete enabled with column range 5-10 provided and find/replace provided
        //When - call function
        //Then - input string without chars 5-10 (only delete worked)
        let file_processor = FileProcessor::new(Config {
            cols: RangeInclusive::new(5, 10), //provided range of columns
            rows: RangeInclusive::new(1usize, usize::MAX),
            delete: true,
            filename: "".to_owned(),
            find_string: Some("123".to_owned()),
            replace_string: Some("ABCD".to_owned()),
            sort: false,
        });
        let result = file_processor.modify_line(line_str);
        assert_eq!(result, "Test67891231234567");

        //5
        //Given - delete disabled but find (123)/replace (ABCD) with column range 5-10 provided
        //When - call function
        //Then - input string with first chars (pos. 5-7) replaces by ABCD
        let file_processor = FileProcessor::new(Config {
            cols: RangeInclusive::new(5, 10), //provided range of columns
            rows: RangeInclusive::new(1usize, usize::MAX),
            delete: false,
            filename: "".to_owned(),
            find_string: Some("123".to_owned()),
            replace_string: Some("ABCD".to_owned()),
            sort: false,
        });
        let result = file_processor.modify_line(line_str);
        assert_eq!(result, "Test0ABCD4567891231234567");
    }

    fn create_file_processor() -> FileProcessor {
        let config = Config {
            cols: RangeInclusive::new(1usize, usize::MAX),
            rows: RangeInclusive::new(1usize, usize::MAX),
            delete: false,
            filename: "".to_owned(),
            find_string: None,
            replace_string: None,
            sort: false,
        };

        FileProcessor::new(config)
    }

    #[test]
    fn test_modify_lines() {
        let lines: Vec<&str> = vec!["Line1_02", "Line2_03", "Line3_01"];
        let string_vec: Vec<String> = lines
            .iter()
            .map(|&line| line.to_string())
            .collect();

        //1
        //Given - Ranges/delete/sort/find/replace with default values (not provided)
        //When - call function
        //Then - the same lines w/o modifications
        let file_processor = FileProcessor::new(Config {
            cols: RangeInclusive::new(1usize, usize::MAX),
            rows: RangeInclusive::new(1usize, usize::MAX),
            delete: false,
            filename: "".to_owned(),
            find_string: None,
            replace_string: None,
            sort: false,
        });

        let result = file_processor.modify_lines(&string_vec);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], "Line1_02");
        assert_eq!(result[1], "Line2_03");
        assert_eq!(result[2], "Line3_01");

        //2
        //Given - Delete for column range 1-2 provided
        //When - call function
        //Then - it returns 3 lines with removed columns 1-2
        let file_processor = FileProcessor::new(Config {
            cols: RangeInclusive::new(1usize, 2),
            rows: RangeInclusive::new(1usize, usize::MAX),
            delete: true,
            filename: "".to_owned(),
            find_string: None,
            replace_string: None,
            sort: false,
        });

        let result = file_processor.modify_lines(&string_vec);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], "ne1_02");
        assert_eq!(result[1], "ne2_03");
        assert_eq!(result[2], "ne3_01");

        //3
        //Given - Sort lines by column range 7-8 provided
        //When - call function
        //Then - it returns 3 sorted lines, sorting based on columns 7-8
        let file_processor = FileProcessor::new(Config {
            cols: RangeInclusive::new(7usize, 8),
            rows: RangeInclusive::new(1usize, usize::MAX),
            delete: false,
            filename: "".to_owned(),
            find_string: None,
            replace_string: None,
            sort: true,
        });

        let result = file_processor.modify_lines(&string_vec);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0], "Line3_01");
        assert_eq!(result[1], "Line1_02");
        assert_eq!(result[2], "Line2_03");
    }
}
