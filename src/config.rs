use config::{Config, Environment, File};
use serde::Deserialize;
use std::path::PathBuf;
use std::time::Duration;
use std::collections::HashMap;
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
            // HLS清理配置默认值
            .set_default("hls.cleanup.max_files_per_stream", 10)?
            .set_default("hls.cleanup.min_file_age_seconds", 30)?
            .set_default("hls.cleanup.cleanup_delay_seconds", 5)?
            .set_default("hls.cleanup.enable_size_based_cleanup", true)?
            .set_default("hls.cleanup.max_total_size_mb", 1000)?
            .set_default("http_flv.enable", true)?
            .set_default("http_flv.port", 3002)?
            .set_default("flv.enable", false)?
            .set_default("flv.data_path", "data/flv")?
            .set_default("redis", "redis://localhost:6379")?
            .set_default("auth_enable", false)?
            .set_default("log_level", "info")?
            .set_default("full_gop", true)?
            // 速率限制配置默认值
            .set_default("rate_limit.connection.max_requests", 10)?
            .set_default("rate_limit.connection.window_duration_secs", 60)?
            .set_default("rate_limit.connection.burst_allowance", 5)?
            .set_default("rate_limit.hls_request.max_requests", 100)?
            .set_default("rate_limit.hls_request.window_duration_secs", 60)?
            .set_default("rate_limit.hls_request.burst_allowance", 20)?
            .set_default("rate_limit.stream_creation.max_requests", 5)?
            .set_default("rate_limit.stream_creation.window_duration_secs", 300)?
            .set_default("rate_limit.stream_creation.burst_allowance", 2)?
            .set_default("rate_limit.cleanup_interval_secs", 300)?;

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
    pub rate_limit: RateLimitSettings,
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
            rate_limit: RateLimitSettings::default(),
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
    pub cleanup: HlsCleanupConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct HlsCleanupConfig {
    /// 每个流最多保留的TS文件数量
    pub max_files_per_stream: usize,
    /// 文件最小存在时间（秒）
    pub min_file_age_seconds: u64,
    /// 清理延迟时间（秒）
    pub cleanup_delay_seconds: u64,
    /// 是否启用基于大小的清理
    pub enable_size_based_cleanup: bool,
    /// 每个流最大总大小（MB）
    pub max_total_size_mb: u64,
}

impl Default for Hls {
    fn default() -> Self {
        Self {
            enable: true,
            port: 3001,
            ts_duration: 5,
            data_path: "data".to_string(),
            cleanup: HlsCleanupConfig::default(),
        }
    }
}

impl Default for HlsCleanupConfig {
    fn default() -> Self {
        Self {
            max_files_per_stream: 10,
            min_file_age_seconds: 30,
            cleanup_delay_seconds: 5,
            enable_size_based_cleanup: true,
            max_total_size_mb: 1000,
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

#[derive(Debug, Deserialize, Clone)]
pub struct RateLimitSettings {
    pub connection: RateLimitTypeConfig,
    pub hls_request: RateLimitTypeConfig,
    pub stream_creation: RateLimitTypeConfig,
    pub cleanup_interval_secs: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RateLimitTypeConfig {
    pub max_requests: u32,
    pub window_duration_secs: u64,
    pub burst_allowance: u32,
}

impl Default for RateLimitSettings {
    fn default() -> Self {
        Self {
            connection: RateLimitTypeConfig {
                max_requests: 10,
                window_duration_secs: 60,
                burst_allowance: 5,
            },
            hls_request: RateLimitTypeConfig {
                max_requests: 100,
                window_duration_secs: 60,
                burst_allowance: 20,
            },
            stream_creation: RateLimitTypeConfig {
                max_requests: 5,
                window_duration_secs: 300,
                burst_allowance: 2,
            },
            cleanup_interval_secs: 300,
        }
    }
}

impl RateLimitTypeConfig {
    /// 转换为RateLimitConfig
    pub fn to_rate_limit_config(&self) -> crate::rate_limiter::RateLimitConfig {
        crate::rate_limiter::RateLimitConfig {
            max_requests: self.max_requests,
            window_duration: Duration::from_secs(self.window_duration_secs),
            burst_allowance: self.burst_allowance,
        }
    }
}
