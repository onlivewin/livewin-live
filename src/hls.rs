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
    let path = req.uri().path();

    let mut file_path: String = String::from("");

    if path.ends_with(".m3u8") {
        //http://127.0.0.1:3000/api/app_name.m3u8
        let temp = &path[0..(path.len() - 5)];
        let parts: Vec<_> = temp.split("/").collect();
        let app_name = String::from(parts[1]);
        let mut temp_data = vec![];
        let mut seq = 0;
        let lock = DATA.read().await;
        match lock.get(&app_name) {
            Some(d) => {
                for i in &d.0 {
                    temp_data.push((i.0, i.1));
                }
                seq = d.1;
            }
            None => {}
        }
        drop(lock);
        let m3u8 = render_m3u8(app_name, temp_data, seq);
        let body = Body::from(m3u8);
        return Ok(Response::new(body));
    } else if path.ends_with(".ts") {
        //http://127.0.0.1:3000/data/app_name/ts_name.m3u8
        let temp = &path[0..(path.len() - 3)];
        let part: Vec<_> = temp.split("/").collect();
        let app_name = String::from(part[2]);
        let ts_name = String::from(part[3]);
        file_path = format!("./data/{}/{}.ts", app_name, ts_name);
    }

    if let Ok(file) = File::open(file_path.as_str()).await {
        let stream = FramedRead::new(file, BytesCodec::new());
        let body = Body::wrap_stream(stream);
        return Ok(Response::new(body));
    }
    Ok(Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(NOTFOUND.into())
        .unwrap())
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
                    match lock.get_mut(&app_name) {
                        Some(d) => {
                            d.0.push_back((file_name, duration));
                            if d.0.len() > 6 {
                                let temp = d.0.pop_front();
                                let stream_path = PathBuf::from(format!(
                                    "data/{}/{}.ts",
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
                            lock.insert(app_name, (d, 1));
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

fn render_m3u8(app_name: String, d: Vec<(i64, u8)>, seq: u32) -> String {
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
    for i in &d {
        m3u8 += format!("#EXTINF:{:.3}\ndata/{}/{}.ts\n", i.1 as f64, app_name, i.0).as_str();
    }
    m3u8
}
