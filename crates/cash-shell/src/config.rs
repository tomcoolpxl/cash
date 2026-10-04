//! Configuration file support for cash.
//!
//! This module provides TOML-based configuration file loading with the following features:
//! - Forward-compatible: unknown fields are ignored
//! - Graceful degradation: parse errors are logged but don't prevent shell startup
//! - Layered configuration: defaults < config file < command-line arguments

use cash_interactive::UIOptions;
use etcetera::BaseStrategy;
use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};

use crate::args::CommandLineArgs;

/// Root configuration structure for cash.
///
/// All fields are optional to support forward compatibility and partial configuration.
/// Unknown fields in the TOML file are silently ignored.
#[derive(Debug, Default, Clone, serde::Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(default)]
pub struct Config {
    /// User interface configuration options.
    pub ui: UiConfig,

    /// Experimental features configuration.
    pub experimental: ExperimentalConfig,
}

/// User interface configuration options.
#[derive(Debug, Default, Clone, serde::Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(default)]
pub struct UiConfig {
    /// Enable syntax highlighting in the input line.
    #[serde(rename = "syntax-highlighting")]
    pub syntax_highlighting: Option<bool>,
}

/// Experimental features configuration.
///
/// These options control unstable features that may change or be removed in future versions.
#[derive(Debug, Default, Clone, serde::Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(default)]
pub struct ExperimentalConfig {
    /// Enable zsh-style preexec/precmd hooks.
    #[serde(rename = "zsh-hooks")]
    pub zsh_hooks: Option<bool>,

    /// Enable terminal shell integration.
    #[serde(rename = "terminal-shell-integration")]
    pub terminal_shell_integration: Option<bool>,
}

impl Config {
    /// Converts the configuration to [`UIOptions`], merging with CLI arguments.
    ///
    /// Settings are applied with the following priority (highest to lowest):
    /// 1. CLI arguments, when given
    /// 2. Config file values
    /// 3. Default values
    ///
    /// # Arguments
    ///
    /// * `args` - The parsed command-line arguments
    #[must_use]
    pub fn to_ui_options(&self, args: &CommandLineArgs) -> UIOptions {
        let enable_highlighting = args
            .enable_highlighting
            .or(self.ui.syntax_highlighting)
            .unwrap_or(crate::entry::DEFAULT_ENABLE_HIGHLIGHTING);
        // cash (D39): on by default.
        let terminal_shell_integration = args
            .terminal_shell_integration
            .or(self.experimental.terminal_shell_integration)
            .unwrap_or(true);
        // `--enable-zsh-hooks` has no value to turn the hooks off with: given, they are on.
        let zsh_style_hooks = args.zsh_style_hooks || self.experimental.zsh_hooks == Some(true);

        UIOptions::builder()
            .disable_bracketed_paste(args.disable_bracketed_paste)
            .disable_color(args.disable_color)
            .disable_highlighting(!enable_highlighting)
            .terminal_shell_integration(terminal_shell_integration)
            .zsh_style_hooks(zsh_style_hooks)
            .build()
    }
}

/// Result of attempting to load a configuration file.
#[derive(Debug, Default)]
pub struct ConfigLoadResult {
    /// The loaded configuration, or default if loading failed.
    pub config: Config,

    /// The path that was used (or attempted) for loading.
    pub path: Option<PathBuf>,

    /// Any error that occurred during loading.
    pub error: Option<ConfigLoadError>,

    /// Whether the path was explicitly provided by the user (via `--config`).
    pub explicit_path: bool,
}

impl ConfigLoadResult {
    /// Consumes the result and returns the configuration.
    ///
    /// If an error occurred:
    /// - For explicit paths (user-provided via `--config`): returns `Err` with a formatted error
    /// - For default paths: logs a warning and returns the default configuration
    ///
    /// # Errors
    ///
    /// Returns an error if an explicit config path was provided and loading failed.
    pub fn into_config_or_log(self) -> Result<Config, String> {
        let Some(err) = self.error else {
            return Ok(self.config);
        };

        let path_display = self.path.as_ref().map_or(Cow::Borrowed("<unknown>"), |p| {
            Cow::Owned(p.display().to_string())
        });

        if self.explicit_path {
            // User explicitly provided --config; treat errors as fatal.
            return Err(format!("failed to load config from {path_display}: {err}"));
        }

        // Default config path; log warning but continue with defaults.
        tracing::warn!("failed to load config from {path_display}: {err}");
        Ok(self.config)
    }
}

