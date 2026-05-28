# Contributing to gdstyle

Thanks for thinking about contributing. gdstyle is small enough that you can
read most of it in an afternoon, and contributions of all sizes are welcome:
bug reports with a minimal repro, rule suggestions, doc fixes, new rules, or
performance work.

## Reporting a bug

Open an issue with:

1. The smallest GDScript snippet that reproduces the bug.
2. What gdstyle did (paste the diagnostic or the formatter output).
3. What you expected it to do.
4. Your platform (Linux / macOS / Windows) and gdstyle version (`gdstyle --version`).

If gdstyle crashes, please include the stack trace or the panic message.

## Suggesting a new rule

Open an issue describing:

- A short example of the pattern the rule should flag.
- A short example of code the rule should *not* flag (the corner cases that
  make this rule hard to get right).
- A link or quote from the Godot or GDQuest style guide if the rule is based
  on either.

We can discuss in the issue before any code lands.

## Sending a pull request

1. Fork the repo and create a branch from `main`.
2. Make your change. Each PR should be one logical change.
3. Add a test. For new lint rules: at least one input that triggers the
   rule, one that doesn't, and one that exercises any autofix. For bug
   fixes: a regression test that fails on `main` and passes on your branch.
4. Run the full check locally:
   ```bash
   cargo test
   cargo clippy --release
   cargo fmt --check
   ```
5. If you touched any `.gd` file, run gdstyle on it:
   ```bash
   gdstyle check
   gdstyle fmt --check
   ```
6. Commit with a short, conventional-commit-style message
   (`fix: …`, `feat: …`, `docs: …`, `test: …`, `refactor: …`).
7. Push and open a PR. Describe what you changed and why. Reference any
   related issue.

## Project layout

A quick map so you can find the right place to land a change:

```
src/
├── lexer.rs               GDScript tokenizer
├── parser.rs              recursive-descent parser, AST
├── ast.rs                 ClassMember types
├── linter.rs              top-level lint entry point
├── formatter.rs           multi-pass formatter
├── fixer.rs               applies replacements + scene-aware renames
├── reporter.rs            text + JSON output
├── config.rs              TOML config loader
└── rules/
    ├── naming.rs          naming-convention rules
    ├── formatting.rs      formatter-level rules
    ├── ordering.rs        class member ordering
    ├── quality.rs         code quality / complexity rules
    └── mod.rs             rule registry
tests/
├── integration_test.rs    end-to-end tests
└── fixtures/              .gd files exercised by the integration tests
gdstyle-gdext/             GDExtension wrapper (Rust → GDScript)
godot-plugin/              Godot editor plugin source
```

For a new lint rule, the typical change touches `src/rules/<category>.rs` (the
check function) and `src/rules/mod.rs` (registration). Add a test in
`tests/integration_test.rs` and update the rule table in `README.md`.

## Code style

- Rust: `cargo fmt` and `cargo clippy` should pass clean.
- Errors: no `unwrap()` in non-test code without a comment explaining why
  it's unreachable.
- Public functions: a one-line doc comment minimum; an example is welcome.
- Tests: descriptive names (`fmt_normalises_spacing_in_class_header`,
  not `test1`).

## Releasing

Tags on `main` matching `v*` trigger the release workflow. Versioning follows
semver. Before tagging, bump:

- `Cargo.toml` `version`
- `gdstyle-gdext/Cargo.toml` `version`
- `godot-plugin/addons/gdstyle/plugin.cfg` `version`
- `RELEASE_NOTES.md`

Then `git tag -a vX.Y.Z -m "..."` and push the tag. The workflow builds
binaries for Linux, macOS (Intel + Apple Silicon), and Windows, plus the
Godot plugin zip, and attaches them to the GitHub release.

## License

By contributing, you agree your changes are licensed under MIT, the same
license as the rest of the project.
