// pattern: Functional Core

//! Bounded Enhanced AC-3 audio-block syntax traversal.

use openjoc_bitio::{BitRead, BitReader};

use crate::{
    AudioFrameInformation, AuxiliaryData, Eac3Error, StreamType, channel_end_mantissa,
    channel_exponent_group_count, compute_element_bap, decode_exponents, decode_mantissas,
    parse_audio_frame, snr_offsets_are_zero, spx_subband_range,
};

const ENHANCED_COUPLING_SUBBAND_MANTISSA: [usize; 23] = [
    13, 19, 25, 31, 37, 49, 61, 73, 85, 97, 109, 121, 133, 145, 157, 169, 181, 193, 205, 217, 229,
    241, 253,
];
const SPX_SUBBAND_MANTISSA: [usize; 18] = [
    25, 37, 49, 61, 73, 85, 97, 109, 121, 133, 145, 157, 169, 181, 193, 205, 217, 229,
];

const DEFAULT_SPX_BAND_STRUCTURE: [bool; 17] = [
    false, false, false, false, false, false, false, false, true, false, true, false, true, false,
    true, false, true,
];
const DEFAULT_STANDARD_COUPLING_STRUCTURE: [bool; 18] = [
    false, false, false, false, false, false, false, false, true, false, true, true, false, true,
    true, true, true, true,
];
const DEFAULT_ENHANCED_COUPLING_STRUCTURE: [bool; 22] = [
    false, false, false, false, false, false, false, false, false, true, false, true, false, true,
    false, true, true, true, false, true, true, true,
];

/// E.1.2.4 fields through the optional skip field in the first block.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioBlockPrefix {
    pub block_switch: Vec<bool>,
    pub dither: Vec<bool>,
    pub dynamic_range: Option<u8>,
    pub dynamic_range_2: Option<u8>,
    pub spectral_extension: Option<SpectralExtensionInformation>,
    pub coupling: Option<CouplingInformation>,
    pub rematrix_flags: Vec<bool>,
    pub channel_bandwidth_codes: Vec<Option<u8>>,
    pub coupling_exponents: Option<ExponentInformation>,
    pub channel_exponents: Vec<Option<ExponentInformation>>,
    pub lfe_exponents: Option<ExponentInformation>,
    pub bit_allocation_parameters: Option<BitAllocationParameters>,
    pub snr_offsets: Option<SnrOffsets>,
    pub fast_gain_codes: Option<FastGainCodes>,
    pub converter_snr_offset: Option<u16>,
    pub coupling_leak: Option<CouplingLeak>,
    pub delta_bit_allocation: Option<DeltaBitAllocation>,
    pub skip_field: Option<AuxiliaryData>,
    /// Absolute frame bit offset immediately after the optional skip field.
    pub next_offset_bits: usize,
}

/// One newly transmitted E.1.2.4 exponent payload and its decoded bins.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExponentInformation {
    pub strategy: u8,
    pub initial_exponent: u8,
    pub grouped_exponents: Vec<u8>,
    pub start_mantissa: usize,
    pub end_mantissa: usize,
    pub decoded: Vec<u8>,
    pub gain_range: Option<u8>,
}

/// First-block conventional mantissas decoded after the complete side-info
/// prefix. Coupling is emitted once, at the first participating channel, in
/// the same order as clause E.1.2.4's mantissa syntax.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedAudioBlock {
    pub prefix: AudioBlockPrefix,
    pub channel_baps: Vec<Vec<u8>>,
    pub channel_mantissas: Vec<Vec<f64>>,
    pub coupling_bap: Option<Vec<u8>>,
    pub coupling_mantissas: Option<Vec<f64>>,
    pub lfe_bap: Option<Vec<u8>>,
    pub lfe_mantissas: Option<Vec<f64>>,
    /// Absolute frame bit offset immediately after conventional mantissas.
    pub mantissa_end_offset_bits: usize,
}

/// E.1.2.4 bit-allocation parameter codes effective in this block.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BitAllocationParameters {
    pub slow_decay_code: u8,
    pub fast_decay_code: u8,
    pub slow_gain_code: u8,
    pub db_per_bit_code: u8,
    pub floor_code: u8,
}

/// Newly transmitted block SNR-offset codes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnrOffsets {
    pub coarse_code: u8,
    pub coupling_fine_code: Option<u8>,
    pub channel_fine_codes: Vec<u8>,
    pub lfe_fine_code: Option<u8>,
}

/// Effective E.1.2.4 fast-gain codes for the active spectral elements.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FastGainCodes {
    pub coupling: Option<u8>,
    pub channels: Vec<u8>,
    pub lfe: Option<u8>,
}

/// First-block coupling leak initialization codes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CouplingLeak {
    pub fast_code: u8,
    pub slow_code: u8,
}

/// Raw delta-bit-allocation segment syntax for one spectral element.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeltaBitAllocationElement {
    pub strategy: u8,
    pub segments: Vec<DeltaBitAllocationSegment>,
}

/// One raw delta-bit-allocation segment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeltaBitAllocationSegment {
    pub offset: u8,
    pub length: u8,
    pub delta: u8,
}

/// Coupling and full-bandwidth-channel delta-bit-allocation state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeltaBitAllocation {
    pub coupling: Option<DeltaBitAllocationElement>,
    pub channels: Vec<DeltaBitAllocationElement>,
}

/// First-block spectral-extension strategy and coordinate syntax.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpectralExtensionInformation {
    pub channel_in_use: Vec<bool>,
    pub start_copy_frequency_code: u8,
    pub begin_frequency_code: u8,
    pub begin_subband: u8,
    pub end_subband: u8,
    pub band_structure: [bool; 17],
    pub band_count: u8,
    pub coordinates: Vec<Option<SpectralExtensionCoordinates>>,
}

