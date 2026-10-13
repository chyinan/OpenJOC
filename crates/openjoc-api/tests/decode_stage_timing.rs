#![cfg(not(target_arch = "wasm32"))]

#[cfg(feature = "embedded-builtin-hrtf")]
use openjoc_api::{BinauralConfig, BuiltinHrtf};
use openjoc_api::{
    OpenJocConfig, OpenJocDecodeStageTiming, OpenJocDiagnostics, OpenJocError, OpenJocOutputInfo,
    OpenJocPacket, OpenJocPcmFrame, OpenJocSession, OpenJocStatus, RenderMode,
};
use openjoc_eac3::{group_access_units, index_syncframes};

const FIXTURE: &[u8] = include_bytes!("../../openjoc-wasm/testdata/joc.lifecycle.ec3");

#[derive(Clone, Copy)]
struct FixtureAu {
    bytes: &'static [u8],
    input_start_sample: u64,
    sample_count: u16,
    sample_rate: u32,
    syncframes: u64,
    audio_blocks: u64,
}

fn first_six_complete_access_units() -> Vec<FixtureAu> {
    let frames = index_syncframes(FIXTURE).unwrap();
    let units = group_access_units(&frames).unwrap();
    assert!(units.len() >= 6);
    let mut input_start_sample = 0;
    units
        .into_iter()
        .take(6)
        .map(|unit| {
            let unit_frames = &frames[unit.first_frame..unit.first_frame + unit.frame_count];
            let first = unit_frames.first().unwrap();
            let last = unit_frames.last().unwrap();
            // Packet boundaries come from complete indexed AUs, never a fixed
            // byte count or an arbitrary prefix of the elementary stream.
            let end = last.offset.checked_add(last.header.frame_size).unwrap();
            let result = FixtureAu {
                bytes: &FIXTURE[first.offset..end],
                input_start_sample,
                sample_count: unit.samples,
                sample_rate: unit.sample_rate,
                syncframes: u64::try_from(unit.frame_count).unwrap(),
                audio_blocks: unit_frames
                    .iter()
                    .map(|frame| u64::from(frame.header.audio_blocks))
                    .sum(),
            };
            input_start_sample += u64::from(unit.samples);
            result
        })
        .collect()
}

fn packet(au: FixtureAu, pts_samples: Option<i64>, discontinuity: bool) -> OpenJocPacket<'static> {
    OpenJocPacket {
        data: au.bytes,
        pts_samples,
        discontinuity,
        preroll: false,
    }
}

fn push(session: &mut OpenJocSession, au: FixtureAu) -> OpenJocStatus {
    session.push_packet(packet(au, None, false)).unwrap()
}

fn receive_all(session: &mut OpenJocSession) -> Vec<OpenJocPcmFrame> {
    std::iter::from_fn(|| session.receive_frame()).collect()
}

fn assert_record(record: OpenJocDecodeStageTiming, au: FixtureAu, input_start_sample: u64) {
    assert_eq!(record.input_start_sample, input_start_sample);
    assert_eq!(record.input_sample_count, au.sample_count);
    assert_eq!(record.sample_rate, au.sample_rate);
    assert_eq!(record.eac3_stages.syncframes, au.syncframes);
    assert_eq!(record.eac3_stages.audio_blocks, au.audio_blocks);
    assert!(record.eac3_stages.full_bandwidth_channel_blocks > 0);
    // Durations are native wall-clock observations. No measured duration is
    // compared with another observation, a threshold, or a nonzero minimum.
    let _: std::time::Duration = record.eac3_decode;
    let _: std::time::Duration = record.joc_reconstruction;
    let _: openjoc_eac3::Eac3DecodeStageTiming = record.eac3_stages;
    let _: openjoc_joc::ReconstructionStageTiming = record.reconstruction_stages;
}

fn take_record(session: &mut OpenJocSession, au: FixtureAu, input_start_sample: u64) {
    assert_record(
        session
            .take_decode_stage_timing()
            .expect("successful AU timing"),
        au,
        input_start_sample,
    );
    assert!(session.take_decode_stage_timing().is_none());
}

