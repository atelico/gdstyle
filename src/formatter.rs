use crate::ast::ClassMember;
use crate::config::Config;
use crate::fixer;
use crate::lexer::Lexer;
use crate::linter;
use crate::parser::Parser;
use crate::token::TokenKind;

/// Format a GDScript source string according to style rules.
///
/// This applies ALL formatting transformations unconditionally, unlike --fix
/// which only fixes flagged violations. The result is idempotent: formatting
/// already-formatted source returns it unchanged.
///
/// # Example
///
/// ```
/// use gdstyle::{config::Config, formatter};
///
/// let formatted = formatter::format_source("var x = 1   \n\n\n\n", &Config::default());
/// // Trailing whitespace stripped, 3+ blank lines collapsed, single final newline.
/// assert_eq!(formatted, "var x = 1\n");
/// ```
pub fn format_source(source: &str, config: &Config) -> String {
    // Normalize line endings up front so byte-offset computations don't
    // disagree across passes. See `linter::normalize_line_endings` for the
    // full motivation. Idempotent on LF-only input.
    let mut result = crate::linter::normalize_line_endings(source);

    // Multi-pass: apply formatting until stable (idempotent).
    for _ in 0..5 {
        let formatted = format_pass(&result, config);
        if formatted == result {
            break;
        }
        result = formatted;
    }

    result
}

fn format_pass(source: &str, config: &Config) -> String {
    let mut result = source.to_string();

    // 1. Reorder class members FIRST, before any line-changing transformations,
    //    so that parser spans are valid against the source lines.
    //
    //    The reorder is wrapped in safety guards: it is skipped when any
    //    module-level initializer depends on another module-level identifier
    //    (because reordering could change the runtime init order), and the
    //    output is re-parsed to verify it has the same set of class members
    //    as the input: if anything mutated, we keep the original.
    result = safe_reorder_class_members(&result);
    result = reorder_inner_classes(&result);

    // 2. Normalize indentation.
    result = normalize_indentation(&result, config);

    // 2. Strip trailing whitespace from every line.
    result = strip_trailing_whitespace(&result);

    // 3a. Normalize blank lines BETWEEN top-level class members to their
    //     canonical counts (0 between header items, 1 between member
    //     categories, 2 around functions / inner classes). This is the
    //     member-aware pass; it understands what each top-level statement is.
    result = normalize_member_spacing(&result);

    // 3b. Backstop for blank lines INSIDE function bodies and anywhere the
    //     member-aware pass didn't touch: collapse 3+ to 2.
    result = collapse_blank_lines(&result);

    // 4. Replace && || ! with and or not.
    result = normalize_boolean_operators(&result);

    // 5. Normalize quotes (single -> double unless contains double quotes).
    result = normalize_quotes(&result);

    // 6. Normalize comment spacing (add space after #, except #region/#endregion).
    result = normalize_comment_spacing(&result);

    // 7. Normalize float literals (add leading/trailing zeros).
    result = normalize_float_literals(&result);

    // 8. Normalize hex to lowercase.
    result = normalize_hex_literals(&result);

    // 9. Ensure file ends with exactly one \n.
    result = ensure_trailing_newline(&result);

    // 10. Use lint+fix for remaining token-based transformations.
    result = lint_then_fix(&result, config);

    // 11. Break long lines by wrapping at commas inside delimiters.
    result = break_long_lines(&result, config);

    result
}

fn normalize_indentation(source: &str, config: &Config) -> String {
    let lines: Vec<&str> = source.split('\n').collect();
    let mut result = Vec::new();

    for line in &lines {
        if line.is_empty() {
            result.push(String::new());
            continue;
        }

        let indent: String = line
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        let rest = &line[indent.len()..];

        if indent.is_empty() {
            result.push(line.to_string());
            continue;
        }

        let new_indent = if config.use_tabs {
            // Convert spaces to tabs (4 spaces = 1 tab).
            let total_spaces: usize = indent.chars().map(|c| if c == '\t' { 4 } else { 1 }).sum();
            "\t".repeat(total_spaces / 4)
        } else {
            // Convert tabs to spaces.
            indent.replace('\t', "    ")
        };

        result.push(format!("{}{}", new_indent, rest));
    }

    result.join("\n")
}

