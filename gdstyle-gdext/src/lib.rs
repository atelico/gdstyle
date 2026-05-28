use godot::classes::ProjectSettings;
use godot::prelude::*;

use gdstyle::config::Config;
use gdstyle::fixer;
use gdstyle::formatter;
use gdstyle::linter;
use gdstyle::rules;

struct GdStyleExtension;

#[gdextension]
unsafe impl ExtensionLibrary for GdStyleExtension {}

/// A GDScript-accessible linter and formatter powered by gdstyle.
///
/// Use from GDScript:
/// ```gdscript
/// var style = GdStyle.new()
/// var diagnostics = style.lint_file("res://player.gd")
/// for d in diagnostics:
///     print("Line %d: [%s] %s" % [d["line"], d["rule"], d["message"]])
/// ```
#[derive(GodotClass)]
#[class(base=RefCounted, tool)]
struct GdStyle {
    config: Config,
    base: Base<RefCounted>,
}

#[godot_api]
impl IRefCounted for GdStyle {
    fn init(base: Base<RefCounted>) -> Self {
        Self {
            config: Config::default(),
            base,
        }
    }
}

#[godot_api]
impl GdStyle {
    /// Lint a GDScript source string. Returns an array of diagnostic dictionaries.
    ///
    /// Each dictionary has keys: "rule", "message", "severity", "line", "column", "file".
    #[func]
    fn lint_source(&self, source: GString, file_path: GString) -> Array<VarDictionary> {
        let diagnostics =
            linter::lint_source(&source.to_string(), &file_path.to_string(), &self.config);
        diagnostics_to_array(&diagnostics)
    }

    /// Lint a GDScript file from disk. Pass a globalized (absolute) path.
    ///
    /// Returns an array of diagnostic dictionaries, or an empty array on read error.
    #[func]
    fn lint_file(&self, path: GString) -> Array<VarDictionary> {
        let path_str = path.to_string();
        let file_path = std::path::Path::new(&path_str);
        match linter::lint_file(file_path, &self.config) {
            Ok(diagnostics) => diagnostics_to_array(&diagnostics),
            Err(e) => {
                godot_error!("gdstyle: {}", e);
                Array::new()
            }
        }
    }

    /// Lint a file given its `res://` path. Automatically converts to a filesystem path.
    #[func]
    fn lint_res_file(&self, res_path: GString) -> Array<VarDictionary> {
        let globalized = ProjectSettings::singleton().globalize_path(&res_path);
        self.lint_file(globalized)
    }

    /// List all available rule names.
    #[func]
    fn list_rules(&self) -> PackedStringArray {
        let names = rules::all_rule_names();
        let mut arr = PackedStringArray::new();
        for name in names {
            arr.push(&GString::from(name));
        }
        arr
    }

    /// Set the maximum line length.
    #[func]
    fn set_max_line_length(&mut self, length: i64) {
        self.config.max_line_length = length.max(0) as usize;
    }

    /// Set the maximum function body length.
    #[func]
    fn set_max_function_length(&mut self, length: i64) {
        self.config.max_function_length = length.max(0) as usize;
    }

    /// Set the maximum file length.
    #[func]
    fn set_max_file_length(&mut self, length: i64) {
        self.config.max_file_length = length.max(0) as usize;
    }

    /// Set the maximum number of function parameters.
    #[func]
    fn set_max_parameters(&mut self, count: i64) {
        self.config.max_parameters = count.max(0) as usize;
    }

    /// Set whether to use tabs (true) or spaces (false) for indentation checking.
    #[func]
    fn set_use_tabs(&mut self, use_tabs: bool) {
        self.config.use_tabs = use_tabs;
    }

    /// Disable a specific rule by name.
    #[func]
    fn disable_rule(&mut self, rule_name: GString) {
        self.config.rules.insert(
            rule_name.to_string(),
            gdstyle::config::RuleSeverityConfig::Off,
        );
    }

    /// Enable a specific rule (removes any override, restoring the default).
    #[func]
    fn enable_rule(&mut self, rule_name: GString) {
        self.config.rules.remove(&rule_name.to_string());
    }

