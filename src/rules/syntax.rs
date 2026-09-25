use crate::ast::ScriptFile;
use crate::diagnostic::Diagnostic;
use crate::token::{Token, TokenKind};

/// Words Godot's tokenizer turns into keyword or literal tokens, which its
/// parser then refuses wherever it expects a name.
///
/// Mirrors the keyword table in Godot 4's `gdscript_tokenizer.cpp`, minus the
/// entries `Token::is_identifier()` lets through: `match` (for
/// `String.match()`), `when` (added in 4.3 without breaking old code), and the
/// `PI`/`TAU`/`INF`/`NAN` constants. Checked against Godot 4.6.2 in every
/// declaration position this rule inspects. `namespace` and `yield` are
/// reserved without any grammar using them, so the gdstyle lexer tokenises
/// them as plain identifiers; matching on text rather than token kind covers
/// both groups the same way.
const RESERVED_WORDS: &[&str] = &[
    "and",
    "as",
    "assert",
    "await",
    "break",
    "breakpoint",
    "class",
    "class_name",
    "const",
    "continue",
    "elif",
    "else",
    "enum",
    "extends",
    "false",
    "for",
    "func",
    "if",
    "in",
    "is",
    "namespace",
    "not",
    "null",
    "or",
    "pass",
    "preload",
    "return",
    "self",
    "signal",
    "static",
    "super",
    "trait",
    "true",
    "var",
    "void",
    "while",
    "yield",
];

/// True when Godot rejects `word` as a declared name.
///
/// # Example
///
/// ```
/// use gdstyle::rules::syntax::is_reserved_word;
///
/// assert!(is_reserved_word("namespace"));
/// assert!(!is_reserved_word("match")); // Godot allows it as a name.
/// ```
pub fn is_reserved_word(word: &str) -> bool {
    RESERVED_WORDS.contains(&word)
}

/// Flag reserved words used where GDScript expects a declared name:
/// function, variable, constant, signal, enum, enum member, class and loop
/// variable names, plus function, lambda and signal parameters.
///
/// Godot fails to parse such a file (`Expected parameter name.` and
/// friends), so this reports at error severity. There is no autofix: the
/// replacement name, and every use of it, is the author's call.
///
/// Works on tokens, not the AST, because the parser skips over declarations
/// whose name is a keyword and so never produces a member to inspect.
///
/// # Example
///
/// ```
/// use gdstyle::{config::Config, linter};
///
/// let source = "func f(namespace: String) -> void:\n\tpass\n";
/// let diagnostics = linter::lint_source(source, "demo.gd", &Config::default());
/// assert!(diagnostics.iter().any(|d| d.rule == "syntax/reserved-identifier"));
/// ```
pub fn check_reserved_identifier(
    tokens: &[Token],
    file: &ScriptFile,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Layout tokens never separate a keyword from the name it declares, and
    // comments can sit inside a multi-line parameter list.
    let significant: Vec<&Token> = tokens
        .iter()
        .filter(|t| {
            !matches!(
                t.kind,
                TokenKind::Newline
                    | TokenKind::Indent
                    | TokenKind::Dedent
                    | TokenKind::Comment(_)
                    | TokenKind::DocComment(_)
            )
        })
        .collect();

    let mut report = |token: &Token, role: &str| {
        if is_reserved_word(&token.text) {
            diagnostics.push(Diagnostic::error(
                "syntax/reserved-identifier",
                format!(
                    "'{}' is a reserved word in GDScript and cannot be used as a {} name",
                    token.text, role
                ),
                token.span,
                &file.path,
            ));
        }
    };

    for (index, token) in significant.iter().enumerate() {
        // `obj.signal` and the like are member accesses, not declarations.
        if index > 0 && significant[index - 1].kind == TokenKind::Dot {
            continue;
        }
        let Some(next) = significant.get(index + 1) else {
            break;
        };
        match token.kind {
            TokenKind::Func => {
                if next.kind == TokenKind::LeftParen {
                    // Lambda: no name, straight into the parameter list.
                    report_list_entries(&significant, index + 1, "parameter", &mut report);
                } else {
                    report(next, "function");
                    if significant.get(index + 2).map(|t| &t.kind) == Some(&TokenKind::LeftParen) {
                        report_list_entries(&significant, index + 2, "parameter", &mut report);
                    }
                }
            }
            TokenKind::Signal => {
                report(next, "signal");
                if significant.get(index + 2).map(|t| &t.kind) == Some(&TokenKind::LeftParen) {
                    report_list_entries(&significant, index + 2, "parameter", &mut report);
                }
            }
            TokenKind::Enum => {
                if next.kind == TokenKind::LeftBrace {
                    report_list_entries(&significant, index + 1, "enum member", &mut report);
                } else {
                    report(next, "enum");
                    if significant.get(index + 2).map(|t| &t.kind) == Some(&TokenKind::LeftBrace) {
                        report_list_entries(&significant, index + 2, "enum member", &mut report);
                    }
                }
            }
            TokenKind::Var => report(next, "variable"),
            TokenKind::Const => report(next, "constant"),
            TokenKind::Class | TokenKind::ClassName => report(next, "class"),
            TokenKind::For => report(next, "loop variable"),
            _ => {}
        }
    }
}

