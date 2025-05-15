mod cli_args;
mod constants;
mod file_processor;

use cli_args::ConfigBuilder;
use file_processor::FileProcessor;

fn main() -> std::result::Result<(), String> {
    let config = ConfigBuilder::new()
        .rows()
        .cols()
        .sort()
        .delete()
        .filename()
        .replace()
        .output()
        .build();

    let config = match config {
        Ok(config) => config,
        Err(e) => {
            return Err(format!("User input: {}", e));
        }
    };

    let file_processor = FileProcessor::new(config);
    match file_processor.process() {
        Ok(ok) => Ok(ok),
        Err(e) => Err(format!("Processing error: {}", e)),
    }
}
