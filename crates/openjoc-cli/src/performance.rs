use openjoc_eac3::Eac3DecodeStageTiming;
use openjoc_joc::ReconstructionStageTiming;
use serde::Serialize;
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::Path,
    time::{Duration, Instant},
};

pub(crate) const PERFORMANCE_REPORT_SCHEMA: &str = "openjoc.joc-render-performance.v1";

#[derive(Debug)]
pub(crate) struct DecodeStageTiming {
    pub(crate) eac3_decode: Duration,
    pub(crate) joc_reconstruction: Duration,
    pub(crate) frame_times: Vec<Duration>,
    pub(crate) collect_frame_times: bool,
    pub(crate) reconstruction_stages: ReconstructionStageTiming,
    pub(crate) eac3_stages: Eac3DecodeStageTiming,
    pub(crate) eac3_frame_times: Vec<Duration>,
    pub(crate) slowest_eac3_frames: Vec<Eac3SlowFrameTiming>,
}

#[derive(Debug)]
pub(crate) struct Eac3SlowFrameTiming {
    frame_index: usize,
    duration: Duration,
    stages: Eac3DecodeStageTiming,
}

impl DecodeStageTiming {
    pub(crate) fn new(collect_frame_times: bool) -> Self {
        Self {
            eac3_decode: Duration::ZERO,
            joc_reconstruction: Duration::ZERO,
            frame_times: Vec::new(),
            collect_frame_times,
            reconstruction_stages: ReconstructionStageTiming::default(),
            eac3_stages: Eac3DecodeStageTiming::default(),
            eac3_frame_times: Vec::new(),
            slowest_eac3_frames: Vec::new(),
        }
    }

    pub(crate) fn record_eac3_frame(
        &mut self,
        frame_index: usize,
        duration: Duration,
        stages: Eac3DecodeStageTiming,
    ) {
        self.eac3_stages.add_assign(&stages);
        if !self.collect_frame_times {
            return;
        }
        self.eac3_frame_times.push(duration);
        self.slowest_eac3_frames.push(Eac3SlowFrameTiming {
            frame_index,
            duration,
            stages,
        });
        self.slowest_eac3_frames
            .sort_by(|left, right| right.duration.cmp(&left.duration));
        self.slowest_eac3_frames.truncate(16);
    }
}

impl Default for DecodeStageTiming {
    fn default() -> Self {
        Self::new(false)
    }
}

#[derive(Debug, Default)]
pub(crate) struct RenderStageTiming {
    pub(crate) bridge_control_assembly: Duration,
    pub(crate) spatial_bridge_render: Duration,
    pub(crate) binaural_render: Duration,
    pub(crate) output_conversion_wav_write: Duration,
    pub(crate) rendered_frames: u64,
    pub(crate) rendered_samples: u64,
}

#[derive(Debug)]
pub(crate) struct RenderPerformance {
    pub(crate) started_at: Instant,
    pub(crate) input_container: Duration,
    pub(crate) profile_validation: Duration,
    pub(crate) eac3_decode: Duration,
    pub(crate) joc_reconstruction: Duration,
    pub(crate) bridge_control_assembly: Duration,
    pub(crate) spatial_bridge_render: Duration,
    pub(crate) binaural_render: Duration,
    pub(crate) output_conversion_wav_write: Duration,
    pub(crate) frame_times: Vec<Duration>,
    pub(crate) sample_rate_hz: Option<u32>,
    pub(crate) processed_access_units: u64,
    pub(crate) rendered_frames: u64,
    pub(crate) total_audio_samples: u64,
    pub(crate) output_frames: u64,
    pub(crate) output_bytes: u64,
    pub(crate) progress_enabled: bool,
    pub(crate) progress_updates: u64,
    pub(crate) progress_overhead: Duration,
    pub(crate) reconstruction_stages: ReconstructionStageTiming,
    pub(crate) eac3_stages: Eac3DecodeStageTiming,
    pub(crate) eac3_frame_times: Vec<Duration>,
    pub(crate) slowest_eac3_frames: Vec<Eac3SlowFrameTiming>,
}

