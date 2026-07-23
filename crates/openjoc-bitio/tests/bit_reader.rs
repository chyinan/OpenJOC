use openjoc_bitio::{BitError, BitRead, BitReader};
use proptest::prelude::*;

#[test]
fn reads_bits_msb_first_across_byte_boundaries() {
    let mut reader = BitReader::new(&[0b1011_0010, 0b0110_1001]);

    assert_eq!(reader.read_bits(3), Ok(0b101));
    assert_eq!(reader.read_bits(7), Ok(0b100_1001));
    assert_eq!(reader.read_bits(6), Ok(0b10_1001));
    assert_eq!(reader.bits_remaining(), 0);
}

#[test]
fn rejects_widths_larger_than_u64_without_consuming_input() {
    let mut reader = BitReader::new(&[0xff]);

    assert_eq!(
        reader.read_bits(65),
        Err(BitError::InvalidWidth { width: 65 })
    );
    assert_eq!(reader.bits_remaining(), 8);
}

#[test]
fn reports_truncation_without_consuming_input() {
    let mut reader = BitReader::new(&[0b1000_0000]);
    assert_eq!(reader.read_bit(), Ok(true));

    assert_eq!(
        reader.read_bits(8),
        Err(BitError::EndOfInput {
            requested: 8,
            remaining: 7,
        })
    );
    assert_eq!(reader.bits_remaining(), 7);
}

#[test]
fn byte_align_discards_only_the_partial_byte() {
    let mut reader = BitReader::new(&[0xff, 0x5a]);
    assert_eq!(reader.read_bits(3), Ok(7));

    assert_eq!(reader.byte_align(), Ok(()));
    assert_eq!(reader.bits_remaining(), 8);
    assert_eq!(reader.read_bits(8), Ok(0x5a));
    assert_eq!(reader.byte_align(), Ok(()));
}

proptest! {
    #[test]
    fn sequential_bits_match_a_boolean_oracle(bytes in proptest::collection::vec(any::<u8>(), 0..128)) {
        let expected = bytes.iter().flat_map(|byte| (0..8).map(move |shift| byte & (0x80 >> shift) != 0));
        let mut reader = BitReader::new(&bytes);

        for bit in expected {
            prop_assert_eq!(reader.read_bit(), Ok(bit));
        }
        prop_assert_eq!(reader.bits_remaining(), 0);
    }

    #[test]
    fn chunking_preserves_consumption(
        bytes in proptest::collection::vec(any::<u8>(), 1..128),
        widths in proptest::collection::vec(0u8..=64, 0..64),
    ) {
        let mut reader = BitReader::new(&bytes);
        let initial = reader.bits_remaining();
        let mut consumed = 0usize;

        for width in widths {
            if usize::from(width) > reader.bits_remaining() {
                break;
            }
            let before = reader.bits_remaining();
            let _ = reader.read_bits(width)?;
            prop_assert_eq!(reader.bits_remaining(), before - usize::from(width));
            consumed += usize::from(width);
        }
        prop_assert_eq!(reader.bits_remaining(), initial - consumed);
    }
}
