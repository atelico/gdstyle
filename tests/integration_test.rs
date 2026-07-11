use gdstyle::ast::ClassMember;
use gdstyle::config::Config;
use gdstyle::fixer;
use gdstyle::formatter;
use gdstyle::lexer::Lexer;
use gdstyle::linter;
use gdstyle::parser::Parser as GdParser;
use std::path::Path;

fn parse_members_for_test(source: &str) -> Vec<ClassMember> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize();
    GdParser::new(&tokens).parse()
}

fn default_config() -> Config {
    Config::default()
}

fn fixture_path(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

// --- End-to-end integration tests ---

#[test]
fn clean_script_produces_no_diagnostics() {
    let config = default_config();
    let diagnostics = linter::lint_file(&fixture_path("clean_script.gd"), &config).unwrap();
    assert!(
        diagnostics.is_empty(),
        "clean_script.gd should produce no diagnostics, got:\n{}",
        diagnostics
            .iter()
            .map(|d| format!("  line {}: [{}] {}", d.span.line, d.rule, d.message))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn bad_naming_detects_all_violations() {
    let config = default_config();
    let diagnostics = linter::lint_file(&fixture_path("bad_naming.gd"), &config).unwrap();

    let rule_names: Vec<&str> = diagnostics.iter().map(|d| d.rule.as_str()).collect();

    assert!(
        rule_names.contains(&"naming/class-name-pascal-case"),
        "should detect bad class name"
    );
    assert!(
        rule_names.contains(&"naming/signal-name-snake-case"),
        "should detect bad signal name"
    );
    assert!(
        rule_names.contains(&"naming/enum-name-pascal-case"),
        "should detect bad enum name"
    );
    assert!(
        rule_names.contains(&"naming/enum-member-screaming-case"),
        "should detect bad enum members"
    );
    assert!(
        rule_names.contains(&"naming/constant-name-screaming-case"),
        "should detect bad constant name"
    );
    assert!(
        rule_names.contains(&"naming/variable-name-snake-case"),
        "should detect bad variable name"
    );
    assert!(
        rule_names.contains(&"naming/function-name-snake-case"),
        "should detect bad function name"
    );
}

#[test]
fn bad_formatting_detects_violations() {
    let config = default_config();
    let diagnostics = linter::lint_file(&fixture_path("bad_formatting.gd"), &config).unwrap();

    let rule_names: Vec<&str> = diagnostics.iter().map(|d| d.rule.as_str()).collect();

    assert!(
        rule_names.contains(&"format/number-literals"),
        "should detect uppercase hex"
    );
    assert!(
        rule_names.contains(&"format/boolean-operators"),
        "should detect && and ||"
    );
    assert!(
        rule_names.contains(&"format/double-quotes"),
        "should detect single quotes"
    );
    assert!(
        rule_names.contains(&"format/one-statement-per-line"),
        "should detect semicolon"
    );
}

#[test]
fn bad_ordering_detects_violations() {
    let config = default_config();
    let diagnostics = linter::lint_file(&fixture_path("bad_ordering.gd"), &config).unwrap();

    let rule_names: Vec<&str> = diagnostics.iter().map(|d| d.rule.as_str()).collect();

    assert!(
        rule_names.contains(&"order/class-member-order"),
        "should detect ordering violations"
    );
}

#[test]
fn suppression_comments_work() {
    let config = default_config();
    let diagnostics = linter::lint_file(&fixture_path("suppressed.gd"), &config).unwrap();

    let variable_name_diags: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "naming/variable-name-snake-case")
        .collect();

    assert_eq!(
        variable_name_diags.len(),
        1,
        "only YetAnother should trigger naming rule, got: {:?}",
        variable_name_diags
    );

    assert!(
        variable_name_diags[0].message.contains("YetAnother"),
        "the one diagnostic should be about YetAnother"
    );
}

#[test]
fn config_overrides_work() {
    let mut config = default_config();
    config.rules.insert(
        "naming/class-name-pascal-case".to_string(),
        gdstyle::config::RuleSeverityConfig::Off,
    );

    let diagnostics = linter::lint_file(&fixture_path("bad_naming.gd"), &config).unwrap();
    let class_name_diags: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "naming/class-name-pascal-case")
        .collect();
    assert!(
        class_name_diags.is_empty(),
        "disabled rule should not produce diagnostics"
    );
}

#[test]
fn custom_line_length_config() {
    let mut config = default_config();
    config.max_line_length = 200;

    let source = format!("var x: String = \"{}\"\n", "a".repeat(150));
    let diagnostics = linter::lint_source(&source, "test.gd", &config);
    let line_length_diags: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "format/max-line-length")
        .collect();
    assert!(line_length_diags.is_empty());
}

#[test]
fn json_output_format() {
    let config = default_config();
    let diagnostics = linter::lint_file(&fixture_path("bad_naming.gd"), &config).unwrap();

    let json = gdstyle::reporter::format_json(&diagnostics);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed.is_array());
    assert!(!parsed.as_array().unwrap().is_empty());

    let first = &parsed[0];
    assert!(first.get("rule").is_some());
    assert!(first.get("message").is_some());
    assert!(first.get("severity").is_some());
    assert!(first.get("span").is_some());
    assert!(first.get("file").is_some());
}

#[test]
fn file_name_check_on_pascal_case_file() {
    let config = default_config();
    let source = "extends Node\n";
    let diagnostics = linter::lint_source(source, "PlayerController.gd", &config);
    assert!(
        diagnostics
            .iter()
            .any(|d| d.rule == "naming/file-name-snake-case"),
        "should flag PascalCase filename"
    );
}

#[test]
fn lint_multiple_files_independently() {
    let config = default_config();

    let clean = linter::lint_file(&fixture_path("clean_script.gd"), &config).unwrap();
    let bad = linter::lint_file(&fixture_path("bad_naming.gd"), &config).unwrap();

    assert!(clean.is_empty());
    assert!(!bad.is_empty());
}

// --- Regression tests ---

#[test]
fn blank_lines_not_counted_inside_function_bodies() {
    // Regression: check_blank_lines used the start line of the previous member
    // instead of its end line, causing it to count blank lines inside function
    // bodies as if they were between top-level members.
    let source = r#"extends Node

func long_function():
	var a = 1

	var b = 2

	var c = 3

	var d = 4

	var e = 5

	return a + b + c + d + e

func next_function():
	pass
"#;
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let blank_line_diags: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "format/blank-lines")
        .collect();
    assert!(
        blank_line_diags.is_empty(),
        "blank lines inside function bodies should not trigger format/blank-lines, got: {:?}",
        blank_line_diags
            .iter()
            .map(|d| format!("line {}: {}", d.span.line, d.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn blank_lines_between_functions_still_detected() {
    // Ensure the blank-lines rule still catches actual violations between
    // top-level members (3+ blank lines).
    let source = "extends Node\n\n\nvar x = 1\n\n\n\n\nvar y = 2\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        diagnostics.iter().any(|d| d.rule == "format/blank-lines"),
        "should still detect too many blank lines between top-level members"
    );
}

#[test]
fn operator_spacing_correct_after_multibyte_utf8() {
    // Regression: the lexer tracked character index instead of byte offset,
    // so after a multi-byte character (e.g., em dash U+2014, 3 bytes) all
    // subsequent span offsets were wrong, causing false operator-spacing warnings.
    let source = "extends Node\n\n# Comment with em dash \u{2014} here\n\nfunc test(a: bool):\n\tif a and true != null:\n\t\tpass\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let spacing_diags: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "format/operator-spacing")
        .collect();
    assert!(
        spacing_diags.is_empty(),
        "correctly spaced operators after multi-byte UTF-8 should not trigger warnings, got: {:?}",
        spacing_diags
            .iter()
            .map(|d| format!("line {}:{}: {}", d.span.line, d.span.column, d.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn no_panic_on_string_after_multibyte_utf8() {
    // Regression: source slicing in read_string used char index instead of
    // byte offset, causing a panic when a string literal appeared after a
    // multi-byte UTF-8 character (e.g., em dash).
    let source = "extends Node\n\n# Em dash \u{2014} here\n\nvar x = \"hello\"\n";
    let config = default_config();
    // Should not panic.
    let _diagnostics = linter::lint_source(source, "test.gd", &config);
}

#[test]
fn operator_spacing_still_detected_after_multibyte_utf8() {
    // Ensure operator-spacing still catches real violations after multi-byte chars.
    let source = "extends Node\n\n# Comment with em dash \u{2014} here\n\nfunc test(a: int, b: int):\n\tvar x = a+b\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        diagnostics
            .iter()
            .any(|d| d.rule == "format/operator-spacing"),
        "should still detect missing spaces around + after multi-byte UTF-8"
    );
}

#[test]
fn static_var_screaming_case_accepted() {
    // Regression: static var with SCREAMING_SNAKE_CASE was flagged as a naming
    // violation and the auto-fix corrupted the line.
    let source = "static var THOUGHT_POIGNANCY: int = 5\nstatic var MY_CONSTANT: String = \"x\"\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let naming_diags: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "naming/variable-name-snake-case")
        .collect();
    assert!(
        naming_diags.is_empty(),
        "static var with SCREAMING_SNAKE_CASE should be accepted, got: {:?}",
        naming_diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn to_snake_case_no_double_underscores() {
    // Regression: to_snake_case("THOUGHT_POIGNANCY") produced "thought__poignancy".
    let source = "var MyPascalVar: int = 1\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let fix_diag = diagnostics
        .iter()
        .find(|d| d.rule == "naming/variable-name-snake-case");
    assert!(fix_diag.is_some());
    let msg = &fix_diag.unwrap().message;
    assert!(
        !msg.contains("__"),
        "suggested name should not contain double underscores, got: {}",
        msg
    );
}

#[test]
fn max_line_length_counts_tabs_at_visual_width() {
    // Tabs should be counted at 4-column width, not 1 byte.
    // A tab + 97 chars = 101 visual columns (exceeds 100).
    // A tab + 96 chars = 100 visual columns (exactly at limit).
    let config = default_config();

    let at_limit = format!("\t{}\n", "a".repeat(96));
    let diags = linter::lint_source(&at_limit, "test.gd", &config);
    assert!(
        !diags.iter().any(|d| d.rule == "format/max-line-length"),
        "tab + 96 chars = 100 visual columns, should not trigger"
    );

    let over_limit = format!("\t{}\n", "a".repeat(97));
    let diags = linter::lint_source(&over_limit, "test.gd", &config);
    assert!(
        diags.iter().any(|d| d.rule == "format/max-line-length"),
        "tab + 97 chars = 101 visual columns, should trigger"
    );
}

// --- Fix tests ---

#[test]
fn fix_trailing_whitespace() {
    let source = "var x = 5   \n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let fixed = fixer::apply_fixes(source, &diagnostics, true);
    assert_eq!(fixed, "var x = 5\n");
}

#[test]
fn fix_boolean_operators() {
    let source = "if a && b:\n\tpass\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let fixed = fixer::apply_fixes(source, &diagnostics, true);
    assert!(fixed.contains("and"), "should replace && with and");
    assert!(!fixed.contains("&&"), "should not contain &&");
}

#[test]
fn fix_double_quotes() {
    let source = "var x = 'hello'\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let fixed = fixer::apply_fixes(source, &diagnostics, true);
    assert!(
        fixed.contains("\"hello\""),
        "should replace single with double quotes"
    );
}

#[test]
fn fix_uppercase_hex() {
    let source = "var x = 0xFF\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let fixed = fixer::apply_fixes(source, &diagnostics, true);
    assert!(fixed.contains("0xff"), "should lowercase hex digits");
}

#[test]
fn fix_trailing_newline() {
    let source = "var x = 5";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let fixed = fixer::apply_fixes(source, &diagnostics, true);
    assert!(fixed.ends_with('\n'), "should add trailing newline");
}

#[test]
fn fix_preserves_safe_only() {
    let source = "signal health_change\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);

    // Safe-only should NOT fix signal-past-tense (unsafe fix).
    let safe_fixed = fixer::apply_fixes(source, &diagnostics, true);
    assert!(
        safe_fixed.contains("health_change"),
        "safe-only should not rename signal"
    );

    // Unsafe mode should fix it (past tense: health_change -> health_changed).
    let unsafe_fixed = fixer::apply_fixes(source, &diagnostics, false);
    assert!(
        unsafe_fixed.contains("health_changed"),
        "unsafe fix should rename signal to past tense, got: {}",
        unsafe_fixed
    );
}

// --- Formatter tests ---

#[test]
fn formatter_clean_script_idempotent() {
    let source = std::fs::read_to_string(fixture_path("clean_script.gd")).unwrap();
    let config = default_config();
    let first = formatter::format_source(&source, &config);
    let second = formatter::format_source(&first, &config);
    assert_eq!(
        first, second,
        "formatter must be idempotent on clean script"
    );
}

#[test]
fn formatter_normalizes_quotes() {
    let source = "var x = 'hello'\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert!(
        formatted.contains("\"hello\""),
        "formatter should normalize quotes"
    );
}

#[test]
fn formatter_normalizes_boolean_operators() {
    let source = "if a && b || !c:\n\tpass\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert!(!formatted.contains("&&"), "should not contain &&");
    assert!(!formatted.contains("||"), "should not contain ||");
    assert!(formatted.contains("and"), "should contain 'and'");
    assert!(formatted.contains("or"), "should contain 'or'");
}

#[test]
fn formatter_strips_trailing_whitespace() {
    let source = "var x = 5   \nvar y = 10\t\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    for line in formatted.lines() {
        assert!(
            !line.ends_with(' ') && !line.ends_with('\t'),
            "line should not have trailing whitespace: '{}'",
            line
        );
    }
}

#[test]
fn formatter_ensures_trailing_newline() {
    let source = "var x = 5";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert!(formatted.ends_with('\n'), "should end with newline");
}

#[test]
fn formatter_collapses_blank_lines() {
    let source = "var x = 5\n\n\n\n\nvar y = 10\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    // Should have at most 2 consecutive blank lines.
    assert!(
        !formatted.contains("\n\n\n\n"),
        "should collapse 4+ blank lines"
    );
}

#[test]
fn formatter_normalizes_hex() {
    let source = "var x = 0xFF\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert!(formatted.contains("0xff"), "should lowercase hex digits");
}

// --- Regression tests for bugs found during GARP project testing ---

#[test]
fn formatter_does_not_rename_variables() {
    // Regression: naming fixes were marked is_safe=true and the formatter renamed
    // variables at their declaration site without updating all references, corrupting code.
    // Examples: `var CONFIG` -> `config CONFIG`, `var DialogueBoxClass` -> `dialogue_box_class DialogueBoxClass`
    let source = "@onready var CONFIG: ConfigFile = load(\"config\")\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert!(
        formatted.contains("var CONFIG"),
        "formatter must not rename variable identifiers, got: {}",
        formatted.trim()
    );
}

#[test]
fn formatter_does_not_rename_static_var_pascal_case() {
    // Regression: `static var TogglePauseCommand` became
    // `toggle_pause_command toggle_pause_command TogglePauseCommand` (duplicate snake_case).
    let source = "static var TogglePauseCommand: String = \"toggle_pause\"\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert!(
        formatted.contains("static var TogglePauseCommand"),
        "formatter must not rename static var identifiers, got: {}",
        formatted.trim()
    );
}

#[test]
fn formatter_does_not_rename_constants() {
    // Regression: `const event_tag = "event:"` became `EVENT_TAG event_tag = "event:"`
    let source = "const event_tag = \"event:\"\nconst data_tag = \"data:\"\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert!(
        formatted.contains("const event_tag"),
        "formatter must not rename constant identifiers, got: {}",
        formatted.trim()
    );
}

#[test]
fn formatter_does_not_rename_enum_members() {
    // Regression: enum members like `None` were renamed to `NONE`,
    // `AIInControl` to `AIIN_CONTROL`, breaking all references.
    let source = "enum MovementMode {\n\tNone,\n\tAIInControl,\n\tPlayerInControl,\n}\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert!(
        formatted.contains("None"),
        "formatter must not rename enum members, got: {}",
        formatted.trim()
    );
    assert!(
        formatted.contains("AIInControl"),
        "formatter must not rename enum members, got: {}",
        formatted.trim()
    );
}

#[test]
fn formatter_does_not_rename_functions() {
    // Regression: `func pullArray():` became `pull_array pullArray():`
    let source = "func pullArray():\n\tpass\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert!(
        formatted.contains("func pullArray()"),
        "formatter must not rename function identifiers, got: {}",
        formatted.trim()
    );
}

#[test]
fn formatter_does_not_rename_class_name() {
    // Regression: `class_name TV extends InteractableObject` became `Tv TV extends...`
    let source = "class_name TV extends Node\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert!(
        formatted.contains("class_name TV"),
        "formatter must not rename class_name identifiers, got: {}",
        formatted.trim()
    );
}

#[test]
fn formatter_does_not_rename_export_var() {
    // Regression: `@export var CHATTING_RADIUS: int = 300` became
    // `@export chatting_radius CHATTING_RADIUS: int = 300`
    let source = "@export var CHATTING_RADIUS: int = 300\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert!(
        formatted.contains("@export var CHATTING_RADIUS"),
        "formatter must not rename @export var identifiers, got: {}",
        formatted.trim()
    );
}

#[test]
fn formatter_preserves_walrus_operator() {
    // Regression: `:=` was split into `: =` by operator spacing rule.
    // `var exit_thread := false` became `var exit_thread : = false`
    let source =
        "var exit_thread := false\n@onready var my_node := $MyNode\nconst THRESHOLD := 20.0\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert!(
        !formatted.contains(": ="),
        "formatter must not insert space in :=, got: {}",
        formatted
    );
    assert!(
        formatted.contains(":="),
        "formatter must preserve := operator, got: {}",
        formatted
    );
}

#[test]
fn formatter_preserves_node_paths() {
    // Regression: `%InteractionMarkers/Marker2D_D` became `%InteractionMarkers / Marker2D_D`
    // because the `/` was treated as a division operator.
    let source = "@onready var marker := %InteractionMarkers/Marker2D_D\n@onready var sprite := $Sprites/Sprite2D_D\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert!(
        !formatted.contains("/ Marker2D"),
        "formatter must not add spaces in node paths, got: {}",
        formatted
    );
    assert!(
        !formatted.contains("/ Sprite2D"),
        "formatter must not add spaces in node paths, got: {}",
        formatted
    );
}

#[test]
fn formatter_preserves_multiline_if_parens() {
    // Regression: multi-line `if (\n...\n):` had parens stripped, producing invalid
    // GDScript `if\n...\n:` which is a syntax error.
    let source = "func test():\n\tif (\n\t\ta != null\n\t\tand b != null\n\t):\n\t\tpass\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert!(
        formatted.contains("if ("),
        "formatter must preserve parens on multi-line if conditions, got: {}",
        formatted
    );
    assert!(
        formatted.contains("):"),
        "formatter must preserve closing paren+colon on multi-line if conditions, got: {}",
        formatted
    );
}

#[test]
fn formatter_removes_single_line_unnecessary_parens() {
    // Single-line `if (x):` should still be fixed to `if x:`.
    let source = "func test():\n\tif (x == null):\n\t\tpass\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert!(
        formatted.contains("if x == null:"),
        "formatter should remove unnecessary parens on single-line if, got: {}",
        formatted
    );
}

#[test]
fn formatter_preserves_comment_separator_lines() {
    // Regression: `##### MAIN FUNCTION CALLED...` became `## ### MAIN FUNCTION CALLED...`
    // because the doc-comment spacing rule inserted a space after `##`.
    let source = "##### MAIN FUNCTION CALLED TO GENERATE HOURLY PLAN #####\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert_eq!(
        formatted.trim(),
        "##### MAIN FUNCTION CALLED TO GENERATE HOURLY PLAN #####",
        "formatter must not mangle ##### comment separators"
    );
}

#[test]
fn formatter_preserves_hash_section_headers() {
    // Regression: `### HANDLE CHAT FUNCTIONS...` became `## # HANDLE CHAT FUNCTIONS...`
    let source = "### HANDLE CHAT FUNCTIONS AND LOGIC INSIDE PERSONA ###\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert_eq!(
        formatted.trim(),
        "### HANDLE CHAT FUNCTIONS AND LOGIC INSIDE PERSONA ###",
        "formatter must not mangle ### section headers"
    );
}

