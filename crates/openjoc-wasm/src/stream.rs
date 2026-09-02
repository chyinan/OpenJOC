// pattern: Functional Core

use openjoc_eac3::{AccessUnitParse, Eac3Error, parse_access_unit_bounds};

const MAX_PENDING_INPUT_BYTES: usize = openjoc_eac3::GENERAL_MAX_ACCESS_UNIT_BYTES * 2;

#[derive(Debug)]
pub struct ElementaryStreamFramer {
    pending: Vec<u8>,
}

#[derive(Debug)]
pub enum FramingStatus {
    NeedMoreInput,
    AccessUnit(Vec<u8>),
    EndOfStream,
}

#[derive(Debug)]
pub struct FramingError(Eac3Error);

impl std::fmt::Display for FramingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl ElementaryStreamFramer {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pending: Vec::new(),
        }
    }

    pub fn push(&mut self, bytes: &[u8]) -> Result<(), FramingError> {
        let total = self
            .pending
            .len()
            .checked_add(bytes.len())
            .ok_or(FramingError(Eac3Error::InvalidAccessUnitRange))?;
        if total > MAX_PENDING_INPUT_BYTES {
            return Err(FramingError(
                Eac3Error::UnsupportedJocAccessUnitFrameCount { actual: total },
            ));
        }
        self.pending
            .try_reserve(bytes.len())
            .map_err(|_| FramingError(Eac3Error::FrameSizeOverflow))?;
        self.pending.extend_from_slice(bytes);
        Ok(())
    }

    pub fn next(&mut self, eos: bool) -> Result<FramingStatus, FramingError> {
        if self.pending.is_empty() {
            return Ok(if eos {
                FramingStatus::EndOfStream
            } else {
                FramingStatus::NeedMoreInput
            });
        }
        match parse_access_unit_bounds(&self.pending, eos).map_err(FramingError)? {
            AccessUnitParse::NeedMore => Ok(FramingStatus::NeedMoreInput),
            AccessUnitParse::Complete(length) => {
                let packet = self.pending.drain(..length).collect();
                Ok(FramingStatus::AccessUnit(packet))
            }
        }
    }

    pub fn reset(&mut self) {
        self.pending.clear();
    }
}
