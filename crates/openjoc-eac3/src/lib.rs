// pattern: Functional Core

//! Clean-room Enhanced AC-3 frontend from ETSI TS 102 366 Annex E.

use core::fmt;
use openjoc_bitio::{BitError, BitRead, BitReader};

const EAC3_SYNCWORD: u16 = 0x0b77;

/// Checked frontend failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Eac3Error {
    Bit(BitError),
    InvalidSyncword {
        actual: u16,
    },
    ReservedStreamType,
    ReservedSampleRate,
    FrameSizeOverflow,
    TruncatedFrame {
        offset: usize,
        declared: usize,
        available: usize,
    },
    InvalidAddbsiLength {
        actual: usize,
    },
    NonzeroReservedData,
    MissingJocExtensionFlag,
    ComplexityIndexOutOfRange {
        actual: u8,
    },
}

impl fmt::Display for Eac3Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bit(error) => write!(formatter, "failed to read E-AC-3 bitstream: {error}"),
            Self::InvalidSyncword { actual } => {
                write!(formatter, "invalid E-AC-3 syncword 0x{actual:04x}")
            }
            Self::ReservedStreamType => formatter.write_str("reserved E-AC-3 stream type"),
            Self::ReservedSampleRate => formatter.write_str("reserved E-AC-3 sample-rate code"),
            Self::FrameSizeOverflow => formatter.write_str("E-AC-3 frame-size overflow"),
            Self::TruncatedFrame {
                offset,
                declared,
                available,
            } => write!(
                formatter,
                "truncated E-AC-3 frame at byte {offset}: declared {declared} bytes with {available} available"
            ),
            Self::InvalidAddbsiLength { actual } => {
                write!(formatter, "invalid JOC addbsi length {actual}; expected 2")
            }
            Self::NonzeroReservedData => formatter.write_str("nonzero reserved E-AC-3 data"),
            Self::MissingJocExtensionFlag => {
                formatter.write_str("missing E-AC-3 JOC extension flag")
            }
            Self::ComplexityIndexOutOfRange { actual } => {
                write!(formatter, "E-AC-3 JOC complexity index {actual} exceeds 16")
            }
        }
    }
}

impl std::error::Error for Eac3Error {}

impl From<BitError> for Eac3Error {
    fn from(value: BitError) -> Self {
        Self::Bit(value)
    }
}

/// Table E.1.1 stream identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StreamType {
    Independent,
    Dependent,
    ConvertedIndependent,
}

/// Fixed-length acquisition fields at the start of one syncframe.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyncframeHeader {
    pub stream_type: StreamType,
    pub substream_id: u8,
    pub frame_size: usize,
    pub sample_rate: u32,
    pub audio_blocks: u8,
    pub samples: u16,
}

/// Byte location and acquisition header for one complete syncframe.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SyncframeIndexEntry {
    pub offset: usize,
    pub header: SyncframeHeader,
}

/// TS 103 420 clause 8.3 type-A extension fields.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JocAddbsi {
    pub complexity_index: u8,
}

/// Parses the fixed acquisition prefix from clauses E.1.2.1 and E.1.2.2.
///
/// # Errors
/// Returns an error for truncation, invalid synchronization, or reserved table
/// values.
pub fn parse_syncframe_header(bytes: &[u8]) -> Result<SyncframeHeader, Eac3Error> {
    let mut bits = BitReader::new(bytes);
    let syncword = u16::try_from(bits.read_bits(16)?).map_err(|_| Eac3Error::FrameSizeOverflow)?;
    if syncword != EAC3_SYNCWORD {
        return Err(Eac3Error::InvalidSyncword { actual: syncword });
    }
    let stream_type = match bits.read_bits(2)? {
        0 => StreamType::Independent,
        1 => StreamType::Dependent,
        2 => StreamType::ConvertedIndependent,
        3 => return Err(Eac3Error::ReservedStreamType),
        _ => unreachable!("two-bit field"),
    };
    let substream_id =
        u8::try_from(bits.read_bits(3)?).map_err(|_| Eac3Error::FrameSizeOverflow)?;
    let frame_words = usize::try_from(bits.read_bits(11)?)
        .map_err(|_| Eac3Error::FrameSizeOverflow)?
        .checked_add(1)
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    let frame_size = frame_words
        .checked_mul(2)
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    let sample_rate = match bits.read_bits(2)? {
        0 => 48_000,
        1 => 44_100,
        2 => 32_000,
        3 => return Err(Eac3Error::ReservedSampleRate),
        _ => unreachable!("two-bit field"),
    };
    let audio_blocks = match bits.read_bits(2)? {
        0 => 1,
        1 => 2,
        2 => 3,
        3 => 6,
        _ => unreachable!("two-bit field"),
    };
    Ok(SyncframeHeader {
        stream_type,
        substream_id,
        frame_size,
        sample_rate,
        audio_blocks,
        samples: u16::from(audio_blocks) * 256,
    })
}

/// Indexes a byte-concatenated sequence using declared frame sizes only.
///
/// # Errors
/// Returns an error at the first malformed header or incomplete declared frame.
pub fn index_syncframes(bytes: &[u8]) -> Result<Vec<SyncframeIndexEntry>, Eac3Error> {
    let mut entries = Vec::new();
    let mut offset = 0_usize;
    while offset < bytes.len() {
        let header = parse_syncframe_header(&bytes[offset..])?;
        let end = offset
            .checked_add(header.frame_size)
            .ok_or(Eac3Error::FrameSizeOverflow)?;
        if end > bytes.len() {
            return Err(Eac3Error::TruncatedFrame {
                offset,
                declared: header.frame_size,
                available: bytes.len() - offset,
            });
        }
        entries.push(SyncframeIndexEntry { offset, header });
        offset = end;
    }
    Ok(entries)
}

/// Parses the exact two-byte TS 103 420 clause 8.3 `addbsi` payload.
///
/// # Errors
/// Returns an error for the wrong length, nonzero reserved bits, an absent
/// extension flag, or complexity above the normative maximum of 16.
pub fn parse_joc_addbsi(bytes: &[u8]) -> Result<JocAddbsi, Eac3Error> {
    if bytes.len() != 2 {
        return Err(Eac3Error::InvalidAddbsiLength {
            actual: bytes.len(),
        });
    }
    if bytes[0] >> 1 != 0 {
        return Err(Eac3Error::NonzeroReservedData);
    }
    if bytes[0] & 1 == 0 {
        return Err(Eac3Error::MissingJocExtensionFlag);
    }
    let complexity_index = bytes[1];
    if complexity_index > 16 {
        return Err(Eac3Error::ComplexityIndexOutOfRange {
            actual: complexity_index,
        });
    }
    Ok(JocAddbsi { complexity_index })
}
