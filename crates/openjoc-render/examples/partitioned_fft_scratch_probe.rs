//! Paired bit-exact and allocation/timing probe for the optional partitioned renderer.
//!
//! This example is a test harness, not a user-facing renderer. `capture` emits
//! deterministic same-backend PCM; `alloc` counts allocations inside rendering
//! calls; `timing` measures preallocated steady-state rendering without file I/O.

#![allow(unexpected_cfgs)]

#[cfg(openjoc_alloc_probe)]
use std::alloc::{GlobalAlloc, Layout, System};
use std::error::Error;
use std::fmt::Write as _;
use std::fs::File;
use std::hint::black_box;
use std::io::Write;
use std::path::Path;
#[cfg(openjoc_alloc_probe)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

use openjoc_render::{
    BinauralSourceBlock, CartesianPosition, HrirBank, HrirEntry, HrirEntryId, HrirPair,
    PartitionedBinauralRenderer, RenderError, SourceId, StaticBinauralSource,
    UniformPartitionedConfig,
};

#[cfg(openjoc_alloc_probe)]
struct CountingAllocator;

#[cfg(openjoc_alloc_probe)]
static TRACK_ALLOCATIONS: AtomicBool = AtomicBool::new(false);
#[cfg(openjoc_alloc_probe)]
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
#[cfg(openjoc_alloc_probe)]
static REALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
#[cfg(openjoc_alloc_probe)]
static DEALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
#[cfg(openjoc_alloc_probe)]
static ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);

// SAFETY: The allocator forwards every operation unchanged to the system allocator.
#[cfg(openjoc_alloc_probe)]
#[allow(unsafe_code)]
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded layout is unchanged from the caller.
        let pointer = unsafe { System.alloc(layout) };
        if TRACK_ALLOCATIONS.load(Ordering::Relaxed) && !pointer.is_null() {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if TRACK_ALLOCATIONS.load(Ordering::Relaxed) {
            DEALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: forwarded pointer and layout are unchanged from the caller.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: forwarded pointer, layout, and size are unchanged from the caller.
        let new_pointer = unsafe { System.realloc(pointer, layout, new_size) };
        if TRACK_ALLOCATIONS.load(Ordering::Relaxed) && !new_pointer.is_null() {
            REALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(new_size, Ordering::Relaxed);
        }
        new_pointer
    }
}

#[global_allocator]
#[cfg(openjoc_alloc_probe)]
static GLOBAL: CountingAllocator = CountingAllocator;

const SAMPLE_RATE: u32 = 48_000;
const SAMPLE_BYTES: usize = 8;
const DEFAULT_CAPTURE_PARTITION: usize = 256;
const MEASURE_SOURCE_COUNT: usize = 11;
const MEASURE_TAPS: usize = 256;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let mode = args
        .next()
        .ok_or("usage: partitioned_fft_scratch_probe <generate-inputs PATH|capture INPUTS PREFIX [PARTITION]|config PARTITION|alloc|timing>")?;
    match mode.as_str() {
        "generate-inputs" => {
            let path = args
                .next()
                .ok_or("generate-inputs requires an output path")?;
            if args.next().is_some() {
                return Err("unexpected generate-inputs argument".into());
            }
            write_capture_inputs(Path::new(&path))
        }
        "capture" => {
            let input_path = args
                .next()
                .ok_or("capture requires an input fixture path")?;
            let prefix = args.next().ok_or("capture requires an output prefix")?;
            let partition_size = args.next().map_or(Ok(DEFAULT_CAPTURE_PARTITION), |value| {
                value.parse::<usize>()
            })?;
            if args.next().is_some() {
                return Err("unexpected capture argument".into());
            }
            capture(Path::new(&input_path), Path::new(&prefix), partition_size)
        }
        "config" => {
            let partition_size = args
                .next()
                .ok_or("config requires a partition size")?
                .parse::<usize>()?;
            if args.next().is_some() {
                return Err("unexpected config argument".into());
            }
            match UniformPartitionedConfig::new(partition_size) {
                Err(RenderError::InvalidPartitionSize { size }) if size == partition_size => {
                    println!("unsupported,{partition_size},InvalidPartitionSize");
                    Ok(())
                }
                Err(error) => Err(format!("unexpected configuration error: {error}").into()),
                Ok(_) => {
                    Err(format!("partition size {partition_size} unexpectedly accepted").into())
                }
            }
        }
        "alloc" => {
            if args.next().is_some() {
                return Err("alloc takes no arguments".into());
            }
            measure_allocations()
        }
        "timing" => {
            if args.next().is_some() {
                return Err("timing takes no arguments".into());
            }
            measure_timing()
        }
        _ => Err(format!("unknown mode {mode:?}").into()),
    }
}

