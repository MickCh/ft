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
    fn new(name: &str, content: impl AsRef<[u8]>) -> TempFile {
        let path = std::env::temp_dir().join(format!("fxt-test-{}-{}", std::process::id(), name));
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

fn run_fxt(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fxt"))
        .args(args)
        .output()
        .expect("failed to run fxt binary")
}

fn run_fxt_with_stdin(args: &[&str], input: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_fxt"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn fxt binary");

    child
        .stdin
        .take()
        .expect("stdin not captured")
        .write_all(input.as_bytes())
        .expect("failed to write to fxt stdin");

    child
        .wait_with_output()
        .expect("failed to wait for fxt binary")
}

fn run_fxt_stdout(args: &[&str]) -> String {
    let output = run_fxt(args);
    assert!(
        output.status.success(),
        "fxt failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is not valid UTF-8")
}

#[test]
fn no_flags_passes_file_through() {
    let input = TempFile::new("passthrough", INPUT);
    assert_eq!(run_fxt_stdout(&[input.path_str()]), INPUT);
}

#[test]
fn replace_works_without_delete() {
    let input = TempFile::new("replace", INPUT);
    let stdout = run_fxt_stdout(&["-f", "foo", "-r", "BAR", input.path_str()]);
    assert_eq!(stdout, "delta BAR\nalpha BAR\ncharlie BAR\nbravo BAR\n");
}

#[test]
fn multiple_find_replace_pairs_apply_in_order() {
    let input = TempFile::new("replace-pairs", "cat dog\n");
    //cat->dog runs first, then dog->wolf rewrites both occurrences
    let stdout = run_fxt_stdout(&[
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
    let output = run_fxt(&["-f", "a", "-r", "1", "-f", "b", "input.txt"]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--replace"),
        "stderr should explain the find/replace mismatch"
    );
}

#[test]
fn find_without_replace_is_rejected() {
    //a lone --find would otherwise do nothing at all
    let output = run_fxt(&["-f", "foo", "input.txt"]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--replace"),
        "stderr should point at the missing --replace"
    );
}

#[test]
fn replace_applies_only_to_selected_rows() {
    let input = TempFile::new("replace-rows", INPUT);
    let stdout = run_fxt_stdout(&["-R", "2-3", "-f", "foo", "-r", "BAR", input.path_str()]);
    assert_eq!(stdout, "alpha BAR\ncharlie BAR\n");
}

#[test]
fn replace_applies_only_inside_column_range() {
    let input = TempFile::new("replace-cols", INPUT);
    let stdout = run_fxt_stdout(&["-C", "7-9", "-f", "foo", "-r", "BAR", input.path_str()]);
    //"foo" occupies columns 7-9 in every line except "charlie foo" (columns 9-11)
    assert_eq!(stdout, "delta BAR\nalpha BAR\ncharlie foo\nbravo BAR\n");
}

#[test]
fn sort_works_without_delete() {
    let input = TempFile::new("sort", INPUT);
    let stdout = run_fxt_stdout(&["-s", input.path_str()]);
    assert_eq!(stdout, "alpha foo\nbravo foo\ncharlie foo\ndelta foo\n");
}

#[test]
fn sort_uses_column_range_as_key() {
    let input = TempFile::new("sort-cols", "b 2\nc 1\na 3\n");
    let stdout = run_fxt_stdout(&["-s", "-C", "3-3", input.path_str()]);
    assert_eq!(stdout, "c 1\nb 2\na 3\n");
}

#[test]
fn wrap_expands_lines_into_chunks() {
    let input = TempFile::new("wrap", "abcdefg\nhi\n");
    let stdout = run_fxt_stdout(&["--wrap", "3", input.path_str()]);
    assert_eq!(stdout, "abc\ndef\ng\nhi\n");
}

#[test]
fn wrap_rejects_a_zero_width() {
    let input = TempFile::new("wrap-zero", "abc\n");
    let output = run_fxt(&["--wrap", "0", input.path_str()]);
    assert!(!output.status.success());
}

#[test]
fn encode_and_decode_each_field() {
    let input = TempFile::new("encode", "id,żółw a/b\n2,\n");
    assert_eq!(
        run_fxt_stdout(&["-F", ",", "-C", "2", "--encode", "url", input.path_str()]),
        "id,%C5%BC%C3%B3%C5%82w%20a%2Fb\n2,\n"
    );
    //fields 1-2 are encoded one by one, the empty one too
    assert_eq!(
        run_fxt_stdout(&[
            "-F",
            ",",
            "-C",
            "1-2",
            "--encode",
            "base64",
            input.path_str()
        ]),
        "aWQ=,xbzDs8WCdyBhL2I=\nMg==,\n"
    );

    let encoded = TempFile::new("decode", "1,Zm9v\n2,YmFy\n");
    assert_eq!(
        run_fxt_stdout(&[
            "-F",
            ",",
            "-C",
            "2",
            "--decode",
            "base64",
            encoded.path_str()
        ]),
        "1,foo\n2,bar\n"
    );
    //decoding, then encoding again, converts between codecs
    assert_eq!(
        run_fxt_stdout(&[
            "-F",
            ",",
            "-C",
            "2",
            "--decode",
            "base64",
            "--encode",
            "hex",
            encoded.path_str()
        ]),
        "1,666f6f\n2,626172\n"
    );
}

#[test]
fn decoding_fails_on_invalid_input_and_lossy_repairs_bad_utf8() {
    let invalid = TempFile::new("decode-invalid", "%zz\n");
    let output = run_fxt(&["--decode", "url", invalid.path_str()]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("`%zz` is not valid url"), "{stderr}");

    //0xFF is no UTF-8: an error, unless --lossy repairs it
    let not_utf8 = TempFile::new("decode-not-utf8", "61ff62\n");
    let output = run_fxt(&["--decode", "hex", not_utf8.path_str()]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        run_fxt_stdout(&["--lossy", "--decode", "hex", not_utf8.path_str()]),
        "a\u{FFFD}b\n"
    );
}

#[test]
fn expand_and_unexpand_convert_tabs_and_spaces() {
    let tabbed = TempFile::new("expand", "a\tbc\td\n");
    assert_eq!(
        run_fxt_stdout(&["--expand", tabbed.path_str()]),
        "a       bc      d\n"
    );
    assert_eq!(
        run_fxt_stdout(&["--expand=4", tabbed.path_str()]),
        "a   bc  d\n"
    );

    let spaced = TempFile::new("unexpand", "a   bc  d\n");
    assert_eq!(
        run_fxt_stdout(&["--unexpand=4", spaced.path_str()]),
        "a\tbc\td\n"
    );
}

#[test]
fn expand_takes_its_tab_size_only_after_an_equals_sign() {
    //a bare --expand must not swallow the file name that follows
    let tabbed = TempFile::new("expand-bare", "a\tb\n");
    assert_eq!(
        run_fxt_stdout(&["--expand", tabbed.path_str()]),
        "a       b\n"
    );
    let output = run_fxt_with_stdin(&["--expand=0"], "a\tb\n");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn reverse_chars_reverses_the_column_range() {
    let input = TempFile::new("reverse-chars", "abc,żółw\ne\u{301}x\n");
    assert_eq!(
        run_fxt_stdout(&["-F", ",", "-C", "2", "--reverse-chars", input.path_str()]),
        "abc,włóż\ne\u{301}x\n"
    );
    //by chars the accent comes off its letter; by graphemes it stays on
    assert_eq!(
        run_fxt_stdout(&["-R", "2", "--reverse-chars", input.path_str()]),
        "x\u{301}e\n"
    );
    assert_eq!(
        run_fxt_stdout(&[
            "-R",
            "2",
            "--graphemes",
            "--reverse-chars",
            input.path_str()
        ]),
        "xe\u{301}\n"
    );
}

#[test]
fn pad_left_right_aligns_each_field_on_its_own() {
    let input = TempFile::new("pad-left", "a,10,x\nlonger,2,\n,,z\n");
    let stdout = run_fxt_stdout(&["-F", ",", "-C", "1-2", "--pad-left", "4", input.path_str()]);
    //fields 1 and 2 are padded separately, the empty ones too, and a
    //field already wider is left whole
    assert_eq!(stdout, "   a,  10,x\nlonger,   2,\n    ,    ,z\n");
}

#[test]
fn pad_right_counts_chars_and_with_graphemes_clusters() {
    let input = TempFile::new("pad-right", "ż\ne\u{301}\n");
    assert_eq!(
        run_fxt_stdout(&["--pad-right", "3", input.path_str()]),
        "ż  \ne\u{301} \n"
    );
    assert_eq!(
        run_fxt_stdout(&["--graphemes", "--pad-right", "3", input.path_str()]),
        "ż  \ne\u{301}  \n"
    );
}

#[test]
fn padding_pads_an_empty_line_too() {
    let input = TempFile::new("pad-empty", "\nab\n");
    assert_eq!(
        run_fxt_stdout(&["--pad-left", "3", input.path_str()]),
        "   \n ab\n"
    );
    assert_eq!(
        run_fxt_stdout(&["-C", "1-", "--pad-right", "2", input.path_str()]),
        "  \nab\n"
    );
    //a span starting further in has no cell on an empty line
    assert_eq!(
        run_fxt_stdout(&["-C", "2-", "--pad-left", "3", input.path_str()]),
        "\na  b\n"
    );
}

#[test]
fn an_empty_find_is_refused() {
    let output = run_fxt_with_stdin(&["-f", "", "-r", "x"], "abc\n");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("The pattern to find cannot be empty"),
        "{stderr}"
    );
}

#[test]
fn in_place_leaves_a_file_no_row_was_selected_in_as_it_was() {
    let input = TempFile::new("in-place-no-row", "a\n");
    let output = run_fxt(&["-i", "-R", "5", input.path_str()]);
    //no filter, so nothing failed to match: exit 0, the file intact
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(fs::read_to_string(&input.path).unwrap(), "a\n");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no row selected, left unchanged"),
        "{stderr}"
    );

    let dry_run = run_fxt(&["-i", "--dry-run", "--from", "zz", input.path_str()]);
    let stdout = String::from_utf8_lossy(&dry_run.stdout);
    assert!(stdout.contains("unchanged (nothing matched)"), "{stdout}");

    //an empty file has nothing to lose, and nothing to say about it
    let empty = TempFile::new("in-place-empty", "");
    let output = run_fxt(&["-i", "-R", "5", empty.path_str()]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
}

#[test]
fn output_delimiter_rewrites_rows_no_selection_cut() {
    let stdout = |args: &[&str], input: &str| {
        let output = run_fxt_with_stdin(args, input);
        assert!(output.status.success(), "{args:?}");
        String::from_utf8(output.stdout).unwrap()
    };
    //a plain conversion, and with the fields' quotes kept
    assert_eq!(
        stdout(&["-F", ",", "--output-delimiter", ";"], "a,b,c\n"),
        "a;b;c\n"
    );
    assert_eq!(
        stdout(
            &["-F", ",", "--quoted", "--output-delimiter", ";"],
            "a,\"x,y\"\n"
        ),
        "a;\"x,y\"\n"
    );
    //after a transform that took -C, and after deleting a field
    assert_eq!(
        stdout(
            &["-F", ",", "-C", "2", "--upper", "--output-delimiter", ";"],
            "a,b\n"
        ),
        "a;B\n"
    );
    assert_eq!(
        stdout(
            &["-F", ",", "-d", "-C", "2", "--output-delimiter", ";"],
            "a,b,c\n"
        ),
        "a;c\n"
    );
    //keys are still read with the input delimiter, and numbering comes after
    assert_eq!(
        stdout(
            &["-F", ",", "-s", "-C", "2", "--output-delimiter", ";"],
            "x,2\ny,1\n"
        ),
        "y;1\nx;2\n"
    );
    assert_eq!(
        stdout(&["-F", ",", "--number", "--output-delimiter", ";"], "a,b\n"),
        "1;a;b\n"
    );
    //a selection rejoins once, and an empty delimiter runs fields together
    assert_eq!(
        stdout(
            &["-F", ",", "-C", "3,1", "--output-delimiter", ";"],
            "a,b,c\n"
        ),
        "c;a\n"
    );
    assert_eq!(
        stdout(&["-F", ",", "--output-delimiter", ""], "a,b\n"),
        "ab\n"
    );
    //a summary's columns take it, its rows are not rewritten
    assert_eq!(
        stdout(
            &[
                "-F",
                ",",
                "--group-by",
                "1",
                "--sum",
                "2",
                "--output-delimiter",
                ";"
            ],
            "a,1\na,2\n"
        ),
        "a;3\n"
    );
}

#[test]
fn keys_are_read_with_the_input_delimiter() {
    let run = |args: &[&str], input: &str| {
        let output = run_fxt_with_stdin(args, input);
        (
            output.status.code(),
            String::from_utf8(output.stdout).unwrap(),
        )
    };
    //unique: "a,b" and "a;b" are different rows, whatever the output says
    assert_eq!(
        run(&["-F", ",", "-u", "--output-delimiter", ";"], "a,b\na;b\n"),
        (Some(0), "a;b\na;b\n".to_owned())
    );
    //grep: the pattern sees the row as it was read
    assert_eq!(
        run(&["-F", ",", "-g", ",", "--output-delimiter", ";"], "a,b\n"),
        (Some(0), "a;b\n".to_owned())
    );
    assert_eq!(
        run(&["-F", ",", "-g", ";", "--output-delimiter", ";"], "a,b\n"),
        (Some(1), String::new())
    );
    assert_eq!(
        run(
            &[
                "-F",
                ",",
                "--grep-key",
                "1-2",
                "-g",
                "a,b",
                "--output-delimiter",
                ";"
            ],
            "a,b\n"
        ),
        (Some(0), "a;b\n".to_owned())
    );
    //sort: "," sorts before "-"
    assert_eq!(
        run(&["-F", ",", "-s", "--output-delimiter", "~"], "a-b\na,z\n"),
        (Some(0), "a~z\na-b\n".to_owned())
    );
}

#[test]
fn group_keys_and_distinct_values_compare_the_fields_as_read() {
    //"a;b"+"1" and "a"+"b;1" both print as "a;b;1", but are two groups
    let input = "a;b,1\na,b;1\na;b,1\n";
    let output = run_fxt_with_stdin(
        &[
            "-F",
            ",",
            "--group-by",
            "1-2",
            "--count",
            "--output-delimiter",
            ";",
        ],
        input,
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "a;b;1;2\na;b;1;1\n"
    );
    //and two distinct values
    let output = run_fxt_with_stdin(
        &["-F", ",", "--distinct", "1-2", "--output-delimiter", ";"],
        input,
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "2\n");
}

#[test]
fn a_quote_cannot_delimit_csv_output() {
    let output = run_fxt_with_stdin(
        &["-F", ",", "--quoted", "--output-delimiter", "\""],
        "a,b\n",
    );
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cannot contain a quote"), "{stderr}");
    //without --quoted a quote is just text
    let output = run_fxt_with_stdin(&["-F", ",", "--output-delimiter", "\""], "a,b\n");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "a\"b\n");
}

#[test]
fn join_rows_get_the_output_delimiter_too() {
    let output = run_fxt_with_stdin(
        &["-F", ",", "--join", "|", "--output-delimiter", ";"],
        "a,b\nc,d\n",
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout), "a;b|c;d\n");
}

