// pattern: Functional Core

use crate::{HuffmanCodeword, HuffmanError, all_huffman_tables, decode_huffman_codeword};
use openjoc_bitio::{BitError, BitRead, BitReader};
use std::fmt;

/// Quantization resolution signalled by TS 103 420 table 51.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QuantMode {
    Coarse96,
    Fine192,
}

impl QuantMode {
    /// Returns the number of quantizer steps.
    #[must_use]
    pub const fn steps(self) -> u16 {
        match self {
            Self::Coarse96 => 96,
            Self::Fine192 => 192,
        }
    }

    pub(crate) const fn index(self) -> u8 {
        match self {
            Self::Coarse96 => 0,
            Self::Fine192 => 1,
        }
    }
}

/// Temporal interpolation type from TS 103 420 table 52.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Slope {
    Smooth,
    Steep,
}

/// Retained top-level JOC header syntax and derived counts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JocHeader {
    pub downmix_index: u8,
    pub channel_count: u8,
    pub object_count_bits: u8,
    pub object_count: u8,
    pub extension_index: u8,
}

/// The mode-dependent coded values for one temporal data point.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JocPayloadData {
    Sparse {
        initial_channel: u8,
        channel_deltas: Vec<HuffmanCodeword>,
        vector_symbols: Vec<HuffmanCodeword>,
    },
    Full {
        matrix_symbols: Vec<Vec<HuffmanCodeword>>,
    },
}

/// One retained data point and its optional steep-transition offset.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JocDataPoint {
    pub offset_timeslot: Option<u8>,
    pub payload: JocPayloadData,
}

/// Raw and derived syntax for one output object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JocObjectFrame {
    pub present: bool,
    pub band_index: Option<u8>,
    pub band_count: Option<u8>,
    pub sparse: Option<bool>,
    pub quant_mode: Option<QuantMode>,
    pub slope: Option<Slope>,
    pub data_points: Vec<JocDataPoint>,
}

/// Fully retained TS 103 420 clauses 6.2 and 6.3 payload.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JocFrame {
    pub header: JocHeader,
    pub clip_gain_x_bits: u8,
    pub clip_gain_y_bits: u8,
    pub sequence_count: u16,
    pub objects: Vec<JocObjectFrame>,
}

/// Structured syntax and semantic validation errors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JocParseError {
    Bit(BitError),
    Huffman(HuffmanError),
    ReservedDownmix { index: u8 },
    TooManyObjects { count: u8 },
    ReservedExtension { index: u8 },
    InvalidSparseChannel { index: u8, channel_count: u8 },
    TrailingData { bits: usize },
    NonZeroPadding,
}

impl fmt::Display for JocParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bit(error) => write!(formatter, "failed to read JOC syntax: {error}"),
            Self::Huffman(error) => write!(formatter, "failed to decode JOC codeword: {error}"),
            Self::ReservedDownmix { index } => {
                write!(formatter, "reserved JOC downmix index {index}")
            }
            Self::TooManyObjects { count } => {
                write!(formatter, "invalid JOC object count {count}; maximum is 16")
            }
            Self::ReservedExtension { index } => {
                write!(formatter, "reserved JOC extension index {index}")
            }
            Self::InvalidSparseChannel {
                index,
                channel_count,
            } => write!(
                formatter,
                "invalid sparse channel index {index} for {channel_count}-channel downmix"
            ),
            Self::TrailingData { bits } => write!(
                formatter,
                "JOC payload has {bits} trailing non-padding bits"
            ),
            Self::NonZeroPadding => formatter.write_str("JOC payload padding is nonzero"),
        }
    }
}

impl std::error::Error for JocParseError {}

impl From<BitError> for JocParseError {
    fn from(value: BitError) -> Self {
        Self::Bit(value)
    }
}

impl From<HuffmanError> for JocParseError {
    fn from(value: HuffmanError) -> Self {
        Self::Huffman(value)
    }
}

struct ObjectInfo {
    band_index: u8,
    band_count: u8,
    sparse: bool,
    quant_mode: QuantMode,
    slope: Slope,
    offsets: Vec<Option<u8>>,
}

