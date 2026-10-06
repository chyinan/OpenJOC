//! Public-API-only micro probe for the static direct binaural renderer.
//! The baseline run captures exact block-plus-tail bits; a candidate run checks
//! every bit before timing. No FFT/partitioned backend or file I/O is timed.
use openjoc_render::{
    BinauralRenderer, BinauralSourceBlock, CartesianPosition, HrirBank, HrirEntry, HrirEntryId,
    HrirPair, SourceId, StaticBinauralSource,
};
use std::env;
use std::fs::File;
use std::hint::black_box;
use std::io::{Read, Write};
use std::time::Instant;

const SAMPLE_RATE: u32 = 48_000;
const SOURCE_COUNT: usize = 11;
const TAPS_PER_SOURCE: usize = 256;
const BLOCK_LEN: usize = 1536;
const WARMUPS: usize = 8;
const ITERATIONS: usize = 128;
const CAPTURE_MAGIC: &[u8; 8] = b"OJDIRECT";

struct Fixture {
    bank: HrirBank,
    sources: Vec<StaticBinauralSource>,
    samples: Vec<Vec<f64>>,
    tap_lengths: [usize; SOURCE_COUNT],
    input_hash: u64,
}

struct CapturedPcm {
    block_left: Vec<f64>,
    block_right: Vec<f64>,
    left: Vec<f64>,
    right: Vec<f64>,
    tail_samples_before_drain: usize,
    finished_after_drain: bool,
}

impl Fixture {
    fn renderer(&self) -> BinauralRenderer {
        BinauralRenderer::new(SAMPLE_RATE, self.bank.clone(), self.sources.clone())
            .expect("deterministic probe renderer must construct")
    }

    fn blocks(&self) -> Vec<BinauralSourceBlock<'_>> {
        // Reverse caller order while the renderer retains its registered order;
        // render_block resolves block IDs before deterministic accumulation.
        (0..SOURCE_COUNT)
            .rev()
            .map(|index| {
                BinauralSourceBlock::new(SourceId::new(1000 + index as u64), &self.samples[index])
            })
            .collect()
    }
}

fn build_fixture() -> Fixture {
    let mut rng = SplitMix64::new(0x4d59_5df4_d0f3_3173);
    let gains: [f64; SOURCE_COUNT] = [
        1.0, -0.75, 0.0, -0.0, 1.25, -1.0, 0.5, -1.5, 0.25, -0.5, 0.875,
    ];
    let mut entries = Vec::with_capacity(SOURCE_COUNT);
    let mut sources = Vec::with_capacity(SOURCE_COUNT);
    let mut samples = Vec::with_capacity(SOURCE_COUNT);
    let mut input_hash = Fnv64::new();
    input_hash.word(SOURCE_COUNT as u64);
    input_hash.word(BLOCK_LEN as u64);
    input_hash.word(TAPS_PER_SOURCE as u64);

    for (index, gain) in gains.iter().copied().enumerate() {
        // The primary public-renderer probe is uniform 11 x 256. Unequal source
        // lengths are covered by the exact state regression tests.
        let tap_count = TAPS_PER_SOURCE;
        let angle = std::f64::consts::TAU * (index as f64 + 0.5) / SOURCE_COUNT as f64;
        let direction = CartesianPosition::new(angle.cos(), angle.sin(), 0.125);
        let entry_id = HrirEntryId::new(1 + index as u64);
        let source_id = SourceId::new(1000 + index as u64);
        let left_taps = (0..tap_count)
            .map(|_| rng.finite_unit())
            .collect::<Vec<_>>();
        let right_taps = (0..tap_count)
            .map(|_| rng.finite_unit())
            .collect::<Vec<_>>();
        let block = (0..BLOCK_LEN)
            .map(|_| rng.finite_unit())
            .collect::<Vec<_>>();

        input_hash.word(source_id.get());
        input_hash.word(entry_id.get());
        input_hash.word(gain.to_bits());
        for axis in [direction.x, direction.y, direction.z] {
            input_hash.word(axis.to_bits());
        }
        input_hash.word(tap_count as u64);
        for value in left_taps.iter().chain(&right_taps).chain(&block) {
            input_hash.word(value.to_bits());
        }

        entries.push(
            HrirEntry::new(
                entry_id,
                direction,
                HrirPair::new(SAMPLE_RATE, left_taps, right_taps)
                    .expect("finite deterministic taps must validate"),
            )
            .expect("finite direction must validate"),
        );
        sources.push(
            StaticBinauralSource::new(source_id, direction, gain, entry_id)
                .expect("finite deterministic source must validate"),
        );
        samples.push(block);
    }
    Fixture {
        bank: HrirBank::new(SAMPLE_RATE, entries).expect("unique directions must validate"),
        sources,
        samples,
        tap_lengths: [TAPS_PER_SOURCE; SOURCE_COUNT],
        input_hash: input_hash.finish(),
    }
}

