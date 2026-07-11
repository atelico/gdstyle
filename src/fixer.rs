use crate::ast::ClassMember;
use crate::diagnostic::{Diagnostic, Replacement};
use crate::lexer::Lexer;
use crate::parser::Parser as GdParser;
use crate::token::{Token, TokenKind};

/// Classification of an identifier rename, used to decide which token contexts
/// it is safe to rewrite.
#[derive(Debug, Clone, PartialEq)]
pub enum RenameKind {
    Function,
    Variable,
    Constant,
    Signal,
    Class,
    EnumName,
    EnumMember { enum_name: Option<String> },
    NodePath,
}

/// Map a lint rule id to the rename kind whose references it produces.
pub fn rule_to_kind(rule: &str) -> Option<RenameKind> {
    Some(match rule {
        "naming/class-name-pascal-case" => RenameKind::Class,
        "naming/function-name-snake-case" => RenameKind::Function,
        "naming/variable-name-snake-case" => RenameKind::Variable,
        "naming/constant-name-screaming-case" => RenameKind::Constant,
        "naming/signal-name-snake-case" => RenameKind::Signal,
        "naming/signal-past-tense" => RenameKind::Signal,
        "naming/enum-name-pascal-case" => RenameKind::EnumName,
        "naming/enum-member-screaming-case" => RenameKind::EnumMember { enum_name: None },
        "naming/node-name-pascal-case" => RenameKind::NodePath,
        _ => return None,
    })
}

/// A rename that was applied during --unsafe-fix.
#[derive(Debug, Clone)]
pub struct AppliedRename {
    pub old_name: String,
    pub new_name: String,
    pub source_file: String,
    /// `class_name` declaration of the source file, if any. Used by the
    /// cross-file rewriter to gate rewrites on a fully qualified `Class.member`
    /// access rather than blanket identifier matching.
    pub source_class_name: Option<String>,
    pub kind: RenameKind,
    /// True for instance-level class members (non-static `var`, non-static
    /// `func`, signals). For these, cross-file references typically appear as
    /// `instance.member`, where `instance` cannot be matched against the
    /// source class name. Set to false for static members, constants, classes,
    /// enums, etc., where access is always qualified by class.
    pub is_instance_member: bool,
}

/// A cross-file reference to a renamed identifier.
#[derive(Debug)]
pub struct CrossFileReference {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub old_name: String,
    pub new_name: String,
    pub source_file: String,
    pub offset: usize,
    pub length: usize,
}

