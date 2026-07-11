use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

/// Top-level configuration for gdstyle.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Maximum line length (default: 100).
    pub max_line_length: usize,

    /// Whether to use tabs for indentation (default: true).
    pub use_tabs: bool,

    /// Maximum function body length in lines (default: 50).
    pub max_function_length: usize,

    /// Maximum file length in lines (default: 1000).
    pub max_file_length: usize,

    /// Maximum number of function parameters (default: 5).
    pub max_parameters: usize,

    /// Maximum number of return statements per function (default: 6).
    pub max_returns: usize,

    /// Maximum nesting depth inside a function (default: 4).
    pub max_nesting_depth: usize,

    /// Maximum number of local variables per function (default: 10).
    pub max_local_variables: usize,

    /// Maximum number of branches (if/elif/match) per function (default: 8).
    pub max_branches: usize,

    /// Maximum number of class-level variables (default: 15).
    pub max_class_variables: usize,

    /// Maximum number of public methods per class (default: 20).
    pub max_public_methods: usize,

    /// Maximum number of inner classes per file/class (default: 5).
    pub max_inner_classes: usize,

    /// File/directory patterns to exclude from linting.
    pub exclude: Vec<String>,

    /// Patterns that force-include paths even when `exclude` matches them.
    /// An include always wins over an exclude, regardless of order, so you can
    /// carve a specific subtree out of a broad exclude (e.g. exclude `addons`
    /// but include `addons/my_plugin`). Empty by default.
    pub include: Vec<String>,

    /// Per-rule configuration overrides.
    /// Key is the rule name, value is "off", "warn", or "error".
    pub rules: HashMap<String, RuleSeverityConfig>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RuleSeverityConfig {
    Off,
    Warn,
    Error,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_line_length: 100,
            use_tabs: true,
            max_function_length: 50,
            max_file_length: 1000,
            max_parameters: 5,
            max_returns: 6,
            max_nesting_depth: 4,
            max_local_variables: 10,
            max_branches: 8,
            max_class_variables: 15,
            max_public_methods: 20,
            max_inner_classes: 5,
            exclude: vec![".godot".to_string(), "addons".to_string()],
            include: Vec::new(),
            rules: HashMap::new(),
        }
    }
}

impl Config {
    /// Load configuration from a TOML file.
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::ReadError(path.display().to_string(), e.to_string()))?;
        let config: Config = toml::from_str(&content)
            .map_err(|e| ConfigError::ParseError(path.display().to_string(), e.to_string()))?;
        Ok(config)
    }

    /// Search for a config file starting from the given directory, walking up.
    pub fn find_and_load(start_dir: &Path) -> Self {
        let config_names = ["gdstyle.toml", ".gdstyle.toml"];
        let mut dir = start_dir.to_path_buf();

        loop {
            for name in &config_names {
                let path = dir.join(name);
                if path.exists() {
                    match Self::from_file(&path) {
                        Ok(config) => return config,
                        Err(e) => {
                            eprintln!("warning: failed to load {}: {}", path.display(), e);
                        }
                    }
                }
            }

            if !dir.pop() {
                break;
            }
        }

        Self::default()
    }

    /// Rules that are off by default and must be explicitly enabled.
    const OFF_BY_DEFAULT: &[&str] = &[
        "quality/type-hint",
        "quality/empty-function",
        "quality/no-debug-print",
    ];

    /// Check if a rule is enabled with the given default severity.
    pub fn is_rule_enabled(&self, rule_name: &str) -> bool {
        match self.rules.get(rule_name) {
            Some(RuleSeverityConfig::Off) => false,
            Some(_) => true,
            None => !Self::OFF_BY_DEFAULT.contains(&rule_name),
        }
    }
}

#[derive(Debug)]
pub enum ConfigError {
    ReadError(String, String),
    ParseError(String, String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::ReadError(path, err) => write!(f, "cannot read {}: {}", path, err),
            ConfigError::ParseError(path, err) => write!(f, "cannot parse {}: {}", path, err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = Config::default();
        assert_eq!(config.max_line_length, 100);
        assert!(config.use_tabs);
        assert_eq!(config.max_function_length, 50);
        assert!(config.exclude.contains(&".godot".to_string()));
        assert!(config.exclude.contains(&"addons".to_string()));
        assert!(config.include.is_empty());
    }

    #[test]
    fn parse_toml_config() {
        let toml = r#"
max_line_length = 80
use_tabs = false
max_function_length = 30
exclude = [".godot", "addons", "generated"]

[rules]
"naming/class-name-pascal-case" = "error"
"format/max-line-length" = "warn"
"quality/max-function-length" = "off"
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.max_line_length, 80);
        assert!(!config.use_tabs);
        assert_eq!(config.max_function_length, 30);
        assert_eq!(config.exclude.len(), 3);
        assert_eq!(
            config.rules.get("quality/max-function-length"),
            Some(&RuleSeverityConfig::Off)
        );
    }

    #[test]
    fn parse_include_overrides_exclude() {
        let toml = r#"
exclude = ["addons"]
include = ["addons/my_plugin"]
"#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.exclude, vec!["addons".to_string()]);
        assert_eq!(config.include, vec!["addons/my_plugin".to_string()]);
    }

    #[test]
    fn include_defaults_to_empty_when_omitted() {
        // Existing configs that predate `include` must still parse.
        let config: Config = toml::from_str("exclude = [\"addons\"]").unwrap();
        assert!(config.include.is_empty());
    }

    #[test]
    fn rule_enabled_check() {
        let mut config = Config::default();
        config.rules.insert(
            "naming/class-name-pascal-case".to_string(),
            RuleSeverityConfig::Off,
        );
        assert!(!config.is_rule_enabled("naming/class-name-pascal-case"));
        assert!(config.is_rule_enabled("naming/function-name-snake-case"));
    }
}
