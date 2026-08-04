use openjoc_bitio::{BitRead, BitReader};
use openjoc_eac3::{
    decode_mantissa_code, decode_mantissas, mantissa_quantizer, ungroup_mantissa_code, Eac3Error,
    MantissaQuantizer,
};

fn packed(fields: &[(u16, u8)]) -> Vec<u8> {
    let mut bits = Vec::new();
    for &(value, width) in fields {
        for shift in (0..width).rev() {
            bits.push((value >> shift) & 1 != 0);
        }
    }
    let byte_count = bits.len().div_ceil(8);
    bits.resize(byte_count * 8, false);
    bits.chunks(8)
        .map(|chunk| {
            chunk
                .iter()
                .fold(0_u8, |value, bit| (value << 1) | u8::from(*bit))
        })
        .collect()
}

#[test]
fn exposes_the_normative_quantizer_mapping_for_every_bap() {
    let expected = [
        MantissaQuantizer {
            levels: 0,
            group_bits: 0,
            group_size: 1,
            symmetric: false,
        },
        MantissaQuantizer {
            levels: 3,
            group_bits: 5,
            group_size: 3,
            symmetric: true,
        },
        MantissaQuantizer {
            levels: 5,
            group_bits: 7,
            group_size: 3,
            symmetric: true,
        },
        MantissaQuantizer {
            levels: 7,
            group_bits: 3,
            group_size: 1,
            symmetric: true,
        },
        MantissaQuantizer {
            levels: 11,
            group_bits: 7,
            group_size: 2,
            symmetric: true,
        },
        MantissaQuantizer {
            levels: 15,
            group_bits: 4,
            group_size: 1,
            symmetric: true,
        },
        MantissaQuantizer {
            levels: 32,
            group_bits: 5,
            group_size: 1,
            symmetric: false,
        },
        MantissaQuantizer {
            levels: 64,
            group_bits: 6,
            group_size: 1,
            symmetric: false,
        },
        MantissaQuantizer {
            levels: 128,
            group_bits: 7,
            group_size: 1,
            symmetric: false,
        },
        MantissaQuantizer {
            levels: 256,
            group_bits: 8,
            group_size: 1,
            symmetric: false,
        },
        MantissaQuantizer {
            levels: 512,
            group_bits: 9,
            group_size: 1,
            symmetric: false,
        },
        MantissaQuantizer {
            levels: 1_024,
            group_bits: 10,
            group_size: 1,
            symmetric: false,
        },
        MantissaQuantizer {
            levels: 2_048,
            group_bits: 11,
            group_size: 1,
            symmetric: false,
        },
        MantissaQuantizer {
            levels: 4_096,
            group_bits: 12,
            group_size: 1,
            symmetric: false,
        },
        MantissaQuantizer {
            levels: 16_384,
            group_bits: 14,
            group_size: 1,
            symmetric: false,
        },
        MantissaQuantizer {
            levels: 65_536,
            group_bits: 16,
            group_size: 1,
            symmetric: false,
        },
    ];

    for (bap, expected) in expected.into_iter().enumerate() {
        assert_eq!(
            mantissa_quantizer(u8::try_from(bap).expect("bap")),
            Ok(expected)
        );
    }
    assert_eq!(
        mantissa_quantizer(16),
        Err(Eac3Error::InvalidMantissaBap { actual: 16 })
    );
}

#[test]
fn decodes_all_symmetric_table_endpoints() {
    for (bap, expected) in [
        (1, [-2.0 / 3.0, 0.0, 2.0 / 3.0].as_slice()),
        (2, [-4.0 / 5.0, 0.0, 4.0 / 5.0].as_slice()),
        (3, [-6.0 / 7.0, 0.0, 6.0 / 7.0].as_slice()),
        (4, [-10.0 / 11.0, 0.0, 10.0 / 11.0].as_slice()),
        (5, [-14.0 / 15.0, 0.0, 14.0 / 15.0].as_slice()),
    ] {
        let quantizer = mantissa_quantizer(bap).expect("symmetric bap");
        for (code, expected) in [
            (0, expected[0]),
            ((quantizer.levels / 2) as u16, expected[1]),
            ((quantizer.levels - 1) as u16, expected[2]),
        ] {
            let actual = decode_mantissa_code(bap, code).expect("valid table code");
            assert!(
                (actual - expected).abs() < 1.0e-12,
                "bap={bap}, code={code}"
            );
        }
    }
}

#[test]
fn decodes_asymmetric_twos_complement_fractional_words() {
    assert_eq!(decode_mantissa_code(6, 0b0_1111), Ok(15.0 / 16.0));
    assert_eq!(decode_mantissa_code(6, 0b1_0000), Ok(-1.0));
    assert_eq!(decode_mantissa_code(15, 0x7fff), Ok(32767.0 / 32768.0));
    assert_eq!(decode_mantissa_code(15, 0x8000), Ok(-1.0));
}

