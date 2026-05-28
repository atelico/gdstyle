use crate::ast::ScriptFile;
use crate::config::Config;
use crate::diagnostic::{line_byte_offset, Diagnostic, Fix, Replacement};
use crate::token::{QuoteStyle, Span, StringPrefix, Token, TokenKind};

/// Check that no line exceeds the maximum length.
///
/// Tabs are counted at their visual width (4 columns, matching Godot's default)
/// rather than as a single character, so indented lines are measured as they
/// appear in the editor.
pub fn check_max_line_length(
    file: &ScriptFile,
    config: &Config,
    diagnostics: &mut Vec<Diagnostic>,
) {
    const TAB_WIDTH: usize = 4;
    for (i, line) in file.lines.iter().enumerate() {
        let line_num = i + 1;
        let visual_len = visual_line_length(line, TAB_WIDTH);
        if visual_len > config.max_line_length {
            diagnostics.push(Diagnostic::warning(
                "format/max-line-length",
                format!(
                    "line is {} characters long (max {})",
                    visual_len, config.max_line_length
                ),
                Span::new(line_num, config.max_line_length + 1, 0, 0),
                &file.path,
            ));
        }
    }
}

/// Compute the visual column width of a line, expanding tabs to `tab_width`.
fn visual_line_length(line: &str, tab_width: usize) -> usize {
    let mut col = 0;
    for ch in line.chars() {
        if ch == '\t' {
            col = (col / tab_width + 1) * tab_width;
        } else {
            col += 1;
        }
    }
    col
}

/// Check for trailing whitespace on any line.
pub fn check_trailing_whitespace(file: &ScriptFile, diagnostics: &mut Vec<Diagnostic>) {
    for (i, line) in file.lines.iter().enumerate() {
        let line_num = i + 1;
        if line.ends_with(' ') || line.ends_with('\t') {
            let trimmed_len = line.trim_end().len();
            let offset = line_byte_offset(&file.lines, i) + trimmed_len;
            let ws_len = line.len() - trimmed_len;
            diagnostics.push(
                Diagnostic::warning(
                    "format/trailing-whitespace",
                    "trailing whitespace".to_string(),
                    Span::new(line_num, trimmed_len + 1, offset, ws_len),
                    &file.path,
                )
                .with_fix(Fix {
                    replacements: vec![Replacement {
                        offset,
                        length: ws_len,
                        new_text: String::new(),
                    }],
                    is_safe: true,
                }),
            );
        }
    }
}

/// Check that the file ends with exactly one newline.
pub fn check_trailing_newline(file: &ScriptFile, diagnostics: &mut Vec<Diagnostic>) {
    if file.lines.is_empty() {
        return;
    }

    let last_line = &file.lines[file.lines.len() - 1];
    if !last_line.is_empty() {
        let offset = line_byte_offset(&file.lines, file.lines.len() - 1) + last_line.len();
        diagnostics.push(
            Diagnostic::warning(
                "format/trailing-newline",
                "file should end with a newline".to_string(),
                Span::new(file.lines.len(), last_line.len() + 1, offset, 0),
                &file.path,
            )
            .with_fix(Fix {
                replacements: vec![Replacement {
                    offset,
                    length: 0,
                    new_text: "\n".to_string(),
                }],
                is_safe: true,
            }),
        );
    }
}

/// Check indentation style (tabs or spaces based on config).
pub fn check_indentation_style(
    file: &ScriptFile,
    config: &Config,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (i, line) in file.lines.iter().enumerate() {
        let line_num = i + 1;
        if line.is_empty() {
            continue;
        }

        let indent: String = line
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        if indent.is_empty() {
            continue;
        }

        let has_tabs = indent.contains('\t');
        let has_spaces = indent.contains(' ');
        let offset = line_byte_offset(&file.lines, i);
        let indent_len = indent.len();

        // Mixed indentation is always wrong.
        if has_tabs && has_spaces {
            let new_indent = if config.use_tabs {
                // Convert spaces to tabs (assume 4 spaces = 1 tab).
                let tab_count =
                    indent
                        .chars()
                        .fold(0usize, |acc, c| if c == '\t' { acc + 4 } else { acc + 1 });
                "\t".repeat(tab_count / 4)
            } else {
                // Convert tabs to spaces (1 tab = 4 spaces).
                indent.replace('\t', "    ")
            };
            diagnostics.push(
                Diagnostic::warning(
                    "format/no-tabs-as-spaces",
                    "mixed tabs and spaces in indentation".to_string(),
                    Span::new(line_num, 1, offset, indent_len),
                    &file.path,
                )
                .with_fix(Fix {
                    replacements: vec![Replacement {
                        offset,
                        length: indent_len,
                        new_text: new_indent,
                    }],
                    is_safe: true,
                }),
            );
            continue;
        }

        if config.use_tabs && has_spaces {
            let tab_count = indent.len() / 4;
            let new_indent = "\t".repeat(if tab_count == 0 { 1 } else { tab_count });
            diagnostics.push(
                Diagnostic::warning(
                    "format/no-tabs-as-spaces",
                    "use tabs for indentation, not spaces".to_string(),
                    Span::new(line_num, 1, offset, indent_len),
                    &file.path,
                )
                .with_fix(Fix {
                    replacements: vec![Replacement {
                        offset,
                        length: indent_len,
                        new_text: new_indent,
                    }],
                    is_safe: true,
                }),
            );
        } else if !config.use_tabs && has_tabs {
            let new_indent = indent.replace('\t', "    ");
            diagnostics.push(
                Diagnostic::warning(
                    "format/no-tabs-as-spaces",
                    "use spaces for indentation, not tabs".to_string(),
                    Span::new(line_num, 1, offset, indent_len),
                    &file.path,
                )
                .with_fix(Fix {
                    replacements: vec![Replacement {
                        offset,
                        length: indent_len,
                        new_text: new_indent,
                    }],
                    is_safe: true,
                }),
            );
        }
    }
}

