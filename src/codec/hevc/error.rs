use thiserror::Error;

#[derive(Debug, Error)]
pub enum HevcError {
    #[error("Failed to initialize the HEVC decoder")]
    DecoderInitializationFailed,

    #[error("Hevc coder not initialized")]
    NotInitialized,

    #[error("Not enough data: {0}")]
    NotEnoughData(&'static str),

    #[error("Unsupported configuration record version {0}")]
    UnsupportedConfigurationRecordVersion(u8),

    #[error("Unsupported or unknown NAL unit type {0}")]
    UnsupportedNalUnitType(u8),
}