fn strip_trailing_whitespace(source: &str) -> String {
    source
        .split('\n')
        .map(|line| line.trim_end().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Canonical number of blank lines between two adjacent class-level members
/// of the given ordering categories. Categories come from
/// `ClassMember::ordering_category()`:
///
/// - 0..=2: header (`@tool`/`@icon`, `class_name`, `extends`)
/// - 3: free-standing class-level `##` doc comment
/// - 4..=10: signals, enums, consts, vars
/// - 11..=12: virtual / regular methods
/// - 13: inner classes
///
/// Rules:
/// - Header items cluster tight against one another and against the class
///   docstring (0 blank lines).
/// - Functions and inner classes always get 2 blank lines before and after.
/// - Same-category neighbours (signal+signal, var+var, etc.) sit tight.
/// - Anything else inside the class body gets a single separator line.
fn canonical_blank_lines_between(prev: usize, curr: usize) -> usize {
    if prev <= 3 && curr <= 3 {
        return 0;
    }
    if curr == 11 || curr == 12 || curr == 13 {
        return 2;
    }
    if prev == 11 || prev == 12 || prev == 13 {
        return 2;
    }
    if prev == curr {
        return 0;
    }
    1
}

/// Earliest line occupied by any annotation attached to this member (e.g.
/// `@export_range(...)` written above a `var`). Returns `None` when the
/// member carries no annotations.
fn leading_annotation_line(member: &ClassMember) -> Option<usize> {
    let annotations = match member {
        ClassMember::Variable { annotations, .. }
        | ClassMember::Function { annotations, .. }
        | ClassMember::StaticVariable { annotations, .. } => annotations.as_slice(),
        _ => return None,
    };
    annotations.iter().map(|a| a.span.line).min()
}

/// Top-level item in the source, used by `normalize_member_spacing`. A unit
/// is either a single declaration, or a leading `##` doc comment block plus
/// the declaration it attaches to. The category drives the spacing rule; the
/// `start` line is the unit's first line in the source. `decl_start` is the
/// line where the declaration itself begins (after any attached `##` docs,
/// but including annotations like `@export_range`): used to tighten the doc
/// block against the declaration when normalising.
struct MemberUnit {
    category: usize,
    start: usize,      // 1-indexed
    decl_start: usize, // 1-indexed; >= start
}

/// Rewrite the source so the blank lines BETWEEN top-level class members match
/// the canonical Godot style guide spacing. The contents of each member are
/// preserved verbatim; only the gaps between them change.
///
/// The function intentionally uses each unit's `start` as the boundary rather
/// than the AST's `end_line()` (which under-counts multi-line bodies for
/// enums and computed properties).
fn normalize_member_spacing(source: &str) -> String {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize();
    let members = Parser::new(&tokens).parse();
    if members.is_empty() {
        return source.to_string();
    }

    let lines: Vec<&str> = source.split('\n').collect();

    // Walk parsed members and group adjacent `##` doc-comment lines with the
    // next non-doc declaration (the doc "attaches" to it). A blank line
    // between two doc blocks splits them: the older block becomes a
    // standalone class-level doc unit, the newer block is still pending.
    let mut units: Vec<MemberUnit> = Vec::new();
    let mut pending_doc_start: Option<usize> = None;
    let mut pending_doc_last: Option<usize> = None;
    // A contiguous block of plain `#` comments awaiting the next declaration.
    // Unlike doc comments it never becomes its own unit; it only ever gets
    // absorbed as a leading comment when it sits tight against a declaration.
    let mut pending_comment_start: Option<usize> = None;
    let mut pending_comment_last: Option<usize> = None;

    let flush_standalone_docs =
        |units: &mut Vec<MemberUnit>, start: &mut Option<usize>, last: &mut Option<usize>| {
            if let (Some(s), Some(_)) = (*start, *last) {
                units.push(MemberUnit {
                    category: 3,
                    start: s,
                    decl_start: s,
                });
            }
            *start = None;
            *last = None;
        };

    for member in &members {
        match member {
            ClassMember::DocComment { span, .. } => {
                if let Some(prev_last) = pending_doc_last {
                    if span.line != prev_last + 1 {
                        // Non-consecutive: the older block is a standalone
                        // class doc.
                        flush_standalone_docs(
                            &mut units,
                            &mut pending_doc_start,
                            &mut pending_doc_last,
                        );
                    }
                }
                if pending_doc_start.is_none() {
                    pending_doc_start = Some(span.line);
                }
                pending_doc_last = Some(span.line);
            }
            ClassMember::Comment { span, .. } => {
                // Track a contiguous block of plain `#` comments so a block
                // sitting tight against the next declaration can attach to it
                // as a leading comment (see the declaration arm). A
                // non-consecutive comment starts a fresh block. Pending docs
                // remain pending; they may still attach to a later declaration.
                if let Some(prev_last) = pending_comment_last {
                    if span.line != prev_last + 1 {
                        // A gap: the previous block can no longer be a leading
                        // comment for the upcoming declaration; start fresh.
                        pending_comment_start = None;
                    }
                }
                if pending_comment_start.is_none() {
                    pending_comment_start = Some(span.line);
                }
                pending_comment_last = Some(span.line);
            }
            ClassMember::BlankLine { .. } => {
                // Blank lines sit where they are. A blank line breaks the
                // tightness between a pending comment block and a following
                // declaration, but the line-adjacency check in the declaration
                // arm already accounts for that gap, so no reset is needed.
                // Pending docs remain pending.
            }
            _ => {
                // The declaration itself starts at the earliest of: its
                // leading annotation line (e.g. `@export_range(...)`) or its
                // keyword. The whole unit starts at the leading `##` doc
                // block if one is attached, otherwise at the declaration.
                let kw_line = member.span().line;
                let annotation_start = leading_annotation_line(member);
                let decl_start = match annotation_start {
                    Some(a) => a.min(kw_line),
                    None => kw_line,
                };

                // A pending doc block only "attaches" to this declaration
                // when it sits tight against it (the line after the last doc
                // is the first line of this declaration). If the user left a
                // blank line between, the doc is standalone (typically the
                // class-level docstring sitting between `extends` and the
                // first member): flush it as its own unit.
                if let Some(last_doc_line) = pending_doc_last {
                    if decl_start > last_doc_line + 1 {
                        flush_standalone_docs(
                            &mut units,
                            &mut pending_doc_start,
                            &mut pending_doc_last,
                        );
                    }
                }

                let mut start = pending_doc_start
                    .map(|d| d.min(decl_start))
                    .unwrap_or(decl_start);

                // A plain `#` comment block sitting tight against this unit's
                // first line (no blank line between) is a leading comment for
                // the declaration: absorb it so the canonical member gap is
                // inserted ABOVE the comment, not between the comment and the
                // declaration it describes. A blank line breaks the tightness
                // (`c_last + 1 != start`), leaving the comment where it sits.
                if let (Some(c_start), Some(c_last)) = (pending_comment_start, pending_comment_last)
                {
                    if c_last + 1 == start {
                        start = c_start;
                    }
                }

                pending_doc_start = None;
                pending_doc_last = None;
                pending_comment_start = None;
                pending_comment_last = None;
                units.push(MemberUnit {
                    category: member.ordering_category(),
                    start,
                    decl_start,
                });
            }
        }
    }
    // Trailing pending docs (no declaration follows) become a standalone
    // class-level doc unit.
    flush_standalone_docs(&mut units, &mut pending_doc_start, &mut pending_doc_last);

    if units.is_empty() {
        return source.to_string();
    }

    let mut result: Vec<String> = Vec::new();

    // Lines before the first unit: file-level shebang or comments. Preserve
    // them verbatim, then strip any trailing blank lines so the canonical
    // gap below applies cleanly.
    let first_start = units[0].start;
    for i in 1..first_start {
        if let Some(line) = lines.get(i - 1) {
            result.push(line.to_string());
        }
    }
    while result.last().is_some_and(|l| l.trim().is_empty()) {
        result.pop();
    }
    let had_preamble = !result.is_empty();

    let mut prev_category: Option<usize> = None;
    let total_lines = lines.len();
    for (idx, unit) in units.iter().enumerate() {
        let blanks = if let Some(prev) = prev_category {
            canonical_blank_lines_between(prev, unit.category)
        } else if had_preamble {
            // First class member after file-level comments: one blank line.
            1
        } else {
            0
        };
        for _ in 0..blanks {
            result.push(String::new());
        }

        // Emit this unit's source lines: leading doc block (from `start` to
        // `decl_start`) tightened against the declaration, then the
        // declaration body itself (from `decl_start` up to but not including
        // the next unit's `start`). Trailing blank lines on both sides are
        // stripped; the canonical gap is reinserted on the next iteration.
        let end_excl = units
            .get(idx + 1)
            .map(|u| u.start)
            .unwrap_or(total_lines + 1);

        // Doc block (if any): everything from start to decl_start, blanks
        // between docs and the declaration stripped.
        let mut doc_lines: Vec<String> = Vec::new();
        for i in unit.start..unit.decl_start {
            if let Some(line) = lines.get(i - 1) {
                doc_lines.push(line.to_string());
            }
        }
        while doc_lines.last().is_some_and(|l| l.trim().is_empty()) {
            doc_lines.pop();
        }
        result.extend(doc_lines);

        // Declaration + body.
        let mut decl_lines: Vec<String> = Vec::new();
        for i in unit.decl_start..end_excl {
            if let Some(line) = lines.get(i - 1) {
                decl_lines.push(line.to_string());
            }
        }
        while decl_lines.last().is_some_and(|l| l.trim().is_empty()) {
            decl_lines.pop();
        }
        result.extend(decl_lines);

        prev_category = Some(unit.category);
    }

    // Ensure the file ends with exactly one trailing newline.
    while result.last().is_some_and(|l| l.is_empty()) {
        result.pop();
    }
    result.push(String::new());

    result.join("\n")
}

fn collapse_blank_lines(source: &str) -> String {
    let lines: Vec<&str> = source.split('\n').collect();
    let mut result = Vec::new();
    let mut blank_count = 0;

    for line in &lines {
        if line.trim().is_empty() {
            blank_count += 1;
            if blank_count <= 2 {
                result.push(String::new());
            }
        } else {
            blank_count = 0;
            result.push(line.to_string());
        }
    }

    result.join("\n")
}

fn normalize_boolean_operators(source: &str) -> String {
    // Use the lexer to find && || ! tokens and replace them.
    let mut lexer = crate::lexer::Lexer::new(source);
    let tokens = lexer.tokenize();
    let mut replacements: Vec<(usize, usize, &str)> = Vec::new();

    for token in &tokens {
        match &token.kind {
            crate::token::TokenKind::AmpersandAmpersand => {
                replacements.push((token.span.offset, token.span.length, "and"));
            }
            crate::token::TokenKind::PipePipe => {
                replacements.push((token.span.offset, token.span.length, "or"));
            }
            crate::token::TokenKind::Bang => {
                replacements.push((token.span.offset, token.span.length, "not "));
            }
            _ => {}
        }
    }

    if replacements.is_empty() {
        return source.to_string();
    }

    // Apply from end to start.
    let mut result = source.to_string();
    for (offset, length, new_text) in replacements.into_iter().rev() {
        let end = (offset + length).min(result.len());
        result.replace_range(offset..end, new_text);
    }
    result
}

fn normalize_quotes(source: &str) -> String {
    let mut lexer = crate::lexer::Lexer::new(source);
    let tokens = lexer.tokenize();
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();

    for token in &tokens {
        if let crate::token::TokenKind::String(ref info) = token.kind {
            if info.quote_style == crate::token::QuoteStyle::Single
                && info.prefix == crate::token::StringPrefix::None
                && !info.is_multiline
                && !info.value.contains('"')
            {
                let new_text = format!("\"{}\"", info.value);
                replacements.push((token.span.offset, token.span.length, new_text));
            }
        }
    }

    if replacements.is_empty() {
        return source.to_string();
    }

    let mut result = source.to_string();
    for (offset, length, new_text) in replacements.into_iter().rev() {
        let end = (offset + length).min(result.len());
        result.replace_range(offset..end, &new_text);
    }
    result
}

fn normalize_comment_spacing(source: &str) -> String {
    let mut lexer = crate::lexer::Lexer::new(source);
    let tokens = lexer.tokenize();
    let mut insertions: Vec<(usize, &str)> = Vec::new();

    for token in &tokens {
        match &token.kind {
            crate::token::TokenKind::Comment(content) => {
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
                    insertions.push((token.span.offset + 1, " "));
                }
            }
            crate::token::TokenKind::DocComment(content) => {
                // Don't add space if content starts with '#' (separator lines like #####)
                // or is empty, or already starts with space.
                if !content.is_empty() && !content.starts_with(' ') && !content.starts_with('#') {
                    insertions.push((token.span.offset + 2, " "));
                }
            }
            _ => {}
        }
    }

    if insertions.is_empty() {
        return source.to_string();
    }

    let mut result = source.to_string();
    for (offset, text) in insertions.into_iter().rev() {
        result.insert_str(offset, text);
    }
    result
}

fn normalize_float_literals(source: &str) -> String {
    let mut lexer = crate::lexer::Lexer::new(source);
    let tokens = lexer.tokenize();
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();

    for token in &tokens {
        if let crate::token::TokenKind::Float(_) = &token.kind {
            let text = &token.text;
            if text.starts_with('.') {
                replacements.push((token.span.offset, token.span.length, format!("0{}", text)));
            } else if text.ends_with('.') {
                replacements.push((token.span.offset, token.span.length, format!("{}0", text)));
            }
        }
    }

    if replacements.is_empty() {
        return source.to_string();
    }

    let mut result = source.to_string();
    for (offset, length, new_text) in replacements.into_iter().rev() {
        let end = (offset + length).min(result.len());
        result.replace_range(offset..end, &new_text);
    }
    result
}

fn normalize_hex_literals(source: &str) -> String {
    let mut lexer = crate::lexer::Lexer::new(source);
    let tokens = lexer.tokenize();
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();

    for token in &tokens {
        if let crate::token::TokenKind::Integer(_) = &token.kind {
            let text = &token.text;
            if (text.starts_with("0x") || text.starts_with("0X"))
                && text[2..]
                    .chars()
                    .any(|c| c.is_ascii_uppercase() && c != '_')
            {
                let fixed = format!("0x{}", text[2..].to_lowercase());
                replacements.push((token.span.offset, token.span.length, fixed));
            }
        }
    }

    if replacements.is_empty() {
        return source.to_string();
    }

    let mut result = source.to_string();
    for (offset, length, new_text) in replacements.into_iter().rev() {
        let end = (offset + length).min(result.len());
        result.replace_range(offset..end, &new_text);
    }
    result
}

fn ensure_trailing_newline(source: &str) -> String {
    if source.is_empty() {
        return String::new();
    }
    // Remove trailing newlines, then add exactly one.
    let trimmed = source.trim_end_matches('\n');
    format!("{}\n", trimmed)
}

fn lint_then_fix(source: &str, config: &Config) -> String {
    // Run lint rules and apply all safe fixes.
    let diagnostics = linter::lint_source(source, "<fmt>", config);
    fixer::apply_fixes(source, &diagnostics, true)
}

/// Break lines that exceed max_line_length by wrapping at commas inside
/// parentheses, brackets, or braces. Only breaks "obvious" cases where
/// implicit line continuation inside delimiters is safe in GDScript.
///
/// Does NOT attempt to break:
/// - Lines without delimiters (operator expressions, long strings)
/// - Lines where the content inside delimiters has only one item
/// - Lines where breaking wouldn't reduce the length below the limit
fn break_long_lines(source: &str, config: &Config) -> String {
    const TAB_WIDTH: usize = 4;
    let max_len = config.max_line_length;
    let lines: Vec<&str> = source.split('\n').collect();
    let mut result: Vec<String> = Vec::new();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let visual_len = visual_line_len(line, TAB_WIDTH);

        if visual_len <= max_len {
            result.push(line.to_string());
            i += 1;
            continue;
        }

        // Try multiple strategies to break this line.
        if let Some(broken) = try_break_line(line, config) {
            result.extend(broken);
        } else if let Some(broken) = try_break_comment(line, max_len) {
            result.extend(broken);
        } else if let Some(broken) = try_break_at_bool_operators(line, config) {
            result.extend(broken);
        } else {
            result.push(line.to_string());
        }
        i += 1;
    }

    result.join("\n")
}

