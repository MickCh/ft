//! Streaming orchestrator: applies row selection, the per-line transform
//! pipeline and optional sorting to any `BufRead` source and `Write` sink.

use crate::cli_args::Config;
use crate::constants::NEW_LINE;
use crate::text;
use crate::transform::{self, LineTransform};

use bstr::io::BufReadExt;
use std::io;
use std::io::prelude::*;
use std::ops::RangeInclusive;
use std::str::from_utf8;

/// A buffered line split into content and its original terminator.
struct Line {
    content: String,
    terminator: String,
}

pub struct FileProcessor {
    rows: RangeInclusive<usize>,
    keep_lines_outside_rows: bool,
    delete_lines_in_rows: bool,
    //sorting requires buffering the whole row range before writing;
    //`None` means lines stream straight to the writer
    sort_key_cols: Option<RangeInclusive<usize>>,
    transforms: Vec<Box<dyn LineTransform>>,
}

impl FileProcessor {
    pub fn new(config: &Config) -> FileProcessor {
        FileProcessor {
            rows: config.rows.clone(),
            //delete mode keeps lines outside the row range,
            //selection mode (no delete) drops them
            keep_lines_outside_rows: config.delete,
            delete_lines_in_rows: config.delete
                && config.is_rows_range_provided()
                && !config.is_cols_range_provided(),
            sort_key_cols: config.sort.then(|| config.cols.clone()),
            transforms: transform::build_pipeline(config),
        }
    }

    /// Stream `reader` line by line into `writer`, applying the configured
    /// row selection, per-line transforms and optional sorting.
    pub fn run<R: BufRead, W: Write>(&self, mut reader: R, writer: &mut W) -> io::Result<()> {
        let mut sort_buffer: Vec<Line> = Vec::new();
        let mut line_number = 0usize;
        let mut buffer_flushed = false;

        reader.for_byte_line_with_terminator(|raw_line| {
            line_number += 1;
            self.process_line(
                raw_line,
                line_number,
                &mut sort_buffer,
                &mut buffer_flushed,
                writer,
            )
            .map(|_| true)
        })?;

        if !buffer_flushed {
            self.flush_sorted(&mut sort_buffer, writer)?;
        }
        writer.flush()
    }

