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
    MissingIndependentSubstreamZero {
        frame: usize,
    },
    NonsequentialIndependentSubstream {
        expected: u8,
        actual: u8,
    },
    NonsequentialDependentSubstream {
        expected: u8,
        actual: u8,
    },
    DependentAfterConvertedSubstream {
        frame: usize,
    },
    SubstreamTimingMismatch {
        frame: usize,
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
            Self::MissingIndependentSubstreamZero { frame } => write!(
                formatter,
                "E-AC-3 access unit at frame {frame} does not begin with independent substream 0"
            ),
            Self::NonsequentialIndependentSubstream { expected, actual } => write!(
                formatter,
                "nonsequential E-AC-3 independent substream: expected {expected}, got {actual}"
            ),
            Self::NonsequentialDependentSubstream { expected, actual } => write!(
                formatter,
                "nonsequential E-AC-3 dependent substream: expected {expected}, got {actual}"
            ),
            Self::DependentAfterConvertedSubstream { frame } => write!(
                formatter,
                "dependent E-AC-3 frame {frame} follows a converted independent substream"
            ),
            Self::SubstreamTimingMismatch { frame } => {
                write!(
                    formatter,
                    "E-AC-3 substream timing mismatch at frame {frame}"
                )
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

/// One time-aligned set of independent and dependent substream frames.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AccessUnitIndex {
    pub first_frame: usize,
    pub frame_count: usize,
    pub sample_rate: u32,
    pub samples: u16,
}

/// TS 103 420 clause 8.3 type-A extension fields.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JocAddbsi {
    pub complexity_index: u8,
}

/// Decoded E.1.2.2 fields required by the JOC frontend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BitstreamInformation {
    pub header: SyncframeHeader,
    pub audio_coding_mode: u8,
    pub lfe_on: bool,
    pub bitstream_id: u8,
    pub addbsi: Option<Vec<u8>>,
}

/// Parses the fixed acquisition prefix from clauses E.1.2.1 and E.1.2.2.
///
/// # Errors
/// Returns an error for truncation, invalid synchronization, or reserved table
/// values.
pub fn parse_syncframe_header(bytes: &[u8]) -> Result<SyncframeHeader, Eac3Error> {
    let mut bits = BitReader::new(bytes);
    Ok(parse_header_reader(&mut bits)?.0)
}

fn parse_header_reader(bits: &mut BitReader<'_>) -> Result<(SyncframeHeader, u8), Eac3Error> {
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
    let num_blocks_code =
        u8::try_from(bits.read_bits(2)?).map_err(|_| Eac3Error::FrameSizeOverflow)?;
    let audio_blocks = match num_blocks_code {
        0 => 1,
        1 => 2,
        2 => 3,
        3 => 6,
        _ => unreachable!("two-bit field"),
    };
    Ok((
        SyncframeHeader {
            stream_type,
            substream_id,
            frame_size,
            sample_rate,
            audio_blocks,
            samples: u16::from(audio_blocks) * 256,
        },
        num_blocks_code,
    ))
}

/// Parses complete E.1.2.2 conditional syntax through the terminal `addbsi`.
///
/// # Errors
/// Returns an error for any truncated conditional branch, invalid acquisition
/// field, or frame shorter than its declared size.
pub fn parse_bsi(bytes: &[u8]) -> Result<BitstreamInformation, Eac3Error> {
    let header = parse_syncframe_header(bytes)?;
    if header.frame_size > bytes.len() {
        return Err(Eac3Error::TruncatedFrame {
            offset: 0,
            declared: header.frame_size,
            available: bytes.len(),
        });
    }
    let mut bits = BitReader::new(&bytes[..header.frame_size]);
    let (header, num_blocks_code) = parse_header_reader(&mut bits)?;
    let acmod = read_u8(&mut bits, 3)?;
    let lfe_on = bits.read_bit()?;
    let bitstream_id = read_u8(&mut bits, 5)?;
    skip(&mut bits, 5)?; // dialnorm
    if bits.read_bit()? {
        skip(&mut bits, 8)?;
    }
    if acmod == 0 {
        skip(&mut bits, 5)?;
        if bits.read_bit()? {
            skip(&mut bits, 8)?;
        }
    }
    if header.stream_type == StreamType::Dependent && bits.read_bit()? {
        skip(&mut bits, 16)?;
    }
    if bits.read_bit()? {
        parse_mixing_metadata(
            &mut bits,
            header.stream_type,
            acmod,
            lfe_on,
            num_blocks_code,
        )?;
    }
    if bits.read_bit()? {
        parse_informational_metadata(&mut bits, acmod)?;
    }
    if header.stream_type == StreamType::Independent && num_blocks_code != 3 {
        skip(&mut bits, 1)?;
    }
    if header.stream_type == StreamType::ConvertedIndependent {
        let block_id = num_blocks_code == 3 || bits.read_bit()?;
        if block_id {
            skip(&mut bits, 6)?;
        }
    }
    let addbsi = if bits.read_bit()? {
        let length = usize::from(read_u8(&mut bits, 6)?) + 1;
        let mut data = Vec::with_capacity(length);
        for _ in 0..length {
            data.push(read_u8(&mut bits, 8)?);
        }
        Some(data)
    } else {
        None
    };
    Ok(BitstreamInformation {
        header,
        audio_coding_mode: acmod,
        lfe_on,
        bitstream_id,
        addbsi,
    })
}

