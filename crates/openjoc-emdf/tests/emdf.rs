use openjoc_bitio::BitReader;
use openjoc_emdf::{
    CarrierClassification, EmdfContainer, EmdfError, EmdfPayload, EmdfPayloadConfig,
    EmdfProtection, JOC_PAYLOAD_ID, OAMD_PAYLOAD_ID, classify_emdf_carrier, parse_emdf_sync,
    validate_joc_profile, variable_bits,
};

#[derive(Default)]
struct Bits {
    values: Vec<bool>,
}

impl Bits {
    fn push(&mut self, value: u64, width: u8) {
        for shift in (0..width).rev() {
            self.values.push((value >> shift) & 1 != 0);
        }
    }

    fn variable(&mut self, groups: &[(u64, bool)], width: u8) {
        for &(value, read_more) in groups {
            self.push(value, width);
            self.push(u64::from(read_more), 1);
        }
    }

    fn bytes(mut self) -> Vec<u8> {
        while self.values.len() % 8 != 0 {
            self.values.push(false);
        }
        self.values
            .chunks(8)
            .map(|chunk| {
                chunk
                    .iter()
                    .fold(0_u8, |value, bit| (value << 1) | u8::from(*bit))
            })
            .collect()
    }
}

fn wrap_sync(container: &[u8]) -> Vec<u8> {
    let length = u16::try_from(container.len()).expect("test container length");
    let mut bytes = vec![0x58, 0x38];
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(container);
    bytes
}

fn minimal_container(version: u8, primary_code: u8, secondary_code: u8) -> Bits {
    let mut container = Bits::default();
    container.push(u64::from(version), 2);
    container.push(0, 3);
    container.push(0, 5);
    container.push(u64::from(primary_code), 2);
    container.push(u64::from(secondary_code), 2);
    let primary_bytes = [0, 1, 4, 16][usize::from(primary_code)];
    let secondary_bytes = [0, 1, 4, 16][usize::from(secondary_code)];
    for byte in 0..primary_bytes + secondary_bytes {
        container.push(u64::try_from(byte).expect("byte"), 8);
    }
    container
}

#[test]
fn variable_bits_adds_the_normative_group_offset() {
    let mut two = Bits::default();
    two.variable(&[(0, true), (3, false)], 2);
    let bytes = two.bytes();
    let mut reader = BitReader::new(&bytes);
    assert_eq!(variable_bits(&mut reader, 2, 3), Ok(7));

    let mut three = Bits::default();
    three.variable(&[(1, true), (2, true), (3, false)], 2);
    assert_eq!(
        variable_bits(&mut BitReader::new(&three.bytes()), 2, 3),
        Ok(47)
    );
}

#[test]
fn variable_bits_enforces_group_and_arithmetic_bounds() {
    let mut too_many = Bits::default();
    too_many.variable(&[(0, true), (0, true), (0, false)], 2);
    assert_eq!(
        variable_bits(&mut BitReader::new(&too_many.bytes()), 2, 2),
        Err(EmdfError::VariableBitsGroupLimit { width: 2, limit: 2 })
    );
    assert_eq!(
        variable_bits(&mut BitReader::new(&[0]), 0, 1),
        Err(EmdfError::InvalidVariableBits {
            width: 0,
            max_groups: 1
        })
    );
}

#[test]
fn parses_payload_configuration_conditionals_and_unknown_payload_bytes() {
    let mut container = Bits::default();
    container.push(0, 2); // version
    container.push(2, 3); // key ID
    container.push(14, 5); // JOC payload ID
    container.push(1, 1); // sample offset exists
    container.push(123, 11);
    container.push(0, 1); // reserved
    container.push(1, 1); // duration exists
    container.variable(&[(7, true), (9, false)], 11);
    container.push(1, 1); // group ID exists
    container.variable(&[(2, true), (1, false)], 2);
    container.push(0, 1); // no codec data
    container.push(0, 1); // retain unknown
    container.push(17, 5); // priority (sample offset exists)
    container.push(2, 2); // processing allowed
    container.variable(&[(2, false)], 8); // payload size
    container.push(0xa5, 8);
    container.push(0x5a, 8);
    container.push(0, 5); // container end
    container.push(1, 2); // primary: 8 bits
    container.push(2, 2); // secondary: 32 bits
    container.push(0x7e, 8);
    for byte in [1, 2, 3, 4] {
        container.push(byte, 8);
    }

    let bytes = wrap_sync(&container.bytes());
    let parsed = parse_emdf_sync(&bytes).expect("valid EMDF");
    assert_eq!(parsed.bytes_consumed, bytes.len());
    assert_eq!(parsed.container.version, 0);
    assert_eq!(parsed.container.key_id, 2);
    assert_eq!(parsed.container.payloads.len(), 1);
    let payload = &parsed.container.payloads[0];
    assert_eq!(payload.id, 14);
    assert_eq!(payload.data, [0xa5, 0x5a]);
    assert_eq!(
        payload.config,
        EmdfPayloadConfig {
            sample_offset: Some(123),
            duration: Some(16_393),
            group_id: Some(13),
            codec_data_present: false,
            discard_unknown_payload: false,
            payload_frame_aligned: None,
            create_duplicate: None,
            remove_duplicate: None,
            priority: Some(17),
            processing_allowed: Some(2),
        }
    );
    assert_eq!(parsed.container.protection.primary, [0x7e]);
    assert_eq!(parsed.container.protection.secondary, [1, 2, 3, 4]);
}

