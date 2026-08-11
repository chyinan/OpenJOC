use openjoc_bitio::{BitError, BitReader};
use openjoc_eac3::{
    Eac3Error, decode_aht_element_mantissas, decode_aht_gaq_mantissa, decode_aht_vq_vector,
    expand_aht_gaq_gains, high_efficiency_bit_allocation_pointer, inverse_aht_dct,
};
use sha2::{Digest, Sha256};

const HIGH_EFFICIENCY_POINTERS: [u8; 64] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 8, 8, 8, 9, 9, 9, 10, 10, 10, 10, 11, 11, 11, 11, 12, 12, 12, 12,
    13, 13, 13, 13, 14, 14, 14, 14, 15, 15, 15, 15, 16, 16, 16, 16, 17, 17, 17, 17, 18, 18, 18, 18,
    18, 18, 18, 18, 19, 19, 19, 19, 19, 19, 19, 19, 19,
];

const VQ_CARDINALITIES: [usize; 7] = [4, 8, 16, 32, 128, 256, 512];

// SHA-256 over each visually verified TS 102 366 V1.4.1 Table E.3.1-E.3.7,
// in index/transform order, with every printed 16-bit word serialized big
// endian. These constants were extracted independently from PDF pages
// 175-191; the test never reads production AHT table constants directly.
const VQ_TABLE_SHA256: [&str; 7] = [
    "22cf15eeed4287e04ac079e64ae309d98936f1365016a4f6e53e9c9f6612c6d3",
    "9a625d26112d8ab90567768908da92c89087cc2d94101b9ad4ecd3e74269fa0d",
    "7befe198a8f814e4a16923dd8cc6f7d0e042e32a036a789497638ee0fe3bcd06",
    "2dabd30d4327c2dff7594b8dd4096008d5401e58bcf74dfa21d56033760f464f",
    "1ef73f003ab8687d1030915ff35f1d43c96aa968788de824e9f3ef0de7a1229e",
    "0908a70d8a65266fd13b0f775d0596994df012b55390872931f75967351c6f0c",
    "9eb2333307b227b7ace8cd569e6062f336d162d476349046ebae1913eb92b5b5",
];

fn signed_fraction(code: u16, bits: u8) -> f64 {
    let sign = 1_i32 << (bits - 1);
    let raw = i32::from(code);
    let signed = if raw & sign == 0 {
        raw
    } else {
        raw - (1_i32 << bits)
    };
    f64::from(signed) / f64::from(1_i32 << (bits - 1))
}

fn signed_word(word: u16) -> f64 {
    f64::from(i16::from_be_bytes(word.to_be_bytes())) / 32768.0
}

fn oracle_code_bits(hebap: u8, gain: u8, large: bool) -> Option<u8> {
    let mantissa_bits = match hebap {
        8..=16 => hebap - 5,
        17 => 12,
        18 => 14,
        19 => 16,
        _ => return None,
    };
    match gain {
        1 => Some(mantissa_bits),
        2 if hebap <= 16 => Some(mantissa_bits - 1),
        4 if hebap <= 16 && large => Some(mantissa_bits),
        4 if hebap <= 16 => Some(mantissa_bits - 2),
        _ => None,
    }
}

fn oracle_gaq(hebap: u8, gain: u8, large: bool, code: u16) -> Option<f64> {
    const G1_A: [u16; 12] = [
        0x1249, 0x0889, 0x0421, 0x0208, 0x0102, 0x0081, 0x0040, 0x0020, 0x0010, 0x0008, 0x0002,
        0x0000,
    ];
    const G2_A: [u16; 9] = [
        0xd555, 0xc925, 0xc444, 0xc211, 0xc104, 0xc081, 0xc040, 0xc020, 0xc010,
    ];
    const G2_B_NEG: [u16; 9] = [
        0xeaab, 0xd249, 0xc889, 0xc421, 0xc208, 0xc102, 0xc081, 0xc040, 0xc020,
    ];
    const G4_A: [u16; 9] = [
        0xedb7, 0xe666, 0xe319, 0xe186, 0xe0c2, 0xe060, 0xe030, 0xe018, 0xe00c,
    ];
    const G4_B_NEG: [u16; 9] = [
        0xfb6e, 0xeccd, 0xe632, 0xe30c, 0xe183, 0xe0c1, 0xe060, 0xe030, 0xe018,
    ];

    let bits = oracle_code_bits(hebap, gain, large)?;
    if u32::from(code) >= 1_u32 << bits {
        return None;
    }
    let value = signed_fraction(code, bits);
    if gain != 1 && !large {
        return Some(value / f64::from(gain));
    }
    let index = usize::from(hebap - 8);
    let (a, positive_b, negative_b) = match gain {
        1 => (*G1_A.get(index)?, 0, 0),
        2 => (*G2_A.get(index)?, 0x4000, *G2_B_NEG.get(index)?),
        4 => (*G4_A.get(index)?, 0x2000, *G4_B_NEG.get(index)?),
        _ => return None,
    };
    let b = if value >= 0.0 { positive_b } else { negative_b };
    Some(value + signed_word(a) * value + signed_word(b))
}

fn oracle_inverse_dct(input: [f64; 6]) -> [f64; 6] {
    let mut output = [0.0; 6];
    for (m, value) in output.iter_mut().enumerate() {
        let mut sum = 0.0;
        for (j, coefficient) in input.iter().enumerate() {
            let r = if j == 0 { 1.0 / 2.0_f64.sqrt() } else { 1.0 };
            let angle = (j * (2 * m + 1)) as f64 * core::f64::consts::PI / 12.0;
            sum += r * coefficient * angle.cos();
        }
        *value = 2.0_f64.sqrt() * sum;
    }
    output
}

