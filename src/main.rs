use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
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

    let input = File::open(&config.filename).map_err(|source| AppError::OpenInput {
        path: config.filename.clone(),
        source,
    })?;
    let reader = BufReader::new(input);

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

    FileProcessor::new(&config)
        .run(reader, &mut writer)
        .map_err(AppError::Processing)
}
