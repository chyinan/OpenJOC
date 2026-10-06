//! Bit-exact core-only E-AC-3 probe (does not use the JOC session API).
//!
//! Usage: eac3_pcm_regression_probe INPUT.eac3 LAYOUT OUTPUT_PREFIX [--stage|--alloc|--timing-only]
//! Capture mode emits public interleaved f32 to `.pcm32le` and complete core
//! f64 planes to `.pcm64le`; timing-only consumes/drops PCM in the timed call.

#![allow(unsafe_code)]

use openjoc_eac3::{
    AudioPcmSynthesizer, ChannelLocation, InternalBasePolicy, decode_audio_blocks_with_policy,
    decode_audio_frame_pcm_with_policy, index_syncframes, inspect_programme_channels, parse_bsi,
};
use std::{
    collections::HashSet,
    error::Error,
    fs::{self, File},
    io::{BufWriter, Write},
    path::PathBuf,
    time::Instant,
};

#[cfg(feature = "allocation-profile")]
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

static DITHER: [f64; 16_384] = [0.5; 16_384];

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

#[derive(Clone, Default)]
struct ChannelAudit {
    fnv64: u64,
    nonzero: u64,
    finite: u64,
    nonfinite: u64,
    min_bits: u32,
    max_bits: u32,
    initialized: bool,
}

