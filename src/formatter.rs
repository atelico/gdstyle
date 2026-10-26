use crate::ast::ClassMember;
use crate::config::Config;
use crate::fixer;
use crate::lexer::Lexer;
use crate::linter;
use crate::parser::Parser;
use crate::token::{StringLines, Token, TokenKind};

/// Format a GDScript source string according to style rules.
///
/// This applies ALL formatting transformations unconditionally, unlike --fix
/// which only fixes flagged violations. The result is idempotent: formatting
/// already-formatted source returns it unchanged.
///
/// Line endings are kept: a CRLF file comes back as CRLF, like it does from
/// `--fix`, and a string that spans lines keeps its own line endings, since
/// Godot keeps a `\r` inside a string as part of its value. See
/// [`crate::linter::restore_line_endings`].
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

    crate::linter::restore_line_endings(source, &result)
}

fn format_pass(source: &str, config: &Config) -> String {
    // Lex once to learn whether any string runs over several lines. The
    // string-aware passes only need a mask when one does, which most files
    // never have, so they can skip lexing the source again.
    format_pass_with(source, config, SpanningStrings::find(source))
}

/// [`format_pass`] with the [`SpanningStrings`] answer for `source` given.
fn format_pass_with(source: &str, config: &Config, spanning_strings: SpanningStrings) -> String {
    let mut result = source.to_string();
    // Each pass below applies one lint rule (named next to it), so turning
    // that rule off in the config turns the pass off too. The remaining
    // rules are applied through `lint_then_fix`, which honors the config
    // already.
    let is_enabled = |rule_name: &str| config.is_rule_enabled(rule_name);

    // 1. Reorder class members FIRST, before any line-changing transformations,
    //    so that parser spans are valid against the source lines.
    //
    //    The reorder is wrapped in safety guards: it is skipped when any
    //    module-level initializer depends on another module-level identifier
    //    (because reordering could change the runtime init order), and the
    //    output is re-parsed to verify it has the same set of class members
    //    as the input: if anything mutated, we keep the original.
    if is_enabled("order/class-member-order") {
        result = safe_reorder_class_members(&result);
        result = safe_reorder_inner_classes(&result, spanning_strings);
    }

    // 2. Normalize indentation, then strip trailing whitespace. Neither pass
    //    adds or removes lines, so one string-line mask serves both.
    let string_line_mask = string_lines(&result, spanning_strings);
    if is_enabled("format/no-tabs-as-spaces") {
        result = normalize_indentation(&result, config, &string_line_mask);
    }
    if is_enabled("format/trailing-whitespace") {
        result = strip_trailing_whitespace(&result, &string_line_mask);
    }

    // 3a. Normalize blank lines BETWEEN top-level class members to their
    //     canonical counts (0 between header items, 1 between member
    //     categories, 2 around functions / inner classes). This is the
    //     member-aware pass; it understands what each top-level statement is.
    //     No lint rule covers it, so it always runs.
    result = normalize_member_spacing(&result, config);

    // 3b. Backstop for blank lines INSIDE function bodies and anywhere the
    //     member-aware pass didn't touch: collapse 3+ to 2.
    if is_enabled("format/blank-lines") {
        result = collapse_blank_lines(&result, spanning_strings);
    }

    // 4. Replace && || ! with and or not.
    if is_enabled("format/boolean-operators") {
        result = normalize_boolean_operators(&result);
    }

    // 5. Normalize quotes (single -> double unless contains double quotes).
    if is_enabled("format/double-quotes") {
        result = normalize_quotes(&result);
    }

    // 6. Normalize comment spacing (add space after #, except #region/#endregion).
    if is_enabled("format/comment-spacing") {
        result = normalize_comment_spacing(&result);
    }

    // 7. Normalize float literals (add leading/trailing zeros). Both rules
    //    ask for the zeros, so either one keeps the pass on.
    if is_enabled("format/float-literal-zeros") || is_enabled("format/number-literals") {
        result = normalize_float_literals(&result);
    }

    // 8. Normalize hex to lowercase.
    if is_enabled("format/number-literals") {
        result = normalize_hex_literals(&result);
    }

    // 9. Ensure file ends with exactly one \n.
    if is_enabled("format/trailing-newline") {
        result = ensure_trailing_newline(&result);
    }

    // 10. Use lint+fix for remaining token-based transformations.
    result = lint_then_fix(&result, config);

    // 11. Break long lines by wrapping at commas inside delimiters.
    if is_enabled("format/max-line-length") {
        result = break_long_lines(&result, config, spanning_strings);
    }

    result
}

/// Whether a source has a string literal that runs over more than one line:
/// a `"""` literal, a backslash continuation, or a quoted string with a raw
/// newline, which Godot also accepts.
///
/// The passes in [`format_pass_with`] keep string literals intact: they move,
/// re-indent or wrap whole lines around them but don't split or join one. So
/// the answer for a pass's input holds for every intermediate result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SpanningStrings {
    Present,
    Absent,
}

