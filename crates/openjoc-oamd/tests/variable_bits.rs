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
