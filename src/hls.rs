use crate::transport::{TsMessageQueue, TsMessageReceiver};

use {
    hyper::{
        service::{make_service_fn, service_fn},
        Body, Request, Response, Server, StatusCode,
    },
    tokio::fs::File,
    tokio_util::codec::{BytesCodec, FramedRead},
};

use lazy_static::*;
use std::{
    collections::{HashMap, VecDeque},
    vec,
};
use std::{fs, path::PathBuf, sync::Arc};
use tokio::sync::RwLock;

type GenericError = Box<dyn std::error::Error + Send + Sync>;
type Result<T> = std::result::Result<T, GenericError>;

static NOTFOUND: &[u8] = b"Not Found";

lazy_static! {
    static ref DATA: Arc<RwLock<HashMap<String, (VecDeque<(i64, u8)>, u32)>>> =
        Arc::new(RwLock::new(HashMap::new()));
}

async fn handle_connection(req: Request<Body>) -> Result<Response<Body>> {
    // Handle CORS preflight requests
    if req.method() == hyper::Method::OPTIONS {
        let mut response = Response::new(Body::empty());
        response.headers_mut()
            .insert("Access-Control-Allow-Origin", "*".parse().unwrap());
        response.headers_mut()
            .insert("Access-Control-Allow-Methods", "GET, POST, OPTIONS".parse().unwrap());
        response.headers_mut()
            .insert("Access-Control-Allow-Headers", "Content-Type".parse().unwrap());
        response.headers_mut()
            .insert("Access-Control-Max-Age", "86400".parse().unwrap());
        return Ok(response);
    }

    let path = req.uri().path();

    let mut file_path: String = String::from("");

    log::info!("Request path: {}", path);

    if path.ends_with(".m3u8") {
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

        let mut temp_data = vec![];
        let mut seq = 0;
        let mut found_app_name = base_app_name.clone();

        let lock = DATA.read().await;
        for app_name in &app_names_to_try {
            if let Some(d) = lock.get(app_name) {
                for i in &d.0 {
                    temp_data.push((i.0, i.1));
                }
                seq = d.1;
                found_app_name = app_name.clone();
                break;
            }
        }
        drop(lock);

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
        return Ok(response);
    }
    let mut response = Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(NOTFOUND.into())
        .unwrap();
    response.headers_mut()
        .insert("Access-Control-Allow-Origin", "*".parse().unwrap());
    Ok(response)
}

pub async fn run(mut recv: TsMessageReceiver, port: u32) -> Result<()> {
    let listen_address = format!("[::]:{}", port);
    let sock_addr = listen_address.parse().unwrap();

    let new_service = make_service_fn(move |_| async {
        Ok::<_, GenericError>(service_fn(move |req| handle_connection(req)))
    });

    tokio::spawn(async move {
        while let Some(msg) = recv.recv().await {
            let mut lock = DATA.write().await;
            match msg {
                TsMessageQueue::Ts(app_name, file_name, duration) => {
                    log::info!("Received TS message: app_name={}, file_name={}, duration={}", app_name, file_name, duration);
                    match lock.get_mut(&app_name) {
                        Some(d) => {
                            d.0.push_back((file_name, duration));
                            log::info!("Added TS to existing queue for {}, queue length: {}", app_name, d.0.len());
                            if d.0.len() > 6 {
                                let temp = d.0.pop_front();
                                let stream_path = PathBuf::from(format!(
                                    "data/{}/{}/{}.ts",
                                    app_name,
                                    app_name,
                                    temp.unwrap().0
                                ));
                                if stream_path.exists() {
                                    _ = fs::remove_file(stream_path);
                                }
                            }
                            d.1 += 1;
                        }
                        None => {
                            let mut d = VecDeque::new();
                            d.push_back((file_name, duration));
                            lock.insert(app_name.clone(), (d, 1));
                            log::info!("Created new TS queue for {}", app_name);
                        }
                    }
                }
            }
            drop(lock);
        }
    });

    let server = Server::bind(&sock_addr).serve(new_service);
    log::info!("Hls services listening on http://{}", sock_addr);
    server.await?;

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