#[test]
fn detailed_timing_is_disabled_by_default_and_by_aggregate_timing_alone() {
    fn assert_public_traits<T: Clone + std::fmt::Debug + Default + PartialEq>() {}
    assert_public_traits::<OpenJocDecodeStageTiming>();

    let units = first_six_complete_access_units();
    let mut session = OpenJocSession::new(OpenJocConfig::default()).unwrap();
    assert!(session.take_decode_stage_timing().is_none());
    assert_eq!(push(&mut session, units[0]), OpenJocStatus::NeedMoreInput);
    assert!(session.take_decode_stage_timing().is_none());

    session.enable_stage_timing();
    assert!(session.take_decode_stage_timing().is_none());
    assert_eq!(push(&mut session, units[1]), OpenJocStatus::FrameAvailable);
    assert!(session.take_decode_stage_timing().is_none());
    let _ = session.take_stage_timing();
    assert!(!receive_all(&mut session).is_empty());
    assert!(session.take_decode_stage_timing().is_none());
    session.drain().unwrap();
    assert!(!receive_all(&mut session).is_empty());
    assert!(session.take_decode_stage_timing().is_none());
}

#[test]
fn detailed_enable_clears_its_slot_and_aggregate_operations_leave_it_alone() {
    let units = first_six_complete_access_units();
    let mut session = OpenJocSession::new(OpenJocConfig::default()).unwrap();
    session.enable_decode_stage_timing();
    session.enable_decode_stage_timing();
    assert!(session.take_decode_stage_timing().is_none());
    session.enable_stage_timing();

    assert_eq!(push(&mut session, units[0]), OpenJocStatus::NeedMoreInput);
    let _ = session.take_stage_timing();
    let _ = session.take_stage_timing();
    take_record(&mut session, units[0], units[0].input_start_sample);

    assert_eq!(push(&mut session, units[1]), OpenJocStatus::FrameAvailable);
    session.enable_stage_timing();
    take_record(&mut session, units[1], units[1].input_start_sample);
    assert!(!receive_all(&mut session).is_empty());

    assert_eq!(push(&mut session, units[2]), OpenJocStatus::FrameAvailable);
    session.enable_decode_stage_timing();
    assert!(session.take_decode_stage_timing().is_none());
    let _ = session.take_stage_timing();
    assert!(session.take_decode_stage_timing().is_none());
    assert!(!receive_all(&mut session).is_empty());
    assert_eq!(push(&mut session, units[3]), OpenJocStatus::FrameAvailable);
    take_record(&mut session, units[3], units[3].input_start_sample);
}

#[test]
fn take_is_destructive_latest_success_replaces_and_backpressure_preserves() {
    let units = first_six_complete_access_units();
    let mut session = OpenJocSession::new(OpenJocConfig::default()).unwrap();
    session.enable_decode_stage_timing();

    assert_eq!(push(&mut session, units[0]), OpenJocStatus::NeedMoreInput);
    take_record(&mut session, units[0], units[0].input_start_sample);
    assert_eq!(push(&mut session, units[1]), OpenJocStatus::FrameAvailable);
    assert_eq!(push(&mut session, units[2]), OpenJocStatus::OutputPending);
    assert_eq!(session.drain().unwrap(), OpenJocStatus::OutputPending);
    take_record(&mut session, units[1], units[1].input_start_sample);
    assert_eq!(push(&mut session, units[2]), OpenJocStatus::OutputPending);
    assert!(session.take_decode_stage_timing().is_none());

    assert!(!receive_all(&mut session).is_empty());
    assert_eq!(push(&mut session, units[2]), OpenJocStatus::FrameAvailable);
    assert!(!receive_all(&mut session).is_empty());
    // Leave AU 2's record untaken. A later successful push replaces it.
    assert_eq!(push(&mut session, units[3]), OpenJocStatus::FrameAvailable);
    take_record(&mut session, units[3], units[3].input_start_sample);
}

#[test]
fn legal_access_unit_with_wrong_timestamp_preserves_the_last_success() {
    let units = first_six_complete_access_units();
    let mut session = OpenJocSession::new(OpenJocConfig::default()).unwrap();
    session.enable_decode_stage_timing();
    let origin = 12_345;
    assert_eq!(
        session
            .push_packet(packet(units[0], Some(origin), false))
            .unwrap(),
        OpenJocStatus::NeedMoreInput,
    );
    let expected = origin + i64::from(units[0].sample_count);
    let info = session.output_info();
    // Only packet metadata is wrong. The second AU remains the original,
    // complete indexed codec data and is retried unchanged below.
    assert_eq!(
        session.push_packet(packet(units[1], Some(origin), false)),
        Err(OpenJocError::TimestampDiscontinuity {
            expected,
            actual: origin,
        }),
    );
    assert_eq!(session.output_info(), info);
    take_record(&mut session, units[0], units[0].input_start_sample);
    assert_eq!(
        session.push_packet(packet(units[1], Some(origin), false)),
        Err(OpenJocError::TimestampDiscontinuity {
            expected,
            actual: origin,
        }),
    );
    assert!(session.take_decode_stage_timing().is_none());
    assert_eq!(
        session
            .push_packet(packet(units[1], Some(expected), false))
            .unwrap(),
        OpenJocStatus::FrameAvailable,
    );
    take_record(&mut session, units[1], units[1].input_start_sample);
}

