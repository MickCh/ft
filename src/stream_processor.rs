//! Streaming engine: applies row selection, the per-line transform
//! pipeline and optional reordering to any `BufRead` source and `Write`
//! sink. The processor is assembled field by field from already-built
//! parts (see [`crate::compose`]), so this module stays independent of
//! the CLI layer.

use crate::columns::ColumnSpan;
use crate::number::Number;
use crate::pattern_range::PatternRange;
use crate::predicate::LinePredicate;
use crate::ranges::{RangeSet, RangeSpec};
use crate::records::Records;
use crate::reduce::{InputEnding, LineReducer};
use crate::text;
use crate::transform::{Lines, Pipeline};

use bstr::io::BufReadExt;
use std::borrow::Cow;
use std::collections::{HashSet, VecDeque};
use std::io;
use std::io::{BufRead, Write};
use std::str::from_utf8;

/// Lines held back for reordering, stored back to back in one string:
/// a single allocation grows for all of them (and is kept for the next
/// run of rows) instead of one per line, and reordering moves only their
/// small [`Line`] records, never the text.
#[derive(Default)]
struct LineBuffer {
    text: String,
    lines: Vec<Line>,
}

/// Where a buffered line lies in its buffer's text, and the terminator
/// it came with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Line {
    start: usize,
    end: usize,
    terminator: &'static str,
}

impl LineBuffer {
    fn push(&mut self, content: &str, terminator: &'static str) {
        let start = self.text.len();
        self.text.push_str(content);
        self.lines.push(Line {
            start,
            end: self.text.len(),
            terminator,
        });
    }

    fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    fn content(&self, line: Line) -> &str {
        &self.text[line.start..line.end]
    }

    /// The buffered lines' contents, in their current order.
    fn contents(&self) -> impl Iterator<Item = &str> {
        self.lines
            .iter()
            .map(|&line| self.content(line))
    }

    /// Empty the buffer, keeping its allocations for what comes next.
    fn clear(&mut self) {
        self.text.clear();
        self.lines.clear();
    }
}

/// The first eight bytes of `key` as a big-endian number, zero-padded:
/// keys whose numbers differ compare as the numbers do, so it can settle
/// a comparison before the keys themselves are read. (Padding cannot
/// mislead: where a shorter key's padding meets a nonzero byte, the
/// shorter key is a prefix of the other and sorts first either way.)
fn byte_prefix(key: &str) -> u64 {
    let mut prefix = [0; 8];
    let len = key.len().min(8);
    prefix[..len].copy_from_slice(&key.as_bytes()[..len]);
    u64::from_be_bytes(prefix)
}

/// Mutable state threaded through one `run` call.
#[derive(Default)]
struct RunState {
    //lines held back for reordering
    reorder_buffer: LineBuffer,
    //unique keys already written (`--unique`)
    seen_keys: HashSet<String>,
    //the line just written carried no terminator (it was the input's
    //last), so anything written after it needs one put in between
    needs_separator: bool,
    //how the input ends its lines, as far as it has been read: what a
    //line that owes a terminator gets, and how a reducer ends its rows
    ending: InputEnding,
    //at least one row was selected for processing (`RunOutcome::matched`)
    matched: bool,
}

/// What a run has to say for itself once it is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunOutcome {
    /// Whether any row was selected for processing: it lay inside the
    /// row range *and* satisfied the predicate. This is what a `grep`-like
    /// exit code reports, so it counts the rows a `--delete` removed just
    /// the same — they matched, which is why they went.
    pub matched: bool,
}

/// How to order the buffered lines: by which columns, compared
/// lexicographically or numerically, ascending or descending.
pub struct SortSpec {
    pub key_span: ColumnSpan,
    pub numeric: bool,
    pub reverse: bool,
}

/// A sequence-breaking operation: unlike per-line transforms, it needs
/// the whole row range buffered before anything can be written.
pub enum Reorder {
    Sort(SortSpec),
    /// Reverse the order of the buffered lines, like `tac`.
    Tac,
    /// Write the buffered lines in random order.
    Shuffle,
}

/// What happens to a row depending on whether it lies inside the row
/// range (selected) or outside it. Encoding the three valid
/// combinations as one enum keeps contradictory ones (drop everything)
/// unrepresentable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowMode {
    /// Keep only the selected rows and process them; drop the rest.
    Select,
    /// Delete the selected rows (those matching the predicate, if any)
    /// and keep the rest verbatim.
    DeleteSelected,
    /// Process the selected rows and keep the rest verbatim
    /// (`--keep-others`, and delete mode with a column range: a
    /// transform removes the columns, the rows themselves survive).
    EditSelected,
}

