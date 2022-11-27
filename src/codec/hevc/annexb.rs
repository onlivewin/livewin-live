use std::convert::TryFrom;

use super::{ReadFormat, WriteFormat};
use crate::codec::hevc::{config::HEVCDecoderConfigurationRecord, error::HevcError, nal, Hevc};
use log::info;
pub struct AnnexB;

impl AnnexB {
    const DELIMITER1: &'static [u8] = &[0x00, 0x00, 0x01];
    const DELIMITER2: &'static [u8] = &[0x00, 0x00, 0x00, 0x01];
    //hevc aud nalu
    const ACCESS_UNIT_DELIMITER: &'static [u8] = &[0x00, 0x00, 0x00, 0x01, 0x46, 0x01, 0x50];
}

impl WriteFormat<Hevc> for AnnexB {
    type Context = HEVCDecoderConfigurationRecord;
    type Error = HevcError;

    fn write_format(&self, input: Hevc, ctx: &Self::Context) -> Result<Vec<u8>, Self::Error> {
        let mut out_buffer = Vec::new();
        let mut aud_appended = false;
        let mut vps_sps_pps_appended = false;
        let nalus: Vec<nal::Unit> = input.into();

        for nalu in nalus {
            use nal::NaluType::*;
            match &nalu.kind {
                NaluTypeVps | NaluTypeSps | NaluTypePps | NaluTypeAud => continue,
                NaluTypeSliceBlaWlp
                | NaluTypeSliceBlaWradl
                | NaluTypeSliceBlaNlp
                | NaluTypeSliceIdr
                | NaluTypeSliceIdrNlp
                | NaluTypeSliceCranut
                | NaluTypeSliceRsvIrapVcl22
                | NaluTypeSliceRsvIrapVcl23 => {
                    // info!("key frame  {:?}",ctx);
                    if !aud_appended {
                        out_buffer.extend(Self::ACCESS_UNIT_DELIMITER);
                        aud_appended = true;
                    }
                    if !vps_sps_pps_appended {
                        if let Some(vps) = ctx.vps.first() {
                            out_buffer.extend(Self::DELIMITER2);
                            let tmp: Vec<u8> = vps.into();
                            out_buffer.extend(tmp);
                        }

                        if let Some(sps) = ctx.sps.first() {
                            out_buffer.extend(Self::DELIMITER2);
                            let tmp: Vec<u8> = sps.into();
                            out_buffer.extend(tmp);
                        }

                        if let Some(pps) = ctx.pps.first() {
                            out_buffer.extend(Self::DELIMITER2);
                            let tmp: Vec<u8> = pps.into();
                            out_buffer.extend(tmp);
                            vps_sps_pps_appended = true;
                        }
                    }
                }
                _ => {
                    if !aud_appended {
                        out_buffer.extend(Self::ACCESS_UNIT_DELIMITER);
                        aud_appended = true;
                    }
                    vps_sps_pps_appended = false
                }
            }

            out_buffer.extend(Self::DELIMITER1);
            let nalu_data: Vec<u8> = nalu.into();
            out_buffer.extend(nalu_data);
        }

        Ok(out_buffer)
    }
}

impl ReadFormat<Hevc> for AnnexB {
    type Context = HEVCDecoderConfigurationRecord;
    type Error = HevcError;

    fn read_format(&self, nals: &[u8], ctx: &mut Self::Context) -> Result<Hevc, Self::Error> {
        let mut nal_units: Vec<nal::Unit> = Vec::new();

        let (mut pre_pos, mut pre_length) = match iterate_nalu_startcode(nals, 0) {
            Ok(e) => e,
            Err(e) => {
                let nal_unit = nal::Unit::try_from(&nals[0..])?;
                match nal_unit.kind {
                    nal::NaluType::NaluTypeVps => {
                        ctx.vps = vec![nal_unit];
                    }
                    nal::NaluType::NaluTypeSps => {
                        ctx.sps = vec![nal_unit];
                    }
                    nal::NaluType::NaluTypePps => {
                        ctx.pps = vec![nal_unit];
                        ctx.parse()?;
                    }
                    nal::NaluType::NaluTypeAud
                    | nal::NaluType::NaluTypeSei
                    | nal::NaluType::NaluTypeSeiSuffix => {}

                    _ => nal_units.push(nal_unit),
                }
                return Err(HevcError::NotEnoughData("NALU data"));
            }
        };
        loop {
            let start = pre_pos + pre_length;
            let (pos, length) = match iterate_nalu_startcode(nals, start) {
                Ok(e) => e,
                Err(e) => {
                    if start < nals.len() {
                        let nal_unit = nal::Unit::try_from(&nals[start..])?;
                        match nal_unit.kind {
                            nal::NaluType::NaluTypeVps => {
                                ctx.vps = vec![nal_unit];
                            }
                            nal::NaluType::NaluTypeSps => {
                                ctx.sps = vec![nal_unit];
                            }
                            nal::NaluType::NaluTypePps => {
                                ctx.pps = vec![nal_unit];
                                ctx.parse()?;
                            }
                            nal::NaluType::NaluTypeAud
                            | nal::NaluType::NaluTypeSei
                            | nal::NaluType::NaluTypeSeiSuffix => {}

                            _ => nal_units.push(nal_unit),
                        }
                        return Ok(nal_units.into());
                    } else {
                        return Err(HevcError::NotEnoughData("NALU data"));
                    }
                }
            };

            if start < pos {
                let nal_unit = nal::Unit::try_from(&nals[start..pos])?;
                match nal_unit.kind {
                    nal::NaluType::NaluTypeVps => {
                        ctx.vps = vec![nal_unit];
                    }
                    nal::NaluType::NaluTypeSps => {
                        ctx.sps = vec![nal_unit];
                    }
                    nal::NaluType::NaluTypePps => {
                        ctx.pps = vec![nal_unit];
                        ctx.parse()?;
                    }
                    nal::NaluType::NaluTypeAud
                    | nal::NaluType::NaluTypeSei
                    | nal::NaluType::NaluTypeSeiSuffix => {}
                    _ => nal_units.push(nal_unit),
                }
            } else {
                return Err(HevcError::NotEnoughData("NALU data"));
            }
            pre_pos = pos;
            pre_length = length;
        }
    }
}

fn iterate_nalu_startcode(nalu: &[u8], start: usize) -> Result<(usize, usize), HevcError> {
    if nalu.len() == 0 || start >= nalu.len() {
        return Err(HevcError::NotEnoughData("NALU data"));
    }
    let mut count = 0;
    for i in 0..(nalu.len() - start) {
        match nalu[start + i] {
            0u8 => {
                count += 1;
            }
            1u8 => {
                if count >= 2 {
                    return Ok((start + i - count, count + 1));
                }
                count = 0
            }
            _ => count = 0,
        }
    }
    Err(HevcError::NotEnoughData("NALU data"))
}
