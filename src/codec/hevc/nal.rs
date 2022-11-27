use {
    super::HevcError,
    bytes::{Buf, BufMut, Bytes},
    std::{convert::TryFrom, fmt, io::Cursor},
};

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord)]
pub enum NaluType {
    NaluTypeSliceTrailN = 0, // 0x0
    NaluTypeSliceTrailR = 1, // 0x01
    NaluTypeSliceTsaN = 2,   // 0x02
    NaluTypeSliceTsaR = 3,   // 0x03
    NaluTypeSliceStsaN = 4,  // 0x04
    NaluTypeSliceStsaR = 5,  // 0x05
    NaluTypeSliceRadlN = 6,  // 0x06
    NaluTypeSliceRadlR = 7,  // 0x07
    NaluTypeSliceRaslN = 8,  // 0x06
    NaluTypeSliceRaslR = 9,  // 0x09

    NaluTypeSliceBlaWlp = 16,       // 0x10
    NaluTypeSliceBlaWradl = 17,     // 0x11
    NaluTypeSliceBlaNlp = 18,       // 0x12
    NaluTypeSliceIdr = 19,          // 0x13
    NaluTypeSliceIdrNlp = 20,       // 0x14
    NaluTypeSliceCranut = 21,       // 0x15
    NaluTypeSliceRsvIrapVcl22 = 22, // 0x16
    NaluTypeSliceRsvIrapVcl23 = 23, // 0x17

    NaluTypeVps = 32,       // 0x20
    NaluTypeSps = 33,       // 0x21
    NaluTypePps = 34,       // 0x22
    NaluTypeAud = 35,       // 0x23
    NaluTypeSei = 39,       // 0x27
    NaluTypeSeiSuffix = 40, // 0x28

    NalUnitReserved41 = 41,
    NalUnitReserved42 = 42,
    NalUnitReserved43 = 43,
    NalUnitReserved44 = 44,
    NalUnitReserved45 = 45,
    NalUnitReserved46 = 46,
    NalUnitReserved47 = 47,
    NalUnitUnspecified48 = 48,
    NalUnitUnspecified49 = 49,
    NalUnitUnspecified50 = 50,
    NalUnitUnspecified51 = 51,
    NalUnitUnspecified52 = 52,
    NalUnitUnspecified53 = 53,
    NalUnitUnspecified54 = 54,
    NalUnitUnspecified55 = 55,
    NalUnitUnspecified56 = 56,
    NalUnitUnspecified57 = 57,
    NalUnitUnspecified58 = 58,
    NalUnitUnspecified59 = 59,
    NalUnitUnspecified60 = 60,
    NalUnitUnspecified61 = 61,
    NalUnitUnspecified62 = 62,
    NalUnitUnspecified63 = 63,
}

impl NaluType {
    pub fn to_string(&self) -> &'static str {
        match self {
            NaluType::NaluTypeSliceTrailN => "TrailN",
            NaluType::NaluTypeSliceTrailR => "TrailR",
            NaluType::NaluTypeSliceTsaN => "TsaN",
            NaluType::NaluTypeSliceTsaR => "TsaR",
            NaluType::NaluTypeSliceStsaN => "StsaN",
            NaluType::NaluTypeSliceStsaR => "StsaR",
            NaluType::NaluTypeSliceRadlN => "RadlN",
            NaluType::NaluTypeSliceRadlR => "RadlR",
            NaluType::NaluTypeSliceRaslN => "RaslN",
            NaluType::NaluTypeSliceRaslR => "RaslR",
            NaluType::NaluTypeSliceBlaWlp => "BlaWlp",
            NaluType::NaluTypeSliceBlaWradl => "BlaWradl",
            NaluType::NaluTypeSliceBlaNlp => "BlaNlp",
            NaluType::NaluTypeSliceIdr => "IDR",
            NaluType::NaluTypeSliceIdrNlp => "IDRNLP",
            NaluType::NaluTypeSliceCranut => "CRANUT",
            NaluType::NaluTypeSliceRsvIrapVcl22 => "IrapVcl22",
            NaluType::NaluTypeSliceRsvIrapVcl23 => "IrapVcl23",
            NaluType::NaluTypeVps => "VPS",
            NaluType::NaluTypeSps => "SPS",
            NaluType::NaluTypePps => "PPS",
            NaluType::NaluTypeAud => "AUD",
            NaluType::NaluTypeSei => "SEI",
            NaluType::NaluTypeSeiSuffix => "SEISuffix",
            _ => "other",
        }
    }
}

