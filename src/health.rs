use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use serde::Serialize;
use tokio::sync::RwLock;
use async_trait::async_trait;
use crate::errors::Result;
use crate::metrics::get_global_metrics;

/// 健康检查状态
#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Degraded(String),
    Unhealthy(String),
}

impl HealthStatus {
    pub fn is_healthy(&self) -> bool {
        matches!(self, HealthStatus::Healthy)
    }

    pub fn is_degraded(&self) -> bool {
        matches!(self, HealthStatus::Degraded(_))
    }

    pub fn is_unhealthy(&self) -> bool {
        matches!(self, HealthStatus::Unhealthy(_))
    }
}

/// 健康检查接口
#[async_trait]
pub trait HealthCheck: Send + Sync {
    async fn check(&self) -> HealthStatus;
    fn name(&self) -> &'static str;
    fn timeout(&self) -> Duration {
        Duration::from_secs(5)
    }
}

/// 系统资源健康检查
pub struct SystemResourceCheck {
    max_memory_usage_percent: f64,
    max_cpu_usage_percent: f64,
}

impl SystemResourceCheck {
    pub fn new(max_memory_usage_percent: f64, max_cpu_usage_percent: f64) -> Self {
        Self {
            max_memory_usage_percent,
            max_cpu_usage_percent,
        }
    }
}

#[async_trait]
impl HealthCheck for SystemResourceCheck {
    async fn check(&self) -> HealthStatus {
        // 简化的系统资源检查
        // 在实际应用中，你可能需要使用系统API来获取真实的资源使用情况
        
        // 模拟内存使用检查
        let memory_usage = self.get_memory_usage_percent().await;
        if memory_usage > self.max_memory_usage_percent {
            return HealthStatus::Unhealthy(format!(
                "Memory usage too high: {:.1}% > {:.1}%", 
                memory_usage, self.max_memory_usage_percent
            ));
        }

        // 模拟CPU使用检查
        let cpu_usage = self.get_cpu_usage_percent().await;
        if cpu_usage > self.max_cpu_usage_percent {
            return HealthStatus::Degraded(format!(
                "CPU usage high: {:.1}% > {:.1}%", 
                cpu_usage, self.max_cpu_usage_percent
            ));
        }

        HealthStatus::Healthy
    }

    fn name(&self) -> &'static str {
        "system_resources"
    }
}

impl SystemResourceCheck {
    async fn get_memory_usage_percent(&self) -> f64 {
        // 简化实现 - 在实际应用中应该使用系统API
        // 这里返回一个模拟值
        30.0
    }

    async fn get_cpu_usage_percent(&self) -> f64 {
        // 简化实现 - 在实际应用中应该使用系统API
        // 这里返回一个模拟值
        25.0
    }
}

/// 连接健康检查
pub struct ConnectionHealthCheck {
    max_active_connections: u64,
    max_failed_connections_rate: f64,
}

impl ConnectionHealthCheck {
    pub fn new(max_active_connections: u64, max_failed_connections_rate: f64) -> Self {
        Self {
            max_active_connections,
            max_failed_connections_rate,
        }
    }
}

#[async_trait]
impl HealthCheck for ConnectionHealthCheck {
    async fn check(&self) -> HealthStatus {
        let metrics = get_global_metrics();
        let snapshot = metrics.get_snapshot().await;

        // 检查活跃连接数
        if snapshot.connections_active > self.max_active_connections {
            return HealthStatus::Degraded(format!(
                "Too many active connections: {} > {}", 
                snapshot.connections_active, self.max_active_connections
            ));
        }

        // 检查连接失败率
        let total_connections = snapshot.connections_total;
        if total_connections > 0 {
            let failure_rate = snapshot.connections_failed as f64 / total_connections as f64;
            if failure_rate > self.max_failed_connections_rate {
                return HealthStatus::Unhealthy(format!(
                    "Connection failure rate too high: {:.2}% > {:.2}%", 
                    failure_rate * 100.0, self.max_failed_connections_rate * 100.0
                ));
            }
        }

        HealthStatus::Healthy
    }

