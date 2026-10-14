#![cfg(feature = "embedded-builtin-hrtf")]

use openjoc_api::{
    BinauralConfig, BuiltinHrtf, FINAL_LINKED_GAIN_LATENCY_SAMPLES, OpenJocConfig,
    OpenJocOutputInfo, OpenJocPacket, OpenJocSession, OpenJocStatus, QMF_LATENCY_SAMPLES,
    RenderMode, SofaLoadLimits,
};
use openjoc_eac3::{group_access_units, index_syncframes};
use std::{fmt::Write as _, io::Write as _, path::Path};

#[derive(Debug, Eq, PartialEq)]
struct Construction {
    descriptor: String,
    fingerprint: String,
    output: OpenJocOutputInfo,
    resources: Option<String>,
    epoch: Option<u64>,
}

#[derive(Debug, Eq, PartialEq)]
struct Capture {
    construction: Construction,
    stream: String,
    pcm: Vec<u8>,
}

fn receive(session: &mut OpenJocSession, pull: bool, capture: &mut Capture) {
    loop {
        let frame = if pull {
            session.receive_binaural_frame().unwrap()
        } else {
            session.receive_frame()
        };
        let Some(mut frame) = frame else { break };
        assert_eq!(
            frame.interleaved_f32.len(),
            frame.sample_count * frame.channel_count
        );
        if pull {
            assert!(frame.sample_count <= 128);
        }
        for sample in std::mem::take(&mut frame.interleaved_f32) {
            capture
                .pcm
                .extend_from_slice(&sample.to_bits().to_le_bytes());
        }
        writeln!(capture.stream, "frame={frame:?}").unwrap();
    }
}

fn capture(name: &str, mut session: OpenJocSession, pull: bool) -> Capture {
    let resources = session.listener_orientation_preparer().map(|preparer| {
        format!(
            "identity={:?};sources={:?};max_taps={};tap_bytes={};rate={};lfe={:?};lfe_delay={}",
            preparer.resource_identity(),
            preparer.source_mappings().collect::<Vec<_>>(),
            preparer.max_filter_taps(),
            preparer.query_hrir_tap_payload_bytes(),
            preparer.sample_rate_hz(),
            preparer.lfe_channel_index(),
            preparer.lfe_alignment_delay_samples(),
        )
    });
    let mut result = Capture {
        construction: Construction {
            descriptor: session.effective_config_descriptor(),
            fingerprint: session.effective_config_fingerprint(),
            output: session.output_info(),
            resources,
            epoch: session.listener_orientation_stream_epoch(),
        },
        stream: String::new(),
        pcm: Vec::new(),
    };
    // Use complete, already-admitted access units, without synthetic codec input.
    let bytes = include_bytes!("../../openjoc-wasm/testdata/joc.lifecycle.ec3");
    let frames = index_syncframes(bytes).unwrap();
    let units = group_access_units(&frames).unwrap();
    assert!(units.len() >= 6);
    let mut pts = 0;
    for unit in units.iter().take(6) {
        let first = &frames[unit.first_frame];
        let last = &frames[unit.first_frame + unit.frame_count - 1];
        let status = session
            .push_packet(OpenJocPacket {
                data: &bytes[first.offset..last.offset + last.header.frame_size],
                pts_samples: Some(pts),
                discontinuity: false,
                preroll: false,
            })
            .unwrap();
        assert_ne!(status, OpenJocStatus::OutputPending);
        writeln!(result.stream, "push={status:?}").unwrap();
        receive(&mut session, pull, &mut result);
        pts += i64::from(unit.samples);
    }
    loop {
        let status = session.drain().unwrap();
        writeln!(result.stream, "drain={status:?}").unwrap();
        receive(&mut session, pull, &mut result);
        if status == OpenJocStatus::EndOfStream {
            assert!(session.is_drained());
            break;
        }
    }
    assert_ne!(result.pcm, [] as [u8; 0]);
    writeln!(
        result.stream,
        "output={:?}\ndiagnostics={:?}",
        session.output_info(),
        session.diagnostics(),
    )
    .unwrap();

    // Optional external before/after evidence. Never replace an existing capture.
    if let Some(directory) = std::env::var_os("OPENJOC_CONSTRUCTOR_CAPTURE") {
        let directory = Path::new(&directory);
        for (extension, bytes) in [
            (
                "txt",
                format!("{:?}\n{}", result.construction, result.stream).into_bytes(),
            ),
            ("f32le", result.pcm.clone()),
        ] {
            std::fs::File::create_new(directory.join(format!("{name}.{extension}")))
                .unwrap()
                .write_all(&bytes)
                .unwrap();
        }
    }
    result
}

