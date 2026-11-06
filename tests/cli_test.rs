//! Tests that drive the `gdstyle` binary itself.
//!
//! Exit codes and the messages printed alongside them live in `main.rs`, so
//! they can't be reached from the library tests in `integration_test.rs`.
//! Every test passes `--config` explicitly rather than relying on the
//! upward config search, so results don't depend on what sits above the
//! temporary directory.

use std::path::Path;
use std::process::{Command, Output};

/// A script with exactly two naming violations: one variable, one function.
const TWO_VIOLATIONS: &str =
    "extends Node\n\nvar BadName: int = 5\n\nfunc DoThing() -> void:\n\tpass\n";

struct Project {
    _dir: tempfile::TempDir,
    config: std::path::PathBuf,
    script: std::path::PathBuf,
}

/// Write a config and a script into a fresh temporary directory.
fn project(config_body: &str, source: &str) -> Project {
    let dir = tempfile::tempdir().expect("temp dir");
    let config = dir.path().join("gdstyle.toml");
    let script = dir.path().join("bad_test.gd");
    std::fs::write(&config, config_body).expect("write config");
    std::fs::write(&script, source).expect("write script");
    Project {
        _dir: dir,
        config,
        script,
    }
}

fn check(config: &Path, script: &Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_gdstyle"))
        .arg("check")
        .arg(script)
        .arg("--config")
        .arg(config)
        .arg("--no-color")
        .args(extra)
        .output()
        .expect("run gdstyle")
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn error_severity_makes_check_exit_1() {
    // The reported bug end to end: rules set to "error" printed as
    // warnings and `check` exited 0, so CI stayed green.
    let p = project(
        "[rules]\n\
         \"naming/variable-name-snake-case\" = \"error\"\n\
         \"naming/function-name-snake-case\" = \"error\"\n",
        TWO_VIOLATIONS,
    );
    let output = check(&p.config, &p.script, &[]);
    let stdout = stdout_of(&output);

    assert_eq!(output.status.code(), Some(1), "stdout:\n{}", stdout);
    assert!(
        stdout.contains("2 errors found"),
        "summary should count errors; got:\n{}",
        stdout
    );
    assert!(
        !stdout.contains("warning"),
        "no diagnostic should print as a warning; got:\n{}",
        stdout
    );
}

#[test]
fn warnings_alone_still_exit_0() {
    // The documented default: warnings report but don't fail a build.
    let p = project("[rules]\n", TWO_VIOLATIONS);
    let output = check(&p.config, &p.script, &[]);
    let stdout = stdout_of(&output);

    assert_eq!(output.status.code(), Some(0), "stdout:\n{}", stdout);
    assert!(stdout.contains("2 warnings found"), "got:\n{}", stdout);
}

#[test]
fn max_warnings_fails_when_the_count_exceeds_the_limit() {
    let p = project("[rules]\n", TWO_VIOLATIONS);
    let output = check(&p.config, &p.script, &["--max-warnings", "1"]);

    assert_eq!(
        output.status.code(),
        Some(1),
        "stdout:\n{}",
        stdout_of(&output)
    );
    assert!(
        stderr_of(&output).contains("2 warnings found, exceeding the --max-warnings limit of 1"),
        "should explain why it failed; got stderr:\n{}",
        stderr_of(&output)
    );
}

#[test]
fn max_warnings_passes_when_the_count_equals_the_limit() {
    // Boundary: the limit is a maximum, so N warnings under `--max-warnings N`
    // is a pass. Only N+1 fails.
    let p = project("[rules]\n", TWO_VIOLATIONS);
    let output = check(&p.config, &p.script, &["--max-warnings", "2"]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{}",
        stdout_of(&output)
    );
}

#[test]
fn max_warnings_ignores_diagnostics_escalated_to_error() {
    // Errors already fail on their own, and they must not also be counted
    // against the warning budget: `--max-warnings 0` with one error and no
    // warnings should report the error, not a bogus warning overflow.
    let p = project(
        "[rules]\n\"naming/variable-name-snake-case\" = \"error\"\n\
         \"naming/function-name-snake-case\" = \"off\"\n",
        TWO_VIOLATIONS,
    );
    let output = check(&p.config, &p.script, &["--max-warnings", "0"]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        !stderr_of(&output).contains("--max-warnings"),
        "an error should fail before the warning budget is consulted; got stderr:\n{}",
        stderr_of(&output)
    );
}

#[test]
fn max_warnings_keeps_json_stdout_parsable() {
    // The failure note goes to stderr so `--format json` stays machine-readable.
    let p = project("[rules]\n", TWO_VIOLATIONS);
    let output = Command::new(env!("CARGO_BIN_EXE_gdstyle"))
        .arg("check")
        .arg(&p.script)
        .arg("--config")
        .arg(&p.config)
        .arg("--format")
        .arg("json")
        .arg("--max-warnings")
        .arg("0")
        .output()
        .expect("run gdstyle");

    assert_eq!(output.status.code(), Some(1));
    let stdout = stdout_of(&output);
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout should be valid JSON ({}); got:\n{}", e, stdout));
    assert_eq!(parsed.as_array().map(Vec::len), Some(2));
}

#[test]
fn unknown_rule_names_are_reported() {
    // A misspelled rule name is valid TOML and was silently ignored, so
    // the config looked like it took effect when it did nothing.
    let p = project(
        "[rules]\n\
         \"naming/variable-snake-case\" = \"error\"\n\
         \"format/tabs\" = \"off\"\n",
        TWO_VIOLATIONS,
    );
    let output = check(&p.config, &p.script, &[]);
    let stderr = stderr_of(&output);

    assert!(
        stderr.contains("unknown rule names in [rules]: format/tabs, naming/variable-snake-case"),
        "both typos should be listed, sorted; got stderr:\n{}",
        stderr
    );
    assert!(
        stderr.contains("gdstyle rules"),
        "should point at the rule listing; got stderr:\n{}",
        stderr
    );
}

#[test]
fn a_correct_config_reports_no_unknown_rules() {
    let p = project(
        "[rules]\n\"naming/variable-name-snake-case\" = \"error\"\n",
        TWO_VIOLATIONS,
    );
    let output = check(&p.config, &p.script, &[]);

    assert!(
        !stderr_of(&output).contains("unknown rule"),
        "a valid config must stay quiet; got stderr:\n{}",
        stderr_of(&output)
    );
}

#[test]
fn fmt_also_reports_unknown_rule_names() {
    // `fmt` applies safe lint fixes, so a typo changes what gets formatted
    // just as it changes what gets reported.
    let p = project("[rules]\n\"format/tabs\" = \"off\"\n", TWO_VIOLATIONS);
    let output = Command::new(env!("CARGO_BIN_EXE_gdstyle"))
        .arg("fmt")
        .arg(&p.script)
        .arg("--config")
        .arg(&p.config)
        .arg("--no-color")
        .output()
        .expect("run gdstyle");

    assert!(
        stderr_of(&output).contains("unknown rule name in [rules]: format/tabs"),
        "a single typo should be singular; got stderr:\n{}",
        stderr_of(&output)
    );
}

#[test]
fn fmt_check_counts_only_the_files_that_would_change() {
    let p = project("", "extends Node\n\nvar x := 'a'\n");
    let formatted = p.script.with_file_name("formatted.gd");
    std::fs::write(&formatted, "extends Node\n\nvar x := \"a\"\n").expect("write script");
    let output = Command::new(env!("CARGO_BIN_EXE_gdstyle"))
        .arg("fmt")
        .arg("--check")
        .arg(&p.script)
        .arg(&formatted)
        .arg("--config")
        .arg(&p.config)
        .arg("--no-color")
        .output()
        .expect("run gdstyle");

    let stdout = stdout_of(&output);
    assert!(
        stdout.contains("1 file would be reformatted."),
        "two files checked, one needs changes; got stdout:\n{stdout}"
    );
}

#[test]
fn fmt_diff_prints_unified_hunks() {
    // Issue #54: the diff compared line i with line i, so a moved member
    // printed as unrelated -/+ pairs with no `@@` headers. Moving `var a`
    // above the functions must print two hunks with three lines of context.
    let source = "extends Node\n\n\nfunc f() -> void:\n\tpass\n\n\nfunc g() -> void:\n\tpass\n\n\nfunc h() -> void:\n\tpass\n\nvar a = 1\n";
    let p = project("", source);
    let output = Command::new(env!("CARGO_BIN_EXE_gdstyle"))
        .arg("fmt")
        .arg("--diff")
        .arg(&p.script)
        .arg("--config")
        .arg(&p.config)
        .arg("--no-color")
        .output()
        .expect("run gdstyle");

    let stdout = stdout_of(&output);
    let hunks =
        "@@ -1,5 +1,7 @@\n extends Node\n \n+var a = 1\n+\n \n func f() -> void:\n \tpass\n\
                 @@ -11,5 +13,3 @@\n \n func h() -> void:\n \tpass\n-\n-var a = 1\n";
    assert!(stdout.contains(hunks), "got stdout:\n{stdout}");
}

#[test]
fn fmt_diff_shows_line_ending_changes() {
    // The file is CRLF except for one LF line, which `fmt` converts. The diff
    // must show that line as changed rather than print only context lines.
    let p = project("", "extends Node\r\n\r\nvar x := 1\n");
    let output = Command::new(env!("CARGO_BIN_EXE_gdstyle"))
        .arg("fmt")
        .arg("--diff")
        .arg(&p.script)
        .arg("--config")
        .arg(&p.config)
        .arg("--no-color")
        .output()
        .expect("run gdstyle");

    let stdout = stdout_of(&output);
    assert!(
        stdout.contains("-var x := 1\n+var x := 1^M\n"),
        "expected the converted line in the diff; got stdout:\n{stdout}"
    );
}

#[test]
fn fmt_preserves_hash_banner_blocks() {
    let source = "extends Node\n\n\n################################################################################\n# signal handlers\n################################################################################\nfunc _on_event() -> void:\n\tpass\n";
    let p = project("", source);
    for extra in [vec!["--check"], vec![]] {
        let output = Command::new(env!("CARGO_BIN_EXE_gdstyle"))
            .arg("fmt")
            .arg(&p.script)
            .arg("--config")
            .arg(&p.config)
            .arg("--no-color")
            .args(extra)
            .output()
            .expect("run gdstyle fmt");
        assert!(output.status.success(), "{}", stdout_of(&output));
        assert_eq!(std::fs::read_to_string(&p.script).unwrap(), source);
    }
}

#[test]
fn the_generated_config_names_only_real_rules() {
    // `gdstyle init` writes a template listing every rule. Now that an
    // unrecognised name is reported, the tool's own output must pass its
    // own validation. This catches a rule renamed in the registry but
    // not in the template.
    let dir = tempfile::tempdir().expect("temp dir");
    let status = Command::new(env!("CARGO_BIN_EXE_gdstyle"))
        .arg("init")
        .current_dir(dir.path())
        .status()
        .expect("run gdstyle init");
    assert!(status.success());

    let template = std::fs::read_to_string(dir.path().join("gdstyle.toml")).expect("read template");
    let known = gdstyle::rules::all_rule_names();
    // Only the keys under `[rules]`; other tables quote paths, not rules.
    let named: Vec<&str> = template
        .lines()
        .skip_while(|line| line.trim() != "[rules]")
        .filter(|line| line.contains('='))
        .filter_map(|line| line.split('"').nth(1))
        .collect();

    assert!(!named.is_empty(), "template should name some rules");
    for name in named {
        assert!(
            known.contains(&name),
            "template names {:?}, which is not in the rule registry",
            name
        );
    }
}

#[test]
fn the_example_config_matches_the_generated_one() {
    // `gdstyle.example.toml` is the template `gdstyle init` writes, under
    // another title. Kept by hand, it drifted from the template before.
    let dir = tempfile::tempdir().expect("temp dir");
    let status = Command::new(env!("CARGO_BIN_EXE_gdstyle"))
        .arg("init")
        .current_dir(dir.path())
        .status()
        .expect("run gdstyle init");
    assert!(status.success());
    let template = std::fs::read_to_string(dir.path().join("gdstyle.toml")).expect("read template");
    let example =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("gdstyle.example.toml"))
            .expect("read gdstyle.example.toml");
    let without_title = |text: &str| text.lines().skip(1).collect::<Vec<_>>().join("\n");
    assert_eq!(
        without_title(&example),
        without_title(&template),
        "gdstyle.example.toml differs from the `gdstyle init` template; copy the template over it"
    );
}

