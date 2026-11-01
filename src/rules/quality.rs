use std::borrow::Cow;
use std::collections::HashMap;

use crate::ast::{for_each_member, ClassMember, ScriptFile};
use crate::config::Config;
use crate::diagnostic::Diagnostic;
use crate::token::{Span, StringLines, Token, TokenKind};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns the indentation level (number of leading tabs or spaces) of a line.
fn indent_level(line: &str) -> usize {
    line.chars()
        .take_while(|ch| *ch == '\t' || *ch == ' ')
        .count()
}

/// Returns the body lines (0-indexed into `file.lines`) for a function.
/// `span.line` is 1-indexed; the body starts on the line after the signature.
fn function_body_range(
    span: &Span,
    body_line_count: usize,
    total_lines: usize,
) -> std::ops::Range<usize> {
    let start = span.line; // 0-indexed: line after the `func` line
    let end = (start + body_line_count).min(total_lines);
    start..end
}

/// Check if a trimmed line is purely a comment or blank.
fn is_comment_or_blank(trimmed: &str) -> bool {
    trimmed.is_empty() || trimmed.starts_with('#')
}

/// Return `line` with string-literal contents and any trailing comment
/// replaced by spaces, keeping the quotes so the structure is still visible.
/// Every byte of string data or comment becomes one space, so the copy has
/// the same byte length and a byte offset in it is the same offset in
/// `line`. Used by the line-based quality rules so a keyword appearing
/// inside a string literal (`var s = " await "`) isn't mistaken for code.
/// A line with no quote and no `#` comes back as it is, without a copy.
///
/// Literals are read as [`for_each_line_piece`] reads them.
fn blank_strings_and_comments(line: &str) -> Cow<'_, str> {
    if !has_string_or_comment(line) {
        return Cow::Borrowed(line);
    }
    let mut out = String::with_capacity(line.len());
    let blank = |out: &mut String, length: usize| out.extend(std::iter::repeat_n(' ', length));
    for_each_line_piece(line, |piece| match piece {
        LinePiece::Code(code) => out.push_str(code),
        LinePiece::String {
            opening,
            data_length,
            closing,
        } => {
            out.push_str(opening);
            blank(&mut out, data_length);
            out.push_str(closing);
        }
        LinePiece::Comment(comment) => blank(&mut out, comment.len()),
    });
    Cow::Owned(out)
}

/// True when `line` has a quote or a `#`, the only bytes that start a
/// string literal or a comment. Most lines have neither, and checking for
/// them is cheaper than splitting the line with [`for_each_line_piece`].
fn has_string_or_comment(line: &str) -> bool {
    line.bytes().any(|byte| matches!(byte, b'"' | b'\'' | b'#'))
}

/// A piece of one line of code, from [`for_each_line_piece`].
enum LinePiece<'a> {
    /// Code outside string literals and comments.
    Code(&'a str),
    /// A string literal: its opening quotes, the byte length of its data,
    /// and its closing quotes (empty when it is still open at the line end).
    String {
        opening: &'a str,
        data_length: usize,
        closing: &'a str,
    },
    /// The trailing comment, from its `#` to the end of the line.
    Comment(&'a str),
}

/// Split one line of code into code, string literals and the trailing
/// comment, and pass each piece to `visit` in order. The pieces cover the
/// whole line.
///
/// Literals are read like Godot reads them, one line at a time: `"""` and
/// `'''` open a triple-quoted literal that only the same three quotes close,
/// and a backslash keeps the next character from closing the literal, raw
/// strings (`r"\""`) included. A literal still open at the end of the line
/// takes the rest of it. Every piece starts and ends at an ASCII byte or at
/// the end of the line, so slicing there is always on a character boundary.
fn for_each_line_piece<'a>(line: &'a str, mut visit: impl FnMut(LinePiece<'a>)) {
    let bytes = line.as_bytes();
    // `get` rather than indexing: the offsets below are always character
    // boundaries (see above), so this never falls back, but it can't panic.
    let slice = |start: usize, end: usize| line.get(start..end).unwrap_or("");
    let mut code_start = 0;
    loop {
        let special = bytes[code_start..]
            .iter()
            .position(|byte| matches!(byte, b'"' | b'\'' | b'#'))
            .map(|position| code_start + position);
        let Some(piece_start) = special else {
            if code_start < bytes.len() {
                visit(LinePiece::Code(slice(code_start, bytes.len())));
            }
            return;
        };
        if piece_start > code_start {
            visit(LinePiece::Code(slice(code_start, piece_start)));
        }
        let quote = bytes[piece_start];
        if quote == b'#' {
            visit(LinePiece::Comment(slice(piece_start, bytes.len())));
            return;
        }
        let delimiter = if bytes[piece_start..].starts_with(&[quote; 3]) {
            3
        } else {
            1
        };
        let data_start = piece_start + delimiter;
        let mut search_from = data_start;
        // The data ends at the closing delimiter, or at the line end.
        let (data_end, closing_end) = loop {
            let Some(position) = bytes
                .get(search_from..)
                .and_then(|rest| rest.iter().position(|&byte| byte == quote || byte == b'\\'))
            else {
                break (bytes.len(), bytes.len());
            };
            let at = search_from + position;
            if bytes[at] == b'\\' {
                // The escaped byte may start a multi-byte character; the
                // search goes on from inside it, which is fine, since it only
                // stops at ASCII bytes.
                search_from = at + 2;
            } else if delimiter == 1 || bytes[at..].starts_with(&[quote; 3]) {
                break (at, at + delimiter);
            } else {
                search_from = at + 1;
            }
        };
        visit(LinePiece::String {
            opening: slice(piece_start, data_start.min(bytes.len())),
            data_length: data_end.saturating_sub(data_start),
            closing: slice(data_end, closing_end),
        });
        if closing_end >= bytes.len() {
            return;
        }
        code_start = closing_end;
    }
}

/// Returns the net `(`+`[`+`{` minus `)`+`]`+`}` count on a single line,
/// ignoring delimiters inside string literals and trailing `#` comments.
/// Used to tell when a statement continues on the next line.
fn line_bracket_delta(line: &str) -> i32 {
    let code_delta = |code: &str| -> i32 {
        code.bytes()
            .map(|byte| match byte {
                b'(' | b'[' | b'{' => 1,
                b')' | b']' | b'}' => -1,
                _ => 0,
            })
            .sum()
    };
    if !has_string_or_comment(line) {
        return code_delta(line);
    }
    let mut depth = 0;
    for_each_line_piece(line, |piece| {
        if let LinePiece::Code(code) = piece {
            depth += code_delta(code);
        }
    });
    depth
}

/// Returns true if `line` ends with a backslash continuation. Trailing
/// whitespace after the backslash is allowed; a backslash inside a comment
/// doesn't count. Godot wants the line break right after the backslash, so
/// one before a comment (`x + \ # note`) is not a continuation either.
fn ends_with_backslash_continuation(line: &str) -> bool {
    // Checking the end first saves reading the many lines without one.
    line.trim_end().ends_with('\\') && strip_trailing_comment(line).trim_end().ends_with('\\')
}

/// Strip the trailing `# ...` comment from a line, respecting strings.
fn strip_trailing_comment(line: &str) -> &str {
    if !line.contains('#') {
        return line;
    }
    let mut code_length = line.len();
    for_each_line_piece(line, |piece| {
        if let LinePiece::Comment(comment) = piece {
            code_length = line.len() - comment.len();
        }
    });
    line.get(..code_length).unwrap_or(line)
}

/// True when `c` can be part of a GDScript name. Godot names take Unicode
/// letters and combining marks (`cafe\u{301}`), and GDScript has no non-ASCII
/// operators, so any non-ASCII character that isn't a space counts.
fn is_identifier_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || (!c.is_ascii() && !c.is_whitespace())
}

/// True when the `length` bytes of `code` at `index` are a keyword used as
/// one: a whole word, and not a member or node name (no `.`, `$` or `%`
/// right before it), so `return` in `x.return` or `return_value` is not.
fn is_keyword_at(code: &str, index: usize, length: usize) -> bool {
    let is_word_start =
        !code[..index].ends_with(|c: char| is_identifier_char(c) || matches!(c, '.' | '$' | '%'));
    let is_word_end = !code[index + length..].starts_with(is_identifier_char);
    is_word_start && is_word_end
}

/// The code of line `index` that is not inside a string spanning lines: the
/// whole line normally, the part after the literal's end on its closing
/// line, and nothing on the lines in between.
fn code_outside_spanning_strings<'a>(
    file: &'a ScriptFile,
    string_line_mask: &StringLines,
    index: usize,
) -> &'a str {
    let line = file.lines[index].as_str();
    if !string_line_mask.is_interior(index) {
        return line;
    }
    string_line_mask
        .literal_end(index)
        .and_then(|end| line.get(end..))
        .unwrap_or("")
}

/// True when trimmed `line` is a branch header starting with `keyword`
/// (`if` or `elif`), as in `if x:` or `elif(x):`.
fn is_branch_header(line: &str, keyword: &str) -> bool {
    line.strip_prefix(keyword)
        .is_some_and(|rest| rest.starts_with([' ', '\t', '(']))
}

/// True when trimmed `line` is an `else` header: `else:` or `else :`.
fn is_else_header(line: &str) -> bool {
    line.strip_prefix("else")
        .is_some_and(|rest| rest.trim_start().starts_with(':'))
}

/// True when the trimmed line is a `return` statement (`return`, `return x`,
/// `return(x)`, `return;`, `return-1`), not an identifier that starts with
/// `return` (`returned = true`, `return_value.clear()`).
fn is_return_statement(trimmed: &str) -> bool {
    trimmed
        .strip_prefix("return")
        .is_some_and(|rest| !rest.starts_with(is_identifier_char))
}

// ---------------------------------------------------------------------------
// Existing rules
// ---------------------------------------------------------------------------

/// Check that no function body exceeds the maximum length.
pub fn check_max_function_length(
    file: &ScriptFile,
    config: &Config,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for_each_member(&file.members, |member| {
        if let ClassMember::Function {
            name,
            body_line_count,
            span,
            ..
        } = member
        {
            if *body_line_count > config.max_function_length {
                diagnostics.push(Diagnostic::warning(
                    "quality/max-function-length",
                    format!(
                        "function '{}' is {} lines long (max {})",
                        name, body_line_count, config.max_function_length
                    ),
                    *span,
                    &file.path,
                ));
            }
        }
    });
}

