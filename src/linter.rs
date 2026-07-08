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

/// Per-line and per-file suppression state. For each scope, `None` in
/// the rule list means "suppress all diagnostics in this scope"; `Some`
/// means "suppress only these rules".
#[derive(Default)]
pub(crate) struct Suppressions {
    /// Per-line: indexed by line number so `is_suppressed` is O(1).
    per_line: std::collections::HashMap<usize, Vec<Option<Vec<String>>>>,
    /// Per-file: all `# gdstyle:ignore-file` directives merged together,
    /// regardless of where they appear in the source.
    file_level: Vec<Option<Vec<String>>>,
}

impl Suppressions {
    /// Parse every suppression directive out of `source`.
    pub(crate) fn parse(source: &str) -> Self {
        parse_suppression_comments(source)
    }

    /// True when `rule` is suppressed for a member whose header spans lines
    /// `first..=last` (1-based, inclusive) — that is, a `# gdstyle:ignore-file`
    /// covering the rule, or a per-line `# gdstyle:ignore` that resolves onto
    /// any line of that header. Used both to pin a member against reordering
    /// and to exempt it from the `order/class-member-order` check, so `fmt` and
    /// `check` agree on which members opt out.
    pub(crate) fn suppresses_member(&self, first: usize, last: usize, rule: &str) -> bool {
        if self.file_level.iter().any(|r| matches_rule_list(r, rule)) {
            return true;
        }
        (first..=last).any(|line| {
            self.per_line
                .get(&line)
                .is_some_and(|lists| lists.iter().any(|r| matches_rule_list(r, rule)))
        })
    }
}

/// Parse suppression comments. Two forms are recognised:
///
/// 1. `# gdstyle:ignore[=rule1,rule2]` — per-line scope. On its own line
///    it suppresses the NEXT line; placed at the end of a code line it
///    suppresses the SAME line.
/// 2. `# gdstyle:ignore-file[=rule1,rule2]` — per-file scope. Suppresses
///    the listed rules (or all rules, when no `=...` is given) for every
///    diagnostic in the file, regardless of line. Anywhere in the file
///    works; convention is at the top so it's visible to readers.
fn parse_suppression_comments(source: &str) -> Suppressions {
    let mut suppressions = Suppressions::default();
    let file_prefix = "# gdstyle:ignore-file";
    let line_prefix = "# gdstyle:ignore";

    for (i, line) in source.lines().enumerate() {
        let line_num = i + 1;
        let trimmed = line.trim();

        // File-scope check must come BEFORE the per-line check because
        // `# gdstyle:ignore` is a prefix of `# gdstyle:ignore-file` and
        // would otherwise match first and steal the directive.
        if let Some(rest) = trimmed.strip_prefix(file_prefix) {
            if directive_terminator_ok(rest) {
                let rules = parse_suppression_rules(rest);
                suppressions.file_level.push(rules);
                continue;
            }
        }
        if let Some(pos) = line.find(file_prefix) {
            let rest = &line[pos + file_prefix.len()..];
            if directive_terminator_ok(rest) {
                let rules = parse_suppression_rules(rest);
                suppressions.file_level.push(rules);
                // An inline file-level directive on a code line behaves
                // identically to a standalone one; we don't ALSO record
                // a per-line suppression for that line.
                continue;
            }
        }

        // Standalone per-line suppression: applies to the NEXT line.
        if let Some(rest) = trimmed.strip_prefix(line_prefix) {
            if directive_terminator_ok(rest) {
                let rules = parse_suppression_rules(rest);
                suppressions
                    .per_line
                    .entry(line_num + 1)
                    .or_default()
                    .push(rules);
            }
        }

        // Inline per-line suppression: applies to the SAME line.
        if let Some(pos) = line.find(line_prefix) {
            let before = line[..pos].trim();
            if !before.is_empty() {
                let rest = &line[pos + line_prefix.len()..];
                if directive_terminator_ok(rest) {
                    let rules = parse_suppression_rules(rest);
                    suppressions
                        .per_line
                        .entry(line_num)
                        .or_default()
                        .push(rules);
                }
            }
        }
    }

    suppressions
}

