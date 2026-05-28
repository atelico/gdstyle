use crate::ast::{for_each_member, ClassMember, ScriptFile};
use crate::diagnostic::{Diagnostic, Fix, Replacement};
use std::path::Path;

/// Check that class_name uses PascalCase.
pub fn check_class_name_pascal_case(file: &ScriptFile, diagnostics: &mut Vec<Diagnostic>) {
    for member in &file.members {
        if let ClassMember::ClassNameDecl {
            name,
            name_span,
            span,
        } = member
        {
            if !name.is_empty() && !is_pascal_case(name) {
                let fixed = to_pascal_case(name);
                diagnostics.push(
                    Diagnostic::warning(
                        "naming/class-name-pascal-case",
                        format!(
                            "class name '{}' should use PascalCase (e.g., '{}')",
                            name, fixed
                        ),
                        *span,
                        &file.path,
                    )
                    .with_fix(Fix {
                        replacements: vec![Replacement {
                            offset: name_span.offset,
                            length: name_span.length,
                            new_text: fixed,
                        }],
                        is_safe: false, // renaming identifiers is never safe for auto-fix
                    }),
                );
            }
        }
    }
}

/// Check that function names use snake_case.
pub fn check_function_name_snake_case(file: &ScriptFile, diagnostics: &mut Vec<Diagnostic>) {
    for_each_member(&file.members, |member| {
        let ClassMember::Function {
            name,
            name_span,
            span,
            ..
        } = member
        else {
            return;
        };
        if name.is_empty() || is_snake_case(name) {
            return;
        }
        let fixed = to_snake_case(name);
        diagnostics.push(Diagnostic::warning_with_fix(
            "naming/function-name-snake-case",
            format!(
                "function name '{}' should use snake_case (e.g., '{}')",
                name, fixed
            ),
            *span,
            &file.path,
            name_span.offset,
            name_span.length,
            fixed,
            false,
        ));
    });
}

/// Check that variable names use snake_case.
pub fn check_variable_name_snake_case(file: &ScriptFile, diagnostics: &mut Vec<Diagnostic>) {
    check_variables_recursive(&file.members, &file.path, diagnostics);
}

fn check_variables_recursive(
    members: &[ClassMember],
    file_path: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for_each_member(members, |member| match member {
        ClassMember::Variable {
            name,
            name_span,
            span,
            ..
        } if !name.is_empty() && !is_snake_case(name) => {
            let fixed = to_snake_case(name);
            diagnostics.push(Diagnostic::warning_with_fix(
                "naming/variable-name-snake-case",
                format!(
                    "variable name '{}' should use snake_case (e.g., '{}')",
                    name, fixed
                ),
                *span,
                file_path,
                name_span.offset,
                name_span.length,
                fixed,
                false,
            ));
        }
        // Static variables accept both snake_case and SCREAMING_SNAKE_CASE
        // (commonly used as class-level constants).
        ClassMember::StaticVariable {
            name,
            name_span,
            span,
            ..
        } if !name.is_empty() && !is_snake_case(name) && !is_screaming_snake_case(name) => {
            let fixed = to_snake_case(name);
            diagnostics.push(Diagnostic::warning_with_fix(
                "naming/variable-name-snake-case",
                format!(
                    "variable name '{}' should use snake_case or SCREAMING_SNAKE_CASE (e.g., '{}')",
                    name, fixed
                ),
                *span,
                file_path,
                name_span.offset,
                name_span.length,
                fixed,
                false,
            ));
        }
        _ => {}
    });
}

/// Check that constants use SCREAMING_SNAKE_CASE.
pub fn check_constant_name_screaming_case(file: &ScriptFile, diagnostics: &mut Vec<Diagnostic>) {
    check_constants_recursive(&file.members, &file.path, diagnostics);
}

fn check_constants_recursive(
    members: &[ClassMember],
    file_path: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for_each_member(members, |member| {
        let ClassMember::Constant {
            name,
            name_span,
            span,
            ..
        } = member
        else {
            return;
        };
        // Allow PascalCase for preloaded scenes/scripts (common pattern).
        // Accept private convention (`_FooBar`) too, strip the leading
        // underscores before checking PascalCase.
        let pascal_candidate = name.trim_start_matches('_');
        if name.is_empty() || is_screaming_snake_case(name) || is_pascal_case(pascal_candidate) {
            return;
        }
        let fixed = to_screaming_snake_case(name);
        diagnostics.push(Diagnostic::warning_with_fix(
            "naming/constant-name-screaming-case",
            format!(
                "constant name '{}' should use SCREAMING_SNAKE_CASE (e.g., '{}') \
                 or PascalCase for preloaded resources",
                name, fixed
            ),
            *span,
            file_path,
            name_span.offset,
            name_span.length,
            fixed,
            false,
        ));
    });
}

