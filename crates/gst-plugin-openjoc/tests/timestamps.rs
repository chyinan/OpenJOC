#![cfg(feature = "gstreamer")]

use gst::prelude::*;
use gst_audio::prelude::*;
use openjoc_api::{BinauralConfig, OpenJocConfig, OpenJocPacket, OpenJocSession, RenderMode};

const FIXTURE: &[u8] = include_bytes!("../../openjoc-api/tests/fixtures/timestamps.ec3");
const AU_BYTES: usize = 4096;
const AU_SAMPLES: u64 = 1536;
const AU_NS: u64 = 32_000_000;

fn sample_time(samples: u64) -> u64 {
    (samples * 1_000_000_000 + 24_000) / 48_000
}

fn reference_pcm_from(first: usize, count: usize, binaural: bool) -> Vec<u8> {
    let config = if binaural {
        OpenJocConfig {
            render_mode: RenderMode::Binaural,
            speaker_layout: "7.1.4".to_owned(),
            binaural: Some(BinauralConfig::builtin_generic("7.1.4")),
            ..OpenJocConfig::default()
        }
    } else {
        OpenJocConfig::default()
    };
    let mut session = OpenJocSession::new(config).unwrap();
    let mut output = Vec::new();
    for index in 0..=count {
        if index == count {
            session.drain().unwrap();
        } else {
            session
                .push_packet(OpenJocPacket {
                    data: &FIXTURE[(first + index) * AU_BYTES..(first + index + 1) * AU_BYTES],
                    pts_samples: Some(i64::try_from(index as u64 * AU_SAMPLES).unwrap()),
                    discontinuity: false,
                    preroll: false,
                })
                .unwrap();
        }
        while let Some(frame) = session.receive_frame() {
            // GstAudio's canonical 5.1 order is FL FR FC LFE SL SR.
            let labels: &[&str] = if binaural {
                &["Left Ear", "Right Ear"]
            } else {
                &["FL", "FR", "FC", "LFE", "Ls", "Rs"]
            };
            let order: Vec<_> = labels
                .iter()
                .map(|label| {
                    frame
                        .channel_labels
                        .iter()
                        .position(|s| s == label || (*label == "LFE" && s == "LFE1"))
                        .unwrap()
                })
                .collect();
            for sample in frame.interleaved_f32.chunks_exact(frame.channel_count) {
                for &channel in &order {
                    output.extend_from_slice(&sample[channel].to_le_bytes());
                }
            }
        }
    }
    output
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Boundary {
    None,
    HardFlush,
    CapsChange,
    Discontinuity,
}

fn check_pipeline(origin_ns: u64, count: usize, reset: Boundary, stop_samples: Option<u64>) {
    run_pipeline(origin_ns, count, reset, stop_samples, false);
}

fn run_pipeline(
    origin_ns: u64,
    count: usize,
    reset: Boundary,
    stop_samples: Option<u64>,
    binaural: bool,
) {
    let bytes_per_sample = if binaural { 8 } else { 24 };
    gst::init().unwrap();
    gstopenjoc::register_static_plugin().unwrap();
    let caps = gst::Caps::builder("audio/x-eac3")
        .features(["openjoc:joc"])
        .field("framed", true)
        .field("alignment", "frame")
        .field("openjoc-joc", true)
        .build();
    let source = gst_app::AppSrc::builder()
        .caps(&caps)
        .format(gst::Format::Time)
        .handle_segment_change(true)
        .build();
    let decoder = gst::ElementFactory::make("openjocdec").build().unwrap();
    if binaural {
        decoder.set_property("render-mode", "binaural");
    }
    let sink = gst_app::AppSink::builder()
        .sync(false)
        .async_(false)
        .build();
    let pipeline = gst::Pipeline::new();
    pipeline
        .add_many([source.upcast_ref(), &decoder, sink.upcast_ref()])
        .unwrap();
    gst::Element::link_many([source.upcast_ref(), &decoder, sink.upcast_ref()]).unwrap();
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
    source.set_callbacks(
        gst_app::AppSrcCallbacks::builder()
            .need_data(move |_, _| {
                let _ = ready_tx.try_send(());
            })
            .build(),
    );
    if reset != Boundary::None {
        // The second AU proves the first AU boundary. Exactly one AU has
        // reached OpenJOC and is delayed when appsrc asks for more data.
        for index in 0..2 {
            let mut buffer =
                gst::Buffer::from_slice(FIXTURE[index * AU_BYTES..(index + 1) * AU_BYTES].to_vec());
            buffer
                .get_mut()
                .unwrap()
                .set_pts(gst::ClockTime::from_nseconds(index as u64 * AU_NS));
            source.push_buffer(buffer).unwrap();
        }
    }
    pipeline.set_state(gst::State::Playing).unwrap();
    ready_rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    let pad = source.static_pad("src").unwrap();
    if reset != Boundary::None {
        assert!(
            sink.try_pull_sample(gst::ClockTime::ZERO).is_none(),
            "warm-up must not emit PCM"
        );
        if reset == Boundary::HardFlush {
            assert!(pad.push_event(gst::event::FlushStart::new()));
            assert!(pad.push_event(gst::event::FlushStop::new(true)));
        } else if reset == Boundary::CapsChange {
            let mut changed = caps.clone();
            changed
                .make_mut()
                .structure_mut(0)
                .unwrap()
                .set("rate", 48_000_i32);
            source.set_caps(Some(&changed));
        }
    }
    let segment = if reset == Boundary::HardFlush || stop_samples.is_some() {
        let mut segment = gst::FormattedSegment::<gst::ClockTime>::new();
        segment.set_start(gst::ClockTime::from_nseconds(origin_ns));
        segment.set_time(gst::ClockTime::from_nseconds(origin_ns));
        segment.set_stop(stop_samples.map(|samples| {
            gst::ClockTime::from_nseconds(origin_ns + samples * 1_000_000_000 / 48_000)
        }));
        Some(segment)
    } else {
        None
    };
    for index in 0..count {
        let fixture_index = if reset == Boundary::CapsChange {
            index + 2
        } else {
            index
        };
        let mut buffer = gst::Buffer::from_slice(
            FIXTURE[fixture_index * AU_BYTES..(fixture_index + 1) * AU_BYTES].to_vec(),
        );
        let writable = buffer.get_mut().unwrap();
        writable.set_pts(gst::ClockTime::from_nseconds(
            origin_ns + index as u64 * AU_NS,
        ));
        writable.set_duration(gst::ClockTime::from_nseconds(AU_NS));
        if reset == Boundary::Discontinuity && index == 0 {
            writable.set_flags(gst::BufferFlags::DISCONT);
        }
        if let Some(segment) = &segment {
            let sample = gst::Sample::builder()
                .buffer(&buffer)
                .segment(segment)
                .build();
            source.push_sample(&sample).unwrap();
        } else {
            source.push_buffer(buffer).unwrap();
        }
    }
    source.end_of_stream().unwrap();
    let mut buffers = Vec::new();
    while let Some(sample) = sink.try_pull_sample(gst::ClockTime::from_seconds(10)) {
        let buffer = sample.buffer().unwrap();
        buffers.push((
            buffer.pts(),
            buffer.duration(),
            buffer.map_readable().unwrap().as_slice().to_vec(),
        ));
    }
    let eos = sink.is_eos();
    let error = pipeline
        .bus()
        .unwrap()
        .pop_filtered(&[gst::MessageType::Error]);
    let latency = decoder
        .downcast_ref::<gst_audio::AudioDecoder>()
        .unwrap()
        .latency();
    pipeline.set_state(gst::State::Null).unwrap();
    let expected_latency =
        gst::ClockTime::from_nseconds(if binaural { 12_020_833 } else { 12_687_500 });
    assert_eq!(latency, (expected_latency, Some(expected_latency)));
    assert!(error.is_none(), "pipeline error: {error:?}");
    assert!(eos, "pipeline timed out before EOS");
    assert_ne!(buffers.len(), 0);
    let output_origin = if reset == Boundary::CapsChange {
        AU_NS
    } else {
        origin_ns
    };
    let mut old_pcm = Vec::new();
    let mut pcm = Vec::new();
    let mut samples = 0_u64;
    for (pts, duration, bytes) in buffers {
        if reset == Boundary::Discontinuity && pts.unwrap().nseconds() < origin_ns {
            assert_eq!(
                pts.unwrap().nseconds(),
                sample_time(old_pcm.len() as u64 / bytes_per_sample)
            );
            old_pcm.extend(bytes);
            continue;
        }
        assert_eq!(bytes.len() % bytes_per_sample as usize, 0);
        let length = bytes.len() as u64 / bytes_per_sample;
        assert_eq!(
            pts,
            Some(gst::ClockTime::from_nseconds(
                output_origin + sample_time(samples)
            )),
            "PCM must retain the first compressed AU's timestamp"
        );
        assert_eq!(
            duration,
            Some(gst::ClockTime::from_nseconds(
                sample_time(samples + length) - sample_time(samples)
            ))
        );
        samples += length;
        pcm.extend(bytes);
    }
    let expected_count = count + usize::from(reset == Boundary::CapsChange);
    let mut expected = reference_pcm_from(
        usize::from(reset == Boundary::CapsChange),
        expected_count,
        binaural,
    );
    if reset == Boundary::Discontinuity {
        assert_eq!(old_pcm, reference_pcm_from(0, 2, binaural));
    }
    if let Some(stop) = stop_samples {
        expected.truncate(stop as usize * bytes_per_sample as usize);
    }
    assert_eq!(
        pcm.len(),
        expected.len(),
        "EOS must preserve all delayed PCM"
    );
    assert_eq!(
        pcm, expected,
        "native output must be bitwise identical to the session oracle"
    );
    assert_eq!(samples, expected.len() as u64 / bytes_per_sample);
}

#[test]
fn delayed_pcm_starts_at_first_au_and_eos_preserves_tail() {
    check_pipeline(0, 6, Boundary::None, None);
}

#[test]
fn delayed_pcm_preserves_nonzero_segment_origin() {
    check_pipeline(2_000_000_000, 6, Boundary::None, None);
}

#[test]
fn hard_flush_during_warmup_reanchors_pending_input() {
    check_pipeline(3_000_000_000, 6, Boundary::HardFlush, None);
}

#[test]
fn finite_segment_clips_correctly_timed_pcm() {
    check_pipeline(0, 6, Boundary::None, Some(4 * AU_SAMPLES + 768));
}

#[test]
fn caps_change_discards_pending_input_before_new_pcm() {
    check_pipeline(2 * AU_NS, 4, Boundary::CapsChange, None);
}

#[test]
fn discontinuity_drains_old_pending_input_then_reanchors() {
    check_pipeline(3_000_000_000, 6, Boundary::Discontinuity, None);
}

#[test]
fn single_au_drain_anchors_first_pcm_and_preserves_tail() {
    check_pipeline(1_000_000_000, 1, Boundary::None, None);
}

#[test]
fn binaural_eos_preserves_fir_tail_pcm_and_timestamps() {
    run_pipeline(2_000_000_000, 3, Boundary::None, None, true);
}
