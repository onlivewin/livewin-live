pub mod annexb;
pub mod avcc;
pub mod config;
mod error;
pub mod nal;

use {
    self::config::DecoderConfigurationRecord,
    super::{FormatReader, FormatWriter, ReadFormat, WriteFormat},
    std::{
        convert::TryInto,
        fmt::{self, Debug},
    },
};

pub use self::{annexb::AnnexB, avcc::Avcc, error::AvcError};

pub struct Avc(Vec<nal::Unit>);

impl From<Vec<nal::Unit>> for Avc {
    fn from(val: Vec<nal::Unit>) -> Self {
        Self(val)
    }
}

impl From<Avc> for Vec<nal::Unit> {
    fn from(val: Avc) -> Self {
        val.0
    }
}

#[derive(Debug, PartialEq, Eq)]
enum State {
    Initializing,
    Ready,
}

impl Default for State {
    fn default() -> Self {
        Self::Initializing
    }
}

#[derive(Default)]
pub struct AvcCoder {
    pub dcr: Option<DecoderConfigurationRecord>,
    state: State,
}

impl AvcCoder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_dcr<D>(&mut self, dcr: D) -> Result<(), AvcError>
    where
        D: TryInto<DecoderConfigurationRecord, Error = AvcError>,
    {
        let dcr = dcr.try_into()?;
        self.dcr = Some(dcr);
        self.state = State::Ready;
        Ok(())
    }
}

impl Debug for AvcCoder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AvcDecoder")
            .field("state", &self.state)
            .finish()
    }
}

impl FormatReader<Avcc> for AvcCoder {
    type Output = Avc;
    type Error = AvcError;

    fn read_format(
        &mut self,
        format: Avcc,
        input: &[u8],
    ) -> Result<Option<Self::Output>, Self::Error> {
        Ok(match &self.state {
            State::Initializing => {
                self.set_dcr(input)
                    .map_err(|_| AvcError::DecoderInitializationFailed)?;
                None
            }
            State::Ready => {
                let mut dcr = self.dcr.as_mut().unwrap();
                Some(format.read_format(input, &mut dcr)?)
            }
        })
    }
}

impl FormatWriter<Avcc> for AvcCoder {
    type Input = Avc;
    type Error = AvcError;

    fn write_format(&mut self, format: Avcc, input: Self::Input) -> Result<Vec<u8>, Self::Error> {
        match &self.state {
            State::Initializing => Err(AvcError::NotInitialized),
            State::Ready => {
                let dcr = self.dcr.as_ref().unwrap();
                Ok(format.write_format(input, dcr)?)
            }
        }
    }
}

impl FormatReader<AnnexB> for AvcCoder {
    type Output = Avc;
    type Error = AvcError;

    fn read_format(
        &mut self,
        format: AnnexB,
        input: &[u8],
    ) -> Result<Option<Self::Output>, Self::Error> {
        Ok(match &self.state {
            State::Initializing => {
                self.dcr = Some(DecoderConfigurationRecord::default());
                let mut dcr = self.dcr.as_mut().unwrap();
                let nals = format.read_format(input, &mut dcr)?;
                self.state = State::Ready;
                if dcr.ready() {
                    Some(nals)
                } else {
                    None
                }
            }
            State::Ready => {
                let mut dcr = self.dcr.as_mut().unwrap();
                Some(format.read_format(input, &mut dcr)?)
            }
        })
    }
}

impl FormatWriter<AnnexB> for AvcCoder {
    type Input = Avc;
    type Error = AvcError;

    fn write_format(&mut self, format: AnnexB, input: Self::Input) -> Result<Vec<u8>, Self::Error> {
        match &self.state {
            State::Initializing => Err(AvcError::NotInitialized),
            State::Ready => {
                let dcr = self.dcr.as_ref().unwrap();
                Ok(format.write_format(input, dcr)?)
            }
        }
    }
}
