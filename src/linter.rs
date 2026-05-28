use crate::ast::ScriptFile;
use crate::config::Config;
use crate::diagnostic::Diagnostic;
use crate::lexer::Lexer;
use crate::parser::Parser;
use crate::rules;

/// Lint a single GDScript source string and return diagnostics.
///
/// # Example
///
/// ```
/// use gdstyle::{config::Config, linter};
///
/// let diagnostics = linter::lint_source("func BadName():\n\tpass\n", "demo.gd", &Config::default());
/// assert!(diagnostics.iter().any(|d| d.rule == "naming/function-name-snake-case"));
/// ```
pub fn lint_source(source: &str, file_path: &str, config: &Config) -> Vec<Diagnostic> {
    // Normalize line endings up front. The rest of the pipeline mixes
    // `source.lines()` (strips `\r`) and `source.split('\n')` (keeps `\r`),
    // so on Windows-encoded files every byte offset disagrees by one per
    // line, autofix replacements then land mid-`\r\n` and trailing-
    // whitespace fixes leave the `\r` behind.
    let normalized = normalize_line_endings(source);
    let source = normalized.as_str();

    // Check for inline suppression comments.
    let suppressed_lines = parse_suppression_comments(source);

    // Tokenize.
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize();

    // Parse.
    let mut parser = Parser::new(&tokens);
    let members = parser.parse();

    // Build the script file representation.
    // Use split('\n') instead of lines() to preserve trailing newline detection.
    // split('\n') on "hello\n" gives ["hello", ""], where the final "" signals the
    // trailing newline. The trailing-newline rule checks if the last element is empty.
    let lines: Vec<String> = source.split('\n').map(|l| l.to_string()).collect();
    let file = ScriptFile {
        path: file_path.to_string(),
        members,
        lines,
    };

    // Run all rules.
    let mut diagnostics = rules::run_all_rules_with_source(&file, &tokens, config, Some(source));

    // Surface lexer errors (unterminated strings, invalid numbers, unexpected
    // characters). Without this, a syntactically broken file would be
    // reported as clean: the worst failure mode for a linter.
    if config.is_rule_enabled("syntax/lex-error") {
        for token in &tokens {
            if let crate::token::TokenKind::Error(ref message) = token.kind {
                diagnostics.push(Diagnostic::error(
                    "syntax/lex-error",
                    message.clone(),
                    token.span,
                    file_path,
                ));
            }
        }
    }

    // Filter out suppressed diagnostics.
    diagnostics.retain(|d| !is_suppressed(d, &suppressed_lines));

    diagnostics
}

/// Convert any `\r\n` or bare `\r` to `\n`. We do this before any byte-offset
/// computation so the rest of the pipeline can assume LF-only newlines.
pub fn normalize_line_endings(source: &str) -> String {
    if !source.contains('\r') {
        return source.to_string();
    }
    source.replace("\r\n", "\n").replace('\r', "\n")
}

/// Lint a file from disk.
pub fn lint_file(path: &std::path::Path, config: &Config) -> Result<Vec<Diagnostic>, String> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {}", path.display(), e))?;

    let file_path = path.to_string_lossy().to_string();
    Ok(lint_source(&source, &file_path, config))
}

/// Per-line suppression state. `None` rule list means "suppress all
/// diagnostics on this line"; `Some(rules)` means "suppress only these".
/// We index by line number so `is_suppressed` is O(1).
type Suppressions = std::collections::HashMap<usize, Vec<Option<Vec<String>>>>;

/// Parse suppression comments (`# gdstyle:ignore`).
fn parse_suppression_comments(source: &str) -> Suppressions {
    let mut suppressions: Suppressions = std::collections::HashMap::new();
    let prefix = "# gdstyle:ignore";

    for (i, line) in source.lines().enumerate() {
        let line_num = i + 1;
        let trimmed = line.trim();

        // Standalone suppression: applies to the NEXT line.
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            let rules = parse_suppression_rules(rest);
            suppressions.entry(line_num + 1).or_default().push(rules);
        }

        // Inline suppression: applies to the SAME line.
        if let Some(pos) = line.find(prefix) {
            let before = line[..pos].trim();
            if !before.is_empty() {
                let rest = &line[pos + prefix.len()..];
                let rules = parse_suppression_rules(rest);
                suppressions.entry(line_num).or_default().push(rules);
            }
        }
    }

    suppressions
}

fn parse_suppression_rules(rest: &str) -> Option<Vec<String>> {
    let rest = rest.trim();
    rest.strip_prefix('=').map(|rules_str| {
        rules_str
            .split(',')
            .map(|r| r.trim().to_string())
            .filter(|r| !r.is_empty())
            .collect()
    })
}

