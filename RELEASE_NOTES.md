## gdstyle 0.2.2

A release driven by two user-reported issues: dictionary and call spacing the
formatter used to leave untouched, and a way to lint a first-party addon that
lives inside an otherwise-excluded directory. The Godot editor plugin now
selects the same files as the CLI.

### Fixed

- **Single-line dictionaries and call parentheses are now formatted.**
  `gdstyle fmt` reported "already formatted" while leaving both of these,
  which the [official Godot style guide](https://docs.godotengine.org/en/stable/tutorials/scripting/gdscript/gdscript_styleguide.html#whitespace)
  calls out, untouched:

  ```gdscript
  # before
  my_dictionary = {key = "value"}
  print ("foo")

  # gdstyle fmt now produces
  my_dictionary = { key = "value" }
  print("foo")
  ```

  Two safe-fixable rules, on by default (56 rules total now, 20 of them
  formatting):

  - `format/brace-spacing` pads single-line dictionary literals, so
    `{key = "value"}` becomes `{ key = "value" }`. Enum bodies, empty dicts,
    and multi-line dictionaries are left alone; nested dictionaries are each
    padded.
  - `format/call-paren-spacing` removes the space before a call's `(`, so
    `print ("foo")` becomes `print("foo")`. It covers method and chained
    calls plus `preload`/`assert`/`super`; control-flow keyword parentheses
    (`if (x):`) and lambda `func (...)` are untouched.

  Reported in [#20](https://github.com/atelico/gdstyle/issues/20).

### Added

- **`include` config to force-lint paths inside an excluded directory.**
  Exclude `addons` wholesale yet still lint your own plugin:

  ```toml
  exclude = [".godot", "addons"]
  include = ["addons/my_plugin"]
  ```

  An `include` always wins over an `exclude`, regardless of order. The whole
  included subtree is linted (nested files included), and its excluded siblings
  stay excluded. `check`, `fmt`, and the `--unsafe-fix` scene pass all honor it.
  Requested in [#19](https://github.com/atelico/gdstyle/issues/19).

### Improved

- **The Godot editor plugin honors `exclude`/`include`.** Project-wide Lint and
  Format in the plugin walked the project with a hardcoded `addons` skip, so the
  editor disagreed with the CLI and could not be told to lint a first-party
  addon. It now applies the same file selection as the CLI, including the new
  `include` carve-outs. A new GDExtension method, `collect_project_gd_files()`,
  exposes the config-aware walk to GDScript.

### Install

CLI from crates.io:
```bash
cargo install gdstyle
```

Or grab a prebuilt binary from this release page, drop it on your `PATH`, and
run `gdstyle` in your project directory.

For the Godot editor plugin: download `gdstyle-godot-plugin.zip` from this
release, extract the `addons/gdstyle/` folder into your Godot project, then
enable the plugin in *Project > Project Settings > Plugins*.

For the [pre-commit](https://pre-commit.com) framework, bump your config to:
```yaml
- repo: https://github.com/atelico/gdstyle
  rev: v0.2.2
  hooks:
    - id: gdstyle
    - id: gdstyle-fmt
```
or run `pre-commit autoupdate`.

Full documentation, rule list, configuration reference, and the GDExtension API
live in the [README](./README.md).