#[test]
fn reset_flush_and_discontinuity_start_a_new_enabled_timing_timeline() {
    let units = first_six_complete_access_units();
    let resets: [fn(&mut OpenJocSession); 4] = [
        OpenJocSession::reset,
        OpenJocSession::flush,
        |session| session.try_reset().unwrap(),
        |session| session.try_flush().unwrap(),
    ];
    for reset in resets {
        let mut session = OpenJocSession::new(OpenJocConfig::default()).unwrap();
        session.enable_decode_stage_timing();
        assert_eq!(push(&mut session, units[0]), OpenJocStatus::NeedMoreInput);
        assert_eq!(push(&mut session, units[1]), OpenJocStatus::FrameAvailable);
        reset(&mut session);
        assert!(session.take_decode_stage_timing().is_none());
        assert!(session.receive_frame().is_none());
        // No second enable call: both the session slot and the detailed core
        // decoder must still be enabled after rebuilding stream state.
        assert_eq!(push(&mut session, units[0]), OpenJocStatus::NeedMoreInput);
        take_record(&mut session, units[0], 0);
    }

    let mut session = OpenJocSession::new(OpenJocConfig::default()).unwrap();
    session.enable_decode_stage_timing();
    assert_eq!(push(&mut session, units[0]), OpenJocStatus::NeedMoreInput);
    assert_eq!(push(&mut session, units[1]), OpenJocStatus::FrameAvailable);
    assert!(!receive_all(&mut session).is_empty());
    assert_eq!(
        session.push_packet(packet(units[0], None, true)).unwrap(),
        OpenJocStatus::NeedMoreInput,
    );
    take_record(&mut session, units[0], 0);
    assert_eq!(push(&mut session, units[1]), OpenJocStatus::FrameAvailable);
    take_record(&mut session, units[1], u64::from(units[0].sample_count));
}

fn rendering_configs() -> Vec<OpenJocConfig> {
    let configs = vec![
        OpenJocConfig::default(),
        OpenJocConfig {
            render_mode: RenderMode::Stereo,
            ..OpenJocConfig::default()
        },
    ];
    #[cfg(feature = "embedded-builtin-hrtf")]
    let configs = {
        let mut configs = configs;
        for builtin in [BuiltinHrtf::SadieD1Ku100, BuiltinHrtf::SadieD2Kemar] {
            configs.push(OpenJocConfig {
                render_mode: RenderMode::Binaural,
                speaker_layout: "7.1.4".to_owned(),
                binaural: Some(BinauralConfig::builtin(builtin, "7.1.4")),
                ..OpenJocConfig::default()
            });
        }
        configs
    };
    configs
}

#[test]
fn receive_and_drain_preserve_the_latest_au_and_never_create_a_record() {
    let units = first_six_complete_access_units();
    for config in rendering_configs() {
        let mut session = OpenJocSession::new(config).unwrap();
        session.enable_decode_stage_timing();
        assert_eq!(push(&mut session, units[0]), OpenJocStatus::NeedMoreInput);
        assert!(session.receive_frame().is_none());
        take_record(&mut session, units[0], units[0].input_start_sample);

        assert_eq!(push(&mut session, units[1]), OpenJocStatus::FrameAvailable);
        assert!(!receive_all(&mut session).is_empty());
        take_record(&mut session, units[1], units[1].input_start_sample);
        assert_eq!(push(&mut session, units[2]), OpenJocStatus::FrameAvailable);
        assert!(!receive_all(&mut session).is_empty());
        assert_eq!(session.drain().unwrap(), OpenJocStatus::FrameAvailable);
        assert!(!receive_all(&mut session).is_empty());
        assert!(session.is_drained());
        assert_eq!(session.drain().unwrap(), OpenJocStatus::EndOfStream);
        assert_eq!(
            session.push_packet(packet(units[3], None, false)),
            Err(OpenJocError::AlreadyDrained),
        );
        take_record(&mut session, units[2], units[2].input_start_sample);
        assert!(session.receive_frame().is_none());
        assert_eq!(session.drain().unwrap(), OpenJocStatus::EndOfStream);
        assert!(session.take_decode_stage_timing().is_none());
    }
    let mut empty = OpenJocSession::new(OpenJocConfig::default()).unwrap();
    empty.enable_decode_stage_timing();
    assert_eq!(empty.drain().unwrap(), OpenJocStatus::EndOfStream);
    assert!(empty.receive_frame().is_none());
    assert!(empty.take_decode_stage_timing().is_none());
}

