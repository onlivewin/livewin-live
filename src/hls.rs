use crate::transport::{TsMessageQueue, TsMessageReceiver};
use crate::hls_manager::HlsStreamManager;
use crate::errors::{ErrorHandler, StreamingError, Result};
use crate::metrics::get_global_metrics;
use crate::health::get_global_health_checker;
use crate::rate_limiter::get_global_rate_limiter;
use crate::auth::{get_auth_middleware, Permission};

use {
    hyper::{
        service::{make_service_fn, service_fn},
        Body, Request, Response, Server, StatusCode,
    },
    tokio::fs::File,
    tokio_util::codec::{BytesCodec, FramedRead},
};

use std::{fs, path::PathBuf, sync::Arc, time::Duration, time::SystemTime};

static NOTFOUND: &[u8] = b"Not Found";

use std::sync::OnceLock;

// 全局HLS管理器实例 - 使用OnceLock避免unsafe
static HLS_MANAGER: OnceLock<Arc<HlsStreamManager>> = OnceLock::new();

fn get_hls_manager() -> Arc<HlsStreamManager> {
    HLS_MANAGER.get_or_init(|| {
        Arc::new(HlsStreamManager::new(
            6,                              // max_segments
            Duration::from_secs(300),       // stream_ttl (5 minutes)
            Duration::from_secs(60),        // cleanup_interval (1 minute)
        ))
    }).clone()
}