/// Raw E.1.3.3.10 through E.1.3.3.13 SPX coordinate fields.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpectralExtensionCoordinates {
    pub blend: u8,
    pub master: u8,
    pub bands: Vec<(u8, u8)>,
}

/// Standard or enhanced E.1.2.4 coupling state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CouplingInformation {
    Standard(StandardCouplingInformation),
    Enhanced(EnhancedCouplingInformation),
}

/// Raw standard-coupling strategy and coordinate fields.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StandardCouplingInformation {
    pub channel_in_use: Vec<bool>,
    pub phase_flags_in_use: bool,
    pub begin_frequency_code: u8,
    pub end_frequency_code: i8,
    pub subband_count: u8,
    pub band_structure: [bool; 18],
    pub band_count: u8,
    pub coordinates: Vec<Option<StandardCouplingCoordinates>>,
    pub phase_flags: Vec<bool>,
}

/// Raw standard-coupling coordinate fields for one participating channel.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StandardCouplingCoordinates {
    pub master: u8,
    pub bands: Vec<(u8, u8)>,
}

/// Raw enhanced-coupling strategy and amplitude fields.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnhancedCouplingInformation {
    pub channel_in_use: Vec<bool>,
    pub begin_frequency_code: u8,
    pub begin_subband: u8,
    pub end_subband: u8,
    pub band_structure: [bool; 22],
    pub band_count: u8,
    pub amplitudes: Vec<Option<Vec<u8>>>,
}

/// Parses the first `audblk` through the optional skip field.
///
/// This is the first stateful stage of full E.1.2.4 traversal. The returned
/// offset identifies the quantized-mantissa boundary without scanning.
///
/// # Errors
/// Returns an error for malformed frame syntax, truncation, invalid SPX,
/// coupling, or exponent dimensions, or checked cursor arithmetic failure.
pub fn parse_first_audio_block_prefix(bytes: &[u8]) -> Result<AudioBlockPrefix, Eac3Error> {
    let frame = parse_audio_frame(bytes)?;
    let frame_bytes = &bytes[..frame.bsi.header.frame_size];
    let mut bits = BitReader::new(frame_bytes);
    let _consumed = bits.take_bits(frame.audio_blocks_offset_bits)?;
    parse_first_prefix_reader(&mut bits, &frame)
}

