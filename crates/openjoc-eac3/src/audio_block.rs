// pattern: Functional Core

//! Bounded Enhanced AC-3 audio-block syntax traversal.

use openjoc_bitio::{BitRead, BitReader};

use crate::{AudioFrameInformation, Eac3Error, parse_audio_frame, spx_subband_range};

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

/// E.1.2.4 fields preceding coupling-strategy information in the first block.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioBlockPrefix {
    pub block_switch: Vec<bool>,
    pub dither: Vec<bool>,
    pub dynamic_range: Option<u8>,
    pub dynamic_range_2: Option<u8>,
    pub spectral_extension: Option<SpectralExtensionInformation>,
    pub coupling: Option<CouplingInformation>,
    /// Absolute frame bit offset immediately after coupling coordinates.
    pub next_offset_bits: usize,
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

/// Parses the first `audblk` through the terminal SPX coordinate fields.
///
/// This is the first stateful stage of full E.1.2.4 traversal. The returned
/// offset identifies the coupling-strategy boundary without scanning bytes.
///
/// # Errors
/// Returns an error for malformed frame syntax, truncation, invalid SPX
/// dimensions, or checked cursor arithmetic failure.
pub fn parse_first_audio_block_prefix(bytes: &[u8]) -> Result<AudioBlockPrefix, Eac3Error> {
    let frame = parse_audio_frame(bytes)?;
    let frame_bytes = &bytes[..frame.bsi.header.frame_size];
    let mut bits = BitReader::new(frame_bytes);
    let _consumed = bits.take_bits(frame.audio_blocks_offset_bits)?;
    parse_first_prefix_reader(&mut bits, &frame)
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
        next_offset_bits,
    })
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