/// Apply fixes from diagnostics to the source string.
///
/// Returns the modified source with all non-overlapping fixes applied. If
/// `safe_only` is true, only fixes with `is_safe == true` are applied.
///
/// For naming renames (only when `safe_only == false`), references in the same
/// file are updated using context-aware matching (member access, call
/// position, collision detection) so that an identifier rename does not
/// accidentally rewrite an unrelated declaration that happens to share a name.
///
/// # Example
///
/// ```
/// use gdstyle::{config::Config, linter, fixer};
///
/// let source = "var x = 1   \n"; // trailing whitespace
/// let diagnostics = linter::lint_source(source, "demo.gd", &Config::default());
/// let fixed = fixer::apply_fixes(source, &diagnostics, true); // safe fixes only
/// assert_eq!(fixed, "var x = 1\n");
/// ```
pub fn apply_fixes(source: &str, diagnostics: &[Diagnostic], safe_only: bool) -> String {
    // `diagnostics` was produced by `linter::lint_source`, which normalizes
    // `\r\n`/`\r` to `\n` before computing byte offsets (see
    // `normalize_line_endings`). Normalize here too so those offsets land on
    // the right bytes, then restore the original line-ending convention on
    // the way out so CRLF files stay CRLF.
    let has_crlf = source.contains('\r');
    let normalized = crate::linter::normalize_line_endings(source);
    let source = normalized.as_str();

    // Single tokenize + parse pass for everything we need below.
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize();
    let members = GdParser::new(&tokens).parse();
    let class_name = extract_class_name(&members);
    let existing_names = collect_existing_names(&members);
    let identifier_tokens = collect_identifier_tokens(&tokens);

    let mut replacements: Vec<Replacement> = Vec::new();

    // Renames carry their kind so the same-file rewriter can apply context
    // rules (call position vs. bare reference, qualified vs. unqualified).
    let mut renames: Vec<(String, String, RenameKind)> = Vec::new();

    for diag in diagnostics {
        if let Some(ref fix) = diag.fix {
            if safe_only && !fix.is_safe {
                continue;
            }
            // For unsafe naming rule fixes, suppress the rename when the new
            // name already exists in the file (prevents duplicate-declaration
            // parse errors when the original was an intentional alias such as
            // `const e = E`).
            let is_naming_fix = !safe_only && rule_to_kind(&diag.rule).is_some();
            let mut suppress_this_diag = false;
            if is_naming_fix {
                for replacement in &fix.replacements {
                    let end = (replacement.offset + replacement.length).min(source.len());
                    if replacement.offset <= source.len() {
                        let old_name = &source[replacement.offset..end];
                        if old_name.is_empty() || old_name == replacement.new_text {
                            continue;
                        }
                        // Suppress if the proposed new name is already
                        // declared OR appears anywhere in the file as an
                        // identifier token (Godot built-in, autoload,
                        // imported class, etc.). The rename would either
                        // duplicate a declaration or create a self-
                        // referential / shadowing definition.
                        if existing_names.contains(replacement.new_text.as_str())
                            || identifier_tokens.contains(replacement.new_text.as_str())
                        {
                            suppress_this_diag = true;
                            break;
                        }
                    }
                }
            }
            if suppress_this_diag {
                continue;
            }
            for replacement in &fix.replacements {
                if !safe_only {
                    if let Some(kind) = rule_to_kind(&diag.rule) {
                        let end = (replacement.offset + replacement.length).min(source.len());
                        if replacement.offset <= source.len() {
                            let old_name = &source[replacement.offset..end];
                            if !old_name.is_empty() && old_name != replacement.new_text {
                                let resolved_kind = resolve_kind(kind, &members, old_name);
                                renames.push((
                                    old_name.to_string(),
                                    replacement.new_text.clone(),
                                    resolved_kind,
                                ));
                            }
                        }
                    }
                }
                replacements.push(replacement.clone());
            }
        }
    }

    // For unsafe naming fixes, look up same-file references using context-aware
    // matching. Reuses the tokens lexed at the top of the function.
    if !safe_only && !renames.is_empty() {
        for (old_name, new_name, kind) in &renames {
            for (offset, length) in
                same_file_references(&tokens, old_name, kind, class_name.as_deref())
            {
                let candidate = Replacement {
                    offset,
                    length,
                    new_text: new_name.clone(),
                };
                let already_exists = replacements
                    .iter()
                    .any(|r| r.offset == candidate.offset && r.length == candidate.length);
                if !already_exists {
                    replacements.push(candidate);
                }
            }
        }
    }

    let fixed = if replacements.is_empty() {
        source.to_string()
    } else {
        apply_replacements(source, replacements)
    };

    if has_crlf {
        fixed.replace('\n', "\r\n")
    } else {
        fixed
    }
}

fn apply_replacements(source: &str, replacements: Vec<Replacement>) -> String {
    let result = apply_replacements_no_collapse(source, replacements);
    collapse_blank_lines(&result)
}

/// Extract per-file rename records for cross-file reference tracking.
///
/// `members` should be the parsed AST of the source file; it is used to
/// determine the file's `class_name` and the parent enum of any renamed enum
/// member.
pub fn extract_renames(
    source: &str,
    diagnostics: &[Diagnostic],
    file_path: &str,
    members: &[ClassMember],
) -> Vec<AppliedRename> {
    // `diagnostics` carries byte offsets computed against the LF-normalized
    // source (see `apply_fixes` for the full rationale), so normalize here
    // too before slicing `source` with those offsets.
    let normalized = crate::linter::normalize_line_endings(source);
    let source = normalized.as_str();

    let class_name = extract_class_name(members);
    let existing_names = collect_existing_names(members);
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize();
    let identifier_tokens = collect_identifier_tokens(&tokens);
    let mut renames = Vec::new();
    for diag in diagnostics {
        if let Some(ref fix) = diag.fix {
            if fix.is_safe {
                continue;
            }
            let Some(kind) = rule_to_kind(&diag.rule) else {
                continue;
            };
            for replacement in &fix.replacements {
                let end = (replacement.offset + replacement.length).min(source.len());
                if replacement.offset <= source.len() {
                    let old_name = &source[replacement.offset..end];
                    if old_name.is_empty() || old_name == replacement.new_text {
                        continue;
                    }
                    // Skip when the proposed new name would collide with an
                    // existing declaration OR with any identifier already
                    // used in the file (autoload, built-in, etc.). Without
                    // this guard the rename can create duplicate decls
                    // (`const e = E` → `const E = E`) or self-referential
                    // ones (`const pi = PI` → `const PI = PI`).
                    if existing_names.contains(replacement.new_text.as_str())
                        || identifier_tokens.contains(replacement.new_text.as_str())
                    {
                        continue;
                    }
                    let resolved_kind = resolve_kind(kind.clone(), members, old_name);
                    let is_instance_member =
                        is_instance_member_in(members, old_name, &resolved_kind);
                    renames.push(AppliedRename {
                        old_name: old_name.to_string(),
                        new_name: replacement.new_text.clone(),
                        source_file: file_path.to_string(),
                        source_class_name: class_name.clone(),
                        kind: resolved_kind,
                        is_instance_member,
                    });
                }
            }
        }
    }
    renames
}

