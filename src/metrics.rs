use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use std::collections::HashMap;
use serde::Serialize;
use tokio::sync::RwLock;
use std::sync::Arc;

/// 性能指标收集器
#[derive(Debug)]
pub struct PerformanceMetrics {
    // 连接相关指标
    pub connections_total: AtomicU64,
    pub connections_active: AtomicU64,
    pub connections_failed: AtomicU64,
    
    // 流相关指标
    pub streams_active: AtomicU64,
    pub streams_created_total: AtomicU64,
    pub streams_closed_total: AtomicU64,
    
    // 数据传输指标
    pub bytes_received_total: AtomicU64,
    pub bytes_sent_total: AtomicU64,
    pub packets_processed_total: AtomicU64,
    pub packets_dropped_total: AtomicU64,
    
    // 错误指标
    pub errors_total: AtomicU64,
    pub auth_failures_total: AtomicU64,
    pub protocol_errors_total: AtomicU64,
    
    // HLS相关指标
    pub hls_requests_total: AtomicU64,
    pub hls_segments_generated_total: AtomicU64,
    pub hls_playlist_requests_total: AtomicU64,
    
    // 系统指标
    start_time: Instant,
    
    // 延迟统计
    latency_stats: Arc<RwLock<LatencyStats>>,
}

#[derive(Debug, Default)]
struct LatencyStats {
    packet_processing_times: Vec<Duration>,
    request_processing_times: Vec<Duration>,
    max_samples: usize,
}

impl LatencyStats {
    fn new(max_samples: usize) -> Self {
        Self {
            packet_processing_times: Vec::with_capacity(max_samples),
            request_processing_times: Vec::with_capacity(max_samples),
            max_samples,
        }
    }

    fn add_packet_processing_time(&mut self, duration: Duration) {
        if self.packet_processing_times.len() >= self.max_samples {
            self.packet_processing_times.remove(0);
        }
        self.packet_processing_times.push(duration);
    }

    fn add_request_processing_time(&mut self, duration: Duration) {
        if self.request_processing_times.len() >= self.max_samples {
            self.request_processing_times.remove(0);
        }
        self.request_processing_times.push(duration);
    }

    fn calculate_percentiles(times: &[Duration]) -> (Duration, Duration, Duration) {
        if times.is_empty() {
            return (Duration::ZERO, Duration::ZERO, Duration::ZERO);
        }

        let mut sorted_times = times.to_vec();
        sorted_times.sort();

        let len = sorted_times.len();
        let p50_idx = len / 2;
        let p95_idx = (len * 95) / 100;
        let p99_idx = (len * 99) / 100;

        (
            sorted_times[p50_idx.min(len - 1)],
            sorted_times[p95_idx.min(len - 1)],
            sorted_times[p99_idx.min(len - 1)],
        )
    }
}

impl PerformanceMetrics {
    pub fn new() -> Self {
        Self {
            connections_total: AtomicU64::new(0),
            connections_active: AtomicU64::new(0),
            connections_failed: AtomicU64::new(0),
            streams_active: AtomicU64::new(0),
            streams_created_total: AtomicU64::new(0),
            streams_closed_total: AtomicU64::new(0),
            bytes_received_total: AtomicU64::new(0),
            bytes_sent_total: AtomicU64::new(0),
            packets_processed_total: AtomicU64::new(0),
            packets_dropped_total: AtomicU64::new(0),
            errors_total: AtomicU64::new(0),
            auth_failures_total: AtomicU64::new(0),
            protocol_errors_total: AtomicU64::new(0),
            hls_requests_total: AtomicU64::new(0),
            hls_segments_generated_total: AtomicU64::new(0),
            hls_playlist_requests_total: AtomicU64::new(0),
            start_time: Instant::now(),
            latency_stats: Arc::new(RwLock::new(LatencyStats::new(1000))),
        }
    }

    // 连接指标
    pub fn increment_connections(&self) {
        self.connections_total.fetch_add(1, Ordering::Relaxed);
        self.connections_active.fetch_add(1, Ordering::Relaxed);
    }

