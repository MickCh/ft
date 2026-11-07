//! Sequence-level operations. A [`LineReducer`] does what no
//! [`crate::transform::LineTransform`] can: instead of turning a line
//! into lines, it consumes every processed line and writes a summary
//! once the input ends (`--count`, `--sum`, `--avg`, `--min`, `--max`,
//! `--median`, `--distinct`, `--first`, `--last`,
//! optionally per `--group-by` key).
//!
//! The engine writes the lines *or* hands them to a reducer, never both:
//! a summary replaces the rows it summarizes.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::io::{self, Write};

use crate::columns::{ColumnSpan, SplitLine};
use crate::number::Number;

/// Something computed over the processed lines as a whole.
pub trait LineReducer {
    /// Take one processed line (without its terminator). The writer is
    /// there for a reducer that can already say something — `--join`
    /// writes each line as it arrives — rather than forcing every one of
    /// them to hoard the input until the end.
    fn accept(&mut self, line: &str, writer: &mut dyn Write) -> io::Result<()>;
    /// Write what is left to write, once every line has been accepted,
    /// ending the rows it writes the way the input ended its own.
    fn finish(&mut self, ending: InputEnding, writer: &mut dyn Write) -> io::Result<()>;
}

/// How the input ended its lines, for a reducer that writes rows of its
/// own: with which terminator, and whether its last line had one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputEnding {
    /// The terminator the input's lines end with ("\r\n" or "\n"; the
    /// last one seen, and "\n" for an input that has none at all).
    pub newline: &'static str,
    /// Whether the input's last line was terminated.
    pub terminated: bool,
}

impl Default for InputEnding {
    /// An input with no lines: nothing says otherwise, so `\n`.
    fn default() -> InputEnding {
        InputEnding {
            newline: "\n",
            terminated: true,
        }
    }
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

    fn finish(&mut self, ending: InputEnding, writer: &mut dyn Write) -> io::Result<()> {
        //the one row that was written ends the way the input did: with
        //its terminator, or with none when its last line had none. With
        //no rows at all there is nothing to terminate
        if self.started && ending.terminated {
            writer.write_all(ending.newline.as_bytes())?;
        }
        Ok(())
    }
}

/// One summary column: how many rows, a statistic over the numbers found
/// in a column span, or something about the text there.
#[derive(Debug, Clone)]
pub enum Aggregate {
    Count,
    Sum(ColumnSpan),
    Avg(ColumnSpan),
    Min(ColumnSpan),
    Max(ColumnSpan),
    /// The middle number, or the mean of the two middle ones.
    Median(ColumnSpan),
    /// How many different values the span holds.
    Distinct(ColumnSpan),
    /// The span's value in the group's first row.
    First(ColumnSpan),
    /// The span's value in the group's last row.
    Last(ColumnSpan),
}

impl Aggregate {
    /// The number this aggregate reads from a line. `None` for a value
    /// that is not a number — those rows are skipped rather than counted
    /// as zero, which would bend both the sum and the average. The value
    /// is read unquoted, so `"10"` in a `--quoted` CSV field counts as 10.
    fn number(span: &ColumnSpan, line: &SplitLine<'_, '_>) -> Option<Number> {
        Number::parse(&span.select_value_in(line))
    }

    /// The columns this aggregate reads a number from; `None` for the
    /// count and the text aggregates.
    fn number_span(&self) -> Option<&ColumnSpan> {
        match self {
            Aggregate::Sum(span)
            | Aggregate::Avg(span)
            | Aggregate::Min(span)
            | Aggregate::Max(span)
            | Aggregate::Median(span) => Some(span),
            Aggregate::Count
            | Aggregate::Distinct(_)
            | Aggregate::First(_)
            | Aggregate::Last(_) => None,
        }
    }

    /// The aggregate with the span it compares values by read with the
    /// input delimiter (`--distinct`), so values with different fields
    /// never count as one.
    fn compared_as_read(self) -> Aggregate {
        match self {
            Aggregate::Distinct(span) => Aggregate::Distinct(span.keyed()),
            other => other,
        }
    }

    /// The columns this aggregate reads; `Count` reads none.
    fn span(&self) -> Option<&ColumnSpan> {
        match self {
            Aggregate::Count => None,
            Aggregate::Sum(span)
            | Aggregate::Avg(span)
            | Aggregate::Min(span)
            | Aggregate::Max(span)
            | Aggregate::Median(span)
            | Aggregate::Distinct(span)
            | Aggregate::First(span)
            | Aggregate::Last(span) => Some(span),
        }
    }
}