    fn name(&self) -> &'static str {
        "connections"
    }
}

/// HLS服务健康检查
pub struct HlsHealthCheck {
    max_error_rate: f64,
}

impl HlsHealthCheck {
    pub fn new(max_error_rate: f64) -> Self {
        Self { max_error_rate }
    }
}

#[async_trait]
impl HealthCheck for HlsHealthCheck {
    async fn check(&self) -> HealthStatus {
        let metrics = get_global_metrics();
        let snapshot = metrics.get_snapshot().await;

        // 检查HLS请求错误率
        let total_requests = snapshot.hls_requests_total;
        if total_requests > 0 {
            let error_rate = snapshot.errors_total as f64 / total_requests as f64;
            if error_rate > self.max_error_rate {
                return HealthStatus::Degraded(format!(
                    "HLS error rate too high: {:.2}% > {:.2}%", 
                    error_rate * 100.0, self.max_error_rate * 100.0
                ));
            }
        }

        // 检查是否有活跃的流
        if snapshot.streams_active == 0 && total_requests > 10 {
            return HealthStatus::Degraded("No active streams but receiving requests".to_string());
        }

        HealthStatus::Healthy
    }

    fn name(&self) -> &'static str {
        "hls_service"
    }
}

/// 健康检查管理器
pub struct HealthChecker {
    checks: Vec<Box<dyn HealthCheck>>,
    last_check_time: Arc<RwLock<Option<Instant>>>,
    last_results: Arc<RwLock<HashMap<String, (HealthStatus, Instant)>>>,
    cache_duration: Duration,
}

impl HealthChecker {
    pub fn new() -> Self {
        Self {
            checks: Vec::new(),
            last_check_time: Arc::new(RwLock::new(None)),
            last_results: Arc::new(RwLock::new(HashMap::new())),
            cache_duration: Duration::from_secs(30), // 缓存30秒
        }
    }

    pub fn with_cache_duration(mut self, duration: Duration) -> Self {
        self.cache_duration = duration;
        self
    }

    pub fn add_check(mut self, check: Box<dyn HealthCheck>) -> Self {
        self.checks.push(check);
        self
    }

    pub async fn check_all(&self) -> Result<HealthCheckResult> {
        let now = Instant::now();
        
        // 检查缓存是否有效
        {
            let last_check = self.last_check_time.read().await;
            if let Some(last_time) = *last_check {
                if now.duration_since(last_time) < self.cache_duration {
                    let results = self.last_results.read().await;
                    return Ok(self.build_result(&results, last_time));
                }
            }
        }

        // 执行所有健康检查
        let mut results = HashMap::new();
        for check in &self.checks {
            let start_time = Instant::now();
            let status = match tokio::time::timeout(check.timeout(), check.check()).await {
                Ok(status) => status,
                Err(_) => HealthStatus::Unhealthy(format!("Health check '{}' timed out", check.name())),
            };
            let check_duration = start_time.elapsed();
            
            log::debug!("Health check '{}' completed in {:?}: {:?}", 
                check.name(), check_duration, status);
            
            results.insert(check.name().to_string(), (status, now));
        }

        // 更新缓存
        {
            let mut last_check = self.last_check_time.write().await;
            *last_check = Some(now);
            
            let mut last_results = self.last_results.write().await;
            *last_results = results.clone();
        }

        Ok(self.build_result(&results, now))
    }