#[derive(Debug, PartialEq)]
struct FrameSnapshot {
    // Keep all public frame metadata, but compare samples as raw IEEE-754 bits.
    // Ordinary f32 equality does not provide the required raw-bit contract.
    metadata: OpenJocPcmFrame,
    pcm_bits: Vec<u32>,
}

impl From<OpenJocPcmFrame> for FrameSnapshot {
    fn from(mut frame: OpenJocPcmFrame) -> Self {
        assert_eq!(
            frame.interleaved_f32.len(),
            frame.sample_count * frame.channel_count,
        );
        let pcm_bits = std::mem::take(&mut frame.interleaved_f32)
            .into_iter()
            .map(f32::to_bits)
            .collect();
        Self {
            metadata: frame,
            pcm_bits,
        }
    }
}

#[derive(Debug, PartialEq)]
struct DecodeSnapshot {
    push_statuses: Vec<OpenJocStatus>,
    output_info: Vec<OpenJocOutputInfo>,
    diagnostics: Vec<OpenJocDiagnostics>,
    frames: Vec<FrameSnapshot>,
    drain_status: OpenJocStatus,
    tail_frames: Vec<FrameSnapshot>,
}

fn decode_snapshot(
    config: OpenJocConfig,
    units: &[FixtureAu],
    detailed: bool,
    aggregate: bool,
) -> DecodeSnapshot {
    let mut session = OpenJocSession::new(config).unwrap();
    if aggregate {
        session.enable_stage_timing();
    }
    if detailed {
        session.enable_decode_stage_timing();
    }
    assert!(session.take_decode_stage_timing().is_none());
    let mut push_statuses = Vec::new();
    let mut output_info = vec![session.output_info()];
    let mut diagnostics = vec![session.diagnostics()];
    let mut frames = Vec::new();
    for &unit in units {
        let pts = -48_000 + i64::try_from(unit.input_start_sample).unwrap();
        let status = session.push_packet(packet(unit, Some(pts), false)).unwrap();
        assert!(matches!(
            status,
            OpenJocStatus::NeedMoreInput | OpenJocStatus::FrameAvailable,
        ));
        push_statuses.push(status);
        output_info.push(session.output_info());
        diagnostics.push(session.diagnostics());
        // Taking aggregate timing must not consume the independent detailed slot.
        if aggregate {
            let _ = session.take_stage_timing();
        }
        if detailed {
            take_record(&mut session, unit, unit.input_start_sample);
        } else {
            assert!(session.take_decode_stage_timing().is_none());
        }
        frames.extend(
            receive_all(&mut session)
                .into_iter()
                .map(FrameSnapshot::from),
        );
    }
    let drain_status = session.drain().unwrap();
    let tail_frames: Vec<_> = receive_all(&mut session)
        .into_iter()
        .map(FrameSnapshot::from)
        .collect();
    output_info.push(session.output_info());
    diagnostics.push(session.diagnostics());
    assert!(session.is_drained());
    assert_eq!(session.drain().unwrap(), OpenJocStatus::EndOfStream);
    assert!(session.take_decode_stage_timing().is_none());
    let has_nonzero = |frames: &[FrameSnapshot]| {
        frames
            .iter()
            .flat_map(|frame| &frame.pcm_bits)
            .any(|bits| bits & 0x7fff_ffff != 0)
    };
    assert!(
        has_nonzero(&frames),
        "fixture must exercise nonzero rendered PCM",
    );
    assert!(
        has_nonzero(&tail_frames),
        "fixture must exercise a nonzero drain tail",
    );
    DecodeSnapshot {
        push_statuses,
        output_info,
        diagnostics,
        frames,
        drain_status,
        tail_frames,
    }
}

#[test]
fn all_timing_flag_combinations_preserve_raw_pcm_tail_and_frame_metadata() {
    let units = first_six_complete_access_units();
    for config in rendering_configs() {
        let plain = decode_snapshot(config.clone(), &units, false, false);
        for (detailed, aggregate) in [(true, false), (false, true), (true, true)] {
            let measured = decode_snapshot(config.clone(), &units, detailed, aggregate);
            assert_eq!(
                measured, plain,
                "config={config:?}, detailed={detailed}, aggregate={aggregate}",
            );
        }
    }
}