/// Collect every declared name in the file (vars, consts, signals, funcs,
/// inner classes, enum members). Used by the rename extractor to skip fixes
/// that would create a duplicate declaration.
///
/// Returns string slices borrowed from `members`: no allocation per name.
fn collect_existing_names(members: &[ClassMember]) -> std::collections::HashSet<&str> {
    let mut names = std::collections::HashSet::new();
    fn walk<'a>(members: &'a [ClassMember], names: &mut std::collections::HashSet<&'a str>) {
        for m in members {
            match m {
                ClassMember::Variable { name, .. }
                | ClassMember::StaticVariable { name, .. }
                | ClassMember::Constant { name, .. }
                | ClassMember::Signal { name, .. }
                | ClassMember::Function { name, .. }
                | ClassMember::ClassNameDecl { name, .. } => {
                    names.insert(name.as_str());
                }
                ClassMember::Enum {
                    name, members: ems, ..
                } => {
                    if let Some(n) = name {
                        names.insert(n.as_str());
                    }
                    for em in ems {
                        names.insert(em.name.as_str());
                    }
                }
                ClassMember::InnerClass {
                    name,
                    members: inner,
                    ..
                } => {
                    names.insert(name.as_str());
                    walk(inner, names);
                }
                _ => {}
            }
        }
    }
    walk(members, &mut names);
    names
}

/// Collect every identifier token that appears anywhere in the source. Used
/// by the rename suppressor: if the proposed `new_name` appears as an
/// identifier already (whether a Godot built-in like `PI`, an autoload name,
/// or an external class), the rename would either shadow that name or
/// create a self-referential definition. Either way, skip.
///
/// Borrows from `tokens`: no per-name allocation.
fn collect_identifier_tokens(tokens: &[Token]) -> std::collections::HashSet<&str> {
    tokens
        .iter()
        .filter_map(|t| match &t.kind {
            TokenKind::Identifier(name) => Some(name.as_str()),
            _ => None,
        })
        .collect()
}

/// True when the renamed symbol is a non-static instance member (`var` or
/// `func` without `static`), or any signal. Such members are typically
/// accessed as `instance.member` from other files, so the cross-file rewriter
/// can't gate on the source class name alone.
fn is_instance_member_in(members: &[ClassMember], name: &str, kind: &RenameKind) -> bool {
    match kind {
        RenameKind::Signal => true,
        RenameKind::Variable => member_is_non_static_var(members, name),
        RenameKind::Function => member_is_non_static_func(members, name),
        _ => false,
    }
}