fn visual_line_len(line: &str, tab_width: usize) -> usize {
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

/// Try to break a long line at commas inside the first top-level delimiter.
/// Returns None if the line can't be meaningfully broken.
fn try_break_line(line: &str, config: &Config) -> Option<Vec<String>> {
    let indent: String = line
        .chars()
        .take_while(|c| *c == '\t' || *c == ' ')
        .collect();
    let content = &line[indent.len()..];

    // Find the first opening delimiter at the top level.
    let (open_byte, open_ch, close_ch) = find_first_delimiter(content)?;

    // Find its matching close.
    let close_byte = find_matching_close(content, open_byte, open_ch, close_ch)?;

    // Get inner content and split by top-level commas.
    let inner = &content[open_byte + 1..close_byte];
    let items = split_top_level_commas(inner);

    // Need at least 2 items to make breaking worthwhile.
    if items.len() < 2 {
        return None;
    }

    let prefix = &content[..open_byte + 1]; // e.g., "func foo("
    let suffix = &content[close_byte..]; // e.g., ") -> void:"

    // GDScript does not allow line-wrapped typed-collection brackets
    // (`Dictionary[int, X]`, `Array[X]`) or subscripts (`arr[0]`). The parser
    // requires the type spec / index expression to be on one line. So when
    // the bracket we're wrapping is one of those (the prior char is an
    // identifier / `]` / `)`), refuse to break this `[...]` and let the
    // long-line warning stand.
    if open_ch == '[' {
        let prior = content[..open_byte]
            .chars()
            .rev()
            .find(|c| !c.is_whitespace());
        if matches!(prior, Some(c) if c.is_ascii_alphanumeric() || c == '_' || c == ']' || c == ')')
        {
            return None;
        }
    }
    let allows_trailing_comma = true;

    // Build the item indent: one tab deeper than the line's indentation.
    let item_indent = if config.use_tabs {
        format!("{}\t", indent)
    } else {
        format!("{}    ", indent)
    };

    let mut broken: Vec<String> = Vec::new();
    broken.push(format!("{}{}", indent, prefix));
    let last = items.len() - 1;
    for (idx, item) in items.iter().enumerate() {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        let is_last = idx == last;
        if is_last && !allows_trailing_comma {
            broken.push(format!("{}{}", item_indent, trimmed));
        } else {
            broken.push(format!("{}{},", item_indent, trimmed));
        }
    }
    broken.push(format!("{}{}", indent, suffix));

    Some(broken)
}

/// Track whether the scanner is currently inside a string literal, including
/// raw / `&` / `^` prefixed strings (no escape processing) and respecting
/// the prior char so `r"foo\"bar"` terminates at the literal `"`.
struct StringScanner {
    in_string: bool,
    quote: char,
    escaped: bool,
    raw: bool,
}

impl StringScanner {
    fn new() -> Self {
        Self {
            in_string: false,
            quote: '"',
            escaped: false,
            raw: false,
        }
    }
    /// Step the scanner one character forward. Returns true if `ch` should
    /// be ignored (we're inside a string).
    fn step(&mut self, prior: Option<char>, ch: char) -> bool {
        if self.in_string {
            if !self.raw {
                if self.escaped {
                    self.escaped = false;
                    return true;
                }
                if ch == '\\' {
                    self.escaped = true;
                    return true;
                }
            }
            if ch == self.quote {
                self.in_string = false;
                self.raw = false;
            }
            return true;
        }
        if ch == '"' || ch == '\'' {
            self.in_string = true;
            self.quote = ch;
            self.escaped = false;
            self.raw = matches!(prior, Some('r' | 'R'));
            return true;
        }
        false
    }
}

/// Find the first `(`, `[`, or `{` in content, respecting strings.
fn find_first_delimiter(content: &str) -> Option<(usize, char, char)> {
    let mut scanner = StringScanner::new();
    let chars: Vec<(usize, char)> = content.char_indices().collect();

    for window in 0..chars.len() {
        let (i, ch) = chars[window];
        let prior = if window == 0 {
            None
        } else {
            Some(chars[window - 1].1)
        };
        if scanner.step(prior, ch) {
            continue;
        }
        if ch == '#' {
            return None; // Rest is a comment
        }
        match ch {
            '(' => return Some((i, '(', ')')),
            '[' => return Some((i, '[', ']')),
            '{' => return Some((i, '{', '}')),
            _ => {}
        }
    }
    None
}

/// Find the byte position of the matching close delimiter.
fn find_matching_close(content: &str, open_pos: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 1;
    let mut scanner = StringScanner::new();
    let mut prior = Some(content[..open_pos + 1].chars().next_back().unwrap_or(' '));

    for (i, ch) in content[open_pos + 1..].char_indices() {
        let abs_pos = open_pos + 1 + i;
        if scanner.step(prior, ch) {
            prior = Some(ch);
            continue;
        }
        prior = Some(ch);
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth -= 1;
            if depth == 0 {
                return Some(abs_pos);
            }
        }
    }
    None
}

