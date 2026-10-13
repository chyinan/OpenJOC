//! Regression contracts for shared CLI performance accumulation.

use super::*;
use crate::progress::ProgressReporter;
use openjoc_emdf::JocValidationProfile;
use openjoc_wave::SampleFormat;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

/// Owns only a directory this test successfully created, never an existing one.
struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(label: &str) -> Self {
        loop {
            let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "openjoc-performance-refactor-{label}-{}-{sequence}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create isolated test directory: {error}"),
            }
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn inventory(&self) -> Vec<std::ffi::OsString> {
        let mut names = fs::read_dir(&self.0)
            .expect("read isolated test directory")
            .map(|entry| entry.expect("read directory entry").file_name())
            .collect::<Vec<_>>();
        names.sort();
        names
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        // A failed cleanup must not cause a second panic during an assertion.
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn ns(value: u64) -> Duration {
    Duration::from_nanos(value)
}

fn reconstruction_timing(scale: u64) -> ReconstructionStageTiming {
    ReconstructionStageTiming {
        payload_parsing: ns(scale),
        coefficient_decode: ns(2 * scale),
        dequantization: ns(3 * scale),
        qmf_analysis: ns(4 * scale),
        interpolation: ns(5 * scale),
        matrix_reconstruction: ns(6 * scale),
        qmf_synthesis: ns(7 * scale),
        output_assembly: ns(8 * scale),
        buffer_initialization: ns(9 * scale),
    }
}

fn eac3_timing(scale: u64) -> Eac3DecodeStageTiming {
    Eac3DecodeStageTiming {
        total: ns(1_000 * scale),
        syncframe_and_header_parsing: ns(scale),
        audio_block_syntax_and_exponents: ns(2 * scale),
        bit_allocation: ns(3 * scale),
        mantissa_unpack_and_dequantization: ns(4 * scale),
        coupling_rematrix_and_spx: ns(5 * scale),
        inverse_transform: ns(6 * scale),
        window_and_overlap_add: ns(7 * scale),
        pcm_assembly: ns(8 * scale),
        allocation_and_copy: ns(9 * scale),
        decoder_state_commit: ns(10 * scale),
        syncframes: 11 * scale,
        audio_blocks: 12 * scale,
        full_bandwidth_channel_blocks: 13 * scale,
        lfe_blocks: 14 * scale,
        long_transforms: 15 * scale,
        short_transforms: 16 * scale,
        aht_elements: 17 * scale,
        coupling_blocks: 18 * scale,
        spx_blocks: 19 * scale,
    }
}

fn seeded_performance() -> RenderPerformance {
    let mut performance = RenderPerformance::new();
    performance.input_container = ns(100);
    performance.profile_validation = ns(200);
    performance.eac3_decode = ns(300);
    performance.joc_reconstruction = ns(400);
    performance.bridge_control_assembly = ns(500);
    performance.spatial_bridge_render = ns(600);
    performance.binaural_render = ns(700);
    performance.output_conversion_wav_write = ns(800);
    performance.frame_times = vec![ns(901)];
    performance.sample_rate_hz = Some(48_000);
    performance.processed_access_units = 41;
    performance.rendered_frames = 43;
    performance.total_audio_samples = 47;
    performance.output_frames = 53;
    performance.output_bytes = 59;
    performance.progress_enabled = true;
    performance.progress_updates = 61;
    performance.progress_overhead = ns(67);
    performance.reconstruction_stages = reconstruction_timing(2);
    performance.eac3_stages = eac3_timing(2);
    performance.eac3_frame_times = vec![ns(902)];
    performance.slowest_eac3_frames = vec![Eac3SlowFrameTiming {
        frame_index: 903,
        duration: ns(904),
        stages: eac3_timing(13),
    }];
    performance
}

fn decode_input() -> DecodeStageTiming {
    DecodeStageTiming {
        eac3_decode: ns(11),
        joc_reconstruction: ns(13),
        frame_times: vec![ns(17), ns(19)],
        collect_frame_times: true,
        reconstruction_stages: reconstruction_timing(3),
        eac3_stages: eac3_timing(3),
        eac3_frame_times: vec![ns(23), ns(29)],
        slowest_eac3_frames: vec![
            Eac3SlowFrameTiming {
                frame_index: 107,
                duration: ns(29),
                stages: eac3_timing(7),
            },
            Eac3SlowFrameTiming {
                frame_index: 103,
                duration: ns(23),
                stages: eac3_timing(11),
            },
        ],
    }
}

fn render_input() -> RenderStageTiming {
    RenderStageTiming {
        bridge_control_assembly: ns(31),
        spatial_bridge_render: ns(37),
        binaural_render: ns(41),
        output_conversion_wav_write: ns(43),
        rendered_frames: 47,
        // This field does not replace the report's total_audio_samples.
        rendered_samples: u64::MAX,
    }
}

fn assert_merged_fields(performance: &RenderPerformance, started_at: Instant) {
    assert_eq!(performance.started_at, started_at);
    assert_eq!(performance.input_container, ns(100));
    assert_eq!(performance.profile_validation, ns(200));
    assert_eq!(performance.eac3_decode, ns(311));
    assert_eq!(performance.joc_reconstruction, ns(413));
    assert_eq!(performance.bridge_control_assembly, ns(531));
    assert_eq!(performance.spatial_bridge_render, ns(637));
    assert_eq!(performance.binaural_render, ns(741));
    assert_eq!(performance.output_conversion_wav_write, ns(843));
    assert_eq!(performance.frame_times, vec![ns(17), ns(19)]);
    assert_eq!(performance.sample_rate_hz, Some(48_000));
    assert_eq!(performance.processed_access_units, 41);
    assert_eq!(performance.rendered_frames, 90);
    assert_eq!(performance.total_audio_samples, 47);
    assert_eq!(performance.reconstruction_stages, reconstruction_timing(5));
    assert_eq!(performance.eac3_stages, eac3_timing(5));
    assert_eq!(performance.eac3_frame_times, vec![ns(23), ns(29)]);
    assert_eq!(performance.slowest_eac3_frames.len(), 2);
    let first = &performance.slowest_eac3_frames[0];
    assert_eq!(first.frame_index, 107);
    assert_eq!(first.duration, ns(29));
    assert_eq!(first.stages, eac3_timing(7));
    let second = &performance.slowest_eac3_frames[1];
    assert_eq!(second.frame_index, 103);
    assert_eq!(second.duration, ns(23));
    assert_eq!(second.stages, eac3_timing(11));
}

fn staging_path(report: &Path) -> PathBuf {
    report.with_file_name(format!(
        ".{}.openjoc-report-partial",
        report
            .file_name()
            .expect("report filename")
            .to_string_lossy()
    ))
}

#[test]
fn rendered_au_accumulates_only_three_stages_and_explicit_input_samples() {
    let mut aggregate = RenderStageTiming {
        bridge_control_assembly: ns(100),
        spatial_bridge_render: ns(200),
        binaural_render: ns(300),
        output_conversion_wav_write: ns(400),
        rendered_frames: 500,
        rendered_samples: 600,
    };
    let stages = RenderStageTiming {
        bridge_control_assembly: ns(11),
        spatial_bridge_render: ns(13),
        binaural_render: ns(17),
        output_conversion_wav_write: ns(19),
        rendered_frames: u64::MAX,
        rendered_samples: u64::MAX,
    };

    aggregate.record_rendered_au(&stages, 1_536);
    assert_eq!(aggregate.bridge_control_assembly, ns(111));
    assert_eq!(aggregate.spatial_bridge_render, ns(213));
    assert_eq!(aggregate.binaural_render, ns(317));
    assert_eq!(aggregate.output_conversion_wav_write, ns(400));
    assert_eq!(aggregate.rendered_frames, 501);
    assert_eq!(aggregate.rendered_samples, 2_136);

    aggregate.record_rendered_au(&stages, u16::MAX);
    assert_eq!(aggregate.bridge_control_assembly, ns(122));
    assert_eq!(aggregate.spatial_bridge_render, ns(226));
    assert_eq!(aggregate.binaural_render, ns(334));
    assert_eq!(aggregate.output_conversion_wav_write, ns(400));
    assert_eq!(aggregate.rendered_frames, 502);
    assert_eq!(aggregate.rendered_samples, 67_671);

    assert_eq!(stages.bridge_control_assembly, ns(11));
    assert_eq!(stages.spatial_bridge_render, ns(13));
    assert_eq!(stages.binaural_render, ns(17));
    assert_eq!(stages.output_conversion_wav_write, ns(19));
    assert_eq!(stages.rendered_frames, u64::MAX);
    assert_eq!(stages.rendered_samples, u64::MAX);
}

#[test]
fn completed_output_merges_existing_values_and_copies_real_progress_snapshot() {
    let directory = TestDirectory::new("completed");
    let output_path = directory.path("completed-output.bin");
    // A metadata-only file sentinel; this is not a generated PCM/WAV fixture.
    let output_bytes = b"completed output byte sentinel\n";
    fs::write(&output_path, output_bytes).expect("write completed-output sentinel");

    for enabled in [false, true] {
        let mut progress = ProgressReporter::new(enabled, "7.1.4", 1, 1_536, 48_000, Some(80));
        progress.update(1, 1_536);
        progress.finish();
        let expected_updates = progress.updates();
        let expected_overhead = progress.overhead();
        assert_eq!(expected_updates, u64::from(enabled));

        let mut performance = seeded_performance();
        performance.progress_enabled = !enabled;
        let started_at = performance.started_at;
        performance
            .merge_completed_output(decode_input(), &render_input(), 79, &output_path, &progress)
            .expect("merge completed output");

        assert_merged_fields(&performance, started_at);
        assert_eq!(performance.output_frames, 79);
        assert_eq!(
            performance.output_bytes,
            u64::try_from(output_bytes.len()).unwrap()
        );
        assert_eq!(performance.progress_enabled, enabled);
        assert_eq!(performance.progress_updates, expected_updates);
        assert_eq!(performance.progress_overhead, expected_overhead);
        assert_eq!(fs::read(&output_path).unwrap(), output_bytes);

        let report_path = directory.path(if enabled {
            "enabled.json"
        } else {
            "disabled.json"
        });
        assert!(!report_path.exists());
        // As in the existing routes, publication is a separate subsequent call.
        write_report(
            &report_path,
            &performance,
            "7.1.4",
            JocValidationProfile::EtsiStrict,
            SampleFormat::F32,
            false,
        )
        .expect("publish report after successful merge");
        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(&report_path).unwrap()).unwrap();
        assert_eq!(report["schema"], PERFORMANCE_REPORT_SCHEMA);
        assert_eq!(report["sample_rate_hz"], 48_000);
        assert_eq!(report["processed_access_units"], 41);
        assert_eq!(report["rendered_frames"], 90);
        assert_eq!(report["rendered_samples"], 47);
        assert_eq!(report["output_frames"], 79);
        assert_eq!(report["output_bytes"], performance.output_bytes);
        assert_eq!(report["progress"]["enabled"], enabled);
        assert_eq!(report["progress"]["updates"], expected_updates);
        assert!(!staging_path(&report_path).exists());
    }
}

