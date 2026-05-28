# CLAUDE.md

## Project overview

gdstyle is a fast, opinionated linter and formatter for GDScript (Godot 4.x), written in Rust. Single binary, zero runtime dependencies.

## Build & test

```bash
cargo build --release    # build
cargo test               # run all tests
cargo clippy             # lint Rust code
cargo fmt                # format Rust code
```

## Architecture

Pipeline: source text → lexer (`src/lexer.rs`) → tokens (`src/token.rs`) → parser (`src/parser.rs`) → AST (`src/ast.rs`) → linter/formatter/fixer.

Key modules:
- `src/lexer.rs`, tokenizer for GDScript
- `src/parser.rs`, recursive-descent parser producing AST nodes
- `src/ast.rs`, AST node types
- `src/linter.rs`, runs lint rules against the AST
- `src/rules/`, lint rules organized by category (naming, formatting, ordering, quality)
- `src/formatter.rs`, GDScript code formatter (multi-pass, idempotent)
- `src/fixer.rs`, auto-fix engine (safe and unsafe fixes, cross-file renaming)
- `src/config.rs`, TOML configuration
- `src/diagnostic.rs`, diagnostic/warning types
- `src/reporter.rs`, text and JSON output
- `src/main.rs`, CLI entry point (clap)

## Conventions

- Test fixtures live in `tests/fixtures/`
- Integration tests in `tests/`
- Config format is TOML (`gdstyle.toml`)
- 51 lint rules total; most enabled by default (3 advisory rules are opt-in)