fn capture(input_path: &Path, prefix: &Path, partition_size: usize) -> Result<(), Box<dyn Error>> {
    let (bank, sources) = capture_scene()?;
    let mut renderer = PartitionedBinauralRenderer::new(
        SAMPLE_RATE,
        UniformPartitionedConfig::new(partition_size)?,
        bank,
        sources,
    )?;
    let fixtures = read_capture_inputs(input_path)?;
    let mut manifest = String::from("openjoc-partitioned-pcm-probe\t1\n");
    writeln!(
        manifest,
        "H\tsample_rate_hz\t{SAMPLE_RATE}\nH\tpartition_size\t{partition_size}\nH\tfft_size\t{}\nH\tsource_count\t2\nH\thrir_tap_counts\t300,533\nH\tscenario_count\t{}\nH\tpcm_format\tf64le-interleaved-stereo\nH\tinput_fixture_fnv1a64\t{:016x}",
        partition_size * 2,
        fixtures.len(),
        fnv1a64(&std::fs::read(input_path)?)
    )?;
    let mut input_bytes = Vec::new();
    let mut pcm_bytes = Vec::new();
    let mut scenario_output_bytes = Vec::new();
    let mut frame_id = 0usize;

    for (scenario, (first, second)) in fixtures.iter().enumerate() {
        renderer.reset();
        let scenario_start = pcm_bytes.len();
        let input_start = input_bytes.len();
        for sample in first.iter().chain(second) {
            input_bytes.extend_from_slice(&sample.to_le_bytes());
        }
        writeln!(
            manifest,
            "I\t{scenario}\t{}\t{}\t{}",
            first.len(),
            second.len(),
            fnv1a64(&input_bytes[input_start..])
        )?;
        let mut offset = 0usize;
        let mut block_index = 0usize;
        while offset + partition_size <= first.len() {
            let end = offset + partition_size;
            let reverse = match scenario {
                0 => block_index % 2 == 1,
                1 | 3 => block_index % 2 == 0,
                _ => block_index % 3 != 1,
            };
            let blocks = if reverse {
                [
                    BinauralSourceBlock::new(SourceId::new(20), &second[offset..end]),
                    BinauralSourceBlock::new(SourceId::new(10), &first[offset..end]),
                ]
            } else {
                [
                    BinauralSourceBlock::new(SourceId::new(10), &first[offset..end]),
                    BinauralSourceBlock::new(SourceId::new(20), &second[offset..end]),
                ]
            };
            let mut left = vec![0.0; partition_size];
            let mut right = vec![0.0; partition_size];
            renderer.render_partition(&blocks, &mut left, &mut right)?;
            append_interleaved(&mut pcm_bytes, &left, &right);
            writeln!(
                manifest,
                "F\t{frame_id}\t{scenario}\tinput\t{block_index}\t{offset}\t{}\t{}",
                partition_size,
                if reverse { "20,10" } else { "10,20" }
            )?;
            frame_id += 1;
            offset = end;
            block_index += 1;
        }

        let partial_len = first.len() - offset;
        let partial_blocks = if scenario % 2 == 1 {
            [
                BinauralSourceBlock::new(SourceId::new(20), &second[offset..]),
                BinauralSourceBlock::new(SourceId::new(10), &first[offset..]),
            ]
        } else {
            [
                BinauralSourceBlock::new(SourceId::new(10), &first[offset..]),
                BinauralSourceBlock::new(SourceId::new(20), &second[offset..]),
            ]
        };
        let mut left = vec![0.0; partial_len];
        let mut right = vec![0.0; partial_len];
        renderer.finish_input(&partial_blocks, partial_len, &mut left, &mut right)?;
        append_interleaved(&mut pcm_bytes, &left, &right);
        writeln!(
            manifest,
            "F\t{frame_id}\t{scenario}\tfinish\t{block_index}\t{offset}\t{partial_len}\t{}",
            if scenario % 2 == 1 { "20,10" } else { "10,20" }
        )?;
        frame_id += 1;

        let mut drain_index = 0usize;
        let mut drained_samples = 0usize;
        let drain_sizes = [1usize, 73, 16, 257, 9, 128];
        while renderer.remaining_tail_samples() > 0 {
            let count = renderer
                .remaining_tail_samples()
                .min(drain_sizes[drain_index % drain_sizes.len()]);
            let mut left = vec![0.0; count];
            let mut right = vec![0.0; count];
            renderer.drain_tail_block(&mut left, &mut right)?;
            append_interleaved(&mut pcm_bytes, &left, &right);
            writeln!(
                manifest,
                "F\t{frame_id}\t{scenario}\tdrain\t{drain_index}\t{}\t{count}\t-",
                offset + partial_len + drained_samples
            )?;
            frame_id += 1;
            drain_index += 1;
            drained_samples += count;
        }
        if !renderer.is_finished() {
            return Err("renderer did not finish after exact tail drain".into());
        }
        let scenario_bytes = &pcm_bytes[scenario_start..];
        if scenario == 1
            && scenario_bytes
                .chunks_exact(SAMPLE_BYTES)
                .any(|word| f64::from_le_bytes(word.try_into().unwrap()) != 0.0)
        {
            return Err("zero-input scenario produced nonzero PCM after reset".into());
        }
        scenario_output_bytes.push(scenario_bytes.to_vec());
        let sample_count = scenario_bytes.len() / 16;
        writeln!(
            manifest,
            "S\t{scenario}\t{sample_count}\t{}\t{}\t{}",
            scenario_bytes.len(),
            fnv1a64(scenario_bytes),
            scenario_nonzero_counts(scenario_bytes)
        )?;
    }

    if scenario_output_bytes[0] != scenario_output_bytes[3] {
        return Err("reset/source-order replay did not reproduce identical f64 PCM bits".into());
    }
    if pcm_bytes.is_empty()
        || !pcm_bytes
            .chunks_exact(8)
            .all(|word| f64::from_le_bytes(word.try_into().unwrap()).is_finite())
    {
        return Err("capture has empty or non-finite PCM".into());
    }
    writeln!(
        manifest,
        "R\tframe_count\t{frame_id}\nR\tpcm_bytes\t{}\nR\tinput_f64le_fnv1a64\t{:016x}\nR\treset_replay\tbit-identical\nR\tterminal_state\tfinished-after-each-tail",
        pcm_bytes.len(),
        fnv1a64(&input_bytes)
    )?;

    let manifest_path = with_suffix(prefix, ".manifest.tsv");
    let pcm_path = with_suffix(prefix, ".pcm64le");
    File::create(manifest_path)?.write_all(manifest.as_bytes())?;
    File::create(pcm_path)?.write_all(&pcm_bytes)?;
    Ok(())
}