/// Check that boolean operators use English keywords (and/or/not) instead of &&/||/!.
pub fn check_boolean_operators(
    tokens: &[Token],
    file: &ScriptFile,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for token in tokens {
        match &token.kind {
            TokenKind::AmpersandAmpersand => {
                diagnostics.push(
                    Diagnostic::warning(
                        "format/boolean-operators",
                        "use 'and' instead of '&&'".to_string(),
                        token.span,
                        &file.path,
                    )
                    .with_fix(Fix {
                        replacements: vec![Replacement {
                            offset: token.span.offset,
                            length: token.span.length,
                            new_text: "and".to_string(),
                        }],
                        is_safe: true,
                    }),
                );
            }
            TokenKind::PipePipe => {
                diagnostics.push(
                    Diagnostic::warning(
                        "format/boolean-operators",
                        "use 'or' instead of '||'".to_string(),
                        token.span,
                        &file.path,
                    )
                    .with_fix(Fix {
                        replacements: vec![Replacement {
                            offset: token.span.offset,
                            length: token.span.length,
                            new_text: "or".to_string(),
                        }],
                        is_safe: true,
                    }),
                );
            }
            TokenKind::Bang => {
                diagnostics.push(
                    Diagnostic::warning(
                        "format/boolean-operators",
                        "use 'not' instead of '!'".to_string(),
                        token.span,
                        &file.path,
                    )
                    .with_fix(Fix {
                        replacements: vec![Replacement {
                            offset: token.span.offset,
                            length: token.span.length,
                            new_text: "not ".to_string(),
                        }],
                        is_safe: true,
                    }),
                );
            }
            _ => {}
        }
    }
}

/// Check that strings prefer double quotes.
pub fn check_double_quotes(tokens: &[Token], file: &ScriptFile, diagnostics: &mut Vec<Diagnostic>) {
    for token in tokens {
        if let TokenKind::String(ref info) = token.kind {
            if info.quote_style == QuoteStyle::Single
                && info.prefix == StringPrefix::None
                && !info.is_multiline
                && !info.value.contains('"')
            {
                // Build replacement: replace ' with " on both sides.
                let new_text = format!("\"{}\"", info.value);
                diagnostics.push(
                    Diagnostic::warning(
                        "format/double-quotes",
                        "use double quotes for strings".to_string(),
                        token.span,
                        &file.path,
                    )
                    .with_fix(Fix {
                        replacements: vec![Replacement {
                            offset: token.span.offset,
                            length: token.span.length,
                            new_text,
                        }],
                        is_safe: true,
                    }),
                );
            }
        }
    }
}

/// Check that comments have proper spacing after #.
pub fn check_comment_spacing(
    tokens: &[Token],
    file: &ScriptFile,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for token in tokens {
        match &token.kind {
            TokenKind::Comment(content) => {
                if !content.is_empty()
                    && !content.starts_with(' ')
                    && !content.starts_with('!')
                    && !content.starts_with("region")
                    && !content.starts_with("endregion")
                {
                    let first_char = content.chars().next().unwrap();
                    if first_char.is_ascii_alphabetic() || first_char == '_' || first_char == '\t' {
                        continue;
                    }
                    // Insert a space after the '#'.
                    let insert_offset = token.span.offset + 1; // after '#'
                    diagnostics.push(
                        Diagnostic::warning(
                            "format/comment-spacing",
                            "add a space after '#' in comments".to_string(),
                            token.span,
                            &file.path,
                        )
                        .with_fix(Fix {
                            replacements: vec![Replacement {
                                offset: insert_offset,
                                length: 0,
                                new_text: " ".to_string(),
                            }],
                            is_safe: true,
                        }),
                    );
                }
            }
            TokenKind::DocComment(_) => {
                // Don't touch ## doc comments. ## is meaningful in GDScript, and
                // codebases sometimes use ## as a "double-comment-out" of code
                // (e.g. `##var foo`) where a blanket "add a space after ##" rewrite
                // changes how a reader interprets the line.
            }
            _ => {}
        }
    }
}

/// Check for unnecessary parentheses in if/elif/while conditions.
///
/// Only flags single-line conditions where the outer parens wrap the entire
/// condition. Multi-line conditions need parentheses for line continuation
/// in GDScript, so removing them would break the code.
pub fn check_unnecessary_parens(
    tokens: &[Token],
    file: &ScriptFile,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut i = 0;
    while i < tokens.len() {
        if matches!(
            tokens[i].kind,
            TokenKind::If | TokenKind::Elif | TokenKind::While
        ) {
            let keyword_span = tokens[i].span;
            let j = i + 1;
            if j < tokens.len() && tokens[j].kind == TokenKind::LeftParen {
                let mut depth = 1;
                let mut k = j + 1;
                while k < tokens.len() && depth > 0 {
                    match tokens[k].kind {
                        TokenKind::LeftParen => depth += 1,
                        TokenKind::RightParen => depth -= 1,
                        _ => {}
                    }
                    if depth > 0 {
                        k += 1;
                    }
                }
                if k < tokens.len() && k + 1 < tokens.len() {
                    let next = &tokens[k + 1];
                    if next.kind == TokenKind::Colon {
                        let open_paren = &tokens[j];
                        let close_paren = &tokens[k];
                        // Only safe to remove parens if the condition is on a single line.
                        // Multi-line conditions need parens for line continuation in GDScript.
                        if open_paren.span.line == close_paren.span.line {
                            // If there's no whitespace between the keyword and
                            // the open paren (e.g. `if(cond):`), replace the
                            // open paren with a space so we don't end up with
                            // `ifcond:` after the strip.
                            let needs_space =
                                open_paren.span.offset == keyword_span.offset + keyword_span.length;
                            let open_replacement = if needs_space {
                                " ".to_string()
                            } else {
                                String::new()
                            };
                            diagnostics.push(
                                Diagnostic::warning(
                                    "format/no-unnecessary-parens",
                                    "unnecessary parentheses around condition".to_string(),
                                    keyword_span,
                                    &file.path,
                                )
                                .with_fix(Fix {
                                    replacements: vec![
                                        Replacement {
                                            offset: open_paren.span.offset,
                                            length: open_paren.span.length,
                                            new_text: open_replacement,
                                        },
                                        Replacement {
                                            offset: close_paren.span.offset,
                                            length: close_paren.span.length,
                                            new_text: String::new(),
                                        },
                                    ],
                                    is_safe: true,
                                }),
                            );
                        }
                    }
                }
            }
        }
        i += 1;
    }
}