impl SpanningStrings {
    fn find(source: &str) -> Self {
        let is_spanning =
            |token: &Token| matches!(token.kind, TokenKind::String(_)) && token.text.contains('\n');
        if Lexer::new(source).tokenize().iter().any(is_spanning) {
            Self::Present
        } else {
            Self::Absent
        }
    }
}

/// The lines of `source` that multi-line string literals cover; see
/// [`StringLines`]. Their whitespace, blank runs and length are string data.
/// With [`SpanningStrings::Absent`] no line is covered, so the source isn't
/// lexed.
fn string_lines(source: &str, spanning_strings: SpanningStrings) -> StringLines {
    let line_count = source.split('\n').count();
    match spanning_strings {
        SpanningStrings::Absent => StringLines::new(&[], line_count),
        SpanningStrings::Present => StringLines::new(&Lexer::new(source).tokenize(), line_count),
    }
}

/// `string_line_mask` must describe `source` line for line.
fn normalize_indentation(source: &str, config: &Config, string_line_mask: &StringLines) -> String {
    let lines: Vec<&str> = source.split('\n').collect();
    let mut result = Vec::new();

    for (idx, line) in lines.iter().enumerate() {
        if line.is_empty() || string_line_mask.is_interior(idx) {
            result.push(line.to_string());
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

/// `string_line_mask` must describe `source` line for line.
fn strip_trailing_whitespace(source: &str, string_line_mask: &StringLines) -> String {
    source
        .split('\n')
        .enumerate()
        .map(|(idx, line)| {
            if string_line_mask.ends_inside(idx) {
                line.to_string()
            } else {
                line.trim_end().to_string()
            }
        })
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

/// Whether `member` is a constant or variable whose name marks it private
/// (leading underscore). Other member kinds are never private here: the
/// public/private separator only applies to data declarations, since
/// functions already get two blank lines around them.
fn is_private_data_member(member: &ClassMember) -> bool {
    match member {
        ClassMember::Constant { name, .. }
        | ClassMember::StaticVariable { name, .. }
        | ClassMember::Variable { name, .. } => name.starts_with('_'),
        _ => false,
    }
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

/// First line (1-indexed) of the run of standalone `@warning_ignore(...)`
/// and `@warning_ignore_start(...)` lines sitting directly above `below`, or
/// `below` itself when there is none.
///
/// The parser deliberately drops these annotations instead of attaching them
/// to a member (they may precede any statement, including ones whose AST
/// node has no annotation slot), so `leading_annotation_line` never sees
/// them. Without this the annotation would count as the tail of the previous
/// member and the canonical gap would land between it and the declaration
/// it applies to (or, for a region, the first declaration it covers).
///
/// `declaration_lines` holds the keyword line of every member: a line such
/// as `@warning_ignore("x") var y = 1` is a declaration of its own and must
/// not be absorbed. `@warning_ignore_restore` is not matched: it closes a
/// region, so it stays the tail of the member above it.
fn leading_warning_ignore_start(
    lines: &[&str],
    declaration_lines: &std::collections::HashSet<usize>,
    below: usize,
) -> usize {
    let mut first_line = below;
    while first_line > 1 {
        let candidate = first_line - 1;
        let is_standalone_warning_ignore = lines.get(candidate - 1).is_some_and(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("@warning_ignore(") || trimmed.starts_with("@warning_ignore_start(")
        }) && !declaration_lines.contains(&candidate);
        if !is_standalone_warning_ignore {
            break;
        }
        first_line = candidate;
    }
    first_line
}

/// Remove the blank lines that separate a script-level region marker from
/// the code it brackets: those directly below `@warning_ignore_start(...)`
/// and those directly above `@warning_ignore_restore(...)`.
///
/// With the markers tight against the region, `normalize_member_spacing`
/// can treat the start marker as the head of the first covered member and
/// the restore marker as the tail of the last one, so the canonical gaps
/// fall outside the region instead of between a marker and its code.
///
/// Marker lines are taken from the lexer's annotation tokens rather than
/// matched as text, so a look-alike line inside a multi-line string is
/// never touched. Only column-1 markers count; indented ones belong to an
/// inner class or a function body, which this pass leaves alone.
fn tighten_warning_ignore_region_markers(source: &str, tokens: &[Token]) -> String {
    let marker_lines = |marker_name: &str| -> std::collections::HashSet<usize> {
        tokens
            .iter()
            .filter(|token| {
                token.span.column == 1
                    && matches!(&token.kind, TokenKind::Annotation(name) if name == marker_name)
            })
            .map(|token| token.span.line)
            .collect()
    };
    let start_lines = marker_lines("warning_ignore_start");
    let restore_lines = marker_lines("warning_ignore_restore");
    if start_lines.is_empty() && restore_lines.is_empty() {
        return source.to_string();
    }

    let lines: Vec<&str> = source.split('\n').collect();
    // A blank-looking line inside a multi-line string is string data, not
    // spacing: it is never dropped, and it ends a run of blank lines.
    let string_line_mask = StringLines::new(tokens, lines.len());
    let mut kept: Vec<&str> = Vec::with_capacity(lines.len());
    // Line number (1-indexed) of the last non-blank line seen so far.
    let mut last_code_line: Option<usize> = None;
    // Blank lines since `last_code_line`, as written, in case they stay.
    let mut pending_blank_lines: Vec<&str> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let line_number = index + 1;
        if line.trim().is_empty() && !string_line_mask.is_interior(index) {
            pending_blank_lines.push(line);
            continue;
        }
        let follows_start_marker =
            last_code_line.is_some_and(|previous| start_lines.contains(&previous));
        // A restore marker with nothing above it has no region to close up
        // against, so its leading blank lines stay.
        let closes_region = restore_lines.contains(&line_number) && last_code_line.is_some();
        if !follows_start_marker && !closes_region {
            kept.append(&mut pending_blank_lines);
        }
        pending_blank_lines.clear();
        kept.push(line);
        last_code_line = Some(line_number);
    }
    // Trailing blank lines (including the empty piece after the final
    // newline) are not this pass's business.
    kept.append(&mut pending_blank_lines);
    kept.join("\n")
}

/// Top-level item in the source, used by `normalize_member_spacing`. A unit
/// is either a single declaration, or a leading `##` doc comment block plus
/// the declaration it attaches to. The category drives the spacing rule; the
/// `start` line is the unit's first line in the source. `decl_start` is the
/// line where the declaration itself begins (after any attached `##` docs,
/// but including annotations like `@export_range`): used to tighten the doc
/// block against the declaration when normalising. `is_private` is set for
/// `_`-prefixed constants and variables and drives the optional
/// public/private separator line. `has_accessor_block` marks a variable with
/// an indented `set` / `get` block, which gets a blank line on each side.
struct MemberUnit {
    category: usize,
    is_private: bool,
    has_accessor_block: bool,
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
///
/// With `config.blank_line_between_public_and_private` set, same-category
/// constants and variables that would otherwise sit tight get one blank line
/// wherever a public declaration meets a private (`_`-prefixed) one.
fn normalize_member_spacing(source: &str, config: &Config) -> String {
    let tightened = tighten_warning_ignore_region_markers(source, &Lexer::new(source).tokenize());
    let source = tightened.as_str();
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize();
    let members = Parser::new(&tokens).parse();
    if members.is_empty() {
        return source.to_string();
    }

    let lines: Vec<&str> = source.split('\n').collect();
    let declaration_lines: std::collections::HashSet<usize> = members
        .iter()
        .filter(|member| {
            !matches!(
                member,
                ClassMember::DocComment { .. }
                    | ClassMember::Comment { .. }
                    | ClassMember::BlankLine { .. }
            )
        })
        .map(|member| member.span().line)
        .collect();

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
                    is_private: false,
                    has_accessor_block: false,
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
                let decl_start =
                    leading_warning_ignore_start(&lines, &declaration_lines, decl_start);

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
                // A region may open above the member's doc or comment block
                // rather than directly above the declaration.
                let start = leading_warning_ignore_start(&lines, &declaration_lines, start);

                pending_doc_start = None;
                pending_doc_last = None;
                pending_comment_start = None;
                pending_comment_last = None;
                units.push(MemberUnit {
                    category: member.ordering_category(),
                    is_private: is_private_data_member(member),
                    has_accessor_block: matches!(
                        member,
                        ClassMember::Variable {
                            has_accessor_block: true,
                            ..
                        }
                    ),
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

    let mut prev_unit: Option<&MemberUnit> = None;
    let total_lines = lines.len();
    for (idx, unit) in units.iter().enumerate() {
        let blanks = if let Some(prev) = prev_unit {
            let canonical = canonical_blank_lines_between(prev.category, unit.category);
            let crosses_visibility_boundary = config.blank_line_between_public_and_private
                && prev.category == unit.category
                && prev.is_private != unit.is_private;
            // A property's `set` / `get` block reads like a small function
            // body, so it is set apart from the declarations around it.
            let touches_accessor_block = prev.has_accessor_block || unit.has_accessor_block;
            // `max` so the separator never shrinks a gap that is already
            // wider (it only ever opens up a tight same-category pair).
            if crosses_visibility_boundary || touches_accessor_block {
                canonical.max(1)
            } else {
                canonical
            }
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

        prev_unit = Some(unit);
    }

    // Drop blank lines at the end of the file. A final newline is kept but
    // not added: `ensure_trailing_newline` adds it, under its own rule.
    while result.last().is_some_and(|l| l.is_empty()) {
        result.pop();
    }
    if source.ends_with('\n') {
        result.push(String::new());
    }

    result.join("\n")
}

/// `spanning_strings` is the [`SpanningStrings`] answer for `source`.
fn collapse_blank_lines(source: &str, spanning_strings: SpanningStrings) -> String {
    let lines: Vec<&str> = source.split('\n').collect();
    let string_line_mask = string_lines(source, spanning_strings);
    let mut result = Vec::new();
    let mut blank_count = 0;

    for (idx, line) in lines.iter().enumerate() {
        if line.trim().is_empty() && !string_line_mask.ends_inside(idx) {
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
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();

    for (idx, token) in tokens.iter().enumerate() {
        if let Some(word) = crate::token::boolean_operator_word(&tokens, idx) {
            replacements.push((token.span.offset, token.span.length, word));
        }
    }

    if replacements.is_empty() {
        return source.to_string();
    }

    // Apply from end to start.
    let mut result = source.to_string();
    for (offset, length, new_text) in replacements.into_iter().rev() {
        let end = (offset + length).min(result.len());
        result.replace_range(offset..end, &new_text);
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
///
/// `spanning_strings` is the [`SpanningStrings`] answer for `source`.
fn break_long_lines(source: &str, config: &Config, spanning_strings: SpanningStrings) -> String {
    const TAB_WIDTH: usize = 4;
    let max_len = config.max_line_length;
    let lines: Vec<&str> = source.split('\n').collect();
    let string_line_mask = string_lines(source, spanning_strings);
    let mut result: Vec<String> = Vec::new();

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let visual_len = visual_line_len(line, TAB_WIDTH);

        if visual_len <= max_len || string_line_mask.is_interior(i) {
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
        match ch {
            '\t' => col = (col / tab_width + 1) * tab_width,
            // A `\r` left in a string's CRLF line ending takes no columns.
            '\r' => {}
            _ => col += 1,
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

/// Track whether the scanner is currently inside a string literal.
///
/// A backslash always skips the next character, prefixed strings included:
/// Godot won't let `\"` close a raw string either (`r"foo\"bar"` is one
/// literal), so raw strings end exactly where escaped ones do.
///
/// Triple-quoted strings (`"""..."""`, `'''...'''`) close only on three
/// unescaped quotes in a row. The scanner has no lookahead, so it reads `""`
/// as an empty string and upgrades it to a triple-quoted one when the same
/// quote follows immediately.
struct StringScanner {
    in_string: bool,
    quote: char,
    escaped: bool,
    is_triple: bool,
    /// Unescaped closing quotes seen in a row inside a triple-quoted string.
    closing_quote_run: u8,
    /// Characters consumed since the opening quote(s), to recognise `""`.
    content_length: usize,
    /// The previous character closed an empty `""`/`''` string.
    just_closed_empty_string: bool,
}

impl StringScanner {
    fn new() -> Self {
        Self {
            in_string: false,
            quote: '"',
            escaped: false,
            is_triple: false,
            closing_quote_run: 0,
            content_length: 0,
            just_closed_empty_string: false,
        }
    }

    /// Step the scanner one character forward. Returns true if `ch` should
    /// be ignored (it is part of a string literal, quotes included).
    fn step(&mut self, ch: char) -> bool {
        if self.in_string {
            if self.escaped || ch == '\\' {
                self.escaped = !self.escaped;
                self.closing_quote_run = 0;
                self.content_length += 1;
                return true;
            }
            if ch == self.quote {
                if self.is_triple {
                    self.closing_quote_run += 1;
                    if self.closing_quote_run == 3 {
                        self.in_string = false;
                    }
                } else {
                    self.in_string = false;
                    self.just_closed_empty_string = self.content_length == 0;
                }
                return true;
            }
            self.closing_quote_run = 0;
            self.content_length += 1;
            return true;
        }
        let follows_empty_string = std::mem::take(&mut self.just_closed_empty_string);
        if follows_empty_string && ch == self.quote {
            // `""` + `"`: the opening of a triple-quoted string.
            self.in_string = true;
            self.is_triple = true;
            self.closing_quote_run = 0;
            return true;
        }
        if ch == '"' || ch == '\'' {
            self.in_string = true;
            self.quote = ch;
            self.escaped = false;
            self.is_triple = false;
            self.closing_quote_run = 0;
            self.content_length = 0;
            return true;
        }
        false
    }
}

/// Find the first `(`, `[`, or `{` in content, respecting strings.
fn find_first_delimiter(content: &str) -> Option<(usize, char, char)> {
    let mut scanner = StringScanner::new();

    for (i, ch) in content.char_indices() {
        if scanner.step(ch) {
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

    for (i, ch) in content[open_pos + 1..].char_indices() {
        let abs_pos = open_pos + 1 + i;
        if scanner.step(ch) {
            continue;
        }
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

    for ch in content.chars() {
        if scanner.step(ch) {
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
    let bytes = text.as_bytes();
    for (i, ch) in text.char_indices() {
        if scanner.step(ch) {
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

    // Guard 3: keep `@warning_ignore_start` / `@warning_ignore_restore` and
    // `#region` / `#endregion` regions covering the same code.
    if changes_region_coverage(source, &reordered) {
        return source.to_string();
    }
    reordered
}

/// Reorder the members of each inner class, unless doing so would change
/// which code a region covers (see `changes_region_coverage`).
fn safe_reorder_inner_classes(source: &str, spanning_strings: SpanningStrings) -> String {
    let reordered = reorder_inner_classes(source, spanning_strings);
    if changes_region_coverage(source, &reordered) {
        return source.to_string();
    }
    reordered
}

/// True if moving lines from `original` to `reordered` changed which code is
/// covered by a `@warning_ignore_start(...)` region or a `#region` fold.
///
/// The markers are standalone lines, not part of any member. The reorder
/// moves a marker with whichever member happens to follow it, so the pair can
/// end up bracketing different code: a region written around two signals may
/// afterwards also silence warnings on unrelated variables, or stop covering
/// the signals; a `#region helpers` may end up holding the tests. Skipping
/// the reorder is the only safe answer, because there is no ordering that
/// keeps an arbitrary region intact and still sorts.
fn changes_region_coverage(original: &str, reordered: &str) -> bool {
    if original == reordered {
        return false;
    }
    let changes_warning_ignore = original.contains("@warning_ignore_start")
        && warning_ignore_region_coverage(original) != warning_ignore_region_coverage(reordered);
    changes_warning_ignore
        || (original.contains("#region")
            && fold_region_coverage(original) != fold_region_coverage(reordered))
}

/// Every code line paired with the `#region` names enclosing it, outermost
/// first, sorted so two sources can be compared regardless of line order.
/// Godot pairs `#endregion` with the nearest open `#region`, so regions nest.
fn fold_region_coverage(source: &str) -> Vec<(&str, Vec<&str>)> {
    let mut open_regions: Vec<&str> = Vec::new();
    let mut coverage: Vec<(&str, Vec<&str>)> = Vec::new();
    for line in source.split('\n') {
        let trimmed = line.trim();
        if let Some(name) = trimmed.strip_prefix("#region") {
            open_regions.push(name.trim());
        } else if trimmed.starts_with("#endregion") {
            open_regions.pop();
        } else if !trimmed.is_empty() && !trimmed.starts_with('#') && !open_regions.is_empty() {
            coverage.push((trimmed, open_regions.clone()));
        }
    }
    coverage.sort_unstable();
    coverage
}

/// Every code line that sits inside a `@warning_ignore_start` region, paired
/// with the warning codes ignored at that point, sorted so two sources can be
/// compared regardless of line order. Reordering only moves lines, so equal
/// coverage means every line is still ignored for exactly the same warnings.
///
/// Regions are textual in Godot: one runs from its `@warning_ignore_start`
/// line to the matching `@warning_ignore_restore`, or to the end of the file.
fn warning_ignore_region_coverage(source: &str) -> Vec<(&str, Vec<&str>)> {
    let mut active_codes: Vec<&str> = Vec::new();
    let mut coverage: Vec<(&str, Vec<&str>)> = Vec::new();
    for line in source.split('\n') {
        let trimmed = line.trim();
        if let Some(arguments) = trimmed.strip_prefix("@warning_ignore_start(") {
            for code in quoted_strings(arguments) {
                if !active_codes.contains(&code) {
                    active_codes.push(code);
                }
            }
        } else if let Some(arguments) = trimmed.strip_prefix("@warning_ignore_restore(") {
            let restored = quoted_strings(arguments);
            active_codes.retain(|code| !restored.contains(code));
        } else if !trimmed.is_empty() && !trimmed.starts_with('#') && !active_codes.is_empty() {
            let mut codes = active_codes.clone();
            codes.sort_unstable();
            coverage.push((trimmed, codes));
        }
    }
    coverage.sort_unstable();
    coverage
}

/// The contents of each `"..."` or `'...'` literal in `text`, in order.
/// Used on annotation arguments, which are plain warning names with no
/// escapes.
fn quoted_strings(text: &str) -> Vec<&str> {
    let mut contents = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find(['"', '\'']) {
        let quote = rest[open..].chars().next().unwrap_or('"');
        let after_open = &rest[open + 1..];
        let Some(close) = after_open.find(quote) else {
            break;
        };
        contents.push(&after_open[..close]);
        rest = &after_open[close + 1..];
    }
    contents
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
/// share a source line (e.g. `class_name X extends Y`). Indented declaration
/// lines are skipped so a member can't be torn away from its `class X:`
/// header. The parser nests inner-class members properly, but it is tolerant
/// of malformed input, so this stays as a guard.
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
/// The walk stops at string data, even a line that looks like a comment.
fn compute_attached_starts(
    merged: &[Anchor],
    lines: &[&str],
    string_line_mask: &StringLines,
) -> Vec<usize> {
    let mut attached_starts: Vec<usize> = Vec::new();
    for (i, &(decl_line, _, _)) in merged.iter().enumerate() {
        let prev_boundary = if i > 0 { merged[i - 1].0 + 1 } else { 0 };
        let mut start = decl_line;
        let mut j = decl_line;
        while j > prev_boundary {
            j -= 1;
            if string_line_mask.is_interior(j) {
                break;
            }
            let content = if j < lines.len() { lines[j].trim() } else { "" };
            if content.starts_with('#') || content.starts_with('@') {
                start = j;
            } else if content.is_empty() {
                // Blank line: keep looking only if a comment sits directly above.
                let above = if j > prev_boundary
                    && j - 1 < lines.len()
                    && !string_line_mask.is_interior(j - 1)
                {
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

    let string_line_mask = StringLines::new(&tokens, lines.len());
    let attached_starts = compute_attached_starts(&merged, &lines, &string_line_mask);

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

    // The class header follows the doc block: `class_name` and `extends`
    // lines, possibly with annotations such as `@tool` or `@abstract` before
    // them or on lines of their own. It ends at the last `class_name` or
    // `extends`, so an `@export` above the first variable stays put.
    let decl_start = doc_end + 1;
    let mut decl_end = None;
    for (i, line) in lines.iter().enumerate().skip(decl_start) {
        match class_header_keyword_line(line.trim()) {
            Some(true) => decl_end = Some(i),
            Some(false) => {}
            None => break,
        }
    }
    let Some(decl_end) = decl_end else {
        return source.to_string();
    };

    // The search above reads raw lines, so it can match text inside a
    // multi-line string (a script template, say). Moving those lines would
    // rewrite the string, so leave the source alone if any of them is string
    // data. Lexing only here keeps the common case cheap.
    let string_line_mask = StringLines::new(&Lexer::new(source).tokenize(), lines.len());
    if (doc_start..=decl_end).any(|idx| string_line_mask.is_interior(idx)) {
        return source.to_string();
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

/// Classify a trimmed line of a class header. `Some(true)` for a
/// `class_name` or `extends` line, with or without annotations before the
/// keyword (`@abstract extends Node`); `Some(false)` for a line holding only
/// annotations (`@tool`), which may belong to the header; `None` otherwise.
fn class_header_keyword_line(trimmed: &str) -> Option<bool> {
    let mut rest = trimmed;
    while let Some(after_at) = rest.strip_prefix('@') {
        let name_len = after_at
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(after_at.len());
        rest = &after_at[name_len..];
        // Skip arguments such as `@icon("res://icon.svg")`.
        if rest.starts_with('(') {
            let close = rest.find(')')?;
            rest = &rest[close + 1..];
        }
        rest = rest.trim_start();
    }
    if rest.starts_with("class_name") || rest.starts_with("extends") {
        Some(true)
    } else if rest.is_empty() && trimmed.starts_with('@') {
        Some(false)
    } else {
        None
    }
}

/// Find inner class blocks in the source and recursively reorder their members.
///
/// The body is dedented, reordered and re-indented line by line, so lines
/// inside a multi-line string are kept out of it: they are copied verbatim,
/// never end the body, and never start a class.
fn reorder_inner_classes(source: &str, spanning_strings: SpanningStrings) -> String {
    let lines: Vec<&str> = source.split('\n').collect();
    let string_line_mask = string_lines(source, spanning_strings);
    let mut result: Vec<String> = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();
        if string_line_mask.is_interior(i) {
            result.push(lines[i].to_string());
            i += 1;
            continue;
        }
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
                if string_line_mask.is_interior(i) {
                    // String data belongs to the member that opened it.
                    i += 1;
                    continue;
                }
                if line.trim().is_empty() {
                    // Blank line: include if the next non-blank line is still in the body.
                    let mut next_nonblank = i + 1;
                    while next_nonblank < lines.len()
                        && (lines[next_nonblank].trim().is_empty()
                            || string_line_mask.is_interior(next_nonblank))
                    {
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
                    .enumerate()
                    .map(|(offset, l)| {
                        if string_line_mask.is_interior(body_start + offset) {
                            l.to_string()
                        } else if l.trim().is_empty() {
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
                let reordered_mask = string_lines(&reordered, spanning_strings);
                for (idx, line) in reordered.split('\n').enumerate() {
                    if reordered_mask.is_interior(idx) {
                        result.push(line.to_string());
                    } else if line.trim().is_empty() {
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

    /// `S` for every character the scanner treats as string, `.` otherwise.
    fn string_mask(line: &str) -> String {
        let mut scanner = StringScanner::new();
        line.chars()
            .map(|ch| if scanner.step(ch) { 'S' } else { '.' })
            .collect()
    }

    #[test]
    fn string_scanner_marks_literal_boundaries() {
        let cases = [
            // Plain, escaped quote, and the other quote style inside.
            (r#"a("x,y", b)"#, "..SSSSS...."),
            (r#"a("x\"y", b)"#, "..SSSSSS...."),
            (r#"a('x"y', b)"#, "..SSSSS...."),
            // Raw: `\"` does not close; `\\` pairs so the next quote does.
            (r#"a(r"x\"y", b)"#, "...SSSSSS...."),
            (r#"a(r"x\\", b)"#, "...SSSSS...."),
            // Empty strings next to each other and to other tokens.
            (r#"a("", '', b)"#, "..SS..SS...."),
            // Triple-quoted: lone quotes inside don't close it.
            (r#"a("""x", y""", b)"#, "..SSSSSSSSSSS...."),
            (r#"a(r'''a, ' '', b''', c)"#, "...SSSSSSSSSSSSSSSS...."),
            // A value ending in a quote: three quotes close, the fourth
            // starts a new string.
            (r#"a("""x"""", b)"#, "..SSSSSSSSSSSS"),
        ];
        for (line, expected) in cases {
            assert_eq!(string_mask(line), expected, "for {}", line);
        }
    }

    #[test]
    fn test_strip_trailing_whitespace() {
        let source = "var x = 5   \nvar y = 10\t\t\n";
        let result =
            strip_trailing_whitespace(source, &string_lines(source, SpanningStrings::Absent));
        assert_eq!(result, "var x = 5\nvar y = 10\n");
    }

    #[test]
    fn line_passes_skip_multiline_string_contents() {
        // Everything between the opening and closing `"""` is string data.
        let source = "extends Node\n\nconst T = \"\"\"a   \n\n\n\n    b\n\tc\n\"\"\"\n";
        assert_eq!(SpanningStrings::find(source), SpanningStrings::Present);
        let string_line_mask = string_lines(source, SpanningStrings::Present);
        assert_eq!(strip_trailing_whitespace(source, &string_line_mask), source);
        assert_eq!(
            collapse_blank_lines(source, SpanningStrings::Present),
            source
        );
        assert_eq!(
            normalize_indentation(source, &Config::default(), &string_line_mask),
            source
        );
        let long = format!("var s = \"\"\"\nf({})\n\"\"\"\n", "a, ".repeat(60));
        assert_eq!(
            break_long_lines(&long, &Config::default(), SpanningStrings::Present),
            long
        );
    }

    #[test]
    fn spanning_strings_detects_every_kind_of_spanning_literal() {
        use SpanningStrings::{Absent, Present};
        // Triple-quoted, a quoted string with a raw newline (Godot accepts
        // it), and a backslash continuation.
        assert_eq!(SpanningStrings::find("var s = \"\"\"a\nb\"\"\"\n"), Present);
        assert_eq!(SpanningStrings::find("var s = \"a\nb\"\n"), Present);
        assert_eq!(SpanningStrings::find("var s = 'a\\\nb'\n"), Present);
        // Single-line strings, including a `"""` one, and no strings at all.
        let single = "var s = \"a\"\nvar t = \"\"\"b\"\"\"\n# \"\"\"\n";
        assert_eq!(SpanningStrings::find(single), Absent);
        assert_eq!(SpanningStrings::find("var x = 1\n"), Absent);
    }

    #[test]
    fn skipping_masks_without_spanning_strings_changes_no_output() {
        // `format_pass` skips the string masks when `SpanningStrings::find`
        // says there are none. Always building them must give the same
        // output on every fixture and example.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut checked = 0;
        for dir in ["tests/fixtures", "examples"] {
            let entries = std::fs::read_dir(root.join(dir)).expect("fixture dir is readable");
            for entry in entries {
                let path = entry.expect("fixture dir entry is readable").path();
                if path.extension().is_none_or(|ext| ext != "gd") {
                    continue;
                }
                let source = std::fs::read_to_string(&path).expect("fixture is UTF-8");
                let config = Config::default();
                assert_eq!(
                    format_pass(&source, &config),
                    format_pass_with(&source, &config, SpanningStrings::Present),
                    "{}",
                    path.display()
                );
                checked += 1;
            }
        }
        assert!(checked > 10, "only {checked} fixtures found");
    }

    #[test]
    fn inner_class_reorder_keeps_multiline_string_contents() {
        // The body is dedented and re-indented line by line; string lines
        // must come through as they are: a four-space line, a spaces-only
        // line, and a line that looks like a class header.
        let source = "extends Node\n\n\nclass A:\n\tvar v = 1\n\tconst X = \"\"\"\nclass Fake:\n    x\n   \n\t\"\"\"\n";
        let formatted = format_source(source, &Config::default());
        assert!(
            formatted.contains("\tconst X = \"\"\"\nclass Fake:\n    x\n   \n\t\"\"\"\n"),
            "{formatted:?}"
        );
        assert!(
            formatted.find("const X") < formatted.find("var v"),
            "{formatted:?}"
        );
    }

    #[test]
    fn inner_class_reorder_keeps_crlf_blank_line_in_string() {
        // Reduced from a real plugin: the blank line inside the string is
        // `\r` once the file is normalized and must keep it.
        let source = "class A extends B:\r\n\tconst X = \"\"\"\r\n\r\n\tconst Y = \"\"\"\r\n";
        assert_eq!(format_source(source, &Config::default()), source);
    }

    #[test]
    fn region_marker_pass_keeps_blank_looking_string_lines() {
        // A column-1 region marker makes the pass rebuild the file's blank
        // lines; lines inside a string are data, with LF or CRLF.
        let crlf = "extends Node\r\n\r\nconst T = \"\"\"\r\na\r\n\r\nb\r\n\"\"\"\r\n@warning_ignore_restore(\"unused\")\r\n";
        assert_eq!(format_source(crlf, &Config::default()), crlf);
        let lf = "extends Node\n\nconst T = \"\"\"\na\n   \n\t\nb\n\"\"\"\n@warning_ignore_restore(\"unused\")\n";
        assert_eq!(format_source(lf, &Config::default()), lf);
    }

    #[test]
    fn doc_comment_move_leaves_a_template_string_alone() {
        let source =
            "extends Node\n\nconst TEMPLATE = \"\"\"\n## Generated script\nextends Node\n\"\"\"\n";
        assert_eq!(format_source(source, &Config::default()), source);
    }

    #[test]
    fn reorder_does_not_attach_a_comment_looking_closing_line() {
        // Walking back from `var b` must stop at the string's closing line,
        // even though it starts with `#`.
        let source = "extends Node\n\nfunc f():\n\tpass\n\n\nconst A = \"\"\"\nx\n\n# note\n## doc\"\"\"\nvar b = 1\n";
        let formatted = format_source(source, &Config::default());
        assert!(
            formatted.contains("const A = \"\"\"\nx\n\n# note\n## doc\"\"\"\n"),
            "{formatted:?}"
        );
    }

    #[test]
    fn format_source_keeps_quoted_string_with_raw_newline() {
        // A plain `"..."` literal can hold raw newlines too; its contents are
        // data just like a `"""` literal's.
        let source = "extends Node\n\nconst T = \"a   \n\n\n\n    b\n\tc\"\n";
        assert_eq!(format_source(source, &Config::default()), source);
    }

    #[test]
    fn format_source_keeps_multiline_string_contents() {
        let source = "extends Node\n\nconst T = \"\"\"a   \n\n\n\n    b\n\tc\n\"\"\"\n";
        assert_eq!(format_source(source, &Config::default()), source);
    }

    #[test]
    fn format_source_keeps_closing_line_followed_by_another_string() {
        // A second string on the closing line must not clear that line's mask.
        let tail = "func f() -> String:\n\tvar s := \"\"\"\nline\n    closing indent kept\"\"\" % \"x\"\n\treturn s\n";
        assert_eq!(format_source(tail, &Config::default()), tail);
        let pair = "var t := \"\"\"a\n    b\"\"\" + \"\"\"c\n    d\"\"\"\n";
        assert_eq!(format_source(pair, &Config::default()), pair);
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
        let result = collapse_blank_lines(source, SpanningStrings::Absent);
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
    fn normalize_boolean_operators_keeps_words_apart() {
        let cases = [
            ("a&&b", "a and b"),
            ("a||b", "a or b"),
            ("!c", "not c"),
            ("a&&b||!c", "a and b or not c"),
            ("(a)&&(b)", "(a) and (b)"),
            ("a &&b", "a and b"),
            ("! a", "not a"),
            ("(!a)", "(not a)"),
            ("a && b", "a and b"),
            ("\"a&&b||!c\"", "\"a&&b||!c\""),
        ];
        for (expr, expected) in cases {
            let result = normalize_boolean_operators(&format!("var o = {expr}\n"));
            assert_eq!(result, format!("var o = {expected}\n"));
        }
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