/// Decodes the first audio block's conventional bit allocation and mantissas.
///
/// This function starts at `AudioBlockPrefix::next_offset_bits`, computes the
/// normative BAP array for every active element, and then consumes mantissa
/// codewords in the syntax order: each full-bandwidth channel, one coupling
/// channel at the first participating channel, and finally LFE. Adaptive
/// Hybrid Transform payloads are rejected explicitly until their Annex E
/// vector/gain syntax is implemented; no conventional bits are consumed for
/// such a block.
///
/// `dither_values` is a deterministic caller-owned sequence used only for
/// channel dither flags. Values are consumed in channel/LFE syntax order.
///
/// # Errors
/// Returns a checked parser, bit-allocation, mantissa, or unsupported-AHT
/// error. A failure does not expose a partially decoded block.
pub fn decode_first_audio_block(
    bytes: &[u8],
    dither_values: &[f64],
) -> Result<DecodedAudioBlock, Eac3Error> {
    let frame = parse_audio_frame(bytes)?;
    if frame.coupling_aht_in_use
        || frame.channel_aht_in_use.iter().any(|in_use| *in_use)
        || frame.lfe_aht_in_use
    {
        return Err(Eac3Error::UnsupportedAdaptiveHybridTransform);
    }
    let frame_bytes = &bytes[..frame.bsi.header.frame_size];
    let prefix = parse_first_audio_block_prefix(bytes)?;
    let parameter_codes = prefix
        .bit_allocation_parameters
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    let snr = prefix.snr_offsets.clone().or_else(|| {
        frame
            .frame_coarse_snr_code
            .zip(frame.frame_fine_snr_code)
            .map(|(coarse_code, fine)| SnrOffsets {
                coarse_code,
                coupling_fine_code: prefix.coupling.as_ref().map(|_| fine),
                channel_fine_codes: vec![fine; usize::from(frame.full_bandwidth_channels)],
                lfe_fine_code: frame.bsi.lfe_on.then_some(fine),
            })
    });
    let snr = snr.ok_or(Eac3Error::FrameSizeOverflow)?;
    let fast_gain = prefix
        .fast_gain_codes
        .as_ref()
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    let mut fine_codes = Vec::with_capacity(frame.full_bandwidth_channels as usize + 2);
    if let Some(code) = snr.coupling_fine_code {
        fine_codes.push(code);
    }
    fine_codes.extend_from_slice(&snr.channel_fine_codes);
    if let Some(code) = snr.lfe_fine_code {
        fine_codes.push(code);
    }
    let zero_bap = snr_offsets_are_zero(snr.coarse_code, &fine_codes)?;
    let fscod = match frame.bsi.header.sample_rate {
        48_000 => 0,
        44_100 => 1,
        32_000 => 2,
        _ => return Err(Eac3Error::ReservedSampleRate),
    };

    let mut bits = BitReader::new(frame_bytes);
    let _ = bits.take_bits(prefix.next_offset_bits)?;
    let mut dither_index = 0_usize;
    let mut channel_baps = Vec::with_capacity(usize::from(frame.full_bandwidth_channels));
    let mut channel_mantissas = Vec::with_capacity(usize::from(frame.full_bandwidth_channels));
    let mut coupling_bap = None;
    let mut coupling_mantissas = None;
    let mut coupling_decoded = false;

    for channel in 0..usize::from(frame.full_bandwidth_channels) {
        let information = prefix.channel_exponents[channel]
            .as_ref()
            .ok_or(Eac3Error::FrameSizeOverflow)?;
        let baps = element_baps(
            information,
            parameter_codes,
            fast_gain.channels[channel],
            snr.coarse_code,
            *snr.channel_fine_codes
                .get(channel)
                .ok_or(Eac3Error::FrameSizeOverflow)?,
            fscod,
            prefix
                .delta_bit_allocation
                .as_ref()
                .and_then(|delta| delta.channels.get(channel)),
            None,
            zero_bap,
        )?;
        let (baps, exponents) = active_baps_and_exponents(information, baps)?;
        let dither = prefix.dither[channel];
        let mantissas = decode_element_mantissas(
            &mut bits,
            &baps,
            &exponents,
            dither,
            dither_values,
            &mut dither_index,
        )?;
        channel_baps.push(baps);
        channel_mantissas.push(mantissas);

        if !coupling_decoded && channel_uses_coupling(prefix.coupling.as_ref(), channel) {
            let information = prefix
                .coupling_exponents
                .as_ref()
                .ok_or(Eac3Error::FrameSizeOverflow)?;
            let leaks = prefix
                .coupling_leak
                .map(|leak| (leak.fast_code, leak.slow_code));
            let baps = element_baps(
                information,
                parameter_codes,
                fast_gain.coupling.ok_or(Eac3Error::FrameSizeOverflow)?,
                snr.coarse_code,
                snr.coupling_fine_code.ok_or(Eac3Error::FrameSizeOverflow)?,
                fscod,
                prefix
                    .delta_bit_allocation
                    .as_ref()
                    .and_then(|delta| delta.coupling.as_ref()),
                leaks,
                zero_bap,
            )?;
            let (baps, exponents) = active_baps_and_exponents(information, baps)?;
            let dither_flags = vec![false; baps.len()];
            let mantissas = decode_mantissas(&mut bits, &baps, &exponents, &dither_flags, &[])?;
            coupling_bap = Some(baps);
            coupling_mantissas = Some(mantissas);
            coupling_decoded = true;
        }
    }

    let (lfe_bap, lfe_mantissas) = if let Some(information) = prefix.lfe_exponents.as_ref() {
        let baps = element_baps(
            information,
            parameter_codes,
            fast_gain.lfe.ok_or(Eac3Error::FrameSizeOverflow)?,
            snr.coarse_code,
            snr.lfe_fine_code.ok_or(Eac3Error::FrameSizeOverflow)?,
            fscod,
            None,
            None,
            zero_bap,
        )?;
        let (baps, exponents) = active_baps_and_exponents(information, baps)?;
        let dither_flags = vec![false; baps.len()];
        let mantissas = decode_mantissas(&mut bits, &baps, &exponents, &dither_flags, &[])?;
        (Some(baps), Some(mantissas))
    } else {
        (None, None)
    };
    let frame_bits = frame
        .bsi
        .header
        .frame_size
        .checked_mul(8)
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    let mantissa_end_offset_bits = frame_bits
        .checked_sub(bits.bits_remaining())
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    Ok(DecodedAudioBlock {
        prefix,
        channel_baps,
        channel_mantissas,
        coupling_bap,
        coupling_mantissas,
        lfe_bap,
        lfe_mantissas,
        mantissa_end_offset_bits,
    })
}

fn element_baps(
    information: &ExponentInformation,
    parameter_codes: BitAllocationParameters,
    fast_gain_code: u8,
    coarse_snr_code: u8,
    fine_snr_code: u8,
    fscod: u8,
    delta: Option<&DeltaBitAllocationElement>,
    coupling_leaks: Option<(u8, u8)>,
    zero_bap: bool,
) -> Result<Vec<u8>, Eac3Error> {
    let exponents = full_exponents(information)?;
    if zero_bap {
        return Ok(vec![0; information.end_mantissa]);
    }
    compute_element_bap(
        &exponents,
        information.start_mantissa,
        information.end_mantissa,
        parameter_codes,
        fast_gain_code,
        coarse_snr_code,
        fine_snr_code,
        fscod,
        delta,
        coupling_leaks,
    )
}

fn full_exponents(information: &ExponentInformation) -> Result<Vec<u8>, Eac3Error> {
    if information.start_mantissa > information.end_mantissa
        || information.decoded.len()
            != information
                .end_mantissa
                .saturating_sub(information.start_mantissa)
    {
        return Err(Eac3Error::MantissaExponentLengthMismatch {
            baps: information
                .end_mantissa
                .saturating_sub(information.start_mantissa),
            exponents: information.decoded.len(),
        });
    }
    let mut exponents = vec![0_u8; information.end_mantissa];
    exponents[information.start_mantissa..information.end_mantissa]
        .copy_from_slice(&information.decoded);
    Ok(exponents)
}

fn active_baps_and_exponents(
    information: &ExponentInformation,
    baps: Vec<u8>,
) -> Result<(Vec<u8>, Vec<u8>), Eac3Error> {
    let start = information.start_mantissa;
    let end = information.end_mantissa;
    let active_baps = baps
        .get(start..end)
        .ok_or(Eac3Error::MantissaExponentLengthMismatch {
            baps: baps.len(),
            exponents: end.saturating_sub(start),
        })?
        .to_vec();
    Ok((active_baps, information.decoded.clone()))
}

