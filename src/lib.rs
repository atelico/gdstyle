// Two clippy lints fire prolifically in our `match { Variant { … } => { if cond { … } } }`
// rule code. The "collapse" suggested rewrites move the condition into the
// match arm guard, which is harder to read alongside dozens of other arms;
// `map_or` collapsed onto an `Option<&T>` is similarly less clear than the
// explicit form when readers skim the rule files. Silence both crate-wide.
#![allow(clippy::collapsible_match, clippy::collapsible_if)]

pub mod ast;
pub mod config;
pub mod diagnostic;
pub mod fixer;
pub mod formatter;
pub mod lexer;
pub mod linter;
pub mod parser;
pub mod reporter;
pub mod rules;
pub mod token;