    fn process_line<W: Write>(
        &self,
        raw_line: &[u8],
        line_number: usize,
        sort_buffer: &mut Vec<Line>,
        buffer_flushed: &mut bool,
        writer: &mut W,
    ) -> io::Result<()> {
        if !self.rows.contains(&line_number) {
            if self.keep_lines_outside_rows {
                writer.write_all(raw_line)?;
            }
            return Ok(());
        }

        if self.delete_lines_in_rows {
            return Ok(());
        }

        let utf8_line = from_utf8(raw_line).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("line {line_number} is not valid UTF-8: {e}"),
            )
        })?;

        let (content, terminator) = text::split_line_terminator(utf8_line);
        let content = self.apply_transforms(content);

        if self.sort_key_cols.is_some() {
            sort_buffer.push(Line {
                content,
                terminator: terminator.to_owned(),
            });
            if line_number >= *self.rows.end() {
                self.flush_sorted(sort_buffer, writer)?;
                *buffer_flushed = true;
            }
        } else {
            writer.write_all(content.as_bytes())?;
            writer.write_all(terminator.as_bytes())?;
        }

        Ok(())
    }

    fn apply_transforms(&self, content: &str) -> String {
        self.transforms
            .iter()
            .fold(content.to_owned(), |line, transform| transform.apply(&line))
    }

    fn flush_sorted<W: Write>(&self, buffer: &mut Vec<Line>, writer: &mut W) -> io::Result<()> {
        let Some(cols) = &self.sort_key_cols else {
            return Ok(());
        };

        buffer.sort_by_cached_key(|line| text::substring(&line.content, cols));

        let last_index = buffer.len().saturating_sub(1);
        for (index, line) in buffer.iter().enumerate() {
            writer.write_all(line.content.as_bytes())?;
            if !line.terminator.is_empty() {
                writer.write_all(line.terminator.as_bytes())?;
            } else if index < last_index {
                //a line missing its terminator must not glue to the next one
                writer.write_all(NEW_LINE.as_bytes())?;
            }
        }
        buffer.clear();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn config() -> Config {
        Config {
            rows: 1..=usize::MAX,
            cols: 1..=usize::MAX,
            sort: false,
            delete: false,
            filename: String::new(),
            find_string: None,
            replace_string: None,
            output_filename: None,
        }
    }

    fn run(config: Config, input: &str) -> String {
        let processor = FileProcessor::new(&config);
        let mut output = Vec::new();
        processor
            .run(Cursor::new(input.as_bytes()), &mut output)
            .expect("processing failed");
        String::from_utf8(output).expect("output is not valid UTF-8")
    }

    #[test]
    fn passes_input_through_by_default() {
        let input = "line1\nline2\nline3\n";
        assert_eq!(run(config(), input), input);
    }

    #[test]
    fn streams_replace_without_buffering() {
        let mut config = config();
        config.find_string = Some("foo".to_owned());
        config.replace_string = Some("BAR".to_owned());

        let result = run(config, "a foo\nb foo\n");
        assert_eq!(result, "a BAR\nb BAR\n");
    }

    #[test]
    fn sorts_whole_input() {
        let mut config = config();
        config.sort = true;

        let result = run(config, "delta\nalpha\ncharlie\nbravo\n");
        assert_eq!(result, "alpha\nbravo\ncharlie\ndelta\n");
    }

    #[test]
    fn sorts_only_selected_rows() {
        let mut config = config();
        config.sort = true;
        config.rows = 2..=4;

        let result = run(config, "header\nc\na\nb\n");
        //row 1 is dropped in selection mode, rows 2-4 are sorted
        assert_eq!(result, "a\nb\nc\n");
    }

    #[test]
    fn sort_preserves_crlf_terminators() {
        let mut config = config();
        config.sort = true;

        let result = run(config, "b\r\na\r\n");
        assert_eq!(result, "a\r\nb\r\n");
    }

    #[test]
    fn sort_adds_terminator_when_unterminated_line_moves_up() {
        let mut config = config();
        config.sort = true;

        //"a" has no trailing newline and sorts before "b"
        let result = run(config, "b\na");
        assert_eq!(result, format!("a{}b\n", NEW_LINE));
    }

    #[test]
    fn replace_respects_column_boundaries_per_line() {
        let mut config = config();
        config.cols = 7..=9;
        config.find_string = Some("foo".to_owned());
        config.replace_string = Some("BAR".to_owned());

        //"foo" starts at column 7 in the first line and column 9 in the second
        let result = run(config, "delta foo\ncharlie foo\n");
        assert_eq!(result, "delta BAR\ncharlie foo\n");
    }

    #[test]
    fn delete_keeps_lines_outside_row_range() {
        let mut config = config();
        config.delete = true;
        config.rows = 2..=3;

        let result = run(config, "one\ntwo\nthree\nfour\n");
        assert_eq!(result, "one\nfour\n");
    }

    #[test]
    fn delete_columns_applies_only_to_selected_rows() {
        let mut config = config();
        config.delete = true;
        config.rows = 1..=1;
        config.cols = 1..=4;

        let result = run(config, "one one\ntwo two\n");
        assert_eq!(result, "one\ntwo two\n");
    }

    #[test]
    fn invalid_utf8_reports_line_number() {
        let processor = FileProcessor::new(&config());
        let mut output = Vec::new();
        let input: &[u8] = b"ok\n\xFF\xFE\n";

        let error = processor
            .run(Cursor::new(input), &mut output)
            .expect_err("invalid UTF-8 must fail");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("line 2"));
    }
}
