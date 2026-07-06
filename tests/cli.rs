use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

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

fn run_ft_with_stdin(args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ft"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ft binary");

    child
        .stdin
        .take()
        .expect("stdin not captured")
        .write_all(input.as_bytes())
        .expect("failed to write to ft stdin");

    child
        .wait_with_output()
        .expect("failed to wait for ft binary")
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
fn multiple_find_replace_pairs_apply_in_order() {
    let input = TempFile::new("replace-pairs", "cat dog\n");
    //cat->dog runs first, then dog->wolf rewrites both occurrences
    let stdout = run_ft_stdout(&[
        "-f",
        "cat",
        "-r",
        "dog",
        "-f",
        "dog",
        "-r",
        "wolf",
        input.path_str(),
    ]);
    assert_eq!(stdout, "wolf wolf\n");
}

#[test]
fn unbalanced_find_replace_pairs_are_rejected() {
    let output = run_ft(&["-f", "a", "-r", "1", "-f", "b", "input.txt"]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--replace"),
        "stderr should explain the find/replace mismatch"
    );
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
fn numeric_sort_works() {
    let input = TempFile::new("sort-numeric", "10\n9\n2\n");
    let stdout = run_ft_stdout(&["-s", "-n", input.path_str()]);
    assert_eq!(stdout, "2\n9\n10\n");
}

#[test]
fn reverse_sort_works() {
    let input = TempFile::new("sort-reverse", INPUT);
    let stdout = run_ft_stdout(&["-s", "--reverse", input.path_str()]);
    assert_eq!(stdout, "delta foo\ncharlie foo\nbravo foo\nalpha foo\n");
}

#[test]
fn numeric_without_sort_is_rejected() {
    let input = TempFile::new("numeric-no-sort", INPUT);
    let output = run_ft(&["-n", input.path_str()]);
    assert!(!output.status.success());
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
fn rows_accept_a_list_of_ranges() {
    let input = TempFile::new("rows-list", INPUT);
    let stdout = run_ft_stdout(&["-R", "1,3-4", input.path_str()]);
    assert_eq!(stdout, "delta foo\ncharlie foo\nbravo foo\n");
}

#[test]
fn rows_accept_open_ended_ranges() {
    let input = TempFile::new("rows-open", INPUT);
    assert_eq!(
        run_ft_stdout(&["-R", "3-", input.path_str()]),
        "charlie foo\nbravo foo\n"
    );
    assert_eq!(
        run_ft_stdout(&["-R", "-1", input.path_str()]),
        "delta foo\n"
    );
}

#[test]
fn rows_accept_end_relative_ranges() {
    let input = TempFile::new("rows-end-relative", INPUT);
    assert_eq!(
        run_ft_stdout(&["-R", "~2-~1", input.path_str()]),
        "charlie foo\nbravo foo\n"
    );
    assert_eq!(
        run_ft_stdout(&["-R", "2-~2", input.path_str()]),
        "alpha foo\ncharlie foo\n"
    );
}

#[test]
fn end_relative_rows_work_on_stdin() {
    let output = run_ft_with_stdin(&["-R", "~1"], INPUT);
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "bravo foo\n");
}

#[test]
fn delete_removes_end_relative_rows() {
    let input = TempFile::new("delete-rows-end-relative", INPUT);
    let stdout = run_ft_stdout(&["-d", "-R", "~1", input.path_str()]);
    assert_eq!(stdout, "delta foo\nalpha foo\ncharlie foo\n");
}

#[test]
fn columns_reject_end_relative_ranges() {
    let output = run_ft(&["-C", "~2-~1", "input.txt"]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("end-relative"),
        "stderr should mention end-relative values"
    );
}

#[test]
fn delete_removes_a_list_of_row_ranges() {
    let input = TempFile::new("delete-rows-list", INPUT);
    let stdout = run_ft_stdout(&["-d", "-R", "1,4", input.path_str()]);
    assert_eq!(stdout, "alpha foo\ncharlie foo\n");
}

#[test]
fn columns_accept_open_ended_range() {
    let input = TempFile::new("cols-open", INPUT);
    let stdout = run_ft_stdout(&["-C", "7-", input.path_str()]);
    //"charlie foo" has its 7th character at "e"
    assert_eq!(stdout, "foo\nfoo\ne foo\nfoo\n");
}

#[test]
fn cols_range_alone_selects_columns() {
    let input = TempFile::new("select-cols", INPUT);
    let stdout = run_ft_stdout(&["-C", "1-5", input.path_str()]);
    assert_eq!(stdout, "delta\nalpha\ncharl\nbravo\n");
}

#[test]
fn cols_selection_combines_with_row_selection() {
    let input = TempFile::new("select-rows-cols", INPUT);
    let stdout = run_ft_stdout(&["-R", "2-3", "-C", "7-9", input.path_str()]);
    //"alpha foo" has "foo" at columns 7-9; "charlie foo" has "e f" there
    assert_eq!(stdout, "foo\ne f\n");
}

#[test]
fn fields_mode_selects_delimited_fields() {
    let input = TempFile::new("fields-select", "one,two,three\nuno,dos,tres\n");
    let stdout = run_ft_stdout(&["-F", ",", "-C", "2", input.path_str()]);
    assert_eq!(stdout, "two\ndos\n");
}

#[test]
fn fields_mode_delete_removes_field_and_delimiter() {
    let input = TempFile::new("fields-delete", "one,two,three\nuno,dos,tres\n");
    let stdout = run_ft_stdout(&["-d", "-F", ",", "-C", "2", input.path_str()]);
    assert_eq!(stdout, "one,three\nuno,tres\n");
}

#[test]
fn fields_mode_sorts_by_field_key() {
    let input = TempFile::new("fields-sort", "x;30\ny;4\nz;19\n");
    let stdout = run_ft_stdout(&["-s", "-n", "-F", ";", "-C", "2", input.path_str()]);
    assert_eq!(stdout, "y;4\nz;19\nx;30\n");
}

#[test]
fn fields_flag_requires_columns() {
    let output = run_ft(&["-F", ",", "input.txt"]);
    assert!(!output.status.success());
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
fn in_place_rewrites_the_input_file() {
    let input = TempFile::new("in-place", INPUT);
    let stdout = run_ft_stdout(&["-i", "-f", "foo", "-r", "BAR", input.path_str()]);

    //nothing goes to stdout; the file itself is updated
    assert_eq!(stdout, "");
    let rewritten = fs::read_to_string(input.path_str()).expect("input file vanished");
    assert_eq!(rewritten, "delta BAR\nalpha BAR\ncharlie BAR\nbravo BAR\n");
}

#[cfg(unix)]
#[test]
fn in_place_preserves_file_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let input = TempFile::new("in-place-perms", INPUT);
    //a non-default mode the umask would not produce on its own
    fs::set_permissions(input.path_str(), fs::Permissions::from_mode(0o640)).unwrap();

    run_ft_stdout(&["-i", "-f", "foo", "-r", "BAR", input.path_str()]);

    let mode = fs::metadata(input.path_str())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o640);
}