fn parse_mixing_metadata(
    bits: &mut BitReader<'_>,
    stream_type: StreamType,
    acmod: u8,
    lfe_on: bool,
    num_blocks_code: u8,
) -> Result<(), Eac3Error> {
    if acmod > 2 {
        skip(bits, 2)?;
    }
    if acmod & 1 != 0 && acmod > 2 {
        skip(bits, 6)?;
    }
    if acmod & 4 != 0 {
        skip(bits, 6)?;
    }
    if lfe_on && bits.read_bit()? {
        skip(bits, 5)?;
    }
    if stream_type != StreamType::Independent {
        return Ok(());
    }
    skip_optional(bits, 6)?;
    if acmod == 0 {
        skip_optional(bits, 6)?;
    }
    skip_optional(bits, 6)?;
    match bits.read_bits(2)? {
        0 => {}
        1 => skip(bits, 5)?,
        2 => skip(bits, 12)?,
        3 => {
            let length_code = usize::from(read_u8(bits, 5)?);
            let region_bits = length_code
                .checked_add(2)
                .and_then(|bytes| bytes.checked_mul(8))
                .and_then(|total| total.checked_sub(5))
                .ok_or(Eac3Error::FrameSizeOverflow)?;
            let _region = bits.take_bits(region_bits)?;
        }
        _ => unreachable!("two-bit field"),
    }
    if acmod < 2 {
        skip_optional(bits, 14)?;
        if acmod == 0 {
            skip_optional(bits, 14)?;
        }
    }
    if bits.read_bit()? {
        if num_blocks_code == 0 {
            skip(bits, 5)?;
        } else {
            for _ in 0..blocks_from_code(num_blocks_code) {
                skip_optional(bits, 5)?;
            }
        }
    }
    Ok(())
}

fn parse_informational_metadata(bits: &mut BitReader<'_>, acmod: u8) -> Result<(), Eac3Error> {
    skip(bits, 5)?;
    if acmod == 2 {
        skip(bits, 4)?;
    }
    if acmod >= 6 {
        skip(bits, 2)?;
    }
    skip_optional(bits, 8)?;
    if acmod == 0 {
        skip_optional(bits, 8)?;
    }
    skip(bits, 1)?;
    Ok(())
}

fn blocks_from_code(code: u8) -> u8 {
    [1, 2, 3, 6][usize::from(code)]
}

fn skip_optional(bits: &mut BitReader<'_>, width: u8) -> Result<(), Eac3Error> {
    if bits.read_bit()? {
        skip(bits, width)?;
    }
    Ok(())
}

fn skip(bits: &mut BitReader<'_>, width: u8) -> Result<(), Eac3Error> {
    let _ignored = bits.read_bits(width)?;
    Ok(())
}

fn read_u8(bits: &mut BitReader<'_>, width: u8) -> Result<u8, Eac3Error> {
    u8::try_from(bits.read_bits(width)?).map_err(|_| Eac3Error::FrameSizeOverflow)
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

/// Groups E.1.3.1.2 ordered substreams into time-aligned access units.
///
/// # Errors
/// Returns an error unless each unit starts with independent substream zero,
/// independent and dependent IDs are sequential, dependent streams immediately
/// follow their parent, and every substream has identical rate/block timing.
pub fn group_access_units(
    frames: &[SyncframeIndexEntry],
) -> Result<Vec<AccessUnitIndex>, Eac3Error> {
    let mut units = Vec::new();
    let mut first = 0_usize;
    while first < frames.len() {
        let base = frames[first].header;
        if base.stream_type == StreamType::Dependent || base.substream_id != 0 {
            return Err(Eac3Error::MissingIndependentSubstreamZero { frame: first });
        }
        let mut expected_independent = 0_u8;
        let mut expected_dependent = 0_u8;
        let mut dependent_allowed = false;
        let mut index = first;
        while index < frames.len() {
            let header = frames[index].header;
            if index > first
                && header.stream_type != StreamType::Dependent
                && header.substream_id == 0
            {
                break;
            }
            if header.sample_rate != base.sample_rate || header.audio_blocks != base.audio_blocks {
                return Err(Eac3Error::SubstreamTimingMismatch { frame: index });
            }
            match header.stream_type {
                StreamType::Dependent => {
                    if !dependent_allowed {
                        return Err(Eac3Error::DependentAfterConvertedSubstream { frame: index });
                    }
                    if header.substream_id != expected_dependent {
                        return Err(Eac3Error::NonsequentialDependentSubstream {
                            expected: expected_dependent,
                            actual: header.substream_id,
                        });
                    }
                    expected_dependent += 1;
                }
                StreamType::Independent | StreamType::ConvertedIndependent => {
                    if header.substream_id != expected_independent {
                        return Err(Eac3Error::NonsequentialIndependentSubstream {
                            expected: expected_independent,
                            actual: header.substream_id,
                        });
                    }
                    expected_independent += 1;
                    expected_dependent = 0;
                    dependent_allowed = header.stream_type == StreamType::Independent;
                }
            }
            index += 1;
        }
        units.push(AccessUnitIndex {
            first_frame: first,
            frame_count: index - first,
            sample_rate: base.sample_rate,
            samples: base.samples,
        });
        first = index;
    }
    Ok(units)
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
