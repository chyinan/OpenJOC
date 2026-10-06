//! Bit-exact API probe used by `scripts/verify_pcm_bitexact.py`.
//!
//! Usage: pcm_regression_probe INPUT MODE LAYOUT OUTPUT_PREFIX [--stage|--alloc] [--timing-only]
//! Modes: speaker, stereo, binaural-d1, binaural-d2, orientation-d1, orientation-d2,
//!        pull-d1, pull-d2. Output PCM is interleaved f32 little-endian.
//!
//! Timing-only mode black-boxes and drops returned PCM inside the measured
//! push/receive call; PCM serialization, hashing, and report formatting stay
//! outside that scope.

#![allow(unsafe_code)]

use openjoc_api::{
    BinauralConfig, BuiltinHrtf, ListenerOrientation, OpenJocConfig, OpenJocPacket,
    OpenJocPcmFrame, OpenJocSession, OpenJocStatus, RenderMode, ValidationProfile,
};
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fs::{self, File},
    hint::black_box,
    io::{BufWriter, Write},
    path::PathBuf,
    time::Instant,
};

#[cfg(feature = "allocation-profile")]
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

#[derive(Clone, Copy, Debug, Default)]
struct AllocationCounts {
    alloc_calls: u64,
    realloc_calls: u64,
    dealloc_calls: u64,
    requested_bytes: u64,
}

#[cfg(feature = "allocation-profile")]
thread_local! {
    static ACTIVE_ALLOCATION_COUNTS: Cell<Option<AllocationCounts>> = const { Cell::new(None) };
}

#[cfg(feature = "allocation-profile")]
struct CountingAllocator;

#[cfg(feature = "allocation-profile")]
// SAFETY: Each operation delegates to System with the original pointer/layout.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        update_allocation_counts(|counts| {
            counts.alloc_calls += 1;
            counts.requested_bytes = counts.requested_bytes.saturating_add(layout.size() as u64);
        });
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        update_allocation_counts(|counts| {
            counts.alloc_calls += 1;
            counts.requested_bytes = counts.requested_bytes.saturating_add(layout.size() as u64);
        });
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        update_allocation_counts(|counts| counts.dealloc_calls += 1);
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        update_allocation_counts(|counts| {
            counts.realloc_calls += 1;
            counts.requested_bytes = counts.requested_bytes.saturating_add(new_size as u64);
        });
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[cfg(feature = "allocation-profile")]
#[global_allocator]
static COUNTING_ALLOCATOR: CountingAllocator = CountingAllocator;

#[cfg(feature = "allocation-profile")]
fn update_allocation_counts(update: impl FnOnce(&mut AllocationCounts)) {
    ACTIVE_ALLOCATION_COUNTS.with(|cell| {
        if let Some(mut counts) = cell.get() {
            update(&mut counts);
            cell.set(Some(counts));
        }
    });
}

#[cfg(feature = "allocation-profile")]
fn allocation_begin() {
    ACTIVE_ALLOCATION_COUNTS.with(|cell| cell.set(Some(AllocationCounts::default())));
}

#[cfg(feature = "allocation-profile")]
fn allocation_end() -> AllocationCounts {
    ACTIVE_ALLOCATION_COUNTS.with(|cell| cell.replace(None).unwrap_or_default())
}

#[derive(Clone, Copy, Debug)]
enum ProbeMode {
    Speaker,
    Stereo,
    Binaural(BuiltinHrtf),
    Orientation(BuiltinHrtf),
    Pull(BuiltinHrtf),
}

impl ProbeMode {
    fn parse(value: &str) -> Result<Self, Box<dyn Error>> {
        match value {
            "speaker" => Ok(Self::Speaker),
            "stereo" => Ok(Self::Stereo),
            "binaural-d1" => Ok(Self::Binaural(BuiltinHrtf::SadieD1Ku100)),
            "binaural-d2" => Ok(Self::Binaural(BuiltinHrtf::SadieD2Kemar)),
            "orientation-d1" => Ok(Self::Orientation(BuiltinHrtf::SadieD1Ku100)),
            "orientation-d2" => Ok(Self::Orientation(BuiltinHrtf::SadieD2Kemar)),
            "pull-d1" => Ok(Self::Pull(BuiltinHrtf::SadieD1Ku100)),
            "pull-d2" => Ok(Self::Pull(BuiltinHrtf::SadieD2Kemar)),
            _ => Err(format!("unknown probe mode {value:?}").into()),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Speaker => "speaker",
            Self::Stereo => "stereo",
            Self::Binaural(BuiltinHrtf::SadieD1Ku100) => "binaural-d1",
            Self::Binaural(BuiltinHrtf::SadieD2Kemar) => "binaural-d2",
            Self::Orientation(BuiltinHrtf::SadieD1Ku100) => "orientation-d1",
            Self::Orientation(BuiltinHrtf::SadieD2Kemar) => "orientation-d2",
            Self::Pull(BuiltinHrtf::SadieD1Ku100) => "pull-d1",
            Self::Pull(BuiltinHrtf::SadieD2Kemar) => "pull-d2",
        }
    }