impl RowMode {
    /// Whether rows outside the row range pass through unchanged.
    fn keeps_unselected(self) -> bool {
        matches!(self, RowMode::DeleteSelected | RowMode::EditSelected)
    }

    /// Whether rows inside the row range are dropped.
    fn deletes_selected(self) -> bool {
        self == RowMode::DeleteSelected
    }
}

/// Write one line of content, terminated as it was on input. An input
/// line without a terminator is the last one of the file — but it may
/// have been reordered, or expanded into several lines, so it can end
/// up with output after it. Rather than looking ahead, the terminator
/// it owes is only written once something actually follows it.
fn write_content<W: Write>(
    content: &str,
    terminator: &str,
    state: &mut RunState,
    writer: &mut W,
) -> io::Result<()> {
    write_pending_separator(state, writer)?;
    writer.write_all(content.as_bytes())?;

    if terminator.is_empty() {
        state.needs_separator = true;
    } else {
        writer.write_all(terminator.as_bytes())?;
    }
    Ok(())
}

fn write_pending_separator<W: Write>(state: &mut RunState, writer: &mut W) -> io::Result<()> {
    if state.needs_separator {
        //the terminator the input's other lines end with, not the
        //platform's: a CRLF file stays CRLF, an LF one LF, everywhere
        writer.write_all(state.ending.newline.as_bytes())?;
        state.needs_separator = false;
    }
    Ok(())
}

/// The streaming processor, configured by its fields and assembled by
/// the composition layer.
pub struct StreamProcessor {
    /// Rows taking part in processing (every row when no range given).
    pub rows: RangeSpec,
    /// `--from`/`--to`: rows addressed by content, resolved as the lines
    /// stream past. A row is processed when it lies in `rows` *and* here.
    pub pattern_range: Option<PatternRange>,
    /// What happens to rows inside vs outside the row range.
    pub row_mode: RowMode,
    /// `None` means lines stream straight to the writer.
    pub reorder: Option<Reorder>,
    /// Content filter applied to lines within the row range.
    pub predicate: Option<Box<dyn LinePredicate>>,
    /// Key columns for `--unique`; `None` means duplicates are kept.
    pub unique_key_span: Option<ColumnSpan>,
    /// Per-line transforms, applied in order.
    pub transforms: Pipeline,
    /// Transforms for each line on its way out, after `--unique` let it
    /// pass (`--number`, which counts the rows actually written).
    pub outgoing: Pipeline,
    /// `Some` summarizes the processed lines instead of writing them
    /// (`--count`, `--sum`, …): the summary replaces the rows.
    pub reducer: Option<Box<dyn LineReducer>>,
    /// Stop reading as soon as a row matches (`--quiet`, which asks only
    /// whether anything matches at all).
    pub stop_when_matched: bool,
    /// Rows are CSV records (`--quoted`): a line that leaves a `"quoted"`
    /// field open goes on in the next, and the row is the whole record.
    /// `--rows`, `--number` and error messages then count records.
    pub quoted_records: bool,
    /// A processed row that is not valid UTF-8 has its invalid bytes
    /// replaced with U+FFFD (`--lossy`) instead of failing the run. Rows
    /// passed through untouched keep their bytes as they were.
    pub lossy: bool,
}