#[test]
fn ungroups_every_legal_group_code_in_frequency_order() {
    assert_eq!(ungroup_mantissa_code(1, 0), Ok(vec![0, 0, 0]));
    assert_eq!(ungroup_mantissa_code(1, 26), Ok(vec![2, 2, 2]));
    assert_eq!(ungroup_mantissa_code(2, 0), Ok(vec![0, 0, 0]));
    assert_eq!(ungroup_mantissa_code(2, 124), Ok(vec![4, 4, 4]));
    assert_eq!(ungroup_mantissa_code(4, 0), Ok(vec![0, 0]));
    assert_eq!(ungroup_mantissa_code(4, 120), Ok(vec![10, 10]));
    assert_eq!(
        ungroup_mantissa_code(1, 27),
        Err(Eac3Error::InvalidMantissaGroupCode { bap: 1, actual: 27 })
    );
}

#[test]
fn traverses_grouped_words_across_an_exponent_set_boundary() {
    // bap=1 groups are 5-bit triples. The first group contains bins 1..3,
    // and its final value is consumed from the next exponent set.
    let bytes = packed(&[(26, 5), (0, 5)]);
    let mut reader = BitReader::new(&bytes);
    let values = decode_mantissas(
        &mut reader,
        &[1, 1, 1, 1],
        &[0, 1, 2, 3],
        &[false, false, false, false],
        &[],
    )
    .expect("grouped mantissas");
    assert_eq!(values.len(), 4);
    assert!((values[0] - (2.0 / 3.0)).abs() < 1.0e-12);
    assert!((values[1] - (2.0 / 3.0) / 2.0).abs() < 1.0e-12);
    assert!((values[2] - (2.0 / 3.0) / 4.0).abs() < 1.0e-12);
    assert!((values[3] - (-2.0 / 3.0) / 8.0).abs() < 1.0e-12);
    assert_eq!(reader.bits_remaining(), 6);
}

#[test]
fn shares_group_state_when_other_bap_interleaves_the_group() {
    // TS 102 366 clause 6.3.5 keeps a partial grouped quantizer alive while
    // the frequency-ordered stream visits a different bap. One bap=1 group
    // therefore supplies bins 0, 2, and 3; bap=3 bin 1 has its own word.
    let bytes = packed(&[(26, 5), (3, 3)]);
    let mut reader = BitReader::new(&bytes);
    let values = decode_mantissas(
        &mut reader,
        &[1, 3, 1, 1],
        &[0, 0, 0, 0],
        &[false, false, false, false],
        &[],
    )
    .expect("interleaved grouped mantissas");
    assert_eq!(values, vec![2.0 / 3.0, 0.0, 2.0 / 3.0, 2.0 / 3.0]);
    assert_eq!(reader.bits_remaining(), 0);
}

#[test]
fn emits_zero_or_supplied_dither_for_bap_zero() {
    let bytes = [0xff];
    let mut reader = BitReader::new(&bytes);
    assert_eq!(
        decode_mantissas(&mut reader, &[0, 0], &[0, 1], &[false, false], &[]),
        Ok(vec![0.0, 0.0])
    );
    assert_eq!(reader.bits_remaining(), 8);

    let mut reader = BitReader::new(&bytes);
    assert_eq!(
        decode_mantissas(&mut reader, &[0, 0], &[0, 1], &[true, true], &[0.25, -0.5],),
        Ok(vec![0.25, -0.25])
    );
}

#[test]
fn rejects_mantissa_dimensions_codes_exponents_and_dither() {
    let mut reader = BitReader::new(&[0]);
    assert_eq!(
        decode_mantissas(&mut reader, &[3], &[], &[false], &[]),
        Err(Eac3Error::MantissaExponentLengthMismatch {
            baps: 1,
            exponents: 0,
        })
    );
    let mut reader = BitReader::new(&[0]);
    assert_eq!(
        decode_mantissas(&mut reader, &[1], &[0], &[false, false], &[]),
        Err(Eac3Error::MantissaDitherLengthMismatch {
            expected: 1,
            actual: 2,
        })
    );
    let mut reader = BitReader::new(&[0xff]);
    assert_eq!(
        decode_mantissas(&mut reader, &[0], &[0], &[true], &[]),
        Err(Eac3Error::MissingDitherValue { index: 0 })
    );
    let mut reader = BitReader::new(&[0xff]);
    assert_eq!(
        decode_mantissa_code(0, 1),
        Err(Eac3Error::InvalidMantissaCode { bap: 0, actual: 1 })
    );
    assert_eq!(
        decode_mantissa_code(1, 27),
        Err(Eac3Error::InvalidMantissaCode { bap: 1, actual: 27 })
    );
    assert_eq!(
        decode_mantissas(&mut reader, &[3], &[25], &[false], &[]),
        Err(Eac3Error::ExponentOutOfRange { actual: 25 })
    );
}
