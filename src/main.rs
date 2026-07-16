use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ft::cli_args::{Config, InPlace, Input, cli};
use ft::compose;
use ft::error::AppError;
use ft::file_processor::RunOutcome;

/// Exit codes follow `grep`: 0 when rows matched, 1 when a filter was
/// given and nothing matched at all, 2 when the run failed outright.
const NO_MATCH: u8 = 1;
const FAILURE: u8 = 2;

fn main() -> ExitCode {
    match run() {
        Ok(Verdict::Matched) => ExitCode::SUCCESS,
        Ok(Verdict::NothingMatched) => ExitCode::from(NO_MATCH),
        //a consumer closing the pipe early (`ft file | head`) ends the
        //output stream normally and is not worth reporting
        Err(AppError::Processing(error)) if error.kind() == ErrorKind::BrokenPipe => {
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("Error: {error}");
            ExitCode::from(FAILURE)
        }
    }
}

/// What the run reports back to the shell. Without a `--grep` filter
/// there is nothing that could fail to match, so a plain transformation
/// always counts as a match.
enum Verdict {
    Matched,
    NothingMatched,
}

impl Verdict {
    fn of(config: &Config, matched: bool) -> Verdict {
        match config.grep.is_none() || matched {
            true => Verdict::Matched,
            false => Verdict::NothingMatched,
        }
    }
}

fn run() -> Result<Verdict, AppError> {
    let config = Config::try_from(cli().get_matches())?;

    if let Some(in_place) = &config.in_place {
        //validated in Config: --in-place only ever has file inputs, and
        //each is edited on its own, with its own row numbering
        let mut matched = false;
        //a file named twice would be edited twice — the second pass
        //reprocessing the first's output and overwriting its backup —
        //so each distinct file is edited once
        let mut seen = HashSet::new();
        for path in config.input_files() {
            let identity = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
            if !seen.insert(identity) {
                continue;
            }
            let outcome = match in_place.dry_run {
                true => report_changes(&config, path)?,
                false => run_in_place(&config, in_place, path)?,
            };
            //one file matching is enough for the batch to count as a match
            matched |= outcome.matched;
        }
        return Ok(Verdict::of(&config, matched));
    }

    let outcome = run_streaming(&config)?;
    Ok(Verdict::of(&config, outcome.matched))
}

/// Process a file without writing anything, reporting whether the edit
/// would change it — what `--in-place --dry-run` is for.
fn report_changes(config: &Config, path: &Path) -> Result<RunOutcome, AppError> {
    //the file is read twice: once as the input, once as what the result
    //is compared against
    let mut comparison = CompareWriter::new(open_file(path)?);
    let outcome = process(config, open_file(path)?, &mut comparison)?;

    let verdict = if comparison
        .differs()
        .map_err(AppError::Processing)?
    {
        "would change"
    } else {
        "unchanged"
    };
    writeln!(std::io::stdout(), "{}: {verdict}", path.display()).map_err(AppError::Processing)?;

    Ok(outcome)
}

/// Stream the inputs (files, stdin, or both — read one after another as
/// a single stream) to the configured output (file or stdout).
fn run_streaming(config: &Config) -> Result<RunOutcome, AppError> {
    //`File::create` truncates, so an output aliasing any input would
    //destroy that data before the first line is read
    if let Some(output) = &config.output_filename
        && let Some(input) = config
            .input_files()
            .find(|input| same_file(input, output))
    {
        return Err(AppError::OutputIsInput {
            path: input.to_path_buf(),
        });
    }

    let reader = open_inputs(config)?;

    let mut writer: Box<dyn Write> = match (&config.output_filename, config.quiet) {
        //--quiet asks only whether anything matched: the answer is the
        //exit code, and the rows themselves go nowhere
        (_, true) => Box::new(std::io::sink()),
        (Some(path), _) => {
            let file = File::create(path).map_err(|source| AppError::CreateOutput {
                path: path.clone(),
                source,
            })?;
            Box::new(BufWriter::new(file))
        }
        (None, _) => Box::new(BufWriter::new(std::io::stdout())),
    };

    process(config, reader, &mut writer)
}

