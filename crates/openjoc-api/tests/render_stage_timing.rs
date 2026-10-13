#![cfg(not(target_arch = "wasm32"))]

#[cfg(feature = "embedded-builtin-hrtf")]
use openjoc_api::{BinauralConfig, BuiltinHrtf, ListenerOrientation};
use openjoc_api::{
    FINAL_LINKED_GAIN_LATENCY_SAMPLES, OpenJocConfig, OpenJocDiagnostics, OpenJocError,
    OpenJocOutputInfo, OpenJocPacket, OpenJocPcmFrame, OpenJocRenderOperation,
    OpenJocRenderStageTiming, OpenJocSession, OpenJocStatus, RenderMode,
};
use openjoc_eac3::{group_access_units, index_syncframes};
use std::time::Duration;

// Intended destination: crates/openjoc-api/tests/render_stage_timing.rs.
// Keep this existing fixture unchanged. No byte-prefix or synthetic codec probes.
const FIXTURE: &[u8] = include_bytes!("../../openjoc-wasm/testdata/joc.lifecycle.ec3");

#[derive(Clone, Copy)]
struct FixtureAu {
    bytes: &'static [u8],
    input_start_sample: u64,
    sample_count: u16,
    sample_rate: u32,
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
            let end = last.offset.checked_add(last.header.frame_size).unwrap();
            let result = FixtureAu {
                bytes: &FIXTURE[first.offset..end],
                input_start_sample,
                sample_count: unit.samples,
                sample_rate: unit.sample_rate,
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

fn counts(frames: &[OpenJocPcmFrame]) -> (u64, u64) {
    (
        u64::try_from(frames.len()).unwrap(),
        frames
            .iter()
            .map(|frame| u64::try_from(frame.sample_count).unwrap())
            .sum(),
    )
}

fn assert_push(record: &OpenJocRenderStageTiming, au: FixtureAu, input_start_sample: u64) {
    assert_eq!(
        record.operation,
        OpenJocRenderOperation::Push {
            input_start_sample,
            input_sample_count: au.sample_count,
            sample_rate: au.sample_rate,
        },
    );
}

fn take(session: &mut OpenJocSession) -> OpenJocRenderStageTiming {
    let record = session
        .take_render_stage_timing()
        .expect("latest successful renderer operation");
    assert!(session.take_render_stage_timing().is_none());
    record
}

fn assert_pcm_counts(record: &OpenJocRenderStageTiming, frames: &[OpenJocPcmFrame]) {
    assert_eq!((record.pcm_blocks, record.pcm_samples), counts(frames));
}

fn assert_speaker_counts(record: &OpenJocRenderStageTiming, frames: &[OpenJocPcmFrame]) {
    assert_eq!(
        (record.speaker_blocks, record.speaker_samples),
        counts(frames)
    );
}

fn assert_no_work(record: &OpenJocRenderStageTiming) {
    assert_eq!((record.speaker_blocks, record.speaker_samples), (0, 0));
    assert_eq!((record.pcm_blocks, record.pcm_samples), (0, 0));
    assert_eq!(record.bridge_control_assembly, Duration::ZERO);
    assert_eq!(record.spatial_bridge_render, Duration::ZERO);
    assert_eq!(record.binaural_render, Duration::ZERO);
    assert_eq!(record.pcm_conversion, Duration::ZERO);
}

fn static_configs() -> Vec<OpenJocConfig> {
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
        configs.extend(binaural_configs());
        configs
    };
    configs
}

#[cfg(feature = "embedded-builtin-hrtf")]
fn binaural_configs() -> Vec<OpenJocConfig> {
    [BuiltinHrtf::SadieD1Ku100, BuiltinHrtf::SadieD2Kemar]
        .into_iter()
        .map(|builtin| OpenJocConfig {
            render_mode: RenderMode::Binaural,
            speaker_layout: "7.1.4".to_owned(),
            binaural: Some(BinauralConfig::builtin(builtin, "7.1.4")),
            ..OpenJocConfig::default()
        })
        .collect()
}

fn speaker_control_config(config: &OpenJocConfig) -> OpenJocConfig {
    let mut control = config.clone();
    if control.render_mode == RenderMode::Binaural {
        control.render_mode = RenderMode::Speaker;
        control.binaural = None;
    }
    control
}

fn speaker_projection_tail(
    config: &OpenJocConfig,
    mut control_tail: Vec<OpenJocPcmFrame>,
) -> Vec<OpenJocPcmFrame> {
    if config.render_mode == RenderMode::Binaural {
        // Existing session construction disables speaker linked gain only in
        // binaural mode. The corresponding public speaker control has an extra
        // final linked-gain block; it is not part of binaural projection work.
        let linked_gain = control_tail.pop().expect("speaker linked-gain drain block");
        assert_eq!(linked_gain.sample_count, FINAL_LINKED_GAIN_LATENCY_SAMPLES);
    }
    control_tail
}

#[test]
fn renderer_timing_defaults_off_and_other_timing_flags_do_not_enable_it() {
    fn operation_traits<T: Clone + Copy + std::fmt::Debug + Eq>() {}
    fn record_traits<T: Clone + std::fmt::Debug + Eq>() {}
    operation_traits::<OpenJocRenderOperation>();
    record_traits::<OpenJocRenderStageTiming>();
    // No Default bound: every public record must name its actual operation.
    let units = first_six_complete_access_units();
    for (aggregate, decode) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut session = OpenJocSession::new(OpenJocConfig::default()).unwrap();
        if aggregate {
            session.enable_stage_timing();
        }
        if decode {
            session.enable_decode_stage_timing();
        }
        assert!(session.take_render_stage_timing().is_none());
        for &au in &units[..2] {
            push(&mut session, au);
            receive_all(&mut session);
            assert!(session.take_render_stage_timing().is_none());
        }
        session.drain().unwrap();
        receive_all(&mut session);
        assert!(session.take_render_stage_timing().is_none());
    }
}

