use openjoc_bitio::{BitError, BitRead, BitReader};
use openjoc_oamd::{OamdError, variable_bits_max};

#[test]
fn decodes_one_and_multiple_groups_per_clause_5_5_1() {
    let mut one = BitReader::new(&[0b0011_0000]);
    assert_eq!(variable_bits_max(&mut one, 4, 4), Ok(3));
    assert_eq!(one.bits_remaining(), 3);

    // groups: 0010, continue; 0101, stop
    let mut two = BitReader::new(&[0b0010_1010, 0b1000_0000]);
    assert_eq!(variable_bits_max(&mut two, 4, 4), Ok(53));
    assert_eq!(two.bits_remaining(), 6);
}

#[test]
fn stops_after_the_normative_maximum_group_count() {
    let mut reader = BitReader::new(&[0b0001_1001, 0b0100_0000]);
    assert_eq!(variable_bits_max(&mut reader, 4, 2), Ok(34));
    assert_eq!(reader.bits_remaining(), 6);
}

#[test]
fn rejects_invalid_configuration_overflow_and_truncation() {
    let mut reader = BitReader::new(&[0xff]);
    assert_eq!(
        variable_bits_max(&mut reader, 0, 4),
        Err(OamdError::InvalidVariableBits {
            width: 0,
            max_groups: 4
        })
    );

    let mut truncated = BitReader::new(&[0b0001_1000]);
    assert_eq!(
        variable_bits_max(&mut truncated, 4, 4),
        Err(OamdError::Bit(BitError::EndOfInput {
            requested: 4,
            remaining: 3
        }))
    );

    let mut overflow = BitReader::new(&[0xff; 16]);
    assert_eq!(
        variable_bits_max(&mut overflow, 63, 2),
        Err(OamdError::ValueOverflow)
    );
}

fn variable_groups(mut value: u128, width: u8) -> Vec<(u64, bool)> {
    let radix = 1_u128 << width;
    let mut groups = Vec::new();
    loop {
        let digit = u64::try_from(value % radix).expect("bounded test digit");
        groups.push((digit, !groups.is_empty()));
        value /= radix;
        if value == 0 {
            break;
        }
        value -= 1;
    }
    groups.reverse();
    groups
}

fn group_bytes(groups: &[(u64, bool)], width: u8) -> Vec<u8> {
    let mut bits = Vec::new();
    for &(digit, more) in groups {
        for shift in (0..width).rev() {
            bits.push((digit >> shift) & 1 != 0);
        }
        bits.push(more);
    }
    bits.extend([true, false, true, true]); // unread following field
    let mut bytes = vec![0_u8; bits.len().div_ceil(8)];
    for (index, bit) in bits.into_iter().enumerate() {
        if bit {
            bytes[index / 8] |= 1 << (7 - index % 8);
        }
    }
    bytes
}

#[test]
fn preserves_full_u64_range_at_every_supported_width() {
    for width in 1..=63 {
        let radix = 1_u64 << width;
        for value in [0, 1, radix - 1, radix, u64::MAX - 1, u64::MAX] {
            let groups = variable_groups(u128::from(value), width);
            let bytes = group_bytes(&groups, width);
            let mut reader = BitReader::new(&bytes);
            assert_eq!(variable_bits_max(&mut reader, width, 255), Ok(value));
            assert_eq!(
                reader.bits_remaining(),
                bytes.len() * 8 - groups.len() * (usize::from(width) + 1)
            );
            assert_eq!(reader.read_bits(4), Ok(0b1011));
        }
    }
}

#[test]
fn rejects_lost_high_bits_at_every_supported_width() {
    for width in 1..=63 {
        for value in [1_u128 << 64, (1_u128 << 64) + (1_u128 << width)] {
            let groups = variable_groups(value, width);
            let bytes = group_bytes(&groups, width);
            assert_eq!(
                variable_bits_max(&mut BitReader::new(&bytes), width, 255),
                Err(OamdError::ValueOverflow)
            );
        }
    }
}

#[test]
fn group_cap_precedes_continuation_overflow_and_preserves_cursor() {
    for width in [1, 4, 32, 63] {
        let groups = variable_groups((1_u128 << 64) + (1_u128 << width), width);
        let prefix_groups = u8::try_from(groups.len() - 1).expect("bounded prefix");
        let prefix = 1_u64 << (64 - width);
        let bytes = group_bytes(&groups, width);
        let consumed = usize::from(prefix_groups) * (usize::from(width) + 1);
        for (limit, expected) in [
            (prefix_groups, Ok(prefix)),
            (255, Err(OamdError::ValueOverflow)),
        ] {
            let mut reader = BitReader::new(&bytes);
            assert_eq!(variable_bits_max(&mut reader, width, limit), expected);
            assert_eq!(reader.bits_remaining(), bytes.len() * 8 - consumed);
        }
    }

    // Even without a following group, the continuation already overflows.
    let bytes = ((2_u64 << 1) | 1).to_be_bytes();
    let mut reader = BitReader::new(&bytes);
    assert_eq!(
        variable_bits_max(&mut reader, 63, 2),
        Err(OamdError::ValueOverflow)
    );
    assert_eq!(reader.bits_remaining(), 0);
}

#[test]
fn preserves_all_values_reachable_by_payload_size_bounds() {
    // Both payload entry points use width=4, max_groups=4 (maximum 69,903).
    for value in 0_u64..=69_903 {
        let mut groups = variable_groups(u128::from(value), 4);
        let limit = u8::try_from(groups.len()).expect("bounded size groups");
        assert!(limit <= 4);
        for final_more in [false, true] {
            groups.last_mut().expect("nonempty groups").1 = final_more;
            let bytes = group_bytes(&groups, 4);
            let mut reader = BitReader::new(&bytes);
            let max_groups = if final_more { limit } else { 4 };
            assert_eq!(variable_bits_max(&mut reader, 4, max_groups), Ok(value));
            assert_eq!(reader.read_bits(4), Ok(0b1011));
        }
    }
}
