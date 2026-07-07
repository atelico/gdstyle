## gdstyle 0.2.0

A code-quality release: two user-reported false positives fixed, duplicate
dictionary-key detection extended to nested dictionaries, theme-aware colors
in the Godot editor plugin, and a new continuous-integration pipeline that
runs rustfmt, clippy, and the full test suite on Linux, macOS, and Windows for
every pull request.

### Fixed

- **`quality/self-comparison` no longer fires on different dotted paths.**
  The rule compared only the last identifier on each side of the operator, so
  `stored_goods.label == label` was read as `label == label` and flagged. It
  now compares the full dotted chain on both sides and requires each side to be
  a standalone operand, so partial matches inside arithmetic (`foo + x == x`,
  `x == x + 1`) no longer trigger. Genuine dotted self-comparisons like
  `obj.foo == obj.foo` are now caught, which the old single-token check missed.
  Reported in [#7](https://github.com/atelico/gdstyle/issues/7).

- **`quality/duplicate-dict-key` no longer flags enum keys from another class.**
  The scanner walked dotted keys token by token, so `Enums.E.KEY_A` was treated
  as five separate keys and the two `.` tokens collided into a bogus
  `duplicate dictionary key '.'`. Anyone keying a dictionary by an enum from a
  shared class got flooded with these. Keys are now compared as whole
  expressions, so distinct entries (`Enums.E.KEY_A` vs `Enums.E.KEY_B`) stay
  distinct, while genuine duplicates (including a repeated dotted key) are still
  reported. Quote-style normalization is preserved, so `{"foo": 1, 'foo': 2}`
  is still caught.
  Reported in [#8](https://github.com/atelico/gdstyle/issues/8).

- **Godot plugin diagnostic colors are now theme-aware.** The status label and
  the error/warning colors in the bottom panel were hard-coded, and the warning
  yellow was hard to read on light editor themes. They now follow the current
  editor theme, with a `has_color` fallback to the previous colors so older
  Godot 4.x keeps working on the CLI-backed path (`get_editor_theme()` exists
  from 4.2, and `font_placeholder_color` on the `Editor` theme type only from
  4.3). Thanks to [@trianglebreaker](https://github.com/trianglebreaker) for the
  original fix in [#9](https://github.com/atelico/gdstyle/pull/9).

### Improved

- **`quality/duplicate-dict-key` now detects duplicates inside nested
  dictionaries.** Previously only the top level of each literal was checked, so
  a real duplicate inside a value dictionary
  (`{"outer": {"a": 1, "a": 2}}`) went unreported. The scanner now tracks each
  open dictionary independently, so every nesting level is checked and a key
  reused at a different level stays distinct.

### Internals

- **New CI workflow** (`.github/workflows/ci.yml`): rustfmt, clippy
  (`-D warnings`), and `cargo test` across Linux, macOS, and Windows on every
  pull request and push to `main`.
- The Rust codebase was formatted with `cargo fmt` so the new rustfmt gate is
  green.

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
  rev: v0.2.0
  hooks:
    - id: gdstyle
    - id: gdstyle-fmt
```
or run `pre-commit autoupdate`.

Full documentation, rule list, configuration reference, and the GDExtension API
live in the [README](./README.md).
