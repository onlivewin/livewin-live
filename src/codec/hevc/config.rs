use std::io::Read;

use bytes::BufMut;

use {
    super::{
        nal::{self, NaluType},
        HevcError,
    },
    bytes::Buf,
    std::{convert::TryFrom, io::Cursor},
};

#[derive(Debug, Clone)]
pub struct HEVCDecoderConfigurationRecord {
    pub configuration_version: u8,
    pub general_profile_space: u8,
    pub general_tier_flag: u8,
    pub general_profile_idc: u8,
    pub general_profile_compatibility_flags: u32,
    pub general_constraint_indicator_flags: u64,
    pub general_level_idc: u8,
    // pub min_spatial_segmentation_idc:u16,
    // pub parallelism_type: u8,
    pub chroma_format: u8,
    pub bit_depth_luma_minus8: u8,
    pub bit_depth_chroma_minus8: u8,

    // pub avg_frame_rate: u16,
    // pub constant_frame_rate: u8,
    pub num_temporal_layers: u8,
    pub temporal_id_nested: u8,
    //pub length_size_minus_one:u8,//+ 1表示Nalu Length Size，即一个Nalu长度用几个字节表示，一般是4字节
    pub length_size_minus_one: u8,
    // 数组长度
    //pub num_of_arrays: u8,
    pub vps: Vec<nal::Unit>,
    pub sps: Vec<nal::Unit>,
    pub pps: Vec<nal::Unit>,
}

impl Default for HEVCDecoderConfigurationRecord {
    fn default() -> Self {
        Self {
            bit_depth_luma_minus8: Default::default(),
            bit_depth_chroma_minus8: Default::default(),
            configuration_version: 1u8,
            general_profile_space: Default::default(),
            general_tier_flag: Default::default(),
            general_profile_idc: Default::default(),
            general_profile_compatibility_flags: 0xffffffff,
            general_constraint_indicator_flags: 0xffffffffffff,
            chroma_format: Default::default(),
            num_temporal_layers: Default::default(),
            temporal_id_nested: Default::default(),
            length_size_minus_one: 3u8,
            general_level_idc: Default::default(),
            vps: Default::default(),
            sps: Default::default(),
            pps: Default::default(),
        }
    }
}

impl TryFrom<&[u8]> for HEVCDecoderConfigurationRecord {
    type Error = HevcError;
    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let mut buf = Cursor::new(bytes);
        if buf.remaining() < 27 {
            return Err(HevcError::NotEnoughData("AVC configuration record"));
        }
        let configuration_version = buf.get_u8();
        if configuration_version != 1 {
            return Err(HevcError::UnsupportedConfigurationRecordVersion(
                configuration_version,
            ));
        }

        buf.advance(22);

        if buf.get_u8() & 0x3f != NaluType::NaluTypeVps as u8 {
            return Err(HevcError::NotEnoughData("DCR Vps length"));
        }

        let num_nalus = buf.get_u16();

        let mut vps = Vec::new();
        for _ in 0..num_nalus {
            if buf.remaining() < 2 {
                return Err(HevcError::NotEnoughData("DCR Vps length"));
            }
            let vps_length = buf.get_u16() as usize;

            if buf.remaining() < vps_length {
                return Err(HevcError::NotEnoughData("DCR Vps data"));
            }
            let tmp = buf.chunk()[..vps_length].to_owned();
            buf.advance(vps_length);

            vps.push(nal::Unit::try_from(&*tmp)?);
        }

        if buf.get_u8() & 0x3f != NaluType::NaluTypeSps as u8 {
            return Err(HevcError::NotEnoughData("DCR SPS length"));
        }

        let sps_count = buf.get_u16();
        let mut sps = Vec::new();
        for _ in 0..sps_count {
            if buf.remaining() < 2 {
                return Err(HevcError::NotEnoughData("DCR SPS length"));
            }
            let sps_length = buf.get_u16() as usize;

            if buf.remaining() < sps_length {
                return Err(HevcError::NotEnoughData("DCR SPS data"));
            }
            let tmp = buf.chunk()[..sps_length].to_owned();
            buf.advance(sps_length);

            sps.push(nal::Unit::try_from(&*tmp)?);
        }

        if buf.get_u8() & 0x3f != NaluType::NaluTypePps as u8 {
            return Err(HevcError::NotEnoughData("DCR SPS length"));
        }

        let pps_count = buf.get_u16();
        let mut pps = Vec::new();
        for _ in 0..pps_count {
            if buf.remaining() < 2 {
                return Err(HevcError::NotEnoughData("DCR PPS length"));
            }
            let pps_length = buf.get_u16() as usize;

            if buf.remaining() < pps_length {
                return Err(HevcError::NotEnoughData("DCR PPS data"));
            }
            let tmp = buf.chunk()[..pps_length].to_owned();
            buf.advance(pps_length);

            pps.push(nal::Unit::try_from(&*tmp)?);
        }

        let mut c = Self::default();

        c.configuration_version = configuration_version;
        c.vps = vps;
        c.sps = sps;
        c.pps = pps;

