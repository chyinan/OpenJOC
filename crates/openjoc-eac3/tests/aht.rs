use openjoc_bitio::{BitRead, BitReader};
use openjoc_eac3::{
    Eac3Error, decode_aht_element_mantissas, decode_aht_gaq_mantissa, decode_aht_vq_vector,
    expand_aht_gaq_gains, inverse_aht_dct,
};

fn packed(fields: &[(u16, u8)]) -> Vec<u8> {
    let mut bits = Vec::new();
    for &(value, width) in fields {
        for shift in (0..width).rev() {
            bits.push((value >> shift) & 1 != 0);
        }
    }
    bits.resize(bits.len().div_ceil(8) * 8, false);
    bits.chunks(8)
        .map(|chunk| {
            chunk
                .iter()
                .fold(0_u8, |acc, bit| (acc << 1) | u8::from(*bit))
        })
        .collect()
}

#[test]
fn decodes_aht_vq_and_scalar_bins_in_transform_order() {
    let bytes = packed(&[(2, 2), (0, 3)]);
    let mut bits = BitReader::new(&bytes);
    let values = decode_aht_element_mantissas(&mut bits, &[1, 8], 0).expect("AHT bins");
    assert_eq!(values.len(), 2);
    assert_eq!(values[0], decode_aht_vq_vector(1, 2).expect("VQ vector"));
    assert_eq!(values[1][0], 0.0);
    assert_eq!(values[1][1..], [0.0; 5]);
}

#[test]
fn decodes_gaq_tags_for_all_six_transform_coefficients() {
    let bytes = packed(&[
        (1, 1),
        (0b10, 2),
        (0, 2),
        (1, 2),
        (1, 2),
        (1, 2),
        (1, 2),
        (1, 2),
    ]);
    let mut bits = BitReader::new(&bytes);
    let values = decode_aht_element_mantissas(&mut bits, &[8], 1).expect("GAQ bin");
    assert_eq!(values.len(), 1);
    assert_eq!(values[0].len(), 6);
    assert!(values[0][0].is_finite());
    assert_eq!(bits.bits_remaining(), 1);
}

#[test]
fn decodes_visual_etsi_vq_table_vectors_as_signed_fractions() {
    let vector = decode_aht_vq_vector(1, 0).expect("Table E.3.1 vector");
    let expected = [
        0x1bff_i16 as f64 / 32768.0,
        0x1283_i16 as f64 / 32768.0,
        0x0452_i16 as f64 / 32768.0,
        0x10ad_i16 as f64 / 32768.0,
        0x28ac_i16 as f64 / 32768.0,
        0x12d4_i16 as f64 / 32768.0,
    ];
    for (actual, expected) in vector.into_iter().zip(expected) {
        assert!((actual - expected).abs() < 1.0e-12);
    }
    let last = decode_aht_vq_vector(7, 511).expect("Table E.3.7 last vector")[0];
    let expected = f64::from(i16::from_be_bytes(0x0c9fu16.to_be_bytes())) / 32768.0;
    assert!((last - expected).abs() < 1.0e-12);
}

#[test]
fn rejects_vq_indices_outside_the_etsi_table() {
    assert_eq!(
        decode_aht_vq_vector(0, 0),
        Err(Eac3Error::InvalidAhtVqHebap { actual: 0 })
    );
    assert_eq!(
        decode_aht_vq_vector(1, 4),
        Err(Eac3Error::InvalidAhtVqIndex {
            hebap: 1,
            actual: 4
        })
    );
}

#[test]
fn inverse_aht_dct_reconstructs_dc_and_preserves_bin_energy() {
    let dc = inverse_aht_dct(&[[1.0, 0.0, 0.0, 0.0, 0.0, 0.0]]).expect("DC AHT spectrum");
    assert!(dc[0].iter().all(|sample| (*sample - 1.0).abs() < 1.0e-12));

    let first_ac =
        inverse_aht_dct(&[[0.0, 1.0, 0.0, 0.0, 0.0, 0.0]]).expect("first AHT AC spectrum");
    let energy = first_ac[0]
        .iter()
        .map(|sample| sample * sample)
        .sum::<f64>();
    assert!((energy - 6.0).abs() < 1.0e-12);
    let expected = 2.0_f64.sqrt() * (core::f64::consts::PI / 12.0).cos();
    assert!((first_ac[0][0] - expected).abs() < 1.0e-12);
}

#[test]
fn inverse_aht_dct_rejects_nonfinite_input() {
    assert!(inverse_aht_dct(&[[f64::NAN, 0.0, 0.0, 0.0, 0.0, 0.0]]).is_err());
}

#[test]
fn expands_all_normative_aht_gaq_gain_modes() {
    assert_eq!(expand_aht_gaq_gains(0, &[], 3), Ok(vec![1, 1, 1]));
    assert_eq!(expand_aht_gaq_gains(1, &[0, 1, 0], 3), Ok(vec![1, 2, 1]));
    assert_eq!(expand_aht_gaq_gains(2, &[1, 0], 2), Ok(vec![4, 1]));
    assert_eq!(expand_aht_gaq_gains(3, &[26], 2), Ok(vec![4, 4]));
}

#[test]
fn rejects_invalid_aht_gaq_gain_syntax() {
    assert_eq!(
        expand_aht_gaq_gains(4, &[], 0),
        Err(Eac3Error::InvalidAhtGaqMode { actual: 4 })
    );
    assert_eq!(
        expand_aht_gaq_gains(1, &[2], 1),
        Err(Eac3Error::InvalidAhtGaqGainWord { actual: 2 })
    );
}

#[test]
fn decodes_aht_gaq_small_and_large_mantissas() {
    assert_eq!(decode_aht_gaq_mantissa(8, 1, false, 0), Ok(0.0));

    let small = decode_aht_gaq_mantissa(8, 2, false, 1).expect("GAQ small code");
    assert!((small - 0.25).abs() < 1.0e-12);

    let large = decode_aht_gaq_mantissa(8, 2, true, 1).expect("GAQ large code");
    let expected = 0.5 + 0.5 * (1.0 - 0x2aabu16 as f64 / 32768.0);
    assert!((large - expected).abs() < 1.0e-12);
}

#[test]
fn rejects_invalid_aht_gaq_quantizer_inputs() {
    assert_eq!(
        decode_aht_gaq_mantissa(7, 1, false, 0),
        Err(Eac3Error::InvalidAhtGaqHebap { actual: 7 })
    );
    assert_eq!(
        decode_aht_gaq_mantissa(8, 3, false, 0),
        Err(Eac3Error::InvalidAhtGaqGain { actual: 3 })
    );
    assert_eq!(
        decode_aht_gaq_mantissa(8, 1, false, 8),
        Err(Eac3Error::InvalidAhtGaqCode { actual: 8 })
    );
}
