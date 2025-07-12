use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    sync::RwLock,
    task::JoinHandle,
    time::interval,
};
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct HlsSegment {
    pub timestamp: i64,
    pub duration: u8,
}

#[derive(Debug)]
pub struct HlsStream {
    pub segments: VecDeque<HlsSegment>,
    pub sequence: u32,
    pub last_access: Instant,
    pub max_segments: usize,
}

impl HlsStream {
    pub fn new(max_segments: usize) -> Self {
        Self {
            segments: VecDeque::new(),
            sequence: 0,
            last_access: Instant::now(),
            max_segments,
        }
    }

    pub fn add_segment(&mut self, timestamp: i64, duration: u8) {
        self.segments.push_back(HlsSegment { timestamp, duration });
        self.last_access = Instant::now();
        
        // 保持段数量限制
        while self.segments.len() > self.max_segments {
            self.segments.pop_front();
        }
        
        self.sequence += 1;
    }

    pub fn get_segments(&self) -> Vec<HlsSegment> {
        self.segments.iter().cloned().collect()
    }

    pub fn is_expired(&self, ttl: Duration) -> bool {
        Instant::now().duration_since(self.last_access) > ttl
    }

    pub fn touch(&mut self) {
        self.last_access = Instant::now();
    }
}

#[derive(Debug, Serialize)]
pub struct HlsStats {
    pub total_streams: usize,
    pub total_segments: usize,
    pub memory_usage_bytes: usize,
    pub oldest_stream_age_seconds: u64,
}

pub struct HlsStreamManager {
    streams: Arc<RwLock<HashMap<String, HlsStream>>>,
    cleanup_task: Option<JoinHandle<()>>,
    max_segments: usize,
    stream_ttl: Duration,
    cleanup_interval: Duration,
}

impl HlsStreamManager {
    pub fn new(
        max_segments: usize,
        stream_ttl: Duration,
        cleanup_interval: Duration,
    ) -> Self {
        let streams = Arc::new(RwLock::new(HashMap::new()));
        let cleanup_task = Self::start_cleanup_task(
            streams.clone(),
            cleanup_interval,
            stream_ttl,
        );
        
        Self {
            streams,
            cleanup_task: Some(cleanup_task),
            max_segments,
            stream_ttl,
            cleanup_interval,
        }
    }

    fn start_cleanup_task(
        streams: Arc<RwLock<HashMap<String, HlsStream>>>,
        cleanup_interval: Duration,
        stream_ttl: Duration,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = interval(cleanup_interval);
            loop {
                ticker.tick().await;
                Self::cleanup_expired_streams(&streams, stream_ttl).await;
            }
        })
    }

    async fn cleanup_expired_streams(
        streams: &Arc<RwLock<HashMap<String, HlsStream>>>,
        ttl: Duration,
    ) {
        let mut streams = streams.write().await;
        let initial_count = streams.len();
        
        streams.retain(|name, stream| {
            let is_active = !stream.is_expired(ttl);
            if !is_active {
                log::info!("Cleaning up expired HLS stream: {} (inactive for {:?})", 
                    name, stream.last_access.elapsed());
            }
            is_active
        });

        let cleaned_count = initial_count - streams.len();
        if cleaned_count > 0 {
            log::info!("Cleaned up {} expired HLS streams", cleaned_count);
        }
    }

    pub async fn add_segment(&self, app_name: &str, timestamp: i64, duration: u8) -> Result<(), String> {
        let mut streams = self.streams.write().await;
        let stream = streams.entry(app_name.to_string()).or_insert_with(|| {
            log::info!("Creating new HLS stream: {}", app_name);
            HlsStream::new(self.max_segments)
        });
        
        stream.add_segment(timestamp, duration);
        log::debug!("Added segment to stream {}: timestamp={}, duration={}, total_segments={}", 
            app_name, timestamp, duration, stream.segments.len());
        
        Ok(())
    }

    pub async fn get_stream_data(&self, app_name: &str) -> Option<(Vec<HlsSegment>, u32)> {
        let mut streams = self.streams.write().await;
        if let Some(stream) = streams.get_mut(app_name) {
            stream.touch(); // 更新访问时间
            Some((stream.get_segments(), stream.sequence))
        } else {
            log::debug!("Stream not found: {}", app_name);
            None
        }
    }

    pub async fn remove_stream(&self, app_name: &str) -> bool {
        let mut streams = self.streams.write().await;
        if streams.remove(app_name).is_some() {
            log::info!("Removed HLS stream: {}", app_name);
            true
        } else {
            false
        }
    }

    pub async fn get_stats(&self) -> HlsStats {
        let streams = self.streams.read().await;
        let total_streams = streams.len();
        let total_segments: usize = streams.values().map(|s| s.segments.len()).sum();
        
        // 估算内存使用量 (粗略计算)
        let memory_usage_bytes = total_segments * std::mem::size_of::<HlsSegment>() 
            + total_streams * std::mem::size_of::<HlsStream>();
        
        let oldest_stream_age_seconds = streams.values()
            .map(|s| s.last_access.elapsed().as_secs())
            .max()
            .unwrap_or(0);

        HlsStats {
            total_streams,
            total_segments,
            memory_usage_bytes,
            oldest_stream_age_seconds,
        }
    }

    pub async fn list_streams(&self) -> Vec<String> {
        let streams = self.streams.read().await;
        streams.keys().cloned().collect()
    }
}

impl Drop for HlsStreamManager {
    fn drop(&mut self) {
        if let Some(task) = self.cleanup_task.take() {
            task.abort();
            log::info!("HLS stream manager cleanup task stopped");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_stream_creation_and_cleanup() {
        let manager = HlsStreamManager::new(
            6,
            Duration::from_millis(100), // 很短的TTL用于测试
            Duration::from_millis(50),  // 很短的清理间隔
        );

        // 添加一个流
        manager.add_segment("test_stream", 1000, 5).await.unwrap();
        
        // 验证流存在
        assert!(manager.get_stream_data("test_stream").await.is_some());
        
        // 等待流过期和清理
        sleep(Duration::from_millis(200)).await;
        
        // 验证流被清理
        assert!(manager.get_stream_data("test_stream").await.is_none());
    }

    #[tokio::test]
    async fn test_segment_limit() {
        let manager = HlsStreamManager::new(
            3, // 最多3个段
            Duration::from_secs(300),
            Duration::from_secs(60),
        );

        // 添加超过限制的段
        for i in 0..5 {
            manager.add_segment("test_stream", 1000 + i * 5, 5).await.unwrap();
        }

        // 验证只保留最新的3个段
        let (segments, _) = manager.get_stream_data("test_stream").await.unwrap();
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].timestamp, 1010); // 最老的应该是第3个
        assert_eq!(segments[2].timestamp, 1020); // 最新的应该是第5个
    }

    #[tokio::test]
    async fn test_stats() {
        let manager = HlsStreamManager::new(6, Duration::from_secs(300), Duration::from_secs(60));
        
        // 添加一些流和段
        manager.add_segment("stream1", 1000, 5).await.unwrap();
        manager.add_segment("stream1", 1005, 5).await.unwrap();
        manager.add_segment("stream2", 2000, 6).await.unwrap();
        
        let stats = manager.get_stats().await;
        assert_eq!(stats.total_streams, 2);
        assert_eq!(stats.total_segments, 3);
        assert!(stats.memory_usage_bytes > 0);
    }
}