/// One aggregate's running value within one group. Sums stay exact
/// integers for as long as every value is one (see [`Number`]).
#[derive(Debug)]
enum Accumulator {
    Count(u64),
    Sum(Number),
    Avg {
        sum: Number,
        count: u64,
    },
    Min(Option<Number>),
    Max(Option<Number>),
    /// A median needs every value: there is no running middle.
    Median(Vec<Number>),
    Distinct(HashSet<String>),
    First(Option<String>),
    Last(Option<String>),
}

impl Accumulator {
    fn start(aggregate: &Aggregate) -> Accumulator {
        match aggregate {
            Aggregate::Count => Accumulator::Count(0),
            Aggregate::Sum(_) => Accumulator::Sum(Number::Int(0)),
            Aggregate::Avg(_) => Accumulator::Avg {
                sum: Number::Int(0),
                count: 0,
            },
            Aggregate::Min(_) => Accumulator::Min(None),
            Aggregate::Max(_) => Accumulator::Max(None),
            Aggregate::Median(_) => Accumulator::Median(Vec::new()),
            Aggregate::Distinct(_) => Accumulator::Distinct(HashSet::new()),
            Aggregate::First(_) => Accumulator::First(None),
            Aggregate::Last(_) => Accumulator::Last(None),
        }
    }

    fn accept(&mut self, aggregate: &Aggregate, line: &SplitLine<'_, '_>) {
        let Some(span) = aggregate.span() else {
            //only Count reads no columns
            if let Accumulator::Count(rows) = self {
                *rows += 1;
            }
            return;
        };
        match self {
            Accumulator::Count(_) => {}
            //the text aggregates take every row, whatever it holds: an
            //empty value is a value too. They read it as --group-by does,
            //so under --quoted `"a"` and `a` are the same value
            Accumulator::Distinct(seen) => {
                let value = span.select_canonical_in(line);
                if !seen.contains(value.as_ref()) {
                    seen.insert(value.into_owned());
                }
            }
            Accumulator::First(first) => {
                if first.is_none() {
                    *first = Some(
                        span.select_canonical_in(line)
                            .into_owned(),
                    );
                }
            }
            Accumulator::Last(last) => {
                let value = span.select_canonical_in(line);
                //reuse the previous row's allocation
                match last {
                    Some(text) => {
                        text.clear();
                        text.push_str(&value);
                    }
                    None => *last = Some(value.into_owned()),
                }
            }
            //the numeric ones are handed their number, read once per span
            //for all of them (`accept_number`)
            Accumulator::Sum(_)
            | Accumulator::Avg { .. }
            | Accumulator::Min(_)
            | Accumulator::Max(_)
            | Accumulator::Median(_) => {}
        }
    }

    fn accept_number(&mut self, value: Number) {
        match self {
            Accumulator::Sum(sum) => *sum = *sum + value,
            Accumulator::Avg { sum, count } => {
                *sum = *sum + value;
                *count += 1;
            }
            Accumulator::Min(least) => {
                *least = Some(least.map_or(value, |current| current.min(value)))
            }
            Accumulator::Max(greatest) => {
                *greatest = Some(greatest.map_or(value, |current| current.max(value)))
            }
            Accumulator::Median(values) => values.push(value),
            Accumulator::Count(_)
            | Accumulator::Distinct(_)
            | Accumulator::First(_)
            | Accumulator::Last(_) => {}
        }
    }

    /// The value to print. A group with no numbers at all has no minimum,
    /// maximum or median to show, and an average of nothing is nothing.
    fn value(&self) -> String {
        match self {
            Accumulator::Count(rows) => rows.to_string(),
            Accumulator::Sum(sum) => sum.to_string(),
            Accumulator::Avg { count: 0, .. } => String::new(),
            Accumulator::Avg { sum, count } => (sum.as_f64() / *count as f64).to_string(),
            Accumulator::Min(value) | Accumulator::Max(value) => value
                .map(|value| value.to_string())
                .unwrap_or_default(),
            Accumulator::Median(values) => median(values),
            Accumulator::Distinct(seen) => seen.len().to_string(),
            Accumulator::First(value) | Accumulator::Last(value) => {
                value.clone().unwrap_or_default()
            }
        }
    }
}

