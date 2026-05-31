## gdstyle 0.1.1

Small follow-up to the initial release: one false-positive fix and a
pre-commit integration that came out of community feedback.

### Fixes

- **`quality/no-self-assign`** no longer flags `obj.field = field` style
  assignments. The previous implementation compared one identifier on
  each side of `=`, so `moon.size = size * 0.5` matched `size == size`
  even though the LHS is the property `moon.size` and the RHS is the
  local `size`. Same bug hit `self.position = position` and
  `x = x + 1`. The rule now compares full dotted paths (including
  `self.` / `super.` heads) and only flags when the assignment ends
  at a statement terminator. True positives like
  `obj.foo = obj.foo` and `self.player.health = self.player.health`
  still trigger.

### Added

- **Pre-commit framework integration.** gdstyle now ships a
  `.pre-commit-hooks.yaml` at the repo root, exposing two hooks for
  [pre-commit](https://pre-commit.com): `gdstyle` (lint) and
  `gdstyle-fmt` (format in place). Drop this into your project's
  `.pre-commit-config.yaml`:

  ```yaml
  repos:
    - repo: https://github.com/atelico/gdstyle
      rev: v0.1.1
      hooks:
        - id: gdstyle
        - id: gdstyle-fmt
  ```

  Then `pre-commit install`. First run builds gdstyle from source via
  cargo (Rust toolchain required); subsequent runs are cached.

### Install

CLI from crates.io:
```bash
cargo install gdstyle
```

Or grab a prebuilt binary from this release page, drop it on your `PATH`,
and run `gdstyle` in your project directory.

For the Godot editor plugin: download `gdstyle-godot-plugin.zip` from
this release, extract the `addons/gdstyle/` folder into your Godot
project, then enable the plugin in *Project > Project Settings > Plugins*.

Full documentation, rule list, configuration reference, and the
GDExtension API live in the [README](./README.md).
