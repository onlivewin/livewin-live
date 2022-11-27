use std::cmp::{max, min};
use std::io::Cursor;

use super::aac_codec::{AacProfile, RawAacStreamCodec};
use super::config::AudioSpecificConfiguration;
use super::{ReadFormat, WriteFormat};
use crate::codec::aac::{error::AacError, Aac};
use bytes::{Buf, BufMut};
// Bits | Description
// ---- | -----------
// 12   | Sync word, constant 0xFFF
// 1    | MPEG version
// 2    | Layer, constant 0x00
// 1    | Protection flag
// 2    | Profile
// 4    | MPEG-4 sampling frequency index
// 1    | Private, constant 0x00
// 3    | MPEG-4 channel configuration
// 1    | Originality
// 1    | Home
// 1    | Copyrighted ID
// 1    | Copyrighted ID start
// 13   | Frame length
// 11   | Buffer fullness
// 2    | Number of AAC frames - 1
// 16   | CRC if protection flag not set
//
// https://wiki.multimedia.cx/index.php/ADTS
#[derive(Debug, Clone)]
pub struct AudioDataTransportStream;

impl AudioDataTransportStream {
    const SYNCWORD: u16 = 0xFFF0;
    const PROTECTION_ABSENCE: u16 = 0x0001;
}

impl WriteFormat<Aac> for AudioDataTransportStream {
    type Context = AudioSpecificConfiguration;
    type Error = AacError;

    fn write_format(&self, input: Aac, ctx: &Self::Context) -> Result<Vec<u8>, Self::Error> {
        let payload: Vec<u8> = input.into();
        let mut tmp = Vec::with_capacity(56 + payload.len());

        // Syncword (12 bits), MPEG version (1 bit = 0),
        // layer (2 bits = 0) and protection absence (1 bit = 1)
        tmp.put_u16(Self::SYNCWORD | Self::PROTECTION_ABSENCE);

        // Profile (2 bits = 0), sampling frequency index (4 bits),
        // private (1 bit = 0) and channel configuration (1 bit)
        let object_type = ctx.object_type as u8;
        let profile = (object_type - 1) << 6;

        let sampling_frequency_index = u8::from(ctx.sampling_frequency_index) << 2;
        if sampling_frequency_index == 0x0F {
            return Err(AacError::ForbiddenSamplingFrequencyIndex(
                sampling_frequency_index,
            ));
        }

        let channel_configuration: u8 = ctx.channel_configuration.into();
        let channel_configuration1 = (channel_configuration & 0x07) >> 2;
        tmp.put_u8(profile | sampling_frequency_index | channel_configuration1);

        // Channel configuration cont. (2 bits), originality (1 bit = 0),
        // home (1 bit = 0), copyrighted id (1 bit = 0)
        // copyright id start (1 bit = 0) and frame length (2 bits)
        let channel_configuration2 = (channel_configuration & 0x03) << 6;

        // Header is 7 bytes long if protection is absent,
        // 9 bytes otherwise (CRC requires 2 bytes).
        let frame_length = (payload.len() + 7) as u16;
        let frame_length1 = ((frame_length & 0x1FFF) >> 11) as u8;
        tmp.put_u8(channel_configuration2 | frame_length1);

        // Frame length cont. (11 bits) and buffer fullness (5 bits)
        let frame_length2 = ((frame_length & 0x7FF) << 5) as u16;
        tmp.put_u16(frame_length2 | 0b0000_0000_0001_1111);

        // Buffer fullness cont. (6 bits) and number of AAC frames minus one (2 bits = 0)
        tmp.put_u8(0b1111_1100);

        tmp.extend(payload);

        Ok(tmp)
    }
}

impl ReadFormat<Vec<Aac>> for AudioDataTransportStream {
    type Context = ();
    type Error = AacError;

    fn read_format(&self, input: &[u8], _ctx: &mut Self::Context) -> Result<Vec<Aac>, Self::Error> {
        let mut buf = Cursor::new(input);
        let mut aacs = vec![];
        while buf.has_remaining() {
            if buf.remaining() < 7 {
                return Err(AacError::NotEnoughData("not enough data"));
            }

            buf.get_u8();

            let pav = buf.get_u8() & 0x0f;

            // let mut id = (pav >> 3) & 0x01;
            let protection_absent = pav & 0x01;

            // if id != 0x01 {
            //     id = 0x01;
            // }

            let sfiv = buf.get_u16();

            let profile: AacProfile = (((sfiv >> 14) & 0x03) as u8).into();

            let sampling_frequency_index = ((sfiv >> 10) & 0x0f) as u8;
            let channel_configuration = ((sfiv >> 6) & 0x07) as u8;

            let mut frame_length = (sfiv << 11) & 0x1800;

            let abfv = (buf.get_u16() as u32) << 8 | (buf.get_u8()) as u32;
            frame_length |= ((abfv >> 13) & 0x07ff) as u16;

            let mut adts_header_size = 7;
            if protection_absent == 0 {
                if buf.remaining() < 2 {
                    return Err(AacError::NotEnoughData("not enough data"));
                }
                buf.get_u16();
                adts_header_size += 2;
            }

            let raw_data_size = frame_length - adts_header_size;
            if buf.remaining() < raw_data_size as usize {
                return Err(AacError::NotEnoughData("not enough data"));
            }

            let data = buf
                .chunk()
                .get(..raw_data_size as usize)
                .unwrap()
                .to_owned();

            buf.advance(raw_data_size as usize);
            let aac_object = profile.into();

            let sound_format = 10;
            let sound_rate = match sampling_frequency_index {
                0x0a | 0x0b => 0u8,
                0x07 | 0x08 | 0x09 => 1u8,
                0x04 | 0x05 | 0x06 => 2u8,
                _ => 3u8,
            };
            let sound_type = max(0, min(1, channel_configuration - 1)) as u8;
            let sound_size = 1u8;

            let aac_packet_type = 0u8;
            let rcodec = Some(RawAacStreamCodec {
                protection_absent,
                aac_object,
                sampling_frequency_index,
                channel_configuration,
                frame_length,
                sound_format,
                sound_rate,
                sound_type,
                sound_size,
                aac_packet_type,
            });

            aacs.push(Aac { data, rcodec });
        }

        Ok(aacs)
    }
}
