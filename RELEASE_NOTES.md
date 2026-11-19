## gdstyle 0.8.0

gdstyle 0.8.0 adds `format/extra-spacing`, which removes extra spaces
between tokens, and makes every fix run settle in one go. Projects that
align code with spaces will see new warnings and `fmt` changes, see
[Upgrading](#upgrading).

### Highlights

- **New rule: `format/extra-spacing`** (safe, on by default). A run of
  spaces or tabs between two tokens becomes one space, or none where the
  style guide wants none: inside `()` and `[]`, around `.`, after a prefix
  `-`/`+`/`~`, before a subscript. So
  `func     _ready()       ->       void:` becomes `func _ready() -> void:`
  and `print( a )` becomes `print(a)`. Indentation, strings, the padding of
  single-line dictionaries (`{ "k": 1 }`) and the gap before a comment are
  left alone, so aligned trailing comments keep their column.
  ([#103](https://github.com/atelico/gdstyle/issues/103),
  [#105](https://github.com/atelico/gdstyle/pull/105))
- **One fix run is enough.** `check --fix`, `check --unsafe-fix`, the
  editor's Fix File, the GDExtension's `fix_source` and the formatter now
  repeat safe fixes until nothing changes. Before, overlapping fixes such
  as `enum K {A,B}` needed a second run. Renames still happen once, and
  `fix-at` still applies only the fix it is asked for.
  ([#105](https://github.com/atelico/gdstyle/pull/105))
- **`fmt` always settles.** On some invalid lines (`var x = , 1`,
  `a * : b`), two spacing rules undid each other, so `fmt` changed the file
  on every run and `fmt --check` never passed. `,` and `:` now attach to
  the token on their left while `format/comma-spacing` and
  `format/colon-spacing` are on. Valid code formats as before. A seeded
  randomized test now checks this on thousands of lines, also with each
  spacing rule and each pair of them turned off.
  ([#106](https://github.com/atelico/gdstyle/pull/106))

### Performance

`check` and `fmt` run about 8% fewer instructions than 0.7.0 on the Godot
demo projects and on large generated files, measured single-threaded. Most
of it comes from `format/operator-spacing`, which now matches operator
kinds directly instead of searching a list for every token.

### Library

- `fixer::apply_fixes_until_settled` applies fixes and repeats the safe ones
  until the source settles, returning the fixed text and the diagnostics
  left.
- `check_operator_spacing_with_config`, `check_colon_spacing_with_config`,
  `check_comma_spacing_with_config` and `check_brace_spacing_with_config` in
  `rules::formatting` take a `Config`, so a caller with comma- or
  colon-spacing off gets the same results as the linter. The versions
  without `_with_config` keep their 0.7 signatures and use the default
  config.

### Changelog

- Added: `format/extra-spacing` removes extra spaces and tabs between tokens,
  including alignment and padding inside `()` and `[]` (#103, #105).
- Changed: `check --fix`, `--unsafe-fix`, `fix_source` and `fmt` repeat safe
  fixes until the source settles, so one run is enough (#105).
- Fixed: `fmt` no longer flips between two layouts on invalid lines where a
  space before `,` or `:` was both added and removed (#106).
- Performance: `check` and `fmt` run about 8% fewer instructions (#105).
- Library: added `fixer::apply_fixes_until_settled` and the
  `*_with_config` spacing checks (#105, #106).

### Upgrading

- Expect new `format/extra-spacing` warnings, and `fmt` changes, wherever
  code is aligned with spaces or brackets are padded (`f( a )`). Run
  `gdstyle fmt` once and commit the result before turning on
  `fmt --check` in CI.
- To keep alignment, set `"format/extra-spacing" = "off"` under `[rules]`
  in `gdstyle.toml`, or end each aligned line with
  `# gdstyle:ignore=format/extra-spacing`.
- Update the pre-commit `rev` to `v0.8.0`.