#[cfg(openjoc_alloc_probe)]
fn measure_allocations() -> Result<(), Box<dyn Error>> {
    let (mut renderer, source_audio, mut left, mut right) = measurement_fixture()?;
    let blocks = measurement_blocks(&source_audio);
    for _ in 0..8 {
        renderer.render_partition(&blocks, &mut left, &mut right)?;
    }
    ALLOCATIONS.store(0, Ordering::Relaxed);
    REALLOCATIONS.store(0, Ordering::Relaxed);
    DEALLOCATIONS.store(0, Ordering::Relaxed);
    ALLOCATED_BYTES.store(0, Ordering::Relaxed);
    let calls = 64usize;
    TRACK_ALLOCATIONS.store(true, Ordering::SeqCst);
    for _ in 0..calls {
        renderer.render_partition(&blocks, &mut left, &mut right)?;
        black_box((&left, &right));
    }
    TRACK_ALLOCATIONS.store(false, Ordering::SeqCst);
    println!(
        "alloc,{calls},{},{},{},{},{}",
        ALLOCATIONS.load(Ordering::Relaxed),
        REALLOCATIONS.load(Ordering::Relaxed),
        DEALLOCATIONS.load(Ordering::Relaxed),
        ALLOCATED_BYTES.load(Ordering::Relaxed),
        fnv1a64_f64_slices(&left, &right)
    );
    Ok(())
}

