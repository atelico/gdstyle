## gdstyle 0.1.6

Patch release fixing a parser bug that caused `order/class-member-order`
false positives on inner classes whose first body line was a comment.

### Fixed

- **`order/class-member-order` no longer fires on every inner-class
  member** when the inner class body starts with a comment-only line
  (`#` or `##` on its own line, e.g. a leading doc comment). The lexer
  was preserving the indent stack on any comment-only line, so the
  parser saw no `Indent` at the start of the body, returned an empty
  inner class, and the following `var`/`func` tokens fell back to the
  outer parse loop. Every inner-class member then looked like a
  sibling of the inner class and tripped the ordering rule.

  The lexer now splits comment-only line handling into three cases
  by relative indent: same indent preserves the stack (unchanged),
  shallower indent uses the existing peek-ahead to disambiguate
  boundary vs mid-body noise (unchanged), and deeper indent falls
  through to `Indent` emission so the leading comment opens the new
  block.

  Reported in [#5](https://github.com/atelico/gdstyle/issues/5) with
  a full root-cause trace from the reporter.

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
  rev: v0.1.6
  hooks:
    - id: gdstyle
    - id: gdstyle-fmt
```
or run `pre-commit autoupdate`.

Full documentation, rule list, configuration reference, and the
GDExtension API live in the [README](./README.md).
