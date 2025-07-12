pub mod auth;
mod connection;
mod packet;
pub mod rate_limiter;
mod rtmp;
pub mod service;

mod channel;
pub mod config;
mod error;
pub mod errors;
pub mod health;
mod manager;
pub mod metrics;
pub mod transport;
pub mod user;

#[cfg(feature = "flv")]
pub mod flv;

#[cfg(feature = "http-flv")]
pub mod http_flv;

#[cfg(feature = "hls")]
pub mod hls;
#[cfg(feature = "hls")]
pub mod hls_manager;
#[cfg(feature = "hls")]
mod transport_stream;
#[cfg(feature = "hls")]
pub mod ts;

#[cfg(feature = "hls")]
pub mod mq_sender;

mod codec;
type Event = &'static str;
type AppName = String;
type StreamKey = String;

use std::{path::Path, fs};

use anyhow::{bail, Result};

pub use self::{
    manager::Manager,
    transport::{trigger_channel, ChannelMessage, Handle, ManagerHandle, Message, Watcher},
};

const FLV_HEADER: [u8; 13] = [
    0x46, 0x4c, 0x56, 0x01, 0x05, 0x00, 0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0x00,
];
fn put_i24_be(b: &mut [u8], v: i32) {
    b[0] = (v >> 16) as u8;
    b[1] = (v >> 8) as u8;
    b[2] = v as u8;
}

fn put_i32_be(b: &mut [u8], v: i32) {
    b[0] = (v >> 24) as u8;
    b[1] = (v >> 16) as u8;
    b[2] = (v >> 8) as u8;
    b[3] = v as u8;
}


fn prepare_stream_directory<P: AsRef<Path>>(path: P) -> Result<()> {
    let stream_path = path.as_ref();
    if stream_path.exists() && !stream_path.is_dir() {
        bail!(
            "Path '{}' exists, but is not a directory",
            stream_path.display()
        );
    }
    log::debug!("Creating HLS directory at '{}'", stream_path.display());
    fs::create_dir_all(&stream_path)?;
    Ok(())
}
