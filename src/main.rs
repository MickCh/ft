use file_processor::FileProcessor;

use crate::config::config::Config;

mod config;
mod file_processor;

fn main() -> std::result::Result<(), String> {
    let config = Config::new()
        .rows()
        .cols()
        .sort()
        .delete()
        .filename()
        .replace()
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
