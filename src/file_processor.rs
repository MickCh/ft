//! Streaming engine: applies row selection, the per-line transform
//! pipeline and optional reordering to any `BufRead` source and `Write`
//! sink. The processor is assembled field by field from already-built
//! parts (see [`crate::compose`]), so this module stays independent of
//! the CLI layer.

use crate::columns::ColumnSpan;
use crate::constants::NEW_LINE;
use crate::predicate::LinePredicate;
use crate::ranges::{RangeSet, RangeSpec};
use crate::text;
use crate::transform::LineTransform;

use bstr::io::BufReadExt;
use std::borrow::Cow;
use std::collections::HashSet;
use std::io;
use std::io::{BufRead, Write};
use std::str::from_utf8;

/// A buffered line split into content and its original terminator.
struct Line {
    content: String,
    terminator: &'static str,
}

/// Mutable state threaded through one `run` call.
#[derive(Default)]
struct RunState {
    //lines held back for reordering
    reorder_buffer: Vec<Line>,
    //unique keys already written (`--unique`)
    seen_keys: HashSet<String>,
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
    /// Process the selected rows and keep the rest verbatim (delete
    /// mode with a column range: a transform removes the columns, the
    /// rows themselves survive).
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

/// An `Ord` wrapper around the parsed numeric sort key. Lines that do
/// not parse as a number (`None`, including `NaN`) sort before all
/// numbers — a sentinel value would collide with an actual `-inf` key.
struct NumericKey(Option<f64>);

impl NumericKey {
    fn parse(text: &str) -> NumericKey {
        NumericKey(
            text.trim()
                .parse()
                .ok()
                .filter(|value: &f64| !value.is_nan()),
        )
    }
}

impl PartialEq for NumericKey {
    fn eq(&self, other: &NumericKey) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}

impl Eq for NumericKey {}

impl PartialOrd for NumericKey {
    fn partial_cmp(&self, other: &NumericKey) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for NumericKey {
    fn cmp(&self, other: &NumericKey) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match (&self.0, &other.0) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Less,
            (Some(_), None) => Ordering::Greater,
            (Some(this), Some(that)) => this.total_cmp(that),
        }
    }
}

/// The sort/unique key of a line: the content of its key columns, read
/// in the order written. A key span beyond the line yields an empty
/// key, like `cut`.
fn key_of(content: &str, span: &ColumnSpan) -> String {
    text::select_ranges(content, &span.read_ranges(content), span.joiner()).into_owned()
}

/// The streaming processor, configured by its fields and assembled by
/// the composition layer.
pub struct FileProcessor {
    /// Rows taking part in processing (every row when no range given).
    pub rows: RangeSpec,
    /// What happens to rows inside vs outside the row range.
    pub row_mode: RowMode,
    /// `None` means lines stream straight to the writer.
    pub reorder: Option<Reorder>,
    /// Content filter applied to lines within the row range.
    pub predicate: Option<Box<dyn LinePredicate>>,
    /// Key columns for `--unique`; `None` means duplicates are kept.
    pub unique_key_span: Option<ColumnSpan>,
    /// Per-line transforms, applied in order.
    pub transforms: Vec<Box<dyn LineTransform>>,
}

impl FileProcessor {
    /// Stream `reader` line by line into `writer`, applying the configured
    /// row selection, per-line transforms and optional reordering.
    pub fn run<R: BufRead, W: Write>(&self, mut reader: R, writer: &mut W) -> io::Result<()> {
        let mut state = RunState::default();

        if self.rows.is_absolute() {
            //the total line count is irrelevant, so lines can stream through
            let rows = self.rows.resolve(usize::MAX);
            let mut line_number = 0usize;
            reader.for_byte_line_with_terminator(|raw_line| {
                line_number += 1;
                self.process_line(raw_line, line_number, &rows, &mut state, writer)
                    .map(|_| true)
            })?;
        } else {
            //end-relative bounds (`~N`) only resolve once the total
            //line count is known: the whole input must be buffered
            let mut lines: Vec<Vec<u8>> = Vec::new();
            reader.for_byte_line_with_terminator(|raw_line| {
                lines.push(raw_line.to_vec());
                Ok(true)
            })?;
            let rows = self.rows.resolve(lines.len());
            for (index, raw_line) in lines.iter().enumerate() {
                self.process_line(raw_line, index + 1, &rows, &mut state, writer)?;
            }
        }

        self.flush_reordered(&mut state, writer)?;
        writer.flush()
    }

