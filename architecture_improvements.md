# 直播软件架构改进建议

## 1. 架构重构建议

### A. 引入依赖注入容器

```rust
// 建议的新架构
pub struct StreamingServer {
    config: Arc<Config>,
    manager: Arc<dyn StreamManager>,
    protocols: Vec<Box<dyn ProtocolHandler>>,
    storage: Arc<dyn StorageProvider>,
    metrics: Arc<dyn MetricsCollector>,
}

pub trait StreamManager: Send + Sync {
    async fn create_stream(&self, app_name: &str, stream_key: &str) -> Result<StreamHandle>;
    async fn join_stream(&self, app_name: &str) -> Result<StreamWatcher>;
    async fn release_stream(&self, app_name: &str) -> Result<()>;
}

pub trait ProtocolHandler: Send + Sync {
    fn protocol_name(&self) -> &'static str;
    async fn handle_connection(&self, stream: TcpStream) -> Result<()>;
}

pub trait StorageProvider: Send + Sync {
    async fn store_segment(&self, app_name: &str, segment: &[u8]) -> Result<String>;
    async fn get_playlist(&self, app_name: &str) -> Result<String>;
    async fn cleanup_old_segments(&self, app_name: &str, keep_count: usize) -> Result<()>;
}
```

### B. 改进内存管理

```rust
// 替换全局静态数据
pub struct HlsDataManager {
    streams: Arc<RwLock<HashMap<String, StreamData>>>,
    cleanup_interval: Duration,
    max_segments: usize,
}

pub struct StreamData {
    segments: VecDeque<Segment>,
    sequence: u32,
    last_access: Instant,
    ttl: Duration,
}

impl HlsDataManager {
    pub async fn start_cleanup_task(&self) {
        let streams = self.streams.clone();
        let interval = self.cleanup_interval;
        
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                Self::cleanup_expired_streams(&streams).await;
            }
        });
    }
    
    async fn cleanup_expired_streams(streams: &Arc<RwLock<HashMap<String, StreamData>>>) {
        let mut streams = streams.write().await;
        let now = Instant::now();
        streams.retain(|_, data| now.duration_since(data.last_access) < data.ttl);
    }
}
```

### C. 统一错误处理

```rust
#[derive(Debug, thiserror::Error)]
pub enum StreamingError {
    #[error("Stream not found: {stream_name}")]
    StreamNotFound { stream_name: String },
    
    #[error("Authentication failed for stream: {stream_name}")]
    AuthenticationFailed { stream_name: String },
    
    #[error("Protocol error: {message}")]
    ProtocolError { message: String },
    
    #[error("Storage error: {source}")]
    StorageError { #[from] source: std::io::Error },
    
    #[error("Configuration error: {message}")]
    ConfigError { message: String },
}

pub type Result<T> = std::result::Result<T, StreamingError>;

// 统一的错误处理中间件
pub struct ErrorHandler;

impl ErrorHandler {
    pub fn handle_error(error: &StreamingError) -> Response<Body> {
        match error {
            StreamingError::StreamNotFound { stream_name } => {
                log::warn!("Stream not found: {}", stream_name);
                Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::from("Stream not found"))
                    .unwrap()
            }
            StreamingError::AuthenticationFailed { stream_name } => {
                log::warn!("Authentication failed for: {}", stream_name);
                Response::builder()
                    .status(StatusCode::UNAUTHORIZED)
                    .body(Body::from("Authentication failed"))
                    .unwrap()
            }
            _ => {
                log::error!("Internal server error: {}", error);
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from("Internal server error"))
                    .unwrap()
            }
        }
    }
}
```

## 2. 性能优化建议

### A. 连接池和资源复用