#[test]
fn parses_frame_aligned_duplicate_controls() {
    let mut container = Bits::default();
    container.push(0, 2);
    container.push(0, 3);
    container.push(11, 5); // OAMD
    container.push(0, 1); // no sample offset
    container.push(0, 1); // no duration
    container.push(0, 1); // no group
    container.push(0, 1); // no codec data
    container.push(0, 1); // retain unknown
    container.push(1, 1); // frame aligned
    container.push(1, 1); // create duplicate
    container.push(0, 1); // remove duplicate
    container.push(3, 5);
    container.push(1, 2);
    container.variable(&[(0, false)], 8);
    container.push(0, 5);
    container.push(1, 2);
    container.push(0, 2);
    container.push(0, 8);

    let parsed = parse_emdf_sync(&wrap_sync(&container.bytes())).expect("valid EMDF");
    let config = &parsed.container.payloads[0].config;
    assert_eq!(config.payload_frame_aligned, Some(true));
    assert_eq!(config.create_duplicate, Some(true));
    assert_eq!(config.remove_duplicate, Some(false));
    assert_eq!(config.priority, Some(3));
    assert_eq!(config.processing_allowed, Some(1));
}

#[test]
fn rejects_nonzero_base_version_and_more_than_byte_boundary_padding() {
    assert_eq!(
        parse_emdf_sync(&wrap_sync(&minimal_container(1, 1, 0).bytes())),
        Err(EmdfError::UnsupportedVersion { version: 1 })
    );

    let mut extra_byte = minimal_container(0, 1, 0).bytes();
    extra_byte.push(0);
    assert_eq!(
        parse_emdf_sync(&wrap_sync(&extra_byte)),
        Err(EmdfError::ExcessPadding { bits: 10 })
    );
}

#[test]
fn parses_every_normative_protection_length_pair() {
    for primary_code in 1..=3 {
        for secondary_code in 0..=3 {
            let parsed = parse_emdf_sync(&wrap_sync(
                &minimal_container(0, primary_code, secondary_code).bytes(),
            ))
            .expect("normative protection lengths");
            assert_eq!(
                parsed.container.protection.primary.len(),
                [0, 1, 4, 16][usize::from(primary_code)]
            );
            assert_eq!(
                parsed.container.protection.secondary.len(),
                [0, 1, 4, 16][usize::from(secondary_code)]
            );
        }
    }
    assert_eq!(
        parse_emdf_sync(&wrap_sync(&minimal_container(0, 0, 0).bytes())),
        Err(EmdfError::ReservedPrimaryProtectionLength)
    );
}

#[test]
fn rejects_truncation_reserved_data_codec_data_and_nonzero_padding() {
    assert_eq!(
        parse_emdf_sync(&[0x58, 0x38, 0, 2, 0]),
        Err(EmdfError::TruncatedContainer {
            declared: 2,
            available: 1,
        })
    );

    let mut reserved = Bits::default();
    reserved.push(0, 2);
    reserved.push(0, 3);
    reserved.push(11, 5);
    reserved.push(1, 1);
    reserved.push(0, 11);
    reserved.push(1, 1);
    assert_eq!(
        parse_emdf_sync(&wrap_sync(&reserved.bytes())),
        Err(EmdfError::NonzeroReservedData)
    );

    let mut codec_data = Bits::default();
    codec_data.push(0, 2);
    codec_data.push(0, 3);
    codec_data.push(11, 5);
    codec_data.push(0, 1);
    codec_data.push(0, 1);
    codec_data.push(0, 1);
    codec_data.push(1, 1);
    codec_data.push(1, 8);
    assert_eq!(
        parse_emdf_sync(&wrap_sync(&codec_data.bytes())),
        Err(EmdfError::NonzeroReservedData)
    );

    let mut padding = minimal_container(0, 1, 0);
    padding.values.push(true);
    assert_eq!(
        parse_emdf_sync(&wrap_sync(&padding.bytes())),
        Err(EmdfError::NonzeroPadding)
    );
}