impl RenderPerformance {
    pub(crate) fn new() -> Self {
        Self {
            started_at: Instant::now(),
            input_container: Duration::ZERO,
            profile_validation: Duration::ZERO,
            eac3_decode: Duration::ZERO,
            joc_reconstruction: Duration::ZERO,
            bridge_control_assembly: Duration::ZERO,
            spatial_bridge_render: Duration::ZERO,
            binaural_render: Duration::ZERO,
            output_conversion_wav_write: Duration::ZERO,
            frame_times: Vec::new(),
            sample_rate_hz: None,
            processed_access_units: 0,
            rendered_frames: 0,
            total_audio_samples: 0,
            output_frames: 0,
            output_bytes: 0,
            progress_enabled: false,
            progress_updates: 0,
            progress_overhead: Duration::ZERO,
            reconstruction_stages: ReconstructionStageTiming::default(),
            eac3_stages: Eac3DecodeStageTiming::default(),
            eac3_frame_times: Vec::new(),
            slowest_eac3_frames: Vec::new(),
        }
    }

    pub(crate) fn merge_decode(&mut self, timing: DecodeStageTiming) {
        self.eac3_decode += timing.eac3_decode;
        self.joc_reconstruction += timing.joc_reconstruction;
        self.frame_times = timing.frame_times;
        self.reconstruction_stages
            .add_assign(&timing.reconstruction_stages);
        self.eac3_stages.add_assign(&timing.eac3_stages);
        self.eac3_frame_times = timing.eac3_frame_times;
        self.slowest_eac3_frames = timing.slowest_eac3_frames;
    }

    pub(crate) fn merge_render(&mut self, timing: &RenderStageTiming) {
        self.bridge_control_assembly += timing.bridge_control_assembly;
        self.spatial_bridge_render += timing.spatial_bridge_render;
        self.binaural_render += timing.binaural_render;
        self.output_conversion_wav_write += timing.output_conversion_wav_write;
        self.rendered_frames += timing.rendered_frames;
    }
}

#[derive(Serialize)]
struct PerformanceReport<'a> {
    schema: &'static str,
    openjoc_version: &'static str,
    selected_layout: &'a str,
    selected_validation_profile: &'static str,
    sample_rate_hz: u32,
    processed_access_units: u64,
    rendered_frames: u64,
    rendered_samples: u64,
    output_frames: u64,
    audio_duration_seconds: f64,
    wall_duration_seconds: f64,
    realtime_factor: f64,
    output_bytes: u64,
    build_mode: &'static str,
    stage_timings_ms: StageTimings,
    eac3_decode_stages_ms: Eac3DecodeStages,
    eac3_decode_workload: Eac3DecodeWorkload,
    eac3_decode_frame_ms: FrameTimingDistribution,
    eac3_slowest_frames: Vec<Eac3SlowFrameReport>,
    joc_reconstruction_stages_ms: JocReconstructionStages,
    core_frame_processing_ms: FrameTimingDistribution,
    progress: ProgressReport,
}

#[derive(Serialize)]
struct StageTimings {
    input_container: f64,
    profile_validation: f64,
    eac3_decode: f64,
    joc_reconstruction: f64,
    bridge_control_assembly: f64,
    spatial_bridge_render: f64,
    binaural_render: f64,
    output_conversion_wav_write: f64,
}

#[derive(Serialize)]
struct Eac3DecodeStages {
    total: f64,
    syncframe_and_header_parsing: f64,
    audio_block_syntax_and_exponents: f64,
    bit_allocation: f64,
    mantissa_unpack_and_dequantization: f64,
    coupling_rematrix_and_spx: f64,
    inverse_transform: f64,
    window_and_overlap_add: f64,
    pcm_assembly: f64,
    allocation_and_copy: f64,
    decoder_state_commit: f64,
    other: f64,
}

