pub mod error;
pub mod tag;
pub mod writer;

pub use {
    tag::audio, tag::audio::AudioData, tag::video::AvcPacketType, tag::video::Codec,
    tag::video::VideoData,
};
