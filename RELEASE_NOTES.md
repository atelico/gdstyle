## gdstyle 0.4.0

A correctness release for the formatter and the parser. `fmt` and
`check --fix` no longer change the contents of multi-line strings and keep a
file's line endings. Single-line enums expand without losing code or
comments, and nested inner classes parse properly. The version is 0.4.0
rather than 0.3.4 because a few functions in the Rust library changed
signature (see Upgrading). The CLI, the config file and the Godot plugin
work as before.

Thanks to [@calpeyser](https://github.com/calpeyser) for #42, which started
this release.

### Fixed

- **`fmt` and `check --fix` leave the contents of multi-line strings alone.**
  The text inside a `"""..."""` literal is data, but the formatter treated it
  as code. It converted leading spaces to tabs, stripped trailing whitespace,
  collapsed blank lines and wrapped long lines at commas:

  ```gdscript
  const HELP = """
  Usage:
      run [options]   
  """
  ```

  0.3.3 turned the third line into a tab-indented `run [options]` with the
  trailing spaces removed, so the string's value changed and Godot gave no
  sign of it. The file now stays as written. The layout rules
  (`format/trailing-whitespace`, `format/blank-lines`,
  `format/no-tabs-as-spaces`, `format/max-line-length`) don't report lines
  inside such a string either. `format/max-line-length` still reports a
  string's closing line when the code after the closing quote is what makes
  it too long. This also covers quoted strings with a raw newline or a
  backslash continuation, and a string line starting with `##`, `#` or
  `extends`, which other passes used to mistake for code.
  ([#42](https://github.com/atelico/gdstyle/pull/42),
  [#43](https://github.com/atelico/gdstyle/pull/43))

- **Line endings are kept.** `fmt` converted CRLF files to LF. Since Godot
  keeps a `\r` inside a string as part of its value, that also changed every
  multi-line string in the file. `fmt` and `check --fix` now keep the style
  that most of the file's line breaks use, and a string keeps its own line
  endings even when they differ from the rest of the file. A lone `\r` is no
  longer treated as a line break, which used to split comments into code.
  **CRLF files that were already formatted are no longer reported by
  `fmt --check`.** ([#43](https://github.com/atelico/gdstyle/pull/43))

- **Single-line enums expand safely.** `format/enum-one-per-line` worked on
  raw text, so a brace in a trailing comment broke the code:

  ```gdscript
  enum State {IDLE, RUN}  # see {docs}
  ```

  0.3.3 produced:

  ```gdscript
  enum State {
  	IDLE,
  	RUN}  # see {docs,
  }
  ```

  0.4.0 produces:

  ```gdscript
  enum State {
  	IDLE,
  	RUN,
  }  # see {docs}
  ```

  The fix now splits only at top-level commas, so a comma inside a string or
  a call (`S = "x, y"`, `T = max(1, 2)`) no longer splits a member in two. It
  also keeps a leading annotation and a trailing comment, and indents the
  members in the project's indentation style. It skips an enum with an empty
  member (`{A,, B}`) or a `# gdstyle:ignore` directive on its line. The rule
  now covers enums in inner classes too.
  ([#44](https://github.com/atelico/gdstyle/pull/44))

- **`fmt` settles on consecutive commas.** On invalid input such as
  `["arrays",,,,]`, every `fmt` run changed the file again, because two
  spacing rules disagreed about the space before the second comma.
  ([#44](https://github.com/atelico/gdstyle/pull/44))

- **Nested inner classes parse correctly.** A class inside an inner class was
  misread: its members counted as the parent's, and the parent's remaining
  members moved to the script level. That caused false
  `order/class-member-order` warnings:

  ```gdscript
  class Outer:
  	var speed = 1

  	class Inner:
  		const LIMIT = 3
  ```

  0.3.3 warned that `LIMIT` should come before `speed`, although they belong
  to different classes. ([#45](https://github.com/atelico/gdstyle/pull/45))

- **Inner classes with a dotted or path base parse correctly.** In
  `class Stub extends "res://base.gd":` or `class A extends Node2D.Foo:` the
  whole body was read as script-level code. `check` reported false ordering
  warnings, and `fmt` added two blank lines under the class header.
  ([#45](https://github.com/atelico/gdstyle/pull/45))

- **Smaller fixes.** `fmt --diff` now shows line-ending changes (as `^M`)
  instead of printing them as unchanged lines. `fmt --check` reported the
  number of files checked as the number to reformat. `format/trailing-comma`
  added a comma after a closing `"""` followed by `}` or `]`.
  ([#43](https://github.com/atelico/gdstyle/pull/43))

### Changed

- **`quality/max-inner-classes` counts nested classes.** The limit is per
  file, as the README says, but the count used to depend on which nested
  classes the parser pushed to the top level. Files with nested classes may
  report a higher count, or a new warning.
  ([#45](https://github.com/atelico/gdstyle/pull/45))

### Changelog

- Fixed: `fmt` and `check --fix` no longer change the contents of multi-line
  strings, and layout rules skip them (#42, #43).
- Fixed: `fmt` and `check --fix` keep a file's line endings, and strings keep
  their own (#43).
- Fixed: `format/enum-one-per-line` keeps comments, annotations and commas
  inside members, and covers inner classes (#44).
- Fixed: `fmt` settles on consecutive commas (#44).
- Fixed: nested inner classes and dotted or path `extends` parse correctly
  (#45).
- Changed: `quality/max-inner-classes` counts nested classes (#45).
- Library: rule functions in `rules::formatting` take the token list (#42,
  #43, #44).

### Upgrading

No configuration changes are required. Some files will be reformatted once:
enums that now expand inside inner classes, and inner classes that were
misparsed. Files with multi-line strings that an earlier `fmt` already
changed are not restored, so check those strings if they matter.

If you use gdstyle as a Rust library, these functions in
`gdstyle::rules::formatting` now take `tokens: &[Token]` as their first
argument: `check_trailing_whitespace`, `check_blank_lines`,
`check_max_line_length` and `check_indentation_style`.
`check_enum_one_per_line` also takes `tokens` first, plus `config: &Config`
after `file`. The new `token::StringLines` marks lines inside multi-line
strings, and the new `linter::restore_line_endings` reapplies a file's line
endings after formatting.