#[test]
fn enabling_is_independent_and_reenabling_clears_only_the_renderer_slot() {
    let units = first_six_complete_access_units();
    let mut session = OpenJocSession::new(OpenJocConfig::default()).unwrap();
    session.enable_render_stage_timing();
    assert_eq!(push(&mut session, units[0]), OpenJocStatus::NeedMoreInput);
    assert!(session.take_decode_stage_timing().is_none());
    assert_eq!(
        session.take_stage_timing(),
        openjoc_api::OpenJocStageTiming::default()
    );
    session.enable_stage_timing();
    session.enable_decode_stage_timing();
    assert_push(&take(&mut session), units[0], 0);

    assert_eq!(push(&mut session, units[1]), OpenJocStatus::FrameAvailable);
    let info = session.output_info();
    let descriptor = session.effective_config_descriptor();
    session.enable_render_stage_timing();
    assert!(session.take_render_stage_timing().is_none());
    assert_eq!(session.output_info(), info);
    assert_eq!(session.effective_config_descriptor(), descriptor);
    let decode = session.take_decode_stage_timing().unwrap();
    assert_eq!(decode.input_start_sample, units[1].input_start_sample);
    // Aggregate clocks may be exactly zero; presence cannot be inferred from
    // elapsed time. Detailed-slot preservation is independently observable.
    let _ = session.take_stage_timing();
    assert_ne!(receive_all(&mut session), [] as [OpenJocPcmFrame; 0]);

    assert_eq!(push(&mut session, units[2]), OpenJocStatus::FrameAvailable);
    let _ = session.take_stage_timing();
    let _ = session.take_decode_stage_timing();
    session.enable_stage_timing();
    session.enable_decode_stage_timing();
    assert_push(&take(&mut session), units[2], units[2].input_start_sample);
    assert_ne!(receive_all(&mut session), [] as [OpenJocPcmFrame; 0]);
    session.enable_render_stage_timing();
    assert!(session.take_render_stage_timing().is_none());
    assert_eq!(push(&mut session, units[3]), OpenJocStatus::FrameAvailable);
    assert_push(&take(&mut session), units[3], units[3].input_start_sample);
}

#[test]
fn empty_aligned_output_is_a_real_success_and_latest_success_wins() {
    let units = first_six_complete_access_units();
    for config in static_configs() {
        let mut session = OpenJocSession::new(config).unwrap();
        session.enable_render_stage_timing();
        assert_eq!(push(&mut session, units[0]), OpenJocStatus::NeedMoreInput);
        assert!(session.receive_frame().is_none());
        let first = take(&mut session);
        assert_push(&first, units[0], 0);
        assert_no_work(&first);

        assert_eq!(push(&mut session, units[1]), OpenJocStatus::FrameAvailable);
        let frames = receive_all(&mut session);
        assert_ne!(frames, [] as [OpenJocPcmFrame; 0]);
        // Leave this record untaken. Receiving owned PCM does not consume it;
        // the next successful operation replaces it rather than queueing it.
        assert_eq!(push(&mut session, units[2]), OpenJocStatus::FrameAvailable);
        let latest_frames = receive_all(&mut session);
        let latest = take(&mut session);
        assert_push(&latest, units[2], units[2].input_start_sample);
        assert_pcm_counts(&latest, &latest_frames);
        assert_speaker_counts(&latest, &latest_frames);
        assert!(session.receive_frame().is_none());
        assert!(session.take_render_stage_timing().is_none());
    }
}