#[test]
fn issue_29_full_user_config() {
    // The reporter's own files, kept verbatim in tests/fixtures/issue29:
    // a config setting every rule to "error" and a script violating two of
    // them. Reported as warnings with exit 0; must be errors with exit 1.
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("issue29");
    let output = check(
        &fixtures.join("gdstyle.toml"),
        &fixtures.join("bad_test.gd"),
        &[],
    );
    let stdout = stdout_of(&output);

    assert_eq!(output.status.code(), Some(1), "stdout:\n{}", stdout);
    assert!(stdout.contains("2 errors found"), "got:\n{}", stdout);
    for expected in [
        "3:1 error variable name \'BadName\'",
        "5:1 error function name \'DoThing\'",
    ] {
        assert!(
            stdout.contains(expected),
            "missing {:?}; got:\n{}",
            expected,
            stdout
        );
    }
    assert!(
        !stdout.contains("warning"),
        "nothing should report as a warning; got:\n{}",
        stdout
    );
    // Every name in that config is real, so it must not trip the new
    // unknown-rule warning either.
    assert!(
        !stderr_of(&output).contains("unknown rule"),
        "the reported config is valid; got stderr:\n{}",
        stderr_of(&output)
    );
}

#[test]
fn issue_32_reserved_identifiers_fail_check() {
    // Godot 4.6 refuses to parse this file (`Expected parameter name.`),
    // but `check` used to report it clean and exit 0.
    let p = project(
        "[rules]\n",
        "extends Node\n\n\nfunc _prose(namespace: String) -> Dictionary:\n\treturn { \"namespace\": namespace }\n\n\nfunc trait(id: String) -> float:\n\treturn 0.0\n",
    );
    let output = check(&p.config, &p.script, &[]);
    let stdout = stdout_of(&output);

    assert_eq!(output.status.code(), Some(1), "stdout:\n{}", stdout);
    assert!(
        stdout.contains("4:13 error 'namespace' is a reserved word"),
        "got:\n{}",
        stdout
    );
    assert!(
        stdout.contains("8:6 error 'trait' is a reserved word"),
        "got:\n{}",
        stdout
    );
    assert!(stdout.contains("2 errors found"), "got:\n{}", stdout);
}

