use super::common::AudioObjectType;

#[derive(Debug, Clone)]
pub enum AacProfile {
    AacProfileReserved = 3,

    // @see 7.1 Profiles, ISO_IEC_13818-7-AAC-2004.pdf, page 40
    AacProfileMain = 0,
    AacProfileLC = 1,
    AacProfileSSR = 2,
}

impl Default for AacProfile {
    fn default() -> Self {
        AacProfile::AacProfileReserved
    }
}

impl From<u8> for AacProfile {
    fn from(u: u8) -> Self {
        match u {
            3u8 => Self::AacProfileReserved,
            0u8 => Self::AacProfileMain,
            1u8 => Self::AacProfileLC,
            2u8 => Self::AacProfileSSR,
            _ => Self::AacProfileReserved,
        }
    }
}

impl Into<AudioObjectType> for AacProfile {
    fn into(self) -> AudioObjectType {
        match self {
            Self::AacProfileMain => AudioObjectType::AacMain,
            Self::AacProfileLC => AudioObjectType::AacLowComplexity,
            Self::AacProfileSSR => AudioObjectType::AacScalableSampleRate,
            _ => AudioObjectType::Reserved,
        }
    }
}

pub struct RawAacStreamCodec {
    // Codec level informations.
    pub protection_absent: u8,
    pub aac_object: AudioObjectType,
    pub sampling_frequency_index: u8,
    pub channel_configuration: u8,
    pub frame_length: u16,
    // Format level, RTMP as such, informations.
    pub sound_format: u8,
    pub sound_rate: u8,
    pub sound_size: u8,
    pub sound_type: u8,
    // 0 for sh; 1 for raw data.
    pub aac_packet_type: u8,
}

impl Default for RawAacStreamCodec {
    fn default() -> Self {
        Self {
            protection_absent: Default::default(),
            aac_object: Default::default(),
            sampling_frequency_index: Default::default(),
            channel_configuration: Default::default(),
            frame_length: Default::default(),
            sound_format: Default::default(),
            sound_rate: Default::default(),
            sound_size: Default::default(),
            sound_type: Default::default(),
            aac_packet_type: Default::default(),
        }
    }
}
