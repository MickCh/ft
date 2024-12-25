use clap::{Arg, ArgAction, Command};
use std::ops::RangeInclusive;

pub fn cli() -> Command {
    Command::new("tf [File Transformer]")
        .version("0.0.2")
        .arg(
            Arg::new("rows")
                .short('R')
                .long("rows")
                .required(false)
                .value_parser(parse_range_lines),
        )
        .arg(
            Arg::new("columns")
                .short('C')
                .long("cols")
                .required(false)
                .value_parser(parse_range_lines),
        )
        .arg(
            Arg::new("sort")
                .short('s')
                .long("sort")
                .required(false)
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("delete")
                .short('d')
                .long("delete")
                .required(false)
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("find")
                .short('f')
                .long("find")
                .required(false),
        )
        .arg(
            Arg::new("replace")
                .short('r')
                .long("replace")
                .required(false),
        )
        .arg(Arg::new("filename").required(true))
}

fn parse_range_lines(input: &str) -> Result<RangeInclusive<usize>, String> {
    let parts: Vec<&str> = input.split("-").collect();

    if parts.len() != 2 {
        return Err("Invalid range format, expected: <value1>-<value2>".to_owned());
    }

    let from: usize = parts[0]
        .parse()
        .map_err(|_| format!("First value `{input}` isn't a number"))?;

    let to: usize = parts[1]
        .parse()
        .map_err(|_| format!("Second value `{input}` isn't a number"))?;

    Ok(RangeInclusive::new(from, to))
}