impl StreamProcessor {
    /// Stream `reader` line by line into `writer`, applying the configured
    /// row selection, per-line transforms and optional reordering.
    /// Takes `self`: a reducer accumulates into itself, and a processor
    /// is built fresh per input anyway, so there is nothing to reuse.
    pub fn run<R: BufRead, W: Write>(
        mut self,
        mut reader: R,
        writer: &mut W,
    ) -> io::Result<RunOutcome> {
        let mut state = RunState::default();

        if self.rows.is_absolute() {
            //the total line count is irrelevant, so lines can stream through
            let rows = self.rows.resolve(usize::MAX);
            let mut records = Records::new(self.quoted_records);
            let mut row_number = 0usize;
            reader.for_byte_line_with_terminator(|raw_line| {
                let Some(row) = records.push(raw_line) else {
                    return Ok(true);
                };
                row_number += 1;
                self.process_line(&row, row_number, &rows, &mut state, writer)?;
                //asked only whether anything matches, there is no reason
                //to read the rest of the input once something has
                Ok(!(self.stop_when_matched && state.matched))
            })?;
            //stopping early leaves the rest unread, open quote or not
            if !(self.stop_when_matched && state.matched) {
                records.finish()?;
            }
        } else if let Some(tail_len) = self.rows.tail_len() {
            //every part starts counting from the end (`~10-`): nothing
            //before the last `tail_len` rows can be selected, so only they
            //are held, in a ring, until the line count is known
            let tail_len = tail_len.max(1);
            let unselected = RangeSet::new(Vec::new());
            let mut records = Records::new(self.quoted_records);
            let mut tail: VecDeque<Vec<u8>> = VecDeque::new();
            let mut row_number = 0usize;
            reader.for_byte_line_with_terminator(|raw_line| {
                let Some(row) = records.push(raw_line) else {
                    return Ok(true);
                };
                row_number += 1;
                //the row leaving the ring lies before the tail: it goes
                //the way of every unselected row — on through `--from`,
                //and out unchanged where the mode keeps such rows
                let mut slot = Vec::new();
                if tail.len() == tail_len
                    && let Some(evicted) = tail.pop_front()
                {
                    let evicted_number = row_number - tail_len;
                    self.process_line(&evicted, evicted_number, &unselected, &mut state, writer)?;
                    slot = evicted;
                }
                //the allocation is reused for the row coming in
                slot.clear();
                slot.extend_from_slice(&row);
                tail.push_back(slot);
                Ok(true)
            })?;
            records.finish()?;
            let rows = self.rows.resolve(row_number);
            let first = row_number - tail.len() + 1;
            for (offset, row) in tail.iter().enumerate() {
                self.process_line(row, first + offset, &rows, &mut state, writer)?;
                if self.stop_when_matched && state.matched {
                    break;
                }
            }
        } else {
            //bounds counted from both ends (`2-~2`) only resolve once the
            //total line count is known: the whole input must be buffered,
            //back to back in one allocation, each row remembered by its end
            let mut records = Records::new(self.quoted_records);
            let mut bytes = Vec::new();
            let mut ends = Vec::new();
            reader.for_byte_line_with_terminator(|raw_line| {
                if let Some(row) = records.push(raw_line) {
                    bytes.extend_from_slice(&row);
                    ends.push(bytes.len());
                }
                Ok(true)
            })?;
            records.finish()?;
            let rows = self.rows.resolve(ends.len());
            let starts = std::iter::once(0).chain(ends.iter().copied());
            for (index, (start, end)) in starts
                .zip(ends.iter().copied())
                .enumerate()
            {
                self.process_line(&bytes[start..end], index + 1, &rows, &mut state, writer)?;
                if self.stop_when_matched && state.matched {
                    break;
                }
            }
        }

        self.flush_reordered(&mut state, writer)?;
        //the summary replaces the rows it summarizes, so it is all the
        //output there is
        if let Some(reducer) = &mut self.reducer {
            reducer.finish(state.ending, writer)?;
        }
        writer.flush()?;

        Ok(RunOutcome {
            matched: state.matched,
        })
    }