impl TryFrom<u8> for NaluType {
    type Error = HevcError;
    fn try_from(val: u8) -> Result<Self, Self::Error> {
        Ok(match val {
            0 => NaluType::NaluTypeSliceTrailN,        // 0x0
            1 => NaluType::NaluTypeSliceTrailR,        // 0x01
            2 => NaluType::NaluTypeSliceTsaN,          // 0x02
            3 => NaluType::NaluTypeSliceTsaR,          // 0x03
            4 => NaluType::NaluTypeSliceStsaN,         // 0x04
            5 => NaluType::NaluTypeSliceStsaR,         // 0x05
            6 => NaluType::NaluTypeSliceRadlN,         // 0x06
            7 => NaluType::NaluTypeSliceRadlR,         // 0x07
            8 => NaluType::NaluTypeSliceRaslN,         // 0x06
            9 => NaluType::NaluTypeSliceRaslR,         // 0x09,
            16 => NaluType::NaluTypeSliceBlaWlp,       // 0x10
            17 => NaluType::NaluTypeSliceBlaWradl,     // 0x11
            18 => NaluType::NaluTypeSliceBlaNlp,       // 0x12
            19 => NaluType::NaluTypeSliceIdr,          // 0x13
            20 => NaluType::NaluTypeSliceIdrNlp,       // 0x14
            21 => NaluType::NaluTypeSliceCranut,       // 0x15
            22 => NaluType::NaluTypeSliceRsvIrapVcl22, // 0x16
            23 => NaluType::NaluTypeSliceRsvIrapVcl23, // 0x17

            32 => NaluType::NaluTypeVps,       // 0x20
            33 => NaluType::NaluTypeSps,       // 0x21
            34 => NaluType::NaluTypePps,       // 0x22
            35 => NaluType::NaluTypeAud,       // 0x23
            39 => NaluType::NaluTypeSei,       // 0x27
            40 => NaluType::NaluTypeSeiSuffix, // 0x28

            41 => NaluType::NalUnitReserved41,
            42 => NaluType::NalUnitReserved42,
            43 => NaluType::NalUnitReserved43,
            44 => NaluType::NalUnitReserved44,
            45 => NaluType::NalUnitReserved45,
            46 => NaluType::NalUnitReserved46,
            47 => NaluType::NalUnitReserved47,
            48 => NaluType::NalUnitUnspecified48,
            49 => NaluType::NalUnitUnspecified49,
            50 => NaluType::NalUnitUnspecified50,
            51 => NaluType::NalUnitUnspecified51,
            52 => NaluType::NalUnitUnspecified52,
            53 => NaluType::NalUnitUnspecified53,
            54 => NaluType::NalUnitUnspecified54,
            55 => NaluType::NalUnitUnspecified55,
            56 => NaluType::NalUnitUnspecified56,
            57 => NaluType::NalUnitUnspecified57,
            58 => NaluType::NalUnitUnspecified58,
            59 => NaluType::NalUnitUnspecified59,
            60 => NaluType::NalUnitUnspecified60,
            61 => NaluType::NalUnitUnspecified61,
            62 => NaluType::NalUnitUnspecified62,
            63 => NaluType::NalUnitUnspecified63,
            _ => return Err(HevcError::UnsupportedNalUnitType(val)),
        })
    }
}

/// Network Abstraction Layer Unit (aka NALU) of a H.265 bitstream.
#[derive(Clone, PartialEq, Eq)]
pub struct Unit {
    pub header: u16,
    pub kind: NaluType,
    pub data: Bytes, // Raw Byte Sequence Payload (RBSP)
}

impl Unit {
    pub fn is_keyframe(&self) -> bool {
        matches!(
            &self.kind,
            NaluType::NaluTypeSliceBlaWlp|
            NaluType::NaluTypeSliceBlaWradl|
            NaluType::NaluTypeSliceBlaNlp|
            NaluType::NaluTypeSliceIdr |
            NaluType::NaluTypeSliceIdrNlp |
            NaluType::NaluTypeSliceCranut |
            NaluType::NaluTypeSliceRsvIrapVcl22| // 0x16
            NaluType::NaluTypeSliceRsvIrapVcl23
        )
    }

    pub fn payload(&self) -> &[u8] {
        &self.data
    }
}

impl TryFrom<&[u8]> for Unit {
    type Error = HevcError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let mut buf = Cursor::new(bytes);
        let header = buf.get_u16();
        let kind = NaluType::try_from(((header >> 8) as u8 & 0x7E) >> 1)?;
        let data = buf.copy_to_bytes(bytes.len() - 2);
        Ok(Self { header, kind, data })
    }
}

impl From<&Unit> for Vec<u8> {
    fn from(val: &Unit) -> Self {
        let mut tmp = Vec::with_capacity(val.data.len() + 2);
        tmp.put_u16(val.header);
        // tmp.put(val.data.clone());
        tmp.extend_from_slice(&val.data);
        tmp
    }
}

impl From<Unit> for Vec<u8> {
    fn from(val: Unit) -> Self {
        Self::from(&val)
    }
}

impl fmt::Debug for Unit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Unit").field("kind", &self.kind).finish()
    }
}