/// Check that signal names use snake_case.
pub fn check_signal_name_snake_case(file: &ScriptFile, diagnostics: &mut Vec<Diagnostic>) {
    check_signals_recursive(&file.members, &file.path, diagnostics);
}

fn check_signals_recursive(
    members: &[ClassMember],
    file_path: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for_each_member(members, |member| {
        let ClassMember::Signal {
            name,
            name_span,
            span,
            ..
        } = member
        else {
            return;
        };
        if name.is_empty() || is_snake_case(name) {
            return;
        }
        let fixed = to_snake_case(name);
        diagnostics.push(Diagnostic::warning_with_fix(
            "naming/signal-name-snake-case",
            format!(
                "signal name '{}' should use snake_case (e.g., '{}')",
                name, fixed
            ),
            *span,
            file_path,
            name_span.offset,
            name_span.length,
            fixed,
            false,
        ));
    });
}

/// Check that enum names use PascalCase.
pub fn check_enum_name_pascal_case(file: &ScriptFile, diagnostics: &mut Vec<Diagnostic>) {
    check_enums_recursive(&file.members, &file.path, diagnostics, true);
}

/// Check that enum members use SCREAMING_SNAKE_CASE.
pub fn check_enum_member_screaming_case(file: &ScriptFile, diagnostics: &mut Vec<Diagnostic>) {
    check_enums_recursive(&file.members, &file.path, diagnostics, false);
}

fn check_enums_recursive(
    members: &[ClassMember],
    file_path: &str,
    diagnostics: &mut Vec<Diagnostic>,
    check_name: bool,
) {
    for_each_member(members, |member| {
        let ClassMember::Enum {
            name,
            name_span,
            members: enum_members,
            span,
        } = member
        else {
            return;
        };
        if check_name {
            if let (Some(name), Some(ns)) = (name, name_span) {
                if !is_pascal_case(name) {
                    let fixed = to_pascal_case(name);
                    diagnostics.push(Diagnostic::warning_with_fix(
                        "naming/enum-name-pascal-case",
                        format!(
                            "enum name '{}' should use PascalCase (e.g., '{}')",
                            name, fixed
                        ),
                        *span,
                        file_path,
                        ns.offset,
                        ns.length,
                        fixed,
                        false,
                    ));
                }
            }
        } else {
            for em in enum_members {
                if is_screaming_snake_case(&em.name) {
                    continue;
                }
                let fixed = to_screaming_snake_case(&em.name);
                diagnostics.push(Diagnostic::warning_with_fix(
                    "naming/enum-member-screaming-case",
                    format!(
                        "enum member '{}' should use SCREAMING_SNAKE_CASE (e.g., '{}')",
                        em.name, fixed
                    ),
                    em.span,
                    file_path,
                    em.span.offset,
                    em.span.length,
                    fixed,
                    false,
                ));
            }
        }
    });
}

/// Check that .gd file names use snake_case.
pub fn check_file_name_snake_case(file: &ScriptFile, diagnostics: &mut Vec<Diagnostic>) {
    let path = Path::new(&file.path);
    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
        if !is_snake_case(stem) && !stem.starts_with('.') {
            let span = crate::token::Span::new(1, 1, 0, 0);
            diagnostics.push(Diagnostic::warning(
                "naming/file-name-snake-case",
                format!(
                    "file name '{}' should use snake_case (e.g., '{}.gd')",
                    path.file_name().unwrap_or_default().to_string_lossy(),
                    to_snake_case(stem)
                ),
                span,
                &file.path,
            ));
            // No auto-fix for file names (requires file system rename).
        }
    }
}

/// Check that signal names suggest past tense.
pub fn check_signal_past_tense(file: &ScriptFile, diagnostics: &mut Vec<Diagnostic>) {
    for member in &file.members {
        if let ClassMember::Signal {
            name,
            name_span,
            span,
            ..
        } = member
        {
            if name.is_empty() {
                continue;
            }
            if signal_is_past_tense(name) {
                continue;
            }

            // Try to produce a suggestion. Only suggest if the last word is a
            // known regular verb we can confidently inflect.
            let last_word = last_word_of(name);
            if let Some(past) = try_inflect_past_tense(&last_word) {
                let suggested = replace_last_word(name, &past);
                diagnostics.push(
                    Diagnostic::warning(
                        "naming/signal-past-tense",
                        format!(
                            "signal '{}' should use past tense (e.g., '{}')",
                            name, suggested
                        ),
                        *span,
                        &file.path,
                    )
                    .with_fix(Fix {
                        replacements: vec![Replacement {
                            offset: name_span.offset,
                            length: name_span.length,
                            new_text: suggested,
                        }],
                        is_safe: false,
                    }),
                );
            }
        }
    }
}

