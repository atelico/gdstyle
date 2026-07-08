use crate::ast::{ClassMember, ScriptFile};
use crate::diagnostic::Diagnostic;
use crate::linter::Suppressions;

/// Check that class members follow the canonical ordering.
///
/// The expected order is:
/// 1. @tool / @icon / @static_unload
/// 2. class_name
/// 3. extends
/// 4. Doc comments (class docstring)
/// 5. Signals
/// 6. Enums
/// 7. Constants
/// 8. Static variables
/// 9. @export variables
/// 10. Regular variables
/// 11. @onready variables
/// 12. Static methods
/// 13. Virtual/override methods (_init, _ready, etc.)
/// 14. Custom methods
/// 15. Inner classes
pub fn check_class_member_order(file: &ScriptFile, diagnostics: &mut Vec<Diagnostic>) {
    // Members carrying a `# gdstyle:ignore=order/class-member-order` directive
    // opt out of the ordering check. They are skipped entirely — neither
    // flagged nor counted towards the running "highest category seen" — so a
    // pinned member (e.g. a const kept next to the enum it mirrors) does not
    // make its neighbours look out of order. This mirrors how the formatter's
    // reorder pass pins the same members in place.
    let suppressions = Suppressions::parse(&file.lines.join("\n"));
    check_member_order_recursive(&file.members, &file.path, &suppressions, diagnostics);
}

fn check_member_order_recursive(
    members: &[ClassMember],
    file_path: &str,
    suppressions: &Suppressions,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut highest_category_seen: usize = 0;
    let mut highest_category_name: &str = "";
    for (i, member) in members.iter().enumerate() {
        let category = member.ordering_category();

        // Skip comments and blank lines (they don't affect ordering).
        if category == usize::MAX {
            continue;
        }

        // Skip members that opted out of the ordering rule.
        if is_order_ignored(member, suppressions) {
            continue;
        }

        // Skip doc comments that directly precede a real declaration.
        // These are attached to the next member and should not be checked
        // independently (e.g., ## above a static func between var declarations).
        if matches!(member, ClassMember::DocComment { .. }) {
            let is_attached = members[i + 1..].iter().any(|next| {
                match next {
                    ClassMember::BlankLine { .. } => false, // keep looking
                    ClassMember::Comment { .. } | ClassMember::DocComment { .. } => false,
                    _ => {
                        // Found the next real member, this doc comment is attached to it.
                        true
                    }
                }
            });
            if is_attached {
                continue;
            }
        }

        if category < highest_category_seen {
            diagnostics.push(Diagnostic::warning(
                "order/class-member-order",
                format!(
                    "{} should appear before {} (see GDScript style guide for ordering)",
                    member.category_name(),
                    highest_category_name
                ),
                member.span(),
                file_path,
            ));
        } else {
            highest_category_seen = category;
            highest_category_name = member.category_name();
        }

        // Recursively check inner classes.
        if let ClassMember::InnerClass { members: inner, .. } = member {
            check_member_order_recursive(inner, file_path, suppressions, diagnostics);
        }
    }
}