/// Check number literal formatting (leading/trailing zeros, lowercase hex).
pub fn check_number_literals(
    tokens: &[Token],
    file: &ScriptFile,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for token in tokens {
        match &token.kind {
            TokenKind::Float(_) => {
                let text = &token.text;
                if text.starts_with('.') {
                    let fixed = format!("0{}", text);
                    diagnostics.push(
                        Diagnostic::warning(
                            "format/number-literals",
                            format!("add leading zero: '0{}'", text),
                            token.span,
                            &file.path,
                        )
                        .with_fix(Fix {
                            replacements: vec![Replacement {
                                offset: token.span.offset,
                                length: token.span.length,
                                new_text: fixed,
                            }],
                            is_safe: true,
                        }),
                    );
                }
                if text.ends_with('.') {
                    let fixed = format!("{}0", text);
                    diagnostics.push(
                        Diagnostic::warning(
                            "format/number-literals",
                            format!("add trailing zero: '{}0'", text),
                            token.span,
                            &file.path,
                        )
                        .with_fix(Fix {
                            replacements: vec![Replacement {
                                offset: token.span.offset,
                                length: token.span.length,
                                new_text: fixed,
                            }],
                            is_safe: true,
                        }),
                    );
                }
            }
            TokenKind::Integer(_) => {
                let text = &token.text;
                if text.starts_with("0x") || text.starts_with("0X") {
                    let hex_part = &text[2..];
                    if hex_part.chars().any(|c| c.is_ascii_uppercase() && c != '_') {
                        let fixed = format!("0x{}", hex_part.to_lowercase());
                        diagnostics.push(
                            Diagnostic::warning(
                                "format/number-literals",
                                format!(
                                    "use lowercase hex digits: '{}'",
                                    text[..2].to_lowercase() + &hex_part.to_lowercase()
                                ),
                                token.span,
                                &file.path,
                            )
                            .with_fix(Fix {
                                replacements: vec![Replacement {
                                    offset: token.span.offset,
                                    length: token.span.length,
                                    new_text: fixed,
                                }],
                                is_safe: true,
                            }),
                        );
                    }
                    if let Some(suffix) = text.strip_prefix("0X") {
                        let fixed = format!("0x{}", suffix.to_lowercase());
                        diagnostics.push(
                            Diagnostic::warning(
                                "format/number-literals",
                                "use lowercase '0x' prefix for hex numbers".to_string(),
                                token.span,
                                &file.path,
                            )
                            .with_fix(Fix {
                                replacements: vec![Replacement {
                                    offset: token.span.offset,
                                    length: token.span.length,
                                    new_text: fixed,
                                }],
                                is_safe: true,
                            }),
                        );
                    }
                }
            }
            _ => {}
        }
    }
}

/// Check for multiple statements on one line.
pub fn check_one_statement_per_line(
    tokens: &[Token],
    file: &ScriptFile,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (idx, token) in tokens.iter().enumerate() {
        if token.kind == TokenKind::Semicolon {
            // Figure out the indentation of the current line for the replacement.
            let line_idx = token.span.line - 1;
            let line = if line_idx < file.lines.len() {
                file.lines[line_idx].as_str()
            } else {
                ""
            };
            let indent: String = line
                .chars()
                .take_while(|c| *c == '\t' || *c == ' ')
                .collect();

            // Skip match-arm bodies: `<pattern>: stmt1; stmt2`. The second
            // statement isn't a peer of the first; it belongs to the same arm.
            // Splitting it onto a new line at the same indent would orphan
            // it. We detect the pattern conservatively: the line, after its
            // leading indent, starts with a pattern followed by `:` and
            // that `:` is to the LEFT of the `;`. Pattern tokens are
            // identifiers, integers, strings, `_`, with optional `,`-joined
            // alternatives, but never contain `=`/`if`/`for`/`while`.
            let semi_col =
                (token.span.offset).saturating_sub(line_byte_offset(&file.lines, line_idx));
            if is_match_arm_line(line, semi_col) {
                continue;
            }

            // Replace semicolon and any trailing whitespace with newline + indent.
            let mut replace_len = token.span.length;
            // Consume whitespace between the semicolon and the next token.
            if idx + 1 < tokens.len() {
                let next = &tokens[idx + 1];
                if next.kind != TokenKind::Newline {
                    let gap = next.span.offset - (token.span.offset + token.span.length);
                    replace_len += gap;
                }
            }

            diagnostics.push(
                Diagnostic::warning(
                    "format/one-statement-per-line",
                    "use one statement per line instead of ';'".to_string(),
                    token.span,
                    &file.path,
                )
                .with_fix(Fix {
                    replacements: vec![Replacement {
                        offset: token.span.offset,
                        length: replace_len,
                        new_text: format!("\n{}", indent),
                    }],
                    is_safe: true,
                }),
            );
        }
    }
}

// --- New formatting rules ---

