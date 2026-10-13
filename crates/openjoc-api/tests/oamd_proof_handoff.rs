#![cfg(not(target_arch = "wasm32"))]

use openjoc_eac3::{
    DecodedJocAccessUnitPcm, InternalBasePolicy, JocAccessUnitPcmDecoder, JocMetadataFrame,
    extract_joc_access_unit, group_access_units, index_syncframes,
};
use openjoc_joc::{ReconstructionBasis, ReconstructionStageTiming};
use openjoc_oamd::{
    OAMD_PAYLOAD_ID, OamdDecoderConfig, OamdParseProfile, OamdPayload, ParsedOamdProof,
    parse_oamd_payload_with_config, parse_oamd_payload_with_profile,
};
use openjoc_scene::{
    BindingCodecProfile, DecodedJocBindingFacts, DecodedJocBindingUnavailable, DecodedPayloadFrame,
    JocFrameInput, OamdBindingObjectClass, PayloadDecodeError, PayloadDecoder,
    PayloadDecoderConfig, admit_decoded_joc_binding,
};
use std::num::NonZeroU8;

// Intended destination: crates/openjoc-api/tests/oamd_proof_handoff.rs.
// Use only complete, unchanged existing lifecycle AUs. These comparisons supplement,
// and do not replace, the independent frozen pristine/candidate raw-PCM gates.
const FIXTURE: &[u8] = include_bytes!("../../openjoc-wasm/testdata/joc.lifecycle.ec3");
const ORDINARY: BindingCodecProfile = BindingCodecProfile::EAc3JocObservedOrdinary;
const PROFILES: [OamdParseProfile; 2] = [
    OamdParseProfile::EtsiStrict,
    OamdParseProfile::ObservedVendorCompat,
];

struct FixtureFrame {
    metadata: JocMetadataFrame,
    pcm: DecodedJocAccessUnitPcm,
}

impl FixtureFrame {
    fn input(&self, frame_index: usize) -> JocFrameInput<'_> {
        JocFrameInput {
            sample_rate: self.metadata.sample_rate,
            downmix_pcm: &self.pcm.joc_input_pcm.channels,
            base_lfe_pcm: self.pcm.compatibility_pcm.lfe.as_deref(),
            joc_payload: &self.metadata.joc,
            oamd_payload: &self.metadata.oamd,
            frame_index: u64::try_from(frame_index).unwrap(),
        }
    }
}

fn metadata_frames() -> Vec<JocMetadataFrame> {
    let frames = index_syncframes(FIXTURE).unwrap();
    let units = group_access_units(&frames).unwrap();
    assert!(units.len() >= 6);
    let metadata: Vec<_> = units
        .into_iter()
        .take(6)
        .map(|unit| {
            extract_joc_access_unit(FIXTURE, &frames, unit)
                .unwrap()
                .unwrap()
        })
        .collect();
    assert_ne!(metadata[0].oamd, metadata[1].oamd);
    assert_ne!(
        legacy_parse(&metadata[0].oamd, OamdDecoderConfig::default(), PROFILES[0]),
        legacy_parse(&metadata[1].oamd, OamdDecoderConfig::default(), PROFILES[0]),
        "fixture byte diversity must also expose incorrect OAMD metadata reuse",
    );
    metadata
}

fn fixture_frames() -> Vec<FixtureFrame> {
    let frames = index_syncframes(FIXTURE).unwrap();
    let units = group_access_units(&frames).unwrap();
    assert!(units.len() >= 6);
    let mut decoder = JocAccessUnitPcmDecoder::new();
    // Same deterministic dither sequence as OpenJocSession and the frozen
    // owned_frame_bitexact integration test; do not alter fixture bytes.
    let mut state = 0x6d2b_79f5_u32;
    let dither: Vec<f64> = (0..32_768)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (f64::from(state) / f64::from(u32::MAX) - 0.5) * 0.5
        })
        .collect();
    let result: Vec<_> = units
        .into_iter()
        .take(6)
        .map(|unit| FixtureFrame {
            metadata: extract_joc_access_unit(FIXTURE, &frames, unit)
                .unwrap()
                .unwrap(),
            pcm: decoder
                .decode_pcm_planes_with_policy(
                    FIXTURE,
                    &frames,
                    unit,
                    &dither,
                    InternalBasePolicy::CurrentDefault,
                )
                .unwrap(),
        })
        .collect();
    assert_ne!(result[0].metadata.oamd, result[1].metadata.oamd);
    result
}

