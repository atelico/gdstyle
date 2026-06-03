use crate::token::Span;

/// Represents a parsed GDScript file with just enough structure for linting.
#[derive(Debug)]
pub struct ScriptFile {
    pub path: String,
    pub members: Vec<ClassMember>,
    pub lines: Vec<String>,
}

/// A member of a class (top-level or inner class).
#[derive(Debug, Clone)]
pub enum ClassMember {
    ToolAnnotation {
        span: Span,
    },
    IconAnnotation {
        span: Span,
    },
    StaticUnloadAnnotation {
        span: Span,
    },
    /// Catch-all for top-level class annotations the parser doesn't
    /// recognise by keyword — `@abstract` today, and whatever Godot
    /// ships next. Emitted only when the annotation can't logically
    /// attach to a following declaration (e.g. it sits before
    /// `class_name`/`extends`). Sorted with the other class-level
    /// annotations (category 0).
    ClassAnnotation {
        name: String,
        span: Span,
    },
    ClassNameDecl {
        name: String,
        name_span: Span,
        span: Span,
    },
    ExtendsDecl {
        base: String,
        span: Span,
    },
    DocComment {
        text: String,
        span: Span,
    },
    Signal {
        name: String,
        name_span: Span,
        parameters: Vec<Parameter>,
        span: Span,
    },
    Enum {
        name: Option<String>,
        name_span: Option<Span>,
        members: Vec<EnumMember>,
        span: Span,
    },
    Constant {
        name: String,
        name_span: Span,
        type_hint: Option<String>,
        span: Span,
    },
    StaticVariable {
        name: String,
        name_span: Span,
        type_hint: Option<String>,
        annotations: Vec<AnnotationInfo>,
        span: Span,
    },
    Variable {
        name: String,
        name_span: Span,
        type_hint: Option<String>,
        annotations: Vec<AnnotationInfo>,
        span: Span,
    },
    Function {
        name: String,
        name_span: Span,
        parameters: Vec<Parameter>,
        return_type: Option<String>,
        is_static: bool,
        annotations: Vec<AnnotationInfo>,
        body_line_count: usize,
        span: Span,
    },
    InnerClass {
        name: String,
        name_span: Span,
        members: Vec<ClassMember>,
        span: Span,
    },
    Comment {
        text: String,
        is_doc: bool,
        span: Span,
    },
    BlankLine {
        span: Span,
    },
}

impl ClassMember {
    pub fn span(&self) -> Span {
        match self {
            ClassMember::ToolAnnotation { span }
            | ClassMember::IconAnnotation { span }
            | ClassMember::StaticUnloadAnnotation { span }
            | ClassMember::ClassAnnotation { span, .. }
            | ClassMember::ClassNameDecl { span, .. }
            | ClassMember::ExtendsDecl { span, .. }
            | ClassMember::DocComment { span, .. }
            | ClassMember::Signal { span, .. }
            | ClassMember::Enum { span, .. }
            | ClassMember::Constant { span, .. }
            | ClassMember::StaticVariable { span, .. }
            | ClassMember::Variable { span, .. }
            | ClassMember::Function { span, .. }
            | ClassMember::InnerClass { span, .. }
            | ClassMember::Comment { span, .. }
            | ClassMember::BlankLine { span } => *span,
        }
    }

    /// Returns the ordering category for class member ordering checks.
    /// Lower values should appear earlier in the file.
    pub fn ordering_category(&self) -> usize {
        match self {
            ClassMember::ToolAnnotation { .. }
            | ClassMember::IconAnnotation { .. }
            | ClassMember::StaticUnloadAnnotation { .. }
            | ClassMember::ClassAnnotation { .. } => 0,
            ClassMember::ClassNameDecl { .. } => 1,
            ClassMember::ExtendsDecl { .. } => 2,
            ClassMember::DocComment { .. } => 3,
            ClassMember::Signal { .. } => 4,
            ClassMember::Enum { .. } => 5,
            ClassMember::Constant { .. } => 6,
            ClassMember::StaticVariable { .. } => 7,
            ClassMember::Variable { annotations, .. } => {
                if annotations
                    .iter()
                    .any(|a| a.name == "export" || a.name.starts_with("export_"))
                {
                    8
                } else if annotations.iter().any(|a| a.name == "onready") {
                    10
                } else {
                    9
                }
            }
            ClassMember::Function { name, .. } => {
                // Godot's official style guide orders methods as: virtual /
                // override methods (_init, _ready, …) first, then all other
                // methods. It does NOT carve out a slot for `static` methods:
                // a static factory is just a public method and may sit
                // anywhere among the regular methods.
                if is_virtual_method(name) {
                    11
                } else {
                    12
                }
            }
            ClassMember::InnerClass { .. } => 13,
            ClassMember::Comment { .. } | ClassMember::BlankLine { .. } => {
                // Comments and blank lines don't affect ordering.
                usize::MAX
            }
        }
    }