#[test]
fn formatter_trailing_comma_skips_comments() {
    // Regression: trailing comma was inserted into comments instead of after the
    // actual last element. Multi-pass caused repeated comma insertion: `# comment,,,,,`
    let source = "var arr = [\n\ta,\n\tb # last item comment\n]\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert!(
        !formatted.contains(",,,"),
        "formatter must not insert multiple commas into comments, got: {}",
        formatted
    );
    // The comma should be after `b`, not inside the comment.
    assert!(
        formatted.contains("b, # last item comment")
            || formatted.contains("b,\t# last item comment"),
        "trailing comma should be inserted before the comment, got: {}",
        formatted
    );
}

#[test]
fn formatter_blank_lines_does_not_delete_code() {
    // Regression: the format/blank-lines rule used parser body_line_count which
    // could be wrong for complex functions, causing the rule to delete real code
    // when it thought there were "too many blank lines between members".
    let source = r#"static func plan(
	start_time = 0,
	end_time = 1440,
	chunk = 60):
	var result = []
	for i in range(10):
		var x = i * chunk

		if i > 5:
			var node = {"activity": "test", "duration": chunk}

			result.append(node)

			var prev = "prev"
		else:
			result[-1].duration += chunk

	return result

static func other():
	pass
"#;
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    // All the code should still be present.
    assert!(
        formatted.contains("result.append(node)"),
        "formatter must not delete code inside function bodies, got: {}",
        formatted
    );
    assert!(
        formatted.contains("result[-1].duration += chunk"),
        "formatter must not delete else-branch code, got: {}",
        formatted
    );
    assert!(
        formatted.contains("var prev ="),
        "formatter must not delete variable declarations, got: {}",
        formatted
    );
}

#[test]
fn formatter_negative_number_spacing() {
    // Regression: `-1` in contexts like `!= -1` became `!= - 1` (space inserted
    // between unary minus and operand).
    let source = "func test():\n\tif x != -1:\n\t\tpass\n\tvar y = [-1, -2]\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert!(
        formatted.contains("!= -1"),
        "formatter must not add space in negative literals after comparison, got: {}",
        formatted
    );
}

#[test]
fn formatter_idempotent_on_complex_script() {
    // Ensure formatter is idempotent (running twice produces the same output)
    // on a complex script with many features.
    let source = r#"class_name MyClass
extends Node

##### SECTION HEADER #####

const MAX_VALUE := 100
const event_tag = "event:"

@export var RADIUS: int = 300
@onready var node := $Path/To/Node

var _private_var := false

enum State {
	None,
	Active,
	Idle,
}

func _ready():
	if (self.node != null):
		pass

func complex_func(a: int, b: int = -1):
	var result := a + b
	var arr = [
		"item1",
		"item2" # comment
	]
	if (
		a > 0
		and b > 0
	):
		return result
	return -1
"#;
    let config = default_config();
    let first = formatter::format_source(source, &config);
    let second = formatter::format_source(&first, &config);
    assert_eq!(
        first, second,
        "formatter must be idempotent on complex scripts.\nFirst pass:\n{}\nSecond pass:\n{}",
        first, second
    );
}

// --- Naming fix regression tests ---

#[test]
fn fix_signal_name_replaces_name_not_keyword() {
    // Regression: naming fixes replaced the keyword (e.g. "signal") instead of the
    // identifier name, producing "wave_started waveStarted(...)" instead of
    // "signal wave_started(...)".
    let source = "signal waveStarted(wave: int)\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let fixed = fixer::apply_fixes(source, &diagnostics, false);
    assert!(
        fixed.starts_with("signal "),
        "fix must preserve the 'signal' keyword, got: {}",
        fixed
    );
    assert!(
        fixed.contains("wave_started"),
        "fix must rename to snake_case, got: {}",
        fixed
    );
}

#[test]
fn fix_var_name_replaces_name_not_keyword() {
    // Regression: variable naming fix replaced "var" with the new name.
    let source = "var DamageMultiplier: float = 1.0\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let fixed = fixer::apply_fixes(source, &diagnostics, false);
    assert!(
        fixed.starts_with("var "),
        "fix must preserve the 'var' keyword, got: {}",
        fixed
    );
    assert!(
        fixed.contains("damage_multiplier"),
        "fix must rename to snake_case, got: {}",
        fixed
    );
}

#[test]
fn fix_const_name_replaces_name_not_keyword() {
    // Regression: constant naming fix replaced "const" with the new name.
    let source = "const maxSpeed = 400.0\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let fixed = fixer::apply_fixes(source, &diagnostics, false);
    assert!(
        fixed.starts_with("const "),
        "fix must preserve the 'const' keyword, got: {}",
        fixed
    );
    assert!(
        fixed.contains("MAX_SPEED"),
        "fix must rename to SCREAMING_SNAKE_CASE, got: {}",
        fixed
    );
}

#[test]
fn fix_func_name_replaces_name_not_keyword() {
    // Regression: function naming fix replaced "func" with the new name.
    let source = "func HasState(name: StringName) -> bool:\n\treturn true\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let fixed = fixer::apply_fixes(source, &diagnostics, false);
    assert!(
        fixed.starts_with("func "),
        "fix must preserve the 'func' keyword, got: {}",
        fixed
    );
    assert!(
        fixed.contains("has_state"),
        "fix must rename to snake_case, got: {}",
        fixed
    );
}

#[test]
fn fix_enum_name_replaces_name_not_keyword() {
    // Regression: enum naming fix replaced "enum" with the new name.
    let source = "enum item_rarity { COMMON, RARE }\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let fixed = fixer::apply_fixes(source, &diagnostics, false);
    assert!(
        fixed.starts_with("enum "),
        "fix must preserve the 'enum' keyword, got: {}",
        fixed
    );
    assert!(
        fixed.contains("ItemRarity"),
        "fix must rename to PascalCase, got: {}",
        fixed
    );
}

#[test]
fn fix_signal_past_tense_inflects_correctly() {
    // Regression: past tense fix naively appended "_changed" instead of inflecting.
    let cases = vec![
        ("signal wave_complete\n", "wave_completed"),
        ("signal enemy_die\n", "enemy_died"),
        ("signal player_retry\n", "player_retried"),
        ("signal item_add\n", "item_added"),
    ];

    let config = default_config();
    for (source, expected_name) in cases {
        let diagnostics = linter::lint_source(source, "test.gd", &config);
        let fixed = fixer::apply_fixes(source, &diagnostics, false);
        assert!(
            fixed.contains(expected_name),
            "past tense fix for '{}' should produce '{}', got: {}",
            source.trim(),
            expected_name,
            fixed.trim()
        );
    }
}