/// Returns true if the signal name already conveys past tense.
fn signal_is_past_tense(name: &str) -> bool {
    let last = last_word_of(name);

    // Already ends in -ed (regular past tense).
    if last.ends_with("ed") {
        return true;
    }

    // Last word is a known irregular past tense or past participle.
    if is_irregular_past_form(&last) {
        return true;
    }

    // Any word in the signal is already past tense (e.g., "finished_displaying").
    for word in name.split('_') {
        if !word.is_empty() && (word.ends_with("ed") || is_irregular_past_form(word)) {
            return true;
        }
    }

    // Gerunds (-ing): "finished_displaying" describes an ongoing action;
    // adding "ed" makes no sense.
    if last.ends_with("ing") {
        return true;
    }

    // Last word is a common noun, not a verb.
    if is_common_signal_noun(&last) {
        return true;
    }

    // Last word is a state adjective (ready, active, visible, etc.).
    if is_state_adjective(&last) {
        return true;
    }

    false
}

fn last_word_of(name: &str) -> String {
    name.rsplit('_').next().unwrap_or(name).to_string()
}

fn replace_last_word(name: &str, replacement: &str) -> String {
    if let Some(pos) = name.rfind('_') {
        format!("{}_{}", &name[..pos], replacement)
    } else {
        replacement.to_string()
    }
}

/// Try to inflect a word to past tense. Returns None unless the word is a known
/// verb, either an irregular verb (with a hand-mapped past tense) or a regular
/// verb in the curated list. Words that aren't in either dictionary are assumed
/// not to be verbs, so no autofix is suggested. This guards against nonsense
/// renames like `launch_game` → `launch_gamed`, where "game" is a noun.
fn try_inflect_past_tense(word: &str) -> Option<String> {
    if word.is_empty() || word.ends_with("ed") {
        return None;
    }
    // Irregular verbs first, return the known past tense.
    if let Some(past) = irregular_verb_past(word) {
        return Some(past.to_string());
    }
    // Regular verbs, apply standard inflection.
    if !is_regular_verb(word) {
        return None;
    }
    // Words ending in 'e' → append 'd' (e.g. close → closed).
    if word.ends_with('e') {
        return Some(format!("{}d", word));
    }
    // Words ending in consonant+'y' → replace 'y' with 'ied' (e.g. try → tried).
    if word.ends_with('y') && word.len() >= 2 {
        let before_y = word.as_bytes()[word.len() - 2];
        if !matches!(before_y, b'a' | b'e' | b'i' | b'o' | b'u') {
            return Some(format!("{}ied", &word[..word.len() - 1]));
        }
    }
    // Default → append 'ed'.
    Some(format!("{}ed", word))
}

// ---------------------------------------------------------------------------
// English-morphology dictionaries
// ---------------------------------------------------------------------------
//
// These were originally enormous `matches!` blocks inline in this file. They
// now live as plain word-lists under `src/rules/data/`, embedded with
// `include_str!` and lazy-loaded into hash sets / a hash map. Easier for
// contributors to edit (no Rust touched), easier for CI to dedupe / sort,
// and faster to query than walking the compiled match tree.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

const REGULAR_VERBS_DATA: &str = include_str!("data/regular_verbs.txt");
const IRREGULAR_PAST_FORMS_DATA: &str = include_str!("data/irregular_past.txt");
const IRREGULAR_VERB_PAIRS_DATA: &str = include_str!("data/irregular_verb_pairs.txt");
const SIGNAL_NOUNS_DATA: &str = include_str!("data/signal_nouns.txt");
const STATE_ADJECTIVES_DATA: &str = include_str!("data/state_adjectives.txt");

fn word_set(data: &'static str) -> HashSet<&'static str> {
    data.lines()
        .map(str::trim)
        .filter(|w| !w.is_empty())
        .collect()
}

fn regular_verbs() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| word_set(REGULAR_VERBS_DATA))
}

fn irregular_past_forms() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| word_set(IRREGULAR_PAST_FORMS_DATA))
}

fn irregular_verb_pairs() -> &'static HashMap<&'static str, &'static str> {
    static MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    MAP.get_or_init(|| {
        IRREGULAR_VERB_PAIRS_DATA
            .lines()
            .filter_map(|line| {
                let mut parts = line.split_whitespace();
                let base = parts.next()?;
                let past = parts.next()?;
                Some((base, past))
            })
            .collect()
    })
}

fn signal_nouns() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| word_set(SIGNAL_NOUNS_DATA))
}

fn state_adjectives() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| word_set(STATE_ADJECTIVES_DATA))
}

/// Returns true if the word is a known irregular past tense or past participle form.
fn is_irregular_past_form(word: &str) -> bool {
    irregular_past_forms().contains(word)
}

/// Maps an irregular verb base form to its past tense.
fn irregular_verb_past(word: &str) -> Option<&'static str> {
    irregular_verb_pairs().get(word).copied()
}