#[derive(Serialize)]
struct Eac3DecodeWorkload {
    syncframes: u64,
    audio_blocks: u64,
    full_bandwidth_channel_blocks: u64,
    lfe_blocks: u64,
    long_transforms: u64,
    short_transforms: u64,
    aht_elements: u64,
    coupling_blocks: u64,
    spx_blocks: u64,
}

#[derive(Serialize)]
struct Eac3SlowFrameReport {
    frame_index: usize,
    duration_ms: f64,
    stages_ms: Eac3DecodeStages,
    workload: Eac3DecodeWorkload,
}

#[derive(Serialize)]
struct JocReconstructionStages {
    payload_parsing: f64,
    coefficient_decode: f64,
    dequantization: f64,
    qmf_analysis: f64,
    interpolation: f64,
    matrix_reconstruction: f64,
    qmf_synthesis: f64,
    output_assembly: f64,
    buffer_initialization: f64,
}

#[derive(Serialize)]
struct FrameTimingDistribution {
    count: usize,
    p50: f64,
    p95: f64,
    p99: f64,
    maximum: f64,
}

#[derive(Serialize)]
struct ProgressReport {
    enabled: bool,
    updates: u64,
    overhead_ms: f64,
}

pub(crate) fn write_report(
    path: &Path,
    performance: &RenderPerformance,
    layout: &str,
    selected_profile: openjoc_emdf::JocValidationProfile,
    _output_format: openjoc_wave::SampleFormat,
    overwrite: bool,
) -> Result<(), io::Error> {
    if path.exists() && !overwrite {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "refusing to overwrite performance report {}",
                path.display()
            ),
        ));
    }
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let sample_rate = performance.sample_rate_hz.unwrap_or(0);
    let wall = performance.started_at.elapsed().as_secs_f64();
    let audio_duration = if sample_rate == 0 {
        0.0
    } else {
        performance.total_audio_samples as f64 / f64::from(sample_rate)
    };
    let report = PerformanceReport {
        schema: PERFORMANCE_REPORT_SCHEMA,
        openjoc_version: env!("CARGO_PKG_VERSION"),
        selected_layout: layout,
        selected_validation_profile: selected_profile.as_str(),
        sample_rate_hz: sample_rate,
        processed_access_units: performance.processed_access_units,
        rendered_frames: performance.rendered_frames,
        rendered_samples: performance.total_audio_samples,
        output_frames: performance.output_frames,
        audio_duration_seconds: audio_duration,
        wall_duration_seconds: wall,
        realtime_factor: if wall > 0.0 {
            audio_duration / wall
        } else {
            0.0
        },
        output_bytes: performance.output_bytes,
        build_mode: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        stage_timings_ms: StageTimings {
            input_container: milliseconds(performance.input_container),
            profile_validation: milliseconds(performance.profile_validation),
            eac3_decode: milliseconds(performance.eac3_decode),
            joc_reconstruction: milliseconds(performance.joc_reconstruction),
            bridge_control_assembly: milliseconds(performance.bridge_control_assembly),
            spatial_bridge_render: milliseconds(performance.spatial_bridge_render),
            binaural_render: milliseconds(performance.binaural_render),
            output_conversion_wav_write: milliseconds(performance.output_conversion_wav_write),
        },
        eac3_decode_stages_ms: eac3_decode_stages(&performance.eac3_stages),
        eac3_decode_workload: eac3_decode_workload(&performance.eac3_stages),
        eac3_decode_frame_ms: timing_distribution(&performance.eac3_frame_times),
        eac3_slowest_frames: performance
            .slowest_eac3_frames
            .iter()
            .map(|frame| Eac3SlowFrameReport {
                frame_index: frame.frame_index,
                duration_ms: milliseconds(frame.duration),
                stages_ms: eac3_decode_stages(&frame.stages),
                workload: eac3_decode_workload(&frame.stages),
            })
            .collect(),
        joc_reconstruction_stages_ms: JocReconstructionStages {
            payload_parsing: milliseconds(performance.reconstruction_stages.payload_parsing),
            coefficient_decode: milliseconds(performance.reconstruction_stages.coefficient_decode),
            dequantization: milliseconds(performance.reconstruction_stages.dequantization),
            qmf_analysis: milliseconds(performance.reconstruction_stages.qmf_analysis),
            interpolation: milliseconds(performance.reconstruction_stages.interpolation),
            matrix_reconstruction: milliseconds(
                performance.reconstruction_stages.matrix_reconstruction,
            ),
            qmf_synthesis: milliseconds(performance.reconstruction_stages.qmf_synthesis),
            output_assembly: milliseconds(performance.reconstruction_stages.output_assembly),
            buffer_initialization: milliseconds(
                performance.reconstruction_stages.buffer_initialization,
            ),
        },
        core_frame_processing_ms: timing_distribution(&performance.frame_times),
        progress: ProgressReport {
            enabled: performance.progress_enabled,
            updates: performance.progress_updates,
            overhead_ms: milliseconds(performance.progress_overhead),
        },
    };
    let bytes = serde_json::to_vec_pretty(&report)
        .map_err(|error| io::Error::other(format!("serialize performance report: {error}")))?;
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "report has no filename"))?
        .to_string_lossy();
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let staging = parent.join(format!(".{name}.openjoc-report-partial"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staging)?;
    if let Err(error) = file.write_all(&bytes).and_then(|()| file.sync_all()) {
        let _ = fs::remove_file(&staging);
        return Err(error);
    }
    drop(file);
    let result = if overwrite {
        crate::joc_render::replace_existing_file(&staging, path)
    } else if path.exists() {
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "refusing to overwrite performance report {}",
                path.display()
            ),
        ))
    } else {
        fs::rename(&staging, path)
    };
    if let Err(error) = result {
        let _ = fs::remove_file(&staging);
        return Err(error);
    }
    Ok(())
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn eac3_decode_stages(timing: &Eac3DecodeStageTiming) -> Eac3DecodeStages {
    let attributed = timing.syncframe_and_header_parsing
        + timing.audio_block_syntax_and_exponents
        + timing.bit_allocation
        + timing.mantissa_unpack_and_dequantization
        + timing.coupling_rematrix_and_spx
        + timing.inverse_transform
        + timing.window_and_overlap_add
        + timing.pcm_assembly
        + timing.allocation_and_copy
        + timing.decoder_state_commit;
    Eac3DecodeStages {
        total: milliseconds(timing.total),
        syncframe_and_header_parsing: milliseconds(timing.syncframe_and_header_parsing),
        audio_block_syntax_and_exponents: milliseconds(timing.audio_block_syntax_and_exponents),
        bit_allocation: milliseconds(timing.bit_allocation),
        mantissa_unpack_and_dequantization: milliseconds(timing.mantissa_unpack_and_dequantization),
        coupling_rematrix_and_spx: milliseconds(timing.coupling_rematrix_and_spx),
        inverse_transform: milliseconds(timing.inverse_transform),
        window_and_overlap_add: milliseconds(timing.window_and_overlap_add),
        pcm_assembly: milliseconds(timing.pcm_assembly),
        allocation_and_copy: milliseconds(timing.allocation_and_copy),
        decoder_state_commit: milliseconds(timing.decoder_state_commit),
        other: milliseconds(timing.total.saturating_sub(attributed)),
    }
}