/// The middle of `values`: the middle one of an odd count, the mean of
/// the two middle ones of an even count — kept an integer when it is
/// one, so the median of 2 and 4 is `3`, of 2 and 3 `2.5`.
fn median(values: &[Number]) -> String {
    if values.is_empty() {
        return String::new();
    }
    let mut sorted = values.to_vec();
    let middle = sorted.len() / 2;
    let (below, &mut upper, _) = sorted.select_nth_unstable(middle);
    if values.len() % 2 == 1 {
        return upper.to_string();
    }
    //the lower middle is the greatest of the values below the upper one
    let lower = *below
        .iter()
        .max()
        .expect("an even count has values below the middle");
    midpoint(lower, upper)
}

fn midpoint(lower: Number, upper: Number) -> String {
    if let (Number::Int(lower), Number::Int(upper)) = (lower, upper) {
        //exact, without the sum, which may not fit: halve each value
        //(rounding down) and add back what the halving dropped
        let halves = lower.div_euclid(2) + upper.div_euclid(2);
        return match lower.rem_euclid(2) + upper.rem_euclid(2) {
            0 => halves.to_string(),
            2 => (halves + 1).to_string(),
            //`halves + 0.5`, written out: an f64 could not hold it once
            //the integers grow past 2^53
            _ if halves >= 0 => format!("{halves}.5"),
            _ => format!("-{}.5", -(halves + 1)),
        };
    }
    let (lower, upper) = (lower.as_f64(), upper.as_f64());
    (lower + (upper - lower) / 2.0).to_string()
}

/// Summarizes the processed lines: one output row per `--group-by` key
/// (in the order the keys first appear, so the summary follows the
/// input), or a single row when there is nothing to group by.
pub struct Summarize {
    /// The `--group-by` key, as it is compared (fields as read).
    key_span: Option<ColumnSpan>,
    /// The same key, as it is printed (`--output-delimiter`).
    key_label_span: Option<ColumnSpan>,
    aggregates: Vec<Aggregate>,
    separator: String,
    groups: Groups,
    /// The numbers a line holds, read once per distinct span however many
    /// aggregates ask for it (`--sum 3 --avg 3 --max 3` reads column 3
    /// once), and where each numeric aggregate finds its own.
    readings: NumberReadings,
}

/// The distinct spans the numeric aggregates read, and per aggregate the
/// one it reads — `None` for the count and the text aggregates.
struct NumberReadings {
    spans: Vec<ColumnSpan>,
    slots: Vec<Option<usize>>,
    /// The current line's numbers, one per span; kept to reuse its room.
    numbers: Vec<Option<Number>>,
}

impl NumberReadings {
    fn new(aggregates: &[Aggregate]) -> NumberReadings {
        let mut spans: Vec<ColumnSpan> = Vec::new();
        let slots = aggregates
            .iter()
            .map(|aggregate| {
                let span = aggregate.number_span()?;
                Some(
                    match spans
                        .iter()
                        .position(|known| known == span)
                    {
                        Some(slot) => slot,
                        None => {
                            spans.push(span.clone());
                            spans.len() - 1
                        }
                    },
                )
            })
            .collect();
        NumberReadings {
            spans,
            slots,
            numbers: Vec::new(),
        }
    }

    /// Read every span's number from the line, once each.
    fn read(&mut self, line: &SplitLine<'_, '_>) {
        self.numbers.clear();
        self.numbers.extend(
            self.spans
                .iter()
                .map(|span| Aggregate::number(span, line)),
        );
    }
}

/// The running accumulators per group key, kept in the order the keys
/// first appear, so the summary does not come out shuffled by the hash
/// map.
#[derive(Default)]
struct Groups {
    //per group in first-seen order: the key as it is printed, and its
    //accumulators
    groups: Vec<(String, Vec<Accumulator>)>,
    //where each key, as it is compared, has its group
    index: HashMap<String, usize>,
}

impl Groups {
    /// The accumulators of the group `key`, started for `aggregates` the
    /// first time the key appears, printed as `label` makes it. The key
    /// is looked up borrowed; only a new group takes a copy of it.
    fn get_or_start(
        &mut self,
        key: &str,
        label: impl FnOnce() -> String,
        aggregates: &[Aggregate],
    ) -> &mut Vec<Accumulator> {
        let position = match self.index.get(key) {
            Some(&position) => position,
            None => {
                let started = aggregates
                    .iter()
                    .map(Accumulator::start)
                    .collect();
                self.groups.push((label(), started));
                self.index
                    .insert(key.to_owned(), self.groups.len() - 1);
                self.groups.len() - 1
            }
        };
        &mut self.groups[position].1
    }

    /// Every group, in first-seen order, by its printed key.
    fn in_order(&self) -> impl Iterator<Item = (&str, &[Accumulator])> {
        self.groups
            .iter()
            .map(|(label, accumulators)| (label.as_str(), accumulators.as_slice()))
    }
}

