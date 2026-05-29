# gdstyle

<video src="https://github.com/user-attachments/assets/314e0f55-33e2-4365-bef8-87cf4fdaaa1e" controls autoplay loop muted playsinline width="900">
  gdstyle running in the Godot editor: linting, format-on-save, and right-click single-fix on the bottom panel.
</video>

A fast, opinionated linter and formatter for GDScript (Godot 4.x), built in Rust.

gdstyle catches style violations, naming inconsistencies, and common code-quality issues, and auto-formats GDScript to the [official Godot style guide](https://docs.godotengine.org/en/stable/tutorials/scripting/gdscript/gdscript_styleguide.html). Many of the conventions are taken from Nathan Lovato and [GDQuest's GDScript style guide](https://gdquest.gitbook.io/gdquests-guidelines).

## Features

- 54 lint rules across syntax, naming, formatting, ordering, and code quality.
- Formatter (`gdstyle fmt`) that's in-place and idempotent, and reorders class members into the canonical Godot order.
- Auto-fix: `--fix` for the safe ones, `--unsafe-fix` for renames. Renames follow into other `.gd` files and into the `.tscn`/`.tres` scene wiring.
- Single static binary. No Python, no Rust toolchain, no Godot install required to run it.
- Text and JSON output with configurable exit codes, so it slots into CI.
- TOML config (`gdstyle.toml`) with per-rule overrides and CLI flags for one-off tweaks.
- Inline suppression with `# gdstyle:ignore` comments.
- Usable as a Rust library, not just a CLI.

## Installation

Pre-built binaries are available for all major platforms. You don't need a Rust toolchain unless you want to build from source.

### Pre-built binaries (recommended)

1. Go to the [latest release](https://github.com/atelico/gdstyle/releases/latest)
2. Download the archive for your platform:
   - **Linux**: `gdstyle-x86_64-unknown-linux-gnu.tar.gz`
   - **macOS (Intel)**: `gdstyle-x86_64-apple-darwin.tar.gz`
   - **macOS (Apple Silicon)**: `gdstyle-aarch64-apple-darwin.tar.gz`
   - **Windows**: `gdstyle-x86_64-pc-windows-msvc.zip`
3. Extract the `gdstyle` binary and place it somewhere on your `PATH`:

```bash
# Example for Linux / macOS
tar xzf gdstyle-*.tar.gz
cp gdstyle ~/.local/bin/   # or /usr/local/bin/, or anywhere on your PATH
```

### Building from source

To build from source you need a [Rust toolchain](https://rustup.rs/).

```bash
git clone https://github.com/atelico/gdstyle.git
cd gdstyle
cargo build --release

# The binary is at target/release/gdstyle
cp target/release/gdstyle ~/.local/bin/
```

### From crates.io

```bash
cargo install gdstyle
```

## Quick start

```bash
# Lint the current directory (recursively finds all .gd files)
gdstyle

# Lint specific files or directories
gdstyle check src/player.gd src/enemies/

# Auto-fix safe violations
gdstyle check --fix

# Auto-fix all violations including unsafe ones
gdstyle check --unsafe-fix

# Format all GDScript files in place
gdstyle fmt

# Check formatting without modifying files (exit 1 if changes needed)
gdstyle fmt --check

# Show formatting diff without modifying files
gdstyle fmt --diff

# Output lint results as JSON (for CI integration)
gdstyle check --format json

# List all available rules
gdstyle rules

# Generate a starter config file
gdstyle init

# Only check naming rules
gdstyle check --select naming

# Ignore specific rules
gdstyle check --ignore "format/max-line-length,format/double-quotes"

# Override max line length
gdstyle check --max-line-length 120
```

## Rules

gdstyle ships with 54 rules organized into five categories. Most rules are enabled by default (a few advisory rules are opt-in).

### Syntax (1 rule)

| Rule | Description | Fixable |
|------|-------------|---------|
| `syntax/lex-error` | Report lexer errors: unterminated strings, invalid numbers, unexpected characters | - |

### Naming (11 rules)

| Rule | Description | Fixable |
|------|-------------|---------|
| `naming/class-name-pascal-case` | Class names must use `PascalCase` | unsafe |
| `naming/function-name-snake-case` | Function names must use `snake_case` | unsafe |
| `naming/variable-name-snake-case` | Variable names must use `snake_case` | unsafe |
| `naming/constant-name-screaming-case` | Constants must use `SCREAMING_SNAKE_CASE` (or `PascalCase` for preloads) | unsafe |
| `naming/signal-name-snake-case` | Signal names must use `snake_case` | unsafe |
| `naming/enum-name-pascal-case` | Enum type names must use `PascalCase` | unsafe |
| `naming/enum-member-screaming-case` | Enum members must use `SCREAMING_SNAKE_CASE` | unsafe |
| `naming/file-name-snake-case` | File names must use `snake_case` | - |
| `naming/signal-past-tense` | Signal names should use past tense (handles irregular verbs, gerunds, nouns) | unsafe |
| `naming/private-underscore-prefix` | Private members with `_` should not have `@export` | - |
| `naming/node-name-pascal-case` | `$NodePath` references should use `PascalCase` | unsafe |

### Formatting (18 rules)

| Rule | Description | Fixable |
|------|-------------|---------|
| `format/max-line-length` | Lines must not exceed the configured max length (default: 100) | fmt |
| `format/trailing-whitespace` | No trailing whitespace on any line | safe |
| `format/trailing-newline` | Files must end with a newline character | safe |
| `format/no-tabs-as-spaces` | Indentation must use tabs (configurable to spaces) | safe |
| `format/boolean-operators` | Use `and`/`or`/`not` instead of `&&`/`\|\|`/`!` | safe |
| `format/double-quotes` | Prefer double quotes for strings | safe |
| `format/comment-spacing` | Comments must have a space after `#` | safe |
| `format/no-unnecessary-parens` | No unnecessary parentheses in `if`/`while`/`elif` conditions | safe |
| `format/number-literals` | Hex digits must be lowercase (`0xff`, not `0xFF`) | safe |
| `format/one-statement-per-line` | One statement per line (no semicolons to separate statements) | safe |
| `format/blank-lines` | Collapse 3+ blank lines to 2 | safe |
| `format/trailing-comma` | Trailing comma on last item of multi-line collections | safe |
| `format/operator-spacing` | One space around binary operators | safe |
| `format/colon-spacing` | No space before `:`, one space after (except `:=` and end of line) | safe |
| `format/comma-spacing` | No space before `,`, one space after (except newline / closing bracket) | safe |
| `format/float-literal-zeros` | Float literals need leading/trailing zeros (`0.5`, not `.5`) | safe |
| `format/large-number-underscores` | Large numbers (>=10000) should use underscores | safe |
| `format/enum-one-per-line` | Each enum member on its own line | safe |

### Ordering (1 rule)

| Rule | Description | Fixable |
|------|-------------|---------|
| `order/class-member-order` | Class members must follow the canonical Godot ordering | fmt |

The canonical ordering enforced by `order/class-member-order` is:

1. `@tool`
2. `@icon`
3. `class_name`
4. `extends`
5. Doc comments (`##`)
6. Signals
7. Enums
8. Constants
9. Static variables
10. `@export` variables
11. Regular variables
12. `@onready` variables
13. Virtual methods (`_init`, `_ready`, `_process`, etc.)
14. Regular methods
15. Inner classes

### Quality (23 rules)

| Rule | Description | Default | Fixable |
|------|-------------|---------|---------|
| `quality/max-function-length` | Functions must not exceed the configured max body length (default: 50 lines) | on | - |
| `quality/max-file-length` | Files must not exceed the configured max length (default: 1000 lines) | on | - |
| `quality/max-parameters` | Functions must not have more than the configured max parameters (default: 5) | on | - |
| `quality/unnecessary-pass` | `pass` alongside other statements is unnecessary | on | - |
| `quality/no-debug-print` | Debug `print()`/`prints()`/`printerr()` calls left in code | **off** | - |
| `quality/self-comparison` | Comparing a value with itself (`x == x`) | on | - |
| `quality/no-self-assign` | Self-assignment (`x = x`) | on | - |
| `quality/duplicate-dict-key` | Duplicate keys in dictionary literals | on | - |
| `quality/duplicated-load` | Same path passed to `load()`/`preload()` multiple times | on | - |
| `quality/type-hint` | Missing type hints on variables, parameters, and return types | **off** | - |
| `quality/empty-function` | Empty or pass-only functions | **off** | - |
| `quality/max-class-variables` | Too many class-level variables (default: 15) | on | - |
| `quality/max-public-methods` | Too many public methods per class (default: 20) | on | - |
| `quality/max-inner-classes` | Too many inner classes per file (default: 5) | on | - |
| `quality/no-else-return` | Unnecessary `else`/`elif` after `return` | on | - |
| `quality/unreachable-code` | Code after `return`, `break`, or `continue` | on | - |
| `quality/await-in-loop` | `await` used inside a `for`/`while` loop | on | - |
| `quality/allocation-in-loop` | Object allocation (`.new()`) inside a loop | on | - |
| `quality/process-get-node` | Node lookups (`$`, `get_node()`) in `_process`/`_physics_process` | on | - |
| `quality/max-nesting-depth` | Nesting depth exceeds limit (default: 4) | on | - |
| `quality/max-returns` | Too many `return` statements per function (default: 6) | on | - |
| `quality/max-branches` | Too many branches (`if`/`elif`/`match`) per function (default: 8) | on | - |
| `quality/max-local-variables` | Too many local variables per function (default: 10) | on | - |

## Auto-fix

gdstyle can automatically fix many violations:

```bash
# Fix safe violations only (formatting, naming conventions)
gdstyle check --fix

# Fix all violations including unsafe ones (signal renaming, member reordering)
gdstyle check --unsafe-fix
```

> **Fixes are written to disk in place.** There's no backup and no
> confirmation prompt. Commit or stash before running `--fix` or
> `--unsafe-fix`, then review the diff. (`gdstyle fmt --diff` previews
> formatter changes without writing.)

Safe fixes preserve behavior. Unsafe fixes can change semantics (renaming signals, variables, and other identifiers), so review the diff before committing.

When `--unsafe-fix` renames an identifier, gdstyle follows the rename across every `.gd` file in the project and into the `.tscn`/`.tres` scene files that wire signals or methods to that name. Anything it can't safely rewrite is reported as a warning so you can fix it by hand.

## Formatter

The `fmt` subcommand reformats GDScript files in a single pass:

```bash
# Format all .gd files in place
gdstyle fmt

# Check if files are already formatted (exit 1 if not)
gdstyle fmt --check

# Show a diff of what would change
gdstyle fmt --diff
```

The formatter normalizes indentation, trailing whitespace, blank lines (including the spacing between class members), boolean operators (`&&`/`||`/`!` to `and`/`or`/`not`), string quotes, comment spacing, colon and comma spacing, float and hex literals, trailing newlines, and single-line enums (expanded to multi-line). It reorders class members to match the canonical style guide order, and wraps long lines at commas inside parentheses, brackets, and braces. Running it twice produces the same output.

> **Note:** Line wrapping breaks at comma boundaries inside delimiters (parentheses, brackets, braces), `and`/`or` operators in `if`/`elif`/`while` conditions, and word boundaries in long comments. Lines without any breakable pattern (e.g., long strings, property chains, continuation lines without enclosing delimiters) are left alone.

## Configuration

gdstyle looks for a config file named `gdstyle.toml` or `.gdstyle.toml` in the current directory and walks up the directory tree until it finds one. You can also specify a config file explicitly with `--config`, or generate a starter one:

```bash
gdstyle init
```

### Example config file

```toml
# gdstyle.toml

# Maximum line length (default: 100)
max_line_length = 100

# Use tabs for indentation (default: true)
use_tabs = true

# Maximum function body length in lines (default: 50)
max_function_length = 50

# Maximum file length in lines (default: 1000)
max_file_length = 1000

# Maximum number of function parameters (default: 5)
max_parameters = 5

# Maximum return statements per function (default: 6)
max_returns = 6

# Maximum nesting depth inside a function (default: 4)
max_nesting_depth = 4

# Maximum local variables per function (default: 10)
max_local_variables = 10

# Maximum branches (if/elif/match) per function (default: 8)
max_branches = 8

# Maximum class-level variables (default: 15)
max_class_variables = 15

# Maximum public methods per class (default: 20)
max_public_methods = 20

# Maximum inner classes per file (default: 5)
max_inner_classes = 5

# File/directory patterns to exclude
exclude = [".godot", "addons"]

# Per-rule severity overrides
# Values: "off", "warn", "error"
[rules]
"format/double-quotes" = "off"             # Disable a rule entirely
"naming/class-name-pascal-case" = "error"   # Escalate to error
"quality/max-function-length" = "warn"      # Keep as warning
"quality/no-debug-print" = "warn"          # Enable an off-by-default rule
```

### Default configuration

When no config file is found, gdstyle uses these defaults:

| Setting | Default |
|---------|---------|
| `max_line_length` | 100 |
| `use_tabs` | `true` |
| `max_function_length` | 50 |
| `max_file_length` | 1000 |
| `max_parameters` | 5 |
| `max_returns` | 6 |
| `max_nesting_depth` | 4 |
| `max_local_variables` | 10 |
| `max_branches` | 8 |
| `max_class_variables` | 15 |
| `max_public_methods` | 20 |
| `max_inner_classes` | 5 |
| `exclude` | `[".godot", "addons"]` |

Most rules are enabled by default with `warn` severity. Three advisory rules (`quality/type-hint`, `quality/empty-function`, `quality/no-debug-print`) are off by default and must be explicitly enabled.

## Inline suppression

You can suppress warnings on specific lines using `# gdstyle:ignore` comments.

### Suppress all rules on the next line

```gdscript
# gdstyle:ignore
var BadName: int = 5
```

### Suppress specific rules on the next line

```gdscript
# gdstyle:ignore=naming/variable-name-snake-case
var BadName: int = 5
```

### Suppress on the same line (inline)

```gdscript
var BadName: int = 5  # gdstyle:ignore=naming/variable-name-snake-case
```

### Suppress multiple rules

```gdscript
# gdstyle:ignore=naming/variable-name-snake-case,format/max-line-length
var SomeReallyLongVariableNameThatExceedsTheMaxLineLengthAndAlsoUsesTheBadNamingConvention: int = 5
```

## CLI reference

```
gdstyle [COMMAND] [OPTIONS] [PATHS]...
```

### Subcommands

| Command | Description |
|---------|-------------|
| `check` | Lint files (default when no subcommand given) |
| `fmt` | Format files in place |
| `rules` | List all available lint rules |
| `init` | Generate a starter `gdstyle.toml` configuration file |

### `check` options

| Option | Description |
|--------|-------------|
| `--fix` | Auto-fix safe violations |
| `--unsafe-fix` | Auto-fix all violations including unsafe ones |
| `--format <FORMAT>` | Output format: `text` (default) or `json` |
| `-c, --config <PATH>` | Path to configuration file |
| `--select <RULES>` | Only check specific rules (comma-separated, supports partial matching) |
| `--ignore <RULES>` | Ignore specific rules (comma-separated) |
| `--max-line-length <N>` | Override the maximum line length |
| `--no-color` | Disable colored output |

### `fmt` options

| Option | Description |
|--------|-------------|
| `--check` | Dry-run: exit 1 if any file would change |
| `--diff` | Print a diff of what would change |
| `-c, --config <PATH>` | Path to configuration file |
| `--no-color` | Disable colored output |

### `init` options

| Option | Description |
|--------|-------------|
| `--force` | Overwrite existing config file |

### Exit codes

| Code | Meaning |
|------|---------|
| `0` | Linting/formatting completed (warnings only, or no issues) |
| `1` | Linting completed with errors, or `fmt --check` found changes |
| `2` | Configuration error |

## CI/CD integration

### GitHub Actions

```yaml
name: GDScript Style

on: [push, pull_request]

jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install gdstyle
        run: cargo install gdstyle

      - name: Check formatting
        run: gdstyle fmt --check

      - name: Lint GDScript files
        run: gdstyle check
```

### Pre-commit hook

```bash
#!/bin/bash
# .git/hooks/pre-commit

GD_FILES=$(git diff --cached --name-only --diff-filter=ACM -- '*.gd')

if [ -n "$GD_FILES" ]; then
    gdstyle check $GD_FILES
    if [ $? -ne 0 ]; then
        echo "GDScript lint failed. Fix with 'gdstyle check --fix' or suppress with '# gdstyle:ignore'."
        exit 1
    fi
fi
```

### JSON output format

When using `--format json`, gdstyle outputs a JSON array of diagnostics:

```json
[
  {
    "rule": "naming/variable-name-snake-case",
    "message": "Variable 'BadName' should use snake_case: 'bad_name'",
    "severity": "warn",
    "span": {
      "line": 5,
      "column": 1
    },
    "file": "src/player.gd"
  }
]
```

## Using as a library

You can also use gdstyle as a Rust library:

```rust
use gdstyle::config::Config;
use gdstyle::linter;
use gdstyle::formatter;

fn main() {
    let config = Config::default();

    // Lint a file
    let diagnostics = linter::lint_file(
        std::path::Path::new("player.gd"),
        &config,
    ).unwrap();

    for d in &diagnostics {
        println!("line {}: [{}] {}", d.span.line, d.rule, d.message);
    }

    // Format a source string
    let source = "var x = 'hello'\n";
    let formatted = formatter::format_source(source, &config);
    assert!(formatted.contains("\"hello\""));
}
```

## Project structure

```
gdstyle/
├── src/
│   ├── main.rs              # CLI entry point (clap subcommands)
│   ├── lib.rs               # Library root
│   ├── token.rs             # Token types (Span, TokenKind, Token)
│   ├── lexer.rs             # Tokenizer (indentation-aware, GDScript 4.x)
│   ├── ast.rs               # AST node types for linting
│   ├── parser.rs            # Lightweight parser (just enough for linting)
│   ├── diagnostic.rs        # Diagnostic, Fix, and Replacement types
│   ├── config.rs            # TOML configuration loading
│   ├── linter.rs            # Main lint pipeline (tokenize -> parse -> rules -> filter)
│   ├── reporter.rs          # Text and JSON output formatting
│   ├── fixer.rs             # Auto-fix engine (applies replacements)
│   ├── formatter.rs         # Multi-pass formatter
│   └── rules/
│       ├── mod.rs           # Rule dispatcher
│       ├── naming.rs        # 11 naming convention rules
│       ├── formatting.rs    # 18 formatting rules
│       ├── ordering.rs      # Class member ordering rule
│       └── quality.rs       # 23 code quality rules
├── gdstyle-gdext/           # GDExtension wrapper (exposes linter/formatter to Godot)
│   ├── Cargo.toml
│   └── src/lib.rs
├── godot-plugin/            # Godot 4.x editor plugin
│   └── addons/gdstyle/
│       ├── plugin.cfg
│       ├── plugin.gd
│       ├── gdstyle_panel.gd
│       └── gdstyle.gdextension
├── tests/
│   ├── integration_test.rs  # End-to-end integration tests
│   └── fixtures/            # GDScript test fixtures
├── .github/workflows/
│   └── release.yml          # CI: builds CLI + GDExtension for all platforms
├── examples/                # Example GDScript files for trying out gdstyle
├── Cargo.toml
└── gdstyle.example.toml
```

## Examples

The `examples/` directory contains sample GDScript files you can use to try out linting and formatting:

```bash
# Lint the examples. Expect warnings about naming, formatting, and quality.
gdstyle check examples/

# See what the formatter would change
gdstyle fmt --diff examples/

# Auto-fix all safe violations
gdstyle check --fix examples/

# Format everything
gdstyle fmt examples/
```

## Testing

gdstyle has 383 tests: 161 unit tests, 219 integration tests, and 3 doctests.

```bash
cargo test           # Run all tests
cargo test --lib     # Unit tests only
cargo test --test integration_test  # Integration tests only
cargo clippy         # Lint check
```

## Godot editor plugin

A Godot 4.x editor plugin lives in `godot-plugin/`. It adds a bottom panel that runs gdstyle and shows clickable diagnostics inside the editor.

The plugin supports two backends:

- **GDExtension (native).** If the GDExtension library is present, it calls into Rust directly with no process overhead. Requires Godot 4.6+.
- **CLI fallback.** Spawns the `gdstyle` binary. If it isn't on `PATH`, a **Download** button fetches the right release from GitHub.

You can switch between backends at any time from the mode dropdown in the toolbar.

### Plugin features

- **Lint Project / Lint File.** Run the linter on every script in the project, or just the one you have open.
- **Fix File.** Apply all available auto-fixes to the current script in one click.
- **Format Project / Format File.** Same split for the formatter.
- **Lint on Save.** Lint after every save (on by default).
- **Format on Save.** Format before linting on save.
- **Right-click Fix.** Right-click any diagnostic with an auto-fix to apply it in place.
- **Click to navigate.** Double-click a diagnostic to jump to the source line.
- **In-memory editing.** Lint, fix, and format work directly on the editor buffer, no disk I/O.

### Installation (pre-built plugin)

1. Download `gdstyle-godot-plugin.zip` from the [latest release](https://github.com/atelico/gdstyle/releases)
2. Extract the `addons/gdstyle/` folder into your Godot project
3. Enable the plugin in **Project > Project Settings > Plugins**

### GDExtension API

When using the GDExtension backend, you can use `GdStyle` directly in GDScript:

```gdscript
var style = GdStyle.new()

# Lint a file.
var diagnostics = style.lint_res_file("res://player.gd")
for d in diagnostics:
    print("Line %d: [%s] %s" % [d["line"], d["rule"], d["message"]])
    if d["has_fix"]:
        print("  (auto-fixable, safe=%s)" % d["is_safe_fix"])

# Format a source string.
var formatted = style.format_source(source_code)

# Auto-fix violations.
var fixed = style.fix_source(source_code, "player.gd")

# Fix a single diagnostic by line and rule.
style.fix_at_line("res://player.gd", 12, "naming/variable-name-snake-case")

# Configure.
style.set_max_line_length(120)
style.disable_rule("format/double-quotes")
style.load_config_res("res://gdstyle.toml")
```

## Contributing

1. Fork the repository
2. Create a feature branch: `git checkout -b my-feature`
3. Write tests first, then implement
4. Run the full test suite: `cargo test`
5. Run clippy: `cargo clippy`
6. Commit and push
7. Open a pull request

## License

MIT