/// After stripping the directive prefix, the remainder must either be
/// empty / pure whitespace / a comment continuation, or start with `=`.
/// Without this check `# gdstyle:ignore-foo` would also match the
/// `# gdstyle:ignore` prefix and be treated as a bare-suppression.
fn directive_terminator_ok(rest: &str) -> bool {
    match rest.chars().next() {
        None => true,
        Some(c) => c == '=' || c.is_whitespace(),
    }
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

fn matches_rule_list(rules: &Option<Vec<String>>, rule: &str) -> bool {
    match rules {
        None => true, // bare directive suppresses everything
        Some(rs) => rs.iter().any(|r| r == rule),
    }
}

fn is_suppressed(diagnostic: &Diagnostic, suppressions: &Suppressions) -> bool {
    // File-level wins first: a `# gdstyle:ignore-file` at any line
    // covers diagnostics anywhere in the file. This is the right home
    // for class/file-scope limits like `quality/max-public-methods`,
    // which report against the class header itself and can't be
    // suppressed with a same-line comment without uglifying the
    // signature.
    if suppressions
        .file_level
        .iter()
        .any(|rules| matches_rule_list(rules, &diagnostic.rule))
    {
        return true;
    }
    let Some(line_suppressions) = suppressions.per_line.get(&diagnostic.span.line) else {
        return false;
    };
    line_suppressions
        .iter()
        .any(|rules| matches_rule_list(rules, &diagnostic.rule))
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

    /// Build a 25-public-method class for the file-scope tests below.
    /// We rebuild it inline because every test needs to start from a
    /// known-clean source — sharing a `const` across tests would make
    /// failures harder to trace.
    fn bloated_class(prelude: &str) -> String {
        let mut s = String::from(prelude);
        s.push_str("class_name Bloated\nextends Node\n");
        for i in 0..25 {
            s.push_str(&format!("\nfunc method_{}() -> void:\n\tpass\n", i));
        }
        s
    }

    #[test]
    fn file_suppression_silences_class_level_rule() {
        // Regression for the user-reported gap: a `# gdstyle:ignore`
        // standalone above the class header doesn't catch
        // `quality/max-public-methods` because the diagnostic anchors
        // at line 1 and the standalone form suppresses the NEXT line.
        // The file-level directive sidesteps that mismatch entirely.
        let source = bloated_class("# gdstyle:ignore-file=quality/max-public-methods\n");
        let config = Config::default();
        let diagnostics = lint_source(&source, "test.gd", &config);
        assert!(
            !diagnostics
                .iter()
                .any(|d| d.rule == "quality/max-public-methods"),
            "file-level suppression should silence max-public-methods, got: {:?}",
            diagnostics
        );
    }

    #[test]
    fn file_suppression_bare_form_silences_everything() {
        // `# gdstyle:ignore-file` (no `=...`) drops every diagnostic
        // in the file — for generated code or third-party drops.
        let source = "# gdstyle:ignore-file\nclass_name my_bad_name\nvar AlsoBad: int = 5\n";
        let config = Config::default();
        let diagnostics = lint_source(source, "test.gd", &config);
        assert!(
            diagnostics.is_empty(),
            "bare ignore-file should drop everything, got: {:?}",
            diagnostics
        );
    }

    #[test]
    fn file_suppression_anywhere_in_file() {
        // The directive doesn't have to live on line 1 — anywhere in
        // the file works. This matters for files that already start
        // with a class-level `##` docstring before the suppression.
        let mut source = String::from("## Docstring for Bloated.\n");
        source.push_str(&bloated_class(""));
        source.push_str("\n# gdstyle:ignore-file=quality/max-public-methods\n");
        let config = Config::default();
        let diagnostics = lint_source(&source, "test.gd", &config);
        assert!(
            !diagnostics
                .iter()
                .any(|d| d.rule == "quality/max-public-methods"),
            "file-level suppression must apply regardless of position, got: {:?}",
            diagnostics
        );
    }

    #[test]
    fn file_suppression_narrows_to_listed_rules() {
        // A scoped `# gdstyle:ignore-file=...` only silences the
        // listed rules — others still fire. The unlisted naming rule
        // on `BadName` must remain.
        let source = "# gdstyle:ignore-file=quality/max-public-methods\nclass_name Bloated\nextends Node\nvar BadName: int = 5\n";
        let config = Config::default();
        let diagnostics = lint_source(source, "test.gd", &config);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.rule == "naming/variable-name-snake-case"),
            "scoped ignore-file should NOT silence unlisted rules, got: {:?}",
            diagnostics
        );
    }

    #[test]
    fn file_suppression_does_not_swallow_per_line_directive() {
        // Regression guard: the `ignore-file` parsing path runs before
        // `ignore` and `continue`s when it matches. Make sure a
        // regular per-line `# gdstyle:ignore` directive still routes
        // through the per-line path.
        let source = "# gdstyle:ignore=naming/class-name-pascal-case\nclass_name my_player\n";
        let config = Config::default();
        let diagnostics = lint_source(source, "test.gd", &config);
        assert!(
            !diagnostics
                .iter()
                .any(|d| d.rule == "naming/class-name-pascal-case"),
            "per-line directive still works alongside ignore-file support"
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
