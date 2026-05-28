pub mod formatting;
pub mod naming;
pub mod ordering;
pub mod quality;

use crate::ast::ScriptFile;
use crate::config::Config;
use crate::diagnostic::Diagnostic;
use crate::token::Token;

/// Run all enabled lint rules on a parsed script file.
pub fn run_all_rules(file: &ScriptFile, tokens: &[Token], config: &Config) -> Vec<Diagnostic> {
    run_all_rules_with_source(file, tokens, config, None)
}

/// Run all enabled lint rules, optionally with source for token-gap analysis.
pub fn run_all_rules_with_source(
    file: &ScriptFile,
    tokens: &[Token],
    config: &Config,
    source: Option<&str>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Naming rules.
    if config.is_rule_enabled("naming/class-name-pascal-case") {
        naming::check_class_name_pascal_case(file, &mut diagnostics);
    }
    if config.is_rule_enabled("naming/function-name-snake-case") {
        naming::check_function_name_snake_case(file, &mut diagnostics);
    }
    if config.is_rule_enabled("naming/variable-name-snake-case") {
        naming::check_variable_name_snake_case(file, &mut diagnostics);
    }
    if config.is_rule_enabled("naming/constant-name-screaming-case") {
        naming::check_constant_name_screaming_case(file, &mut diagnostics);
    }
    if config.is_rule_enabled("naming/signal-name-snake-case") {
        naming::check_signal_name_snake_case(file, &mut diagnostics);
    }
    if config.is_rule_enabled("naming/enum-name-pascal-case") {
        naming::check_enum_name_pascal_case(file, &mut diagnostics);
    }
    if config.is_rule_enabled("naming/enum-member-screaming-case") {
        naming::check_enum_member_screaming_case(file, &mut diagnostics);
    }
    if config.is_rule_enabled("naming/file-name-snake-case") {
        naming::check_file_name_snake_case(file, &mut diagnostics);
    }
    if config.is_rule_enabled("naming/signal-past-tense") {
        naming::check_signal_past_tense(file, &mut diagnostics);
    }
    if config.is_rule_enabled("naming/private-underscore-prefix") {
        naming::check_private_underscore_prefix(file, &mut diagnostics);
    }
    if config.is_rule_enabled("naming/node-name-pascal-case") {
        naming::check_node_name_pascal_case(tokens, file, &mut diagnostics);
    }

    // Formatting rules.
    if config.is_rule_enabled("format/max-line-length") {
        formatting::check_max_line_length(file, config, &mut diagnostics);
    }
    if config.is_rule_enabled("format/trailing-whitespace") {
        formatting::check_trailing_whitespace(file, &mut diagnostics);
    }
    if config.is_rule_enabled("format/trailing-newline") {
        formatting::check_trailing_newline(file, &mut diagnostics);
    }
    if config.is_rule_enabled("format/no-tabs-as-spaces") {
        formatting::check_indentation_style(file, config, &mut diagnostics);
    }
    if config.is_rule_enabled("format/boolean-operators") {
        formatting::check_boolean_operators(tokens, file, &mut diagnostics);
    }
    if config.is_rule_enabled("format/double-quotes") {
        formatting::check_double_quotes(tokens, file, &mut diagnostics);
    }
    if config.is_rule_enabled("format/comment-spacing") {
        formatting::check_comment_spacing(tokens, file, &mut diagnostics);
    }
    if config.is_rule_enabled("format/no-unnecessary-parens") {
        formatting::check_unnecessary_parens(tokens, file, &mut diagnostics);
    }
    if config.is_rule_enabled("format/number-literals") {
        formatting::check_number_literals(tokens, file, &mut diagnostics);
    }
    if config.is_rule_enabled("format/one-statement-per-line") {
        formatting::check_one_statement_per_line(tokens, file, &mut diagnostics);
    }
    if config.is_rule_enabled("format/blank-lines") {
        formatting::check_blank_lines(file, &mut diagnostics);
    }
    if config.is_rule_enabled("format/trailing-comma") {
        formatting::check_trailing_comma(tokens, file, &mut diagnostics);
    }
    if config.is_rule_enabled("format/operator-spacing") {
        if let Some(src) = source {
            formatting::check_operator_spacing(tokens, file, src, &mut diagnostics);
        }
    }
    if config.is_rule_enabled("format/colon-spacing") {
        if let Some(src) = source {
            formatting::check_colon_spacing(tokens, file, src, &mut diagnostics);
        }
    }
    if config.is_rule_enabled("format/comma-spacing") {
        if let Some(src) = source {
            formatting::check_comma_spacing(tokens, file, src, &mut diagnostics);
        }
    }
    if config.is_rule_enabled("format/float-literal-zeros") {
        formatting::check_float_literal_zeros(tokens, file, &mut diagnostics);
    }
    if config.is_rule_enabled("format/large-number-underscores") {
        formatting::check_large_number_underscores(tokens, file, &mut diagnostics);
    }
    if config.is_rule_enabled("format/enum-one-per-line") {
        formatting::check_enum_one_per_line(file, source, &mut diagnostics);
    }

    // Ordering rules.
    if config.is_rule_enabled("order/class-member-order") {
        ordering::check_class_member_order(file, &mut diagnostics);
    }

    // Quality rules.
    if config.is_rule_enabled("quality/max-function-length") {
        quality::check_max_function_length(file, config, &mut diagnostics);
    }
    if config.is_rule_enabled("quality/max-file-length") {
        quality::check_max_file_length(file, config, &mut diagnostics);
    }
    if config.is_rule_enabled("quality/max-parameters") {
        quality::check_max_parameters(file, config, &mut diagnostics);
    }
    if config.is_rule_enabled("quality/unnecessary-pass") {
        quality::check_unnecessary_pass_in_functions(file, &mut diagnostics);
    }
    if config.is_rule_enabled("quality/no-debug-print") {
        quality::check_no_debug_print(tokens, file, &mut diagnostics);
    }
    if config.is_rule_enabled("quality/self-comparison") {
        quality::check_self_comparison(tokens, file, &mut diagnostics);
    }
    if config.is_rule_enabled("quality/no-self-assign") {
        quality::check_no_self_assign(tokens, file, &mut diagnostics);
    }
    if config.is_rule_enabled("quality/duplicate-dict-key") {
        quality::check_duplicate_dict_key(tokens, file, &mut diagnostics);
    }
    if config.is_rule_enabled("quality/duplicated-load") {
        quality::check_duplicated_load(tokens, file, &mut diagnostics);
    }
    if config.is_rule_enabled("quality/type-hint") {
        quality::check_type_hint(file, &mut diagnostics);
    }
    if config.is_rule_enabled("quality/empty-function") {
        quality::check_empty_function(file, &mut diagnostics);
    }
    if config.is_rule_enabled("quality/max-class-variables") {
        quality::check_max_class_variables(file, config, &mut diagnostics);
    }
    if config.is_rule_enabled("quality/max-public-methods") {
        quality::check_max_public_methods(file, config, &mut diagnostics);
    }
    if config.is_rule_enabled("quality/max-inner-classes") {
        quality::check_max_inner_classes(file, config, &mut diagnostics);
    }
    if config.is_rule_enabled("quality/no-else-return") {
        quality::check_no_else_return(file, &mut diagnostics);
    }
    if config.is_rule_enabled("quality/unreachable-code") {
        quality::check_unreachable_code(file, &mut diagnostics);
    }
    if config.is_rule_enabled("quality/await-in-loop") {
        quality::check_await_in_loop(file, &mut diagnostics);
    }
    if config.is_rule_enabled("quality/allocation-in-loop") {
        quality::check_allocation_in_loop(file, &mut diagnostics);
    }
    if config.is_rule_enabled("quality/process-get-node") {
        quality::check_process_get_node(file, &mut diagnostics);
    }
    if config.is_rule_enabled("quality/max-nesting-depth") {
        quality::check_max_nesting_depth(file, config, &mut diagnostics);
    }
    if config.is_rule_enabled("quality/max-returns") {
        quality::check_max_returns(file, config, &mut diagnostics);
    }
    if config.is_rule_enabled("quality/max-branches") {
        quality::check_max_branches(file, config, &mut diagnostics);
    }
    if config.is_rule_enabled("quality/max-local-variables") {
        quality::check_max_local_variables(file, config, &mut diagnostics);
    }

    // Sort diagnostics by line number.
    diagnostics.sort_by_key(|d| (d.span.line, d.span.column));
    diagnostics
}

