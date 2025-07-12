use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use crate::errors::{Result, StreamingError};

/// 速率限制配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    pub max_requests: u32,
    pub window_duration: Duration,
    pub burst_allowance: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 100,
            window_duration: Duration::from_secs(60),
            burst_allowance: 10,
        }
    }
}

/// 速率限制窗口数据
#[derive(Debug, Clone)]
struct RateLimitWindow {
    count: u32,
    window_start: Instant,
    burst_count: u32,
    last_request: Instant,
}

impl RateLimitWindow {
    fn new() -> Self {
        let now = Instant::now();
        Self {
            count: 0,
            window_start: now,
            burst_count: 0,
            last_request: now,
        }
    }

    fn reset_window(&mut self, now: Instant) {
        self.count = 0;
        self.window_start = now;
        self.burst_count = 0;
    }

    fn is_window_expired(&self, now: Instant, window_duration: Duration) -> bool {
        now.duration_since(self.window_start) >= window_duration
    }

    fn check_burst(&mut self, now: Instant, burst_allowance: u32) -> bool {
        let time_since_last = now.duration_since(self.last_request);
        
        // 如果距离上次请求超过1秒，重置突发计数
        if time_since_last >= Duration::from_secs(1) {
            self.burst_count = 0;
        }
        
        self.last_request = now;
        
        if self.burst_count >= burst_allowance {
            return false;
        }
        
        self.burst_count += 1;
        true
    }
}

/// 速率限制器
pub struct RateLimiter {
    limits: HashMap<String, RateLimitConfig>,
    windows: Arc<RwLock<HashMap<String, RateLimitWindow>>>,
    cleanup_interval: Duration,
    cleanup_task: Option<tokio::task::JoinHandle<()>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        let windows = Arc::new(RwLock::new(HashMap::new()));
        let cleanup_interval = Duration::from_secs(300); // 5分钟清理一次
        
        let cleanup_task = Self::start_cleanup_task(windows.clone(), cleanup_interval);
        
        Self {
            limits: HashMap::new(),
            windows,
            cleanup_interval,
            cleanup_task: Some(cleanup_task),
        }
    }

    pub fn add_limit(mut self, name: String, config: RateLimitConfig) -> Self {
        self.limits.insert(name, config);
        self
    }

    fn start_cleanup_task(
        windows: Arc<RwLock<HashMap<String, RateLimitWindow>>>,
        interval: Duration,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                Self::cleanup_expired_windows(&windows).await;
            }
        })
    }

    async fn cleanup_expired_windows(windows: &Arc<RwLock<HashMap<String, RateLimitWindow>>>) {
        let mut windows = windows.write().await;
        let now = Instant::now();
        let cleanup_threshold = Duration::from_secs(3600); // 1小时未使用的窗口
        
        let initial_count = windows.len();
        windows.retain(|_, window| {
            now.duration_since(window.last_request) < cleanup_threshold
        });
        
        let cleaned_count = initial_count - windows.len();
        if cleaned_count > 0 {
            log::debug!("Cleaned up {} expired rate limit windows", cleaned_count);
        }
    }

    /// 检查是否允许请求
    pub async fn check_limit(&self, identifier: &str, limit_type: &str) -> Result<bool> {
        let config = self.limits.get(limit_type)
            .ok_or_else(|| StreamingError::ConfigError {
                message: format!("Unknown rate limit type: {}", limit_type),
            })?;

        let key = format!("{}:{}", limit_type, identifier);
        let now = Instant::now();

        let mut windows = self.windows.write().await;
        let window = windows.entry(key).or_insert_with(RateLimitWindow::new);

        // 检查窗口是否过期，如果过期则重置
        if window.is_window_expired(now, config.window_duration) {
            window.reset_window(now);
        }

        // 检查突发限制
        if !window.check_burst(now, config.burst_allowance) {
            log::warn!("Rate limit exceeded (burst) for {}: {}", limit_type, identifier);
            return Ok(false);
        }

        // 检查窗口限制
        if window.count >= config.max_requests {
            log::warn!("Rate limit exceeded (window) for {}: {} ({}/{})", 
                limit_type, identifier, window.count, config.max_requests);
            return Ok(false);
        }

        window.count += 1;
        Ok(true)
    }

    /// 获取当前限制状态
    pub async fn get_limit_status(&self, identifier: &str, limit_type: &str) -> Option<RateLimitStatus> {
        let config = self.limits.get(limit_type)?;
        let key = format!("{}:{}", limit_type, identifier);
        
        let windows = self.windows.read().await;
        let window = windows.get(&key)?;
        
        let now = Instant::now();
        let time_until_reset = if window.is_window_expired(now, config.window_duration) {
            Duration::ZERO
        } else {
            config.window_duration - now.duration_since(window.window_start)
        };

        Some(RateLimitStatus {
            limit: config.max_requests,
            remaining: config.max_requests.saturating_sub(window.count),
            reset_time: time_until_reset,
            burst_remaining: config.burst_allowance.saturating_sub(window.burst_count),
        })
    }

    /// 重置特定标识符的限制
    pub async fn reset_limit(&self, identifier: &str, limit_type: &str) {
        let key = format!("{}:{}", limit_type, identifier);
        let mut windows = self.windows.write().await;
        windows.remove(&key);
        log::info!("Reset rate limit for {}: {}", limit_type, identifier);
    }

    /// 获取所有活跃的限制状态
    pub async fn get_all_limits(&self) -> HashMap<String, RateLimitStatus> {
        let mut result = HashMap::new();
        let windows = self.windows.read().await;
        let now = Instant::now();

        for (key, window) in windows.iter() {
            if let Some((limit_type, identifier)) = key.split_once(':') {
                if let Some(config) = self.limits.get(limit_type) {
                    let time_until_reset = if window.is_window_expired(now, config.window_duration) {
                        Duration::ZERO
                    } else {
                        config.window_duration - now.duration_since(window.window_start)
                    };

                    let status = RateLimitStatus {
                        limit: config.max_requests,
                        remaining: config.max_requests.saturating_sub(window.count),
                        reset_time: time_until_reset,
                        burst_remaining: config.burst_allowance.saturating_sub(window.burst_count),
                    };

                    result.insert(key.clone(), status);
                }
            }
        }

        result
    }
}