fn configs() -> [OamdDecoderConfig; 3] {
    let default = OamdDecoderConfig::default();
    assert_eq!(default.trim_configuration_count, NonZeroU8::new(9));
    [
        default,
        OamdDecoderConfig {
            trim_configuration_count: NonZeroU8::new(1),
        },
        OamdDecoderConfig {
            trim_configuration_count: None,
        },
    ]
}

fn payload_id(profile: OamdParseProfile) -> Option<u64> {
    match profile {
        OamdParseProfile::EtsiStrict => None,
        OamdParseProfile::ObservedVendorCompat => Some(OAMD_PAYLOAD_ID),
    }
}

fn other_profile(profile: OamdParseProfile) -> OamdParseProfile {
    match profile {
        OamdParseProfile::EtsiStrict => OamdParseProfile::ObservedVendorCompat,
        OamdParseProfile::ObservedVendorCompat => OamdParseProfile::EtsiStrict,
    }
}

fn legacy_parse(raw: &[u8], config: OamdDecoderConfig, profile: OamdParseProfile) -> OamdPayload {
    match profile {
        OamdParseProfile::EtsiStrict => parse_oamd_payload_with_config(raw, config).unwrap(),
        OamdParseProfile::ObservedVendorCompat => {
            parse_oamd_payload_with_profile(raw, config, profile, OAMD_PAYLOAD_ID).unwrap()
        }
    }
}

fn proof(raw: &[u8], config: OamdDecoderConfig, profile: OamdParseProfile) -> ParsedOamdProof {
    match profile {
        OamdParseProfile::EtsiStrict => ParsedOamdProof::parse_with_config(raw, config).unwrap(),
        OamdParseProfile::ObservedVendorCompat => {
            ParsedOamdProof::parse_with_profile(raw, config, profile, OAMD_PAYLOAD_ID).unwrap()
        }
    }
}

fn decoder(config: OamdDecoderConfig, profile: OamdParseProfile) -> PayloadDecoder {
    PayloadDecoder::streaming_with_oamd_profile(
        PayloadDecoderConfig {
            reference_screen: None,
            oamd: config,
        },
        profile,
    )
}

fn assert_slice_bits(left: &[f64], right: &[f64]) -> bool {
    assert_eq!(left.len(), right.len());
    let mut nonzero = false;
    for (index, (left, right)) in left.iter().zip(right).enumerate() {
        assert_eq!(left.to_bits(), right.to_bits(), "sample {index}");
        nonzero |= left.to_bits() & 0x7fff_ffff_ffff_ffff != 0;
    }
    nonzero
}

fn assert_basis_bits(left: &ReconstructionBasis, right: &ReconstructionBasis) -> bool {
    assert_eq!(left.rows.len(), right.rows.len());
    let mut nonzero = false;
    for (left, right) in left.rows.iter().zip(&right.rows) {
        // Do not short-circuit once nonzero data has been seen: compare every sample.
        nonzero |= assert_slice_bits(left, right);
    }
    nonzero
}

// Frozen D2C baseline: one dynamic OAMD slot, no OAMD LFE slot, one JOC row.
// Ordinary semantic admission requires sixteen OAMD slots and fifteen JOC rows;
// the fixture carries structural reconstruction without an admitted binding.
fn assert_fixture_binding_contract(frame: &DecodedPayloadFrame) {
    let layout = &frame.programme_layout;
    assert_eq!(layout.total_oamd_count, 1);
    assert_eq!(layout.speaker_anchored_count, 0);
    assert_eq!(layout.bed_count, 0);
    assert_eq!(layout.lfe_count, 0);
    assert_eq!(layout.isf_count, 0);
    assert_eq!(layout.dynamic_slot_count, 1);
    assert_eq!(frame.joc.header.object_count, 1);
    assert_eq!(frame.decoded.reconstruction_basis.rows.len(), 1);
    let facts = DecodedJocBindingFacts::from_programme_layout_with_profile(
        ORDINARY,
        usize::from(frame.joc.header.object_count),
        frame.decoded.reconstruction_basis.rows.len(),
        layout,
    );
    assert_eq!(
        facts.oamd_total_classes,
        vec![OamdBindingObjectClass::Dynamic]
    );
    assert_eq!(
        admit_decoded_joc_binding(&facts),
        Err(DecodedJocBindingUnavailable::OamdTotalPopulationMismatch {
            expected: 16,
            actual: 1,
        }),
    );
    assert!(frame.admitted_decoded_joc_binding().is_none());
}

