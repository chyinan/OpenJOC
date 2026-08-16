// pattern: Functional Core

//! ETSI TS 102 366 clause 6.9 inverse TDAC transforms.

use core::f64::consts::PI;
use std::sync::LazyLock;

use crate::Eac3Error;

const TRANSFORM_COEFFICIENTS: usize = 256;
const TRANSFORM_SAMPLES: usize = 512;
const HALF_SAMPLES: usize = 256;
const QUARTER_SAMPLES: usize = 128;
const EIGHTH_SAMPLES: usize = 64;

// ETSI TS 102 366 V1.4.1 Table 6.33, rendered and visually inspected on page
// 86 at 300 DPI. The table address is (10 * A) + B.
const TRANSFORM_WINDOW: [f64; HALF_SAMPLES] = [
    0.00014, 0.00024, 0.00037, 0.00051, 0.00067, 0.00086, 0.00107, 0.00130, 0.00157, 0.00187,
    0.00220, 0.00256, 0.00297, 0.00341, 0.00390, 0.00443, 0.00501, 0.00564, 0.00632, 0.00706,
    0.00785, 0.00871, 0.00962, 0.01061, 0.01166, 0.01279, 0.01399, 0.01526, 0.01662, 0.01806,
    0.01959, 0.02121, 0.02292, 0.02472, 0.02662, 0.02863, 0.03073, 0.03294, 0.03527, 0.03770,
    0.04025, 0.04292, 0.04571, 0.04862, 0.05165, 0.05481, 0.05810, 0.06153, 0.06508, 0.06878,
    0.07261, 0.07658, 0.08069, 0.08495, 0.08935, 0.09389, 0.09859, 0.10343, 0.10842, 0.11356,
    0.11885, 0.12429, 0.12988, 0.13563, 0.14152, 0.14757, 0.15376, 0.16011, 0.16661, 0.17325,
    0.18005, 0.18699, 0.19407, 0.20130, 0.20867, 0.21618, 0.22382, 0.23161, 0.23952, 0.24757,
    0.25574, 0.26404, 0.27246, 0.28100, 0.28965, 0.29841, 0.30729, 0.31626, 0.32533, 0.33450,
    0.34376, 0.35311, 0.36253, 0.37204, 0.38161, 0.39126, 0.40096, 0.41072, 0.42054, 0.43040,
    0.44030, 0.45023, 0.46020, 0.47019, 0.48020, 0.49022, 0.50025, 0.51028, 0.52031, 0.53033,
    0.54033, 0.55031, 0.56026, 0.57019, 0.58007, 0.58991, 0.59970, 0.60944, 0.61912, 0.62873,
    0.63827, 0.64774, 0.65713, 0.66643, 0.67564, 0.68476, 0.69377, 0.70269, 0.71150, 0.72019,
    0.72877, 0.73723, 0.74557, 0.75378, 0.76186, 0.76981, 0.77762, 0.78530, 0.79283, 0.80022,
    0.80747, 0.81457, 0.82151, 0.82831, 0.83496, 0.84145, 0.84779, 0.85398, 0.86001, 0.86588,
    0.87160, 0.87716, 0.88257, 0.88782, 0.89291, 0.89785, 0.90264, 0.90728, 0.91176, 0.91610,
    0.92028, 0.92432, 0.92822, 0.93197, 0.93558, 0.93906, 0.94240, 0.94560, 0.94867, 0.95162,
    0.95444, 0.95713, 0.95971, 0.96217, 0.96451, 0.96674, 0.96887, 0.97089, 0.97281, 0.97463,
    0.97635, 0.97799, 0.97953, 0.98099, 0.98236, 0.98366, 0.98488, 0.98602, 0.98710, 0.98811,
    0.98905, 0.98994, 0.99076, 0.99153, 0.99225, 0.99291, 0.99353, 0.99411, 0.99464, 0.99513,
    0.99558, 0.99600, 0.99639, 0.99674, 0.99706, 0.99736, 0.99763, 0.99788, 0.99811, 0.99831,
    0.99850, 0.99867, 0.99882, 0.99895, 0.99908, 0.99919, 0.99929, 0.99938, 0.99946, 0.99953,
    0.99959, 0.99965, 0.99969, 0.99974, 0.99978, 0.99981, 0.99984, 0.99986, 0.99988, 0.99990,
    0.99992, 0.99993, 0.99994, 0.99995, 0.99996, 0.99997, 0.99998, 0.99998, 0.99998, 0.99999,
    0.99999, 0.99999, 0.99999, 1.00000, 1.00000, 1.00000, 1.00000, 1.00000, 1.00000, 1.00000,
    1.00000, 1.00000, 1.00000, 1.00000, 1.00000, 1.00000,
];