```rust
pub struct ConnectionPool<T> {
    pool: Arc<Mutex<VecDeque<T>>>,
    factory: Arc<dyn Fn() -> T + Send + Sync>,
    max_size: usize,
}

impl<T> ConnectionPool<T> {
    pub async fn acquire(&self) -> Option<T> {
        let mut pool = self.pool.lock().await;
        pool.pop_front().or_else(|| {
            if pool.len() < self.max_size {
                Some((self.factory)())
            } else {
                None
            }
        })
    }
    
    pub async fn release(&self, item: T) {
        let mut pool = self.pool.lock().await;
        if pool.len() < self.max_size {
            pool.push_back(item);
        }
    }
}
```

### B. 零拷贝数据传输

```rust
use bytes::{Bytes, BytesMut};

pub struct ZeroCopyPacket {
    data: Bytes,
    metadata: PacketMetadata,
}

impl ZeroCopyPacket {
    pub fn new(data: Bytes, metadata: PacketMetadata) -> Self {
        Self { data, metadata }
    }
    
    pub fn slice(&self, range: std::ops::Range<usize>) -> Bytes {
        self.data.slice(range)
    }
}

// 使用Arc<Bytes>避免数据拷贝
pub type SharedPacket = Arc<ZeroCopyPacket>;
```

## 3. 监控和可观测性

### A. 指标收集

```rust
use prometheus::{Counter, Histogram, Gauge};

pub struct Metrics {
    pub connections_total: Counter,
    pub streams_active: Gauge,
    pub packet_processing_duration: Histogram,
    pub bandwidth_bytes: Counter,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            connections_total: Counter::new("connections_total", "Total connections").unwrap(),
            streams_active: Gauge::new("streams_active", "Active streams").unwrap(),
            packet_processing_duration: Histogram::new("packet_processing_duration_seconds", "Packet processing duration").unwrap(),
            bandwidth_bytes: Counter::new("bandwidth_bytes_total", "Total bandwidth").unwrap(),
        }
    }
}
```

### B. 健康检查

```rust
pub struct HealthChecker {
    checks: Vec<Box<dyn HealthCheck>>,
}

#[async_trait]
pub trait HealthCheck: Send + Sync {
    async fn check(&self) -> HealthStatus;
    fn name(&self) -> &'static str;
}

pub enum HealthStatus {
    Healthy,
    Unhealthy(String),
}

impl HealthChecker {
    pub async fn check_all(&self) -> HashMap<String, HealthStatus> {
        let mut results = HashMap::new();
        for check in &self.checks {
            results.insert(check.name().to_string(), check.check().await);
        }
        results
    }
}
```

## 4. 配置管理改进

### A. 分层配置系统

```rust
use config::{Config, Environment, File};

pub struct ConfigManager {
    config: Config,
}

impl ConfigManager {
    pub fn new() -> Result<Self> {
        let mut config = Config::builder()
            // 默认配置
            .add_source(File::with_name("config/default").required(false))
            // 环境特定配置
            .add_source(File::with_name(&format!("config/{}", 
                std::env::var("ENVIRONMENT").unwrap_or_else(|_| "development".into())
            )).required(false))
            // 本地配置覆盖
            .add_source(File::with_name("config/local").required(false))
            // 环境变量覆盖
            .add_source(Environment::with_prefix("XLIVE").separator("_"))
            .build()?;
            
        Ok(Self { config })
    }
    
    pub fn get<T>(&self, key: &str) -> Result<T> 
    where
        T: serde::de::DeserializeOwned,
    {
        self.config.get(key).map_err(|e| ConfigError { message: e.to_string() }.into())
    }
}
```

## 5. 安全性改进

### A. 认证和授权

```rust
#[async_trait]
pub trait AuthProvider: Send + Sync {
    async fn authenticate(&self, token: &str) -> Result<User>;
    async fn authorize(&self, user: &User, resource: &str, action: &str) -> Result<bool>;
}

pub struct JwtAuthProvider {
    secret: String,
}

impl JwtAuthProvider {
    pub fn new(secret: String) -> Self {
        Self { secret }
    }
}

#[async_trait]
impl AuthProvider for JwtAuthProvider {
    async fn authenticate(&self, token: &str) -> Result<User> {
        // JWT验证逻辑
        todo!()
    }
    
    async fn authorize(&self, user: &User, resource: &str, action: &str) -> Result<bool> {
        // 权限检查逻辑
        todo!()
    }
}
```

