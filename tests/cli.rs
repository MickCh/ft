use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};

const INPUT: &str = "delta foo\nalpha foo\ncharlie foo\nbravo foo\n";

/// Temporary input file removed when the test ends.
struct TempFile {
    path: PathBuf,
}

impl TempFile {
    fn new(name: &str, content: &str) -> TempFile {
        let path = std::env::temp_dir().join(format!("ft-test-{}-{}", std::process::id(), name));
        fs::write(&path, content).expect("failed to write test input file");
        TempFile { path }
    }

    fn path_str(&self) -> &str {
        self.path.to_str().unwrap()
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn run_ft(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ft"))
        .args(args)
        .output()
        .expect("failed to run ft binary")
}

fn run_ft_stdout(args: &[&str]) -> String {
    let output = run_ft(args);
    assert!(
        output.status.success(),
        "ft failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is not valid UTF-8")
}

#[test]
fn no_flags_passes_file_through() {
    let input = TempFile::new("passthrough", INPUT);
    assert_eq!(run_ft_stdout(&[input.path_str()]), INPUT);
}

#[test]
fn replace_works_without_delete() {
    let input = TempFile::new("replace", INPUT);
    let stdout = run_ft_stdout(&["-f", "foo", "-r", "BAR", input.path_str()]);
    assert_eq!(stdout, "delta BAR\nalpha BAR\ncharlie BAR\nbravo BAR\n");
}

#[test]
fn replace_applies_only_to_selected_rows() {
    let input = TempFile::new("replace-rows", INPUT);
    let stdout = run_ft_stdout(&["-R", "2-3", "-f", "foo", "-r", "BAR", input.path_str()]);
    assert_eq!(stdout, "alpha BAR\ncharlie BAR\n");
}

#[test]
fn replace_applies_only_inside_column_range() {
    let input = TempFile::new("replace-cols", INPUT);
    let stdout = run_ft_stdout(&["-C", "7-9", "-f", "foo", "-r", "BAR", input.path_str()]);
    //"foo" occupies columns 7-9 in every line except "charlie foo" (columns 9-11)
    assert_eq!(stdout, "delta BAR\nalpha BAR\ncharlie foo\nbravo BAR\n");
}

#[test]
fn sort_works_without_delete() {
    let input = TempFile::new("sort", INPUT);
    let stdout = run_ft_stdout(&["-s", input.path_str()]);
    assert_eq!(stdout, "alpha foo\nbravo foo\ncharlie foo\ndelta foo\n");
}

#[test]
fn sort_uses_column_range_as_key() {
    let input = TempFile::new("sort-cols", "b 2\nc 1\na 3\n");
    let stdout = run_ft_stdout(&["-s", "-C", "3-3", input.path_str()]);
    assert_eq!(stdout, "c 1\nb 2\na 3\n");
}

#[test]
fn sort_applies_only_to_selected_rows() {
    let input = TempFile::new("sort-rows", INPUT);
    let stdout = run_ft_stdout(&["-s", "-R", "2-4", input.path_str()]);
    assert_eq!(stdout, "alpha foo\nbravo foo\ncharlie foo\n");
}

#[test]
fn rows_range_selects_lines() {
    let input = TempFile::new("select-rows", INPUT);
    let stdout = run_ft_stdout(&["-R", "1-2", input.path_str()]);
    assert_eq!(stdout, "delta foo\nalpha foo\n");
}

#[test]
fn delete_removes_rows_in_range() {
    let input = TempFile::new("delete-rows", INPUT);
    let stdout = run_ft_stdout(&["-d", "-R", "2-3", input.path_str()]);
    assert_eq!(stdout, "delta foo\nbravo foo\n");
}

#[test]
fn delete_removes_columns_in_every_line() {
    let input = TempFile::new("delete-cols", INPUT);
    let stdout = run_ft_stdout(&["-d", "-C", "7-9", input.path_str()]);
    //in "charlie foo" columns 7-9 are "e f", leaving "charli" + "oo"
    assert_eq!(stdout, "delta \nalpha \ncharlioo\nbravo \n");
}

#[test]
fn delete_removes_columns_only_in_selected_rows() {
    let input = TempFile::new("delete-rows-cols", INPUT);
    let stdout = run_ft_stdout(&["-d", "-R", "1-1", "-C", "1-6", input.path_str()]);
    assert_eq!(stdout, "foo\nalpha foo\ncharlie foo\nbravo foo\n");
}

#[test]
fn output_flag_writes_to_file_instead_of_stdout() {
    let input = TempFile::new("output-in", INPUT);
    let out_path = std::env::temp_dir().join(format!("ft-test-{}-output-out", std::process::id()));

    let stdout = run_ft_stdout(&[
        "-f",
        "foo",
        "-r",
        "BAR",
        "-o",
        out_path.to_str().unwrap(),
        input.path_str(),
    ]);

    assert_eq!(stdout, "");
    let written = fs::read_to_string(&out_path).expect("output file not created");
    let _ = fs::remove_file(&out_path);
    assert_eq!(written, "delta BAR\nalpha BAR\ncharlie BAR\nbravo BAR\n");
}

#[test]
fn replace_without_find_is_rejected() {
    let input = TempFile::new("replace-no-find", INPUT);
    let output = run_ft(&["-r", "BAR", input.path_str()]);
    assert!(!output.status.success());
}

#[test]
fn replace_with_delete_is_rejected() {
    let input = TempFile::new("replace-with-delete", INPUT);
    let output = run_ft(&["-d", "-f", "foo", "-r", "BAR", input.path_str()]);
    assert!(!output.status.success());
}

#[test]
fn inverted_range_is_rejected() {
    let input = TempFile::new("inverted-range", INPUT);
    let output = run_ft(&["-R", "5-2", input.path_str()]);
    assert!(!output.status.success());
}

#[test]
fn zero_based_range_is_rejected() {
    let input = TempFile::new("zero-range", INPUT);
    let output = run_ft(&["-C", "0-5", input.path_str()]);
    assert!(!output.status.success());
}

#[test]
fn missing_input_file_fails() {
    let output = run_ft(&["/nonexistent/ft-test-missing.txt"]);
    assert!(!output.status.success());
}