fn member_is_non_static_var(members: &[ClassMember], name: &str) -> bool {
    for m in members {
        match m {
            ClassMember::Variable { name: n, .. } if n == name => return true,
            ClassMember::StaticVariable { name: n, .. } if n == name => return false,
            ClassMember::InnerClass { members: inner, .. } => {
                if member_is_non_static_var(inner, name) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn member_is_non_static_func(members: &[ClassMember], name: &str) -> bool {
    for m in members {
        match m {
            ClassMember::Function {
                name: n, is_static, ..
            } if n == name => return !is_static,
            ClassMember::InnerClass { members: inner, .. } => {
                if member_is_non_static_func(inner, name) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Scan a target file for references to identifiers that were renamed in other
/// files. References are matched in context: a `Function` rename only matches
/// `Class.func(`; a `Variable`/`Constant`/`Signal` rename only matches
/// `Class.member`; an `EnumMember` rename matches `EnumName.MEMBER` (and
/// `Class.EnumName.MEMBER`); a `Class`/`EnumName` rename matches bare
/// occurrences not preceded by `.`.
pub fn find_cross_file_references(
    source: &str,
    file_path: &str,
    renames: &[AppliedRename],
) -> Vec<CrossFileReference> {
    let mut refs = Vec::new();
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize();

    for rename in renames {
        // Don't report the source file's own references; those are handled by
        // the same-file rewriter.
        if rename.source_file == file_path {
            continue;
        }

        for (i, token) in tokens.iter().enumerate() {
            let TokenKind::Identifier(ref name) = token.kind else {
                continue;
            };
            if name != &rename.old_name {
                continue;
            }
            if !cross_file_reference_allowed(&tokens, i, rename) {
                continue;
            }
            refs.push(CrossFileReference {
                file: file_path.to_string(),
                line: token.span.line,
                column: token.span.column,
                old_name: rename.old_name.clone(),
                new_name: rename.new_name.clone(),
                source_file: rename.source_file.clone(),
                offset: token.span.offset,
                length: token.span.length,
            });
        }
    }
    refs
}

/// Apply cross-file reference fixes to a source string.
pub fn apply_cross_file_fixes(source: &str, refs: &[CrossFileReference]) -> String {
    if refs.is_empty() {
        return source.to_string();
    }
    let replacements: Vec<Replacement> = refs
        .iter()
        .map(|r| Replacement {
            offset: r.offset,
            length: r.length,
            new_text: r.new_name.clone(),
        })
        .collect();
    apply_replacements_no_collapse(source, replacements)
}

/// A scene-file connection reference that an `--unsafe-fix` rename should
/// rewrite: `signal="..."` / `method="..."` attributes inside a `.tscn`
/// or `.tres` `[connection]` row.
#[derive(Debug)]
pub struct SceneReference {
    pub line: usize,
    pub attribute: &'static str,
    pub old_name: String,
    pub new_name: String,
}

/// Rewrite signal/method connection attributes in a `.tscn`/`.tres` scene
/// file for any `Signal` or `Function` renames, and report what changed.
///
/// Godot stores editor-wired signal connections as
/// `[connection signal="x" from="." to="Y" method="z"]`. A `.gd`-only
/// rename leaves these stale and the connection silently fails at runtime,
/// so `--unsafe-fix` must rewrite the scene too.
///
/// Returns the rewritten source and the list of references that were
/// changed (for reporting).
pub fn apply_scene_renames(
    scene_source: &str,
    renames: &[AppliedRename],
) -> (String, Vec<SceneReference>) {
    let mut applied: Vec<SceneReference> = Vec::new();
    let mut out_lines: Vec<String> = Vec::with_capacity(scene_source.split('\n').count());

    for (line_idx, line) in scene_source.split('\n').enumerate() {
        let mut new_line = line.to_string();
        for rename in renames {
            let attr = match rename.kind {
                RenameKind::Signal => "signal",
                RenameKind::Function => "method",
                _ => continue,
            };
            // Exact, quoted attribute match: `signal="old"` -> `signal="new"`.
            let needle = format!("{}=\"{}\"", attr, rename.old_name);
            let replacement = format!("{}=\"{}\"", attr, rename.new_name);
            if new_line.contains(&needle) {
                new_line = new_line.replace(&needle, &replacement);
                applied.push(SceneReference {
                    line: line_idx + 1,
                    attribute: if attr == "signal" { "signal" } else { "method" },
                    old_name: rename.old_name.clone(),
                    new_name: rename.new_name.clone(),
                });
            }
        }
        out_lines.push(new_line);
    }

    (out_lines.join("\n"), applied)
}

/// Same as [`apply_replacements`] but doesn't run the blank-line collapse:
/// cross-file rewrites don't restructure the file the way same-file fixes
/// can.
///
/// Two phases:
/// 1. **Overlap resolution.** Sort the replacements by length ascending so
///    narrower spans win over wider ones (a one-token rename always beats
///    a whole-line rewrite that happens to cover the same identifier).
///    Walk this list and keep each replacement that doesn't overlap any
///    already-accepted one. O(K²) in the worst case over the K
///    replacements, but K is bounded by the number of diagnostics on a
///    single file (hundreds at most).
/// 2. **Apply.** Sort the accepted replacements by offset ascending and
///    walk the source once, copying input bytes up to each replacement
///    and substituting `new_text`. Insertions at the same offset as a
///    span apply BEFORE the span (length-ascending tie-break) so e.g.
///    an operator-spacing insertion adjacent to a string-literal
///    replacement comes out as `<insert>` followed by `<replaced text>`,
///    never inside it.
fn apply_replacements_no_collapse(source: &str, replacements: Vec<Replacement>) -> String {
    if replacements.is_empty() {
        return source.to_string();
    }

    // Phase 1: pick a maximal set of non-overlapping replacements,
    // preferring narrower spans.
    let mut by_width: Vec<&Replacement> = replacements.iter().collect();
    by_width.sort_by_key(|r| r.length);
    let mut accepted: Vec<&Replacement> = Vec::with_capacity(by_width.len());
    for r in by_width {
        let r_end = r.offset + r.length;
        let overlaps = accepted.iter().any(|a| {
            let a_end = a.offset + a.length;
            r.offset < a_end && r_end > a.offset
        });
        if !overlaps {
            accepted.push(r);
        }
    }

    // Phase 2: apply accepted replacements in offset-ascending order. Same-
    // offset insertions come before spans (length asc) so the inserted
    // text appears as a prefix to the replacement text.
    accepted.sort_by(|a, b| a.offset.cmp(&b.offset).then(a.length.cmp(&b.length)));

    let mut out = String::with_capacity(source.len());
    let mut cursor = 0usize;
    for r in accepted {
        if r.offset > source.len() || r.offset < cursor {
            continue;
        }
        if cursor < r.offset {
            out.push_str(&source[cursor..r.offset]);
        }
        out.push_str(&r.new_text);
        cursor = (r.offset + r.length).min(source.len()).max(cursor);
    }
    if cursor < source.len() {
        out.push_str(&source[cursor..]);
    }
    out
}

// ---------------------------------------------------------------------------
// Context-aware matching helpers
// ---------------------------------------------------------------------------

/// True if the identifier token at `idx` in `tokens` is a permissible
/// cross-file reference for `rename` (i.e. the surrounding tokens prove it
/// refers to the renamed symbol, not a coincidentally-named local symbol).
fn cross_file_reference_allowed(tokens: &[Token], idx: usize, rename: &AppliedRename) -> bool {
    let prev_dot_qualifier = qualifier_before_dot(tokens, idx);
    let preceded_by_dot = is_member_access(tokens, idx);
    let followed_by_paren = matches!(
        next_significant(tokens, idx).map(|t| &t.kind),
        Some(TokenKind::LeftParen)
    );

    match &rename.kind {
        RenameKind::Function => {
            // A function call cross-file is `<...>.func(`. We need a `.`
            // before the identifier and a `(` after.
            if !preceded_by_dot {
                return false;
            }
            if !followed_by_paren {
                return false;
            }
            // Static functions are always accessed as `Class.func(`.
            // Instance methods are typically `instance.func(` where the
            // instance type can't be checked syntactically, so we accept any
            // qualifier in that case (matches the user's --unsafe-fix intent
            // at the cost of possibly renaming an unrelated `.func()` on a
            // different type).
            if rename.is_instance_member {
                return true;
            }
            prev_dot_qualifier
                .as_deref()
                .zip(rename.source_class_name.as_deref())
                .map(|(q, cn)| q == cn)
                .unwrap_or(false)
        }
        RenameKind::Variable => {
            if !preceded_by_dot {
                return false;
            }
            if rename.is_instance_member {
                return true;
            }
            prev_dot_qualifier
                .as_deref()
                .zip(rename.source_class_name.as_deref())
                .map(|(q, cn)| q == cn)
                .unwrap_or(false)
        }
        RenameKind::Constant => {
            // Constants are always class-level: `Class.CONST`.
            let Some(qualifier) = prev_dot_qualifier else {
                return false;
            };
            rename
                .source_class_name
                .as_deref()
                .map(|cn| cn == qualifier)
                .unwrap_or(false)
        }
        RenameKind::Signal => {
            // Signals are commonly accessed on instances rather than classes:
            // `obj.signal_name.connect(handler)`, `obj.signal_name.emit(args)`,
            // `obj.signal_name.disconnect(...)`, `obj.signal_name.is_connected(...)`.
            // Allow rewriting when:
            //   1) qualifier is the source class_name (static-style access), OR
            //   2) the token is followed by `.connect|emit|disconnect|...`,
            //      a strong indicator the identifier is a signal.
            // We do NOT rewrite bare/unqualified or instance-qualified references
            // without a signal-method follow-up, since they could be unrelated
            // identifiers on objects of a different class.
            if !preceded_by_dot {
                return false;
            }
            if rename
                .source_class_name
                .as_deref()
                .map(|cn| Some(cn) == prev_dot_qualifier.as_deref())
                .unwrap_or(false)
            {
                return true;
            }
            is_signal_method_follow_up(tokens, idx)
        }
        RenameKind::EnumMember { enum_name } => {
            // Match `EnumName.MEMBER`. Optionally `SourceClass.EnumName.MEMBER`
            // (the second-level qualifier check covers both).
            let Some(qualifier) = prev_dot_qualifier else {
                return false;
            };
            let Some(en) = enum_name else {
                return false;
            };
            qualifier == en.as_str()
        }
        RenameKind::EnumName => {
            // EnumName access is always qualified: either `Class.EnumName` (a
            // class with `class_name`) or `Autoload.EnumName` (where the
            // autoload mapping isn't known to us). Accept any `.EnumName`
            // member access; the static-only constraint isn't safe for
            // autoloads, which don't have a class_name.
            preceded_by_dot
        }
        RenameKind::Class => {
            // A bare `OldClassName` may be a class reference OR something
            // unrelated (an enum member, a local variable, a parameter, …).
            // We only rewrite when the token sits in an unambiguous class-
            // referencing position: type hints (`: X`, `-> X`, `as X`,
            // `is X`, `extends X`), static access (`X.method`), or
            // constructor / call (`X.new(`, `X(`).
            !preceded_by_dot && is_class_reference_position(tokens, idx)
        }
        RenameKind::NodePath => false,
    }
}

/// Find same-file references to `old_name` that are safe to rewrite given the
/// rename `kind` and the file's `class_name`.
fn same_file_references(
    tokens: &[Token],
    old_name: &str,
    kind: &RenameKind,
    class_name: Option<&str>,
) -> Vec<(usize, usize)> {
    let mut out = Vec::new();

    // For variable/constant/signal/function renames, a bare reference is
    // only unambiguous when the file declares this name exactly once. If the
    // same name appears as a local var/param/func elsewhere in the file we
    // restrict ourselves to qualified references (`self.X`, `Class.X`) and
    // explicit calls.
    let has_collision = matches!(
        kind,
        RenameKind::Variable | RenameKind::Constant | RenameKind::Signal | RenameKind::Function
    ) && declaration_count(tokens, old_name) > 1;

    for (i, tok) in tokens.iter().enumerate() {
        let TokenKind::Identifier(ref name) = tok.kind else {
            continue;
        };
        if name != old_name {
            continue;
        }
        let qualifier = qualifier_before_dot(tokens, i);
        let preceded_by_dot = qualifier.is_some();
        let followed_by_paren = matches!(
            next_significant(tokens, i).map(|t| &t.kind),
            Some(TokenKind::LeftParen)
        );

        let allow = match kind {
            RenameKind::Function => {
                if followed_by_paren {
                    if !preceded_by_dot {
                        true
                    } else {
                        matches_self_or_class(qualifier.as_deref(), class_name)
                    }
                } else if preceded_by_dot {
                    // `obj.func` callable reference (Godot 4 first-class
                    // function refs). Allow if qualifier is self/class name.
                    matches_self_or_class(qualifier.as_deref(), class_name)
                } else {
                    // Bare `func_name` callable reference, e.g.
                    // `signal.connect(_on_x)` or `var cb := my_func`.
                    // Safe to rewrite only if no name collision in the file.
                    !has_collision
                }
            }
            RenameKind::Variable | RenameKind::Constant => {
                if preceded_by_dot {
                    matches_self_or_class(qualifier.as_deref(), class_name)
                } else {
                    !has_collision
                }
            }
            RenameKind::Signal => {
                if preceded_by_dot {
                    matches_self_or_class(qualifier.as_deref(), class_name)
                        || is_signal_method_follow_up(tokens, i)
                } else {
                    // Bare `signal_name.emit(...)` or `signal_name.connect(...)`
                    // is the in-class first-class access. Allow on collision-free
                    // signal-method patterns; otherwise leave bare references.
                    if is_signal_method_follow_up(tokens, i) {
                        true
                    } else {
                        !has_collision
                    }
                }
            }
            RenameKind::EnumMember { enum_name } => {
                let Some(qualifier) = qualifier.as_deref() else {
                    continue;
                };
                match enum_name {
                    Some(en) => qualifier == en,
                    None => false,
                }
            }
            RenameKind::Class | RenameKind::EnumName => !preceded_by_dot,
            RenameKind::NodePath => false,
        };
        if allow {
            out.push((tok.span.offset, tok.span.length));
        }
    }
    out
}

/// True when the identifier at `idx` sits in a position that unambiguously
/// references a class type: type hints, `extends`/`as`/`is`, static access,
/// constructor call, etc. Used by the Class rename rewriter to avoid
/// stomping on unrelated identifiers (e.g. enum members) that happen to
/// share a name with a renamed class.
fn is_class_reference_position(tokens: &[Token], idx: usize) -> bool {
    // Look at the next significant token: `.` means static access,
    // `(` means a call (constructor or function-style cast).
    let next = next_significant(tokens, idx);
    if matches!(
        next.map(|t| &t.kind),
        Some(TokenKind::Dot) | Some(TokenKind::LeftParen)
    ) {
        return true;
    }

    // Look at the previous significant token: type-position prefixes.
    let mut j = idx;
    while j > 0 {
        j -= 1;
        match &tokens[j].kind {
            TokenKind::Newline
            | TokenKind::Indent
            | TokenKind::Dedent
            | TokenKind::Comment(_)
            | TokenKind::DocComment(_) => continue,
            TokenKind::Colon
            | TokenKind::Arrow
            | TokenKind::Extends
            | TokenKind::As
            | TokenKind::Is
            | TokenKind::ClassName => return true,
            // After `class_name <Old>` (the declaration site): handled by the
            // diagnostic's own Replacement, not by the rewriter; skip here.
            _ => return false,
        }
    }
    false
}

/// True for tokens that carry no program structure: newlines, indentation
/// markers, and comments (line and doc). The token-walking helpers below all
/// skip these so a stray comment between two real tokens doesn't defeat
/// context matching.
fn is_trivia(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Newline
            | TokenKind::Indent
            | TokenKind::Dedent
            | TokenKind::Comment(_)
            | TokenKind::DocComment(_)
    )
}

/// True if the identifier token at `idx` is preceded (skipping trivia) by a
/// `.`, i.e. it appears in member-access position. Unlike
/// `qualifier_before_dot`, this returns true even when the dot is preceded
/// by `]`, `)`, another expression, or a chained access.
fn is_member_access(tokens: &[Token], idx: usize) -> bool {
    let mut j = idx;
    while j > 0 {
        j -= 1;
        if is_trivia(&tokens[j].kind) {
            continue;
        }
        return tokens[j].kind == TokenKind::Dot;
    }
    false
}

/// True if the identifier at `idx` is followed by `.<signal-method>(`, i.e. a
/// pattern like `signal_name.connect(...)` / `.emit(...)` / `.disconnect(...)`.
/// These first-class signal accesses are unambiguous indicators that the
/// identifier names a signal and can be safely rewritten on a rename.
fn is_signal_method_follow_up(tokens: &[Token], idx: usize) -> bool {
    let Some(next) = next_significant(tokens, idx) else {
        return false;
    };
    if !matches!(next.kind, TokenKind::Dot) {
        return false;
    }
    // Find position of the dot, then the next token.
    let dot_pos = tokens
        .iter()
        .enumerate()
        .skip(idx + 1)
        .find(|(_, t)| !is_trivia(&t.kind))
        .map(|(i, _)| i);
    let Some(dot_pos) = dot_pos else {
        return false;
    };
    let Some(method) = next_significant(tokens, dot_pos) else {
        return false;
    };
    if let TokenKind::Identifier(ref name) = method.kind {
        matches!(
            name.as_str(),
            "connect"
                | "disconnect"
                | "is_connected"
                | "emit"
                | "get_connections"
                | "has_connections"
        )
    } else {
        false
    }
}

fn matches_self_or_class(qualifier: Option<&str>, class_name: Option<&str>) -> bool {
    match qualifier {
        Some("self") => true,
        Some(q) => class_name.map(|cn| cn == q).unwrap_or(false),
        None => false,
    }
}

/// If the token at `idx` is preceded (skipping trivia) by a `.` and that `.`
/// is preceded by an Identifier, return that identifier's name: the
/// qualifier. Otherwise None.
fn qualifier_before_dot(tokens: &[Token], idx: usize) -> Option<String> {
    let mut j = idx;
    while j > 0 {
        j -= 1;
        if is_trivia(&tokens[j].kind) {
            continue;
        }
        match &tokens[j].kind {
            TokenKind::Dot => {
                // Found the dot. Look at what comes before.
                let mut k = j;
                while k > 0 {
                    k -= 1;
                    if is_trivia(&tokens[k].kind) {
                        continue;
                    }
                    return match &tokens[k].kind {
                        TokenKind::Identifier(name) => Some(name.clone()),
                        TokenKind::Self_ => Some("self".to_string()),
                        TokenKind::Super => Some("super".to_string()),
                        _ => None,
                    };
                }
                return None;
            }
            _ => return None,
        }
    }
    None
}

fn next_significant(tokens: &[Token], idx: usize) -> Option<&Token> {
    tokens
        .iter()
        .skip(idx + 1)
        .find(|tok| !is_trivia(&tok.kind))
}

/// Count declarations of `name` in `tokens`: `var name`, `const name`,
/// `signal name`, `func name`, `static var name`, plus function parameters
/// (`(name`, `, name`). Used to decide whether bare references in the same
/// file can be safely rewritten.
fn declaration_count(tokens: &[Token], name: &str) -> usize {
    let mut count = 0;
    for (i, tok) in tokens.iter().enumerate() {
        let is_decl_keyword = matches!(
            tok.kind,
            TokenKind::Var | TokenKind::Const | TokenKind::Signal | TokenKind::Func
        );
        if is_decl_keyword {
            // Walk forward, skipping `static` / annotations / colons, until
            // an identifier; if it matches, count.
            for next in tokens.iter().skip(i + 1) {
                match &next.kind {
                    TokenKind::Static | TokenKind::Annotation(_) => continue,
                    TokenKind::Identifier(n) => {
                        if n == name {
                            count += 1;
                        }
                        break;
                    }
                    _ => break,
                }
            }
        }
    }
    count
}

fn extract_class_name(members: &[ClassMember]) -> Option<String> {
    for m in members {
        if let ClassMember::ClassNameDecl { name, .. } = m {
            if !name.is_empty() {
                return Some(name.clone());
            }
        }
    }
    None
}

/// If `kind` is `EnumMember` with no enum_name yet, look up the parent enum by
/// searching the parsed members for an enum that contains a member with this
/// `old_name`. Returns the kind unchanged otherwise.
fn resolve_kind(kind: RenameKind, members: &[ClassMember], old_name: &str) -> RenameKind {
    match kind {
        RenameKind::EnumMember { enum_name: None } => RenameKind::EnumMember {
            enum_name: find_enum_for_member(members, old_name),
        },
        other => other,
    }
}

fn find_enum_for_member(members: &[ClassMember], member_old_name: &str) -> Option<String> {
    for m in members {
        match m {
            ClassMember::Enum {
                name, members: ems, ..
            } => {
                if ems.iter().any(|em| em.name == member_old_name) {
                    return name.clone();
                }
            }
            ClassMember::InnerClass { members: inner, .. } => {
                if let Some(n) = find_enum_for_member(inner, member_old_name) {
                    return Some(n);
                }
            }
            _ => {}
        }
    }
    None
}

fn collapse_blank_lines(source: &str) -> String {
    let mut result = String::with_capacity(source.len());
    let mut consecutive_empty = 0;
    for line in source.split('\n') {
        if line.trim().is_empty() {
            consecutive_empty += 1;
            if consecutive_empty <= 2 {
                if !result.is_empty() {
                    result.push('\n');
                }
                result.push_str(line);
            }
        } else {
            consecutive_empty = 0;
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str(line);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::{Diagnostic, DiagnosticSpan, Fix, Replacement, Severity};

    fn make_diag(offset: usize, length: usize, new_text: &str, is_safe: bool) -> Diagnostic {
        Diagnostic {
            rule: "test".to_string(),
            message: "test".to_string(),
            severity: Severity::Warning,
            span: DiagnosticSpan { line: 1, column: 1 },
            file: "test.gd".to_string(),
            fix: Some(Fix {
                replacements: vec![Replacement {
                    offset,
                    length,
                    new_text: new_text.to_string(),
                }],
                is_safe,
            }),
        }
    }

    #[test]
    fn apply_single_replacement() {
        let source = "var x = 5; var y = 10\n";
        let diags = vec![make_diag(9, 1, "\nvar y = 10", true)];
        let result = apply_fixes(source, &diags, true);
        assert_eq!(result, "var x = 5\nvar y = 10 var y = 10\n");
    }

    #[test]
    fn apply_insertion() {
        let source = "var x = 5";
        let diags = vec![make_diag(9, 0, "\n", true)];
        let result = apply_fixes(source, &diags, true);
        assert_eq!(result, "var x = 5\n");
    }

    #[test]
    fn apply_deletion() {
        let source = "var x = 5   \n";
        let diags = vec![make_diag(9, 3, "", true)];
        let result = apply_fixes(source, &diags, true);
        assert_eq!(result, "var x = 5\n");
    }

    #[test]
    fn safe_only_skips_unsafe() {
        let source = "hello";
        let diags = vec![make_diag(0, 5, "world", false)];
        let result = apply_fixes(source, &diags, true);
        assert_eq!(result, "hello"); // unchanged
    }

    #[test]
    fn unsafe_fix_applied_when_not_safe_only() {
        let source = "hello";
        let diags = vec![make_diag(0, 5, "world", false)];
        let result = apply_fixes(source, &diags, false);
        assert_eq!(result, "world");
    }

    #[test]
    fn multiple_non_overlapping_fixes() {
        let source = "&&||";
        let diags = vec![make_diag(0, 2, "and", true), make_diag(2, 2, "or", true)];
        let result = apply_fixes(source, &diags, true);
        assert_eq!(result, "andor");
    }

    #[test]
    fn overlapping_fixes_second_skipped() {
        let source = "abcdef";
        let diags = vec![
            make_diag(1, 3, "XX", true), // replace "bcd"
            make_diag(2, 2, "YY", true), // replace "cd" - overlaps!
        ];
        let result = apply_fixes(source, &diags, true);
        // One of them should be applied, the other skipped.
        assert!(result == "aXXef" || result == "abYYef");
    }

    #[test]
    fn no_fixes_returns_unchanged() {
        let source = "var x = 5\n";
        let diags: Vec<Diagnostic> = vec![];
        let result = apply_fixes(source, &diags, true);
        assert_eq!(result, source);
    }

    // Regression test for https://github.com/atelico/gdstyle/issues/24:
    // `lint_source` computes diagnostic offsets against an internally
    // LF-normalized copy of the source, so a CRLF file's own `\r` bytes must
    // not shift those offsets when the fix is applied back.
    #[test]
    fn apply_fixes_on_crlf_source_does_not_corrupt_offsets() {
        let source =
            "extends Node\r\n\r\nfunc _demo() -> void:\r\n\tvar w := _make({\"k\": 1})\r\n";
        let diagnostics =
            crate::linter::lint_source(source, "test.gd", &crate::config::Config::default());
        let fixed = apply_fixes(source, &diagnostics, true);
        assert_eq!(
            fixed,
            "extends Node\r\n\r\nfunc _demo() -> void:\r\n\tvar w := _make({ \"k\": 1 })\r\n"
        );
    }

    #[test]
    fn apply_fixes_preserves_crlf_line_endings_when_source_has_none_fixed() {
        let source = "extends Node\r\n\r\nfunc _demo() -> void:\r\n\tpass\r\n";
        let diagnostics =
            crate::linter::lint_source(source, "test.gd", &crate::config::Config::default());
        let fixed = apply_fixes(source, &diagnostics, true);
        assert_eq!(fixed, source);
    }
}
