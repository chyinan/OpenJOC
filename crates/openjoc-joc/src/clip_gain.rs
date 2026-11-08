//! Frame clip gain applied to reconstructed JOC objects.
//!
//! The printed ETSI equation is recorded in `docs/PROVENANCE.md`. Playback uses
//! the grouping implemented by Dolby decoders.

/// Linear gain for one JOC frame.
///
/// `x_bits` is the 3-bit `joc_clipgain_x_bits` field and `y_bits` is the 5-bit
/// `joc_clipgain_y_bits` field. The result is `(1 + y/32) * 2^(x-4)`.
/// `x = 4`, `y = 0` is exactly 1.
#[must_use]
pub fn joc_clip_gain(x_bits: u8, y_bits: u8) -> f64 {
    debug_assert!(x_bits < 8, "joc_clipgain_x_bits is a 3-bit field");
    debug_assert!(y_bits < 32, "joc_clipgain_y_bits is a 5-bit field");
    let exponent = i32::from(x_bits) - 4;
    let mantissa = 1.0 + f64::from(y_bits) / 32.0;
    mantissa * 2.0_f64.powi(exponent)
}

#[cfg(test)]
mod tests {
    use super::joc_clip_gain;

    #[test]
    fn unity_and_endpoints_match_the_dolby_grouping() {
        assert_eq!(joc_clip_gain(4, 0), 1.0);
        assert_eq!(joc_clip_gain(4, 31), 63.0 / 32.0);
        assert_eq!(joc_clip_gain(0, 0), 0.0625);
        assert_eq!(joc_clip_gain(0, 31), (63.0 / 32.0) / 16.0);
        assert_eq!(joc_clip_gain(7, 0), 8.0);
        assert_eq!(joc_clip_gain(7, 31), (63.0 / 32.0) * 8.0);
    }

    #[test]
    fn printed_etsi_grouping_differs_exactly_when_x_is_not_four() {
        for x in 0..8_u8 {
            for y in 0..32_u8 {
                let dolby = joc_clip_gain(x, y);
                let printed = 1.0 + (f64::from(y) / 32.0) * 2.0_f64.powi(i32::from(x) - 4);
                let difference = dolby - printed;
                let expected = 2.0_f64.powi(i32::from(x) - 4) - 1.0;
                assert!((difference - expected).abs() < 1.0e-12);
                if x == 4 {
                    assert_eq!(dolby, printed);
                } else {
                    assert_ne!(dolby, printed);
                }
            }
        }
    }
}