/// Edit `path` in place: write the result to a temporary file in the
/// same directory, then atomically rename it over the original, so a
/// failure part-way through never leaves the input truncated. Symlinks
/// are resolved first, so the link's target is edited instead of the
/// link being replaced by a regular file. The temporary file inherits
/// the original's permissions and is synced to disk before the swap.
fn run_in_place(config: &Config, in_place: &InPlace, path: &Path) -> Result<RunOutcome, AppError> {
    let reader = open_file(path)?;
    //resolve symlinks so the rename below swaps out the link's target,
    //not the link itself (which would turn it into a regular file)
    let path = &std::fs::canonicalize(path).map_err(|source| replace_error(path, source))?;
    //capture the original permissions up front so the replacement keeps
    //them instead of the temporary file's umask-derived defaults
    let permissions = std::fs::metadata(path)
        .map_err(|source| replace_error(path, source))?
        .permissions();
    let temp_path = temporary_sibling(path);

    //create_new refuses to open a pre-existing file or symlink, so a
    //temp path planted at this predictable name cannot redirect the
    //write to another file
    let temp_file = File::create_new(&temp_path).map_err(|source| AppError::CreateOutput {
        path: temp_path.clone(),
        source,
    })?;

    let mut writer = BufWriter::new(temp_file);

    //finish writing and match the original permissions before renaming,
    //so the temp file holds the whole output and the right mode
    let result = process(config, reader, &mut writer).and_then(|outcome| {
        writer
            .flush()
            .map_err(AppError::Processing)?;
        std::fs::set_permissions(&temp_path, permissions)
            .map_err(|source| replace_error(path, source))?;
        //rename is atomic in the namespace but says nothing about
        //durability: sync so a crash right after the swap cannot
        //leave an empty or truncated file behind
        writer
            .get_ref()
            .sync_all()
            .map_err(|source| replace_error(path, source))?;
        //the backup is taken from the original, while it is still there,
        //and before the swap: a failure anywhere above leaves the input
        //untouched
        if let Some(suffix) = &in_place.backup {
            back_up(path, suffix)?;
        }
        Ok(outcome)
    });

    let outcome = match result {
        Ok(outcome) => outcome,
        Err(error) => {
            let _ = std::fs::remove_file(&temp_path);
            return Err(error);
        }
    };

    std::fs::rename(&temp_path, path).map_err(|source| {
        let _ = std::fs::remove_file(&temp_path);
        replace_error(path, source)
    })?;

    Ok(outcome)
}

/// Copy the file about to be edited to a sibling named after it plus
/// `suffix`, so `notes.txt` with `.bak` is kept as `notes.txt.bak`.
fn back_up(path: &Path, suffix: &str) -> Result<(), AppError> {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    let backup = PathBuf::from(name);

    std::fs::copy(path, &backup)
        .map(|_| ())
        .map_err(|source| AppError::Backup {
            path: backup,
            source,
        })
}

fn replace_error(path: &Path, source: std::io::Error) -> AppError {
    AppError::ReplaceInput {
        path: path.to_path_buf(),
        source,
    }
}

/// Whether two paths point at the same existing file, resolving
/// symlinks and relative components. A path that cannot be resolved
/// (e.g. an output file that does not exist yet) aliases nothing.
fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Open every input and read them as one stream, in the order given —
/// like `cat`, so a row range addresses the concatenation rather than
/// each file separately.
fn open_inputs(config: &Config) -> Result<Box<dyn BufRead>, AppError> {
    let mut readers: Vec<Box<dyn Read>> = Vec::with_capacity(config.inputs.len());
    for input in &config.inputs {
        readers.push(match input {
            Input::Stdin => Box::new(std::io::stdin()),
            Input::File(path) => Box::new(open_file(path)?),
        });
    }

    let chained = readers
        .into_iter()
        .reduce(|left, right| Box::new(left.chain(right)))
        //`Config` guarantees at least one input, but a missing one would
        //simply be an empty stream rather than a reason to panic
        .unwrap_or_else(|| Box::new(std::io::empty()));

    Ok(Box::new(BufReader::new(chained)))
}

fn open_file(path: &Path) -> Result<BufReader<File>, AppError> {
    let file = File::open(path).map_err(|source| AppError::OpenInput {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(BufReader::new(file))
}

fn process<R: BufRead, W: Write>(
    config: &Config,
    reader: R,
    writer: &mut W,
) -> Result<RunOutcome, AppError> {
    compose::build_processor(config)
        .run(reader, writer)
        .map_err(AppError::Processing)
}

/// A sink that compares what it is asked to write against the bytes of
/// the file it stands in for, rather than writing them. That is what
/// `--dry-run` needs: whether the edit would change the file, without
/// touching it and without holding the result in memory.
struct CompareWriter<R: Read> {
    original: R,
    //as much of the original as the current write covers
    window: Vec<u8>,
    differs: bool,
}

impl<R: Read> CompareWriter<R> {
    fn new(original: R) -> CompareWriter<R> {
        CompareWriter {
            original,
            window: Vec::new(),
            differs: false,
        }
    }

    /// Whether the result differs from the original — including when the
    /// original still holds bytes the result never produced (a shorter
    /// output matches byte for byte up to where it ends).
    fn differs(&mut self) -> std::io::Result<bool> {
        if self.differs {
            return Ok(true);
        }
        let mut leftover = [0u8; 1];
        Ok(self.original.read(&mut leftover)? > 0)
    }
}

impl<R: Read> Write for CompareWriter<R> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        //once a difference is found there is nothing left to learn, so
        //the rest of the output is only counted, not compared
        if !self.differs {
            self.window.resize(data.len(), 0);
            let read = fill(&mut self.original, &mut self.window)?;
            self.differs = read != data.len() || self.window != data;
        }
        //the bytes are consumed either way: they are compared, not written
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Read until `buffer` is full, stopping early only at end of input.
fn fill<R: Read>(reader: &mut R, buffer: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        match reader.read(&mut buffer[filled..])? {
            0 => break,
            read => filled += read,
        }
    }
    Ok(filled)
}

/// A temporary path next to `path` (same directory, so a rename onto
/// `path` stays on one filesystem and is atomic).
fn temporary_sibling(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let temp_name = format!(".{name}.ft-{}.tmp", std::process::id());
    match path.parent() {
        Some(dir) => dir.join(temp_name),
        None => PathBuf::from(temp_name),
    }
}