/// Split content by commas at the top level (not inside nested delimiters or strings).
fn split_top_level_commas(content: &str) -> Vec<String> {
    let mut items: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    let mut scanner = StringScanner::new();
    let mut prior: Option<char> = None;

    for ch in content.chars() {
        let in_string = scanner.step(prior, ch);
        prior = Some(ch);
        if in_string {
            current.push(ch);
            continue;
        }
        match ch {
            '(' | '[' | '{' => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                items.push(current.clone());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        items.push(current);
    }
    items
}

/// Break a long comment at word boundaries.
fn try_break_comment(line: &str, max_len: usize) -> Option<Vec<String>> {
    const TAB_WIDTH: usize = 4;
    let indent: String = line
        .chars()
        .take_while(|c| *c == '\t' || *c == ' ')
        .collect();
    let content = &line[indent.len()..];

    // Must be a comment line (# or ##).
    if !content.starts_with('#') {
        return None;
    }

    // Extract the comment prefix (# or ##) and the text. A bare `#`, a `#!`
    // shebang, or any non-comment line yields None (the `?` on the `# `
    // branch), so we never try to wrap those.
    let (prefix, text) = if let Some(rest) = content.strip_prefix("## ") {
        ("## ", rest)
    } else {
        ("# ", content.strip_prefix("# ")?)
    };

    let prefix_visual_len = visual_line_len(&format!("{}{}", indent, prefix), TAB_WIDTH);

    // Split text into words and wrap.
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return None;
    }

    let mut lines_out: Vec<String> = Vec::new();
    let mut current_line = format!("{}{}", indent, prefix);
    let mut current_visual = prefix_visual_len;

    for word in &words {
        let word_len = word.len();
        if current_visual > prefix_visual_len && current_visual + 1 + word_len > max_len {
            // Start a new line.
            lines_out.push(current_line);
            current_line = format!("{}{}{}", indent, prefix, word);
            current_visual = prefix_visual_len + word_len;
        } else {
            if current_visual > prefix_visual_len {
                current_line.push(' ');
                current_visual += 1;
            }
            current_line.push_str(word);
            current_visual += word_len;
        }
    }
    lines_out.push(current_line);

    // Only return if we actually split into multiple lines.
    if lines_out.len() < 2 {
        return None;
    }
    Some(lines_out)
}

/// Break long `if`/`elif`/`while` conditions at `and`/`or` operators.
fn try_break_at_bool_operators(line: &str, config: &Config) -> Option<Vec<String>> {
    let indent: String = line
        .chars()
        .take_while(|c| *c == '\t' || *c == ' ')
        .collect();
    let content = &line[indent.len()..];

    // Must start with if/elif/while.
    let keyword = if content.starts_with("if ") {
        "if"
    } else if content.starts_with("elif ") {
        "elif"
    } else if content.starts_with("while ") {
        "while"
    } else {
        return None;
    };

    // Extract the condition (between keyword and the statement colon).
    // A naive `rfind(':')` would mis-pick a `:` inside a dictionary literal,
    // a slice, or a string in the condition. The statement colon is the
    // last colon at bracket-depth 0 that is not inside a string.
    let after_kw = &content[keyword.len() + 1..];
    let colon_pos = find_statement_colon(after_kw)?;
    let condition = after_kw[..colon_pos].trim();
    let after_colon = &after_kw[colon_pos..]; // ":" or ": # comment"

    // Split condition at " and " / " or " (top-level only, not inside strings/parens).
    let parts = split_at_bool_operators(condition);
    if parts.len() < 2 {
        return None;
    }

    // Wrap condition in parentheses with each part on its own line.
    let cont_indent = if config.use_tabs {
        format!("{}\t\t", indent)
    } else {
        format!("{}        ", indent)
    };

    let mut broken: Vec<String> = Vec::new();
    broken.push(format!("{}{} ({}", indent, keyword, parts[0].trim()));
    for part in &parts[1..] {
        broken.push(format!("{}{}", cont_indent, part.trim()));
    }
    // Close with ): on the continuation indent level
    let last = broken.last_mut().unwrap();
    *last = format!("{}){}", last, after_colon);

    Some(broken)
}

