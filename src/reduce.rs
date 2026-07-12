//! Sequence-level operations. A [`LineReducer`] does what no
//! [`crate::transform::LineTransform`] can: instead of turning a line
//! into lines, it consumes every processed line and writes a summary
//! once the input ends (`--count`, `--sum`, `--avg`, `--min`, `--max`,
//! optionally per `--group-by` key).
//!
//! The engine writes the lines *or* hands them to a reducer, never both:
//! a summary replaces the rows it summarizes.

use std::collections::HashMap;
use std::io::{self, Write};

use crate::columns::ColumnSpan;
use crate::constants::NEW_LINE;

/// Something computed over the processed lines as a whole.
pub trait LineReducer {
    /// Take one processed line (without its terminator). The writer is
    /// there for a reducer that can already say something — `--join`
    /// writes each line as it arrives — rather than forcing every one of
    /// them to hoard the input until the end.
    fn accept(&mut self, line: &str, writer: &mut dyn Write) -> io::Result<()>;
    /// Write what is left to write, once every line has been accepted.
    fn finish(&mut self, writer: &mut dyn Write) -> io::Result<()>;
}

/// Joins every processed line into a single row (`paste -s`). It has
/// nothing to accumulate: each line is written as it arrives, with the
/// separator in front of all but the first.
pub struct Join {
    separator: String,
    started: bool,
}

impl Join {
    pub fn new(separator: impl Into<String>) -> Join {
        Join {
            separator: separator.into(),
            started: false,
        }
    }
}

impl LineReducer for Join {
    fn accept(&mut self, line: &str, writer: &mut dyn Write) -> io::Result<()> {
        if self.started {
            writer.write_all(self.separator.as_bytes())?;
        }
        writer.write_all(line.as_bytes())?;
        self.started = true;
        Ok(())
    }

    fn finish(&mut self, writer: &mut dyn Write) -> io::Result<()> {
        //the one row that was written still needs its terminator; with no
        //rows at all there is nothing to terminate
        if self.started {
            writer.write_all(NEW_LINE.as_bytes())?;
        }
        Ok(())
    }
}

/// One summary column: how many rows, or a statistic over the numbers
/// found in a column span.
#[derive(Debug, Clone)]
pub enum Aggregate {
    Count,
    Sum(ColumnSpan),
    Avg(ColumnSpan),
    Min(ColumnSpan),
    Max(ColumnSpan),
}

impl Aggregate {
    /// The number this aggregate reads from a line. `None` for `Count`,
    /// which counts rows rather than values, and for a value that is not
    /// a number — those rows are skipped rather than counted as zero,
    /// which would bend both the sum and the average.
    fn number(&self, line: &str) -> Option<f64> {
        let span = match self {
            Aggregate::Count => return None,
            Aggregate::Sum(span)
            | Aggregate::Avg(span)
            | Aggregate::Min(span)
            | Aggregate::Max(span) => span,
        };
        span.select(line)
            .trim()
            .parse()
            .ok()
            .filter(|value: &f64| !value.is_nan())
    }
}

/// One aggregate's running value within one group.
#[derive(Debug)]
enum Accumulator {
    Count(u64),
    Sum(f64),
    Avg { sum: f64, count: u64 },
    Min(Option<f64>),
    Max(Option<f64>),
}

impl Accumulator {
    fn start(aggregate: &Aggregate) -> Accumulator {
        match aggregate {
            Aggregate::Count => Accumulator::Count(0),
            Aggregate::Sum(_) => Accumulator::Sum(0.0),
            Aggregate::Avg(_) => Accumulator::Avg { sum: 0.0, count: 0 },
            Aggregate::Min(_) => Accumulator::Min(None),
            Aggregate::Max(_) => Accumulator::Max(None),
        }
    }

    fn accept(&mut self, aggregate: &Aggregate, line: &str) {
        if let Accumulator::Count(rows) = self {
            *rows += 1;
            return;
        }
        //every other aggregate reads a number, and a row without one
        //takes no part in it
        let Some(value) = aggregate.number(line) else {
            return;
        };
        match self {
            Accumulator::Count(_) => unreachable!("counted above"),
            Accumulator::Sum(sum) => *sum += value,
            Accumulator::Avg { sum, count } => {
                *sum += value;
                *count += 1;
            }
            Accumulator::Min(least) => {
                *least = Some(match least {
                    Some(current) => current.min(value),
                    None => value,
                })
            }
            Accumulator::Max(greatest) => {
                *greatest = Some(match greatest {
                    Some(current) => current.max(value),
                    None => value,
                })
            }
        }
    }

