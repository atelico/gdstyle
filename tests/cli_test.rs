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