/// Check for runs of 3+ consecutive blank lines anywhere in the file.
pub fn check_blank_lines(file: &ScriptFile, diagnostics: &mut Vec<Diagnostic>) {
    let mut consecutive_blank = 0;
    let mut run_start_idx: usize = 0;

    for (idx, line) in file.lines.iter().enumerate() {
        if line.trim().is_empty() {
            consecutive_blank += 1;
            if consecutive_blank == 1 {
                run_start_idx = idx;
            }
        } else {
            if consecutive_blank > 2 {
                // Lines run_start_idx..=idx-1 are blank; keep first 2, remove rest.
                let first_excess_idx = run_start_idx + 2;
                let last_excess_idx = idx - 1;
                let excess_start = line_byte_offset(&file.lines, first_excess_idx);
                let excess_end = line_byte_offset(&file.lines, last_excess_idx)
                    + file.lines[last_excess_idx].len()
                    + 1; // +1 for the newline

                let report_offset = line_byte_offset(&file.lines, run_start_idx);
                diagnostics.push(
                    Diagnostic::warning(
                        "format/blank-lines",
                        format!(
                            "too many blank lines ({}, expected at most 2)",
                            consecutive_blank
                        ),
                        Span::new(run_start_idx + 1, 1, report_offset, 0),
                        &file.path,
                    )
                    .with_fix(Fix {
                        replacements: vec![Replacement {
                            offset: excess_start,
                            length: excess_end - excess_start,
                            new_text: String::new(),
                        }],
                        is_safe: true,
                    }),
                );
            }
            consecutive_blank = 0;
        }
    }
}

/// Check that float literals have leading/trailing zeros.
pub fn check_float_literal_zeros(
    tokens: &[Token],
    file: &ScriptFile,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for token in tokens {
        if let TokenKind::Float(_) = &token.kind {
            let text = &token.text;
            if text.starts_with('.') {
                let fixed = format!("0{}", text);
                diagnostics.push(
                    Diagnostic::warning(
                        "format/float-literal-zeros",
                        format!("use '{}' instead of '{}'", fixed, text),
                        token.span,
                        &file.path,
                    )
                    .with_fix(Fix {
                        replacements: vec![Replacement {
                            offset: token.span.offset,
                            length: token.span.length,
                            new_text: fixed,
                        }],
                        is_safe: true,
                    }),
                );
            } else if text.ends_with('.') {
                let fixed = format!("{}0", text);
                diagnostics.push(
                    Diagnostic::warning(
                        "format/float-literal-zeros",
                        format!("use '{}' instead of '{}'", fixed, text),
                        token.span,
                        &file.path,
                    )
                    .with_fix(Fix {
                        replacements: vec![Replacement {
                            offset: token.span.offset,
                            length: token.span.length,
                            new_text: fixed,
                        }],
                        is_safe: true,
                    }),
                );
            }
        }
    }
}

/// Check that large numbers use underscores for readability.
pub fn check_large_number_underscores(
    tokens: &[Token],
    file: &ScriptFile,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for token in tokens {
        if let TokenKind::Integer(val) = &token.kind {
            let text = &token.text;
            // Skip hex/binary/octal and already-grouped numbers.
            if text.starts_with("0x")
                || text.starts_with("0X")
                || text.starts_with("0b")
                || text.starts_with("0B")
                || text.contains('_')
            {
                continue;
            }
            if *val >= 10_000 || *val <= -10_000 {
                let fixed = format_with_underscores(*val);
                diagnostics.push(
                    Diagnostic::warning(
                        "format/large-number-underscores",
                        format!("use '{}' instead of '{}' for readability", fixed, text),
                        token.span,
                        &file.path,
                    )
                    .with_fix(Fix {
                        replacements: vec![Replacement {
                            offset: token.span.offset,
                            length: token.span.length,
                            new_text: fixed,
                        }],
                        is_safe: true,
                    }),
                );
            }
        }
    }
}

fn format_with_underscores(val: i64) -> String {
    let negative = val < 0;
    let abs_str = val.unsigned_abs().to_string();
    let chars: Vec<char> = abs_str.chars().collect();
    let mut result = String::new();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && (chars.len() - i).is_multiple_of(3) {
            result.push('_');
        }
        result.push(*c);
    }
    if negative {
        format!("-{}", result)
    } else {
        result
    }
}

use crate::ast::ClassMember;

/// Check for trailing comma on last item of multi-line arrays/dicts/enums.
pub fn check_trailing_comma(
    tokens: &[Token],
    file: &ScriptFile,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut i = 0;
    while i < tokens.len() {
        let open_kind = &tokens[i].kind;
        let (close_kind, name) = match open_kind {
            TokenKind::LeftBracket => (TokenKind::RightBracket, "array"),
            TokenKind::LeftBrace => (TokenKind::RightBrace, "dictionary"),
            _ => {
                i += 1;
                continue;
            }
        };

        // For `[`, distinguish array literal from subscript by looking at the
        // previous significant token. A subscript is `<expr>[idx]`, where
        // the previous token ends an expression. GDScript does not allow
        // trailing commas inside subscripts, so we must skip those.
        if matches!(open_kind, TokenKind::LeftBracket) && is_subscript_open_bracket(tokens, i) {
            i += 1;
            continue;
        }

        let open_line = tokens[i].span.line;

        // Find matching close.
        let mut depth = 1;
        let mut k = i + 1;
        while k < tokens.len() && depth > 0 {
            if tokens[k].kind == *open_kind {
                depth += 1;
            } else if tokens[k].kind == close_kind {
                depth -= 1;
            }
            if depth > 0 {
                k += 1;
            }
        }

        if k < tokens.len() {
            let close_line = tokens[k].span.line;
            // Multi-line if close is on a different line than open.
            if close_line > open_line {
                // Find the last non-newline, non-indent/dedent, non-comment token before close.
                let mut last_item = k - 1;
                while last_item > i {
                    match tokens[last_item].kind {
                        TokenKind::Newline
                        | TokenKind::Indent
                        | TokenKind::Dedent
                        | TokenKind::Comment(_)
                        | TokenKind::DocComment(_) => {
                            last_item -= 1;
                        }
                        _ => break,
                    }
                }
                if last_item > i && tokens[last_item].kind != TokenKind::Comma {
                    let insert_offset =
                        tokens[last_item].span.offset + tokens[last_item].span.length;
                    diagnostics.push(
                        Diagnostic::warning(
                            "format/trailing-comma",
                            format!("add trailing comma in multi-line {}", name),
                            tokens[last_item].span,
                            &file.path,
                        )
                        .with_fix(Fix {
                            replacements: vec![Replacement {
                                offset: insert_offset,
                                length: 0,
                                new_text: ",".to_string(),
                            }],
                            is_safe: true,
                        }),
                    );
                }
            }
        }

        i += 1;
    }
}