    /// The value to print. A group with no numbers at all has no minimum
    /// or maximum to show, and an average of nothing is nothing.
    fn value(&self) -> String {
        match self {
            Accumulator::Count(rows) => rows.to_string(),
            Accumulator::Sum(sum) => sum.to_string(),
            Accumulator::Avg { count: 0, .. } => String::new(),
            Accumulator::Avg { sum, count } => (sum / *count as f64).to_string(),
            Accumulator::Min(value) | Accumulator::Max(value) => value
                .map(|value| value.to_string())
                .unwrap_or_default(),
        }
    }
}

/// Summarizes the processed lines: one output row per `--group-by` key
/// (in the order the keys first appear, so the summary follows the
/// input), or a single row when there is nothing to group by.
pub struct Summarize {
    key_span: Option<ColumnSpan>,
    aggregates: Vec<Aggregate>,
    separator: String,
    //keys in first-seen order, so the summary does not come out shuffled
    //by the hash map
    order: Vec<String>,
    groups: HashMap<String, Vec<Accumulator>>,
}

impl Summarize {
    pub fn new(
        key_span: Option<ColumnSpan>,
        aggregates: Vec<Aggregate>,
        separator: impl Into<String>,
    ) -> Summarize {
        let mut summarize = Summarize {
            key_span,
            aggregates,
            separator: separator.into(),
            order: Vec::new(),
            groups: HashMap::new(),
        };
        //without --group-by the summary describes the whole input, and
        //an input with no rows still has a description: a count of 0,
        //a sum of 0 — like `grep -c` or `wc -l`. With --group-by, no
        //rows means no groups, and no rows to print.
        if summarize.key_span.is_none() {
            summarize.start_group(String::new());
        }
        summarize
    }

    /// Register a group the first time its key appears, keeping the
    /// first-seen order the summary is printed in.
    fn start_group(&mut self, key: String) {
        self.order.push(key.clone());
        let accumulators = self
            .aggregates
            .iter()
            .map(Accumulator::start)
            .collect();
        self.groups.insert(key, accumulators);
    }
}

impl LineReducer for Summarize {
    /// A summary can say nothing until the input ends, so the writer
    /// goes unused here.
    fn accept(&mut self, line: &str, _writer: &mut dyn Write) -> io::Result<()> {
        //without --group-by every row lands in the same, unnamed group
        let key = match &self.key_span {
            Some(span) => span.select(line).into_owned(),
            None => String::new(),
        };

        if !self.groups.contains_key(&key) {
            self.start_group(key.clone());
        }

        let Some(accumulators) = self.groups.get_mut(&key) else {
            return Ok(());
        };
        for (aggregate, accumulator) in self.aggregates.iter().zip(accumulators) {
            accumulator.accept(aggregate, line);
        }
        Ok(())
    }

