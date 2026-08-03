// pattern: Functional Core

//! ETSI TS 102 366 clause 6.5 rematrix reconstruction.

use crate::{CouplingInformation, Eac3Error, SpectralExtensionInformation};

const REMATRIX_TABLE_A: [(usize, usize); 4] = [(13, 25), (25, 37), (37, 61), (61, 253)];
const ENHANCED_COUPLING_SUBBAND_MANTISSA: [usize; 23] = [
    13, 19, 25, 31, 37, 49, 61, 73, 85, 97, 109, 121, 133, 145, 157, 169, 181, 193, 205, 217, 229,
    241, 253,
];

/// Restores left/right transform coefficients for flagged rematrix bands.
///
/// The band boundaries are the coefficient-number ranges from TS 102 366
/// Tables 6.25 through 6.28.  A flagged range is reconstructed using the
/// clause 6.5.4 equations `left = received_left + received_right` and
/// `right = received_left - received_right`.  When the two channels have
/// different bandwidths, the operation is clipped to their common range as
/// required by clause 6.5.4.
pub fn rematrix_channels(
    channels: &[Vec<f64>],
    rematrix_flags: &[bool],
    coupling: Option<&CouplingInformation>,
    spectral_extension: Option<&SpectralExtensionInformation>,
) -> Result<Vec<Vec<f64>>, Eac3Error> {
    if channels.len() != 2 {
        return Err(Eac3Error::InvalidRematrixChannelCount {
            actual: channels.len(),
        });
    }
    for (channel, values) in channels.iter().enumerate() {
        for (index, value) in values.iter().enumerate() {
            if !value.is_finite() {
                return Err(Eac3Error::NonFiniteRematrixCoefficient { channel, index });
            }
        }
    }
    let bands = rematrix_bands(coupling, spectral_extension)?;
    if rematrix_flags.len() != bands.len() {
        return Err(Eac3Error::InvalidRematrixFlagCount {
            expected: bands.len(),
            actual: rematrix_flags.len(),
        });
    }

    let mut output = channels.to_vec();
    let common_end = output[0].len().min(output[1].len());
    for ((low, high), flagged) in bands.into_iter().zip(rematrix_flags.iter().copied()) {
        if !flagged {
            continue;
        }
        let start = low.min(common_end);
        let stop = high.min(common_end);
        for index in start..stop {
            let received_left = output[0][index];
            let received_right = output[1][index];
            output[0][index] = received_left + received_right;
            output[1][index] = received_left - received_right;
        }
    }
    Ok(output)
}

fn rematrix_bands(
    coupling: Option<&CouplingInformation>,
    spectral_extension: Option<&SpectralExtensionInformation>,
) -> Result<Vec<(usize, usize)>, Eac3Error> {
    match coupling {
        None => {
            let count = if spectral_extension.is_some_and(|value| value.begin_frequency_code < 2) {
                3
            } else {
                4
            };
            Ok(REMATRIX_TABLE_A[..count].to_vec())
        }
        Some(CouplingInformation::Standard(info)) => {
            let begin = 37_usize
                .checked_add(
                    usize::from(info.begin_frequency_code)
                        .checked_mul(12)
                        .ok_or(Eac3Error::FrameSizeOverflow)?,
                )
                .ok_or(Eac3Error::FrameSizeOverflow)?;
            if begin > 253 {
                return Err(Eac3Error::InvalidCouplingRange {
                    begin: i16::from(info.begin_frequency_code),
                    end: i16::from(info.end_frequency_code),
                });
            }
            match info.begin_frequency_code {
                0 => Ok(vec![(13, 25), (25, 37)]),
                1 | 2 => Ok(vec![(13, 25), (25, 37), (37, begin)]),
                _ => Ok(vec![(13, 25), (25, 37), (37, 61), (61, begin)]),
            }
        }
        Some(CouplingInformation::Enhanced(info)) => {
            let begin = *ENHANCED_COUPLING_SUBBAND_MANTISSA
                .get(usize::from(info.begin_subband))
                .ok_or(Eac3Error::InvalidCouplingRange {
                    begin: i16::from(info.begin_subband),
                    end: i16::from(info.end_subband),
                })?;
            match info.begin_frequency_code {
                0 => Ok(Vec::new()),
                1 => Ok(vec![(13, begin)]),
                2 => Ok(vec![(13, 25), (25, begin)]),
                3 | 4 => Ok(vec![(13, 25), (25, 37), (37, begin)]),
                _ => Ok(vec![(13, 25), (25, 37), (37, 61), (61, begin)]),
            }
        }
    }
}
