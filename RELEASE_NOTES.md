## gdstyle 0.2.1

A formatter release: two user-reported issues around comments and member
ordering. Leading comments no longer drift away from the declaration they
describe, and `order/class-member-order` can now be disabled per member so a
lookup table can stay next to the enum it mirrors.

### Fixed

- **Leading comments stay attached to their declaration.** A plain `#` comment
  written directly above a declaration is a comment *for* that declaration, but
  the member-spacing pass treated it as trailing content of the previous member
  and then inserted the canonical blank line between the comment and the
  declaration it describes. So

  ```gdscript
  # File
  const SETTINGS_FILE: String = "user://mods.cfg"
  ```

  was reformatted with a blank line wedged between `# File` and the `const`.
  A comment block sitting tight against the following declaration (no blank
  line between) is now kept with it, and the member gap is placed above the
  comment instead. A blank line between the comment and the declaration still
  marks it as a standalone section comment, so that case is unchanged.
  Reported in [#15](https://github.com/atelico/gdstyle/issues/15).

### Improved

- **`order/class-member-order` can be disabled per member.**
  `order/class-member-order` is enforced by the formatter's reorder pass, so a
  `# gdstyle:ignore=order/class-member-order` comment was treated as a plain
  comment and the member got reordered anyway. The directive now *pins* the
  member: `gdstyle fmt` keeps it where you wrote it while everything else still
  reorders around it, so a `const` can be kept next to the `enum` it mirrors.

  ```gdscript
  enum SaveGameFormat {
  	BINARY,
  	TEXT,
  }
  # gdstyle:ignore=order/class-member-order
  const SAVE_GAME_FORMAT_DISPLAY_NAMES: PackedStringArray = [
  	"OPTIONS_GENERAL_BINARY",
  	"OPTIONS_GENERAL_TEXT",
  ]
  ```

  `gdstyle check` skips a pinned member too (it is neither flagged nor counted
  toward the running category, so its neighbours are not flagged either), so
  `check` and `fmt` agree on which members opt out. All other rules still apply
  to the member. A file-level `# gdstyle:ignore-file=order/class-member-order`
  pins every member, disabling reordering for the whole file. See "Pinning a
  member against reordering" in the README.
  Requested in [#16](https://github.com/atelico/gdstyle/issues/16).

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
  rev: v0.2.1
  hooks:
    - id: gdstyle
    - id: gdstyle-fmt
```
or run `pre-commit autoupdate`.

Full documentation, rule list, configuration reference, and the GDExtension API
live in the [README](./README.md).
