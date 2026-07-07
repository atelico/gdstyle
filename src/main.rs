use clap::{Parser, Subcommand};
use colored::Colorize;
use gdstyle::ast::ClassMember;
use gdstyle::config::Config;
use gdstyle::diagnostic::{Diagnostic, Severity};
use gdstyle::fixer;
use gdstyle::lexer::Lexer;
use gdstyle::linter;
use gdstyle::parser::Parser as GdParser;
use gdstyle::reporter::{self, OutputFormat};
use gdstyle::rules;
use globset::Glob;
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};

fn parse_members(source: &str) -> Vec<ClassMember> {
    let mut lexer = Lexer::new(source);
    let tokens = lexer.tokenize();
    GdParser::new(&tokens).parse()
}

#[derive(Parser)]
#[command(name = "gdstyle")]
#[command(about = "A fast, opinionated linter and formatter for GDScript (Godot 4.x)")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Files or directories to lint. Defaults to current directory.
    #[arg(default_value = ".")]
    paths: Vec<PathBuf>,

    /// Output format: text or json.
    #[arg(long, default_value = "text")]
    format: String,

    /// Path to configuration file.
    #[arg(long, short)]
    config: Option<PathBuf>,

    /// Only check specific rules (comma-separated).
    #[arg(long)]
    select: Option<String>,

    /// Ignore specific rules (comma-separated).
    #[arg(long)]
    ignore: Option<String>,

    /// Maximum line length override.
    #[arg(long)]
    max_line_length: Option<usize>,

    /// Disable colored output.
    #[arg(long)]
    no_color: bool,

    /// Auto-fix safe violations.
    #[arg(long)]
    fix: bool,

    /// Auto-fix all violations including unsafe ones.
    #[arg(long)]
    unsafe_fix: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Lint files (default behavior).
    Check {
        /// Files or directories to lint.
        #[arg(default_value = ".")]
        paths: Vec<PathBuf>,

        /// Auto-fix safe violations.
        #[arg(long)]
        fix: bool,

        /// Auto-fix all violations including unsafe ones.
        #[arg(long)]
        unsafe_fix: bool,

        /// Output format: text or json.
        #[arg(long, default_value = "text")]
        format: String,

        /// Path to configuration file.
        #[arg(long, short)]
        config: Option<PathBuf>,

        /// Only check specific rules (comma-separated).
        #[arg(long)]
        select: Option<String>,

        /// Ignore specific rules (comma-separated).
        #[arg(long)]
        ignore: Option<String>,

        /// Maximum line length override.
        #[arg(long)]
        max_line_length: Option<usize>,

        /// Disable colored output.
        #[arg(long)]
        no_color: bool,
    },
    /// Format files in place.
    Fmt {
        /// Files or directories to format.
        #[arg(default_value = ".")]
        paths: Vec<PathBuf>,

        /// Dry-run: exit 1 if any file would change.
        #[arg(long)]
        check: bool,

        /// Print unified diff of what would change.
        #[arg(long)]
        diff: bool,

        /// Path to configuration file.
        #[arg(long, short)]
        config: Option<PathBuf>,

        /// Disable colored output.
        #[arg(long)]
        no_color: bool,
    },
    /// List all available lint rules.
    Rules {
        /// Disable colored output.
        #[arg(long)]
        no_color: bool,
    },
    /// Generate a starter gdstyle.toml configuration file.
    Init {
        /// Overwrite existing config file.
        #[arg(long)]
        force: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Check {
            paths,
            fix,
            unsafe_fix,
            format,
            config,
            select,
            ignore,
            max_line_length,
            no_color,
        }) => {
            run_check(
                &paths,
                fix,
                unsafe_fix,
                &format,
                config.as_deref(),
                select.as_deref(),
                ignore.as_deref(),
                max_line_length,
                no_color,
            );
        }
        Some(Commands::Fmt {
            paths,
            check,
            diff,
            config,
            no_color,
        }) => {
            run_fmt(&paths, check, diff, config.as_deref(), no_color);
        }
        Some(Commands::Rules { no_color }) => {
            if no_color {
                colored::control::set_override(false);
            }
            print_rules();
        }
        Some(Commands::Init { force }) => {
            run_init(force);
        }
        None => {
            run_check(
                &cli.paths,
                cli.fix,
                cli.unsafe_fix,
                &cli.format,
                cli.config.as_deref(),
                cli.select.as_deref(),
                cli.ignore.as_deref(),
                cli.max_line_length,
                cli.no_color,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_check(
    paths: &[PathBuf],
    fix: bool,
    unsafe_fix: bool,
    format: &str,
    config_path: Option<&Path>,
    select: Option<&str>,
    ignore: Option<&str>,
    max_line_length: Option<usize>,
    no_color: bool,
) {
    if no_color {
        colored::control::set_override(false);
    }

    // Load config.
    let mut config = match config_path {
        Some(path) => match Config::from_file(path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{}: {}", "error".red(), e);
                process::exit(2);
            }
        },
        None => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            Config::find_and_load(&cwd)
        }
    };

    // Apply CLI overrides.
    if let Some(max_len) = max_line_length {
        config.max_line_length = max_len;
    }

    // Apply --select and --ignore.
    if let Some(select) = select {
        let selected: Vec<&str> = select.split(',').map(|s| s.trim()).collect();
        for rule in rules::all_rule_names() {
            if !selected.iter().any(|s| rule.contains(s)) {
                config
                    .rules
                    .insert(rule.to_string(), gdstyle::config::RuleSeverityConfig::Off);
            }
        }
    }
    if let Some(ignore) = ignore {
        for rule in ignore.split(',').map(|s| s.trim()) {
            config
                .rules
                .insert(rule.to_string(), gdstyle::config::RuleSeverityConfig::Off);
        }
    }

    let output_format = match format {
        "json" => OutputFormat::Json,
        _ => OutputFormat::Text,
    };

    // Collect files.
    let exclude_patterns = build_exclude_patterns(&config.exclude);
    let files = collect_gdscript_files(paths, &exclude_patterns);

    if files.is_empty() {
        eprintln!("{}: no .gd files found", "warning".yellow());
        process::exit(0);
    }

    let do_fix = fix || unsafe_fix;
    let safe_only = fix && !unsafe_fix;

    // Fixes are written to disk in place. No backup, no prompt. Remind the
    // user to have a clean working tree they can revert to.
    if do_fix {
        eprintln!(
            "{}: fixes are applied in place. Commit or back up your work first, \
             then review the diff.",
            "note".cyan()
        );
        if unsafe_fix {
            eprintln!(
                "{}: --unsafe-fix renames identifiers and rewrites references \
                 across .gd and .tscn/.tres files; review carefully.",
                "note".cyan()
            );
        }
    }

    // Lint (and optionally fix) all files. The per-file work is independent,
    // so we run it across rayon's thread pool and aggregate the results.
    struct LintFileResult {
        diagnostics: Vec<Diagnostic>,
        renames: Vec<fixer::AppliedRename>,
        was_fixed: bool,
    }

    let lint_results: Vec<LintFileResult> = files
        .par_iter()
        .map(|file_path| {
            let mut result = LintFileResult {
                diagnostics: Vec::new(),
                renames: Vec::new(),
                was_fixed: false,
            };
            let source = match std::fs::read_to_string(file_path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("{}: {}", "error".red(), e);
                    return result;
                }
            };
            let file_str = file_path.to_string_lossy().to_string();
            let diagnostics = linter::lint_source(&source, &file_str, &config);

            if do_fix {
                // Track renames for cross-file reference updates.
                if unsafe_fix {
                    let members = parse_members(&source);
                    result.renames =
                        fixer::extract_renames(&source, &diagnostics, &file_str, &members);
                }

                let fixed_source = fixer::apply_fixes(&source, &diagnostics, safe_only);
                if fixed_source != source {
                    if let Err(e) = std::fs::write(file_path, &fixed_source) {
                        eprintln!(
                            "{}: cannot write {}: {}",
                            "error".red(),
                            file_path.display(),
                            e
                        );
                    } else {
                        result.was_fixed = true;
                    }
                    // Re-lint to report remaining issues.
                    result.diagnostics = linter::lint_source(&fixed_source, &file_str, &config);
                } else {
                    result.diagnostics = diagnostics;
                }
            } else {
                result.diagnostics = diagnostics;
            }

            result
        })
        .collect();

    let mut all_diagnostics: Vec<Diagnostic> = Vec::new();
    let mut all_renames: Vec<fixer::AppliedRename> = Vec::new();
    let mut fixed_count = 0;
    for r in lint_results {
        all_diagnostics.extend(r.diagnostics);
        all_renames.extend(r.renames);
        if r.was_fixed {
            fixed_count += 1;
        }
    }
    // Sort diagnostics for deterministic output across runs (parallel
    // iteration produces them in non-deterministic order).
    all_diagnostics.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then_with(|| a.span.line.cmp(&b.span.line))
            .then_with(|| a.span.column.cmp(&b.span.column))
    });

    // Cross-file reference updates for --unsafe-fix. Each file's refs are
    // independent now that `all_renames` is fully gathered, so this fans out
    // across the thread pool too.
    if unsafe_fix && !all_renames.is_empty() {
        let cross_ref_fixed = AtomicUsize::new(0);
        files.par_iter().for_each(|file_path| {
            let source = match std::fs::read_to_string(file_path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!(
                        "{}: cannot read {}: {}",
                        "error".red(),
                        file_path.display(),
                        e
                    );
                    return;
                }
            };
            let file_str = file_path.to_string_lossy().to_string();
            let refs = fixer::find_cross_file_references(&source, &file_str, &all_renames);
            if refs.is_empty() {
                return;
            }
            let fixed = fixer::apply_cross_file_fixes(&source, &refs);
            if fixed != source {
                if let Err(e) = std::fs::write(file_path, &fixed) {
                    eprintln!(
                        "{}: cannot write {}: {}",
                        "error".red(),
                        file_path.display(),
                        e
                    );
                } else {
                    cross_ref_fixed.fetch_add(1, Ordering::Relaxed);
                }
            }

            // Report any remaining references that couldn't be fixed.
            let remaining_source =
                std::fs::read_to_string(file_path).unwrap_or_else(|_| source.clone());
            let remaining_refs =
                fixer::find_cross_file_references(&remaining_source, &file_str, &all_renames);
            for r in &remaining_refs {
                eprintln!(
                    "{}: {}:{}:{} reference to '{}' (renamed to '{}' in {}) was not updated",
                    "warning".yellow(),
                    r.file,
                    r.line,
                    r.column,
                    r.old_name,
                    r.new_name,
                    r.source_file,
                );
            }
        });
        let cross_ref_fixed_count = cross_ref_fixed.load(Ordering::Relaxed);
        if cross_ref_fixed_count > 0 {
            fixed_count += cross_ref_fixed_count;
        }

        // Rewrite editor-wired signal/method connections in scene files.
        // A `.gd`-only signal/function rename leaves `[connection signal="…"
        // method="…"]` rows stale, and the connection fails silently at
        // runtime, so `--unsafe-fix` must touch `.tscn`/`.tres` too.
        let scene_files = collect_scene_files(paths, &exclude_patterns);
        for scene_path in &scene_files {
            let Ok(scene_source) = std::fs::read_to_string(scene_path) else {
                continue;
            };
            let (rewritten, applied) = fixer::apply_scene_renames(&scene_source, &all_renames);
            if applied.is_empty() {
                continue;
            }
            match std::fs::write(scene_path, &rewritten) {
                Ok(()) => {
                    fixed_count += 1;
                    for r in &applied {
                        eprintln!(
                            "{}: {}:{} scene {} connection '{}' updated to '{}'",
                            "fixed".green(),
                            scene_path.display(),
                            r.line,
                            r.attribute,
                            r.old_name,
                            r.new_name,
                        );
                    }
                }
                Err(e) => eprintln!(
                    "{}: cannot write {}: {}",
                    "error".red(),
                    scene_path.display(),
                    e
                ),
            }
        }
    }

    // Output results.
    match output_format {
        OutputFormat::Text => {
            if !all_diagnostics.is_empty() {
                print!("{}", reporter::format_text(&all_diagnostics));
            }
            println!(
                "{}",
                reporter::format_summary(&all_diagnostics, files.len())
            );
            if do_fix && fixed_count > 0 {
                println!(
                    "{}",
                    format!(
                        "Fixed {} file{}.",
                        fixed_count,
                        if fixed_count == 1 { "" } else { "s" }
                    )
                    .green()
                );
            }
        }
        OutputFormat::Json => {
            println!("{}", reporter::format_json(&all_diagnostics));
        }
    }

    // Exit code.
    let has_errors = all_diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error);
    if has_errors {
        process::exit(1);
    }
}