/// Check that no file exceeds the maximum length.
pub fn check_max_file_length(
    file: &ScriptFile,
    config: &Config,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let line_count = file.lines.len();
    if line_count > config.max_file_length {
        diagnostics.push(Diagnostic::warning(
            "quality/max-file-length",
            format!(
                "file is {} lines long (max {})",
                line_count, config.max_file_length
            ),
            Span::new(1, 1, 0, 0),
            &file.path,
        ));
    }
}

/// Check that no function has too many parameters.
pub fn check_max_parameters(file: &ScriptFile, config: &Config, diagnostics: &mut Vec<Diagnostic>) {
    for_each_member(&file.members, |member| {
        if let ClassMember::Function {
            name,
            parameters,
            span,
            ..
        } = member
        {
            if parameters.len() > config.max_parameters {
                diagnostics.push(Diagnostic::warning(
                    "quality/max-parameters",
                    format!(
                        "function '{}' has {} parameters (max {})",
                        name,
                        parameters.len(),
                        config.max_parameters
                    ),
                    *span,
                    &file.path,
                ));
            }
        }
    });
}

// ---------------------------------------------------------------------------
// New rule 1: unnecessary-pass
// ---------------------------------------------------------------------------

/// Warn about `pass` statements that aren't the only statement in a block.
/// A lone `pass` in an otherwise-empty function is fine; `pass` alongside
/// other statements is unnecessary.
pub fn check_unnecessary_pass_in_functions(file: &ScriptFile, diagnostics: &mut Vec<Diagnostic>) {
    check_unnecessary_pass_fns_recursive(&file.members, file, diagnostics);
}

/// True if the `pass` statement at `code_lines[pass_pos]` has a sibling
/// statement at the same indentation within the same enclosing block,
/// meaning the `pass` is genuinely redundant. A `pass` that is alone in its
/// block (a `match` arm body, an otherwise-empty `if`/`for`/`while` body) is
/// required and must not be flagged.
///
/// `code_lines` is the function's non-blank, non-comment lines as
/// `(line_index, text)` pairs.
fn pass_has_sibling_statement(code_lines: &[(usize, &str)], pass_pos: usize) -> bool {
    let pass_indent = indent_level(code_lines[pass_pos].1);

    // Scan outward in both directions. A line at the same indent inside the
    // same block is a sibling; a line at a lower indent closes the block.
    let mut has_sibling = false;
    // Backward.
    for (_, line) in code_lines[..pass_pos].iter().rev() {
        let indent = indent_level(line);
        if indent < pass_indent {
            break;
        }
        if indent == pass_indent {
            has_sibling = true;
            break;
        }
    }
    if has_sibling {
        return true;
    }
    // Forward.
    for (_, line) in &code_lines[pass_pos + 1..] {
        let indent = indent_level(line);
        if indent < pass_indent {
            break;
        }
        if indent == pass_indent {
            return true;
        }
    }
    false
}

fn check_unnecessary_pass_fns_recursive(
    members: &[ClassMember],
    file: &ScriptFile,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for_each_member(members, |member| {
        let ClassMember::Function {
            body_line_count,
            span,
            ..
        } = member
        else {
            return;
        };
        let range = function_body_range(span, *body_line_count, file.lines.len());

        // Body lines that carry code, as (absolute line index, text).
        let code_lines: Vec<(usize, &str)> = file.lines[range.clone()]
            .iter()
            .enumerate()
            .filter(|(_, l)| !is_comment_or_blank(l.trim()))
            .map(|(i, l)| (range.start + i, l.as_str()))
            .collect();

        if code_lines.len() <= 1 {
            // Only `pass` or empty, that's fine.
            return;
        }

        // Flag a `pass` only when it has a sibling statement in the same
        // block. A `pass` that is the *sole* statement of its block (a
        // `match` arm, an `if`/`else`/`for`/`while` body that would
        // otherwise be empty) is required, not redundant.
        for (pos, (line_idx, line)) in code_lines.iter().enumerate() {
            if line.trim() == "pass" && pass_has_sibling_statement(&code_lines, pos) {
                diagnostics.push(Diagnostic::warning(
                    "quality/unnecessary-pass",
                    "unnecessary 'pass' statement".to_string(),
                    Span::new(line_idx + 1, 1, 0, 0),
                    &file.path,
                ));
            }
        }
    });
}

// ---------------------------------------------------------------------------
// New rule 2: no-debug-print
// ---------------------------------------------------------------------------

const DEBUG_PRINT_FUNCTIONS: &[&str] = &[
    "print",
    "prints",
    "printt",
    "printraw",
    "print_debug",
    "print_verbose",
    "print_rich",
    "printerr",
];