async fn handle_connection(req: Request<Body>) -> Result<Response<Body>> {
    let start_time = std::time::Instant::now();
    let metrics = get_global_metrics();
    let rate_limiter = get_global_rate_limiter();

    // 获取客户端IP（简化版本，实际应用中需要考虑代理）
    let client_ip = req.headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    // Handle CORS preflight requests
    if req.method() == hyper::Method::OPTIONS {
        let mut response = Response::new(Body::empty());
        response.headers_mut()
            .insert("Access-Control-Allow-Origin", "*".parse().unwrap());
        response.headers_mut()
            .insert("Access-Control-Allow-Methods", "GET, POST, OPTIONS".parse().unwrap());
        response.headers_mut()
            .insert("Access-Control-Allow-Headers", "Content-Type, Authorization".parse().unwrap());
        response.headers_mut()
            .insert("Access-Control-Max-Age", "86400".parse().unwrap());
        return Ok(response);
    }

    let path = req.uri().path();
    log::info!("Request path: {} from IP: {}", path, client_ip);

    // 速率限制检查
    if !rate_limiter.check_limit(client_ip, "hls_request").await? {
        metrics.increment_errors();
        let processing_time = start_time.elapsed();
        metrics.record_request_processing_time(processing_time).await;

        return Ok(ErrorHandler::handle_error(&StreamingError::RateLimitExceeded {
            identifier: client_ip.to_string(),
        }));
    }

    // 记录HLS请求
    metrics.increment_hls_requests();

    // Handle stats endpoint
    if path == "/stats" {
        let manager = get_hls_manager();
        let stats = manager.get_stats().await;
        let processing_time = start_time.elapsed();
        metrics.record_request_processing_time(processing_time).await;
        return Ok(ErrorHandler::handle_success(stats));
    }

    // Handle metrics endpoint (需要认证)
    if path == "/metrics" {
        // 检查认证
        if let Some(auth_header) = req.headers().get("authorization") {
            if let Ok(auth_str) = auth_header.to_str() {
                let auth_middleware = get_auth_middleware();
                if let Some(token) = auth_middleware.extract_token_from_header(auth_str) {
                    match auth_middleware.verify_permission(token, &Permission::ViewMetrics).await {
                        Ok(_) => {
                            let metrics_snapshot = metrics.get_snapshot().await;
                            let processing_time = start_time.elapsed();
                            metrics.record_request_processing_time(processing_time).await;
                            return Ok(ErrorHandler::handle_success(metrics_snapshot));
                        }
                        Err(e) => {
                            metrics.increment_auth_failures();
                            let processing_time = start_time.elapsed();
                            metrics.record_request_processing_time(processing_time).await;
                            return Ok(ErrorHandler::handle_error(&e));
                        }
                    }
                }
            }
        }

        // 未认证或认证失败
        metrics.increment_auth_failures();
        let processing_time = start_time.elapsed();
        metrics.record_request_processing_time(processing_time).await;
        return Ok(ErrorHandler::handle_error(&StreamingError::AuthenticationFailed {
            stream_name: "metrics".to_string(),
        }));
    }

    // Handle health check endpoint (需要认证)
    if path == "/health" {
        // 检查认证
        if let Some(auth_header) = req.headers().get("authorization") {
            if let Ok(auth_str) = auth_header.to_str() {
                let auth_middleware = get_auth_middleware();
                if let Some(token) = auth_middleware.extract_token_from_header(auth_str) {
                    match auth_middleware.verify_permission(token, &Permission::ViewHealth).await {
                        Ok(_) => {
                            let health_checker = get_global_health_checker();
                            match health_checker.check_all().await {
                                Ok(result) => {
                                    let status_code = if result.is_healthy() {
                                        hyper::StatusCode::OK
                                    } else if result.is_degraded() {
                                        hyper::StatusCode::OK // 200 but with degraded status
                                    } else {
                                        hyper::StatusCode::SERVICE_UNAVAILABLE
                                    };

                                    let processing_time = start_time.elapsed();
                                    metrics.record_request_processing_time(processing_time).await;

                                    let mut response = ErrorHandler::handle_success(result);
                                    *response.status_mut() = status_code;
                                    return Ok(response);
                                }
                                Err(e) => {
                                    log::error!("Health check failed: {}", e);
                                    metrics.increment_errors();
                                    let processing_time = start_time.elapsed();
                                    metrics.record_request_processing_time(processing_time).await;
                                    return Ok(ErrorHandler::handle_error(&e));
                                }
                            }
                        }
                        Err(e) => {
                            metrics.increment_auth_failures();
                            let processing_time = start_time.elapsed();
                            metrics.record_request_processing_time(processing_time).await;
                            return Ok(ErrorHandler::handle_error(&e));
                        }
                    }
                }
            }
        }

        // 未认证或认证失败
        metrics.increment_auth_failures();
        let processing_time = start_time.elapsed();
        metrics.record_request_processing_time(processing_time).await;
        return Ok(ErrorHandler::handle_error(&StreamingError::AuthenticationFailed {
            stream_name: "health".to_string(),
        }));
    }

    // Handle stream list endpoint
    if path == "/streams" {
        let manager = get_hls_manager();
        let streams = manager.list_streams().await;
        let processing_time = start_time.elapsed();
        metrics.record_request_processing_time(processing_time).await;
        return Ok(ErrorHandler::handle_success(streams));
    }

    let mut file_path: String = String::from("");

    if path.ends_with(".m3u8") {
        // 记录M3U8播放列表请求
        metrics.increment_hls_playlist_requests();

        // Support both formats:
        // http://127.0.0.1:3001/app_name.m3u8
        // http://127.0.0.1:3001/app_name/stream_key.m3u8
        let temp = &path[0..(path.len() - 5)];
        let parts: Vec<_> = temp.split("/").filter(|s| !s.is_empty()).collect();

        let (base_app_name, app_names_to_try) = if parts.len() == 1 {
            // Format: /app_name.m3u8
            let base_app_name = String::from(parts[0]);
            let app_names_to_try = vec![
                base_app_name.clone(),
                format!("{}/{}", base_app_name, base_app_name)
            ];
            (base_app_name, app_names_to_try)
        } else if parts.len() == 2 {
            // Format: /app_name/stream_key.m3u8
            let app_name = String::from(parts[0]);
            let stream_key = String::from(parts[1]);
            let full_app_name = format!("{}/{}", app_name, stream_key);
            let app_names_to_try = vec![
                full_app_name.clone(),
                app_name.clone()
            ];
            (full_app_name, app_names_to_try)
        } else {
            // Fallback for unexpected formats
            let base_app_name = String::from(parts[parts.len() - 1]);
            let app_names_to_try = vec![base_app_name.clone()];
            (base_app_name, app_names_to_try)
        };

        let manager = get_hls_manager();
        let mut temp_data = vec![];
        let mut seq = 0;
        let mut found_app_name = base_app_name.clone();

        // Try to find stream data
        for app_name in &app_names_to_try {
            if let Some((segments, sequence)) = manager.get_stream_data(app_name).await {
                for segment in segments {
                    temp_data.push((segment.timestamp, segment.duration));
                }
                seq = sequence;
                found_app_name = app_name.clone();
                break;
            }
        }

        log::info!("M3U8 request for {}, found data for {}, {} segments", base_app_name, found_app_name, temp_data.len());

        // Get the base URL from the request
        let host = req.headers()
            .get("host")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("localhost:3001");
        let base_url = format!("http://{}", host);

        let m3u8 = render_m3u8(base_app_name, temp_data, seq, base_url);
        log::info!("Generated M3U8 content: {}", m3u8);
        let body = Body::from(m3u8);
        let mut response = Response::new(body);
        response.headers_mut()
            .insert("Content-Type", "application/vnd.apple.mpegurl".parse().unwrap());
        response.headers_mut()
            .insert("Access-Control-Allow-Origin", "*".parse().unwrap());
        response.headers_mut()
            .insert("Access-Control-Allow-Methods", "GET, POST, OPTIONS".parse().unwrap());
        response.headers_mut()
            .insert("Access-Control-Allow-Headers", "Content-Type".parse().unwrap());

        // 记录请求处理时间
        let processing_time = start_time.elapsed();
        metrics.record_request_processing_time(processing_time).await;

        return Ok(response);
    } else if path.ends_with(".ts") {
        // Support both formats:
        // http://127.0.0.1:3001/data/app_name/app_name/ts_name.ts (old format)
        // http://127.0.0.1:3001/app_name/stream_key/ts_name.ts (new format)
        let temp = &path[0..(path.len() - 3)];
        let parts: Vec<_> = temp.split("/").filter(|s| !s.is_empty()).collect();

        if parts.len() >= 3 && parts[0] == "data" {
            // Old format: /data/app_name/app_name/ts_name.ts
            let app_name = String::from(parts[1]);
            let ts_name = String::from(parts[3]);
            file_path = format!("./data/{}/{}/{}.ts", app_name, app_name, ts_name);
        } else if parts.len() >= 3 {
            // New format: /app_name/stream_key/ts_name.ts
            let app_name = String::from(parts[0]);
            let stream_key = String::from(parts[1]);
            let ts_name = String::from(parts[2]);
            file_path = format!("./data/{}/{}/{}.ts", app_name, stream_key, ts_name);
        }
    }

    if let Ok(file) = File::open(file_path.as_str()).await {
        let stream = FramedRead::new(file, BytesCodec::new());
        let body = Body::wrap_stream(stream);
        let mut response = Response::new(body);
        response.headers_mut()
            .insert("Content-Type", "video/mp2t".parse().unwrap());
        response.headers_mut()
            .insert("Access-Control-Allow-Origin", "*".parse().unwrap());
        response.headers_mut()
            .insert("Access-Control-Allow-Methods", "GET, POST, OPTIONS".parse().unwrap());
        response.headers_mut()
            .insert("Access-Control-Allow-Headers", "Content-Type".parse().unwrap());

        // 记录请求处理时间和传输字节数
        let processing_time = start_time.elapsed();
        metrics.record_request_processing_time(processing_time).await;
        // 注意：这里无法准确计算文件大小，在实际应用中可以通过文件元数据获取

        return Ok(response);
    }
    let mut response = Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(NOTFOUND.into())
        .unwrap();
    response.headers_mut()
        .insert("Access-Control-Allow-Origin", "*".parse().unwrap());

    // 记录请求处理时间
    let processing_time = start_time.elapsed();
    metrics.record_request_processing_time(processing_time).await;

    Ok(response)
}