fn decode_element_mantissas(
    bits: &mut BitReader<'_>,
    baps: &[u8],
    exponents: &[u8],
    dither: bool,
    dither_values: &[f64],
    dither_index: &mut usize,
) -> Result<Vec<f64>, Eac3Error> {
    let dither_flags = vec![dither; baps.len()];
    let needed = if dither {
        baps.iter().filter(|bap| **bap == 0).count()
    } else {
        0
    };
    let end = dither_index
        .checked_add(needed)
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    let values = dither_values
        .get(*dither_index..end)
        .ok_or(Eac3Error::MissingDitherValue {
            index: *dither_index,
        })?;
    let decoded = decode_mantissas(bits, baps, exponents, &dither_flags, values)?;
    *dither_index = end;
    Ok(decoded)
}

fn channel_uses_coupling(coupling: Option<&CouplingInformation>, channel: usize) -> bool {
    match coupling {
        Some(CouplingInformation::Standard(info)) => info.channel_in_use[channel],
        Some(CouplingInformation::Enhanced(info)) => info.channel_in_use[channel],
        None => false,
    }
}

fn parse_first_prefix_reader(
    bits: &mut BitReader<'_>,
    frame: &AudioFrameInformation,
) -> Result<AudioBlockPrefix, Eac3Error> {
    let channels = usize::from(frame.full_bandwidth_channels);
    let block_switch = read_flags_or_default(bits, channels, frame.syntax.block_switch(), false)?;
    let dither = read_flags_or_default(bits, channels, frame.syntax.dither(), true)?;
    let dynamic_range = read_optional_u8(bits, 8)?;
    let dynamic_range_2 = if frame.bsi.audio_coding_mode == 0 {
        read_optional_u8(bits, 8)?
    } else {
        None
    };
    let spectral_extension = if bits.read_bit()? {
        Some(parse_first_spx(bits, frame, channels)?)
    } else {
        None
    };
    let coupling = if frame.coupling_in_use[0] {
        Some(parse_first_coupling(
            bits,
            frame,
            channels,
            spectral_extension.as_ref(),
        )?)
    } else {
        None
    };
    let rematrix_flags = if frame.bsi.audio_coding_mode == 2 {
        read_flags_or_default(
            bits,
            usize::from(rematrix_band_count(
                coupling.as_ref(),
                spectral_extension.as_ref(),
            )),
            true,
            false,
        )?
    } else {
        Vec::new()
    };
    let (channel_bandwidth_codes, channel_end_mantissas) = parse_channel_bandwidths(
        bits,
        coupling.as_ref(),
        spectral_extension.as_ref(),
        channels,
    )?;
    let coupling_exponents = parse_coupling_exponents(bits, frame, coupling.as_ref())?;
    let channel_exponents = parse_channel_exponents(bits, frame, &channel_end_mantissas)?;
    let lfe_exponents = parse_lfe_exponents(bits, frame)?;
    let bit_allocation_parameters = parse_bit_allocation_parameters(bits, frame)?;
    let snr_offsets = parse_snr_offsets(bits, frame, coupling.as_ref(), channels)?;
    let fast_gain_codes = parse_fast_gain_codes(bits, frame, coupling.as_ref(), channels)?;
    let converter_snr_offset = parse_converter_snr_offset(bits, frame)?;
    let coupling_leak = parse_first_coupling_leak(bits, coupling.as_ref())?;
    let delta_bit_allocation =
        parse_delta_bit_allocation(bits, frame, coupling.as_ref(), channels)?;
    let skip_field = parse_skip_field(bits, frame)?;
    let frame_bits = frame
        .bsi
        .header
        .frame_size
        .checked_mul(8)
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    let next_offset_bits = frame_bits
        .checked_sub(bits.bits_remaining())
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    Ok(AudioBlockPrefix {
        block_switch,
        dither,
        dynamic_range,
        dynamic_range_2,
        spectral_extension,
        coupling,
        rematrix_flags,
        channel_bandwidth_codes,
        coupling_exponents,
        channel_exponents,
        lfe_exponents,
        bit_allocation_parameters,
        snr_offsets,
        fast_gain_codes,
        converter_snr_offset,
        coupling_leak,
        delta_bit_allocation,
        skip_field,
        next_offset_bits,
    })
}

fn parse_channel_bandwidths(
    bits: &mut BitReader<'_>,
    coupling: Option<&CouplingInformation>,
    spx: Option<&SpectralExtensionInformation>,
    channels: usize,
) -> Result<(Vec<Option<u8>>, Vec<usize>), Eac3Error> {
    let mut codes = Vec::with_capacity(channels);
    let mut end_mantissas = Vec::with_capacity(channels);
    for channel in 0..channels {
        let coupling_end = match coupling {
            Some(CouplingInformation::Standard(value)) if value.channel_in_use[channel] => Some(
                usize::from(value.begin_frequency_code)
                    .checked_mul(12)
                    .and_then(|value| value.checked_add(37))
                    .ok_or(Eac3Error::FrameSizeOverflow)?,
            ),
            Some(CouplingInformation::Enhanced(value)) if value.channel_in_use[channel] => Some(
                *ENHANCED_COUPLING_SUBBAND_MANTISSA
                    .get(usize::from(value.begin_subband))
                    .ok_or(Eac3Error::InvalidCouplingRange {
                        begin: i16::from(value.begin_subband),
                        end: i16::from(value.end_subband),
                    })?,
            ),
            Some(CouplingInformation::Standard(_) | CouplingInformation::Enhanced(_)) | None => {
                None
            }
        };
        let spx_end = spx.and_then(|information| {
            information
                .channel_in_use
                .get(channel)
                .copied()
                .filter(|in_use| *in_use)
                .and_then(|_| {
                    SPX_SUBBAND_MANTISSA
                        .get(usize::from(information.begin_subband))
                        .copied()
                })
        });
        if let Some(end_mantissa) = coupling_end.or(spx_end) {
            codes.push(None);
            end_mantissas.push(end_mantissa);
        } else {
            let code = read_u8(bits, 6)?;
            codes.push(Some(code));
            end_mantissas.push(channel_end_mantissa(code)?);
        }
    }
    Ok((codes, end_mantissas))
}

