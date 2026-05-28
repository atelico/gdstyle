use crate::diagnostic::{Diagnostic, Severity};
use colored::Colorize;

/// Output format for lint results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
}

/// Format diagnostics for human-readable terminal output.
///
/// Each diagnostic prints `line:col severity message [rule]` followed by the
/// offending source line and a caret underline (like rustc / ruff), so the
/// reader doesn't have to jump back to an editor. The source line is read
/// lazily from disk per file; if the file can't be read (e.g. linting an
/// in-memory string) the caret is simply omitted.
pub fn format_text(diagnostics: &[Diagnostic]) -> String {
    let mut output = String::new();
    let mut current_file = String::new();
    let mut current_lines: Vec<String> = Vec::new();

    for diag in diagnostics {
        if diag.file != current_file {
            if !current_file.is_empty() {
                output.push('\n');
            }
            current_file = diag.file.clone();
            output.push_str(&current_file.bold().to_string());
            output.push('\n');
            current_lines = std::fs::read_to_string(&current_file)
                .map(|s| s.lines().map(str::to_string).collect())
                .unwrap_or_default();
        }

        let severity_str = match diag.severity {
            Severity::Warning => "warning".yellow().to_string(),
            Severity::Error => "error".red().to_string(),
        };

        let location = format!("  {}:{}", diag.span.line, diag.span.column)
            .dimmed()
            .to_string();
        let rule = format!("[{}]", diag.rule).dimmed().to_string();

        output.push_str(&format!(
            "{} {} {} {}\n",
            location, severity_str, diag.message, rule
        ));

        // Source line + caret underline, when we have the source and a
        // real (1-based) column.
        if let Some(src_line) = current_lines.get(diag.span.line.wrapping_sub(1)) {
            if diag.span.column >= 1 {
                output.push_str(&format!("      {}\n", src_line.dimmed()));
                let caret_pad = caret_padding(src_line, diag.span.column - 1);
                output.push_str(&format!("      {}{}\n", caret_pad, "^".cyan()));
            }
        }
    }

    output
}

/// Build the indentation that places a caret under column `col` (0-based) of
/// `line`, preserving tabs so the caret lines up in a tab-rendering terminal.
fn caret_padding(line: &str, col: usize) -> String {
    line.chars()
        .take(col)
        .map(|c| if c == '\t' { '\t' } else { ' ' })
        .collect()
}

/// Format diagnostics as JSON.
///
/// Serialization of these plain `#[derive(Serialize)]` structs cannot fail in
/// practice, so we `expect` rather than swallowing the error into `"[]"`:
/// emitting an empty array on failure would make a CI pipeline read a broken
/// run as "no issues" and pass.
pub fn format_json(diagnostics: &[Diagnostic]) -> String {
    serde_json::to_string_pretty(diagnostics).expect("diagnostics serialize to JSON")
}

/// Print a summary of lint results.
pub fn format_summary(diagnostics: &[Diagnostic], file_count: usize) -> String {
    let warning_count = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .count();
    let error_count = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();

    if diagnostics.is_empty() {
        format!(
            "\n{} {} {} checked, no issues found.",
            "✓".green(),
            file_count,
            pluralize(file_count, "file", "files")
        )
    } else {
        let mut parts = Vec::new();
        if error_count > 0 {
            parts.push(
                format!(
                    "{} {}",
                    error_count,
                    pluralize(error_count, "error", "errors")
                )
                .red()
                .to_string(),
            );
        }
        if warning_count > 0 {
            parts.push(
                format!(
                    "{} {}",
                    warning_count,
                    pluralize(warning_count, "warning", "warnings")
                )
                .yellow()
                .to_string(),
            );
        }

        format!(
            "\n{} {} checked, {} found.",
            file_count,
            pluralize(file_count, "file", "files"),
            parts.join(", ")
        )
    }
}

fn pluralize<'a>(count: usize, singular: &'a str, plural: &'a str) -> &'a str {
    if count == 1 {
        singular
    } else {
        plural
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::Span;

    #[test]
    fn empty_diagnostics_summary() {
        let summary = format_summary(&[], 5);
        assert!(summary.contains("5 files"));
        assert!(summary.contains("no issues"));
    }

    #[test]
    fn json_format_produces_valid_json() {
        let diags = vec![Diagnostic::warning(
            "test/rule",
            "test message".to_string(),
            Span::new(1, 1, 0, 0),
            "test.gd",
        )];
        let json = format_json(&diags);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed.as_array().unwrap().len(), 1);
    }

    #[test]
    fn text_format_includes_file_and_line() {
        let diags = vec![Diagnostic::warning(
            "test/rule",
            "test message".to_string(),
            Span::new(5, 10, 0, 0),
            "player.gd",
        )];
        let text = format_text(&diags);
        assert!(text.contains("player.gd"));
        assert!(text.contains("5:10"));
        assert!(text.contains("test message"));
        assert!(text.contains("test/rule"));
    }
}