    fn build_result(&self, results: &HashMap<String, (HealthStatus, Instant)>, check_time: Instant) -> HealthCheckResult {
        let mut overall_status = HealthStatus::Healthy;
        let mut check_results = HashMap::new();

        for (name, (status, _)) in results {
            match status {
                HealthStatus::Unhealthy(_) => {
                    overall_status = HealthStatus::Unhealthy("One or more checks failed".to_string());
                }
                HealthStatus::Degraded(_) => {
                    if overall_status.is_healthy() {
                        overall_status = HealthStatus::Degraded("One or more checks degraded".to_string());
                    }
                }
                HealthStatus::Healthy => {}
            }
            check_results.insert(name.clone(), status.clone());
        }

        HealthCheckResult {
            overall_status,
            checks: check_results,
            timestamp: check_time,
            timestamp_unix: check_time.elapsed().as_secs(),
        }
    }

    pub async fn is_healthy(&self) -> bool {
        match self.check_all().await {
            Ok(result) => result.overall_status.is_healthy(),
            Err(_) => false,
        }
    }
}

impl Default for HealthChecker {
    fn default() -> Self {
        Self::new()
            .add_check(Box::new(SystemResourceCheck::new(80.0, 90.0)))
            .add_check(Box::new(ConnectionHealthCheck::new(1000, 0.1)))
            .add_check(Box::new(HlsHealthCheck::new(0.05)))
    }
}

#[derive(Debug, Serialize)]
pub struct HealthCheckResult {
    pub overall_status: HealthStatus,
    pub checks: HashMap<String, HealthStatus>,
    #[serde(skip)]
    pub timestamp: Instant,
    pub timestamp_unix: u64,
}

impl HealthCheckResult {
    pub fn is_healthy(&self) -> bool {
        self.overall_status.is_healthy()
    }

    pub fn is_degraded(&self) -> bool {
        self.overall_status.is_degraded()
    }

    pub fn is_unhealthy(&self) -> bool {
        self.overall_status.is_unhealthy()
    }
}

// 全局健康检查器
use std::sync::OnceLock;
static GLOBAL_HEALTH_CHECKER: OnceLock<Arc<HealthChecker>> = OnceLock::new();

pub fn get_global_health_checker() -> Arc<HealthChecker> {
    GLOBAL_HEALTH_CHECKER.get_or_init(|| {
        Arc::new(HealthChecker::default())
    }).clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockHealthCheck {
        name: &'static str,
        status: HealthStatus,
    }

    impl MockHealthCheck {
        fn new(name: &'static str, status: HealthStatus) -> Self {
            Self { name, status }
        }
    }

    #[async_trait]
    impl HealthCheck for MockHealthCheck {
        async fn check(&self) -> HealthStatus {
            self.status.clone()
        }

        fn name(&self) -> &'static str {
            self.name
        }
    }

    #[tokio::test]
    async fn test_health_checker_all_healthy() {
        let checker = HealthChecker::new()
            .add_check(Box::new(MockHealthCheck::new("test1", HealthStatus::Healthy)))
            .add_check(Box::new(MockHealthCheck::new("test2", HealthStatus::Healthy)));

        let result = checker.check_all().await.unwrap();
        assert!(result.is_healthy());
        assert_eq!(result.checks.len(), 2);
    }

    #[tokio::test]
    async fn test_health_checker_with_degraded() {
        let checker = HealthChecker::new()
            .add_check(Box::new(MockHealthCheck::new("test1", HealthStatus::Healthy)))
            .add_check(Box::new(MockHealthCheck::new("test2", HealthStatus::Degraded("test".to_string()))));

        let result = checker.check_all().await.unwrap();
        assert!(result.is_degraded());
        assert!(!result.is_healthy());
    }

    #[tokio::test]
    async fn test_health_checker_with_unhealthy() {
        let checker = HealthChecker::new()
            .add_check(Box::new(MockHealthCheck::new("test1", HealthStatus::Healthy)))
            .add_check(Box::new(MockHealthCheck::new("test2", HealthStatus::Unhealthy("test".to_string()))));

        let result = checker.check_all().await.unwrap();
        assert!(result.is_unhealthy());
        assert!(!result.is_healthy());
    }
}
