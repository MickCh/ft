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
fn find_without_replace_is_rejected() {
    //a lone --find would otherwise do nothing at all
    let output = run_ft(&["-f", "foo", "input.txt"]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--replace"),
        "stderr should point at the missing --replace"
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
fn wrap_expands_lines_into_chunks() {
    let input = TempFile::new("wrap", "abcdefg\nhi\n");
    let stdout = run_ft_stdout(&["--wrap", "3", input.path_str()]);
    assert_eq!(stdout, "abc\ndef\ng\nhi\n");
}

#[test]
fn wrap_rejects_a_zero_width() {
    let input = TempFile::new("wrap-zero", "abc\n");
    let output = run_ft(&["--wrap", "0", input.path_str()]);
    assert!(!output.status.success());
}

#[test]
fn drop_empty_removes_lines_emptied_by_a_transform() {
    //cutting column 3 leaves the short row empty, and --drop-empty
    //removes it — a predicate could not, it runs before the cut
    let input = TempFile::new("drop-empty", "abc\nx\ndef\n");
    let stdout = run_ft_stdout(&["-C", "3", "--drop-empty", input.path_str()]);
    assert_eq!(stdout, "c\nf\n");
}

#[test]
fn trim_and_drop_empty_remove_whitespace_only_lines() {
    let input = TempFile::new("drop-blank", "a\n   \nb\n\n");
    let stdout = run_ft_stdout(&["--trim", "--drop-empty", input.path_str()]);
    assert_eq!(stdout, "a\nb\n");
}

#[test]
fn quoted_csv_does_not_split_inside_quotes() {
    let input = TempFile::new("csv-quoted", "a,\"b,c\",d\nx,\"y,z\",w\n");

    //without --quoted the comma inside the quotes splits the field
    let stdout = run_ft_stdout(&["-F", ",", "-C", "2", input.path_str()]);
    assert_eq!(stdout, "\"b\n\"y\n");

    //with it, field 2 is the quoted one, comma and all
    let stdout = run_ft_stdout(&["-F", ",", "--quoted", "-C", "2", input.path_str()]);
    assert_eq!(stdout, "\"b,c\"\n\"y,z\"\n");
}

#[test]
fn quoted_csv_projection_stays_valid_csv() {
    let input = TempFile::new("csv-project", "a,\"b,c\",d\n");
    let stdout = run_ft_stdout(&["-F", ",", "--quoted", "-C", "2,1", input.path_str()]);
    assert_eq!(stdout, "\"b,c\",a\n");
}

#[test]
fn quoted_requires_fields() {
    let input = TempFile::new("csv-quoted-alone", "a\n");
    let output = run_ft(&["--quoted", input.path_str()]);
    assert!(!output.status.success());
}

#[test]
fn column_list_permutes_fields() {
    //an awk-style projection: -F , -C 3,1,2 reorders the fields
    let input = TempFile::new("cols-permute", "a,b,c\nx,y,z\n");
    let stdout = run_ft_stdout(&["-F", ",", "-C", "3,1,2", input.path_str()]);
    assert_eq!(stdout, "c,a,b\nz,x,y\n");
}

#[test]
fn column_list_selects_disjoint_char_ranges() {
    let input = TempFile::new("cols-list", "abcdef\n");
    let stdout = run_ft_stdout(&["-C", "1,3,5-6", input.path_str()]);
    assert_eq!(stdout, "acef\n");
}

#[test]
fn column_list_with_output_delimiter() {
    let input = TempFile::new("cols-outdelim", "a,b,c\n");
    let stdout = run_ft_stdout(&[
        "-F",
        ",",
        "-C",
        "3,1",
        "--output-delimiter",
        " | ",
        input.path_str(),
    ]);
    assert_eq!(stdout, "c | a\n");
}

#[test]
fn column_list_deletes_disjoint_fields() {
    let input = TempFile::new("cols-del-list", "a,b,c,d\n");
    let stdout = run_ft_stdout(&["-d", "-F", ",", "-C", "1,3", input.path_str()]);
    assert_eq!(stdout, "b,d\n");
}

#[test]
fn column_list_scopes_a_replacement_to_every_part() {
    let input = TempFile::new("cols-replace-list", "x,x,x\n");
    let stdout = run_ft_stdout(&[
        "-F",
        ",",
        "-C",
        "1,3",
        "-f",
        "x",
        "-r",
        "Y",
        input.path_str(),
    ]);
    assert_eq!(stdout, "Y,x,Y\n");
}

#[test]
fn sort_key_leaves_cols_to_another_operation() {
    //sort by field 1, while --cols scopes the replacement to field 2
    let input = TempFile::new("sort-key", "b,x\na,x\n");
    let stdout = run_ft_stdout(&[
        "-s",
        "--sort-key",
        "1",
        "-F",
        ",",
        "-C",
        "2",
        "-f",
        "x",
        "-r",
        "X",
        input.path_str(),
    ]);
    assert_eq!(stdout, "a,X\nb,X\n");
}

#[test]
fn unique_key_is_independent_of_cols() {
    //dedupe on field 1, uppercase field 2
    let input = TempFile::new("unique-key", "a,x\na,y\nb,z\n");
    let stdout = run_ft_stdout(&[
        "-u",
        "--unique-key",
        "1",
        "-F",
        ",",
        "-C",
        "2",
        "--upper",
        input.path_str(),
    ]);
    assert_eq!(stdout, "a,X\nb,Z\n");
}

#[test]
fn sort_key_requires_sort() {
    let input = TempFile::new("sort-key-alone", "a\n");
    let output = run_ft(&["--sort-key", "1", input.path_str()]);
    assert!(!output.status.success());
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
fn sort_key_beyond_short_lines_is_empty() {
    //no line reaches column 5, so all keys compare equal and the
    //stable sort keeps the input order (like `sort -k` on missing fields)
    let input = TempFile::new("sort-short-lines", "zz\nabc\nxy\n");
    let stdout = run_ft_stdout(&["-s", "-C", "5-6", input.path_str()]);
    assert_eq!(stdout, "zz\nabc\nxy\n");
}

#[test]
fn reorder_keeps_noncontiguous_row_segments_in_place() {
    //deleting a column keeps rows outside the range, so each selected
    //segment reverses in place instead of drifting past the kept rows
    let input = TempFile::new("tac-segments", "Xa\nXb\nXc\nXd\nXe\nXf\nXg\n");
    let stdout = run_ft_stdout(&["-d", "-C", "1", "--tac", "-R", "2-3,6-7", input.path_str()]);
    assert_eq!(stdout, "Xa\nc\nb\nXd\nXe\ng\nf\n");
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
fn output_aliasing_the_input_file_is_rejected() {
    //`File::create` would truncate the input before the first read
    let input = TempFile::new("output-is-input", INPUT);
    let output = run_ft(&["-R", "1-2", "-o", input.path_str(), input.path_str()]);

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--in-place"),
        "stderr should point at --in-place: {stderr}"
    );
    //the input file must be left untouched, not truncated
    assert_eq!(fs::read_to_string(&input.path).unwrap(), INPUT);
}

#[cfg(unix)]
#[test]
fn output_symlink_aliasing_the_input_is_rejected() {
    let input = TempFile::new("output-is-input-target", INPUT);
    let link = std::env::temp_dir().join(format!("ft-test-{}-output-link", std::process::id()));
    std::os::unix::fs::symlink(&input.path, &link).unwrap();

    let output = run_ft(&["-o", link.to_str().unwrap(), input.path_str()]);
    let _ = fs::remove_file(&link);

    assert!(!output.status.success());
    assert_eq!(fs::read_to_string(&input.path).unwrap(), INPUT);
}

#[cfg(unix)]
#[test]
fn broken_pipe_from_a_closed_consumer_is_not_an_error() {
    //enough output to overfill the pipe buffer, so writing blocks until
    //the reading end is gone and fails with EPIPE
    let content = "0123456789abcdef\n".repeat(100_000);
    let input = TempFile::new("broken-pipe", &content);

    let mut child = Command::new(env!("CARGO_BIN_EXE_ft"))
        .arg(input.path_str())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn ft binary");
    //close the reading end without consuming anything, like `head -0`
    drop(child.stdout.take());

    let output = child
        .wait_with_output()
        .expect("failed to wait for ft binary");
    assert!(
        output.status.success(),
        "expected a quiet exit, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
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

#[test]
fn several_files_are_read_as_one_stream() {
    let first = TempFile::new("multi-a", "a1\na2\n");
    let second = TempFile::new("multi-b", "b1\nb2\n");

    let stdout = run_ft_stdout(&[first.path_str(), second.path_str()]);
    assert_eq!(stdout, "a1\na2\nb1\nb2\n");

    //row numbers address the concatenation, like `cat a b | ft`
    let stdout = run_ft_stdout(&["-R", "2-3", first.path_str(), second.path_str()]);
    assert_eq!(stdout, "a2\nb1\n");
}

#[test]
fn in_place_edits_each_file_on_its_own() {
    let first = TempFile::new("batch-a", "foo one\nfoo two\n");
    let second = TempFile::new("batch-b", "foo three\n");

    let stdout = run_ft_stdout(&[
        "-i",
        "-f",
        "foo",
        "-r",
        "BAR",
        first.path_str(),
        second.path_str(),
    ]);
    assert_eq!(stdout, "");

    assert_eq!(
        fs::read_to_string(first.path_str()).expect("first file vanished"),
        "BAR one\nBAR two\n"
    );
    assert_eq!(
        fs::read_to_string(second.path_str()).expect("second file vanished"),
        "BAR three\n"
    );
}

#[test]
fn in_place_numbers_rows_per_file() {
    //row 1 of each file, not row 1 of the concatenation
    let first = TempFile::new("batch-rows-a", "a1\na2\n");
    let second = TempFile::new("batch-rows-b", "b1\nb2\n");

    run_ft_stdout(&["-i", "-d", "-R", "1", first.path_str(), second.path_str()]);

    assert_eq!(
        fs::read_to_string(first.path_str()).expect("first file vanished"),
        "a2\n"
    );
    assert_eq!(
        fs::read_to_string(second.path_str()).expect("second file vanished"),
        "b2\n"
    );
}

#[test]
fn output_aliasing_any_input_is_rejected() {
    let first = TempFile::new("alias-a", "a\n");
    let second = TempFile::new("alias-b", "b\n");

    //the output aliases the *second* input, which would be truncated
    let output = run_ft(&["-o", second.path_str(), first.path_str(), second.path_str()]);
    assert!(!output.status.success());
    assert_eq!(
        fs::read_to_string(second.path_str()).expect("input file vanished"),
        "b\n"
    );
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

#[cfg(unix)]
#[test]
fn in_place_edits_the_symlink_target_and_keeps_the_link() {
    let target = TempFile::new("in-place-link-target", INPUT);
    let link = std::env::temp_dir().join(format!("ft-test-{}-in-place-link", std::process::id()));
    std::os::unix::fs::symlink(&target.path, &link).unwrap();

    let stdout = run_ft_stdout(&["-i", "-f", "foo", "-r", "BAR", link.to_str().unwrap()]);
    assert_eq!(stdout, "");

    let still_symlink = fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink();
    let rewritten = fs::read_to_string(&target.path).unwrap();
    let _ = fs::remove_file(&link);
    assert!(still_symlink, "the symlink must survive in-place editing");
    assert_eq!(rewritten, "delta BAR\nalpha BAR\ncharlie BAR\nbravo BAR\n");
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
fn delete_conflicts_with_reorder() {
    let input = TempFile::new("delete-reorder", INPUT);
    //deleting whole rows leaves nothing to sort
    let output = run_ft(&["-d", "-s", "-R", "2-3", input.path_str()]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("reorder"),
        "stderr should explain the delete/reorder conflict"
    );
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
fn unique_dedupes_empty_fields() {
    //"b," and "c," share the empty field 2 as their key
    let input = TempFile::new("unique-empty-field", "a,1\nb,\nc,\nd,1\n");
    let stdout = run_ft_stdout(&["-u", "-F", ",", "-C", "2", input.path_str()]);
    assert_eq!(stdout, "a,1\nb,\n");
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
