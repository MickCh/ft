use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ft::cli_args::{Config, cli};
use ft::compose;
use ft::error::AppError;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        //a consumer closing the pipe early (`ft file | head`) ends the
        //output stream normally and is not worth reporting
        Err(AppError::Processing(error)) if error.kind() == ErrorKind::BrokenPipe => {
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("Error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), AppError> {
    let config = Config::try_from(cli().get_matches())?;

    match (&config.in_place, &config.filename) {
        //validated in Config: --in-place always has an input file
        (true, Some(path)) => run_in_place(&config, path),
        _ => run_streaming(&config),
    }
}

/// Stream from the configured input (file or stdin) to the configured
/// output (file or stdout).
fn run_streaming(config: &Config) -> Result<(), AppError> {
    //`File::create` truncates, so an output aliasing the input would
    //destroy the data before the first line is read
    if let (Some(input), Some(output)) = (&config.filename, &config.output_filename)
        && same_file(input, output)
    {
        return Err(AppError::OutputIsInput {
            path: output.clone(),
        });
    }

    let reader = open_input(config)?;

    let mut writer: Box<dyn Write> = match &config.output_filename {
        Some(path) => {
            let file = File::create(path).map_err(|source| AppError::CreateOutput {
                path: path.clone(),
                source,
            })?;
            Box::new(BufWriter::new(file))
        }
        None => Box::new(BufWriter::new(std::io::stdout())),
    };

    process(config, reader, &mut writer)
}

/// Edit `path` in place: write the result to a temporary file in the
/// same directory, then atomically rename it over the original, so a
/// failure part-way through never leaves the input truncated. Symlinks
/// are resolved first, so the link's target is edited instead of the
/// link being replaced by a regular file. The temporary file inherits
/// the original's permissions and is synced to disk before the swap.
fn run_in_place(config: &Config, path: &Path) -> Result<(), AppError> {
    let reader = open_input(config)?;
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
    let processed = process(config, reader, &mut writer);

    //finish writing and match the original permissions before renaming,
    //so the temp file holds the whole output and the right mode
    let result = processed
        .and_then(|()| {
            writer
                .flush()
                .map_err(AppError::Processing)
        })
        .and_then(|()| {
            std::fs::set_permissions(&temp_path, permissions)
                .map_err(|source| replace_error(path, source))
        })
        .and_then(|()| {
            //rename is atomic in the namespace but says nothing about
            //durability: sync so a crash right after the swap cannot
            //leave an empty or truncated file behind
            writer
                .get_ref()
                .sync_all()
                .map_err(|source| replace_error(path, source))
        });
    if let Err(error) = result {
        let _ = std::fs::remove_file(&temp_path);
        return Err(error);
    }

    std::fs::rename(&temp_path, path).map_err(|source| {
        let _ = std::fs::remove_file(&temp_path);
        replace_error(path, source)
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

fn open_input(config: &Config) -> Result<Box<dyn BufRead>, AppError> {
    match &config.filename {
        Some(path) => {
            let file = File::open(path).map_err(|source| AppError::OpenInput {
                path: path.clone(),
                source,
            })?;
            Ok(Box::new(BufReader::new(file)))
        }
        None => Ok(Box::new(std::io::stdin().lock())),
    }
}

fn process<R: BufRead, W: Write>(
    config: &Config,
    reader: R,
    writer: &mut W,
) -> Result<(), AppError> {
    compose::build_processor(config)
        .run(reader, writer)
        .map_err(AppError::Processing)
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