#[test]
fn conflict_markers_fail_check() {
    // A marker read as operators was spaced out to `<<<<<< < HEAD`, and
    // `check` passed the file with a spacing warning.
    let source = "extends Node\n\nconst OUT: String = \"--shots-out=\"\n<<<<<<< HEAD\nconst A: int = 1\n=======\nconst A: int = 2\n>>>>>>> topic\n";
    let p = project("[rules]\n", source);

    let output = check(&p.config, &p.script, &[]);
    let stdout = stdout_of(&output);
    assert_eq!(output.status.code(), Some(1), "stdout:\n{}", stdout);
    assert!(
        stdout.contains("4:1 error merge conflict marker '<<<<<<<'"),
        "got:\n{}",
        stdout
    );
    assert!(
        stdout.contains("6:1 error merge conflict marker '======='"),
        "got:\n{}",
        stdout
    );
    assert!(
        stdout.contains("8:1 error merge conflict marker '>>>>>>>'"),
        "got:\n{}",
        stdout
    );
    assert!(stdout.contains("3 errors found"), "got:\n{}", stdout);
}

#[test]
fn diff3_and_longer_conflict_markers_fail_check() {
    let source = "extends Node\n\n<<<<<<<< HEAD\nconst A: int = 1\n||||||| base\nconst A: int = 0\n==========\nconst A: int = 2\n>>>>>>>>> topic\n";
    let p = project("[rules]\n", source);

    let output = check(&p.config, &p.script, &[]);
    let stdout = stdout_of(&output);
    assert_eq!(output.status.code(), Some(1), "stdout:\n{}", stdout);
    assert!(
        stdout.contains("5:1 error merge conflict marker '|||||||'"),
        "got:\n{}",
        stdout
    );
    assert!(stdout.contains("4 errors found"), "got:\n{}", stdout);
}