#[test]
fn in_place_refuses_a_summary_or_a_join() {
    let input = TempFile::new("in-place-summary", "a\n");
    for args in [["-i", "--count"], ["-i", "--join=,"]] {
        let output = run_fxt(&[args[0], args[1], input.path_str()]);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
    }
    assert_eq!(fs::read_to_string(&input.path).unwrap(), "a\n");
}

#[test]
fn a_new_delimiter_quotes_the_fields_that_hold_it() {
    //under --quoted the output stays CSV: "a;b" is one field
    let output = run_fxt_with_stdin(
        &["-F", ",", "--quoted", "--output-delimiter", ";"],
        "a;b,c\n\"x;y\",z\n",
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "\"a;b\";c\n\"x;y\";z\n"
    );
    //without --quoted the fields are text, written as they are
    let output = run_fxt_with_stdin(&["-F", ",", "--output-delimiter", ";"], "a;b,c\n");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "a;b;c\n");
}

#[test]
fn title_case_keeps_combining_accents_and_final_sigma_in_the_word() {
    //"e" + U+0301 is one letter: the word goes on after it
    let input = TempFile::new(
        "title-nfd",
        "e\u{301}cole cafe\u{301} n\u{303}o \u{3a3}\u{39f}\u{3a6}\u{39f}\u{3a3}\n",
    );
    assert_eq!(
        run_fxt_stdout(&["--title-case", input.path_str()]),
        "E\u{301}cole Cafe\u{301} N\u{303}o \u{3a3}\u{3bf}\u{3c6}\u{3bf}\u{3c2}\n"
    );
}

#[test]
fn an_empty_split_separator_is_named_for_what_it_is() {
    let output = run_fxt_with_stdin(&["--split-on", ""], "a\n");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("The separator to split on cannot be empty"),
        "{stderr}"
    );
}

#[test]
fn join_takes_an_empty_separator() {
    let output = run_fxt_with_stdin(&["--join", ""], "a\nb\n");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "ab\n");
}

#[test]
fn a_bad_count_says_what_it_is() {
    let output = run_fxt_with_stdin(&["--min-fields", "0"], "a\n");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("The field count must be at least 1"),
        "{stderr}"
    );

    let output = run_fxt_with_stdin(&["--wrap", "x"], "a\n");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("The width `x` isn't a number"), "{stderr}");
}