/// Returns a list of all available rule names.
/// Every lint rule, paired with a one-line human description. This is the
/// single source of truth, `all_rule_names()` and the CLI `rules`
/// subcommand both derive from it, so the two can never drift apart.
pub fn all_rules() -> &'static [(&'static str, &'static str)] {
    &[
        (
            "syntax/lex-error",
            "Report lexer errors (unterminated strings, invalid numbers, bad characters)",
        ),
        (
            "naming/class-name-pascal-case",
            "Class names must use PascalCase",
        ),
        (
            "naming/function-name-snake-case",
            "Function names must use snake_case",
        ),
        (
            "naming/variable-name-snake-case",
            "Variable names must use snake_case",
        ),
        (
            "naming/constant-name-screaming-case",
            "Constants must use SCREAMING_SNAKE_CASE (or PascalCase for preloads)",
        ),
        (
            "naming/signal-name-snake-case",
            "Signal names must use snake_case",
        ),
        (
            "naming/enum-name-pascal-case",
            "Enum names must use PascalCase",
        ),
        (
            "naming/enum-member-screaming-case",
            "Enum members must use SCREAMING_SNAKE_CASE",
        ),
        (
            "naming/file-name-snake-case",
            "File names must use snake_case",
        ),
        (
            "naming/signal-past-tense",
            "Signal names should use past tense",
        ),
        (
            "naming/private-underscore-prefix",
            "Private members with _ should not have @export",
        ),
        (
            "naming/node-name-pascal-case",
            "$NodePath references should use PascalCase",
        ),
        (
            "format/max-line-length",
            "Lines must not exceed max length (default: 100)",
        ),
        ("format/trailing-whitespace", "No trailing whitespace"),
        ("format/trailing-newline", "Files must end with a newline"),
        (
            "format/no-tabs-as-spaces",
            "Use consistent indentation (tabs by default)",
        ),
        (
            "format/boolean-operators",
            "Use 'and'/'or'/'not' instead of '&&'/'||'/'!'",
        ),
        ("format/double-quotes", "Prefer double quotes for strings"),
        (
            "format/comment-spacing",
            "Comments must have a space after #",
        ),
        (
            "format/no-unnecessary-parens",
            "No unnecessary parentheses in if/while conditions",
        ),
        (
            "format/number-literals",
            "Number literals must follow formatting rules",
        ),
        (
            "format/one-statement-per-line",
            "One statement per line (no semicolons)",
        ),
        (
            "format/blank-lines",
            "Collapse 3+ blank lines to 2 between top-level members",
        ),
        (
            "format/trailing-comma",
            "Trailing comma on last item of multi-line collections",
        ),
        (
            "format/operator-spacing",
            "One space around binary operators",
        ),
        (
            "format/colon-spacing",
            "No space before ':' and one space after (except `:=` and end of line)",
        ),
        (
            "format/comma-spacing",
            "No space before ',' and one space after (except newline / closing bracket)",
        ),
        (
            "format/float-literal-zeros",
            "Float literals need leading/trailing zeros",
        ),
        (
            "format/large-number-underscores",
            "Large numbers (>=10000) should use underscores",
        ),
        (
            "format/enum-one-per-line",
            "Each enum member on its own line",
        ),
        (
            "order/class-member-order",
            "Class members must follow canonical ordering",
        ),
        (
            "quality/max-function-length",
            "Functions must not exceed max length (default: 50 lines)",
        ),
        (
            "quality/max-file-length",
            "Files must not exceed max length (default: 1000 lines)",
        ),
        (
            "quality/max-parameters",
            "Functions must not have too many parameters (default: 5)",
        ),
        (
            "quality/unnecessary-pass",
            "'pass' alongside other statements is redundant",
        ),
        (
            "quality/no-debug-print",
            "Flag leftover print()/print_debug() calls (advisory, opt-in)",
        ),
        (
            "quality/self-comparison",
            "Flag comparisons of a value with itself",
        ),
        (
            "quality/no-self-assign",
            "Flag assignments of a variable to itself",
        ),
        (
            "quality/duplicate-dict-key",
            "Flag duplicate keys in dictionary literals",
        ),
        (
            "quality/duplicated-load",
            "Flag the same path passed to load()/preload() twice",
        ),
        (
            "quality/type-hint",
            "Declarations should have explicit type hints (advisory, opt-in)",
        ),
        (
            "quality/empty-function",
            "Flag functions whose body is only 'pass' (advisory, opt-in)",
        ),
        (
            "quality/max-class-variables",
            "Classes must not have too many member variables",
        ),
        (
            "quality/max-public-methods",
            "Classes must not have too many public methods",
        ),
        (
            "quality/max-inner-classes",
            "Files must not have too many inner classes",
        ),
        (
            "quality/no-else-return",
            "Drop the 'else' when the 'if' branch returns",
        ),
        (
            "quality/unreachable-code",
            "Flag code after return/break/continue",
        ),
        (
            "quality/await-in-loop",
            "Flag 'await' inside a for/while loop",
        ),
        (
            "quality/allocation-in-loop",
            "Flag .new() allocations inside loops",
        ),
        (
            "quality/process-get-node",
            "Flag $/get_node() lookups inside _process/_physics_process",
        ),
        (
            "quality/max-nesting-depth",
            "Functions must not nest blocks too deeply",
        ),
        (
            "quality/max-returns",
            "Functions must not have too many return statements",
        ),
        (
            "quality/max-branches",
            "Functions must not have too many branches",
        ),
        (
            "quality/max-local-variables",
            "Functions must not have too many local variables",
        ),
    ]
}

pub fn all_rule_names() -> Vec<&'static str> {
    all_rules().iter().map(|(name, _)| *name).collect()
}