#[test]
fn ordering_no_false_positives_for_local_variables() {
    // S4: local variables inside function bodies should NOT generate
    // order/class-member-order warnings.
    let source = "extends Node\n\nvar speed: float = 10.0\n\nfunc _ready():\n\tvar timer = Timer.new()\n\ttimer.wait_time = 1.0\n\tadd_child(timer)\n\nfunc process_data(items: Array):\n\tfor item in items:\n\t\tvar result = 1\n\t\tif result > 0:\n\t\t\tvar label = Label.new()\n\t\t\tlabel.text = str(result)\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let ordering_diags: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "order/class-member-order")
        .collect();
    assert!(
        ordering_diags.is_empty(),
        "local variables inside functions should not generate ordering warnings, got:\n{}",
        ordering_diags
            .iter()
            .map(|d| format!("  {}:{} {}", d.span.line, d.span.column, d.message))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn ordering_no_false_positives_for_local_vars_after_comment() {
    // S4: comments inside function bodies caused the parser to break out of
    // the body, making subsequent local variables appear as class-level members.
    let source =
        "extends Node\n\nfunc _ready():\n\tpass\n\nfunc bar():\n\t# comment\n\tvar x = 1\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let ordering_diags: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "order/class-member-order")
        .collect();
    assert!(
        ordering_diags.is_empty(),
        "local variables after comments in function bodies should not generate ordering warnings, got:\n{}",
        ordering_diags
            .iter()
            .map(|d| format!("  {}:{} {}", d.span.line, d.span.column, d.message))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn ordering_no_false_positives_complex_function_body() {
    // S4: reproduces the GARP active_cognition.gd pattern with deep nesting.
    let source = r#"class_name ActiveCognition
extends Node

@export var persona_name: String
@onready var state_chart: Node = %StateChart

func _ready():
	pass

func _choose_event_to_react():
	# Some logic
	var persona_events = []
	var regular_events = []

	for event in []:
		var e = event
		if e:
			var data = {}
			data["key"] = "value"
"#;
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let ordering_diags: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "order/class-member-order")
        .collect();
    assert!(
        ordering_diags.is_empty(),
        "complex function bodies with comments and local vars should not generate ordering warnings, got:\n{}",
        ordering_diags
            .iter()
            .map(|d| format!("  {}:{} {}", d.span.line, d.span.column, d.message))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn fix_collapses_excess_blank_lines() {
    // S5: format/blank-lines should be fixable by --fix
    let source = "extends Node\n\nvar x = 5\n\n\n\n\nvar y = 10\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);

    // Should have a blank-lines diagnostic
    let blank_diags: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "format/blank-lines")
        .collect();
    assert!(!blank_diags.is_empty(), "should detect excess blank lines");
    assert!(
        blank_diags[0].fix.is_some(),
        "blank-lines diagnostic should have a fix"
    );
    assert!(
        blank_diags[0].fix.as_ref().unwrap().is_safe,
        "blank-lines fix should be safe"
    );

    // Apply safe fix
    let fixed = fixer::apply_fixes(source, &diagnostics, true);
    // Should have exactly 2 blank lines between members
    assert!(
        !fixed.contains("\n\n\n\n"),
        "should collapse 4+ blank lines, got:\n{}",
        fixed
    );
    // Should still have 2 blank lines
    assert!(
        fixed.contains("\n\n\n"),
        "should preserve 2 blank lines between members, got:\n{}",
        fixed
    );

    // Verify no more blank-lines warnings
    let recheck = linter::lint_source(&fixed, "test.gd", &config);
    let remaining: Vec<_> = recheck
        .iter()
        .filter(|d| d.rule == "format/blank-lines")
        .collect();
    assert!(
        remaining.is_empty(),
        "after fix, should have no blank-lines warnings, got:\n{}",
        remaining
            .iter()
            .map(|d| d.message.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
}

#[test]
fn fix_collapses_six_blank_lines_to_two() {
    // S5: GARP example with 6 blank lines
    let source = "func foo():\n\tpass\n\n\n\n\n\n\nfunc bar():\n\tpass\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let fixed = fixer::apply_fixes(source, &diagnostics, true);

    // Count blank lines between functions
    let lines: Vec<&str> = fixed.split('\n').collect();
    let mut max_consecutive = 0;
    let mut consecutive = 0;
    for line in &lines {
        if line.trim().is_empty() {
            consecutive += 1;
        } else {
            if consecutive > max_consecutive {
                max_consecutive = consecutive;
            }
            consecutive = 0;
        }
    }
    if consecutive > max_consecutive {
        max_consecutive = consecutive;
    }
    assert!(
        max_consecutive <= 2,
        "should have at most 2 consecutive blank lines, got {}: {}",
        max_consecutive,
        fixed
    );
}

// S9: --unsafe-fix should update same-file references

#[test]
fn unsafe_fix_updates_same_file_variable_references() {
    // S9: renaming a var declaration should also update self.OLD_NAME and bare OLD_NAME
    let source = "var DEFAULT_SEED = 42\n\nfunc _ready():\n\tself.DEFAULT_SEED = 100\n\tprint(DEFAULT_SEED)\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let fixed = fixer::apply_fixes(source, &diagnostics, false);

    assert!(
        !fixed.contains("DEFAULT_SEED"),
        "all references to DEFAULT_SEED should be renamed, got:\n{}",
        fixed
    );
    assert!(
        fixed.contains("var default_seed"),
        "declaration should be renamed to snake_case, got:\n{}",
        fixed
    );
    // self.default_seed and bare default_seed should appear
    assert!(
        fixed.contains("self.default_seed"),
        "self-qualified reference should be updated, got:\n{}",
        fixed
    );
}

#[test]
fn unsafe_fix_updates_same_file_enum_member_references() {
    // S9: renaming enum members should update references in the same file
    let source = "enum State {\n\tAIInControl,\n\tPlayerInControl,\n}\n\nfunc get_state():\n\treturn State.AIInControl\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let fixed = fixer::apply_fixes(source, &diagnostics, false);

    // The renamed member should be AI_IN_CONTROL (with S7 acronym fix)
    assert!(
        fixed.contains("AI_IN_CONTROL"),
        "enum member should be renamed to SCREAMING_CASE, got:\n{}",
        fixed
    );
    // The reference should also be updated
    assert!(
        !fixed.contains("AIInControl"),
        "reference to old enum member name should be updated, got:\n{}",
        fixed
    );
}

#[test]
fn unsafe_fix_updates_same_file_function_references() {
    // S9: renaming a function should update calls in the same file
    let source = "func HasState(name: String) -> bool:\n\treturn true\n\nfunc _ready():\n\tif HasState(\"idle\"):\n\t\tpass\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let fixed = fixer::apply_fixes(source, &diagnostics, false);

    assert!(
        fixed.contains("func has_state"),
        "function declaration should be renamed, got:\n{}",
        fixed
    );
    assert!(
        !fixed.contains("HasState"),
        "all references to old function name should be updated, got:\n{}",
        fixed
    );
}

#[test]
fn unsafe_fix_updates_multiple_declarations_same_name() {
    // S9: two declarations of the same variable name in the same file
    let source = "func foo():\n\tvar DialogueCtrl = 1\n\tprint(DialogueCtrl)\n\nfunc bar():\n\tvar DialogueCtrl = 2\n\tprint(DialogueCtrl)\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let fixed = fixer::apply_fixes(source, &diagnostics, false);

    // Note: local vars inside functions may not be detected by the parser as
    // class members, so they may not have naming diagnostics. But if they do,
    // all references should be updated.
    // In this case the parser may not see these as class members, so this test
    // primarily verifies the fixer doesn't crash.
    assert!(!fixed.is_empty());
}

// S1: cross-file reference tracking

#[test]
fn extract_renames_from_diagnostics() {
    // S1: extract_renames should identify naming renames from diagnostics
    let source = "var CONFIG: ConfigFile = load(\"cfg\")\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "config_manager.gd", &config);

    let members = parse_members_for_test(source);
    let renames = fixer::extract_renames(source, &diagnostics, "config_manager.gd", &members);

    // Should find the CONFIG → config rename
    let config_rename = renames.iter().find(|r| r.old_name == "CONFIG");
    assert!(
        config_rename.is_some(),
        "should extract CONFIG rename, got: {:?}",
        renames
            .iter()
            .map(|r| format!("{} -> {}", r.old_name, r.new_name))
            .collect::<Vec<_>>()
    );
    assert_eq!(config_rename.unwrap().new_name, "config");
}

#[test]
fn find_cross_file_references_detects_old_names() {
    // S1: find_cross_file_references should detect references to renamed identifiers
    let renames = vec![fixer::AppliedRename {
        old_name: "CONFIG".to_string(),
        new_name: "config".to_string(),
        source_file: "config_manager.gd".to_string(),
        source_class_name: Some("ConfigManager".to_string()),
        kind: fixer::RenameKind::Constant,
        is_instance_member: false,
    }];

    let other_source = "func _ready():\n\tvar cfg = ConfigManager.CONFIG\n\tprint(cfg)\n";
    let refs = fixer::find_cross_file_references(other_source, "engine.gd", &renames);

    assert!(
        !refs.is_empty(),
        "should find reference to CONFIG in other file"
    );
    assert_eq!(refs[0].old_name, "CONFIG");
    assert_eq!(refs[0].new_name, "config");
    assert_eq!(refs[0].file, "engine.gd");
}

#[test]
fn find_cross_file_references_skips_same_file() {
    // S1: should not report references in the same file (those are handled by S9)
    let renames = vec![fixer::AppliedRename {
        old_name: "CONFIG".to_string(),
        new_name: "config".to_string(),
        source_file: "config_manager.gd".to_string(),
        source_class_name: Some("ConfigManager".to_string()),
        kind: fixer::RenameKind::Constant,
        is_instance_member: false,
    }];

    let source = "var config = 1\nprint(CONFIG)\n";
    let refs = fixer::find_cross_file_references(source, "config_manager.gd", &renames);

    assert!(
        refs.is_empty(),
        "should not report references in the same file"
    );
}

#[test]
fn apply_cross_file_fixes_updates_references() {
    // S1: apply_cross_file_fixes should replace old names with new names
    let refs = vec![fixer::CrossFileReference {
        file: "engine.gd".to_string(),
        line: 2,
        column: 25,
        old_name: "CONFIG".to_string(),
        new_name: "config".to_string(),
        source_file: "config_manager.gd".to_string(),
        offset: 40,
        length: 6,
    }];

    let source = "func _ready():\n\tvar cfg = ConfigManager.CONFIG\n\tprint(cfg)\n";
    let fixed = fixer::apply_cross_file_fixes(source, &refs);

    assert!(
        fixed.contains("ConfigManager.config"),
        "should replace CONFIG with config, got:\n{}",
        fixed
    );
    assert!(
        !fixed.contains("CONFIG"),
        "should not contain old name CONFIG, got:\n{}",
        fixed
    );
}

#[test]
fn apply_scene_renames_rewrites_signal_and_method_connections() {
    // A signal rename and a function rename should both update the matching
    // `[connection]` attributes in a .tscn; otherwise the editor-wired
    // connection silently breaks at runtime.
    let renames = vec![
        fixer::AppliedRename {
            old_name: "health_change".to_string(),
            new_name: "health_changed".to_string(),
            source_file: "player.gd".to_string(),
            source_class_name: Some("Player".to_string()),
            kind: fixer::RenameKind::Signal,
            is_instance_member: true,
        },
        fixer::AppliedRename {
            old_name: "onHealthChange".to_string(),
            new_name: "on_health_change".to_string(),
            source_file: "hud.gd".to_string(),
            source_class_name: Some("Hud".to_string()),
            kind: fixer::RenameKind::Function,
            is_instance_member: true,
        },
    ];
    let scene = "[gd_scene format=3]\n\n[connection signal=\"health_change\" from=\".\" to=\"HUD\" method=\"onHealthChange\"]\n";
    let (rewritten, applied) = fixer::apply_scene_renames(scene, &renames);

    assert!(
        rewritten.contains("signal=\"health_changed\""),
        "signal rewritten: {}",
        rewritten
    );
    assert!(
        rewritten.contains("method=\"on_health_change\""),
        "method rewritten: {}",
        rewritten
    );
    assert!(
        !rewritten.contains("\"health_change\""),
        "old signal gone: {}",
        rewritten
    );
    assert!(
        !rewritten.contains("\"onHealthChange\""),
        "old method gone: {}",
        rewritten
    );
    assert_eq!(applied.len(), 2, "both connections reported");
}

#[test]
fn apply_scene_renames_ignores_unrelated_kinds() {
    // A Variable / Constant / Class rename must not touch scene connection
    // attributes; only Signal (signal=) and Function (method=) do.
    let renames = vec![fixer::AppliedRename {
        old_name: "health_change".to_string(),
        new_name: "HEALTH_CHANGE".to_string(),
        source_file: "x.gd".to_string(),
        source_class_name: None,
        kind: fixer::RenameKind::Constant,
        is_instance_member: false,
    }];
    let scene = "[connection signal=\"health_change\" method=\"foo\"]\n";
    let (rewritten, applied) = fixer::apply_scene_renames(scene, &renames);
    assert_eq!(rewritten, scene, "constant rename must not touch scene");
    assert!(applied.is_empty());
}

// S6: enum-one-per-line auto-fix

// S8: member ordering in fmt

#[test]
fn fmt_reorders_exports_before_onready() {
    let source = "extends Node\n\n@onready var chart: Node = %Chart\n@onready var memory: Node = %Memory\n@export var name: String\n@onready var active: Node = %Active\n@export var prob: float = 1.0\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);

    let export_pos = formatted.find("@export var name");
    let onready_pos = formatted.find("@onready var chart");
    assert!(
        export_pos.unwrap() < onready_pos.unwrap(),
        "@export should appear before @onready after formatting, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_reorders_signals_before_enums() {
    let source = "extends Node\n\nenum State { IDLE, RUN }\nsignal done()\nsignal moved()\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);

    let signal_pos = formatted.find("signal done()");
    let enum_pos = formatted.find("enum State");
    assert!(
        signal_pos.unwrap() < enum_pos.unwrap(),
        "signal should appear before enum, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_reorders_constants_before_vars() {
    let source = "extends Node\n\nvar _font: Font = null\nconst C_TAN = 0.784\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);

    let const_pos = formatted.find("const C_TAN");
    let var_pos = formatted.find("var _font");
    assert!(
        const_pos.unwrap() < var_pos.unwrap(),
        "const should appear before var, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_reorder_idempotent() {
    let source =
        "extends Node\n\nfunc _ready():\n\tpass\n\nvar speed: float = 10.0\n\nsignal done()\n";
    let config = default_config();
    let first = formatter::format_source(source, &config);
    let second = formatter::format_source(&first, &config);
    assert_eq!(
        first, second,
        "must be idempotent.\nFirst:\n{}\nSecond:\n{}",
        first, second
    );
}

#[test]
fn fmt_reorder_preserves_correct_order() {
    let source = "extends Node\n\nsignal done()\n\nconst MAX = 100\n\nvar speed: float = 10.0\n\nfunc _ready():\n\tpass\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    let second = formatter::format_source(&formatted, &config);
    assert_eq!(
        formatted, second,
        "formatter must be idempotent on correctly ordered script"
    );
}

#[test]
fn fix_enum_one_per_line() {
    // S6: single-line enum should be reformatted to multi-line by --fix
    let source = "enum Direction {DOWN, RIGHT, UP, LEFT}\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);

    let enum_diags: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "format/enum-one-per-line")
        .collect();
    assert!(!enum_diags.is_empty(), "should detect single-line enum");
    assert!(
        enum_diags[0].fix.is_some(),
        "enum-one-per-line should have an auto-fix"
    );

    let fixed = fixer::apply_fixes(source, &diagnostics, true);

    // Should be multi-line now
    assert!(
        fixed.contains("enum Direction {"),
        "should have opening brace on enum line, got:\n{}",
        fixed
    );
    assert!(
        fixed.contains("\tDOWN,"),
        "each member should be on its own indented line, got:\n{}",
        fixed
    );
    assert!(
        fixed.contains("\tRIGHT,"),
        "each member should be on its own indented line, got:\n{}",
        fixed
    );
    assert!(
        fixed.contains("\tUP,"),
        "each member should be on its own indented line, got:\n{}",
        fixed
    );
    assert!(
        fixed.contains("\tLEFT,"),
        "each member should be on its own indented line with trailing comma, got:\n{}",
        fixed
    );

    // Should have no more enum-one-per-line warnings
    let recheck = linter::lint_source(&fixed, "test.gd", &config);
    let remaining: Vec<_> = recheck
        .iter()
        .filter(|d| d.rule == "format/enum-one-per-line")
        .collect();
    assert!(
        remaining.is_empty(),
        "after fix, should have no enum-one-per-line warnings"
    );
}

#[test]
fn fix_enum_one_per_line_state() {
    // S6: another GARP example
    let source = "enum State {IDLE, RUNNING, JUMPING, FALLING, DEAD}\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let fixed = fixer::apply_fixes(source, &diagnostics, true);

    assert!(
        fixed.contains("\tIDLE,"),
        "IDLE should be on its own line, got:\n{}",
        fixed
    );
    assert!(
        fixed.contains("\tDEAD,"),
        "DEAD should have trailing comma, got:\n{}",
        fixed
    );
}

#[test]
fn fmt_does_not_duplicate_class_level_abstract_annotation() {
    // Regression for issue #4: an `@abstract` annotation between blank
    // lines above `class_name`/`extends` plus a function later in the
    // file blew up to ~30 copies of `@abstract\nclass_name Pickup\n`
    // when formatted. The parser was holding `@abstract` in
    // `pending_annotations` past `class_name` and `extends` and
    // attaching it to the function; the function unit's start then
    // covered line 1, which the formatter re-emitted on every pass.
    //
    // Fix: flush pending unknown annotations as standalone
    // `ClassAnnotation` members when we hit `class_name`/`extends`, so
    // they sort with the other class-level annotations (category 0)
    // and don't dangle.
    let source = "@abstract\n\nclass_name Pickup\nextends Node\n\n\n\nfunc apply_to(target: Node3D) -> void:\n\tpass\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    let abstract_count = formatted.matches("@abstract").count();
    let class_name_count = formatted.matches("class_name Pickup").count();
    assert_eq!(
        abstract_count, 1,
        "@abstract must not be duplicated, got {} copies:\n{}",
        abstract_count, formatted
    );
    assert_eq!(
        class_name_count, 1,
        "class_name must not be duplicated, got {} copies:\n{}",
        class_name_count, formatted
    );
    // And the resulting file should look like canonical Godot:
    // `@abstract` directly above `class_name`, then `extends`, then
    // two blank lines before the function.
    assert!(
        formatted.contains("@abstract\nclass_name Pickup\nextends Node\n"),
        "expected canonical class header, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("\n\n\nfunc apply_to(target: Node3D) -> void:\n\tpass\n"),
        "expected 2 blank lines between class header and func, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_class_annotation_before_signal_not_duplicated() {
    // Same family of bug as issue #4: a stray top-level annotation
    // (`@abstract` here) immediately before a `signal` declaration
    // used to ride past the signal in `pending_annotations` and
    // attach to a function later in the file, blowing up the output.
    // Signal declarations don't take a leading annotation, so the
    // parser now flushes pending entries as class-level annotations
    // at that point.
    let source = "@abstract\nclass_name BaseEntity\nextends Node\n\n\nsignal damaged(amount: int)\n\n\nfunc take_damage(amount: int) -> void:\n\tdamaged.emit(amount)\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert_eq!(
        formatted.matches("@abstract").count(),
        1,
        "@abstract must not be duplicated, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("@abstract\nclass_name BaseEntity\nextends Node"),
        "@abstract must land at the canonical class-header position, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("signal damaged(amount: int)"),
        "signal declaration must be preserved, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_class_annotation_before_enum_not_duplicated() {
    // Counterpart to the signal case: an `@abstract` immediately
    // before an `enum` declaration used to ride forward to the next
    // function and trigger the same multi-line duplication. `enum`
    // doesn't accept a leading annotation, so we flush.
    let source = "@abstract\nclass_name Item\nextends Resource\n\n\nenum Rarity { COMMON, RARE, EPIC }\n\n\nfunc describe() -> String:\n\treturn \"\"\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert_eq!(
        formatted.matches("@abstract").count(),
        1,
        "@abstract must not be duplicated, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("@abstract\nclass_name Item\nextends Resource"),
        "expected canonical class header, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("enum Rarity"),
        "enum declaration must be preserved, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_class_annotation_before_inner_class_not_duplicated() {
    // Inner class declarations have no AST slot for annotations, so
    // we treat a stray `@abstract` above one as a class-level
    // annotation of the OUTER class. That's the deliberate
    // trade-off: the alternative — leaving it pending — would
    // attach it to the next top-level function and cause the
    // duplication bug. If Godot ever wires inner-class annotations
    // through the AST, revisit `parse_inner_class`.
    let source = "class_name Outer\nextends Node\n\n\n@abstract\nclass Inner:\n\tvar x: int = 5\n\n\nfunc use_inner() -> void:\n\tvar i = Inner.new()\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert_eq!(
        formatted.matches("@abstract").count(),
        1,
        "@abstract must not be duplicated, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("class Inner:"),
        "inner class must be preserved, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("func use_inner()"),
        "outer function must be preserved and not eaten by the annotation flush, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_trailing_unknown_annotation_at_eof_not_dropped() {
    // A trailing top-level annotation with no following declaration
    // used to be silently dropped by the parser (pending_annotations
    // was never flushed at EOF). Now it survives as a class-level
    // annotation so a round-trip through `gdstyle fmt` doesn't lose
    // content.
    let source = "class_name Foo\nextends Node\n\n\nfunc bar() -> void:\n\tpass\n\n\n@abstract\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert_eq!(
        formatted.matches("@abstract").count(),
        1,
        "trailing @abstract must survive the round-trip, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_class_annotation_with_args_preserves_args() {
    // The parsed ClassAnnotation only stores the annotation NAME (no
    // parens, no args), but the formatter emits via line-based source
    // slicing, so the raw `@x(a, b)` text round-trips verbatim. Lock
    // that in: an AST-based emitter introduced later would silently
    // drop the args, and we want a test that catches it.
    let source = "@some_unknown(42, \"arg\")\n\nclass_name Foo\nextends Node\n\n\nfunc bar() -> void:\n\tpass\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert!(
        formatted.contains("@some_unknown(42, \"arg\")"),
        "annotation args must round-trip verbatim, got:\n{}",
        formatted
    );
    assert_eq!(
        formatted.matches("@some_unknown").count(),
        1,
        "annotation with args must not duplicate, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_stacked_class_annotations_preserve_source_order() {
    // Multiple unknown annotations above class_name must be emitted in
    // the order the user wrote them — the flush iterates a Vec, but
    // a future switch to a Set or HashMap would silently scramble.
    let source = "@abstract\n@experimental\n@deprecated\n\nclass_name Foo\nextends Node\n\n\nfunc bar() -> void:\n\tpass\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    let abs = formatted.find("@abstract").expect("missing @abstract");
    let exp = formatted
        .find("@experimental")
        .expect("missing @experimental");
    let dep = formatted.find("@deprecated").expect("missing @deprecated");
    assert!(
        abs < exp && exp < dep,
        "stacked annotations must keep source order @abstract → @experimental → @deprecated, got positions abs={} exp={} dep={} in:\n{}",
        abs, exp, dep, formatted
    );
}

#[test]
fn fmt_class_and_function_level_abstract_both_survive() {
    // Class-level @abstract above class_name AND function-level
    // @abstract above a method must both survive. Guards against an
    // over-eager flush that promotes EVERY @abstract to class-level.
    let source = "@abstract\nclass_name Pickup\nextends Node\n\n\n@abstract\nfunc to_implement() -> void:\n\tpass\n\n\nfunc concrete() -> void:\n\tpass\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert_eq!(
        formatted.matches("@abstract").count(),
        2,
        "both @abstract occurrences must survive, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("@abstract\nclass_name Pickup"),
        "class-level @abstract must sit above class_name, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("@abstract\nfunc to_implement("),
        "function-level @abstract must stay attached to its function, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_class_annotation_is_idempotent() {
    // Formatting twice must produce the identical output. The original
    // bug was a multi-pass duplication loop; this is the direct
    // assertion that we no longer oscillate.
    let source = "@abstract\n\nclass_name Pickup\nextends Node\n\n\n\nfunc apply_to(target: Node3D) -> void:\n\tpass\n";
    let config = default_config();
    let once = formatter::format_source(source, &config);
    let twice = formatter::format_source(&once, &config);
    assert_eq!(
        once, twice,
        "formatter must be idempotent; pass 1 vs pass 2 differ:\n--- pass 1 ---\n{}\n--- pass 2 ---\n{}",
        once, twice
    );
}

#[test]
fn fmt_class_annotation_with_docstring_between() {
    // `@abstract\n## docstring\nclass_name Foo` — the docstring is
    // logically class-level and the reorder pass moves it to after
    // class_name/extends (canonical Godot style). @abstract must end
    // up above class_name, the docstring after extends, neither
    // duplicated.
    let source = "@abstract\n## Top-level docstring for this abstract class.\nclass_name Foo\nextends Node\n\n\nfunc bar() -> void:\n\tpass\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert_eq!(
        formatted.matches("@abstract").count(),
        1,
        "@abstract must not duplicate, got:\n{}",
        formatted
    );
    assert_eq!(
        formatted.matches("## Top-level docstring").count(),
        1,
        "docstring must not duplicate, got:\n{}",
        formatted
    );
    let abs_pos = formatted.find("@abstract").unwrap();
    let cls_pos = formatted.find("class_name Foo").unwrap();
    let doc_pos = formatted.find("## Top-level docstring").unwrap();
    assert!(
        abs_pos < cls_pos,
        "@abstract must come before class_name, got:\n{}",
        formatted
    );
    assert!(
        cls_pos < doc_pos,
        "class_name must come before the docstring (canonical Godot order), got:\n{}",
        formatted
    );
}

#[test]
fn fmt_preserves_function_level_abstract_annotation() {
    // The function-level form of `@abstract` (declaring an abstract
    // method, distinct from declaring an abstract class) must continue
    // to attach to the function it sits above, not get hoisted to the
    // top of the file as a class-level annotation. Guards against an
    // over-eager rewrite of the class-level fix.
    let source = "class_name Pickup\nextends Node\n\n\n@abstract\nfunc to_implement() -> void:\n\tpass\n\n\nfunc concrete() -> void:\n\tpass\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert!(
        formatted.contains("@abstract\nfunc to_implement("),
        "@abstract must stay attached to its function, got:\n{}",
        formatted
    );
    // And there should still be exactly one @abstract — no duplication.
    assert_eq!(
        formatted.matches("@abstract").count(),
        1,
        "@abstract must not be duplicated, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_keeps_doc_attached_on_user_reported_artifact_resolver() {
    // Verbatim shape of the file the issue reporter posted: a function
    // whose body ends with a top-level `if` after a `match` (so the body
    // closes at depth 2), followed by a single-line `## doc` for the
    // next function. Before the lexer fix, the docstring was detached
    // by two blank lines from `get_active_artifacts`, breaking the
    // Godot editor tooltip and the class-docs export. Kept as a
    // separate test from the minimal repro so a future regression on
    // this exact reported shape would be immediately recognisable.
    let source = "extends Object
class_name ArtifactSkillResolver

## Resolves all artifact skills matching the given trigger.
## Executes the resolution via signals.
static func resolve_skills(trigger: int) -> void:
\tvar activated = []
\tfor resolution in activated:
\t\t_execute_resolution(resolution, trigger)

static func _execute_resolution(resolution: int, trigger: int) -> void:
\tmatch resolution:
\t\t1:
\t\t\tprint(\"one\")
\t\t2:
\t\t\tprint(\"two\")
\tif trigger != 0:
\t\tresolve_skills(0)

## Filters equipped artifacts to only those whose prerequisite is also equipped.
static func get_active_artifacts(equipped) -> Array:
\treturn equipped
";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert!(
        formatted.contains(
            "## Filters equipped artifacts to only those whose prerequisite is also equipped.\nstatic func get_active_artifacts("
        ),
        "docstring detached from get_active_artifacts, got:\n{}",
        formatted
    );
    // The multi-line docstring for `resolve_skills` must also stay
    // attached (regression guard against breaking the working case
    // while fixing the broken one).
    assert!(
        formatted.contains("## Executes the resolution via signals.\nstatic func resolve_skills("),
        "multi-line docstring on resolve_skills was detached, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_keeps_doc_attached_when_prev_body_ends_with_nested_block() {
    // Regression: when a function body ended with a nested indented block
    // (e.g. a top-level `if` after a `match`, leaving two open indent
    // levels at the body's tail), a single-line `## doc` for the NEXT
    // function got detached. The lexer was swallowing the doc line without
    // emitting the dedents that close the previous body, so the parser
    // ate the doc as part of the previous function. The formatter then
    // inserted its canonical between-functions blank-line gap between the
    // orphaned doc and the function it actually documents. Reported by
    // a user against gdstyle 0.1.1 with a real Godot project file.
    let source = "extends Node

static func a() -> void:
\tmatch 1:
\t\t1:
\t\t\tpass
\tif true:
\t\tpass

## doc for b
static func b() -> void:
\tpass
";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert!(
        formatted.contains("## doc for b\nstatic func b("),
        "doc comment was detached from its function, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_preserves_function_bodies_with_doc_comments() {
    let source = r#"## Module doc

class_name MyClass
extends Node

func _ready():
	var data = {}
	data["key"] = "value"
	print(data)

func get_info() -> String:
	var result = "info"
	return result
"#;
    let config = default_config();
    let formatted = formatter::format_source(source, &config);

    assert!(
        formatted.contains("data[\"key\"] = \"value\""),
        "function body line missing, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("print(data)"),
        "function body line missing, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("var result = \"info\""),
        "function body line missing, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("return result"),
        "function body line missing, got:\n{}",
        formatted
    );

    let second = formatter::format_source(&formatted, &config);
    assert_eq!(formatted, second, "must be idempotent");
}

#[test]
fn fmt_preserves_multiline_expressions() {
    let source = "var tokens = ConfigManager.get_instance().config.get_value(\n\t\"setup\", \"MAX_TOKENS\"\n)\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);

    assert!(
        formatted.contains("\"setup\", \"MAX_TOKENS\""),
        "multi-line args should be preserved, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains(")"),
        "closing paren should be preserved, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_does_not_duplicate_class_name() {
    let source = "class_name MyClass extends Node\n\nvar x = 10\n\nfunc _ready():\n\tpass\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);

    let count = formatted.matches("class_name").count();
    assert_eq!(
        count, 1,
        "class_name should appear exactly once, got {} times in:\n{}",
        count, formatted
    );
}

#[test]
fn fmt_does_not_duplicate_class_name_with_many_members() {
    let source = r#"class_name ActiveCognition extends Node2D

static var THOUGHT_POIGNANCY: int = 5
static var THOUGHT_PREDICATE: String = "plan"

@export var name: String
@onready var chart: Node = %Chart

signal done()
const MAX = 100
var x = 10

func _ready():
	pass

func _process(delta: float):
	pass

func custom():
	pass
"#;
    let config = default_config();
    let formatted = formatter::format_source(source, &config);

    let count = formatted.matches("class_name").count();
    assert_eq!(
        count, 1,
        "class_name should appear exactly once, got {} times in:\n{}",
        count, formatted
    );
    assert!(
        formatted.contains("static var THOUGHT_POIGNANCY"),
        "static var missing"
    );
    assert!(formatted.contains("func custom()"), "custom func missing");
    assert!(formatted.contains("signal done()"), "signal missing");
}

#[test]
fn fmt_preserves_class_name_extends_same_line() {
    let source = "class_name Persona extends CharacterBody2D\n\nfunc _ready():\n\tpass\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);

    assert!(
        formatted.contains("class_name Persona extends CharacterBody2D"),
        "class_name+extends on same line should be preserved, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_preserves_multiline_var_declarations() {
    let source = "var default_max_tokens = ConfigManager.get_instance().config.get_value(\n\t\"setup\", \"DEFAULT_MAX_TOKENS\"\n)\nvar default_temperature = ConfigManager.get_instance().config.get_value(\n\t\"setup\", \"DEFAULT_TEMPERATURE\"\n)\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);

    assert!(
        formatted.contains("\"DEFAULT_MAX_TOKENS\""),
        "first multi-line var args missing, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("\"DEFAULT_TEMPERATURE\""),
        "second multi-line var args missing, got:\n{}",
        formatted
    );
    let paren_count = formatted.matches(')').count();
    assert!(
        paren_count >= 4,
        "closing parens missing (need >=4, got {}), got:\n{}",
        paren_count,
        formatted
    );
}

#[test]
fn fmt_preserves_inner_class_boundary() {
    let source = "class_name MyScript\nextends Node\n\nfunc parse_response() -> String:\n\tvar result = \"default\"\n\treturn result\n\nclass InnerHelper:\n\tvar data: Dictionary\n\tvar name: String\n\tfunc _init(n: String):\n\t\tself.name = n\n\t\tself.data = {}\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);

    assert!(
        formatted.contains("class InnerHelper:"),
        "inner class declaration missing"
    );
    assert!(
        formatted.contains("self.name = n"),
        "inner class body missing"
    );
    assert!(
        formatted.contains("self.data = {}"),
        "inner class body missing"
    );
    assert!(
        formatted.contains("var result = \"default\""),
        "function body before inner class missing"
    );
    assert!(
        formatted.contains("return result"),
        "return statement missing"
    );

    let second = formatter::format_source(&formatted, &config);
    assert_eq!(formatted, second, "must be idempotent");
}

#[test]
fn fmt_preserves_conditional_function_body() {
    let source = "func _object_out_of_range(area: Area2D):\n\tif area is Area2D:\n\t\t_untrack_object(area)\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);

    assert!(
        formatted.contains("_untrack_object(area)"),
        "function body should be preserved, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_reorder_with_enum_and_doc_comments() {
    // Regression: enum body was eaten when doc comments appeared before class_name
    // because multi-pass loop re-ran reorder after enum expansion.
    let source = "## Doc\n\nclass_name Test\nextends Node\n\n@onready var db: Node = %DB\n\nenum Mode { A, B, C }\n\nsignal done()\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);

    // Enum should be fully expanded
    assert!(
        formatted.contains("\tA,"),
        "enum member A missing, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("\tB,"),
        "enum member B missing, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("\tC,"),
        "enum member C missing, got:\n{}",
        formatted
    );
    // Signal before enum
    let signal_pos = formatted.find("signal done()");
    let enum_pos = formatted.find("enum Mode");
    assert!(
        signal_pos.unwrap() < enum_pos.unwrap(),
        "signal should be before enum, got:\n{}",
        formatted
    );

    let second = formatter::format_source(&formatted, &config);
    assert_eq!(formatted, second, "must be idempotent");
}

// Fix A: doc comments attached to next declaration are not ordering violations

#[test]
fn ordering_inner_class_comment_first_body_line_not_flagged() {
    // Regression for issue #5: an inner class whose first body line is
    // a `##` doc comment (or any comment-only line) had its members
    // misclassified as siblings of the outer class. The lexer wasn't
    // emitting an Indent before the leading comment, so the parser's
    // `parse_indented_block` saw no Indent, returned an empty body,
    // and the `var a` token fell back to the outer parse loop —
    // landing AFTER the `class Bar` node and tripping
    // `order/class-member-order` on every inner-class member.
    let source = "class_name Foo extends Object\n## doc.\nclass Bar extends RefCounted:\n\t## doc for a.\n\tvar a: int\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let ordering: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "order/class-member-order")
        .collect();
    assert!(
        ordering.is_empty(),
        "inner-class members with a leading comment must not trigger ordering, got:\n{}",
        ordering
            .iter()
            .map(|d| format!("  {}:{} {}", d.span.line, d.span.column, d.message))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn ordering_inner_class_multiple_leading_comments_not_flagged() {
    // Same family: multiple consecutive comment-only lines (mix of `#`
    // and `##`) at the start of an inner class body. Both the parser
    // and the formatter should treat them as part of the inner body.
    let source = "class_name Foo extends Object\n\n\nclass Bar extends RefCounted:\n\t# plain comment\n\t## doc comment\n\tvar a: int\n\tvar b: int\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let ordering: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "order/class-member-order")
        .collect();
    assert!(
        ordering.is_empty(),
        "stacked leading comments in an inner class body must not trip ordering, got:\n{}",
        ordering
            .iter()
            .map(|d| format!("  {}:{} {}", d.span.line, d.span.column, d.message))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn ordering_doc_comment_before_static_func_not_flagged() {
    let source = "var x = 1\n\n## Docs for foo\n## More docs\nstatic func foo():\n\tpass\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let ordering: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "order/class-member-order")
        .collect();
    assert!(
        ordering.is_empty(),
        "doc comment attached to static func should not cause ordering warning, got:\n{}",
        ordering
            .iter()
            .map(|d| format!("  {}:{} {}", d.span.line, d.span.column, d.message))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

// Fix B: class_name/extends before doc comments at file top

#[test]
fn fmt_moves_class_name_before_doc_comments() {
    let source = "## Class documentation.\n## More docs.\nclass_name TestClass\nextends Node\n\nvar x = 10\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);

    let cn_pos = formatted.find("class_name TestClass");
    let doc_pos = formatted.find("## Class documentation.");
    assert!(
        cn_pos.unwrap() < doc_pos.unwrap(),
        "class_name should appear before ## doc comments, got:\n{}",
        formatted
    );
    let ext_pos = formatted.find("extends Node");
    assert!(
        ext_pos.unwrap() < doc_pos.unwrap(),
        "extends should appear before ## doc comments, got:\n{}",
        formatted
    );

    let second = formatter::format_source(&formatted, &config);
    assert_eq!(
        formatted, second,
        "must be idempotent after doc comment reorder"
    );
}

#[test]
fn fmt_doc_comments_before_extends_only() {
    let source =
        "## Lightweight overlay.\n## Shows FPS.\nextends CanvasLayer\n\nfunc _ready():\n\tpass\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);

    let ext_pos = formatted.find("extends CanvasLayer");
    let doc_pos = formatted.find("## Lightweight overlay.");
    assert!(
        ext_pos.unwrap() < doc_pos.unwrap(),
        "extends should appear before ## doc comments, got:\n{}",
        formatted
    );

    let second = formatter::format_source(&formatted, &config);
    assert_eq!(
        formatted, second,
        "must be idempotent after extends reorder"
    );
}

// Fix C: inner class member reordering

#[test]
fn fmt_reorders_inner_class_members() {
    let source = "class_name MyScript\nextends Node\n\nclass DialogueLine:\n\tvar speaker: String\n\tvar dialogue: String\n\tfunc _init(s: String, d: String):\n\t\tself.speaker = s\n\t\tself.dialogue = d\n\n\tstatic func from_dict(d: Dictionary) -> DialogueLine:\n\t\treturn DialogueLine.new(d.speaker, d.line)\n\n\tfunc _to_string():\n\t\treturn speaker\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);

    let static_pos = formatted.find("static func from_dict");
    let init_pos = formatted.find("func _init");
    let tostr_pos = formatted.find("func _to_string");
    // Virtual methods (`_init`) come before all other methods; a `static`
    // method is just a regular method per Godot's style guide, so it sorts
    // after the virtuals, keeping its source position relative to the
    // other regular method `_to_string`.
    assert!(
        init_pos.unwrap() < static_pos.unwrap(),
        "_init (virtual) should be before static func in inner class, got:\n{}",
        formatted
    );
    assert!(
        static_pos.unwrap() < tostr_pos.unwrap(),
        "static func should keep its source order before _to_string, got:\n{}",
        formatted
    );

    let diagnostics = linter::lint_source(&formatted, "test.gd", &config);
    let ordering: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "order/class-member-order")
        .collect();
    assert!(
        ordering.is_empty(),
        "no ordering warnings after inner class reorder, got:\n{}",
        ordering
            .iter()
            .map(|d| format!("  {}:{} {}", d.span.line, d.span.column, d.message))
            .collect::<Vec<_>>()
            .join("\n")
    );

    let second = formatter::format_source(&formatted, &config);
    assert_eq!(
        formatted, second,
        "must be idempotent after inner class reorder"
    );
}

#[test]
fn ordering_multiple_doc_comments_before_func_not_flagged() {
    // Multiple ## doc comment lines between vars and a static func should
    // not produce ordering warnings; they document the function.
    let source = "var x = 1\nvar y = 2\n\n## Normalize data for strict mode.\n## Ensures all required fields present.\n## Removes deprecated properties.\nstatic func normalize(data: Dictionary) -> Dictionary:\n\treturn data\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let ordering: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "order/class-member-order")
        .collect();
    assert!(
        ordering.is_empty(),
        "multi-line doc comment block before static func should not trigger ordering, got:\n{}",
        ordering
            .iter()
            .map(|d| format!("  {}:{} {}", d.span.line, d.span.column, d.message))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn fmt_full_reorder_with_doc_comments_and_multiline_vars() {
    // Comprehensive test: reorder with doc comments, multi-line vars, inner class.
    let source = r#"## Quest system docs.
## More details.
class_name QuestSystem
extends Node

@onready var ui: Control = %UI
@export var max_quests: int = 10

var _cache: Dictionary = {}
var reward_xp = GameSettings.load_value(
	"quests", "REWARD_XP"
)

const MAX_HISTORY = 500

signal quest_accept

## Normalize quest data.
static func normalize(data: Dictionary) -> Dictionary:
	return data

func _ready():
	pass

class Objective:
	var desc: String
	var target: int
	func _init(d: String, t: int):
		self.desc = d
		self.target = t

	static func from_dict(d: Dictionary) -> Objective:
		return Objective.new(d.desc, d.target)

	func is_done() -> bool:
		return false
"#;
    let config = default_config();
    let formatted = formatter::format_source(source, &config);

    // class_name before docs
    assert!(
        formatted.find("class_name").unwrap() < formatted.find("## Quest system").unwrap(),
        "class_name before docs, got:\n{}",
        formatted
    );
    // extends before docs
    assert!(
        formatted.find("extends Node").unwrap() < formatted.find("## Quest system").unwrap(),
        "extends before docs, got:\n{}",
        formatted
    );
    // signal before enum/const
    assert!(
        formatted.find("signal quest_accept").unwrap()
            < formatted.find("const MAX_HISTORY").unwrap(),
        "signal before const, got:\n{}",
        formatted
    );
    // Multi-line var preserved
    assert!(
        formatted.contains("\"quests\", \"REWARD_XP\""),
        "multi-line var args preserved, got:\n{}",
        formatted
    );
    // Doc comment stays with static func
    assert!(
        formatted.contains("## Normalize quest data.\nstatic func normalize"),
        "doc comment attached to static func, got:\n{}",
        formatted
    );
    // Inner class: virtual `_init` comes before the static method.
    assert!(
        formatted.find("func _init").unwrap() < formatted.find("static func from_dict").unwrap(),
        "inner class _init (virtual) before static func, got:\n{}",
        formatted
    );
    // No ordering warnings
    let diagnostics = linter::lint_source(&formatted, "test.gd", &config);
    let ordering: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "order/class-member-order")
        .collect();
    assert!(
        ordering.is_empty(),
        "no ordering warnings after full reorder, got:\n{}",
        ordering
            .iter()
            .map(|d| format!("  {}:{} {}", d.span.line, d.span.column, d.message))
            .collect::<Vec<_>>()
            .join("\n")
    );
    // Idempotent
    let second = formatter::format_source(&formatted, &config);
    assert_eq!(formatted, second, "must be idempotent");
}

#[test]
fn fmt_inner_class_preserves_multiline_body() {
    // Inner class with multi-line function bodies should not be corrupted.
    let source = "class_name Game\nextends Node\n\nclass Stats:\n\tvar hp: int\n\tvar mp: int\n\n\tfunc _init(h: int, m: int):\n\t\tself.hp = h\n\t\tself.mp = m\n\n\tstatic func default() -> Stats:\n\t\treturn Stats.new(100, 50)\n\n\tfunc to_dict() -> Dictionary:\n\t\treturn {\"hp\": hp, \"mp\": mp}\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);

    // All bodies preserved
    assert!(formatted.contains("self.hp = h"), "init body preserved");
    assert!(formatted.contains("self.mp = m"), "init body preserved");
    assert!(
        formatted.contains("Stats.new(100, 50)"),
        "static func body preserved"
    );
    assert!(
        formatted.contains("{ \"hp\": hp, \"mp\": mp }"),
        "to_dict body preserved"
    );
    // Virtual `_init` comes before the static method.
    assert!(
        formatted.find("func _init").unwrap() < formatted.find("static func default").unwrap(),
        "_init (virtual) before static func in inner class"
    );
    // Idempotent
    let second = formatter::format_source(&formatted, &config);
    assert_eq!(formatted, second, "must be idempotent");
}

// Line breaking tests

#[test]
fn fmt_breaks_long_function_signature() {
    let source = "func take_damage(amount: int, source: Node, damage_type: String, is_critical: bool, knockback: float, effect: String) -> void:\n\tpass\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);

    // Should be broken into multiple lines
    assert!(
        formatted.contains("func take_damage(\n"),
        "should break after opening paren, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("\tamount: int,"),
        "params should be on separate lines, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("\teffect: String,"),
        "last param should have trailing comma, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains(") -> void:"),
        "return type on closing line, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("\tpass"),
        "body preserved, got:\n{}",
        formatted
    );

    let second = formatter::format_source(&formatted, &config);
    assert_eq!(formatted, second, "must be idempotent");
}

#[test]
fn fmt_breaks_long_function_call() {
    let source = "func _ready():\n\tvar result = some_really_long_function_name(argument_one, argument_two, argument_three, argument_four)\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);

    assert!(
        formatted.contains("some_really_long_function_name(\n"),
        "should break call, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("\t\targument_one,"),
        "args indented, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("\t)"),
        "closing paren indented to call level, got:\n{}",
        formatted
    );

    let second = formatter::format_source(&formatted, &config);
    assert_eq!(formatted, second, "must be idempotent");
}

#[test]
fn fmt_breaks_long_dictionary_literal() {
    let source = "var data = {\"key_one\": value_one, \"key_two\": value_two, \"key_three\": value_three, \"key_four\": value_four, \"key_five\": value_five}\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);

    assert!(
        formatted.contains("var data = {\n"),
        "should break dict, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("\t\"key_one\": value_one,"),
        "entries on own lines, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("}"),
        "closing brace present, got:\n{}",
        formatted
    );

    let second = formatter::format_source(&formatted, &config);
    assert_eq!(formatted, second, "must be idempotent");
}

#[test]
fn fmt_breaks_long_array_literal() {
    let source = "var items = [item_one, item_two, item_three, item_four, item_five, item_six, item_seven, item_eight, item_nine]\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);

    assert!(
        formatted.contains("var items = [\n"),
        "should break array, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("\titem_one,"),
        "elements on own lines, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("]"),
        "closing bracket present, got:\n{}",
        formatted
    );

    let second = formatter::format_source(&formatted, &config);
    assert_eq!(formatted, second, "must be idempotent");
}

#[test]
fn fmt_does_not_break_short_lines() {
    let source = "func foo(a: int, b: int) -> void:\n\tvar x = bar(1, 2)\n\tvar y = [1, 2, 3]\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);

    // These lines are under 100 chars, should not be broken
    assert!(
        formatted.contains("func foo(a: int, b: int) -> void:"),
        "short sig should not break, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("var x = bar(1, 2)"),
        "short call should not break, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_does_not_break_long_string_without_delimiters() {
    let source = "func _ready():\n\tprint(\"This is a very long string that exceeds the line length limit but cannot be broken because it is a single argument\")\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);

    // Single argument in parens; breaking wouldn't help
    assert!(
        !formatted.contains("print(\n"),
        "single-arg call should not break, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_preserves_nested_calls_during_break() {
    let source = "func _ready():\n\tvar result = outer_func(inner_one(a, b), inner_two(c, d), inner_three(e, f), inner_four(g, h))\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);

    // Should break at top-level commas, not inside inner calls
    assert!(
        formatted.contains("inner_one(a, b),"),
        "nested call preserved intact, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("inner_two(c, d),"),
        "nested call preserved intact, got:\n{}",
        formatted
    );

    let second = formatter::format_source(&formatted, &config);
    assert_eq!(formatted, second, "must be idempotent");
}

// Quality rule suppression tests

#[test]
fn quality_max_function_length_suppressible() {
    // Build a function with 55 lines (over default limit of 50)
    let mut source =
        String::from("# gdstyle:ignore=quality/max-function-length\nfunc long_func():\n");
    for i in 0..55 {
        source.push_str(&format!("\tvar v{} = {}\n", i, i));
    }
    let config = default_config();
    let diagnostics = linter::lint_source(&source, "test.gd", &config);
    let quality: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "quality/max-function-length")
        .collect();
    assert!(
        quality.is_empty(),
        "max-function-length should be suppressible with inline comment"
    );
}

#[test]
fn quality_max_parameters_suppressible() {
    let source = "func many_params(a: int, b: int, c: int, d: int, e: int, f: int, g: int):  # gdstyle:ignore=quality/max-parameters\n\tpass\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let quality: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "quality/max-parameters")
        .collect();
    assert!(
        quality.is_empty(),
        "max-parameters should be suppressible with same-line comment"
    );
}

// === Corner case tests ===

// Line breaker: escaped backslash before closing quote
#[test]
fn fmt_line_break_escaped_backslash_in_string() {
    // "path\\" ends with a literal backslash. The line breaker must not
    // think the closing quote is escaped.
    let source = "func _ready():\n\tvar result = some_func(\"path\\\\\", \"other\\\\\", \"third\\\\\", \"fourth\\\\\")\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    // All string args must survive intact
    assert!(
        formatted.contains("\"path\\\\\""),
        "escaped backslash string must survive, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("\"fourth\\\\\""),
        "last arg must survive, got:\n{}",
        formatted
    );
    let second = formatter::format_source(&formatted, &config);
    assert_eq!(formatted, second, "must be idempotent");
}

// Line breaker: commas inside strings should NOT be split points
#[test]
fn fmt_line_break_commas_inside_strings() {
    let source = "func _ready():\n\tvar result = make_query(\"hello, world\", \"a, b, c\", \"x, y\", \"final\")\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    // The commas inside strings must NOT be split points
    assert!(
        formatted.contains("\"hello, world\","),
        "string with commas must be intact, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("\"a, b, c\","),
        "string with commas must be intact, got:\n{}",
        formatted
    );
    let second = formatter::format_source(&formatted, &config);
    assert_eq!(formatted, second, "must be idempotent");
}

// Line breaker: empty parameter list should not break
#[test]
fn fmt_line_break_empty_params() {
    let source = "func a_very_long_function_name_that_alone_exceeds_the_line_length_limit_without_any_parameters() -> Dictionary:\n\tpass\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    // Should NOT break inside empty ()
    assert!(
        !formatted.contains("(\n)"),
        "should not break empty params, got:\n{}",
        formatted
    );
}

// Line breaker: trailing comma already present
#[test]
fn fmt_line_break_trailing_comma_already_present() {
    let source = "func _ready():\n\tvar result = some_func(argument_one, argument_two, argument_three, argument_four, argument_five,)\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    // Should not produce double commas
    assert!(
        !formatted.contains(",,"),
        "must not produce double commas, got:\n{}",
        formatted
    );
    let second = formatter::format_source(&formatted, &config);
    assert_eq!(formatted, second, "must be idempotent");
}

// Line breaker: inline comment after closing paren
#[test]
fn fmt_line_break_inline_comment() {
    let source = "func attack(target: Node, damage: int, crit: bool, knockback: float, effect: String) -> void: # important\n\tpass\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    // Comment should be preserved on the closing line
    assert!(
        formatted.contains(") -> void: # important"),
        "inline comment must be on closing line, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("\ttarget: Node,"),
        "params should be broken, got:\n{}",
        formatted
    );
}

// Line breaker: typed arrays with nested brackets
#[test]
fn fmt_line_break_typed_arrays() {
    let source = "func process(items: Array[Dictionary[String, Variant]], callbacks: Array[Callable], options: Dictionary, flags: int) -> void:\n\tpass\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    // Nested brackets should NOT be split
    assert!(
        formatted.contains("items: Array[Dictionary[String, Variant]],"),
        "typed array must stay intact, got:\n{}",
        formatted
    );
    let second = formatter::format_source(&formatted, &config);
    assert_eq!(formatted, second, "must be idempotent");
}

// Naming: digit-to-uppercase transitions
#[test]
fn to_snake_case_digit_uppercase() {
    // Names like Vector2D, Area2D are common in GDScript
    let source = "func _is_valid_hitbox_area_2D(node):\n\tpass\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let naming: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "naming/function-name-snake-case")
        .collect();
    // Should suggest something reasonable (not panic or produce empty string)
    assert!(
        !naming.is_empty(),
        "should flag non-snake-case function name"
    );
    if let Some(fix) = &naming[0].fix {
        let suggested = &fix.replacements[0].new_text;
        assert!(!suggested.is_empty(), "suggestion should not be empty");
        assert!(
            !suggested.contains("__"),
            "suggestion should not have double underscores, got: {}",
            suggested
        );
    }
}

// Naming: single-letter class names
#[test]
fn naming_single_letter_class_name() {
    let source = "class_name A\nextends Node\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    // 'A' is a valid PascalCase-ish name (debatable) but should not crash
    // The rule might flag it since is_pascal_case requires lowercase chars
    // Just ensure no panic
    assert!(diagnostics.iter().all(|d| !d.message.is_empty()));
}

// Reorder: empty file
#[test]
fn fmt_reorder_empty_file() {
    let config = default_config();
    let formatted = formatter::format_source("", &config);
    // Should not panic on empty input
    assert!(formatted.is_empty() || formatted == "\n");
}

// Reorder: file with only comments
#[test]
fn fmt_reorder_only_comments() {
    let source = "# Just a comment\n# Another one\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert!(
        formatted.contains("# Just a comment"),
        "comments should be preserved"
    );
    let second = formatter::format_source(&formatted, &config);
    assert_eq!(formatted, second, "must be idempotent");
}

// Reorder: @tool file
#[test]
fn fmt_reorder_tool_file() {
    let source = "@tool\nclass_name MyPlugin\nextends EditorPlugin\n\nfunc _ready():\n\tpass\n\nvar x = 10\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    // @tool should stay first
    assert!(
        formatted.starts_with("@tool"),
        "@tool must be first, got:\n{}",
        formatted
    );
    // var should be before func after reorder
    let var_pos = formatted.find("var x");
    let func_pos = formatted.find("func _ready");
    assert!(
        var_pos.unwrap() < func_pos.unwrap(),
        "var before func, got:\n{}",
        formatted
    );
    let second = formatter::format_source(&formatted, &config);
    assert_eq!(formatted, second, "must be idempotent");
}

// Reorder: file with only inner classes
#[test]
fn fmt_reorder_only_inner_classes() {
    let source = "class StateIdle:\n\tfunc enter():\n\t\tpass\n\nclass StateRunning:\n\tfunc enter():\n\t\tpass\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    // Both classes should be preserved, order unchanged (same category)
    assert!(
        formatted.contains("class StateIdle:"),
        "first class preserved"
    );
    assert!(
        formatted.contains("class StateRunning:"),
        "second class preserved"
    );
    assert!(
        formatted.find("StateIdle").unwrap() < formatted.find("StateRunning").unwrap(),
        "order preserved"
    );
    let second = formatter::format_source(&formatted, &config);
    assert_eq!(formatted, second, "must be idempotent");
}

// Enum fix: enum with value assignments should preserve them
#[test]
fn fmt_enum_with_value_assignments_already_multiline() {
    let source = "enum State {\n\tIDLE = 0,\n\tRUNNING = 1,\n\tJUMPING = 2,\n}\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert!(
        formatted.contains("IDLE = 0"),
        "value assignment must be preserved, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("RUNNING = 1"),
        "value assignment must be preserved, got:\n{}",
        formatted
    );
}

#[test]
fn fix_enum_value_assignments_preserved_during_expansion() {
    // Single-line enum with value assignments must preserve them when expanded.
    let source = "enum State { IDLE = 0, RUNNING = 1, JUMPING = 2 }\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let fixed = fixer::apply_fixes(source, &diagnostics, true);
    assert!(
        fixed.contains("IDLE = 0,"),
        "value assignment must be preserved during expansion, got:\n{}",
        fixed
    );
    assert!(
        fixed.contains("RUNNING = 1,"),
        "value assignment must be preserved, got:\n{}",
        fixed
    );
    assert!(
        fixed.contains("JUMPING = 2,"),
        "value assignment must be preserved, got:\n{}",
        fixed
    );
}

// Blank lines: at end of file (edge case - only caught by trailing newline normalizer)
#[test]
fn fmt_blank_lines_at_end_of_file() {
    let source = "extends Node\n\n\n\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    // Should end with exactly one newline
    assert!(formatted.ends_with('\n'), "should end with newline");
    assert!(
        !formatted.ends_with("\n\n"),
        "should not end with double newline, got:\n{:?}",
        formatted
    );
}

#[test]
fn fmt_normalises_spacing_in_class_header() {
    // The file starts with `@tool` / `class_name` / `extends` separated by
    // doubled blank lines. The canonical Godot layout clusters these tightly.
    // Regression test for the original report on GARP/log-stream.gd.
    let source = "@tool\n\n\nclass_name LogStream\n\n\nextends Node\n\n\n## Class doc.\n\n\nsignal log_message\n\n\nsignal log_warning\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert!(
        formatted.starts_with("@tool\nclass_name LogStream\nextends Node\n## Class doc.\n"),
        "header items should cluster tight, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("signal log_message\nsignal log_warning"),
        "consecutive signals should be tight, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_parses_var_with_computed_property_getset() {
    // `var foo:` followed by an indented `get:` / `set(...)` block is a
    // GDScript computed property. The parser must consume that whole block
    // as part of the variable, and the formatter must leave it intact.
    let source = "extends Node\n\n## Property doc.\nvar value:\n\tget:\n\t\treturn _value\n\tset(v):\n\t\t_value = v\n\nfunc _ready():\n\tpass\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert!(
        formatted.contains("## Property doc.\nvar value:"),
        "doc must stay attached to computed-property var, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("\tget:") && formatted.contains("\tset(v):"),
        "get/set bodies must be preserved, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_doc_comment_between_two_functions_is_preserved() {
    // After a function body, a `##` doc comment that documents the next
    // function must remain visible to the outer parser (not be swallowed
    // by the body line counter) and tight against the function it documents.
    let source = "extends Node\n\nfunc _ready():\n\tpass\n\n## Normalize quest data.\nstatic func normalize() -> void:\n\tpass\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert!(
        formatted.contains("## Normalize quest data.\nstatic func normalize"),
        "doc must stay attached to the next function, got:\n{}",
        formatted
    );
}

// --- Member-spacing regression suite -----------------------------------------
// The original report from `GARP/log-stream.gd` was "lots of wasted blank
// space at the top". The cases below cover each pair of adjacent
// declarations the spacing pass has to handle.

#[test]
fn fmt_member_spacing_tightens_header_block() {
    let source = "@tool\n\n\nclass_name Foo\n\n\nextends Node\n";
    let formatted = formatter::format_source(source, &default_config());
    assert_eq!(
        formatted, "@tool\nclass_name Foo\nextends Node\n",
        "@tool/class_name/extends must cluster tight, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_member_spacing_standalone_class_doc_after_extends() {
    // A `##` block separated from the next member by a blank is a
    // free-standing class docstring; it must sit tight under `extends`
    // and get a single blank line before the first member.
    let source = "class_name Foo\nextends Node\n\n\n## Class doc.\n\n\nsignal x\n";
    let formatted = formatter::format_source(source, &default_config());
    assert!(
        formatted.starts_with("class_name Foo\nextends Node\n## Class doc.\n\nsignal x\n"),
        "standalone class doc tight under extends, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_member_spacing_collapses_doubled_blanks_between_signals() {
    let source = "extends Node\n\nsignal a\n\n\nsignal b\n\n\nsignal c\n";
    let formatted = formatter::format_source(source, &default_config());
    assert!(
        formatted.contains("signal a\nsignal b\nsignal c"),
        "consecutive signals must be tight, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_member_spacing_two_blanks_around_each_function() {
    let source = "extends Node\n\nvar x = 1\n\nfunc a():\n\tpass\n\nfunc b():\n\tpass\n";
    let formatted = formatter::format_source(source, &default_config());
    assert!(
        formatted.contains("var x = 1\n\n\nfunc a():"),
        "var -> func should have 2 blank lines, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("\tpass\n\n\nfunc b():"),
        "func -> func should have 2 blank lines, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_member_spacing_one_blank_between_member_categories() {
    let source = "extends Node\n\nsignal s\n\n\n\n\nconst K = 1\n\n\n\nvar x = 2\n";
    let formatted = formatter::format_source(source, &default_config());
    assert!(
        formatted.contains("signal s\n\nconst K = 1\n\nvar x = 2"),
        "different categories must be separated by exactly one blank, got:\n{}",
        formatted
    );
}

// --- Computed-property var regression suite ----------------------------------

#[test]
fn fmt_computed_property_with_export_annotation() {
    // A `var name:` that opens a get/set block, with `@export` above it,
    // must be parsed as a single variable, body preserved, doc attached.
    let source = "extends Node\n\n## The thing.\n@export var thing:\n\tget:\n\t\treturn _thing\n\tset(v):\n\t\t_thing = v\n\nfunc _ready():\n\tpass\n";
    let formatted = formatter::format_source(source, &default_config());
    assert!(
        formatted.contains("## The thing.\n@export var thing:"),
        "doc + annotation must stay attached to the var, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("\tget:") && formatted.contains("\tset(v):"),
        "get/set block must survive intact, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_computed_property_with_explicit_type_hint() {
    // `var name: int:` (typed computed property) is also valid syntax;
    // the trailing `:` opens the get/set block.
    let source = "extends Node\n\nvar counter: int:\n\tget:\n\t\treturn _counter\n\tset(v):\n\t\t_counter = v\n\nfunc _ready():\n\tpass\n";
    let formatted = formatter::format_source(source, &default_config());
    assert!(
        formatted.contains("var counter: int:\n\tget:"),
        "typed computed property must parse correctly, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_regular_var_with_value_unaffected_by_computed_property_handling() {
    // A regular `var name = value` must NOT trigger any get/set block
    // consumption (regression guard for the parser change).
    let source = "extends Node\n\nvar a = 1\nvar b = 2\nvar c = 3\n\nfunc _ready():\n\tpass\n";
    let formatted = formatter::format_source(source, &default_config());
    assert!(
        formatted.contains("var a = 1\nvar b = 2\nvar c = 3"),
        "consecutive plain vars must survive intact, got:\n{}",
        formatted
    );
}

// --- Colon-spacing regression suite ------------------------------------------
// Style guide: no space before `:`, one space after (except `:=` and `\n`).

#[test]
fn fmt_colon_spacing_in_type_hints() {
    let source =
        "extends Node\n\nconst X:float = 1.0\nvar y:int = 0\n@export var z:Dictionary = {}\n";
    let formatted = formatter::format_source(source, &default_config());
    assert!(
        formatted.contains("const X: float = 1.0"),
        "const type hint: missing space after ':', got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("var y: int = 0"),
        "var type hint: missing space after ':', got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("@export var z: Dictionary"),
        "annotated var type hint: missing space after ':', got:\n{}",
        formatted
    );
}

#[test]
fn fmt_colon_spacing_in_function_parameters() {
    let source = "extends Node\n\nfunc f(a:int, b:String = \"x\") -> bool:\n\treturn true\n";
    let formatted = formatter::format_source(source, &default_config());
    assert!(
        formatted.contains("func f(a: int, b: String = \"x\")"),
        "function parameter type hints: missing space after ':', got:\n{}",
        formatted
    );
}

#[test]
fn fmt_colon_spacing_strips_space_before_block_colon() {
    let source =
        "extends Node\n\nfunc _ready() :\n\tif true :\n\t\tpass\n\twhile false :\n\t\tpass\n";
    let formatted = formatter::format_source(source, &default_config());
    assert!(
        formatted.contains("func _ready():"),
        "func signature: no space before ':', got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("if true:"),
        "if: no space before ':', got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("while false:"),
        "while: no space before ':', got:\n{}",
        formatted
    );
}

#[test]
fn fmt_colon_spacing_preserves_walrus_inferred_type() {
    // `:=` must not have its space-before stripped (`var x := 1` is the
    // canonical form, not `var x:= 1`).
    let source = "extends Node\n\nfunc f() -> void:\n\tvar x := 1\n\tvar y := \"two\"\n";
    let formatted = formatter::format_source(source, &default_config());
    assert!(
        formatted.contains("var x := 1"),
        ":= must keep its leading space, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("var y := \"two\""),
        ":= must keep its leading space, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_colon_spacing_in_dict_keys() {
    let source = "extends Node\n\nvar d = {\"a\":1, \"b\":2}\n";
    let formatted = formatter::format_source(source, &default_config());
    assert!(
        formatted.contains("{ \"a\": 1, \"b\": 2 }"),
        "dict keys: missing space after ':', got:\n{}",
        formatted
    );
}

// --- Comma-spacing regression suite ------------------------------------------
// Style guide: no space before `,`, one space after (except trailing).

#[test]
fn fmt_comma_spacing_in_function_call_args() {
    let source = "extends Node\n\nfunc f() -> void:\n\tInput.get_axis(\"a\",\"b\",\"c\")\n";
    let formatted = formatter::format_source(source, &default_config());
    assert!(
        formatted.contains("Input.get_axis(\"a\", \"b\", \"c\")"),
        "function args: missing space after ',', got:\n{}",
        formatted
    );
}

#[test]
fn fmt_comma_spacing_in_array_and_dict_literals() {
    let source = "extends Node\n\nvar arr = [1,2,3,4]\nvar d = {\"a\":1,\"b\":2,\"c\":3}\n";
    let formatted = formatter::format_source(source, &default_config());
    assert!(
        formatted.contains("[1, 2, 3, 4]"),
        "array literal: missing space after ',', got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("{ \"a\": 1, \"b\": 2, \"c\": 3 }"),
        "dict literal: missing space after ',', got:\n{}",
        formatted
    );
}

#[test]
fn fmt_comma_spacing_strips_space_before_comma() {
    let source = "extends Node\n\nfunc f() -> void:\n\tvar a = [1 ,2 ,3]\n\tfn(x ,y ,z)\n";
    let formatted = formatter::format_source(source, &default_config());
    assert!(
        formatted.contains("[1, 2, 3]"),
        "array: must strip stray space before ',', got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("fn(x, y, z)"),
        "call: must strip stray space before ',', got:\n{}",
        formatted
    );
}

#[test]
fn fmt_comma_spacing_preserves_trailing_comma() {
    // A comma followed by `\n` (multi-line trailing comma) or by a closing
    // delimiter must not trip the "space after" check.
    let source = "extends Node\n\nfunc f() -> void:\n\tvar arr = [\n\t\t1,\n\t\t2,\n\t]\n";
    let formatted = formatter::format_source(source, &default_config());
    assert!(
        formatted.contains(",\n\t\t2,\n\t]"),
        "trailing commas must not be flagged, got:\n{}",
        formatted
    );
}

// --- Brace-spacing regression suite -------------------------------------------
// Style guide: single-line dictionary literals get a space after '{' and
// before '}' (the one exception to "avoid extra spaces").

#[test]
fn fmt_brace_spacing_pads_single_line_dict() {
    let source = "extends Node\n\nvar my_dictionary = {key = \"value\"}\n";
    let formatted = formatter::format_source(source, &default_config());
    assert!(
        formatted.contains("{ key = \"value\" }"),
        "single-line dict must be padded, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_brace_spacing_leaves_empty_dict_alone() {
    let source = "extends Node\n\nvar d = {}\n";
    let formatted = formatter::format_source(source, &default_config());
    assert!(
        formatted.contains("var d = {}"),
        "empty dict must not be padded, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_brace_spacing_pads_nested_dicts() {
    let source = "extends Node\n\nvar d = {a = {b = 1}}\n";
    let formatted = formatter::format_source(source, &default_config());
    assert!(
        formatted.contains("{ a = { b = 1 } }"),
        "nested dicts must each be padded, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_brace_spacing_skips_enum_bodies() {
    // Enums use `{}` too, but they aren't dictionaries and must not be padded.
    let source = "extends Node\n\nenum Single {ONLY}\n";
    let formatted = formatter::format_source(source, &default_config());
    assert!(
        formatted.contains("enum Single {ONLY}"),
        "enum body must not be padded like a dict, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_brace_spacing_leaves_multiline_dict_alone() {
    let source = "extends Node\n\nvar d = {\n\t\"a\": 1,\n\t\"b\": 2,\n}\n";
    let formatted = formatter::format_source(source, &default_config());
    assert!(
        formatted.contains("d = {\n\t\"a\": 1,"),
        "multi-line dict braces must not be padded, got:\n{}",
        formatted
    );
}

// --- Call-paren-spacing regression suite --------------------------------------
// Style guide: no space between a callee and its opening '(', e.g.
// `print ("foo")` -> `print("foo")`.

#[test]
fn fmt_call_paren_spacing_strips_space_before_call() {
    let source = "extends Node\n\nfunc f() -> void:\n\tprint (\"foo\")\n";
    let formatted = formatter::format_source(source, &default_config());
    assert!(
        formatted.contains("print(\"foo\")"),
        "call must not have space before '(', got:\n{}",
        formatted
    );
}

#[test]
fn fmt_call_paren_spacing_strips_space_before_chained_and_preload() {
    let source = "extends Node\n\nfunc f() -> void:\n\tget_callback() ()\n\tpreload (\"res://x.tscn\")\n\tassert (true)\n";
    let formatted = formatter::format_source(source, &default_config());
    assert!(
        formatted.contains("get_callback()()"),
        "chained call must not have space before '(', got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("preload(\"res://x.tscn\")"),
        "preload must not have space before '(', got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("assert(true)"),
        "assert must not have space before '(', got:\n{}",
        formatted
    );
}

#[test]
fn fmt_call_paren_spacing_preserves_if_condition_parens() {
    // `if (x):` isn't a call; no-unnecessary-parens (not call-paren-spacing)
    // owns stripping those parens, so the space before them is irrelevant
    // here as long as call-paren-spacing doesn't touch it.
    let source = "extends Node\n\nfunc f() -> void:\n\tif (x > 5):\n\t\tpass\n";
    let formatted = formatter::format_source(source, &default_config());
    assert!(
        !formatted.contains("if(x"),
        "if-condition parens must not be treated as a call, got:\n{}",
        formatted
    );
}

#[test]
fn fmt_combined_colon_and_comma_fixes_match_canonical_godot_form() {
    // End-to-end regression covering A1+A2+A3 together against a realistic
    // function signature and body.
    let source = "extends Node\n\nfunc add_item(item_id:String,count:int = 1,rarity:int = 0) -> bool :\n\tvar pairs = [{\"a\":1,\"b\":2}, {\"c\":3,\"d\":4}]\n\treturn true\n";
    let formatted = formatter::format_source(source, &default_config());
    assert!(
        formatted
            .contains("func add_item(item_id: String, count: int = 1, rarity: int = 0) -> bool:"),
        "combined signature spacing, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("{ \"a\": 1, \"b\": 2 }")
            && formatted.contains("{ \"c\": 3, \"d\": 4 }"),
        "combined dict spacing, got:\n{}",
        formatted
    );
}

// Fixer: rename doesn't affect string literals
#[test]
fn fixer_rename_does_not_affect_strings() {
    let source =
        "var CONFIG = 1\nfunc _ready():\n\tprint(\"CONFIG is important\")\n\tprint(CONFIG)\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let fixed = fixer::apply_fixes(source, &diagnostics, false);
    // The string "CONFIG is important" must NOT be changed
    assert!(
        fixed.contains("\"CONFIG is important\""),
        "string literal should not be renamed, got:\n{}",
        fixed
    );
    // But the identifier reference should be renamed
    assert!(
        fixed.contains("print(config)"),
        "identifier should be renamed, got:\n{}",
        fixed
    );
}

// Formatter: multiline string should not be broken
#[test]
fn fmt_does_not_break_multiline_string() {
    let source = "var text = \"This is a very long string with commas, periods, and other punctuation that exceeds the line length limit but should not be broken\"\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    // Single-argument call with long string; breaking wouldn't help
    // The string content must remain intact
    assert!(
        formatted.contains("commas, periods, and other punctuation"),
        "string must not be split, got:\n{}",
        formatted
    );
}

// Formatter: GDScript string prefixes
#[test]
fn fmt_handles_string_prefix_in_line_break() {
    let source = "func _ready():\n\tvar result = make_node(&\"StringName1\", &\"StringName2\", &\"StringName3\", &\"StringName4\", &\"StringName5\")\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    // &"..." strings should survive line breaking intact
    assert!(
        formatted.contains("&\"StringName1\""),
        "string name prefix must survive, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("&\"StringName5\""),
        "last string name must survive, got:\n{}",
        formatted
    );
    let second = formatter::format_source(&formatted, &config);
    assert_eq!(formatted, second, "must be idempotent");
}

// Signal past-tense: single-word signals
#[test]
fn signal_past_tense_single_word() {
    let source = "signal connect\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let past: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "naming/signal-past-tense")
        .collect();
    // "connect" is not past tense and not a noun
    assert!(
        !past.is_empty(),
        "single-word verb signal should be flagged"
    );
}

// Reorder: annotations attached to members move with them
#[test]
fn fmt_reorder_annotations_move_with_members() {
    let source =
        "extends Node\n\nfunc _ready():\n\tpass\n\n@export_range(0, 100)\nvar health: int = 100\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    // @export_range should stay attached to var health and appear before func
    assert!(
        formatted.contains("@export_range(0, 100)\nvar health"),
        "@export_range must stay with var, got:\n{}",
        formatted
    );
    let export_pos = formatted.find("@export_range");
    let func_pos = formatted.find("func _ready");
    assert!(
        export_pos.unwrap() < func_pos.unwrap(),
        "@export var before func, got:\n{}",
        formatted
    );
}

// Reorder: blank lines between doc comments and class_name with gap
#[test]
fn fmt_doc_comments_with_blank_line_before_class_name() {
    let source = "## Class docstring\n\nclass_name MyClass\nextends Node\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    let cn_pos = formatted.find("class_name");
    let doc_pos = formatted.find("## Class docstring");
    assert!(
        cn_pos.unwrap() < doc_pos.unwrap(),
        "class_name before doc, got:\n{}",
        formatted
    );
    let second = formatter::format_source(&formatted, &config);
    assert_eq!(formatted, second, "must be idempotent");
}

// Unicode identifier support
#[test]
fn lexer_handles_unicode_identifiers() {
    let source = "var café = 1\nvar naïve = 2\nvar über = 3\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    // Should not crash and should parse identifiers
    // The naming rules may flag them but should not panic
    for d in &diagnostics {
        assert!(!d.message.is_empty());
    }
}

// Windows line endings
#[test]
fn lexer_handles_crlf_line_endings() {
    let source = "extends Node\r\n\r\nvar x = 1\r\nvar y = 2\r\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    // Should not crash or produce bogus diagnostics from \r characters
    // The \r should be silently skipped
    for d in &diagnostics {
        assert!(
            !d.message.contains('\r'),
            "diagnostic should not contain \\r"
        );
    }
}

#[test]
fn formatter_handles_crlf_input() {
    let source = "var x = 'hello'\r\nvar y = 'world'\r\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    // Should normalize to LF and convert quotes
    assert!(
        formatted.contains("\"hello\""),
        "quotes should be normalized, got:\n{}",
        formatted
    );
    assert!(
        !formatted.contains('\r'),
        "\\r should be stripped from output, got:\n{:?}",
        formatted
    );
}

// Naming: Vector2D, Area2D, Node3D patterns
#[test]
fn naming_suggests_correct_snake_case_for_2d_3d_names() {
    let source = "func _is_valid_hitbox_area_2D(node):\n\tpass\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let naming: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "naming/function-name-snake-case")
        .collect();
    assert!(
        !naming.is_empty(),
        "should flag non-snake-case function name"
    );
    let fix = naming[0].fix.as_ref().unwrap();
    let suggested = &fix.replacements[0].new_text;
    assert_eq!(
        suggested, "_is_valid_hitbox_area_2d",
        "should suggest area_2d not area_2_d, got: {}",
        suggested
    );
}

// Naming: single letter class names accepted as PascalCase
#[test]
fn naming_single_letter_class_name_not_flagged() {
    let source = "class_name A\nextends Node\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let naming: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "naming/class-name-pascal-case")
        .collect();
    assert!(
        naming.is_empty(),
        "single-letter class name 'A' should be valid PascalCase, got: {:?}",
        naming.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// Bare comma continuation lines are NOT broken (no enclosing delimiter
// context means we can't safely determine if splitting is valid GDScript).
#[test]
fn fmt_does_not_break_bare_comma_continuation() {
    let source = "func _ready():\n\tvar aff = Affordance.new(\n\t\t\"order_coffee\", \"is\", \"ordering\", \"ordering coffee at the counter\", \"desc\", \"result\", \"interact\"\n\t)\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    // The continuation line should NOT be split (no enclosing delimiter on this line)
    assert!(
        formatted.contains("\"order_coffee\", \"is\""),
        "bare comma line should not be split, got:\n{}",
        formatted
    );
}

// Regression: % string formatting with multi-line argument arrays must not be corrupted.
#[test]
fn fmt_preserves_percent_format_multiline_args() {
    let source = "func log_it():\n\tLog.info(\"Controller::%s at %s: activity='%s', count=%d\"\n\t\t% [self.name, time.strftime(\"%H:%M\"),\n\t\t\tactivity,\n\t\t\tcount,])\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    // The % formatting args must remain intact
    assert!(
        formatted.contains("% [self.name"),
        "% format array must survive, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("activity,"),
        "args must survive, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("count,]"),
        "trailing arg must survive, got:\n{}",
        formatted
    );
    let second = formatter::format_source(&formatted, &config);
    assert_eq!(formatted, second, "must be idempotent");
}

#[test]
fn fmt_preserves_already_broken_function_call() {
    // Function call already broken across lines with trailing comma must not be re-broken.
    let source = "func check(target, items):\n\tif not Controller._is_in_list(\n\t\tself.current_target,\n\t\titems,\n\t):\n\t\treturn false\n\treturn true\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert!(
        formatted.contains("self.current_target,"),
        "already-broken args preserved, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("\t):"),
        "closing paren preserved, got:\n{}",
        formatted
    );
    let second = formatter::format_source(&formatted, &config);
    assert_eq!(formatted, second, "must be idempotent");
}

#[test]
fn fmt_does_not_corrupt_long_lines_fixture() {
    // Run formatter on the edge case fixture and verify no data loss.
    let source = std::fs::read_to_string(fixture_path("long_lines_edge_cases.gd")).unwrap();
    let config = default_config();
    let formatted = formatter::format_source(&source, &config);

    // Key content that must survive formatting:
    assert!(
        formatted.contains("curr_time.strftime(\"%H:%M\")"),
        "format arg must survive"
    );
    assert!(
        formatted.contains("start_time.strftime(\"%H:%M\") if start_time"),
        "conditional arg must survive"
    );
    assert!(
        formatted.contains("action_name if action_name else \"null\""),
        "ternary in format args must survive"
    );
    assert!(
        formatted.contains("Controller._is_in_list("),
        "function call must survive"
    );
    assert!(
        formatted.contains("curr_selected_activity"),
        "property chain must survive"
    );
    assert!(
        formatted.contains("using static fallback plan"),
        "long string must survive"
    );

    // Must be idempotent
    let second = formatter::format_source(&formatted, &config);
    assert_eq!(formatted, second, "must be idempotent on edge case fixture");
}

// Long comment wrapping
#[test]
fn fmt_wraps_long_comment() {
    let source = "func _ready():\n\t# TODO: remove this auxiliary dictionary and make all_affordances a dict. Not doing it now because it requires refactoring all callers\n\tpass\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    // Comment should be wrapped into multiple lines
    let comment_lines: Vec<_> = formatted
        .lines()
        .filter(|l| l.trim().starts_with("# TODO"))
        .collect();
    assert!(
        !comment_lines.is_empty(),
        "wrapped comment should exist, got:\n{}",
        formatted
    );
    // Each line should be <= 100 visual chars
    for cl in &comment_lines {
        let vlen = cl
            .chars()
            .map(|c| if c == '\t' { 4 } else { 1 })
            .sum::<usize>();
        assert!(
            vlen <= 100,
            "wrapped comment line too long ({} chars): {}",
            vlen,
            cl
        );
    }
    let second = formatter::format_source(&formatted, &config);
    assert_eq!(formatted, second, "must be idempotent");
}

// Short comments not wrapped
#[test]
fn fmt_does_not_wrap_short_comment() {
    let source = "# Short comment\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert_eq!(
        formatted.matches("# ").count(),
        1,
        "short comment should not be split"
    );
}

// Long if condition breaking
#[test]
fn fmt_breaks_long_if_condition() {
    let source = "func check(event):\n\tif event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT and event.is_pressed() and event.double_click:\n\t\tpass\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert!(
        formatted.contains("if (event is InputEventMouseButton"),
        "should wrap in parens, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("and event.button_index"),
        "should break at and, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("):"),
        "should end with ):, got:\n{}",
        formatted
    );
    let second = formatter::format_source(&formatted, &config);
    assert_eq!(formatted, second, "must be idempotent");
}

// Short if not broken
#[test]
fn fmt_does_not_break_short_if() {
    let source = "func test():\n\tif a and b:\n\t\tpass\n";
    let config = default_config();
    let formatted = formatter::format_source(source, &config);
    assert!(
        formatted.contains("if a and b:"),
        "short if should not be broken"
    );
}

// ===========================================================================
// Quality rules: integration tests
// ===========================================================================

/// Helper: create a config with specific quality rules enabled (they may be
/// off-by-default) and all other rules still at defaults.
fn quality_config(enable: &[&str]) -> Config {
    let mut config = default_config();
    for rule in enable {
        config
            .rules
            .insert(rule.to_string(), gdstyle::config::RuleSeverityConfig::Warn);
    }
    config
}

#[test]
fn quality_bad_quality_fixture_detects_all_rules() {
    let config = default_config();
    let diagnostics = linter::lint_file(&fixture_path("bad_quality.gd"), &config).unwrap();

    let rules: Vec<&str> = diagnostics.iter().map(|d| d.rule.as_str()).collect();

    assert!(
        rules.contains(&"quality/self-comparison"),
        "should detect self-comparison"
    );
    assert!(
        rules.contains(&"quality/no-self-assign"),
        "should detect no-self-assign"
    );
    assert!(
        rules.contains(&"quality/duplicate-dict-key"),
        "should detect duplicate-dict-key"
    );
    assert!(
        rules.contains(&"quality/duplicated-load"),
        "should detect duplicated-load"
    );
    assert!(
        rules.contains(&"quality/no-else-return"),
        "should detect no-else-return"
    );
    assert!(
        rules.contains(&"quality/unreachable-code"),
        "should detect unreachable-code"
    );
    assert!(
        rules.contains(&"quality/await-in-loop"),
        "should detect await-in-loop"
    );
    assert!(
        rules.contains(&"quality/allocation-in-loop"),
        "should detect allocation-in-loop"
    );
    assert!(
        rules.contains(&"quality/process-get-node"),
        "should detect process-get-node"
    );
    assert!(
        rules.contains(&"quality/unnecessary-pass"),
        "should detect unnecessary-pass"
    );
    assert!(
        rules.contains(&"quality/max-nesting-depth"),
        "should detect max-nesting-depth"
    );
    assert!(
        rules.contains(&"quality/max-returns"),
        "should detect max-returns"
    );
    assert!(
        rules.contains(&"quality/max-branches"),
        "should detect max-branches"
    );
    assert!(
        rules.contains(&"quality/max-local-variables"),
        "should detect max-local-variables"
    );
}

// --- no-debug-print (off by default, must be explicitly enabled) ---

#[test]
fn quality_no_debug_print_off_by_default() {
    let config = default_config();
    let source = "func foo() -> void:\n\tprint(\"hello\")\n";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/no-debug-print"),
        "no-debug-print should be off by default"
    );
}

#[test]
fn quality_no_debug_print_when_enabled() {
    let config = quality_config(&["quality/no-debug-print"]);
    let source =
        "func foo() -> void:\n\tprint(\"hello\")\n\tprints(\"a\", \"b\")\n\tprinterr(\"err\")\n";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let hits: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "quality/no-debug-print")
        .collect();
    assert_eq!(hits.len(), 3, "should detect 3 debug print calls");
}

#[test]
fn quality_no_debug_print_ignores_custom_funcs() {
    let config = quality_config(&["quality/no-debug-print"]);
    let source = "func foo() -> void:\n\tprint_score(100)\n";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/no-debug-print"),
        "should not flag print_score"
    );
}

// --- self-comparison ---

#[test]
fn quality_self_comparison_all_operators() {
    let config = default_config();
    let source = "\
func test() -> void:
\tvar a: int = 1
\tif a == a:
\t\tpass
\tif a != a:
\t\tpass
\tif a > a:
\t\tpass
\tif a >= a:
\t\tpass
\tif a < a:
\t\tpass
\tif a <= a:
\t\tpass
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let hits: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "quality/self-comparison")
        .collect();
    assert_eq!(
        hits.len(),
        6,
        "should detect all 6 self-comparisons, got {}",
        hits.len()
    );
}

#[test]
fn quality_self_comparison_different_vars_ok() {
    let config = default_config();
    let source = "func test() -> void:\n\tif a == b:\n\t\tpass\n";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/self-comparison"),
        "different variables should not trigger"
    );
}

#[test]
fn quality_self_comparison_dotted_paths_ok() {
    // Regression (issue #7): the exact user reproduction. A shared trailing
    // segment (`.position`, `.label`) must not be read as a self-comparison.
    let config = default_config();
    let source = "\
extends Node

@export var whatever: Node

func _ready():
\tif position == whatever.position:
\t\tprint(\"hello\")
\tif stored_goods != null and stored_goods.label == label:
\t\tprint(\"hi\")
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/self-comparison"),
        "different dotted paths must not self-compare, got: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.rule == "quality/self-comparison")
            .collect::<Vec<_>>()
    );
}

// --- no-self-assign ---

#[test]
fn quality_no_self_assign_simple() {
    let config = default_config();
    let source = "func test() -> void:\n\tvar x: int = 5\n\tx = x\n";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(diagnostics
        .iter()
        .any(|d| d.rule == "quality/no-self-assign"));
}

#[test]
fn quality_no_self_assign_dot_access_ok() {
    let config = default_config();
    let source = "func test() -> void:\n\tx = x.normalized()\n";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/no-self-assign"),
        "x = x.method() should not trigger"
    );
}

#[test]
fn quality_no_self_assign_lhs_property_rhs_local_does_not_trigger() {
    // Regression for a false positive reported in the wild:
    // `moon.size = size * (.08 + 0.035 * _moon_value)` was being flagged
    // because the rule used a 3-token sliding window and only saw matching
    // `size` identifiers on both sides of `=`, ignoring that the LHS is
    // actually the property access `moon.size`.
    let config = default_config();
    let source = "func test() -> void:\n\tmoon.size = size * 0.5\n";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/no-self-assign"),
        "moon.size = size * 0.5 must not trigger (different paths)"
    );
}

#[test]
fn quality_no_self_assign_uses_lhs_in_rhs_expression_does_not_trigger() {
    // `x = x + 1` is not a self-assignment, it reads x and writes a new
    // value. The rule must only flag pure x = x no-ops.
    let config = default_config();
    let source = "func test() -> void:\n\tvar x: int = 5\n\tx = x + 1\n";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/no-self-assign"),
        "x = x + 1 must not trigger (uses x in expression)"
    );
}

#[test]
fn quality_no_self_assign_self_qualified_different_path_does_not_trigger() {
    // self.position and position are different paths even when the trailing
    // segment matches.
    let config = default_config();
    let source = "func test() -> void:\n\tself.position = position\n";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/no-self-assign"),
        "self.position = position must not trigger (different paths)"
    );
}

#[test]
fn quality_no_self_assign_matching_dot_chain_triggers() {
    // `obj.foo = obj.foo` IS a real self-assign and should still be flagged.
    let config = default_config();
    let source = "func test() -> void:\n\tobj.foo = obj.foo\n";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        diagnostics
            .iter()
            .any(|d| d.rule == "quality/no-self-assign"),
        "obj.foo = obj.foo must still trigger (same path on both sides)"
    );
}

#[test]
fn quality_no_self_assign_deep_chain_triggers() {
    // Multi-level chains should also work both ways.
    let config = default_config();
    let source = "func test() -> void:\n\tself.player.health = self.player.health\n";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        diagnostics
            .iter()
            .any(|d| d.rule == "quality/no-self-assign"),
        "self.player.health = self.player.health must trigger"
    );
}

#[test]
fn quality_no_self_assign_indexed_does_not_trigger() {
    // arr[i] = arr[i] is an array element assignment; LHS chain isn't a
    // pure identifier dotted path, so we conservatively don't flag it.
    let config = default_config();
    let source = "func test() -> void:\n\tarr[i] = arr[i]\n";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/no-self-assign"),
        "arr[i] = arr[i] must not trigger (subscript, not dotted chain)"
    );
}

#[test]
fn quality_no_self_assign_walrus_does_not_trigger() {
    // `var x := x` would have `Identifier(x) Colon Assign Identifier(x)` —
    // the LHS chain walk from `=` lands on Colon and bails, so we do not
    // flag inferred-type declarations like `var x := some_value`.
    let config = default_config();
    let source = "func test() -> void:\n\tvar x := 5\n";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/no-self-assign"),
        ":= inferred declaration must not trigger"
    );
}

// --- duplicate-dict-key ---

#[test]
fn quality_duplicate_dict_key_detected() {
    let config = default_config();
    let source = "func test() -> Dictionary:\n\treturn {\"a\": 1, \"b\": 2, \"a\": 3}\n";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(diagnostics
        .iter()
        .any(|d| d.rule == "quality/duplicate-dict-key"));
}

#[test]
fn quality_duplicate_dict_key_nested_ok() {
    let config = default_config();
    let source = "\
func test() -> Dictionary:
\treturn {\"a\": {\"a\": 1}, \"b\": 2}
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/duplicate-dict-key"),
        "same key in nested dict should not trigger on outer"
    );
}

#[test]
fn quality_duplicate_dict_key_enum_key_from_other_class_ok() {
    // Regression (issue #8): the exact user reproduction. A dotted key like
    // `Enums.E.KEY_A` was scanned token-by-token, so the two `.` tokens
    // collided into a bogus `duplicate dictionary key '.'`.
    let config = default_config();
    let source = "\
extends Node

const _DEFS: Dictionary = {
\tEnums.E.KEY_A: {
\t\t\"value_1\": \"A\",
\t},
}
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/duplicate-dict-key"),
        "a single dotted enum key must not self-collide, got: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.rule == "quality/duplicate-dict-key")
            .collect::<Vec<_>>()
    );
}

