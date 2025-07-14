use crate::codec::aac::{self, AacCoder};
use crate::codec::avc::{self, AvcCoder};
use crate::codec::flv::{AudioData, Codec, VideoData};
use crate::codec::hevc::{self, HevcCoder};
use crate::codec::FormatReader;
use crate::codec::FormatWriter;
use crate::error::Error;
use crate::packet::{Packet, PacketType};
use crate::transport::{
    trigger_channel, ChannelMessage, ManagerHandle, TsMessageQueue, TsMessageQueueHandle, Watcher,
};
use anyhow::{bail, Result};
use chrono::prelude::*;
use std::convert::TryFrom;
use std::fs;
use std::path::{Path, PathBuf};

//static  self.ts_duration: u64 = 5;
use crate::transport_stream::{SuportCodec, TransportStream};
pub struct Writer {
    app_name: String,
    watcher: Watcher,
    ts_duration: u64, //ts_duration秒切一个ts
    next_write: u64,
    last_keyframe: u64,
    keyframe_counter: usize,
    buffer: TransportStream,
    avc_coder: AvcCoder,
    hevc_coder: HevcCoder,
    aac_coder: AacCoder,
    stream_path: PathBuf,
    mq_message_handle: TsMessageQueueHandle,
}

impl Writer {
    pub fn create(
        app_name: String,
        watcher: Watcher,
        stream_path: String,
        mq_message_handle: TsMessageQueueHandle,
        ts_duration: u64,
    ) -> Result<Self> {
        log::info!("Creating TS writer: app_name={}, stream_path={}", app_name, stream_path);
        let next_write: u64 = Utc::now().timestamp() as u64 + ts_duration; // seconds
        let stream_path = PathBuf::from(stream_path).join(app_name.clone());
        log::info!("Final stream_path: {}", stream_path.display());
        super::prepare_stream_directory(&stream_path)?;

        Ok(Self {
            app_name,
            watcher,
            ts_duration,
            next_write,
            last_keyframe: 0,
            keyframe_counter: 0,
            buffer: TransportStream::new(),
            avc_coder: AvcCoder::new(),
            aac_coder: AacCoder::new(),
            hevc_coder: HevcCoder::new(),
            stream_path,
            mq_message_handle,
        })
    }

    pub async fn run(mut self) -> Result<()> {
        use tokio::sync::broadcast::error::RecvError;
        loop {
            let packet = match self.watcher.recv().await {
                Ok(packet) => packet,
                Err(RecvError::Closed) => break,
                Err(_) => continue,
            };

            match self.handle_packet(packet) {
                Ok(_) => {}
                Err(err) => {
                    log::error!("handle_packet err {}", err);
                    break;
                }
            }
        }
        Ok(())
    }

    fn handle_video<T>(&mut self, timestamp: T, bytes: &[u8]) -> Result<()>
    where
        T: Into<u64>,
    {
        let timestamp: u64 = timestamp.into();

        let flv_packet = VideoData::try_from(bytes)?;
        let payload = &flv_packet.body;

        if flv_packet.is_sequence_header() {
            match flv_packet.codec {
                Codec::H264 => {
                    self.avc_coder.set_dcr(payload.as_ref())?;
                }
                Codec::H265 => {
                    self.hevc_coder.set_dcr(payload.as_ref())?;
                    self.buffer.set_codec(SuportCodec::H265);
                }
            }

            return Ok(());
        }

        let keyframe = flv_packet.is_keyframe();

        //  println!("{} keyframe {}",timestamp,flv_packet.is_keyframe());
        let _keyframe_duration = timestamp - self.last_keyframe;
        if keyframe {
            let current_time = Utc::now().timestamp() as u64;
            if current_time >= self.next_write {
                let ts_filename = (self.next_write - self.ts_duration) as i64;
                let filename = format!("{}.ts", ts_filename);
                let path = self.stream_path.join(&filename);
                self.buffer.write_to_file(&path)?;

                log::info!("Sending TS message: app_name={}, filename={}, duration={}",
                    self.app_name, ts_filename, self.ts_duration);

                self.mq_message_handle
                    .send(TsMessageQueue::Ts(
                        self.app_name.clone(),
                        ts_filename,
                        self.ts_duration as u8,
                    ))
                    .map_err(|_| Error::SendTsToMqErr)?;

                self.next_write = current_time + self.ts_duration;
                self.last_keyframe = timestamp;
            }
            self.keyframe_counter += 1;
        }

        match flv_packet.codec {
            Codec::H264 => {
                let video = match self.avc_coder.read_format(avc::Avcc, &payload)? {
                    Some(avc) => self.avc_coder.write_format(avc::AnnexB, avc)?,
                    None => return Ok(()),
                };

                let comp_time = flv_packet.composition_time as u64;

                if let Err(why) = self
                    .buffer
                    .push_video(timestamp, comp_time, keyframe, video)
                {
                    log::warn!("Failed to put data into buffer: {:?}", why);
                }
            }

            Codec::H265 => {
                let video = match self.hevc_coder.read_format(hevc::Hvcc, &payload)? {
                    Some(hevc) => self.hevc_coder.write_format(hevc::AnnexB, hevc)?,
                    None => return Ok(()),
                };

                let comp_time = flv_packet.composition_time as u64;

                if let Err(why) = self
                    .buffer
                    .push_video(timestamp, comp_time, keyframe, video)
                {
                    log::warn!("Failed to put data into buffer: {:?}", why);
                }
            }
        }

        Ok(())
    }