/// Find the byte offset of the statement-terminating `:` in `text`: the last
/// colon that sits at bracket-depth 0 and outside any string literal.
/// Returns None if there is no such colon (e.g. the `:` is only inside a
/// dict literal). A `:=` (inferred-assignment) colon is skipped.
fn find_statement_colon(text: &str) -> Option<usize> {
    let mut scanner = StringScanner::new();
    let mut depth: i32 = 0;
    let mut found: Option<usize> = None;
    let mut prior: Option<char> = None;
    let bytes = text.as_bytes();
    for (i, ch) in text.char_indices() {
        if scanner.step(prior, ch) {
            prior = Some(ch);
            continue;
        }
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            '#' => break, // trailing comment, stop
            ':' if depth == 0 => {
                // Skip `:=` (typed-inference assignment).
                if bytes.get(i + 1) != Some(&b'=') {
                    found = Some(i);
                }
            }
            _ => {}
        }
        prior = Some(ch);
    }
    found
}

/// Split a condition string at top-level `and` / `or` operators.
fn split_at_bool_operators(condition: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    let mut in_string = false;
    let mut string_char = '"';
    let mut escaped = false;
    let chars: Vec<char> = condition.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if in_string {
            current.push(chars[i]);
            if escaped {
                escaped = false;
                i += 1;
                continue;
            }
            if chars[i] == '\\' {
                escaped = true;
                i += 1;
                continue;
            }
            if chars[i] == string_char {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if chars[i] == '"' || chars[i] == '\'' {
            in_string = true;
            string_char = chars[i];
            current.push(chars[i]);
            i += 1;
            continue;
        }
        match chars[i] {
            '(' | '[' | '{' => {
                depth += 1;
                current.push(chars[i]);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                current.push(chars[i]);
            }
            _ if depth == 0 => {
                // Check for " and " or " or " at this position.
                let remaining = &condition[i..];
                if remaining.starts_with(" and ") {
                    parts.push(current.clone());
                    current = String::from("and ");
                    i += 5; // skip " and "
                    continue;
                } else if remaining.starts_with(" or ") {
                    parts.push(current.clone());
                    current = String::from("or ");
                    i += 4; // skip " or "
                    continue;
                }
                current.push(chars[i]);
            }
            _ => current.push(chars[i]),
        }
        i += 1;
    }
    if !current.trim().is_empty() {
        parts.push(current);
    }
    parts
}

/// Safety wrapper around `reorder_class_members`. Skips the reorder when it
/// would risk changing program semantics, and verifies the output is
/// structurally equivalent to the input. Returns the original source if the
/// reorder would be unsafe or appears to have lost/mutated members.
fn safe_reorder_class_members(source: &str) -> String {
    let original_members = parse_members(source);

    // Guard 1: refuse to reorder when a module-level var initializer references
    // another module-level identifier in the same file. GDScript evaluates
    // these initializers in source order, so reordering can change which
    // value gets assigned. This is the class of failure that broke
    // ai-battleground when running `gdstyle fmt`.
    if has_cross_referencing_initializer(source, &original_members) {
        return source.to_string();
    }

    let reordered = reorder_class_members(source);
    if reordered == source {
        return reordered;
    }

    // Guard 2: re-parse the output and verify the set of declared symbols is
    // unchanged. If the formatter accidentally dropped, duplicated, or
    // corrupted any class member, fall back to the original source rather
    // than silently breaking the file at runtime.
    let reordered_members = parse_members(&reordered);
    if !same_member_signatures(&original_members, &reordered_members) {
        return source.to_string();
    }
    reordered
}

fn parse_members(source: &str) -> Vec<ClassMember> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize();
    Parser::new(&tokens).parse()
}

/// True if any module-level `var`/`const`/`static var` initializer references
/// the name of another module-level declaration. We can't safely reorder such
/// files because GDScript runs the initializers in source order.
///
/// AST-driven so we don't have to track Indent/Dedent depth across the whole
/// file. We only inspect the source bytes between the module-level
/// declaration's span and the next member's span; this slice is guaranteed
/// to be the initializer expression, never function bodies.
fn has_cross_referencing_initializer(source: &str, members: &[ClassMember]) -> bool {
    use std::collections::HashSet;

    let module_names: HashSet<&str> = members
        .iter()
        .filter_map(|m| match m {
            ClassMember::Variable { name, .. }
            | ClassMember::StaticVariable { name, .. }
            | ClassMember::Constant { name, .. } => {
                if name.is_empty() {
                    None
                } else {
                    Some(name.as_str())
                }
            }
            _ => None,
        })
        .collect();

    if module_names.len() < 2 {
        return false;
    }

    let lines: Vec<&str> = source.split('\n').collect();
    let line_start = |line_1based: usize| -> usize {
        lines[..line_1based.saturating_sub(1)]
            .iter()
            .map(|l| l.len() + 1)
            .sum()
    };

    for (idx, member) in members.iter().enumerate() {
        let (own_name, member_line) = match member {
            ClassMember::Variable { name, span, .. }
            | ClassMember::StaticVariable { name, span, .. }
            | ClassMember::Constant { name, span, .. } => (name.as_str(), span.line),
            _ => continue,
        };

        let start = line_start(member_line);
        let end = members
            .get(idx + 1)
            .map(|m| line_start(m.span().line))
            .unwrap_or(source.len());
        let slice = &source[start..end.min(source.len())];

        // Lex just this slice and look for any identifier matching another
        // module-level declaration (excluding self), provided it's not a
        // member-access (preceded by `.`).
        let mut lexer = Lexer::new(slice);
        let tokens = lexer.tokenize();
        for (i, tok) in tokens.iter().enumerate() {
            let TokenKind::Identifier(ref n) = tok.kind else {
                continue;
            };
            if n.as_str() == own_name {
                continue;
            }
            if !module_names.contains(n.as_str()) {
                continue;
            }
            let prev_is_dot = i > 0 && matches!(tokens[i - 1].kind, TokenKind::Dot);
            if !prev_is_dot {
                return true;
            }
        }
    }
    false
}

/// Compare two member lists by the names they declare AND their parent
/// (outer or inner-class) context. We don't compare positions, only the
/// *set* of declared symbols, qualified by the inner class they live in.
/// This catches both lost members and members that got relocated across
/// class boundaries (e.g. an inner class's var being lifted to top-level).
///
/// Each entry is a `(scope_path, kind_tag, name)` triple: borrowed slices
/// from the AST plus a static `&str` kind tag. No `format!()` allocations
/// per member.
fn same_member_signatures(a: &[ClassMember], b: &[ClassMember]) -> bool {
    type Sig<'a> = (Vec<&'a str>, &'static str, &'a str);
    fn collect<'a>(members: &'a [ClassMember], scope: &[&'a str], out: &mut Vec<Sig<'a>>) {
        for m in members {
            let (kind_tag, name): (&'static str, &str) = match m {
                ClassMember::Function { name, .. } => ("func", name.as_str()),
                ClassMember::Variable { name, .. } => ("var", name.as_str()),
                ClassMember::StaticVariable { name, .. } => ("svar", name.as_str()),
                ClassMember::Constant { name, .. } => ("const", name.as_str()),
                ClassMember::Signal { name, .. } => ("signal", name.as_str()),
                ClassMember::Enum {
                    name, members: ems, ..
                } => {
                    let n = name.as_deref().unwrap_or("");
                    out.push((scope.to_vec(), "enum", n));
                    // Enum members: prefix the enum name into the scope so
                    // moving an enum to a different class is detected.
                    let mut enum_scope = scope.to_vec();
                    enum_scope.push(n);
                    for em in ems {
                        out.push((enum_scope.clone(), "enum_member", em.name.as_str()));
                    }
                    continue;
                }
                ClassMember::ClassNameDecl { name, .. } => ("class", name.as_str()),
                ClassMember::ExtendsDecl { base, .. } => ("extends", base.as_str()),
                ClassMember::InnerClass {
                    name,
                    members: inner,
                    ..
                } => {
                    out.push((scope.to_vec(), "inner", name.as_str()));
                    let mut inner_scope = scope.to_vec();
                    inner_scope.push(name.as_str());
                    collect(inner, &inner_scope, out);
                    continue;
                }
                _ => continue,
            };
            out.push((scope.to_vec(), kind_tag, name));
        }
    }

    let mut a_sigs: Vec<Sig> = Vec::new();
    let mut b_sigs: Vec<Sig> = Vec::new();
    collect(a, &[], &mut a_sigs);
    collect(b, &[], &mut b_sigs);
    a_sigs.sort();
    b_sigs.sort();
    a_sigs == b_sigs
}

