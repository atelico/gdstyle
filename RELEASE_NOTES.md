## gdstyle 0.5.0

A release about trust in `fmt`: it now does what the config and the
`# gdstyle:ignore` comments say, and it fixes every formatter problem reported
since 0.4.0. Turning a rule off, or suppressing it on a line or a file, now
stops `fmt` as well as `check`. Member reordering keeps `#region` folds intact,
`fmt --diff` prints a real unified diff, and a long list of smaller fixes
covers banners, doc comments, properties, semicolons, number literals and
non-ASCII text.

The version is 0.5.0 rather than 0.4.1 because a few types and functions in
the Rust library changed (see Upgrading). The CLI, the config file and the
Godot plugin work as before.

Thanks to [@MikeSchulze](https://github.com/MikeSchulze) for the detailed
reports behind most of this release, to
[@calpeyser](https://github.com/calpeyser) for #57 and to
[@niangaoxiao](https://github.com/niangaoxiao) for #58.

### Highlights

- **`fmt` respects the config.** A rule set to `"off"` used to silence
  `check` while `fmt` kept applying it: `"format/double-quotes" = "off"`
  still turned `'x'` into `"x"`, and `"order/class-member-order" = "off"`
  still reordered members. Every rule `fmt` applies now stops when it is off.
  ([#53](https://github.com/atelico/gdstyle/issues/53),
  [#59](https://github.com/atelico/gdstyle/pull/59),
  [#65](https://github.com/atelico/gdstyle/pull/65))

- **`fmt` honors `# gdstyle:ignore` comments.** A suppressed line or file is
  no longer rewritten by `fmt`, the same as with `check --fix`:

  ```gdscript
  var s = 'x'  # gdstyle:ignore=format/double-quotes
  ```

  0.4.0 turned this into `"x"`. This covers every rule, including the layout
  rules (indentation, trailing whitespace, blank lines, the final newline and
  line wrapping). For `format/no-tabs-as-spaces`, a directive that keeps a
  line's indentation keeps the whole file's: converting only the other lines
  would mix tabs and spaces, which Godot rejects. Directives are only read
  from comments now, so the same text inside a string no longer silences
  rules.
  ([#68](https://github.com/atelico/gdstyle/pull/68),
  [#72](https://github.com/atelico/gdstyle/pull/72),
  [#78](https://github.com/atelico/gdstyle/pull/78))

- **`fmt --diff` is a unified diff.** It compared the files line by line, so
  one moved member turned the rest of the file into unrelated `-`/`+` pairs,
  with no `@@` headers. It now prints hunks with three lines of context, and
  `git apply` and `patch -p1` accept the output.
  ([#54](https://github.com/atelico/gdstyle/issues/54),
  [#63](https://github.com/atelico/gdstyle/pull/63))

### Fixed

- **Reordering keeps `#region` folds intact.** Moving an inner class below a
  function carried the following `#endregion` with it, so members ended up in
  the wrong region. A reorder that would change which code a `#region`
  encloses is now skipped, as it already was for `@warning_ignore_start`
  regions. ([#52](https://github.com/atelico/gdstyle/issues/52),
  [#62](https://github.com/atelico/gdstyle/pull/62))

- **`####` banners stay in one piece.** A banner line starting with `##` was
  read as a class doc comment, moved up against `extends` and split from the
  rest of the banner. Lines made only of `#` are now decorative, and a banner
  next to a `## Title` stays with it.
  ([#50](https://github.com/atelico/gdstyle/issues/50),
  [#58](https://github.com/atelico/gdstyle/pull/58))

- **Doc comments on bodyless `@abstract func` stay attached.** The second of
  two consecutive bodyless functions lost its doc comment, which `fmt` then
  separated from it with two blank lines.
  ([#47](https://github.com/atelico/gdstyle/issues/47),
  [#60](https://github.com/atelico/gdstyle/pull/60))

- **The class doc moves below annotated headers.** With `@abstract extends
  RefCounted`, `@tool class_name Foo` or a standalone `@tool` line, the class
  doc comment stayed above the header. The header is now read from the
  parser, so annotations of any form, including `@icon("res://a(1).svg")`,
  are handled. ([#48](https://github.com/atelico/gdstyle/issues/48),
  [#61](https://github.com/atelico/gdstyle/pull/61))

- **`&&`, `||` and `!` no longer join neighbouring names.** `a&&b||!c` became
  `aandbornot c`, which Godot rejects. It now becomes `a and b or not c`.
  ([#57](https://github.com/atelico/gdstyle/pull/57))

- **Property accessors are parsed correctly.** The `set` / `get` block of a
  `static var`, or of a `var` with a comment after its `:`, was read as
  script-level code, so `fmt` inserted a blank line inside the getter.
  ([#66](https://github.com/atelico/gdstyle/pull/66),
  [#67](https://github.com/atelico/gdstyle/pull/67))

- **No crash on non-ASCII text in long conditions.** Wrapping a long
  `if` / `elif` / `while` condition containing text such as `"é"` panicked or
  dropped characters. ([#70](https://github.com/atelico/gdstyle/pull/70))

- **Accented text is measured in characters.** Long comments with accented
  words wrapped well before the line limit, and the columns reported by
  `format/trailing-whitespace` and `format/trailing-newline` pointed past the
  spot. ([#73](https://github.com/atelico/gdstyle/pull/73))

- **Blank lines follow their own rules.** Applying any fix collapsed every run
  of blank lines, even with `format/blank-lines` off or suppressed, and the
  formatter trimmed blank lines at the end of a file even with
  `format/trailing-newline` off. Both now leave them to their rule.
  ([#71](https://github.com/atelico/gdstyle/pull/71),
  [#78](https://github.com/atelico/gdstyle/pull/78))

- **Trailing semicolons are removed cleanly.** `x = 1;` left an empty,
  indented line behind, and `x = 1; # note` moved the comment onto a line of
  its own. Now they become `x = 1` and `x = 1 # note`.
  ([#71](https://github.com/atelico/gdstyle/pull/71),
  [#77](https://github.com/atelico/gdstyle/pull/77))

- **Floats that start with a dot are recognised.** `.5` was read as `.`
  followed by `5`, so the float-zero rules never saw it. It now becomes `0.5`,
  also after a keyword (`return.5` becomes `return 0.5`) without joining the
  two into one name. ([#69](https://github.com/atelico/gdstyle/pull/69),
  [#75](https://github.com/atelico/gdstyle/pull/75))

- **Number literals Godot rejects are reported.** `1.5.5`, `1.5.abs()`,
  `1.abs()`, `1e5.5`, `0x1F.5`, `0x_FF`, `1__0` and `1._5` are now
  `syntax/lex-error` errors, matching Godot 4. Before, `check` was silent and
  `fmt` could make things worse (`1.abs()` became `1.0abs()`).
  ([#76](https://github.com/atelico/gdstyle/pull/76))

- **One warning per problem.** A missing float zero (`5.`) was reported by
  both `format/number-literals` and `format/float-literal-zeros`, and `0XAB`
  drew two `format/number-literals` warnings.
  ([#79](https://github.com/atelico/gdstyle/pull/79))

### Changed

- **Properties with `set` / `get` blocks are set apart.** A variable with an
  indented accessor block now gets one blank line on each side, like a small
  function, instead of being packed against its neighbours.
  ([#49](https://github.com/atelico/gdstyle/issues/49),
  [#64](https://github.com/atelico/gdstyle/pull/64))

- **`format/trailing-newline` reports blank lines at the end of a file.** It
  only reported a missing final newline, so `check --fix` left trailing blank
  lines that `fmt` removed. It now reports and removes them too.
  ([#74](https://github.com/atelico/gdstyle/pull/74))

- **`format/number-literals` covers hex and binary literals only.** Missing
  float zeros belong to `format/float-literal-zeros`, so turning that rule off
  now silences them. Uppercase binary prefixes (`0B11`) are now reported.
  ([#79](https://github.com/atelico/gdstyle/pull/79))

- **A directive must start its comment.** `# gdstyle:ignore` placed after
  other comment text (`#1 note  # gdstyle:ignore=...`) no longer applies; put
  it on its own line above instead.
  ([#72](https://github.com/atelico/gdstyle/pull/72))

- **Faster.** On a 3,300-file project, `fmt --check` takes 22% less time than
  in 0.4.0 and `check` uses 17% less CPU, from fewer lexing passes and a
  cheaper directive scan.
  ([#67](https://github.com/atelico/gdstyle/pull/67),
  [#68](https://github.com/atelico/gdstyle/pull/68),
  [#72](https://github.com/atelico/gdstyle/pull/72))

### Changelog

- Fixed: `fmt` honors rules set to `"off"` and `# gdstyle:ignore` comments,
  for every rule (#53, #59, #65, #68, #72, #78).
- Fixed: `fmt --diff` prints a unified diff that `git apply` accepts (#54,
  #63).
- Fixed: reordering keeps `#region` folds intact (#52, #62).
- Fixed: `####` banners, doc comments on bodyless `@abstract func` and class
  docs above annotated headers are formatted correctly (#47, #48, #50, #58,
  #60, #61).
- Fixed: boolean operators, property accessors, non-ASCII text, blank lines,
  trailing semicolons and leading-dot floats (#57, #66, #67, #69, #70, #71,
  #73, #75, #77).
- Fixed: number literals Godot rejects are reported as errors (#76).
- Changed: properties with `set` / `get` blocks get a blank line on each side
  (#49, #64).
- Changed: `format/trailing-newline` also reports trailing blank lines (#74),
  and `format/number-literals` covers hex and binary only (#79).
- Library: `ClassMember::Variable` / `StaticVariable` gain
  `has_accessor_block`, and `check_class_member_order` takes the token list
  (#64, #66, #72).

### Upgrading

No configuration changes are required. Some files will be reformatted once:
properties with accessor blocks get blank lines around them, `.5` becomes
`0.5`, trailing semicolons are removed, and trailing blank lines are trimmed by
`check --fix` too. If you relied on `fmt` applying a rule you had turned off or
suppressed, it no longer does. Files with number literals Godot rejects now
fail `check` with `syntax/lex-error`; Godot doesn't run them either.

If you use gdstyle as a Rust library: `ClassMember::Variable` and
`ClassMember::StaticVariable` have a new `has_accessor_block: bool` field, so
code that builds them or matches them without `..` needs updating, and
`rules::ordering::check_class_member_order` now takes `tokens: &[Token]` after
`file`.