#[test]
fn pad_left_and_pad_right_are_exclusive() {
    let output = run_fxt_with_stdin(&["--pad-left", "3", "--pad-right", "3"], "a\n");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn drop_empty_removes_lines_emptied_by_a_transform() {
    //cutting column 3 leaves the short row empty, and --drop-empty
    //removes it — a predicate could not, it runs before the cut
    let input = TempFile::new("drop-empty", "abc\nx\ndef\n");
    let stdout = run_fxt_stdout(&["-C", "3", "--drop-empty", input.path_str()]);
    assert_eq!(stdout, "c\nf\n");
}

#[test]
fn trim_and_drop_empty_remove_whitespace_only_lines() {
    let input = TempFile::new("drop-blank", "a\n   \nb\n\n");
    let stdout = run_fxt_stdout(&["--trim", "--drop-empty", input.path_str()]);
    assert_eq!(stdout, "a\nb\n");
}

#[test]
fn grep_reports_a_match_through_the_exit_code() {
    let input = TempFile::new("exit-grep", "a ERROR\nb INFO\n");

    //0 when something matched, 1 when nothing did — like grep
    let output = run_fxt(&["-g", "ERROR", input.path_str()]);
    assert_eq!(output.status.code(), Some(0));

    let output = run_fxt(&["-g", "NOTHING", input.path_str()]);
    assert_eq!(output.status.code(), Some(1));
    //the rows are still written (there just are none)
    assert!(output.stdout.is_empty());
}

#[test]
fn a_transformation_without_a_filter_always_succeeds() {
    //nothing to fail to match, so an empty input is not a failure
    let input = TempFile::new("exit-plain", "");
    let output = run_fxt(&["--upper", input.path_str()]);
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn a_failure_exits_with_two() {
    //an error is distinguishable from "nothing matched"
    let output = run_fxt(&["-g", "x", "no-such-file.txt"]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn several_greps_match_any_or_with_all_every_one() {
    let input = TempFile::new("greps", "a b\na\nb\nc\n");
    assert_eq!(
        run_fxt_stdout(&["-g", "a", "-g", "b", input.path_str()]),
        "a b\na\nb\n"
    );
    assert_eq!(
        run_fxt_stdout(&["-g", "a", "-g", "b", "--all", input.path_str()]),
        "a b\n"
    );
    //--invert flips the combined match: rows that do not have both
    assert_eq!(
        run_fxt_stdout(&["-g", "a", "-g", "b", "--all", "-v", input.path_str()]),
        "a\nb\nc\n"
    );
}

#[test]
fn min_fields_and_max_len_filter_rows() {
    let input = TempFile::new("min-fields", "a,b,c\na,b\n\"x,y\",z,w\n");
    assert_eq!(
        run_fxt_stdout(&["-F", ",", "--quoted", "--min-fields", "3", input.path_str()]),
        "a,b,c\n\"x,y\",z,w\n"
    );
    //with --delete they choose what goes
    assert_eq!(
        run_fxt_stdout(&["-d", "-F", ",", "--min-fields", "3", input.path_str()]),
        "a,b\n"
    );
    //filters given together narrow each other down
    assert_eq!(
        run_fxt_stdout(&["--max-len", "5", "-g", "b", input.path_str()]),
        "a,b,c\na,b\n"
    );

    //like --grep, a filter that keeps nothing exits with 1
    let output = run_fxt(&["--max-len", "2", input.path_str()]);
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn max_fields_and_min_len_are_the_other_bounds() {
    let input = TempFile::new("max-fields", "a,b,c\na,b\n\"x,y\",z\nlonger line\n");
    //under --quoted "x,y" is one field
    assert_eq!(
        run_fxt_stdout(&["-F", ",", "--quoted", "--max-fields", "2", input.path_str()]),
        "a,b\n\"x,y\",z\nlonger line\n"
    );
    //together they keep the rows between the bounds
    assert_eq!(
        run_fxt_stdout(&[
            "-F",
            ",",
            "--min-fields",
            "2",
            "--max-fields",
            "2",
            input.path_str()
        ]),
        "a,b\n"
    );
    //lengths count chars, not bytes
    let input = TempFile::new("min-len", "ab\nżół\nabcd\n\n");
    assert_eq!(
        run_fxt_stdout(&["--min-len", "3", input.path_str()]),
        "żół\nabcd\n"
    );
    assert_eq!(
        run_fxt_stdout(&["--min-len", "2", "--max-len", "3", input.path_str()]),
        "ab\nżół\n"
    );
    //--delete with --min-len drops the long ones, like -v
    assert_eq!(
        run_fxt_stdout(&["-d", "--min-len", "3", input.path_str()]),
        "ab\n\n"
    );
    //content filters: a --quiet run answers with the exit code
    let output = run_fxt(&["-q", "--min-len", "10", input.path_str()]);
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn a_lower_bound_above_the_upper_one_is_refused() {
    let output = run_fxt_with_stdin(&["--min-len", "5", "--max-len", "3"], "a\n");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--min-len 5 is above --max-len 3"),
        "{stderr}"
    );

    let output = run_fxt_with_stdin(
        &["-F", ",", "--min-fields", "3", "--max-fields", "2"],
        "a\n",
    );
    assert_eq!(output.status.code(), Some(2));
    //a bound of 0 says nothing (--min-len) or keeps nothing (--max-fields)
    for option in ["--min-len", "--max-fields"] {
        let output = run_fxt_with_stdin(&["-F", ",", option, "0"], "a\n");
        assert_eq!(output.status.code(), Some(2), "{option}");
    }
    assert_eq!(
        run_fxt_with_stdin(&["--max-fields", "2"], "a\n")
            .status
            .code(),
        Some(2)
    );
}

#[test]
fn center_pads_each_field_on_both_sides() {
    let input = TempFile::new("center", "a,bb,ccc\n,x,\n");
    assert_eq!(
        run_fxt_stdout(&["-F", ",", "--center", "4", input.path_str()]),
        " a  , bb ,ccc \n    , x  ,    \n"
    );
    //a decomposed accent is one cluster under --graphemes
    let output = run_fxt_with_stdin(&["--graphemes", "--center", "3"], "e\u{301}\n");
    assert_eq!(String::from_utf8_lossy(&output.stdout), " e\u{301} \n");
}

#[test]
fn rot13_rotates_the_column_range_and_undoes_itself() {
    let input = TempFile::new("rot13", "Hello, World!\nfxt,Zebra\n");
    assert_eq!(
        run_fxt_stdout(&["--rot13", input.path_str()]),
        "Uryyb, Jbeyq!\nskg,Mroen\n"
    );
    assert_eq!(
        run_fxt_stdout(&["-F", ",", "-C", "2", "--rot13", input.path_str()]),
        "Hello, Jbeyq!\nfxt,Mroen\n"
    );
    let once = run_fxt_with_stdin(&["--rot13"], "Hello, World!\n");
    let twice = run_fxt_with_stdin(&["--rot13"], &String::from_utf8_lossy(&once.stdout));
    assert_eq!(String::from_utf8_lossy(&twice.stdout), "Hello, World!\n");
}

#[test]
fn min_fields_requires_fields_and_all_requires_grep() {
    assert_eq!(
        run_fxt_with_stdin(&["--min-fields", "2"], "a\n")
            .status
            .code(),
        Some(2)
    );
    assert_eq!(
        run_fxt_with_stdin(&["--all"], "a\n")
            .status
            .code(),
        Some(2)
    );
}

#[test]
fn quiet_answers_with_the_exit_code_alone() {
    let input = TempFile::new("quiet", "a ERROR\nb INFO\n");

    let output = run_fxt(&["-q", "-g", "ERROR", input.path_str()]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty(), "--quiet must write nothing");

    let output = run_fxt(&["-q", "-g", "NOTHING", input.path_str()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
}

#[test]
fn quiet_requires_grep() {
    let input = TempFile::new("quiet-alone", "a\n");
    let output = run_fxt(&["-q", input.path_str()]);
    assert!(!output.status.success());
}

#[test]
fn delete_counts_the_rows_it_removed_as_matches() {
    //the rows matched — that is why they went
    let input = TempFile::new("exit-delete", "a ERROR\nb INFO\n");
    let output = run_fxt(&["-d", "-g", "ERROR", input.path_str()]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&output.stdout), "b INFO\n");
}

#[test]
fn number_numbers_the_output_rows() {
    let input = TempFile::new("number", "a\nb\nc\n");
    let stdout = run_fxt_stdout(&["--number", input.path_str()]);
    assert_eq!(stdout, "1\ta\n2\tb\n3\tc\n");

    //the numbers count the rows that survive the filters, contiguously
    let stdout = run_fxt_stdout(&["--number", "-g", "[ac]", input.path_str()]);
    assert_eq!(stdout, "1\ta\n2\tc\n");

    //and those --unique lets through: the number is no part of the key
    let duplicates = TempFile::new("number-unique", "a\na\nb\n");
    let stdout = run_fxt_stdout(&["--number", "-u", duplicates.path_str()]);
    assert_eq!(stdout, "1\ta\n2\tb\n");
}

#[test]
fn title_case_and_squeeze_clean_up_text() {
    let input = TempFile::new("tidy", "  hello   WIDE world  \n");
    let stdout = run_fxt_stdout(&["--title-case", "--squeeze", "--trim", input.path_str()]);
    assert_eq!(stdout, "Hello Wide World\n");
}

#[test]
fn title_case_capitalizes_only_the_first_letter_of_a_ligature() {
    //titlecase, not uppercase: "fi" ligature, the "dz" digraph, sharp s
    let input = TempFile::new(
        "title-ligature",
        "\u{fb01}sh \u{1c6}ungla stra\u{df}e \u{df}\n",
    );
    let stdout = run_fxt_stdout(&["--title-case", input.path_str()]);
    assert_eq!(stdout, "Fish \u{1c5}ungla Stra\u{df}e Ss\n");
}

#[test]
fn split_on_makes_one_row_per_piece() {
    let input = TempFile::new("split-on", "a,b,c\nd\n");
    let stdout = run_fxt_stdout(&["--split-on", ",", input.path_str()]);
    assert_eq!(stdout, "a\nb\nc\nd\n");
}

#[test]
fn join_folds_the_rows_into_one() {
    let input = TempFile::new("join", "a\nb\nc\n");
    let stdout = run_fxt_stdout(&["--join", ",", input.path_str()]);
    assert_eq!(stdout.trim_end(), "a,b,c");
}

#[test]
fn join_and_split_on_are_inverses() {
    let input = TempFile::new("join-split", "a,b\nc\n");
    let stdout = run_fxt_stdout(&["--split-on", ",", "--join", ",", input.path_str()]);
    assert_eq!(stdout.trim_end(), "a,b,c");
}

#[test]
fn join_conflicts_with_a_summary() {
    //both consume the rows, and there is only one set of them
    let input = TempFile::new("join-count", "a\n");
    let output = run_fxt(&["--join", ",", "--count", input.path_str()]);
    assert!(!output.status.success());
}

#[test]
fn count_summarizes_the_rows() {
    let input = TempFile::new("count", "a ERROR\nb INFO\nc ERROR\n");

    let stdout = run_fxt_stdout(&["--count", input.path_str()]);
    assert_eq!(stdout, "3\n");

    //the summary counts what the filters let through
    let stdout = run_fxt_stdout(&["--count", "-g", "ERROR", input.path_str()]);
    assert_eq!(stdout, "2\n");
}

#[test]
fn count_reports_zero_when_nothing_matches() {
    //like `grep -c`: the count is 0 and the exit code still says
    //"nothing matched"
    let input = TempFile::new("count-zero", "a\nb\n");
    let output = run_fxt(&["--count", "-g", "NOTHING", input.path_str()]);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "0\n");
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn group_by_summarizes_per_key() {
    let input = TempFile::new("group-by", "fruit,3\nveg,10\nfruit,4\n");
    let stdout = run_fxt_stdout(&[
        "-F",
        ",",
        "--group-by",
        "1",
        "--count",
        "--sum",
        "2",
        "--avg",
        "2",
        input.path_str(),
    ]);
    //key, count, sum, avg — one row per key, in first-seen order
    assert_eq!(stdout, "fruit,2,7,3.5\nveg,1,10,10\n");
}

#[test]
fn group_by_with_median_distinct_first_and_last() {
    let input = TempFile::new(
        "group-by-more",
        "fruit,apple,3\nveg,leek,10\nfruit,pear,8\nfruit,apple,4\nveg,kale,x\n",
    );
    //printed in a fixed order, whatever the order of the flags
    let stdout = run_fxt_stdout(&[
        "-F",
        ",",
        "--group-by",
        "1",
        "--last",
        "2",
        "--first",
        "2",
        "--distinct",
        "2",
        "--median",
        "3",
        input.path_str(),
    ]);
    //key, median, distinct, first, last; the `x` is no number for the
    //median but a value like any other for the rest
    assert_eq!(stdout, "fruit,4,2,apple,apple\nveg,10,2,leek,kale\n");
}

#[test]
fn quoted_unique_keys_compare_by_value() {
    //`"a"` and `a` are one value, so the second row is a duplicate; the
    //row that survives is written as it was read
    let input = TempFile::new("unique-quoted", "\"a\",1\na,2\nb,3\n");
    let args = [
        "-F",
        ",",
        "--quoted",
        "-u",
        "--unique-key",
        "1",
        input.path_str(),
    ];
    assert_eq!(run_fxt_stdout(&args), "\"a\",1\nb,3\n");

    //without --quoted a quote is just a character, and the keys differ
    let stdout = run_fxt_stdout(&["-F", ",", "-u", "--unique-key", "1", input.path_str()]);
    assert_eq!(stdout, "\"a\",1\na,2\nb,3\n");
}

#[test]
fn quoted_group_by_keys_group_by_value() {
    //`"a"` and `a` are one group, printed without needless quotes;
    //`"x,y"` keeps the quotes it needs to stay one CSV field
    let input = TempFile::new("group-by-quoted", "\"a\",1\na,2\n\"x,y\",3\nx,4\n");
    let stdout = run_fxt_stdout(&[
        "-F",
        ",",
        "--quoted",
        "--group-by",
        "1",
        "--sum",
        "2",
        input.path_str(),
    ]);
    assert_eq!(stdout, "a,3\n\"x,y\",3\nx,4\n");
}

#[test]
fn quoted_group_by_keeps_multi_field_keys_apart() {
    //both keys unquote to the text a,b,c, but they are different fields
    let input = TempFile::new("group-by-quoted-multi", "a,\"b,c\",1\n\"a,b\",c,2\n");
    let stdout = run_fxt_stdout(&[
        "-F",
        ",",
        "--quoted",
        "--group-by",
        "1-2",
        "--count",
        input.path_str(),
    ]);
    assert_eq!(stdout, "a,\"b,c\",1\n\"a,b\",c,1\n");
}

#[test]
fn min_and_max_report_the_extremes() {
    let input = TempFile::new("min-max", "a,3\nb,-1.5\nc,7\n");
    let stdout = run_fxt_stdout(&["-F", ",", "--min", "2", "--max", "2", input.path_str()]);
    assert_eq!(stdout, "-1.5,7\n");
}

#[test]
fn group_by_requires_a_summary() {
    let input = TempFile::new("group-alone", "a,1\n");
    let output = run_fxt(&["-F", ",", "--group-by", "1", input.path_str()]);
    assert!(!output.status.success());
}

#[test]
fn sort_feeds_the_reducer_in_sorted_order() {
    //a reducer takes the rows in the order they come out, so sorting
    //first reports the groups in sorted order
    let input = TempFile::new("sort-group", "b,1\na,2\nb,3\na,4\n");
    let stdout = run_fxt_stdout(&[
        "-F",
        ",",
        "-s",
        "--group-by",
        "1",
        "--sum",
        "2",
        input.path_str(),
    ]);
    assert_eq!(stdout, "a,6\nb,4\n");
}

#[test]
fn sort_and_join_fold_the_rows_in_sorted_order() {
    let input = TempFile::new("sort-join", "c\na\nb\n");
    let stdout = run_fxt_stdout(&["-s", "--join", ",", input.path_str()]);
    assert_eq!(stdout.trim_end(), "a,b,c");
}

#[test]
fn a_summary_conflicts_with_number() {
    //a summary consumes the rows, so there is nothing left to number
    let input = TempFile::new("count-number", "a\n");
    let output = run_fxt(&["--count", "--number", input.path_str()]);
    assert!(!output.status.success());
}

#[test]
fn quoted_csv_does_not_split_inside_quotes() {
    let input = TempFile::new("csv-quoted", "a,\"b,c\",d\nx,\"y,z\",w\n");

    //without --quoted the comma inside the quotes splits the field
    let stdout = run_fxt_stdout(&["-F", ",", "-C", "2", input.path_str()]);
    assert_eq!(stdout, "\"b\n\"y\n");

    //with it, field 2 is the quoted one, comma and all
    let stdout = run_fxt_stdout(&["-F", ",", "--quoted", "-C", "2", input.path_str()]);
    assert_eq!(stdout, "\"b,c\"\n\"y,z\"\n");
}

#[test]
fn quoted_csv_projection_stays_valid_csv() {
    let input = TempFile::new("csv-project", "a,\"b,c\",d\n");
    let stdout = run_fxt_stdout(&["-F", ",", "--quoted", "-C", "2,1", input.path_str()]);
    assert_eq!(stdout, "\"b,c\",a\n");
}

#[test]
fn quoted_requires_fields() {
    let input = TempFile::new("csv-quoted-alone", "a\n");
    let output = run_fxt(&["--quoted", input.path_str()]);
    assert!(!output.status.success());
}

#[test]
fn column_list_permutes_fields() {
    //an awk-style projection: -F , -C 3,1,2 reorders the fields
    let input = TempFile::new("cols-permute", "a,b,c\nx,y,z\n");
    let stdout = run_fxt_stdout(&["-F", ",", "-C", "3,1,2", input.path_str()]);
    assert_eq!(stdout, "c,a,b\nz,x,y\n");
}

#[test]
fn column_list_selects_disjoint_char_ranges() {
    let input = TempFile::new("cols-list", "abcdef\n");
    let stdout = run_fxt_stdout(&["-C", "1,3,5-6", input.path_str()]);
    assert_eq!(stdout, "acef\n");
}

#[test]
fn column_list_with_output_delimiter() {
    let input = TempFile::new("cols-outdelim", "a,b,c\n");
    let stdout = run_fxt_stdout(&[
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
    let stdout = run_fxt_stdout(&["-d", "-F", ",", "-C", "1,3", input.path_str()]);
    assert_eq!(stdout, "b,d\n");
}

#[test]
fn column_list_scopes_a_replacement_to_every_part() {
    let input = TempFile::new("cols-replace-list", "x,x,x\n");
    let stdout = run_fxt_stdout(&[
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
    let stdout = run_fxt_stdout(&[
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
    let stdout = run_fxt_stdout(&[
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
    let output = run_fxt(&["--sort-key", "1", input.path_str()]);
    assert!(!output.status.success());
}

#[test]
fn numeric_sort_works() {
    let input = TempFile::new("sort-numeric", "10\n9\n2\n");
    let stdout = run_fxt_stdout(&["-s", "-n", input.path_str()]);
    assert_eq!(stdout, "2\n9\n10\n");
}

#[test]
fn reverse_sort_works() {
    let input = TempFile::new("sort-reverse", INPUT);
    let stdout = run_fxt_stdout(&["-s", "--reverse", input.path_str()]);
    assert_eq!(stdout, "delta foo\ncharlie foo\nbravo foo\nalpha foo\n");
}

#[test]
fn numeric_without_sort_is_rejected() {
    let input = TempFile::new("numeric-no-sort", INPUT);
    let output = run_fxt(&["-n", input.path_str()]);
    assert!(!output.status.success());
}

#[test]
fn sort_applies_only_to_selected_rows() {
    let input = TempFile::new("sort-rows", INPUT);
    let stdout = run_fxt_stdout(&["-s", "-R", "2-4", input.path_str()]);
    assert_eq!(stdout, "alpha foo\nbravo foo\ncharlie foo\n");
}

#[test]
fn sort_key_beyond_short_lines_is_empty() {
    //no line reaches column 5, so all keys compare equal and the
    //stable sort keeps the input order (like `sort -k` on missing fields)
    let input = TempFile::new("sort-short-lines", "zz\nabc\nxy\n");
    let stdout = run_fxt_stdout(&["-s", "-C", "5-6", input.path_str()]);
    assert_eq!(stdout, "zz\nabc\nxy\n");
}

#[test]
fn reorder_keeps_noncontiguous_row_segments_in_place() {
    //deleting a column keeps rows outside the range, so each selected
    //segment reverses in place instead of drifting past the kept rows
    let input = TempFile::new("tac-segments", "Xa\nXb\nXc\nXd\nXe\nXf\nXg\n");
    let stdout = run_fxt_stdout(&["-d", "-C", "1", "--tac", "-R", "2-3,6-7", input.path_str()]);
    assert_eq!(stdout, "Xa\nc\nb\nXd\nXe\ng\nf\n");
}

#[test]
fn rows_range_selects_lines() {
    let input = TempFile::new("select-rows", INPUT);
    let stdout = run_fxt_stdout(&["-R", "1-2", input.path_str()]);
    assert_eq!(stdout, "delta foo\nalpha foo\n");
}

#[test]
fn rows_accept_a_list_of_ranges() {
    let input = TempFile::new("rows-list", INPUT);
    let stdout = run_fxt_stdout(&["-R", "1,3-4", input.path_str()]);
    assert_eq!(stdout, "delta foo\ncharlie foo\nbravo foo\n");
}

#[test]
fn rows_accept_open_ended_ranges() {
    let input = TempFile::new("rows-open", INPUT);
    assert_eq!(
        run_fxt_stdout(&["-R", "3-", input.path_str()]),
        "charlie foo\nbravo foo\n"
    );
    assert_eq!(
        run_fxt_stdout(&["-R", "-1", input.path_str()]),
        "delta foo\n"
    );
}

#[test]
fn rows_accept_end_relative_ranges() {
    let input = TempFile::new("rows-end-relative", INPUT);
    assert_eq!(
        run_fxt_stdout(&["-R", "~2-~1", input.path_str()]),
        "charlie foo\nbravo foo\n"
    );
    assert_eq!(
        run_fxt_stdout(&["-R", "2-~2", input.path_str()]),
        "alpha foo\ncharlie foo\n"
    );
}

#[test]
fn end_relative_rows_work_on_stdin() {
    let output = run_fxt_with_stdin(&["-R", "~1"], INPUT);
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "bravo foo\n");
}

#[test]
fn a_tail_still_sees_the_blocks_opened_before_it() {
    //--from must be told every row, also those that never reach the tail
    let input = "b\nx\ny\nz\n";
    let output = run_fxt_with_stdin(&["-R", "~2-", "--from", "b"], input);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "y\nz\n");
    let output = run_fxt_with_stdin(&["-R", "~2-", "--from", "y"], input);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "y\nz\n");
    //and rows before the tail stay where they were when they are kept
    let output = run_fxt_with_stdin(&["-R", "~2-", "--keep-others", "--upper"], input);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "b\nx\nY\nZ\n");
}

#[test]
fn delete_removes_end_relative_rows() {
    let input = TempFile::new("delete-rows-end-relative", INPUT);
    let stdout = run_fxt_stdout(&["-d", "-R", "~1", input.path_str()]);
    assert_eq!(stdout, "delta foo\nalpha foo\ncharlie foo\n");
}

#[test]
fn columns_reject_end_relative_ranges() {
    let output = run_fxt(&["-C", "~2-~1", "input.txt"]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("end-relative"),
        "stderr should mention end-relative values"
    );
}

#[test]
fn delete_removes_a_list_of_row_ranges() {
    let input = TempFile::new("delete-rows-list", INPUT);
    let stdout = run_fxt_stdout(&["-d", "-R", "1,4", input.path_str()]);
    assert_eq!(stdout, "alpha foo\ncharlie foo\n");
}

#[test]
fn columns_accept_open_ended_range() {
    let input = TempFile::new("cols-open", INPUT);
    let stdout = run_fxt_stdout(&["-C", "7-", input.path_str()]);
    //"charlie foo" has its 7th character at "e"
    assert_eq!(stdout, "foo\nfoo\ne foo\nfoo\n");
}

#[test]
fn cols_range_alone_selects_columns() {
    let input = TempFile::new("select-cols", INPUT);
    let stdout = run_fxt_stdout(&["-C", "1-5", input.path_str()]);
    assert_eq!(stdout, "delta\nalpha\ncharl\nbravo\n");
}

#[test]
fn cols_selection_combines_with_row_selection() {
    let input = TempFile::new("select-rows-cols", INPUT);
    let stdout = run_fxt_stdout(&["-R", "2-3", "-C", "7-9", input.path_str()]);
    //"alpha foo" has "foo" at columns 7-9; "charlie foo" has "e f" there
    assert_eq!(stdout, "foo\ne f\n");
}

#[test]
fn fields_mode_selects_delimited_fields() {
    let input = TempFile::new("fields-select", "one,two,three\nuno,dos,tres\n");
    let stdout = run_fxt_stdout(&["-F", ",", "-C", "2", input.path_str()]);
    assert_eq!(stdout, "two\ndos\n");
}

#[test]
fn fields_mode_delete_removes_field_and_delimiter() {
    let input = TempFile::new("fields-delete", "one,two,three\nuno,dos,tres\n");
    let stdout = run_fxt_stdout(&["-d", "-F", ",", "-C", "2", input.path_str()]);
    assert_eq!(stdout, "one,three\nuno,tres\n");
}

#[test]
fn fields_mode_sorts_by_field_key() {
    let input = TempFile::new("fields-sort", "x;30\ny;4\nz;19\n");
    let stdout = run_fxt_stdout(&["-s", "-n", "-F", ";", "-C", "2", input.path_str()]);
    assert_eq!(stdout, "y;4\nz;19\nx;30\n");
}

#[test]
fn fields_stands_alone_without_a_column_option() {
    //a record count needs --quoted, and so --fields, but no columns
    let records = TempFile::new("fields-alone-count", "a,\"l1\nl2\",c\nd,e,f\n");
    assert_eq!(
        run_fxt_stdout(&["-F", ",", "--quoted", "--count", records.path_str()]),
        "2\n"
    );
    //the delimiter separates fxt's own columns
    let plain = TempFile::new("fields-alone-number", "a\nb\n");
    assert_eq!(
        run_fxt_stdout(&["-F", ",", "--number", plain.path_str()]),
        "1,a\n2,b\n"
    );
    //and a transform without -C works on every field
    let padded = TempFile::new("fields-alone-trim", " a , b \n");
    assert_eq!(
        run_fxt_stdout(&["-F", ",", "--trim", padded.path_str()]),
        "a,b\n"
    );
}

#[test]
fn delete_removes_rows_in_range() {
    let input = TempFile::new("delete-rows", INPUT);
    let stdout = run_fxt_stdout(&["-d", "-R", "2-3", input.path_str()]);
    assert_eq!(stdout, "delta foo\nbravo foo\n");
}

#[test]
fn delete_removes_columns_in_every_line() {
    let input = TempFile::new("delete-cols", INPUT);
    let stdout = run_fxt_stdout(&["-d", "-C", "7-9", input.path_str()]);
    //in "charlie foo" columns 7-9 are "e f", leaving "charli" + "oo"
    assert_eq!(stdout, "delta \nalpha \ncharlioo\nbravo \n");
}

#[test]
fn delete_removes_columns_only_in_selected_rows() {
    let input = TempFile::new("delete-rows-cols", INPUT);
    let stdout = run_fxt_stdout(&["-d", "-R", "1-1", "-C", "1-6", input.path_str()]);
    assert_eq!(stdout, "foo\nalpha foo\ncharlie foo\nbravo foo\n");
}

#[test]
fn output_flag_writes_to_file_instead_of_stdout() {
    let input = TempFile::new("output-in", INPUT);
    let out_path = std::env::temp_dir().join(format!("fxt-test-{}-output-out", std::process::id()));

    let stdout = run_fxt_stdout(&[
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
    let output = run_fxt(&["-R", "1-2", "-o", input.path_str(), input.path_str()]);

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
    let link = std::env::temp_dir().join(format!("fxt-test-{}-output-link", std::process::id()));
    std::os::unix::fs::symlink(&input.path, &link).unwrap();

    let output = run_fxt(&["-o", link.to_str().unwrap(), input.path_str()]);
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

    let mut child = Command::new(env!("CARGO_BIN_EXE_fxt"))
        .arg(input.path_str())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn fxt binary");
    //close the reading end without consuming anything, like `head -0`
    drop(child.stdout.take());

    let output = child
        .wait_with_output()
        .expect("failed to wait for fxt binary");
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
    let stdout = run_fxt_stdout(&["-i", "-f", "foo", "-r", "BAR", input.path_str()]);

    //nothing goes to stdout; the file itself is updated
    assert_eq!(stdout, "");
    let rewritten = fs::read_to_string(input.path_str()).expect("input file vanished");
    assert_eq!(rewritten, "delta BAR\nalpha BAR\ncharlie BAR\nbravo BAR\n");
}

#[test]
fn backup_keeps_a_copy_of_the_original() {
    let input = TempFile::new("backup", "foo one\n");
    let backup = format!("{}.bak", input.path_str());

    run_fxt_stdout(&[
        "-i",
        "--backup",
        ".bak",
        "-f",
        "foo",
        "-r",
        "BAR",
        input.path_str(),
    ]);

    assert_eq!(
        fs::read_to_string(input.path_str()).expect("input file vanished"),
        "BAR one\n"
    );
    assert_eq!(
        fs::read_to_string(&backup).expect("backup file was not written"),
        "foo one\n"
    );
    let _ = fs::remove_file(&backup);
}

#[test]
fn backup_requires_in_place() {
    let input = TempFile::new("backup-alone", "a\n");
    let output = run_fxt(&["--backup", ".bak", input.path_str()]);
    assert!(!output.status.success());
}

#[test]
fn dry_run_reports_changes_without_writing() {
    let input = TempFile::new("dry-run", "foo one\n");

    let stdout = run_fxt_stdout(&[
        "-i",
        "--dry-run",
        "-f",
        "foo",
        "-r",
        "BAR",
        input.path_str(),
    ]);

    assert!(
        stdout.contains("would change"),
        "expected a change report, got {stdout:?}"
    );
    //the file itself is untouched
    assert_eq!(
        fs::read_to_string(input.path_str()).expect("input file vanished"),
        "foo one\n"
    );
}

#[test]
fn output_delimiter_rejoins_the_fields_inside_a_range() {
    let input = TempFile::new("output-delimiter-range", "a,b,c\n");
    for (cols, expected) in [
        ("1-3", "a;b;c\n"),
        ("1-", "a;b;c\n"),
        ("2-3", "b;c\n"),
        ("1-2,3", "a;b;c\n"),
    ] {
        assert_eq!(
            run_fxt_stdout(&[
                "-F",
                ",",
                "-C",
                cols,
                "--output-delimiter",
                ";",
                input.path_str()
            ]),
            expected,
            "{cols}"
        );
    }
    let quoted = TempFile::new("output-delimiter-quoted", "a,\"x,y\",c\n");
    assert_eq!(
        run_fxt_stdout(&[
            "-F",
            ",",
            "--quoted",
            "-C",
            "1-2",
            "--output-delimiter",
            ";",
            quoted.path_str()
        ]),
        "a;\"x,y\"\n"
    );
}

#[test]
fn field_mode_transforms_work_on_each_field() {
    let input = TempFile::new("per-field", " a , b , c \n");
    for (args, expected) in [
        (vec!["-C", "1-", "--trim"], "a,b,c\n"),
        (vec!["-C", "1,2,3", "--trim"], "a,b,c\n"),
        (vec!["-C", "2-3", "--trim"], " a ,b,c\n"),
        (vec!["-C", "1-", "--title-case"], " A , B , C \n"),
    ] {
        let mut full = vec!["-F", ","];
        full.extend(args.iter());
        full.push(input.path_str());
        assert_eq!(run_fxt_stdout(&full), expected, "{args:?}");
    }

    let words = TempFile::new("per-field-reverse", "ab,cd\na,a\n");
    assert_eq!(
        run_fxt_stdout(&["-F", ",", "-C", "1-", "--reverse-chars", words.path_str()]),
        "ba,dc\na,a\n"
    );
    //a replacement does not match across a delimiter either
    assert_eq!(
        run_fxt_stdout(&[
            "-F",
            ",",
            "-C",
            "1-",
            "-f",
            "a,a",
            "-r",
            "X",
            words.path_str()
        ]),
        "ab,cd\na,a\n"
    );

    //tab stops count from the start of each field
    let tabbed = TempFile::new("per-field-expand", "a\tb,\tc\n");
    assert_eq!(
        run_fxt_stdout(&["-F", ",", "-C", "1-", "--expand=4", tabbed.path_str()]),
        "a   b,    c\n"
    );
}

#[test]
fn a_terminator_put_in_follows_the_input_not_the_platform() {
    let stdout = |args: &[&str], input: &str| {
        let output = run_fxt_with_stdin(args, input);
        assert!(output.status.success(), "{args:?}");
        String::from_utf8(output.stdout).unwrap()
    };
    //an unterminated last line moved up gets the input's terminator
    assert_eq!(stdout(&["--tac"], "a\r\nb"), "b\r\na\r\n");
    assert_eq!(stdout(&["--tac"], "a\nb"), "b\na\n");
    assert_eq!(stdout(&["--wrap", "1"], "ab"), "a\nb");
    //a joined row ends the way the input does
    assert_eq!(stdout(&["--join", ","], "a\r\nb\r\n"), "a,b\r\n");
    assert_eq!(stdout(&["--join", ","], "a\nb\n"), "a,b\n");
    assert_eq!(stdout(&["--join", ","], "a\nb"), "a,b");
    //a summary row is terminated, in the input's style
    assert_eq!(stdout(&["--count"], "a\r\nb\r\n"), "2\r\n");
    assert_eq!(stdout(&["--count"], "a\nb"), "2\n");
}

#[test]
fn keep_others_edits_the_selection_and_keeps_the_rest() {
    let input = TempFile::new("keep-others", "h\nb\na\nb\n");
    //rows outside -R pass through, the selected ones are edited
    assert_eq!(
        run_fxt_stdout(&[
            "--keep-others",
            "-R",
            "2-3",
            "-f",
            "b",
            "-r",
            "X",
            input.path_str()
        ]),
        "h\nX\na\nb\n"
    );
    //rows --grep leaves out pass through too, like sed '/b/s/b/X/'
    assert_eq!(
        run_fxt_stdout(&["--keep-others", "-g", "b", "--upper", input.path_str()]),
        "h\nB\na\nB\n"
    );
    //a reordering sorts the selected run in place, the header stays first
    assert_eq!(
        run_fxt_stdout(&["--keep-others", "-s", "-R", "2-", input.path_str()]),
        "h\na\nb\nb\n"
    );
}

#[test]
fn keep_others_edits_in_place_without_losing_rows() {
    let input = TempFile::new("keep-others-in-place", "a\nb\nc\n");
    run_fxt_stdout(&[
        "-i",
        "--keep-others",
        "-R",
        "2",
        "-f",
        "b",
        "-r",
        "X",
        input.path_str(),
    ]);
    assert_eq!(fs::read_to_string(&input.path).unwrap(), "a\nX\nc\n");
}

#[test]
fn keep_others_conflicts_with_what_consumes_rows() {
    for args in [
        ["--keep-others", "-d"],
        ["--keep-others", "--count"],
        ["--keep-others", "--number"],
    ] {
        let output = run_fxt_with_stdin(&args, "a\n");
        assert_eq!(output.status.code(), Some(2), "{args:?}");
    }
}

#[test]
fn in_place_leaves_a_file_nothing_matched_in_as_it_was() {
    let input = TempFile::new("in-place-no-match", "a\nb\n");
    let output = run_fxt(&["-i", "-g", "zz", input.path_str()]);
    //exit 1, as for any filter that matched nothing, and the file intact
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(fs::read_to_string(&input.path).unwrap(), "a\nb\n");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("nothing matched, left unchanged"),
        "{stderr}"
    );
    //the file is named as it was given, not as a verbatim Windows path
    assert!(stderr.starts_with(input.path_str()), "{stderr}");

    let dry_run = run_fxt(&["-i", "--dry-run", "-g", "zz", input.path_str()]);
    let stdout = String::from_utf8_lossy(&dry_run.stdout);
    assert!(stdout.contains("unchanged (nothing matched)"), "{stdout}");
}

#[test]
fn dry_run_reports_an_unchanged_file() {
    let input = TempFile::new("dry-run-same", "nothing to find here\n");

    let stdout = run_fxt_stdout(&[
        "-i",
        "--dry-run",
        "-f",
        "foo",
        "-r",
        "BAR",
        input.path_str(),
    ]);
    assert!(
        stdout.contains("unchanged"),
        "expected an unchanged report, got {stdout:?}"
    );
}

#[test]
fn dry_run_reports_a_shorter_result_as_changed() {
    //the result is a prefix of the original: comparing byte by byte is
    //not enough, the leftover input has to count as a difference
    let input = TempFile::new("dry-run-shorter", "one\ntwo\n");

    let stdout = run_fxt_stdout(&["-i", "--dry-run", "-d", "-R", "2", input.path_str()]);
    assert!(
        stdout.contains("would change"),
        "expected a change report, got {stdout:?}"
    );
}

#[test]
fn dry_run_reports_every_file() {
    let first = TempFile::new("dry-run-multi-a", "foo\n");
    let second = TempFile::new("dry-run-multi-b", "bar\n");

    let stdout = run_fxt_stdout(&[
        "-i",
        "--dry-run",
        "-f",
        "foo",
        "-r",
        "BAZ",
        first.path_str(),
        second.path_str(),
    ]);

    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines.len(),
        2,
        "expected one report per file, got {stdout:?}"
    );
    assert!(lines[0].contains("would change"));
    assert!(lines[1].contains("unchanged"));
}

#[test]
fn several_files_are_read_as_one_stream() {
    let first = TempFile::new("multi-a", "a1\na2\n");
    let second = TempFile::new("multi-b", "b1\nb2\n");

    let stdout = run_fxt_stdout(&[first.path_str(), second.path_str()]);
    assert_eq!(stdout, "a1\na2\nb1\nb2\n");

    //row numbers address the concatenation, like `cat a b | fxt`
    let stdout = run_fxt_stdout(&["-R", "2-3", first.path_str(), second.path_str()]);
    assert_eq!(stdout, "a2\nb1\n");
}

#[test]
fn in_place_edits_each_file_on_its_own() {
    let first = TempFile::new("batch-a", "foo one\nfoo two\n");
    let second = TempFile::new("batch-b", "foo three\n");

    let stdout = run_fxt_stdout(&[
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
fn in_place_edits_a_file_named_twice_only_once() {
    //a second pass would reprocess the first's output ("xyy") and
    //overwrite the backup with the already-edited content
    let input = TempFile::new("batch-dup", "x\n");

    run_fxt_stdout(&[
        "-i",
        "-f",
        "x",
        "-r",
        "xy",
        input.path_str(),
        input.path_str(),
    ]);

    assert_eq!(
        fs::read_to_string(input.path_str()).expect("input file vanished"),
        "xy\n"
    );
}

#[test]
fn in_place_numbers_rows_per_file() {
    //row 1 of each file, not row 1 of the concatenation
    let first = TempFile::new("batch-rows-a", "a1\na2\n");
    let second = TempFile::new("batch-rows-b", "b1\nb2\n");

    run_fxt_stdout(&["-i", "-d", "-R", "1", first.path_str(), second.path_str()]);

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
    let output = run_fxt(&["-o", second.path_str(), first.path_str(), second.path_str()]);
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

    run_fxt_stdout(&["-i", "-f", "foo", "-r", "BAR", input.path_str()]);

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
    let dir = std::env::temp_dir().join(format!("fxt-test-{}-in-place-clean", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let input = dir.join("data.txt");
    fs::write(&input, INPUT).unwrap();

    run_fxt_stdout(&["-i", "-d", "-R", "1", input.to_str().unwrap()]);

    let entries: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name())
        .collect();
    let _ = fs::remove_dir_all(&dir);
    //only the rewritten input remains, no `.fxt-*.tmp` sibling
    assert_eq!(entries, [std::ffi::OsString::from("data.txt")]);
}

/// A directory of its own for a test that looks at every file in it.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> TempDir {
        let path = std::env::temp_dir().join(format!("fxt-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        TempDir { path }
    }

    fn write(&self, name: &str, content: impl AsRef<[u8]>) -> PathBuf {
        let path = self.path.join(name);
        fs::write(&path, content).unwrap();
        path
    }

    /// The names of the files in it, sorted.
    fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(&self.path)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn in_place_failing_part_way_leaves_the_file_and_nothing_else() {
    //row 3 is not UTF-8: the edit fails after rows 1 and 2 were written
    //to the temporary file
    let dir = TempDir::new("in-place-fails");
    let input = dir.write("data.txt", b"a\nb\n\xff\nc\n");
    let output = run_fxt(&["-i", "--backup", ".bak", "--upper", input.to_str().unwrap()]);

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("line 3 is not valid UTF-8"), "{stderr}");
    //the original as it was, no temporary file, and no backup either:
    //the backup is only taken right before the swap
    assert_eq!(fs::read(&input).unwrap(), b"a\nb\n\xff\nc\n");
    assert_eq!(dir.names(), ["data.txt"]);
}

#[test]
fn in_place_stops_at_the_first_file_that_fails() {
    //files are edited one by one: the one before the failure is done,
    //the failing one and those after it are left as they were
    let dir = TempDir::new("in-place-batch-fails");
    let first = dir.write("1.txt", "a\n");
    let broken = dir.write("2.txt", b"\xff\n");
    let last = dir.write("3.txt", "c\n");
    let output = run_fxt(&[
        "-i",
        "--upper",
        first.to_str().unwrap(),
        broken.to_str().unwrap(),
        last.to_str().unwrap(),
    ]);

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(fs::read_to_string(&first).unwrap(), "A\n");
    assert_eq!(fs::read(&broken).unwrap(), b"\xff\n");
    assert_eq!(fs::read_to_string(&last).unwrap(), "c\n");
    assert_eq!(dir.names(), ["1.txt", "2.txt", "3.txt"]);
}

#[cfg(windows)]
#[test]
fn in_place_refuses_a_read_only_file_on_windows() {
    //Windows will not rename over a read-only file: the edit fails
    //cleanly, the file intact and no temporary file left behind
    let dir = TempDir::new("in-place-read-only");
    let input = dir.write("data.txt", "a\n");
    let mut permissions = fs::metadata(&input)
        .unwrap()
        .permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&input, permissions.clone()).unwrap();

    let output = run_fxt(&["-i", "--upper", input.to_str().unwrap()]);
    let content = fs::read_to_string(&input).unwrap();
    let names = dir.names();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    fs::set_permissions(&input, permissions).unwrap();

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Cannot replace input file"), "{stderr}");
    assert_eq!(content, "a\n");
    assert_eq!(names, ["data.txt"]);
}

#[cfg(unix)]
#[test]
fn in_place_edits_a_read_only_file_and_keeps_it_read_only() {
    use std::os::unix::fs::PermissionsExt;

    //on Unix the directory decides whether a file may be replaced, so a
    //read-only file is edited like any other, and stays read-only
    let dir = TempDir::new("in-place-read-only");
    let input = dir.write("data.txt", "a\n");
    fs::set_permissions(&input, fs::Permissions::from_mode(0o444)).unwrap();

    run_fxt_stdout(&["-i", "--upper", input.to_str().unwrap()]);
    assert_eq!(fs::read_to_string(&input).unwrap(), "A\n");
    let mode = fs::metadata(&input)
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o444);
    assert_eq!(dir.names(), ["data.txt"]);
}

#[cfg(unix)]
#[test]
fn in_place_fails_cleanly_in_a_directory_it_cannot_write() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new("in-place-locked-dir");
    let input = dir.write("data.txt", "a\n");
    fs::set_permissions(&dir.path, fs::Permissions::from_mode(0o555)).unwrap();
    //root may write anyway, and then there is nothing to test
    let writable = fs::write(dir.path.join("probe"), "").is_ok();
    let output = run_fxt(&["-i", "--upper", input.to_str().unwrap()]);
    let content = fs::read_to_string(&input).unwrap();
    fs::set_permissions(&dir.path, fs::Permissions::from_mode(0o755)).unwrap();
    if writable {
        return;
    }

    //the temporary file cannot be created: nothing was touched
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(content, "a\n");
    assert_eq!(dir.names(), ["data.txt"]);
}

#[cfg(unix)]
#[test]
fn in_place_refuses_a_dangling_symlink() {
    let dir = TempDir::new("in-place-dangling");
    let link = dir.path.join("link.txt");
    std::os::unix::fs::symlink(dir.path.join("missing.txt"), &link).unwrap();

    let output = run_fxt(&["-i", "--upper", link.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(2));
    //no file appears where the link points
    assert_eq!(dir.names(), ["link.txt"]);
}

#[cfg(unix)]
#[test]
fn in_place_backs_up_the_symlink_target_next_to_it() {
    let targets = TempDir::new("in-place-backup-target");
    let links = TempDir::new("in-place-backup-link");
    let target = targets.write("data.txt", "a\n");
    let link = links.path.join("link.txt");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    run_fxt_stdout(&["-i", "--backup", ".bak", "--upper", link.to_str().unwrap()]);
    //the target is what is edited, so its backup lies beside it
    assert_eq!(targets.names(), ["data.txt", "data.txt.bak"]);
    assert_eq!(
        fs::read_to_string(targets.path.join("data.txt.bak")).unwrap(),
        "a\n"
    );
    assert_eq!(links.names(), ["link.txt"]);
    assert_eq!(fs::read_to_string(&link).unwrap(), "A\n");
}

#[cfg(unix)]
#[test]
fn in_place_edits_the_symlink_target_and_keeps_the_link() {
    let target = TempFile::new("in-place-link-target", INPUT);
    let link = std::env::temp_dir().join(format!("fxt-test-{}-in-place-link", std::process::id()));
    std::os::unix::fs::symlink(&target.path, &link).unwrap();

    let stdout = run_fxt_stdout(&["-i", "-f", "foo", "-r", "BAR", link.to_str().unwrap()]);
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
    let output = run_fxt_with_stdin(&["-i", "-f", "a", "-r", "b"], INPUT);
    assert!(!output.status.success());
}

#[test]
fn regex_replace_works() {
    let input = TempFile::new("regex-replace", "a1 bb22\nccc333 d\n");
    let stdout = run_fxt_stdout(&["-e", "-f", r"\d+", "-r", "N", input.path_str()]);
    assert_eq!(stdout, "aN bbN\ncccN d\n");
}

#[test]
fn regex_replace_supports_captures() {
    let input = TempFile::new("regex-captures", "user@host\n");
    let stdout = run_fxt_stdout(&["-e", "-f", r"(\w+)@(\w+)", "-r", "$2.$1", input.path_str()]);
    assert_eq!(stdout, "host.user\n");
}

#[test]
fn tac_reverses_row_order() {
    let input = TempFile::new("tac", INPUT);
    let stdout = run_fxt_stdout(&["--tac", input.path_str()]);
    assert_eq!(stdout, "bravo foo\ncharlie foo\nalpha foo\ndelta foo\n");
}

#[test]
fn shuffle_keeps_all_rows() {
    let input = TempFile::new("shuffle", INPUT);
    let stdout = run_fxt_stdout(&["--shuffle", input.path_str()]);
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
    let output = run_fxt(&["--tac", "-s", input.path_str()]);
    assert!(!output.status.success());
}

#[test]
fn delete_conflicts_with_reorder() {
    let input = TempFile::new("delete-reorder", INPUT);
    //deleting whole rows leaves nothing to sort
    let output = run_fxt(&["-d", "-s", "-R", "2-3", input.path_str()]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("reorder"),
        "stderr should explain the delete/reorder conflict"
    );
}

#[test]
fn unique_drops_duplicate_rows() {
    let input = TempFile::new("unique", "one\ntwo\none\nthree\n");
    let stdout = run_fxt_stdout(&["-u", input.path_str()]);
    assert_eq!(stdout, "one\ntwo\nthree\n");
}

#[test]
fn unique_with_sort_and_column_key() {
    let input = TempFile::new("unique-sort", "b 2\na 1\nb 9\n");
    let stdout = run_fxt_stdout(&["-s", "-u", "-C", "1-1", input.path_str()]);
    assert_eq!(stdout, "a 1\nb 2\n");
}

#[test]
fn unique_dedupes_empty_fields() {
    //"b," and "c," share the empty field 2 as their key
    let input = TempFile::new("unique-empty-field", "a,1\nb,\nc,\nd,1\n");
    let stdout = run_fxt_stdout(&["-u", "-F", ",", "-C", "2", input.path_str()]);
    assert_eq!(stdout, "a,1\nb,\n");
}

#[test]
fn grep_keeps_matching_rows() {
    let input = TempFile::new("grep", INPUT);
    let stdout = run_fxt_stdout(&["-g", "^(a|b)", input.path_str()]);
    assert_eq!(stdout, "alpha foo\nbravo foo\n");
}

#[test]
fn grep_invert_keeps_non_matching_rows() {
    let input = TempFile::new("grep-invert", INPUT);
    let stdout = run_fxt_stdout(&["-g", "^(a|b)", "--invert", input.path_str()]);
    assert_eq!(stdout, "delta foo\ncharlie foo\n");

    //-v, the short form every grep user reaches for
    let input = TempFile::new("grep-invert-short", INPUT);
    let stdout = run_fxt_stdout(&["-g", "^(a|b)", "-v", input.path_str()]);
    assert_eq!(stdout, "delta foo\ncharlie foo\n");
}

#[test]
fn help_explains_examples_and_exit_codes() {
    let stdout = run_fxt_stdout(&["--help"]);
    assert!(stdout.contains("Examples:"), "help should show examples");
    assert!(
        stdout.contains("Exit codes"),
        "help should explain the exit codes"
    );
}

#[test]
fn grep_with_delete_removes_matching_rows() {
    let input = TempFile::new("grep-delete", INPUT);
    let stdout = run_fxt_stdout(&["-d", "-g", "charlie", input.path_str()]);
    assert_eq!(stdout, "delta foo\nalpha foo\nbravo foo\n");
}

#[test]
fn upper_transforms_column_range() {
    let input = TempFile::new("upper", INPUT);
    let stdout = run_fxt_stdout(&["--upper", "-C", "1-3", input.path_str()]);
    assert_eq!(stdout, "DELta foo\nALPha foo\nCHArlie foo\nBRAvo foo\n");
}

#[test]
fn lower_transforms_whole_line_without_columns() {
    let input = TempFile::new("lower", "AbC dEf\n");
    let stdout = run_fxt_stdout(&["--lower", input.path_str()]);
    assert_eq!(stdout, "abc def\n");
}

#[test]
fn trim_removes_surrounding_whitespace() {
    let input = TempFile::new("trim", "  a  \n\tb\t\n");
    let stdout = run_fxt_stdout(&["--trim", input.path_str()]);
    assert_eq!(stdout, "a\nb\n");
}

#[test]
fn upper_conflicts_with_lower() {
    let input = TempFile::new("upper-lower", INPUT);
    let output = run_fxt(&["--upper", "--lower", input.path_str()]);
    assert!(!output.status.success());
}

#[test]
fn ignore_case_replaces_literal_in_any_case() {
    let input = TempFile::new("ignore-case", "Foo bar FOO\n");
    let stdout = run_fxt_stdout(&["--ignore-case", "-f", "foo", "-r", "X", input.path_str()]);
    assert_eq!(stdout, "X bar X\n");
}

#[test]
fn ignore_case_applies_to_regex_patterns() {
    let input = TempFile::new("ignore-case-regex", "abc DEF\n");
    let stdout = run_fxt_stdout(&[
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
    let output = run_fxt(&["-e", "-f", "[unclosed", "-r", "N", input.path_str()]);
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
    let output = run_fxt(&["-r", "BAR", input.path_str()]);
    assert!(!output.status.success());
}

#[test]
fn replace_with_delete_is_rejected() {
    let input = TempFile::new("replace-with-delete", INPUT);
    let output = run_fxt(&["-d", "-f", "foo", "-r", "BAR", input.path_str()]);
    assert!(!output.status.success());
}

#[test]
fn reads_stdin_when_no_filename_given() {
    let output = run_fxt_with_stdin(&["-s"], INPUT);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "alpha foo\nbravo foo\ncharlie foo\ndelta foo\n"
    );
}

#[test]
fn dash_filename_reads_stdin() {
    let output = run_fxt_with_stdin(&["-f", "foo", "-r", "BAR", "-"], "one foo\n");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "one BAR\n");
}

#[test]
fn delete_without_any_range_is_rejected() {
    let input = TempFile::new("delete-no-range", INPUT);
    let output = run_fxt(&["-d", input.path_str()]);
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
    let output = run_fxt(&["-R", "5-2", input.path_str()]);
    assert!(!output.status.success());
}

#[test]
fn zero_based_range_is_rejected() {
    let input = TempFile::new("zero-range", INPUT);
    let output = run_fxt(&["-C", "0-5", input.path_str()]);
    assert!(!output.status.success());
}

#[test]
fn missing_input_file_fails_with_message_on_stderr() {
    let output = run_fxt(&["/nonexistent/fxt-test-missing.txt"]);
    assert!(!output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Cannot open input file"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn quoted_csv_numbers_count_in_a_summary() {
    let input = TempFile::new("csv-quoted-sum", "a,\"10\"\na,\"20\"\nb,5\n");
    let stdout = run_fxt_stdout(&[
        "-F",
        ",",
        "--quoted",
        "--group-by",
        "1",
        "--sum",
        "2",
        input.path_str(),
    ]);
    assert_eq!(stdout, "a,30\nb,5\n");
}

#[test]
fn quoted_csv_text_sorts_by_value() {
    //the quote is not part of the value, so `"b"` sorts between a and c;
    //the rows themselves come out as they were written
    let input = TempFile::new("csv-quoted-text-sort", "\"b\",1\nc,2\na,3\n");
    let args = [
        "-F",
        ",",
        "--quoted",
        "-s",
        "--sort-key",
        "1",
        input.path_str(),
    ];
    assert_eq!(run_fxt_stdout(&args), "a,3\n\"b\",1\nc,2\n");

    //without --quoted a quote is just a character, and sorts first
    let stdout = run_fxt_stdout(&["-F", ",", "-s", "--sort-key", "1", input.path_str()]);
    assert_eq!(stdout, "\"b\",1\na,3\nc,2\n");
}

#[test]
fn quoted_csv_numbers_sort_numerically() {
    let input = TempFile::new("csv-quoted-sort", "x,\"10\"\ny,\"9\"\n");
    let stdout = run_fxt_stdout(&[
        "-F",
        ",",
        "--quoted",
        "-s",
        "-n",
        "--sort-key",
        "2",
        input.path_str(),
    ]);
    assert_eq!(stdout, "y,\"9\"\nx,\"10\"\n");
}

/// A CSV file whose second record holds a newline inside a quoted field.
const MULTILINE_CSV: &str = "id,note,amount\n1,\"hello\nworld\",20\n2,\"fine\",10\n";

#[test]
fn quoted_csv_reads_a_record_spanning_lines() {
    //the newline inside quotes belongs to the field, so it comes out with it
    let input = TempFile::new("csv-multiline", MULTILINE_CSV);
    let stdout = run_fxt_stdout(&["-F", ",", "--quoted", "-C", "2", input.path_str()]);
    assert_eq!(stdout, "note\n\"hello\nworld\"\n\"fine\"\n");
}

#[test]
fn quoted_csv_counts_records_not_lines() {
    //row 3 is the third record, which starts on line 4
    let input = TempFile::new("csv-multiline-rows", MULTILINE_CSV);
    let stdout = run_fxt_stdout(&[
        "-F",
        ",",
        "--quoted",
        "-C",
        "1-",
        "-R",
        "3",
        input.path_str(),
    ]);
    assert_eq!(stdout, "2,\"fine\",10\n");

    let stdout = run_fxt_stdout(&[
        "-F",
        ",",
        "--quoted",
        "-C",
        "1-",
        "-R",
        "2-",
        "--number",
        input.path_str(),
    ]);
    //in field mode the number is a field of its own
    assert_eq!(stdout, "1,1,\"hello\nworld\",20\n2,2,\"fine\",10\n");

    let stdout = run_fxt_stdout(&[
        "-F",
        ",",
        "--quoted",
        "-C",
        "1-",
        "--count",
        input.path_str(),
    ]);
    assert_eq!(stdout, "3\n");
}

#[test]
fn quoted_csv_sorts_and_sums_multiline_records_whole() {
    let input = TempFile::new("csv-multiline-sort", MULTILINE_CSV);
    let stdout = run_fxt_stdout(&[
        "-F",
        ",",
        "--quoted",
        "-C",
        "1-",
        "-R",
        "2-",
        "-s",
        "-n",
        "--sort-key",
        "3",
        input.path_str(),
    ]);
    assert_eq!(stdout, "2,\"fine\",10\n1,\"hello\nworld\",20\n");

    let stdout = run_fxt_stdout(&[
        "-F",
        ",",
        "--quoted",
        "-C",
        "1-",
        "-R",
        "2-",
        "--sum",
        "3",
        input.path_str(),
    ]);
    assert_eq!(stdout, "30\n");
}

#[test]
fn quoted_csv_end_relative_rows_count_records() {
    let input = TempFile::new("csv-multiline-tail", MULTILINE_CSV);
    let stdout = run_fxt_stdout(&[
        "-F",
        ",",
        "--quoted",
        "-C",
        "1-",
        "-R",
        "~2",
        input.path_str(),
    ]);
    assert_eq!(stdout, "1,\"hello\nworld\",20\n");
}

#[test]
fn quoted_csv_rejects_a_quote_never_closed() {
    //reading to the end of the input as one field would answer wrongly
    let input = TempFile::new("csv-unclosed", "1,x\n2,\"open\nstill open\n");
    let output = run_fxt(&["-F", ",", "--quoted", "-C", "2", input.path_str()]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("the quoted field opened on line 2 is never closed"),
        "{stderr}"
    );
}

#[test]
fn without_quoted_a_quote_does_not_join_lines() {
    let input = TempFile::new("csv-plain-split", MULTILINE_CSV);
    let stdout = run_fxt_stdout(&["-F", ",", "-C", "1-", "-R", "3", input.path_str()]);
    assert_eq!(stdout, "world\",20\n");
}

#[test]
fn numeric_sort_is_exact_beyond_f64_precision() {
    //2^53 and its neighbours, which an f64 key cannot tell apart
    let input = TempFile::new(
        "bigints",
        "9007199254740993\n9007199254740992\n9007199254740995\n9007199254740994\n",
    );
    let stdout = run_fxt_stdout(&["-s", "-n", input.path_str()]);
    assert_eq!(
        stdout,
        "9007199254740992\n9007199254740993\n9007199254740994\n9007199254740995\n"
    );
}

#[test]
fn integer_sums_stay_exact() {
    let input = TempFile::new("bigint-sum", "x 9007199254740993\nx 1\n");
    let stdout = run_fxt_stdout(&["-F", " ", "--sum", "2", input.path_str()]);
    assert_eq!(stdout, "9007199254740994\n");
}

#[test]
fn grep_key_filters_on_one_field_and_cols_selects_another() {
    //the grep key addresses the line as read, so field 1 is matched
    //while field 2 is what comes out
    let input = TempFile::new("grep-key", "ERROR,disk full\nINFO,ERROR seen\n");
    let stdout = run_fxt_stdout(&[
        "-F",
        ",",
        "-C",
        "2",
        "--grep-key",
        "1",
        "-g",
        "ERROR",
        input.path_str(),
    ]);
    assert_eq!(stdout, "disk full\n");
}

#[test]
fn a_bom_does_not_hide_the_first_field() {
    //the mark is encoding, not content: field 1 of line 1 is `a`, and the
    //output carries no BOM
    let input = TempFile::new("bom-grep", "\u{feff}a,1\nb,2\n");
    let stdout = run_fxt_stdout(&["-F", ",", "-C", "1", "-g", "^a$", input.path_str()]);
    assert_eq!(stdout, "a,1\n");
}

#[test]
fn every_concatenated_input_loses_its_bom() {
    //a second file's mark would otherwise land mid-stream
    let first = TempFile::new("bom-cat-1", "\u{feff}1\n");
    let second = TempFile::new("bom-cat-2", "\u{feff}2\n");
    let stdout = run_fxt_stdout(&["--sum", "1", first.path_str(), second.path_str()]);
    assert_eq!(stdout, "3\n");
}

#[test]
fn a_bom_on_stdin_is_taken_off() {
    let output = run_fxt_with_stdin(&["-C", "1"], "\u{feff}xy\n");
    assert_eq!(String::from_utf8_lossy(&output.stdout), "x\n");
}

#[test]
fn in_place_keeps_the_bom() {
    let input = TempFile::new("bom-in-place", "\u{feff}a\n");
    let output = run_fxt(&["-i", "--upper", input.path_str()]);
    assert!(output.status.success());
    assert_eq!(fs::read_to_string(&input.path).unwrap(), "\u{feff}A\n");
}

#[test]
fn dry_run_does_not_count_the_bom_as_a_change() {
    let input = TempFile::new("bom-dry-run", "\u{feff}nothing here\n");
    let stdout = run_fxt_stdout(&[
        "-i",
        "--dry-run",
        "-f",
        "foo",
        "-r",
        "BAR",
        input.path_str(),
    ]);
    assert!(stdout.contains("unchanged"), "{stdout:?}");
}

/// "Łódź;żółw\nZażółć;gęślą\n" in windows-1250.
const CP1250: &[u8] = b"\xA3\xF3d\x9F;\xBF\xF3\xB3w\nZa\xBF\xF3\xB3\xE6;g\xEA\x9Cl\xB9\n";

/// `text` in UTF-16LE, with its byte order mark.
fn utf16le_with_bom(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend(
        text.encode_utf16()
            .flat_map(u16::to_le_bytes),
    );
    bytes
}

#[test]
fn a_declared_encoding_is_decoded_into_utf8() {
    let input = TempFile::new("cp1250", CP1250);
    let stdout = run_fxt_stdout(&[
        "--encoding",
        "windows-1250",
        "-F",
        ";",
        "-C",
        "2",
        input.path_str(),
    ]);
    assert_eq!(stdout, "żółw\ngęślą\n");
}

#[test]
fn an_undeclared_legacy_encoding_still_fails_as_utf8() {
    let input = TempFile::new("cp1250-undeclared", CP1250);
    let output = run_fxt(&[input.path_str()]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not valid UTF-8"), "{stderr}");
}

#[test]
fn utf16_is_recognized_by_its_bom() {
    let input = TempFile::new("utf16", utf16le_with_bom("b\na\nc\n"));
    assert_eq!(run_fxt_stdout(&["-s", input.path_str()]), "a\nb\nc\n");
}

#[test]
fn in_place_keeps_the_file_encoding() {
    //a legacy encoding, declared
    let legacy = TempFile::new("cp1250-in-place", CP1250);
    run_fxt_stdout(&["-i", "--encoding", "cp1250", "--upper", legacy.path_str()]);
    let expected = b"\xA3\xD3D\x8F;\xAF\xD3\xA3W\nZA\xAF\xD3\xA3\xC6;G\xCA\x8CL\xA5\n";
    assert_eq!(fs::read(&legacy.path).unwrap(), expected);

    //UTF-16, found by its mark, which is written back
    let utf16 = TempFile::new("utf16-in-place", utf16le_with_bom("b\na\n"));
    run_fxt_stdout(&["-i", "-s", utf16.path_str()]);
    assert_eq!(fs::read(&utf16.path).unwrap(), utf16le_with_bom("a\nb\n"));
}

#[test]
fn in_place_with_an_output_encoding_converts_the_file() {
    let input = TempFile::new("cp1250-convert", CP1250);
    run_fxt_stdout(&[
        "-i",
        "--encoding",
        "windows-1250",
        "--output-encoding",
        "utf-8",
        "-R",
        "1-",
        input.path_str(),
    ]);
    assert_eq!(
        fs::read_to_string(&input.path).unwrap(),
        "Łódź;żółw\nZażółć;gęślą\n"
    );
}

#[test]
fn invalid_utf8_fails_and_points_at_lossy() {
    let input = TempFile::new("invalid-utf8", b"ok\nbad\xFF\n");
    let output = run_fxt(&["--upper", input.path_str()]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("line 2 is not valid UTF-8"), "{stderr}");
    assert!(stderr.contains("--lossy"), "{stderr}");
}

#[test]
fn lossy_replaces_invalid_bytes_in_the_rows_it_rewrites() {
    let input = TempFile::new("lossy-rewrite", b"ok\nbad\xFF\xFEend\n");
    let output = run_fxt(&["--lossy", "--upper", input.path_str()]);
    assert!(output.status.success());
    assert_eq!(output.stdout, "OK\nBAD\u{FFFD}\u{FFFD}END\n".as_bytes());
}

#[test]
fn lossy_leaves_the_rows_it_does_not_change_as_they_were() {
    //read: `--grep` matches the repaired text, the row goes out unrepaired,
    //like `grep -a`
    let input = TempFile::new("lossy-keep", b"ok\nbad\xFF\n");
    let output = run_fxt(&["--lossy", "-g", "bad", input.path_str()]);
    assert_eq!(output.stdout, b"bad\xFF\n");

    //a replacement that misses the row leaves the file unchanged...
    let dry_run = run_fxt(&[
        "--lossy",
        "-i",
        "--dry-run",
        "-f",
        "zz",
        "-r",
        "y",
        input.path_str(),
    ]);
    let stdout = String::from_utf8_lossy(&dry_run.stdout);
    assert!(stdout.contains("unchanged"), "{stdout}");

    //...and one that hits another row edits only that row
    run_fxt_stdout(&["--lossy", "-i", "-f", "ok", "-r", "OK", input.path_str()]);
    assert_eq!(fs::read(&input.path).unwrap(), b"OK\nbad\xFF\n");
}

#[test]
fn lossy_decodes_other_encodings_with_replacement() {
    //an unpaired surrogate in UTF-16LE, found by its BOM
    let input = TempFile::new("lossy-utf16", b"\xFF\xFEa\x00\x00\xD8b\x00\n\x00");
    let output = run_fxt(&[input.path_str()]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        run_fxt_stdout(&["--lossy", input.path_str()]),
        "a\u{FFFD}b\n"
    );
}

#[test]
fn output_is_encoded_as_asked() {
    let input = TempFile::new("to-cp1250", "Łódź\n");
    let output = run_fxt(&["--output-encoding", "windows-1250", input.path_str()]);
    assert!(output.status.success());
    assert_eq!(output.stdout, b"\xA3\xF3d\x9F\n");
}

#[test]
fn a_character_the_output_encoding_lacks_fails() {
    let input = TempFile::new("unmappable", "ok\n日本\n");
    let output = run_fxt(&["--output-encoding", "windows-1250", input.path_str()]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("'日' (U+65E5) cannot be written in windows-1250"),
        "{stderr}"
    );
}

#[test]
fn a_failed_conversion_leaves_the_file_alone() {
    let input = TempFile::new("unmappable-in-place", "ok\n日本\n");
    let output = run_fxt(&["-i", "--output-encoding", "latin1", input.path_str()]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(fs::read_to_string(&input.path).unwrap(), "ok\n日本\n");
}

#[test]
fn dry_run_compares_in_the_file_encoding() {
    let input = TempFile::new("cp1250-dry-run", CP1250);
    let stdout = run_fxt_stdout(&[
        "-i",
        "--dry-run",
        "--encoding",
        "windows-1250",
        "-f",
        "nothing",
        "-r",
        "x",
        input.path_str(),
    ]);
    assert!(stdout.contains("unchanged"), "{stdout:?}");
}

#[test]
fn unknown_encodings_are_rejected() {
    let output = run_fxt(&["--encoding", "klingon", "-"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Unknown encoding `klingon`"));

    let output = run_fxt(&["--output-encoding", "replacement", "-"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot be written"));
}

#[test]
fn completions_are_generated_per_shell() {
    let stdout = run_fxt_stdout(&["--completions", "bash"]);
    assert!(stdout.contains("fxt"), "{stdout}");
    assert!(stdout.contains("--grep-key"), "{stdout}");

    let stdout = run_fxt_stdout(&["--completions", "powershell"]);
    assert!(stdout.contains("Register-ArgumentCompleter"), "{stdout}");
}

#[test]
fn man_page_is_roff() {
    let stdout = run_fxt_stdout(&["--man"]);
    assert!(stdout.contains(".TH fxt 1"), "{stdout}");
    assert!(stdout.contains("Examples:"), "{stdout}");
}

#[test]
fn completions_and_man_stand_alone() {
    //they describe the tool rather than run it, so nothing combines
    let output = run_fxt(&["--man", "-C", "1"]);
    assert_eq!(output.status.code(), Some(2));
    let output = run_fxt(&["--completions", "bash", "input.txt"]);
    assert_eq!(output.status.code(), Some(2));
}

const SECTIONS: &str = "intro\n[one]\na=1\n[two]\nb=2\nb=0\n[three]\nc=3\n";

#[test]
fn from_to_selects_a_block_like_sed() {
    //inclusive at both ends; the end is looked for after the start line,
    //so the next header closes the block (and belongs to it)
    let input = TempFile::new("from-to", SECTIONS);
    let stdout = run_fxt_stdout(&["--from", r"^\[two", "--to", r"^\[", input.path_str()]);
    assert_eq!(stdout, "[two]\nb=2\nb=0\n[three]\n");
}

#[test]
fn from_to_selects_every_block() {
    let input = TempFile::new("from-to-blocks", "x\nBEGIN\n1\nEND\ny\nBEGIN\n2\nEND\nz\n");
    let stdout = run_fxt_stdout(&["--from", "BEGIN", "--to", "END", input.path_str()]);
    assert_eq!(stdout, "BEGIN\n1\nEND\nBEGIN\n2\nEND\n");
}

#[test]
fn from_alone_runs_to_the_end_and_to_alone_from_the_start() {
    let input = TempFile::new("from-to-open", SECTIONS);
    let stdout = run_fxt_stdout(&["--from", r"^\[three", input.path_str()]);
    assert_eq!(stdout, "[three]\nc=3\n");
    let stdout = run_fxt_stdout(&["--to", r"^\[", input.path_str()]);
    assert_eq!(stdout, "intro\n[one]\n");
}

#[test]
fn delete_removes_the_block() {
    let input = TempFile::new("from-to-delete", SECTIONS);
    let stdout = run_fxt_stdout(&["-d", "--from", r"^\[one", "--to", "a=", input.path_str()]);
    assert_eq!(stdout, "intro\n[two]\nb=2\nb=0\n[three]\nc=3\n");
}

#[test]
fn from_to_combines_with_rows_grep_and_sort() {
    let input = TempFile::new("from-to-combined", SECTIONS);
    //rows 5-6 that are also in the block
    let stdout = run_fxt_stdout(&[
        "-R",
        "5-",
        "--from",
        r"^\[two",
        "--to",
        "b=0",
        input.path_str(),
    ]);
    assert_eq!(stdout, "b=2\nb=0\n");
    //only the block's rows are filtered, and sorted in place
    let stdout = run_fxt_stdout(&[
        "--from",
        r"^\[two",
        "--to",
        "b=0",
        "-g",
        "=",
        "-s",
        input.path_str(),
    ]);
    assert_eq!(stdout, "b=0\nb=2\n");
}

#[test]
fn from_to_works_with_end_relative_rows() {
    //~N buffers the input; the pattern range must still see every line
    let input = TempFile::new("from-to-tail", SECTIONS);
    let stdout = run_fxt_stdout(&["-R", "~3-", "--from", r"^\[two", input.path_str()]);
    assert_eq!(stdout, "b=0\n[three]\nc=3\n");
}

#[test]
fn from_honours_ignore_case() {
    let input = TempFile::new("from-ignore-case", "a\nstart\nb\n");
    let stdout = run_fxt_stdout(&["--ignore-case", "--from", "START", input.path_str()]);
    assert_eq!(stdout, "start\nb\n");
}

#[test]
fn from_that_never_matches_exits_one_and_quiet_asks_it() {
    let input = TempFile::new("from-none", SECTIONS);
    let output = run_fxt(&["--from", "NOWHERE", input.path_str()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());

    let output = run_fxt(&["-q", "--from", r"^\[two", input.path_str()]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
}

#[test]
fn rows_outside_the_block_need_not_be_utf8() {
    //matched as bytes: a stray byte outside the block passes through
    let input = TempFile::new("from-bytes", "");
    fs::write(&input.path, b"bad \xff\nSTART\nok\n").unwrap();
    let output = run_fxt(&["-d", "--from", "START", input.path_str()]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"bad \xff\n");
}

#[test]
fn from_to_matches_lines_without_their_crlf() {
    let input = TempFile::new("from-crlf", "a\r\nSTART\r\nb\r\nEND\r\nc\r\n");
    let stdout = run_fxt_stdout(&["--from", "^START$", "--to", "^END$", input.path_str()]);
    assert_eq!(stdout, "START\r\nb\r\nEND\r\n");
}

/// A decomposed accent (`e` + U+0301) and a ZWJ emoji sequence.
const CLUSTERS: &str = "e\u{301}tude\n\u{1F469}\u{200D}\u{1F4BB}xy\n";

#[test]
fn graphemes_count_user_perceived_characters() {
    let input = TempFile::new("graphemes", CLUSTERS);
    //by code point, the first column tears each cluster apart
    assert_eq!(
        run_fxt_stdout(&["-C", "1", input.path_str()]),
        "e\n\u{1F469}\n"
    );
    assert_eq!(
        run_fxt_stdout(&["--graphemes", "-C", "1", input.path_str()]),
        "e\u{301}\n\u{1F469}\u{200D}\u{1F4BB}\n"
    );
    assert_eq!(
        run_fxt_stdout(&["--graphemes", "-d", "-C", "1", input.path_str()]),
        "tude\nxy\n"
    );
}

#[test]
fn graphemes_wrap_without_splitting_a_cluster() {
    let input = TempFile::new("graphemes-wrap", CLUSTERS);
    assert_eq!(
        run_fxt_stdout(&["--graphemes", "--wrap", "2", input.path_str()]),
        "e\u{301}t\nud\ne\n\u{1F469}\u{200D}\u{1F4BB}x\ny\n"
    );
}

#[test]
fn perl_regexp_allows_backreferences_in_grep() {
    let input = TempFile::new("pcre-grep", "book\nbok\nkeep\nkep\n");
    let stdout = run_fxt_stdout(&["-P", "-g", r"(.)\1", input.path_str()]);
    assert_eq!(stdout, "book\nkeep\n");

    //the default engine rejects them, and says so
    let output = run_fxt(&["-g", r"(.)\1", input.path_str()]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("backreferences are not supported"));
}

#[test]
fn perl_regexp_replaces_with_lookaround_and_groups() {
    let input = TempFile::new("pcre-replace", "$15 and 15% and $7\nthe the end\n");
    let stdout = run_fxt_stdout(&[
        "-P",
        "-e",
        "-f",
        r"(?<=\$)\d+",
        "-r",
        "N",
        "-f",
        r"\b(\w+) \1\b",
        "-r",
        "$1",
        input.path_str(),
    ]);
    assert_eq!(stdout, "$N and 15% and $N\nthe end\n");
}

#[test]
fn perl_regexp_applies_to_from_and_to() {
    let input = TempFile::new("pcre-from-to", "a\n[x]\nb\n[x]\nc\n");
    //a block from a line to the next line that repeats it
    let stdout = run_fxt_stdout(&[
        "-P",
        "--from",
        r"^\[(?!y)",
        "--to",
        r"^\[x\]$",
        input.path_str(),
    ]);
    assert_eq!(stdout, "[x]\nb\n[x]\n");
}

#[test]
fn perl_regexp_that_backtracks_out_fails_the_run() {
    let input = TempFile::new("pcre-runaway", format!("{}b\n", "a".repeat(40)));
    let output = run_fxt(&["-P", "-g", r"^(a|aa)+\1$", input.path_str()]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("matching the pattern"), "{stderr}");
}
