use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap_complete::Shell;

use encoding_rs::{Encoding, UTF_8};

use fxt::cli_args::{Config, InPlace, Input, cli};
use fxt::compose;
use fxt::encoding::{self, Decoded, EncodeWriter, SourceEncoding};
use fxt::error::AppError;
use fxt::file_processor::RunOutcome;

/// Exit codes follow `grep`: 0 when rows matched, 1 when a filter was
/// given and nothing matched at all, 2 when the run failed outright.
const NO_MATCH: u8 = 1;
const FAILURE: u8 = 2;

fn main() -> ExitCode {
    match run() {
        Ok(Verdict::Matched) => ExitCode::SUCCESS,
        Ok(Verdict::NothingMatched) => ExitCode::from(NO_MATCH),
        //a consumer closing the pipe early (`fxt file | head`) ends the
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

/// What the run reports back to the shell. Without a filter by content
/// (`--grep`, `--from`, `--to`) there is nothing that could fail to
/// match, so a plain transformation always counts as a match.
enum Verdict {
    Matched,
    NothingMatched,
}

impl Verdict {
    fn of(config: &Config, matched: bool) -> Verdict {
        match !config.filters_by_content() || matched {
            true => Verdict::Matched,
            false => Verdict::NothingMatched,
        }
    }
}

fn run() -> Result<Verdict, AppError> {
    let matches = cli().get_matches();
    //--completions and --man describe the tool instead of running it;
    //clap makes them exclusive, so there is no transformation to validate
    if let Some(&shell) = matches.get_one::<Shell>("completions") {
        let mut command = cli();
        let name = command.get_name().to_owned();
        clap_complete::generate(shell, &mut command, name, &mut std::io::stdout());
        return Ok(Verdict::Matched);
    }
    if matches.get_flag("man") {
        clap_mangen::Man::new(cli())
            .render(&mut std::io::stdout())
            .map_err(AppError::Processing)?;
        return Ok(Verdict::Matched);
    }

    let config = Config::try_from(matches)?;

    if let Some(in_place) = &config.in_place {
        //validated in Config: --in-place only ever has file inputs, and
        //each is edited on its own, with its own row numbering
        let mut matched = false;
        //a file named twice would be edited twice — the second pass
        //reprocessing the first's output and overwriting its backup —
        //so each distinct file is edited once
        let mut seen = HashSet::new();
        for path in config.input_files() {
            let identity = resolve(path).unwrap_or_else(|_| path.to_path_buf());
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
    let (source, reader) = open_decoded(config, path)?;
    //the edit would put the mark back and encode the result as the file
    //is, so the comparison expects both too
    let target = in_place_encoding(config, source);
    write_bom(source, target, &mut comparison)?;
    let mut writer = encoded(target, comparison);
    let outcome = process(config, BufReader::new(reader), &mut writer)?;

    let original_size = std::fs::metadata(path)
        .map_err(|source| open_error(path, source))?
        .len();
    let verdict = if let Some(reason) = leaves_nothing(config, outcome, original_size) {
        format!("unchanged ({reason})")
    } else if writer
        .get_mut()
        .differs()
        .map_err(AppError::Processing)?
    {
        "would change".to_owned()
    } else {
        "unchanged".to_owned()
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

    let destination: Box<dyn Write> = match (&config.output_filename, config.quiet) {
        //--quiet asks only whether anything matched: the answer is the
        //exit code, and the rows themselves go nowhere
        (_, true) => Box::new(std::io::sink()),
        (Some(path), _) => {
            let file = File::create(path).map_err(|source| AppError::CreateOutput {
                path: path.clone(),
                source,
            })?;
            Box::new(file)
        }
        (None, _) => Box::new(std::io::stdout()),
    };
    //buffered in front of the encoder, which then gets large writes to
    //convert rather than every short piece the engine writes
    let target = config.output_encoding.unwrap_or(UTF_8);
    let mut writer = BufWriter::new(encoded(target, destination));

    process(config, reader, &mut writer)
}

/// Edit `path` in place: write the result to a temporary file in the
/// same directory, then atomically rename it over the original, so a
/// failure part-way through never leaves the input truncated. Symlinks
/// are resolved first, so the link's target is edited instead of the
/// link being replaced by a regular file. The temporary file inherits
/// the original's permissions and is synced to disk before the swap.
fn run_in_place(config: &Config, in_place: &InPlace, path: &Path) -> Result<RunOutcome, AppError> {
    //the BOM is taken off for processing and put back below, and the
    //result encoded as the input was: the edited file stays encoded as
    //it was, unless --output-encoding converts it
    let (source, reader) = open_decoded(config, path)?;
    let target = in_place_encoding(config, source);
    let reader = BufReader::new(reader);
    //resolve symlinks so the rename below swaps out the link's target,
    //not the link itself (which would turn it into a regular file)
    let given = path;
    let path = &resolve(path).map_err(|source| replace_error(path, source))?;
    //capture the original permissions up front so the replacement keeps
    //them instead of the temporary file's umask-derived defaults
    let metadata = std::fs::metadata(path).map_err(|source| replace_error(path, source))?;
    let permissions = metadata.permissions();
    let temp_path = temporary_sibling(path);

    //create_new refuses to open a pre-existing file or symlink, so a
    //temp path planted at this predictable name cannot redirect the
    //write to another file
    let temp_file = File::create_new(&temp_path).map_err(|source| AppError::CreateOutput {
        path: temp_path.clone(),
        source,
    })?;

    let mut writer = BufWriter::new(encoded(target, temp_file));

    //finish writing and match the original permissions before renaming,
    //so the temp file holds the whole output and the right mode
    let result = write_bom(source, target, writer.get_mut().get_mut())
        .and_then(|()| process(config, reader, &mut writer))
        .and_then(|outcome| {
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

    //a selection that took no row would write the file back empty: that
    //is never what an edit means, so the file is left as it was
    if let Some(reason) = leaves_nothing(config, outcome, metadata.len()) {
        let _ = std::fs::remove_file(&temp_path);
        eprintln!("{}: {reason}, left unchanged", given.display());
        return Ok(outcome);
    }

    std::fs::rename(&temp_path, path).map_err(|source| {
        let _ = std::fs::remove_file(&temp_path);
        replace_error(path, source)
    })?;

    Ok(outcome)
}

/// Why an in-place edit would leave nothing of a file that had something
/// in it, if it would: no row was selected — a filter matched none, or
/// `--rows`/`--from`/`--to` lie past them — and the selection drops the
/// rows it leaves out. Such a file is not written back (and `--dry-run`
/// reports it unchanged). An empty file has nothing to lose.
fn leaves_nothing(
    config: &Config,
    outcome: RunOutcome,
    original_size: u64,
) -> Option<&'static str> {
    if outcome.matched || !config.drops_unselected() || original_size == 0 {
        return None;
    }
    Some(match config.filters_by_content() {
        true => "nothing matched",
        false => "no row selected",
    })
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

/// The absolute path of an existing file, symlinks resolved. On Windows
/// `canonicalize` answers with a verbatim path (`\\?\C:\…`), which would
/// leak into messages and into the names of the temporary and backup
/// files made next to it, so it is given back in its ordinary form.
fn resolve(path: &Path) -> std::io::Result<PathBuf> {
    std::fs::canonicalize(path).map(simplified)
}

/// A verbatim Windows path in its ordinary form: `\\?\C:\a` as `C:\a`,
/// `\\?\UNC\server\share` as `\\server\share`. Anything else — a volume
/// GUID path, or one too long for the ordinary form (`MAX_PATH`) — stays
/// verbatim, since only the verbatim form can name it.
fn simplified(path: PathBuf) -> PathBuf {
    const MAX_PATH: usize = 260;
    let Some(text) = path.to_str() else {
        return path;
    };
    let ordinary = if let Some(share) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{share}")
    } else if let Some(local) = text.strip_prefix(r"\\?\")
        && local.as_bytes().get(1) == Some(&b':')
    {
        local.to_owned()
    } else {
        return path;
    };
    match ordinary.len() < MAX_PATH {
        true => PathBuf::from(ordinary),
        false => path,
    }
}

/// Whether two paths point at the same existing file, resolving
/// symlinks and relative components. A path that cannot be resolved
/// (e.g. an output file that does not exist yet) aliases nothing.
fn same_file(a: &Path, b: &Path) -> bool {
    match (resolve(a), resolve(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Open every input and read them as one stream, in the order given —
/// like `cat`, so a row range addresses the concatenation rather than
/// each file separately. Each input loses its own BOM before the inputs
/// are joined, or a second file's mark would land mid-stream; none is
/// written back, since the output is a new text, not one of the files.
fn open_inputs(config: &Config) -> Result<Box<dyn BufRead>, AppError> {
    let mut readers: Vec<Box<dyn Read>> = Vec::with_capacity(config.inputs.len());
    for input in &config.inputs {
        readers.push(match input {
            Input::Stdin => {
                let (_, reader) = encoding::open(
                    std::io::stdin(),
                    config.input_encoding(),
                    config.on_malformed(),
                )
                .map_err(AppError::Processing)?;
                Box::new(reader)
            }
            Input::File(path) => Box::new(open_decoded(config, path)?.1),
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

/// Open `path` as UTF-8 text, with its BOM, if any, taken off — and say
/// how it was encoded. Unbuffered: the caller buffers whatever reader it
/// ends up with.
fn open_decoded(config: &Config, path: &Path) -> Result<(SourceEncoding, Decoded<File>), AppError> {
    let file = File::open(path).map_err(|source| open_error(path, source))?;
    encoding::open(file, config.input_encoding(), config.on_malformed())
        .map_err(|source| open_error(path, source))
}

/// What an edited file is written back in: `--output-encoding`, or else
/// the encoding it was read in, so editing a file never converts it.
fn in_place_encoding(config: &Config, source: SourceEncoding) -> &'static Encoding {
    config
        .output_encoding
        .unwrap_or(source.encoding)
}

/// Put back the byte order mark the file started with, in the form of
/// the encoding it is written in (none, for one that has no mark).
fn write_bom<W: Write>(
    source: SourceEncoding,
    target: &'static Encoding,
    writer: &mut W,
) -> Result<(), AppError> {
    if !source.bom {
        return Ok(());
    }
    writer
        .write_all(encoding::bom_of(target))
        .map_err(AppError::Processing)
}

/// A writer in `encoding`: UTF-8, which is what the engine writes, goes
/// straight through; anything else is encoded on the way.
enum Encoded<W: Write> {
    Utf8(W),
    Other(EncodeWriter<W>),
}

fn encoded<W: Write>(target: &'static Encoding, writer: W) -> Encoded<W> {
    match target == UTF_8 {
        true => Encoded::Utf8(writer),
        false => Encoded::Other(EncodeWriter::new(writer, target)),
    }
}

impl<W: Write> Encoded<W> {
    fn get_ref(&self) -> &W {
        match self {
            Encoded::Utf8(writer) => writer,
            Encoded::Other(writer) => writer.get_ref(),
        }
    }

    fn get_mut(&mut self) -> &mut W {
        match self {
            Encoded::Utf8(writer) => writer,
            Encoded::Other(writer) => writer.get_mut(),
        }
    }
}

impl<W: Write> Write for Encoded<W> {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        match self {
            Encoded::Utf8(writer) => writer.write(data),
            Encoded::Other(writer) => writer.write(data),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Encoded::Utf8(writer) => writer.flush(),
            Encoded::Other(writer) => writer.flush(),
        }
    }
}

fn open_error(path: &Path, source: std::io::Error) -> AppError {
    AppError::OpenInput {
        path: path.to_path_buf(),
        source,
    }
}

/// Open `path` as it is, BOM and all — what a dry run compares against.
fn open_file(path: &Path) -> Result<BufReader<File>, AppError> {
    let file = File::open(path).map_err(|source| open_error(path, source))?;
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
    let temp_name = format!(".{name}.fxt-{}.tmp", std::process::id());
    match path.parent() {
        Some(dir) => dir.join(temp_name),
        None => PathBuf::from(temp_name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_verbatim_path_is_given_back_in_its_ordinary_form() {
        // Given verbatim paths / When simplified / Then the prefix is gone
        let simple = |text: &str| simplified(PathBuf::from(text));
        assert_eq!(
            simple(r"\\?\C:\data\a.txt"),
            PathBuf::from(r"C:\data\a.txt")
        );
        assert_eq!(
            simple(r"\\?\UNC\server\share\a"),
            PathBuf::from(r"\\server\share\a")
        );

        // And what only the verbatim form can name is left as it was
        assert_eq!(
            simple(r"\\?\Volume{x}\a"),
            PathBuf::from(r"\\?\Volume{x}\a")
        );
        let long = format!(r"\\?\C:\{}", "a".repeat(300));
        assert_eq!(simple(&long), PathBuf::from(&long));

        // And an ordinary path is no business of it
        assert_eq!(simple("/home/a.txt"), PathBuf::from("/home/a.txt"));
    }
}
