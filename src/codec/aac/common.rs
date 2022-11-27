use {super::AacError, std::convert::TryFrom};

#[derive(Debug, Clone, Copy)]
pub struct SamplingFrequencyIndex(u8);

impl From<SamplingFrequencyIndex> for u8 {
    fn from(val: SamplingFrequencyIndex) -> Self {
        val.0
    }
}

impl TryFrom<u8> for SamplingFrequencyIndex {
    type Error = AacError;

    fn try_from(val: u8) -> Result<Self, AacError> {
        match val {
            0..=12 | 15 => Ok(Self(val)),
            _ => Err(AacError::UnsupportedFrequencyIndex(val)),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ChannelConfiguration(u8);

impl From<ChannelConfiguration> for u8 {
    fn from(val: ChannelConfiguration) -> Self {
        val.0
    }
}

impl TryFrom<u8> for ChannelConfiguration {
    type Error = AacError;

    fn try_from(val: u8) -> Result<Self, AacError> {
        match val {
            0..=7 => Ok(Self(val)),
            _ => Err(AacError::UnsupportedChannelConfiguration(val)),
        }
    }
}

// See [MPEG-4 Audio Object Types][audio_object_types]
//
// [audio_object_types]: https://en.wikipedia.org/wiki/MPEG-4_Part_3#MPEG-4_Audio_Object_Types
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum AudioObjectType {
    Reserved = 0,
    AacMain = 1,
    AacLowComplexity = 2,
    AacScalableSampleRate = 3,
    AacLongTermPrediction = 4,
}

impl Default for AudioObjectType {
    fn default() -> Self {
        Self::Reserved
    }
}

impl TryFrom<u8> for AudioObjectType {
    type Error = AacError;

    fn try_from(value: u8) -> Result<Self, AacError> {
        Ok(match value {
            1 => Self::AacMain,
            2 => Self::AacLowComplexity,
            3 => Self::AacScalableSampleRate,
            4 => Self::AacLongTermPrediction,
            0 => Self::Reserved,
            _ => return Err(AacError::UnsupportedAudioFormat),
        })
    }
}

impl Into<u8> for AudioObjectType {
    fn into(self) -> u8 {
        match self {
            Self::AacMain => 1,
            Self::AacLowComplexity => 2,
            Self::AacScalableSampleRate => 3,
            Self::AacLongTermPrediction => 4,
            Self::Reserved => 0,
        }
    }
}
