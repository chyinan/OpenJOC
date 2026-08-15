use openjoc_oamd::{
    ContentDescription, Gain, OamdContentPrefix, OamdDecoderConfig, OamdElement, OamdError,
    OamdParseProfile, ObjectBasicInfo, ObjectClass, ObjectRenderInfo, OpaqueBits,
    parse_oamd_payload, parse_oamd_payload_with_config, parse_oamd_payload_with_profile,
    trace_oamd_payload,
};
use std::num::NonZeroU8;

fn push(bits: &mut Vec<bool>, value: u64, width: u8) {
    for shift in (0..width).rev() {
        bits.push(value & (1_u64 << shift) != 0);
    }
}

fn pack(mut bits: Vec<bool>) -> Vec<u8> {
    while bits.len() % 8 != 0 {
        bits.push(false);
    }
    let mut bytes = vec![0; bits.len() / 8];
    for (index, bit) in bits.into_iter().enumerate() {
        if bit {
            bytes[index / 8] |= 0x80 >> (index % 8);
        }
    }
    bytes
}

fn dynamic_prefix(bits: &mut Vec<bool>, object_count_minus_one: u8, element_count: u8) {
    push(bits, 0, 2);
    push(bits, object_count_minus_one.into(), 5);
    push(bits, 1, 1); // dynamic-only program
    push(bits, 0, 1); // no LFE
    push(bits, 0, 1); // no alternate object data
    push(bits, element_count.into(), 4);
}

fn inactive_object_element_body() -> Vec<bool> {
    let mut body = Vec::new();
    push(&mut body, 0, 2); // sample offset 0
    push(&mut body, 0, 3); // one update block
    push(&mut body, 0, 6);
    push(&mut body, 0, 2); // no ramp
    push(&mut body, 1, 1); // reserved data absent
    push(&mut body, 1, 1); // object inactive
    push(&mut body, 0, 1); // no additional table data
    body
}

fn push_element(bits: &mut Vec<bool>, id: u8, size_bytes: u8, content: &[bool]) {
    push(bits, id.into(), 4);
    push(bits, u64::from(size_bytes - 1), 4);
    push(bits, 0, 1); // variable_bits_max continuation false
    bits.extend_from_slice(content);
    let target = usize::from(size_bytes) * 8;
    assert!(content.len() <= target);
    bits.resize(bits.len() + target - content.len(), false);
}

#[test]
fn parses_a_complete_bounded_object_audio_metadata_payload() {
    let mut bits = Vec::new();
    dynamic_prefix(&mut bits, 0, 1);
    let mut content = vec![false]; // discard unknown false
    content.extend(inactive_object_element_body());
    push_element(&mut bits, 1, 3, &content);

    let payload = parse_oamd_payload(&pack(bits)).expect("payload");
    assert_eq!(
        payload.prefix,
        OamdContentPrefix {
            syntax_version: 0,
            object_count: 1,
            content: ContentDescription::DynamicOnly { lfe_present: false },
            alternate_object_data_present: false,
            element_count: 1,
            consumed_bits: 14,
        }
    );
    assert_eq!(payload.object_classes, vec![ObjectClass::Dynamic]);
    assert_eq!(payload.elements.len(), 1);
    let OamdElement::Objects(metadata) = &payload.elements[0].element else {
        panic!("expected object element");
    };
    assert!(!metadata.objects[0][0].active);
    assert_eq!(metadata.objects[0][0].basic, ObjectBasicInfo::DEFAULT);
    assert_eq!(metadata.objects[0][0].render, ObjectRenderInfo::DEFAULT);
    assert!(!payload.elements[0].discard_unknown);
}

#[test]
fn retains_unknown_element_bits_and_discard_intent() {
    let mut bits = Vec::new();
    dynamic_prefix(&mut bits, 0, 1);
    let mut content = vec![true]; // discard unknown true
    for shift in (0..7).rev() {
        content.push(0b101_0101 & (1 << shift) != 0);
    }
    push_element(&mut bits, 9, 1, &content);

    let payload = parse_oamd_payload(&pack(bits)).expect("unknown payload");
    assert!(payload.elements[0].discard_unknown);
    assert_eq!(
        payload.elements[0].element,
        OamdElement::Unknown(OpaqueBits {
            bytes: vec![0b1010_1010],
            bit_len: 7,
        })
    );
}

