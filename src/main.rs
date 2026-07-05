use std::fs::File;
use std::io::{BufReader, BufWriter, Write};

use ft::cli_args::{Config, cli};
use ft::file_processor::FileProcessor;

fn main() -> Result<(), String> {
    let config = Config::try_from(cli().get_matches()).map_err(|e| format!("User input: {e}"))?;

    let input = File::open(&config.filename).map_err(|e| {
        format!(
            "Cannot open input file `{}`: {e}",
            config.filename.display()
        )
    })?;
    let reader = BufReader::new(input);

    let mut writer: Box<dyn Write> = match &config.output_filename {
        Some(path) => {
            let file = File::create(path)
                .map_err(|e| format!("Cannot create output file `{}`: {e}", path.display()))?;
            Box::new(BufWriter::new(file))
        }
        None => Box::new(BufWriter::new(std::io::stdout())),
    };

    FileProcessor::new(&config)
        .run(reader, &mut writer)
        .map_err(|e| format!("Processing error: {e}"))
}
