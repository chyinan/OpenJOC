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