fn parse_coupling_exponents(
    bits: &mut BitReader<'_>,
    frame: &AudioFrameInformation,
    coupling: Option<&CouplingInformation>,
) -> Result<Option<ExponentInformation>, Eac3Error> {
    let Some(coupling) = coupling else {
        return Ok(None);
    };
    let strategy = frame.coupling_exponent_strategy[0];
    let (start_mantissa, end_mantissa) = match coupling {
        CouplingInformation::Standard(value) => {
            let start = usize::from(value.begin_frequency_code)
                .checked_mul(12)
                .and_then(|value| value.checked_add(37))
                .ok_or(Eac3Error::FrameSizeOverflow)?;
            let end_code = i16::from(value.end_frequency_code) + 3;
            let end_code =
                usize::try_from(end_code).map_err(|_| Eac3Error::InvalidCouplingRange {
                    begin: i16::from(value.begin_frequency_code),
                    end: i16::from(value.end_frequency_code),
                })?;
            let end = end_code
                .checked_mul(12)
                .and_then(|value| value.checked_add(37))
                .ok_or(Eac3Error::FrameSizeOverflow)?;
            (start, end)
        }
        CouplingInformation::Enhanced(value) => (
            *ENHANCED_COUPLING_SUBBAND_MANTISSA
                .get(usize::from(value.begin_subband))
                .ok_or(Eac3Error::InvalidCouplingRange {
                    begin: i16::from(value.begin_subband),
                    end: i16::from(value.end_subband),
                })?,
            *ENHANCED_COUPLING_SUBBAND_MANTISSA
                .get(usize::from(value.end_subband))
                .ok_or(Eac3Error::InvalidCouplingRange {
                    begin: i16::from(value.begin_subband),
                    end: i16::from(value.end_subband),
                })?,
        ),
    };
    let initial_exponent = read_u8(bits, 4)?
        .checked_mul(2)
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    let length =
        end_mantissa
            .checked_sub(start_mantissa)
            .ok_or(Eac3Error::InvalidCouplingRange {
                begin: i16::try_from(start_mantissa).unwrap_or(i16::MAX),
                end: i16::try_from(end_mantissa).unwrap_or(i16::MAX),
            })?;
    let decoded_length = length.checked_add(1).ok_or(Eac3Error::FrameSizeOverflow)?;
    let group_count = channel_exponent_group_count(decoded_length, strategy)?;
    let grouped_exponents = read_grouped_exponents(bits, group_count)?;
    let mut decoded = decode_exponents(
        initial_exponent,
        &grouped_exponents,
        strategy,
        decoded_length,
    )?;
    decoded.remove(0);
    Ok(Some(ExponentInformation {
        strategy,
        initial_exponent,
        grouped_exponents,
        start_mantissa,
        end_mantissa,
        decoded,
        gain_range: None,
    }))
}

fn parse_channel_exponents(
    bits: &mut BitReader<'_>,
    frame: &AudioFrameInformation,
    end_mantissas: &[usize],
) -> Result<Vec<Option<ExponentInformation>>, Eac3Error> {
    end_mantissas
        .iter()
        .copied()
        .enumerate()
        .map(|(channel, end_mantissa)| {
            let strategy = frame.channel_exponent_strategy[0][channel];
            let initial_exponent = read_u8(bits, 4)?;
            let group_count = channel_exponent_group_count(end_mantissa, strategy)?;
            let grouped_exponents = read_grouped_exponents(bits, group_count)?;
            let decoded =
                decode_exponents(initial_exponent, &grouped_exponents, strategy, end_mantissa)?;
            let gain_range = read_u8(bits, 2)?;
            Ok(Some(ExponentInformation {
                strategy,
                initial_exponent,
                grouped_exponents,
                start_mantissa: 0,
                end_mantissa,
                decoded,
                gain_range: Some(gain_range),
            }))
        })
        .collect()
}

fn parse_lfe_exponents(
    bits: &mut BitReader<'_>,
    frame: &AudioFrameInformation,
) -> Result<Option<ExponentInformation>, Eac3Error> {
    if !frame.bsi.lfe_on {
        return Ok(None);
    }
    if !frame.lfe_exponent_strategy[0] {
        return Err(Eac3Error::InvalidExponentStrategy { actual: 0 });
    }
    let strategy = 1;
    let initial_exponent = read_u8(bits, 4)?;
    let grouped_exponents = read_grouped_exponents(bits, 2)?;
    let decoded = decode_exponents(initial_exponent, &grouped_exponents, strategy, 7)?;
    Ok(Some(ExponentInformation {
        strategy,
        initial_exponent,
        grouped_exponents,
        start_mantissa: 0,
        end_mantissa: 7,
        decoded,
        gain_range: None,
    }))
}

fn read_grouped_exponents(bits: &mut BitReader<'_>, count: usize) -> Result<Vec<u8>, Eac3Error> {
    (0..count).map(|_| read_u8(bits, 7)).collect()
}

