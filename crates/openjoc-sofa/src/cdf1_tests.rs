// pattern: Functional Core

use super::{
    Cursor, SofaError, SofaLoadLimits, parse_attributes, parse_dimensions, parse_variables,
};

fn words(values: &[u32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_be_bytes())
        .collect()
}

#[test]
fn standard_absent_attributes_consume_both_zero_words() {
    for variable in [false, true] {
        let bytes = words(&[0, 0, 6]);
        let mut cursor = Cursor::new(&bytes);
        assert!(
            parse_attributes(&mut cursor, 1024, variable)
                .unwrap()
                .is_empty()
        );
        assert_eq!(cursor.pos, 8);
        assert_eq!(cursor.u32().unwrap(), 6);
    }
}

#[test]
fn legacy_variable_attributes_leave_the_known_type_unconsumed() {
    for ty in 1..=6 {
        let bytes = words(&[0, ty, 16, 64]);
        let mut cursor = Cursor::new(&bytes);
        assert!(
            parse_attributes(&mut cursor, 1024, true)
                .unwrap()
                .is_empty()
        );
        assert_eq!(cursor.pos, 4);
        assert_eq!(cursor.u32().unwrap(), ty);
        assert_eq!(cursor.u32().unwrap(), 16);
    }
}

#[test]
fn legacy_attributes_do_not_extend_to_globals_or_unknown_types() {
    for (variable, count) in [(false, 6), (true, 7), (true, 12), (true, u32::MAX)] {
        let bytes = words(&[0, count]);
        assert!(matches!(
            parse_attributes(&mut Cursor::new(&bytes), 1024, variable),
            Err(SofaError::InvalidContainer("absent attribute count"))
        ));
    }
}

#[test]
fn empty_dimension_and_variable_lists_consume_and_validate_the_count() {
    let bytes = words(&[0, 0, 12]);
    let mut dimensions = Cursor::new(&bytes);
    assert!(parse_dimensions(&mut dimensions).unwrap().is_empty());
    assert_eq!(dimensions.u32().unwrap(), 12);
    let mut variables = Cursor::new(&bytes);
    assert!(
        parse_variables(&mut variables, &[], SofaLoadLimits::default())
            .unwrap()
            .is_empty()
    );
    assert_eq!(variables.u32().unwrap(), 12);

    let invalid = words(&[0, 1]);
    assert!(matches!(
        parse_dimensions(&mut Cursor::new(&invalid)),
        Err(SofaError::InvalidContainer("absent dimension count"))
    ));
    assert!(matches!(
        parse_variables(&mut Cursor::new(&invalid), &[], SofaLoadLimits::default()),
        Err(SofaError::InvalidContainer("absent variable count"))
    ));
}

#[test]
fn truncated_absent_lists_are_rejected() {
    let bytes = words(&[0, 0]);
    for length in 0..bytes.len() {
        let truncated = &bytes[..length];
        assert!(matches!(
            parse_dimensions(&mut Cursor::new(truncated)),
            Err(SofaError::TruncatedContainer)
        ));
        assert!(matches!(
            parse_variables(&mut Cursor::new(truncated), &[], SofaLoadLimits::default()),
            Err(SofaError::TruncatedContainer)
        ));
        for variable in [false, true] {
            assert!(matches!(
                parse_attributes(&mut Cursor::new(truncated), 1024, variable),
                Err(SofaError::TruncatedContainer)
            ));
        }
    }
}

#[test]
fn explicit_zero_length_attribute_list_remains_supported() {
    let bytes = words(&[12, 0, 5]);
    let mut cursor = Cursor::new(&bytes);
    assert!(
        parse_attributes(&mut cursor, 1024, true)
            .unwrap()
            .is_empty()
    );
    assert_eq!(cursor.u32().unwrap(), 5);
}
