//! Configuration management

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
        }
    }
}

impl Config {
    /// Load configuration from file or create default
    pub fn load_or_create() -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = Self::config_path()?;

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            let config = Config::default();
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