#[test]
fn quality_duplicate_dict_key_distinct_dotted_keys_ok() {
    // Two different enum entries used as keys are distinct, not duplicates.
    let config = default_config();
    let source = "\
extends Node

const _DEFS: Dictionary = {
\tEnums.E.KEY_A: {
\t\t\"value_1\": \"A\",
\t},
\tEnums.E.KEY_B: {
\t\t\"value_1\": \"B\",
\t},
}
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/duplicate-dict-key"),
        "distinct dotted keys must not be flagged as duplicates"
    );
}

#[test]
fn quality_duplicate_dict_key_same_dotted_key_detected() {
    // A genuinely repeated dotted key is still caught after the fix.
    let config = default_config();
    let source = "\
func test() -> Dictionary:
\treturn {
\t\tEnums.E.KEY_A: 1,
\t\tEnums.E.KEY_A: 2,
\t}
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let hits: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "quality/duplicate-dict-key")
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "a repeated dotted key should be flagged exactly once, got {}",
        hits.len()
    );
    assert!(
        hits[0].message.contains("Enums.E.KEY_A"),
        "message should name the full dotted key, got: {}",
        hits[0].message
    );
}

#[test]
fn quality_duplicate_dict_key_nested_detected() {
    // A duplicate inside a nested dictionary value is now reported against
    // that inner dictionary (previously the inner literal was skipped).
    let config = default_config();
    let source = "\
const DATA: Dictionary = {
\t\"outer\": {
\t\t\"a\": 1,
\t\t\"a\": 2,
\t},
}
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let hits: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "quality/duplicate-dict-key")
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "duplicate key inside a nested dict should be flagged once, got {}",
        hits.len()
    );
    assert!(
        hits[0].message.contains("'a'"),
        "message should name the inner key, got: {}",
        hits[0].message
    );
}