/// Check operator spacing (one space around binary operators, one after commas).
pub fn check_operator_spacing(
    tokens: &[Token],
    file: &ScriptFile,
    source: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let binary_ops = [
        TokenKind::Plus,
        TokenKind::Minus,
        TokenKind::Star,
        TokenKind::Slash,
        TokenKind::Percent,
        TokenKind::Equal,
        TokenKind::NotEqual,
        TokenKind::Less,
        TokenKind::LessEqual,
        TokenKind::Greater,
        TokenKind::GreaterEqual,
        TokenKind::Assign,
        TokenKind::PlusAssign,
        TokenKind::MinusAssign,
        TokenKind::StarAssign,
        TokenKind::SlashAssign,
        TokenKind::PercentAssign,
        TokenKind::And,
        TokenKind::Or,
    ];

    for (idx, token) in tokens.iter().enumerate() {
        // Check binary operator spacing.
        if binary_ops.contains(&token.kind) && idx > 0 && idx + 1 < tokens.len() {
            // Skip := (inferred type assign), Colon followed immediately by Assign.
            // Don't add space after Colon when next is Assign, or before Assign when prev is Colon.
            if token.kind == TokenKind::Assign {
                let prev = &tokens[idx - 1];
                if prev.kind == TokenKind::Colon
                    && prev.span.offset + prev.span.length == token.span.offset
                {
                    continue;
                }
            }

            // Skip / in node paths ($NodePath/Child, %UniqueNode/Child).
            // A slash is a node path separator when preceded by an Identifier (or RightParen, etc.)
            // that follows a $ or % token.
            if token.kind == TokenKind::Slash {
                // Walk backwards to see if we're in a node path expression.
                let mut is_node_path = false;
                let mut scan = idx - 1;
                loop {
                    match &tokens[scan].kind {
                        TokenKind::Identifier(_) | TokenKind::Slash => {
                            if scan == 0 {
                                break;
                            }
                            scan -= 1;
                        }
                        TokenKind::Dollar | TokenKind::UniqueNodeMarker => {
                            is_node_path = true;
                            break;
                        }
                        _ => break,
                    }
                }
                if is_node_path {
                    continue;
                }
            }

            // Skip unary minus/plus. The operator is BINARY only when the
            // preceding token is a value-producing operand (identifier, literal,
            // closing bracket, self/super). In every other context, keywords
            // like `else`/`return`/`if`/`and`, operators, commas, opening
            // brackets, line starts, `-`/`+` is unary.
            if matches!(token.kind, TokenKind::Plus | TokenKind::Minus)
                && !is_operand_end(&tokens[idx - 1].kind)
            {
                continue;
            }

            // Skip -> arrow (type hints).
            if token.kind == TokenKind::Minus
                && idx + 1 < tokens.len()
                && tokens[idx + 1].kind == TokenKind::Greater
            {
                continue;
            }
            if token.kind == TokenKind::Greater
                && idx > 0
                && tokens[idx - 1].kind == TokenKind::Minus
            {
                continue;
            }

            // Skip ** (power operator), don't require space around it.
            if token.kind == TokenKind::Star {
                if idx + 1 < tokens.len() && tokens[idx + 1].kind == TokenKind::Star {
                    continue;
                }
                if idx > 0 && tokens[idx - 1].kind == TokenKind::Star {
                    continue;
                }
            }

            let token_end = token.span.offset + token.span.length;

            // Check space before.
            if token.span.offset > 0 {
                let byte_before = source.as_bytes().get(token.span.offset - 1);
                if byte_before != Some(&b' ')
                    && byte_before != Some(&b'\t')
                    && byte_before != Some(&b'\n')
                {
                    diagnostics.push(
                        Diagnostic::warning(
                            "format/operator-spacing",
                            format!("add space before '{}'", token.text),
                            token.span,
                            &file.path,
                        )
                        .with_fix(Fix {
                            replacements: vec![Replacement {
                                offset: token.span.offset,
                                length: 0,
                                new_text: " ".to_string(),
                            }],
                            is_safe: true,
                        }),
                    );
                }
            }

            // Check space after.
            if token_end < source.len() {
                let byte_after = source.as_bytes().get(token_end);
                if byte_after != Some(&b' ')
                    && byte_after != Some(&b'\t')
                    && byte_after != Some(&b'\n')
                {
                    // Skip space after Colon when next is '=' (part of :=).
                    if token.kind == TokenKind::Colon && byte_after == Some(&b'=') {
                        continue;
                    }
                    diagnostics.push(
                        Diagnostic::warning(
                            "format/operator-spacing",
                            format!("add space after '{}'", token.text),
                            token.span,
                            &file.path,
                        )
                        .with_fix(Fix {
                            replacements: vec![Replacement {
                                offset: token_end,
                                length: 0,
                                new_text: " ".to_string(),
                            }],
                            is_safe: true,
                        }),
                    );
                }
            }
        }
    }
}