    pub fn decrement_connections(&self) {
        self.connections_active.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn increment_connection_failures(&self) {
        self.connections_failed.fetch_add(1, Ordering::Relaxed);
    }

    // 流指标
    pub fn increment_streams(&self) {
        self.streams_created_total.fetch_add(1, Ordering::Relaxed);
        self.streams_active.fetch_add(1, Ordering::Relaxed);
    }

    pub fn decrement_streams(&self) {
        self.streams_closed_total.fetch_add(1, Ordering::Relaxed);
        self.streams_active.fetch_sub(1, Ordering::Relaxed);
    }

    // 数据传输指标
    pub fn add_bytes_received(&self, bytes: u64) {
        self.bytes_received_total.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn add_bytes_sent(&self, bytes: u64) {
        self.bytes_sent_total.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn increment_packets_processed(&self) {
        self.packets_processed_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_packets_dropped(&self) {
        self.packets_dropped_total.fetch_add(1, Ordering::Relaxed);
    }

    // 错误指标
    pub fn increment_errors(&self) {
        self.errors_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_auth_failures(&self) {
        self.auth_failures_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_protocol_errors(&self) {
        self.protocol_errors_total.fetch_add(1, Ordering::Relaxed);
    }

    // HLS指标
    pub fn increment_hls_requests(&self) {
        self.hls_requests_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_hls_segments(&self) {
        self.hls_segments_generated_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn increment_hls_playlist_requests(&self) {
        self.hls_playlist_requests_total.fetch_add(1, Ordering::Relaxed);
    }

    // 延迟统计
    pub async fn record_packet_processing_time(&self, duration: Duration) {
        let mut stats = self.latency_stats.write().await;
        stats.add_packet_processing_time(duration);
    }

    pub async fn record_request_processing_time(&self, duration: Duration) {
        let mut stats = self.latency_stats.write().await;
        stats.add_request_processing_time(duration);
    }

    // 获取统计快照
    pub async fn get_snapshot(&self) -> MetricsSnapshot {
        let uptime = self.start_time.elapsed();
        let uptime_seconds = uptime.as_secs_f64();

        let stats = self.latency_stats.read().await;
        let (packet_p50, packet_p95, packet_p99) = 
            LatencyStats::calculate_percentiles(&stats.packet_processing_times);
        let (request_p50, request_p95, request_p99) = 
            LatencyStats::calculate_percentiles(&stats.request_processing_times);

        MetricsSnapshot {
            uptime_seconds: uptime.as_secs(),
            connections_total: self.connections_total.load(Ordering::Relaxed),
            connections_active: self.connections_active.load(Ordering::Relaxed),
            connections_failed: self.connections_failed.load(Ordering::Relaxed),
            streams_active: self.streams_active.load(Ordering::Relaxed),
            streams_created_total: self.streams_created_total.load(Ordering::Relaxed),
            streams_closed_total: self.streams_closed_total.load(Ordering::Relaxed),
            bytes_received_total: self.bytes_received_total.load(Ordering::Relaxed),
            bytes_sent_total: self.bytes_sent_total.load(Ordering::Relaxed),
            packets_processed_total: self.packets_processed_total.load(Ordering::Relaxed),
            packets_dropped_total: self.packets_dropped_total.load(Ordering::Relaxed),
            errors_total: self.errors_total.load(Ordering::Relaxed),
            auth_failures_total: self.auth_failures_total.load(Ordering::Relaxed),
            protocol_errors_total: self.protocol_errors_total.load(Ordering::Relaxed),
            hls_requests_total: self.hls_requests_total.load(Ordering::Relaxed),
            hls_segments_generated_total: self.hls_segments_generated_total.load(Ordering::Relaxed),
            hls_playlist_requests_total: self.hls_playlist_requests_total.load(Ordering::Relaxed),
            
            // 计算速率
            connections_per_second: if uptime_seconds > 0.0 {
                self.connections_total.load(Ordering::Relaxed) as f64 / uptime_seconds
            } else { 0.0 },
            bytes_received_per_second: if uptime_seconds > 0.0 {
                self.bytes_received_total.load(Ordering::Relaxed) as f64 / uptime_seconds
            } else { 0.0 },
            bytes_sent_per_second: if uptime_seconds > 0.0 {
                self.bytes_sent_total.load(Ordering::Relaxed) as f64 / uptime_seconds
            } else { 0.0 },
            packets_per_second: if uptime_seconds > 0.0 {
                self.packets_processed_total.load(Ordering::Relaxed) as f64 / uptime_seconds
            } else { 0.0 },
            
            // 延迟统计
            packet_processing_latency_p50_ms: packet_p50.as_millis() as f64,
            packet_processing_latency_p95_ms: packet_p95.as_millis() as f64,
            packet_processing_latency_p99_ms: packet_p99.as_millis() as f64,
            request_processing_latency_p50_ms: request_p50.as_millis() as f64,
            request_processing_latency_p95_ms: request_p95.as_millis() as f64,
            request_processing_latency_p99_ms: request_p99.as_millis() as f64,
        }
    }

    pub fn reset(&self) {
        // 重置所有计数器（保留累计总数）
        self.connections_active.store(0, Ordering::Relaxed);
        self.streams_active.store(0, Ordering::Relaxed);
    }
}

#[derive(Debug, Serialize)]
pub struct MetricsSnapshot {
    pub uptime_seconds: u64,
    
    // 连接指标
    pub connections_total: u64,
    pub connections_active: u64,
    pub connections_failed: u64,
    pub connections_per_second: f64,
    
    // 流指标
    pub streams_active: u64,
    pub streams_created_total: u64,
    pub streams_closed_total: u64,
    
    // 数据传输指标
    pub bytes_received_total: u64,
    pub bytes_sent_total: u64,
    pub bytes_received_per_second: f64,
    pub bytes_sent_per_second: f64,
    
    // 包处理指标
    pub packets_processed_total: u64,
    pub packets_dropped_total: u64,
    pub packets_per_second: f64,
    
    // 错误指标
    pub errors_total: u64,
    pub auth_failures_total: u64,
    pub protocol_errors_total: u64,
    
    // HLS指标
    pub hls_requests_total: u64,
    pub hls_segments_generated_total: u64,
    pub hls_playlist_requests_total: u64,
    
    // 延迟统计
    pub packet_processing_latency_p50_ms: f64,
    pub packet_processing_latency_p95_ms: f64,
    pub packet_processing_latency_p99_ms: f64,
    pub request_processing_latency_p50_ms: f64,
    pub request_processing_latency_p95_ms: f64,
    pub request_processing_latency_p99_ms: f64,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self::new()
    }
}

// 全局指标实例
use std::sync::OnceLock;
static GLOBAL_METRICS: OnceLock<Arc<PerformanceMetrics>> = OnceLock::new();

pub fn get_global_metrics() -> Arc<PerformanceMetrics> {
    GLOBAL_METRICS.get_or_init(|| {
        Arc::new(PerformanceMetrics::new())
    }).clone()
}

// 便利宏用于记录指标
#[macro_export]
macro_rules! metrics {
    (increment_connections) => {
        $crate::metrics::get_global_metrics().increment_connections()
    };
    (decrement_connections) => {
        $crate::metrics::get_global_metrics().decrement_connections()
    };
    (increment_streams) => {
        $crate::metrics::get_global_metrics().increment_streams()
    };
    (decrement_streams) => {
        $crate::metrics::get_global_metrics().decrement_streams()
    };
    (add_bytes_received, $bytes:expr) => {
        $crate::metrics::get_global_metrics().add_bytes_received($bytes)
    };
    (add_bytes_sent, $bytes:expr) => {
        $crate::metrics::get_global_metrics().add_bytes_sent($bytes)
    };
    (increment_packets_processed) => {
        $crate::metrics::get_global_metrics().increment_packets_processed()
    };
    (increment_errors) => {
        $crate::metrics::get_global_metrics().increment_errors()
    };
    (increment_hls_requests) => {
        $crate::metrics::get_global_metrics().increment_hls_requests()
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_metrics_basic_operations() {
        let metrics = PerformanceMetrics::new();
        
        metrics.increment_connections();
        metrics.increment_streams();
        metrics.add_bytes_received(1024);
        metrics.increment_packets_processed();
        
        let snapshot = metrics.get_snapshot().await;
        assert_eq!(snapshot.connections_total, 1);
        assert_eq!(snapshot.connections_active, 1);
        assert_eq!(snapshot.streams_active, 1);
        assert_eq!(snapshot.bytes_received_total, 1024);
        assert_eq!(snapshot.packets_processed_total, 1);
    }

    #[tokio::test]
    async fn test_latency_recording() {
        let metrics = PerformanceMetrics::new();
        
        metrics.record_packet_processing_time(Duration::from_millis(10)).await;
        metrics.record_packet_processing_time(Duration::from_millis(20)).await;
        metrics.record_request_processing_time(Duration::from_millis(5)).await;
        
        let snapshot = metrics.get_snapshot().await;
        assert!(snapshot.packet_processing_latency_p50_ms > 0.0);
        assert!(snapshot.request_processing_latency_p50_ms > 0.0);
    }
}