/// A reorderable class member: the source span it occupies plus its ordering
/// category and original position (for a stable sort).
#[derive(Clone, Copy)]
struct Block {
    /// 0-indexed first line, including any attached comments / annotations.
    start: usize,
    /// 0-indexed last line.
    end: usize,
    /// Ordering category. See `ClassMember::ordering_category`.
    category: usize,
    /// Position in the original member list, for stable sorting.
    original_index: usize,
}

/// One member's anchor: `(0-indexed decl line, ordering category, original
/// index)`.
type Anchor = (usize, usize, usize);

/// Collect a top-level anchor for each real class member, merging anchors that
/// share a source line (e.g. `class_name X extends Y`). Members the parser
/// surfaced from inside an inner-class body (indented decl lines) are skipped
/// so they don't get torn away from their `class X:` header.
fn collect_member_anchors(members: &[ClassMember], source_lines: &[&str]) -> Vec<Anchor> {
    let is_top_level_line = |line: usize| -> bool {
        match source_lines.get(line) {
            None => true,
            Some(text) if text.trim().is_empty() => true,
            Some(text) => !text.starts_with(['\t', ' ']),
        }
    };

    let mut anchors: Vec<Anchor> = Vec::new();
    for (orig_idx, member) in members.iter().enumerate() {
        if matches!(
            member,
            ClassMember::DocComment { .. }
                | ClassMember::Comment { .. }
                | ClassMember::BlankLine { .. }
        ) {
            continue;
        }
        let line = member.span().line.saturating_sub(1); // 0-indexed
        if !is_top_level_line(line) {
            continue;
        }
        anchors.push((line, member.ordering_category(), orig_idx));
    }

    // Merge same-line anchors, keeping the lowest category.
    let mut merged: Vec<Anchor> = Vec::new();
    for &anchor in &anchors {
        if let Some(last) = merged.last_mut() {
            if last.0 == anchor.0 {
                last.1 = last.1.min(anchor.1);
                continue;
            }
        }
        merged.push(anchor);
    }
    merged
}

/// For each anchor, walk backward from its declaration line to find attached
/// comments / annotations: those belong to the member and move with it.
fn compute_attached_starts(merged: &[Anchor], lines: &[&str]) -> Vec<usize> {
    let mut attached_starts: Vec<usize> = Vec::new();
    for (i, &(decl_line, _, _)) in merged.iter().enumerate() {
        let prev_boundary = if i > 0 { merged[i - 1].0 + 1 } else { 0 };
        let mut start = decl_line;
        let mut j = decl_line;
        while j > prev_boundary {
            j -= 1;
            let content = if j < lines.len() { lines[j].trim() } else { "" };
            if content.starts_with('#') || content.starts_with('@') {
                start = j;
            } else if content.is_empty() {
                // Blank line: keep looking only if a comment sits directly above.
                let above = if j > prev_boundary && j - 1 < lines.len() {
                    lines[j - 1].trim()
                } else {
                    ""
                };
                if !above.starts_with('#') && !above.starts_with('@') {
                    break;
                }
            } else {
                break;
            }
        }
        attached_starts.push(start);
    }
    attached_starts
}

/// Reorder class members to match the canonical GDScript style guide order.
///
/// Instead of using `end_line()` from the parser (which is unreliable for
/// multi-line expressions), this determines block boundaries by the START
/// of the next member. Everything between two member declarations belongs
/// to the first member: function bodies, multi-line expressions, inner
/// class bodies are all captured correctly.
fn reorder_class_members(source: &str) -> String {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize();
    let members = Parser::new(&tokens).parse();

    let lines: Vec<&str> = source.split('\n').collect();
    let merged = collect_member_anchors(&members, &lines);

    if merged.is_empty() {
        return source.to_string();
    }

    // Check if already ordered: skip full reorder if so.
    let already_ordered = merged.windows(2).all(|w| w[0].1 <= w[1].1);
    if already_ordered {
        // Even when ordered, fix ## doc comments appearing before class_name/extends.
        return move_class_decl_before_doc_comments(source);
    }

    let attached_starts = compute_attached_starts(&merged, &lines);

    // A member carrying a `# gdstyle:ignore=order/class-member-order` directive
    // opts out of reordering: it is pinned to its source position while the
    // rest normalise around it. We look for the directive anywhere in the
    // member's header (its attached comments/annotations through its
    // declaration line), so it can sit above the annotations too.
    let suppressions = crate::linter::Suppressions::parse(source);
    let pinned: Vec<bool> = merged
        .iter()
        .enumerate()
        .map(|(i, &(decl_line, _, _))| {
            suppressions.suppresses_member(
                attached_starts[i] + 1,
                decl_line + 1,
                "order/class-member-order",
            )
        })
        .collect();

    // Build blocks in source order. Each block spans from its attached_start to
    // just before the next block's attached_start (or end of file for the last).
    let mut blocks: Vec<Block> = Vec::new();
    for (i, &(_, cat, orig)) in merged.iter().enumerate() {
        let start = attached_starts[i];
        let end = if i + 1 < merged.len() {
            attached_starts[i + 1].saturating_sub(1)
        } else {
            lines.len().saturating_sub(1)
        };
        blocks.push(Block {
            start,
            end,
            category: cat,
            original_index: orig,
        });
    }

    // Fixed-point stable sort: pinned blocks keep their source position; the
    // remaining blocks are stably sorted by (category, original_index) and slot
    // into the free positions in order. With no pins this reduces to the plain
    // category sort.
    let mut movable = blocks
        .iter()
        .zip(&pinned)
        .filter(|&(_, &p)| !p)
        .map(|(b, _)| *b)
        .collect::<Vec<Block>>();
    movable.sort_by(|a, b| {
        a.category
            .cmp(&b.category)
            .then(a.original_index.cmp(&b.original_index))
    });
    let mut movable = movable.into_iter();
    let ordered: Vec<Block> = blocks
        .iter()
        .zip(&pinned)
        .map(|(b, &p)| {
            if p {
                *b
            } else {
                // Exactly one movable block per non-pinned slot by construction.
                movable
                    .next()
                    .expect("movable block for each non-pinned slot")
            }
        })
        .collect();

    emit_reordered_blocks(&ordered, &lines, &attached_starts)
}

