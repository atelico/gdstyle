use gdstyle::config::Config;
use gdstyle::diagnostic::{Diagnostic, Fix, Replacement};
use gdstyle::token::{Span, TokenKind};
use gdstyle::{fixer, formatter, lexer::Lexer, linter, parser::Parser};

#[test]
fn lexer_handles_long_runs_of_ignored_layout() {
    for source in [
        format!("var x = {}1\n", "\\\n".repeat(100_000)),
        format!("var x = ({}1)\n", "\r\n".repeat(100_000)),
        format!("var x = {}1\n", "\r".repeat(100_000)),
    ] {
        let tokens = Lexer::new(&source).tokenize();
        assert!(tokens.iter().any(|t| t.kind == TokenKind::Integer(1)));
        assert!(!tokens.iter().any(|t| matches!(t.kind, TokenKind::Error(_))));
    }
}

#[test]
fn crlf_blank_lines_after_an_outdented_comment_do_not_close_the_block() {
    let source = "func f():\r\n\tpass\r\n# still in f\r\n \r\n\tpass\r\n";
    let tokens = Lexer::new(source).tokenize();
    assert_eq!(
        tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Indent)
            .count(),
        1
    );
    assert_eq!(
        tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Dedent)
            .count(),
        1
    );
}

#[test]
fn bom_is_preserved_and_combining_marks_are_part_of_identifiers() {
    let source = "\u{feff}var cafe\u{301}=1\r\n";
    let tokens = Lexer::new(source).tokenize();
    assert_eq!(tokens[0].span, Span::new(1, 1, 3, 3));
    assert_eq!(tokens[1].kind, TokenKind::Identifier("cafe\u{301}".into()));
    let config = Config::default();
    let expected = "\u{feff}var cafe\u{301} = 1\r\n";
    let diagnostics = linter::lint_source(source, "test.gd", &config);
    assert!(!diagnostics.iter().any(|d| d.rule == "syntax/lex-error"));
    assert_eq!(fixer::apply_fixes(source, &diagnostics, true), expected);
    assert_eq!(formatter::format_source(source, &config), expected);
    assert!(formatter::try_format_source("\u{feff}<<<<<<< HEAD\n", &config).is_err());
}

#[test]
fn a_long_multiline_call_does_not_close_on_a_parenthesis_in_a_comment() {
    let source = "var point = Vector2(123, 456 # This ) is just a comment\n)\n";
    let config = Config {
        max_line_length: 30,
        ..Config::default()
    };
    assert_eq!(formatter::format_source(source, &config), source);
}

#[test]
fn a_long_call_with_mismatched_delimiters_is_not_rewrapped() {
    let source = "var x = f([one, two), three, four, five)\n";
    let config = Config {
        max_line_length: 25,
        ..Config::default()
    };
    assert_eq!(formatter::format_source(source, &config), source);
}

#[test]
fn parser_accepts_token_slices_without_an_eof_token() {
    let mut tokens = Lexer::new("var value = 1").tokenize();
    assert_eq!(tokens.pop().unwrap().kind, TokenKind::Eof);
    assert_eq!(Parser::new(&tokens).parse().len(), 1);
    assert_eq!(Parser::new(&tokens[..1]).parse().len(), 1);
}

#[test]
fn fixes_with_invalid_utf8_ranges_do_not_displace_valid_fixes() {
    let source = "# 🐈\n";
    let diag = Diagnostic::warning("test", String::new(), Span::new(1, 1, 0, 0), "test.gd")
        .with_fix(Fix {
            is_safe: true,
            replacements: vec![
                Replacement {
                    offset: 3,
                    length: 1,
                    new_text: "bad".into(),
                },
                Replacement {
                    offset: 0,
                    length: usize::MAX,
                    new_text: "bad".into(),
                },
                Replacement {
                    offset: 6,
                    length: usize::MAX,
                    new_text: "bad".into(),
                },
                Replacement {
                    offset: 2,
                    length: 4,
                    new_text: "cat".into(),
                },
            ],
        });
    assert_eq!(fixer::apply_fixes(source, &[diag], true), "# cat\n");
}

#[test]
fn keyword_targets_and_colliding_rename_candidates_are_skipped() {
    let source =
        "var someValue = 0\nvar SomeValue = 1\nvar otherValue = 2\n\n\nfunc Return():\n\tpass\n";
    let diagnostics = linter::lint_source(source, "test.gd", &Config::default());
    let keyword_diag = diagnostics
        .iter()
        .find(|d| d.rule == "naming/function-name-snake-case")
        .unwrap();
    assert!(keyword_diag.fix.is_none());
    assert_eq!(
        fixer::apply_fixes(source, &diagnostics, false),
        source.replace("otherValue", "other_value")
    );
    let members = Parser::new(&Lexer::new(source).tokenize()).parse();
    let renames = fixer::extract_renames(source, &diagnostics, "test.gd", &members);
    assert_eq!(renames.len(), 1);
    assert_eq!(renames[0].new_name, "other_value");
}