#[test]
fn classifies_only_an_exact_carrier_start_as_emdf() {
    let non_emdf = [0x00, 0x58, 0x38, 0x00];
    assert_eq!(
        classify_emdf_carrier(&non_emdf),
        CarrierClassification::NonEmdf
    );

    let truncated = [0x58, 0x38, 0x00];
    assert_eq!(
        classify_emdf_carrier(&truncated),
        CarrierClassification::Malformed(EmdfError::TruncatedContainer {
            declared: 4,
            available: 3,
        })
    );
}

#[test]
fn classifies_exact_container_and_rejects_undeclared_carrier_trailing_bytes() {
    let bytes = wrap_sync(&minimal_container(0, 1, 0).bytes());
    let parsed = parse_emdf_sync(&bytes).expect("valid bounded EMDF");
    assert_eq!(
        classify_emdf_carrier(&bytes),
        CarrierClassification::Parsed(parsed)
    );

    let mut trailing = bytes;
    trailing.push(0);
    assert_eq!(
        classify_emdf_carrier(&trailing),
        CarrierClassification::TrailingData {
            container_bytes: trailing.len() - 1,
            carrier_bytes: trailing.len(),
        }
    );
}

#[test]
fn accepts_zero_reserved_codec_data_required_by_the_joc_profile() {
    let mut container = Bits::default();
    container.push(0, 2);
    container.push(0, 3);
    container.push(11, 5);
    container.push(0, 1); // no sample offset
    container.push(0, 1); // no duration
    container.push(1, 1); // group ID exists
    container.variable(&[(1, false)], 2);
    container.push(1, 1); // codec data exists
    container.push(0, 8); // reserved
    container.push(1, 1); // discard unknown (no further config fields)
    container.variable(&[(0, false)], 8);
    container.push(0, 5);
    container.push(1, 2);
    container.push(0, 2);
    container.push(0, 8);

    let parsed = parse_emdf_sync(&wrap_sync(&container.bytes())).expect("JOC profile config");
    assert!(parsed.container.payloads[0].config.codec_data_present);
}

fn joc_profile_config(group_id: u64) -> EmdfPayloadConfig {
    EmdfPayloadConfig {
        sample_offset: None,
        duration: None,
        group_id: Some(group_id),
        codec_data_present: true,
        discard_unknown_payload: false,
        payload_frame_aligned: Some(true),
        create_duplicate: Some(false),
        remove_duplicate: Some(false),
        priority: Some(0),
        processing_allowed: Some(0),
    }
}

fn joc_profile_container() -> EmdfContainer {
    EmdfContainer {
        version: 0,
        key_id: 0,
        payloads: vec![
            EmdfPayload {
                id: OAMD_PAYLOAD_ID,
                config: joc_profile_config(7),
                data: vec![1],
            },
            EmdfPayload {
                id: JOC_PAYLOAD_ID,
                config: joc_profile_config(7),
                data: vec![2],
            },
        ],
        protection: EmdfProtection {
            primary: vec![0],
            secondary: Vec::new(),
        },
    }
}

#[test]
fn validates_the_complete_table_55_and_56_joc_profile() {
    let container = joc_profile_container();
    let pair = validate_joc_profile(&container).expect("valid OAMD/JOC pair");
    assert_eq!(pair.oamd, [1]);
    assert_eq!(pair.joc, [2]);

    let mut wrong_group = joc_profile_container();
    wrong_group.payloads[1].config.group_id = Some(8);
    assert_eq!(
        validate_joc_profile(&wrong_group),
        Err(EmdfError::JocProfileConfiguration)
    );

    let mut duplicate = joc_profile_container();
    duplicate.payloads.push(duplicate.payloads[0].clone());
    assert_eq!(
        validate_joc_profile(&duplicate),
        Err(EmdfError::JocProfilePayloadCount { oamd: 2, joc: 1 })
    );
}
