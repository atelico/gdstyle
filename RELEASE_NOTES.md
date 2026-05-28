## gdstyle 0.1.0

First public release. gdstyle is a fast, opinionated linter and formatter for
GDScript (Godot 4.x), written in Rust, that ships as a single static binary
plus an optional Godot editor plugin.

We've been using it inside the studio for a while and figured the wider Godot
community might get the same mileage out of it. Open-sourcing it under MIT.

### What's in the box

- **CLI:** prebuilt binaries for Linux, macOS (Intel + Apple Silicon), and
  Windows. No Python, no Rust toolchain, no Godot install required to run it.
- **Lint:** 54 rules across syntax, naming, formatting, ordering, and code
  quality. Most are safely auto-fixable.
- **Format:** in-place, idempotent, member-aware. Same input always produces
  the same output.
- **Scene-aware autofix:** `--unsafe-fix` follows renamed signals and methods
  into `.tscn` and `.tres` files, so a refactor doesn't quietly break editor
  wiring.
- **Godot editor plugin:** bottom panel with clickable diagnostics, single
  right-click fixes, Lint/Format on save. Uses the GDExtension when present,
  falls back to the CLI binary otherwise.
- **TOML config (`gdstyle.toml`):** per-rule severity overrides, limit knobs,
  exclude patterns. gdstyle searches up from the file's directory so each
  subproject or vendored addon can carry its own rules.

### What it draws on

The defaults follow the [official Godot GDScript style guide](https://docs.godotengine.org/en/stable/tutorials/scripting/gdscript/gdscript_styleguide.html)
and the conventions Nathan Lovato and [GDQuest](https://gdquest.gitbook.io/gdquests-guidelines)
have done so much to popularise. Scony's [gdtoolkit](https://github.com/Scony/godot-gdscript-toolkit)
and GDQuest's [GDScript Formatter](https://github.com/GDQuest/GDScript-formatter)
got there first and are still excellent options; gdstyle owes them a debt.

### Heads up for a first release

It's young, and almost certainly wrong about something on a codebase that
isn't ours. We validated it against 30+ open-source Godot projects before
this release, which covers a lot of ground, but the wild is much bigger than
that. If you run gdstyle on your project and it flags something it shouldn't
(or misses something it should), an issue or PR is the most useful thing you
can send our way. Rule suggestions welcome too.

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

Full documentation, rule list, configuration reference, and the GDExtension
API live in the [README](./README.md).
