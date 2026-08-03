// pattern: Functional Core

//! ETSI TS 102 366 Annex E.2.6.4 spectral-extension synthesis.

use crate::{Eac3Error, SpectralExtensionCoordinates, SpectralExtensionInformation};

/// Low transform-coefficient boundaries from Table E.2.11 (`spxbandtable`).
/// The final entry is the exclusive boundary after sub-band 16; sub-band 17
/// is the table's sentinel row and has no high coefficient number.
const SPX_BAND_TABLE: [usize; 18] = [
    25, 37, 49, 61, 73, 85, 97, 109, 121, 133, 145, 157, 169, 181, 193, 205, 217, 229,
];

/// Table E.2.12 attenuation values. The last two taps of the five-tap notch
/// are applied by symmetry, as required by E.2.6.4.2.3.
const SPX_ATTENUATION_TABLE: [[f64; 3]; 32] = [
    [0.954841604, 0.911722489, 0.870550563],
    [0.911722489, 0.831237896, 0.757858283],
    [0.870550563, 0.757858283, 0.659753955],
    [0.831237896, 0.690956440, 0.574349177],
    [0.793700526, 0.629960525, 0.500000000],
    [0.757858283, 0.574349177, 0.435275282],
    [0.723634619, 0.523647061, 0.378929142],
    [0.690956440, 0.477420802, 0.329876978],
    [0.659753955, 0.435275282, 0.287174589],
    [0.629960525, 0.396850263, 0.250000000],
    [0.601512518, 0.361817309, 0.217637641],
    [0.574349177, 0.329876978, 0.189464571],
    [0.548412490, 0.300756259, 0.164938489],
    [0.523647061, 0.274206245, 0.143587294],
    [0.500000000, 0.250000000, 0.125000000],
    [0.477420802, 0.227930622, 0.108818820],
    [0.455861244, 0.207809474, 0.094732285],
    [0.435275282, 0.189464571, 0.082469244],
    [0.415618948, 0.172739110, 0.071797364],
    [0.396850263, 0.157490131, 0.062500000],
    [0.378929142, 0.143587294, 0.054409410],
    [0.361817309, 0.130911765, 0.047366143],
    [0.345478220, 0.119355200, 0.041234622],
    [0.329876978, 0.108818820, 0.035898624],
    [0.314980262, 0.099212566, 0.031250000],
    [0.300756259, 0.090454327, 0.027204705],
    [0.287174589, 0.082469244, 0.023683071],
    [0.274206245, 0.075189065, 0.020671311],
    [0.261823531, 0.068555161, 0.017948412],
    [0.250000000, 0.062500000, 0.015625000],
    [0.238710401, 0.056982656, 0.013602353],
    [0.227930622, 0.051952369, 0.011841536],
];

