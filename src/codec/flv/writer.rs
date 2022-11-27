use crate::packet::{Packet, PacketType};
use crate::{put_i24_be, put_i32_be, FLV_HEADER};
use std::path::Path;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

pub struct Writer {
    file: File,
}

impl Writer {
    pub async fn new<P: AsRef<Path>>(path: P) -> std::io::Result<Self> {
        let mut file = File::create(path).await?;
        file.write(&FLV_HEADER).await?;
        Ok(Self { file })
    }

    pub async fn write(&mut self, packet: &Packet) -> std::io::Result<()> {
        let type_id = match packet.kind {
            PacketType::Audio => 8,
            PacketType::Meta => {
                //@todo
                18
            }
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

        //这边需要使用write_all write可能数据没写完整
        self.file.write_all(&h).await?;
        self.file.write_all(&packet.payload).await?;

        put_i32_be(&mut h[0..4], pre_data_len as i32);
        self.file.write_all(&h[0..4]).await?;

        Ok(())
    }
}
