use super::{ReadFormat, WriteFormat};
use bytes::{Buf, BufMut};
use std::{convert::TryFrom, io::Cursor};

use crate::codec::avc::{config::DecoderConfigurationRecord, error::AvcError, nal, Avc};
pub struct Avcc;

impl ReadFormat<Avc> for Avcc {
    type Context = DecoderConfigurationRecord;
    type Error = AvcError;

    fn read_format(&self, input: &[u8], ctx: &mut Self::Context) -> Result<Avc, Self::Error> {
        let mut buf = Cursor::new(input);
        let mut nal_units = Vec::new();

        while buf.has_remaining() {
            let unit_size = ctx.nalu_size as usize;

            if buf.remaining() < unit_size {
                return Err(AvcError::NotEnoughData("NALU size"));
            }
            let nalu_length = buf.get_uint(unit_size) as usize;

            let nalu_data = buf
                .chunk()
                .get(..nalu_length)
                .ok_or_else(|| AvcError::NotEnoughData("NALU data"))?
                .to_owned();

            buf.advance(nalu_length);

            let nal_unit = nal::Unit::try_from(&*nalu_data)?;
            nal_units.push(nal_unit);
        }

        Ok(nal_units.into())
    }
}

impl WriteFormat<Avc> for Avcc {
    type Context = DecoderConfigurationRecord;
    type Error = AvcError;

    fn write_format(&self, input: Avc, _ctx: &Self::Context) -> Result<Vec<u8>, Self::Error> {
        let nalus: Vec<nal::Unit> = input.into();
        let mut out_buffer = Vec::new();
        //out_buffer.extend(ctx.to_bytes());
        for nalu in nalus {
            use nal::UnitType::*;
            match &nalu.kind {
                SequenceParameterSet | PictureParameterSet | AccessUnitDelimiter => continue,
                NonIdrPicture | SupplementaryEnhancementInformation | IdrPicture => {
                    let nalu_data: Vec<u8> = nalu.into();
                    out_buffer.put_u32(nalu_data.len() as u32);
                    out_buffer.extend(nalu_data);
                }
                t => log::debug!("Received unhandled NALU type {:?}", t),
            }
        }

        Ok(out_buffer)
    }
}