    fn finish(&mut self, writer: &mut dyn Write) -> io::Result<()> {
        for key in &self.order {
            let Some(accumulators) = self.groups.get(key) else {
                continue;
            };

            let mut columns: Vec<String> = Vec::with_capacity(accumulators.len() + 1);
            if self.key_span.is_some() {
                columns.push(key.clone());
            }
            columns.extend(
                accumulators
                    .iter()
                    .map(Accumulator::value),
            );

            writeln!(writer, "{}", columns.join(&self.separator))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::columns::{ColumnList, ColumnSpan};

    fn field(index: usize) -> ColumnSpan {
        ColumnSpan::fields(",", ColumnList::from(index..=index))
    }

    /// Everything a reducer writes over the lines it is given.
    fn reduced(mut reducer: impl LineReducer, lines: &[&str]) -> String {
        let mut output = Vec::new();
        for line in lines {
            reducer
                .accept(line, &mut output)
                .expect("accepting a line failed");
        }
        reducer
            .finish(&mut output)
            .expect("writing the result failed");
        String::from_utf8(output).expect("the result is not valid UTF-8")
    }

    fn summarized(reducer: Summarize, lines: &[&str]) -> String {
        reduced(reducer, lines)
    }

    #[test]
    fn join_writes_the_rows_as_one_row() {
        assert_eq!(
            reduced(Join::new(","), &["a", "b", "c"]),
            format!("a,b,c{NEW_LINE}")
        );
        assert_eq!(
            reduced(Join::new(" | "), &["a", "b"]),
            format!("a | b{NEW_LINE}")
        );
    }

    #[test]
    fn join_writes_nothing_when_there_are_no_rows() {
        //no row means no row to terminate either
        assert_eq!(reduced(Join::new(","), &[]), "");
    }

    #[test]
    fn counts_every_row_without_a_group() {
        let reducer = Summarize::new(None, vec![Aggregate::Count], ",");
        assert_eq!(summarized(reducer, &["a", "b", "c"]), "3\n");
    }

    #[test]
    fn an_input_with_no_rows_still_has_a_summary() {
        //like `grep -c` or `wc -l`: no rows is a count of 0, not silence
        let reducer = Summarize::new(None, vec![Aggregate::Count], ",");
        assert_eq!(summarized(reducer, &[]), "0\n");

        let reducer = Summarize::new(
            None,
            vec![
                Aggregate::Count,
                Aggregate::Sum(field(1)),
                Aggregate::Avg(field(1)),
            ],
            ",",
        );
        //a sum of nothing is 0; an average of nothing is nothing
        assert_eq!(summarized(reducer, &[]), "0,0,\n");
    }

    #[test]
    fn group_by_with_no_rows_prints_no_groups() {
        //with a key there is nothing to describe: no rows, no groups
        let reducer = Summarize::new(Some(field(1)), vec![Aggregate::Count], ",");
        assert_eq!(summarized(reducer, &[]), "");
    }

    #[test]
    fn counts_rows_per_group_in_first_seen_order() {
        let reducer = Summarize::new(Some(field(1)), vec![Aggregate::Count], ",");
        //"b" appears before "a", and the summary keeps that order
        let result = summarized(reducer, &["b,1", "a,2", "b,3"]);
        assert_eq!(result, "b,2\na,1\n");
    }

    #[test]
    fn sums_and_averages_a_numeric_column() {
        let reducer = Summarize::new(
            Some(field(1)),
            vec![Aggregate::Sum(field(2)), Aggregate::Avg(field(2))],
            ",",
        );
        let result = summarized(reducer, &["a,1", "a,3", "b,10"]);
        assert_eq!(result, "a,4,2\nb,10,10\n");
    }

    #[test]
    fn reports_the_smallest_and_largest_value() {
        let reducer = Summarize::new(
            None,
            vec![Aggregate::Min(field(2)), Aggregate::Max(field(2))],
            ",",
        );
        let result = summarized(reducer, &["a,3", "a,-1.5", "a,7"]);
        assert_eq!(result, "-1.5,7\n");
    }

    #[test]
    fn rows_without_a_number_take_no_part_in_the_statistics() {
        let reducer = Summarize::new(
            None,
            vec![
                Aggregate::Count,
                Aggregate::Sum(field(2)),
                Aggregate::Avg(field(2)),
            ],
            ",",
        );
        //3 rows, but only two of them carry a number: the average is 2,
        //not 4/3 — a missing value is not a zero
        let result = summarized(reducer, &["a,1", "a,x", "a,3"]);
        assert_eq!(result, "3,4,2\n");
    }

    #[test]
    fn a_group_without_any_number_has_nothing_to_report() {
        let reducer = Summarize::new(
            None,
            vec![Aggregate::Min(field(2)), Aggregate::Avg(field(2))],
            ",",
        );
        assert_eq!(summarized(reducer, &["a,x"]), ",\n");
    }

    #[test]
    fn combines_a_count_with_a_sum_per_group() {
        let reducer = Summarize::new(
            Some(field(1)),
            vec![Aggregate::Count, Aggregate::Sum(field(2))],
            ",",
        );
        let result = summarized(reducer, &["a,1", "b,2", "a,4"]);
        assert_eq!(result, "a,2,5\nb,1,2\n");
    }
}
