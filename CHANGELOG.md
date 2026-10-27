# Changelog

A short summary of every gdstyle release, newest first. The full notes for
each version are on its
[GitHub release](https://github.com/atelico/gdstyle/releases).

## 0.5.0 (2026-10-26)

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

## 0.4.0 (2026-10-17)

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

## 0.3.3 (2026-10-11)

- Fixed: `fmt` no longer changes which members a `@warning_ignore_start` /
  `@warning_ignore_restore` region covers when it reorders (#41).
- Fixed: `fmt` keeps region markers tight against the region, with blank lines
  outside it (#41).

## 0.3.2 (2026-10-11)

- Added: `blank_line_between_public_and_private` config setting, off by
  default (#38, #39).
- Fixed: `fmt` no longer separates a standalone `@warning_ignore(...)` line
  from its declaration (#40).

## 0.3.1 (2026-09-25)

- Added: `syntax/reserved-identifier` rule, error by default (#32, #34).
- Added: `large_number_threshold` config setting (#33, #35, #36).
- Changed: `format/large-number-underscores` covers floats and skips numbers
  with leading zeros (#35).
- Fixed: raw strings containing `\"` were cut short (#37).
- Fixed: `fmt` no longer changes string values when wrapping long lines (#37).

## 0.3.0 (2026-09-07)

- Fixed: `"error"` severity in `[rules]` is applied to diagnostics (#30).
- Added: unknown rule names in `[rules]` are reported.
- Added: `check --max-warnings <N>`.
- Added: `GdStyle.set_rule_severity(rule, severity)` in the GDExtension.

## 0.2.5 (2026-08-02)

- Fixed: `format/one-statement-per-line` no longer splits a `;` inside an
  inline block.
- Fixed: `syntax/lex-error` no longer fires on an aligned line continuation.
- Changed: `format/trailing-comma` only fires when the closing bracket starts
  its own line.

## 0.2.4 (2026-07-14)

- Fixed: `quality/duplicate-dict-key` no longer misses duplicates after a
  comment (#28).

## 0.2.3 (2026-07-11)

- Fixed: `--fix` no longer corrupts files with CRLF line endings (#25).
- Fixed: `quality/duplicate-dict-key` no longer flags multi-argument
  constructor calls (#26).

## 0.2.2 (2026-07-11)

- Fixed: single-line dictionaries and call parentheses are formatted (#21).
- Added: `include` config to lint paths inside an excluded directory (#22).
- Improved: the Godot editor plugin honors `exclude` and `include`.

## 0.2.1 (2026-07-08)

- Fixed: leading `#` comments stay attached to their declaration (#17).
- Improved: `order/class-member-order` can be disabled per member, which also
  pins the member against reordering (#18).

## 0.2.0 (2026-07-07)

- Fixed: `quality/self-comparison` no longer fires on different dotted paths
  (#10).
- Fixed: `quality/duplicate-dict-key` no longer flags enum keys from another
  class (#11).
- Fixed: Godot plugin diagnostic colors follow the editor theme (#9).
- Improved: `quality/duplicate-dict-key` detects duplicates in nested
  dictionaries (#12).

## 0.1.7 (2026-06-09)

- Fixed: the Godot plugin no longer fails to write `settings.json` on startup.

## 0.1.6 (2026-06-04)

- Fixed: `order/class-member-order` no longer fires on every inner-class
  member.

## 0.1.5 (2026-06-03)

- Fixed: `fmt` no longer duplicates `@abstract` / `class_name` lines.

## 0.1.4 (2026-06-02)

- Added: `# gdstyle:ignore-file` directive for file-scope suppression.

## 0.1.3 (2026-06-01)

- Fixed: `quality/unreachable-code` no longer flags the closing `)` of a
  multi-line statement.

## 0.1.2 (2026-05-31)

- Fixed: `fmt` no longer detaches `##` doc comments from their function.

## 0.1.1 (2026-05-31)

- Fixed: `quality/no-self-assign` no longer flags `obj.field = field`.
- Added: pre-commit framework integration.

## 0.1.0 (2026-05-28)

- First public release: CLI, 54 lint rules, formatter, scene-aware autofix,
  Godot editor plugin and TOML config.