#[test]
fn backpressure_wrong_pts_and_invalid_pull_preserve_the_latest_success() {
    let units = first_six_complete_access_units();
    let mut session = OpenJocSession::new(OpenJocConfig::default()).unwrap();
    session.enable_render_stage_timing();
    let origin = 12_345;
    assert_eq!(
        session
            .push_packet(packet(units[0], Some(origin), false))
            .unwrap(),
        OpenJocStatus::NeedMoreInput,
    );
    let expected = origin + i64::from(units[0].sample_count);
    let info = session.output_info();
    for _ in 0..2 {
        assert_eq!(
            session.push_packet(packet(units[1], Some(origin), false)),
            Err(OpenJocError::TimestampDiscontinuity {
                expected,
                actual: origin
            }),
        );
    }
    assert_eq!(session.output_info(), info);
    assert!(matches!(
        session.receive_binaural_frame(),
        Err(OpenJocError::InvalidConfig(_)),
    ));
    let prior = take(&mut session);
    assert_push(&prior, units[0], 0);
    assert_no_work(&prior);
    assert_eq!(
        session.push_packet(packet(units[1], Some(origin), false)),
        Err(OpenJocError::TimestampDiscontinuity {
            expected,
            actual: origin
        }),
    );
    assert!(session.take_render_stage_timing().is_none());
    // The legal AU is retried byte-for-byte; only its packet timestamp changes.
    assert_eq!(
        session
            .push_packet(packet(units[1], Some(expected), false))
            .unwrap(),
        OpenJocStatus::FrameAvailable,
    );
    assert_eq!(push(&mut session, units[2]), OpenJocStatus::OutputPending);
    assert_eq!(session.drain().unwrap(), OpenJocStatus::OutputPending);
    // Backpressure precedes discontinuity handling and must not clear the slot.
    assert_eq!(
        session.push_packet(packet(units[0], None, true)).unwrap(),
        OpenJocStatus::OutputPending,
    );
    let frames = receive_all(&mut session);
    let prior = take(&mut session);
    assert_push(&prior, units[1], units[1].input_start_sample);
    assert_pcm_counts(&prior, &frames);
    assert_eq!(push(&mut session, units[2]), OpenJocStatus::FrameAvailable);
    assert_push(&take(&mut session), units[2], units[2].input_start_sample);
}

#[test]
fn first_drain_publishes_counts_and_repeated_drain_preserves_the_slot() {
    let units = first_six_complete_access_units();
    for config in static_configs() {
        let mut control = OpenJocSession::new(speaker_control_config(&config)).unwrap();
        let mut session = OpenJocSession::new(config.clone()).unwrap();
        session.enable_render_stage_timing();
        for (index, &au) in units[..3].iter().enumerate() {
            assert_eq!(push(&mut session, au), push(&mut control, au));
            let speakers = receive_all(&mut control);
            let frames = receive_all(&mut session);
            if index < 2 {
                let record = take(&mut session);
                assert_push(&record, au, au.input_start_sample);
                assert_speaker_counts(&record, &speakers);
                assert_pcm_counts(&record, &frames);
                if config.render_mode != RenderMode::Binaural {
                    assert_eq!(record.binaural_render, Duration::ZERO);
                }
            }
        }
        // The final Push remains untaken; successful Drain must replace it.
        assert_eq!(control.drain().unwrap(), OpenJocStatus::FrameAvailable);
        let speaker_tail = speaker_projection_tail(&config, receive_all(&mut control));
        assert_eq!(session.drain().unwrap(), OpenJocStatus::FrameAvailable);
        assert_eq!(session.drain().unwrap(), OpenJocStatus::OutputPending);
        let tail = receive_all(&mut session);
        assert_ne!(tail, [] as [OpenJocPcmFrame; 0]);
        assert!(session.is_drained());
        assert_eq!(session.drain().unwrap(), OpenJocStatus::EndOfStream);
        assert_eq!(
            session.push_packet(packet(units[3], None, false)),
            Err(OpenJocError::AlreadyDrained),
        );
        let record = take(&mut session);
        assert_eq!(record.operation, OpenJocRenderOperation::Drain);
        // Includes the speaker renderer's final linked-gain block. Binaural
        // conversion additionally counts every actually returned FIR/LFE tail.
        assert_speaker_counts(&record, &speaker_tail);
        assert_pcm_counts(&record, &tail);
        if config.render_mode != RenderMode::Binaural {
            assert_eq!(record.binaural_render, Duration::ZERO);
        }
        assert_eq!(session.drain().unwrap(), OpenJocStatus::EndOfStream);
        assert!(session.take_render_stage_timing().is_none());
    }
}

