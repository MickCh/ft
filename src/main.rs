use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ft::cli_args::{Config, cli};
use ft::error::AppError;
use ft::file_processor::FileProcessor;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
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
/// failure part-way through never leaves the input truncated.
fn run_in_place(config: &Config, path: &Path) -> Result<(), AppError> {
    let reader = open_input(config)?;
    let temp_path = temporary_sibling(path);

    let temp_file = File::create(&temp_path).map_err(|source| AppError::CreateOutput {
        path: temp_path.clone(),
        source,
    })?;

    let mut writer = BufWriter::new(temp_file);
    let processed = process(config, reader, &mut writer);

    //flush before renaming so the temp file holds the whole output
    let result = processed.and_then(|()| {
        writer
            .flush()
            .map_err(AppError::Processing)
    });
    if let Err(error) = result {
        let _ = std::fs::remove_file(&temp_path);
        return Err(error);
    }

    std::fs::rename(&temp_path, path).map_err(|source| {
        let _ = std::fs::remove_file(&temp_path);
        AppError::ReplaceInput {
            path: path.to_path_buf(),
            source,
        }
    })
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
    FileProcessor::new(config)
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