/// Run `fmt` (with `--check` when asked) over `files`.
fn fmt(config: &Path, files: &[&Path], check_only: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_gdstyle"));
    command.arg("fmt");
    if check_only {
        command.arg("--check");
    }
    command
        .args(files)
        .arg("--config")
        .arg(config)
        .arg("--no-color")
        .output()
        .expect("run gdstyle")
}

#[test]
fn fmt_exits_2_on_a_conflicted_file_and_still_formats_the_rest() {
    let conflicted =
        "extends Node\n\n<<<<<<< HEAD\nvar x := 'a'\n=======\nvar x := 'b'\n>>>>>>> topic\n";
    let p = project("", conflicted);
    let valid = p.script.with_file_name("valid.gd");
    std::fs::write(&valid, "extends Node\n\nvar x := 'a'\n").expect("write script");

    let output = fmt(&p.config, &[&p.script, &valid], false);
    let stderr = stderr_of(&output);
    assert_eq!(output.status.code(), Some(2), "stderr:\n{stderr}");
    assert!(
        stderr.contains("bad_test.gd") && stderr.contains("merge conflict marker"),
        "got stderr:\n{stderr}"
    );
    assert_eq!(std::fs::read_to_string(&p.script).unwrap(), conflicted);
    assert_eq!(
        std::fs::read_to_string(&valid).unwrap(),
        "extends Node\n\nvar x := \"a\"\n",
        "the valid file is still formatted"
    );
}

#[test]
fn fmt_check_exits_2_on_a_conflicted_file_and_still_checks_the_rest() {
    let conflicted = "extends Node\n\n||||||| base\nvar x := 'a'\n";
    let p = project("", conflicted);
    let valid = p.script.with_file_name("valid.gd");
    std::fs::write(&valid, "extends Node\n\nvar x := 'a'\n").expect("write script");

    let output = fmt(&p.config, &[&p.script, &valid], true);
    let stdout = stdout_of(&output);
    let stderr = stderr_of(&output);
    assert_eq!(output.status.code(), Some(2), "stderr:\n{stderr}");
    assert!(
        stderr.contains("bad_test.gd") && stderr.contains("merge conflict marker"),
        "got stderr:\n{stderr}"
    );
    assert!(
        stdout.contains("valid.gd") && stdout.contains("1 file would be reformatted."),
        "the valid file is still checked; got stdout:\n{stdout}"
    );
    assert_eq!(std::fs::read_to_string(&p.script).unwrap(), conflicted);
}

#[test]
fn fmt_check_exits_2_when_the_only_file_is_conflicted() {
    let source = "extends Node\n\n==========\n";
    let p = project("", source);

    let output = fmt(&p.config, &[&p.script], true);
    assert_eq!(
        output.status.code(),
        Some(2),
        "stderr:\n{}",
        stderr_of(&output)
    );
}

#[test]
fn conflict_markers_after_line_continuations_fail_check_and_fmt() {
    // Every marker follows a line ending in a backslash. A continuation does
    // not start a new logical line, so all three were read as operators:
    // `check` passed the file and `fmt` spaced the markers out.
    let source = "extends Node\n\nvar x: int = 1 + \\\n<<<<<<< HEAD\n\t2 + \\\n=======\n\t3 + \\\n>>>>>>> topic\n\t4\n";
    let p = project("[rules]\n", source);

    let output = check(&p.config, &p.script, &[]);
    let stdout = stdout_of(&output);
    assert_eq!(output.status.code(), Some(1), "stdout:\n{stdout}");
    for marker in [
        "4:1 error merge conflict marker '<<<<<<<'",
        "6:1 error merge conflict marker '======='",
        "8:1 error merge conflict marker '>>>>>>>'",
    ] {
        assert!(stdout.contains(marker), "missing {marker:?} in:\n{stdout}");
    }
    assert!(stdout.contains("3 errors found"), "got:\n{stdout}");

    let output = fmt(&p.config, &[&p.script], false);
    assert_eq!(
        output.status.code(),
        Some(2),
        "stderr:\n{}",
        stderr_of(&output)
    );
    assert_eq!(std::fs::read_to_string(&p.script).unwrap(), source);
}

/// Bytes that are not valid UTF-8, so reading the file as text fails on
/// every platform.
const NOT_UTF8: [u8; 3] = [0xff, 0xfe, b'\n'];

#[test]
fn fmt_exits_2_on_an_unreadable_file_and_still_formats_the_rest() {
    // `fmt` used to print the error and exit 0.
    let p = project("", "extends Node\n\nvar x := 'a'\n");
    let unreadable = p.script.with_file_name("unreadable.gd");
    std::fs::write(&unreadable, NOT_UTF8).expect("write script");

    let output = fmt(&p.config, &[&unreadable, &p.script], false);
    let stderr = stderr_of(&output);
    assert_eq!(output.status.code(), Some(2), "stderr:\n{stderr}");
    assert!(
        stderr.contains("cannot read") && stderr.contains("unreadable.gd"),
        "got stderr:\n{stderr}"
    );
    assert_eq!(std::fs::read(&unreadable).unwrap(), NOT_UTF8);
    assert_eq!(
        std::fs::read_to_string(&p.script).unwrap(),
        "extends Node\n\nvar x := \"a\"\n",
        "the valid file is still formatted"
    );
}