/// Warn about debug print statements that should be removed before shipping.
pub fn check_no_debug_print(
    tokens: &[Token],
    file: &ScriptFile,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (i, token) in tokens.iter().enumerate() {
        if let TokenKind::Identifier(name) = &token.kind {
            if DEBUG_PRINT_FUNCTIONS.contains(&name.as_str()) {
                // Check next non-newline token is '('
                if let Some(next) = tokens.get(i + 1) {
                    if next.kind == TokenKind::LeftParen {
                        diagnostics.push(Diagnostic::warning(
                            "quality/no-debug-print",
                            format!("debug '{}()' call found", name),
                            token.span,
                            &file.path,
                        ));
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// New rule 3: self-comparison
// ---------------------------------------------------------------------------

const COMPARISON_OPS: &[TokenKind] = &[
    TokenKind::Equal,
    TokenKind::NotEqual,
    TokenKind::Less,
    TokenKind::LessEqual,
    TokenKind::Greater,
    TokenKind::GreaterEqual,
];

/// Warn when comparing a value with itself (e.g. `x == x`, `a.b == a.b`).
///
/// Compares the full dotted chain on each side of the operator rather than a
/// single-token window, so a shared trailing segment doesn't produce a false
/// positive:
///
/// - `stored_goods.label == label` — LHS is `stored_goods.label`, RHS is
///   `label`: NOT a self-comparison
/// - `position == whatever.position` — different chains: NOT a self-comparison
/// - `x == x.foo` — RHS chain absorbs `.foo`: NOT a self-comparison
///
/// while still catching `x == x` and `obj.foo == obj.foo`. Each side must be
/// a standalone operand: the left chain must sit on an operand boundary and
/// the right chain must not continue into a larger expression (`x == x + 1`),
/// so partial matches inside arithmetic don't trigger.
pub fn check_self_comparison(
    tokens: &[Token],
    file: &ScriptFile,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (i, op) in tokens.iter().enumerate() {
        if !COMPARISON_OPS.contains(&op.kind) {
            continue;
        }
        let lhs = lhs_chain_ending_at(tokens, i);
        if lhs.is_empty() {
            continue;
        }
        // The LHS chain must be the whole left operand. A dotted chain of N
        // segments spans 2*N - 1 tokens (N identifiers, N-1 dots) ending at
        // `i - 1`, so it starts at `i - (2*N - 1)`. If the token before that
        // start continues an expression (`foo + x == x`), skip.
        let lhs_start = i - (2 * lhs.len() - 1);
        let before_lhs = lhs_start.checked_sub(1).map(|k| &tokens[k]);
        if !is_operand_start_boundary(before_lhs) {
            continue;
        }
        let (rhs, end) = rhs_chain_starting_at(tokens, i + 1);
        if rhs.is_empty() {
            continue;
        }
        // The RHS chain must be the whole right operand; if what follows it
        // continues the expression (`x == x + 1`, `x == x.foo()`), it isn't a
        // comparison between two identical operands.
        if !is_comparison_operand_end(tokens.get(end)) {
            continue;
        }
        if lhs == rhs {
            diagnostics.push(Diagnostic::warning(
                "quality/self-comparison",
                format!("comparing '{}' with itself", lhs.join(".")),
                op.span,
                &file.path,
            ));
        }
    }
}

/// True when the token before a left operand marks a clean operand boundary,
/// so the chain that follows is a standalone operand rather than the tail of a
/// larger sub-expression (e.g. the `x` in `foo + x`). `None` (start of stream)
/// counts as a boundary.
fn is_operand_start_boundary(token: Option<&Token>) -> bool {
    match token {
        None => true,
        Some(t) => matches!(
            t.kind,
            TokenKind::Newline
                | TokenKind::Semicolon
                | TokenKind::Colon
                | TokenKind::Comma
                | TokenKind::LeftParen
                | TokenKind::LeftBracket
                | TokenKind::LeftBrace
                | TokenKind::If
                | TokenKind::Elif
                | TokenKind::While
                | TokenKind::When
                | TokenKind::Return
                | TokenKind::Assert
                | TokenKind::Await
                | TokenKind::And
                | TokenKind::Or
                | TokenKind::Not
                | TokenKind::In
                | TokenKind::Assign
        ),
    }
}

/// True when the token after a right operand ends that operand in a comparison
/// context: a statement terminator, a boolean continuation (`and`/`or`), or a
/// `:`/closing delimiter. Used so `x == x + 1` and `x == x.foo()` aren't read
/// as comparing a value with itself.
fn is_comparison_operand_end(token: Option<&Token>) -> bool {
    match token {
        None => true,
        Some(t) => matches!(
            t.kind,
            TokenKind::Newline
                | TokenKind::Semicolon
                | TokenKind::Eof
                | TokenKind::Colon
                | TokenKind::Comma
                | TokenKind::RightParen
                | TokenKind::RightBracket
                | TokenKind::RightBrace
                | TokenKind::And
                | TokenKind::Or
                | TokenKind::Comment(_)
                | TokenKind::DocComment(_)
        ),
    }
}

// ---------------------------------------------------------------------------
// New rule 4: no-self-assign
// ---------------------------------------------------------------------------

/// Warn when an assignment's left and right sides reference the exact same
/// dotted path with nothing else on the right (e.g. `x = x`, `a.b.c = a.b.c`).
///
/// Uses full-chain comparison instead of a 3-token sliding window so it
/// correctly distinguishes:
///
/// - `moon.size = size` — LHS is `moon.size`, RHS is `size`: NOT a self-assign
/// - `x = x.y` — LHS is `x`, RHS is `x.y`: NOT a self-assign
/// - `x = x + 1` — RHS continues into an expression: NOT a self-assign
///
/// while still catching the typo cases `x = x` and `obj.foo = obj.foo`.
pub fn check_no_self_assign(
    tokens: &[Token],
    file: &ScriptFile,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for (i, op) in tokens.iter().enumerate() {
        if op.kind != TokenKind::Assign {
            continue;
        }
        let lhs = lhs_chain_ending_at(tokens, i);
        if lhs.is_empty() {
            continue;
        }
        let (rhs, end) = rhs_chain_starting_at(tokens, i + 1);
        if rhs.is_empty() {
            continue;
        }
        // The RHS chain must be the full RHS. If anything follows it that
        // isn't a statement-terminating token, the chain is part of a larger
        // expression (e.g. `x = x + 1`, `x = x or fallback`) and the
        // assignment isn't a no-op self-assign.
        if !is_statement_terminator(tokens.get(end)) {
            continue;
        }
        if lhs == rhs {
            diagnostics.push(Diagnostic::warning(
                "quality/no-self-assign",
                format!("self-assignment of '{}'", lhs.join(".")),
                op.span,
                &file.path,
            ));
        }
    }
}

/// Walk backwards from the token at position `before` (exclusive) to build
/// the dotted LHS chain `(self|super|Identifier) (Dot Identifier)*`.
/// Returns an empty vec when the LHS isn't a pure identifier chain (e.g.
/// it ends in `]`, `)`, or starts with `:` as in `:=`).
fn lhs_chain_ending_at(tokens: &[Token], before: usize) -> Vec<String> {
    let mut chain: Vec<String> = Vec::new();
    if before == 0 {
        return chain;
    }
    let mut i = before - 1;
    loop {
        match &tokens[i].kind {
            TokenKind::Identifier(name) => {
                chain.push(name.clone());
                if i >= 2
                    && tokens[i - 1].kind == TokenKind::Dot
                    && matches!(
                        tokens[i - 2].kind,
                        TokenKind::Identifier(_) | TokenKind::Self_ | TokenKind::Super
                    )
                {
                    i -= 2;
                    continue;
                }
                break;
            }
            TokenKind::Self_ => {
                chain.push("self".to_string());
                break;
            }
            TokenKind::Super => {
                chain.push("super".to_string());
                break;
            }
            _ => return Vec::new(),
        }
    }
    chain.reverse();
    chain
}

/// Walk forwards from `start` to build the dotted RHS chain. First segment
/// may be `self`, `super`, or an identifier; subsequent segments must be
/// identifiers. Returns the chain and the index of the first token AFTER
/// the chain (so the caller can check what follows it).
fn rhs_chain_starting_at(tokens: &[Token], start: usize) -> (Vec<String>, usize) {
    let mut chain: Vec<String> = Vec::new();
    let mut i = start;
    // First segment can be a keyword (`self`/`super`) or an identifier.
    match tokens.get(i).map(|t| &t.kind) {
        Some(TokenKind::Identifier(name)) => {
            chain.push(name.clone());
        }
        Some(TokenKind::Self_) => {
            chain.push("self".to_string());
        }
        Some(TokenKind::Super) => {
            chain.push("super".to_string());
        }
        _ => return (chain, i),
    }
    // Subsequent segments must be `.identifier`.
    while i + 2 < tokens.len()
        && tokens[i + 1].kind == TokenKind::Dot
        && matches!(tokens[i + 2].kind, TokenKind::Identifier(_))
    {
        if let TokenKind::Identifier(name) = &tokens[i + 2].kind {
            chain.push(name.clone());
        }
        i += 2;
    }
    (chain, i + 1)
}

/// True when the token (or the absence of it at end-of-stream) marks the end
/// of a statement / expression context. Used to decide whether an RHS
/// identifier chain stands alone or is the start of a larger expression.
fn is_statement_terminator(token: Option<&Token>) -> bool {
    match token {
        None => true,
        Some(t) => matches!(
            t.kind,
            TokenKind::Newline
                | TokenKind::Semicolon
                | TokenKind::Eof
                | TokenKind::RightParen
                | TokenKind::RightBracket
                | TokenKind::RightBrace
                | TokenKind::Comma
                | TokenKind::Comment(_)
                | TokenKind::DocComment(_)
        ),
    }
}

// ---------------------------------------------------------------------------
// New rule 5: duplicate-dict-key
// ---------------------------------------------------------------------------

/// State for scanning one open dictionary literal. A nested literal pushes a
/// new frame, so each dictionary's keys are tracked independently of the dict
/// that contains it.
struct DictKeyFrame {
    /// Keys already seen at this literal's top level, with the span of the
    /// first occurrence (used to point back at it in the message).
    keys: HashMap<String, Span>,
    /// Whether the next tokens belong to a key (`true`) or a value (`false`).
    expect_key: bool,
    /// The current key expression, accumulated token-by-token until the `:`
    /// that ends it, so multi-token keys compare as a single unit.
    key_parts: Vec<String>,
    /// Span of the first token of the current key, for the diagnostic.
    key_span: Option<Span>,
    /// Depth of unclosed `(`/`[` within the entry currently being scanned.
    /// A comma or colon inside a nested call or subscript (e.g. the `,` in
    /// the key expression `Vector2i(1, 0)`) belongs to that expression, not
    /// to this dictionary literal, and must not be mistaken for the
    /// entry-separating comma or the key-ending colon.
    depth: u32,
}

impl DictKeyFrame {
    fn new() -> Self {
        DictKeyFrame {
            keys: HashMap::new(),
            expect_key: true,
            key_parts: Vec::new(),
            key_span: None,
            depth: 0,
        }
    }
}

/// Warn about duplicate keys in dictionary literals.
///
/// A key is compared as a whole expression, not a single token: a dotted key
/// such as `Enums.E.KEY_A` is treated as one key, so two distinct enum entries
/// (`Enums.E.KEY_A` and `Enums.E.KEY_B`) are not mistaken for a duplicate `.`
/// (issue #8). String-literal segments are normalized to their semantic value
/// so `{"foo": 1, 'foo': 2}` is still caught regardless of quote style.
///
/// Nested dictionaries are checked too: each `{` opens a fresh frame, so a
/// duplicate inside a value dictionary (`{"outer": {"a": 1, "a": 2}}`) is
/// reported against that inner dictionary, independently of the outer keys.
pub fn check_duplicate_dict_key(
    tokens: &[Token],
    file: &ScriptFile,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // A stack of frames, one per currently-open `{`. Keys, commas, and colons
    // always apply to the innermost open dictionary (`stack.last_mut()`),
    // which routes each token to the right nesting level. Each frame also
    // tracks its own `(`/`[` depth so a comma or colon nested inside a call
    // or subscript within the current entry isn't mistaken for this
    // dictionary's entry separator or key/value separator.
    let mut stack: Vec<DictKeyFrame> = Vec::new();

    for token in tokens {
        match &token.kind {
            TokenKind::LeftBrace => stack.push(DictKeyFrame::new()),
            TokenKind::RightBrace => {
                stack.pop();
            }
            TokenKind::LeftParen | TokenKind::LeftBracket => {
                if let Some(frame) = stack.last_mut() {
                    frame.depth += 1;
                    push_key_part(frame, token);
                }
            }
            TokenKind::RightParen | TokenKind::RightBracket => {
                if let Some(frame) = stack.last_mut() {
                    frame.depth = frame.depth.saturating_sub(1);
                    push_key_part(frame, token);
                }
            }
            TokenKind::Colon => {
                if let Some(frame) = stack.last_mut() {
                    if frame.expect_key && frame.depth == 0 {
                        // End of the key expression: record it as one unit.
                        if let Some(span) = frame.key_span.take() {
                            let key_text = frame.key_parts.join("");
                            if let Some(prev_span) = frame.keys.get(&key_text) {
                                diagnostics.push(Diagnostic::warning(
                                    "quality/duplicate-dict-key",
                                    format!(
                                        "duplicate dictionary key '{}' (first seen at line {})",
                                        key_text, prev_span.line
                                    ),
                                    span,
                                    &file.path,
                                ));
                            } else {
                                frame.keys.insert(key_text, span);
                            }
                        }
                        frame.key_parts.clear();
                        frame.expect_key = false;
                    } else {
                        // A `:` nested inside a call/subscript in the key
                        // expression is part of that expression, not the
                        // entry's key/value separator.
                        push_key_part(frame, token);
                    }
                }
            }
            TokenKind::Comma => {
                if let Some(frame) = stack.last_mut() {
                    if frame.depth == 0 {
                        // Next entry begins; drop any partially seen value.
                        frame.key_parts.clear();
                        frame.key_span = None;
                        frame.expect_key = true;
                    } else {
                        // An argument separator inside a nested call/subscript
                        // (e.g. `Vector2i(1, 0)`), not the entry separator.
                        push_key_part(frame, token);
                    }
                }
            }
            TokenKind::Newline | TokenKind::Comment(_) | TokenKind::DocComment(_) => {}
            _ => {
                if let Some(frame) = stack.last_mut() {
                    push_key_part(frame, token);
                }
            }
        }
    }
}

/// Append `token` to the key expression currently being accumulated, if the
/// frame is in key position. String tokens use their semantic value so
/// `{"foo": 1, 'foo': 2}` is still caught regardless of quote style.
fn push_key_part(frame: &mut DictKeyFrame, token: &Token) {
    if !frame.expect_key {
        return;
    }
    let part = match &token.kind {
        TokenKind::String(info) => info.value.clone(),
        _ => token.text.clone(),
    };
    if frame.key_span.is_none() {
        frame.key_span = Some(token.span);
    }
    frame.key_parts.push(part);
}

// ---------------------------------------------------------------------------
// New rule 6: duplicated-load
// ---------------------------------------------------------------------------

/// Warn when the same path is passed to `load()` or `preload()` multiple times.
pub fn check_duplicated_load(
    tokens: &[Token],
    file: &ScriptFile,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut seen: HashMap<String, Span> = HashMap::new();

    for i in 0..tokens.len().saturating_sub(2) {
        let is_load = match &tokens[i].kind {
            TokenKind::Identifier(name) if name == "load" => true,
            TokenKind::Preload => true,
            _ => false,
        };

        if !is_load {
            continue;
        }

        // Expect ( "path" )
        if tokens.get(i + 1).map(|t| &t.kind) != Some(&TokenKind::LeftParen) {
            continue;
        }

        if let Some(path_token) = tokens.get(i + 2) {
            if let TokenKind::String(info) = &path_token.kind {
                let path = &info.value;
                if let Some(prev_span) = seen.get(path) {
                    diagnostics.push(Diagnostic::warning(
                        "quality/duplicated-load",
                        format!(
                            "duplicated load of '{}' (first seen at line {})",
                            path, prev_span.line
                        ),
                        tokens[i].span,
                        &file.path,
                    ));
                } else {
                    seen.insert(path.clone(), tokens[i].span);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// New rule 7: type-hint
// ---------------------------------------------------------------------------

/// Suggest adding type hints for variables and parameters that lack them.
pub fn check_type_hint(file: &ScriptFile, diagnostics: &mut Vec<Diagnostic>) {
    check_type_hint_recursive(&file.members, &file.path, diagnostics);
}

fn check_type_hint_recursive(
    members: &[ClassMember],
    file_path: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for_each_member(members, |member| match member {
        ClassMember::Variable {
            name,
            type_hint,
            name_span,
            ..
        }
        | ClassMember::StaticVariable {
            name,
            type_hint,
            name_span,
            ..
        } => {
            if type_hint.is_none() {
                diagnostics.push(Diagnostic::warning(
                    "quality/type-hint",
                    format!("variable '{}' has no type hint", name),
                    *name_span,
                    file_path,
                ));
            }
        }
        ClassMember::Function {
            name,
            parameters,
            return_type,
            name_span,
            ..
        } => {
            if return_type.is_none() {
                diagnostics.push(Diagnostic::warning(
                    "quality/type-hint",
                    format!("function '{}' has no return type hint", name),
                    *name_span,
                    file_path,
                ));
            }
            for param in parameters {
                if param.type_hint.is_none() {
                    diagnostics.push(Diagnostic::warning(
                        "quality/type-hint",
                        format!(
                            "parameter '{}' in function '{}' has no type hint",
                            param.name, name
                        ),
                        param.span,
                        file_path,
                    ));
                }
            }
        }
        _ => {}
    });
}

// ---------------------------------------------------------------------------
// New rule 8: empty-function
// ---------------------------------------------------------------------------

/// Warn about functions that are empty or contain only `pass`.
pub fn check_empty_function(file: &ScriptFile, diagnostics: &mut Vec<Diagnostic>) {
    check_empty_function_recursive(&file.members, file, diagnostics);
}

fn check_empty_function_recursive(
    members: &[ClassMember],
    file: &ScriptFile,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for_each_member(members, |member| {
        let ClassMember::Function {
            name,
            body_line_count,
            span,
            ..
        } = member
        else {
            return;
        };
        if *body_line_count == 0 {
            diagnostics.push(Diagnostic::warning(
                "quality/empty-function",
                format!("function '{}' is empty", name),
                *span,
                &file.path,
            ));
            return;
        }

        let range = function_body_range(span, *body_line_count, file.lines.len());
        let all_pass_or_blank = file.lines[range].iter().all(|l| {
            let t = l.trim();
            t.is_empty() || t == "pass" || t.starts_with('#')
        });

        if all_pass_or_blank {
            diagnostics.push(Diagnostic::warning(
                "quality/empty-function",
                format!("function '{}' only contains 'pass'", name),
                *span,
                &file.path,
            ));
        }
    });
}

// ---------------------------------------------------------------------------
// New rule 9: max-class-variables
// ---------------------------------------------------------------------------

/// Warn when a class has too many member variables.
pub fn check_max_class_variables(
    file: &ScriptFile,
    config: &Config,
    diagnostics: &mut Vec<Diagnostic>,
) {
    check_max_class_vars_impl(&file.members, &file.path, config, diagnostics, true);
}

fn check_max_class_vars_impl(
    members: &[ClassMember],
    file_path: &str,
    config: &Config,
    diagnostics: &mut Vec<Diagnostic>,
    is_top_level: bool,
) {
    let count = members
        .iter()
        .filter(|m| {
            matches!(
                m,
                ClassMember::Variable { .. } | ClassMember::StaticVariable { .. }
            )
        })
        .count();

    if count > config.max_class_variables {
        let span = if is_top_level {
            Span::new(1, 1, 0, 0)
        } else {
            members
                .first()
                .map(|m| m.span())
                .unwrap_or(Span::new(1, 1, 0, 0))
        };
        diagnostics.push(Diagnostic::warning(
            "quality/max-class-variables",
            format!(
                "class has {} variables (max {})",
                count, config.max_class_variables
            ),
            span,
            file_path,
        ));
    }

    for member in members {
        if let ClassMember::InnerClass { members: inner, .. } = member {
            check_max_class_vars_impl(inner, file_path, config, diagnostics, false);
        }
    }
}

// ---------------------------------------------------------------------------
// New rule 10: max-public-methods
// ---------------------------------------------------------------------------

/// Warn when a class has too many public methods.
pub fn check_max_public_methods(
    file: &ScriptFile,
    config: &Config,
    diagnostics: &mut Vec<Diagnostic>,
) {
    check_max_public_methods_impl(&file.members, &file.path, config, diagnostics, true);
}

fn check_max_public_methods_impl(
    members: &[ClassMember],
    file_path: &str,
    config: &Config,
    diagnostics: &mut Vec<Diagnostic>,
    is_top_level: bool,
) {
    let count = members
        .iter()
        .filter(|m| {
            if let ClassMember::Function { name, .. } = m {
                !name.starts_with('_')
            } else {
                false
            }
        })
        .count();

    if count > config.max_public_methods {
        let span = if is_top_level {
            Span::new(1, 1, 0, 0)
        } else {
            members
                .first()
                .map(|m| m.span())
                .unwrap_or(Span::new(1, 1, 0, 0))
        };
        diagnostics.push(Diagnostic::warning(
            "quality/max-public-methods",
            format!(
                "class has {} public methods (max {})",
                count, config.max_public_methods
            ),
            span,
            file_path,
        ));
    }

    for member in members {
        if let ClassMember::InnerClass { members: inner, .. } = member {
            check_max_public_methods_impl(inner, file_path, config, diagnostics, false);
        }
    }
}

// ---------------------------------------------------------------------------
// New rule 11: max-inner-classes
// ---------------------------------------------------------------------------

/// Warn when a class has too many inner classes.
pub fn check_max_inner_classes(
    file: &ScriptFile,
    config: &Config,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Every inner class in the file counts, nested ones included: the limit
    // is per file, and a class nested in another is still one more class.
    let mut count = 0;
    for_each_member(&file.members, |member| {
        if matches!(member, ClassMember::InnerClass { .. }) {
            count += 1;
        }
    });

    if count > config.max_inner_classes {
        diagnostics.push(Diagnostic::warning(
            "quality/max-inner-classes",
            format!(
                "file has {} inner classes (max {})",
                count, config.max_inner_classes
            ),
            Span::new(1, 1, 0, 0),
            &file.path,
        ));
    }
}

// ---------------------------------------------------------------------------
// New rule 12: no-else-return
// ---------------------------------------------------------------------------

/// Warn when an `else` or `elif` follows branches that all end with
/// `return`: the `return` already exits, so the code can be dedented.
///
/// Every earlier branch of the chain must return, not just the one right
/// above. After `if a: foo()` / `elif b: return 1`, dropping the `else`
/// would run its body after `foo()` too. A branch returns when its last
/// statement is a `return` at the branch's own indent (a nested `return`
/// can fall through), or when it is a one-line `if x: return y`. A line like
/// `print(1); return 2` carries a `return` too.
///
/// `string_line_mask` marks the lines inside strings that span lines, so a
/// `return` or `else` in string data is not read as code.
///
/// One pass over the statements: each open `if` chain sits on a stack by
/// indent and remembers whether all its branches so far returned. The last
/// statement of a branch is the statement right before the chain's next
/// `elif` / `else`, so nothing is ever walked twice.
pub fn check_no_else_return(
    string_line_mask: &StringLines,
    file: &ScriptFile,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Most files have no `else` at all; finding none is cheaper than
    // splitting the file into statements.
    if !file.lines.iter().any(|line| line.contains("el")) {
        return;
    }
    let mut open_chains: Vec<OpenChain> = Vec::new();
    let mut previous: Option<StatementSummary> = None;
    for statement in statements(file, string_line_mask) {
        let indent = statement.indent;
        if let Some(chain) = open_chains.last_mut() {
            if chain.body_indent.is_none() && indent > chain.indent {
                chain.body_indent = Some(indent);
            }
        }
        while open_chains
            .last()
            .is_some_and(|chain| chain.indent > indent)
        {
            open_chains.pop();
        }
        let kind = if is_branch_header(statement.text, "elif") {
            Some("elif")
        } else if is_else_header(statement.text) {
            Some("else")
        } else {
            None
        };
        let chain_at_indent = open_chains
            .last_mut()
            .filter(|chain| chain.indent == indent);
        match (kind, chain_at_indent) {
            (Some(kind), Some(chain)) => {
                let branch_returns = chain.inline_body_returns.unwrap_or_else(|| {
                    previous.is_some_and(|last| {
                        last.is_return && Some(last.indent) == chain.body_indent
                    })
                });
                if chain.all_branches_return && branch_returns {
                    diagnostics.push(Diagnostic::warning(
                        "quality/no-else-return",
                        format!("unnecessary '{}' after 'return'", kind),
                        Span::new(statement.line + 1, 1, 0, 0),
                        &file.path,
                    ));
                }
                if kind == "else" {
                    open_chains.pop();
                } else {
                    chain.all_branches_return &= branch_returns;
                    chain.body_indent = None;
                    chain.inline_body_returns =
                        inline_branch_body(statement.text).map(has_return_statement);
                }
            }
            (_, chain_at_indent) => {
                if chain_at_indent.is_some() {
                    // A sibling statement ends the chain at this indent.
                    open_chains.pop();
                }
                if is_branch_header(statement.text, "if") {
                    open_chains.push(OpenChain {
                        indent,
                        all_branches_return: true,
                        body_indent: None,
                        inline_body_returns: inline_branch_body(statement.text)
                            .map(has_return_statement),
                    });
                }
            }
        }
        previous = Some(StatementSummary {
            indent,
            is_return: has_return_statement(statement.text),
        });
    }
}

/// An `if` chain whose next `elif` / `else` may still come, for
/// [`check_no_else_return`].
struct OpenChain {
    /// Indent of the `if` / `elif` / `else` headers.
    indent: usize,
    /// Whether every branch before the current one ended with a `return`.
    all_branches_return: bool,
    /// Indent of the current branch's statements, known after the first one.
    body_indent: Option<usize>,
    /// For a one-line branch (`elif x: return 1`), whether its statement is a
    /// `return`. `None` when the body is on the lines below.
    inline_body_returns: Option<bool>,
}

/// What [`check_no_else_return`] keeps of the statement before the current one.
#[derive(Clone, Copy)]
struct StatementSummary {
    indent: usize,
    is_return: bool,
}

/// A logical statement: its first line, which carries the keyword.
struct Statement<'a> {
    /// 0-indexed line of the statement's first line.
    line: usize,
    indent: usize,
    /// The first line, trimmed.
    text: &'a str,
}

/// The statements of `file` in order, skipping blank and comment lines. A
/// statement continues over the next lines while a bracket is open, a line
/// ends with a backslash, or a string spans lines, so `return max(` / `1,` /
/// `)` is one statement starting at the `return`.
///
/// A multi-line lambda is the exception: inside its brackets, the lines of
/// its body (`connect(func():` / `if a:` / `return` / `)`) are statements of
/// their own, as Godot reads them.
fn statements<'a>(
    file: &'a ScriptFile,
    string_line_mask: &'a StringLines,
) -> impl Iterator<Item = Statement<'a>> + 'a {
    let mut open_brackets = 0i32;
    let mut continues_previous = false;
    // Lambda bodies open inside brackets, innermost last.
    let mut lambda_bodies: Vec<LambdaBody> = Vec::new();
    file.lines
        .iter()
        .enumerate()
        .filter_map(move |(index, line)| {
            let is_interior = string_line_mask.is_interior(index);
            // Bracket balance of the line's code, leaving out string data.
            let code = code_outside_spanning_strings(file, string_line_mask, index);
            let trimmed = line.trim();
            if !is_interior && is_comment_or_blank(trimmed) {
                return None;
            }
            let indent = indent_level(line);
            if !is_interior {
                // A line back at the header's indent (the closing `)`) or
                // shallower has left the body.
                while lambda_bodies
                    .last()
                    .is_some_and(|body| indent <= body.header_indent)
                {
                    lambda_bodies.pop();
                }
            }
            let is_in_lambda_body = lambda_bodies
                .last()
                .is_some_and(|body| body.open_brackets == open_brackets);
            let starts_statement =
                !is_interior && !continues_previous && (open_brackets == 0 || is_in_lambda_body);
            open_brackets = (open_brackets + line_bracket_delta(code)).max(0);
            continues_previous = ends_with_backslash_continuation(code);
            lambda_bodies.retain(|body| body.open_brackets <= open_brackets);
            // A line inside brackets that ends with a colon opens a block:
            // a lambda's body, or a block nested in one.
            if open_brackets > 0 && strip_trailing_comment(code).trim_end().ends_with(':') {
                lambda_bodies.push(LambdaBody {
                    header_indent: indent,
                    open_brackets,
                });
            }
            starts_statement.then_some(Statement {
                line: index,
                indent,
                text: trimmed,
            })
        })
}

/// A block opened inside brackets, for [`statements`].
struct LambdaBody {
    /// Indent of the line whose colon opened it.
    header_indent: usize,
    /// Brackets open at that colon; a body line at this depth is a statement.
    open_brackets: i32,
}

/// The statement after the colon of a one-line `if` / `elif` header:
/// `return 1` for `if x: return 1`. `None` for a header whose body is on the
/// lines below.
fn inline_branch_body(header: &str) -> Option<&str> {
    // Blanking keeps every byte offset, so the colon's offset in `code` is
    // its offset in `header`.
    let code = blank_strings_and_comments(header);
    let mut depth = 0i32;
    let colon = code.bytes().position(|byte| {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            _ => {}
        }
        byte == b':' && depth == 0
    })?;
    let body = header[colon + 1..].trim();
    (!body.is_empty() && !body.starts_with('#')).then_some(body)
}

/// True when one of the `;`-separated statements on trimmed line `text` is
/// a `return`: `return 1`, and also `print(1); return 2`. A line that
/// carries one always exits there, so `return 1; print(2)` counts too.
///
/// A `;` after a colon outside brackets doesn't count: the statements after
/// it may belong to a one-line block (`if y: print(1); return 1`, a lambda
/// `func(a): print(a); return a`) that can be skipped. That also passes over
/// a type hint (`var x: int = 1; return x`), which only misses a report.
fn has_return_statement(text: &str) -> bool {
    if is_return_statement(text) {
        return true;
    }
    if !text.contains(';') {
        return false;
    }
    // Blanking keeps every byte offset, so an offset in `code` is the same
    // offset in `text`.
    let code = blank_strings_and_comments(text);
    let mut depth = 0i32;
    let mut is_after_block_colon = false;
    code.bytes().enumerate().any(|(index, byte)| {
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b':' if depth == 0 => is_after_block_colon = true,
            b';' if depth == 0 && !is_after_block_colon => {
                return is_return_statement(text[index + 1..].trim_start())
            }
            _ => {}
        }
        false
    })
}

/// The number of `return` statements on `line`: a line that starts with
/// one, and also the ones after a `;` (`print(1); return 2`) or after the
/// colon of a one-line header (`1: return "a"`, `if x: return 1`).
/// `return` is a reserved word, so outside strings and comments every
/// `return` that is a whole word is a statement. One after a `func` on the
/// same line is a lambda's (`sort_custom(func(a, b): return a < b)`) and
/// returns from the lambda, so it doesn't count.
fn count_returns(line: &str) -> usize {
    if !line.contains("return") {
        return 0;
    }
    let code = blank_strings_and_comments(line);
    let lambda_start = code
        .match_indices("func")
        .find(|&(index, keyword)| is_keyword_at(&code, index, keyword.len()))
        .map_or(code.len(), |(index, _)| index);
    code[..lambda_start]
        .match_indices("return")
        .filter(|&(index, keyword)| is_keyword_at(&code, index, keyword.len()))
        .count()
}

// ---------------------------------------------------------------------------
// New rule 13: unreachable-code
// ---------------------------------------------------------------------------

/// Warn about code that appears after `return`, `break`, or `continue` at the
/// same indentation level within a block.
///
/// `string_line_mask` marks the lines inside strings that span lines: they
/// are string data, never a terminator or unreachable code, and only the
/// code after the literal's end on its closing line counts.
///
/// One pass over the lines. Each terminator waits on a stack by indent: a
/// deeper line leaves it waiting, a shallower one ends its block, and the
/// first statement at its own indent is unreachable. The terminator's own
/// continuation lines (`return max(` / `1,` / `)`) are part of it, and a
/// line that only closes brackets opened earlier (the `)` after a lambda's
/// `return`) is not a new statement.
pub fn check_unreachable_code(
    string_line_mask: &StringLines,
    file: &ScriptFile,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Indents of the terminators whose block is still open, innermost last.
    let mut waiting_indents: Vec<usize> = Vec::new();
    // Inside a terminator statement that spans lines: its bracket depth.
    let mut terminator_depth = 0i32;
    let mut is_in_terminator = false;
    let mut previous_continues = false;
    for (index, line) in file.lines.iter().enumerate() {
        let is_interior = string_line_mask.is_interior(index);
        let code = code_outside_spanning_strings(file, string_line_mask, index);
        if is_in_terminator {
            terminator_depth += line_bracket_delta(code);
            previous_continues = ends_with_backslash_continuation(code);
            is_in_terminator = terminator_depth > 0 || previous_continues;
            continue;
        }
        let continues_previous = previous_continues;
        previous_continues = ends_with_backslash_continuation(code);
        let trimmed = line.trim();
        if is_interior || continues_previous || is_comment_or_blank(trimmed) {
            continue;
        }

        let indent = indent_level(line);
        while waiting_indents
            .last()
            .is_some_and(|&waiting| waiting > indent)
        {
            waiting_indents.pop();
        }
        // A line that starts by closing brackets opened earlier continues
        // that statement (`)` after a lambda's `return`, `).filter(...)`):
        // it ends deeper blocks but is not a new statement.
        if trimmed.starts_with([')', ']', '}']) {
            continue;
        }
        if waiting_indents.last() == Some(&indent) {
            waiting_indents.pop();
            diagnostics.push(Diagnostic::warning(
                "quality/unreachable-code",
                "unreachable code".to_string(),
                Span::new(index + 1, 1, 0, 0),
                &file.path,
            ));
        }

        if is_return_statement(trimmed) || trimmed == "break" || trimmed == "continue" {
            waiting_indents.push(indent);
            terminator_depth = line_bracket_delta(line);
            is_in_terminator = terminator_depth > 0 || previous_continues;
        }
    }
}

// ---------------------------------------------------------------------------
// New rule 14: await-in-loop
// ---------------------------------------------------------------------------

/// Warn when `await` is used inside a `for` or `while` loop.
pub fn check_await_in_loop(file: &ScriptFile, diagnostics: &mut Vec<Diagnostic>) {
    // Track loop indent levels using a stack
    let mut loop_indents: Vec<usize> = Vec::new();

    for (i, line) in file.lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let line_indent = indent_level(line);

        // Pop loops that we've exited
        while let Some(&loop_indent) = loop_indents.last() {
            if line_indent <= loop_indent {
                loop_indents.pop();
            } else {
                break;
            }
        }

        // Detect loop starts
        if trimmed.starts_with("for ") || trimmed.starts_with("while ") || trimmed == "while true:"
        {
            loop_indents.push(line_indent);
            continue;
        }

        // Check for await inside a loop. Match against a string-blanked copy
        // so `var s = " await "` inside a loop doesn't false-positive.
        let code = blank_strings_and_comments(line);
        let code_trimmed = code.trim();
        if !loop_indents.is_empty()
            && (code_trimmed.starts_with("await ")
                || code_trimmed.contains(" await ")
                || code_trimmed.starts_with("await("))
        {
            diagnostics.push(Diagnostic::warning(
                "quality/await-in-loop",
                "'await' used inside a loop".to_string(),
                Span::new(i + 1, 1, 0, 0),
                &file.path,
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// New rule 15: allocation-in-loop
// ---------------------------------------------------------------------------

/// Warn about object allocations (`.new()`) inside loops.
pub fn check_allocation_in_loop(file: &ScriptFile, diagnostics: &mut Vec<Diagnostic>) {
    let mut loop_indents: Vec<usize> = Vec::new();

    for (i, line) in file.lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let line_indent = indent_level(line);

        while let Some(&loop_indent) = loop_indents.last() {
            if line_indent <= loop_indent {
                loop_indents.pop();
            } else {
                break;
            }
        }

        if trimmed.starts_with("for ") || trimmed.starts_with("while ") || trimmed == "while true:"
        {
            loop_indents.push(line_indent);
            continue;
        }

        // Match against a string-blanked copy so `.new(` inside a string
        // literal doesn't false-positive.
        if !loop_indents.is_empty() && blank_strings_and_comments(line).contains(".new(") {
            diagnostics.push(Diagnostic::warning(
                "quality/allocation-in-loop",
                "object allocation '.new()' inside a loop".to_string(),
                Span::new(i + 1, 1, 0, 0),
                &file.path,
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// New rule 16: process-get-node
// ---------------------------------------------------------------------------

/// Warn about node lookups (`get_node()`, `$`, `%`) in `_process` and
/// `_physics_process` methods. These should be cached in `_ready` or `@onready`.
///
/// `string_line_mask` marks the lines inside strings that span lines: their
/// string data is skipped, and on a literal's closing line only the code
/// after it is checked (`""" + str($Node)`).
pub fn check_process_get_node(
    string_line_mask: &StringLines,
    file: &ScriptFile,
    diagnostics: &mut Vec<Diagnostic>,
) {
    check_process_get_node_recursive(&file.members, file, string_line_mask, diagnostics);
}

fn check_process_get_node_recursive(
    members: &[ClassMember],
    file: &ScriptFile,
    string_line_mask: &StringLines,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for_each_member(members, |member| {
        let ClassMember::Function {
            name,
            body_line_count,
            span,
            ..
        } = member
        else {
            return;
        };
        if name != "_process" && name != "_physics_process" {
            return;
        }
        let range = function_body_range(span, *body_line_count, file.lines.len());
        for idx in range {
            let line = code_outside_spanning_strings(file, string_line_mask, idx);
            if line.is_empty() {
                continue;
            }
            // Match against a string-blanked copy so a `%d` placeholder or a
            // `$` inside a string literal isn't read as a node lookup. The
            // quotes stay, so `%"Health Bar"` is still one.
            let code = blank_strings_and_comments(line);
            let has_node_ref = calls_function(&code, "get_node")
                || calls_function(&code, "get_node_or_null")
                || code.contains('$')
                || has_unique_node_ref(&code);
            if has_node_ref {
                diagnostics.push(Diagnostic::warning(
                    "quality/process-get-node",
                    format!(
                        "node lookup in '{}()'; cache the node in '_ready' or use '@onready'",
                        name
                    ),
                    Span::new(idx + 1, 1, 0, 0),
                    &file.path,
                ));
            }
        }
    });
}

/// True when `code` calls the function `name`: `name(` with no identifier
/// character right before it, so `my_get_node(` is not `get_node(`. A method
/// call like `owner.get_node(` counts, and so does a space before the
/// parenthesis (`get_node ("A")`).
fn calls_function(code: &str, name: &str) -> bool {
    code.match_indices(name).any(|(index, _)| {
        let is_word_start = !code[..index].ends_with(is_identifier_char);
        is_word_start
            && code[index + name.len()..]
                .trim_start_matches([' ', '\t'])
                .starts_with('(')
    })
}

/// Check if a line contains a `%UniqueNode` reference, distinguishing it from
/// the modulo and format operator.
///
/// A unique-node reference is `%` immediately followed by a character that
/// starts a name or a quote (`%Player`, `%Ünique`, `%"Health Bar"`), with no
/// space. That alone still reads `"%s"%name` and `a %b` as lookups, so the
/// `%` must also not follow an operand: like Godot's parser, a `%` after a
/// value is the binary operator. Expects a line from
/// `blank_strings_and_comments`, so the quotes before an operator `%` are
/// still there. Only the line itself is seen, so a `%name` that continues the
/// previous line (`"%d" \` then `%count`) still reads as a lookup.
fn has_unique_node_ref(line: &str) -> bool {
    for (i, _) in line.match_indices('%') {
        // A digit after `%` is a number, so `%5` is a modulo, not a name.
        let starts_name = line[i + 1..].chars().next().is_some_and(|c| {
            (is_identifier_char(c) && !c.is_ascii_digit()) || c == '"' || c == '\''
        });
        if starts_name && !follows_operand(&line[..i]) {
            return true;
        }
    }
    false
}

/// True when `before` (the code ahead of an operator) ends with a value, so an
/// operator after it is binary. A closing bracket, a quote, a number or a name
/// is a value. A keyword like `return` or `and` is not, so `return %Node`
/// still starts with a lookup.
fn follows_operand(before: &str) -> bool {
    let before = before.trim_end();
    let Some(last) = before.chars().last() else {
        return false;
    };
    if matches!(last, ')' | ']' | '}' | '"' | '\'') {
        return true;
    }
    if !is_identifier_char(last) {
        return false;
    }
    let word_start = before
        .char_indices()
        .rev()
        .find(|&(_, c)| !is_identifier_char(c))
        .map_or(0, |(index, c)| index + c.len_utf8());
    let word = &before[word_start..];
    !matches!(
        word,
        "and"
            | "or"
            | "not"
            | "in"
            | "is"
            | "as"
            | "return"
            | "await"
            | "if"
            | "elif"
            | "while"
            | "match"
            | "when"
            | "else"
    )
}

// ---------------------------------------------------------------------------
// New rule 17: max-nesting-depth
// ---------------------------------------------------------------------------

/// Warn when nesting depth inside a function exceeds the limit.
pub fn check_max_nesting_depth(
    file: &ScriptFile,
    config: &Config,
    diagnostics: &mut Vec<Diagnostic>,
) {
    check_nesting_recursive(&file.members, file, config, diagnostics);
}

fn check_nesting_recursive(
    members: &[ClassMember],
    file: &ScriptFile,
    config: &Config,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for_each_member(members, |member| {
        let ClassMember::Function {
            name,
            body_line_count,
            span,
            ..
        } = member
        else {
            return;
        };
        if *body_line_count == 0 {
            return;
        }
        let range = function_body_range(span, *body_line_count, file.lines.len());
        // The body's base indent is the shallowest non-empty body line.
        let base_indent = file.lines[range.clone()]
            .iter()
            .filter(|l| !l.trim().is_empty())
            .map(|l| indent_level(l))
            .min()
            .unwrap_or(0);

        let mut max_depth = 0;
        let mut max_depth_line = range.start;
        for idx in range {
            let trimmed = file.lines[idx].trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let depth = indent_level(&file.lines[idx]).saturating_sub(base_indent);
            if depth > max_depth {
                max_depth = depth;
                max_depth_line = idx;
            }
        }

        if max_depth > config.max_nesting_depth {
            diagnostics.push(Diagnostic::warning(
                "quality/max-nesting-depth",
                format!(
                    "function '{}' has nesting depth {} (max {})",
                    name, max_depth, config.max_nesting_depth
                ),
                Span::new(max_depth_line + 1, 1, 0, 0),
                &file.path,
            ));
        }
    });
}

// ---------------------------------------------------------------------------
// New rule 18: max-returns
// ---------------------------------------------------------------------------

/// Warn when a function has too many return statements. Every `return` on a
/// line counts, including one-line ones like `1: return "a"` and
/// `print(1); return 2`.
///
/// `string_line_mask` marks the lines inside strings that span lines: their
/// string data is not code, so a `return` there is not counted.
pub fn check_max_returns(
    string_line_mask: &StringLines,
    file: &ScriptFile,
    config: &Config,
    diagnostics: &mut Vec<Diagnostic>,
) {
    check_returns_recursive(&file.members, file, string_line_mask, config, diagnostics);
}

fn check_returns_recursive(
    members: &[ClassMember],
    file: &ScriptFile,
    string_line_mask: &StringLines,
    config: &Config,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for_each_member(members, |member| {
        let ClassMember::Function {
            name,
            body_line_count,
            span,
            ..
        } = member
        else {
            return;
        };
        let range = function_body_range(span, *body_line_count, file.lines.len());
        let count: usize = range
            .map(|index| {
                count_returns(code_outside_spanning_strings(file, string_line_mask, index))
            })
            .sum();

        if count > config.max_returns {
            diagnostics.push(Diagnostic::warning(
                "quality/max-returns",
                format!(
                    "function '{}' has {} return statements (max {})",
                    name, count, config.max_returns
                ),
                *span,
                &file.path,
            ));
        }
    });
}

// ---------------------------------------------------------------------------
// New rule 19: max-branches
// ---------------------------------------------------------------------------

/// Warn when a function has too many branches (if/elif/match).
pub fn check_max_branches(file: &ScriptFile, config: &Config, diagnostics: &mut Vec<Diagnostic>) {
    check_branches_recursive(&file.members, file, config, diagnostics);
}

fn check_branches_recursive(
    members: &[ClassMember],
    file: &ScriptFile,
    config: &Config,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for_each_member(members, |member| {
        let ClassMember::Function {
            name,
            body_line_count,
            span,
            ..
        } = member
        else {
            return;
        };
        let range = function_body_range(span, *body_line_count, file.lines.len());
        let count = file.lines[range]
            .iter()
            .filter(|line| {
                let trimmed = line.trim();
                is_branch_header(trimmed, "if")
                    || is_branch_header(trimmed, "elif")
                    || is_branch_header(trimmed, "match")
            })
            .count();

        if count > config.max_branches {
            diagnostics.push(Diagnostic::warning(
                "quality/max-branches",
                format!(
                    "function '{}' has {} branches (max {})",
                    name, count, config.max_branches
                ),
                *span,
                &file.path,
            ));
        }
    });
}

// ---------------------------------------------------------------------------
// New rule 20: max-local-variables
// ---------------------------------------------------------------------------

/// Warn when a function declares too many local variables.
pub fn check_max_local_variables(
    file: &ScriptFile,
    config: &Config,
    diagnostics: &mut Vec<Diagnostic>,
) {
    check_local_vars_recursive(&file.members, file, config, diagnostics);
}

fn check_local_vars_recursive(
    members: &[ClassMember],
    file: &ScriptFile,
    config: &Config,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for_each_member(members, |member| {
        let ClassMember::Function {
            name,
            body_line_count,
            span,
            ..
        } = member
        else {
            return;
        };
        let range = function_body_range(span, *body_line_count, file.lines.len());
        let count = file.lines[range]
            .iter()
            .filter(|l| l.trim().starts_with("var "))
            .count();

        if count > config.max_local_variables {
            diagnostics.push(Diagnostic::warning(
                "quality/max-local-variables",
                format!(
                    "function '{}' has {} local variables (max {})",
                    name, count, config.max_local_variables
                ),
                *span,
                &file.path,
            ));
        }
    });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::*;

    /// The string-line map of a hand-built file that holds no strings.
    fn no_strings(file: &ScriptFile) -> StringLines {
        StringLines::new(&[], file.lines.len())
    }

    fn span(line: usize) -> Span {
        Span::new(line, 1, 0, 0)
    }

    /// Lex a source snippet into the token stream the rules operate on.
    /// Keeps dotted-path regression tests readable instead of hand-building
    /// `Identifier`/`Dot` sequences.
    fn lex(source: &str) -> Vec<Token> {
        crate::lexer::Lexer::new(source).tokenize()
    }

    fn empty_file() -> ScriptFile {
        ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![],
            members: vec![],
        }
    }

    // --- Existing rule tests ---

    #[test]
    fn function_within_limit_no_diagnostic() {
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![],
            members: vec![ClassMember::Function {
                name: "short_func".to_string(),
                name_span: span(1),
                parameters: vec![],
                return_type: None,
                is_static: false,
                annotations: vec![],
                body_line_count: 10,
                span: span(1),
            }],
        };
        let config = Config::default();
        let mut diags = Vec::new();
        check_max_function_length(&file, &config, &mut diags);
        assert!(diags.is_empty());
    }

    #[test]
    fn function_exceeds_limit() {
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![],
            members: vec![ClassMember::Function {
                name: "long_func".to_string(),
                name_span: span(1),
                parameters: vec![],
                return_type: None,
                is_static: false,
                annotations: vec![],
                body_line_count: 60,
                span: span(1),
            }],
        };
        let config = Config::default();
        let mut diags = Vec::new();
        check_max_function_length(&file, &config, &mut diags);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("60 lines"));
    }

    #[test]
    fn file_within_limit() {
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec!["line".to_string(); 100],
            members: vec![],
        };
        let config = Config::default();
        let mut diags = Vec::new();
        check_max_file_length(&file, &config, &mut diags);
        assert!(diags.is_empty());
    }

    #[test]
    fn file_exceeds_limit() {
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec!["line".to_string(); 1500],
            members: vec![],
        };
        let config = Config::default();
        let mut diags = Vec::new();
        check_max_file_length(&file, &config, &mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn too_many_parameters() {
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![],
            members: vec![ClassMember::Function {
                name: "complex_func".to_string(),
                name_span: span(1),
                parameters: vec![
                    Parameter {
                        name: "a".into(),
                        type_hint: None,
                        span: span(1),
                    },
                    Parameter {
                        name: "b".into(),
                        type_hint: None,
                        span: span(1),
                    },
                    Parameter {
                        name: "c".into(),
                        type_hint: None,
                        span: span(1),
                    },
                    Parameter {
                        name: "d".into(),
                        type_hint: None,
                        span: span(1),
                    },
                    Parameter {
                        name: "e".into(),
                        type_hint: None,
                        span: span(1),
                    },
                    Parameter {
                        name: "f".into(),
                        type_hint: None,
                        span: span(1),
                    },
                ],
                return_type: None,
                is_static: false,
                annotations: vec![],
                body_line_count: 5,
                span: span(1),
            }],
        };
        let config = Config::default();
        let mut diags = Vec::new();
        check_max_parameters(&file, &config, &mut diags);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("6 parameters"));
    }

    #[test]
    fn parameters_within_limit() {
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![],
            members: vec![ClassMember::Function {
                name: "ok_func".to_string(),
                name_span: span(1),
                parameters: vec![
                    Parameter {
                        name: "a".into(),
                        type_hint: None,
                        span: span(1),
                    },
                    Parameter {
                        name: "b".into(),
                        type_hint: None,
                        span: span(1),
                    },
                ],
                return_type: None,
                is_static: false,
                annotations: vec![],
                body_line_count: 5,
                span: span(1),
            }],
        };
        let config = Config::default();
        let mut diags = Vec::new();
        check_max_parameters(&file, &config, &mut diags);
        assert!(diags.is_empty());
    }

    // --- New rule tests ---

    #[test]
    fn no_debug_print_catches_print() {
        let tokens = vec![
            Token::new(
                TokenKind::Identifier("print".into()),
                span(1),
                "print".into(),
            ),
            Token::new(TokenKind::LeftParen, span(1), "(".into()),
        ];
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec!["print(\"hello\")".to_string()],
            members: vec![],
        };
        let mut diags = Vec::new();
        check_no_debug_print(&tokens, &file, &mut diags);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("print()"));
    }

    #[test]
    fn no_debug_print_ignores_custom_functions() {
        let tokens = vec![
            Token::new(
                TokenKind::Identifier("print_score".into()),
                span(1),
                "print_score".into(),
            ),
            Token::new(TokenKind::LeftParen, span(1), "(".into()),
        ];
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec!["print_score(100)".to_string()],
            members: vec![],
        };
        let mut diags = Vec::new();
        check_no_debug_print(&tokens, &file, &mut diags);
        assert!(diags.is_empty());
    }

    #[test]
    fn self_comparison_detected() {
        let tokens = vec![
            Token::new(TokenKind::Identifier("x".into()), span(1), "x".into()),
            Token::new(TokenKind::Equal, span(1), "==".into()),
            Token::new(TokenKind::Identifier("x".into()), span(1), "x".into()),
        ];
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![],
            members: vec![],
        };
        let mut diags = Vec::new();
        check_self_comparison(&tokens, &file, &mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn self_comparison_different_vars_ok() {
        let tokens = vec![
            Token::new(TokenKind::Identifier("x".into()), span(1), "x".into()),
            Token::new(TokenKind::Equal, span(1), "==".into()),
            Token::new(TokenKind::Identifier("y".into()), span(1), "y".into()),
        ];
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![],
            members: vec![],
        };
        let mut diags = Vec::new();
        check_self_comparison(&tokens, &file, &mut diags);
        assert!(diags.is_empty());
    }

    #[test]
    fn self_comparison_dotted_tail_not_flagged() {
        // Regression (issue #7): `stored_goods.label == label` shares the
        // trailing segment `label`, but the operands are different values.
        let tokens = lex("if stored_goods.label == label:\n\tpass\n");
        let mut diags = Vec::new();
        check_self_comparison(&tokens, &empty_file(), &mut diags);
        assert!(
            diags.is_empty(),
            "a shared trailing segment must not self-compare: {diags:?}"
        );
    }

    #[test]
    fn self_comparison_different_dotted_paths_not_flagged() {
        // Issue #7 minimal repro: `position == whatever.position`.
        let tokens = lex("if position == whatever.position:\n\tpass\n");
        let mut diags = Vec::new();
        check_self_comparison(&tokens, &empty_file(), &mut diags);
        assert!(
            diags.is_empty(),
            "different chains must not self-compare: {diags:?}"
        );
    }

    #[test]
    fn self_comparison_rhs_dotted_head_not_flagged() {
        // `x == x.foo` — the RHS chain absorbs `.foo`, so operands differ.
        let tokens = lex("if x == x.foo:\n\tpass\n");
        let mut diags = Vec::new();
        check_self_comparison(&tokens, &empty_file(), &mut diags);
        assert!(
            diags.is_empty(),
            "dotted head must not self-compare: {diags:?}"
        );
    }

    #[test]
    fn self_comparison_identical_dotted_paths_flagged() {
        // `obj.foo == obj.foo` genuinely compares a value with itself.
        let tokens = lex("if obj.foo == obj.foo:\n\tpass\n");
        let mut diags = Vec::new();
        check_self_comparison(&tokens, &empty_file(), &mut diags);
        assert_eq!(diags.len(), 1, "identical dotted paths should be flagged");
    }

    #[test]
    fn self_comparison_partial_match_in_arithmetic_not_flagged() {
        // `foo + x == x` is `(foo + x) == x`, not a self-comparison.
        let tokens = lex("if foo + x == x:\n\tpass\n");
        let mut diags = Vec::new();
        check_self_comparison(&tokens, &empty_file(), &mut diags);
        assert!(
            diags.is_empty(),
            "arithmetic LHS must not self-compare: {diags:?}"
        );
    }

    #[test]
    fn self_comparison_rhs_continues_expression_not_flagged() {
        // `x == x + 1` is `x == (x + 1)`.
        let tokens = lex("if x == x + 1:\n\tpass\n");
        let mut diags = Vec::new();
        check_self_comparison(&tokens, &empty_file(), &mut diags);
        assert!(
            diags.is_empty(),
            "continued RHS must not self-compare: {diags:?}"
        );
    }

    #[test]
    fn no_self_assign_detected() {
        let tokens = vec![
            Token::new(TokenKind::Identifier("x".into()), span(1), "x".into()),
            Token::new(TokenKind::Assign, span(1), "=".into()),
            Token::new(TokenKind::Identifier("x".into()), span(1), "x".into()),
            Token::new(TokenKind::Newline, span(1), "\n".into()),
        ];
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![],
            members: vec![],
        };
        let mut diags = Vec::new();
        check_no_self_assign(&tokens, &file, &mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_self_assign_dot_access_ok() {
        let tokens = vec![
            Token::new(TokenKind::Identifier("x".into()), span(1), "x".into()),
            Token::new(TokenKind::Assign, span(1), "=".into()),
            Token::new(TokenKind::Identifier("x".into()), span(1), "x".into()),
            Token::new(TokenKind::Dot, span(1), ".".into()),
        ];
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![],
            members: vec![],
        };
        let mut diags = Vec::new();
        check_no_self_assign(&tokens, &file, &mut diags);
        assert!(diags.is_empty());
    }

    #[test]
    fn empty_function_detected() {
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec!["func do_nothing():".to_string(), "\tpass".to_string()],
            members: vec![ClassMember::Function {
                name: "do_nothing".to_string(),
                name_span: span(1),
                parameters: vec![],
                return_type: None,
                is_static: false,
                annotations: vec![],
                body_line_count: 1,
                span: span(1),
            }],
        };
        let mut diags = Vec::new();
        check_empty_function(&file, &mut diags);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("only contains 'pass'"));
    }

    #[test]
    fn type_hint_warns_on_missing() {
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![],
            members: vec![
                ClassMember::Variable {
                    name: "speed".to_string(),
                    name_span: span(1),
                    type_hint: None,
                    annotations: vec![],
                    has_accessor_block: false,
                    span: span(1),
                },
                ClassMember::Variable {
                    name: "health".to_string(),
                    name_span: span(2),
                    type_hint: Some("int".to_string()),
                    annotations: vec![],
                    has_accessor_block: false,
                    span: span(2),
                },
            ],
        };
        let mut diags = Vec::new();
        check_type_hint(&file, &mut diags);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("speed"));
    }

    #[test]
    fn unreachable_code_after_return() {
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![
                "func foo():".to_string(),
                "\treturn 1".to_string(),
                "\tvar x = 2".to_string(),
            ],
            members: vec![],
        };
        let mut diags = Vec::new();
        check_unreachable_code(&no_strings(&file), &file, &mut diags);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].span.line, 3);
    }

    #[test]
    fn unreachable_code_multiline_return_close_paren_not_flagged() {
        // Regression for issue #3: a multi-line return like
        //     return floori(
        //         1.2
        //     )
        // would have its closing `)` flagged as "unreachable code"
        // because the rule scanned forward line-by-line ignoring that
        // the closing paren is part of the same `return` statement.
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![
                "func single_return() -> int:".to_string(),
                "\treturn floori(".to_string(),
                "\t\t1.2".to_string(),
                "\t)".to_string(),
            ],
            members: vec![],
        };
        let mut diags = Vec::new();
        check_unreachable_code(&no_strings(&file), &file, &mut diags);
        assert!(
            diags.is_empty(),
            "closing paren of multi-line return must not be flagged, got: {:?}",
            diags
        );
    }

    #[test]
    fn unreachable_code_multiline_return_user_reported_shape() {
        // Verbatim shape from issue #3 reporter (EvilsPresley): two
        // multi-line `return` statements, one inside `if` with an
        // inconsistently-indented close paren, one at function-body
        // level with the canonical close-paren indent. Neither should
        // produce an unreachable-code diagnostic.
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![
                "func unreachable_code(".to_string(),
                "\treachable: bool = true".to_string(),
                ") -> int:".to_string(),
                "\tif reachable:".to_string(),
                "\t\treturn floori(".to_string(),
                "\t\t\t1.1".to_string(),
                "\t\t\t)".to_string(),
                "\treturn floori(".to_string(),
                "\t\t1.2".to_string(),
                "\t)".to_string(),
            ],
            members: vec![],
        };
        let mut diags = Vec::new();
        check_unreachable_code(&no_strings(&file), &file, &mut diags);
        assert!(
            diags.is_empty(),
            "no unreachable-code expected on issue #3 example, got: {:?}",
            diags
        );
    }

    #[test]
    fn unreachable_code_backslash_continuation_not_flagged() {
        // A `return` whose expression continues onto the next line via
        // backslash continuation is one statement, not two — and the
        // continuation line lands at the same indent as `return`.
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![
                "func sum() -> int:".to_string(),
                "\treturn 1 + 2 \\".to_string(),
                "\t\t+ 3".to_string(),
            ],
            members: vec![],
        };
        let mut diags = Vec::new();
        check_unreachable_code(&no_strings(&file), &file, &mut diags);
        assert!(diags.is_empty(), "got: {:?}", diags);
    }

    #[test]
    fn unreachable_code_after_multiline_return_still_flagged() {
        // True positive guard: code at the same indent AFTER the
        // multi-line return has closed must still be flagged.
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![
                "func foo() -> int:".to_string(),
                "\treturn floori(".to_string(),
                "\t\t1.2".to_string(),
                "\t)".to_string(),
                "\tvar x = 2".to_string(),
            ],
            members: vec![],
        };
        let mut diags = Vec::new();
        check_unreachable_code(&no_strings(&file), &file, &mut diags);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].span.line, 5);
    }

    #[test]
    fn unreachable_code_else_not_flagged() {
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![
                "func foo():".to_string(),
                "\tif true:".to_string(),
                "\t\treturn 1".to_string(),
                "\telse:".to_string(),
                "\t\treturn 2".to_string(),
            ],
            members: vec![],
        };
        let mut diags = Vec::new();
        check_unreachable_code(&no_strings(&file), &file, &mut diags);
        assert!(diags.is_empty());
    }

    #[test]
    fn await_in_loop_detected() {
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![
                "func fetch_all():".to_string(),
                "\tfor item in items:".to_string(),
                "\t\tawait http_request(item)".to_string(),
            ],
            members: vec![],
        };
        let mut diags = Vec::new();
        check_await_in_loop(&file, &mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn await_in_loop_ignores_keyword_inside_string() {
        // `" await "` inside a string literal is not an await statement.
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![
                "func fetch_all():".to_string(),
                "\tfor item in items:".to_string(),
                "\t\tvar label = \"please await the result\"".to_string(),
            ],
            members: vec![],
        };
        let mut diags = Vec::new();
        check_await_in_loop(&file, &mut diags);
        assert!(
            diags.is_empty(),
            "await inside a string must not be flagged"
        );
    }

    #[test]
    fn allocation_in_loop_detected() {
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![
                "func spawn():".to_string(),
                "\tfor i in range(10):".to_string(),
                "\t\tvar enemy = Enemy.new()".to_string(),
            ],
            members: vec![],
        };
        let mut diags = Vec::new();
        check_allocation_in_loop(&file, &mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_else_return_detected() {
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![
                "func foo(x):".to_string(),
                "\tif x > 0:".to_string(),
                "\t\treturn true".to_string(),
                "\telse:".to_string(),
                "\t\treturn false".to_string(),
            ],
            members: vec![],
        };
        let mut diags = Vec::new();
        check_no_else_return(&no_strings(&file), &file, &mut diags);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("else"));
    }

    #[test]
    fn max_class_variables_exceeded() {
        let members: Vec<ClassMember> = (0..20)
            .map(|i| ClassMember::Variable {
                name: format!("var_{}", i),
                name_span: span(i + 1),
                type_hint: None,
                annotations: vec![],
                has_accessor_block: false,
                span: span(i + 1),
            })
            .collect();
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![],
            members,
        };
        let config = Config::default(); // max 15
        let mut diags = Vec::new();
        check_max_class_variables(&file, &config, &mut diags);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("20 variables"));
    }

    #[test]
    fn max_public_methods_exceeded() {
        let members: Vec<ClassMember> = (0..25)
            .map(|i| ClassMember::Function {
                name: format!("method_{}", i),
                name_span: span(i + 1),
                parameters: vec![],
                return_type: None,
                is_static: false,
                annotations: vec![],
                body_line_count: 1,
                span: span(i + 1),
            })
            .collect();
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![],
            members,
        };
        let config = Config::default(); // max 20
        let mut diags = Vec::new();
        check_max_public_methods(&file, &config, &mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn max_inner_classes_exceeded() {
        let members: Vec<ClassMember> = (0..8)
            .map(|i| ClassMember::InnerClass {
                name: format!("Inner{}", i),
                name_span: span(i + 1),
                members: vec![],
                span: span(i + 1),
            })
            .collect();
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![],
            members,
        };
        let config = Config::default(); // max 5
        let mut diags = Vec::new();
        check_max_inner_classes(&file, &config, &mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn max_inner_classes_counts_nested_classes() {
        // Two top-level classes holding four nested ones: six in the file.
        let class = |name: &str, line: usize, members: Vec<ClassMember>| ClassMember::InnerClass {
            name: name.to_string(),
            name_span: span(line),
            members,
            span: span(line),
        };
        let members = vec![
            class("A", 1, vec![class("A1", 2, vec![]), class("A2", 3, vec![])]),
            class("B", 4, vec![class("B1", 5, vec![class("B2", 6, vec![])])]),
        ];
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![],
            members,
        };
        let mut diags = Vec::new();
        check_max_inner_classes(&file, &Config::default(), &mut diags);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("6 inner classes"), "{diags:?}");
    }

    // --- Line helpers ---

    #[test]
    fn blank_strings_and_comments_keeps_every_byte_offset() {
        let line = "var s = \"é日\" + x  # ñ";
        let blanked = blank_strings_and_comments(line);
        assert_eq!(blanked.len(), line.len());
        assert_eq!(blanked, "var s = \"     \" + x      ");
        assert!(matches!(
            blank_strings_and_comments("var x = 1"),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn blank_strings_and_comments_reads_triple_quotes_on_one_line() {
        assert_eq!(
            blank_strings_and_comments("\"\"\"a\"b\"\"\" + $Foo"),
            "\"\"\"   \"\"\" + $Foo"
        );
        assert_eq!(
            blank_strings_and_comments("'''it's''' + $Foo"),
            "'''    ''' + $Foo"
        );
        // Still open at the end of the line: the rest is string data.
        assert_eq!(blank_strings_and_comments("x(\"\"\"a)"), "x(\"\"\"  ");
    }

    #[test]
    fn blank_strings_and_comments_lets_a_backslash_hold_the_quote() {
        // Like Godot, a backslash keeps the quote from closing the string,
        // raw strings included, so `r"C:\"` is still open.
        assert_eq!(
            blank_strings_and_comments("r\"C:\\\" + $Foo"),
            "r\"           "
        );
        assert_eq!(
            blank_strings_and_comments("\"a\\\\\" + $Foo"),
            "\"   \" + $Foo"
        );
    }

    #[test]
    fn line_helpers_skip_strings_and_comments() {
        assert_eq!(line_bracket_delta("foo(\"(\", [1]  # ("), 1);
        assert_eq!(line_bracket_delta("\"\"\"a\"(\"\"\" + (1"), 1);
        assert_eq!(strip_trailing_comment("x = \"#\" # c"), "x = \"#\" ");
        assert!(ends_with_backslash_continuation("x = 1 + \\  "));
        assert!(!ends_with_backslash_continuation("x = 1  # C:\\"));
        assert!(!ends_with_backslash_continuation("x = 1 + \\ # note"));
    }

    #[test]
    fn return_helpers_find_every_return_statement() {
        assert!(has_return_statement("print(1); return 2"));
        assert!(has_return_statement("return 1; print(2)"));
        assert!(!has_return_statement("print(\"; return\"); pass"));
        assert_eq!(count_returns("1: return \"return\"  # return"), 1);
        assert_eq!(count_returns("if x: return 1; else_return()"), 1);
        assert_eq!(count_returns("x.return_value; y.return"), 0);
    }
}
