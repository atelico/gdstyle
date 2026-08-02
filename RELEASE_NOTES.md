## gdstyle 0.2.5

A patch release fixing two bugs found by running gdstyle across a 1100-file
Godot 4.6 project: a formatter bug that could turn valid code into a parse
error, and a false `error`-severity diagnostic that failed CI on valid
GDScript. It also narrows when `format/trailing-comma` fires, which changes
formatter output.

### Fixed

- **`format/one-statement-per-line` no longer splits a `;` inside an inline
  callable body.** The rule lifted every statement after a `;` onto its own
  line at the enclosing indent. Inside a lambda body that dropped the trailing
  statements out of the lambda, and when the lambda was a call argument it
  broke the argument list too, turning valid input into a parse error:

  ```gdscript
  _active_tween.tween_method(
      func(v: float) -> void: _calc.visual = v; queue_redraw(),
      from, to, duration
  )
  ```

  The statements after the `;` belong to the callable body, not to the
  enclosing scope, so they cannot be lifted. This is the same shape as the
  match arms the rule already skipped. Single-line named functions
  (`func _ready() -> void: setup(); start()`) were affected identically and are
  fixed too. A `;` that genuinely separates peers still splits, including
  `add(func(): pass); other()`, where the body closes before the semicolon.

- **`syntax/lex-error` no longer fires on a line continuation aligned with
  tabs and spaces.** The lexer consumed mid-line whitespace in two sequential
  passes, all spaces and then all tabs, which cannot handle a tab followed by
  spaces. On a `\` continuation line indented with a tab for block depth plus
  spaces for alignment, it skipped the tab, stopped on the first space, and
  reported `unexpected character: ' '`:

  ```gdscript
  if a() and \
     b():
      return true
  ```

  Because the severity was `error`, this failed CI for anyone running
  `gdstyle check` on perfectly valid code. Whitespace is now consumed in one
  interleaved pass. Tabs at line start are still indentation and are still
  handled by the indentation tracker.

### Changed

- **`format/trailing-comma` now only fires when the closing bracket starts its
  own line.** Previously any collection spanning more than one line qualified,
  so a closer sharing the last element's line still got a comma:

  ```gdscript
  # 0.2.4 rewrote this ...
  print("%s %s" % [
      alpha, beta])

  # ... into this. 0.2.5 leaves it alone.
  print("%s %s" % [
      alpha, beta,])
  ```

  A trailing comma earns its place by keeping diffs clean when each element
  owns a line and the closer owns the last one. When the closer trails the
  final element it adds churn and nothing else, and black, rustfmt and prettier
  all require the closer on its own line before adding one. Collections written
  the conventional way are unaffected:

  ```gdscript
  var xs := [
      alpha,
      beta,   # still added, the closer owns its line
  ]
  ```

  A trailing comment on the last element still counts as the closer owning its
  line, so those keep the comma too.

  This changes formatter output. The first `gdstyle fmt` after upgrading will
  produce a diff on code that adopted the old shape. It is a net reduction: on
  the 1100-file project used for testing, `format/trailing-comma` warnings drop
  from 992 to 314.

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
  rev: v0.2.5
  hooks:
    - id: gdstyle
    - id: gdstyle-fmt
```
or run `pre-commit autoupdate`.

Full documentation, rule list, configuration reference, and the GDExtension API
live in the [README](./README.md).