/// Reconstruct the source with `blocks` in their new (sorted) order,
/// normalising blank-line separators and deferring class doc comments to
/// after the `class_name`/`extends` declarations.
fn emit_reordered_blocks(blocks: &[Block], lines: &[&str], attached_starts: &[usize]) -> String {
    let mut result: Vec<String> = Vec::new();

    // Lines before the first block (e.g., initial blank lines, shebang).
    let first_block_start = attached_starts.iter().copied().min().unwrap_or(0);
    for line in &lines[..first_block_start] {
        result.push(line.to_string());
    }

    // Emit blocks in sorted order with normalized separators.
    let mut prev_category: Option<usize> = None;
    let mut deferred_doc_lines: Vec<String> = Vec::new();
    for block in blocks {
        // Collect this block's lines, stripping trailing blank lines.
        let block_end = block.end.min(lines.len().saturating_sub(1));
        let mut block_lines: Vec<&str> = lines[block.start..=block_end].to_vec();
        while block_lines.last().is_some_and(|l| l.trim().is_empty()) {
            block_lines.pop();
        }
        if block_lines.is_empty() {
            continue;
        }

        // For class_name/extends blocks (categories 1-2): if the block has
        // leading ## doc comments, defer the docs until after all category 1-2
        // blocks. The canonical order is class_name → extends → ## class docstring.
        if block.category <= 2 {
            let doc_end = block_lines
                .iter()
                .position(|l| {
                    let t = l.trim();
                    !t.starts_with("##") && !t.is_empty()
                })
                .unwrap_or(0);
            if doc_end > 0 {
                let decl_lines: Vec<&str> = block_lines[doc_end..].to_vec();

                // Add separator if needed.
                if prev_category.is_some() {
                    while result.last().is_some_and(|l| l.trim().is_empty()) {
                        result.pop();
                    }
                    result.push(String::new());
                    result.push(String::new());
                }

                for line in &decl_lines {
                    result.push(line.to_string());
                }

                // Stash the doc comment lines; they'll be emitted after the
                // last class_name/extends block.
                deferred_doc_lines.extend(block_lines[..doc_end].iter().map(|l| l.to_string()));
                prev_category = Some(block.category);
                continue;
            }
        }

        // If we're leaving the class_name/extends range, flush deferred doc comments.
        if block.category > 2 && !deferred_doc_lines.is_empty() {
            while result.last().is_some_and(|l| l.trim().is_empty()) {
                result.pop();
            }
            result.push(String::new());
            result.push(String::new());
            result.append(&mut deferred_doc_lines);
        }

        // Add separator between blocks.
        if prev_category.is_some() {
            // Strip trailing blank lines from result before adding separator.
            while result.last().is_some_and(|l| l.trim().is_empty()) {
                result.pop();
            }
            result.push(String::new());
            result.push(String::new());
        }

        for line in &block_lines {
            result.push(line.to_string());
        }
        prev_category = Some(block.category);
    }

    // Flush any remaining deferred doc comments.
    if !deferred_doc_lines.is_empty() {
        while result.last().is_some_and(|l| l.trim().is_empty()) {
            result.pop();
        }
        result.push(String::new());
        result.push(String::new());
        result.extend(deferred_doc_lines);
    }

    // Ensure file ends with exactly one newline.
    while result.last().is_some_and(|l| l.is_empty()) {
        result.pop();
    }
    result.push(String::new());

    result.join("\n")
}

/// If ## doc comments appear before the first class_name/extends, move the
/// declaration above the doc comments. Returns source unchanged if not needed.
fn move_class_decl_before_doc_comments(source: &str) -> String {
    let lines: Vec<&str> = source.split('\n').collect();

    // Find the first ## doc comment line.
    let doc_start = lines.iter().position(|l| l.trim().starts_with("##"));
    if doc_start.is_none() {
        return source.to_string();
    }
    let doc_start = doc_start.unwrap();

    // Find where the doc block ends (first non-## non-blank line after doc_start).
    let mut doc_end = doc_start;
    for (i, line) in lines.iter().enumerate().skip(doc_start) {
        let t = line.trim();
        if t.starts_with("##") || t.is_empty() {
            doc_end = i;
        } else {
            break;
        }
    }

    // Check if the line after the doc block is class_name or extends.
    let decl_start = doc_end + 1;
    if decl_start >= lines.len() {
        return source.to_string();
    }
    let decl_trimmed = lines[decl_start].trim();
    if !decl_trimmed.starts_with("class_name") && !decl_trimmed.starts_with("extends") {
        return source.to_string();
    }

    // Find how many consecutive declaration lines (class_name, extends).
    let mut decl_end = decl_start;
    for (i, line) in lines.iter().enumerate().skip(decl_start) {
        let t = line.trim();
        if t.starts_with("class_name") || t.starts_with("extends") {
            decl_end = i;
        } else {
            break;
        }
    }

    // Reconstruct: lines before docs, then decl lines, blank, doc lines, rest.
    let mut result: Vec<String> = Vec::new();
    for line in &lines[..doc_start] {
        result.push(line.to_string());
    }
    for line in &lines[decl_start..=decl_end] {
        result.push(line.to_string());
    }
    result.push(String::new());
    result.push(String::new());
    for line in &lines[doc_start..=doc_end] {
        if !line.trim().is_empty() {
            result.push(line.to_string());
        }
    }
    for line in &lines[(decl_end + 1)..] {
        result.push(line.to_string());
    }

    result.join("\n")
}

/// Find inner class blocks in the source and recursively reorder their members.
fn reorder_inner_classes(source: &str) -> String {
    let lines: Vec<&str> = source.split('\n').collect();
    let mut result: Vec<String> = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();
        // Detect inner class declaration: starts with "class " at indent > 0.
        let indent: String = lines[i]
            .chars()
            .take_while(|c| *c == '\t' || *c == ' ')
            .collect();
        if trimmed.starts_with("class ") && trimmed.ends_with(':') {
            // Found an inner class. Collect its header and indented body.
            result.push(lines[i].to_string());
            let class_indent_len = indent.len();
            i += 1;

            // Collect all lines that are more indented than the class declaration
            // (the class body) or blank lines within it.
            let body_start = i;
            while i < lines.len() {
                let line = lines[i];
                if line.trim().is_empty() {
                    // Blank line: include if the next non-blank line is still in the body.
                    let mut next_nonblank = i + 1;
                    while next_nonblank < lines.len() && lines[next_nonblank].trim().is_empty() {
                        next_nonblank += 1;
                    }
                    if next_nonblank < lines.len() {
                        let next_indent: String = lines[next_nonblank]
                            .chars()
                            .take_while(|c| *c == '\t' || *c == ' ')
                            .collect();
                        if next_indent.len() > class_indent_len {
                            i += 1;
                            continue;
                        }
                    }
                    break;
                }
                let line_indent: String = line
                    .chars()
                    .take_while(|c| *c == '\t' || *c == ' ')
                    .collect();
                if line_indent.len() <= class_indent_len {
                    break;
                }
                i += 1;
            }
            let body_end = i;

            if body_start < body_end {
                // Extract body, dedent by one level, reorder, re-indent.
                let body_lines: Vec<String> = lines[body_start..body_end]
                    .iter()
                    .map(|l| {
                        if l.trim().is_empty() {
                            String::new()
                        } else if l.starts_with(&format!("{}\t", indent)) {
                            l[indent.len() + 1..].to_string()
                        } else if l.starts_with(&format!("{}    ", indent)) {
                            l[indent.len() + 4..].to_string()
                        } else if l.len() > indent.len() {
                            l[indent.len()..].to_string()
                        } else {
                            l.to_string()
                        }
                    })
                    .collect();

                let dedented_body = body_lines.join("\n");
                // Use the safe wrapper so inner classes get the same guards
                // as the file-level reorder (no member loss, no reordering
                // when initialisers cross-reference).
                let reordered = safe_reorder_class_members(&dedented_body);

                // Re-indent and add to result.
                let member_indent = format!("{}\t", indent);
                for line in reordered.split('\n') {
                    if line.trim().is_empty() {
                        result.push(String::new());
                    } else {
                        result.push(format!("{}{}", member_indent, line));
                    }
                }
                // Remove trailing blank line added by reorder's ensure-newline.
                while result.last().is_some_and(|l| l.is_empty()) {
                    result.pop();
                }
            }
        } else {
            result.push(lines[i].to_string());
            i += 1;
        }
    }

    result.join("\n")
}