#[test]
fn default_constructor_preserves_the_static_speaker_contract() {
    let config = OpenJocConfig::default();
    let session = OpenJocSession::new(config.clone()).unwrap();
    assert_eq!(
        session.effective_config_descriptor(),
        config.effective_config_descriptor()
    );
    assert_eq!(
        session.effective_config_fingerprint(),
        config.effective_config_fingerprint()
    );
    assert_eq!(
        session.latency_samples(),
        QMF_LATENCY_SAMPLES + FINAL_LINKED_GAIN_LATENCY_SAMPLES
    );
    assert!(session.listener_orientation_preparer().is_none());
    assert_eq!(session.listener_orientation_stream_epoch(), None);
    let expected = capture("default", session, false);
    let limited =
        OpenJocSession::new_with_sofa_load_limits(config, SofaLoadLimits::default()).unwrap();
    assert_eq!(expected, capture("default-limits", limited, false));
}

#[test]
fn legal_binaural_constructors_preserve_identity_resources_and_pcm() {
    for (name, builtin) in [
        ("d1", BuiltinHrtf::SadieD1Ku100),
        ("d2", BuiltinHrtf::SadieD2Kemar),
    ] {
        let config = OpenJocConfig {
            render_mode: RenderMode::Binaural,
            speaker_layout: "7.1.4".to_owned(),
            binaural: Some(BinauralConfig::builtin(builtin, "7.1.4")),
            ..OpenJocConfig::default()
        };
        let session = OpenJocSession::new(config.clone()).unwrap();
        assert_eq!(
            session.effective_config_descriptor(),
            config.effective_config_descriptor()
        );
        assert_eq!(
            session.effective_config_fingerprint(),
            config.effective_config_fingerprint()
        );
        assert_eq!(session.latency_samples(), QMF_LATENCY_SAMPLES);
        assert!(session.listener_orientation_preparer().is_none());
        let expected = capture(&format!("{name}-builtin"), session, false);
        let asset = openjoc_sofa::builtin_hrtf_asset_bytes(builtin).unwrap();
        let external = OpenJocSession::new_with_hrtf_asset(config.clone(), asset).unwrap();
        assert_eq!(
            expected,
            capture(&format!("{name}-external"), external, false)
        );
        let limited =
            OpenJocSession::new_with_sofa_load_limits(config.clone(), SofaLoadLimits::default())
                .unwrap();
        assert_eq!(expected, capture(&format!("{name}-limits"), limited, false));

        let orientation = OpenJocSession::new_with_listener_orientation(config.clone()).unwrap();
        assert_eq!(orientation.listener_orientation_stream_epoch(), Some(0));
        let orientation = capture(&format!("{name}-orientation"), orientation, false);
        assert!(
            orientation
                .construction
                .descriptor
                .contains("\nlistener_orientation=enabled\n")
        );
        assert_ne!(
            expected.construction.fingerprint,
            orientation.construction.fingerprint
        );
        assert!(orientation.construction.resources.is_some());

        let pull = OpenJocSession::new_with_listener_orientation_pull(config.clone(), 128).unwrap();
        let pull = capture(&format!("{name}-pull"), pull, true);
        assert!(pull.construction.descriptor.ends_with(
            "\nlistener_orientation_pull=enabled\nlistener_orientation_pull_max_samples=128"
        ));
        assert_ne!(
            orientation.construction.fingerprint,
            pull.construction.fingerprint
        );
        assert_eq!(pull.construction.epoch, Some(0));

        let epoch =
            OpenJocSession::new_with_listener_orientation_pull_at_epoch(config.clone(), 128, 7)
                .unwrap();
        let preparer = epoch.listener_orientation_preparer().unwrap();
        assert_eq!(preparer.source_count(), 11);
        let shared = OpenJocSession::new_with_listener_orientation_pull_using_preparer(
            config, 128, 7, preparer,
        )
        .unwrap();
        let epoch = capture(&format!("{name}-epoch"), epoch, true);
        assert_eq!(epoch, capture(&format!("{name}-shared"), shared, true));
        assert_eq!(epoch.construction.epoch, Some(7));
        assert_eq!(pull.construction.descriptor, epoch.construction.descriptor);
        assert_eq!(
            pull.construction.fingerprint,
            epoch.construction.fingerprint
        );
        assert_eq!(
            orientation.construction.resources,
            pull.construction.resources
        );
        assert_eq!(pull.construction.resources, epoch.construction.resources);
        assert_eq!(pull.construction.output, epoch.construction.output);
        assert_eq!(pull.stream, epoch.stream);
        assert_eq!(pull.pcm, epoch.pcm);
    }
}