/// Curated list of common English regular verbs whose base form is also
/// unambiguous (i.e. not also a common noun in domain code). Only verbs
/// in this list, or in the irregular dictionary, get auto-inflected to
/// past tense by the signal-past-tense fixer. This prevents nonsense
/// renames such as `launch_game` → `launch_gamed`.
fn is_regular_verb(word: &str) -> bool {
    regular_verbs().contains(word)
}

/// Common nouns that appear as the last word in signal names and should not
/// be treated as verbs.
fn is_common_signal_noun(word: &str) -> bool {
    signal_nouns().contains(word)
}

/// State adjectives that describe a state rather than an action.
fn is_state_adjective(word: &str) -> bool {
    state_adjectives().contains(word)
}

/// Check that private members (funcs/vars starting with _) are actually private pattern.
pub fn check_private_underscore_prefix(file: &ScriptFile, diagnostics: &mut Vec<Diagnostic>) {
    // This rule checks that functions/vars starting with _ don't have @export.
    // It's more of a convention check.
    for member in &file.members {
        if let ClassMember::Variable {
            name,
            annotations,
            span,
            ..
        } = member
        {
            if name.starts_with('_')
                && annotations
                    .iter()
                    .any(|a| a.name == "export" || a.name.starts_with("export_"))
            {
                diagnostics.push(Diagnostic::warning(
                    "naming/private-underscore-prefix",
                    format!(
                        "variable '{}' starts with '_' (private) but has @export",
                        name
                    ),
                    *span,
                    &file.path,
                ));
            }
        }
    }
}

/// Check that $NodePath references use PascalCase.
pub fn check_node_name_pascal_case(
    tokens: &[crate::token::Token],
    file: &ScriptFile,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (idx, token) in tokens.iter().enumerate() {
        if token.kind == crate::token::TokenKind::Dollar {
            // Next token should be an identifier (the node name).
            if idx + 1 < tokens.len() {
                if let crate::token::TokenKind::Identifier(ref name) = tokens[idx + 1].kind {
                    if !is_pascal_case(name) && !name.contains('/') {
                        let fixed = to_pascal_case(name);
                        let name_span = tokens[idx + 1].span;
                        diagnostics.push(
                            Diagnostic::warning(
                                "naming/node-name-pascal-case",
                                format!(
                                    "node reference '{}' should use PascalCase (e.g., '{}'); also rename the node in the scene tree",
                                    name, fixed
                                ),
                                name_span,
                                &file.path,
                            )
                            .with_fix(Fix {
                                replacements: vec![Replacement {
                                    offset: name_span.offset,
                                    length: name_span.length,
                                    new_text: fixed,
                                }],
                                is_safe: false, // unsafe: user must also rename node in scene tree
                            }),
                        );
                    }
                }
            }
        }
    }
}

// --- Case detection and conversion utilities ---

pub(crate) fn is_pascal_case(name: &str) -> bool {
    if name.is_empty() {
        return true;
    }
    let first = name.chars().next().unwrap();
    if !first.is_ascii_uppercase() {
        return false;
    }
    // Single uppercase letter (e.g., class_name A) is valid PascalCase.
    if name.len() == 1 {
        return true;
    }
    !name.contains('_') && name.chars().any(|c| c.is_ascii_lowercase())
}