#[test]
fn missing_output_metadata_leaves_merges_and_frames_but_preserves_bytes_and_progress() {
    let directory = TestDirectory::new("missing-output");
    let output_path = directory.path("absent-output.bin");
    let report_path = directory.path("previous-report.json");
    let previous_report = b"previous report sentinel";
    fs::write(&report_path, previous_report).expect("write previous report");
    let inventory = directory.inventory();
    let progress = ProgressReporter::new(false, "7.1.4", 1, 1_536, 48_000, Some(80));
    let mut performance = seeded_performance();
    let started_at = performance.started_at;
    let mut attempted_publication = false;

    // This pins the helper's error boundary and the caller's `?` choreography.
    // The independent CLI gate must still verify the actual production routes.
    let result = (|| -> io::Result<()> {
        performance.merge_completed_output(
            decode_input(),
            &render_input(),
            79,
            &output_path,
            &progress,
        )?;
        attempted_publication = true;
        write_report(
            &report_path,
            &performance,
            "7.1.4",
            JocValidationProfile::EtsiStrict,
            SampleFormat::F32,
            true,
        )
    })();

    assert_eq!(
        result.expect_err("missing output metadata").kind(),
        io::ErrorKind::NotFound
    );
    assert_merged_fields(&performance, started_at);
    assert_eq!(performance.output_frames, 79);
    assert_eq!(performance.output_bytes, 59);
    assert!(performance.progress_enabled);
    assert_eq!(performance.progress_updates, 61);
    assert_eq!(performance.progress_overhead, ns(67));
    assert!(!attempted_publication);
    assert_eq!(fs::read(&report_path).unwrap(), previous_report);
    assert!(!output_path.exists());
    assert!(!staging_path(&report_path).exists());
    assert_eq!(directory.inventory(), inventory);
}