impl Summarize {
    pub fn new(
        key_span: Option<ColumnSpan>,
        aggregates: Vec<Aggregate>,
        separator: impl Into<String>,
    ) -> Summarize {
        let mut groups = Groups::default();
        //without --group-by the summary describes the whole input, and
        //an input with no rows still has a description: a count of 0,
        //a sum of 0 — like `grep -c` or `wc -l`. With --group-by, no
        //rows means no groups, and no rows to print.
        if key_span.is_none() {
            groups.get_or_start("", String::new, &aggregates);
        }
        //a key and a distinct value are compared as the input holds them,
        //with its delimiter between their fields: under another output
        //delimiter `a;b`+`1` and `a`+`b;1` would otherwise meet as
        //`a;b;1`. A key is still printed in the output's form
        let aggregates: Vec<Aggregate> = aggregates
            .into_iter()
            .map(Aggregate::compared_as_read)
            .collect();
        let key_label_span = key_span.clone();
        let key_span = key_span.map(ColumnSpan::keyed);
        Summarize {
            key_span,
            key_label_span,
            readings: NumberReadings::new(&aggregates),
            aggregates,
            separator: separator.into(),
            groups,
        }
    }
}

impl LineReducer for Summarize {
    /// A summary can say nothing until the input ends, so the writer
    /// goes unused here.
    fn accept(&mut self, line: &str, _writer: &mut dyn Write) -> io::Result<()> {
        //split once for the key and every aggregate, rather than once each
        let spans = self.key_span.iter().chain(
            self.aggregates
                .iter()
                .filter_map(Aggregate::span),
        );
        let split = SplitLine::new(line, spans);

        //without --group-by every row lands in the same, unnamed group;
        //the canonical form makes `"a"` and `a` one group under --quoted
        let key = match &self.key_span {
            Some(span) => span.select_canonical_in(&split),
            None => Cow::Borrowed(""),
        };

        self.readings.read(&split);
        let label_span = &self.key_label_span;
        let label = || match label_span {
            Some(span) => span
                .select_canonical_in(&split)
                .into_owned(),
            None => String::new(),
        };
        let accumulators = self
            .groups
            .get_or_start(&key, label, &self.aggregates);
        let aggregates = self
            .aggregates
            .iter()
            .zip(&self.readings.slots);
        for ((aggregate, slot), accumulator) in aggregates.zip(accumulators) {
            match slot {
                //every numeric aggregate skips a row without a number
                Some(slot) => {
                    if let Some(value) = self.readings.numbers[*slot] {
                        accumulator.accept_number(value);
                    }
                }
                None => accumulator.accept(aggregate, &split),
            }
        }
        Ok(())
    }