/// True when `member` carries a `# gdstyle:ignore` directive covering
/// `order/class-member-order` (or a file-level one). The directive may sit
/// directly above the declaration or above any of its leading annotations.
fn is_order_ignored(member: &ClassMember, suppressions: &Suppressions) -> bool {
    let decl_line = member.span().line;
    let header_start = match member {
        ClassMember::Variable { annotations, .. }
        | ClassMember::Function { annotations, .. }
        | ClassMember::StaticVariable { annotations, .. } => annotations
            .iter()
            .map(|a| a.span.line)
            .min()
            .map(|a| a.min(decl_line))
            .unwrap_or(decl_line),
        _ => decl_line,
    };
    suppressions.suppresses_member(header_start, decl_line, "order/class-member-order")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::*;
    use crate::token::Span;

    fn span(line: usize) -> Span {
        Span::new(line, 1, 0, 0)
    }

    #[test]
    fn correct_order_produces_no_diagnostics() {
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![],
            members: vec![
                ClassMember::ClassNameDecl {
                    name: "Test".to_string(),
                    name_span: span(1),
                    span: span(1),
                },
                ClassMember::ExtendsDecl {
                    base: "Node".to_string(),
                    span: span(2),
                },
                ClassMember::Signal {
                    name: "done".to_string(),
                    name_span: span(0),
                    parameters: vec![],
                    span: span(3),
                },
                ClassMember::Constant {
                    name: "MAX".to_string(),
                    name_span: span(4),
                    type_hint: None,
                    span: span(4),
                },
                ClassMember::Variable {
                    name: "speed".to_string(),
                    name_span: span(5),
                    type_hint: None,
                    annotations: vec![AnnotationInfo {
                        name: "export".to_string(),
                        span: span(5),
                    }],
                    span: span(5),
                },
                ClassMember::Variable {
                    name: "health".to_string(),
                    name_span: span(6),
                    type_hint: None,
                    annotations: vec![],
                    span: span(6),
                },
                ClassMember::Function {
                    name: "_ready".to_string(),
                    name_span: span(0),
                    parameters: vec![],
                    return_type: Some("void".to_string()),
                    is_static: false,
                    annotations: vec![],
                    body_line_count: 1,
                    span: span(7),
                },
                ClassMember::Function {
                    name: "custom".to_string(),
                    name_span: span(0),
                    parameters: vec![],
                    return_type: None,
                    is_static: false,
                    annotations: vec![],
                    body_line_count: 1,
                    span: span(8),
                },
            ],
        };

        let mut diags = Vec::new();
        check_class_member_order(&file, &mut diags);
        assert!(
            diags.is_empty(),
            "correct order should produce no diagnostics, got: {:?}",
            diags
        );
    }

    #[test]
    fn wrong_order_detected() {
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![],
            members: vec![
                ClassMember::Function {
                    name: "custom".to_string(),
                    name_span: span(0),
                    parameters: vec![],
                    return_type: None,
                    is_static: false,
                    annotations: vec![],
                    body_line_count: 1,
                    span: span(1),
                },
                // Signal after function is wrong.
                ClassMember::Signal {
                    name: "done".to_string(),
                    name_span: span(0),
                    parameters: vec![],
                    span: span(5),
                },
            ],
        };

        let mut diags = Vec::new();
        check_class_member_order(&file, &mut diags);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("signal declaration"));
        assert!(diags[0].message.contains("should appear before"));
    }

    #[test]
    fn extends_before_class_name_is_wrong() {
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![],
            members: vec![
                ClassMember::ExtendsDecl {
                    base: "Node".to_string(),
                    span: span(1),
                },
                ClassMember::ClassNameDecl {
                    name: "Test".to_string(),
                    name_span: span(2),
                    span: span(2),
                },
            ],
        };

        let mut diags = Vec::new();
        check_class_member_order(&file, &mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn variable_after_function_is_wrong() {
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![],
            members: vec![
                ClassMember::Function {
                    name: "_ready".to_string(),
                    name_span: span(1),
                    parameters: vec![],
                    return_type: None,
                    is_static: false,
                    annotations: vec![],
                    body_line_count: 1,
                    span: span(1),
                },
                ClassMember::Variable {
                    name: "health".to_string(),
                    name_span: span(5),
                    type_hint: None,
                    annotations: vec![],
                    span: span(5),
                },
            ],
        };

        let mut diags = Vec::new();
        check_class_member_order(&file, &mut diags);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn order_ignore_directive_exempts_member_and_neighbours() {
        // Layout (1-based lines):
        //   1  extends Node
        //   2  enum First {...}       (enum, category 5)
        //   3  # gdstyle:ignore=order/class-member-order
        //   4  const NAMES := [...]   (const, category 6, pinned)
        //   5  enum Second {...}      (enum, category 5)
        // The pinned const must not raise the ordering bar, so the trailing
        // enum (5) after it is not flagged.
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![
                "extends Node".to_string(),
                "enum First { A }".to_string(),
                "# gdstyle:ignore=order/class-member-order".to_string(),
                "const NAMES := [\"a\"]".to_string(),
                "enum Second { B }".to_string(),
            ],
            members: vec![
                ClassMember::ExtendsDecl {
                    base: "Node".to_string(),
                    span: span(1),
                },
                ClassMember::Enum {
                    name: Some("First".to_string()),
                    name_span: Some(span(2)),
                    members: vec![],
                    span: span(2),
                },
                ClassMember::Constant {
                    name: "NAMES".to_string(),
                    name_span: span(4),
                    type_hint: None,
                    span: span(4),
                },
                ClassMember::Enum {
                    name: Some("Second".to_string()),
                    name_span: Some(span(5)),
                    members: vec![],
                    span: span(5),
                },
            ],
        };

        let mut diags = Vec::new();
        check_class_member_order(&file, &mut diags);
        assert!(
            diags.is_empty(),
            "pinned const should exempt itself and its neighbours, got: {:?}",
            diags.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn order_violation_without_directive_still_flagged() {
        // Same shape as above but with no ignore directive: the trailing enum
        // after the const is a genuine ordering violation and must be reported.
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![
                "enum First { A }".to_string(),
                "const NAMES := [\"a\"]".to_string(),
                "enum Second { B }".to_string(),
            ],
            members: vec![
                ClassMember::Enum {
                    name: Some("First".to_string()),
                    name_span: Some(span(1)),
                    members: vec![],
                    span: span(1),
                },
                ClassMember::Constant {
                    name: "NAMES".to_string(),
                    name_span: span(2),
                    type_hint: None,
                    span: span(2),
                },
                ClassMember::Enum {
                    name: Some("Second".to_string()),
                    name_span: Some(span(3)),
                    members: vec![],
                    span: span(3),
                },
            ],
        };

        let mut diags = Vec::new();
        check_class_member_order(&file, &mut diags);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("enum declaration"));
    }

    #[test]
    fn doc_comment_before_declaration_not_flagged() {
        // Doc comments directly preceding a declaration should not be flagged
        // as ordering violations (they are attached to that declaration).
        let file = ScriptFile {
            path: "test.gd".to_string(),
            lines: vec![],
            members: vec![
                ClassMember::Variable {
                    name: "x".to_string(),
                    name_span: span(1),
                    type_hint: None,
                    annotations: vec![],
                    span: span(1),
                },
                ClassMember::DocComment {
                    text: "## Docs for the function".to_string(),
                    span: span(3),
                },
                ClassMember::Function {
                    name: "foo".to_string(),
                    name_span: span(4),
                    parameters: vec![],
                    return_type: None,
                    is_static: true,
                    annotations: vec![],
                    body_line_count: 1,
                    span: span(4),
                },
            ],
        };

        let mut diags = Vec::new();
        check_class_member_order(&file, &mut diags);
        assert!(
            diags.is_empty(),
            "doc comment attached to static func should not be flagged, got: {:?}",
            diags.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }
}
