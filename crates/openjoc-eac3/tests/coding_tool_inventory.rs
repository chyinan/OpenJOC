//! Diagnostic-only regressions. Existing encoded fixtures are never rewritten.

use openjoc_eac3::{
    AudioFrameInformation, CouplingInformation, DecodedAudioBlock, EnhancedCouplingInformation,
    SemanticChannel, decode_audio_blocks, emit_coding_tool_inventory, parse_audio_frame,
    rematrix_channels,
};

const COUPLED: [&[u8]; 2] = [
    include_bytes!("fixtures/coupling/acmod5.eac3"),
    include_bytes!("fixtures/coupling/acmod6.eac3"),
];
const UNCOUPLED: &[u8] = include_bytes!("fixtures/coupling/acmod5-nocpl.eac3");

fn decoded(bytes: &[u8]) -> (AudioFrameInformation, Vec<DecodedAudioBlock>) {
    (
        parse_audio_frame(bytes).expect("public fixture frame"),
        decode_audio_blocks(bytes, &vec![0.0; 20_000]).expect("public fixture blocks"),
    )
}

#[test]
fn inventory_reports_absolute_coupling_bins_for_every_block() {
    for bytes in COUPLED {
        let (frame, blocks) = decoded(bytes);
        let before = blocks.clone();
        let inventory =
            emit_coding_tool_inventory("coupled", 7, &frame, &blocks).expect("complete inventory");
        assert_eq!(inventory.blocks.len(), 24);
        for record in inventory.blocks {
            assert!(record.coupling_in_use);
            // Frozen fixture codes are cplbegf=11 and cplendf=12.
            // Absolute bins are 37 + 12*11 and 37 + 12*(12+3).
            assert_eq!(record.coupling_start_bin, Some(169));
            assert_eq!(record.coupling_end_bin, Some(217));
            assert_eq!(record.au_index, 7);
            assert_eq!(record.rematrix_flags, [] as [bool; 0]);
        }
        assert_eq!(blocks, before);
    }
}

#[test]
fn inventory_omits_coupling_bins_for_uncoupled_channels() {
    let (frame, blocks) = decoded(UNCOUPLED);
    let inventory = emit_coding_tool_inventory("uncoupled", 0, &frame, &blocks).expect("inventory");
    assert_eq!(inventory.blocks.len(), 24);
    for record in inventory.blocks {
        assert!(!record.coupling_in_use);
        assert_eq!(record.coupling_start_bin, None);
        assert_eq!(record.coupling_end_bin, None);
        assert_eq!(record.coupling_phase_flags, [] as [bool; 0]);
        assert_eq!(record.rematrix_flags, [] as [bool; 0]);
    }
}

#[test]
fn inventory_uses_parsed_enhanced_bins_only_for_participating_channels() {
    // Public diagnostic state, not a new encoded enhanced-coupling fixture.
    let (frame, mut blocks) = decoded(COUPLED[0]);
    for block in &mut blocks {
        block.prefix.coupling = Some(CouplingInformation::Enhanced(EnhancedCouplingInformation {
            channel_in_use: vec![true, false, false, false],
            begin_frequency_code: 12,
            begin_subband: 14,
            end_subband: 19,
            band_structure: [false; 22],
            band_count: 5,
            amplitudes: vec![Some(vec![0; 5]), None, None, None],
        }));
        let exponents = block.prefix.coupling_exponents.as_mut().expect("exponents");
        exponents.start_mantissa = 157;
        exponents.end_mantissa = 217;
        exponents.decoded.resize(60, 0);
    }
    let inventory = emit_coding_tool_inventory("enhanced", 0, &frame, &blocks).expect("inventory");
    assert_eq!(inventory.blocks.len(), 24);
    for records in inventory.blocks.chunks_exact(4) {
        assert!(records[0].coupling_in_use);
        assert_eq!(records[0].coupling_start_bin, Some(157));
        assert_eq!(records[0].coupling_end_bin, Some(217));
        assert_eq!(records[0].coupling_phase_flags, [] as [bool; 0]);
        for record in &records[1..] {
            assert!(!record.coupling_in_use);
            assert_eq!(record.coupling_start_bin, None);
            assert_eq!(record.coupling_end_bin, None);
            assert_eq!(record.coupling_phase_flags, [] as [bool; 0]);
        }
    }
}

// Build public diagnostic inputs from decoded uncoupled state. This helper is
// not an encoder or a claim of a new encoded stereo conformance fixture.
fn stereo_diagnostic_state() -> (AudioFrameInformation, Vec<DecodedAudioBlock>) {
    let (mut frame, mut blocks) = decoded(UNCOUPLED);
    frame.bsi.audio_coding_mode = 2;
    frame.full_bandwidth_channels = 2;
    frame.channel_aht_in_use.truncate(2);
    frame.spx_attenuation_codes.truncate(2);
    for strategies in &mut frame.channel_exponent_strategy {
        strategies.truncate(2);
    }
    for block in &mut blocks {
        block.channel_baps.truncate(2);
        block.channel_mantissas.truncate(2);
        block.channel_aht.truncate(2);
        block.prefix.block_switch.truncate(2);
        block.prefix.dither.truncate(2);
        block.prefix.channel_bandwidth_codes.truncate(2);
        block.prefix.channel_exponents.truncate(2);
    }
    (frame, blocks)
}

#[test]
fn inventory_preserves_all_uncoupled_stereo_rematrix_flag_combinations() {
    let (frame, mut blocks) = stereo_diagnostic_state();
    for mask in 0_u8..16 {
        let flags = (0..4).map(|bit| mask & (1 << bit) != 0).collect::<Vec<_>>();
        for block in &mut blocks {
            block.prefix.rematrix_flags.clone_from(&flags);
        }
        let before = blocks.clone();
        let inventory =
            emit_coding_tool_inventory("stereo", 3, &frame, &blocks).expect("stereo inventory");
        assert_eq!(inventory.blocks.len(), 12);
        for pair in inventory.blocks.chunks_exact(2) {
            assert_eq!(pair[0].channel, SemanticChannel::Left);
            assert_eq!(pair[1].channel, SemanticChannel::Right);
            for record in pair {
                assert!(!record.coupling_in_use);
                assert_eq!(record.rematrix_flags, flags);
            }
        }
        assert_eq!(blocks, before);
        // The production helper accepts these flags without coupling too.
        let restored = rematrix_channels(&[vec![1.0; 80], vec![2.0; 80]], &flags, None, None)
            .expect("uncoupled rematrix");
        let expected = if flags[0] { (3.0, -1.0) } else { (1.0, 2.0) };
        assert_eq!((restored[0][13], restored[1][13]), expected);
    }
}