#[derive(Clone, Copy, Default)]
struct Complex {
    real: f64,
    imag: f64,
}

static LONG_ROTATIONS: LazyLock<[Complex; QUARTER_SAMPLES]> = LazyLock::new(|| {
    std::array::from_fn(|index| {
        let angle = 2.0 * PI * (8.0 * index as f64 + 1.0) / (8.0 * TRANSFORM_SAMPLES as f64);
        Complex {
            real: -angle.cos(),
            imag: -angle.sin(),
        }
    })
});

static SHORT_ROTATIONS: LazyLock<[Complex; EIGHTH_SAMPLES]> = LazyLock::new(|| {
    std::array::from_fn(|index| {
        let angle = 2.0 * PI * (8.0 * index as f64 + 1.0) / (4.0 * TRANSFORM_SAMPLES as f64);
        Complex {
            real: -angle.cos(),
            imag: -angle.sin(),
        }
    })
});

static LONG_INVERSE_ROTATIONS: LazyLock<Vec<Complex>> =
    LazyLock::new(|| inverse_rotation_table(QUARTER_SAMPLES, 8.0));
static SHORT_INVERSE_ROTATIONS: LazyLock<Vec<Complex>> =
    LazyLock::new(|| inverse_rotation_table(EIGHTH_SAMPLES, 16.0));

/// The bit-exact intermediate values of one inverse transform.
///
/// This is an opt-in diagnostic representation.  Production decoding uses
/// the same values but does not retain or emit these arrays.
#[derive(Clone, Debug, PartialEq)]
pub struct InverseTransformTrace {
    pub block_switch: bool,
    pub pre_window: Vec<f64>,
    pub window_coefficients: Vec<f64>,
    pub windowed: Vec<f64>,
}

/// The contribution identity for one TDAC block.
///
/// `output_sum = carry_in + current_head` and `output = 2 * output_sum`.
/// The explicit unscaled sum avoids hiding the normative overlap/add factor.
#[derive(Clone, Debug, PartialEq)]
pub struct OverlapAddTrace {
    pub carry_in: Vec<f64>,
    pub current_head: Vec<f64>,
    pub output_sum: Vec<f64>,
    pub output: Vec<f64>,
    pub carry_out: Vec<f64>,
}

/// Applies the clause 6.9 inverse transform for one E-AC-3 audio block.
///
/// `coefficients` contains the 256 interleaved transform coefficients. With
/// `block_switch == false`, clause 6.9.4.1 performs one 512-sample transform.
/// With `block_switch == true`, clause 6.9.4.2 de-interleaves two 256-sample
/// transforms and returns their 512-sample windowed block.
pub fn inverse_transform(coefficients: &[f64], block_switch: bool) -> Result<Vec<f64>, Eac3Error> {
    validate_transform_coefficients(coefficients)?;
    let mut windowed = if block_switch {
        inverse_short(coefficients)?
    } else {
        inverse_long(coefficients)?
    };
    for (index, value) in windowed.iter_mut().enumerate() {
        *value *= window_coefficient(index);
    }
    Ok(windowed)
}

/// Applies the inverse transform and exposes its pre-window and windowed
/// stages for deterministic diagnostics.
pub fn inverse_transform_with_trace(
    coefficients: &[f64],
    block_switch: bool,
) -> Result<InverseTransformTrace, Eac3Error> {
    validate_transform_coefficients(coefficients)?;
    let pre_window = if block_switch {
        inverse_short(coefficients)?
    } else {
        inverse_long(coefficients)?
    };
    let window_coefficients = (0..TRANSFORM_SAMPLES)
        .map(window_coefficient)
        .collect::<Vec<_>>();
    let windowed = pre_window
        .iter()
        .zip(&window_coefficients)
        .map(|(value, coefficient)| value * coefficient)
        .collect();
    Ok(InverseTransformTrace {
        block_switch,
        pre_window,
        window_coefficients,
        windowed,
    })
}