    /// Check if a specific rule is enabled.
    #[func]
    fn is_rule_enabled(&self, rule_name: GString) -> bool {
        self.config.is_rule_enabled(&rule_name.to_string())
    }

    /// Load configuration from a TOML file. Returns true on success.
    #[func]
    fn load_config(&mut self, path: GString) -> bool {
        let path_str = path.to_string();
        match Config::from_file(std::path::Path::new(&path_str)) {
            Ok(config) => {
                self.config = config;
                true
            }
            Err(e) => {
                godot_error!("gdstyle: failed to load config: {}", e);
                false
            }
        }
    }

    /// Load configuration from a `res://` path. Returns true on success.
    #[func]
    fn load_config_res(&mut self, res_path: GString) -> bool {
        let globalized = ProjectSettings::singleton().globalize_path(&res_path);
        self.load_config(globalized)
    }

    /// Reset configuration to defaults.
    #[func]
    fn reset_config(&mut self) {
        self.config = Config::default();
    }

    /// Format a GDScript source string. Returns the formatted source.
    #[func]
    fn format_source(&self, source: GString) -> GString {
        let formatted = formatter::format_source(&source.to_string(), &self.config);
        GString::from(&formatted)
    }

    /// Format a file on disk. Returns true if the file was changed.
    #[func]
    fn format_file(&self, path: GString) -> bool {
        let path_str = path.to_string();
        match formatter::format_file(std::path::Path::new(&path_str), &self.config) {
            Ok(changed) => changed,
            Err(e) => {
                godot_error!("gdstyle: {}", e);
                false
            }
        }
    }

    /// Format a file given its `res://` path. Returns true if the file was changed.
    #[func]
    fn format_res_file(&self, res_path: GString) -> bool {
        let globalized = ProjectSettings::singleton().globalize_path(&res_path);
        self.format_file(globalized)
    }

    /// Auto-fix safe lint violations in a source string. Returns the fixed source.
    #[func]
    fn fix_source(&self, source: GString, file_path: GString) -> GString {
        let src = source.to_string();
        let diagnostics = linter::lint_source(&src, &file_path.to_string(), &self.config);
        let fixed = fixer::apply_fixes(&src, &diagnostics, true);
        GString::from(&fixed)
    }

    /// Auto-fix all lint violations (including unsafe) in a source string.
    #[func]
    fn fix_source_unsafe(&self, source: GString, file_path: GString) -> GString {
        let src = source.to_string();
        let diagnostics = linter::lint_source(&src, &file_path.to_string(), &self.config);
        let fixed = fixer::apply_fixes(&src, &diagnostics, false);
        GString::from(&fixed)
    }

    /// Apply the single fix for the diagnostic at `(line, rule)` within the
    /// given source string and return the result. Returns the original source
    /// when no matching diagnostic has a fix attached. The editor plugin uses
    /// this for the right-click "Fix" action so a single buggy line can be
    /// repaired without touching the rest of the buffer.
    #[func]
    fn fix_one_in_source(
        &self,
        source: GString,
        file_path: GString,
        line: i64,
        rule: GString,
    ) -> GString {
        let src = source.to_string();
        let diagnostics = linter::lint_source(&src, &file_path.to_string(), &self.config);
        let rule_str = rule.to_string();
        let line_usize = line as usize;
        let matching: Vec<_> = diagnostics
            .into_iter()
            .filter(|d| d.span.line == line_usize && d.rule == rule_str && d.fix.is_some())
            .collect();
        if matching.is_empty() {
            return source;
        }
        let fixed = fixer::apply_fixes(&src, &matching, false);
        GString::from(&fixed)
    }

    /// Fix a single diagnostic identified by rule name and line number.
    ///
    /// Re-lints the file, finds the matching diagnostic, and applies only its fix.
    /// Returns true if a fix was applied and the file was written.
    #[func]
    fn fix_at_line(&self, res_path: GString, line: i64, rule: GString) -> bool {
        let globalized = ProjectSettings::singleton().globalize_path(&res_path);
        let path_str = globalized.to_string();
        let file_path = std::path::Path::new(&path_str);

        let source = match std::fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(e) => {
                godot_error!("gdstyle: cannot read file: {}", e);
                return false;
            }
        };

