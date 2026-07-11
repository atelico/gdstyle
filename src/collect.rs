//! File discovery: walk directories and pick the `.gd` (and, for scene-aware
//! renames, `.tscn`/`.tres`) files to lint or format, honoring the config's
//! `exclude`/`include` globs.
//!
//! Shared by the CLI (`src/main.rs`) and the Godot GDExtension
//! (`gdstyle-gdext`) so both apply identical selection rules.

use crate::config::Config;
use colored::Colorize;
use globset::Glob;
use std::path::{Path, PathBuf};

/// Compile pattern strings into matchers. Each pattern is tried first as
/// `**/{pattern}` (so a bare name like `addons` matches at any depth) and then
/// verbatim, so an already-anchored pattern still works. Invalid globs are
/// skipped rather than aborting the run.
fn build_glob_patterns(patterns: &[String]) -> Vec<globset::GlobMatcher> {
    patterns
        .iter()
        .filter_map(|p| {
            Glob::new(&format!("**/{}", p))
                .or_else(|_| Glob::new(p))
                .ok()
                .map(|g| g.compile_matcher())
        })
        .collect()
}

/// Decides which paths are walked and linted, combining `exclude` and
/// `include` globs. An `include` always wins over an `exclude`, regardless of
/// order, so a broad exclude (e.g. `addons`) can be carved out with a narrower
/// include (e.g. `addons/my_plugin`).
///
/// Excluding a directory normally prunes its whole subtree, so the walkers
/// can't reach a nested include by matching alone. When any include is
/// configured, they instead keep descending into excluded (non-hidden)
/// directories, carrying an inherited-exclusion flag so only force-included
/// paths beneath survive and their excluded siblings don't leak back in.
/// Hidden directories (`.godot`, `.git`, …) are always pruned regardless.
pub struct PathFilter {
    exclude: Vec<globset::GlobMatcher>,
    include: Vec<globset::GlobMatcher>,
}

impl PathFilter {
    /// Build a filter from raw exclude/include pattern strings.
    pub fn new(exclude: &[String], include: &[String]) -> Self {
        Self {
            exclude: build_glob_patterns(exclude),
            include: build_glob_patterns(include),
        }
    }

    /// Build a filter from a loaded [`Config`]'s `exclude`/`include` lists.
    pub fn from_config(config: &Config) -> Self {
        Self::new(&config.exclude, &config.include)
    }

    fn is_excluded(&self, path: &str) -> bool {
        self.exclude.iter().any(|m| m.is_match(path))
    }

    fn is_included(&self, path: &str) -> bool {
        self.include.iter().any(|m| m.is_match(path))
    }

    fn has_includes(&self) -> bool {
        !self.include.is_empty()
    }

    /// Whether `path` is excluded once inherited exclusion and force-includes
    /// are resolved. An include wins over an exclude, whether the exclusion is
    /// local to `path` or inherited from an excluded ancestor.
    fn resolve_excluded(&self, path: &str, subtree_excluded: bool) -> bool {
        (subtree_excluded || self.is_excluded(path)) && !self.is_included(path)
    }
}

/// Collect every `.gd` file reachable from `paths`, applying `filter`. Explicit
/// file paths are always kept; directories are walked recursively. The result
/// is sorted for deterministic output.
///
/// # Example
/// ```no_run
/// use gdstyle::collect::{collect_gdscript_files, PathFilter};
/// use std::path::PathBuf;
///
/// let filter = PathFilter::new(&["addons".into()], &["addons/my_plugin".into()]);
/// let files = collect_gdscript_files(&[PathBuf::from("res-project")], &filter);
/// # let _ = files;
/// ```
pub fn collect_gdscript_files(paths: &[PathBuf], filter: &PathFilter) -> Vec<PathBuf> {
    let mut files = Vec::new();

    for path in paths {
        if path.is_file() {
            if path.extension().is_some_and(|ext| ext == "gd") {
                files.push(path.clone());
            }
        } else if path.is_dir() {
            collect_from_directory(path, filter, false, &mut files);
        }
    }

    files.sort();
    files
}