#[test]
fn fmt_check_exits_2_on_an_unreadable_file() {
    let p = project("", "extends Node\n\nvar x := \"a\"\n");
    let unreadable = p.script.with_file_name("unreadable.gd");
    std::fs::write(&unreadable, NOT_UTF8).expect("write script");

    let output = fmt(&p.config, &[&unreadable, &p.script], true);
    let stdout = stdout_of(&output);
    let stderr = stderr_of(&output);
    assert_eq!(output.status.code(), Some(2), "stderr:\n{stderr}");
    assert!(
        stderr.contains("cannot read") && stderr.contains("unreadable.gd"),
        "got stderr:\n{stderr}"
    );
    assert!(
        !stdout.contains("already formatted"),
        "a file that was never read is not reported as formatted; got stdout:\n{stdout}"
    );
}

/// Make `path` read-only and return the permissions to restore afterwards.
/// `None` when this user can still write the file (root in a container):
/// there is then no write failure to observe, and the caller skips the test.
fn make_read_only(path: &Path) -> Option<std::fs::Permissions> {
    let writable = std::fs::metadata(path)
        .expect("file metadata")
        .permissions();
    let mut read_only = writable.clone();
    read_only.set_readonly(true);
    std::fs::set_permissions(path, read_only).expect("make the file read-only");
    if std::fs::OpenOptions::new().write(true).open(path).is_ok() {
        eprintln!("skipped: this user can write a read-only file");
        return None;
    }
    Some(writable)
}

#[test]
fn fmt_exits_2_when_a_file_cannot_be_written() {
    // `fmt` used to print the error and exit 0.
    let source = "extends Node\n\nvar x := 'a'\n";
    let p = project("", source);
    let Some(writable) = make_read_only(&p.script) else {
        return;
    };

    let output = fmt(&p.config, &[&p.script], false);
    // Windows cannot delete a read-only file, so restore the permissions
    // before any assertion can fail and leave the temporary directory behind.
    std::fs::set_permissions(&p.script, writable).expect("restore the permissions");

    let stderr = stderr_of(&output);
    assert_eq!(output.status.code(), Some(2), "stderr:\n{stderr}");
    assert!(
        stderr.contains("cannot write") && stderr.contains("bad_test.gd"),
        "got stderr:\n{stderr}"
    );
    assert_eq!(std::fs::read_to_string(&p.script).unwrap(), source);
}

/// A class whose method name `--unsafe-fix` renames to `do_thing`.
const DECLARES_DO_THING: &str =
    "class_name Thing\nextends Node\n\n\nfunc DoThing() -> void:\n\tpass\n";

/// A script that calls the method `DECLARES_DO_THING` declares.
const CALLS_DO_THING: &str =
    "extends Node\n\n\nfunc _ready() -> void:\n\tvar thing := Thing.new()\n\tthing.DoThing()\n";

#[test]
fn check_exits_2_on_an_unreadable_file_and_still_reports_the_rest() {
    // `check` used to print the error without the file name and exit by the
    // diagnostics alone. Those would exit 1 here: the unreadable file
    // outranks them.
    let p = project("", "extends Node\n\n<<<<<<< HEAD\n");
    let unreadable = p.script.with_file_name("unreadable.gd");
    std::fs::write(&unreadable, NOT_UTF8).expect("write script");

    let output = check(
        &p.config,
        &p.script,
        &[unreadable.to_str().expect("UTF-8 path")],
    );
    let stdout = stdout_of(&output);
    let stderr = stderr_of(&output);
    assert_eq!(output.status.code(), Some(2), "stderr:\n{stderr}");
    assert!(
        stderr.contains("cannot read") && stderr.contains("unreadable.gd"),
        "got stderr:\n{stderr}"
    );
    assert!(
        stdout.contains("3:1 error merge conflict marker '<<<<<<<'"),
        "the readable file is still reported; got stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("1 file checked"),
        "a file that was never read is not counted as checked; got stdout:\n{stdout}"
    );
}

#[test]
fn check_fix_exits_2_when_a_file_cannot_be_written() {
    // `check --fix` used to print the error and exit 0.
    let source = "extends Node\n\nvar y=1\n";
    let p = project("", source);
    let Some(writable) = make_read_only(&p.script) else {
        return;
    };

    let output = check(&p.config, &p.script, &["--fix"]);
    // Windows cannot delete a read-only file, so restore the permissions
    // before any assertion can fail and leave the temporary directory behind.
    std::fs::set_permissions(&p.script, writable).expect("restore the permissions");

    let stderr = stderr_of(&output);
    assert_eq!(output.status.code(), Some(2), "stderr:\n{stderr}");
    assert!(
        stderr.contains("cannot write") && stderr.contains("bad_test.gd"),
        "got stderr:\n{stderr}"
    );
    assert_eq!(std::fs::read_to_string(&p.script).unwrap(), source);
}

#[test]
fn check_unsafe_fix_exits_2_when_a_reference_cannot_be_rewritten() {
    // The rename lands in the declaring file, but the read-only file that
    // calls the method keeps the old name. `check` used to exit 0.
    let p = project("", DECLARES_DO_THING);
    let caller = p.script.with_file_name("caller.gd");
    std::fs::write(&caller, CALLS_DO_THING).expect("write script");
    let Some(writable) = make_read_only(&caller) else {
        return;
    };

    let output = check(
        &p.config,
        &p.script,
        &["--unsafe-fix", caller.to_str().expect("UTF-8 path")],
    );
    std::fs::set_permissions(&caller, writable).expect("restore the permissions");

    let stderr = stderr_of(&output);
    assert_eq!(output.status.code(), Some(2), "stderr:\n{stderr}");
    assert!(
        stderr.contains("cannot write") && stderr.contains("caller.gd"),
        "got stderr:\n{stderr}"
    );
    assert_eq!(std::fs::read_to_string(&caller).unwrap(), CALLS_DO_THING);
}

