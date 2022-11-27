use crate::codec::flv::{audio::AudioFormat::Aac, AudioData, VideoData};
use crate::packet::{Packet, PacketType};
use crate::transport::{IncomingBroadcast, Message, OutgoingBroadcast};
use anyhow::Result;
#[cfg(feature = "keyframe_image")]
use chrono::prelude::*;
use std::convert::TryFrom;

#[cfg(feature = "keyframe_image")]
use {
    crate::codec::avc::{self, AvcCoder},
    crate::codec::FormatReader,
    crate::codec::FormatWriter,
};
#[cfg(feature = "keyframe_image")]
use {pic::video_decode, std::fs};

pub struct Channel {
    name: String,
    incoming: IncomingBroadcast,
    outgoing: OutgoingBroadcast,
    metadata: Option<Packet>,
    video_seq_header: Option<Packet>,
    audio_seq_header: Option<Packet>,
    gop: Option<Vec<Packet>>,
    closing: bool,
    full_gop: bool,
    #[cfg(feature = "keyframe_image")]
    coder: AvcCoder,
}

impl Channel {
    pub fn new(
        name: String,
        incoming: IncomingBroadcast,
        outgoing: OutgoingBroadcast,
        full_gop: bool,
    ) -> Self {
        Self {
            name,
            incoming,
            outgoing,
            metadata: None,
            video_seq_header: None,
            audio_seq_header: None,
            gop: None,
            closing: false,
            full_gop,
            #[cfg(feature = "keyframe_image")]
            coder: AvcCoder::new(),
        }
    }

    pub async fn run(mut self) {
        while !self.closing {
            if let Some(message) = self.incoming.recv().await {
                self.handle_message(message).await;
            }
        }
    }

    async fn handle_message(&mut self, message: Message) {
        match message {
            Message::Packet(packet) => {
                if let Err(e) = self.set_cache(&packet) {
                    log::error!("Failed to set channel cache {}", e);
                }
                self.broadcast_packet(packet);
            }
            Message::InitData(responder) => {
                let response = (
                    self.metadata.clone(),
                    self.video_seq_header.clone(),
                    self.audio_seq_header.clone(),
                    self.gop.clone(),
                );
                if responder.send(response).is_err() {
                    log::error!("Failed to send init data");
                }
            }
            Message::Disconnect => {
                self.closing = true;
            }
        }
    }

    fn broadcast_packet(&self, packet: Packet) {
        if self.outgoing.receiver_count() != 0 && self.outgoing.send(packet).is_err() {
            log::error!("Failed to broadcast packet");
        }
    }

    fn set_cache(&mut self, packet: &Packet) -> Result<()> {
        match packet.kind {
            PacketType::Meta => {
                self.metadata = Some(packet.clone());
            }
            PacketType::Video => {
                let flv_packet = VideoData::try_from(packet.as_ref())?;
                if flv_packet.is_sequence_header() && flv_packet.is_keyframe() {
                    self.video_seq_header = Some(packet.clone());

                    #[cfg(feature = "keyframe_image")]
                    self.coder.set_dcr(flv_packet.body.as_ref())?;
                } else if !flv_packet.is_sequence_header() && flv_packet.is_keyframe() {
                    #[cfg(feature = "keyframe_image")]
                    {
                        //提取关键帧AnnexB,保持成文件，需要ffmpeg 转码成jpg（参考readme 命令）
                        let video = match self.coder.read_format(avc::Avcc, &flv_packet.body)? {
                            Some(avc) => self.coder.write_format(avc::AnnexB, avc)?,
                            None => return Ok(()),
                        };
                        let file_name =
                            format!("data/keyframe/{}_{}.jpg", self.name, Utc::now().timestamp());

                        if !pic::keyframe_to_jpg(video, file_name.clone()) {
                            log::info!("keyframe_to_jpg err {}", file_name);
                        }
                    }

                    let mut pck = vec![];
                    pck.push(packet.clone());
                    self.gop = Some(pck);
                } else if self.full_gop {
                    if let Some(ref mut v) = self.gop {
                        v.push(packet.clone());
                    }
                }
            }
            PacketType::Audio => {
                let audio_packet = AudioData::try_from(packet.as_ref())?;
                if audio_packet.is_sequence_header() && audio_packet.format == Aac {
                    self.audio_seq_header = Some(packet.clone());
                }
            }
        }
        Ok(())
    }
}

impl Drop for Channel {
    fn drop(&mut self) {
        log::info!("channel {} closed", self.name);
    }
}
