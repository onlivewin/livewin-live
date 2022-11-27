use crate::packet::{Packet, PacketType};
use crate::rtmp::{Event, Protocol};
use crate::{error::Error as PError, ChannelMessage, Handle, ManagerHandle, Message, Watcher};
use anyhow::Result;
use futures::SinkExt;
use log;
use std::time::Duration;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{mpsc, oneshot},
    time::timeout,
};
use tokio_stream::StreamExt;
use tokio_util::codec::{BytesCodec, Framed};
type ReturnQueue<P> = (mpsc::UnboundedSender<P>, mpsc::UnboundedReceiver<P>);
const TIME_OUT: std::time::Duration = Duration::from_secs(5);

enum State {
    Initializing,
    Publishing(Handle),
    Playing(Handle, Watcher),
    Disconnecting,
}

pub struct Connection<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    id: u64,
    bytes_stream: Framed<S, BytesCodec>,
    manager_handle: ManagerHandle,
    return_queue: ReturnQueue<Packet>,
    proto: Protocol,
    app_name: Option<String>,
    state: State,
}

impl<S> Connection<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    pub fn new(id: u64, stream: S, manager_handle: ManagerHandle) -> Self {
        Self {
            id,
            bytes_stream: Framed::new(stream, BytesCodec::new()),
            manager_handle,
            return_queue: mpsc::unbounded_channel(),
            proto: Protocol::new(),
            app_name: None,
            state: State::Initializing,
        }
    }

    pub async fn run(mut self) -> Result<()> {
        loop {
            while let Ok(packet) = self.return_queue.1.try_recv() {
                if self.handle_return_packet(packet).await.is_err() {
                    self.disconnect()?
                }
            }

            match &mut self.state {
                State::Initializing | State::Publishing(_) => {
                    let val = self.bytes_stream.try_next();
                    match timeout(TIME_OUT, val).await? {
                        Ok(Some(data)) => {
                            for event in self.proto.handle_bytes(&data)? {
                                self.handle_event(event).await?;
                            }
                        }
                        _ => self.disconnect()?,
                    }
                }
                State::Playing(_, watcher) => {
                    use tokio::sync::broadcast::error::RecvError;
                    match watcher.recv().await {
                        Ok(packet) => match packet.kind {
                            PacketType::Meta => self.send_back(packet)?,
                            PacketType::Video => self.send_back(packet)?,
                            PacketType::Audio => self.send_back(packet)?,
                        },
                        Err(RecvError::Closed) => self.disconnect()?,
                        Err(_) => (),
                    }
                }
                State::Disconnecting => {
                    log::debug!("Disconnecting...");
                    return Ok(());
                }
            }
        }
    }

    async fn handle_return_packet(&mut self, packet: Packet) -> Result<()> {
        let bytes = match packet.kind {
            PacketType::Meta => self.proto.pack_metadata(packet)?,
            PacketType::Video => self.proto.pack_video(packet)?,
            PacketType::Audio => self.proto.pack_audio(packet)?,
        };
        let res = timeout(TIME_OUT, self.bytes_stream.send(bytes.into())).await?;
        Ok(res?)
    }

    async fn handle_event(&mut self, event: Event) -> Result<()> {
        match event {
            Event::ReturnData(data) => {
                self.bytes_stream
                    .send(data)
                    .await
                    .expect("Failed to return data");
            }
            Event::SendPacket(packet) => {
                if let State::Publishing(session) = &mut self.state {
                    session
                        .send(Message::Packet(packet))
                        .map_err(|_| PError::ChannelSendFailed)?;
                }
            }
            Event::AcquireChannel {
                app_name,
                stream_key,
            } => {
                self.app_name = Some(app_name.clone());
                let (request, response) = oneshot::channel();
                self.manager_handle
                    .send(ChannelMessage::Create((app_name, stream_key, request)))
                    .map_err(|_| PError::ChannelCreationFailed)?;
                let session_sender = response.await.map_err(|_| PError::ChannelCreationFailed)?;
                self.state = State::Publishing(session_sender);
            }
            Event::JoinChannel { app_name, .. } => {
                let (request, response) = oneshot::channel();
                self.manager_handle
                    .send(ChannelMessage::Join((app_name, request)))
                    .map_err(|_| PError::ChannelJoinFailed)?;

                match response.await {
                    Ok((session_sender, session_receiver)) => {
                        self.state = State::Playing(session_sender, session_receiver);
                    }
                    Err(_) => self.disconnect()?,
                }
            }
            Event::SendInitData { .. } => {
                if let State::Playing(session, _) = &mut self.state {
                    let (request, response) = oneshot::channel();
                    session
                        .send(Message::InitData(request))
                        .map_err(|_| PError::ChannelSendFailed)?;
                    //这边可能出现一致性错误,可能掉帧
                    if let Ok((meta, video, audio, gop)) = response.await {
                        meta.map(|m| self.send_back(m));
                        video.map(|v| self.send_back(v));
                        audio.map(|a| self.send_back(a));
                        gop.map(|gop| {
                            for g in gop {
                                match self.send_back(g) {
                                    Ok(_) => {}
                                    Err(e) => {
                                        log::error!("{}", e);
                                        _ = self.disconnect();
                                    }
                                }
                            }
                        });
                    }
                }
            }
            Event::ReleaseChannel | Event::LeaveChannel => self.disconnect()?,
        }
        Ok(())
    }

    fn send_back(&mut self, packet: Packet) -> Result<(), PError> {
        self.return_queue
            .0
            .send(packet)
            .map_err(|_| PError::ReturnPacketFailed(self.id))
    }

    fn disconnect(&mut self) -> Result<(), PError> {
        if let State::Publishing(session) = &mut self.state {
            let app_name = self.app_name.clone().unwrap();
            session
                .send(Message::Disconnect)
                .map_err(|_| PError::ChannelSendFailed)?;

            self.manager_handle
                .send(ChannelMessage::Release(app_name))
                .map_err(|_| PError::ChannelReleaseFailed)?;
        }
        self.state = State::Disconnecting;
        Ok(())
    }
}

impl<S> Drop for Connection<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    fn drop(&mut self) {
        log::info!("Client {} disconnected", self.id);
    }
}