fn assert_frame_bits(left: &DecodedPayloadFrame, right: &DecodedPayloadFrame) -> bool {
    assert_fixture_binding_contract(left);
    assert_fixture_binding_contract(right);
    assert_eq!(left.frame_index, right.frame_index);
    assert_eq!(left.sample_rate, right.sample_rate);
    assert_eq!(left.sample_range, right.sample_range);
    assert_eq!(left.joc, right.joc);
    assert_eq!(left.oamd, right.oamd);
    assert_eq!(left.programme_layout, right.programme_layout);
    assert_eq!(
        left.admitted_decoded_joc_binding(),
        right.admitted_decoded_joc_binding()
    );
    assert_eq!(left, right, "all public retained result fields");
    let left_decoded = &left.decoded;
    let right_decoded = &right.decoded;
    assert_eq!(
        left_decoded.reconstruction_qmf.len(),
        right_decoded.reconstruction_qmf.len()
    );
    for (left_row, right_row) in left_decoded
        .reconstruction_qmf
        .iter()
        .zip(&right_decoded.reconstruction_qmf)
    {
        assert_eq!(left_row.len(), right_row.len());
        for (left_slot, right_slot) in left_row.iter().zip(right_row) {
            for (left, right) in left_slot.iter().zip(right_slot) {
                assert_eq!(left.re.to_bits(), right.re.to_bits());
                assert_eq!(left.im.to_bits(), right.im.to_bits());
            }
        }
    }
    assert_eq!(left_decoded.stages.len(), right_decoded.stages.len());
    for (left, right) in left_decoded.stages.iter().zip(&right_decoded.stages) {
        match (left, right) {
            (None, None) => {}
            (Some(left), Some(right)) => {
                assert_eq!(left.quantized, right.quantized);
                assert_eq!(left.dequantized.len(), right.dequantized.len());
                for (left_point, right_point) in left.dequantized.iter().zip(&right.dequantized) {
                    assert_eq!(left_point.len(), right_point.len());
                    for (left, right) in left_point.iter().zip(right_point) {
                        assert_slice_bits(left, right);
                    }
                }
                assert_eq!(left.interpolated.len(), right.interpolated.len());
                for (left_slot, right_slot) in left.interpolated.iter().zip(&right.interpolated) {
                    assert_eq!(left_slot.len(), right_slot.len());
                    for (left, right) in left_slot.iter().zip(right_slot) {
                        assert_slice_bits(left, right);
                    }
                }
            }
            _ => panic!("reconstruction stage presence differs"),
        }
    }
    assert_basis_bits(
        &left_decoded.reconstruction_basis,
        &right_decoded.reconstruction_basis,
    )
}

fn assert_finish_bits(left: PayloadDecoder, right: PayloadDecoder) {
    let (left_summary, left_tail) = left.finish_streaming_with_reconstruction_tail().unwrap();
    let (right_summary, right_tail) = right.finish_streaming_with_reconstruction_tail().unwrap();
    assert_eq!(left_summary, right_summary);
    assert_eq!(left_tail, right_tail);
    assert!(
        assert_basis_bits(&left_tail, &right_tail),
        "fixture must exercise a nonzero tail"
    );
}

#[derive(Clone, Copy, Debug)]
enum Offer {
    Exact,
    OtherAu,
    OtherProfile,
    OtherConfig(OamdDecoderConfig),
    ExplicitStrict,
    None,
}