fn eac3_decode_workload(timing: &Eac3DecodeStageTiming) -> Eac3DecodeWorkload {
    Eac3DecodeWorkload {
        syncframes: timing.syncframes,
        audio_blocks: timing.audio_blocks,
        full_bandwidth_channel_blocks: timing.full_bandwidth_channel_blocks,
        lfe_blocks: timing.lfe_blocks,
        long_transforms: timing.long_transforms,
        short_transforms: timing.short_transforms,
        aht_elements: timing.aht_elements,
        coupling_blocks: timing.coupling_blocks,
        spx_blocks: timing.spx_blocks,
    }
}

fn timing_distribution(timings: &[Duration]) -> FrameTimingDistribution {
    if timings.is_empty() {
        return FrameTimingDistribution {
            count: 0,
            p50: 0.0,
            p95: 0.0,
            p99: 0.0,
            maximum: 0.0,
        };
    }
    let mut values = timings
        .iter()
        .map(|value| milliseconds(*value))
        .collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    FrameTimingDistribution {
        count: values.len(),
        p50: percentile(&values, 50),
        p95: percentile(&values, 95),
        p99: percentile(&values, 99),
        maximum: values.last().copied().unwrap_or(0.0),
    }
}

fn percentile(values: &[f64], percentile: u32) -> f64 {
    let maximum = values.len() - 1;
    let index = (u128::try_from(maximum).unwrap_or(u128::MAX) * u128::from(percentile) + 50) / 100;
    let index = usize::try_from(index).unwrap_or(maximum).min(maximum);
    values[index]
}