fn parse_bit_allocation_parameters(
    bits: &mut BitReader<'_>,
    frame: &AudioFrameInformation,
) -> Result<Option<BitAllocationParameters>, Eac3Error> {
    if !frame.syntax.bit_allocation() {
        return Ok(Some(BitAllocationParameters {
            slow_decay_code: 2,
            fast_decay_code: 1,
            slow_gain_code: 1,
            db_per_bit_code: 2,
            floor_code: 7,
        }));
    }
    if !bits.read_bit()? {
        return Ok(None);
    }
    Ok(Some(BitAllocationParameters {
        slow_decay_code: read_u8(bits, 2)?,
        fast_decay_code: read_u8(bits, 2)?,
        slow_gain_code: read_u8(bits, 2)?,
        db_per_bit_code: read_u8(bits, 2)?,
        floor_code: read_u8(bits, 3)?,
    }))
}

fn parse_snr_offsets(
    bits: &mut BitReader<'_>,
    frame: &AudioFrameInformation,
    coupling: Option<&CouplingInformation>,
    channels: usize,
) -> Result<Option<SnrOffsets>, Eac3Error> {
    let strategy = frame.snr_offset_strategy;
    if strategy == 0 {
        return Ok(None);
    }
    let coarse_code = read_u8(bits, 6)?;
    let (coupling_fine_code, channel_fine_codes, lfe_fine_code) = if strategy == 1 {
        let fine = read_u8(bits, 4)?;
        (
            coupling.map(|_| fine),
            vec![fine; channels],
            frame.bsi.lfe_on.then_some(fine),
        )
    } else {
        let coupling_fine = coupling.map(|_| read_u8(bits, 4)).transpose()?;
        let channel_fine = (0..channels)
            .map(|_| read_u8(bits, 4))
            .collect::<Result<Vec<_>, _>>()?;
        let lfe_fine = frame.bsi.lfe_on.then(|| read_u8(bits, 4)).transpose()?;
        (coupling_fine, channel_fine, lfe_fine)
    };
    Ok(Some(SnrOffsets {
        coarse_code,
        coupling_fine_code,
        channel_fine_codes,
        lfe_fine_code,
    }))
}

fn parse_fast_gain_codes(
    bits: &mut BitReader<'_>,
    frame: &AudioFrameInformation,
    coupling: Option<&CouplingInformation>,
    channels: usize,
) -> Result<Option<FastGainCodes>, Eac3Error> {
    let new_codes = frame.syntax.frame_fast_gain() && bits.read_bit()?;
    if !new_codes {
        return Ok(Some(FastGainCodes {
            coupling: coupling.map(|_| 4),
            channels: vec![4; channels],
            lfe: frame.bsi.lfe_on.then_some(4),
        }));
    }
    let coupling_code = coupling.map(|_| read_u8(bits, 3)).transpose()?;
    let channel_codes = (0..channels)
        .map(|_| read_u8(bits, 3))
        .collect::<Result<Vec<_>, _>>()?;
    let lfe_code = frame.bsi.lfe_on.then(|| read_u8(bits, 3)).transpose()?;
    Ok(Some(FastGainCodes {
        coupling: coupling_code,
        channels: channel_codes,
        lfe: lfe_code,
    }))
}

fn parse_converter_snr_offset(
    bits: &mut BitReader<'_>,
    frame: &AudioFrameInformation,
) -> Result<Option<u16>, Eac3Error> {
    if frame.bsi.header.stream_type != StreamType::Independent || !bits.read_bit()? {
        return Ok(None);
    }
    u16::try_from(bits.read_bits(10)?)
        .map(Some)
        .map_err(|_| Eac3Error::FrameSizeOverflow)
}

fn parse_first_coupling_leak(
    bits: &mut BitReader<'_>,
    coupling: Option<&CouplingInformation>,
) -> Result<Option<CouplingLeak>, Eac3Error> {
    if coupling.is_none() {
        return Ok(None);
    }
    Ok(Some(CouplingLeak {
        fast_code: read_u8(bits, 3)?,
        slow_code: read_u8(bits, 3)?,
    }))
}

fn parse_delta_bit_allocation(
    bits: &mut BitReader<'_>,
    frame: &AudioFrameInformation,
    coupling: Option<&CouplingInformation>,
    channels: usize,
) -> Result<Option<DeltaBitAllocation>, Eac3Error> {
    if !frame.syntax.delta_bit_allocation() {
        return Ok(None);
    }
    if !bits.read_bit()? {
        return Ok(Some(DeltaBitAllocation {
            coupling: coupling.map(|_| no_delta_allocation()),
            channels: (0..channels).map(|_| no_delta_allocation()).collect(),
        }));
    }
    let coupling_strategy = coupling.map(|_| read_delta_strategy(bits)).transpose()?;
    let channel_strategies = (0..channels)
        .map(|_| read_delta_strategy(bits))
        .collect::<Result<Vec<_>, _>>()?;
    let coupling = coupling_strategy
        .map(|strategy| parse_delta_element(bits, strategy))
        .transpose()?;
    let channels = channel_strategies
        .into_iter()
        .map(|strategy| parse_delta_element(bits, strategy))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(DeltaBitAllocation { coupling, channels }))
}

fn read_delta_strategy(bits: &mut BitReader<'_>) -> Result<u8, Eac3Error> {
    let strategy = read_u8(bits, 2)?;
    if !matches!(strategy, 1 | 2) {
        return Err(Eac3Error::InvalidDeltaBitAllocationStrategy { actual: strategy });
    }
    Ok(strategy)
}