/// Performs the clause 6.9.4.1 overlap/add operation and advances its delay.
pub fn overlap_add(windowed: &[f64], delay: &mut [f64]) -> Result<Vec<f64>, Eac3Error> {
    if windowed.len() != TRANSFORM_SAMPLES || delay.len() != HALF_SAMPLES {
        return Err(Eac3Error::InvalidTransformWindowLength {
            actual: windowed.len(),
        });
    }
    let mut pcm = vec![0.0; HALF_SAMPLES];
    for index in 0..HALF_SAMPLES {
        pcm[index] = 2.0 * (windowed[index] + delay[index]);
        delay[index] = windowed[index + HALF_SAMPLES];
    }
    Ok(pcm)
}

/// Applies overlap/add and exposes the four contribution components.
pub fn overlap_add_with_trace(
    windowed: &[f64],
    delay: &mut [f64],
) -> Result<OverlapAddTrace, Eac3Error> {
    if windowed.len() != TRANSFORM_SAMPLES || delay.len() != HALF_SAMPLES {
        return Err(Eac3Error::InvalidTransformWindowLength {
            actual: windowed.len(),
        });
    }
    let carry_in = delay.to_vec();
    let current_head = windowed[..HALF_SAMPLES].to_vec();
    let output_sum = current_head
        .iter()
        .zip(&carry_in)
        .map(|(head, carry)| head + carry)
        .collect::<Vec<_>>();
    let output = output_sum.iter().map(|sample| 2.0 * sample).collect();
    let carry_out = windowed[HALF_SAMPLES..].to_vec();
    delay.copy_from_slice(&carry_out);
    Ok(OverlapAddTrace {
        carry_in,
        current_head,
        output_sum,
        output,
        carry_out,
    })
}

fn inverse_long(coefficients: &[f64]) -> Result<Vec<f64>, Eac3Error> {
    let mut z = [Complex::default(); QUARTER_SAMPLES];
    for k in 0..QUARTER_SAMPLES {
        let rotation = LONG_ROTATIONS[k];
        let odd = coefficients[TRANSFORM_COEFFICIENTS - 2 * k - 1];
        let even = coefficients[2 * k];
        z[k] = Complex {
            real: odd * rotation.real - even * rotation.imag,
            imag: even * rotation.real + odd * rotation.imag,
        };
    }
    let z = inverse_complex(&z, &LONG_INVERSE_ROTATIONS);
    let mut y = [Complex::default(); QUARTER_SAMPLES];
    for n in 0..QUARTER_SAMPLES {
        let rotation = LONG_ROTATIONS[n];
        y[n] = Complex {
            real: z[n].real * rotation.real - z[n].imag * rotation.imag,
            imag: z[n].imag * rotation.real + z[n].real * rotation.imag,
        };
    }
    let mut output = vec![0.0; TRANSFORM_SAMPLES];
    for n in 0..EIGHTH_SAMPLES {
        let n8 = EIGHTH_SAMPLES;
        output[2 * n] = -y[n8 + n].imag;
        output[2 * n + 1] = y[n8 - n - 1].real;
        output[QUARTER_SAMPLES + 2 * n] = -y[n].real;
        output[QUARTER_SAMPLES + 2 * n + 1] = y[QUARTER_SAMPLES - n - 1].imag;
        output[HALF_SAMPLES + 2 * n] = -y[n8 + n].real;
        output[HALF_SAMPLES + 2 * n + 1] = y[n8 - n - 1].imag;
        output[3 * QUARTER_SAMPLES + 2 * n] = y[n].imag;
        output[3 * QUARTER_SAMPLES + 2 * n + 1] = -y[QUARTER_SAMPLES - n - 1].real;
    }
    Ok(output)
}