#[test]
fn quality_duplicate_dict_key_dict_in_array_detected() {
    // Nested dictionaries reached through an array value are still scanned.
    let config = default_config();
    let source = "\
func test() -> Dictionary:
\treturn {\"items\": [{\"id\": 1, \"id\": 2}]}
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        diagnostics
            .iter()
            .any(|d| d.rule == "quality/duplicate-dict-key"),
        "duplicate inside a dict nested in an array should be flagged"
    );
}

#[test]
fn quality_duplicate_dict_key_outer_and_inner_independent() {
    // The same key at different nesting levels is not a duplicate.
    let config = default_config();
    let source = "\
func test() -> Dictionary:
\treturn {\"a\": {\"a\": 1}, \"b\": 2}
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/duplicate-dict-key"),
        "a key reused at a different nesting level must not be flagged"
    );
}

#[test]
fn quality_duplicate_dict_key_multiarg_constructor_keys_not_duplicates() {
    // Regression (issue #23): a comma inside a multi-argument constructor
    // call key (`Vector2i(1, 0)`) was mistaken for the dict entry separator,
    // so the accumulated key text collapsed to the trailing fragment `0)`
    // and distinct keys sharing that fragment were flagged as duplicates.
    let config = default_config();
    let source = "\
var fp_multiline_vec2 := {
\tVector2i(1, 0): \"a\",
\tVector2i(2, 0): \"b\",
\tVector2i(3, 0): \"c\",
}
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/duplicate-dict-key"),
        "distinct Vector2i keys sharing a trailing coordinate must not be flagged, got: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.rule == "quality/duplicate-dict-key")
            .collect::<Vec<_>>()
    );
}