#[test]
fn rejects_nonzero_known_padding_and_truncated_element_windows() {
    let mut padding = Vec::new();
    dynamic_prefix(&mut padding, 0, 1);
    let mut content = vec![false];
    content.extend(inactive_object_element_body());
    content.push(true); // first padding bit must be zero
    push_element(&mut padding, 1, 3, &content);
    assert_eq!(
        parse_oamd_payload(&pack(padding)),
        Err(OamdError::NonzeroPadding)
    );

    let mut truncated = Vec::new();
    dynamic_prefix(&mut truncated, 0, 1);
    push(&mut truncated, 1, 4);
    push(&mut truncated, 2, 4); // declares three bytes
    push(&mut truncated, 0, 1);
    push(&mut truncated, 0, 8); // fewer than three bytes remain
    assert!(parse_oamd_payload(&pack(truncated)).is_err());
}

#[test]
fn rejects_content_description_object_count_mismatch() {
    let mut bits = Vec::new();
    push(&mut bits, 0, 2);
    push(&mut bits, 1, 5); // two total objects
    push(&mut bits, 0, 1); // mixed program
    push(&mut bits, 0b0010, 4); // dynamic objects only in mixed form
    push(&mut bits, 0, 5); // one dynamic object
    push(&mut bits, 0, 1);
    push(&mut bits, 0, 4);
    assert_eq!(
        parse_oamd_payload(&pack(bits)),
        Err(OamdError::ObjectCountMismatch {
            declared: 2,
            described: 1,
        })
    );
}

#[test]
fn rejects_dynamic_only_lfe_without_a_dynamic_object() {
    let mut bits = Vec::new();
    push(&mut bits, 0, 2);
    push(&mut bits, 0, 5); // one total object
    push(&mut bits, 1, 1); // dynamic-only program
    push(&mut bits, 1, 1); // LFE present, leaving no dynamic object
    push(&mut bits, 0, 1);
    push(&mut bits, 0, 4);
    assert_eq!(
        parse_oamd_payload(&pack(bits)),
        Err(OamdError::ObjectCountMismatch {
            declared: 1,
            described: 2,
        })
    );

    let mut valid = Vec::new();
    push(&mut valid, 0, 2);
    push(&mut valid, 1, 5); // LFE plus one dynamic object
    push(&mut valid, 1, 1);
    push(&mut valid, 1, 1);
    push(&mut valid, 0, 1);
    push(&mut valid, 0, 4);
    assert_eq!(
        parse_oamd_payload(&pack(valid))
            .expect("dynamic plus LFE")
            .object_classes,
        vec![ObjectClass::BedOrIsf, ObjectClass::Dynamic]
    );
}

#[test]
fn rejects_reserved_alternate_object_data_for_object_elements() {
    let mut bits = Vec::new();
    push(&mut bits, 0, 2);
    push(&mut bits, 0, 5);
    push(&mut bits, 1, 1);
    push(&mut bits, 0, 1);
    push(&mut bits, 1, 1); // alternate IDs present
    push(&mut bits, 1, 4);
    let mut content = Vec::new();
    push(&mut content, 1, 4); // reserved alternate object data ID
    content.push(false);
    content.extend(inactive_object_element_body());
    push_element(&mut bits, 1, 3, &content);
    assert_eq!(
        parse_oamd_payload(&pack(bits)),
        Err(OamdError::ReservedAlternateObjectData { id: 1 })
    );
}

#[test]
fn derives_bed_isf_and_dynamic_classes_in_normative_order() {
    let mut bits = Vec::new();
    push(&mut bits, 0, 2);
    push(&mut bits, 6, 5); // seven total objects
    push(&mut bits, 0, 1); // mixed program
    push(&mut bits, 0b1110, 4); // bed + ISF + dynamic
    push(&mut bits, 0, 1); // do not distribute bed
    push(&mut bits, 0, 1); // one bed instance
    push(&mut bits, 0, 1); // not LFE-only
    push(&mut bits, 1, 1); // standard assignment
    push(&mut bits, 1 << 9, 10); // RC_L/RC_R: two bed objects
    push(&mut bits, 0, 3); // four-object ISF
    push(&mut bits, 0, 5); // one dynamic object
    push(&mut bits, 0, 1);
    push(&mut bits, 0, 4);

    let payload = parse_oamd_payload(&pack(bits)).expect("mixed classes");
    assert_eq!(
        payload.object_classes,
        vec![
            ObjectClass::BedOrIsf,
            ObjectClass::BedOrIsf,
            ObjectClass::BedOrIsf,
            ObjectClass::BedOrIsf,
            ObjectClass::BedOrIsf,
            ObjectClass::BedOrIsf,
            ObjectClass::Dynamic,
        ]
    );
}