#[test]
fn check_unsafe_fix_exits_2_on_an_unreadable_scene() {
    // A scene that cannot be read keeps its connections to the old names.
    // `check` used to skip it without a word.
    let p = project("", DECLARES_DO_THING);
    let scene = p.script.with_file_name("broken.tscn");
    std::fs::write(&scene, NOT_UTF8).expect("write scene");
    let project_dir = p.script.parent().expect("script directory");

    let output = check(&p.config, project_dir, &["--unsafe-fix"]);
    let stderr = stderr_of(&output);
    assert_eq!(output.status.code(), Some(2), "stderr:\n{stderr}");
    assert!(
        stderr.contains("cannot read") && stderr.contains("broken.tscn"),
        "got stderr:\n{stderr}"
    );
}

#[test]
fn check_unsafe_fix_exits_2_when_a_scene_cannot_be_written() {
    // The method is renamed in the script, but the read-only scene keeps
    // connecting to the old name. `check` used to exit 0.
    let scene_source = "[gd_scene format=3]\n\n[connection signal=\"pressed\" from=\"Button\" to=\".\" method=\"DoThing\"]\n";
    let p = project("", DECLARES_DO_THING);
    let scene = p.script.with_file_name("main.tscn");
    std::fs::write(&scene, scene_source).expect("write scene");
    let project_dir = p.script.parent().expect("script directory");
    let Some(writable) = make_read_only(&scene) else {
        return;
    };

    let output = check(&p.config, project_dir, &["--unsafe-fix"]);
    std::fs::set_permissions(&scene, writable).expect("restore the permissions");

    let stderr = stderr_of(&output);
    assert_eq!(output.status.code(), Some(2), "stderr:\n{stderr}");
    assert!(
        stderr.contains("cannot write") && stderr.contains("main.tscn"),
        "got stderr:\n{stderr}"
    );
    assert_eq!(std::fs::read_to_string(&scene).unwrap(), scene_source);
}

#[test]
fn check_unsafe_fix_keeps_references_when_the_declaration_cannot_be_written() {
    // The rename never reached the read-only declaring file. `check` still
    // renamed the call in the other file, which then named a method that
    // does not exist, and reported the declaring file as fixed.
    let p = project("", DECLARES_DO_THING);
    let caller = p.script.with_file_name("caller.gd");
    std::fs::write(&caller, CALLS_DO_THING).expect("write script");
    let Some(writable) = make_read_only(&p.script) else {
        return;
    };

    let output = check(
        &p.config,
        &p.script,
        &["--unsafe-fix", caller.to_str().expect("UTF-8 path")],
    );
    std::fs::set_permissions(&p.script, writable).expect("restore the permissions");

    let stdout = stdout_of(&output);
    let stderr = stderr_of(&output);
    assert_eq!(output.status.code(), Some(2), "stderr:\n{stderr}");
    assert!(
        stderr.contains("cannot write") && stderr.contains("bad_test.gd"),
        "got stderr:\n{stderr}"
    );
    assert_eq!(
        std::fs::read_to_string(&p.script).unwrap(),
        DECLARES_DO_THING
    );
    assert_eq!(
        std::fs::read_to_string(&caller).unwrap(),
        CALLS_DO_THING,
        "the call keeps the name the declaration still has"
    );
    assert!(
        stdout.contains("function name 'DoThing' should use snake_case"),
        "the declaring file is unchanged, so its diagnostic is still reported; got stdout:\n{stdout}"
    );
}

#[test]
fn check_prints_no_summary_when_no_file_could_be_read() {
    // `check` used to print "0 files checked, no issues found." with a
    // green tick next to the read error.
    let p = project("", "");
    std::fs::write(&p.script, NOT_UTF8).expect("write script");

    let output = check(&p.config, &p.script, &[]);
    let stdout = stdout_of(&output);
    let stderr = stderr_of(&output);
    assert_eq!(output.status.code(), Some(2), "stderr:\n{stderr}");
    assert!(
        stdout.trim().is_empty(),
        "nothing was checked, so there is nothing to summarize; got stdout:\n{stdout}"
    );
    assert!(
        stderr.contains("cannot read") && stderr.contains("bad_test.gd"),
        "got stderr:\n{stderr}"
    );
}

#[test]
fn check_ends_with_a_note_that_explains_exit_code_2() {
    // The per-file errors are printed before the report, so on a large
    // project they scroll far away from the end of the output.
    let p = project("", TWO_VIOLATIONS);
    let unreadable = p.script.with_file_name("unreadable.gd");
    std::fs::write(&unreadable, NOT_UTF8).expect("write script");

    let output = check(
        &p.config,
        &p.script,
        &[unreadable.to_str().expect("UTF-8 path")],
    );
    let stderr = stderr_of(&output);
    assert_eq!(output.status.code(), Some(2), "stderr:\n{stderr}");
    assert!(
        stderr
            .trim_end()
            .ends_with("error: 1 path could not be read or written, see above"),
        "got stderr:\n{stderr}"
    );
}

