## gdstyle 0.2.3

A patch release fixing two user-reported bugs: `--fix` corrupting
CRLF-terminated files, and a false positive in `quality/duplicate-dict-key`
for dictionary keys built from multi-argument constructor calls.

### Fixed

- **`--fix` no longer corrupts CRLF-terminated (Windows line ending) files.**
  Diagnostic byte offsets are computed against an internally LF-normalized
  copy of the source, but `--fix` and `--unsafe-fix` were applying those
  offsets straight to the original CRLF source. Every `\r` before the edit
  point shifted the insertion point left by one byte, so fixes landed
  mid-identifier:

  ```gdscript
  # before --fix (CRLF file)
  var weapon := _make({"hold_type": ..., "mp": 40})

  # gdstyle 0.2.2 --fix produced (corrupted)
  var weapon := _ma ke({"hold_type": ..., "mp" : 40})

  # gdstyle 0.2.3 --fix produces
  var weapon := _make({ "hold_type": ..., "mp": 40 })
  ```

  Fixes now normalize internally before applying replacements and restore
  the file's original line endings afterward, so CRLF files stay CRLF. This
  covers the CLI (`check --fix`, `check --unsafe-fix`) and the GDExtension
  fix bindings used by the Godot editor plugin.
  Reported in [#24](https://github.com/atelico/gdstyle/issues/24).

- **`quality/duplicate-dict-key` no longer flags multi-argument constructor
  keys as duplicates.** A comma inside a key like `Vector2i(1, 0)` was
  mistaken for the dictionary's entry separator, so the tracked key
  collapsed to a trailing fragment (`0)`) and unrelated keys sharing that
  fragment were flagged:

  ```gdscript
  # gdstyle 0.2.2 incorrectly flagged these as duplicates
  var directions := {
      Vector2i(1, 0): "east",
      Vector2i(2, 0): "far_east",
      Vector2i(3, 0): "farther_east",
  }
  ```

  Each dictionary literal now tracks its own paren/bracket depth, so a
  comma or colon nested inside a call or subscript is treated as part of
  that key expression instead of the entry separator. Genuine duplicate
  constructor-call keys are still caught.
  Reported in [#23](https://github.com/atelico/gdstyle/issues/23).

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
  rev: v0.2.3
  hooks:
    - id: gdstyle
    - id: gdstyle-fmt
```
or run `pre-commit autoupdate`.

Full documentation, rule list, configuration reference, and the GDExtension API
live in the [README](./README.md).