fn offered_proof(
    offer: Offer,
    index: usize,
    frames: &[FixtureFrame],
    config: OamdDecoderConfig,
    profile: OamdParseProfile,
) -> Option<ParsedOamdProof> {
    let raw = &frames[index].metadata.oamd;
    match offer {
        Offer::Exact => {
            let separately_allocated = raw.clone();
            assert_ne!(raw.as_ptr(), separately_allocated.as_ptr());
            Some(proof(&separately_allocated, config, profile))
        }
        Offer::OtherAu => {
            let other = if *raw != frames[0].metadata.oamd {
                0
            } else {
                1
            };
            assert_ne!(*raw, frames[other].metadata.oamd);
            Some(proof(&frames[other].metadata.oamd, config, profile))
        }
        Offer::OtherProfile => Some(proof(raw, config, other_profile(profile))),
        Offer::OtherConfig(other) => {
            assert_ne!(config, other);
            Some(proof(raw, other, profile))
        }
        Offer::ExplicitStrict => Some(
            ParsedOamdProof::parse_with_profile(
                raw,
                config,
                OamdParseProfile::EtsiStrict,
                OAMD_PAYLOAD_ID,
            )
            .unwrap(),
        ),
        Offer::None => None,
    }
}

fn assert_stream_equivalent(
    frames: &[FixtureFrame],
    config: OamdDecoderConfig,
    profile: OamdParseProfile,
    offers: [Offer; 6],
    timing: bool,
) {
    let mut old = decoder(config, profile);
    let mut new = decoder(config, profile);
    if timing {
        new.enable_reconstruction_timing();
    }
    let mut nonzero = false;
    for (index, (frame, offer)) in frames.iter().zip(offers).enumerate() {
        // Check each claimed hit/miss directly, then construct a fresh, single-use
        // proof for scene. No Clone implementation on the proof is required.
        if let Some(offered) = offered_proof(offer, index, frames, config, profile) {
            let matched = offered.into_matching_payload(
                &frame.metadata.oamd,
                config,
                profile,
                payload_id(profile),
            );
            assert_eq!(
                matched.is_some(),
                matches!(offer, Offer::Exact),
                "{offer:?}"
            );
            if let Some(parsed) = matched {
                assert_eq!(parsed, legacy_parse(&frame.metadata.oamd, config, profile));
            }
        }
        let left = old
            .decode_frame_with_profile_and_binding_profile_to_frame(
                frame.input(index),
                profile,
                ORDINARY,
            )
            .unwrap();
        let right = new
            .decode_frame_with_profile_and_binding_profile_and_oamd_proof_to_frame(
                frame.input(index),
                profile,
                ORDINARY,
                offered_proof(offer, index, frames, config, profile),
            )
            .unwrap();
        assert_eq!(
            left.oamd,
            legacy_parse(&frame.metadata.oamd, config, profile)
        );
        nonzero |= assert_frame_bits(&left, &right);
        assert_eq!(
            old.take_reconstruction_timing(),
            ReconstructionStageTiming::default()
        );
        let timing_record = new.take_reconstruction_timing();
        if !timing {
            assert_eq!(timing_record, ReconstructionStageTiming::default());
        }
        // A timer is allowed to resolve to zero. Do not compare elapsed values
        // or attribute OAMD work to JOC's existing payload_parsing timer.
        assert_eq!(
            new.take_reconstruction_timing(),
            ReconstructionStageTiming::default()
        );
    }
    assert!(nonzero, "fixture must exercise nonzero reconstructed PCM");
    assert_finish_bits(old, new);
}

#[test]
fn exact_proofs_match_independent_parsers_for_separate_allocations() {
    let metadata = metadata_frames();
    let raw = &metadata[0].oamd;
    for config in configs() {
        for profile in PROFILES {
            let expected = legacy_parse(raw, config, profile);
            assert!(
                expected.elements.iter().all(|element| element.id != 2),
                "use the existing no-trim fixture"
            );
            let separate = raw.clone();
            assert_ne!(raw.as_ptr(), separate.as_ptr());
            let parsed = proof(&separate, config, profile);
            assert_eq!(parsed.parsed(), &expected);
            assert_eq!(
                parsed.into_matching_payload(raw, config, profile, payload_id(profile)),
                Some(expected)
            );
        }
    }
}