#[test]
fn equal_duration_top_sixteen_preserves_insertion_order_at_cutoff_and_in_report() {
    let indices = [
        90, 3, 80, 7, 70, 11, 60, 15, 50, 19, 40, 23, 30, 27, 20, 31, 10, 35, 0, 39,
    ];
    let mut decode = DecodeStageTiming::new(true);
    for (position, frame_index) in indices.into_iter().enumerate() {
        decode.record_eac3_frame(
            frame_index,
            ns(50),
            eac3_timing(u64::try_from(position + 1).unwrap()),
        );
    }
    assert_eq!(
        decode
            .slowest_eac3_frames
            .iter()
            .map(|frame| frame.frame_index)
            .collect::<Vec<_>>(),
        indices[..16]
    );
    assert_eq!(decode.eac3_frame_times, vec![ns(50); 20]);
    assert_eq!(decode.eac3_stages, eac3_timing(210));

    decode.record_eac3_frame(1_001, ns(80), eac3_timing(21));
    decode.record_eac3_frame(1_002, ns(1), eac3_timing(22));
    decode.record_eac3_frame(1_003, ns(50), eac3_timing(23));
    let expected_indices = std::iter::once(1_001)
        .chain(indices[..15].iter().copied())
        .collect::<Vec<_>>();
    assert_eq!(
        decode
            .slowest_eac3_frames
            .iter()
            .map(|frame| frame.frame_index)
            .collect::<Vec<_>>(),
        expected_indices
    );
    assert_eq!(decode.eac3_frame_times.len(), 23);
    assert_eq!(&decode.eac3_frame_times[..20], vec![ns(50); 20]);
    assert_eq!(&decode.eac3_frame_times[20..], &[ns(80), ns(1), ns(50)]);
    assert_eq!(decode.eac3_stages, eac3_timing(276));
    assert_eq!(decode.slowest_eac3_frames[0].stages, eac3_timing(21));
    for (position, frame) in decode.slowest_eac3_frames[1..].iter().enumerate() {
        assert_eq!(frame.duration, ns(50));
        assert_eq!(
            frame.stages,
            eac3_timing(u64::try_from(position + 1).unwrap())
        );
    }

    let directory = TestDirectory::new("stable-ties");
    let report_path = directory.path("performance.json");
    let mut performance = RenderPerformance::new();
    performance.merge_decode(decode);
    write_report(
        &report_path,
        &performance,
        "7.1.4",
        JocValidationProfile::EtsiStrict,
        SampleFormat::F32,
        false,
    )
    .expect("write tied-frame report");
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(&report_path).unwrap()).unwrap();
    let published_indices = report["eac3_slowest_frames"]
        .as_array()
        .unwrap()
        .iter()
        .map(|frame| frame["frame_index"].as_u64().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        published_indices,
        expected_indices
            .into_iter()
            .map(|index| u64::try_from(index).unwrap())
            .collect::<Vec<_>>()
    );
    assert_eq!(report["eac3_decode_frame_ms"]["count"], 23);
    assert_eq!(report["eac3_decode_workload"]["syncframes"], 11 * 276);
}