#[test]
fn exhaustively_matches_the_high_efficiency_pointer_table() {
    for (address, expected) in HIGH_EFFICIENCY_POINTERS.into_iter().enumerate() {
        assert_eq!(
            high_efficiency_bit_allocation_pointer(u8::try_from(address).expect("six-bit address")),
            Ok(expected)
        );
    }
    assert_eq!(
        high_efficiency_bit_allocation_pointer(64),
        Err(Eac3Error::InvalidBitAllocationTableIndex {
            table: "high-efficiency pointer",
            actual: 64,
        })
    );
}

#[test]
fn all_vq_entries_match_independent_pdf_table_digests() {
    for (table, (&cardinality, expected_digest)) in
        VQ_CARDINALITIES.iter().zip(VQ_TABLE_SHA256).enumerate()
    {
        let hebap = u8::try_from(table + 1).expect("hebap");
        let mut hasher = Sha256::new();
        for index in 0..cardinality {
            let vector = decode_aht_vq_vector(
                hebap,
                u16::try_from(index).expect("normative VQ table index"),
            )
            .expect("normative VQ entry");
            for value in vector {
                let scaled = value * 32768.0;
                assert_eq!(scaled.fract(), 0.0, "hebap {hebap}, index {index}");
                let raw = scaled as i16;
                hasher.update(raw.to_be_bytes());
            }
        }
        assert_eq!(format!("{:x}", hasher.finalize()), expected_digest);
        assert!(decode_aht_vq_vector(hebap, cardinality as u16).is_err());
    }
}

#[test]
fn exhaustively_matches_all_valid_gaq_quantizer_codewords() {
    let mut compared = 0_usize;
    let configurations = (8..=19)
        .map(|hebap| (hebap, 1, false))
        .chain((8..=16).flat_map(|hebap| {
            [
                (hebap, 2, false),
                (hebap, 2, true),
                (hebap, 4, false),
                (hebap, 4, true),
            ]
        }));
    for (hebap, gain, large) in configurations {
        let bits = oracle_code_bits(hebap, gain, large).expect("valid GAQ configuration");
        for raw in 0..(1_u32 << bits) {
            let code = u16::try_from(raw).expect("at most 16-bit codeword");
            let expected = oracle_gaq(hebap, gain, large, code).expect("oracle value");
            let actual =
                decode_aht_gaq_mantissa(hebap, gain, large, code).expect("production GAQ value");
            assert!(
                (actual - expected).abs() <= 1.0e-12,
                "hebap={hebap}, gain={gain}, large={large}, code={code}: {actual} != {expected}"
            );
            compared += 1;
        }
    }
    assert_eq!(compared, 99_302);
}

#[test]
fn exhaustively_matches_each_normative_gaq_gain_word() {
    assert_eq!(expand_aht_gaq_gains(0, &[], 4), Ok(vec![1; 4]));
    for word in 0..=1 {
        assert_eq!(
            expand_aht_gaq_gains(1, &[word], 1),
            Ok(vec![if word == 0 { 1 } else { 2 }])
        );
        assert_eq!(
            expand_aht_gaq_gains(2, &[word], 1),
            Ok(vec![if word == 0 { 1 } else { 4 }])
        );
    }
    for word in 0..=26 {
        let mapped = [word / 9, (word % 9) / 3, word % 3].map(|value| match value {
            0 => 1,
            1 => 2,
            2 => 4,
            _ => unreachable!("base-three digit"),
        });
        assert_eq!(expand_aht_gaq_gains(3, &[word], 3), Ok(mapped.to_vec()));
        for sections in 1..=2 {
            assert_eq!(
                expand_aht_gaq_gains(3, &[word], sections),
                Ok(mapped[..sections].to_vec())
            );
        }
    }
    assert_eq!(
        expand_aht_gaq_gains(3, &[27], 3),
        Err(Eac3Error::InvalidAhtGaqGainWord { actual: 27 })
    );
}

#[test]
fn independent_formula_matches_inverse_dct_for_basis_and_boundary_vectors() {
    let mut vectors = vec![
        [0.0; 6],
        [1.0, -0.75, 0.5, -0.25, 0.125, -0.0625],
        [-1.0, 0.999_969_482_421_875, -0.5, 0.25, -0.125, 0.0],
    ];
    for j in 0..6 {
        let mut basis = [0.0; 6];
        basis[j] = 1.0;
        vectors.push(basis);
    }
    let actual = inverse_aht_dct(&vectors).expect("finite AHT vectors");
    for (case, (input, output)) in vectors.into_iter().zip(actual).enumerate() {
        let expected = oracle_inverse_dct(input);
        for (block, (actual, expected)) in output.into_iter().zip(expected).enumerate() {
            assert!(
                (actual - expected).abs() <= 1.0e-12,
                "case {case}, block {block}: {actual} != {expected}"
            );
        }
    }
}

#[test]
fn truncated_aht_payloads_fail_without_partial_values() {
    let mut empty = BitReader::new(&[]);
    assert_eq!(
        decode_aht_element_mantissas(&mut empty, &[1], 0),
        Err(Eac3Error::Bit(BitError::EndOfInput {
            requested: 2,
            remaining: 0,
        }))
    );

    let mut empty = BitReader::new(&[]);
    assert_eq!(
        decode_aht_element_mantissas(&mut empty, &[8], 1),
        Err(Eac3Error::Bit(BitError::EndOfInput {
            requested: 1,
            remaining: 0,
        }))
    );
}