    fn handle_audio<T>(&mut self, timestamp: T, bytes: &[u8]) -> Result<()>
    where
        T: Into<u64>,
    {
        let timestamp: u64 = timestamp.into();

        let flv = AudioData::try_from(bytes).unwrap();

        if flv.is_sequence_header() {
            self.aac_coder.set_asc(flv.body.as_ref())?;
            return Ok(());
        }

        if self.keyframe_counter == 0 {
            return Ok(());
        }

        let audio = match self.aac_coder.read_format(aac::Raw, &flv.body)? {
            Some(raw_aac) => self
                .aac_coder
                .write_format(aac::AudioDataTransportStream, raw_aac)?,
            None => return Ok(()),
        };

        if let Err(why) = self.buffer.push_audio(timestamp, audio) {
            log::warn!("Failed to put data into buffer: {:?}", why);
        }

        Ok(())
    }

    fn handle_packet(&mut self, packet: Packet) -> Result<()> {
        match packet.kind {
            PacketType::Video => self.handle_video(packet.timestamp.unwrap(), packet.as_ref()),
            PacketType::Audio => self.handle_audio(packet.timestamp.unwrap(), packet.as_ref()),
            _ => Ok(()),
        }
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        //解决视频最后几秒丢失问题
        if self.buffer.size() > 0 {
            let len = Utc::now().timestamp() as u64 - (self.next_write - self.ts_duration);
            let filename = format!("{}.ts", self.next_write - self.ts_duration);
            let path = self.stream_path.join(&filename);
            _ = self.buffer.write_to_file(&path);
            _ = self
                .mq_message_handle
                .send(TsMessageQueue::Ts(
                    self.app_name.clone(),
                    (self.next_write - self.ts_duration) as i64,
                    len as u8,
                ))
                .map_err(|_| Error::SendTsToMqErr);
        }
        log::info!("Closing HLS writer for {}", self.stream_path.display());
    }
}

pub struct Service {
    manager_handle: ManagerHandle,
    ts_data_path: String,
    sender: TsMessageQueueHandle,
    ts_duration: u64,
}

impl Service {
    pub fn new(
        manager_handle: ManagerHandle,
        ts_data_path: String,
        sender: TsMessageQueueHandle,
        ts_duration: u64,
    ) -> Self {
        Self {
            manager_handle,
            ts_data_path,
            sender,
            ts_duration,
        }
    }

    pub async fn run(self) {
        let (trigger, mut trigger_handle) = trigger_channel();
        if let Err(_) = self
            .manager_handle
            .send(ChannelMessage::RegisterTrigger("create_session", trigger))
        {
            log::error!("Failed to register session trigger");
            return;
        }

        while let Some((app_name, watcher)) = trigger_handle.recv().await {
            let sender = self.sender.clone();
            match Writer::create(
                app_name,
                watcher,
                self.ts_data_path.clone(),
                sender,
                self.ts_duration,
            ) {
                Ok(writer) => {
                    tokio::spawn(async move { writer.run().await.unwrap() });
                }
                Err(why) => log::error!("Failed to create writer: {:?}", why),
            }
        }
    }
}