    /// Returns the last line (1-indexed) occupied by this member.
    ///
    /// For single-line members this equals `span().line`. For functions
    /// it accounts for the body length.
    pub fn end_line(&self) -> usize {
        match self {
            ClassMember::Function {
                body_line_count,
                span,
                ..
            } => {
                // +1 for the `func` signature line itself.
                span.line + body_line_count
            }
            ClassMember::InnerClass { members, span, .. } => {
                if let Some(last) = members.last() {
                    last.end_line()
                } else {
                    span.line
                }
            }
            _ => self.span().line,
        }
    }

    /// Returns a human-readable category name for diagnostics.
    pub fn category_name(&self) -> &'static str {
        match self {
            ClassMember::ToolAnnotation { .. }
            | ClassMember::IconAnnotation { .. }
            | ClassMember::StaticUnloadAnnotation { .. }
            | ClassMember::ClassAnnotation { .. } => {
                "script annotation (@tool/@icon/@static_unload/@abstract)"
            }
            ClassMember::ClassNameDecl { .. } => "class_name declaration",
            ClassMember::ExtendsDecl { .. } => "extends declaration",
            ClassMember::DocComment { .. } => "documentation comment",
            ClassMember::Signal { .. } => "signal declaration",
            ClassMember::Enum { .. } => "enum declaration",
            ClassMember::Constant { .. } => "constant declaration",
            ClassMember::StaticVariable { .. } => "static variable",
            ClassMember::Variable { annotations, .. } => {
                if annotations
                    .iter()
                    .any(|a| a.name == "export" || a.name.starts_with("export_"))
                {
                    "@export variable"
                } else if annotations.iter().any(|a| a.name == "onready") {
                    "@onready variable"
                } else {
                    "variable declaration"
                }
            }
            ClassMember::Function {
                is_static, name, ..
            } => {
                if *is_static {
                    "static method"
                } else if is_virtual_method(name) {
                    "virtual/override method"
                } else {
                    "method"
                }
            }
            ClassMember::InnerClass { .. } => "inner class",
            ClassMember::Comment { .. } => "comment",
            ClassMember::BlankLine { .. } => "blank line",
        }
    }
}

/// Walk every class member in `members`, recursing into inner classes.
/// Visits members in source order (and inner-class members immediately
/// after their containing class header). The closure runs on every member
/// regardless of kind; filter on the kind inside the closure.
///
/// Replaces the dozen `check_*_recursive(members) { for m { match { …
/// InnerClass => recurse, _ => {} } } }` boilerplate functions across
/// the rule modules.
pub fn for_each_member<F: FnMut(&ClassMember)>(members: &[ClassMember], mut visit: F) {
    fn walk<F: FnMut(&ClassMember)>(members: &[ClassMember], visit: &mut F) {
        for m in members {
            visit(m);
            if let ClassMember::InnerClass { members: inner, .. } = m {
                walk(inner, visit);
            }
        }
    }
    walk(members, &mut visit);
}

fn is_virtual_method(name: &str) -> bool {
    matches!(
        name,
        "_init"
            | "_enter_tree"
            | "_exit_tree"
            | "_ready"
            | "_process"
            | "_physics_process"
            | "_input"
            | "_unhandled_input"
            | "_unhandled_key_input"
            | "_notification"
            | "_draw"
            | "_gui_input"
            | "_get_configuration_warnings"
            | "_static_init"
    )
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub type_hint: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct EnumMember {
    pub name: String,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct AnnotationInfo {
    pub name: String,
    pub span: Span,
}