#[test]
fn check_reports_a_failing_file_once() {
    // `--unsafe-fix` comes back to every file to rewrite references, so a
    // file the lint pass could not read was reported a second time.
    let p = project("", DECLARES_DO_THING);
    let caller = p.script.with_file_name("caller.gd");
    std::fs::write(&caller, CALLS_DO_THING).expect("write script");
    let unreadable = p.script.with_file_name("unreadable.gd");
    std::fs::write(&unreadable, NOT_UTF8).expect("write script");

    let output = check(
        &p.config,
        &p.script,
        &[
            "--unsafe-fix",
            caller.to_str().expect("UTF-8 path"),
            unreadable.to_str().expect("UTF-8 path"),
        ],
    );
    let stderr = stderr_of(&output);
    assert_eq!(output.status.code(), Some(2), "stderr:\n{stderr}");
    assert_eq!(
        stderr.matches("cannot read").count(),
        1,
        "got stderr:\n{stderr}"
    );
    assert!(
        stderr
            .trim_end()
            .ends_with("error: 1 path could not be read or written, see above"),
        "got stderr:\n{stderr}"
    );
    assert_eq!(
        std::fs::read_to_string(&caller).unwrap(),
        CALLS_DO_THING.replace("DoThing", "do_thing"),
        "the rename still reaches the readable caller"
    );
}

#[test]
fn check_fix_leaves_a_conflicted_file_untouched() {
    // `fmt` refuses a conflicted file, but `check --fix` still spaced out
    // the code on both sides, and the `%%%%%%%` line of a Jujutsu conflict.
    for conflicted in [
        "extends Node\n\n<<<<<<< HEAD\nvar y=1\n=======\nvar y=2\n>>>>>>> topic\n",
        "extends Node\n\n<<<<<<< Conflict 1 of 1\n%%%%%%% Changes from base to side #1\n-var y=0\n+var y=1\n+++++++ Contents of side #2\nvar y=2\n>>>>>>> Conflict 1 of 1 ends\n",
    ] {
        let p = project("", conflicted);

        let output = check(&p.config, &p.script, &["--fix"]);
        let stdout = stdout_of(&output);
        let stderr = stderr_of(&output);
        assert_eq!(output.status.code(), Some(1), "stderr:\n{stderr}");
        assert_eq!(std::fs::read_to_string(&p.script).unwrap(), conflicted);
        assert!(
            stderr.contains("fixes not applied to")
                && stderr.contains("bad_test.gd")
                && stderr.contains("merge conflict marker"),
            "got stderr:\n{stderr}"
        );
        assert!(
            stdout.contains("[format/operator-spacing]"),
            "the file is unchanged, so its fixable diagnostics are still reported; got stdout:\n{stdout}"
        );
    }
}

#[test]
fn check_unsafe_fix_leaves_references_in_a_conflicted_file_untouched() {
    // The rename lands in the declaring file. The conflicted caller is left
    // as it is, and each stale reference is named.
    let conflicted_caller = "extends Node\n\n\nfunc _ready() -> void:\n\tvar thing := Thing.new()\n<<<<<<< HEAD\n\tthing.DoThing()\n=======\n\tthing.DoThing()\n\tprint(thing)\n>>>>>>> topic\n";
    let p = project("", DECLARES_DO_THING);
    let caller = p.script.with_file_name("caller.gd");
    std::fs::write(&caller, conflicted_caller).expect("write script");

    let output = check(
        &p.config,
        &p.script,
        &["--unsafe-fix", caller.to_str().expect("UTF-8 path")],
    );
    let stderr = stderr_of(&output);
    assert_eq!(output.status.code(), Some(1), "stderr:\n{stderr}");
    assert_eq!(
        std::fs::read_to_string(&p.script).unwrap(),
        DECLARES_DO_THING.replace("DoThing", "do_thing")
    );
    assert_eq!(std::fs::read_to_string(&caller).unwrap(), conflicted_caller);
    assert_eq!(
        stderr
            .matches("reference to 'DoThing' (renamed to 'do_thing'")
            .count(),
        2,
        "both stale references are named; got stderr:\n{stderr}"
    );
}

#[test]
fn fmt_ends_with_a_note_that_explains_exit_code_2() {
    // `check` closes with the reason for exit 2. `fmt` only had the
    // per-file error, somewhere among the lines for the other files.
    let conflicted = "extends Node\n\n<<<<<<< HEAD\n";
    for check_only in [false, true] {
        let p = project("", conflicted);
        let unformatted = p.script.with_file_name("unformatted.gd");
        std::fs::write(&unformatted, "extends Node\n\nvar y=1\n").expect("write script");

        let output = fmt(&p.config, &[&p.script, &unformatted], check_only);
        let stderr = stderr_of(&output);
        assert_eq!(output.status.code(), Some(2), "stderr:\n{stderr}");
        assert!(
            stderr
                .trim_end()
                .ends_with("error: 1 path could not be formatted, see above"),
            "check_only={check_only}; got stderr:\n{stderr}"
        );
    }
}

