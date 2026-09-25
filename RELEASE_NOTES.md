## gdstyle 0.3.1

`check` now fails on code that Godot won't load: reserved words used as names,
and a raw-string case that used to be misread. `format/large-number-underscores`
gets a threshold you can tune. The formatter no longer changes the value of a
string literal when it wraps a long line.

### Added

- **New rule `syntax/reserved-identifier` (default severity: error).** A
  reserved word used as a declared name passed `check`, but Godot won't parse
  the file (`Parse Error: Expected parameter name.`):

  ```gdscript
  func _prose(namespace: String) -> Dictionary:
  	return { "namespace": namespace }

  func trait(id: String) -> float:
  	return 0.0
  ```

  ```
  4:13 error 'namespace' is a reserved word in GDScript and cannot be used as a parameter name [syntax/reserved-identifier]
  8:6 error 'trait' is a reserved word in GDScript and cannot be used as a function name [syntax/reserved-identifier]
  1 file checked, 2 errors found.          $? = 1
  ```

  The rule checks function, variable, constant, signal, enum, enum-member,
  inner-class, `class_name` and loop-variable names. It also checks the
  parameters of functions, lambdas, signals and property setters.

  The word list follows Godot's tokenizer and was checked against Godot 4.6.2
  in every one of those positions. `match`, `when`, `PI`, `TAU`, `INF` and
  `NAN` are left out on purpose, because Godot accepts them as names. Member
  access (`obj.class`), node paths (`$UI/class`, `%signal`), dictionary keys
  and strings are not flagged.

  Because it is an error, `check` exits `1` on these files. Any file it flags
  already fails to load in Godot. To keep it as a warning, set
  `"syntax/reserved-identifier" = "warn"` under `[rules]`.
  ([#32](https://github.com/atelico/gdstyle/issues/32),
  [#34](https://github.com/atelico/gdstyle/pull/34))

- **`large_number_threshold` config setting** for
  `format/large-number-underscores`. The rule flags numbers at or above this
  value. The default, `10_000`, is the rule's existing floor, so integers
  behave as before. Audio and similar code can raise it to leave sample rates
  (`44100`) and PCM limits (`32768`) as they are:

  ```toml
  large_number_threshold = 1_000_000
  ```

  ([#33](https://github.com/atelico/gdstyle/issues/33),
  [#35](https://github.com/atelico/gdstyle/pull/35),
  [#36](https://github.com/atelico/gdstyle/pull/36))

### Changed

- **`format/large-number-underscores` now covers floats.** Before, `32768`
  was a warning and `32768.0` on the same line was not. The rule now groups
  the digits before the decimal point (`32768.0` → `32_768.0`,
  `1234567.5` → `1_234_567.5`). Digits after the point and exponents are left
  alone. **Existing projects may see new warnings on large float literals.**
  Raise `large_number_threshold`, or turn the rule off, if you don't want them.
- Numbers written with leading zeros (`00010000`) are no longer flagged. Before,
  the fix removed the zeros.
- On a float like `1000000.`, the fix for this rule and the fix for the missing
  trailing zero overlap. `gdstyle fmt` applies both in one run. `check --fix`
  applies one fix per run, so it needs two runs to reach `1_000_000.0`.

### Fixed

- **Raw strings containing `\"` were cut short.** In Godot,
  `r"a\" var self"` is a single string, but gdstyle ended it at the `\"`. It
  then reported a false `unterminated string` error and linted the rest of the
  string as code. Now the lexer and Godot agree on where every raw string ends.
  This was checked against Godot 4.6.2 on 60,000 generated literals. Godot's own
  `parser/features/r_strings.gd` test file used to produce 14 errors and now
  checks clean. `r"\"`, which Godot rejects, is now reported as an error; it
  used to pass. `R"..."` is no longer treated as a raw string, because Godot
  only accepts lowercase `r`.
  ([#37](https://github.com/atelico/gdstyle/pull/37))
- **`fmt` no longer changes string values when it wraps long lines.** To find
  where it can break a line, the formatter looks for commas outside strings. It
  had the same raw-string bug as above, and it also didn't recognise
  triple-quoted strings (`'''...'''`, `"""..."""`). A comma inside either kind of
  string could therefore become a line break, adding a newline and indentation
  to the string's value. The resulting file still loaded in Godot, so the change
  went unnoticed. Both cases are fixed. ([#37](https://github.com/atelico/gdstyle/pull/37))

### Upgrading

No configuration changes are required. Two things can change what `check`
reports:

- Files that use a reserved word as a name now fail `check` with an error.
- Large float literals now get `format/large-number-underscores` warnings.