    fn binaural(self) -> Option<BuiltinHrtf> {
        match self {
            Self::Speaker | Self::Stereo => None,
            Self::Binaural(preset) | Self::Orientation(preset) | Self::Pull(preset) => Some(preset),
        }
    }

    fn orientation(self) -> bool {
        matches!(self, Self::Orientation(_) | Self::Pull(_))
    }

    fn pull(self) -> bool {
        matches!(self, Self::Pull(_))
    }
}

#[derive(Default)]
struct ChannelAudit {
    fnv64: u64,
    nonzero: u64,
    finite: u64,
    nonfinite: u64,
    min: f32,
    max: f32,
    initialized: bool,
}

impl ChannelAudit {
    fn update(&mut self, value: f32) {
        if self.initialized {
            self.min = self.min.min(value);
            self.max = self.max.max(value);
        } else {
            self.min = value;
            self.max = value;
            self.initialized = true;
        }
        if value == 0.0 {
            // +0 and -0 both count as zero; their exact bits remain in PCM.
        } else {
            self.nonzero += 1;
        }
        if value.is_finite() {
            self.finite += 1;
        } else {
            self.nonfinite += 1;
        }
        if self.fnv64 == 0 {
            self.fnv64 = 0xcbf29ce484222325;
        }
        for byte in value.to_bits().to_le_bytes() {
            self.fnv64 ^= u64::from(byte);
            self.fnv64 = self.fnv64.wrapping_mul(0x100000001b3);
        }
    }
}

struct Reports {
    pcm: Option<BufWriter<File>>,
    manifest: Option<BufWriter<File>>,
    timing: BufWriter<File>,
    timing_only: bool,
    pcm_bytes: u64,
    output_frames: u64,
    sample_count: u64,
    tail_samples: u64,
    channels: Option<Vec<ChannelAudit>>,
}