fn run_fmt(paths: &[PathBuf], check: bool, diff: bool, config_path: Option<&Path>, no_color: bool) {
    if no_color {
        colored::control::set_override(false);
    }

    let config = match config_path {
        Some(path) => match Config::from_file(path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{}: {}", "error".red(), e);
                process::exit(2);
            }
        },
        None => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            Config::find_and_load(&cwd)
        }
    };

    let exclude_patterns = build_exclude_patterns(&config.exclude);
    let files = collect_gdscript_files(paths, &exclude_patterns);

    if files.is_empty() {
        eprintln!("{}: no .gd files found", "warning".yellow());
        process::exit(0);
    }

    // Per-file format work runs in parallel; output is then printed in path
    // order so the result is identical across runs.
    struct FormatFileResult {
        path: PathBuf,
        changed: bool,
        source: Option<String>,
        formatted: Option<String>,
        read_error: Option<String>,
        write_error: Option<String>,
    }

    let format_results: Vec<FormatFileResult> = files
        .par_iter()
        .map(|file_path| {
            let mut r = FormatFileResult {
                path: file_path.clone(),
                changed: false,
                source: None,
                formatted: None,
                read_error: None,
                write_error: None,
            };
            let source = match std::fs::read_to_string(file_path) {
                Ok(s) => s,
                Err(e) => {
                    r.read_error = Some(e.to_string());
                    return r;
                }
            };
            let formatted = gdstyle::formatter::format_source(&source, &config);
            if formatted == source {
                return r;
            }
            r.changed = true;
            if !check && !diff {
                if let Err(e) = std::fs::write(file_path, &formatted) {
                    r.write_error = Some(e.to_string());
                }
            } else if diff {
                r.source = Some(source);
                r.formatted = Some(formatted);
            }
            r
        })
        .collect();

    let mut would_change = false;
    let mut formatted_count = 0;
    for r in &format_results {
        if let Some(e) = &r.read_error {
            eprintln!("{}: {}", "error".red(), e);
            continue;
        }
        if let Some(e) = &r.write_error {
            eprintln!(
                "{}: cannot write {}: {}",
                "error".red(),
                r.path.display(),
                e
            );
            continue;
        }
        if !r.changed {
            continue;
        }
        would_change = true;
        if diff {
            if let (Some(src), Some(fmt)) = (&r.source, &r.formatted) {
                print_diff(&r.path, src, fmt);
            }
        } else if check {
            println!("{} {}", "Would reformat:".yellow(), r.path.display());
        } else {
            formatted_count += 1;
            println!("Formatted {}", r.path.display());
        }
    }

    if check || diff {
        if would_change {
            println!(
                "{}",
                format!(
                    "{} file{} would be reformatted.",
                    files.len(),
                    if files.len() == 1 { "" } else { "s" }
                )
                .yellow()
            );
            process::exit(1);
        } else {
            println!(
                "{}",
                format!(
                    "{} file{} already formatted.",
                    files.len(),
                    if files.len() == 1 { "" } else { "s" }
                )
                .green()
            );
        }
    } else {
        println!(
            "{}",
            format!(
                "Formatted {} file{}.",
                formatted_count,
                if formatted_count == 1 { "" } else { "s" }
            )
            .green()
        );
    }
}

