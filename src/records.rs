//! Rows as the engine reads them. Normally a row is a line; under
//! `--quoted` it is a CSV *record*, which goes on over the next line for
//! as long as a quoted field is open — a newline inside `"…"` is part of
//! the field, not the end of the row (RFC 4180).

use std::borrow::Cow;
use std::io;

/// Assembles rows from the lines of the input, handed to it one by one.
/// Joining lines needs to know only where quotes are: `"` is ASCII, so
/// it never occurs inside a multi-byte char, and the bytes need not be
/// valid UTF-8 for a record to be found.
pub struct Records {
    /// Whether a quoted field may span lines (`--quoted`); otherwise
    /// every line is a row of its own.
    quoted: bool,
    /// The lines of an unfinished record, joined with their terminators.
    pending: Vec<u8>,
    /// Lines read so far, for an error message to point at.
    line_number: usize,
    /// The line an unfinished record started on.
    pending_from: usize,
}

impl Records {
    pub fn new(quoted: bool) -> Self {
        Self {
            quoted,
            pending: Vec::new(),
            line_number: 0,
            pending_from: 0,
        }
    }

    /// Take the next line (with its terminator). Returns the row it
    /// completes, if any: the line itself — borrowed, so a plain line
    /// costs nothing — or the record it closes.
    pub fn push<'line>(&mut self, line: &'line [u8]) -> Option<Cow<'line, [u8]>> {
        self.line_number += 1;
        if !self.quoted {
            return Some(Cow::Borrowed(line));
        }

        let flips_quote = quote_count(line) % 2 == 1;
        if self.pending.is_empty() {
            if !flips_quote {
                return Some(Cow::Borrowed(line));
            }
            //a quoted field stays open: the record goes on
            self.pending_from = self.line_number;
            self.pending.extend_from_slice(line);
            return None;
        }

        self.pending.extend_from_slice(line);
        //an odd count closes the field left open — a doubled `""` inside
        //it is even, so it changes nothing
        flips_quote.then(|| Cow::Owned(std::mem::take(&mut self.pending)))
    }

    /// The input is over: a record still open was never closed, and
    /// reading it as a row would cut it off wherever the input ends.
    pub fn finish(&self) -> io::Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "the quoted field opened on line {} is never closed",
                self.pending_from
            ),
        ))
    }
}

fn quote_count(line: &[u8]) -> usize {
    line.iter()
        .filter(|&&byte| byte == b'"')
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feed `lines` through and collect the rows that come out.
    fn rows(quoted: bool, lines: &[&str]) -> (Vec<String>, io::Result<()>) {
        let mut records = Records::new(quoted);
        let rows = lines
            .iter()
            .filter_map(|line| records.push(line.as_bytes()))
            .map(|row| String::from_utf8(row.into_owned()).unwrap())
            .collect();
        (rows, records.finish())
    }

    #[test]
    fn without_quoting_every_line_is_a_row() {
        // Given / When
        let (rows, end) = rows(false, &["1,\"a\n", "b\",2\n"]);

        // Then
        assert_eq!(rows, ["1,\"a\n", "b\",2\n"]);
        assert!(end.is_ok());
    }

    #[test]
    fn an_open_quote_joins_the_next_lines() {
        // Given a record whose field spans three lines
        let lines = ["1,\"a\n", "b\n", "c\",2\n", "3,x\n"];

        // When
        let (rows, end) = rows(true, &lines);

        // Then
        assert_eq!(rows, ["1,\"a\nb\nc\",2\n", "3,x\n"]);
        assert!(end.is_ok());
    }

    #[test]
    fn crlf_inside_a_field_is_kept() {
        // Given / When
        let (rows, _) = rows(true, &["\"a\r\n", "b\"\r\n"]);

        // Then
        assert_eq!(rows, ["\"a\r\nb\"\r\n"]);
    }

    #[test]
    fn escaped_quotes_do_not_close_a_field() {
        // Given a continuation line holding a doubled quote only
        let lines = ["\"say\n", "\"\"hi\"\"\n", "now\",1\n"];

        // When
        let (rows, _) = rows(true, &lines);

        // Then
        assert_eq!(rows, ["\"say\n\"\"hi\"\"\nnow\",1\n"]);
    }

    #[test]
    fn balanced_quotes_on_one_line_make_a_row() {
        // Given / When
        let (rows, _) = rows(true, &["1,\"say \"\"hi\"\"\",2\n", "plain"]);

        // Then
        assert_eq!(rows, ["1,\"say \"\"hi\"\"\",2\n", "plain"]);
    }

    #[test]
    fn a_quote_left_open_at_the_end_is_an_error() {
        // Given / When
        let (rows, end) = rows(true, &["1,x\n", "2,\"open\n", "still\n"]);

        // Then
        assert_eq!(rows, ["1,x\n"]);
        let message = end.unwrap_err().to_string();
        assert!(message.contains("opened on line 2"), "{message}");
    }
}