/// 使用配置文件的TS文件清理逻辑
async fn cleanup_ts_files_with_config(app_name: &str) {
    use crate::config::get_setting;

    let settings = get_setting();
    let cleanup_config = &settings.hls.cleanup;

    log::debug!("Starting TS cleanup for stream: {} with config: max_files={}, min_age={}s, delay={}s",
        app_name, cleanup_config.max_files_per_stream, cleanup_config.min_file_age_seconds, cleanup_config.cleanup_delay_seconds);

    // 延迟清理，给正在播放的客户端一些缓冲时间
    tokio::time::sleep(Duration::from_secs(cleanup_config.cleanup_delay_seconds)).await;

    let stream_dir = PathBuf::from(format!("data/{}", app_name));

    if !stream_dir.exists() {
        log::debug!("Stream directory does not exist: {:?}", stream_dir);
        return;
    }

    log::debug!("Checking TS files in directory: {:?}", stream_dir);

    match fs::read_dir(&stream_dir) {
        Ok(entries) => {
            let mut ts_files = Vec::new();

            // 收集所有TS文件信息
            for entry in entries {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if let Some(extension) = path.extension() {
                        if extension == "ts" {
                            if let Ok(metadata) = entry.metadata() {
                                if let Ok(modified) = metadata.modified() {
                                    ts_files.push((path, modified));
                                }
                            }
                        }
                    }
                }
            }

            // 按修改时间排序（最新的在前）
            ts_files.sort_by(|a, b| b.1.cmp(&a.1));

            log::debug!("Found {} TS files in directory", ts_files.len());

            let now = SystemTime::now();
            let mut deleted_count = 0;
            let mut total_size = 0u64;

            // 计算总大小（如果启用基于大小的清理）
            if cleanup_config.enable_size_based_cleanup {
                for (file_path, _) in &ts_files {
                    if let Ok(metadata) = fs::metadata(file_path) {
                        total_size += metadata.len();
                    }
                }
                log::debug!("Total size of TS files: {:.2} MB", total_size as f64 / (1024.0 * 1024.0));
            }

            let max_size_bytes = cleanup_config.max_total_size_mb * 1024 * 1024;
            let size_exceeded = cleanup_config.enable_size_based_cleanup && total_size > max_size_bytes;

            if size_exceeded {
                log::debug!("Size limit exceeded: {:.2} MB > {:.2} MB",
                    total_size as f64 / (1024.0 * 1024.0),
                    cleanup_config.max_total_size_mb as f64);
            }

            // 删除策略：
            // 1. 保留最新的max_files_per_stream个文件
            // 2. 删除超过min_file_age_seconds秒的旧文件
            // 3. 如果启用大小限制且超过限制，删除更多文件
            for (i, (file_path, modified_time)) in ts_files.iter().enumerate() {
                let file_age = if let Ok(age) = now.duration_since(*modified_time) {
                    age.as_secs()
                } else {
                    0
                };

                let should_delete = if i >= cleanup_config.max_files_per_stream {
                    // 超过文件数量限制
                    log::debug!("File {} should be deleted: exceeds max files limit (index {} >= {})",
                        file_path.display(), i, cleanup_config.max_files_per_stream);
                    true
                } else if size_exceeded && i >= cleanup_config.max_files_per_stream / 2 {
                    // 大小超限且超过一半文件数量
                    log::debug!("File {} should be deleted: size exceeded and index {} >= {}",
                        file_path.display(), i, cleanup_config.max_files_per_stream / 2);
                    true
                } else if file_age > cleanup_config.min_file_age_seconds {
                    // 检查文件年龄（简化条件）
                    log::debug!("File {} should be deleted: age {}s > {}s",
                        file_path.display(), file_age, cleanup_config.min_file_age_seconds);
                    true
                } else {
                    log::debug!("File {} kept: index={}, age={}s, size_exceeded={}",
                        file_path.display(), i, file_age, size_exceeded);
                    false
                };

                if should_delete {
                    // 获取文件大小（在删除前）
                    let file_size = if let Ok(metadata) = fs::metadata(file_path) {
                        metadata.len()
                    } else {
                        0
                    };

                    match fs::remove_file(file_path) {
                        Ok(_) => {
                            deleted_count += 1;
                            total_size = total_size.saturating_sub(file_size);
                            log::debug!("Cleaned up old TS file: {:?} (size: {} bytes)", file_path, file_size);
                        }
                        Err(e) => {
                            log::warn!("Failed to remove TS file {:?}: {}", file_path, e);
                        }
                    }
                }
            }

            if deleted_count > 0 {
                log::info!("Cleaned up {} old TS files for stream: {} (total size: {:.2} MB)",
                    deleted_count, app_name, total_size as f64 / (1024.0 * 1024.0));
            }
        }
        Err(e) => {
            log::warn!("Failed to read stream directory {:?}: {}", stream_dir, e);
        }
    }
}

