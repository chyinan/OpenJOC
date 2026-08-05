//! Opt-in, parser-emitted coding-tool inventory.
//!
//! This module deliberately consumes the already decoded `AudioBlockPrefix`
//! and mantissa arrays. It never reparses a bitstream and is not consulted by
//! the production PCM path.

use crate::{AudioFrameInformation, DecodedAudioBlock, Eac3Error};
use serde::Serialize;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InventoryProvenance {
    ParsedExplicitly,
    ReusedFromPreviousBlock,
    DerivedFromNormativeState,
    DerivedFromParsedRanges,
    NotApplicable,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum SemanticChannel {
    Left,
    Right,
    Centre,
    LeftSurround,
    RightSurround,
    LeftBack,
    RightBack,
    Other(u8),
    Lfe,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CodingToolBlockInventory {
    pub vector_id: String,
    pub au_index: usize,
    pub block_index: usize,
    pub channel: SemanticChannel,
    pub provenance: InventoryProvenance,
    pub block_switch: bool,
    pub block_switch_provenance: InventoryProvenance,
    pub exponent_strategy: Option<u8>,
    pub exponent_reused: bool,
    pub exponent_source_au: Option<usize>,
    pub exponent_source_block: Option<usize>,
    pub bandwidth_end_bin: usize,
    pub bap_histogram: Vec<usize>,
    pub bap_zero_count: usize,
    pub grouped_bap_1_count: usize,
    pub grouped_bap_2_count: usize,
    pub grouped_bap_4_count: usize,
    pub dither_enabled: bool,
    pub dither_provenance: InventoryProvenance,
    pub coupling_in_use: bool,
    pub coupling_start_bin: Option<usize>,
    pub coupling_end_bin: Option<usize>,
    pub coupling_coordinates_reused: bool,
    pub coupling_phase_flags: Vec<bool>,
    pub spx_in_use: bool,
    pub spx_source_start: Option<usize>,
    pub spx_source_end: Option<usize>,
    pub spx_coordinates_reused: bool,
    pub rematrix_flags: Vec<bool>,
    pub aht_in_use: bool,
    pub dynrng_present: bool,
    pub dynrng_value: Option<u8>,
    pub mantissa_group_state_used: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CodingToolInventory {
    pub vector_id: String,
    pub au_index: usize,
    pub blocks: Vec<CodingToolBlockInventory>,
}

fn channel_labels(acmod: u8, lfe: bool) -> Vec<SemanticChannel> {
    let mut labels = match acmod {
        0 => vec![SemanticChannel::Left, SemanticChannel::Right],
        1 => vec![SemanticChannel::Centre],
        2 => vec![SemanticChannel::Left, SemanticChannel::Right],
        3 => vec![
            SemanticChannel::Left,
            SemanticChannel::Centre,
            SemanticChannel::Right,
        ],
        4 => vec![
            SemanticChannel::Left,
            SemanticChannel::Right,
            SemanticChannel::Other(3),
        ],
        5 => vec![
            SemanticChannel::Left,
            SemanticChannel::Right,
            SemanticChannel::Other(3),
        ],
        6 => vec![
            SemanticChannel::Left,
            SemanticChannel::Centre,
            SemanticChannel::Right,
            SemanticChannel::Other(3),
        ],
        7 => vec![
            SemanticChannel::Left,
            SemanticChannel::Centre,
            SemanticChannel::Right,
            SemanticChannel::LeftSurround,
            SemanticChannel::RightSurround,
        ],
        _ => (0..8).map(SemanticChannel::Other).collect(),
    };
    if lfe {
        labels.push(SemanticChannel::Lfe);
    }
    labels
}

fn histogram(baps: &[u8]) -> (Vec<usize>, usize, usize, usize, usize) {
    let mut values = vec![0; 16];
    for &bap in baps {
        if let Some(slot) = values.get_mut(usize::from(bap)) {
            *slot += 1;
        }
    }
    let zero = values[0];
    let grouped_1 = baps.iter().filter(|&&bap| bap == 1).count();
    let grouped_2 = baps.iter().filter(|&&bap| bap == 2).count();
    let grouped_4 = baps.iter().filter(|&&bap| bap == 4).count();
    (values, zero, grouped_1, grouped_2, grouped_4)
}

/// Builds a complete diagnostic inventory from parser-emitted block state.
///
/// The returned value is committed only after all six blocks and all channel
/// invariants pass. It is therefore safe for callers to treat this as an
/// atomic AU diagnostic record.
pub fn emit_coding_tool_inventory(
    vector_id: impl Into<String>,
    au_index: usize,
    frame: &AudioFrameInformation,
    blocks: &[DecodedAudioBlock],
) -> Result<CodingToolInventory, Eac3Error> {
    if blocks.len() != usize::from(frame.bsi.header.audio_blocks)
        || blocks.len() != 6
        || frame.full_bandwidth_channels == 0
    {
        return Err(Eac3Error::InvalidAudioBlockSwitchCount {
            expected: 6,
            actual: blocks.len(),
        });
    }
    let vector_id = vector_id.into();
    let labels = channel_labels(frame.bsi.audio_coding_mode, frame.bsi.lfe_on);
    let mut records = Vec::with_capacity(blocks.len() * labels.len());
    for (block_index, block) in blocks.iter().enumerate() {
        if block.block_index != block_index
            || block.channel_baps.len() != usize::from(frame.full_bandwidth_channels)
            || block.prefix.block_switch.len() != usize::from(frame.full_bandwidth_channels)
        {
            return Err(Eac3Error::InvalidAudioBlockChannelCount {
                expected: usize::from(frame.full_bandwidth_channels),
                actual: block.channel_baps.len(),
            });
        }
        for (channel_index, baps) in block.channel_baps.iter().enumerate() {
            let (hist, zero, one, two, four) = histogram(baps);
            let exponent = block.prefix.channel_exponents[channel_index].as_ref();
            let coupling_active =
                block
                    .prefix
                    .coupling
                    .as_ref()
                    .is_some_and(|coupling| match coupling {
                        crate::CouplingInformation::Standard(value) => {
                            value.channel_in_use[channel_index]
                        }
                        crate::CouplingInformation::Enhanced(value) => {
                            value.channel_in_use[channel_index]
                        }
                    });
            let (coupling_start_bin, coupling_end_bin, phase_flags) =
                match block.prefix.coupling.as_ref() {
                    Some(crate::CouplingInformation::Standard(value)) => (
                        Some(usize::from(value.begin_frequency_code)),
                        usize::try_from(value.end_frequency_code).ok(),
                        value.phase_flags.clone(),
                    ),
                    Some(crate::CouplingInformation::Enhanced(value)) => (
                        Some(usize::from(value.begin_subband)),
                        Some(usize::from(value.end_subband)),
                        Vec::new(),
                    ),
                    None => (None, None, Vec::new()),
                };
            let spx = block.prefix.spectral_extension.as_ref();
            let dither_enabled = block
                .prefix
                .dither
                .get(channel_index)
                .copied()
                .unwrap_or(false);
            let exponent_reused = frame
                .channel_exponent_strategy
                .get(block_index)
                .and_then(|strategies| strategies.get(channel_index))
                .copied()
                == Some(0);
            records.push(CodingToolBlockInventory {
                vector_id: vector_id.clone(),
                au_index,
                block_index,
                channel: labels
                    .get(channel_index)
                    .copied()
                    .unwrap_or(SemanticChannel::Other(channel_index as u8)),
                provenance: InventoryProvenance::DerivedFromNormativeState,
                block_switch: block.prefix.block_switch[channel_index],
                block_switch_provenance: InventoryProvenance::ParsedExplicitly,
                exponent_strategy: exponent.map(|value| value.strategy),
                exponent_reused,
                exponent_source_au: if block_index > 0 && exponent_reused {
                    Some(au_index)
                } else {
                    None
                },
                exponent_source_block: if block_index > 0 && exponent_reused {
                    Some(block_index - 1)
                } else {
                    None
                },
                bandwidth_end_bin: baps.len(),
                bap_histogram: hist,
                bap_zero_count: zero,
                grouped_bap_1_count: one,
                grouped_bap_2_count: two,
                grouped_bap_4_count: four,
                dither_enabled,
                dither_provenance: InventoryProvenance::ParsedExplicitly,
                coupling_in_use: coupling_active,
                coupling_start_bin: coupling_active.then_some(coupling_start_bin).flatten(),
                coupling_end_bin: coupling_active.then_some(coupling_end_bin).flatten(),
                coupling_coordinates_reused: coupling_active
                    && block_index > 0
                    && !frame.coupling_strategy_exists[block_index],
                coupling_phase_flags: if coupling_active {
                    phase_flags
                } else {
                    Vec::new()
                },
                spx_in_use: spx.is_some_and(|value| {
                    value
                        .channel_in_use
                        .get(channel_index)
                        .copied()
                        .unwrap_or(false)
                }),
                spx_source_start: spx.map(|value| usize::from(value.begin_subband)),
                spx_source_end: spx.map(|value| usize::from(value.end_subband)),
                spx_coordinates_reused: spx.is_some() && block_index > 0,
                rematrix_flags: if coupling_active {
                    block.prefix.rematrix_flags.clone()
                } else {
                    Vec::new()
                },
                aht_in_use: block
                    .channel_aht
                    .get(channel_index)
                    .and_then(Option::as_ref)
                    .is_some(),
                dynrng_present: block.prefix.dynamic_range.is_some(),
                dynrng_value: block.prefix.dynamic_range,
                mantissa_group_state_used: baps.iter().any(|&bap| matches!(bap, 1 | 2 | 4)),
            });
        }
        if frame.bsi.lfe_on {
            if let Some(baps) = block.lfe_bap.as_ref() {
                let (hist, zero, one, two, four) = histogram(baps);
                records.push(CodingToolBlockInventory {
                    vector_id: vector_id.clone(),
                    au_index,
                    block_index,
                    channel: SemanticChannel::Lfe,
                    provenance: InventoryProvenance::DerivedFromNormativeState,
                    block_switch: false,
                    block_switch_provenance: InventoryProvenance::NotApplicable,
                    exponent_strategy: block
                        .prefix
                        .lfe_exponents
                        .as_ref()
                        .map(|value| value.strategy),
                    exponent_reused: block
                        .prefix
                        .lfe_exponents
                        .as_ref()
                        .is_some_and(|value| value.strategy == 0),
                    exponent_source_au: None,
                    exponent_source_block: None,
                    bandwidth_end_bin: baps.len(),
                    bap_histogram: hist,
                    bap_zero_count: zero,
                    grouped_bap_1_count: one,
                    grouped_bap_2_count: two,
                    grouped_bap_4_count: four,
                    dither_enabled: false,
                    dither_provenance: InventoryProvenance::NotApplicable,
                    coupling_in_use: false,
                    coupling_start_bin: None,
                    coupling_end_bin: None,
                    coupling_coordinates_reused: false,
                    coupling_phase_flags: Vec::new(),
                    spx_in_use: false,
                    spx_source_start: None,
                    spx_source_end: None,
                    spx_coordinates_reused: false,
                    rematrix_flags: Vec::new(),
                    aht_in_use: block.lfe_aht.is_some(),
                    dynrng_present: block.prefix.dynamic_range.is_some(),
                    dynrng_value: block.prefix.dynamic_range,
                    mantissa_group_state_used: baps.iter().any(|&bap| matches!(bap, 1 | 2 | 4)),
                });
            }
        }
    }
    Ok(CodingToolInventory {
        vector_id,
        au_index,
        blocks: records,
    })
}

#[cfg(test)]
mod tests {
    use super::histogram;

    #[test]
    fn bap_histogram_is_derived_from_expanded_values() {
        let (histogram, zero, one, two, four) = histogram(&[0, 1, 2, 4, 4, 7]);
        assert_eq!(histogram.iter().sum::<usize>(), 6);
        assert_eq!(zero, 1);
        assert_eq!(one, 1);
        assert_eq!(two, 1);
        assert_eq!(four, 2);
    }
}