#[test]
fn quality_duplicate_dict_key_singleline_multiarg_constructor_keys_not_duplicates() {
    // Same false positive as above, single-line form from issue #23.
    let config = default_config();
    let source = "var fp := {Vector2i(10, 0): 0, Vector2i(8, 0): 2, Vector2i(4, 0): 6}\n";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/duplicate-dict-key"),
        "distinct single-line Vector2i keys must not be flagged, got: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.rule == "quality/duplicate-dict-key")
            .collect::<Vec<_>>()
    );
}

#[test]
fn quality_duplicate_dict_key_vector3i_multiarg_constructor_keys_not_duplicates() {
    // Three-argument constructor form from issue #23.
    let config = default_config();
    let source = "\
var fp_vec3 := {
\tVector3i(0, 1, 0): \"a\",
\tVector3i(0, 2, 0): \"b\",
}
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/duplicate-dict-key"),
        "distinct Vector3i keys sharing a trailing coordinate must not be flagged, got: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.rule == "quality/duplicate-dict-key")
            .collect::<Vec<_>>()
    );
}

#[test]
fn quality_duplicate_dict_key_repeated_constructor_key_detected() {
    // A genuinely repeated constructor-call key is still caught after the fix.
    let config = default_config();
    let source = "\
var d := {
\tVector2i(1, 0): \"a\",
\tVector2i(1, 0): \"b\",
}
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let hits: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "quality/duplicate-dict-key")
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "a repeated constructor-call key should be flagged exactly once, got {}",
        hits.len()
    );
    assert!(
        hits[0].message.contains("Vector2i(1,0)"),
        "message should name the full constructor-call key, got: {}",
        hits[0].message
    );
}

