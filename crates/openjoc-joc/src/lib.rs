// pattern: Functional Core

//! Normative JOC parsing and reconstruction primitives from ETSI TS 103 420.

use openjoc_bitio::{BitError, BitRead};
use std::fmt;

mod decoder;
mod parser;
mod reconstruction;
pub use parser::{
    JocDataPoint, JocFrame, JocHeader, JocObjectFrame, JocParseError, JocPayloadData, QuantMode,
    Slope, parse_joc_payload,
};
pub use reconstruction::{
    InterpolatedMatrix, JocBandCount, ReconstructionError, dequantize, interpolate_matrix,
    qmf_subband_to_parameter_band, reconstruct_full, reconstruct_objects, reconstruct_sparse,
};

mod generated {
    include!("generated_etsi_tables.rs");
}

/// A named normative Huffman tree from TS 103 420 Annex A.1.
#[derive(Clone, Copy, Debug)]
pub struct HuffmanTable {
    pub name: &'static str,
    pub nodes: &'static [[i16; 2]],
}

/// Huffman decoding failures for malformed or truncated input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HuffmanError {
    TruncatedCodeword,
    InvalidNode { node: usize, node_count: usize },
    CyclicTree,
    BitReader(BitError),
}

/// One retained Huffman codeword and the symbol produced by clause 6.6.3.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HuffmanCodeword {
    pub bits: Vec<bool>,
    pub symbol: u16,
}

impl fmt::Display for HuffmanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TruncatedCodeword => formatter.write_str("truncated Huffman codeword"),
            Self::InvalidNode { node, node_count } => {
                write!(
                    formatter,
                    "invalid Huffman node {node} in tree of {node_count} nodes"
                )
            }
            Self::CyclicTree => formatter.write_str("cyclic Huffman tree"),
            Self::BitReader(error) => write!(formatter, "failed to read Huffman bit: {error}"),
        }
    }
}

impl std::error::Error for HuffmanError {}

/// Returns all six normative Annex A.1 trees.
#[must_use]
pub fn all_huffman_tables() -> [HuffmanTable; 6] {
    [
        HuffmanTable {
            name: "joc_huff_code_coarse_generic",
            nodes: &generated::JOC_HUFF_CODE_COARSE_GENERIC,
        },
        HuffmanTable {
            name: "joc_huff_code_fine_generic",
            nodes: &generated::JOC_HUFF_CODE_FINE_GENERIC,
        },
        HuffmanTable {
            name: "joc_huff_code_coarse_coeff_sparse",
            nodes: &generated::JOC_HUFF_CODE_COARSE_COEFF_SPARSE,
        },
        HuffmanTable {
            name: "joc_huff_code_fine_coeff_sparse",
            nodes: &generated::JOC_HUFF_CODE_FINE_COEFF_SPARSE,
        },
        HuffmanTable {
            name: "joc_huff_code_5ch_pos_index_sparse",
            nodes: &generated::JOC_HUFF_CODE_5CH_POS_INDEX_SPARSE,
        },
        HuffmanTable {
            name: "joc_huff_code_7ch_pos_index_sparse",
            nodes: &generated::JOC_HUFF_CODE_7CH_POS_INDEX_SPARSE,
        },
    ]
}

/// Decodes one MSB-first symbol using TS 103 420 clause 6.6.3 pseudocode 4.
///
/// `codeword_bits` limits the readable bits when a codeword shares a padded
/// backing byte. Pass `None` when the reader itself ends exactly at the field.
///
/// # Errors
///
/// Returns [`HuffmanError`] for truncated input, invalid node references,
/// cyclic trees, or underlying bit-reader arithmetic failures.
pub fn decode_huffman(
    reader: &mut impl BitRead,
    nodes: &[[i16; 2]],
    codeword_bits: Option<usize>,
) -> Result<u16, HuffmanError> {
    Ok(decode_huffman_codeword(reader, nodes, codeword_bits)?.symbol)
}

/// Decodes and retains one MSB-first codeword for diagnostics and raw syntax dumps.
///
/// # Errors
///
/// Returns [`HuffmanError`] under the same conditions as [`decode_huffman`].
pub fn decode_huffman_codeword(
    reader: &mut impl BitRead,
    nodes: &[[i16; 2]],
    codeword_bits: Option<usize>,
) -> Result<HuffmanCodeword, HuffmanError> {
    if nodes.is_empty() {
        return Err(HuffmanError::InvalidNode {
            node: 0,
            node_count: 0,
        });
    }

    let mut node = 0_usize;
    let mut consumed = 0_usize;
    let mut bits = Vec::new();
    loop {
        if consumed >= codeword_bits.unwrap_or(usize::MAX) {
            return Err(HuffmanError::TruncatedCodeword);
        }
        if consumed > nodes.len() {
            return Err(HuffmanError::CyclicTree);
        }
        let branches = nodes.get(node).ok_or(HuffmanError::InvalidNode {
            node,
            node_count: nodes.len(),
        })?;
        let bit = match reader.read_bit() {
            Ok(bit) => bit,
            Err(BitError::EndOfInput { .. }) => return Err(HuffmanError::TruncatedCodeword),
            Err(error) => return Err(HuffmanError::BitReader(error)),
        };
        bits.push(bit);
        consumed += 1;
        let next = branches[usize::from(bit)];
        if next <= 0 {
            let symbol =
                u16::try_from(-i32::from(next) - 1).map_err(|_| HuffmanError::InvalidNode {
                    node,
                    node_count: nodes.len(),
                })?;
            return Ok(HuffmanCodeword { bits, symbol });
        }
        node = usize::try_from(next).map_err(|_| HuffmanError::InvalidNode {
            node,
            node_count: nodes.len(),
        })?;
        if node >= nodes.len() {
            return Err(HuffmanError::InvalidNode {
                node,
                node_count: nodes.len(),
            });
        }
    }
}

/// Returns the verified 640-coefficient prototype for clause 7 QMF processing.
#[must_use]
pub fn qmf_prototype_64() -> &'static [f32; 640] {
    &generated::PROT64
}
pub use decoder::{
    DecodedJocFrame, JocDecodeError, JocDecoderState, ReconstructionBasis, ReconstructionBasisRow,
    ReconstructionBasisRowIndex, ReconstructionRowStages, ReconstructionStageTiming,
};