fn collect_from_directory(
    dir: &Path,
    filter: &PathFilter,
    subtree_excluded: bool,
    files: &mut Vec<PathBuf>,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            // Surface unreadable directories instead of silently skipping.
            // Otherwise a permission error reads as "no .gd files found".
            eprintln!(
                "{}: cannot read directory {}: {}",
                "warning".yellow(),
                dir.display(),
                e
            );
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let path_str = path.to_string_lossy();
        let excluded = filter.resolve_excluded(path_str.as_ref(), subtree_excluded);

        if path.is_dir() {
            // Skip hidden directories (.godot, .git, …) unconditionally.
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            // Prune an excluded directory, unless an include is configured that
            // might live beneath it — then descend, keeping the subtree
            // excluded-by-default so only the carve-out survives.
            if excluded && !filter.has_includes() {
                continue;
            }
            collect_from_directory(&path, filter, excluded, files);
        } else if !excluded && path.extension().is_some_and(|ext| ext == "gd") {
            files.push(path);
        }
    }
}

/// Recursively collect `.tscn` / `.tres` scene/resource files under `paths`,
/// applying the same `filter`. Used by `--unsafe-fix` to rewrite editor-wired
/// signal/method connections when a `.gd` rename would otherwise leave them
/// stale.
pub fn collect_scene_files(paths: &[PathBuf], filter: &PathFilter) -> Vec<PathBuf> {
    fn walk(dir: &Path, filter: &PathFilter, subtree_excluded: bool, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let path_str = path.to_string_lossy();
            let excluded = filter.resolve_excluded(path_str.as_ref(), subtree_excluded);
            if path.is_dir() {
                if entry.file_name().to_string_lossy().starts_with('.') {
                    continue;
                }
                if excluded && !filter.has_includes() {
                    continue;
                }
                walk(&path, filter, excluded, out);
            } else if !excluded
                && path
                    .extension()
                    .is_some_and(|ext| ext == "tscn" || ext == "tres")
            {
                out.push(path);
            }
        }
    }

    let mut files = Vec::new();
    for path in paths {
        if path.is_dir() {
            walk(path, filter, false, &mut files);
        }
    }
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::fs;
    use tempfile::TempDir;

    /// Create an empty `.gd` file at `root/rel`, making parent directories.
    fn touch(root: &Path, rel: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"extends Node\n").unwrap();
    }

    /// Collect `.gd` files under `root` with the given exclude/include globs,
    /// returning their paths relative to `root` with forward slashes.
    fn collected(root: &Path, exclude: &[&str], include: &[&str]) -> BTreeSet<String> {
        let to_owned = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let filter = PathFilter::new(&to_owned(exclude), &to_owned(include));
        collect_gdscript_files(&[root.to_path_buf()], &filter)
            .into_iter()
            .map(|p| {
                p.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect()
    }

    #[test]
    fn exclude_prunes_subtree_when_no_include() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        touch(root, "src/player.gd");
        touch(root, "addons/plugin_a/a.gd");

        let got = collected(root, &["addons"], &[]);
        assert!(got.contains("src/player.gd"), "{:?}", got);
        assert!(
            !got.iter().any(|p| p.starts_with("addons/")),
            "excluded addons must be pruned: {:?}",
            got
        );
    }

    #[test]
    fn include_carves_one_plugin_out_of_excluded_addons() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        touch(root, "src/player.gd");
        touch(root, "addons/my_plugin/main.gd");
        touch(root, "addons/my_plugin/nested/deep.gd");
        touch(root, "addons/third_party/lib.gd");

        let got = collected(root, &["addons"], &["addons/my_plugin"]);
        assert!(got.contains("src/player.gd"), "{:?}", got);
        assert!(got.contains("addons/my_plugin/main.gd"), "{:?}", got);
        // The whole included subtree is linted, nested files included.
        assert!(got.contains("addons/my_plugin/nested/deep.gd"), "{:?}", got);
        // A sibling plugin under the same excluded dir must not leak back in.
        assert!(!got.contains("addons/third_party/lib.gd"), "{:?}", got);
    }

    #[test]
    fn hidden_dirs_are_pruned_even_with_include_set() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        touch(root, ".godot/imported/cache.gd");
        touch(root, "src/player.gd");

        // A configured include must not cause hidden dirs to be walked.
        let got = collected(root, &["addons"], &["addons/my_plugin"]);
        assert!(got.contains("src/player.gd"), "{:?}", got);
        assert!(
            !got.iter().any(|p| p.starts_with(".godot/")),
            "hidden .godot must stay pruned: {:?}",
            got
        );
    }

    #[test]
    fn include_can_reinclude_a_single_file() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        touch(root, "gen/keep.gd");
        touch(root, "gen/skip.gd");

        let got = collected(root, &["gen"], &["gen/keep.gd"]);
        assert!(got.contains("gen/keep.gd"), "{:?}", got);
        assert!(!got.contains("gen/skip.gd"), "{:?}", got);
    }
}
