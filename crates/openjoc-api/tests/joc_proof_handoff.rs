#![cfg(not(target_arch = "wasm32"))]

use openjoc_eac3::{
    DecodedJocAccessUnitPcm, InternalBasePolicy, JocAccessUnitPcmDecoder, JocMetadataFrame,
    extract_joc_access_unit, group_access_units, index_syncframes,
};
use openjoc_joc::{
    ParsedJocProof, ReconstructionBasis, ReconstructionStageTiming, parse_joc_payload,
};
use openjoc_oamd::{
    OAMD_PAYLOAD_ID, OamdDecoderConfig, OamdParseProfile, OamdPayload, ParsedOamdProof,
    parse_oamd_payload_with_config, parse_oamd_payload_with_profile,
};
use openjoc_scene::{
    BindingCodecProfile, DecodedJocBindingFacts, DecodedJocBindingUnavailable, DecodedPayloadFrame,
    JocFrameInput, OamdBindingObjectClass, PayloadDecodeError, PayloadDecoder,
    PayloadDecoderConfig, admit_decoded_joc_binding,
};

// Intended destination: crates/openjoc-api/tests/joc_proof_handoff.rs.
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

fn assert_fixture_metadata(metadata: &[&JocMetadataFrame]) {
    assert_eq!(metadata.len(), 6);
    let first_joc = parse_joc_payload(&metadata[0].joc).unwrap();
    let second_joc = parse_joc_payload(&metadata[1].joc).unwrap();
    assert_ne!(metadata[0].joc, metadata[1].joc);
    assert_ne!(first_joc.sequence_count, second_joc.sequence_count);
    // Intact AU sequence differences must expose a wrongly reused JOC result,
    // independently of whether that sequence difference changes the PCM.
    assert_ne!(first_joc, second_joc);
    assert_ne!(metadata[0].oamd, metadata[1].oamd);
    for profile in PROFILES {
        assert_ne!(
            legacy_parse(&metadata[0].oamd, OamdDecoderConfig::default(), profile),
            legacy_parse(&metadata[1].oamd, OamdDecoderConfig::default(), profile),
        );
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
    assert_fixture_metadata(&metadata.iter().collect::<Vec<_>>());
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
    assert_fixture_metadata(
        &result
            .iter()
            .map(|frame| &frame.metadata)
            .collect::<Vec<_>>(),
    );
    result
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
    None,
}

const OFFERS: [Offer; 3] = [Offer::Exact, Offer::OtherAu, Offer::None];
const ALTERNATING: [Offer; 6] = [
    Offer::Exact,
    Offer::OtherAu,
    Offer::None,
    Offer::Exact,
    Offer::OtherAu,
    Offer::None,
];

fn offered_joc_proof(
    offer: Offer,
    index: usize,
    frames: &[FixtureFrame],
) -> Option<ParsedJocProof> {
    let raw = &frames[index].metadata.joc;
    match offer {
        Offer::Exact => {
            let separate = raw.clone();
            assert_ne!(raw.as_ptr(), separate.as_ptr());
            // The temporary allocation is dropped before scene consumes the proof.
            Some(ParsedJocProof::parse(&separate).unwrap())
        }
        Offer::OtherAu => {
            let expected = parse_joc_payload(raw).unwrap();
            let other = frames
                .iter()
                .find(|frame| {
                    parse_joc_payload(&frame.metadata.joc)
                        .unwrap()
                        .sequence_count
                        != expected.sequence_count
                })
                .expect("fixture must provide an intact AU with a different JOC sequence");
            assert_ne!(*raw, other.metadata.joc);
            let proof = ParsedJocProof::parse(&other.metadata.joc).unwrap();
            assert_ne!(proof.parsed().sequence_count, expected.sequence_count);
            Some(proof)
        }
        Offer::None => None,
    }
}

fn offered_oamd_proof(
    offer: Offer,
    index: usize,
    frames: &[FixtureFrame],
    profile: OamdParseProfile,
) -> Option<ParsedOamdProof> {
    let raw = &frames[index].metadata.oamd;
    let config = OamdDecoderConfig::default();
    match offer {
        Offer::Exact => {
            let separate = raw.clone();
            assert_ne!(raw.as_ptr(), separate.as_ptr());
            Some(proof(&separate, config, profile))
        }
        Offer::OtherAu => {
            let expected = legacy_parse(raw, config, profile);
            let other = frames
                .iter()
                .find(|frame| legacy_parse(&frame.metadata.oamd, config, profile) != expected)
                .expect("fixture must provide an intact AU with different parsed OAMD");
            assert_ne!(*raw, other.metadata.oamd);
            Some(proof(&other.metadata.oamd, config, profile))
        }
        Offer::None => None,
    }
}

fn assert_offers(
    joc_offer: Offer,
    oamd_offer: Offer,
    index: usize,
    frames: &[FixtureFrame],
    profile: OamdParseProfile,
) {
    let metadata = &frames[index].metadata;
    // Check each claimed hit/miss directly. Supply a fresh single-use proof to
    // scene afterward; neither type needs Clone or a caller-created parse value.
    if let Some(proof) = offered_joc_proof(joc_offer, index, frames) {
        let matched = proof.into_matching_frame(&metadata.joc);
        assert_eq!(matched.is_some(), matches!(joc_offer, Offer::Exact));
        if let Some(parsed) = matched {
            assert_eq!(parsed, parse_joc_payload(&metadata.joc).unwrap());
        }
    }
    if let Some(proof) = offered_oamd_proof(oamd_offer, index, frames, profile) {
        let matched = proof.into_matching_payload(
            &metadata.oamd,
            OamdDecoderConfig::default(),
            profile,
            payload_id(profile),
        );
        assert_eq!(matched.is_some(), matches!(oamd_offer, Offer::Exact));
        if let Some(parsed) = matched {
            assert_eq!(
                parsed,
                legacy_parse(&metadata.oamd, OamdDecoderConfig::default(), profile)
            );
        }
    }
}

fn assert_take_timing(decoder: &mut PayloadDecoder, enabled: bool) {
    let record = decoder.take_reconstruction_timing();
    if !enabled {
        assert_eq!(record, ReconstructionStageTiming::default());
    }
    // Actual wall time may resolve to zero. Never compare elapsed values from
    // separate runs, demand positive durations, or synthesize saved parse time.
    assert_eq!(
        decoder.take_reconstruction_timing(),
        ReconstructionStageTiming::default()
    );
}

fn assert_stream_equivalent(
    frames: &[FixtureFrame],
    profile: OamdParseProfile,
    joc_offers: [Offer; 6],
    oamd_offers: [Offer; 6],
    timing: bool,
) {
    let config = OamdDecoderConfig::default();
    let mut old = decoder(config, profile);
    let mut new = decoder(config, profile);
    if timing {
        new.enable_reconstruction_timing();
    }
    let mut nonzero = false;
    for (index, ((frame, joc_offer), oamd_offer)) in
        frames.iter().zip(joc_offers).zip(oamd_offers).enumerate()
    {
        assert_offers(joc_offer, oamd_offer, index, frames, profile);
        let left = old
            .decode_frame_with_profile_and_binding_profile_to_frame(
                frame.input(index),
                profile,
                ORDINARY,
            )
            .unwrap();
        let right = new
            .decode_frame_with_parsed_payloads_to_frame(
                frame.input(index),
                profile,
                ORDINARY,
                offered_joc_proof(joc_offer, index, frames),
                offered_oamd_proof(oamd_offer, index, frames, profile),
            )
            .unwrap();
        assert_eq!(right.joc, parse_joc_payload(&frame.metadata.joc).unwrap());
        assert_eq!(
            right.oamd,
            legacy_parse(&frame.metadata.oamd, config, profile)
        );
        nonzero |= assert_frame_bits(&left, &right);
        assert_take_timing(&mut old, false);
        assert_take_timing(&mut new, timing);
    }
    assert!(nonzero, "fixture must exercise nonzero reconstructed PCM");
    assert_finish_bits(old, new);
}

#[test]
fn exact_joc_proofs_match_legacy_parser_and_own_separate_allocations() {
    for metadata in metadata_frames() {
        let expected = parse_joc_payload(&metadata.joc).unwrap();
        let proof = {
            let separate = metadata.joc.clone();
            assert_ne!(metadata.joc.as_ptr(), separate.as_ptr());
            ParsedJocProof::parse(&separate).unwrap()
        };
        assert_eq!(proof.parsed(), &expected);
        assert_eq!(proof.into_matching_frame(&metadata.joc), Some(expected));
    }
}

#[test]
fn intact_other_au_joc_proofs_miss_with_distinct_parsed_sequence() {
    let metadata = metadata_frames();
    for current in &metadata {
        let expected = parse_joc_payload(&current.joc).unwrap();
        let other = metadata
            .iter()
            .find(|other| {
                parse_joc_payload(&other.joc).unwrap().sequence_count != expected.sequence_count
            })
            .unwrap();
        let proof = ParsedJocProof::parse(&other.joc).unwrap();
        assert_ne!(other.joc, current.joc);
        assert_ne!(proof.parsed(), &expected);
        assert_ne!(proof.parsed().sequence_count, expected.sequence_count);
        assert!(proof.into_matching_frame(&current.joc).is_none());
    }
}

#[test]
fn legacy_owned_none_routes_remain_bit_exact_with_timing_off_and_on() {
    let frames = fixture_frames();
    for profile in PROFILES {
        for timing in [false, true] {
            assert_stream_equivalent(&frames, profile, [Offer::None; 6], [Offer::None; 6], timing);
        }
    }
}

#[test]
fn independent_joc_and_oamd_hits_misses_and_absence_preserve_stream_bits() {
    let frames = fixture_frames();
    for profile in PROFILES {
        for timing in [false, true] {
            // Each OAMD mode independently meets all three JOC modes twice,
            // giving all nine combinations within ordinary sequential streams.
            for oamd_offer in OFFERS {
                assert_stream_equivalent(&frames, profile, ALTERNATING, [oamd_offer; 6], timing);
            }
        }
    }
}

#[test]
fn legacy_oamd_only_entry_matches_none_joc_handoff_and_implicit_followups() {
    let frames = fixture_frames();
    let config = OamdDecoderConfig::default();
    for default_profile in PROFILES {
        for timing in [false, true] {
            let mut old = decoder(config, default_profile);
            let mut new = decoder(config, default_profile);
            if timing {
                new.enable_reconstruction_timing();
            }
            let temporary_profile = other_profile(default_profile);
            let mut nonzero = false;
            for (index, (frame, offer)) in frames.iter().zip(ALTERNATING).enumerate() {
                let (left, right) =
                    if index < 3 {
                        assert_offers(Offer::None, offer, index, &frames, temporary_profile);
                        (
                        old.decode_frame_with_profile_and_binding_profile_and_oamd_proof_to_frame(
                            frame.input(index), temporary_profile, ORDINARY,
                            offered_oamd_proof(offer, index, &frames, temporary_profile),
                        ).unwrap(),
                        new.decode_frame_with_parsed_payloads_to_frame(
                            frame.input(index), temporary_profile, ORDINARY, None,
                            offered_oamd_proof(offer, index, &frames, temporary_profile),
                        ).unwrap(),
                    )
                    } else {
                        // This one-slot fixture is non-admitted under both profiles.
                        // Implicit calls check legacy output/progression; source
                        // review must verify the save/restore mechanism itself.
                        (
                            old.decode_frame(frame.input(index)).unwrap(),
                            new.decode_frame(frame.input(index)).unwrap(),
                        )
                    };
                nonzero |= assert_frame_bits(&left, &right);
                assert_take_timing(&mut old, false);
                assert_take_timing(&mut new, timing);
            }
            assert!(nonzero);
            assert_finish_bits(old, new);
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum EarlyError {
    Index { expected: u64, actual: u64 },
    Rate { expected: u32, actual: u32 },
}

fn assert_early_error(left: &PayloadDecodeError, right: &PayloadDecodeError, expected: EarlyError) {
    assert_eq!(left.to_string(), right.to_string());
    for error in [left, right] {
        match (expected, error) {
            (
                EarlyError::Index { expected, actual },
                PayloadDecodeError::UnexpectedFrameIndex {
                    expected: got_expected,
                    actual: got_actual,
                },
            ) => assert_eq!((*got_expected, *got_actual), (expected, actual)),
            (
                EarlyError::Rate { expected, actual },
                PayloadDecodeError::SampleRateChanged {
                    expected: got_expected,
                    actual: got_actual,
                },
            ) => assert_eq!((*got_expected, *got_actual), (expected, actual)),
            _ => panic!("unexpected pre-parse error: {error:?}; expected {expected:?}"),
        }
    }
}

#[test]
fn preparse_frame_and_rate_errors_preserve_retry_state_bits_and_timing() {
    let frames = fixture_frames();
    let config = OamdDecoderConfig::default();
    let strict = OamdParseProfile::EtsiStrict;
    let temporary = OamdParseProfile::ObservedVendorCompat;
    for (joc_offer, oamd_offer) in [
        (Offer::Exact, Offer::OtherAu),
        (Offer::OtherAu, Offer::Exact),
        (Offer::None, Offer::None),
    ] {
        for timing in [false, true] {
            let mut old = decoder(config, strict);
            let mut new = decoder(config, strict);
            if timing {
                new.enable_reconstruction_timing();
            }
            let mut nonzero = false;
            for (index, frame) in frames.iter().enumerate() {
                let mut bad_inputs = Vec::new();
                let mut retry_probe = None;
                if index <= 1 {
                    let mut bad = frame.input(index);
                    bad.frame_index += 1;
                    if index == 1 {
                        // With both wrong, frame index must still win over rate.
                        bad.sample_rate += 1;
                    }
                    bad_inputs.push((
                        bad,
                        EarlyError::Index {
                            expected: u64::try_from(index).unwrap(),
                            actual: bad.frame_index,
                        },
                    ));
                }
                if index == 1 {
                    let mut bad = frame.input(index);
                    bad.sample_rate += 1;
                    bad_inputs.push((
                        bad,
                        EarlyError::Rate {
                            expected: frame.metadata.sample_rate,
                            actual: bad.sample_rate,
                        },
                    ));
                }
                for (bad, expected_error) in bad_inputs {
                    assert_offers(joc_offer, oamd_offer, index, &frames, temporary);
                    let left = old
                        .decode_frame_with_profile_and_binding_profile_to_frame(
                            bad, temporary, ORDINARY,
                        )
                        .unwrap_err();
                    let right = new
                        .decode_frame_with_parsed_payloads_to_frame(
                            bad,
                            temporary,
                            ORDINARY,
                            offered_joc_proof(joc_offer, index, &frames),
                            offered_oamd_proof(oamd_offer, index, &frames, temporary),
                        )
                        .unwrap_err();
                    assert_early_error(&left, &right, expected_error);
                    // No successful work since the last take, so early errors
                    // must leave the timing record empty even when enabled.
                    assert_take_timing(&mut old, false);
                    assert_take_timing(&mut new, false);
                    let left_probe = old.clone().decode_frame(frame.input(index)).unwrap();
                    let right_probe = new.clone().decode_frame(frame.input(index)).unwrap();
                    assert_frame_bits(&left_probe, &right_probe);
                    if let Some(previous) = retry_probe.as_ref() {
                        assert_frame_bits(previous, &left_probe);
                    }
                    retry_probe = Some(left_probe);
                }
                // Retry each unchanged complete AU with newly created proofs.
                // Success must commit precisely once, preserving later history.
                let left = old
                    .decode_frame_with_profile_and_binding_profile_to_frame(
                        frame.input(index),
                        strict,
                        ORDINARY,
                    )
                    .unwrap();
                let right = new
                    .decode_frame_with_parsed_payloads_to_frame(
                        frame.input(index),
                        strict,
                        ORDINARY,
                        offered_joc_proof(joc_offer, index, &frames),
                        offered_oamd_proof(oamd_offer, index, &frames, strict),
                    )
                    .unwrap();
                assert_eq!(right.frame_index, u64::try_from(index).unwrap());
                assert_eq!(right.joc, parse_joc_payload(&frame.metadata.joc).unwrap());
                nonzero |= assert_frame_bits(&left, &right);
                if let Some(probe) = retry_probe {
                    assert_frame_bits(&probe, &right);
                }
                assert_take_timing(&mut old, false);
                assert_take_timing(&mut new, timing);
            }
            assert!(nonzero);
            assert_finish_bits(old, new);
        }
    }
}
