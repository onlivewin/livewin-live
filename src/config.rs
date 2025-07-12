use config::{Config, Environment, File};
use serde::Deserialize;
use std::path::PathBuf;
use crate::errors::{Result, StreamingError};

pub struct ConfigManager {
    settings: Settings,
}

impl ConfigManager {
    pub fn new() -> Result<Self> {
        let settings = Self::load_config()?;
        Ok(Self { settings })
    }

    fn find_config_file() -> Result<PathBuf> {
        let possible_paths = [
            std::env::var("XLIVE_CONFIG").ok().map(PathBuf::from),
            Some(PathBuf::from("conf.yaml")),
            Some(PathBuf::from("config/conf.yaml")),
            Some(PathBuf::from("/etc/xlive/conf.yaml")),
            Some(PathBuf::from("config/default.yaml")),
        ];

        for path in possible_paths.iter().flatten() {
            if path.exists() {
                log::info!("Using config file: {}", path.display());
                return Ok(path.clone());
            }
        }

        Err(StreamingError::ConfigError {
            message: "No configuration file found. Tried: conf.yaml, config/conf.yaml, /etc/xlive/conf.yaml, config/default.yaml".to_string(),
        })
    }

    fn load_config() -> Result<Settings> {
        let mut config = Config::builder();

        // 尝试加载配置文件
        if let Ok(config_path) = Self::find_config_file() {
            config = config.add_source(File::from(config_path.as_ref()));
        } else {
            log::warn!("No config file found, using defaults and environment variables only");
        }

        // 添加环境变量支持
        config = config.add_source(Environment::with_prefix("XLIVE").separator("_"));

        // 设置默认值
        config = config
            .set_default("rtmp.port", 1935)?
            .set_default("hls.enable", true)?
            .set_default("hls.port", 3001)?
            .set_default("hls.ts_duration", 5)?
            .set_default("hls.data_path", "data")?
            .set_default("http_flv.enable", true)?
            .set_default("http_flv.port", 3002)?
            .set_default("flv.enable", false)?
            .set_default("flv.data_path", "data/flv")?
            .set_default("redis", "redis://localhost:6379")?
            .set_default("auth_enable", false)?
            .set_default("log_level", "info")?
            .set_default("full_gop", true)?;

        let config = config.build().map_err(|e| StreamingError::ConfigError {
            message: format!("Failed to build config: {}", e),
        })?;

        config.try_deserialize().map_err(|e| StreamingError::ConfigError {
            message: format!("Failed to deserialize config: {}", e),
        })
    }

    pub fn get_settings(&self) -> &Settings {
        &self.settings
    }

    pub fn reload(&mut self) -> Result<()> {
        log::info!("Reloading configuration...");
        self.settings = Self::load_config()?;
        log::info!("Configuration reloaded successfully");
        Ok(())
    }
}

// 保持向后兼容的全局函数
pub fn get_setting() -> Settings {
    match ConfigManager::new() {
        Ok(manager) => manager.settings.clone(),
        Err(e) => {
            log::error!("Failed to load config: {}", e);
            // 返回默认配置
            Settings::default()
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub rtmp: Rtmp,
    pub hls: Hls,
    pub http_flv: HTTPFLV,
    pub redis: String,
    pub auth_enable: bool,
    pub log_level: String,
    pub full_gop: bool,
    pub flv: Flv,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            rtmp: Rtmp::default(),
            hls: Hls::default(),
            http_flv: HTTPFLV::default(),
            redis: "redis://localhost:6379".to_string(),
            auth_enable: false,
            log_level: "info".to_string(),
            full_gop: true,
            flv: Flv::default(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Rtmp {
    pub port: i32,
}

impl Default for Rtmp {
    fn default() -> Self {
        Self { port: 1935 }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Flv {
    pub enable: bool,
    pub data_path: String,
}

impl Default for Flv {
    fn default() -> Self {
        Self {
            enable: false,
            data_path: "data/flv".to_string(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Hls {
    pub enable: bool,
    pub port: i32,
    pub ts_duration: u64,
    pub data_path: String,
}

impl Default for Hls {
    fn default() -> Self {
        Self {
            enable: true,
            port: 3001,
            ts_duration: 5,
            data_path: "data".to_string(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct HTTPFLV {
    pub enable: bool,
    pub port: i32,
}

impl Default for HTTPFLV {
    fn default() -> Self {
        Self {
            enable: true,
            port: 3002,
        }
    }
}