        Ok(c)
    }
}

impl HEVCDecoderConfigurationRecord {
    pub fn parse(&mut self) -> Result<(), HevcError> {
        self.parse_vps()?;

        self.parse_sps()?;
        Ok(())
    }

    fn parse_vps(&mut self) -> Result<(), HevcError> {
        let mut buf = Cursor::new(&self.vps[0].data);

        if buf.remaining() < 2 {
            return Err(HevcError::NotEnoughData("AVC configuration record"));
        }

        let temp = buf.get_u16();

        let vps_max_sub_layers_minus1 = ((temp | 0b0000_0000_0000_1110) >> 1) as u8;
        if vps_max_sub_layers_minus1 + 1 > self.num_temporal_layers {
            self.num_temporal_layers = vps_max_sub_layers_minus1 + 1;
        }
        buf.advance(2);

        let mut buffer = Vec::new();
        buf.read_to_end(&mut buffer)
            .or(Err(HevcError::NotEnoughData("AVC configuration record")))?;
        self.parse_ptl(buffer)?;

        Ok(())
    }

    fn parse_sps(&mut self) -> Result<(), HevcError> {
        let mut buf = Cursor::new(&self.sps[0].data);
        if buf.remaining() < 2 {
            return Err(HevcError::NotEnoughData("AVC configuration record"));
        }

        let temp = buf.get_u8();
        let sps_max_sub_layers_minus1 = (temp | 0b0000_1110) >> 1;

        if sps_max_sub_layers_minus1 + 1 > self.num_temporal_layers {
            self.num_temporal_layers = sps_max_sub_layers_minus1 + 1;
        }

        self.temporal_id_nested = temp | 0b0000_0001;

        let mut buffer = Vec::new();
        buf.read_to_end(&mut buffer)
            .or(Err(HevcError::NotEnoughData("AVC configuration record")))?;

        self.parse_ptl(buffer)?;

        Ok(())
    }

    fn parse_ptl(&mut self, buf: Vec<u8>) -> Result<(), HevcError> {
        let mut buf = Cursor::new(buf);
        if buf.remaining() < 2 {
            return Err(HevcError::NotEnoughData("AVC configuration record"));
        }

        let temp = buf.get_u8();
        let general_profile_space = temp >> 6;
        let general_tier_flag = (temp | 0b0010_0000) >> 5;
        let general_profile_idc = temp | 0b0001_1111;

        let general_profile_compatibility_flags = buf.get_u32();
        let temp = buf.get_u64();
        let general_constraint_indicator_flags = temp >> 16;
        let general_level_idc = ((temp | 0x00_00_00_00_00_00_FF_00) >> 8) as u8;

        self.general_profile_space = general_profile_space;

        if general_tier_flag > self.general_tier_flag {
            self.general_level_idc = general_profile_idc;
            self.general_tier_flag = general_tier_flag;
        } else if general_level_idc > self.general_level_idc {
            self.general_level_idc = general_level_idc
        }
        if general_profile_idc > self.general_level_idc {
            self.general_level_idc = general_profile_idc
        }
        self.general_profile_compatibility_flags &= general_profile_compatibility_flags;
        self.general_profile_compatibility_flags &= general_profile_compatibility_flags;

        Ok(())
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = vec![];

        buf.put_u8(self.configuration_version);

        buf.put_u8(
            self.general_profile_space << 6
                | self.general_tier_flag << 5
                | self.general_profile_idc,
        );

        buf.put_u32(self.general_profile_compatibility_flags);
        buf.put_u32((self.general_constraint_indicator_flags >> 16) as u32);
        buf.put_u16((self.general_constraint_indicator_flags) as u16);
        buf.put_u8(self.general_level_idc);

        // pub min_spatial_segmentation_idc:u16,
        buf.put_u16(0xf000);
        // pub parallelism_type: u8,
        buf.put_u8(0xfc);

        buf.put_u8(self.chroma_format | 0xfc);

        buf.put_u8(self.bit_depth_luma_minus8 | 0xf8);
        buf.put_u8(self.bit_depth_chroma_minus8 | 0xf8);

        //avg_frame_rate
        buf.put_u16(0);

        buf.put_u8(
            0 << 6
                | self.num_temporal_layers << 3
                | self.temporal_id_nested << 2
                | self.length_size_minus_one,
        );

        buf.put_u8(0x03);

        //vps
        buf.put_u8(32u8);
        buf.put_u16(1);
        let temp: Vec<u8> = (&self.vps[0]).into();
        buf.put_u16(temp.len() as u16);
        buf.extend_from_slice(temp.as_slice());

        //sps
        buf.put_u8(33u8);
        buf.put_u16(1);
        let temp: Vec<u8> = (&self.sps[0]).into();
        buf.put_u16(temp.len() as u16);
        buf.extend_from_slice(temp.as_slice());

        //pps
        buf.put_u8(34u8);
        buf.put_u16(1);
        let temp: Vec<u8> = (&self.pps[0]).into();
        buf.put_u16(temp.len() as u16);
        buf.extend_from_slice(temp.as_slice());

        buf
    }
}