/// Enforce canonical spacing around `:` tokens:
///
/// - **No space before** in any context. `if x :` is wrong everywhere.
/// - **One space after** in value-introducing positions: type hints
///   (`var x: int`), dict keys (`{"k": 1}`), single-line statement bodies
///   (`if cond: pass`). Skipped when the next token is `=` (`:=` inferred
///   type) or a newline (block-opening colon at end of line).
pub fn check_colon_spacing(
    tokens: &[Token],
    file: &ScriptFile,
    source: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (idx, token) in tokens.iter().enumerate() {
        if token.kind != TokenKind::Colon {
            continue;
        }

        // The `:=` inferred-type operator gets its own conventional spacing
        // (`var x := y`): a space before the `:`, no space between `:` and
        // `=`, a space after the `=`. The Assign rule handles space-after-`=`;
        // here we just skip both the "no space before" and "space after"
        // checks when this colon is the leading half of `:=`.
        let is_walrus = source
            .as_bytes()
            .get(token.span.offset + token.span.length)
            == Some(&b'=');

        // No space before the colon (skipped for `:=`).
        if !is_walrus && token.span.offset > 0 {
            let prev_byte = source.as_bytes().get(token.span.offset - 1);
            if prev_byte == Some(&b' ') || prev_byte == Some(&b'\t') {
                // Walk back to the start of the whitespace run.
                let mut ws_start = token.span.offset - 1;
                while ws_start > 0 {
                    let b = source.as_bytes().get(ws_start - 1);
                    if b == Some(&b' ') || b == Some(&b'\t') {
                        ws_start -= 1;
                    } else {
                        break;
                    }
                }
                // Only flag stray whitespace mid-line; if the previous
                // non-whitespace is on a different line, skip (line
                // continuation or indentation case).
                let leading_byte = source.as_bytes().get(ws_start.saturating_sub(1));
                if ws_start > 0 && leading_byte != Some(&b'\n') {
                    let ws_len = token.span.offset - ws_start;
                    diagnostics.push(
                        Diagnostic::warning(
                            "format/colon-spacing",
                            "no space before ':'".to_string(),
                            token.span,
                            &file.path,
                        )
                        .with_fix(Fix {
                            replacements: vec![Replacement {
                                offset: ws_start,
                                length: ws_len,
                                new_text: String::new(),
                            }],
                            is_safe: true,
                        }),
                    );
                }
            }
        }

        // One space after the colon, unless followed by `=` (`:=`) or end of
        // line (`if cond:` block opener, `func f():` etc.).
        let token_end = token.span.offset + token.span.length;
        if token_end < source.len() {
            let next_byte = source.as_bytes().get(token_end);
            let no_space_required = matches!(next_byte, Some(&b'=') | Some(&b'\n') | Some(&b' ') | Some(&b'\t'));
            if !no_space_required {
                // Also tolerate when the next token IS a Newline (the byte
                // check above already handles `\n`, but be defensive about
                // CR/EOF).
                let next_tok = tokens.get(idx + 1);
                if next_tok.is_some_and(|t| t.kind == TokenKind::Newline) {
                    continue;
                }
                diagnostics.push(
                    Diagnostic::warning(
                        "format/colon-spacing",
                        "add space after ':'".to_string(),
                        token.span,
                        &file.path,
                    )
                    .with_fix(Fix {
                        replacements: vec![Replacement {
                            offset: token_end,
                            length: 0,
                            new_text: " ".to_string(),
                        }],
                        is_safe: true,
                    }),
                );
            }
        }
    }
}

/// Enforce canonical spacing around `,` tokens:
///
/// - **No space before**: `f(a ,b)` is always wrong.
/// - **One space after**: `f(a,b)` should be `f(a, b)`. Skipped when the
///   comma is the last item before a newline or a closing bracket (trailing
///   commas in multi-line collections / parameter lists are valid and need
///   no following space).
pub fn check_comma_spacing(
    tokens: &[Token],
    file: &ScriptFile,
    source: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for token in tokens.iter() {
        if token.kind != TokenKind::Comma {
            continue;
        }

        // No space before the comma.
        if token.span.offset > 0 {
            let prev_byte = source.as_bytes().get(token.span.offset - 1);
            if prev_byte == Some(&b' ') || prev_byte == Some(&b'\t') {
                let mut ws_start = token.span.offset - 1;
                while ws_start > 0 {
                    let b = source.as_bytes().get(ws_start - 1);
                    if b == Some(&b' ') || b == Some(&b'\t') {
                        ws_start -= 1;
                    } else {
                        break;
                    }
                }
                let leading_byte = source.as_bytes().get(ws_start.saturating_sub(1));
                if ws_start > 0 && leading_byte != Some(&b'\n') {
                    let ws_len = token.span.offset - ws_start;
                    diagnostics.push(
                        Diagnostic::warning(
                            "format/comma-spacing",
                            "no space before ','".to_string(),
                            token.span,
                            &file.path,
                        )
                        .with_fix(Fix {
                            replacements: vec![Replacement {
                                offset: ws_start,
                                length: ws_len,
                                new_text: String::new(),
                            }],
                            is_safe: true,
                        }),
                    );
                }
            }
        }

        // One space after the comma, unless followed by a newline or a
        // closing delimiter (trailing comma in a multi-line list).
        let token_end = token.span.offset + token.span.length;
        if token_end < source.len() {
            let next_byte = source.as_bytes().get(token_end);
            let no_space_required = matches!(
                next_byte,
                Some(&b' ')
                    | Some(&b'\t')
                    | Some(&b'\n')
                    | Some(&b')')
                    | Some(&b']')
                    | Some(&b'}')
            );
            if !no_space_required {
                diagnostics.push(
                    Diagnostic::warning(
                        "format/comma-spacing",
                        "add space after ','".to_string(),
                        token.span,
                        &file.path,
                    )
                    .with_fix(Fix {
                        replacements: vec![Replacement {
                            offset: token_end,
                            length: 0,
                            new_text: " ".to_string(),
                        }],
                        is_safe: true,
                    }),
                );
            }
        }
    }
}

/// Check enum members are each on their own line.
pub fn check_enum_one_per_line(
    file: &ScriptFile,
    source: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for member in &file.members {
        if let ClassMember::Enum {
            name,
            members: enum_members,
            span,
            ..
        } = member
        {
            if enum_members.len() <= 1 {
                continue;
            }
            // Check if all members are on the same line.
            let first_line = enum_members[0].span.line;
            let all_same_line = enum_members.iter().all(|m| m.span.line == first_line);
            if all_same_line && first_line == span.line {
                // Build the auto-fix: reformat enum to multi-line.
                let fix = source.and_then(|src| {
                    build_enum_multiline_fix(src, *span, name.as_deref(), enum_members)
                });

                let mut diag = Diagnostic::warning(
                    "format/enum-one-per-line",
                    "enum members should each be on their own line".to_string(),
                    *span,
                    &file.path,
                );
                if let Some(fix) = fix {
                    diag = diag.with_fix(fix);
                }
                diagnostics.push(diag);
            }
        }
    }
}

