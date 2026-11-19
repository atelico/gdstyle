//! Random token lines with random spacing, to catch rules whose fixes undo
//! each other. Such a pair makes `fmt` flip between two layouts on every
//! run, and leaves `check --fix` reporting a fix it cannot apply. Most of
//! the lines are invalid GDScript: the parser accepts them, so the
//! formatter must settle on them too.
//!
//! The generator is seeded, so every run checks the same lines.

use gdstyle::config::Config;
use gdstyle::{fixer, formatter, linter};

/// Tokens the spacing rules treat differently: operators, brackets,
/// separators, prefixes and keywords that are also names.
const VOCABULARY: &[&str] = &[
    "a", "b", "1", "2.5", "\"s\"", "+", "-", "*", "/", "==", "=", "and", "or", "not", "!", "~",
    "->", "(", ")", "[", "]", "{", "}", ",", ":", ";", ".", "$N", "%M", "await", "when", "match",
    "in", "self", "f(", "if", "return", ":=", "**", "<", ">=", "&&", "#c", "\\",
];

const SPACING: &[&str] = &["", " ", "   ", "\t"];

/// xorshift64: small, deterministic, and enough to spread the cases.
struct Generator(u64);

impl Generator {
    fn below(&mut self, bound: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % bound as u64) as usize
    }

    /// A function whose second line holds 3 to 8 random tokens.
    fn script(&mut self) -> String {
        let mut line = String::new();
        for _ in 0..3 + self.below(6) {
            line.push_str(VOCABULARY[self.below(VOCABULARY.len())]);
            line.push_str(SPACING[self.below(SPACING.len())]);
        }
        let line = line.trim_end().replace('\\', "\\\n\t\t");
        format!("func f(a, b):\n\tvar x = {line}\n\tpass\n")
    }
}

fn config_with_rule_off(rule: &str) -> Config {
    let mut config = Config::default();
    config
        .rules
        .insert(rule.to_string(), gdstyle::config::RuleSeverityConfig::Off);
    config
}

/// Fails with the first script on which `fmt` does not settle, or after
/// which `check --fix` still holds a safe fix.
fn assert_fixes_settle(config: &Config, seed: u64, cases: usize) {
    let mut generator = Generator(seed);
    for _ in 0..cases {
        let source = generator.script();
        let formatted = formatter::format_source(&source, config);
        assert_eq!(
            formatter::format_source(&formatted, config),
            formatted,
            "fmt does not settle on:\n{source}"
        );

        let diagnostics = linter::lint_source(&source, "random.gd", config);
        let remaining =
            fixer::apply_fixes_until_settled(&source, &diagnostics, "random.gd", config, true)
                .map_or(diagnostics, |(_, remaining)| remaining);
        let unapplied: Vec<&str> = remaining
            .iter()
            .filter(|d| d.fix.as_ref().is_some_and(|fix| fix.is_safe))
            .map(|d| d.rule.as_str())
            .collect();
        assert!(
            unapplied.is_empty(),
            "safe fixes {unapplied:?} are left after --fix on:\n{source}"
        );
    }
}

#[test]
fn spacing_fixes_settle_on_random_lines() {
    assert_fixes_settle(&Config::default(), 0x9E37_79B9_7F4A_7C15, 3000);
}

#[test]
fn spacing_fixes_settle_with_each_spacing_rule_off() {
    // A rule that is off hands its gaps to format/extra-spacing, which must
    // then agree with the rules still on.
    for rule in [
        "format/operator-spacing",
        "format/comma-spacing",
        "format/colon-spacing",
        "format/brace-spacing",
        "format/call-paren-spacing",
        "format/one-statement-per-line",
        "format/boolean-operators",
        "format/extra-spacing",
    ] {
        assert_fixes_settle(&config_with_rule_off(rule), 7, 500);
    }
}

#[test]
fn comma_and_colon_attach_left_even_after_an_operator() {
    // Each line used to flip between the two layouts on every `fmt` run:
    // one rule removed the space before `,` / `:` and another put a space
    // after the token before it.
    for (line, expected) in [
        ("var x = , 1", "var x =, 1"),
        ("var x = : 1", "var x =: 1"),
        ("var x = a * : b", "var x = a *: b"),
        ("var x = [1, :]", "var x = [1,:]"),
        ("var x = {: 1}", "var x = {: 1 }"),
    ] {
        let source = format!("func f(a):\n\t{line}\n\tpass\n");
        let formatted = formatter::format_source(&source, &Config::default());
        assert_eq!(
            formatted.lines().nth(1),
            Some(format!("\t{expected}").as_str()),
            "{line}"
        );
        assert_eq!(
            formatter::format_source(&formatted, &Config::default()),
            formatted
        );
    }
}

#[test]
fn inferred_type_operator_keeps_the_space_before_it() {
    let source = "func f(a):\n\tvar x := 1\n\tvar y = a if a else b\n";
    assert_eq!(formatter::format_source(source, &Config::default()), source);
}

#[test]
fn operators_space_before_a_colon_when_colon_spacing_is_off() {
    // Nothing removes the space before `:` then, so the operator gets its
    // space and both spellings settle on the same layout.
    let config = config_with_rule_off("format/colon-spacing");
    for line in ["var x = a *:b", "var x = a * :b"] {
        let source = format!("func f(a):\n\t{line}\n\tpass\n");
        let formatted = formatter::format_source(&source, &config);
        assert_eq!(formatted.lines().nth(1), Some("\tvar x = a * :b"), "{line}");
    }
}
