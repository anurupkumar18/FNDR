//! Configuration management

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Auto-fill configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AutofillConfig {
    /// Whether global screen auto-fill is enabled.
    #[serde(default = "default_autofill_enabled")]
    pub enabled: bool,
    /// Global shortcut in tauri/global-hotkey format, e.g. `Alt+F`.
    #[serde(default = "default_autofill_shortcut")]
    pub shortcut: String,
    /// How far back semantic retrieval should search.
    #[serde(default = "default_autofill_lookback_days")]
    pub lookback_days: u32,
    /// Confidence threshold above which FNDR injects without confirmation.
    #[serde(default = "default_autofill_auto_inject_threshold")]
    pub auto_inject_threshold: f32,
    /// Whether FNDR should prefer system-style typing when the target app remains frontmost.
    #[serde(default = "default_autofill_prefer_typed_injection")]
    pub prefer_typed_injection: bool,
    /// Maximum number of candidates to return for quick-pick conflict resolution.
    #[serde(default = "default_autofill_max_candidates")]
    pub max_candidates: usize,
}

impl Default for AutofillConfig {
    fn default() -> Self {
        Self {
            enabled: default_autofill_enabled(),
            shortcut: default_autofill_shortcut(),
            lookback_days: default_autofill_lookback_days(),
            auto_inject_threshold: default_autofill_auto_inject_threshold(),
            prefer_typed_injection: default_autofill_prefer_typed_injection(),
            max_candidates: default_autofill_max_candidates(),
        }
    }
}

impl AutofillConfig {
    pub fn normalized(mut self) -> Self {
        self.shortcut = self.shortcut.trim().to_string();
        if self.shortcut.is_empty() {
            self.shortcut = default_autofill_shortcut();
        }
        self.lookback_days = self.lookback_days.clamp(7, 365);
        self.auto_inject_threshold = self.auto_inject_threshold.clamp(0.55, 0.995);
        self.max_candidates = self.max_candidates.clamp(1, 6);
        self
    }
}

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Base capture FPS (0.5 - 1.0 recommended)
    pub fps_base: f64,
    /// Seconds of idle before reducing FPS
    pub idle_pause_seconds: u64,
    /// FPS when idle
    pub idle_fps: f64,
    /// Perceptual hash threshold for deduplication (0-64, lower = stricter)
    pub dedupe_threshold: u32,
    /// Force capture every N seconds even if duplicate
    pub forced_capture_interval: u64,
    /// Days to retain records
    pub retention_days: u32,
    /// Blocked application names
    pub blocklist: Vec<String>,
    /// Enable pattern redaction (emails, credit cards)
    pub redact_mode: bool,
    /// Minimum text length to store
    pub min_text_length: usize,
    /// Enable VLM for intelligent image understanding
    #[serde(default = "default_use_vlm")]
    pub use_vlm: bool,
    /// VLM model size: "4B" (primary)
    #[serde(default = "default_vlm_model_size")]
    pub vlm_model_size: String,
    /// Days to retain screenshot files on disk (records kept; only pixel data deleted). 0 = keep forever.
    #[serde(default = "default_screenshot_retention_days")]
    pub screenshot_retention_days: u32,
    /// Enable proactive surface: nudges when current screen is semantically close to old unresolved context.
    #[serde(default = "default_proactive_surface_enabled")]
    pub proactive_surface_enabled: bool,
    /// Half-life for Ebbinghaus memory decay in days. Records decay toward 0.15 floor over time.
    #[serde(default = "default_decay_half_life_days")]
    pub decay_half_life_days: u32,
    /// Intelligent Screen Auto-Fill configuration.
    #[serde(default)]
    pub autofill: AutofillConfig,
}

fn default_use_vlm() -> bool {
    true
}

fn default_vlm_model_size() -> String {
    "4B".to_string()
}

fn default_screenshot_retention_days() -> u32 {
    30
}

fn default_proactive_surface_enabled() -> bool {
    true
}

fn default_decay_half_life_days() -> u32 {
    21
}

fn default_autofill_enabled() -> bool {
    true
}

fn default_autofill_shortcut() -> String {
    "Alt+F".to_string()
}

fn default_autofill_lookback_days() -> u32 {
    90
}

fn default_autofill_auto_inject_threshold() -> f32 {
    0.90
}

fn default_autofill_prefer_typed_injection() -> bool {
    true
}

fn default_autofill_max_candidates() -> usize {
    4
}

impl Default for Config {
    fn default() -> Self {
        Self {
            fps_base: 0.5,
            idle_pause_seconds: 5,
            idle_fps: 0.2,
            dedupe_threshold: 5,
            forced_capture_interval: 60,
            retention_days: 7,
            blocklist: vec![
                "1Password".to_string(),
                "Keychain Access".to_string(),
                "System Preferences".to_string(),
                "System Settings".to_string(),
            ],
            redact_mode: false,
            min_text_length: 20,
            use_vlm: true,
            vlm_model_size: "4B".to_string(),
            screenshot_retention_days: 30,
            proactive_surface_enabled: true,
            decay_half_life_days: 21,
            autofill: AutofillConfig::default(),
        }
    }
}

impl Config {
    pub fn normalized(mut self) -> Self {
        self.autofill = self.autofill.normalized();
        self
    }

    /// Load configuration from file or create default
    pub fn load_or_create() -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = Self::config_path()?;

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let config: Config = toml::from_str(&content)?;
            Ok(config.normalized())
        } else {
            let config = Config::default().normalized();
            config.save()?;
            Ok(config)
        }
    }

    /// Save configuration to file
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = Self::config_path()?;
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&config_path, content)?;
        Ok(())
    }

    fn config_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let dirs = directories::ProjectDirs::from("com", "fndr", "FNDR")
            .ok_or("Could not determine config directory")?;
        Ok(dirs.config_dir().join("config.toml"))
    }
}
