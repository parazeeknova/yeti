use crate::error::{Result, YetiError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const CEREBRAS_API_KEY_ENV: &str = "CEREBRAS_API_KEY";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    pub api_key: Option<String>,
    pub model: Option<String>,
}

impl Config {
    pub fn default_model() -> &'static str {
        "gpt-oss-120b"
    }

    pub fn model(&self) -> &str {
        self.model
            .as_deref()
            .unwrap_or_else(|| Self::default_model())
    }
}

fn config_dir() -> Result<PathBuf> {
    let base = dirs::config_dir()
        .ok_or_else(|| YetiError::IoError("Could not locate config directory".to_string()))?;
    Ok(base.join("yeti"))
}

fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

pub fn load() -> Result<Config> {
    let path = config_path()?;
    if path.exists() {
        let text = fs::read_to_string(&path)?;
        let config: Config = toml::from_str(&text)?;
        return Ok(config);
    }
    Ok(Config::default())
}

pub fn save(config: &Config) -> Result<()> {
    let dir = config_dir()?;
    fs::create_dir_all(&dir)?;
    let path = config_path()?;
    let text = toml::to_string_pretty(config)
        .map_err(|e| YetiError::IoError(format!("Failed to serialize config: {}", e)))?;
    fs::write(&path, text)?;
    Ok(())
}

pub fn get_effective_api_key(config: &Config) -> Option<String> {
    if let Ok(env_key) = std::env::var(CEREBRAS_API_KEY_ENV)
        && !env_key.is_empty()
    {
        return Some(env_key);
    }
    config.api_key.clone()
}

pub fn save_api_key(key: &str) -> Result<()> {
    let mut config = load().unwrap_or_default();
    config.api_key = Some(key.to_string());
    save(&config)
}
