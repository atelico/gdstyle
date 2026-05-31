## gdstyle 0.1.2

Patch release focused on one user-reported formatter bug. The fix is
under the hood in the lexer, so existing CI and editor workflows pick
it up automatically — no config changes required.

### Fixed

- **Formatter no longer detaches `## doc` strings from their function**
  when the previous function's body ended with a nested indented block
  (e.g. a top-level `if` after a `match`, leaving two open indent
  levels at the body's tail). The lexer was emitting the doc comment
  token before the dedents that close the previous body, so the parser
  consumed the doc as part of the wrong function. The formatter then
  inserted its canonical between-functions blank-line gap between the
  orphan doc and the function it actually documents — visible to users
  as docstrings that the Godot editor tooltip and the `class_docs`
  export no longer recognised.

  Fixed in the lexer with read-only peek-ahead: when a comment sits at
  shallower indent than the current block, look at the next real line
  (skipping blanks and more comments). If that line is deeper than the
  comment, the comment is mid-body noise and the block continues; if
  it's at the comment's indent or lower, it's a true boundary and
  dedents fire correctly.

  The fix also tightens spacing between inner-class methods: when the
  previous formatter was confused about member boundaries it was
  inflating single blank lines to double; member-aware spacing now
  produces the canonical PEP-8 / Godot-style single blank between
  methods within a class.

### Install

CLI from crates.io:
```bash
cargo install gdstyle
```

Or grab a prebuilt binary from this release page, drop it on your
`PATH`, and run `gdstyle` in your project directory.

For the Godot editor plugin: download `gdstyle-godot-plugin.zip` from
this release, extract the `addons/gdstyle/` folder into your Godot
project, then enable the plugin in *Project > Project Settings >
Plugins*.

For the [pre-commit](https://pre-commit.com) framework, bump your
config to:
```yaml
- repo: https://github.com/atelico/gdstyle
  rev: v0.1.2
  hooks:
    - id: gdstyle
    - id: gdstyle-fmt
```
or run `pre-commit autoupdate`.

Full documentation, rule list, configuration reference, and the
GDExtension API live in the [README](./README.md).