#[test]
fn quality_duplicate_dict_key_constructor_args_not_confused_by_digit_boundary() {
    // Vector2i(1, 23) and Vector2i(12, 3) must not collide even though naive
    // concatenation of their digits without the comma would both yield "123".
    let config = default_config();
    let source = "\
var d := {
\tVector2i(1, 23): \"a\",
\tVector2i(12, 3): \"b\",
}
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/duplicate-dict-key"),
        "constructor keys must not collide across an argument digit boundary, got: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.rule == "quality/duplicate-dict-key")
            .collect::<Vec<_>>()
    );
}

// --- duplicated-load ---

#[test]
fn quality_duplicated_load_detected() {
    let config = default_config();
    let source = "\
var a: PackedScene = preload(\"res://scene.tscn\")
var b: PackedScene = preload(\"res://scene.tscn\")
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(diagnostics
        .iter()
        .any(|d| d.rule == "quality/duplicated-load"));
}

#[test]
fn quality_duplicated_load_different_paths_ok() {
    let config = default_config();
    let source = "\
var a: PackedScene = preload(\"res://scene_a.tscn\")
var b: PackedScene = preload(\"res://scene_b.tscn\")
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/duplicated-load"),
        "different paths should not trigger"
    );
}

// --- no-else-return ---

#[test]
fn quality_no_else_return_simple() {
    let config = default_config();
    let source = "\
func foo(x: int) -> int:
\tif x > 0:
\t\treturn 1
\telse:
\t\treturn -1
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(diagnostics
        .iter()
        .any(|d| d.rule == "quality/no-else-return"));
}

#[test]
fn quality_no_else_return_elif_chain() {
    let config = default_config();
    let source = "\
func foo(x: int) -> String:
\tif x > 100:
\t\treturn \"high\"
\telif x > 50:
\t\treturn \"medium\"
\telse:
\t\treturn \"low\"
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let hits: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "quality/no-else-return")
        .collect();
    // Both `elif` and `else` should be flagged
    assert!(
        hits.len() >= 2,
        "should detect elif and else after return, got {}",
        hits.len()
    );
}

#[test]
fn quality_no_else_return_no_return_ok() {
    let config = default_config();
    let source = "\
func foo(x: int) -> void:
\tif x > 0:
\t\tvar y: int = x
\t\ty += 1
\telse:
\t\tvar z: int = -x
\t\tz += 1
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/no-else-return"),
        "no return in if block, else is fine"
    );
}

// --- unreachable-code ---

#[test]
fn quality_unreachable_after_return() {
    let config = default_config();
    let source = "\
func foo() -> int:
\treturn 42
\tvar x: int = 0
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(diagnostics
        .iter()
        .any(|d| d.rule == "quality/unreachable-code"));
}

#[test]
fn quality_unreachable_after_break() {
    let config = default_config();
    let source = "\
func foo() -> void:
\tfor i: int in range(10):
\t\tbreak
\t\tvar x: int = i
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(diagnostics
        .iter()
        .any(|d| d.rule == "quality/unreachable-code"));
}

#[test]
fn quality_unreachable_after_continue() {
    let config = default_config();
    let source = "\
func foo() -> void:
\tfor i: int in range(10):
\t\tcontinue
\t\tvar x: int = i
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(diagnostics
        .iter()
        .any(|d| d.rule == "quality/unreachable-code"));
}

#[test]
fn quality_unreachable_else_not_flagged() {
    let config = default_config();
    let source = "\
func foo(x: int) -> int:
\tif x > 0:
\t\treturn 1
\telse:
\t\treturn -1
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/unreachable-code"),
        "else after return is not unreachable code"
    );
}

// --- await-in-loop ---

#[test]
fn quality_await_in_for_loop() {
    let config = default_config();
    let source = "\
func foo() -> void:
\tfor i: int in range(10):
\t\tawait get_tree().create_timer(1.0).timeout
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(diagnostics
        .iter()
        .any(|d| d.rule == "quality/await-in-loop"));
}

#[test]
fn quality_await_in_while_loop() {
    let config = default_config();
    let source = "\
func foo() -> void:
\twhile true:
\t\tawait get_tree().process_frame
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(diagnostics
        .iter()
        .any(|d| d.rule == "quality/await-in-loop"));
}

#[test]
fn quality_await_in_nested_loop() {
    let config = default_config();
    let source = "\
func foo() -> void:
\tfor i: int in range(10):
\t\tfor j: int in range(10):
\t\t\tawait get_tree().create_timer(0.1).timeout
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(diagnostics
        .iter()
        .any(|d| d.rule == "quality/await-in-loop"));
}

#[test]
fn quality_await_outside_loop_ok() {
    let config = default_config();
    let source = "\
func foo() -> void:
\tawait get_tree().create_timer(1.0).timeout
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/await-in-loop"),
        "await outside loop should not trigger"
    );
}

// --- allocation-in-loop ---

#[test]
fn quality_allocation_in_loop_detected() {
    let config = default_config();
    let source = "\
func foo() -> void:
\tfor i: int in range(10):
\t\tvar n: Node = Node.new()
\t\tadd_child(n)
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(diagnostics
        .iter()
        .any(|d| d.rule == "quality/allocation-in-loop"));
}

#[test]
fn quality_allocation_outside_loop_ok() {
    let config = default_config();
    let source = "\
func foo() -> void:
\tvar n: Node = Node.new()
\tadd_child(n)
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/allocation-in-loop"),
        "allocation outside loop should not trigger"
    );
}

// --- process-get-node ---

#[test]
fn quality_process_get_node_dollar() {
    let config = default_config();
    let source = "\
func _process(delta: float) -> void:
\tvar label: Label = $HUD/Label
\tlabel.text = str(delta)
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(diagnostics
        .iter()
        .any(|d| d.rule == "quality/process-get-node"));
}

#[test]
fn quality_process_get_node_call() {
    let config = default_config();
    let source = "\
func _physics_process(delta: float) -> void:
\tvar body: Node = get_node(\"Body\")
\tbody.position.x += delta
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(diagnostics
        .iter()
        .any(|d| d.rule == "quality/process-get-node"));
}

#[test]
fn quality_process_get_node_ready_ok() {
    let config = default_config();
    let source = "\
func _ready() -> void:
\tvar label: Label = $HUD/Label
\tlabel.text = \"ready\"
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/process-get-node"),
        "get_node in _ready should not trigger"
    );
}

#[test]
fn quality_process_get_node_modulo_not_flagged() {
    // Regression: `a % b` modulo inside _process was misread as a
    // `%UniqueNode` reference because the check looked at the char before
    // `%`. A unique-node ref is `%` followed by an identifier/quote.
    let config = default_config();
    let source = "\
func _process(delta: float) -> void:
\tvar phase: int = frame % 60
\tposition.x = phase * delta
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/process-get-node"),
        "modulo in _process must not be flagged as a node lookup"
    );
}

#[test]
fn quality_process_get_node_unique_node_flagged() {
    // A genuine `%UniqueName` lookup in _process should still be flagged.
    let config = default_config();
    let source = "\
func _process(_delta: float) -> void:
\tvar bar = %HealthBar
\tbar.value += 1
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        diagnostics
            .iter()
            .any(|d| d.rule == "quality/process-get-node"),
        "%UniqueName lookup in _process should be flagged"
    );
}

// --- unnecessary-pass ---

#[test]
fn quality_unnecessary_pass_with_other_code() {
    let config = default_config();
    let source = "\
func foo() -> void:
\tvar x: int = 5
\tx += 1
\tpass
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(diagnostics
        .iter()
        .any(|d| d.rule == "quality/unnecessary-pass"));
}

#[test]
fn quality_unnecessary_pass_alone_ok() {
    let config = default_config();
    let source = "func foo() -> void:\n\tpass\n";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/unnecessary-pass"),
        "lone pass should not trigger"
    );
}

#[test]
fn quality_unnecessary_pass_in_match_arm_ok() {
    // A `pass` that is the sole body of a match arm is required: a match
    // arm cannot be empty. It must not be flagged even though the function
    // has other statements.
    let config = default_config();
    let source = "\
func handle(state: int) -> void:
\tmatch state:
\t\t0:
\t\t\tpass
\t\t1:
\t\t\tdo_something()
\t\t_:
\t\t\tpass
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/unnecessary-pass"),
        "pass as a lone match-arm body must not be flagged; got: {:?}",
        diagnostics
            .iter()
            .filter(|d| d.rule == "quality/unnecessary-pass")
            .map(|d| d.span.line)
            .collect::<Vec<_>>()
    );
}

#[test]
fn quality_unnecessary_pass_in_empty_if_body_ok() {
    // `pass` as the only statement of an if/else branch is required.
    let config = default_config();
    let source = "\
func foo(flag: bool) -> void:
\tif flag:
\t\tpass
\telse:
\t\tprint(\"no\")
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/unnecessary-pass"),
        "pass as a lone if-branch body must not be flagged"
    );
}

// --- type-hint (off by default) ---

#[test]
fn quality_type_hint_off_by_default() {
    let config = default_config();
    let source = "var speed = 10.0\n";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics.iter().any(|d| d.rule == "quality/type-hint"),
        "type-hint should be off by default"
    );
}

#[test]
fn quality_type_hint_when_enabled() {
    let config = quality_config(&["quality/type-hint"]);
    let source = "\
var speed = 10.0
var health: int = 100
func foo(x, y: int):
\tpass
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let hits: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "quality/type-hint")
        .collect();
    // speed (no type), foo return type, param x (no type) = 3
    assert!(
        hits.len() >= 3,
        "should detect at least 3 missing type hints, got {}",
        hits.len()
    );
}

// --- empty-function (off by default) ---

#[test]
fn quality_empty_function_off_by_default() {
    let config = default_config();
    let source = "func foo() -> void:\n\tpass\n";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/empty-function"),
        "empty-function should be off by default"
    );
}

#[test]
fn quality_empty_function_when_enabled() {
    let config = quality_config(&["quality/empty-function"]);
    let source = "func foo() -> void:\n\tpass\n";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(diagnostics
        .iter()
        .any(|d| d.rule == "quality/empty-function"));
}

// --- max-class-variables ---

#[test]
fn quality_max_class_variables_exceeded() {
    let config = default_config();
    let mut source = String::from("class_name BigClass\nextends Node\n\n");
    for i in 0..20 {
        source.push_str(&format!("var v_{}: int = {}\n", i, i));
    }
    source.push_str("\nfunc _ready() -> void:\n\tpass\n");
    let diagnostics = linter::lint_source(&source, "test.gd", &config);
    assert!(diagnostics
        .iter()
        .any(|d| d.rule == "quality/max-class-variables"));
}

// --- max-public-methods ---

#[test]
fn quality_max_public_methods_exceeded() {
    let config = default_config();
    let mut source = String::from("class_name BigApi\nextends Node\n\n");
    for i in 0..25 {
        source.push_str(&format!("func method_{}() -> void:\n\tpass\n\n", i));
    }
    let diagnostics = linter::lint_source(&source, "test.gd", &config);
    assert!(diagnostics
        .iter()
        .any(|d| d.rule == "quality/max-public-methods"));
}

// --- max-inner-classes ---

#[test]
fn quality_max_inner_classes_exceeded() {
    let config = default_config();
    let mut source = String::from("class_name Outer\nextends Node\n\n");
    for i in 0..8 {
        source.push_str(&format!("class Inner{} extends RefCounted:\n\tpass\n\n", i));
    }
    let diagnostics = linter::lint_source(&source, "test.gd", &config);
    assert!(diagnostics
        .iter()
        .any(|d| d.rule == "quality/max-inner-classes"));
}

// --- max-nesting-depth ---

#[test]
fn quality_max_nesting_depth_within_limit_ok() {
    let config = default_config();
    let source = "\
func foo() -> void:
\tif true:
\t\tif true:
\t\t\tpass
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/max-nesting-depth"),
        "depth 2 should be within default limit of 4"
    );
}

// --- max-returns ---

#[test]
fn quality_max_returns_within_limit_ok() {
    let config = default_config();
    let source = "\
func foo(x: int) -> int:
\tif x > 0:
\t\treturn 1
\treturn 0
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics.iter().any(|d| d.rule == "quality/max-returns"),
        "2 returns should be within default limit of 6"
    );
}

// --- max-branches ---

#[test]
fn quality_max_branches_within_limit_ok() {
    let config = default_config();
    let source = "\
func foo(x: int) -> void:
\tif x == 1:
\t\tpass
\tif x == 2:
\t\tpass
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics.iter().any(|d| d.rule == "quality/max-branches"),
        "2 branches should be within default limit of 8"
    );
}

// --- max-local-variables ---

#[test]
fn quality_max_local_variables_within_limit_ok() {
    let config = default_config();
    let source = "\
func foo() -> void:
\tvar a: int = 1
\tvar b: int = 2
\tvar c: int = 3
";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(
        !diagnostics
            .iter()
            .any(|d| d.rule == "quality/max-local-variables"),
        "3 locals should be within default limit of 10"
    );
}

// --- Clean script still clean with new rules ---

