use crate::error::Error as PError;
use crate::packet::{Packet, PacketType};
use crate::transport::{ChannelMessage, ManagerHandle};
use crate::Message;
use crate::{put_i24_be, put_i32_be, FLV_HEADER};
use bytes::{Bytes, BytesMut};
use hyper::body::Sender;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server, StatusCode};
use std::collections::HashMap;
use std::convert::Infallible;
use tokio::sync::oneshot;

async fn http_flv(
    manager_handle: ManagerHandle,
    req: Request<Body>,
) -> Result<Response<Body>, PError> {
    let params: HashMap<String, String> = req
        .uri()
        .query()
        .map(|v| {
            url::form_urlencoded::parse(v.as_bytes())
                .into_owned()
                .collect()
        })
        .unwrap_or_else(HashMap::new);

    if let Some(token) = params.get("token") {
        //check token
    } else {
        return Ok(Response::builder()
            .status(StatusCode::FORBIDDEN)
            .body(Body::empty())
            .unwrap());
    }

    let path = req.uri().path();

    if path.is_empty() || !path.ends_with(".flv") {
        return Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap());
    }
    let app_name = &path[1..(path.len() - 4)];

    log::info!("app name {}", app_name);
    let mut conn = Conn::new(manager_handle);
    let (sender, body) = Body::channel();
    match conn.init(app_name.to_owned(), sender).await {
        Ok(_) => {}
        Err(e) => {
            log::error!("{}", e);
            let mut res = Response::new(Body::empty());
            res.headers_mut()
                .insert("Access-Control-Allow-Origin", "*".parse().unwrap());
            *res.status_mut() = StatusCode::NOT_FOUND;
            return Ok(res);
        }
    }
    let mut res = Response::new(body);
    res.headers_mut()
        .insert("Access-Control-Allow-Origin", "*".parse().unwrap());
    Ok(res)
}

pub struct Service {
    manager_handle: ManagerHandle,
}

impl Service {
    pub fn new(manager_handle: ManagerHandle) -> Self {
        Self { manager_handle }
    }

    pub async fn run(&self, port: i32) {
        let manager_handle = self.manager_handle.clone();
        let make_service = make_service_fn(move |_| {
            let manager_handle = manager_handle.clone();
            async move {
                Ok::<_, Infallible>(service_fn(move |req| http_flv(manager_handle.clone(), req)))
            }
        });
        let addr = format!("[::]:{}", port).parse().unwrap();
        let server = Server::bind(&addr).serve(make_service);
        log::info!("http-flv service Listening on http://{}", addr);
        _ = server.await;
    }
}

pub struct Conn {
    manager_handle: ManagerHandle,
}

impl Conn {
    pub fn new(manager_handle: ManagerHandle) -> Self {
        Self { manager_handle }
    }

    async fn init(&mut self, app_name: String, mut body_sender: Sender) -> Result<(), PError> {
        let (request, response) = oneshot::channel();
        self.manager_handle
            .send(ChannelMessage::Join((app_name, request)))
            .map_err(|_| PError::ChannelJoinFailed)?;

        match response.await {
            Ok((session_sender, mut session_receiver)) => {
                tokio::spawn(async move {
                    let mut retrun_data = vec![];
                    let (request, response) = oneshot::channel();
                    match session_sender.send(Message::InitData(request)) {
                        Ok(_) => {}
                        Err(e) => {
                            log::error!("{}", e);
                            return;
                        }
                    }

                    //这边可能出现一致性错误,可能掉帧
                    match body_sender.send_data(Bytes::from(&FLV_HEADER[..])).await {
                        Ok(_) => {}
                        Err(e) => {
                            log::error!("{}", e);
                            return;
                        }
                    }
                    if let Ok((meta, video, audio, gop)) = response.await {
                        log::info!("send init data");
                        meta.map(|m| retrun_data.push(Bytes::from(packet_to_bytes(&m))));
                        audio.map(|a| retrun_data.push(Bytes::from(packet_to_bytes(&a))));
                        video.map(|v| retrun_data.push(Bytes::from(packet_to_bytes(&v))));
                        gop.map(|gop| {
                            for g in gop {
                                retrun_data.push(Bytes::from(packet_to_bytes(&g)));
                            }
                        });
                    }

                    let packets: Vec<Bytes> = retrun_data.drain(..).collect();
                    for p in packets {
                        match body_sender.send_data(p).await {
                            Ok(_) => {}
                            Err(e) => {
                                log::error!("{}", e);
                                return;
                            }
                        }
                    }
                    while let Ok(packet) = session_receiver.recv().await {
                        match body_sender
                            .send_data(Bytes::from(packet_to_bytes(&packet)))
                            .await
                        {
                            Ok(_) => {}
                            Err(e) => {
                                log::error!("send_data err {}", e);
                                return;
                            }
                        }
                    }
                });
            }
            Err(e) => {
                log::error!("join channel  err {}", e);
                return Err(PError::ChannelJoinFailed);
            }
        }
        Ok(())
    }
}

pub fn packet_to_bytes(packet: &Packet) -> BytesMut {
    let type_id = match packet.kind {
        PacketType::Audio => 8,
        PacketType::Meta => 18,
        PacketType::Video => 9,
    };

    let data_len = packet.payload.len();
    let timestamp: u64 = match packet.timestamp {
        Some(u) => u.into(),
        None => 0,
    };

    let pre_data_len = data_len + 11;
    let timestamp_base = timestamp & 0xffffff;
    let timestamp_ext = timestamp >> 24 & 0xff;
    let mut h = [0u8; 11];

    h[0] = type_id;
    put_i24_be(&mut h[1..4], data_len as i32);
    put_i24_be(&mut h[4..7], timestamp_base as i32);
    h[7] = timestamp_ext as u8;

    let mut b = BytesMut::new();
    b.extend(&h);
    b.extend(&packet.payload);

    put_i32_be(&mut h[0..4], pre_data_len as i32);
    b.extend(&h[0..4]);
    b
}
