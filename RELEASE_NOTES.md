## gdstyle 0.3.0

Per-rule `"error"` severity never actually worked. It parsed, and was then
thrown away, so every diagnostic printed as a warning and `gdstyle check`
always exited `0`. If you set rules to `"error"` expecting CI to fail on
them, it never did. This release fixes that and closes the two gaps that
made it hard to notice, plus adds a way to fail a build on warnings without
escalating rules one at a time.

### Fixed

- **`"error"` severity in `[rules]` is now applied to diagnostics.**
  `Config::rules` is three-valued (`"off"`, `"warn"`, `"error"`), but the
  only code reading it collapsed that to a boolean, "is this rule on?". The
  `"warn"` versus `"error"` distinction was parsed and discarded, so every
  rule emitted a warning. Because `check` exits `1` only when it sees an
  error-severity diagnostic, no configuration could fail a build:

  ```toml
  [rules]
  "naming/variable-name-snake-case" = "error"
  "naming/function-name-snake-case" = "error"
  ```

  ```
  # 0.2.5
  3:1 warning variable name 'BadName' should use snake_case ...
  5:1 warning function name 'DoThing' should use snake_case ...
  1 file checked, 2 warnings found.        $? = 0

  # 0.3.0
  3:1 error variable name 'BadName' should use snake_case ...
  5:1 error function name 'DoThing' should use snake_case ...
  1 file checked, 2 errors found.          $? = 1
  ```

  Severity is now applied in `lint_source`, the single entry point the CLI,
  the GDExtension and the formatter all share, so the CLI, `--format json`
  and the Godot editor panel agree. A rule you say nothing about keeps its
  own default, which leaves `syntax/lex-error` an error while still letting
  an explicit `"warn"` downgrade it.

  Reported in [#29](https://github.com/atelico/gdstyle/issues/29), with a
  complete reproduction that is now pinned as a regression fixture.

### Added

- **Unknown rule names in `[rules]` are reported instead of ignored.** A
  misspelled key is valid TOML and used to be dropped on the floor, so a
  config looked like it had taken effect when it silently did nothing. This
  is the failure mode that made the severity bug above hard to tell apart
  from a typo:

  ```
  $ gdstyle check
  warning: unknown rule name in [rules]: naming/variable-snake-case
           run `gdstyle rules` to see the available rules
  ```

  Both `check` and `fmt` report them (`fmt` applies safe lint fixes, so a
  typo changes what gets formatted too), and the editor plugin pushes a
  warning once per config file. It is a warning rather than a hard error:
  an unknown key is harmless, and failing outright would break configs
  written against a different version.

- **`gdstyle check --max-warnings <N>`** exits `1` when more than `N`
  warnings are found. Previously, a default config had no way to fail CI on
  warnings at all, short of escalating rules to `"error"` one by one:

  ```bash
  gdstyle check --max-warnings 0   # no warnings tolerated
  gdstyle check --max-warnings 20  # fails at 21
  ```

  `N` is a maximum, so exactly `N` warnings still passes. The explanation
  goes to stderr, which keeps `--format json` stdout machine-readable.

- **`GdStyle.set_rule_severity(rule, severity)` in GDScript**, taking the
  same `"off"` / `"warn"` / `"error"` vocabulary as `gdstyle.toml`. It sits
  alongside the existing `disable_rule` and `enable_rule`; until now,
  severity could only be set by loading a config file. `GdStyle.unknown_rule_names()`
  exposes the validation above to plugin code:

  ```gdscript
  var style = GdStyle.new()
  style.set_rule_severity("naming/variable-name-snake-case", "error")

  for name in style.unknown_rule_names():
      push_warning("gdstyle: unknown rule name %s" % name)
  ```

### Internal

- The release workflow validates `CARGO_REGISTRY_TOKEN` before anything is
  published. `publish-crates` runs after the GitHub release, so an expired
  token used to surface only once the release and all 9 assets were already
  out, leaving a tag that never reached crates.io.
- The JSON output example in the README said `"severity": "warn"`; the
  serialized value is `"warning"`.

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
  rev: v0.3.0
  hooks:
    - id: gdstyle
    - id: gdstyle-fmt
```
or run `pre-commit autoupdate`.

### Upgrading

If your `gdstyle.toml` already sets rules to `"error"`, this release starts
enforcing them and `gdstyle check` may begin failing where it previously
passed. That is the fix working, but it can arrive as a surprise in CI. To
see what will change before you upgrade the pipeline, run `gdstyle check`
locally and look at the error count, or set the rules back to `"warn"` and
adopt `--max-warnings` instead.

Full documentation, rule list, configuration reference, and the GDExtension API
live in the [README](./README.md).