#[test]
fn extended_element_dispatches_only_with_preceding_object_state() {
    let mut bits = Vec::new();
    dynamic_prefix(&mut bits, 0, 2);
    let mut objects = vec![false]; // discard unknown false
    objects.extend(inactive_object_element_body());
    push_element(&mut bits, 1, 3, &objects);
    let extension = vec![
        false, // discard unknown false
        false, // no divergence block
        true,  // precision block present; inactive object consumes no bits
    ];
    push_element(&mut bits, 5, 1, &extension);

    let payload = parse_oamd_payload(&pack(bits)).expect("extended payload");
    assert!(matches!(
        payload.elements[1].element,
        OamdElement::Extended(_)
    ));

    let mut missing = Vec::new();
    dynamic_prefix(&mut missing, 0, 1);
    push_element(&mut missing, 5, 1, &[false, false, false]);
    assert_eq!(
        parse_oamd_payload(&pack(missing)),
        Err(OamdError::MissingObjectElementForExtension)
    );
}

#[test]
fn trim_element_requires_explicit_cardinality_and_is_then_decoded() {
    let mut bits = Vec::new();
    dynamic_prefix(&mut bits, 0, 1);
    let mut content = vec![false]; // discard unknown false
    push(&mut content, 0, 2); // no warp
    push(&mut content, 0, 2); // reserved
    push(&mut content, 0, 2); // default global trim
    push(&mut content, 0, 1); // no per-object flags
    push_element(&mut bits, 2, 1, &content);
    let bytes = pack(bits);

    assert_eq!(
        parse_oamd_payload(&bytes),
        Err(OamdError::MissingTrimConfigurationCount)
    );
    let payload = parse_oamd_payload_with_config(
        &bytes,
        OamdDecoderConfig {
            trim_configuration_count: Some(NonZeroU8::new(1).expect("nonzero configuration count")),
        },
    )
    .expect("configured trim payload");
    assert!(matches!(payload.elements[0].element, OamdElement::Trim(_)));
}

#[test]
fn forensic_trace_preserves_reserved_trim_warp_bits_without_accepting_them() {
    let mut bits = Vec::new();
    dynamic_prefix(&mut bits, 0, 1);
    let mut content = vec![false]; // discard unknown false
    push(&mut content, 3, 2); // reserved warp mode, retained by the trace
    push(&mut content, 0, 2); // reserved trim data
    push(&mut content, 0, 2); // default global trim
    push(&mut content, 0, 1); // no per-object flags
    push_element(&mut bits, 2, 1, &content);
    let bytes = pack(bits);

    let trace = trace_oamd_payload(&bytes).expect("bounded forensic trace");
    assert_eq!(trace.object_count, 1);
    assert_eq!(trace.element_count, 1);
    assert_eq!(trace.elements[0].body_start_bit, 23);
    assert_eq!(trace.elements[0].warp_mode_start_bit, Some(24));
    assert_eq!(trace.elements[0].warp_mode_raw, Some(3));
    assert_eq!(
        parse_oamd_payload_with_config(
            &bytes,
            OamdDecoderConfig {
                trim_configuration_count: Some(
                    NonZeroU8::new(1).expect("nonzero configuration count"),
                ),
            },
        ),
        Err(OamdError::ReservedWarpMode { code: 3 })
    );
}

#[test]
fn vendor_profile_retains_reserved_trim_body_without_remapping_warp() {
    let mut bits = Vec::new();
    dynamic_prefix(&mut bits, 0, 1);
    let mut content = vec![false]; // discard unknown false
    push(&mut content, 3, 2); // raw reserved warp 3
    push(&mut content, 0, 2); // reserved trim data
    push(&mut content, 0, 2); // default global trim
    push(&mut content, 0, 1); // no per-object flags
    push_element(&mut bits, 2, 1, &content);
    let bytes = pack(bits);

    let strict = parse_oamd_payload_with_config(
        &bytes,
        OamdDecoderConfig {
            trim_configuration_count: Some(NonZeroU8::new(1).expect("nonzero count")),
        },
    );
    assert_eq!(strict, Err(OamdError::ReservedWarpMode { code: 3 }));

    let vendor = parse_oamd_payload_with_profile(
        &bytes,
        OamdDecoderConfig {
            trim_configuration_count: Some(NonZeroU8::new(1).expect("nonzero count")),
        },
        OamdParseProfile::ObservedVendorCompat,
        11,
    )
    .expect("narrow vendor opaque retention");
    let OamdElement::OpaqueObservedKnownElement(opaque) = &vendor.elements[0].element else {
        panic!("expected opaque observed trim element");
    };
    assert_eq!(opaque.element_id, 2);
    assert_eq!(opaque.declared_bits, 8);
    assert_eq!(opaque.declared_bytes, 1);
    assert_eq!(opaque.valid_bits_in_last_byte, 8);
    assert_eq!(opaque.raw_body.bytes, vec![0b0110_0000]);
    assert_eq!(opaque.raw_body.bit_len, 8);
    assert_eq!(opaque.body_payload_start_bit, 23);
    assert_eq!(opaque.body_payload_end_bit, 31);
    assert_eq!(opaque.raw_warp, 3);
    assert_eq!(opaque.warp_element_relative_start_bit, 1);
    assert_eq!(opaque.warp_element_relative_end_bit, 3);
    assert_eq!(opaque.warp_payload_start_bit, 24);
    assert_eq!(opaque.warp_payload_end_bit, 26);
    assert_eq!(opaque.continuation_element_relative_start_bit, 3);
    assert_eq!(opaque.continuation_element_relative_end_bit, 8);
    assert_eq!(opaque.continuation_payload_start_bit, 26);
    assert_eq!(opaque.continuation_payload_end_bit, 31);
    assert_eq!(opaque.vendor_continuation().bit_len(), 5);
    assert_eq!(
        (0..5)
            .map(|index| opaque.vendor_continuation().bit(index))
            .collect::<Vec<_>>(),
        vec![Some(false); 5]
    );
    assert_eq!(opaque.preservation_status, "opaque_lossless_bounded");
    assert_eq!(opaque.provenance, "vendor_observed_normative_unresolved");
    assert_eq!(opaque.interpretation_status, "unresolved");
    assert_eq!(
        opaque.first_parser_error,
        OamdError::ReservedWarpMode { code: 3 }
    );
    assert_eq!(opaque.deviation_code, "LOGIC_OAMD_RESERVED_TRIM_WARP_3");
}

