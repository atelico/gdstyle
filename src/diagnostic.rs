use crate::token::Span;

/// Severity level for a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Warning,
    Error,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Warning => write!(f, "warning"),
            Severity::Error => write!(f, "error"),
        }
    }
}

/// A single text replacement for auto-fixing.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Replacement {
    pub offset: usize,
    pub length: usize,
    pub new_text: String,
}

/// An auto-fix that can be applied to resolve a diagnostic.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Fix {
    pub replacements: Vec<Replacement>,
    pub is_safe: bool,
}

/// A lint diagnostic emitted by a rule.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Diagnostic {
    pub rule: String,
    pub message: String,
    pub severity: Severity,
    pub span: DiagnosticSpan,
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<Fix>,
}

/// Serializable span information.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DiagnosticSpan {
    pub line: usize,
    pub column: usize,
}

impl From<Span> for DiagnosticSpan {
    fn from(span: Span) -> Self {
        Self {
            line: span.line,
            column: span.column,
        }
    }
}

impl Diagnostic {
    pub fn warning(rule: &str, message: String, span: Span, file: &str) -> Self {
        Self {
            rule: rule.to_string(),
            message,
            severity: Severity::Warning,
            span: span.into(),
            file: file.to_string(),
            fix: None,
        }
    }

    pub fn error(rule: &str, message: String, span: Span, file: &str) -> Self {
        Self {
            rule: rule.to_string(),
            message,
            severity: Severity::Error,
            span: span.into(),
            file: file.to_string(),
            fix: None,
        }
    }

    pub fn with_fix(mut self, fix: Fix) -> Self {
        self.fix = Some(fix);
        self
    }

    /// Convenience constructor for the common single-replacement fix shape.
    /// Avoids the multi-line `Diagnostic::warning(...).with_fix(Fix {
    /// replacements: vec![Replacement { ... }], is_safe })` boilerplate that
    /// appears in dozens of rules.
    #[allow(clippy::too_many_arguments)] // deliberate: bundles 4 small args of a fix together
    pub fn warning_with_fix(
        rule: &str,
        message: String,
        span: Span,
        file: &str,
        offset: usize,
        length: usize,
        new_text: String,
        is_safe: bool,
    ) -> Self {
        Self::warning(rule, message, span, file).with_fix(Fix {
            replacements: vec![Replacement {
                offset,
                length,
                new_text,
            }],
            is_safe,
        })
    }
}

/// Compute the byte offset of the start of a given line (0-indexed line_idx).
pub fn line_byte_offset(lines: &[String], line_idx: usize) -> usize {
    lines[..line_idx].iter().map(|l| l.len() + 1).sum()
}