pub(crate) fn is_snake_case(name: &str) -> bool {
    if name.is_empty() {
        return true;
    }
    let trimmed = name.trim_start_matches('_');
    if trimmed.is_empty() {
        return true;
    }
    trimmed
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

pub(crate) fn is_screaming_snake_case(name: &str) -> bool {
    if name.is_empty() {
        return true;
    }
    // Strip leading underscores (private convention in GDScript).
    let stripped = name.trim_start_matches('_');
    if stripped.is_empty() {
        return true;
    }
    let first = stripped.chars().next().unwrap();
    if !first.is_ascii_uppercase() && !first.is_ascii_digit() {
        return false;
    }
    stripped
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

pub fn to_pascal_case(name: &str) -> String {
    name.split('_')
        .filter(|s| !s.is_empty())
        .map(pascal_word)
        .collect()
}

/// Capitalize a single word for PascalCase output. The rule is "preserve
/// what the user already wrote, only fix the first character", that way
/// `HTTPRequest`, `XMLParser`, `myShape` all survive intact and only
/// fully-lowercase or fully-uppercase inputs get a casing transform.
///
/// - Pure-lowercase (`foo`)        → `Foo`
/// - Pure-SCREAMING (`FOO`)        → `FOO` (already PascalCase-acceptable)
/// - Mixed (`HTTPRequest`/`myFoo`) → first char uppercased, rest preserved
fn pascal_word(word: &str) -> String {
    let mut chars = word.chars();
    let first = match chars.next() {
        Some(c) => c,
        None => return String::new(),
    };
    let rest = chars.as_str();
    let has_uppercase_after_first = rest.chars().any(|c| c.is_ascii_uppercase());

    let mut out = String::with_capacity(word.len());
    out.push(first.to_ascii_uppercase());
    if has_uppercase_after_first {
        // Mixed-case input, preserve what the user wrote (camelCase →
        // PascalCase, acronyms stay intact).
        out.push_str(rest);
    } else {
        // Single-case input, lowercase the tail (snake-segment of a
        // SCREAMING word, or a plain lowercase word with first cap).
        out.push_str(&rest.to_lowercase());
    }
    out
}

pub(crate) fn to_snake_case(name: &str) -> String {
    // If already SCREAMING_SNAKE_CASE, just lowercase it.
    if is_screaming_snake_case(name) {
        return name.to_ascii_lowercase();
    }

    let mut result = String::new();
    let mut prev_was_upper = false;
    let mut prev_was_digit = false;
    let mut prev_was_underscore = false;
    let leading_underscores: String = name.chars().take_while(|c| *c == '_').collect();
    let trimmed = name.trim_start_matches('_');
    let chars: Vec<char> = trimmed.chars().collect();

    for (i, &ch) in chars.iter().enumerate() {
        if ch.is_ascii_uppercase() {
            if i > 0 && !prev_was_underscore {
                if !prev_was_upper && !prev_was_digit {
                    // lowercase→uppercase transition: insert underscore
                    result.push('_');
                } else if i + 1 < chars.len()
                    && chars[i + 1].is_ascii_lowercase()
                    && !prev_was_digit
                {
                    // End of acronym: uppercase→uppercase→lowercase
                    result.push('_');
                }
                // After a digit (e.g., "2D"), don't insert underscore.
                // The digit group was already separated from the word before it.
            }
            result.push(ch.to_ascii_lowercase());
            prev_was_upper = true;
            prev_was_digit = false;
            prev_was_underscore = false;
        } else if ch.is_ascii_digit() {
            // Insert underscore before digit if preceded by a letter and not
            // already after an underscore or another digit.
            if i > 0 && !prev_was_underscore && !prev_was_digit {
                result.push('_');
            }
            result.push(ch);
            prev_was_upper = false;
            prev_was_digit = true;
            prev_was_underscore = false;
        } else {
            prev_was_upper = false;
            prev_was_digit = false;
            prev_was_underscore = ch == '_';
            result.push(ch);
        }
    }

    format!("{}{}", leading_underscores, result)
}

pub(crate) fn to_screaming_snake_case(name: &str) -> String {
    to_snake_case(name).to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_pascal_case() {
        assert!(is_pascal_case("Player"));
        assert!(is_pascal_case("StateMachine"));
        assert!(is_pascal_case("YAMLParser"));
        assert!(is_pascal_case("A")); // single uppercase letter
        assert!(is_pascal_case("X")); // single uppercase letter
        assert!(!is_pascal_case("player"));
        assert!(!is_pascal_case("state_machine"));
        assert!(!is_pascal_case("PLAYER"));
    }

    #[test]
    fn test_is_snake_case() {
        assert!(is_snake_case("player_speed"));
        assert!(is_snake_case("_private_var"));
        assert!(is_snake_case("x"));
        assert!(is_snake_case("_"));
        assert!(is_snake_case("max_hp"));
        assert!(!is_snake_case("playerSpeed"));
        assert!(!is_snake_case("PlayerSpeed"));
        assert!(!is_snake_case("PLAYER_SPEED"));
    }

    #[test]
    fn test_is_screaming_snake_case() {
        assert!(is_screaming_snake_case("MAX_SPEED"));
        assert!(is_screaming_snake_case("IDLE"));
        assert!(is_screaming_snake_case("PLAYER_1"));
        assert!(!is_screaming_snake_case("maxSpeed"));
        assert!(!is_screaming_snake_case("max_speed"));
        assert!(!is_screaming_snake_case("MaxSpeed"));
        // S3: underscore-prefixed constants should be recognized
        assert!(is_screaming_snake_case("_FALLBACK_POIGNANCY_SCORE"));
        assert!(is_screaming_snake_case("_HELP_SELECT_NEXT_PERSONA"));
        assert!(is_screaming_snake_case("_MAX_RETRIES"));
        assert!(is_screaming_snake_case("_API_TIMEOUT_MS"));
        assert!(is_screaming_snake_case("_DEFAULT_COLOR"));
        assert!(is_screaming_snake_case("_INTERNAL_BUFFER_SIZE"));
        assert!(is_screaming_snake_case("_MIN_DISTANCE_THRESHOLD"));
        assert!(is_screaming_snake_case("__DOUBLE_UNDERSCORE"));
    }

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(to_pascal_case("state_machine"), "StateMachine");
        assert_eq!(to_pascal_case("player"), "Player");
        assert_eq!(to_pascal_case("yaml_parser"), "YamlParser");
    }

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("PlayerSpeed"), "player_speed");
        assert_eq!(to_snake_case("StateMachine"), "state_machine");
        assert_eq!(to_snake_case("_PrivateVar"), "_private_var");
        // S7: acronym handling
        assert_eq!(to_snake_case("AIInControl"), "ai_in_control");
        assert_eq!(to_snake_case("HTTPRequest"), "http_request");
        assert_eq!(to_snake_case("UIElement"), "ui_element");
        assert_eq!(to_snake_case("IOHandler"), "io_handler");
        assert_eq!(to_snake_case("GPUCompute"), "gpu_compute");
        assert_eq!(to_snake_case("SSEEvent"), "sse_event");
        assert_eq!(to_snake_case("HTMLParser"), "html_parser");
        assert_eq!(to_snake_case("APIKey"), "api_key");
        // Non-acronym cases still work
        assert_eq!(to_snake_case("PlayerInControl"), "player_in_control");
        assert_eq!(to_snake_case("None"), "none");
        // Digit-to-uppercase transitions
        assert_eq!(to_snake_case("Vector2D"), "vector_2d");
        assert_eq!(to_snake_case("Area2D"), "area_2d");
        assert_eq!(to_snake_case("Item3D"), "item_3d");
        assert_eq!(to_snake_case("Sprite2D"), "sprite_2d");
        assert_eq!(to_snake_case("Node2D"), "node_2d");
        // Digit in middle gets underscore before digit group
        assert_eq!(to_snake_case("player1speed"), "player_1speed");
    }

    #[test]
    fn test_to_screaming_snake_case() {
        assert_eq!(to_screaming_snake_case("maxSpeed"), "MAX_SPEED");
        assert_eq!(to_screaming_snake_case("PlayerState"), "PLAYER_STATE");
        // S7: acronym handling
        assert_eq!(to_screaming_snake_case("AIInControl"), "AI_IN_CONTROL");
        assert_eq!(to_screaming_snake_case("HTTPRequest"), "HTTP_REQUEST");
        assert_eq!(to_screaming_snake_case("UIElement"), "UI_ELEMENT");
        assert_eq!(to_screaming_snake_case("SSEEvent"), "SSE_EVENT");
        assert_eq!(
            to_screaming_snake_case("PlayerInControl"),
            "PLAYER_IN_CONTROL"
        );
        assert_eq!(to_screaming_snake_case("None"), "NONE");
    }

    #[test]
    fn test_class_name_rule() {
        let file = ScriptFile {
            path: "test.gd".to_string(),
            members: vec![ClassMember::ClassNameDecl {
                name: "my_player".to_string(),
                name_span: crate::token::Span::new(1, 1, 0, 0),
                span: crate::token::Span::new(1, 1, 0, 0),
            }],
            lines: vec![],
        };
        let mut diagnostics = Vec::new();
        check_class_name_pascal_case(&file, &mut diagnostics);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("PascalCase"));
        assert!(diagnostics[0].fix.is_some());
    }

    #[test]
    fn test_class_name_rule_passes() {
        let file = ScriptFile {
            path: "test.gd".to_string(),
            members: vec![ClassMember::ClassNameDecl {
                name: "MyPlayer".to_string(),
                name_span: crate::token::Span::new(1, 1, 0, 0),
                span: crate::token::Span::new(1, 1, 0, 0),
            }],
            lines: vec![],
        };
        let mut diagnostics = Vec::new();
        check_class_name_pascal_case(&file, &mut diagnostics);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_function_name_rule() {
        let file = ScriptFile {
            path: "test.gd".to_string(),
            members: vec![ClassMember::Function {
                name: "takeDamage".to_string(),
                name_span: crate::token::Span::new(1, 1, 0, 0),
                parameters: vec![],
                return_type: None,
                is_static: false,
                annotations: vec![],
                body_line_count: 1,
                span: crate::token::Span::new(1, 1, 0, 0),
            }],
            lines: vec![],
        };
        let mut diagnostics = Vec::new();
        check_function_name_snake_case(&file, &mut diagnostics);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("snake_case"));
        assert!(diagnostics[0].fix.is_some());
    }

    #[test]
    fn test_constant_name_rule() {
        let file = ScriptFile {
            path: "test.gd".to_string(),
            members: vec![ClassMember::Constant {
                name: "maxSpeed".to_string(),
                name_span: crate::token::Span::new(1, 1, 0, 0),
                type_hint: None,
                span: crate::token::Span::new(1, 1, 0, 0),
            }],
            lines: vec![],
        };
        let mut diagnostics = Vec::new();
        check_constant_name_screaming_case(&file, &mut diagnostics);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].fix.is_some());
    }

    #[test]
    fn test_constant_pascal_case_allowed() {
        let file = ScriptFile {
            path: "test.gd".to_string(),
            members: vec![ClassMember::Constant {
                name: "PlayerScene".to_string(),
                name_span: crate::token::Span::new(1, 1, 0, 0),
                type_hint: None,
                span: crate::token::Span::new(1, 1, 0, 0),
            }],
            lines: vec![],
        };
        let mut diagnostics = Vec::new();
        check_constant_name_screaming_case(&file, &mut diagnostics);
        assert!(
            diagnostics.is_empty(),
            "PascalCase should be allowed for preloaded resources"
        );
    }

    #[test]
    fn test_constant_underscore_prefix_not_flagged() {
        for name in &[
            "_FALLBACK_POIGNANCY_SCORE",
            "_HELP_SELECT_NEXT_PERSONA",
            "_MAX_RETRIES",
            "_API_TIMEOUT_MS",
        ] {
            let file = ScriptFile {
                path: "test.gd".to_string(),
                members: vec![ClassMember::Constant {
                    name: name.to_string(),
                    name_span: crate::token::Span::new(1, 1, 0, 0),
                    type_hint: None,
                    span: crate::token::Span::new(1, 1, 0, 0),
                }],
                lines: vec![],
            };
            let mut diagnostics = Vec::new();
            check_constant_name_screaming_case(&file, &mut diagnostics);
            assert!(
                diagnostics.is_empty(),
                "underscore-prefixed constant '{}' should not be flagged",
                name
            );
        }
    }

    #[test]
    fn test_signal_name_rule() {
        let file = ScriptFile {
            path: "test.gd".to_string(),
            members: vec![ClassMember::Signal {
                name: "healthChanged".to_string(),
                name_span: crate::token::Span::new(1, 1, 0, 0),
                parameters: vec![],
                span: crate::token::Span::new(1, 1, 0, 0),
            }],
            lines: vec![],
        };
        let mut diagnostics = Vec::new();
        check_signal_name_snake_case(&file, &mut diagnostics);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].fix.is_some());
    }

    #[test]
    fn test_enum_name_rule() {
        let file = ScriptFile {
            path: "test.gd".to_string(),
            members: vec![ClassMember::Enum {
                name: Some("player_state".to_string()),
                name_span: Some(crate::token::Span::new(1, 1, 0, 0)),
                members: vec![],
                span: crate::token::Span::new(1, 1, 0, 0),
            }],
            lines: vec![],
        };
        let mut diagnostics = Vec::new();
        check_enum_name_pascal_case(&file, &mut diagnostics);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].fix.is_some());
    }

    #[test]
    fn test_enum_member_rule() {
        let file = ScriptFile {
            path: "test.gd".to_string(),
            members: vec![ClassMember::Enum {
                name: Some("State".to_string()),
                name_span: Some(crate::token::Span::new(1, 1, 0, 0)),
                members: vec![
                    crate::ast::EnumMember {
                        name: "idle".to_string(),
                        span: crate::token::Span::new(2, 5, 0, 0),
                    },
                    crate::ast::EnumMember {
                        name: "WALKING".to_string(),
                        span: crate::token::Span::new(3, 5, 0, 0),
                    },
                ],
                span: crate::token::Span::new(1, 1, 0, 0),
            }],
            lines: vec![],
        };
        let mut diagnostics = Vec::new();
        check_enum_member_screaming_case(&file, &mut diagnostics);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("idle"));
        assert!(diagnostics[0].fix.is_some());
    }

    #[test]
    fn test_file_name_snake_case() {
        let file = ScriptFile {
            path: "PlayerController.gd".to_string(),
            members: vec![],
            lines: vec![],
        };
        let mut diagnostics = Vec::new();
        check_file_name_snake_case(&file, &mut diagnostics);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("snake_case"));
    }

    #[test]
    fn test_file_name_snake_case_passes() {
        let file = ScriptFile {
            path: "player_controller.gd".to_string(),
            members: vec![],
            lines: vec![],
        };
        let mut diagnostics = Vec::new();
        check_file_name_snake_case(&file, &mut diagnostics);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_signal_past_tense() {
        let file = ScriptFile {
            path: "test.gd".to_string(),
            members: vec![ClassMember::Signal {
                name: "health_change".to_string(),
                name_span: crate::token::Span::new(1, 1, 0, 0),
                parameters: vec![],
                span: crate::token::Span::new(1, 1, 0, 0),
            }],
            lines: vec![],
        };
        let mut diagnostics = Vec::new();
        check_signal_past_tense(&file, &mut diagnostics);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].fix.is_some());
        assert!(!diagnostics[0].fix.as_ref().unwrap().is_safe);
    }

    #[test]
    fn test_signal_past_tense_ok() {
        let file = ScriptFile {
            path: "test.gd".to_string(),
            members: vec![ClassMember::Signal {
                name: "health_changed".to_string(),
                name_span: crate::token::Span::new(1, 1, 0, 0),
                parameters: vec![],
                span: crate::token::Span::new(1, 1, 0, 0),
            }],
            lines: vec![],
        };
        let mut diagnostics = Vec::new();
        check_signal_past_tense(&file, &mut diagnostics);
        assert!(diagnostics.is_empty());
    }

    // S2: signal past-tense should NOT flag these (all from GARP feedback)
    #[test]
    fn test_signal_past_tense_no_false_positives() {
        let should_not_flag = vec![
            // Already past tense / past participle
            "finished_displaying", // gerund with past-tense prefix
            "on_memory_written",   // irregular past participle
            "closed_connection",   // past tense + noun
            // Gerunds
            "on_property_start_editing",
            // Nouns as last word
            "on_query_body",
            "on_query_preprocesed_input",
            "persona_view_changed_status",
            "sse_event",
            // State adjectives
            "engine_ready",
            "_write_db_ready",
            // Nouns describing state
            "scene_load_progress",
        ];

        for name in &should_not_flag {
            let file = ScriptFile {
                path: "test.gd".to_string(),
                members: vec![ClassMember::Signal {
                    name: name.to_string(),
                    name_span: crate::token::Span::new(1, 1, 0, 0),
                    parameters: vec![],
                    span: crate::token::Span::new(1, 1, 0, 0),
                }],
                lines: vec![],
            };
            let mut diagnostics = Vec::new();
            check_signal_past_tense(&file, &mut diagnostics);
            assert!(
                diagnostics.is_empty(),
                "signal '{}' should NOT be flagged by past-tense rule",
                name
            );
        }
    }

    // S2: signal past-tense SHOULD correctly flag and suggest for regular verbs
    #[test]
    fn test_signal_past_tense_correct_suggestions() {
        let cases = vec![
            ("plan_change", "plan_changed"),
            ("planning_status_change", "planning_status_changed"),
            ("on_observable_state_change", "on_observable_state_changed"),
            ("chat_event_finish", "chat_event_finished"),
            ("data_receive", "data_received"),
            ("mouse_enter", "mouse_entered"),
            ("player_spawn", "player_spawned"),
            ("node_remove", "node_removed"),
        ];

        for (name, expected) in &cases {
            let file = ScriptFile {
                path: "test.gd".to_string(),
                members: vec![ClassMember::Signal {
                    name: name.to_string(),
                    name_span: crate::token::Span::new(1, 1, 0, 0),
                    parameters: vec![],
                    span: crate::token::Span::new(1, 1, 0, 0),
                }],
                lines: vec![],
            };
            let mut diagnostics = Vec::new();
            check_signal_past_tense(&file, &mut diagnostics);
            assert_eq!(
                diagnostics.len(),
                1,
                "signal '{}' should be flagged by past-tense rule",
                name
            );
            let fix = diagnostics[0].fix.as_ref().unwrap();
            assert_eq!(
                fix.replacements[0].new_text, *expected,
                "signal '{}' should suggest '{}', got '{}'",
                name, expected, fix.replacements[0].new_text
            );
        }
    }

    // S2: irregular verbs should get correct past tense suggestions
    #[test]
    fn test_signal_past_tense_irregular_verbs() {
        let cases = vec![
            ("chat_event_begin", "chat_event_begun"),
            ("data_send", "data_sent"),
            ("item_find", "item_found"),
            ("connection_lose", "connection_lost"),
            ("message_write", "message_written"),
        ];

        for (name, expected) in &cases {
            let file = ScriptFile {
                path: "test.gd".to_string(),
                members: vec![ClassMember::Signal {
                    name: name.to_string(),
                    name_span: crate::token::Span::new(1, 1, 0, 0),
                    parameters: vec![],
                    span: crate::token::Span::new(1, 1, 0, 0),
                }],
                lines: vec![],
            };
            let mut diagnostics = Vec::new();
            check_signal_past_tense(&file, &mut diagnostics);
            assert_eq!(
                diagnostics.len(),
                1,
                "signal '{}' should be flagged by past-tense rule",
                name
            );
            let fix = diagnostics[0].fix.as_ref().unwrap();
            assert_eq!(
                fix.replacements[0].new_text, *expected,
                "signal '{}' should suggest '{}', got '{}'",
                name, expected, fix.replacements[0].new_text
            );
        }
    }

    // S2: already-past-tense irregular forms should not be flagged
    #[test]
    fn test_signal_past_tense_irregular_already_past() {
        let already_past = vec![
            "animation_begun",
            "file_written",
            "connection_lost",
            "data_sent",
            "data_set",
            "value_set",
            "item_found",
        ];

        for name in &already_past {
            let file = ScriptFile {
                path: "test.gd".to_string(),
                members: vec![ClassMember::Signal {
                    name: name.to_string(),
                    name_span: crate::token::Span::new(1, 1, 0, 0),
                    parameters: vec![],
                    span: crate::token::Span::new(1, 1, 0, 0),
                }],
                lines: vec![],
            };
            let mut diagnostics = Vec::new();
            check_signal_past_tense(&file, &mut diagnostics);
            assert!(
                diagnostics.is_empty(),
                "signal '{}' is already past tense and should NOT be flagged",
                name
            );
        }
    }
}