fn is_suppressed(diagnostic: &Diagnostic, suppressions: &Suppressions) -> bool {
    let Some(line_suppressions) = suppressions.get(&diagnostic.span.line) else {
        return false;
    };
    line_suppressions.iter().any(|rules| match rules {
        None => true, // bare `# gdstyle:ignore` suppresses everything
        Some(rs) => rs.iter().any(|r| r == &diagnostic.rule),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lint_clean_source() {
        let source = r#"class_name Player
extends CharacterBody2D

signal health_changed(old_value: int, new_value: int)

const MAX_SPEED: float = 200.0

@export var speed: float = 100.0

var health: int = 100

@onready var label: Label = $Label

func _ready() -> void:
	pass

func take_damage(amount: int) -> void:
	pass
"#;
        let config = Config::default();
        let diagnostics = lint_source(source, "player.gd", &config);
        assert!(
            diagnostics.is_empty(),
            "clean source should produce no diagnostics, got: {:?}",
            diagnostics
        );
    }

    #[test]
    fn lint_detects_bad_class_name() {
        let source = "class_name my_player\n";
        let config = Config::default();
        let diagnostics = lint_source(source, "test.gd", &config);
        assert!(diagnostics
            .iter()
            .any(|d| d.rule == "naming/class-name-pascal-case"));
    }

    #[test]
    fn lint_detects_bad_function_name() {
        let source = "func takeDamage() -> void:\n\tpass\n";
        let config = Config::default();
        let diagnostics = lint_source(source, "test.gd", &config);
        assert!(diagnostics
            .iter()
            .any(|d| d.rule == "naming/function-name-snake-case"));
    }

    #[test]
    fn lint_surfaces_unterminated_string_as_error() {
        let source = "var x = \"oops\nvar y = 5\n";
        let config = Config::default();
        let diagnostics = lint_source(source, "test.gd", &config);
        let lex_errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.rule == "syntax/lex-error")
            .collect();
        assert!(
            !lex_errors.is_empty(),
            "unterminated string must produce a syntax/lex-error diagnostic"
        );
        assert_eq!(lex_errors[0].severity, crate::diagnostic::Severity::Error);
    }

    #[test]
    fn lint_detects_trailing_whitespace() {
        let source = "var x = 5   \n";
        let config = Config::default();
        let diagnostics = lint_source(source, "test.gd", &config);
        assert!(diagnostics
            .iter()
            .any(|d| d.rule == "format/trailing-whitespace"));
    }

    #[test]
    fn lint_detects_long_line() {
        let long_line = format!("var x = \"{}\"", "a".repeat(110));
        let source = format!("{}\n", long_line);
        let config = Config::default();
        let diagnostics = lint_source(&source, "test.gd", &config);
        assert!(diagnostics
            .iter()
            .any(|d| d.rule == "format/max-line-length"));
    }

    #[test]
    fn inline_suppression_works() {
        let source = "class_name my_player  # gdstyle:ignore=naming/class-name-pascal-case\n";
        let config = Config::default();
        let diagnostics = lint_source(source, "test.gd", &config);
        assert!(
            !diagnostics
                .iter()
                .any(|d| d.rule == "naming/class-name-pascal-case"),
            "suppressed rule should not appear"
        );
    }

    #[test]
    fn standalone_suppression_works() {
        let source = "# gdstyle:ignore=naming/class-name-pascal-case\nclass_name my_player\n";
        let config = Config::default();
        let diagnostics = lint_source(source, "test.gd", &config);
        assert!(
            !diagnostics
                .iter()
                .any(|d| d.rule == "naming/class-name-pascal-case"),
            "suppressed rule should not appear"
        );
    }

    #[test]
    fn suppress_all_rules_on_line() {
        let source = "# gdstyle:ignore\nclass_name my_player\n";
        let config = Config::default();
        let diagnostics = lint_source(source, "test.gd", &config);
        // All diagnostics on line 2 should be suppressed.
        let line_2_diags: Vec<_> = diagnostics.iter().filter(|d| d.span.line == 2).collect();
        assert!(line_2_diags.is_empty());
    }

    #[test]
    fn config_disables_rule() {
        let source = "class_name my_player\n";
        let mut config = Config::default();
        config.rules.insert(
            "naming/class-name-pascal-case".to_string(),
            crate::config::RuleSeverityConfig::Off,
        );
        let diagnostics = lint_source(source, "test.gd", &config);
        assert!(!diagnostics
            .iter()
            .any(|d| d.rule == "naming/class-name-pascal-case"));
    }

    #[test]
    fn lint_real_world_script() {
        let source = r#"@tool
class_name StateMachine
extends Node
## Hierarchical state machine for the player.
##
## Initializes states and delegates engine callbacks to the state.

signal state_changed(previous: String, current: String)

@export var initial_state: Node

var is_active: bool = true

@onready var _state: Node = $State

func _init() -> void:
	add_to_group("state_machine")

func _ready() -> void:
	state_changed.connect(_on_state_changed)

func _physics_process(delta: float) -> void:
	_state._physics_process(delta)

func transition_to(target_path: String) -> void:
	pass

func _on_state_changed(previous: String, current: String) -> void:
	pass
"#;
        let config = Config::default();
        let diagnostics = lint_source(source, "state_machine.gd", &config);
        assert!(
            diagnostics.is_empty(),
            "real-world clean script should have no issues, got: {:?}",
            diagnostics
        );
    }
}