impl ChannelAudit {
    fn update(&mut self, value: f32) {
        if !self.initialized {
            self.min_bits = value.to_bits();
            self.max_bits = value.to_bits();
            self.initialized = true;
        } else if value.is_finite() {
            let min = f32::from_bits(self.min_bits);
            let max = f32::from_bits(self.max_bits);
            if value < min {
                self.min_bits = value.to_bits();
            }
            if value > max {
                self.max_bits = value.to_bits();
            }
        }
        self.nonzero += u64::from(value != 0.0);
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

fn main() -> Result<(), Box<dyn Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() < 3 {
        return Err("usage: eac3_pcm_regression_probe INPUT LAYOUT OUTPUT_PREFIX [--stage]".into());
    }
    let input_path = &args[0];
    let layout_arg = &args[1];
    let prefix = PathBuf::from(&args[2]);
    let stage_timing = args[3..].iter().any(|arg| arg == "--stage");
    let allocation_timing = args[3..].iter().any(|arg| arg == "--alloc");
    let timing_only = args[3..].iter().any(|arg| arg == "--timing-only");
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
    let input_sha256 = std::env::var("OPENJOC_INPUT_SHA256")
        .map_err(|_| "paired runner must set OPENJOC_INPUT_SHA256 for the ordinary core probe")?;
    let input = fs::read(input_path)?;
    let frames = index_syncframes(&input)?;
    if frames.is_empty() {
        return Err("ordinary E-AC-3 input has no complete syncframes".into());
    }
    if frames
        .iter()
        .any(|frame| frame.header.sample_rate != 48_000)
    {
        return Err("ordinary core probe currently requires 48-kHz syncframes".into());
    }
    if frames
        .iter()
        .any(|frame| frame.header.stream_type == openjoc_eac3::StreamType::Dependent)
    {
        return Err("ordinary core probe expects independent-only E-AC-3 input".into());
    }
    let layout = core_layout(&input, &frames)?;
    if !layout_arg.is_empty() && layout_arg != "auto" && layout_arg != &layout.0 {
        return Err(format!(
            "declared layout {layout_arg:?} does not match decoded core layout {:?}",
            layout.0
        )
        .into());
    }
    let pcm32_path = PathBuf::from(format!("{}.pcm32le", prefix.display()));
    let pcm64_path = PathBuf::from(format!("{}.pcm64le", prefix.display()));
    let manifest_path = PathBuf::from(format!("{}.manifest.tsv", prefix.display()));
    let timing_path = PathBuf::from(format!("{}.timing.tsv", prefix.display()));
    if let Some(parent) = prefix.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut pcm32 = BufWriter::with_capacity(1024 * 1024, File::create(&pcm32_path)?);
    let mut pcm64 = BufWriter::with_capacity(1024 * 1024, File::create(&pcm64_path)?);
    let mut manifest = BufWriter::new(File::create(&manifest_path)?);
    let mut timing = BufWriter::new(File::create(&timing_path)?);

    let config_descriptor = concat!(
        "openjoc-eac3-core-v1\n",
        "decoder=AudioPcmSynthesizer\n",
        "policy=CurrentDefault\n",
        "dither_f64_bits=3fe0000000000000\n",
        "presentation=interleaved-f32-cast\n",
        "audit=interleaved-f64-core-planes\n",
        "input_policy=one-independent-syncframe-per-packet\n"
    );
    let config_fingerprint = format!("fnv64:{:016x}", fnv64(config_descriptor.as_bytes()));
    writeln!(manifest, "openjoc-pcm-probe\t1")?;
    writeln!(manifest, "H\tinput_sha256\t{input_sha256}")?;
    writeln!(manifest, "H\tmode\tordinary-eac3-core")?;
    writeln!(manifest, "H\tlayout\t{}", hex(layout.0.as_bytes()))?;
    writeln!(manifest, "H\tlatency_samples\t0")?;
    writeln!(manifest, "H\tconfig_fingerprint\t{config_fingerprint}")?;
    writeln!(
        manifest,
        "H\tconfig_descriptor_hex\t{}",
        hex(config_descriptor.as_bytes())
    )?;
    writeln!(manifest, "H\tinput_unit_count\t{}", frames.len())?;
    writeln!(
        timing,
        "record\tindex\ttotal_ns\tdecode_blocks_ns\tsynthesis_ns\talloc_calls\trealloc_calls\tdealloc_calls\trequested_alloc_bytes"
    )?;

    let init_start = Instant::now();
    let mut synthesizer = AudioPcmSynthesizer::new();
    let init_ns = init_start.elapsed().as_nanos();
    let warmup_count = frames.len().min(8);
    let warmup_start = Instant::now();
    for indexed in frames.iter().take(warmup_count) {
        let range = frame_range(indexed.offset, indexed.header.frame_size, input.len())?;
        black_box(decode_audio_frame_pcm_with_policy(
            &input[range],
            &DITHER,
            &mut synthesizer,
            InternalBasePolicy::CurrentDefault,
        )?);
    }
    let warmup_ns = warmup_start.elapsed().as_nanos();
    let reset_start = Instant::now();
    synthesizer.reset();
    let reset_ns = reset_start.elapsed().as_nanos();

    let mut pcm32_bytes = 0_u64;
    let mut pcm64_bytes = 0_u64;
    let mut sample_count = 0_u64;
    let mut audits = vec![ChannelAudit::default(); layout.1.len()];
    let mut au_ns = Vec::with_capacity(frames.len());
    let mut stage_decode_ns = 0_u128;
    let mut stage_synthesis_ns = 0_u128;
    let loop_wall_start = Instant::now();
    for (index, indexed) in frames.iter().enumerate() {
        let bytes = frame_range(indexed.offset, indexed.header.frame_size, input.len())?;
        let raw = &input[bytes];
        #[cfg(feature = "allocation-profile")]
        if allocation_timing {
            allocation_begin();
        }
        let start = Instant::now();
        let mut stage_decode_call_ns = 0_u128;
        let mut stage_synthesis_call_ns = 0_u128;
        let pcm = if stage_timing {
            let decode_start = Instant::now();
            let blocks =
                decode_audio_blocks_with_policy(raw, &DITHER, InternalBasePolicy::CurrentDefault)?;
            let decode_ns = decode_start.elapsed().as_nanos();
            let synthesis_start = Instant::now();
            let pcm = synthesizer.synthesize(&blocks)?;
            let synthesis_ns = synthesis_start.elapsed().as_nanos();
            stage_decode_ns = stage_decode_ns.saturating_add(decode_ns);
            stage_synthesis_ns = stage_synthesis_ns.saturating_add(synthesis_ns);
            stage_decode_call_ns = decode_ns;
            stage_synthesis_call_ns = synthesis_ns;
            pcm
        } else {
            decode_audio_frame_pcm_with_policy(
                raw,
                &DITHER,
                &mut synthesizer,
                InternalBasePolicy::CurrentDefault,
            )?
        };
        let (elapsed, pcm) = if timing_only {
            let sample_count_frame = pcm.channels.first().map_or(0, Vec::len);
            if sample_count_frame == 0
                || pcm
                    .channels
                    .iter()
                    .any(|channel| channel.len() != sample_count_frame)
                || pcm
                    .lfe
                    .as_ref()
                    .is_some_and(|lfe| lfe.len() != sample_count_frame)
            {
                return Err(
                    format!("syncframe {index} produced misaligned core PCM planes").into(),
                );
            }
            let output_channels = pcm.channels.len() + usize::from(pcm.lfe.is_some());
            if output_channels != layout.1.len() {
                return Err(format!("syncframe {index} changed core channel count").into());
            }
            sample_count = sample_count
                .checked_add(u64::try_from(sample_count_frame)?)
                .ok_or("sample count overflow")?;
            black_box(&pcm);
            drop(pcm);
            (start.elapsed().as_nanos(), None)
        } else {
            (start.elapsed().as_nanos(), Some(pcm))
        };
        #[cfg(feature = "allocation-profile")]
        let allocation_counts = if allocation_timing {
            allocation_end()
        } else {
            AllocationCounts::default()
        };
        #[cfg(not(feature = "allocation-profile"))]
        let allocation_counts = AllocationCounts::default();
        au_ns.push(elapsed);
        if !timing_only {
            writeln!(
                timing,
                "AU\t{index}\t{elapsed}\t{stage_decode_call_ns}\t{stage_synthesis_call_ns}\t{}\t{}\t{}\t{}",
                allocation_counts.alloc_calls,
                allocation_counts.realloc_calls,
                allocation_counts.dealloc_calls,
                allocation_counts.requested_bytes,
            )?;
        }
        if timing_only {
            continue;
        }
        let pcm = pcm.ok_or("core timing run did not retain PCM for capture")?;
        let sample_count_frame = pcm.channels.first().map_or(0, Vec::len);
        if sample_count_frame == 0
            || pcm
                .channels
                .iter()
                .any(|channel| channel.len() != sample_count_frame)
            || pcm
                .lfe
                .as_ref()
                .is_some_and(|lfe| lfe.len() != sample_count_frame)
        {
            return Err(format!("syncframe {index} produced misaligned core PCM planes").into());
        }
        let output_channels = pcm.channels.len() + usize::from(pcm.lfe.is_some());
        if output_channels != layout.1.len() {
            return Err(format!("syncframe {index} changed core channel count").into());
        }
        let pts = sample_count;
        let pcm_offset = pcm32_bytes;
        writeln!(
            manifest,
            "AU\t{index}\t{}\t{}\t{}\t{}\t{}\t1\t0",
            indexed.offset,
            indexed.header.frame_size,
            sample_count_frame,
            indexed.header.sample_rate,
            pts,
        )?;
        writeln!(
            manifest,
            "F\t{index}\tprogramme\t{}\t{output_channels}\t{sample_count_frame}\t{pts}\t{}\t{}\tCoreEac3\t0\t{pcm_offset}",
            indexed.header.sample_rate,
            hex(layout.0.as_bytes()),
            layout.2,
        )?;
        for sample in 0..sample_count_frame {
            for (channel, plane) in pcm.channels.iter().enumerate() {
                let value = plane[sample];
                pcm64.write_all(&value.to_bits().to_le_bytes())?;
                let as_f32 = value as f32;
                pcm32.write_all(&as_f32.to_bits().to_le_bytes())?;
                audits[channel].update(as_f32);
            }
            if let Some(lfe) = &pcm.lfe {
                let channel = pcm.channels.len();
                let value = lfe[sample];
                pcm64.write_all(&value.to_bits().to_le_bytes())?;
                let as_f32 = value as f32;
                pcm32.write_all(&as_f32.to_bits().to_le_bytes())?;
                audits[channel].update(as_f32);
            }
        }
        let scalar_count = sample_count_frame
            .checked_mul(output_channels)
            .ok_or("PCM scalar count overflow")?;
        pcm32_bytes = pcm32_bytes
            .checked_add(u64::try_from(scalar_count)?.saturating_mul(4))
            .ok_or("PCM32 byte count overflow")?;
        pcm64_bytes = pcm64_bytes
            .checked_add(u64::try_from(scalar_count)?.saturating_mul(8))
            .ok_or("PCM64 byte count overflow")?;
        sample_count = sample_count
            .checked_add(u64::try_from(sample_count_frame)?)
            .ok_or("sample count overflow")?;
    }
    let loop_wall_ns = loop_wall_start.elapsed().as_nanos();
    let processing_ns = au_ns.iter().copied().sum::<u128>();
    if timing_only {
        for (index, elapsed) in au_ns.iter().copied().enumerate() {
            writeln!(timing, "AU\t{index}\t{elapsed}\t0\t0\t0\t0\t0\t0")?;
        }
    }
    pcm32.flush()?;
    pcm64.flush()?;
    for (index, audit) in audits.iter().enumerate() {
        writeln!(
            manifest,
            "C\t{index}\t{}\t{}\t{}\t{}\t{:08x}\t{:08x}\t{:016x}",
            audit.nonzero,
            audit.finite,
            audit.nonfinite,
            audit.initialized,
            audit.min_bits,
            audit.max_bits,
            audit.fnv64,
        )?;
    }
    let distinct = audits
        .iter()
        .map(|audit| audit.fnv64)
        .collect::<HashSet<_>>()
        .len();
    writeln!(manifest, "R\tinput_units\t{}", frames.len())?;
    writeln!(manifest, "R\toutput_frames\t{}", frames.len())?;
    writeln!(manifest, "R\tpcm_bytes\t{pcm32_bytes}")?;
    writeln!(manifest, "R\tcore_f64_bytes\t{pcm64_bytes}")?;
    writeln!(manifest, "R\tsample_count\t{sample_count}")?;
    writeln!(manifest, "R\ttail_samples\t0")?;
    writeln!(manifest, "R\tchannel_count\t{}", audits.len())?;
    writeln!(manifest, "R\tdistinct_channel_fingerprints\t{distinct}")?;
    writeln!(manifest, "R\tdiagnostics_hex\t{}", hex(layout.0.as_bytes()))?;
    writeln!(manifest, "R\tterminal_state\tall-frames-decoded")?;
    manifest.flush()?;

    au_ns.sort_unstable();
    let audio_seconds = sample_count as f64 / 48_000.0;
    let rtf = processing_ns as f64 / 1e9 / audio_seconds;
    writeln!(timing, "SUMMARY\tinit_ns\t{init_ns}")?;
    writeln!(timing, "SUMMARY\twarmup_ns\t{warmup_ns}")?;
    writeln!(timing, "SUMMARY\treset_after_warmup_ns\t{reset_ns}")?;
    writeln!(timing, "SUMMARY\ttiming_only\t{timing_only}")?;
    writeln!(timing, "SUMMARY\tinput_sha256\t{input_sha256}")?;
    writeln!(timing, "SUMMARY\tconfig_fingerprint\t{config_fingerprint}")?;
    writeln!(
        timing,
        "SUMMARY\tconfig_descriptor_hex\t{}",
        hex(config_descriptor.as_bytes())
    )?;
    writeln!(timing, "SUMMARY\tlatency_samples\t0")?;
    writeln!(timing, "SUMMARY\tmode\tordinary-eac3-core")?;
    writeln!(timing, "SUMMARY\tlayout\t{}", hex(layout.0.as_bytes()))?;
    writeln!(timing, "SUMMARY\tinput_access_units\t{}", frames.len())?;
    writeln!(timing, "SUMMARY\toutput_frames\t{}", frames.len())?;
    writeln!(timing, "SUMMARY\toutput_sample_count\t{sample_count}")?;
    writeln!(timing, "SUMMARY\ttail_samples\t0")?;
    writeln!(timing, "SUMMARY\tchannel_count\t{}", layout.1.len())?;
    writeln!(timing, "SUMMARY\tapi_pipeline_wall_ns\t{processing_ns}")?;
    writeln!(
        timing,
        "SUMMARY\tcore_loop_wall_including_output_writes_ns\t{loop_wall_ns}"
    )?;
    writeln!(timing, "SUMMARY\tprocessing_rtf_lower_is_better\t{rtf:.9}")?;
    writeln!(
        timing,
        "SUMMARY\trealtime_speed_higher_is_better\t{:.9}",
        1.0 / rtf
    )?;
    writeln!(timing, "SUMMARY\tprogram_audio_seconds\t{audio_seconds:.6}")?;
    writeln!(
        timing,
        "SUMMARY\tns_per_au\t{:.3}",
        processing_ns as f64 / frames.len() as f64
    )?;
    writeln!(timing, "SUMMARY\tcall_p50_ns\t{}", percentile(&au_ns, 50))?;
    writeln!(timing, "SUMMARY\tcall_p95_ns\t{}", percentile(&au_ns, 95))?;
    writeln!(timing, "SUMMARY\tcall_p99_ns\t{}", percentile(&au_ns, 99))?;
    writeln!(
        timing,
        "SUMMARY\tcall_max_ns\t{}",
        au_ns.last().copied().unwrap_or(0)
    )?;
    if stage_timing {
        writeln!(timing, "STAGE\tdecode_blocks_sum_ns\t{stage_decode_ns}")?;
        writeln!(timing, "STAGE\tsynthesis_sum_ns\t{stage_synthesis_ns}")?;
    }
    timing.flush()?;
    eprintln!(
        "mode=ordinary-eac3-core layout={} input_syncframes={} output_samples={} channels={} pcm32_bytes={} pcm64_bytes={} processing_ms={:.3} rtf={:.6}",
        layout.0,
        frames.len(),
        sample_count,
        audits.len(),
        pcm32_bytes,
        pcm64_bytes,
        processing_ns as f64 / 1e6,
        rtf,
    );
    Ok(())
}

fn core_layout(
    input: &[u8],
    frames: &[openjoc_eac3::SyncframeIndexEntry],
) -> Result<(String, Vec<ChannelLocation>, String), Box<dyn Error>> {
    let first = frames.first().ok_or("missing syncframe")?;
    let range = frame_range(first.offset, first.header.frame_size, input.len())?;
    let bsi = parse_bsi(&input[range])?;
    let (full, lfe) = inspect_programme_channels(&bsi, &[])?;
    let mut labels = full
        .iter()
        .map(|channel| hex(channel.label().as_bytes()))
        .collect::<Vec<_>>();
    let mut ordered = full;
    if let Some(lfe) = lfe {
        labels.push(hex(lfe.label().as_bytes()));
        ordered.push(lfe);
    }
    let name = openjoc_eac3::programme_layout_name(
        &ordered
            .iter()
            .copied()
            .filter(|channel| !matches!(channel, ChannelLocation::Lfe(_)))
            .collect::<Vec<_>>(),
        ordered
            .iter()
            .copied()
            .find(|channel| matches!(channel, ChannelLocation::Lfe(_))),
    );
    Ok((name, ordered, labels.join(",")))
}

fn frame_range(
    start: usize,
    length: usize,
    input_len: usize,
) -> Result<std::ops::Range<usize>, Box<dyn Error>> {
    let end = start
        .checked_add(length)
        .ok_or("syncframe offset overflow")?;
    if end > input_len {
        return Err("syncframe range exceeds the input".into());
    }
    Ok(start..end)
}

fn percentile(sorted: &[u128], percentile: usize) -> u128 {
    if sorted.is_empty() {
        return 0;
    }
    sorted[((sorted.len() - 1) * percentile / 100).min(sorted.len() - 1)]
}

fn fnv64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
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

fn black_box<T>(value: T) -> T {
    std::hint::black_box(value)
}