pub async fn run(mut recv: TsMessageReceiver, port: u32) -> Result<()> {
    let listen_address = format!("[::]:{}", port);
    let sock_addr = listen_address.parse().map_err(|e| {
        StreamingError::ConfigError {
            message: format!("Invalid listen address {}: {}", listen_address, e),
        }
    })?;

    let new_service = make_service_fn(move |_| async {
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(service_fn(move |req| handle_connection(req)))
    });

    let manager = get_hls_manager();
    let metrics = get_global_metrics();

    tokio::spawn(async move {
        while let Some(msg) = recv.recv().await {
            match msg {
                TsMessageQueue::Ts(app_name, file_name, duration) => {
                    log::info!("Received TS message: app_name={}, file_name={}, duration={}", app_name, file_name, duration);

                    // 记录HLS段生成
                    metrics.increment_hls_segments();

                    if let Err(e) = manager.add_segment(&app_name, file_name, duration).await {
                        log::error!("Failed to add segment to stream {}: {}", app_name, e);
                        metrics.increment_errors();
                    }

                    // 改进的TS文件清理逻辑
                    let app_name_for_cleanup = app_name.clone();
                    tokio::spawn(async move {
                        cleanup_ts_files_with_config(&app_name_for_cleanup).await;
                    });
                }
                TsMessageQueue::Close(app_name) => {
                    log::info!("Received close message for app: {}", app_name);
                    manager.remove_stream(&app_name).await;
                }
            }
        }
    });

    let server = Server::bind(&sock_addr).serve(new_service);
    log::info!("HLS server listening on http://{}", sock_addr);

    if let Err(e) = server.await {
        log::error!("HLS server error: {}", e);
        return Err(StreamingError::NetworkError {
            message: format!("HLS server failed: {}", e),
        });
    }

    Ok(())
}

