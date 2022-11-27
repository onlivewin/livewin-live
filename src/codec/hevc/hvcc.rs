use super::ReadFormat;
use crate::codec::hevc::{config::HEVCDecoderConfigurationRecord, error::HevcError, nal, Hevc};
use bytes::Buf;
use std::{convert::TryFrom, io::Cursor};
pub struct Hvcc;

impl ReadFormat<Hevc> for Hvcc {
    type Context = HEVCDecoderConfigurationRecord;
    type Error = HevcError;

    fn read_format(&self, input: &[u8], _: &mut Self::Context) -> Result<Hevc, Self::Error> {
        let mut buf = Cursor::new(input);
        let mut nal_units = vec![];
        while buf.has_remaining() {
            let nalu_length = buf.get_u32() as usize;
            if buf.remaining() < nalu_length {
                return Err(HevcError::NotEnoughData("NALU size"));
            }
            let nalu_data = buf
                .chunk()
                .get(..nalu_length)
                .ok_or_else(|| HevcError::NotEnoughData("NALU data"))?
                .to_owned();
            buf.advance(nalu_length);
            let nal_unit = nal::Unit::try_from(&*nalu_data)?;
            nal_units.push(nal_unit);
        }
        Ok(nal_units.into())
    }
}
