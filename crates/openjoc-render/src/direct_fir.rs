//! Adjacent-output SIMD for the current-input interior of the static FIR.
//!
//! Each lane is a separate output sample, initialized with the preceding
//! sources' accumulated output. Tap order and gain/multiply/add rounding stay
//! identical to the scalar loop. History, prefix, and remainder stay scalar.

use crate::{BinauralRegisteredSource, OutputChannel};

/// Advances over complete SIMD groups, or leaves `offset` unchanged when this
/// machine has no supported accelerator. Errors retain sample-then-ear order.
pub(crate) fn accumulate_interior(
    source: &BinauralRegisteredSource,
    samples: &[f64],
    left: &mut [f64],
    right: &mut [f64],
    offset: usize,
) -> Result<usize, (OutputChannel, usize)> {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    if samples.len() - offset >= 4 && std::is_x86_feature_detected!("avx") {
        // SAFETY: AVX and OS support were detected above. The helper checks all
        // slice/bounds invariants before its first raw-pointer operation.
        #[allow(unsafe_code)]
        return unsafe { accumulate_avx(source, samples, left, right, offset) };
    }
    // Other architectures retain the original scalar implementation.
    let _ = (source, samples, left, right);
    Ok(offset)
}

// This is the only unsafe DSP boundary. No global CPU target flags, FMA,
// horizontal reductions, alignment assumptions, or floating-point mode changes.
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[allow(unsafe_code)]
#[target_feature(enable = "avx")]
unsafe fn accumulate_avx(
    source: &BinauralRegisteredSource,
    samples: &[f64],
    left: &mut [f64],
    right: &mut [f64],
    mut offset: usize,
) -> Result<usize, (OutputChannel, usize)> {
    #[cfg(target_arch = "x86")]
    use std::arch::x86::{
        _mm256_add_pd, _mm256_loadu_pd, _mm256_mul_pd, _mm256_set1_pd, _mm256_storeu_pd,
    };
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::{
        _mm256_add_pd, _mm256_loadu_pd, _mm256_mul_pd, _mm256_set1_pd, _mm256_storeu_pd,
    };

    let tap_count = source.left_taps.len();
    assert!(tap_count > 0);
    assert_eq!(tap_count, source.right_taps.len());
    assert_eq!(samples.len(), left.len());
    assert_eq!(samples.len(), right.len());
    assert!(offset >= tap_count - 1 && offset <= samples.len());
    let gain = _mm256_set1_pd(source.definition.gain);
    while samples.len() - offset >= 4 {
        // SAFETY: offset + 3 < every output length. loadu/storeu accept any
        // alignment; the distinct mutable slices cannot alias each other/input.
        let (mut left_acc, mut right_acc) = unsafe {
            (
                _mm256_loadu_pd(left.as_ptr().add(offset)),
                _mm256_loadu_pd(right.as_ptr().add(offset)),
            )
        };
        for tap_index in 0..tap_count {
            // SAFETY: offset >= tap_count - 1 >= tap_index, and the last
            // loaded lane is offset - tap_index + 3 <= offset + 3 < len.
            let input = unsafe { _mm256_loadu_pd(samples.as_ptr().add(offset - tap_index)) };
            let input = _mm256_mul_pd(input, gain);
            left_acc = _mm256_add_pd(
                left_acc,
                _mm256_mul_pd(input, _mm256_set1_pd(source.left_taps[tap_index])),
            );
            right_acc = _mm256_add_pd(
                right_acc,
                _mm256_mul_pd(input, _mm256_set1_pd(source.right_taps[tap_index])),
            );
        }
        // SAFETY: the same four in-bounds output elements loaded above.
        unsafe {
            _mm256_storeu_pd(left.as_mut_ptr().add(offset), left_acc);
            _mm256_storeu_pd(right.as_mut_ptr().add(offset), right_acc);
        }
        // Computing later lanes ahead is unobservable: public numeric failure
        // clears both output slices, leaves histories intact, and requires reset.
        for lane in 0..4 {
            if !left[offset + lane].is_finite() {
                return Err((OutputChannel::Left, offset + lane));
            }
            if !right[offset + lane].is_finite() {
                return Err((OutputChannel::Right, offset + lane));
            }
        }
        offset += 4;
    }
    Ok(offset)
}
