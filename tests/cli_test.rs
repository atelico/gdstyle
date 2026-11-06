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
    assert_eq!(std::fs::read_to_string(&p.script).unwrap(), source);
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

#[test]
fn fmt_exits_2_when_a_file_cannot_be_written() {
    // `fmt` used to print the error and exit 0.
    let source = "extends Node\n\nvar x := 'a'\n";
    let p = project("", source);
    let writable = std::fs::metadata(&p.script)
        .expect("script metadata")
        .permissions();
    let mut read_only = writable.clone();
    read_only.set_readonly(true);
    std::fs::set_permissions(&p.script, read_only).expect("make the script read-only");
    // A privileged user (root in a container) can still write the file, and
    // then there is no write failure to observe.
    if std::fs::OpenOptions::new()
        .write(true)
        .open(&p.script)
        .is_ok()
    {
        eprintln!("skipped: this user can write a read-only file");
        return;
    }

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