fn parse_delta_element(
    bits: &mut BitReader<'_>,
    strategy: u8,
) -> Result<DeltaBitAllocationElement, Eac3Error> {
    let segments = if strategy == 1 {
        let count = usize::from(read_u8(bits, 3)?) + 1;
        (0..count)
            .map(|_| {
                Ok(DeltaBitAllocationSegment {
                    offset: read_u8(bits, 5)?,
                    length: read_u8(bits, 4)?,
                    delta: read_u8(bits, 3)?,
                })
            })
            .collect::<Result<Vec<_>, Eac3Error>>()?
    } else {
        Vec::new()
    };
    Ok(DeltaBitAllocationElement { strategy, segments })
}

fn no_delta_allocation() -> DeltaBitAllocationElement {
    DeltaBitAllocationElement {
        strategy: 2,
        segments: Vec::new(),
    }
}

fn parse_skip_field(
    bits: &mut BitReader<'_>,
    frame: &AudioFrameInformation,
) -> Result<Option<AuxiliaryData>, Eac3Error> {
    if !frame.syntax.skip_field() || !bits.read_bit()? {
        return Ok(None);
    }
    let byte_len = usize::try_from(bits.read_bits(9)?).map_err(|_| Eac3Error::FrameSizeOverflow)?;
    let bit_len = byte_len
        .checked_mul(8)
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    let bytes = (0..byte_len)
        .map(|_| read_u8(bits, 8))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(AuxiliaryData { bit_len, bytes }))
}

fn rematrix_band_count(
    coupling: Option<&CouplingInformation>,
    spx: Option<&SpectralExtensionInformation>,
) -> u8 {
    match coupling {
        Some(CouplingInformation::Enhanced(info)) => match info.begin_frequency_code {
            0 => 0,
            1 => 1,
            2 => 2,
            3 | 4 => 3,
            _ => 4,
        },
        Some(CouplingInformation::Standard(info)) => match info.begin_frequency_code {
            0 => 2,
            1 | 2 => 3,
            _ => 4,
        },
        None => match spx {
            Some(info) if info.begin_frequency_code < 2 => 3,
            Some(_) | None => 4,
        },
    }
}

fn parse_first_spx(
    bits: &mut BitReader<'_>,
    frame: &AudioFrameInformation,
    channels: usize,
) -> Result<SpectralExtensionInformation, Eac3Error> {
    let channel_in_use = if frame.bsi.audio_coding_mode == 1 {
        vec![true]
    } else {
        read_flags_or_default(bits, channels, true, false)?
    };
    let start_copy_frequency_code = read_u8(bits, 2)?;
    let begin_code = read_u8(bits, 3)?;
    let end_code = read_u8(bits, 3)?;
    let (begin_subband, end_subband) = spx_subband_range(begin_code, end_code)?;
    let mut band_structure = DEFAULT_SPX_BAND_STRUCTURE;
    if bits.read_bit()? {
        for subband in begin_subband + 1..end_subband {
            band_structure[usize::from(subband)] = bits.read_bit()?;
        }
    }
    let additional_bands = (begin_subband + 1..end_subband)
        .filter(|subband| !band_structure[usize::from(*subband)])
        .count();
    let band_count = u8::try_from(additional_bands)
        .map_err(|_| Eac3Error::FrameSizeOverflow)?
        .checked_add(1)
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    let mut coordinates = vec![None; channels];
    for (channel, in_use) in channel_in_use.iter().copied().enumerate() {
        if in_use {
            coordinates[channel] = Some(read_spx_coordinates(bits, band_count)?);
        }
    }
    Ok(SpectralExtensionInformation {
        channel_in_use,
        start_copy_frequency_code,
        begin_frequency_code: begin_code,
        begin_subband,
        end_subband,
        band_structure,
        band_count,
        coordinates,
    })
}

fn parse_first_coupling(
    bits: &mut BitReader<'_>,
    frame: &AudioFrameInformation,
    channels: usize,
    spx: Option<&SpectralExtensionInformation>,
) -> Result<CouplingInformation, Eac3Error> {
    let enhanced = bits.read_bit()?;
    let channel_in_use = if frame.bsi.audio_coding_mode == 2 {
        vec![true, true]
    } else {
        read_flags_or_default(bits, channels, true, false)?
    };
    if enhanced {
        parse_enhanced_coupling(bits, channel_in_use, spx).map(CouplingInformation::Enhanced)
    } else {
        parse_standard_coupling(bits, frame, channel_in_use, spx).map(CouplingInformation::Standard)
    }
}

fn parse_standard_coupling(
    bits: &mut BitReader<'_>,
    frame: &AudioFrameInformation,
    channel_in_use: Vec<bool>,
    spx: Option<&SpectralExtensionInformation>,
) -> Result<StandardCouplingInformation, Eac3Error> {
    let phase_flags_in_use = frame.bsi.audio_coding_mode == 2 && bits.read_bit()?;
    let begin_frequency_code = read_u8(bits, 4)?;
    let end_frequency_code = if let Some(spx) = spx {
        let code =
            i8::try_from(spx.begin_frequency_code).map_err(|_| Eac3Error::FrameSizeOverflow)?;
        if code < 6 { code - 2 } else { code * 2 - 7 }
    } else {
        i8::try_from(read_u8(bits, 4)?).map_err(|_| Eac3Error::FrameSizeOverflow)?
    };
    let begin = i16::from(begin_frequency_code);
    let end = i16::from(end_frequency_code);
    let subband_count = 3_i16 + end - begin;
    if !(1..=18).contains(&subband_count) {
        return Err(Eac3Error::InvalidCouplingRange { begin, end });
    }
    let subband_count = u8::try_from(subband_count).map_err(|_| Eac3Error::FrameSizeOverflow)?;
    let mut band_structure = DEFAULT_STANDARD_COUPLING_STRUCTURE;
    if bits.read_bit()? {
        for band in 1..subband_count {
            band_structure[usize::from(band)] = bits.read_bit()?;
        }
    }
    let band_count = count_unmerged(&band_structure[..usize::from(subband_count)])?;
    let mut coordinates = vec![None; channel_in_use.len()];
    for (channel, in_use) in channel_in_use.iter().copied().enumerate() {
        if in_use {
            let master = read_u8(bits, 2)?;
            let mut bands = Vec::with_capacity(usize::from(band_count));
            for _ in 0..band_count {
                bands.push((read_u8(bits, 4)?, read_u8(bits, 4)?));
            }
            coordinates[channel] = Some(StandardCouplingCoordinates { master, bands });
        }
    }
    let phase_flags = if phase_flags_in_use && coordinates.iter().any(Option::is_some) {
        read_flags_or_default(bits, usize::from(band_count), true, false)?
    } else {
        Vec::new()
    };
    Ok(StandardCouplingInformation {
        channel_in_use,
        phase_flags_in_use,
        begin_frequency_code,
        end_frequency_code,
        subband_count,
        band_structure,
        band_count,
        coordinates,
        phase_flags,
    })
}

