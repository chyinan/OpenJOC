use serde::Serialize;
use std::{
    fs, io,
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
}

impl DecodeStageTiming {
    pub(crate) fn new(collect_frame_times: bool) -> Self {
        Self {
            eac3_decode: Duration::ZERO,
            joc_reconstruction: Duration::ZERO,
            frame_times: Vec::new(),
            collect_frame_times,
        }
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
        }
    }

    pub(crate) fn merge_decode(&mut self, timing: DecodeStageTiming) {
        self.eac3_decode += timing.eac3_decode;
        self.joc_reconstruction += timing.joc_reconstruction;
        self.frame_times = timing.frame_times;
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
) -> Result<(), io::Error> {
    if path.exists() {
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
        core_frame_processing_ms: timing_distribution(&performance.frame_times),
        progress: ProgressReport {
            enabled: performance.progress_enabled,
            updates: performance.progress_updates,
            overhead_ms: milliseconds(performance.progress_overhead),
        },
    };
    fs::write(
        path,
        serde_json::to_vec_pretty(&report)
            .map_err(|error| io::Error::other(format!("serialize performance report: {error}")))?,
    )
}

fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
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