#[test]
fn empty_drain_publishes_a_zero_count_record_once() {
    for config in static_configs() {
        let mut session = OpenJocSession::new(config).unwrap();
        session.enable_render_stage_timing();
        assert_eq!(session.drain().unwrap(), OpenJocStatus::EndOfStream);
        assert!(session.receive_frame().is_none());
        assert_eq!(session.drain().unwrap(), OpenJocStatus::EndOfStream);
        let record = take(&mut session);
        assert_eq!(record.operation, OpenJocRenderOperation::Drain);
        assert_no_work(&record);
        assert_eq!(session.drain().unwrap(), OpenJocStatus::EndOfStream);
        assert!(session.take_render_stage_timing().is_none());
    }
}

#[test]
fn reset_flush_and_discontinuity_clear_the_slot_and_retain_enablement() {
    let units = first_six_complete_access_units();
    let resets: [fn(&mut OpenJocSession); 4] = [
        OpenJocSession::reset,
        OpenJocSession::flush,
        |session| session.try_reset().unwrap(),
        |session| session.try_flush().unwrap(),
    ];
    for reset in resets {
        let mut session = OpenJocSession::new(OpenJocConfig::default()).unwrap();
        session.enable_render_stage_timing();
        push(&mut session, units[0]);
        assert_eq!(push(&mut session, units[1]), OpenJocStatus::FrameAvailable);
        reset(&mut session);
        assert!(session.take_render_stage_timing().is_none());
        assert!(session.receive_frame().is_none());
        assert_eq!(push(&mut session, units[0]), OpenJocStatus::NeedMoreInput);
        let record = take(&mut session);
        assert_push(&record, units[0], 0);
        assert_no_work(&record);
    }
    let mut session = OpenJocSession::new(OpenJocConfig::default()).unwrap();
    session.enable_render_stage_timing();
    push(&mut session, units[0]);
    push(&mut session, units[1]);
    receive_all(&mut session);
    assert_eq!(
        session.push_packet(packet(units[0], None, true)).unwrap(),
        OpenJocStatus::NeedMoreInput,
    );
    let record = take(&mut session);
    assert_push(&record, units[0], 0);
    assert_no_work(&record);
    assert_eq!(push(&mut session, units[1]), OpenJocStatus::FrameAvailable);
    assert_push(&take(&mut session), units[1], units[1].input_start_sample);
}

#[derive(Debug, PartialEq)]
struct FrameSnapshot {
    // Removing only the sample vector retains every public PCM descriptor.
    // f32 equality alone would miss signed-zero and NaN-payload differences.
    metadata: OpenJocPcmFrame,
    pcm_bits: Vec<u32>,
}