#[test]
fn in_place_leaves_no_temporary_file_behind() {
    //an isolated directory so a concurrent in-place test can't be
    //mistaken for a leftover temp file
    let dir = std::env::temp_dir().join(format!("ft-test-{}-in-place-clean", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let input = dir.join("data.txt");
    fs::write(&input, INPUT).unwrap();

    run_ft_stdout(&["-i", "-d", "-R", "1", input.to_str().unwrap()]);

    let entries: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name())
        .collect();
    let _ = fs::remove_dir_all(&dir);
    //only the rewritten input remains, no `.ft-*.tmp` sibling
    assert_eq!(entries, [std::ffi::OsString::from("data.txt")]);
}

#[test]
fn in_place_requires_a_file_not_stdin() {
    let output = run_ft_with_stdin(&["-i", "-f", "a", "-r", "b"], INPUT);
    assert!(!output.status.success());
}

#[test]
fn regex_replace_works() {
    let input = TempFile::new("regex-replace", "a1 bb22\nccc333 d\n");
    let stdout = run_ft_stdout(&["-e", "-f", r"\d+", "-r", "N", input.path_str()]);
    assert_eq!(stdout, "aN bbN\ncccN d\n");
}

#[test]
fn regex_replace_supports_captures() {
    let input = TempFile::new("regex-captures", "user@host\n");
    let stdout = run_ft_stdout(&["-e", "-f", r"(\w+)@(\w+)", "-r", "$2.$1", input.path_str()]);
    assert_eq!(stdout, "host.user\n");
}

#[test]
fn tac_reverses_row_order() {
    let input = TempFile::new("tac", INPUT);
    let stdout = run_ft_stdout(&["--tac", input.path_str()]);
    assert_eq!(stdout, "bravo foo\ncharlie foo\nalpha foo\ndelta foo\n");
}

#[test]
fn shuffle_keeps_all_rows() {
    let input = TempFile::new("shuffle", INPUT);
    let stdout = run_ft_stdout(&["--shuffle", input.path_str()]);
    let mut lines: Vec<&str> = stdout.lines().collect();
    lines.sort_unstable();
    assert_eq!(
        lines,
        ["alpha foo", "bravo foo", "charlie foo", "delta foo"]
    );
}

#[test]
fn tac_conflicts_with_sort() {
    let input = TempFile::new("tac-sort", INPUT);
    let output = run_ft(&["--tac", "-s", input.path_str()]);
    assert!(!output.status.success());
}

#[test]
fn unique_drops_duplicate_rows() {
    let input = TempFile::new("unique", "one\ntwo\none\nthree\n");
    let stdout = run_ft_stdout(&["-u", input.path_str()]);
    assert_eq!(stdout, "one\ntwo\nthree\n");
}

#[test]
fn unique_with_sort_and_column_key() {
    let input = TempFile::new("unique-sort", "b 2\na 1\nb 9\n");
    let stdout = run_ft_stdout(&["-s", "-u", "-C", "1-1", input.path_str()]);
    assert_eq!(stdout, "a 1\nb 2\n");
}

#[test]
fn grep_keeps_matching_rows() {
    let input = TempFile::new("grep", INPUT);
    let stdout = run_ft_stdout(&["-g", "^(a|b)", input.path_str()]);
    assert_eq!(stdout, "alpha foo\nbravo foo\n");
}

#[test]
fn grep_invert_keeps_non_matching_rows() {
    let input = TempFile::new("grep-invert", INPUT);
    let stdout = run_ft_stdout(&["-g", "^(a|b)", "--invert", input.path_str()]);
    assert_eq!(stdout, "delta foo\ncharlie foo\n");
}

#[test]
fn grep_with_delete_removes_matching_rows() {
    let input = TempFile::new("grep-delete", INPUT);
    let stdout = run_ft_stdout(&["-d", "-g", "charlie", input.path_str()]);
    assert_eq!(stdout, "delta foo\nalpha foo\nbravo foo\n");
}

#[test]
fn upper_transforms_column_range() {
    let input = TempFile::new("upper", INPUT);
    let stdout = run_ft_stdout(&["--upper", "-C", "1-3", input.path_str()]);
    assert_eq!(stdout, "DELta foo\nALPha foo\nCHArlie foo\nBRAvo foo\n");
}

#[test]
fn lower_transforms_whole_line_without_columns() {
    let input = TempFile::new("lower", "AbC dEf\n");
    let stdout = run_ft_stdout(&["--lower", input.path_str()]);
    assert_eq!(stdout, "abc def\n");
}

#[test]
fn trim_removes_surrounding_whitespace() {
    let input = TempFile::new("trim", "  a  \n\tb\t\n");
    let stdout = run_ft_stdout(&["--trim", input.path_str()]);
    assert_eq!(stdout, "a\nb\n");
}

#[test]
fn upper_conflicts_with_lower() {
    let input = TempFile::new("upper-lower", INPUT);
    let output = run_ft(&["--upper", "--lower", input.path_str()]);
    assert!(!output.status.success());
}

#[test]
fn ignore_case_replaces_literal_in_any_case() {
    let input = TempFile::new("ignore-case", "Foo bar FOO\n");
    let stdout = run_ft_stdout(&["--ignore-case", "-f", "foo", "-r", "X", input.path_str()]);
    assert_eq!(stdout, "X bar X\n");
}

#[test]
fn ignore_case_applies_to_regex_patterns() {
    let input = TempFile::new("ignore-case-regex", "abc DEF\n");
    let stdout = run_ft_stdout(&[
        "-e",
        "--ignore-case",
        "-f",
        "[a-z]+",
        "-r",
        "X",
        input.path_str(),
    ]);
    assert_eq!(stdout, "X X\n");
}

#[test]
fn invalid_regex_is_rejected() {
    let input = TempFile::new("regex-invalid", INPUT);
    let output = run_ft(&["-e", "-f", "[unclosed", "-r", "N", input.path_str()]);
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Invalid regular expression"),
        "unexpected stderr: {stderr}"
    );
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
fn reads_stdin_when_no_filename_given() {
    let output = run_ft_with_stdin(&["-s"], INPUT);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "alpha foo\nbravo foo\ncharlie foo\ndelta foo\n"
    );
}

#[test]
fn dash_filename_reads_stdin() {
    let output = run_ft_with_stdin(&["-f", "foo", "-r", "BAR", "-"], "one foo\n");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "one BAR\n");
}

#[test]
fn delete_without_any_range_is_rejected() {
    let input = TempFile::new("delete-no-range", INPUT);
    let output = run_ft(&["-d", input.path_str()]);
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Delete requires"),
        "unexpected stderr: {stderr}"
    );
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
fn missing_input_file_fails_with_message_on_stderr() {
    let output = run_ft(&["/nonexistent/ft-test-missing.txt"]);
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Cannot open input file"),
        "unexpected stderr: {stderr}"
    );
}