#[test]
fn vendor_profile_preserves_nonzero_non_byte_aligned_continuation_exactly() {
    let mut bits = Vec::new();
    dynamic_prefix(&mut bits, 0, 1);
    let mut content = vec![false]; // discard unknown false
    push(&mut content, 3, 2); // raw reserved warp 3
    push(&mut content, 0b10101, 5); // opaque continuation; not a semantic field
    push_element(&mut bits, 2, 1, &content);
    let vendor = parse_oamd_payload_with_profile(
        &pack(bits),
        OamdDecoderConfig {
            trim_configuration_count: Some(NonZeroU8::new(1).expect("nonzero count")),
        },
        OamdParseProfile::ObservedVendorCompat,
        11,
    )
    .expect("vendor opaque retention");
    let OamdElement::OpaqueObservedKnownElement(opaque) = &vendor.elements[0].element else {
        panic!("expected opaque observed trim element");
    };
    let continuation = opaque.vendor_continuation();
    assert_eq!(continuation.start_bit, 3);
    assert_eq!(continuation.end_bit, 8);
    assert_eq!(continuation.bit_len(), 5);
    assert_eq!(
        (0..5)
            .map(|index| continuation.bit(index))
            .collect::<Vec<_>>(),
        vec![Some(true), Some(false), Some(true), Some(false), Some(true),]
    );
    assert_ne!(opaque.continuation_sha256, opaque.raw_body_sha256);
}

#[test]
fn vendor_profile_does_not_generalize_reserved_trim_errors() {
    for (warp, expected) in [
        (2_u8, OamdError::ReservedWarpMode { code: 2 }),
        (0_u8, OamdError::ReservedGlobalTrimMode),
    ] {
        let mut bits = Vec::new();
        dynamic_prefix(&mut bits, 0, 1);
        let mut content = vec![false];
        push(&mut content, warp.into(), 2);
        push(&mut content, 0, 2); // reserved trim data
        push(&mut content, if warp == 0 { 3 } else { 0 }, 2);
        push(&mut content, 0, 1);
        push_element(&mut bits, 2, 1, &content);
        let result = parse_oamd_payload_with_profile(
            &pack(bits),
            OamdDecoderConfig {
                trim_configuration_count: Some(NonZeroU8::new(1).expect("nonzero count")),
            },
            OamdParseProfile::ObservedVendorCompat,
            11,
        );
        assert_eq!(result, Err(expected));
    }
}

#[test]
fn vendor_opaque_retention_requires_payload_id_eleven() {
    let mut bits = Vec::new();
    dynamic_prefix(&mut bits, 0, 1);
    let mut content = vec![false];
    push(&mut content, 3, 2);
    push(&mut content, 0, 2);
    push(&mut content, 0, 2);
    push(&mut content, 0, 1);
    push_element(&mut bits, 2, 1, &content);
    let result = parse_oamd_payload_with_profile(
        &pack(bits),
        OamdDecoderConfig {
            trim_configuration_count: Some(NonZeroU8::new(1).expect("nonzero count")),
        },
        OamdParseProfile::ObservedVendorCompat,
        14,
    );
    assert_eq!(
        result,
        Err(OamdError::VendorProfilePayloadId { payload_id: 14 })
    );
}

#[test]
fn basic_default_is_not_accidentally_a_finite_gain() {
    assert_eq!(ObjectBasicInfo::DEFAULT.gain, Gain::NegativeInfinity);
}
