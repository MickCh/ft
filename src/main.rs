use std::fs::File;
use std::io::{BufReader, BufWriter, Write};

use ft::cli_args::ConfigBuilder;
use ft::file_processor::FileProcessor;

fn main() -> Result<(), String> {
    let config = ConfigBuilder::new()
        .rows()
        .cols()
        .sort()
        .delete()
        .filename()
        .replace()
        .output()
        .build()
        .map_err(|e| format!("User input: {e}"))?;

    let input = File::open(&config.filename)
        .map_err(|e| format!("Cannot open input file `{}`: {e}", config.filename))?;
    let reader = BufReader::new(input);

    let mut writer: Box<dyn Write> = match &config.output_filename {
        Some(filename) => {
            let file = File::create(filename)
                .map_err(|e| format!("Cannot create output file `{filename}`: {e}"))?;
            Box::new(BufWriter::new(file))
        }
        None => Box::new(BufWriter::new(std::io::stdout())),
    };

    FileProcessor::new(&config)
        .run(reader, &mut writer)
        .map_err(|e| format!("Processing error: {e}"))
}