#[cfg(not(openjoc_alloc_probe))]
fn measure_allocations() -> Result<(), Box<dyn Error>> {
    Err("allocation mode requires a build with --cfg openjoc_alloc_probe".into())
}

fn measure_timing() -> Result<(), Box<dyn Error>> {
    let (mut renderer, source_audio, mut left, mut right) = measurement_fixture()?;
    let blocks = measurement_blocks(&source_audio);
    for _ in 0..64 {
        renderer.render_partition(&blocks, &mut left, &mut right)?;
    }
    let calls = 1_024usize;
    let start = Instant::now();
    for _ in 0..calls {
        renderer.render_partition(&blocks, &mut left, &mut right)?;
        black_box((&left, &right));
    }
    let elapsed_ns = start.elapsed().as_nanos();
    println!(
        "timing,{calls},{elapsed_ns},{:.3},{:016x}",
        elapsed_ns as f64 / calls as f64,
        fnv1a64_f64_slices(&left, &right)
    );
    Ok(())
}

#[allow(clippy::type_complexity)]
fn measurement_fixture() -> Result<
    (
        PartitionedBinauralRenderer,
        Vec<Vec<f64>>,
        Vec<f64>,
        Vec<f64>,
    ),
    Box<dyn Error>,
> {
    let (bank, sources) = measurement_scene()?;
    let source_audio = (0..MEASURE_SOURCE_COUNT)
        .map(|source| {
            (0..MEASURE_TAPS)
                .map(|sample| {
                    let n = sample as f64;
                    let source_phase = source as f64 * 0.173;
                    0.09 * (n * 0.023 + source_phase).sin()
                        + 0.035 * (n * 0.011 + source_phase * 0.7).cos()
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let renderer = PartitionedBinauralRenderer::new(
        SAMPLE_RATE,
        UniformPartitionedConfig::new(MEASURE_TAPS)?,
        bank,
        sources,
    )?;
    Ok((
        renderer,
        source_audio,
        vec![0.0; MEASURE_TAPS],
        vec![0.0; MEASURE_TAPS],
    ))
}

fn measurement_blocks(source_audio: &[Vec<f64>]) -> Vec<BinauralSourceBlock<'_>> {
    source_audio
        .iter()
        .enumerate()
        .map(|(index, samples)| {
            BinauralSourceBlock::new(SourceId::new((index + 1) as u64), samples)
        })
        .collect()
}

fn capture_scene() -> Result<(HrirBank, Vec<StaticBinauralSource>), Box<dyn Error>> {
    let entries = vec![
        HrirEntry::new(
            HrirEntryId::new(101),
            CartesianPosition::new(0.0, 1.0, 0.0),
            HrirPair::new(
                SAMPLE_RATE,
                make_taps(300, 0.9, 0.013),
                make_taps(300, 0.7, 0.019),
            )?,
        )?,
        HrirEntry::new(
            HrirEntryId::new(202),
            CartesianPosition::new(1.0, 0.0, 0.0),
            HrirPair::new(
                SAMPLE_RATE,
                make_taps(533, 0.6, 0.011),
                make_taps(533, 0.8, 0.017),
            )?,
        )?,
    ];
    let bank = HrirBank::new(SAMPLE_RATE, entries)?;
    let sources = vec![
        StaticBinauralSource::new(
            SourceId::new(10),
            CartesianPosition::new(0.0, 1.0, 0.0),
            0.85,
            HrirEntryId::new(101),
        )?,
        StaticBinauralSource::new(
            SourceId::new(20),
            CartesianPosition::new(1.0, 0.0, 0.0),
            -0.55,
            HrirEntryId::new(202),
        )?,
    ];
    Ok((bank, sources))
}

fn measurement_scene() -> Result<(HrirBank, Vec<StaticBinauralSource>), Box<dyn Error>> {
    let mut entries = Vec::with_capacity(MEASURE_SOURCE_COUNT);
    let mut sources = Vec::with_capacity(MEASURE_SOURCE_COUNT);
    for index in 0..MEASURE_SOURCE_COUNT {
        let angle = (index as f64 + 0.3) * std::f64::consts::TAU / 13.0;
        let direction = CartesianPosition::new(angle.cos(), angle.sin(), 0.2);
        let id = HrirEntryId::new((index + 1) as u64);
        entries.push(HrirEntry::new(
            id,
            direction,
            HrirPair::new(
                SAMPLE_RATE,
                make_taps(MEASURE_TAPS, 0.5 + index as f64 * 0.01, 0.011),
                make_taps(MEASURE_TAPS, 0.7 + index as f64 * 0.01, 0.017),
            )?,
        )?);
        sources.push(StaticBinauralSource::new(
            SourceId::new((index + 1) as u64),
            direction,
            if index % 2 == 0 { 0.8 } else { -0.6 },
            id,
        )?);
    }
    Ok((HrirBank::new(SAMPLE_RATE, entries)?, sources))
}

fn make_taps(count: usize, scale: f64, phase: f64) -> Vec<f64> {
    (0..count)
        .map(|index| {
            let n = index as f64;
            scale * (n * 0.071 + phase).sin() / (n + 1.0)
                + 0.15 * (n * 0.037 + phase * 0.5).cos() / (n + 1.0)
        })
        .collect()
}

fn make_capture_inputs(count: usize, variant: usize) -> (Vec<f64>, Vec<f64>) {
    let phase = variant as f64 * 0.29;
    let first = (0..count)
        .map(|index| {
            let n = index as f64;
            0.17 * (n * 0.019 + phase).sin() + 0.06 * (n * 0.031 + 0.2).cos()
        })
        .collect();
    let second = (0..count)
        .map(|index| {
            let n = index as f64;
            -0.11 * (n * 0.023 + phase * 0.7).cos() + 0.045 * (n * 0.009 + 0.4).sin()
        })
        .collect();
    (first, second)
}

fn write_capture_inputs(path: &Path) -> Result<(), Box<dyn Error>> {
    let fixtures = [
        make_capture_inputs(791, 0),
        (vec![0.0; 77], vec![0.0; 77]),
        make_capture_inputs(613, 1),
        make_capture_inputs(791, 0),
    ];
    let mut bytes = b"OJPTD001".to_vec();
    bytes.extend_from_slice(&(fixtures.len() as u32).to_le_bytes());
    for (first, second) in fixtures {
        if first.len() != second.len() {
            return Err("generated source input lengths differ".into());
        }
        bytes.extend_from_slice(&(first.len() as u64).to_le_bytes());
        for sample in first.iter().chain(&second) {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
    }
    File::create(path)?.write_all(&bytes)?;
    Ok(())
}

#[allow(clippy::type_complexity)]
fn read_capture_inputs(path: &Path) -> Result<Vec<(Vec<f64>, Vec<f64>)>, Box<dyn Error>> {
    let bytes = std::fs::read(path)?;
    if bytes.get(..8) != Some(b"OJPTD001") {
        return Err("unsupported partitioned input fixture header".into());
    }
    let mut cursor = 8usize;
    let scenario_count = take_u32(&bytes, &mut cursor)? as usize;
    let mut scenarios = Vec::with_capacity(scenario_count);
    for _ in 0..scenario_count {
        let sample_count = usize::try_from(take_u64(&bytes, &mut cursor)?)?;
        let first = take_f64_vec(&bytes, &mut cursor, sample_count)?;
        let second = take_f64_vec(&bytes, &mut cursor, sample_count)?;
        if first
            .iter()
            .chain(&second)
            .any(|sample| !sample.is_finite())
        {
            return Err("partitioned input fixture contains non-finite samples".into());
        }
        scenarios.push((first, second));
    }
    if cursor != bytes.len() || scenarios.is_empty() {
        return Err("partitioned input fixture is truncated, trailing, or empty".into());
    }
    Ok(scenarios)
}

fn take_u32(bytes: &[u8], cursor: &mut usize) -> Result<u32, Box<dyn Error>> {
    let end = cursor
        .checked_add(4)
        .ok_or("input fixture offset overflow")?;
    let word: [u8; 4] = bytes
        .get(*cursor..end)
        .ok_or("truncated partitioned input fixture")?
        .try_into()?;
    *cursor = end;
    Ok(u32::from_le_bytes(word))
}

fn take_u64(bytes: &[u8], cursor: &mut usize) -> Result<u64, Box<dyn Error>> {
    let end = cursor
        .checked_add(8)
        .ok_or("input fixture offset overflow")?;
    let word: [u8; 8] = bytes
        .get(*cursor..end)
        .ok_or("truncated partitioned input fixture")?
        .try_into()?;
    *cursor = end;
    Ok(u64::from_le_bytes(word))
}

fn take_f64_vec(
    bytes: &[u8],
    cursor: &mut usize,
    sample_count: usize,
) -> Result<Vec<f64>, Box<dyn Error>> {
    let byte_count = sample_count
        .checked_mul(8)
        .ok_or("input fixture size overflow")?;
    let end = cursor
        .checked_add(byte_count)
        .ok_or("input fixture offset overflow")?;
    let values = bytes
        .get(*cursor..end)
        .ok_or("truncated partitioned input fixture")?
        .chunks_exact(8)
        .map(|word| f64::from_le_bytes(word.try_into().unwrap()))
        .collect();
    *cursor = end;
    Ok(values)
}

fn append_interleaved(destination: &mut Vec<u8>, left: &[f64], right: &[f64]) {
    for (left, right) in left.iter().zip(right) {
        destination.extend_from_slice(&left.to_le_bytes());
        destination.extend_from_slice(&right.to_le_bytes());
    }
}

fn scenario_nonzero_counts(bytes: &[u8]) -> String {
    let mut left = 0usize;
    let mut right = 0usize;
    for frame in bytes.chunks_exact(16) {
        if f64::from_le_bytes(frame[..8].try_into().unwrap()) != 0.0 {
            left += 1;
        }
        if f64::from_le_bytes(frame[8..].try_into().unwrap()) != 0.0 {
            right += 1;
        }
    }
    format!("{left},{right}")
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn fnv1a64_f64_slices(left: &[f64], right: &[f64]) -> u64 {
    left.iter()
        .chain(right)
        .flat_map(|sample| sample.to_le_bytes())
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

fn with_suffix(prefix: &Path, suffix: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(format!("{}{suffix}", prefix.display()))
}
