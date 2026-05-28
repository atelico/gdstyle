use std::collections::{HashMap, HashSet};

use crate::ast::{for_each_member, ClassMember, ScriptFile};
use crate::config::Config;
use crate::diagnostic::Diagnostic;
use crate::token::{Span, Token, TokenKind};

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

/// Return a copy of `line` with string-literal contents and any trailing
/// comment replaced by spaces, preserving byte length so column offsets
/// stay valid. Used by the line-based quality rules so a keyword appearing
/// inside a string literal (`var s = " await "`) isn't mistaken for code.
fn blank_strings_and_comments(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_string: Option<char> = None;
    let mut escaped = false;
    for ch in line.chars() {
        match in_string {
            Some(quote) => {
                // Inside a string: blank everything (including the content),
                // keep the closing quote so structure is still visible.
                if escaped {
                    escaped = false;
                    out.push(' ');
                } else if ch == '\\' {
                    escaped = true;
                    out.push(' ');
                } else if ch == quote {
                    in_string = None;
                    out.push(ch);
                } else {
                    out.push(' ');
                }
            }
            None => {
                if ch == '"' || ch == '\'' {
                    in_string = Some(ch);
                    out.push(ch);
                } else if ch == '#' {
                    // Rest of the line is a comment, blank it and stop.
                    for _ in 0..(line.len() - out.len()) {
                        out.push(' ');
                    }
                    break;
                } else {
                    out.push(ch);
                }
            }
        }
    }
    out
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

/// Warn when comparing a value with itself (e.g. `x == x`).
pub fn check_self_comparison(
    tokens: &[Token],
    file: &ScriptFile,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Look for patterns: Identifier op Identifier where both are the same
    for i in 0..tokens.len().saturating_sub(2) {
        let left = &tokens[i];
        let op = &tokens[i + 1];
        let right = &tokens[i + 2];

        if !COMPARISON_OPS.contains(&op.kind) {
            continue;
        }

        if let (TokenKind::Identifier(lname), TokenKind::Identifier(rname)) =
            (&left.kind, &right.kind)
        {
            if lname == rname {
                diagnostics.push(Diagnostic::warning(
                    "quality/self-comparison",
                    format!("comparing '{}' with itself", lname),
                    op.span,
                    &file.path,
                ));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// New rule 4: no-self-assign
// ---------------------------------------------------------------------------

/// Warn when assigning a variable to itself (e.g. `x = x`).
pub fn check_no_self_assign(
    tokens: &[Token],
    file: &ScriptFile,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for i in 0..tokens.len().saturating_sub(2) {
        let left = &tokens[i];
        let op = &tokens[i + 1];
        let right = &tokens[i + 2];

        if op.kind != TokenKind::Assign {
            continue;
        }

        if let (TokenKind::Identifier(lname), TokenKind::Identifier(rname)) =
            (&left.kind, &right.kind)
        {
            if lname == rname {
                // Make sure the token after `right` is not a dot (would be `x = x.something`)
                if let Some(after) = tokens.get(i + 3) {
                    if after.kind == TokenKind::Dot {
                        continue;
                    }
                }
                diagnostics.push(Diagnostic::warning(
                    "quality/no-self-assign",
                    format!("self-assignment of '{}'", lname),
                    op.span,
                    &file.path,
                ));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// New rule 5: duplicate-dict-key
// ---------------------------------------------------------------------------

/// Warn about duplicate keys in dictionary literals.
pub fn check_duplicate_dict_key(
    tokens: &[Token],
    file: &ScriptFile,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].kind == TokenKind::LeftBrace {
            // Scan this dictionary literal for duplicate keys
            let mut keys: HashMap<String, Span> = HashMap::new();
            let mut depth = 1;
            let mut j = i + 1;
            let mut expect_key = true;

            while j < tokens.len() && depth > 0 {
                match &tokens[j].kind {
                    TokenKind::LeftBrace => {
                        depth += 1;
                        expect_key = true;
                    }
                    TokenKind::RightBrace => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    TokenKind::Colon if depth == 1 => {
                        expect_key = false;
                    }
                    TokenKind::Comma if depth == 1 => {
                        expect_key = true;
                    }
                    TokenKind::Newline => {}
                    _ if expect_key && depth == 1 => {
                        // Use the semantic value for string keys so
                        // `{"foo": 1, 'foo': 2}` is detected as a duplicate
                        // (same value, different surrounding quotes).
                        let key_text = match &tokens[j].kind {
                            TokenKind::String(info) => info.value.clone(),
                            _ => tokens[j].text.clone(),
                        };
                        if let Some(prev_span) = keys.get(&key_text) {
                            diagnostics.push(Diagnostic::warning(
                                "quality/duplicate-dict-key",
                                format!(
                                    "duplicate dictionary key '{}' (first seen at line {})",
                                    key_text, prev_span.line
                                ),
                                tokens[j].span,
                                &file.path,
                            ));
                        } else {
                            keys.insert(key_text, tokens[j].span);
                        }
                        // After seeing a key token, don't re-flag composite keys
                        // (we only detect simple single-token keys)
                    }
                    _ => {}
                }
                j += 1;
            }
            i = j + 1;
        } else {
            i += 1;
        }
    }
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
    let count = file
        .members
        .iter()
        .filter(|m| matches!(m, ClassMember::InnerClass { .. }))
        .count();

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

/// Warn when an `else` or `elif` follows an `if` block that ends with `return`.
/// The else/elif is unnecessary because the `return` already exits.
pub fn check_no_else_return(file: &ScriptFile, diagnostics: &mut Vec<Diagnostic>) {
    for (i, line) in file.lines.iter().enumerate() {
        let trimmed = line.trim();
        // Look for `elif` or `else` lines
        if !(trimmed.starts_with("elif ")
            || trimmed.starts_with("elif(")
            || trimmed == "else:"
            || trimmed.starts_with("else:"))
        {
            continue;
        }

        let current_indent = indent_level(line);

        // Walk backwards to find if the previous block ended with return
        let mut j = i;
        while j > 0 {
            j -= 1;
            let prev = file.lines[j].trim();
            if prev.is_empty() || prev.starts_with('#') {
                continue;
            }
            // Check if this line is at a deeper indent (inside the if block)
            if indent_level(&file.lines[j]) > current_indent {
                if prev.starts_with("return") {
                    let kind = if trimmed.starts_with("elif") {
                        "elif"
                    } else {
                        "else"
                    };
                    diagnostics.push(Diagnostic::warning(
                        "quality/no-else-return",
                        format!("unnecessary '{}' after 'return'", kind),
                        Span::new(i + 1, 1, 0, 0),
                        &file.path,
                    ));
                }
                break;
            } else {
                break;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// New rule 13: unreachable-code
// ---------------------------------------------------------------------------

/// Warn about code that appears after `return`, `break`, or `continue` at the
/// same indentation level within a block.
pub fn check_unreachable_code(file: &ScriptFile, diagnostics: &mut Vec<Diagnostic>) {
    let mut reported: HashSet<usize> = HashSet::new();

    for (i, line) in file.lines.iter().enumerate() {
        let trimmed = line.trim();
        if !(trimmed.starts_with("return") || trimmed == "break" || trimmed == "continue") {
            continue;
        }

        // `return` can be `return`, `return <expr>`, or `return(expr)`
        // (the latter is rare but legal). Anything else with a `return`
        // prefix (e.g. `return_value`, `returns`, `returnable`) is an
        // identifier and not a return statement.
        if trimmed.starts_with("return") && trimmed.len() > 6 {
            let rest = &trimmed[6..];
            let next = rest.chars().next().unwrap_or('\0');
            if !matches!(next, ' ' | '\t' | '(') {
                continue;
            }
        }

        let stmt_indent = indent_level(line);

        // Check lines after this one at the same indent level
        for j in (i + 1)..file.lines.len() {
            let next = &file.lines[j];
            let next_trimmed = next.trim();

            if next_trimmed.is_empty() || next_trimmed.starts_with('#') {
                continue;
            }

            let next_indent = indent_level(next);

            if next_indent < stmt_indent {
                // We've left the block
                break;
            }

            if next_indent == stmt_indent && !reported.contains(&j) {
                // This is at the same level, it's unreachable
                // But skip if it's an elif/else/except (those are sibling branches)
                if next_trimmed.starts_with("elif ")
                    || next_trimmed.starts_with("elif(")
                    || next_trimmed == "else:"
                    || next_trimmed.starts_with("else:")
                {
                    break;
                }
                reported.insert(j);
                diagnostics.push(Diagnostic::warning(
                    "quality/unreachable-code",
                    "unreachable code".to_string(),
                    Span::new(j + 1, 1, 0, 0),
                    &file.path,
                ));
                break; // One diagnostic per unreachable block
            }
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
pub fn check_process_get_node(file: &ScriptFile, diagnostics: &mut Vec<Diagnostic>) {
    check_process_get_node_recursive(&file.members, file, diagnostics);
}

fn check_process_get_node_recursive(
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
        if name != "_process" && name != "_physics_process" {
            return;
        }
        let range = function_body_range(span, *body_line_count, file.lines.len());
        for idx in range {
            let trimmed = file.lines[idx].trim();
            if trimmed.starts_with('#') {
                continue;
            }
            let has_node_ref = trimmed.contains("get_node(")
                || trimmed.contains("get_node_or_null(")
                || trimmed.contains('$')
                || has_unique_node_ref(trimmed);
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

/// Check if a line contains a `%UniqueNode` reference, distinguishing it from
/// the modulo operator.
///
/// The reliable discriminator is the character *after* `%`: a unique-node
/// reference is `%` immediately followed by an identifier character or a
/// quote (`%Player`, `%"Health Bar"`), with no space. The modulo operator
/// `a % b` always has a space or a digit-with-space around it. Relying on
/// the character *before* `%` was wrong, `a % b` has a space before `%`
/// just like ` %Node` does.
fn has_unique_node_ref(line: &str) -> bool {
    let bytes = line.as_bytes();
    for (i, _) in line.match_indices('%') {
        match bytes.get(i + 1) {
            Some(&c) if c.is_ascii_alphabetic() || c == b'_' || c == b'"' || c == b'\'' => {
                return true;
            }
            _ => {}
        }
    }
    false
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

/// Warn when a function has too many return statements.
pub fn check_max_returns(file: &ScriptFile, config: &Config, diagnostics: &mut Vec<Diagnostic>) {
    check_returns_recursive(&file.members, file, config, diagnostics);
}

fn check_returns_recursive(
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
            .filter(|l| {
                let t = l.trim();
                t == "return" || t.starts_with("return ")
            })
            .count();

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
            .filter(|l| {
                let t = l.trim();
                t.starts_with("if ")
                    || t.starts_with("if(")
                    || t.starts_with("elif ")
                    || t.starts_with("elif(")
                    || t.starts_with("match ")
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

    fn span(line: usize) -> Span {
        Span::new(line, 1, 0, 0)
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
                    span: span(1),
                },
                ClassMember::Variable {
                    name: "health".to_string(),
                    name_span: span(2),
                    type_hint: Some("int".to_string()),
                    annotations: vec![],
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
        check_unreachable_code(&file, &mut diags);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].span.line, 3);
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
        check_unreachable_code(&file, &mut diags);
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
        check_no_else_return(&file, &mut diags);
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
}