fn parse_enhanced_coupling(
    bits: &mut BitReader<'_>,
    channel_in_use: Vec<bool>,
    spx: Option<&SpectralExtensionInformation>,
) -> Result<EnhancedCouplingInformation, Eac3Error> {
    let begin_frequency_code = read_u8(bits, 4)?;
    let begin_subband = if begin_frequency_code < 3 {
        begin_frequency_code * 2
    } else if begin_frequency_code < 13 {
        begin_frequency_code + 2
    } else {
        begin_frequency_code * 2 - 10
    };
    let end_subband = if let Some(spx) = spx {
        if spx.begin_frequency_code < 6 {
            spx.begin_frequency_code + 5
        } else {
            spx.begin_frequency_code * 2
        }
    } else {
        read_u8(bits, 4)? + 7
    };
    if begin_subband >= end_subband || end_subband > 22 {
        return Err(Eac3Error::InvalidCouplingRange {
            begin: i16::from(begin_subband),
            end: i16::from(end_subband),
        });
    }
    let mut band_structure = DEFAULT_ENHANCED_COUPLING_STRUCTURE;
    if bits.read_bit()? {
        for subband in (begin_subband + 1).max(9)..end_subband {
            band_structure[usize::from(subband)] = bits.read_bit()?;
        }
    }
    let band_count =
        count_unmerged(&band_structure[usize::from(begin_subband)..usize::from(end_subband)])?;
    read_zero_bits(bits, 1)?;
    let first_channel = channel_in_use.iter().position(|in_use| *in_use).ok_or(
        Eac3Error::InvalidCouplingRange {
            begin: i16::from(begin_subband),
            end: i16::from(end_subband),
        },
    )?;
    let mut amplitudes = vec![None; channel_in_use.len()];
    for (channel, in_use) in channel_in_use.iter().copied().enumerate() {
        if in_use {
            let mut values = Vec::with_capacity(usize::from(band_count));
            for _ in 0..band_count {
                values.push(read_u8(bits, 5)?);
            }
            amplitudes[channel] = Some(values);
            if channel > first_channel {
                let reserved = usize::from(band_count.saturating_sub(1))
                    .checked_mul(9)
                    .ok_or(Eac3Error::FrameSizeOverflow)?;
                read_zero_bits(bits, reserved)?;
                read_zero_bits(bits, 1)?;
            }
        }
    }
    Ok(EnhancedCouplingInformation {
        channel_in_use,
        begin_frequency_code,
        begin_subband,
        end_subband,
        band_structure,
        band_count,
        amplitudes,
    })
}

fn count_unmerged(structure: &[bool]) -> Result<u8, Eac3Error> {
    u8::try_from(structure.iter().filter(|merged| !**merged).count())
        .map_err(|_| Eac3Error::FrameSizeOverflow)
}

fn read_zero_bits(bits: &mut BitReader<'_>, count: usize) -> Result<(), Eac3Error> {
    for _ in 0..count {
        if bits.read_bit()? {
            return Err(Eac3Error::NonzeroReservedData);
        }
    }
    Ok(())
}

fn read_spx_coordinates(
    bits: &mut BitReader<'_>,
    band_count: u8,
) -> Result<SpectralExtensionCoordinates, Eac3Error> {
    let blend = read_u8(bits, 5)?;
    let master = read_u8(bits, 2)?;
    let mut bands = Vec::with_capacity(usize::from(band_count));
    for _ in 0..band_count {
        bands.push((read_u8(bits, 4)?, read_u8(bits, 2)?));
    }
    Ok(SpectralExtensionCoordinates {
        blend,
        master,
        bands,
    })
}

fn read_flags_or_default(
    bits: &mut BitReader<'_>,
    count: usize,
    encoded: bool,
    default: bool,
) -> Result<Vec<bool>, Eac3Error> {
    if !encoded {
        return Ok(vec![default; count]);
    }
    (0..count)
        .map(|_| bits.read_bit().map_err(Eac3Error::from))
        .collect()
}

fn read_optional_u8(bits: &mut BitReader<'_>, width: u8) -> Result<Option<u8>, Eac3Error> {
    if bits.read_bit()? {
        Ok(Some(read_u8(bits, width)?))
    } else {
        Ok(None)
    }
}

fn read_u8(bits: &mut BitReader<'_>, width: u8) -> Result<u8, Eac3Error> {
    u8::try_from(bits.read_bits(width)?).map_err(|_| Eac3Error::FrameSizeOverflow)
}