impl Drop for RateLimiter {
    fn drop(&mut self) {
        if let Some(task) = self.cleanup_task.take() {
            task.abort();
            log::debug!("Rate limiter cleanup task stopped");
        }
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
            .add_limit("connection".to_string(), RateLimitConfig {
                max_requests: 10,
                window_duration: Duration::from_secs(60),
                burst_allowance: 5,
            })
            .add_limit("hls_request".to_string(), RateLimitConfig {
                max_requests: 100,
                window_duration: Duration::from_secs(60),
                burst_allowance: 20,
            })
            .add_limit("stream_creation".to_string(), RateLimitConfig {
                max_requests: 5,
                window_duration: Duration::from_secs(300), // 5分钟
                burst_allowance: 2,
            })
    }
}

#[derive(Debug, Serialize)]
pub struct RateLimitStatus {
    pub limit: u32,
    pub remaining: u32,
    pub reset_time: Duration,
    pub burst_remaining: u32,
}

// 全局速率限制器
use std::sync::OnceLock;
static GLOBAL_RATE_LIMITER: OnceLock<Arc<RateLimiter>> = OnceLock::new();

pub fn get_global_rate_limiter() -> Arc<RateLimiter> {
    GLOBAL_RATE_LIMITER.get_or_init(|| {
        Arc::new(RateLimiter::default())
    }).clone()
}

/// 便利宏用于检查速率限制
#[macro_export]
macro_rules! check_rate_limit {
    ($identifier:expr, $limit_type:expr) => {
        $crate::rate_limiter::get_global_rate_limiter()
            .check_limit($identifier, $limit_type)
            .await
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_rate_limit_basic() {
        let limiter = RateLimiter::new()
            .add_limit("test".to_string(), RateLimitConfig {
                max_requests: 3,
                window_duration: Duration::from_secs(1),
                burst_allowance: 5, // 增加突发允许量，确保不会被突发限制阻止
            });

        // 前3个请求应该通过
        assert!(limiter.check_limit("user1", "test").await.unwrap());
        assert!(limiter.check_limit("user1", "test").await.unwrap());
        assert!(limiter.check_limit("user1", "test").await.unwrap());

        // 第4个请求应该被拒绝（超过窗口限制）
        assert!(!limiter.check_limit("user1", "test").await.unwrap());
    }

    #[tokio::test]
    async fn test_rate_limit_window_reset() {
        let limiter = RateLimiter::new()
            .add_limit("test".to_string(), RateLimitConfig {
                max_requests: 2,
                window_duration: Duration::from_millis(100),
                burst_allowance: 5,
            });

        // 用完限额
        assert!(limiter.check_limit("user1", "test").await.unwrap());
        assert!(limiter.check_limit("user1", "test").await.unwrap());
        assert!(!limiter.check_limit("user1", "test").await.unwrap());

        // 等待窗口重置
        sleep(Duration::from_millis(150)).await;

        // 现在应该可以再次请求
        assert!(limiter.check_limit("user1", "test").await.unwrap());
    }

    #[tokio::test]
    async fn test_rate_limit_different_users() {
        let limiter = RateLimiter::new()
            .add_limit("test".to_string(), RateLimitConfig {
                max_requests: 1,
                window_duration: Duration::from_secs(1),
                burst_allowance: 1,
            });

        // 不同用户应该有独立的限制
        assert!(limiter.check_limit("user1", "test").await.unwrap());
        assert!(limiter.check_limit("user2", "test").await.unwrap());

        // 但同一用户的第二个请求应该被拒绝
        assert!(!limiter.check_limit("user1", "test").await.unwrap());
    }
}