/// Report the first token of each entry in the bracketed list opening at
/// `open_index` (a parameter list or an enum body). Entries are separated by
/// commas at the list's own depth, so default values such as `f(a, b)` or
/// `[1, 2]` don't start new entries.
fn report_list_entries(
    significant: &[&Token],
    open_index: usize,
    role: &str,
    report: &mut impl FnMut(&Token, &str),
) {
    let mut depth = 0usize;
    let mut expects_name = false;
    for token in &significant[open_index..] {
        match token.kind {
            TokenKind::LeftParen | TokenKind::LeftBracket | TokenKind::LeftBrace => {
                depth += 1;
                expects_name = depth == 1;
                continue;
            }
            TokenKind::RightParen | TokenKind::RightBracket | TokenKind::RightBrace => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return;
                }
                continue;
            }
            TokenKind::Comma if depth == 1 => {
                expects_name = true;
                continue;
            }
            // Variadic parameter (`...args`): the name follows the ellipsis.
            TokenKind::Ellipsis if expects_name => continue,
            _ => {}
        }
        if expects_name {
            report(token, role);
            expects_name = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reserved_diagnostics(source: &str) -> Vec<Diagnostic> {
        let tokens = crate::lexer::Lexer::new(source).tokenize();
        let file = ScriptFile {
            path: "test.gd".to_string(),
            members: vec![],
            lines: source.split('\n').map(str::to_string).collect(),
        };
        let mut diagnostics = Vec::new();
        check_reserved_identifier(&tokens, &file, &mut diagnostics);
        diagnostics
    }

    fn flagged_words(source: &str) -> Vec<String> {
        reserved_diagnostics(source)
            .iter()
            .map(|d| d.message.split('\'').nth(1).unwrap_or_default().to_string())
            .collect()
    }

    #[test]
    fn issue_32_parameter_and_function_name() {
        let source = "extends Node\n\n\nfunc _prose(namespace: String) -> Dictionary:\n\treturn { \"namespace\": namespace }\n\n\nfunc trait(id: String) -> float:\n\treturn 0.0\n";
        let diagnostics = reserved_diagnostics(source);
        assert_eq!(diagnostics.len(), 2, "got {:?}", diagnostics);
        assert_eq!(diagnostics[0].span.line, 4);
        assert!(diagnostics[0].message.contains("'namespace'"));
        assert!(diagnostics[0].message.contains("parameter"));
        assert_eq!(diagnostics[1].span.line, 8);
        assert!(diagnostics[1].message.contains("'trait'"));
        assert!(diagnostics[1].message.contains("function"));
        assert!(diagnostics
            .iter()
            .all(|d| d.severity == crate::diagnostic::Severity::Error));
    }

    #[test]
    fn every_declaration_position_is_checked() {
        let source = "\
class_name yield
extends Node
signal void
signal ok(namespace)
enum trait { A }
enum E { OK, self }
enum { in }
const super = 1
static var static = 2
var namespace := 3
class preload:
\tpass
func f(ok, yield = g(1, 2), ...await) -> void:
\tvar true := 1
\tfor is in []:
\t\tpass
\tvar h := func(null): return 1
";
        assert_eq!(
            flagged_words(source),
            vec![
                "yield",
                "void",
                "namespace",
                "trait",
                "self",
                "in",
                "super",
                "static",
                "namespace",
                "preload",
                "yield",
                "await",
                "true",
                "is",
                "null",
            ]
        );
    }

    #[test]
    fn multi_line_parameter_list_with_comments() {
        let source = "func f(\n\ta: int, # first\n\tnamespace: String,\n) -> void:\n\tpass\n";
        assert_eq!(flagged_words(source), vec!["namespace"]);
    }

    #[test]
    fn names_godot_accepts_are_not_flagged() {
        // `match`, `when` and the math constants are soft keywords in Godot 4;
        // reserved words are fine as dictionary keys, strings, member access
        // and default values.
        let source = "\
var match := 1
var when := 2
const PI = 3
func f(match, when, a = self, b = null) -> void:
\tvar d := { \"namespace\": 1, \"trait\": 2 }
\tvar x = obj.namespace
\tvar s := \"yield\"
\tfor i in range(3):
\t\tpass
";
        assert_eq!(flagged_words(source), Vec::<String>::new());
    }

    #[test]
    fn typed_array_default_does_not_start_an_entry() {
        let source = "func f(a: Array[int] = [1, 2], b := {\"k\": 1}) -> void:\n\tpass\n";
        assert!(reserved_diagnostics(source).is_empty());
    }
}