#[cfg(test)]
mod tests {
    use super::{DecodeStageTiming, RenderPerformance, write_report};
    use openjoc_eac3::Eac3DecodeStageTiming;
    use openjoc_emdf::JocValidationProfile;
    use openjoc_wave::SampleFormat;
    use std::{
        fs,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    fn report_path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "openjoc-performance-report-{label}-{}-{}.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ))
    }

    #[test]
    fn authorized_report_overwrite_replaces_only_the_final_file() {
        let path = report_path("overwrite");
        fs::write(&path, b"previous report").expect("old report");
        let mut performance = RenderPerformance::new();
        performance.sample_rate_hz = Some(48_000);
        performance.total_audio_samples = 48_000;
        let mut decode = DecodeStageTiming::new(true);
        for frame_index in 0..20 {
            let duration = Duration::from_millis(frame_index as u64 + 1);
            decode.eac3_decode += duration;
            decode.record_eac3_frame(
                frame_index,
                duration,
                Eac3DecodeStageTiming {
                    total: duration,
                    syncframes: 1,
                    audio_blocks: 6,
                    long_transforms: 6,
                    ..Eac3DecodeStageTiming::default()
                },
            );
        }
        performance.merge_decode(decode);
        write_report(
            &path,
            &performance,
            "7.1.4",
            JocValidationProfile::EtsiStrict,
            SampleFormat::F32,
            true,
        )
        .expect("overwrite report");
        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).expect("new report")).expect("JSON report");
        assert_eq!(report["schema"], "openjoc.joc-render-performance.v1");
        assert_eq!(report["selected_layout"], "7.1.4");
        assert_eq!(report["eac3_decode_workload"]["syncframes"], 20);
        assert_eq!(report["eac3_decode_workload"]["audio_blocks"], 120);
        assert_eq!(report["eac3_decode_frame_ms"]["count"], 20);
        assert_eq!(report["eac3_slowest_frames"].as_array().unwrap().len(), 16);
        assert_eq!(report["eac3_slowest_frames"][0]["frame_index"], 19);
        assert_eq!(
            report["eac3_slowest_frames"][0]["workload"]["syncframes"],
            1
        );
        assert_eq!(report["eac3_slowest_frames"][15]["frame_index"], 4);
        assert!(
            !path
                .with_file_name(format!(
                    ".{}.openjoc-report-partial",
                    path.file_name().unwrap().to_string_lossy()
                ))
                .exists()
        );
        fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn unauthorized_report_collision_preserves_existing_report() {
        let path = report_path("collision");
        fs::write(&path, b"previous report").expect("old report");
        let error = write_report(
            &path,
            &RenderPerformance::new(),
            "7.1.4",
            JocValidationProfile::EtsiStrict,
            SampleFormat::F32,
            false,
        )
        .expect_err("collision");
        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(
            fs::read(&path).expect("old report remains"),
            b"previous report"
        );
        fs::remove_file(path).expect("cleanup");
    }
}
