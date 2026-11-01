## gdstyle 0.6.0

A release about speed and fewer false warnings. `fmt` is about twice as fast
as 0.5.0 on a project of small files and up to four times as fast on large
files, and every pass now takes time in proportion to the file's length. A
new `comment_wrap` setting controls how long comments are wrapped, and the
rules reported since 0.5.0 no longer fire on format strings, conditional
returns or `@export_group("")`.

The version is 0.6.0 rather than 0.5.1 because a few types and functions in
the Rust library changed (see Upgrading). The CLI, the config file and the
Godot plugin work as before.

Thanks to [@ndhb](https://github.com/ndhb) for #86, #87 and #88, and to
[@Aureliolo](https://github.com/Aureliolo) for #85.

### Highlights

- **Faster, on small and large files.** Several passes walked back over
  earlier lines for every line, so their cost grew with the square of the
  file length. All of them are now linear, and `fmt` lexes and parses each
  version of a file once instead of once per pass. Single-threaded times
  against 0.5.0:

  | | 0.5.0 | 0.6.0 |
  |---|---|---|
  | `fmt --check`, Godot demo projects (461 files) | 579 ms | 309 ms |
  | `fmt --check`, one 42,000-line file | 1,085 ms | 248 ms |
  | `check`, one 42,000-line file | 238 ms | 106 ms |

  A CI gate now fails any pull request that makes `check` or `fmt` more than
  3% slower, measured on the demo projects and on large generated files.
  ([#89](https://github.com/atelico/gdstyle/pull/89),
  [#94](https://github.com/atelico/gdstyle/pull/94))

- **`comment_wrap` setting.** Controls comment lines longer than
  `max_line_length`:

  ```toml
  comment_wrap = "paragraph"   # or "line" (default), or "off"
  ```

  `"line"` wraps the long line on its own, as before. `"paragraph"` also
  refills the comment lines after it, so no word is left alone mid-paragraph.
  `"off"` leaves comments alone and `format/max-line-length` only reports
  them. In every mode, wrapping never splits a `# gdstyle:` directive from its
  reason, never touches `[codeblock]` contents, commented-out code or table
  rows, and never starts a `##` line with an `@` word (Godot would read it as
  a doc tag). ([#85](https://github.com/atelico/gdstyle/issues/85),
  [#91](https://github.com/atelico/gdstyle/pull/91),
  [#92](https://github.com/atelico/gdstyle/pull/92),
  [#95](https://github.com/atelico/gdstyle/pull/95))

### Fixed

- **`quality/process-get-node` ignores strings and comments.** `"%s" % [x]`
  and `"$5"` were read as node lookups. `%"Name"` and `$"Path"` are still
  reported. ([#86](https://github.com/atelico/gdstyle/issues/86),
  [#82](https://github.com/atelico/gdstyle/pull/82),
  [#92](https://github.com/atelico/gdstyle/pull/92))

- **`quality/no-else-return` only fires when the branch always returns.** A
  `return` inside a nested `if` made the `else` look unnecessary, though the
  branch could fall through. Multi-line and one-line returns, and longer
  `elif` chains, are now read correctly too.
  ([#87](https://github.com/atelico/gdstyle/issues/87),
  [#84](https://github.com/atelico/gdstyle/pull/84),
  [#93](https://github.com/atelico/gdstyle/pull/93))

- **`@export_group("")` no longer marks the next variable as exported.**
  `naming/private-underscore-prefix` reported a plain `var _private` after
  it, and the member order put it with the exports.
  `@export_storage var _state` is no longer reported either.
  ([#88](https://github.com/atelico/gdstyle/issues/88),
  [#90](https://github.com/atelico/gdstyle/pull/90),
  [#92](https://github.com/atelico/gdstyle/pull/92))

- **`match` and `when` work as names.** A `when` parameter with a default
  call could make a function lose its body. ([#81](https://github.com/atelico/gdstyle/pull/81),
  [#92](https://github.com/atelico/gdstyle/pull/92))

- **Comments in a parameter list are skipped.** They made
  `quality/max-parameters` miscount.
  ([#83](https://github.com/atelico/gdstyle/pull/83))

- **`unreachable-code` and `max-returns` read returns the same way.** Lambdas,
  backslash continuations, `return;` and strings that span lines no longer
  confuse them. ([#92](https://github.com/atelico/gdstyle/pull/92),
  [#95](https://github.com/atelico/gdstyle/pull/95))

- **Reordering keeps multi-line annotations and indented comments in
  place.** An `@export_range(` whose arguments span lines was split from its
  variable, an indented comment at the end of a function moved to script
  level, and a multi-line `@warning_ignore_start(...)` region could gain or
  lose members. ([#93](https://github.com/atelico/gdstyle/pull/93),
  [#95](https://github.com/atelico/gdstyle/pull/95))

### Changelog

- Changed: `fmt` is about 2x faster than 0.5.0 on small files and up to 4x on
  large ones; `check` up to 2x on large files (#94).
- Added: `comment_wrap = "line" | "paragraph" | "off"` (#85, #91).
- Fixed: `process-get-node` ignores format strings and comments (#86, #82).
- Fixed: `no-else-return` only fires when the branch always returns (#87, #84,
  #93).
- Fixed: `@export_group("")` no longer marks the next variable as exported
  (#88, #90).
- Fixed: `match` / `when` as names, comments in parameter lists, return
  detection, multi-line annotations and indented comments in reorders (#81,
  #83, #92, #93, #95).
- Library: `Config` gains `comment_wrap`, and four `quality` check functions
  take a `&StringLines` (#91, #92).

### Upgrading

No configuration changes are required, and the default comment wrapping is
unchanged. `fmt` now puts a blank line between the last export and a plain
variable under `@export_group("")`, as it does elsewhere.

If you use gdstyle as a Rust library: `Config` has a new `comment_wrap:
CommentWrap` field, so code that builds it field by field needs updating
(`..Config::default()` keeps working). `rules::quality::check_no_else_return`,
`check_unreachable_code`, `check_process_get_node` and `check_max_returns`
take a `&token::StringLines` as their new first argument
(`StringLines::new(&tokens, line_count)` builds it).
