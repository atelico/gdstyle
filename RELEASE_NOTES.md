## gdstyle 0.6.1

A release about leaving a broken file alone and never passing a run that
skipped one. A file that holds a merge conflict marker is now reported and
left untouched by every command, and `check` and `fmt` exit `2` whenever a
path could not be read, written or fixed, instead of printing an error and
passing.

Thanks to [@Aureliolo](https://github.com/Aureliolo) for finding and fixing
the conflict marker bug in #97.

### Highlights

- **Merge conflict markers are reported, and the file is left alone.** `fmt`
  turned `<<<<<<< HEAD` into `<<<<<< < HEAD`, and `check` passed the file
  with a spacing warning, though Godot could not load it. A line starting
  with seven or more `<`, `=`, `>` or `|` outside a string is now one
  `syntax/lex-error` (`merge conflict marker '<<<<<<<'`), which `check`
  reports as an error. That covers the `|||||||` base marker of
  `merge.conflictStyle = diff3`, the longer markers git writes for nested
  conflicts, and a marker after a backslash continuation. `fmt`,
  `check --fix` and `check --unsafe-fix` leave such a script untouched, name
  it on stderr and exit `2`. The guard sits in the library, so the Godot
  plugin's format and fix actions leave it alone too.
  ([#97](https://github.com/atelico/gdstyle/pull/97),
  [#98](https://github.com/atelico/gdstyle/pull/98))

- **A run that skipped something no longer passes.** `check` and `fmt` exit
  `2` when a path does not exist, a file or directory could not be read, a
  file could not be written, or a conflicted script kept a fix out.
  `check src tests` used to pass with `tests` gone, and `fmt` exited `0`
  after printing `cannot write`. Exit `2` outranks `1`, each failing path is
  reported once by name, and both commands repeat the count as their last
  line on stderr: `error: 2 paths could not be processed, see above`.
  ([#98](https://github.com/atelico/gdstyle/pull/98))

### Fixed

- **`--unsafe-fix` no longer leaves a rename half done in silence.** When the
  file that declares a renamed identifier could not be written, the rename
  still reached every file that referenced it. It now reaches none, the
  declaring file keeps its diagnostics, and the run exits `2`. A referencing
  file that could not be written, or that holds a conflict marker, fails the
  run too, with each stale reference named. A `.tscn` / `.tres` file that
  could not be read is reported instead of skipped.
  ([#98](https://github.com/atelico/gdstyle/pull/98))

- **`format/trailing-comma` no longer asks for a comma after a lex error**,
  such as `>>>>>>> topic` at the end of a dictionary. Its fix appended one on
  every run. ([#98](https://github.com/atelico/gdstyle/pull/98))

- **Hidden files are skipped.** An editor lock file such as `.#player.gd`
  ends in `.gd` and was linted, and so was a macOS `._player.gd` companion.
  A file whose name starts with `.` is now skipped inside a directory, as
  hidden directories already were. A hidden file named on the command line is
  still used. ([#98](https://github.com/atelico/gdstyle/pull/98))

- **A path named twice is processed once.** `check` linted it twice and `fmt`
  formatted it twice. ([#98](https://github.com/atelico/gdstyle/pull/98))

- **No summary when nothing was processed.** `check` printed "0 files
  checked, no issues found." and `fmt` a green "Formatted 0 files." next to
  the error. ([#98](https://github.com/atelico/gdstyle/pull/98))

### Library

New: `formatter::try_format_source` returns `Err(ConflictMarkerError)` for a
conflicted source, `collect::collect_gdscript_files_reporting` hands
unreadable paths to a callback, and `lexer::has_conflict_marker` tells
whether a source holds a marker. Changed: `fixer::apply_fixes` and
`formatter::format_source` return a conflicted source unchanged, and
`collect::collect_gdscript_files` skips hidden files, drops duplicate paths
and warns about a path that does not exist. No signature changed.
([#97](https://github.com/atelico/gdstyle/pull/97),
[#98](https://github.com/atelico/gdstyle/pull/98))

### Changelog

- Added: merge conflict markers are a `syntax/lex-error`, and `fmt`, `--fix`
  and `--unsafe-fix` leave a conflicted script untouched (#97, #98).
- Changed: `check` and `fmt` exit `2` when a path does not exist or a file
  could not be read, written or fixed, with a closing note on stderr (#98).
- Changed: hidden files (`.#player.gd`, `._player.gd`) are skipped (#98).
- Fixed: a rename is not applied to referencing files when the declaring file
  could not be written, and a stale reference fails the run (#98).
- Fixed: `format/trailing-comma` after a lex error, a path named twice, and
  empty summaries (#98).
- Library: `formatter::try_format_source`,
  `collect::collect_gdscript_files_reporting` and
  `lexer::has_conflict_marker` are new; `apply_fixes` and `format_source`
  leave a conflicted source alone (#97, #98).

### Upgrading

No configuration changes are required. Three things can change what a CI
job sees:

- A run with a missing path, an unreadable or unwritable file, or a
  conflicted script now exits `2` instead of `0` or `1`. A job that only runs
  `gdstyle fmt --check` now fails on a file with conflict markers, as `check`
  does.
- Hidden `.gd` files are no longer linted or formatted when a directory is
  walked. Name one on the command line to include it.
- `check --fix` and `--unsafe-fix` no longer edit a script that holds a
  conflict marker. Resolve the conflict first.

If you use gdstyle as a Rust library, nothing needs to change: the new
functions are additions, and `format_source` and `apply_fixes` keep their
signatures.