#[test]
fn check_json_stays_parsable_when_a_file_cannot_be_read() {
    // The errors and the closing note go to stderr, so stdout is still the
    // JSON report of the files that were read.
    let p = project("", TWO_VIOLATIONS);
    let unreadable = p.script.with_file_name("unreadable.gd");
    std::fs::write(&unreadable, NOT_UTF8).expect("write script");

    let output = check(
        &p.config,
        &p.script,
        &["--format", "json", unreadable.to_str().expect("UTF-8 path")],
    );
    assert_eq!(
        output.status.code(),
        Some(2),
        "stderr:\n{}",
        stderr_of(&output)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout is valid JSON");
    assert_eq!(
        report.as_array().map(Vec::len),
        Some(2),
        "the two violations of the readable file; got:\n{report}"
    );
}

#[test]
fn check_exits_2_on_a_path_that_does_not_exist() {
    // A missing path was dropped without a word: `check src tests` passed
    // with `tests` gone, and a run with no other path said "no .gd files
    // found" and exited 0.
    let p = project("", "extends Node\n");
    let missing = p.script.with_file_name("missing.gd");
    let missing_arg = missing.to_str().expect("UTF-8 path");

    let output = check(&p.config, &p.script, &[missing_arg]);
    let stdout = stdout_of(&output);
    let stderr = stderr_of(&output);
    assert_eq!(output.status.code(), Some(2), "stderr:\n{stderr}");
    assert!(
        stderr.contains("cannot read") && stderr.contains("missing.gd"),
        "got stderr:\n{stderr}"
    );
    assert!(
        stdout.contains("1 file checked"),
        "the path that exists is still checked; got stdout:\n{stdout}"
    );

    let output = check(&p.config, &missing, &[]);
    let stderr = stderr_of(&output);
    assert_eq!(output.status.code(), Some(2), "stderr:\n{stderr}");
    assert!(
        stderr.contains("cannot read") && !stderr.contains("no .gd files found"),
        "got stderr:\n{stderr}"
    );
}

#[test]
fn fmt_exits_2_on_a_path_that_does_not_exist() {
    let p = project("", "extends Node\n\nvar x := 'a'\n");
    let missing = p.script.with_file_name("missing.gd");

    let output = fmt(&p.config, &[&missing, &p.script], false);
    let stderr = stderr_of(&output);
    assert_eq!(output.status.code(), Some(2), "stderr:\n{stderr}");
    assert!(
        stderr.contains("cannot read") && stderr.contains("missing.gd"),
        "got stderr:\n{stderr}"
    );
    assert_eq!(
        std::fs::read_to_string(&p.script).unwrap(),
        "extends Node\n\nvar x := \"a\"\n",
        "the path that exists is still formatted"
    );

    let output = fmt(&p.config, &[&missing], true);
    let stderr = stderr_of(&output);
    assert_eq!(output.status.code(), Some(2), "stderr:\n{stderr}");
    assert!(
        !stderr.contains("no .gd files found"),
        "got stderr:\n{stderr}"
    );
}

#[test]
fn a_hidden_file_does_not_fail_the_run() {
    // An editor lock file such as `.#player.gd` ends in `.gd` and is not a
    // script. Once an unreadable file meant exit 2, it failed every run.
    let p = project("", "extends Node\n");
    std::fs::write(p.script.with_file_name(".#bad_test.gd"), NOT_UTF8).expect("write lock file");
    let project_dir = p.script.parent().expect("script directory");

    let output = check(&p.config, project_dir, &[]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr:\n{}",
        stderr_of(&output)
    );
    assert!(
        stdout_of(&output).contains("1 file checked"),
        "got stdout:\n{}",
        stdout_of(&output)
    );

    let output = fmt(&p.config, &[project_dir], true);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr:\n{}",
        stderr_of(&output)
    );
}

#[test]
fn the_closing_note_counts_every_failure() {
    let p = project("", "extends Node\n");
    let first = p.script.with_file_name("first_unreadable.gd");
    let second = p.script.with_file_name("second_unreadable.gd");
    std::fs::write(&first, NOT_UTF8).expect("write script");
    std::fs::write(&second, NOT_UTF8).expect("write script");

    let output = check(
        &p.config,
        &p.script,
        &[
            first.to_str().expect("UTF-8 path"),
            second.to_str().expect("UTF-8 path"),
        ],
    );
    let stderr = stderr_of(&output);
    assert_eq!(output.status.code(), Some(2), "stderr:\n{stderr}");
    assert!(
        stderr
            .trim_end()
            .ends_with("error: 2 paths could not be read or written, see above"),
        "got stderr:\n{stderr}"
    );

    let output = fmt(&p.config, &[&p.script, &first, &second], false);
    let stderr = stderr_of(&output);
    assert_eq!(output.status.code(), Some(2), "stderr:\n{stderr}");
    assert!(
        stderr
            .trim_end()
            .ends_with("error: 2 paths could not be formatted, see above"),
        "got stderr:\n{stderr}"
    );
}

#[test]
fn fmt_prints_no_summary_when_no_file_could_be_formatted() {
    // "Formatted 0 files." in green read as a pass.
    let p = project("", "extends Node\n\n<<<<<<< HEAD\n");
    let unreadable = p.script.with_file_name("unreadable.gd");
    std::fs::write(&unreadable, NOT_UTF8).expect("write script");

    let output = fmt(&p.config, &[&p.script, &unreadable], false);
    let stdout = stdout_of(&output);
    assert_eq!(
        output.status.code(),
        Some(2),
        "stderr:\n{}",
        stderr_of(&output)
    );
    assert!(stdout.trim().is_empty(), "got stdout:\n{stdout}");
}

#[test]
fn check_fix_reports_what_is_left_after_the_fix() {
    // The spacing is fixed. The name needs `--unsafe-fix`, so it stays and
    // is the only diagnostic left to report.
    let p = project("", "extends Node\n\nvar BadName=1\n");

    let output = check(&p.config, &p.script, &["--fix"]);
    let stdout = stdout_of(&output);
    assert_eq!(output.status.code(), Some(0), "stdout:\n{stdout}");
    assert_eq!(
        std::fs::read_to_string(&p.script).unwrap(),
        "extends Node\n\nvar BadName = 1\n"
    );
    assert!(
        !stdout.contains("[format/operator-spacing]"),
        "a fixed diagnostic is not reported; got stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("[naming/variable-name-snake-case]")
            && stdout.contains("1 file checked, 1 warning found.")
            && stdout.contains("Fixed 1 file."),
        "got stdout:\n{stdout}"
    );
}