    fn process_line<W: Write>(
        &mut self,
        raw_line: &[u8],
        line_number: usize,
        rows: &RangeSet,
        state: &mut RunState,
        writer: &mut W,
    ) -> io::Result<()> {
        //every line tells how the input ends its lines
        let terminator = text::line_terminator_of(raw_line);
        state.ending.terminated = !terminator.is_empty();
        if state.ending.terminated {
            state.ending.newline = terminator;
        }

        //every line moves the pattern range along — also one outside
        //`--rows`, since it may open or close a block all the same
        let in_pattern_range = match &mut self.pattern_range {
            Some(range) => range.admits(text::without_line_terminator(raw_line))?,
            None => true,
        };

        if !in_pattern_range || !rows.contains(line_number) {
            if self.row_mode.keeps_unselected() {
                self.write_kept_line(raw_line, state, writer)?;
            }
            return Ok(());
        }

        //without a predicate, deleting whole rows needs no UTF-8 look; the
        //row is still a match — `--from`/`--to` may be why it goes
        if self.row_mode.deletes_selected() && self.predicate.is_none() {
            state.matched = true;
            return Ok(());
        }

        let utf8_line = match (from_utf8(raw_line), self.lossy) {
            (Ok(line), _) => Cow::Borrowed(line),
            (Err(_), true) => String::from_utf8_lossy(raw_line),
            (Err(e), false) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "{} {line_number} is not valid UTF-8: {e} (--lossy replaces it)",
                        self.row_noun()
                    ),
                ));
            }
        };

        let (content, terminator) = text::split_line_terminator(&utf8_line);

        if let Some(predicate) = &self.predicate
            && !predicate.matches(content)?
        {
            //a non-matching line is treated like one outside the row range
            if self.row_mode.keeps_unselected() {
                self.write_kept_line(raw_line, state, writer)?;
            }
            return Ok(());
        }

        //the row lies in the range and satisfied the predicate: it is a
        //match, whether it is about to be rewritten or deleted
        state.matched = true;

        if self.row_mode.deletes_selected() {
            return Ok(());
        }

        //a transform may expand the line into several, or drop it
        match self.transforms.apply(content)? {
            //under `--lossy`, a repaired row the pipeline left as it was
            //goes out as it came in, invalid bytes and all — replacing
            //them is only worth it where the row is rewritten anyway, so
            //`--lossy -i` changes no more of a file than the edit does
            Lines::One(Cow::Borrowed(kept))
                if matches!(utf8_line, Cow::Owned(_))
                    && kept.len() == content.len()
                    && self.reorder.is_none()
                    && self.reducer.is_none()
                    && self.outgoing.is_empty() =>
            {
                if self.passes_unique(kept, &mut state.seen_keys) {
                    self.write_kept_line(raw_line, state, writer)?;
                }
                Ok(())
            }
            Lines::One(content) => self.emit(&content, terminator, state, writer),
            Lines::Several(contents) => contents
                .iter()
                .try_for_each(|content| self.emit(content, terminator, state, writer)),
        }
    }

    /// What a row is called in an error message: under `--quoted` it is a
    /// record, which may span several lines.
    fn row_noun(&self) -> &'static str {
        if self.quoted_records {
            "record"
        } else {
            "line"
        }
    }

    /// Take one line the pipeline produced. Reordering holds it back —
    /// its place is not known until the run of selected rows ends — and
    /// everything else about it is settled later, on the way out.
    fn emit<W: Write>(
        &mut self,
        content: &str,
        terminator: &'static str,
        state: &mut RunState,
        writer: &mut W,
    ) -> io::Result<()> {
        if self.reorder.is_some() {
            state
                .reorder_buffer
                .push(content, terminator);
            return Ok(());
        }
        self.output(content, terminator, state, writer)
    }

    /// Put out one line, in the order it is to appear: dropped by
    /// `--unique`, swallowed by a reducer (what reaches the writer is
    /// then whatever the reducer makes of the rows) or written as it is.
    /// Reordered lines take this path too, once their order is known, so
    /// a reducer sees them in the order they are put out.
    fn output<W: Write>(
        &mut self,
        content: &str,
        terminator: &str,
        state: &mut RunState,
        writer: &mut W,
    ) -> io::Result<()> {
        if !self.passes_unique(content, &mut state.seen_keys) {
            return Ok(());
        }
        if self.outgoing.is_empty() {
            return self.deliver(content, terminator, state, writer);
        }
        match self.outgoing.apply(content)? {
            Lines::One(content) => self.deliver(&content, terminator, state, writer),
            Lines::Several(contents) => contents
                .iter()
                .try_for_each(|content| self.deliver(content, terminator, state, writer)),
        }
    }

    /// Hand a finished line to the reducer, or else to the writer.
    fn deliver<W: Write>(
        &mut self,
        content: &str,
        terminator: &str,
        state: &mut RunState,
        writer: &mut W,
    ) -> io::Result<()> {
        if let Some(reducer) = &mut self.reducer {
            return reducer.accept(content, writer);
        }
        write_content(content, terminator, state, writer)
    }

    /// Write a line that passes through unchanged (outside the row
    /// range, or spared by the predicate in delete mode). Reordered
    /// lines buffered so far belong to earlier rows, so they are
    /// flushed first — each contiguous run of selected rows reorders
    /// in place instead of drifting past the kept lines.
    fn write_kept_line<W: Write>(
        &mut self,
        raw_line: &[u8],
        state: &mut RunState,
        writer: &mut W,
    ) -> io::Result<()> {
        self.flush_reordered(state, writer)?;
        //the raw line carries its own terminator, so it is written as it
        //was read — but whatever came before it may still owe one
        write_pending_separator(state, writer)?;
        writer.write_all(raw_line)
    }

    /// Whether the line survives `--unique`: its key columns have not
    /// been written yet. Without `--unique` every line passes.
    /// A key span beyond the line yields an empty key, like `cut`. Under
    /// `--quoted` the key is the fields' values in canonical form, so
    /// `"a"` and `a` are duplicates, as they are one `--group-by` group.
    fn passes_unique(&self, content: &str, seen_keys: &mut HashSet<String>) -> bool {
        match &self.unique_key_span {
            None => true,
            Some(span) => seen_keys.insert(
                span.select_canonical(content)
                    .into_owned(),
            ),
        }
    }

    fn flush_reordered<W: Write>(
        &mut self,
        state: &mut RunState,
        writer: &mut W,
    ) -> io::Result<()> {
        let Some(reorder) = &self.reorder else {
            return Ok(());
        };
        if state.reorder_buffer.is_empty() {
            return Ok(());
        }

        let mut buffer = std::mem::take(&mut state.reorder_buffer);
        match reorder {
            Reorder::Sort(spec) => Self::sort_lines(&mut buffer, spec),
            Reorder::Tac => buffer.lines.reverse(),
            Reorder::Shuffle => {
                use rand::seq::SliceRandom;
                buffer.lines.shuffle(&mut rand::rng());
            }
        }

        //the order is settled: the lines now take the same way out as the
        //ones that never needed reordering, so `--unique` keeps the first
        //per key in reordered order and a reducer sees them in that order
        for &line in &buffer.lines {
            self.output(buffer.content(line), line.terminator, state, writer)?;
        }

        //the next run of rows reuses the room
        buffer.clear();
        state.reorder_buffer = buffer;
        Ok(())
    }

    fn sort_lines(buffer: &mut LineBuffer, spec: &SortSpec) {
        //a numeric key reads the field's value (unquoted under `--quoted`);
        //`None` — not a number — sorts before every number, as `Option`
        //orders it, where a sentinel value would collide with `-inf`
        let order = if spec.numeric {
            let keys = buffer
                .contents()
                .map(|content| Number::parse(&spec.key_span.select_value(content)));
            Self::sorted_order(keys, spec.reverse)
        } else {
            //a key span beyond the line yields an empty key, like `cut`; under
            //`--quoted` the key is the fields' values, so `"b"` sorts after
            //`a` rather than first, on its quote. A key that is one piece of
            //the line — the whole line by default — is borrowed from it, not
            //copied. Its first bytes go in front, as a number: most
            //comparisons are settled by them, without a trip to the line
            let keys = buffer.contents().map(|content| {
                let key = spec.key_span.select_value(content);
                (byte_prefix(&key), key)
            });
            Self::sorted_order(keys, spec.reverse)
        };
        buffer.lines = order
            .iter()
            .map(|&position| buffer.lines[position])
            .collect();
    }

    /// The positions of `keys` in stable sorted order. Each key is computed
    /// once, and the sort moves only keys and positions.
    /// Equal keys are ordered by position — in input order under
    /// `--reverse` too — which makes the sort stable, so the faster
    /// unstable one can do it.
    fn sorted_order<K: Ord>(keys: impl Iterator<Item = K>, reverse: bool) -> Vec<usize> {
        let mut keyed: Vec<(K, usize)> = keys.zip(0..).collect();
        match reverse {
            false => keyed.sort_unstable_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1))),
            true => keyed.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1))),
        }
        keyed
            .into_iter()
            .map(|(_, position)| position)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_prefix_orders_as_the_keys_do() {
        // Given keys that differ early, late, by length and past 8 bytes
        let mut keys = vec![
            "b",
            "ab",
            "a",
            "",
            "a\u{0}",
            "abcdefgh2",
            "abcdefgh1",
            "ż",
            "z",
        ];

        // When they are ordered by prefix first, then in full
        let mut by_prefix = keys.clone();
        by_prefix.sort_by_key(|key| (byte_prefix(key), *key));
        keys.sort();

        // Then the order is the plain one
        assert_eq!(by_prefix, keys);
    }

    #[test]
    fn sorted_order_is_stable_both_ways() {
        // Given keys with duplicates
        let keys = ["b", "a", "b", "a"];

        // When sorted ascending and descending
        let ascending = StreamProcessor::sorted_order(keys.iter(), false);
        let descending = StreamProcessor::sorted_order(keys.iter(), true);

        // Then equal keys keep their input order either way
        assert_eq!(ascending, [1, 3, 0, 2]);
        assert_eq!(descending, [0, 2, 1, 3]);
    }

    #[test]
    fn a_line_buffer_keeps_its_lines_back_to_back() {
        // Given three lines, pushed
        let mut buffer = LineBuffer::default();
        buffer.push("xy", "\n");
        buffer.push("", "\r\n");
        buffer.push("ż", "");

        // When reordered by their records alone
        buffer.lines.reverse();

        // Then the text follows them, terminators included
        assert_eq!(buffer.contents().collect::<Vec<_>>(), ["ż", "", "xy"]);
        assert_eq!(buffer.lines[1].terminator, "\r\n");
        assert_eq!(buffer.text, "xyż");

        // And a cleared buffer is empty again
        buffer.clear();
        assert!(buffer.is_empty());
        assert!(buffer.text.is_empty());
    }
}