    fn process_line<W: Write>(
        &self,
        raw_line: &[u8],
        line_number: usize,
        rows: &RangeSet,
        state: &mut RunState,
        writer: &mut W,
    ) -> io::Result<()> {
        if !rows.contains(line_number) {
            if self.row_mode.keeps_unselected() {
                self.write_kept_line(raw_line, state, writer)?;
            }
            return Ok(());
        }

        //without a content filter, deleting whole rows needs no UTF-8 look
        if self.row_mode.deletes_selected() && self.predicate.is_none() {
            return Ok(());
        }

        let utf8_line = from_utf8(raw_line).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("line {line_number} is not valid UTF-8: {e}"),
            )
        })?;

        let (content, terminator) = text::split_line_terminator(utf8_line);

        if let Some(predicate) = &self.predicate
            && !predicate.matches(content)
        {
            //a non-matching line is treated like one outside the row range
            if self.row_mode.keeps_unselected() {
                self.write_kept_line(raw_line, state, writer)?;
            }
            return Ok(());
        }

        if self.row_mode.deletes_selected() {
            return Ok(());
        }

        let content = self.apply_transforms(content);

        if self.reorder.is_some() {
            state.reorder_buffer.push(Line {
                content: content.into_owned(),
                terminator,
            });
        } else {
            if !self.passes_unique(&content, &mut state.seen_keys) {
                return Ok(());
            }
            writer.write_all(content.as_bytes())?;
            writer.write_all(terminator.as_bytes())?;
        }

        Ok(())
    }

    /// Write a line that passes through unchanged (outside the row
    /// range, or spared by the predicate in delete mode). Reordered
    /// lines buffered so far belong to earlier rows, so they are
    /// flushed first — each contiguous run of selected rows reorders
    /// in place instead of drifting past the kept lines.
    fn write_kept_line<W: Write>(
        &self,
        raw_line: &[u8],
        state: &mut RunState,
        writer: &mut W,
    ) -> io::Result<()> {
        self.flush_reordered(state, writer)?;
        writer.write_all(raw_line)
    }

    /// Whether the line survives `--unique`: its key columns have not
    /// been written yet. Without `--unique` every line passes.
    /// A key span beyond the line yields an empty key, like `cut`.
    fn passes_unique(&self, content: &str, seen_keys: &mut HashSet<String>) -> bool {
        match &self.unique_key_span {
            None => true,
            Some(span) => seen_keys.insert(key_of(content, span)),
        }
    }

    /// Run the line through the transform pipeline; with no transforms
    /// configured the content passes through without an allocation.
    fn apply_transforms<'a>(&self, content: &'a str) -> Cow<'a, str> {
        self.transforms
            .iter()
            .fold(Cow::Borrowed(content), |line, transform| {
                Cow::Owned(transform.apply(&line))
            })
    }

    fn flush_reordered<W: Write>(&self, state: &mut RunState, writer: &mut W) -> io::Result<()> {
        let Some(reorder) = &self.reorder else {
            return Ok(());
        };
        if state.reorder_buffer.is_empty() {
            return Ok(());
        }
        let RunState {
            reorder_buffer,
            seen_keys,
            ..
        } = state;

        match reorder {
            Reorder::Sort(spec) => Self::sort_lines(reorder_buffer, spec),
            Reorder::Tac => reorder_buffer.reverse(),
            Reorder::Shuffle => {
                use rand::seq::SliceRandom;
                reorder_buffer.shuffle(&mut rand::rng());
            }
        }

        //`--unique` keeps the first line per key in reordered order
        let lines: Vec<&Line> = reorder_buffer
            .iter()
            .filter(|line| self.passes_unique(&line.content, seen_keys))
            .collect();

        let last_index = lines.len().saturating_sub(1);
        for (index, line) in lines.iter().enumerate() {
            writer.write_all(line.content.as_bytes())?;
            if !line.terminator.is_empty() {
                writer.write_all(line.terminator.as_bytes())?;
            } else if index < last_index {
                //a line missing its terminator must not glue to the next one
                writer.write_all(NEW_LINE.as_bytes())?;
            }
        }
        state.reorder_buffer.clear();
        Ok(())
    }

    fn sort_lines(buffer: &mut [Line], spec: &SortSpec) {
        //`Reverse` keeps the sort stable in descending order too
        use std::cmp::Reverse;
        let text_key = |line: &Line| key_of(&line.content, &spec.key_span);
        match (spec.numeric, spec.reverse) {
            (false, false) => buffer.sort_by_cached_key(|line| text_key(line)),
            (false, true) => buffer.sort_by_cached_key(|line| Reverse(text_key(line))),
            (true, false) => buffer.sort_by_cached_key(|line| NumericKey::parse(&text_key(line))),
            (true, true) => {
                buffer.sort_by_cached_key(|line| Reverse(NumericKey::parse(&text_key(line))))
            }
        }
    }
}
