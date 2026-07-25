// pattern: Functional Core

use crate::OamdError;
use openjoc_bitio::{BitRead, BitReader};

const SAMPLE_OFFSETS: [u16; 4] = [8, 16, 18, 24];
const RAMP_DURATIONS: [u16; 16] = [
    32, 64, 128, 256, 320, 480, 1000, 1001, 1024, 1600, 1601, 1602, 1920, 2000, 2002, 2048,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MetadataBlockTiming {
    pub start_sample: u16,
    pub ramp_duration: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataTiming {
    pub sample_offset: u16,
    pub blocks: Vec<MetadataBlockTiming>,
}

/// Decoder-interface timing from clause 4.4, including codec-frame position.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimedMetadataBlock {
    pub start_sample: u16,
    pub frame_offset: u64,
    pub ramp_duration: u16,
}

/// Cross-frame timing state mandated by clause 5.3.2.
#[derive(Clone, Debug, Default)]
pub struct MetadataTimelineState {
    frame_offset: u64,
}

impl MetadataTimelineState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.frame_offset = 0;
    }

    /// Decodes one frame's timing and advances the frame offset atomically.
    ///
    /// # Errors
    /// Returns [`OamdError`] for invalid syntax or frame-offset overflow.
    pub fn decode_frame(&mut self, payload: &[u8]) -> Result<Vec<TimedMetadataBlock>, OamdError> {
        let timing = parse_metadata_timing(payload)?;
        let next = self
            .frame_offset
            .checked_add(1536)
            .ok_or(OamdError::FrameOffsetOverflow)?;
        let blocks = timing
            .blocks
            .into_iter()
            .map(|block| TimedMetadataBlock {
                start_sample: block.start_sample,
                frame_offset: self.frame_offset,
                ramp_duration: block.ramp_duration,
            })
            .collect();
        self.frame_offset = next;
        Ok(blocks)
    }
}

/// Decodes clauses 5.5.6 and 5.5.7 timing syntax.
///
/// # Errors
/// Returns [`OamdError`] for truncation or reserved sample-offset coding.
pub fn parse_metadata_timing(payload: &[u8]) -> Result<MetadataTiming, OamdError> {
    let mut reader = BitReader::new(payload);
    let sample_offset = match reader.read_bits(2)? {
        0 => 0,
        1 => SAMPLE_OFFSETS[usize::try_from(reader.read_bits(2)?)?],
        2 => u16::try_from(reader.read_bits(5)?)?,
        3 => return Err(OamdError::ReservedSampleOffsetCode),
        _ => unreachable!(),
    };
    let count = usize::try_from(reader.read_bits(3)?)? + 1;
    let mut blocks = Vec::with_capacity(count);
    for _ in 0..count {
        let factor = u16::try_from(reader.read_bits(6)?)?;
        let ramp_duration = match reader.read_bits(2)? {
            0 => 0,
            1 => 512,
            2 => 1536,
            3 if reader.read_bit()? => RAMP_DURATIONS[usize::try_from(reader.read_bits(4)?)?],
            3 => u16::try_from(reader.read_bits(11)?)?,
            _ => unreachable!(),
        };
        blocks.push(MetadataBlockTiming {
            start_sample: sample_offset + 32 * factor,
            ramp_duration,
        });
    }
    Ok(MetadataTiming {
        sample_offset,
        blocks,
    })
}