        let diagnostics = linter::lint_source(&source, &path_str, &self.config);
        let rule_str = rule.to_string();
        let line_usize = line as usize;

        // Filter to only the matching diagnostic.
        let matching: Vec<_> = diagnostics
            .into_iter()
            .filter(|d| d.span.line == line_usize && d.rule == rule_str && d.fix.is_some())
            .collect();

        if matching.is_empty() {
            return false;
        }

        let fixed = fixer::apply_fixes(&source, &matching, false);
        if fixed == source {
            return false;
        }

        match std::fs::write(file_path, &fixed) {
            Ok(()) => true,
            Err(e) => {
                godot_error!("gdstyle: cannot write file: {}", e);
                false
            }
        }
    }
}

/// Plain (godot-free) projection of a diagnostic into the fields the GDScript
/// side consumes. Kept separate from `diagnostics_to_array` so it can be unit
/// tested without a Godot runtime: the field/key mapping is exactly the bit
/// that silently breaks.
struct DiagnosticFields<'a> {
    rule: &'a str,
    message: &'a str,
    severity: String,
    line: i64,
    column: i64,
    file: &'a str,
    has_fix: bool,
    is_safe_fix: bool,
}

fn diagnostic_fields(d: &gdstyle::diagnostic::Diagnostic) -> DiagnosticFields<'_> {
    DiagnosticFields {
        rule: d.rule.as_str(),
        message: d.message.as_str(),
        severity: format!("{:?}", d.severity).to_lowercase(),
        line: d.span.line as i64,
        column: d.span.column as i64,
        file: d.file.as_str(),
        has_fix: d.fix.is_some(),
        is_safe_fix: d.fix.as_ref().is_some_and(|f| f.is_safe),
    }
}

fn diagnostics_to_array(diagnostics: &[gdstyle::diagnostic::Diagnostic]) -> Array<VarDictionary> {
    let mut arr = Array::new();
    for d in diagnostics {
        let f = diagnostic_fields(d);
        let mut dict = VarDictionary::new();
        dict.set("rule", GString::from(f.rule));
        dict.set("message", GString::from(f.message));
        dict.set("severity", GString::from(&f.severity));
        dict.set("line", f.line);
        dict.set("column", f.column);
        dict.set("file", GString::from(f.file));
        dict.set("has_fix", f.has_fix);
        dict.set("is_safe_fix", f.is_safe_fix);
        arr.push(&dict);
    }
    arr
}

#[cfg(test)]
mod tests {
    use super::diagnostic_fields;
    use gdstyle::diagnostic::{Diagnostic, Fix};
    use gdstyle::token::Span;

    #[test]
    fn diagnostic_fields_maps_a_plain_warning() {
        let d = Diagnostic::warning(
            "naming/function-name-snake-case",
            "function name 'F' should use snake_case".to_string(),
            Span::new(7, 3, 0, 1),
            "player.gd",
        );
        let f = diagnostic_fields(&d);
        assert_eq!(f.rule, "naming/function-name-snake-case");
        assert_eq!(f.severity, "warning");
        assert_eq!(f.line, 7);
        assert_eq!(f.column, 3);
        assert_eq!(f.file, "player.gd");
        assert!(!f.has_fix);
        assert!(!f.is_safe_fix);
    }

    #[test]
    fn diagnostic_fields_reports_error_severity_and_fix_flags() {
        let d = Diagnostic::error(
            "syntax/lex-error",
            "unterminated string".to_string(),
            Span::new(1, 1, 0, 1),
            "x.gd",
        )
        .with_fix(Fix {
            replacements: vec![],
            is_safe: true,
        });
        let f = diagnostic_fields(&d);
        assert_eq!(f.severity, "error");
        assert!(f.has_fix);
        assert!(f.is_safe_fix);
    }
}