fn capture_public_output(fixture: &Fixture) -> CapturedPcm {
    let mut renderer = fixture.renderer();
    let blocks = fixture.blocks();
    let mut left = vec![0.0; BLOCK_LEN];
    let mut right = vec![0.0; BLOCK_LEN];
    for _ in 0..WARMUPS {
        renderer
            .render_block(&blocks, &mut left, &mut right)
            .expect("valid warmup render");
    }
    renderer
        .render_block(&blocks, &mut left, &mut right)
        .expect("valid capture render");
    let block_left = left.clone();
    let block_right = right.clone();
    let tail_samples_before_drain = renderer.remaining_tail_samples();
    let mut tail_left = vec![0.0; tail_samples_before_drain];
    let mut tail_right = vec![0.0; tail_samples_before_drain];
    renderer
        .drain_tail_block(&mut tail_left, &mut tail_right)
        .expect("complete causal tail drain");
    let finished_after_drain = renderer.is_finished();
    left.extend(tail_left);
    right.extend(tail_right);
    CapturedPcm {
        block_left,
        block_right,
        left,
        right,
        tail_samples_before_drain,
        finished_after_drain,
    }
}

fn write_capture(path: &str, fixture: &Fixture, capture: &CapturedPcm) -> std::io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(CAPTURE_MAGIC)?;
    write_u64(&mut file, fixture.input_hash)?;
    write_u64(&mut file, BLOCK_LEN as u64)?;
    write_u64(&mut file, capture.left.len() as u64)?;
    write_u64(&mut file, capture.right.len() as u64)?;
    write_u64(&mut file, capture.tail_samples_before_drain as u64)?;
    write_u64(&mut file, u64::from(capture.finished_after_drain))?;
    for value in capture.left.iter().chain(&capture.right) {
        write_u64(&mut file, value.to_bits())?;
    }
    Ok(())
}

fn compare_capture(path: &str, fixture: &Fixture, capture: &CapturedPcm) -> Result<usize, String> {
    let mut file = File::open(path).map_err(|error| format!("cannot read capture: {error}"))?;
    let mut magic = [0u8; 8];
    file.read_exact(&mut magic)
        .map_err(|error| format!("cannot read capture header: {error}"))?;
    if &magic != CAPTURE_MAGIC {
        return Err("capture version mismatch".to_string());
    }
    let expected_input_hash = read_u64(&mut file).map_err(|error| error.to_string())?;
    if expected_input_hash != fixture.input_hash {
        return Err(format!(
            "input hash differs: expected={expected_input_hash:016x}, actual={:016x}",
            fixture.input_hash
        ));
    }
    let expected_block_len = read_u64(&mut file).map_err(|error| error.to_string())? as usize;
    if expected_block_len != BLOCK_LEN {
        return Err("capture block length differs".to_string());
    }
    let expected_left_len = read_u64(&mut file).map_err(|error| error.to_string())? as usize;
    let expected_right_len = read_u64(&mut file).map_err(|error| error.to_string())? as usize;
    let expected_tail_len = read_u64(&mut file).map_err(|error| error.to_string())? as usize;
    let expected_finished = read_u64(&mut file).map_err(|error| error.to_string())? != 0;
    if expected_left_len != capture.left.len()
        || expected_right_len != capture.right.len()
        || expected_tail_len != capture.tail_samples_before_drain
        || expected_finished != capture.finished_after_drain
    {
        return Err("capture output or tail metadata differs".to_string());
    }
    let mut checked = 0usize;
    for (channel, values) in [("left", &capture.left), ("right", &capture.right)] {
        for (index, actual) in values.iter().enumerate() {
            let expected_bits = read_u64(&mut file).map_err(|error| error.to_string())?;
            checked += 1;
            if expected_bits != actual.to_bits() {
                return Err(format!(
                    "public capture mismatch: {channel}[{index}] expected=0x{expected_bits:016x}, actual=0x{:016x}",
                    actual.to_bits()
                ));
            }
        }
    }
    let mut extra = [0u8; 1];
    if file.read(&mut extra).map_err(|error| error.to_string())? != 0 {
        return Err("capture has trailing bytes".to_string());
    }
    Ok(checked)
}

fn write_u64(writer: &mut impl Write, value: u64) -> std::io::Result<()> {
    writer.write_all(&value.to_le_bytes())
}