impl Reports {
    fn new(prefix: &str, timing_only: bool) -> Result<Self, Box<dyn Error>> {
        let prefix = PathBuf::from(prefix);
        if let Some(parent) = prefix.parent() {
            fs::create_dir_all(parent)?;
        }
        let timing_path = PathBuf::from(format!("{}.timing.tsv", prefix.display()));
        Ok(Self {
            pcm: if timing_only {
                None
            } else {
                Some(BufWriter::with_capacity(
                    1024 * 1024,
                    File::create(PathBuf::from(format!("{}.pcm32le", prefix.display())))?,
                ))
            },
            manifest: if timing_only {
                None
            } else {
                Some(BufWriter::new(File::create(PathBuf::from(format!(
                    "{}.manifest.tsv",
                    prefix.display()
                )))?))
            },
            timing: BufWriter::new(File::create(timing_path)?),
            timing_only,
            pcm_bytes: 0,
            output_frames: 0,
            sample_count: 0,
            tail_samples: 0,
            channels: None,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn write_header(
        &mut self,
        input_sha256: &str,
        config_descriptor: &str,
        config_fingerprint: &str,
        latency_samples: usize,
        mode: ProbeMode,
        layout: &str,
        input_units: &[InputUnit],
    ) -> Result<(), Box<dyn Error>> {
        if let Some(manifest) = self.manifest.as_mut() {
            writeln!(manifest, "openjoc-pcm-probe\t1")?;
            writeln!(manifest, "H\tinput_sha256\t{input_sha256}")?;
            writeln!(manifest, "H\tmode\t{}", mode.name())?;
            writeln!(manifest, "H\tlayout\t{}", hex(layout.as_bytes()))?;
            writeln!(manifest, "H\tlatency_samples\t{latency_samples}")?;
            writeln!(manifest, "H\tconfig_fingerprint\t{config_fingerprint}")?;
            writeln!(
                manifest,
                "H\tconfig_descriptor_hex\t{}",
                hex(config_descriptor.as_bytes())
            )?;
            writeln!(manifest, "H\tinput_unit_count\t{}", input_units.len())?;
            for (index, unit) in input_units.iter().enumerate() {
                writeln!(
                    manifest,
                    "AU\t{index}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    unit.start,
                    unit.end - unit.start,
                    unit.samples,
                    unit.sample_rate,
                    index * usize::from(unit.samples),
                    unit.independent_count,
                    unit.dependent_count,
                )?;
            }
        }
        writeln!(
            self.timing,
            "record\tindex\ttotal_ns\tdecode_ns\trender_ns\tbinaural_ns\torientation_prepare_ns\talloc_calls\trealloc_calls\tdealloc_calls\trequested_alloc_bytes"
        )?;
        Ok(())
    }

    fn write_frame(&mut self, frame: OpenJocPcmFrame, phase: &str) -> Result<(), Box<dyn Error>> {
        if self.timing_only {
            return Err(
                "timing-only mode must consume and drop frames within the timed call".into(),
            );
        }
        let manifest = self
            .manifest
            .as_mut()
            .ok_or("manifest writer unavailable")?;
        let pcm = self.pcm.as_mut().ok_or("PCM writer unavailable")?;
        if frame.interleaved_f32.len() != frame.sample_count * frame.channel_count {
            return Err("PCM frame dimensions do not match interleaved samples".into());
        }
        let channel_audit = self.channels.get_or_insert_with(|| {
            (0..frame.channel_count)
                .map(|_| ChannelAudit::default())
                .collect()
        });
        if channel_audit.len() != frame.channel_count {
            return Err("output channel count changed within one probe run".into());
        }
        let start_offset = self.pcm_bytes;
        let labels = frame
            .channel_labels
            .iter()
            .map(|label| hex(label.as_bytes()))
            .collect::<Vec<_>>()
            .join(",");
        writeln!(
            manifest,
            "F\t{}\t{phase}\t{}\t{}\t{}\t{}\t{}\t{}\t{:?}\t{}\t{}",
            self.output_frames,
            frame.sample_rate,
            frame.channel_count,
            frame.sample_count,
            frame
                .pts_samples
                .map_or_else(|| "none".to_owned(), |pts| pts.to_string()),
            hex(frame.layout_name.as_bytes()),
            labels,
            frame.render_mode,
            frame.sample_format as u8,
            start_offset,
        )?;
        for (index, sample) in frame.interleaved_f32.iter().copied().enumerate() {
            channel_audit[index % frame.channel_count].update(sample);
            pcm.write_all(&sample.to_bits().to_le_bytes())?;
        }
        let bytes = u64::try_from(frame.interleaved_f32.len() * 4)?;
        self.pcm_bytes = self
            .pcm_bytes
            .checked_add(bytes)
            .ok_or("PCM byte count overflow")?;
        self.sample_count = self
            .sample_count
            .checked_add(u64::try_from(frame.sample_count)?)
            .ok_or("sample count overflow")?;
        if phase == "drain" {
            self.tail_samples = self
                .tail_samples
                .checked_add(u64::try_from(frame.sample_count)?)
                .ok_or("tail sample count overflow")?;
        }
        self.output_frames += 1;
        drop(frame);
        Ok(())
    }

    fn finish(
        &mut self,
        input_units: usize,
        diagnostics: &str,
        terminal_state: &str,
    ) -> Result<(), Box<dyn Error>> {
        if self.timing_only {
            self.timing.flush()?;
            return Ok(());
        }
        self.pcm.as_mut().ok_or("PCM writer unavailable")?.flush()?;
        let manifest = self
            .manifest
            .as_mut()
            .ok_or("manifest writer unavailable")?;
        for (index, audit) in self
            .channels
            .as_deref()
            .unwrap_or_default()
            .iter()
            .enumerate()
        {
            writeln!(
                manifest,
                "C\t{index}\t{}\t{}\t{}\t{}\t{:08x}\t{:08x}\t{:016x}",
                audit.nonzero,
                audit.finite,
                audit.nonfinite,
                audit.initialized,
                audit.min.to_bits(),
                audit.max.to_bits(),
                audit.fnv64,
            )?;
        }
        let distinct_channel_fingerprints = self
            .channels
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|channel| channel.fnv64)
            .collect::<std::collections::HashSet<_>>()
            .len();
        writeln!(manifest, "R\tinput_units\t{input_units}")?;
        writeln!(manifest, "R\toutput_frames\t{}", self.output_frames)?;
        writeln!(manifest, "R\tpcm_bytes\t{}", self.pcm_bytes)?;
        writeln!(manifest, "R\tsample_count\t{}", self.sample_count)?;
        writeln!(manifest, "R\ttail_samples\t{}", self.tail_samples)?;
        writeln!(
            manifest,
            "R\tchannel_count\t{}",
            self.channels.as_deref().map_or(0, <[_]>::len)
        )?;
        writeln!(
            manifest,
            "R\tdistinct_channel_fingerprints\t{distinct_channel_fingerprints}"
        )?;
        writeln!(
            manifest,
            "R\tdiagnostics_hex\t{}",
            hex(diagnostics.as_bytes())
        )?;
        writeln!(manifest, "R\tterminal_state\t{terminal_state}")?;
        manifest.flush()?;
        self.timing.flush()?;
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct InputUnit {
    start: usize,
    end: usize,
    samples: u16,
    sample_rate: u32,
    independent_count: usize,
    dependent_count: usize,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() < 4 {
        return Err("usage: pcm_regression_probe INPUT MODE LAYOUT OUTPUT_PREFIX [--stage|--alloc] [--timing-only]".into());
    }
    let input_path = &args[0];
    let mode = ProbeMode::parse(&args[1])?;
    let layout = &args[2];
    let output_prefix = &args[3];
    let stage_timing = args[4..].iter().any(|arg| arg == "--stage");
    let allocation_timing = args[4..].iter().any(|arg| arg == "--alloc");
    let timing_only = args[4..].iter().any(|arg| arg == "--timing-only");
    if stage_timing && allocation_timing {
        return Err("stage timing and allocator counting are separate instrumented runs".into());
    }
    if timing_only && (stage_timing || allocation_timing) {
        return Err(
            "timing-only runs must use the plain allocator without stage instrumentation".into(),
        );
    }
    if allocation_timing && !cfg!(feature = "allocation-profile") {
        return Err("--alloc requires a probe built with allocation-profile".into());
    }

    let input = fs::read(input_path)?;
    let input_hash = sha256_hex(&input);
    let input_units = index_units(&input)?;
    if input_units.is_empty() {
        return Err("input contains no complete access units".into());
    }
    if input_units.iter().any(|unit| unit.sample_rate != 48_000) {
        return Err("probe currently requires 48-kHz JOC access units".into());
    }
    let config = config_for(mode, layout);
    config.validate()?;
    let reports = Reports::new(output_prefix, timing_only)?;
    let init_start = Instant::now();
    let mut session = if mode.pull() {
        OpenJocSession::new_with_listener_orientation_pull(config, 64)?
    } else if mode.orientation() {
        OpenJocSession::new_with_listener_orientation(config)?
    } else {
        OpenJocSession::new(config)?
    };
    let init_ns = init_start.elapsed().as_nanos();
    let warmup_start = Instant::now();
    let warmup_units = input_units.len().min(8);
    warmup(&mut session, mode, &input, &input_units[..warmup_units])?;
    let warmup_ns = warmup_start.elapsed().as_nanos();
    let reset_start = Instant::now();
    session.try_flush()?;
    let reset_after_warmup_ns = reset_start.elapsed().as_nanos();
    if stage_timing {
        session.enable_stage_timing();
    }
    let config_descriptor = session.effective_config_descriptor();
    let config_fingerprint = session.effective_config_fingerprint();
    let output_channel_count = session.output_info().channel_count;
    let mut reports = reports;
    reports.write_header(
        &input_hash,
        &config_descriptor,
        &config_fingerprint,
        session.output_info().latency_samples,
        mode,
        layout,
        &input_units,
    )?;
    let cpu_start = process_cpu_ns();
    let mut frame_buffer = Vec::with_capacity(32);
    let mut call_ns = Vec::with_capacity(input_units.len());
    let mut first_pcm_au = None;
    let mut wall_to_first_pcm_ns = None;
    let mut stage_total = [0_u128; 4];
    let mut orientation_prepare_total_ns = 0_u128;
    let mut last_reported_orientation = 0_u64;
    let mut timing_output_frames = 0_u64;
    let mut timing_output_samples = 0_u64;
    let mut timing_tail_samples = 0_u64;

    for (index, unit) in input_units.iter().enumerate() {
        let orientation_prepare_ns = if mode.orientation() && index > 0 && index % 64 == 0 {
            let epoch = session.listener_orientation_stream_epoch().unwrap_or(0);
            let sequence = u64::try_from(index / 64)?;
            let prepare_start = Instant::now();
            #[cfg(feature = "allocation-profile")]
            if allocation_timing {
                allocation_begin();
            }
            let preparer = session
                .listener_orientation_preparer()
                .ok_or("orientation session did not expose a preparer")?;
            let update = preparer.prepare(trajectory(sequence), epoch, sequence)?;
            let accepted = session
                .apply_prepared_listener_orientation(update)
                .map_err(|failure| failure.error)?;
            drop(accepted.retired_kernels);
            let elapsed = prepare_start.elapsed().as_nanos();
            #[cfg(feature = "allocation-profile")]
            let allocation_counts = if allocation_timing {
                allocation_end()
            } else {
                AllocationCounts::default()
            };
            #[cfg(not(feature = "allocation-profile"))]
            let allocation_counts = AllocationCounts::default();
            orientation_prepare_total_ns += elapsed;
            if let Some(manifest) = reports.manifest.as_mut() {
                writeln!(
                    manifest,
                    "O\t{sequence}\taccepted_before_sample\t{}",
                    index * usize::from(unit.samples)
                )?;
            }
            if !timing_only {
                writeln!(
                    reports.timing,
                    "ORIENT\t{sequence}\t{elapsed}\t{}\t{}\t{}\t{}",
                    allocation_counts.alloc_calls,
                    allocation_counts.realloc_calls,
                    allocation_counts.dealloc_calls,
                    allocation_counts.requested_bytes,
                )?;
            }
            elapsed
        } else {
            0
        };
        let packet = OpenJocPacket {
            data: &input[unit.start..unit.end],
            pts_samples: Some(i64::try_from(index * usize::from(unit.samples))?),
            discontinuity: false,
            preroll: false,
        };
        #[cfg(feature = "allocation-profile")]
        if allocation_timing {
            allocation_begin();
        }
        let start = Instant::now();
        let status = session.push_packet(packet)?;
        if status == OpenJocStatus::OutputPending {
            return Err(format!("output backpressure before input AU {index}").into());
        }
        receive_available(&mut session, mode, &mut frame_buffer)?;
        let has_output = !frame_buffer.is_empty();
        if timing_only {
            for frame in frame_buffer.drain(..) {
                timing_output_frames = timing_output_frames.saturating_add(1);
                timing_output_samples =
                    timing_output_samples.saturating_add(u64::try_from(frame.sample_count)?);
                black_box(frame.interleaved_f32.as_slice());
                black_box(frame);
            }
        }
        let elapsed = start.elapsed().as_nanos();
        #[cfg(feature = "allocation-profile")]
        let allocation_counts = if allocation_timing {
            allocation_end()
        } else {
            AllocationCounts::default()
        };
        #[cfg(not(feature = "allocation-profile"))]
        let allocation_counts = AllocationCounts::default();
        call_ns.push(elapsed);
        if first_pcm_au.is_none() && has_output {
            first_pcm_au = Some(index);
            wall_to_first_pcm_ns = Some(call_ns.iter().copied().sum::<u128>());
        }
        if timing_only {
            // Serialize per-call observations after the processing loop.
        } else if stage_timing {
            let stage = session.take_stage_timing();
            let values = [
                stage.total.as_nanos(),
                stage.decode.as_nanos(),
                stage.render.as_nanos(),
                stage.binaural.as_nanos(),
            ];
            for (target, value) in stage_total.iter_mut().zip(values) {
                *target = target.saturating_add(value);
            }
            writeln!(
                reports.timing,
                "AU\t{index}\t{}\t{}\t{}\t{}\t{orientation_prepare_ns}\t{}\t{}\t{}\t{}",
                elapsed,
                values[1],
                values[2],
                values[3],
                allocation_counts.alloc_calls,
                allocation_counts.realloc_calls,
                allocation_counts.dealloc_calls,
                allocation_counts.requested_bytes,
            )?;
        } else {
            writeln!(
                reports.timing,
                "AU\t{index}\t{elapsed}\t0\t0\t0\t{orientation_prepare_ns}\t{}\t{}\t{}\t{}",
                allocation_counts.alloc_calls,
                allocation_counts.realloc_calls,
                allocation_counts.dealloc_calls,
                allocation_counts.requested_bytes,
            )?;
        }
        for frame in frame_buffer.drain(..) {
            reports.write_frame(frame, "programme")?;
        }
        if let Some(applied) = session.last_applied_listener_orientation()
            && applied.sequence > last_reported_orientation
        {
            if let Some(manifest) = reports.manifest.as_mut() {
                writeln!(
                    manifest,
                    "O\t{}\tapplied\t{}",
                    applied.sequence, applied.logical_start_sample
                )?;
            }
            last_reported_orientation = applied.sequence;
        }
    }

    let drain_start = Instant::now();
    let mut drain_api_ns = 0_u128;
    loop {
        let drain_call_start = Instant::now();
        let status = session.drain()?;
        receive_available(&mut session, mode, &mut frame_buffer)?;
        if timing_only {
            for frame in frame_buffer.drain(..) {
                timing_output_frames = timing_output_frames.saturating_add(1);
                let frame_samples = u64::try_from(frame.sample_count)?;
                timing_output_samples = timing_output_samples.saturating_add(frame_samples);
                timing_tail_samples = timing_tail_samples.saturating_add(frame_samples);
                black_box(frame.interleaved_f32.as_slice());
                black_box(frame);
            }
        }
        drain_api_ns = drain_api_ns.saturating_add(drain_call_start.elapsed().as_nanos());
        for frame in frame_buffer.drain(..) {
            reports.write_frame(frame, "drain")?;
        }
        if session.is_drained() {
            if status != OpenJocStatus::EndOfStream {
                // Once all output is consumed, repeat drain to prove terminal state.
                continue;
            }
            break;
        }
        if status == OpenJocStatus::EndOfStream {
            return Err("drain reported EOS before terminal state".into());
        }
    }
    let drain_loop_wall_ns = drain_start.elapsed().as_nanos();
    let repeated_drain = session.drain()?;
    if repeated_drain != OpenJocStatus::EndOfStream || !session.is_drained() {
        return Err("repeated drain did not preserve the terminal state".into());
    }
    let cpu_ns = cpu_start
        .zip(process_cpu_ns())
        .map(|(start, end)| end.saturating_sub(start));
    let wall_call_sum: u128 = call_ns.iter().copied().sum();
    let processing_ns = wall_call_sum.saturating_add(drain_api_ns);
    let program_samples = input_units
        .iter()
        .map(|unit| u64::from(unit.samples))
        .sum::<u64>();
    let output_samples = if timing_only {
        timing_output_samples
    } else {
        reports.sample_count
    };
    let output_frames = if timing_only {
        timing_output_frames
    } else {
        reports.output_frames
    };
    let tail_samples = if timing_only {
        timing_tail_samples
    } else {
        reports.tail_samples
    };
    let audio_seconds = program_samples as f64 / 48_000.0;
    let rtf = processing_ns as f64 / 1_000_000_000.0 / audio_seconds.max(f64::MIN_POSITIVE);
    let realtime_speed = 1.0 / rtf.max(f64::MIN_POSITIVE);
    let stats = distribution(&call_ns);
    let steady_stats = distribution(call_ns.get(1..).unwrap_or_default());
    if timing_only {
        for (index, elapsed) in call_ns.iter().copied().enumerate() {
            writeln!(
                reports.timing,
                "AU\t{index}\t{elapsed}\t0\t0\t0\t0\t0\t0\t0\t0"
            )?;
        }
    }
    writeln!(reports.timing, "SUMMARY\twarmup_ns\t{warmup_ns}")?;
    writeln!(
        reports.timing,
        "SUMMARY\treset_after_warmup_ns\t{reset_after_warmup_ns}"
    )?;
    writeln!(reports.timing, "SUMMARY\tinit_ns\t{init_ns}")?;
    writeln!(
        reports.timing,
        "SUMMARY\tfirst_pcm_output_after_au\t{}",
        first_pcm_au.map_or_else(|| "none".to_owned(), |n| n.to_string())
    )?;
    writeln!(
        reports.timing,
        "SUMMARY\tapi_time_to_first_pcm_ns\t{}",
        wall_to_first_pcm_ns.map_or_else(|| "unavailable".to_owned(), |n| n.to_string())
    )?;
    writeln!(
        reports.timing,
        "SUMMARY\tfirst_pcm_including_init_ns\t{}",
        wall_to_first_pcm_ns.map_or_else(
            || "unavailable".to_owned(),
            |n| n.saturating_add(init_ns).to_string()
        )
    )?;
    writeln!(
        reports.timing,
        "SUMMARY\tdrain_loop_wall_including_output_write_ns\t{drain_loop_wall_ns}"
    )?;
    writeln!(
        reports.timing,
        "SUMMARY\tapi_pipeline_wall_ns\t{processing_ns}"
    )?;
    writeln!(
        reports.timing,
        "SUMMARY\tapi_pipeline_plus_init_ns\t{}",
        processing_ns.saturating_add(init_ns)
    )?;
    let orientation_inclusive_ns = processing_ns.saturating_add(orientation_prepare_total_ns);
    writeln!(
        reports.timing,
        "SUMMARY\tapi_pipeline_plus_orientation_control_ns\t{orientation_inclusive_ns}"
    )?;
    writeln!(
        reports.timing,
        "SUMMARY\torientation_inclusive_rtf_lower_is_better\t{:.9}",
        orientation_inclusive_ns as f64 / 1_000_000_000.0 / audio_seconds.max(f64::MIN_POSITIVE)
    )?;
    writeln!(reports.timing, "SUMMARY\ttiming_only\t{timing_only}")?;
    writeln!(
        reports.timing,
        "SUMMARY\tinput_access_units\t{}",
        input_units.len()
    )?;
    writeln!(reports.timing, "SUMMARY\toutput_frames\t{output_frames}")?;
    writeln!(
        reports.timing,
        "SUMMARY\toutput_sample_count\t{output_samples}"
    )?;
    writeln!(reports.timing, "SUMMARY\ttail_samples\t{tail_samples}")?;
    writeln!(
        reports.timing,
        "SUMMARY\tchannel_count\t{output_channel_count}"
    )?;
    writeln!(reports.timing, "SUMMARY\tdrain_api_wall_ns\t{drain_api_ns}")?;
    writeln!(
        reports.timing,
        "SUMMARY\tpush_receive_sum_ns\t{wall_call_sum}"
    )?;
    writeln!(
        reports.timing,
        "SUMMARY\tprobe_process_cpu_ns\t{}",
        cpu_ns.map_or_else(|| "unavailable".to_owned(), |n| n.to_string())
    )?;
    writeln!(
        reports.timing,
        "SUMMARY\tprocessing_rtf_excludes_orientation_control_lower_is_better\t{rtf:.9}"
    )?;
    writeln!(
        reports.timing,
        "SUMMARY\trealtime_speed_higher_is_better\t{realtime_speed:.9}"
    )?;
    writeln!(
        reports.timing,
        "SUMMARY\tns_per_au\t{:.3}",
        wall_call_sum as f64 / call_ns.len() as f64
    )?;
    writeln!(
        reports.timing,
        "SUMMARY\tns_per_scalar_sample\t{:.6}",
        processing_ns as f64 / (output_samples as f64 * output_channel_count as f64)
    )?;
    writeln!(reports.timing, "SUMMARY\tcall_p50_ns\t{}", stats.0)?;
    writeln!(reports.timing, "SUMMARY\tcall_p95_ns\t{}", stats.1)?;
    writeln!(reports.timing, "SUMMARY\tcall_p99_ns\t{}", stats.2)?;
    writeln!(reports.timing, "SUMMARY\tcall_max_ns\t{}", stats.3)?;
    writeln!(
        reports.timing,
        "SUMMARY\tsteady_call_p50_ns_excludes_first_au\t{}",
        steady_stats.0
    )?;
    writeln!(
        reports.timing,
        "SUMMARY\tsteady_call_p95_ns_excludes_first_au\t{}",
        steady_stats.1
    )?;
    writeln!(
        reports.timing,
        "SUMMARY\tsteady_call_p99_ns_excludes_first_au\t{}",
        steady_stats.2
    )?;
    writeln!(
        reports.timing,
        "SUMMARY\tsteady_call_max_ns_excludes_first_au\t{}",
        steady_stats.3
    )?;
    writeln!(
        reports.timing,
        "SUMMARY\tprogram_audio_seconds\t{audio_seconds:.6}"
    )?;
    writeln!(
        reports.timing,
        "SUMMARY\torientation_prepare_total_ns\t{orientation_prepare_total_ns}"
    )?;
    writeln!(reports.timing, "SUMMARY\tinput_sha256\t{input_hash}")?;
    writeln!(
        reports.timing,
        "SUMMARY\tconfig_fingerprint\t{config_fingerprint}"
    )?;
    writeln!(
        reports.timing,
        "SUMMARY\tconfig_descriptor_hex\t{}",
        hex(config_descriptor.as_bytes())
    )?;
    writeln!(
        reports.timing,
        "SUMMARY\tlatency_samples\t{}",
        session.output_info().latency_samples
    )?;
    writeln!(reports.timing, "SUMMARY\tmode\t{}", mode.name())?;
    writeln!(
        reports.timing,
        "SUMMARY\tlayout\t{}",
        hex(layout.as_bytes())
    )?;
    if stage_timing {
        writeln!(
            reports.timing,
            "STAGE\tapi_total_sum_ns\t{}",
            stage_total[0]
        )?;
        writeln!(reports.timing, "STAGE\tdecode_sum_ns\t{}", stage_total[1])?;
        writeln!(reports.timing, "STAGE\trender_sum_ns\t{}", stage_total[2])?;
        writeln!(reports.timing, "STAGE\tbinaural_sum_ns\t{}", stage_total[3])?;
    }
    let diagnostics = format!(
        "profile={:?};objects={:?};complexity={:?};downmix={:?}",
        session.diagnostics().profile,
        session.diagnostics().object_count,
        session.diagnostics().complexity_index,
        session.diagnostics().downmix_index,
    );
    reports.finish(
        input_units.len(),
        &diagnostics,
        "drained-repeated-drain-eos",
    )?;
    eprintln!(
        "mode={} layout={} input_aus={} output_frames={} output_samples={} channels={} init_ms={:.3} warmup_ms={:.3} api_pipeline_ms={:.3} orientation_control_ms={:.3} drain_loop_ms={:.3} processing_rtf={:.6}",
        mode.name(),
        layout,
        input_units.len(),
        output_frames,
        output_samples,
        output_channel_count,
        init_ns as f64 / 1e6,
        warmup_ns as f64 / 1e6,
        processing_ns as f64 / 1e6,
        orientation_prepare_total_ns as f64 / 1e6,
        drain_loop_wall_ns as f64 / 1e6,
        rtf,
    );
    Ok(())
}

fn warmup(
    session: &mut OpenJocSession,
    mode: ProbeMode,
    input: &[u8],
    units: &[InputUnit],
) -> Result<(), Box<dyn Error>> {
    let mut frames = Vec::with_capacity(32);
    for (index, unit) in units.iter().enumerate() {
        let status = session.push_packet(OpenJocPacket {
            data: &input[unit.start..unit.end],
            pts_samples: Some(i64::try_from(index * usize::from(unit.samples))?),
            discontinuity: false,
            preroll: false,
        })?;
        if status == OpenJocStatus::OutputPending {
            return Err("warmup unexpectedly encountered output backpressure".into());
        }
        receive_available(session, mode, &mut frames)?;
        frames.clear();
    }
    loop {
        let status = session.drain()?;
        receive_available(session, mode, &mut frames)?;
        frames.clear();
        if session.is_drained() {
            if status == OpenJocStatus::EndOfStream {
                break;
            }
            continue;
        }
        if status == OpenJocStatus::EndOfStream {
            return Err("warmup reported EOS before terminal state".into());
        }
    }
    Ok(())
}

fn config_for(mode: ProbeMode, layout: &str) -> OpenJocConfig {
    let mut config = OpenJocConfig {
        // Keep `stereo` explicit for LAV parity. It uses the API's Stereo
        // mode even though the `speaker,2.0` probe remains available.
        render_mode: if matches!(mode, ProbeMode::Stereo) {
            RenderMode::Stereo
        } else {
            RenderMode::Speaker
        },
        speaker_layout: layout.to_owned(),
        validation_profile: ValidationProfile::Auto,
        ..OpenJocConfig::default()
    };
    if let Some(hrtf) = mode.binaural() {
        config.render_mode = RenderMode::Binaural;
        config.binaural = Some(BinauralConfig::builtin(hrtf, layout));
    }
    config
}

fn index_units(input: &[u8]) -> Result<Vec<InputUnit>, Box<dyn Error>> {
    let frames = openjoc_eac3::index_syncframes(input)?;
    let units = openjoc_eac3::group_access_units(&frames)?;
    units
        .into_iter()
        .map(|unit| {
            let first = *frames
                .get(unit.first_frame)
                .ok_or("invalid access unit start")?;
            let last_index = unit
                .first_frame
                .checked_add(unit.frame_count)
                .and_then(|end| end.checked_sub(1))
                .ok_or("invalid access unit frame range")?;
            let last = *frames.get(last_index).ok_or("invalid access unit end")?;
            let start = first.offset;
            let end = last
                .offset
                .checked_add(last.header.frame_size)
                .ok_or("access unit byte range overflow")?;
            let independent_count = frames[unit.first_frame..=last_index]
                .iter()
                .filter(|frame| frame.header.stream_type != openjoc_eac3::StreamType::Dependent)
                .count();
            Ok(InputUnit {
                start,
                end,
                samples: unit.samples,
                sample_rate: unit.sample_rate,
                independent_count,
                dependent_count: unit.frame_count - independent_count,
            })
        })
        .collect()
}

fn receive_available(
    session: &mut OpenJocSession,
    mode: ProbeMode,
    frames: &mut Vec<OpenJocPcmFrame>,
) -> Result<(), Box<dyn Error>> {
    if mode.pull() {
        while let Some(frame) = session.receive_binaural_frame()? {
            frames.push(frame);
        }
    } else {
        while let Some(frame) = session.receive_frame() {
            frames.push(frame);
        }
    }
    Ok(())
}

fn trajectory(sequence: u64) -> ListenerOrientation {
    let angle = (sequence as f64 * 137.0 % 360.0).to_radians();
    let half = angle / 2.0;
    ListenerOrientation::new(
        half.sin() * 0.2,
        half.sin() * 0.3,
        half.sin() * 0.87_f64.sqrt(),
        half.cos(),
    )
    .expect("deterministic finite orientation")
}

fn distribution(values: &[u128]) -> (u128, u128, u128, u128) {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    if sorted.is_empty() {
        return (0, 0, 0, 0);
    }
    let percentile =
        |percent: usize| sorted[((sorted.len() - 1) * percent / 100).min(sorted.len() - 1)];
    (
        percentile(50),
        percentile(95),
        percentile(99),
        sorted[sorted.len() - 1],
    )
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    hex(&digest.finalize())
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}

#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
fn process_cpu_ns() -> Option<u128> {
    #[repr(C)]
    struct Timespec {
        seconds: std::ffi::c_long,
        nanoseconds: std::ffi::c_long,
    }
    unsafe extern "C" {
        fn clock_gettime(clock: std::ffi::c_int, time: *mut Timespec) -> std::ffi::c_int;
    }
    let mut value = Timespec {
        seconds: 0,
        nanoseconds: 0,
    };
    // SAFETY: CLOCK_PROCESS_CPUTIME_ID=2 and value is a valid writable timespec.
    if unsafe { clock_gettime(2, &raw mut value) } != 0 {
        return None;
    }
    Some(
        u128::try_from(value.seconds).ok()? * 1_000_000_000
            + u128::try_from(value.nanoseconds).ok()?,
    )
}

#[cfg(not(all(target_os = "linux", target_pointer_width = "64")))]
fn process_cpu_ns() -> Option<u128> {
    None
}

#[cfg(test)]
mod tests {
    use super::{ProbeMode, config_for};
    use openjoc_api::RenderMode;

    #[test]
    fn explicit_stereo_selects_stereo_without_changing_speaker_mode() {
        assert_eq!(
            config_for(ProbeMode::parse("stereo").unwrap(), "2.0").render_mode,
            RenderMode::Stereo
        );
        assert_eq!(
            config_for(ProbeMode::parse("speaker").unwrap(), "2.0").render_mode,
            RenderMode::Speaker
        );
        assert_eq!(ProbeMode::parse("stereo").unwrap().name(), "stereo");
        assert_eq!(ProbeMode::parse("stereo").unwrap().binaural(), None);
    }
}