fn print_diff(path: &Path, original: &str, formatted: &str) {
    let path_str = path.display().to_string();
    println!("--- a/{}", path_str);
    println!("+++ b/{}", path_str);

    let orig_lines: Vec<&str> = original.lines().collect();
    let fmt_lines: Vec<&str> = formatted.lines().collect();

    // Simple line-by-line diff.
    let max_lines = orig_lines.len().max(fmt_lines.len());
    for i in 0..max_lines {
        let orig = orig_lines.get(i).copied().unwrap_or("");
        let fmt = fmt_lines.get(i).copied().unwrap_or("");
        if orig != fmt {
            if !orig.is_empty() || i < orig_lines.len() {
                println!("{}", format!("-{}", orig).red());
            }
            if !fmt.is_empty() || i < fmt_lines.len() {
                println!("{}", format!("+{}", fmt).green());
            }
        } else {
            println!(" {}", orig);
        }
    }
    println!();
}

fn run_init(force: bool) {
    let config_name = "gdstyle.toml";
    let path = PathBuf::from(config_name);

    if path.exists() && !force {
        eprintln!(
            "{}: {} already exists. Use --force to overwrite.",
            "error".red(),
            config_name
        );
        process::exit(1);
    }

    let content = r#"# gdstyle.toml: starter configuration
#
# Place this file as `gdstyle.toml` or `.gdstyle.toml` in your project root.
# gdstyle will search for it starting from the current directory and walking
# up the directory tree.

# Maximum line length (default: 100)
max_line_length = 100

# Use tabs for indentation (default: true).
# Set to false if your project uses spaces.
use_tabs = true

# Maximum function body length in lines (default: 50)
max_function_length = 50

# Maximum file length in lines (default: 1000)
max_file_length = 1000

# Maximum number of function parameters (default: 5)
max_parameters = 5

# File and directory patterns to exclude from linting.
# These are matched as glob patterns against file paths.
exclude = [".godot", "addons"]

# Per-rule severity overrides.
# Values: "off" (disable), "warn" (warning), "error" (error)
#
# All rules are enabled with "warn" severity by default.
# Uncomment any line below to change its severity.
[rules]
# --- Naming ---
# "naming/class-name-pascal-case" = "warn"
# "naming/function-name-snake-case" = "warn"
# "naming/variable-name-snake-case" = "warn"
# "naming/constant-name-screaming-case" = "warn"
# "naming/signal-name-snake-case" = "warn"
# "naming/enum-name-pascal-case" = "warn"
# "naming/enum-member-screaming-case" = "warn"
# "naming/file-name-snake-case" = "warn"
# "naming/signal-past-tense" = "warn"
# "naming/private-underscore-prefix" = "warn"
# "naming/node-name-pascal-case" = "warn"

# --- Formatting ---
# "format/max-line-length" = "warn"
# "format/trailing-whitespace" = "warn"
# "format/trailing-newline" = "warn"
# "format/no-tabs-as-spaces" = "warn"
# "format/boolean-operators" = "warn"
# "format/double-quotes" = "warn"
# "format/comment-spacing" = "warn"
# "format/no-unnecessary-parens" = "warn"
# "format/number-literals" = "warn"
# "format/one-statement-per-line" = "warn"
# "format/blank-lines" = "warn"
# "format/trailing-comma" = "warn"
# "format/operator-spacing" = "warn"
# "format/float-literal-zeros" = "warn"
# "format/large-number-underscores" = "warn"
# "format/enum-one-per-line" = "warn"

# --- Ordering ---
# "order/class-member-order" = "warn"

# --- Quality ---
# "quality/max-function-length" = "warn"
# "quality/max-file-length" = "warn"
# "quality/max-parameters" = "warn"
"#;

    if let Err(e) = std::fs::write(&path, content) {
        eprintln!("{}: cannot write {}: {}", "error".red(), config_name, e);
        process::exit(2);
    }

    println!("{}", format!("Created {}", config_name).green());
}

