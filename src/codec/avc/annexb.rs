use super::ReadFormat;
use super::WriteFormat;
use crate::codec::avc::{config::DecoderConfigurationRecord, error::AvcError, nal, Avc};
use std::convert::TryFrom;
pub struct AnnexB;

impl AnnexB {
    const DELIMITER1: &'static [u8] = &[0x00, 0x00, 0x01];
    const DELIMITER2: &'static [u8] = &[0x00, 0x00, 0x00, 0x01];
    const ACCESS_UNIT_DELIMITER: &'static [u8] = &[0x00, 0x00, 0x00, 0x01, 0x09, 0xF0];
}

impl WriteFormat<Avc> for AnnexB {
    type Context = DecoderConfigurationRecord;
    type Error = AvcError;

    fn write_format(&self, input: Avc, ctx: &Self::Context) -> Result<Vec<u8>, Self::Error> {
        let mut out_buffer = Vec::new();
        let mut aud_appended = false;
        let mut sps_and_pps_appended = false;
        let nalus: Vec<nal::Unit> = input.into();

        for nalu in nalus {
            use nal::UnitType::*;

            match &nalu.kind {
                SequenceParameterSet | PictureParameterSet | AccessUnitDelimiter => continue,
                NonIdrPicture | SupplementaryEnhancementInformation => {
                    if !aud_appended {
                        out_buffer.extend(Self::ACCESS_UNIT_DELIMITER);
                        aud_appended = true;
                    }
                }
                IdrPicture => {
                    if !aud_appended {
                        out_buffer.extend(Self::ACCESS_UNIT_DELIMITER);
                        aud_appended = true;
                    }

                    if !sps_and_pps_appended {
                        if let Some(sps) = ctx.sps.first() {
                            out_buffer.extend(Self::DELIMITER2);
                            let tmp: Vec<u8> = sps.into();
                            out_buffer.extend(tmp);
                        }

                        if let Some(pps) = ctx.pps.first() {
                            out_buffer.extend(Self::DELIMITER2);
                            let tmp: Vec<u8> = pps.into();
                            out_buffer.extend(tmp);
                        }

                        sps_and_pps_appended = true;
                    }
                }
                t => log::debug!("Received unhandled NALU type {:?}", t),
            }

            out_buffer.extend(Self::DELIMITER1);

            let nalu_data: Vec<u8> = nalu.into();
            out_buffer.extend(nalu_data);
        }

        Ok(out_buffer)
    }
}

impl ReadFormat<Avc> for AnnexB {
    type Context = DecoderConfigurationRecord;
    type Error = AvcError;

    fn read_format(&self, nals: &[u8], ctx: &mut Self::Context) -> Result<Avc, Self::Error> {
        let mut nal_units: Vec<nal::Unit> = Vec::new();
        let (mut pre_pos, mut pre_length) = match iterate_nalu_startcode(nals, 0) {
            Ok(e) => e,
            Err(_e) => {
                let nal_unit = nal::Unit::try_from(&nals[0..])?;
                println!("nal kind -----{:?}", nal_unit.kind);
                match nal_unit.kind {
                    nal::UnitType::SequenceParameterSet => {
                        ctx.sps = vec![nal_unit];
                        ctx.parse()?;
                    }
                    nal::UnitType::PictureParameterSet => {
                        ctx.pps = vec![nal_unit];
                    }
                    nal::UnitType::AccessUnitDelimiter => {}

                    _ => nal_units.push(nal_unit),
                }
                return Err(AvcError::NotEnoughData("NALU data"));
            }
        };
        loop {
            let start = pre_pos + pre_length;
            let (pos, length) = match iterate_nalu_startcode(nals, start) {
                Ok(e) => e,
                Err(_e) => {
                    if start < nals.len() {
                        let nal_unit = nal::Unit::try_from(&nals[start..])?;
                        println!("nal kind -----{:?}", nal_unit.kind);
                        match nal_unit.kind {
                            nal::UnitType::SequenceParameterSet => {
                                ctx.sps = vec![nal_unit];
                                ctx.parse()?;
                            }
                            nal::UnitType::PictureParameterSet => {
                                ctx.pps = vec![nal_unit];
                            }
                            nal::UnitType::AccessUnitDelimiter => {}

                            _ => nal_units.push(nal_unit),
                        }
                        return Ok(nal_units.into());
                    } else {
                        return Err(AvcError::NotEnoughData("NALU data"));
                    }
                }
            };

            if start < pos {
                let nal_unit = nal::Unit::try_from(&nals[start..pos])?;
                println!("nal kind -----{:?}", nal_unit.kind);
                match nal_unit.kind {
                    nal::UnitType::SequenceParameterSet => {
                        ctx.sps = vec![nal_unit];
                        ctx.parse()?;
                    }
                    nal::UnitType::PictureParameterSet => {
                        ctx.pps = vec![nal_unit];
                    }
                    nal::UnitType::AccessUnitDelimiter => {}
                    _ => nal_units.push(nal_unit),
                }
            } else {
                return Err(AvcError::NotEnoughData("NALU data"));
            }

            pre_pos = pos;
            pre_length = length;
        }
    }
}

fn iterate_nalu_startcode(nalu: &[u8], start: usize) -> Result<(usize, usize), AvcError> {
    if nalu.len() == 0 || start >= nalu.len() {
        return Err(AvcError::NotEnoughData("NALU data"));
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
    Err(AvcError::NotEnoughData("NALU data"))
}
