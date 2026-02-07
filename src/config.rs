//! Configuration management for RustCast
//! Handles saving/loading settings like port number

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// HTTP server port
    pub port: u16,
    /// Audio bitrate for MP3 encoding (kbps)
    pub bitrate: u32,
    /// Auto-start streaming on launch
    pub auto_start: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            port: 3000,
            bitrate: 192,
            auto_start: true,
        }
    }
}

impl Config {
    /// Get the config file path
    fn config_path() -> Option<PathBuf> {
        ProjectDirs::from("com", "rustcast", "RustCast").map(|dirs| {
            let config_dir = dirs.config_dir();
            config_dir.join("config.json")
        })
    }

    /// Load configuration from file, or create default if not exists
    pub fn load() -> Self {
        if let Some(path) = Self::config_path() {
            if path.exists() {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(config) = serde_json::from_str(&content) {
                        log::info!("Loaded config from {:?}", path);
                        return config;
                    }
                }
            }
        }
        log::info!("Using default configuration");
        Self::default()
    }

    /// Save configuration to file
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(path) = Self::config_path() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let content = serde_json::to_string_pretty(self)?;
            fs::write(&path, content)?;
            log::info!("Saved config to {:?}", path);
        }
        Ok(())
    }
}
