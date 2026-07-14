## gdstyle 0.2.4

A patch release fixing a false negative in `quality/duplicate-dict-key`: a
comment inside a dictionary literal caused the rule to stop tracking keys for
the rest of that literal.

### Fixed

- **`quality/duplicate-dict-key` no longer misses duplicates after a comment
  in the dict literal.** A `#` comment sitting in key position (on its own
  line, or trailing an entry's comma) was appended to the key currently being
  accumulated, corrupting its text. The corrupted key no longer matched an
  identical key elsewhere in the literal, so a genuine duplicate went
  unreported:

  ```gdscript
  var with_comment: Dictionary = {
      # a comment inside the dict literal
      Vector3i(0, 0, 0): "a",
      Vector3i(0, 0, 0): "b",  # gdstyle 0.2.3 missed this duplicate
  }
  ```

  Comment tokens are now skipped while scanning a dictionary literal's keys,
  the same way blank lines already are.
  Reported in [#27](https://github.com/atelico/gdstyle/issues/27).

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
  rev: v0.2.4
  hooks:
    - id: gdstyle
    - id: gdstyle-fmt
```
or run `pre-commit autoupdate`.

Full documentation, rule list, configuration reference, and the GDExtension API
live in the [README](./README.md).
