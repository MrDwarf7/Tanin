use anyhow::Result;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,
    pub audio: AudioConfig,
    pub sounds: HashMap<String, SoundConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub enable_bundled_sounds: bool,
    #[serde(default)]
    pub category_order: Vec<String>,
    #[serde(default)]
    pub hidden_categories: Vec<String>,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            enable_bundled_sounds: true,
            category_order: Vec::new(),
            hidden_categories: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub buffer_size: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoundConfig {
    #[serde(default)]
    pub hidden: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            audio: AudioConfig {
                sample_rate: 44100,
                buffer_size: 100,
            },
            sounds: HashMap::new(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        // if let Some(config) = CONFIG.get() {
        //     return Ok(config.to_owned());
        // }

        let path = get_config_path();
        if !path.exists() {
            // CONFIG.set(Self::default()).ok();
            return Ok(Self::default().save()?.to_owned());
        }

        let config: Config = toml::from_str(
            &std::fs::read_to_string(&path)
                .map_err(|e| anyhow::anyhow!("Failed to read config file: {}", e))?,
        )
        .map_err(|e| anyhow::anyhow!("Failed to parse config file: {}", e))?;

        // CONFIG.set(config.clone()).ok();
        Ok(config.save()?.to_owned())
    }

    pub fn save(&self) -> Result<&'_ Self> {
        let path = get_config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(self)
    }
}

fn get_config_path() -> PathBuf {
    if let Some(proj_dirs) = ProjectDirs::from("com", "tanin", "tanin") {
        proj_dirs.config_dir().join("config.toml")
    } else {
        PathBuf::from("config.toml")
    }
}