### B. 速率限制

```rust
use std::time::{Duration, Instant};
use std::collections::HashMap;

pub struct RateLimiter {
    limits: HashMap<String, (u32, Duration)>, // (max_requests, window)
    counters: Arc<RwLock<HashMap<String, (u32, Instant)>>>, // (count, window_start)
}

impl RateLimiter {
    pub async fn check_limit(&self, key: &str, limit_type: &str) -> Result<bool> {
        let (max_requests, window) = self.limits.get(limit_type)
            .ok_or_else(|| StreamingError::ConfigError { 
                message: format!("Unknown limit type: {}", limit_type) 
            })?;
            
        let mut counters = self.counters.write().await;
        let now = Instant::now();
        
        let (count, window_start) = counters.entry(key.to_string())
            .or_insert((0, now));
            
        if now.duration_since(*window_start) > *window {
            *count = 0;
            *window_start = now;
        }
        
        if *count >= *max_requests {
            return Ok(false);
        }
        
        *count += 1;
        Ok(true)
    }
}

## 6. 具体代码改进示例

### A. 修复HLS内存泄漏问题

当前问题代码：
```rust
// src/hls.rs - 存在内存泄漏
lazy_static! {
    static ref DATA: Arc<RwLock<HashMap<String, (VecDeque<(i64, u8)>, u32)>>> =
        Arc::new(RwLock::new(HashMap::new()));
}
```

改进后的代码：
```rust
pub struct HlsStreamManager {
    streams: Arc<RwLock<HashMap<String, HlsStream>>>,
    cleanup_task: Option<JoinHandle<()>>,
}

pub struct HlsStream {
    segments: VecDeque<HlsSegment>,
    sequence: u32,
    last_access: Instant,
    max_segments: usize,
}

impl HlsStreamManager {
    pub fn new(cleanup_interval: Duration, max_segments: usize) -> Self {
        let streams = Arc::new(RwLock::new(HashMap::new()));
        let cleanup_task = Self::start_cleanup_task(streams.clone(), cleanup_interval);

        Self {
            streams,
            cleanup_task: Some(cleanup_task),
        }
    }

    fn start_cleanup_task(
        streams: Arc<RwLock<HashMap<String, HlsStream>>>,
        interval: Duration,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            loop {
                ticker.tick().await;
                Self::cleanup_inactive_streams(&streams).await;
            }
        })
    }

    async fn cleanup_inactive_streams(streams: &Arc<RwLock<HashMap<String, HlsStream>>>) {
        let mut streams = streams.write().await;
        let now = Instant::now();
        let inactive_threshold = Duration::from_secs(300); // 5分钟

        streams.retain(|name, stream| {
            let is_active = now.duration_since(stream.last_access) < inactive_threshold;
            if !is_active {
                log::info!("Cleaning up inactive HLS stream: {}", name);
            }
            is_active
        });
    }

    pub async fn add_segment(&self, app_name: &str, timestamp: i64, duration: u8) {
        let mut streams = self.streams.write().await;
        let stream = streams.entry(app_name.to_string()).or_insert_with(|| {
            HlsStream {
                segments: VecDeque::new(),
                sequence: 0,
                last_access: Instant::now(),
                max_segments: 6,
            }
        });

        stream.segments.push_back(HlsSegment { timestamp, duration });
        stream.last_access = Instant::now();

        // 保持段数量限制
        while stream.segments.len() > stream.max_segments {
            stream.segments.pop_front();
        }

        stream.sequence += 1;
    }
}
```
```