/// Synthesizes one channel's high-frequency transform coefficients.
///
/// This is the pure E.2.6.4 path: band grouping, coefficient translation,
/// optional Table E.2.12 attenuation, banded RMS/noise blending, and the
/// final coordinate scale (`spxco * 32`). `base` contains coefficients below
/// `spx_begin_subbnd`; `noise` supplies one zero-mean, unit-variance sample
/// per inserted coefficient. A caller that wants a deterministic decoder may
/// provide a deterministic noise sequence without changing the normative
/// synthesis operations.
pub fn synthesize_spectral_extension(
    base: &[f64],
    information: &SpectralExtensionInformation,
    coordinates: &SpectralExtensionCoordinates,
    attenuation_code: Option<u8>,
    noise: &[f64],
) -> Result<Vec<f64>, Eac3Error> {
    let begin = usize::from(information.begin_subband);
    let end = usize::from(information.end_subband);
    let copy_start = information.start_copy_frequency_code as usize;
    let copy_end = *SPX_BAND_TABLE
        .get(begin)
        .ok_or(Eac3Error::InvalidSpectralExtensionRange {
            begin: information.begin_subband,
            end: information.end_subband,
        })?;
    let insert_end = *SPX_BAND_TABLE
        .get(end)
        .ok_or(Eac3Error::InvalidSpectralExtensionRange {
            begin: information.begin_subband,
            end: information.end_subband,
        })?;
    let copy_start =
        *SPX_BAND_TABLE
            .get(copy_start)
            .ok_or(Eac3Error::InvalidSpectralExtensionCode {
                begin_code: information.start_copy_frequency_code,
                end_code: information.end_subband,
            })?;
    if begin >= end || copy_start >= copy_end || base.len() != copy_end {
        return Err(Eac3Error::InvalidSpectralExtensionRange {
            begin: information.begin_subband,
            end: information.end_subband,
        });
    }
    if base.iter().any(|value| !value.is_finite()) {
        return Err(Eac3Error::NonFiniteSpectralExtensionCoefficient { index: base.len() });
    }

    let band_sizes = spectral_extension_band_sizes(information)?;
    let expected_inserted = insert_end
        .checked_sub(copy_end)
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    if band_sizes.iter().sum::<usize>() != expected_inserted {
        return Err(Eac3Error::InvalidSpectralExtensionCoordinateDimensions {
            expected: expected_inserted,
            actual: band_sizes.iter().sum(),
        });
    }
    if coordinates.bands.len() != band_sizes.len() {
        return Err(Eac3Error::InvalidSpectralExtensionCoordinateDimensions {
            expected: band_sizes.len(),
            actual: coordinates.bands.len(),
        });
    }
    if noise.len() != expected_inserted {
        return Err(Eac3Error::MissingSpectralExtensionNoise {
            expected: expected_inserted,
            actual: noise.len(),
        });
    }
    if noise.iter().any(|value| !value.is_finite()) {
        return Err(Eac3Error::NonFiniteSpectralExtensionCoefficient {
            index: expected_inserted,
        });
    }
    if let Some(code) = attenuation_code {
        if usize::from(code) >= SPX_ATTENUATION_TABLE.len() {
            return Err(Eac3Error::InvalidSpectralExtensionCode {
                begin_code: code,
                end_code: information.end_subband,
            });
        }
    }

    let mut output = vec![0.0; insert_end];
    output[..base.len()].copy_from_slice(base);
    let mut copy_index = copy_start;
    let mut insert_index = copy_end;
    let mut wrap_flags = vec![false; band_sizes.len()];
    for (band, &band_size) in band_sizes.iter().enumerate() {
        if copy_index
            .checked_add(band_size)
            .ok_or(Eac3Error::FrameSizeOverflow)?
            > copy_end
        {
            copy_index = copy_start;
            wrap_flags[band] = true;
        }
        for _ in 0..band_size {
            if copy_index == copy_end {
                copy_index = copy_start;
            }
            output[insert_index] = output[copy_index];
            insert_index = insert_index
                .checked_add(1)
                .ok_or(Eac3Error::FrameSizeOverflow)?;
            copy_index = copy_index
                .checked_add(1)
                .ok_or(Eac3Error::FrameSizeOverflow)?;
        }
    }

    let mut rms = Vec::with_capacity(band_sizes.len());
    let mut band_start = copy_end;
    for &band_size in &band_sizes {
        let energy = output[band_start..band_start + band_size]
            .iter()
            .map(|value| value * value)
            .sum::<f64>();
        rms.push((energy / band_size as f64).sqrt());
        band_start += band_size;
    }

    if let Some(code) = attenuation_code {
        apply_attenuation_notches(&mut output, copy_end, &band_sizes, &wrap_flags, code)?;
    }

    let noffset = f64::from(coordinates.blend) / 32.0;
    let denominator = SPX_BAND_TABLE[end] as f64;
    let mut spx_mant = copy_end as f64;
    let mut noise_index = 0_usize;
    for (band, &band_size) in band_sizes.iter().enumerate() {
        let mut ratio = (spx_mant + 0.5 * band_size as f64) / denominator - noffset;
        ratio = ratio.clamp(0.0, 1.0);
        let noise_scale = rms[band] * ratio.sqrt();
        let signal_scale = (1.0 - ratio).sqrt();
        for _ in 0..band_size {
            output[copy_end + noise_index] =
                output[copy_end + noise_index] * signal_scale + noise[noise_index] * noise_scale;
            noise_index += 1;
        }
        spx_mant += band_size as f64;
    }

    let coordinate_scale = coordinates
        .bands
        .iter()
        .zip(&band_sizes)
        .map(|(&(exponent, mantissa), _)| {
            spectral_extension_coordinate(exponent, mantissa, coordinates.master)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut offset = copy_end;
    for (scale, &band_size) in coordinate_scale.iter().zip(&band_sizes) {
        for value in &mut output[offset..offset + band_size] {
            *value *= scale * 32.0;
        }
        offset += band_size;
    }
    Ok(output)
}

fn spectral_extension_band_sizes(
    information: &SpectralExtensionInformation,
) -> Result<Vec<usize>, Eac3Error> {
    let begin = usize::from(information.begin_subband);
    let end = usize::from(information.end_subband);
    if begin >= end || end > 17 || information.band_structure.len() != 17 {
        return Err(Eac3Error::InvalidSpectralExtensionRange {
            begin: information.begin_subband,
            end: information.end_subband,
        });
    }
    let mut sizes = vec![12_usize];
    for subband in begin + 1..end {
        if information.band_structure[subband] {
            let last = sizes.last_mut().ok_or(Eac3Error::FrameSizeOverflow)?;
            *last = last.checked_add(12).ok_or(Eac3Error::FrameSizeOverflow)?;
        } else {
            sizes.push(12);
        }
    }
    if sizes.len() != usize::from(information.band_count) {
        return Err(Eac3Error::InvalidSpectralExtensionCoordinateDimensions {
            expected: sizes.len(),
            actual: usize::from(information.band_count),
        });
    }
    Ok(sizes)
}

fn spectral_extension_coordinate(exponent: u8, mantissa: u8, master: u8) -> Result<f64, Eac3Error> {
    if exponent > 15 || mantissa > 3 || master > 3 {
        return Err(Eac3Error::InvalidSpectralExtensionCoordinate {
            exponent,
            mantissa,
            master,
        });
    }
    let temporary = if exponent == 15 {
        f64::from(mantissa) / 4.0
    } else {
        f64::from(mantissa + 4) / 8.0
    };
    Ok(temporary / 2_f64.powi(i32::from(exponent) + 3 * i32::from(master)))
}

fn apply_attenuation_notches(
    output: &mut [f64],
    copy_end: usize,
    band_sizes: &[usize],
    wrap_flags: &[bool],
    code: u8,
) -> Result<(), Eac3Error> {
    let attenuation = *SPX_ATTENUATION_TABLE.get(usize::from(code)).ok_or(
        Eac3Error::InvalidSpectralExtensionCode {
            begin_code: code,
            end_code: 0,
        },
    )?;
    let mut filter_bin = copy_end
        .checked_sub(2)
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    apply_five_tap(output, &mut filter_bin, attenuation)?;
    filter_bin = filter_bin
        .checked_add(*band_sizes.first().ok_or(Eac3Error::FrameSizeOverflow)?)
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    for (band, &band_size) in band_sizes.iter().enumerate().skip(1) {
        if wrap_flags
            .get(band)
            .copied()
            .ok_or(Eac3Error::FrameSizeOverflow)?
        {
            filter_bin = filter_bin
                .checked_sub(5)
                .ok_or(Eac3Error::FrameSizeOverflow)?;
            apply_five_tap(output, &mut filter_bin, attenuation)?;
        }
        filter_bin = filter_bin
            .checked_add(band_size)
            .ok_or(Eac3Error::FrameSizeOverflow)?;
    }
    Ok(())
}

fn apply_five_tap(
    output: &mut [f64],
    filter_bin: &mut usize,
    attenuation: [f64; 3],
) -> Result<(), Eac3Error> {
    for factor in attenuation {
        let value = output
            .get_mut(*filter_bin)
            .ok_or(Eac3Error::FrameSizeOverflow)?;
        *value *= factor;
        *filter_bin += 1;
    }
    for factor in attenuation[..2].iter().rev() {
        let value = output
            .get_mut(*filter_bin)
            .ok_or(Eac3Error::FrameSizeOverflow)?;
        *value *= *factor;
        *filter_bin += 1;
    }
    Ok(())
}