/// Build a Fix that reformats a single-line enum to multi-line.
/// Extracts member text from the original source to preserve value assignments.
fn build_enum_multiline_fix(
    source: &str,
    span: Span,
    name: Option<&str>,
    _members: &[crate::ast::EnumMember],
) -> Option<Fix> {
    // Find the line in the source.
    let line_idx = span.line - 1;
    let lines: Vec<&str> = source.split('\n').collect();
    if line_idx >= lines.len() {
        return None;
    }
    let line = lines[line_idx];
    let line_offset: usize = lines[..line_idx].iter().map(|l| l.len() + 1).sum();

    // Detect the indentation of the enum declaration.
    let indent: String = line
        .chars()
        .take_while(|c| *c == '\t' || *c == ' ')
        .collect();
    let member_indent = format!("{}\t", indent);

    // Extract the content between { and } from the original line to preserve
    // value assignments like IDLE = 0.
    let brace_open = line.find('{')?;
    let brace_close = line.rfind('}')?;
    let inner = &line[brace_open + 1..brace_close];

    // Split by commas (top-level only, respecting nested parens for defaults).
    let items: Vec<&str> = inner
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    if items.is_empty() {
        return None;
    }

    // Build the replacement: enum Name {\n\tMEMBER = VAL,\n...\n}
    let mut new_text = String::new();
    new_text.push_str(&indent);
    new_text.push_str("enum ");
    if let Some(n) = name {
        new_text.push_str(n);
        new_text.push(' ');
    }
    new_text.push_str("{\n");
    for item in &items {
        new_text.push_str(&member_indent);
        new_text.push_str(item);
        new_text.push_str(",\n");
    }
    new_text.push_str(&indent);
    new_text.push('}');

    Some(Fix {
        replacements: vec![Replacement {
            offset: line_offset,
            length: line.len(),
            new_text,
        }],
        is_safe: true,
    })
}

/// True if `line` looks like a `match` arm body (`<indent><pattern>: stmt`
/// with the `:` at column < `semi_col`). Used to skip the
/// one-statement-per-line autofix on lines where splitting `;` would orphan
/// the second statement out of the arm.
fn is_match_arm_line(line: &str, semi_col: usize) -> bool {
    // Find the first `:` on the line that is not part of `:=`, `::`, or a
    // string literal. We scan left to right, byte-by-byte, ignoring chars
    // inside strings.
    let bytes = line.as_bytes();
    let mut i = 0;
    let mut in_string: Option<u8> = None;
    let mut escaped = false;
    while i < bytes.len() && i < semi_col {
        let b = bytes[i];
        if let Some(q) = in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == q {
                in_string = None;
            }
            i += 1;
            continue;
        }
        match b {
            b'"' | b'\'' => {
                in_string = Some(b);
                i += 1;
            }
            b'#' => return false, // comment, not a match arm
            b':' => {
                // Skip `:=` and `::`.
                if i + 1 < bytes.len() && (bytes[i + 1] == b'=' || bytes[i + 1] == b':') {
                    i += 2;
                    continue;
                }
                // Found a real `:`. Inspect what's before it (after stripping
                // leading indent). If it's a "simple match pattern" (only
                // identifiers, literals, `_`, `,`, `.`, `[`/`]`, whitespace)
                // treat the line as a match arm. Anything containing `=` /
                // keyword like `if`/`else`/`for`/`while` is not.
                let leading: usize = bytes
                    .iter()
                    .take_while(|c| **c == b'\t' || **c == b' ')
                    .count();
                let prefix = &line[leading..i];
                if prefix.is_empty() {
                    return false;
                }
                let bad = ["=", " if ", " for ", " while ", "func ", "var ", "const "];
                if bad.iter().any(|kw| prefix.contains(kw)) {
                    return false;
                }
                if prefix.chars().all(|c| {
                    c.is_ascii_alphanumeric()
                        || c == '_'
                        || c == ','
                        || c == '.'
                        || c == ' '
                        || c == '\t'
                        || c == '['
                        || c == ']'
                        || c == '"'
                        || c == '\''
                        || c == '-'
                }) {
                    return true;
                }
                return false;
            }
            _ => i += 1,
        }
    }
    false
}

/// True when a `[` at `idx` opens a subscript expression (e.g. `arr[0]`)
/// rather than an array literal. We detect subscripts by looking at the
/// previous significant token: if it's an identifier, closing bracket, or any
/// value-ending token, the `[` is a subscript and we must NOT treat its
/// contents like an array (e.g. for trailing-comma insertion).
fn is_subscript_open_bracket(tokens: &[Token], idx: usize) -> bool {
    let mut j = idx;
    while j > 0 {
        j -= 1;
        match &tokens[j].kind {
            TokenKind::Newline
            | TokenKind::Indent
            | TokenKind::Dedent
            | TokenKind::Comment(_)
            | TokenKind::DocComment(_) => continue,
            kind => return is_operand_end(kind),
        }
    }
    false
}

