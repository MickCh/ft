//! Throughput of the engine on generated input, in memory: no process
//! start, no disk, so the numbers measure fxt itself. The cases follow
//! the 2026-11-06 comparison with grep/sed/sort/awk, plus the CSV paths.
//!
//! Run with `cargo bench`; one case with `cargo bench -- sort`.

use std::hint::black_box;
use std::io::{Cursor, sink};

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use fxt::cli_args::{Config, cli};
use fxt::compose::build_processor;

const ROWS: usize = 100_000;

/// Log-like rows: an id, a level (every tenth one ERROR) and a payload
/// with something to replace — the shape of the comparison's input.
fn log_input() -> String {
    (0..ROWS)
        .map(|row| {
            let level = if row % 10 == 0 { "ERROR" } else { "INFO" };
            format!(
                "{row:08} {level} request foo handled by worker-{} in {} ms\n",
                row % 17,
                row % 997
            )
        })
        .collect()
}

/// CSV rows: a group key, a quoted text field with a comma inside, and
/// a number.
fn csv_input() -> String {
    (0..ROWS)
        .map(|row| {
            format!(
                "group{},\"name {row}, the {}th\",{}\n",
                row % 50,
                row % 7,
                row % 1000
            )
        })
        .collect()
}

fn config(args: &[&str]) -> Config {
    let argv = std::iter::once("fxt").chain(args.iter().copied());
    Config::try_from(cli().get_matches_from(argv)).expect("a valid benchmark configuration")
}

fn bench_cases(
    criterion: &mut Criterion,
    group_name: &str,
    input: &str,
    cases: &[(&str, &[&str])],
) {
    let mut group = criterion.benchmark_group(group_name);
    group.throughput(Throughput::Bytes(input.len() as u64));
    for (name, args) in cases {
        let config = config(args);
        group.bench_with_input(
            BenchmarkId::from_parameter(name),
            input,
            |bencher, input| {
                bencher.iter(|| {
                    //a processor is consumed by its run, so each iteration
                    //builds one, as main does per input
                    build_processor(&config)
                        .run(Cursor::new(black_box(input.as_bytes())), &mut sink())
                        .expect("the benchmark run succeeds")
                })
            },
        );
    }
    group.finish();
}

fn log_benches(criterion: &mut Criterion) {
    let input = log_input();
    bench_cases(
        criterion,
        "log",
        &input,
        &[
            ("pass-through", &[]),
            ("grep", &["-g", "ERROR"]),
            ("replace", &["-f", "foo", "-r", "bar"]),
            //only every tenth row matches, as in most real files
            ("replace-rare", &["-f", "ERROR", "-r", "E"]),
            ("regex-replace", &["-e", "-f", "worker-[0-9]+", "-r", "w"]),
            ("sort", &["-s"]),
            (
                "sort-numeric-key",
                &["-s", "-n", "-F", " ", "--sort-key", "9"],
            ),
            ("unique", &["-u", "-F", " ", "--unique-key", "7"]),
            (
                "grep-replace-upper",
                &["-g", "ERROR", "-f", "foo", "-r", "bar", "--upper"],
            ),
            ("cols", &["-C", "10-20"]),
        ],
    );
}

fn csv_benches(criterion: &mut Criterion) {
    let input = csv_input();
    bench_cases(
        criterion,
        "csv",
        &input,
        &[
            ("fields", &["-F", ",", "-C", "3,1"]),
            ("quoted-fields", &["-F", ",", "--quoted", "-C", "3,1"]),
            (
                "group-by-sum",
                &["-F", ",", "--group-by", "1", "--count", "--sum", "3"],
            ),
            (
                "quoted-group-by-sum",
                &[
                    "-F",
                    ",",
                    "--quoted",
                    "--group-by",
                    "1",
                    "--count",
                    "--sum",
                    "3",
                ],
            ),
        ],
    );
}

criterion_group!(benches, log_benches, csv_benches);
criterion_main!(benches);