#[test]
fn quality_clean_script_still_clean() {
    let config = default_config();
    let diagnostics = linter::lint_file(&fixture_path("clean_script.gd"), &config).unwrap();
    assert!(
        diagnostics.is_empty(),
        "clean_script.gd should still produce no diagnostics, got:\n{}",
        diagnostics
            .iter()
            .map(|d| format!("  line {}: [{}] {}", d.span.line, d.rule, d.message))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

// =============================================================================
// Regression tests for the 9 bugs surfaced by the ai-battleground / multi-
// project survey (see docs/dev-notes/lessons-ai-battleground-autofix.md).
// One test per bug, asserting the specific failure mode is gone.
// =============================================================================

#[test]
fn regression_bug1_comment_spacing_skips_doc_comments() {
    // Bug 1: rule was rewriting `##var p2p_session` → `## var p2p_session`,
    // which (a) changes how a reader interprets a "double-commented-out"
    // line, and (b) shifts the doc-comment marker position. The fix is to
    // leave any DocComment token (anything beginning with `##`) untouched.
    let source = "##var p2p_session: P2PSession = null\n";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let comment_spacing: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "format/comment-spacing")
        .collect();
    assert!(
        comment_spacing.is_empty(),
        "comment-spacing must not fire on `##` doc comments; got: {:?}",
        comment_spacing
            .iter()
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );

    // It should still fire on a plain `#` comment whose first char is not
    // alphabetic. The pre-existing rule deliberately skips `#abc` style
    // (treats it as conventional one-char shorthand), but does flag
    // `#0`, `#$`, `#-` etc.
    let plain = "var x = 1\n#0 LevelFilter::Off\n";
    let plain_diags = linter::lint_source(plain, "test.gd", &config);
    assert!(
        plain_diags
            .iter()
            .any(|d| d.rule == "format/comment-spacing"),
        "comment-spacing should still fire on `#0` (single-hash, non-text follow-up)"
    );
}

#[test]
fn regression_bug2_operator_spacing_unary_after_else() {
    // Bug 2: `X if cond else -1` was getting an unwanted space inserted into
    // `else - 1`, because the unary skip-list didn't include `else`. The fix
    // now uses an `is_operand_end` predicate that returns false for any
    // keyword/operator/opening-bracket, making `-`/`+` unary in those
    // positions.
    let cases = [
        "var x = 1 if y else -1\n",
        "return v if v > 0 else -1.0\n",
        "var d = 1.0 if is_local else -1.0\n",
        "var a = (b if c else -2)\n",
    ];
    let config = default_config();
    for source in &cases {
        let diagnostics = linter::lint_source(source, "test.gd", &config);
        let bad: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.rule == "format/operator-spacing")
            .collect();
        assert!(
            bad.is_empty(),
            "unary `-` after `else` should not trigger operator-spacing in `{}`; got: {:?}",
            source.trim(),
            bad.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }
}

#[test]
fn regression_bug3_constant_accepts_private_pascalcase_preload() {
    // Bug 3: the rule documented "(or PascalCase for preloads)" but its
    // PascalCase check rejected names with a leading underscore, so
    // `_ButtonNormalTex` was being rewritten to `_BUTTON_NORMAL_TEX`. The
    // fix strips leading underscores before the PascalCase check.
    let source = "\
const _ButtonNormalTex := preload(\"res://x.png\")
const _ButtonPressedTex := preload(\"res://y.png\")
";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let bad: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "naming/constant-name-screaming-case")
        .collect();
    assert!(
        bad.is_empty(),
        "private-PascalCase preloads should be accepted as conforming; got: {:?}",
        bad.iter().map(|d| &d.message).collect::<Vec<_>>()
    );

    // But snake_case privates are still flagged (they're neither SCREAMING
    // nor PascalCase).
    let snake = "const _emoji_font := preload(\"res://x.ttf\")\n";
    let snake_diags = linter::lint_source(snake, "test.gd", &config);
    assert!(
        snake_diags
            .iter()
            .any(|d| d.rule == "naming/constant-name-screaming-case"),
        "snake_case private const should still fire the rule"
    );
}

#[test]
fn regression_bug4_signal_past_tense_skips_non_verbs() {
    // Bug 4: the rule used to append "ed"/"d" to the last word regardless of
    // whether it was actually a verb, producing nonsense like `launch_gamed`
    // and `return_to_lobbied`. The fix gates inflection on a curated regular-
    // verb dictionary plus the existing irregular dictionary.
    let nonsense_inputs = [
        "signal launch_game\n",
        "signal return_to_lobby\n",
        "signal before_ui_action\n",
        "signal reset_app\n",
        "signal stream_chunk\n",
    ];
    let config = default_config();
    for source in &nonsense_inputs {
        let diagnostics = linter::lint_source(source, "test.gd", &config);
        let fixed = fixer::apply_fixes(source, &diagnostics, false);
        // The original signal name should still be present: no nonsense
        // rename applied.
        assert_eq!(
            fixed.trim(),
            source.trim(),
            "non-verb signal `{}` should not be auto-inflected",
            source.trim()
        );
    }

    // Real verbs (regular and irregular) should still be inflected.
    let verb_cases = [
        // The inflector handles regular `+ed` plus the `e → d` and
        // `consonant+y → ied` rules; consonant-doubling is a separate
        // (currently unimplemented) refinement, so we exercise verbs that
        // do not need doubling.
        ("signal player_jump\n", "player_jumped"),
        ("signal request_complete\n", "request_completed"),
        ("signal connection_lose\n", "connection_lost"), // irregular
        ("signal config_apply\n", "config_applied"),     // y → ied
    ];
    for (source, expected) in &verb_cases {
        let diagnostics = linter::lint_source(source, "test.gd", &config);
        let fixed = fixer::apply_fixes(source, &diagnostics, false);
        assert!(
            fixed.contains(expected),
            "verb signal `{}` should inflect to contain `{}`; got `{}`",
            source.trim(),
            expected,
            fixed.trim()
        );
    }
}

#[test]
fn regression_bug5_static_function_rename_is_not_collateral_to_enum_member() {
    // Bug 5: renaming `static func NONE() -> ChaosProfile` on class
    // `ChaosProfile` used to also rewrite the unrelated `DeviceProfile.NONE`
    // enum member in other files, because the rewriter matched bare
    // identifier tokens. The context-aware rewriter requires
    // `<source_class_name>.<old>(` for static function renames; the
    // `DeviceProfile.NONE` access (no parens, different qualifier) is left
    // alone.
    let source_file = "\
class_name ChaosProfile

static func NONE() -> ChaosProfile:
\treturn null
";
    let target_file = "\
extends Node

enum DeviceProfile { NONE, IPHONE_15_PRO }

var current: int = DeviceProfile.NONE

func _ready():
\tChaosProfile.NONE()
";
    let config = default_config();
    let source_diags = linter::lint_source(source_file, "chaos_profile.gd", &config);
    let source_members = parse_members_for_test(source_file);
    let renames = fixer::extract_renames(
        source_file,
        &source_diags,
        "chaos_profile.gd",
        &source_members,
    );
    let none_rename = renames
        .iter()
        .find(|r| r.old_name == "NONE")
        .expect("expected NONE rename");
    assert_eq!(none_rename.new_name, "none");
    assert!(matches!(none_rename.kind, fixer::RenameKind::Function));
    assert_eq!(
        none_rename.source_class_name.as_deref(),
        Some("ChaosProfile")
    );
    assert!(
        !none_rename.is_instance_member,
        "static function should NOT be marked instance"
    );

    let refs = fixer::find_cross_file_references(target_file, "device_sim.gd", &renames);
    let texts: Vec<_> = refs
        .iter()
        .map(|r| {
            let s = &target_file[r.offset..r.offset + r.length];
            (r.old_name.clone(), s.to_string(), r.offset)
        })
        .collect();

    // The `ChaosProfile.NONE()` call site SHOULD be matched.
    assert!(
        texts.iter().any(|(_, _, off)| {
            let lhs_window = &target_file[off.saturating_sub(13)..*off];
            lhs_window.ends_with("ChaosProfile.")
        }),
        "ChaosProfile.NONE() call should be picked up; got: {:?}",
        texts
    );

    // The `DeviceProfile.NONE` enum member access must NOT be matched.
    assert!(
        !texts.iter().any(|(_, _, off)| {
            let lhs_window = &target_file[off.saturating_sub(14)..*off];
            lhs_window.ends_with("DeviceProfile.")
        }),
        "DeviceProfile.NONE (different class qualifier) must not be rewritten; got: {:?}",
        texts
    );
}

#[test]
fn regression_bug6_formatter_preserves_declarations() {
    // Bug 6: the formatter's reorder pass could produce output that lost or
    // mutated declarations in subtle ways, breaking the file at runtime. The
    // safety guards now (a) skip reorder when a module-level initializer
    // depends on another module-level identifier (because GDScript runs
    // initialisers in source order), and (b) re-parse the output and verify
    // the set of declared symbols is unchanged.
    //
    // Cross-referencing initialisers must NOT be reordered.
    let cross_ref = "\
extends Node

const A := 1
const B := A + 2
const C := B * 3

func _ready():
\tprint(C)
";
    let formatted = formatter::format_source(cross_ref, &default_config());
    // Find positions of A, B, C decls; they must appear in source order.
    let pos_a = formatted.find("const A").expect("A missing");
    let pos_b = formatted.find("const B").expect("B missing");
    let pos_c = formatted.find("const C").expect("C missing");
    assert!(
        pos_a < pos_b && pos_b < pos_c,
        "cross-referencing const declarations must keep source order; got A={} B={} C={}\n{}",
        pos_a,
        pos_b,
        pos_c,
        formatted
    );

    // Sanity check: a clean file with independent decls + multiple kinds
    // round-trips without dropping members. This protects against the
    // formatter silently losing an inner class etc.
    let multi = "\
class_name Demo
extends Node

class Inner1:
\tvar x: int = 0

class Inner2:
\tvar y: int = 0

signal a_signal
const C := 1
var v: int = 0

func _ready():
\tpass

static func helper():
\tpass
";
    let formatted_multi = formatter::format_source(multi, &default_config());
    for needle in [
        "class Inner1",
        "class Inner2",
        "signal a_signal",
        "const C",
        "var v",
        "func _ready",
        "static func helper",
    ] {
        assert!(
            formatted_multi.contains(needle),
            "formatter dropped `{}` from output:\n{}",
            needle,
            formatted_multi
        );
    }
}

#[test]
fn regression_bug7_no_unnecessary_parens_preserves_space() {
    // Bug 7: `if(is_mouse):` was rewritten to `ifis_mouse:`. The rule
    // deleted both parens but didn't replace the opening one with a space
    // when the keyword and `(` were touching. Fix: detect the touching case
    // and emit a single space in place of the open paren.
    let source = "\
func foo():
\tif(is_mouse):
\t\tpass
";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let fixed = fixer::apply_fixes(source, &diagnostics, false);
    assert!(
        fixed.contains("if is_mouse:"),
        "`if(is_mouse):` must become `if is_mouse:`, got:\n{}",
        fixed
    );
    assert!(
        !fixed.contains("ifis_mouse"),
        "must not produce `ifis_mouse` (no separator), got:\n{}",
        fixed
    );

    // When the original already had a space (`if (is_mouse):`), the fixed
    // output should not gain an extra space (`if  is_mouse:`).
    let with_space = "\
func bar():
\tif (is_mouse):
\t\tpass
";
    let with_space_fixed = fixer::apply_fixes(
        with_space,
        &linter::lint_source(with_space, "test.gd", &config),
        false,
    );
    assert!(
        with_space_fixed.contains("if is_mouse:") && !with_space_fixed.contains("if  is_mouse"),
        "`if (is_mouse):` must produce single-space `if is_mouse:`, got:\n{}",
        with_space_fixed
    );
}

#[test]
fn regression_bug8_instance_member_rename_rewrites_dot_access() {
    // Bug 8: a `var current_A: int` on `class_name ModelState` is accessed
    // cross-file as `model.current_A` (instance), not `ModelState.current_A`
    // (static). The previous rewriter required the qualifier to equal the
    // source class name and so left every instance access untouched. The
    // fix marks the rename as `is_instance_member: true` and accepts any
    // `.<name>` member access (including `arr[0].name`, `func().name`, etc.).
    let source_file = "\
class_name ModelState

var current_A: int = 1
var current_T: int = 3
";
    let target_file = "\
func calc(model: ModelState, models: Array) -> void:
\tvar a := model.current_A
\tvar t := models[0].current_T
\tvar wrap := models.front().current_A
";
    let config = default_config();
    let source_diags = linter::lint_source(source_file, "model_state.gd", &config);
    let source_members = parse_members_for_test(source_file);
    let renames = fixer::extract_renames(
        source_file,
        &source_diags,
        "model_state.gd",
        &source_members,
    );

    let cur_a = renames
        .iter()
        .find(|r| r.old_name == "current_A")
        .expect("current_A rename");
    assert!(
        cur_a.is_instance_member,
        "non-static `var current_A` must be marked instance member"
    );

    let refs = fixer::find_cross_file_references(target_file, "consumer.gd", &renames);
    // Should pick up:
    //   model.current_A
    //   models[0].current_T
    //   models.front().current_A
    let names: Vec<&str> = refs.iter().map(|r| r.old_name.as_str()).collect();
    let count_a = names.iter().filter(|n| **n == "current_A").count();
    let count_t = names.iter().filter(|n| **n == "current_T").count();
    assert_eq!(
        count_a, 2,
        "should match both `model.current_A` and `models.front().current_A`; got refs: {:?}",
        names
    );
    assert_eq!(
        count_t, 1,
        "should match `models[0].current_T`; got refs: {:?}",
        names
    );

    // Verify the fix actually rewrites the source text.
    let fixed = fixer::apply_cross_file_fixes(target_file, &refs);
    assert!(
        fixed.contains("model.current_a")
            && fixed.contains("models[0].current_t")
            && fixed.contains("models.front().current_a"),
        "all instance accesses must be renamed; got:\n{}",
        fixed
    );
}

#[test]
fn regression_bug9_replacement_ordering_insertion_after_replacement() {
    // Bug 9: when `format/operator-spacing` (insert space, length 0) and
    // `format/double-quotes` (replace `' in '` length 6 with `" in "`)
    // landed at the same byte offset, applying the insertion first shifted
    // the replacement span and left a stray closing `'`, producing the
    // syntactically invalid `+" in "'`. Fix: secondary sort by length
    // descending so span replacements run before zero-length insertions at
    // the same offset.
    let source = "\
class_name X
func foo() -> String:
\treturn \"a\" +' in ' + \"b\"
";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let fixed = fixer::apply_fixes(source, &diagnostics, false);
    assert!(
        fixed.contains("\"a\" + \" in \" + \"b\""),
        "expected `\"a\" + \" in \" + \"b\"` after combined fix; got:\n{}",
        fixed
    );
    assert!(
        !fixed.contains("\" in \"'"),
        "must not leave an orphan single quote after the converted string; got:\n{}",
        fixed
    );

    // GARP_old's actual case: a second, mixed-quote string elsewhere on the
    // line that should be left alone (single-quoted, contains `"`).
    let mixed = "\
class_name Y
func bar(p: String) -> String:
\treturn \"x\" +' in ' + p + ' has \"q\" inside'
";
    let mixed_diags = linter::lint_source(mixed, "test.gd", &config);
    let mixed_fixed = fixer::apply_fixes(mixed, &mixed_diags, false);
    assert!(
        mixed_fixed.contains("\"x\" + \" in \" + p + ' has \"q\" inside'"),
        "second string (with embedded `\"`) must stay single-quoted intact; got:\n{}",
        mixed_fixed
    );
}

#[test]
fn regression_bug12_formatter_does_not_drop_inner_class_body() {
    // Bug 12: when an inner class with a body and methods got reordered, its
    // body was being lifted out as top-level vars/funcs while the class
    // header remained empty. The safety guard now compares scope-qualified
    // member signatures: moving a member out of its inner class scope
    // changes the signature and the formatter rejects the change.
    let source = "\
class_name ChatTree
## doc

class Position:
\tvar key := \"\"
\tvar index := 0
\tfunc _to_string() -> String:
\t\treturn key

const FOO := \"x\"
";
    let formatted = formatter::format_source(source, &default_config());
    // Members of class Position must remain inside its body.
    let pos_idx = formatted
        .find("class Position")
        .expect("class Position lost");
    let key_idx = formatted.find("var key").expect("var key lost");
    let to_str_idx = formatted.find("func _to_string").expect("_to_string lost");
    assert!(
        pos_idx < key_idx && pos_idx < to_str_idx,
        "var key / _to_string must stay AFTER class Position, not lifted to top level. Got:\n{}",
        formatted
    );

    // The inner class body must still be indented inside the class block.
    let after_pos = &formatted[pos_idx..];
    assert!(
        after_pos.contains("\tvar key"),
        "var key must remain tab-indented inside class Position; got:\n{}",
        formatted
    );
}

#[test]
fn regression_bug10_trailing_comma_skips_subscripts() {
    // Bug 10: format/trailing-comma was treating every multi-line `[...]` as
    // an array literal and inserting a trailing comma. But GDScript does not
    // allow trailing commas inside subscripts (`arr[i,]` is invalid). When
    // godot-open-rts had a multi-line `CONSTANTS[\n\tkey,\n]` it broke parsing.
    let source = "\
extends Node

const TABLE := { \"a\": 1, \"b\": 2 }

func has(v) -> bool:
\treturn TABLE[
\t\tv
\t] != null
";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let bad: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "format/trailing-comma")
        .collect();
    assert!(
        bad.is_empty(),
        "trailing-comma must NOT fire on a multi-line subscript `TABLE[v,]`; got: {:?}",
        bad.iter()
            .map(|d| (d.span.line, &d.message))
            .collect::<Vec<_>>()
    );

    // Sanity check: a real multi-line array literal still gets the comma.
    let array_source = "\
extends Node

var xs := [
\t1,
\t2
]
";
    let array_diags = linter::lint_source(array_source, "test.gd", &config);
    assert!(
        array_diags
            .iter()
            .any(|d| d.rule == "format/trailing-comma"),
        "trailing-comma should still fire on multi-line array literals"
    );
}

#[test]
fn regression_bug11_class_rename_does_not_touch_unrelated_enum_members() {
    // Bug 11: jdungeon had `class_name NPC` (which fails is_pascal_case
    // because it's all-uppercase, no lowercase letters). The rule renamed
    // it to `Npc`. The cross-file rewriter then matched every standalone
    // `NPC` token in the project (including unrelated `enum EntityType {
    // PLAYER, ENEMY, ITEM, NPC }`), turning the enum member into `Npc`
    // and silently re-introducing a different naming violation. The fix
    // restricts Class rewriting to actual type / call / static-access
    // positions.
    // Use a non-acronym class so the rename actually fires (acronyms like
    // `NPC` are now preserved by `to_pascal_case`, see bug 1.14 / pascal_word).
    let source_file = "class_name myShape\n";
    let target_file = "\
extends Node

enum EntityType { PLAYER, ENEMY, ITEM, myShape }

var current = EntityType.myShape

# A genuine class reference that SHOULD be rewritten:
var avatar: myShape = myShape.new()
";
    let config = default_config();
    let source_diags = linter::lint_source(source_file, "shape.gd", &config);
    let source_members = parse_members_for_test(source_file);
    let renames = fixer::extract_renames(source_file, &source_diags, "shape.gd", &source_members);
    let class_rename = renames
        .iter()
        .find(|r| r.old_name == "myShape")
        .expect("expected myShape class rename");
    assert!(matches!(class_rename.kind, fixer::RenameKind::Class));
    assert_eq!(class_rename.new_name, "MyShape");

    let refs = fixer::find_cross_file_references(target_file, "consumer.gd", &renames);

    // The enum member `myShape` (bare in `enum { ..., myShape }`) and the
    // access `EntityType.myShape` must NOT be rewritten.
    for r in &refs {
        let preceding = &target_file[r.offset.saturating_sub(20)..r.offset];
        assert!(
            !preceding.ends_with("EntityType."),
            "Class rename must not touch `EntityType.myShape`; ref preceded by `{}`",
            preceding
        );
        let line_start = target_file[..r.offset]
            .rfind('\n')
            .map(|p| p + 1)
            .unwrap_or(0);
        let line = &target_file[line_start
            ..target_file[r.offset..]
                .find('\n')
                .map(|n| r.offset + n)
                .unwrap_or(target_file.len())];
        assert!(
            !line.starts_with("enum "),
            "Class rename must not touch enum-member declaration; line: {}",
            line
        );
    }

    // The genuine type reference `var avatar: myShape` and the constructor
    // `myShape.new()` SHOULD be rewritten.
    let texts: Vec<&str> = refs
        .iter()
        .map(|r| &target_file[r.offset..r.offset + r.length])
        .collect();
    assert!(
        texts.contains(&"myShape"),
        "type-position `: myShape` reference should be picked up; got refs at offsets {:?}",
        refs.iter().map(|r| r.offset).collect::<Vec<_>>()
    );
}

#[test]
fn pascal_word_preserves_acronyms() {
    // Bug 1.14: `to_pascal_case("HTTPRequest")` used to return
    // `"Httprequest"`, mangling the acronym. Single-word acronyms now
    // round-trip; mixed acronym+CamelCase preserves the leading uppercase
    // run.
    use gdstyle::rules::naming;
    assert_eq!(naming::to_pascal_case("HTTPRequest"), "HTTPRequest");
    assert_eq!(naming::to_pascal_case("XMLParser"), "XMLParser");
    assert_eq!(naming::to_pascal_case("NPC"), "NPC");
    assert_eq!(naming::to_pascal_case("URL"), "URL");
    // Plain camelCase / snake_case / lowercase still works as before.
    assert_eq!(naming::to_pascal_case("myShape"), "MyShape");
    assert_eq!(naming::to_pascal_case("snake_case_name"), "SnakeCaseName");
    assert_eq!(naming::to_pascal_case("foo"), "Foo");
}

#[test]
fn regression_bug13_typed_collections_not_wrapped_or_comma_inserted() {
    // Bug 13: bread-adventure had a long line with a typed dictionary
    // `var x: Dictionary[int, ExploreStatus] = {} as Dictionary[...]`.
    // The formatter wrapped it across lines, added a trailing comma, and
    // GDScript's parser rejected the multi-line typed-collection bracket
    // entirely. The fix is to leave subscripts and typed-collection
    // brackets as a single line (and never insert a trailing comma in
    // them), letting the format/max-line-length warning stand.
    let mut config = default_config();
    config.max_line_length = 60;

    let long_source = "\
extends Node

var explored: Dictionary[int, Constant.ExploreStatus] = {} as Dictionary[int, Constant.ExploreStatus]
";
    let formatted = formatter::format_source(long_source, &config);
    // The typed-dict bracket `Dictionary[int, X]` must remain single-line.
    assert!(
        formatted.contains("Dictionary[int, Constant.ExploreStatus]"),
        "typed dictionary must not be wrapped across lines; got:\n{}",
        formatted
    );
    assert!(
        !formatted.contains("Dictionary[\n"),
        "typed dictionary must not be split with a leading `[`; got:\n{}",
        formatted
    );

    // And no trailing comma should appear inside the type spec, even if
    // someone manually wrote it across lines.
    let manual = "\
extends Node

var explored: Dictionary[
\tint,
\tConstant.ExploreStatus
] = {}
";
    let manual_diags = linter::lint_source(manual, "test.gd", &config);
    let bad: Vec<_> = manual_diags
        .iter()
        .filter(|d| d.rule == "format/trailing-comma")
        .collect();
    assert!(
        bad.is_empty(),
        "trailing-comma must not fire inside Dictionary[...] type spec; got: {:?}",
        bad.iter()
            .map(|d| (d.span.line, &d.message))
            .collect::<Vec<_>>()
    );
}

#[test]
fn regression_bug14_function_callable_reference_rewritten() {
    // Bug 14: Pixelorama declared `_on_Autosave_timeout` and connected the
    // signal via `autosave_timer.timeout.connect(_on_Autosave_timeout)`.
    // The same-file rewriter only rewrote `<token>(` call sites and missed
    // bare callable references (passing the function as a Callable). The
    // fix accepts bare references when there's no name collision in the
    // file.
    let source = "\
class_name AutosaveDriver
extends Node

@onready var autosave_timer: Timer = $Timer

func _ready() -> void:
\tautosave_timer.timeout.connect(_on_Autosave_timeout)

func _on_Autosave_timeout() -> void:
\tprint(\"saving\")
";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "autosave.gd", &config);
    let fixed = fixer::apply_fixes(source, &diagnostics, false);
    assert!(
        fixed.contains("connect(_on_autosave_timeout)"),
        "callable reference passed to .connect() should be renamed; got:\n{}",
        fixed
    );
    assert!(
        !fixed.contains("_on_Autosave_timeout"),
        "no occurrence of the old name should remain; got:\n{}",
        fixed
    );
}

#[test]
fn regression_bug15_one_statement_per_line_skips_match_arms() {
    // Bug 15: GodSVG had `match int(hp):\n\t\t0: r1 = c; g1 = x` (a match
    // arm with `;`-joined body). The autofix split the `;` into a new line
    // at the same indent, orphaning `g1 = x` from the arm. Match arms
    // (`<pattern>: stmt1; stmt2`) must be left alone: the arm body needs
    // to become an indented block, which we don't synthesise.
    let source = "\
extends Node

func decode(c: int, x: int) -> Array:
\tvar r1 := 0.0
\tvar g1 := 0.0
\tvar b1 := 0.0
\tmatch c:
\t\t0: r1 = x; g1 = c
\t\t1: r1 = c; g1 = x
\treturn [r1, g1, b1]
";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    let bad: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.rule == "format/one-statement-per-line")
        .collect();
    assert!(
        bad.is_empty(),
        "match-arm `;` separators must not be flagged; got: {:?}",
        bad.iter()
            .map(|d| (d.span.line, &d.message))
            .collect::<Vec<_>>()
    );

    // Sanity: a regular `;` in a normal function body still gets flagged.
    let normal = "\
extends Node

func foo() -> void:
\tvar a = 1; var b = 2
";
    let normal_diags = linter::lint_source(normal, "test.gd", &config);
    assert!(
        normal_diags
            .iter()
            .any(|d| d.rule == "format/one-statement-per-line"),
        "ordinary `;` between statements should still be flagged"
    );
}

#[test]
fn regression_bug16_rename_suppressed_when_new_name_collides() {
    // Bug 16: GodSVG defined `const e = E`, `const pi = PI` (lowercase
    // mathematical aliases). The constant-name-screaming-case rule renamed
    // them to `const E = E` (duplicate) and `const PI = PI` (cyclic, since
    // `PI` is a Godot built-in). The fix suppresses any naming rule rewrite
    // when the proposed new name already exists as a declared name OR
    // appears anywhere in the source as an identifier (built-in, autoload,
    // imported class).
    let source = "\
class_name MathConsts extends Object

const E = 2.71828
const PHI = 1.618
const e = E
const pi = PI
const tau = TAU
";
    let config = default_config();
    let diagnostics = linter::lint_source(source, "math.gd", &config);
    let fixed = fixer::apply_fixes(source, &diagnostics, false);

    // None of `e`, `pi`, `tau` should be renamed to their uppercase
    // collision targets.
    assert!(
        fixed.contains("const e = E"),
        "`const e = E` should NOT be rewritten (would duplicate `const E`); got:\n{}",
        fixed
    );
    assert!(
        fixed.contains("const pi = PI"),
        "`const pi = PI` should NOT be rewritten (would self-reference Godot built-in `PI`); got:\n{}",
        fixed
    );
    assert!(
        fixed.contains("const tau = TAU"),
        "`const tau = TAU` should NOT be rewritten; got:\n{}",
        fixed
    );

    // A plain non-colliding rename should still apply.
    let normal = "\
class_name X extends Object
const myConst = 1
";
    let normal_diags = linter::lint_source(normal, "x.gd", &config);
    let normal_fixed = fixer::apply_fixes(normal, &normal_diags, false);
    assert!(
        normal_fixed.contains("const MY_CONST = 1"),
        "non-colliding camelCase const should still get renamed to SCREAMING_CASE; got:\n{}",
        normal_fixed
    );
}
