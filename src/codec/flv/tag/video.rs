use bytes::BufMut;

use {
    crate::codec::flv::error::FlvError,
    bytes::{Buf, Bytes},
    std::{
        convert::{TryFrom, TryInto},
        fmt::{self, Debug},
        io::{Cursor, Read},
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum FrameType {
    KeyFrame,
    InterFrame,
    DisposableInterFrame,
    GeneratedKeyframe,
    VideoInfoFrame,
}

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum Codec {
    H264,
    H265,
}

impl TryFrom<u8> for FrameType {
    type Error = FlvError;

    fn try_from(val: u8) -> Result<Self, Self::Error> {
        Ok(match val {
            1 => Self::KeyFrame,
            2 => Self::InterFrame,
            3 => Self::DisposableInterFrame,
            4 => Self::GeneratedKeyframe,
            5 => Self::VideoInfoFrame,
            x => return Err(FlvError::UnknownFrameType(x)),
        })
    }
}

impl TryInto<u8> for FrameType {
    type Error = FlvError;
    fn try_into(self) -> Result<u8, Self::Error> {
        Ok(match self {
            Self::KeyFrame => 1u8,
            Self::InterFrame => 2u8,
            Self::DisposableInterFrame => 3u8,
            Self::GeneratedKeyframe => 4u8,
            Self::VideoInfoFrame => 5u8,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub enum AvcPacketType {
    SequenceHeader,
    NalUnit,
    EndOfSequence,
    None,
}

impl TryFrom<u8> for AvcPacketType {
    type Error = FlvError;

    fn try_from(val: u8) -> Result<Self, Self::Error> {
        Ok(match val {
            0 => Self::SequenceHeader,
            1 => Self::NalUnit,
            2 => Self::EndOfSequence,
            x => return Err(FlvError::UnknownPackageType(x)),
        })
    }
}

impl TryInto<u8> for AvcPacketType {
    type Error = FlvError;
    fn try_into(self) -> Result<u8, Self::Error> {
        Ok(match self {
            Self::SequenceHeader => 0,
            Self::NalUnit => 1,
            Self::EndOfSequence => 2,
            Self::None => return Err(FlvError::NotEnoughData("unknow avc")),
        })
    }
}

// Field                | Type
// -------------------- | ---
// Frame Type           | u4
// Codec ID             | u4
// AVC Packet Type      | u8
// Composition Time     | i24
// Body                 | [u8]
#[derive(Clone)]
pub struct VideoData {
    pub frame_type: FrameType,
    pub packet_type: AvcPacketType,
    pub composition_time: i32,
    pub codec: Codec,
    pub body: Bytes,
}

impl VideoData {
    pub fn is_sequence_header(&self) -> bool {
        self.packet_type == AvcPacketType::SequenceHeader
    }

    pub fn is_keyframe(&self) -> bool {
        self.frame_type == FrameType::KeyFrame
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        let mut a = vec![];
        let ft: u8 = self.frame_type.try_into().unwrap();
        let temp: u8 = match self.codec {
            Codec::H264 => ft << 4 | 7u8,
            Codec::H265 => ft << 4 | 12u8,
        };
        a.put_u8(temp);
        let pt: u8 = self.packet_type.try_into().unwrap();
        let t = self.composition_time as u32 | (pt as u32) << 24;
        a.put_u32(t);

        a.extend_from_slice(&self.body);
        a
    }
}

impl Debug for VideoData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Video")
            .field("frame_type", &self.frame_type)
            .field("packet_type", &self.packet_type)
            .field("composition_time", &self.composition_time)
            .finish()
    }
}

impl TryFrom<&[u8]> for VideoData {
    type Error = FlvError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        if bytes.len() < 5 {
            return Err(FlvError::NotEnoughData("FLV Video Tag header"));
        }

        let mut buf = Cursor::new(bytes);
        let header_a = buf.get_u8();
        let codec_id = header_a & 0x0F;
        //h264 h265
        // println!("{}",codec_id);
        if codec_id != 7 && codec_id != 12 {
            return Err(FlvError::UnsupportedVideoFormat(codec_id));
        }

        let mut codec = Codec::H264;

        if codec_id == 12 {
            codec = Codec::H265;
        }

        let frame_type = FrameType::try_from(header_a >> 4)?;
        let header_b = buf.get_u32();
        let packet_type = AvcPacketType::try_from((header_b >> 24) as u8)?;
        let composition_time = (header_b & 0x00_FF_FF_FF) as i32;

        let mut remaining = Vec::new();
        buf.read_to_end(&mut remaining)?;
        Ok(Self {
            frame_type,
            packet_type,
            composition_time,
            body: remaining.into(),
            codec,
        })
    }
}