/// Returns true if a token kind, when appearing immediately to the left of `-`
/// or `+`, would make that operator a binary operator (because the token ends a
/// value-producing expression). Anything else (operators, keywords like `else`/
/// `return`/`if`/`and`, opening brackets, commas, line starts) means the `-`/`+`
/// is unary.
fn is_operand_end(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Identifier(_)
            | TokenKind::Integer(_)
            | TokenKind::Float(_)
            | TokenKind::String(_)
            | TokenKind::Bool(_)
            | TokenKind::Null
            | TokenKind::RightParen
            | TokenKind::RightBracket
            | TokenKind::RightBrace
            | TokenKind::Self_
            | TokenKind::Super
            | TokenKind::ClassName
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_file(lines: &[&str]) -> ScriptFile {
        ScriptFile {
            path: "test.gd".to_string(),
            members: vec![],
            lines: lines.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn tokenize(source: &str) -> Vec<Token> {
        let mut lexer = crate::lexer::Lexer::new(source);
        lexer.tokenize()
    }

    #[test]
    fn test_max_line_length() {
        let long_line = "x".repeat(120);
        let file = make_file(&[&long_line, ""]);
        let config = Config::default();
        let mut diags = Vec::new();
        check_max_line_length(&file, &config, &mut diags);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("120"));
    }

    #[test]
    fn test_max_line_length_passes() {
        let file = make_file(&["var x = 5", ""]);
        let config = Config::default();
        let mut diags = Vec::new();
        check_max_line_length(&file, &config, &mut diags);
        assert!(diags.is_empty());
    }

    #[test]
    fn test_trailing_whitespace() {
        let file = make_file(&["var x = 5   ", ""]);
        let mut diags = Vec::new();
        check_trailing_whitespace(&file, &mut diags);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].fix.is_some());
    }

    #[test]
    fn test_trailing_newline_missing() {
        let file = make_file(&["var x = 5"]);
        let mut diags = Vec::new();
        check_trailing_newline(&file, &mut diags);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].fix.is_some());
    }

    #[test]
    fn test_trailing_newline_present() {
        let file = make_file(&["var x = 5", ""]);
        let mut diags = Vec::new();
        check_trailing_newline(&file, &mut diags);
        assert!(diags.is_empty());
    }

    #[test]
    fn test_indentation_spaces_when_tabs_required() {
        let file = make_file(&["    var x = 5", ""]);
        let config = Config::default(); // use_tabs: true
        let mut diags = Vec::new();
        check_indentation_style(&file, &config, &mut diags);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("tabs"));
        assert!(diags[0].fix.is_some());
    }

    #[test]
    fn test_indentation_mixed() {
        let file = make_file(&["\t var x = 5", ""]);
        let config = Config::default();
        let mut diags = Vec::new();
        check_indentation_style(&file, &config, &mut diags);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("mixed"));
        assert!(diags[0].fix.is_some());
    }

    #[test]
    fn test_boolean_operators_and() {
        let source = "if a && b:\n\tpass\n";
        let tokens = tokenize(source);
        let file = make_file(&["if a && b:", "\tpass", ""]);
        let mut diags = Vec::new();
        check_boolean_operators(&tokens, &file, &mut diags);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'and'"));
        assert!(diags[0].fix.is_some());
    }

    #[test]
    fn test_boolean_operators_or() {
        let source = "if a || b:\n\tpass\n";
        let tokens = tokenize(source);
        let file = make_file(&["if a || b:", "\tpass", ""]);
        let mut diags = Vec::new();
        check_boolean_operators(&tokens, &file, &mut diags);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'or'"));
        assert!(diags[0].fix.is_some());
    }

    #[test]
    fn test_boolean_operators_not() {
        let source = "if !a:\n\tpass\n";
        let tokens = tokenize(source);
        let file = make_file(&["if !a:", "\tpass", ""]);
        let mut diags = Vec::new();
        check_boolean_operators(&tokens, &file, &mut diags);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'not'"));
        assert!(diags[0].fix.is_some());
    }

    #[test]
    fn test_double_quotes() {
        let source = "'hello'";
        let tokens = tokenize(source);
        let file = make_file(&["'hello'", ""]);
        let mut diags = Vec::new();
        check_double_quotes(&tokens, &file, &mut diags);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].fix.is_some());
    }

    #[test]
    fn test_single_quotes_with_double_inside_ok() {
        let source = r#"'he said "hi"'"#;
        let tokens = tokenize(source);
        let file = make_file(&[r#"'he said "hi"'"#, ""]);
        let mut diags = Vec::new();
        check_double_quotes(&tokens, &file, &mut diags);
        assert!(
            diags.is_empty(),
            "single quotes ok when containing double quotes"
        );
    }

    #[test]
    fn test_comment_spacing() {
        let source = "#bad comment\n# good comment\n";
        let tokens = tokenize(source);
        let file = make_file(&["#bad comment", "# good comment", ""]);
        let mut diags = Vec::new();
        check_comment_spacing(&tokens, &file, &mut diags);
        assert!(diags.is_empty());
    }

    #[test]
    fn test_unnecessary_parens() {
        let source = "if (x > 5):\n\tpass\n";
        let tokens = tokenize(source);
        let file = make_file(&["if (x > 5):", "\tpass", ""]);
        let mut diags = Vec::new();
        check_unnecessary_parens(&tokens, &file, &mut diags);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].fix.is_some());
    }

    #[test]
    fn test_number_literals_uppercase_hex() {
        let source = "0xFF";
        let tokens = tokenize(source);
        let file = make_file(&["0xFF", ""]);
        let mut diags = Vec::new();
        check_number_literals(&tokens, &file, &mut diags);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("lowercase"));
        assert!(diags[0].fix.is_some());
    }

    #[test]
    fn test_number_literals_lowercase_hex_ok() {
        let source = "0xff";
        let tokens = tokenize(source);
        let file = make_file(&["0xff", ""]);
        let mut diags = Vec::new();
        check_number_literals(&tokens, &file, &mut diags);
        assert!(diags.is_empty());
    }

    #[test]
    fn test_semicolon_detected() {
        let source = "var x = 5; var y = 10";
        let tokens = tokenize(source);
        let file = make_file(&["var x = 5; var y = 10", ""]);
        let mut diags = Vec::new();
        check_one_statement_per_line(&tokens, &file, &mut diags);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].fix.is_some());
    }

    #[test]
    fn test_large_number_underscores() {
        let source = "var x = 1000000\n";
        let tokens = tokenize(source);
        let file = make_file(&["var x = 1000000", ""]);
        let mut diags = Vec::new();
        check_large_number_underscores(&tokens, &file, &mut diags);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("1_000_000"));
        assert!(diags[0].fix.is_some());
    }

    #[test]
    fn test_large_number_underscores_small_ok() {
        let source = "var x = 999\n";
        let tokens = tokenize(source);
        let file = make_file(&["var x = 999", ""]);
        let mut diags = Vec::new();
        check_large_number_underscores(&tokens, &file, &mut diags);
        assert!(diags.is_empty());
    }
}