fn render_m3u8(app_name: String, d: Vec<(i64, u8)>, seq: u32, base_url: String) -> String {
    let mut max_duration: u32 = 0;
    for i in &d {
        if i.1 as u32 > max_duration {
            max_duration = i.1 as u32
        }
    }
    let mut m3u8 = format!("#EXTM3U\n");
    m3u8 += format!("#EXT-X-VERSION:3\n").as_str();
    m3u8 += format!("#EXT-X-TARGETDURATION:{}\n", max_duration).as_str();
    m3u8 += format!("#EXT-X-MEDIA-SEQUENCE:{}\n", seq).as_str();
    m3u8 += format!("#EXT-X-PLAYLIST-TYPE:LIVE\n").as_str();

    // Generate TS file paths based on app_name format
    for i in &d {
        let ts_path = if app_name.contains('/') {
            // Format: app_name/stream_key -> http://host/app_name/stream_key/timestamp.ts (absolute URL)
            format!("{}/{}/{}.ts", base_url, app_name, i.0)
        } else {
            // Legacy format: app_name -> http://host/data/app_name/app_name/timestamp.ts (absolute URL)
            format!("{}/data/{}/{}/{}.ts", base_url, app_name, app_name, i.0)
        };
        m3u8 += format!("#EXTINF:{:.3}\n{}\n", i.1 as f64, ts_path).as_str();
    }
    m3u8
}