/// Format a file on disk. Returns Ok(true) if the file was changed.
pub fn format_file(path: &std::path::Path, config: &Config) -> Result<bool, String> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {}", path.display(), e))?;
    let formatted = format_source(&source, config);
    if formatted == source {
        return Ok(false);
    }
    std::fs::write(path, &formatted)
        .map_err(|e| format!("cannot write {}: {}", path.display(), e))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_trailing_whitespace() {
        let source = "var x = 5   \nvar y = 10\t\t\n";
        let result = strip_trailing_whitespace(source);
        assert_eq!(result, "var x = 5\nvar y = 10\n");
    }

    #[test]
    fn find_statement_colon_skips_dict_and_string_colons() {
        // The statement colon is the last depth-0 colon outside strings.
        assert_eq!(find_statement_colon("a == {\"k\": 1}:"), Some(13));
        assert_eq!(find_statement_colon("x == \"a: b\":"), Some(11));
        assert_eq!(find_statement_colon("arr[1:2]:"), Some(8));
        // `:=` is not a statement colon.
        assert_eq!(find_statement_colon("y := 3"), None);
        // No statement colon at all (colon only inside the dict).
        assert_eq!(find_statement_colon("{\"k\": 1}"), None);
    }

    #[test]
    fn break_at_bool_operators_keeps_dict_literal_intact() {
        let config = Config {
            max_line_length: 60,
            ..Config::default()
        };
        let line = "\tif lookup == {\"key\": 1} and condition_two and condition_three:";
        let broken = try_break_at_bool_operators(line, &config).expect("should break");
        // The dict literal must survive on the first line, unsplit.
        assert!(
            broken[0].contains("{\"key\": 1}"),
            "dict literal must stay intact, got: {:?}",
            broken
        );
        // The last line must end with the statement colon.
        assert!(
            broken.last().unwrap().trim_end().ends_with("):"),
            "must close with `):`, got: {:?}",
            broken
        );
    }

    #[test]
    fn test_collapse_blank_lines() {
        let source = "a\n\n\n\n\nb\n";
        let result = collapse_blank_lines(source);
        assert_eq!(result, "a\n\n\nb\n");
    }

    #[test]
    fn test_normalize_boolean_operators() {
        let source = "if a && b || !c:\n\tpass\n";
        let result = normalize_boolean_operators(source);
        assert!(result.contains("and"));
        assert!(result.contains("or"));
        assert!(result.contains("not "));
        assert!(!result.contains("&&"));
        assert!(!result.contains("||"));
    }

    #[test]
    fn test_normalize_quotes() {
        let source = "var x = 'hello'\n";
        let result = normalize_quotes(source);
        assert_eq!(result, "var x = \"hello\"\n");
    }

    #[test]
    fn test_normalize_quotes_preserves_double_inside() {
        let source = "var x = 'he said \"hi\"'\n";
        let result = normalize_quotes(source);
        assert_eq!(result, source); // unchanged
    }

    #[test]
    fn test_ensure_trailing_newline() {
        assert_eq!(ensure_trailing_newline("hello"), "hello\n");
        assert_eq!(ensure_trailing_newline("hello\n"), "hello\n");
        assert_eq!(ensure_trailing_newline("hello\n\n\n"), "hello\n");
    }

    #[test]
    fn test_normalize_hex() {
        let source = "var x = 0xFF\n";
        let result = normalize_hex_literals(source);
        assert_eq!(result, "var x = 0xff\n");
    }

    #[test]
    fn test_normalize_float_trailing_zero() {
        let source = "var x = 1.\n";
        let result = normalize_float_literals(source);
        assert_eq!(result, "var x = 1.0\n");
    }

    #[test]
    fn test_format_idempotent() {
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
        let first = format_source(source, &config);
        let second = format_source(&first, &config);
        assert_eq!(first, second, "formatter must be idempotent");
    }

    // Regression: a plain `#` comment written directly above a declaration is a
    // leading comment for it. The member-spacing pass must not slip a blank
    // line between the comment and the declaration it describes.
    // See github.com/atelico/gdstyle issue #15.
    #[test]
    fn leading_comment_stays_tight_against_declaration() {
        let source = "extends \"res://addons/kenyoni/app_settings/app_settings.gd\"\n\n\
             # File\nconst SETTINGS_FILE: String = \"user://mods.cfg\"\n";
        let expected = "extends \"res://addons/kenyoni/app_settings/app_settings.gd\"\n\n\
             # File\nconst SETTINGS_FILE: String = \"user://mods.cfg\"\n";
        let config = Config::default();
        let result = format_source(source, &config);
        assert_eq!(result, expected);
        // And it must be stable under a second pass.
        assert_eq!(format_source(&result, &config), expected);
    }

    // Regression: the canonical gap a member requires (two blank lines before a
    // function) belongs ABOVE its leading comment, never between the comment
    // and the `func` line.
    #[test]
    fn leading_comment_absorbs_member_gap_above_it() {
        let source = "extends Node\n\n# describe foo\nfunc foo():\n\tpass\n";
        let expected = "extends Node\n\n\n# describe foo\nfunc foo():\n\tpass\n";
        let config = Config::default();
        assert_eq!(format_source(source, &config), expected);
    }

    // A comment separated from the following declaration by a blank line is a
    // standalone comment, not a leading comment: it stays where the user put it.
    #[test]
    fn comment_separated_by_blank_line_stays_standalone() {
        let source = "extends Node\n\n# Section header\n\nconst A := 1\n";
        let expected = "extends Node\n\n# Section header\n\nconst A := 1\n";
        let config = Config::default();
        assert_eq!(format_source(source, &config), expected);
    }

    // Enhancement (issue #16): a member tagged with
    // `# gdstyle:ignore=order/class-member-order` is pinned in place, so a const
    // kept next to the enum it mirrors stays between the two enums instead of
    // being hoisted into the constants group. We assert ordering (the feature's
    // guarantee) rather than exact whitespace, which is a separate concern.
    #[test]
    fn order_ignore_directive_pins_member_in_place() {
        let source = "extends Node\n\n\
             enum E {\n\tA,\n}\n\
             # gdstyle:ignore=order/class-member-order\n\
             const NAMES := [\"a\"]\n\
             enum F {\n\tB,\n}\n";
        let config = Config::default();
        let positions = |text: &str| {
            (
                text.find("enum E").unwrap(),
                text.find("const NAMES").unwrap(),
                text.find("enum F").unwrap(),
            )
        };
        let result = format_source(source, &config);
        let (e, c, f) = positions(&result);
        assert!(
            e < c && c < f,
            "pinned const must stay between its enums, got:\n{result}"
        );
        // Pinned ordering must be idempotent.
        let (e2, c2, f2) = positions(&format_source(&result, &config));
        assert!(e2 < c2 && c2 < f2, "pinned order must be stable");
    }

    // Control: without the directive the same members reorder canonically
    // (all enums grouped ahead of the const). Confirms pinning is opt-in.
    #[test]
    fn reorder_still_groups_members_without_directive() {
        let source = "extends Node\n\n\
             enum E {\n\tA,\n}\n\
             const NAMES := [\"a\"]\n\
             enum F {\n\tB,\n}\n";
        let config = Config::default();
        let result = format_source(source, &config);
        let enum_e = result.find("enum E").unwrap();
        let enum_f = result.find("enum F").unwrap();
        let const_pos = result.find("const NAMES").unwrap();
        assert!(
            enum_e < enum_f && enum_f < const_pos,
            "enums should group ahead of the const, got:\n{result}"
        );
    }
}