/// Parses one complete JOC payload according to clauses 6.2 and 6.3.
///
/// # Errors
///
/// Returns [`JocParseError`] for truncation, malformed Huffman data, reserved
/// syntax, invalid sparse channels, trailing fields, or nonzero padding.
#[allow(clippy::similar_names, clippy::too_many_lines)]
pub fn parse_joc_payload(payload: &[u8]) -> Result<JocFrame, JocParseError> {
    let mut reader = BitReader::new(payload);
    let downmix_index = read_u8(&mut reader, 3)?;
    let channel_count = match downmix_index {
        0 | 3 => 5,
        1 | 2 | 4 => 7,
        index => return Err(JocParseError::ReservedDownmix { index }),
    };
    let object_count_bits = read_u8(&mut reader, 6)?;
    let object_count = object_count_bits + 1;
    if object_count > 16 {
        return Err(JocParseError::TooManyObjects {
            count: object_count,
        });
    }
    let extension_index = read_u8(&mut reader, 3)?;
    if extension_index != 0 {
        return Err(JocParseError::ReservedExtension {
            index: extension_index,
        });
    }
    let header = JocHeader {
        downmix_index,
        channel_count,
        object_count_bits,
        object_count,
        extension_index,
    };
    let clip_gain_x_bits = read_u8(&mut reader, 3)?;
    let clip_gain_y_bits = read_u8(&mut reader, 5)?;
    let sequence_count = read_u16(&mut reader, 10)?;

    let mut infos = Vec::with_capacity(usize::from(object_count));
    for _ in 0..object_count {
        if !reader.read_bit()? {
            infos.push(None);
            continue;
        }
        let band_index = read_u8(&mut reader, 3)?;
        let band_count = [1, 3, 5, 7, 9, 12, 15, 23][usize::from(band_index)];
        let sparse = reader.read_bit()?;
        let quant_mode = if reader.read_bit()? {
            QuantMode::Fine192
        } else {
            QuantMode::Coarse96
        };
        let slope = if reader.read_bit()? {
            Slope::Steep
        } else {
            Slope::Smooth
        };
        let data_point_count = read_u8(&mut reader, 1)? + 1;
        let mut offsets = Vec::with_capacity(usize::from(data_point_count));
        for _ in 0..data_point_count {
            offsets.push(if slope == Slope::Steep {
                Some(read_u8(&mut reader, 5)? + 1)
            } else {
                None
            });
        }
        infos.push(Some(ObjectInfo {
            band_index,
            band_count,
            sparse,
            quant_mode,
            slope,
            offsets,
        }));
    }

    let tables = all_huffman_tables();
    let mut objects = Vec::with_capacity(infos.len());
    for info in infos {
        let Some(info) = info else {
            objects.push(JocObjectFrame {
                present: false,
                band_index: None,
                band_count: None,
                sparse: None,
                quant_mode: None,
                slope: None,
                data_points: Vec::new(),
            });
            continue;
        };
        let mut data_points = Vec::with_capacity(info.offsets.len());
        for offset_timeslot in info.offsets {
            let payload = if info.sparse {
                let initial_channel = read_u8(&mut reader, 3)?;
                if initial_channel >= channel_count {
                    return Err(JocParseError::InvalidSparseChannel {
                        index: initial_channel,
                        channel_count,
                    });
                }
                let index_table = if channel_count == 5 {
                    tables[4]
                } else {
                    tables[5]
                };
                let mut channel_deltas = Vec::with_capacity(usize::from(info.band_count - 1));
                for _ in 1..info.band_count {
                    channel_deltas.push(decode_huffman_codeword(
                        &mut reader,
                        index_table.nodes,
                        None,
                    )?);
                }
                let vector_table = match info.quant_mode {
                    QuantMode::Coarse96 => tables[2],
                    QuantMode::Fine192 => tables[3],
                };
                let mut vector_symbols = Vec::with_capacity(usize::from(info.band_count));
                for _ in 0..info.band_count {
                    vector_symbols.push(decode_huffman_codeword(
                        &mut reader,
                        vector_table.nodes,
                        None,
                    )?);
                }
                JocPayloadData::Sparse {
                    initial_channel,
                    channel_deltas,
                    vector_symbols,
                }
            } else {
                let matrix_table = match info.quant_mode {
                    QuantMode::Coarse96 => tables[0],
                    QuantMode::Fine192 => tables[1],
                };
                let mut matrix_symbols = Vec::with_capacity(usize::from(channel_count));
                for _ in 0..channel_count {
                    let mut channel = Vec::with_capacity(usize::from(info.band_count));
                    for _ in 0..info.band_count {
                        channel.push(decode_huffman_codeword(
                            &mut reader,
                            matrix_table.nodes,
                            None,
                        )?);
                    }
                    matrix_symbols.push(channel);
                }
                JocPayloadData::Full { matrix_symbols }
            };
            data_points.push(JocDataPoint {
                offset_timeslot,
                payload,
            });
        }
        objects.push(JocObjectFrame {
            present: true,
            band_index: Some(info.band_index),
            band_count: Some(info.band_count),
            sparse: Some(info.sparse),
            quant_mode: Some(info.quant_mode),
            slope: Some(info.slope),
            data_points,
        });
    }

    let remaining = reader.bits_remaining();
    if remaining > 7 {
        return Err(JocParseError::TrailingData { bits: remaining });
    }
    for _ in 0..remaining {
        if reader.read_bit()? {
            return Err(JocParseError::NonZeroPadding);
        }
    }
    Ok(JocFrame {
        header,
        clip_gain_x_bits,
        clip_gain_y_bits,
        sequence_count,
        objects,
    })
}

fn read_u8(reader: &mut impl BitRead, width: u8) -> Result<u8, JocParseError> {
    Ok(u8::try_from(reader.read_bits(width)?).map_err(|_| BitError::LengthOverflow)?)
}

fn read_u16(reader: &mut impl BitRead, width: u8) -> Result<u16, JocParseError> {
    Ok(u16::try_from(reader.read_bits(width)?).map_err(|_| BitError::LengthOverflow)?)
}