/// Errors that can occur when loading configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigLoadError {
    /// Failed to read the configuration file.
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),

    /// Failed to parse the TOML content.
    #[error("failed to parse config file: {0}")]
    Parse(#[from] toml::de::Error),
}

const CONFIG_SUBDIR_NAME: &str = "cash";
const CONFIG_FILE_NAME: &str = "config.toml";

/// Returns the default configuration file path for the current platform.
///
/// Uses the XDG Base Directory specification on Linux/macOS and appropriate
/// platform conventions on other systems via the `etcetera` crate.
///
/// Returns `None` if the platform's config directory cannot be determined.
pub fn default_config_path() -> Option<PathBuf> {
    let strategy = etcetera::choose_base_strategy().ok()?;
    Some(
        strategy
            .config_dir()
            .join(CONFIG_SUBDIR_NAME)
            .join(CONFIG_FILE_NAME),
    )
}

/// Loads configuration from the specified path.
///
/// Returns a `ConfigLoadResult` containing:
/// - The parsed configuration (or default on error)
/// - The path that was used
/// - Any error that occurred
///
/// Note: This function sets `explicit_path` to `false`. Use `load_config` for
/// proper handling of explicit vs. default paths.
pub fn load_from_path(path: &Path) -> ConfigLoadResult {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(e) => {
            return ConfigLoadResult {
                path: Some(path.to_path_buf()),
                error: Some(ConfigLoadError::Io(e)),
                ..Default::default()
            };
        }
    };

    match toml::from_str(&content) {
        Ok(config) => ConfigLoadResult {
            config,
            path: Some(path.to_path_buf()),
            ..Default::default()
        },
        Err(e) => ConfigLoadResult {
            path: Some(path.to_path_buf()),
            error: Some(ConfigLoadError::Parse(e)),
            ..Default::default()
        },
    }
}