fn read_u64(reader: &mut impl Read) -> std::io::Result<u64> {
    let mut bytes = [0u8; 8];
    reader.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

fn time_public_renderer(fixture: &Fixture) -> (u128, Vec<f64>, Vec<f64>) {
    let mut renderer = fixture.renderer();
    let blocks = fixture.blocks();
    let mut left = vec![0.0; BLOCK_LEN];
    let mut right = vec![0.0; BLOCK_LEN];
    for _ in 0..WARMUPS {
        renderer
            .render_block(&blocks, &mut left, &mut right)
            .expect("valid warmup render");
        black_box(&left);
        black_box(&right);
    }
    let start = Instant::now();
    for _ in 0..ITERATIONS {
        let result = renderer.render_block(
            black_box(&blocks),
            black_box(&mut left),
            black_box(&mut right),
        );
        black_box(result).expect("every timed public render must succeed");
        black_box(&left);
        black_box(&right);
        black_box(renderer.remaining_tail_samples());
    }
    let elapsed = start.elapsed().as_nanos();
    (elapsed, left, right)
}

fn hash_output(left: &[f64], right: &[f64]) -> u64 {
    let mut hash = Fnv64::new();
    hash.bytes(b"left");
    for value in left {
        hash.word(value.to_bits());
    }
    hash.bytes(b"right");
    for value in right {
        hash.word(value.to_bits());
    }
    hash.finish()
}

struct Fnv64(u64);

impl Fnv64 {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= *byte as u64;
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn word(&mut self, word: u64) {
        self.bytes(&word.to_le_bytes());
    }

    fn finish(self) -> u64 {
        self.0
    }
}

struct SplitMix64(u64);

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.0;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    fn finite_unit(&mut self) -> f64 {
        let fraction = (self.next_u64() >> 11) as f64 / ((1u64 << 53) as f64);
        fraction * 2.0 - 1.0
    }
}

fn assert_timed_block_bits(capture: &CapturedPcm, left: &[f64], right: &[f64]) -> usize {
    assert_eq!(capture.block_left.len(), left.len());
    assert_eq!(capture.block_right.len(), right.len());
    let mut checked = 0usize;
    for (channel, expected_values, actual_values) in [
        ("left", &capture.block_left, left),
        ("right", &capture.block_right, right),
    ] {
        for (index, (expected, actual)) in expected_values.iter().zip(actual_values).enumerate() {
            checked += 1;
            assert_eq!(
                expected.to_bits(),
                actual.to_bits(),
                "post-timing {channel}[{index}] differs: expected=0x{:016x}, actual=0x{:016x}",
                expected.to_bits(),
                actual.to_bits()
            );
        }
    }
    checked
}

fn main() {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let (mode, variant, path) = match args.as_slice() {
        [mode, path] if mode == "capture" => ("capture", "baseline", path.as_str()),
        [mode, variant, path]
            if mode == "compare" && ["baseline", "candidate"].contains(&variant.as_str()) =>
        {
            ("compare", variant.as_str(), path.as_str())
        }
        _ => {
            eprintln!(
                "usage: direct_fir_public_probe capture <capture.bin> | compare baseline|candidate <capture.bin>"
            );
            std::process::exit(2);
        }
    };
    let fixture = build_fixture();
    // Exact capture or comparison, including the full tail, happens before timing.
    let capture = capture_public_output(&fixture);
    if mode == "capture" {
        write_capture(path, &fixture, &capture).expect("write baseline capture");
        println!(
            "capture=written,fixture=synthetic,sources={},tap_lengths=256|256|256|256|256|256|256|256|256|256|256,block={},tail_samples={},finished={}",
            SOURCE_COUNT,
            BLOCK_LEN,
            capture.tail_samples_before_drain,
            capture.finished_after_drain
        );
        return;
    }
    let preflight_f64_values = compare_capture(path, &fixture, &capture).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    });

    let (elapsed_ns, left, right) = time_public_renderer(&fixture);
    // A final bitwise check against the same-process preflight block is outside
    // the timed window; all timed calls used the same repeated input.
    let post_timing_f64_values = assert_timed_block_bits(&capture, &left, &right);
    let output_hash = hash_output(&left, &right);
    let tap_lengths = fixture
        .tap_lengths
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join("|");
    println!(
        "record_version,process_id,variant,fixture,sources,tap_lengths,block,iterations,warmups,elapsed_ns,ns_per_call,input_fnv64,block_output_fnv64,preflight_f64_values,post_timing_f64_values,captured_tail_samples,finished_after_tail"
    );
    println!(
        "1,{},{},synthetic,{},{},{},{},{},{},{:.3},{:016x},{:016x},{},{},{},{}",
        std::process::id(),
        variant,
        SOURCE_COUNT,
        tap_lengths,
        BLOCK_LEN,
        ITERATIONS,
        WARMUPS,
        elapsed_ns,
        elapsed_ns as f64 / ITERATIONS as f64,
        fixture.input_hash,
        output_hash,
        preflight_f64_values,
        post_timing_f64_values,
        capture.tail_samples_before_drain,
        capture.finished_after_drain
    );
}
