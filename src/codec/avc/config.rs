use {
    super::{nal, AvcError},
    bytes::{Buf, BufMut},
    std::{convert::TryFrom, io::Cursor},
};

// Bits | Name
// ---- | ----
// 8    | Version
// 8    | Profile Indication
// 8    | Profile Compatability
// 8    | Level Indication
// 6    | Reserved
// 2    | NALU Length
// 3    | Reserved
// 5    | SPS Count
// 16   | SPS Length
// var  | SPS
// 8    | PPS Count
// 16   | PPS Length
// var  | PPS
#[derive(Debug, Clone)]
pub struct DecoderConfigurationRecord {
    pub version: u8,
    pub profile_indication: u8,
    pub profile_compatability: u8,
    pub level_indication: u8,
    pub nalu_size: u8,
    pub sps: Vec<nal::Unit>,
    pub pps: Vec<nal::Unit>,
}

impl Default for DecoderConfigurationRecord {
    fn default() -> Self {
        Self {
            version: 1u8,
            profile_indication: 0u8,
            profile_compatability: 0u8,
            level_indication: 0u8,
            nalu_size: 4u8,
            sps: vec![],
            pps: vec![],
        }
    }
}

impl DecoderConfigurationRecord {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = vec![];

        buf.put_u8(self.version);
        buf.put_u8(self.profile_indication);
        buf.put_u8(self.profile_compatability);
        buf.put_u8(self.level_indication);
        buf.put_u8(0xFF); //4-1

        buf.put_u8(0b11100001); // sps count 1
        let sps: Vec<u8> = self.sps.first().unwrap().into();
        buf.put_u16(sps.len() as u16);

        buf.extend(sps);

        buf.put_u8(1);
        let pps: Vec<u8> = self.pps.first().unwrap().into();
        buf.put_u16(pps.len() as u16);
        buf.extend(pps);
        buf
    }

    pub fn parse(&mut self) -> Result<(), AvcError> {
        let sps_t = Sps::new(&self.sps.first().unwrap().payload());
        self.profile_indication = sps_t.profile_idc; //sps
        self.level_indication = sps_t.level_idc; //sps
        Ok(())
    }
}

impl TryFrom<&[u8]> for DecoderConfigurationRecord {
    type Error = AvcError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        // FIXME: add checks before accessing buf, otherwise could panic
        let mut buf = Cursor::new(bytes);

        if buf.remaining() < 7 {
            return Err(AvcError::NotEnoughData("AVC configuration record"));
        }

        let version = buf.get_u8();
        if version != 1 {
            return Err(AvcError::UnsupportedConfigurationRecordVersion(version));
        }

        let profile_indication = buf.get_u8();
        let profile_compatability = buf.get_u8();
        let level_indication = buf.get_u8();
        let nalu_size = (buf.get_u8() & 0x03) + 1;

        let sps_count = buf.get_u8() & 0x1F;
        let mut sps = Vec::new();
        for _ in 0..sps_count {
            if buf.remaining() < 2 {
                return Err(AvcError::NotEnoughData("DCR SPS length"));
            }
            let sps_length = buf.get_u16() as usize;

            if buf.remaining() < sps_length {
                return Err(AvcError::NotEnoughData("DCR SPS data"));
            }
            let tmp = buf.chunk()[..sps_length].to_owned();
            buf.advance(sps_length);

            sps.push(nal::Unit::try_from(&*tmp)?);
        }

        let pps_count = buf.get_u8();
        let mut pps = Vec::new();
        for _ in 0..pps_count {
            if buf.remaining() < 2 {
                return Err(AvcError::NotEnoughData("DCR PPS length"));
            }
            let pps_length = buf.get_u16() as usize;

            if buf.remaining() < pps_length {
                return Err(AvcError::NotEnoughData("DCR PPS data"));
            }
            let tmp = buf.chunk()[..pps_length].to_owned();
            buf.advance(pps_length);

            pps.push(nal::Unit::try_from(&*tmp)?);
        }

        Ok(Self {
            version,
            profile_indication,
            profile_compatability,
            level_indication,
            nalu_size,
            sps,
            pps,
        })
    }
}

impl DecoderConfigurationRecord {
    pub fn ready(&self) -> bool {
        !self.sps.is_empty() && !self.pps.is_empty()
    }
}

struct Sps {
    profile_idc: u8,
    level_idc: u8,
}

impl Sps {
    fn new(bytes: &[u8]) -> Self {
        let mut buf = Cursor::new(bytes);

        // if buf.remaining() < 5 {

        // }
        assert!(buf.remaining() >= 5);
        let profile_idc = buf.get_u8();
        buf.advance(1);
        let level_idc = buf.get_u8();
        Self {
            profile_idc,
            level_idc,
        }
    }
}