fn print_rules() {
    let rules = rules::all_rules();
    println!(
        "{}",
        format!("Available lint rules ({}):", rules.len()).bold()
    );
    println!();

    for (name, description) in rules {
        println!("  {} {}", name.cyan(), description.dimmed());
    }
}

fn build_exclude_patterns(patterns: &[String]) -> Vec<globset::GlobMatcher> {
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

fn collect_gdscript_files(
    paths: &[PathBuf],
    exclude_patterns: &[globset::GlobMatcher],
) -> Vec<PathBuf> {
    let mut files = Vec::new();

    for path in paths {
        if path.is_file() {
            if path.extension().is_some_and(|ext| ext == "gd") {
                files.push(path.clone());
            }
        } else if path.is_dir() {
            collect_from_directory(path, exclude_patterns, &mut files);
        }
    }

    files.sort();
    files
}

fn collect_from_directory(
    dir: &Path,
    exclude_patterns: &[globset::GlobMatcher],
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

        // Check exclude patterns.
        if exclude_patterns
            .iter()
            .any(|p| p.is_match(path_str.as_ref()))
        {
            continue;
        }

        if path.is_dir() {
            // Skip hidden directories.
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            collect_from_directory(&path, exclude_patterns, files);
        } else if path.extension().is_some_and(|ext| ext == "gd") {
            files.push(path);
        }
    }
}

/// Recursively collect `.tscn` / `.tres` scene/resource files under `paths`.
/// Used by `--unsafe-fix` to rewrite editor-wired signal/method connections
/// when a `.gd` rename would otherwise leave them stale.
fn collect_scene_files(
    paths: &[PathBuf],
    exclude_patterns: &[globset::GlobMatcher],
) -> Vec<PathBuf> {
    fn walk(dir: &Path, exclude: &[globset::GlobMatcher], out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if exclude
                .iter()
                .any(|p| p.is_match(path.to_string_lossy().as_ref()))
            {
                continue;
            }
            if path.is_dir() {
                if entry.file_name().to_string_lossy().starts_with('.') {
                    continue;
                }
                walk(&path, exclude, out);
            } else if path
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
            walk(path, exclude_patterns, &mut files);
        }
    }
    files.sort();
    files
}