#[test]
fn matching_requires_exact_bytes_config_profile_and_entry_point_identifier() {
    let metadata = metadata_frames();
    let raw = &metadata[0].oamd;
    let config = OamdDecoderConfig::default();
    for profile in PROFILES {
        assert!(
            proof(raw, config, profile)
                .into_matching_payload(&metadata[1].oamd, config, profile, payload_id(profile))
                .is_none()
        );
        // Hold every other provenance component constant in each mismatch.
        assert!(
            proof(raw, config, profile)
                .into_matching_payload(raw, config, other_profile(profile), payload_id(profile))
                .is_none()
        );
        assert!(
            proof(raw, config, profile)
                .into_matching_payload(raw, config, profile, Some(OAMD_PAYLOAD_ID + 1))
                .is_none()
        );
        for source_config in configs() {
            for target_config in configs() {
                let expected = legacy_parse(raw, target_config, profile);
                let matched = proof(raw, source_config, profile).into_matching_payload(
                    raw,
                    target_config,
                    profile,
                    payload_id(profile),
                );
                assert_eq!(matched.is_some(), source_config == target_config);
                if let Some(parsed) = matched {
                    assert_eq!(parsed, expected);
                }
            }
        }
    }
    // Strict explicit-profile provenance remains Some(id), even though ordinary
    // fixture metadata equals the strict-config parser's result.
    let explicit = || {
        ParsedOamdProof::parse_with_profile(
            raw,
            config,
            OamdParseProfile::EtsiStrict,
            OAMD_PAYLOAD_ID,
        )
        .unwrap()
    };
    assert_eq!(
        explicit().parsed(),
        &legacy_parse(raw, config, OamdParseProfile::EtsiStrict)
    );
    assert!(
        explicit()
            .into_matching_payload(raw, config, OamdParseProfile::EtsiStrict, None)
            .is_none()
    );
    assert!(
        explicit()
            .into_matching_payload(
                raw,
                config,
                OamdParseProfile::EtsiStrict,
                Some(OAMD_PAYLOAD_ID + 1)
            )
            .is_none()
    );
    assert_eq!(
        explicit().into_matching_payload(
            raw,
            config,
            OamdParseProfile::EtsiStrict,
            Some(OAMD_PAYLOAD_ID)
        ),
        Some(legacy_parse(raw, config, OamdParseProfile::EtsiStrict))
    );
    assert!(
        proof(raw, config, OamdParseProfile::EtsiStrict)
            .into_matching_payload(
                raw,
                config,
                OamdParseProfile::EtsiStrict,
                Some(OAMD_PAYLOAD_ID)
            )
            .is_none()
    );
    assert!(
        proof(raw, config, OamdParseProfile::ObservedVendorCompat)
            .into_matching_payload(raw, config, OamdParseProfile::ObservedVendorCompat, None)
            .is_none()
    );
}

#[test]
fn all_legacy_owned_none_routes_remain_bit_exact() {
    let frames = fixture_frames();
    for config in configs() {
        for profile in PROFILES {
            assert_stream_equivalent(&frames, config, profile, [Offer::None; 6], false);
        }
    }
}

#[test]
fn sequential_hit_byte_miss_none_and_profile_miss_preserve_bits_and_tails() {
    let frames = fixture_frames();
    for profile in PROFILES {
        for timing in [false, true] {
            assert_stream_equivalent(
                &frames,
                OamdDecoderConfig::default(),
                profile,
                [
                    Offer::Exact,
                    Offer::OtherAu,
                    Offer::None,
                    Offer::Exact,
                    Offer::OtherProfile,
                    Offer::None,
                ],
                timing,
            );
        }
    }
}

#[test]
fn config_and_strict_entry_point_misses_keep_legacy_results() {
    let frames = fixture_frames();
    let [default, one, unspecified] = configs();
    for profile in PROFILES {
        for (target, first_other, second_other) in [
            (default, one, unspecified),
            (one, default, unspecified),
            (unspecified, default, one),
        ] {
            assert_stream_equivalent(
                &frames,
                target,
                profile,
                [
                    Offer::Exact,
                    Offer::OtherConfig(first_other),
                    Offer::None,
                    Offer::OtherConfig(second_other),
                    Offer::ExplicitStrict,
                    Offer::Exact,
                ],
                false,
            );
        }
    }
}

