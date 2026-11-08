## gdstyle 0.7.0

gdstyle 0.7.0 preserves more valid GDScript during formatting and makes
unsafe renames more conservative. The Godot editor's CLI backend now uses
the same fixer as the Rust library for individual diagnostics.

### Highlights

- **Formatting preserves valid multiline calls.** A closing delimiter in a
  comment can no longer cause the formatter to split a call and leave extra
  parentheses behind. The lexer also handles long runs of line continuations
  and bracketed blank lines without growing the call stack, recognizes UTF-8
  BOMs and combining marks, and accepts the minimum signed decimal integer.
  ([#100](https://github.com/atelico/gdstyle/pull/100),
  [#101](https://github.com/atelico/gdstyle/pull/101))
- **Unsafe renames avoid several sources of broken references.** gdstyle
  skips replacements that would introduce reserved names or duplicate
  declarations. Scene connections are rewritten only when the endpoint has
  the declaring script attached directly. Unresolved references are reported
  and make the run exit `2`. ([#100](https://github.com/atelico/gdstyle/pull/100),
  [#101](https://github.com/atelico/gdstyle/pull/101))
- **`fix-at` applies one rule's fix at a selected line.** Use
  `gdstyle fix-at player.gd --line 12 --rule format/trailing-whitespace`.
  The Godot editor's right-click action uses this command in CLI mode and
  verifies that the diagnostic belongs to the open script. CLI linting and
  formatting in the editor honor the project configuration.
  ([#101](https://github.com/atelico/gdstyle/pull/101))

### Changelog

- Added `fix-at` and the Rust library functions `fixer::apply_fix_at_line`,
  `fixer::apply_scoped_scene_renames`, and
  `collect::collect_scene_files_reporting` (#101).
- Directory walks skip symbolic links and special files, and explicit aliases
  are deduplicated. Scene discovery reports I/O failures (#101).
- Unknown output formats and unmatched rule filters exit `2`. After
  cross-file fixes, diagnostics reflect the final script contents (#101).
- Fixed formatter and lexer edge cases, including mismatched delimiters,
  Unicode identifiers, CRLF blank lines, and large layout runs. The parser
  also accepts a token slice without an explicit EOF token (#100, #101).
- Invalid fix ranges, reserved rename targets, and conflicting rename
  proposals are skipped (#100, #101).

### Upgrading

- Name a symbolic link explicitly if you want gdstyle to process it.
- Review any unresolved connections after `check --unsafe-fix`. Connections
  through inherited or instanced scenes and built-in scripts may need a
  manual update. Conflicted scenes that need a connection update are left
  unchanged.
- Upgrade the CLI binary to 0.7.0 when using right-click Fix in the Godot
  plugin's CLI mode.