/// Loads configuration based on the provided options.
///
/// # Arguments
///
/// * `disabled` - If true, skip loading and return defaults
/// * `explicit_path` - If provided, use this path instead of the default
///
/// # Returns
///
/// A `ConfigLoadResult` containing the configuration and any errors encountered.
/// If `explicit_path` is provided and loading fails, the result will have
/// `explicit_path: true` to indicate that the error should be treated as fatal.
pub fn load_config(disabled: bool, explicit_path: Option<&Path>) -> ConfigLoadResult {
    if disabled {
        return ConfigLoadResult::default();
    }

    let is_explicit = explicit_path.is_some();

    let path = match explicit_path {
        Some(p) => p.to_path_buf(),
        None => match default_config_path() {
            Some(p) => p,
            None => {
                // Can't determine config path; use defaults silently
                return ConfigLoadResult::default();
            }
        },
    };

    // If using default path and file doesn't exist, silently use defaults
    if !is_explicit && !path.exists() {
        return ConfigLoadResult {
            path: Some(path),
            ..Default::default()
        };
    }

    let mut result = load_from_path(&path);
    result.explicit_path = is_explicit;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_config() {
        let config: Config = toml::from_str("").unwrap();
        assert!(config.ui.syntax_highlighting.is_none());
        assert!(config.experimental.zsh_hooks.is_none());
        assert!(config.experimental.terminal_shell_integration.is_none());
    }

    #[test]
    fn full_config() {
        let toml = r"
            [ui]
            syntax-highlighting = true

            [experimental]
            zsh-hooks = true
            terminal-shell-integration = false
        ";

        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.ui.syntax_highlighting, Some(true));
        assert_eq!(config.experimental.zsh_hooks, Some(true));
        assert_eq!(config.experimental.terminal_shell_integration, Some(false));
    }

    #[test]
    fn partial_config() {
        let toml = r"
            [ui]
            syntax-highlighting = false
        ";

        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.ui.syntax_highlighting, Some(false));
        assert!(config.experimental.zsh_hooks.is_none());
    }

    #[test]
    fn unknown_fields_ignored() {
        let toml = r#"
            [ui]
            syntax-highlighting = true
            unknown-field = "should be ignored"
            another-unknown = 42

            [experimental]
            zsh-hooks = false
            future-feature = true

            [unknown-section]
            foo = "bar"
        "#;

        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.ui.syntax_highlighting, Some(true));
        assert_eq!(config.experimental.zsh_hooks, Some(false));
    }

    #[test]
    fn load_config_disabled() {
        let result = load_config(true, None);
        assert!(result.path.is_none());
        assert!(result.error.is_none());
    }

    #[test]
    fn load_config_nonexistent_default() {
        // When using default path and file doesn't exist, should return defaults without error
        let result = load_config(false, None);
        // We may or may not get a path depending on platform, but shouldn't error
        assert!(result.error.is_none());
    }

    #[test]
    fn load_config_nonexistent_explicit() {
        let path = Path::new("/nonexistent/path/to/config.toml");
        let result = load_config(false, Some(path));
        assert!(result.error.is_some());
        assert!(matches!(result.error, Some(ConfigLoadError::Io(_))));
    }

    #[test]
    fn to_ui_options_defaults_only() {
        let config = Config::default();
        let args = CommandLineArgs::default_values();
        let ui = config.to_ui_options(&args);

        assert!(!ui.disable_bracketed_paste);
        assert!(!ui.disable_color);
        // cash: syntax highlighting is on by default, where upstream had it behind the
        // `experimental` feature.
        assert!(!ui.disable_highlighting);
        // cash (D39): terminal shell integration is on by default, where upstream had it
        // behind an off-by-default experimental flag.
        assert!(ui.terminal_shell_integration);
        assert!(!ui.zsh_style_hooks);
    }

    #[test]
    fn to_ui_options_config_overrides_defaults() {
        let toml = r"
            [ui]
            syntax-highlighting = true

            [experimental]
            zsh-hooks = true
            terminal-shell-integration = true
        ";
        let config: Config = toml::from_str(toml).unwrap();
        let args = CommandLineArgs::default_values();

        // CLI values match defaults, so config should take effect
        let ui = config.to_ui_options(&args);

        assert!(!ui.disable_highlighting); // config enabled highlighting
        assert!(ui.terminal_shell_integration);
        assert!(ui.zsh_style_hooks);
    }

    #[test]
    fn to_ui_options_cli_overrides_config() {
        let toml = r"
            [ui]
            syntax-highlighting = true

            [experimental]
            zsh-hooks = false
        ";
        let config: Config = toml::from_str(toml).unwrap();

        let args = CommandLineArgs::try_parse_from(
            ["cash", "--enable-highlighting=false", "--enable-zsh-hooks"].map(String::from),
        )
        .unwrap();

        let ui = config.to_ui_options(&args);

        assert!(ui.disable_highlighting); // CLI disabled highlighting
        assert!(ui.zsh_style_hooks); // CLI enabled
    }

    /// A flag given as the default still beats the config: `--enable-highlighting` lost
    /// to `syntax-highlighting = false`, being indistinguishable from no flag (BIN-13).
    #[test]
    fn a_flag_that_says_the_default_overrides_the_config() {
        let toml = r"
            [ui]
            syntax-highlighting = false

            [experimental]
            terminal-shell-integration = false
        ";
        let config: Config = toml::from_str(toml).unwrap();
        let ui = config.to_ui_options(&CommandLineArgs::default_values());
        assert!(ui.disable_highlighting);
        assert!(!ui.terminal_shell_integration);

        for flags in [
            [
                "cash",
                "--enable-highlighting",
                "--enable-terminal-integration",
            ],
            [
                "cash",
                "--enable-highlighting=true",
                "--enable-terminal-integration=true",
            ],
        ] {
            let args = CommandLineArgs::try_parse_from(flags.map(String::from)).unwrap();
            let ui = config.to_ui_options(&args);
            assert!(!ui.disable_highlighting, "{flags:?}");
            assert!(ui.terminal_shell_integration, "{flags:?}");
        }
    }

    #[test]
    fn highlighting_can_be_turned_off_from_the_command_line_or_the_config() {
        let args = CommandLineArgs::try_parse_from(
            ["cash", "--enable-highlighting=false"].map(String::from),
        )
        .unwrap();
        assert!(Config::default().to_ui_options(&args).disable_highlighting);

        let config: Config = toml::from_str("[ui]\nsyntax-highlighting = false\n").unwrap();
        let args = CommandLineArgs::default_values();
        assert!(config.to_ui_options(&args).disable_highlighting);
    }

    #[test]
    fn to_ui_options_cli_only_settings() {
        let config = Config::default();
        let args = CommandLineArgs::try_parse_from(
            ["cash", "--disable-bracketed-paste", "--disable-color"].map(String::from),
        )
        .unwrap();

        let ui = config.to_ui_options(&args);

        assert!(ui.disable_bracketed_paste);
        assert!(ui.disable_color);
    }
}