#[test]
fn report_staging_collision_preserves_completed_output_and_all_existing_files() {
    for previous_report_exists in [false, true] {
        let directory = TestDirectory::new("staging-collision");
        let output_path = directory.path("completed-output.bin");
        let report_path = directory.path("performance.json");
        let staging = staging_path(&report_path);
        // Filesystem sentinels only; no synthetic audio or expected PCM changes.
        let output_bytes = b"already completed output sentinel";
        let previous_report = b"previous report sentinel";
        let existing_staging = b"pre-existing staging owner sentinel";
        fs::write(&output_path, output_bytes).unwrap();
        if previous_report_exists {
            fs::write(&report_path, previous_report).unwrap();
        }
        fs::write(&staging, existing_staging).unwrap();
        let inventory = directory.inventory();
        let progress = ProgressReporter::new(false, "7.1.4", 1, 1_536, 48_000, Some(80));
        let mut performance = seeded_performance();
        performance
            .merge_completed_output(decode_input(), &render_input(), 79, &output_path, &progress)
            .expect("completed output metadata succeeds before report collision");

        let error = write_report(
            &report_path,
            &performance,
            "7.1.4",
            JocValidationProfile::EtsiStrict,
            SampleFormat::F32,
            true,
        )
        .expect_err("create_new must reject the existing report staging file");

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&output_path).unwrap(), output_bytes);
        assert_eq!(fs::read(&staging).unwrap(), existing_staging);
        if previous_report_exists {
            assert_eq!(fs::read(&report_path).unwrap(), previous_report);
        } else {
            assert!(!report_path.exists());
        }
        assert_eq!(directory.inventory(), inventory);
    }
}