impl From<OpenJocPcmFrame> for FrameSnapshot {
    fn from(mut frame: OpenJocPcmFrame) -> Self {
        assert_eq!(
            frame.interleaved_f32.len(),
            frame.sample_count * frame.channel_count
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
struct StreamSnapshot {
    descriptor: String,
    fingerprint: String,
    push_statuses: Vec<OpenJocStatus>,
    output_info: Vec<OpenJocOutputInfo>,
    diagnostics: Vec<OpenJocDiagnostics>,
    frames: Vec<Vec<FrameSnapshot>>,
    drain_status: OpenJocStatus,
    tail: Vec<FrameSnapshot>,
}

#[derive(Clone, Copy, Debug)]
enum RenderSchedule {
    Off,
    FromStart,
    AfterPush(usize),
    ReenableAfterPush(usize),
}

fn collect_output(session: &mut OpenJocSession, pull: bool, enabled: bool) -> Vec<OpenJocPcmFrame> {
    if !pull {
        return receive_all(session);
    }
    assert!(session.receive_frame().is_none());
    let mut frames = Vec::new();
    while let Some(frame) = session.receive_binaural_frame().unwrap() {
        if enabled {
            let record = take(session);
            assert_eq!(record.operation, OpenJocRenderOperation::BinauralPull);
            assert_eq!((record.speaker_blocks, record.speaker_samples), (0, 0));
            assert_eq!(record.bridge_control_assembly, Duration::ZERO);
            assert_eq!(record.spatial_bridge_render, Duration::ZERO);
            assert_pcm_counts(&record, std::slice::from_ref(&frame));
        } else {
            assert!(session.take_render_stage_timing().is_none());
        }
        frames.push(frame);
    }
    // Every returned frame's record was consumed; a no-output call does not
    // invent another record, including after reconstruction and FIR tails.
    assert!(session.take_render_stage_timing().is_none());
    frames
}

fn snapshot(
    config: OpenJocConfig,
    units: &[FixtureAu],
    pull: bool,
    aggregate: bool,
    decode: bool,
    schedule: RenderSchedule,
) -> StreamSnapshot {
    let mut session = if pull {
        OpenJocSession::new_with_listener_orientation_pull(config, 128).unwrap()
    } else {
        OpenJocSession::new(config).unwrap()
    };
    let descriptor = session.effective_config_descriptor();
    let fingerprint = session.effective_config_fingerprint();
    if aggregate {
        session.enable_stage_timing();
    }
    if decode {
        session.enable_decode_stage_timing();
    }
    let mut enabled = matches!(
        schedule,
        RenderSchedule::FromStart | RenderSchedule::ReenableAfterPush(_)
    );
    if enabled {
        session.enable_render_stage_timing();
    }
    let mut push_statuses = Vec::new();
    let mut output_info = vec![session.output_info()];
    let mut diagnostics = vec![session.diagnostics()];
    let mut frames = Vec::new();
    for (index, &au) in units.iter().enumerate() {
        let pts = -48_000 + i64::try_from(au.input_start_sample).unwrap();
        let status = session.push_packet(packet(au, Some(pts), false)).unwrap();
        assert!(matches!(
            status,
            OpenJocStatus::NeedMoreInput | OpenJocStatus::FrameAvailable
        ));
        push_statuses.push(status);
        output_info.push(session.output_info());
        diagnostics.push(session.diagnostics());
        let clear_after_push = matches!(
            schedule,
            RenderSchedule::AfterPush(at) | RenderSchedule::ReenableAfterPush(at) if at == index
        );
        let expect_record = enabled && !clear_after_push;
        if clear_after_push {
            // At index 1 this changes collection while delayed PCM is pending.
            // It must neither reset the stream nor discard queued output.
            session.enable_render_stage_timing();
            enabled = true;
        }
        let _ = session.take_stage_timing();
        if decode {
            let record = session.take_decode_stage_timing().unwrap();
            assert_eq!(record.input_start_sample, au.input_start_sample);
        } else {
            assert!(session.take_decode_stage_timing().is_none());
        }
        if pull {
            if expect_record {
                let record = take(&mut session);
                assert_push(&record, au, au.input_start_sample);
                assert_eq!(record.pcm_blocks, 0);
                assert_eq!(record.pcm_samples, 0);
                assert_eq!(record.binaural_render, Duration::ZERO);
                assert_eq!(record.pcm_conversion, Duration::ZERO);
                assert_eq!(
                    record.speaker_samples,
                    u64::try_from(session.pending_binaural_input_samples().unwrap()).unwrap(),
                );
            } else {
                assert!(session.take_render_stage_timing().is_none());
            }
        }
        let output = collect_output(&mut session, pull, enabled);
        if !pull {
            if expect_record {
                let record = take(&mut session);
                assert_push(&record, au, au.input_start_sample);
                assert_pcm_counts(&record, &output);
                assert_speaker_counts(&record, &output);
            } else {
                assert!(session.take_render_stage_timing().is_none());
            }
        }
        frames.push(output.into_iter().map(FrameSnapshot::from).collect());
    }
    let drain_status = session.drain().unwrap();
    let drain_record = if enabled {
        let record = take(&mut session);
        assert_eq!(record.operation, OpenJocRenderOperation::Drain);
        if pull {
            assert_eq!((record.pcm_blocks, record.pcm_samples), (0, 0));
            assert_eq!(record.binaural_render, Duration::ZERO);
            assert_eq!(record.pcm_conversion, Duration::ZERO);
            assert_eq!(
                record.speaker_samples,
                u64::try_from(session.pending_binaural_input_samples().unwrap()).unwrap(),
            );
        }
        Some(record)
    } else {
        assert!(session.take_render_stage_timing().is_none());
        None
    };
    let tail = collect_output(&mut session, pull, enabled);
    if !pull {
        if let Some(record) = drain_record {
            assert_pcm_counts(&record, &tail);
        }
    }
    output_info.push(session.output_info());
    diagnostics.push(session.diagnostics());
    assert!(session.is_drained());
    assert_eq!(session.drain().unwrap(), OpenJocStatus::EndOfStream);
    assert!(session.take_render_stage_timing().is_none());
    assert_eq!(session.effective_config_descriptor(), descriptor);
    assert_eq!(session.effective_config_fingerprint(), fingerprint);
    let tail: Vec<_> = tail.into_iter().map(FrameSnapshot::from).collect();
    let has_nonzero = |frames: &[FrameSnapshot]| {
        frames
            .iter()
            .flat_map(|frame| &frame.pcm_bits)
            .any(|bits| bits & 0x7fff_ffff != 0)
    };
    assert!(
        frames
            .iter()
            .any(|frames: &Vec<FrameSnapshot>| has_nonzero(frames))
    );
    assert!(has_nonzero(&tail));
    StreamSnapshot {
        descriptor,
        fingerprint,
        push_statuses,
        output_info,
        diagnostics,
        frames,
        drain_status,
        tail,
    }
}

#[test]
fn every_timing_flag_combination_preserves_static_pcm_bits_descriptors_and_tails() {
    let units = first_six_complete_access_units();
    for config in static_configs() {
        let plain = snapshot(
            config.clone(),
            &units,
            false,
            false,
            false,
            RenderSchedule::Off,
        );
        for mask in 1..8 {
            let schedule = if mask & 4 == 0 {
                RenderSchedule::Off
            } else {
                RenderSchedule::FromStart
            };
            let measured = snapshot(
                config.clone(),
                &units,
                false,
                mask & 1 != 0,
                mask & 2 != 0,
                schedule,
            );
            assert_eq!(measured, plain, "config={config:?}, timing_mask={mask}");
        }
        for schedule in [
            RenderSchedule::AfterPush(1),
            RenderSchedule::ReenableAfterPush(1),
        ] {
            assert_eq!(
                snapshot(config.clone(), &units, false, true, true, schedule),
                plain,
                "config={config:?}, schedule={schedule:?}",
            );
        }
    }
}

#[cfg(feature = "embedded-builtin-hrtf")]
#[test]
fn every_timing_flag_combination_and_midstream_enable_preserve_pull_pcm_bits() {
    let units = first_six_complete_access_units();
    for config in binaural_configs() {
        let plain = snapshot(
            config.clone(),
            &units,
            true,
            false,
            false,
            RenderSchedule::Off,
        );
        for mask in 1..8 {
            let schedule = if mask & 4 == 0 {
                RenderSchedule::Off
            } else {
                RenderSchedule::FromStart
            };
            assert_eq!(
                snapshot(
                    config.clone(),
                    &units,
                    true,
                    mask & 1 != 0,
                    mask & 2 != 0,
                    schedule
                ),
                plain,
                "config={config:?}, timing_mask={mask}",
            );
        }
        for schedule in [
            RenderSchedule::AfterPush(1),
            RenderSchedule::ReenableAfterPush(1),
        ] {
            assert_eq!(
                snapshot(config.clone(), &units, true, true, true, schedule),
                plain,
                "config={config:?}, schedule={schedule:?}",
            );
        }
    }
}

#[cfg(feature = "embedded-builtin-hrtf")]
#[test]
fn pull_operations_count_speaker_work_and_each_returned_pcm_frame_separately() {
    let units = first_six_complete_access_units();
    for config in binaural_configs() {
        let mut control = OpenJocSession::new(speaker_control_config(&config)).unwrap();
        let mut session =
            OpenJocSession::new_with_listener_orientation_pull(config.clone(), 128).unwrap();
        session.enable_render_stage_timing();
        for &au in &units[..3] {
            assert_eq!(push(&mut session, au), push(&mut control, au));
            let speakers = receive_all(&mut control);
            assert!(session.receive_frame().is_none());
            if speakers.is_empty() {
                assert!(session.receive_binaural_frame().unwrap().is_none());
            } else {
                assert_eq!(push(&mut session, au), OpenJocStatus::OutputPending);
                assert_eq!(session.drain().unwrap(), OpenJocStatus::OutputPending);
            }
            let record = take(&mut session);
            assert_push(&record, au, au.input_start_sample);
            assert_speaker_counts(&record, &speakers);
            assert_eq!(
                record.speaker_samples,
                u64::try_from(session.pending_binaural_input_samples().unwrap()).unwrap(),
            );
            assert_eq!((record.pcm_blocks, record.pcm_samples), (0, 0));
            assert_eq!(record.binaural_render, Duration::ZERO);
            assert_eq!(record.pcm_conversion, Duration::ZERO);
            let frames = collect_output(&mut session, true, true);
            assert_eq!(counts(&frames).1, counts(&speakers).1);
        }
        control.drain().unwrap();
        let speakers = speaker_projection_tail(&config, receive_all(&mut control));
        assert_eq!(session.drain().unwrap(), OpenJocStatus::FrameAvailable);
        assert_eq!(session.drain().unwrap(), OpenJocStatus::OutputPending);
        let record = take(&mut session);
        assert_eq!(record.operation, OpenJocRenderOperation::Drain);
        assert_speaker_counts(&record, &speakers);
        assert_eq!(
            record.speaker_samples,
            u64::try_from(session.pending_binaural_input_samples().unwrap()).unwrap(),
        );
        assert_eq!((record.pcm_blocks, record.pcm_samples), (0, 0));
        assert_eq!(record.binaural_render, Duration::ZERO);
        assert_eq!(record.pcm_conversion, Duration::ZERO);
        let mut projected = Vec::new();
        while session.pending_binaural_input_samples().unwrap() > 0 {
            let frame = session.receive_binaural_frame().unwrap().unwrap();
            if session.pending_binaural_input_samples().unwrap() > 0 {
                let record = take(&mut session);
                assert_eq!(record.operation, OpenJocRenderOperation::BinauralPull);
                assert_pcm_counts(&record, std::slice::from_ref(&frame));
            }
            projected.push(frame);
        }
        assert_eq!(counts(&projected).1, counts(&speakers).1);
        // The remaining native FIR tail makes this a repeated status query,
        // not a second successful Drain transition. Preserve the last pull.
        assert_eq!(session.drain().unwrap(), OpenJocStatus::FrameAvailable);
        let record = take(&mut session);
        assert_eq!(record.operation, OpenJocRenderOperation::BinauralPull);
        assert_pcm_counts(&record, std::slice::from_ref(projected.last().unwrap()));
        let fir_tail = collect_output(&mut session, true, true);
        assert_ne!(fir_tail, [] as [OpenJocPcmFrame; 0]);
        assert!(session.is_drained());
        assert_eq!(session.drain().unwrap(), OpenJocStatus::EndOfStream);
        assert!(session.take_render_stage_timing().is_none());
    }
}

#[cfg(feature = "embedded-builtin-hrtf")]
#[test]
fn pull_empty_drain_and_reset_keep_the_same_renderer_slot_contract() {
    let units = first_six_complete_access_units();
    for config in binaural_configs() {
        let mut session = OpenJocSession::new_with_listener_orientation_pull(config, 128).unwrap();
        session.enable_render_stage_timing();
        assert_eq!(session.drain().unwrap(), OpenJocStatus::EndOfStream);
        assert!(session.receive_binaural_frame().unwrap().is_none());
        assert_eq!(session.drain().unwrap(), OpenJocStatus::EndOfStream);
        let empty = take(&mut session);
        assert_eq!(empty.operation, OpenJocRenderOperation::Drain);
        assert_no_work(&empty);
        let resets: [fn(&mut OpenJocSession); 4] = [
            OpenJocSession::reset,
            OpenJocSession::flush,
            |session| session.try_reset().unwrap(),
            |session| session.try_flush().unwrap(),
        ];
        for reset in resets {
            reset(&mut session);
            assert!(session.take_render_stage_timing().is_none());
            assert_eq!(session.pending_binaural_input_samples(), Some(0));
            assert_eq!(push(&mut session, units[0]), OpenJocStatus::NeedMoreInput);
            assert_push(&take(&mut session), units[0], 0);
            assert_eq!(push(&mut session, units[1]), OpenJocStatus::FrameAvailable);
            assert!(session.pending_binaural_input_samples().unwrap() > 0);
            // Leave the record and pending virtual-speaker input for reset.
        }
        receive_all(&mut session);
        let _ = take(&mut session);
        collect_output(&mut session, true, true);
        assert_eq!(
            session.push_packet(packet(units[0], None, true)).unwrap(),
            OpenJocStatus::NeedMoreInput,
        );
        let record = take(&mut session);
        assert_push(&record, units[0], 0);
        assert_no_work(&record);
    }
}

#[cfg(feature = "embedded-builtin-hrtf")]
#[test]
fn pull_overwrites_push_and_no_output_pull_preserves_the_last_returned_frame_record() {
    let units = first_six_complete_access_units();
    for config in binaural_configs() {
        let mut session = OpenJocSession::new_with_listener_orientation_pull(config, 128).unwrap();
        session.enable_render_stage_timing();
        assert_eq!(push(&mut session, units[0]), OpenJocStatus::NeedMoreInput);
        assert!(session.receive_binaural_frame().unwrap().is_none());
        assert_push(&take(&mut session), units[0], 0);
        assert_eq!(push(&mut session, units[1]), OpenJocStatus::FrameAvailable);
        // Leave Push untaken, then leave every successful pull untaken.
        let mut last = None;
        while let Some(frame) = session.receive_binaural_frame().unwrap() {
            last = Some(frame);
        }
        let last = last.unwrap();
        assert!(session.receive_frame().is_none());
        assert!(session.receive_binaural_frame().unwrap().is_none());
        let record = take(&mut session);
        assert_eq!(record.operation, OpenJocRenderOperation::BinauralPull);
        assert_pcm_counts(&record, std::slice::from_ref(&last));
        assert_eq!((record.speaker_blocks, record.speaker_samples), (0, 0));
        assert_eq!(record.bridge_control_assembly, Duration::ZERO);
        assert_eq!(record.spatial_bridge_render, Duration::ZERO);
        assert!(session.receive_binaural_frame().unwrap().is_none());
        assert!(session.take_render_stage_timing().is_none());

        session.drain().unwrap();
        // Leave Drain untaken. Subsequent reconstruction/FIR-tail pulls replace
        // it, and no-output/repeated-drain queries must keep the last pull.
        let mut last_tail = None;
        while let Some(frame) = session.receive_binaural_frame().unwrap() {
            last_tail = Some(frame);
        }
        let last_tail = last_tail.unwrap();
        assert_eq!(session.drain().unwrap(), OpenJocStatus::EndOfStream);
        assert!(session.receive_binaural_frame().unwrap().is_none());
        let record = take(&mut session);
        assert_eq!(record.operation, OpenJocRenderOperation::BinauralPull);
        assert_pcm_counts(&record, std::slice::from_ref(&last_tail));
        assert!(session.receive_binaural_frame().unwrap().is_none());
        assert!(session.take_render_stage_timing().is_none());
    }
}

#[cfg(feature = "embedded-builtin-hrtf")]
#[test]
fn reenable_preserves_pending_orientation_and_pull_audio_state() {
    let units = first_six_complete_access_units();
    for config in binaural_configs() {
        let mut control =
            OpenJocSession::new_with_listener_orientation_pull(config.clone(), 128).unwrap();
        let mut observed = OpenJocSession::new_with_listener_orientation_pull(config, 128).unwrap();
        observed.enable_render_stage_timing();
        for session in [&mut control, &mut observed] {
            push(session, units[0]);
            push(session, units[1]);
            let preparer = session.listener_orientation_preparer().unwrap();
            let angle = 9.0_f64.to_radians() / 2.0;
            let pose = ListenerOrientation::new(0.0, 0.0, angle.sin(), angle.cos()).unwrap();
            let update = preparer.prepare(pose, 0, 1).unwrap();
            assert_eq!(
                session
                    .apply_prepared_listener_orientation(update)
                    .unwrap()
                    .accepted_sequence,
                1
            );
        }
        let pending = observed.pending_binaural_input_samples();
        assert!(pending.unwrap() > 0);
        assert_eq!(observed.pending_listener_orientation_sequence(), Some(1));
        observed.enable_render_stage_timing();
        assert!(observed.take_render_stage_timing().is_none());
        assert_eq!(observed.pending_binaural_input_samples(), pending);
        assert_eq!(observed.pending_listener_orientation_sequence(), Some(1));
        assert_eq!(observed.listener_orientation_stream_epoch(), Some(0));
        let a: Vec<_> = collect_output(&mut observed, true, true)
            .into_iter()
            .map(FrameSnapshot::from)
            .collect();
        let b: Vec<_> = collect_output(&mut control, true, false)
            .into_iter()
            .map(FrameSnapshot::from)
            .collect();
        assert_eq!(a, b);
        assert_eq!(
            observed.last_applied_listener_orientation(),
            control.last_applied_listener_orientation()
        );
        assert_eq!(
            observed.pending_listener_orientation_sequence(),
            control.pending_listener_orientation_sequence()
        );
        assert_eq!(observed.drain().unwrap(), control.drain().unwrap());
        assert_eq!(take(&mut observed).operation, OpenJocRenderOperation::Drain);
        let a: Vec<_> = collect_output(&mut observed, true, true)
            .into_iter()
            .map(FrameSnapshot::from)
            .collect();
        let b: Vec<_> = collect_output(&mut control, true, false)
            .into_iter()
            .map(FrameSnapshot::from)
            .collect();
        assert_eq!(a, b);
        assert!(observed.is_drained() && control.is_drained());
    }
}