#[test]
fn successful_handoff_preserves_legacy_implicit_followup_results() {
    let frames = fixture_frames();
    let config = OamdDecoderConfig::default();
    for default_profile in PROFILES {
        let mut old = decoder(config, default_profile);
        let mut new = decoder(config, default_profile);
        let temporary_profile = other_profile(default_profile);
        let first = &frames[0];
        let left = old
            .decode_frame_with_profile_and_binding_profile_to_frame(
                first.input(0),
                temporary_profile,
                ORDINARY,
            )
            .unwrap();
        let right = new
            .decode_frame_with_profile_and_binding_profile_and_oamd_proof_to_frame(
                first.input(0),
                temporary_profile,
                ORDINARY,
                Some(proof(&first.metadata.oamd, config, temporary_profile)),
            )
            .unwrap();
        let mut nonzero = assert_frame_bits(&left, &right);
        for (index, frame) in frames.iter().enumerate().skip(1) {
            // This fixture is non-admitted under both syntax profiles. The
            // implicit call is a behavioral regression check, not a
            // discriminating witness of profile restoration. Source review
            // separately verifies the unchanged save/restore mechanism.
            let left = old.decode_frame(frame.input(index)).unwrap();
            let right = new.decode_frame(frame.input(index)).unwrap();
            nonzero |= assert_frame_bits(&left, &right);
        }
        assert!(nonzero);
        assert_finish_bits(old, new);
    }
}

fn assert_early_error(
    left: &PayloadDecodeError,
    right: &PayloadDecodeError,
    index: usize,
    rate: u32,
) {
    assert_eq!(left.to_string(), right.to_string());
    for error in [left, right] {
        match (index, error) {
            (0, PayloadDecodeError::UnexpectedFrameIndex { expected, actual }) => {
                assert_eq!((*expected, *actual), (0, 1));
            }
            (1, PayloadDecodeError::SampleRateChanged { expected, actual }) => {
                assert_eq!((*expected, *actual), (rate, rate + 1));
            }
            _ => panic!("unexpected pre-parse error: {error:?}"),
        }
    }
}

#[test]
fn early_errors_preserve_retry_progression_and_legacy_implicit_followup_results() {
    let frames = fixture_frames();
    let config = OamdDecoderConfig::default();
    let strict = OamdParseProfile::EtsiStrict;
    let temporary = OamdParseProfile::ObservedVendorCompat;
    let mut old = decoder(config, strict);
    let mut new = decoder(config, strict);
    let mut nonzero = false;
    for (index, frame) in frames.iter().enumerate() {
        let mut retry_probe = None;
        if index <= 1 {
            let mut bad = frame.input(index);
            if index == 0 {
                bad.frame_index = 1;
            } else {
                bad.sample_rate += 1;
            }
            let left = old
                .decode_frame_with_profile_and_binding_profile_to_frame(bad, temporary, ORDINARY)
                .unwrap_err();
            let right = new
                .decode_frame_with_profile_and_binding_profile_and_oamd_proof_to_frame(
                    bad,
                    temporary,
                    ORDINARY,
                    Some(proof(&frame.metadata.oamd, config, temporary)),
                )
                .unwrap_err();
            assert_early_error(&left, &right, index, frame.metadata.sample_rate);
            // Retry cloned states through the implicit legacy entry to compare
            // progression and complete output. Both syntax profiles leave this
            // fixture non-admitted, so this does not discriminate restoration;
            // source review verifies the unchanged save/restore mechanism.
            let left_probe = old.clone().decode_frame(frame.input(index)).unwrap();
            let right_probe = new.clone().decode_frame(frame.input(index)).unwrap();
            assert_frame_bits(&left_probe, &right_probe);
            retry_probe = Some(left_probe);
        }
        let left = old
            .decode_frame_with_profile_and_binding_profile_to_frame(
                frame.input(index),
                strict,
                ORDINARY,
            )
            .unwrap();
        let right = new
            .decode_frame_with_profile_and_binding_profile_and_oamd_proof_to_frame(
                frame.input(index),
                strict,
                ORDINARY,
                Some(proof(&frame.metadata.oamd, config, strict)),
            )
            .unwrap();
        assert_eq!(left.frame_index, u64::try_from(index).unwrap());
        nonzero |= assert_frame_bits(&left, &right);
        if let Some(probe) = retry_probe {
            assert_frame_bits(&probe, &right);
        }
    }
    assert!(nonzero);
    assert_finish_bits(old, new);
}
