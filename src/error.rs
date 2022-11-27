use std::io;
use thiserror::Error;
use tokio::time::error::Elapsed;

#[warn(dead_code)]
#[derive(Error, Debug)]
pub enum Error {
    #[error("No stream with name {0} found")]
    NoSuchStream(String),

    #[error("Client disconnected: {0}")]
    Disconnected(#[from] io::Error),

    #[error("Failed to create new channel")]
    ChannelCreationFailed,

    #[error("Failed to release channel")]
    ChannelReleaseFailed,

    #[error("Failed to join channel")]
    ChannelJoinFailed,

    #[error("Failed to send to channel")]
    ChannelSendFailed,

    #[error("Failed to return packet to peer {0}")]
    ReturnPacketFailed(u64),

    //#[error(transparent)]
    //ProtocolError(#[from] ProtocolError),
    #[error("Connection timeout")]
    ConnectionTimeout(#[from] Elapsed),

    #[error("RTMP handshake failed")]
    HandshakeFailed,

    #[error("RTMP channel initialization failed")]
    ChannelInitializationFailed,

    #[error("Tried to use RTMP channel while not initialized")]
    ChannelNotInitialized,

    #[error("Received invalid input")]
    InvalidInput,

    #[error("RTMP request was not accepted")]
    RequestRejected,

    #[error("No stream ID")]
    NoStreamId,

    #[error("Application name cannot be empty")]
    EmptyAppName,

    #[error("Http-flv app name error")]
    HttpFlvAppNameErr,

    #[error("send ts message to redis failed")]
    SendTsToMqErr,
}