    /// Every summary row is terminated, in the input's style (`\r\n` for
    /// a CRLF file), even when the input's last line was not.
    fn finish(&mut self, ending: InputEnding, writer: &mut dyn Write) -> io::Result<()> {
        for (key, accumulators) in self.groups.in_order() {
            let mut columns: Vec<String> = Vec::with_capacity(accumulators.len() + 1);
            if self.key_span.is_some() {
                columns.push(key.to_owned());
            }
            columns.extend(
                accumulators
                    .iter()
                    .map(Accumulator::value),
            );

            writer.write_all(columns.join(&self.separator).as_bytes())?;
            writer.write_all(ending.newline.as_bytes())?;
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
            .finish(InputEnding::default(), &mut output)
            .expect("writing the result failed");
        String::from_utf8(output).expect("the result is not valid UTF-8")
    }

    fn summarized(reducer: Summarize, lines: &[&str]) -> String {
        reduced(reducer, lines)
    }

    #[test]
    fn join_writes_the_rows_as_one_row() {
        assert_eq!(reduced(Join::new(","), &["a", "b", "c"]), "a,b,c\n");
        assert_eq!(reduced(Join::new(" | "), &["a", "b"]), "a | b\n");
    }

    #[test]
    fn join_ends_its_row_the_way_the_input_ended() {
        // Given a CRLF input, and one whose last line has no terminator
        let crlf = InputEnding {
            newline: "\r\n",
            terminated: true,
        };
        let unterminated = InputEnding {
            newline: "\n",
            terminated: false,
        };

        // When joining / Then the joined row ends the same way
        let joined = |ending| {
            let mut output = Vec::new();
            let mut join = Join::new(",");
            join.accept("a", &mut output).unwrap();
            join.accept("b", &mut output).unwrap();
            join.finish(ending, &mut output)
                .unwrap();
            String::from_utf8(output).unwrap()
        };
        assert_eq!(joined(crlf), "a,b\r\n");
        assert_eq!(joined(unterminated), "a,b");
    }

    #[test]
    fn a_summary_writes_its_rows_in_the_input_style() {
        // Given a count over a CRLF input
        let mut reducer = Summarize::new(None, vec![Aggregate::Count], ",");
        let mut output = Vec::new();
        reducer
            .accept("a", &mut output)
            .unwrap();

        // When it finishes / Then its row ends in CRLF
        let ending = InputEnding {
            newline: "\r\n",
            terminated: false,
        };
        reducer
            .finish(ending, &mut output)
            .unwrap();
        assert_eq!(output, b"1\r\n");
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

    #[test]
    fn median_takes_the_middle_of_an_odd_count() {
        // Given / When
        let reducer = Summarize::new(None, vec![Aggregate::Median(field(1))], ",");

        // Then the values are ordered first, and a non-number is skipped
        assert_eq!(
            summarized(reducer, &["8", "x", "1", "3"]),
            "3
"
        );
    }

    #[test]
    fn median_of_an_even_count_is_the_mean_of_the_middle_two() {
        // Given / When / Then: an integer while it is one
        let median = |lines: &[&str]| {
            summarized(
                Summarize::new(None, vec![Aggregate::Median(field(1))], ","),
                lines,
            )
        };
        assert_eq!(
            median(&["4", "2"]),
            "3
"
        );
        assert_eq!(
            median(&["2", "3"]),
            "2.5
"
        );
        assert_eq!(
            median(&["1.5", "2", "9", "-4"]),
            "1.75
"
        );
        assert_eq!(
            median(&[]),
            "
"
        );
    }

    #[test]
    fn median_of_extreme_integers_does_not_overflow() {
        // Given the two ends of i128
        let lines = [i128::MIN.to_string(), i128::MAX.to_string()];
        let lines: Vec<&str> = lines
            .iter()
            .map(String::as_str)
            .collect();

        // When
        let reducer = Summarize::new(None, vec![Aggregate::Median(field(1))], ",");

        // Then their midpoint is exact, not rounded by an f64
        assert_eq!(
            summarized(reducer, &lines),
            "-0.5
"
        );
    }

    #[test]
    fn distinct_counts_different_values_per_group() {
        // Given / When
        let reducer = Summarize::new(Some(field(1)), vec![Aggregate::Distinct(field(2))], ",");
        let result = summarized(reducer, &["a,x", "b,y", "a,y", "a,x", "a,"]);

        // Then an empty value counts as one too
        assert_eq!(
            result,
            "a,3
b,1
"
        );
    }

    #[test]
    fn distinct_compares_quoted_values_by_value() {
        // Given / When
        let span = ColumnSpan::Fields(crate::columns::FieldSpan {
            quoted: true,
            ..crate::columns::FieldSpan::new(",", ColumnList::from(1..=1))
        });
        let reducer = Summarize::new(None, vec![Aggregate::Distinct(span)], ",");

        // Then
        assert_eq!(
            summarized(reducer, &["\"a\"", "a", "b"]),
            "2
"
        );
    }

    #[test]
    fn first_and_last_take_the_ends_of_each_group() {
        // Given / When
        let reducer = Summarize::new(
            Some(field(1)),
            vec![Aggregate::First(field(2)), Aggregate::Last(field(2))],
            ",",
        );
        let result = summarized(reducer, &["a,1", "b,2", "a,x", "a,3", "b,4"]);

        // Then any text will do, not just numbers
        assert_eq!(
            result,
            "a,1,3
b,2,4
"
        );
    }

    #[test]
    fn aggregates_sharing_a_span_read_it_once_and_agree() {
        // Given four statistics over column 2, one over column 3, and the
        // reading table that serves them
        let aggregates = vec![
            Aggregate::Sum(field(2)),
            Aggregate::Count,
            Aggregate::Min(field(2)),
            Aggregate::Max(field(3)),
            Aggregate::Avg(field(2)),
            Aggregate::Median(field(2)),
        ];
        let readings = NumberReadings::new(&aggregates);

        // When
        let reducer = Summarize::new(Some(field(1)), aggregates, ",");
        let result = summarized(reducer, &["a,1,9", "a,x,7", "b,4,y", "a,5,8"]);

        // Then two spans are read per line, and each statistic is its own
        assert_eq!(readings.spans.len(), 2);
        assert_eq!(
            result,
            "a,6,3,1,9,3,3
b,4,1,4,,4,4
"
        );
    }
}