fn inverse_short(coefficients: &[f64]) -> Result<Vec<f64>, Eac3Error> {
    let mut first = [0.0; QUARTER_SAMPLES];
    let mut second = [0.0; QUARTER_SAMPLES];
    for index in 0..QUARTER_SAMPLES {
        first[index] = coefficients[2 * index];
        second[index] = coefficients[2 * index + 1];
    }
    let z1 = pre_short(&first);
    let z2 = pre_short(&second);
    let raw_y1 = inverse_complex(&z1, &SHORT_INVERSE_ROTATIONS);
    let raw_y2 = inverse_complex(&z2, &SHORT_INVERSE_ROTATIONS);
    let y1: [Complex; EIGHTH_SAMPLES] =
        std::array::from_fn(|index| post_short(raw_y1[index], index));
    let y2: [Complex; EIGHTH_SAMPLES] =
        std::array::from_fn(|index| post_short(raw_y2[index], index));
    let mut output = vec![0.0; TRANSFORM_SAMPLES];
    for n in 0..EIGHTH_SAMPLES {
        let value1 = y1[n];
        let value2 = y2[n];
        output[2 * n] = -value1.imag;
        output[2 * n + 1] = y1[EIGHTH_SAMPLES - n - 1].real;
        output[QUARTER_SAMPLES + 2 * n] = -value1.real;
        output[QUARTER_SAMPLES + 2 * n + 1] = y1[EIGHTH_SAMPLES - n - 1].imag;
        output[HALF_SAMPLES + 2 * n] = -value2.real;
        output[HALF_SAMPLES + 2 * n + 1] = y2[EIGHTH_SAMPLES - n - 1].imag;
        output[3 * QUARTER_SAMPLES + 2 * n] = value2.imag;
        output[3 * QUARTER_SAMPLES + 2 * n + 1] = -y2[EIGHTH_SAMPLES - n - 1].real;
    }
    Ok(output)
}

fn pre_short(coefficients: &[f64; QUARTER_SAMPLES]) -> [Complex; EIGHTH_SAMPLES] {
    let mut z = [Complex::default(); EIGHTH_SAMPLES];
    for k in 0..EIGHTH_SAMPLES {
        let rotation = SHORT_ROTATIONS[k];
        let odd = coefficients[QUARTER_SAMPLES - 2 * k - 1];
        let even = coefficients[2 * k];
        z[k] = Complex {
            real: odd * rotation.real - even * rotation.imag,
            imag: even * rotation.real + odd * rotation.imag,
        };
    }
    z
}

fn post_short(value: Complex, index: usize) -> Complex {
    let rotation = SHORT_ROTATIONS[index];
    Complex {
        real: value.real * rotation.real - value.imag * rotation.imag,
        imag: value.imag * rotation.real + value.real * rotation.imag,
    }
}

fn inverse_rotation_table(length: usize, stride: f64) -> Vec<Complex> {
    let mut rotations = Vec::with_capacity(length * length);
    for n in 0..length {
        for k in 0..length {
            let angle = stride * PI * k as f64 * n as f64 / TRANSFORM_SAMPLES as f64;
            rotations.push(Complex {
                real: angle.cos(),
                imag: angle.sin(),
            });
        }
    }
    rotations
}

fn inverse_complex<const N: usize>(input: &[Complex; N], rotations: &[Complex]) -> [Complex; N] {
    debug_assert_eq!(rotations.len(), N * N);
    let mut output = [Complex::default(); N];
    for (value, row) in output.iter_mut().zip(rotations.chunks_exact(N)) {
        for (input_value, rotation) in input.iter().zip(row) {
            value.real += input_value.real * rotation.real - input_value.imag * rotation.imag;
            value.imag += input_value.real * rotation.imag + input_value.imag * rotation.real;
        }
    }
    output
}

fn window(index: usize) -> f64 {
    TRANSFORM_WINDOW[index]
}

fn window_coefficient(index: usize) -> f64 {
    window(index.min(TRANSFORM_SAMPLES - 1 - index))
}

fn validate_transform_coefficients(coefficients: &[f64]) -> Result<(), Eac3Error> {
    if coefficients.len() != TRANSFORM_COEFFICIENTS {
        return Err(Eac3Error::InvalidTransformCoefficientLength {
            expected: TRANSFORM_COEFFICIENTS,
            actual: coefficients.len(),
        });
    }
    for (index, coefficient) in coefficients.iter().enumerate() {
        if !coefficient.is_finite() {
            return Err(Eac3Error::NonFiniteTransformCoefficient { index });
        }
    }
    Ok(())
}
