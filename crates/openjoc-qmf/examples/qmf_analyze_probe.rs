//! Bit-exact and timing probe for the public QMF analysis path.
//!
//! Capture all real/imaginary f64 output bits with:
//! `cargo run -p openjoc-qmf --release --example qmf_analyze_probe -- capture <raw-file>`
//!
//! Measure a fixed number of already-generated analysis calls with:
//! `cargo run -p openjoc-qmf --release --example qmf_analyze_probe -- bench [calls]`

#![forbid(unsafe_code)]

use openjoc_qmf::{QMF_BANDS, ReferenceQmf64F64};
use std::fs::File;
use std::hint::black_box;
use std::io::{self, BufWriter, Write};
use std::time::Instant;

type Block = [f64; QMF_BANDS];

const HISTORY_BLOCKS: usize = 32;
const ZERO_FLUSH_BLOCKS: usize = 16;
const SIGNAL_BLOCKS: usize = 256;
const IMPULSE_POSITIONS: [usize; 5] = [0, 1, 31, 32, 63];
const DEFAULT_BENCH_CALLS: usize = 65_536;

// Fixed-size capture fixtures are small in aggregate and never enter timing.
#[allow(clippy::large_enum_variant)]
enum Operation {
    Analyze(Block),
    Reset,
}

fn main() -> io::Result<()> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("capture") => {
            let path = args.next().ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "capture requires a raw output path",
                )
            })?;
            if args.next().is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "capture accepts exactly one output path",
                ));
            }
            capture(path)
        }
        Some("bench") => {
            let calls = match args.next() {
                Some(value) => value
                    .parse::<usize>()
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?,
                None => DEFAULT_BENCH_CALLS,
            };
            if args.next().is_some() || calls == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "bench accepts an optional positive analysis-call count",
                ));
            }
            benchmark(calls);
            Ok(())
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: qmf_analyze_probe capture <raw-file> | bench [calls]",
        )),
    }
}

fn deterministic_block(block_index: usize) -> Block {
    std::array::from_fn(|sample_index| {
        let residue = (block_index * 73 + sample_index * 19) % 503;
        let value = (residue as f64 - 251.0) / 256.0;
        if (block_index + sample_index) % 2 == 0 {
            value
        } else {
            -value
        }
    })
}

fn signed_zero_block() -> Block {
    std::array::from_fn(|index| if index % 2 == 0 { -0.0 } else { 0.0 })
}

fn push_zero_flush_blocks(operations: &mut Vec<Operation>) {
    operations.push(Operation::Analyze(signed_zero_block()));
    operations.push(Operation::Analyze([-0.0; QMF_BANDS]));
    for _ in 2..ZERO_FLUSH_BLOCKS {
        operations.push(Operation::Analyze([0.0; QMF_BANDS]));
    }
}

fn impulse_block(position: usize, amplitude: f64) -> Block {
    let mut block = [0.0; QMF_BANDS];
    block[position] = amplitude;
    block
}

fn capture_operations() -> Vec<Operation> {
    let mut operations = Vec::new();

    for index in 0..HISTORY_BLOCKS {
        operations.push(Operation::Analyze(deterministic_block(index)));
    }
    operations.push(Operation::Reset);
    push_zero_flush_blocks(&mut operations);

    for index in 0..SIGNAL_BLOCKS {
        operations.push(Operation::Analyze(deterministic_block(index)));
    }

    for (impulse_index, position) in IMPULSE_POSITIONS.iter().copied().enumerate() {
        let amplitude = if impulse_index % 2 == 0 { 1.0 } else { -0.5 };
        operations.push(Operation::Analyze(impulse_block(position, amplitude)));
    }
    push_zero_flush_blocks(&mut operations);

    operations.push(Operation::Reset);
    for (impulse_index, position) in IMPULSE_POSITIONS.iter().copied().enumerate() {
        let amplitude = if impulse_index % 2 == 0 { -1.0 } else { 0.25 };
        operations.push(Operation::Analyze(impulse_block(position, amplitude)));
    }
    push_zero_flush_blocks(&mut operations);

    operations
}

fn capture(path: String) -> io::Result<()> {
    let operations = capture_operations();
    let analyze_blocks = operations
        .iter()
        .filter(|operation| matches!(operation, Operation::Analyze(_)))
        .count();
    let reset_operations = operations
        .iter()
        .filter(|operation| matches!(operation, Operation::Reset))
        .count();

    let mut qmf = ReferenceQmf64F64::new();
    let mut writer = BufWriter::new(File::create(path)?);
    for operation in operations {
        match operation {
            Operation::Analyze(block) => {
                for subband in qmf.analyze(&block) {
                    writer.write_all(&subband.re.to_bits().to_le_bytes())?;
                    writer.write_all(&subband.im.to_bits().to_le_bytes())?;
                }
            }
            Operation::Reset => qmf.reset(),
        }
    }
    writer.flush()?;

    eprintln!(
        "captured {analyze_blocks} blocks ({} f64 components, {} bytes), {reset_operations} resets: {HISTORY_BLOCKS} non-silent history blocks; reset; {ZERO_FLUSH_BLOCKS} signed/positive-zero reset-flush blocks; {SIGNAL_BLOCKS} deterministic non-silent multiblock inputs; {} impulses with prior history; {ZERO_FLUSH_BLOCKS} signed/positive-zero history-decay blocks; reset; {} fresh-state impulses; {ZERO_FLUSH_BLOCKS} signed/positive-zero impulse-decay blocks",
        analyze_blocks * QMF_BANDS * 2,
        analyze_blocks * QMF_BANDS * 2 * std::mem::size_of::<u64>(),
        IMPULSE_POSITIONS.len(),
        IMPULSE_POSITIONS.len(),
    );
    Ok(())
}

fn benchmark(calls: usize) {
    let blocks = (0..SIGNAL_BLOCKS)
        .map(deterministic_block)
        .collect::<Vec<_>>();
    let mut qmf = ReferenceQmf64F64::new();

    // Warm the OnceLock-backed prototype and phase tables, and prime the state,
    // before starting the timer.
    for block in &blocks {
        black_box(qmf.analyze(black_box(block)));
    }

    let full_passes = calls / blocks.len();
    let remaining = calls % blocks.len();
    let started = Instant::now();
    for _ in 0..full_passes {
        for block in &blocks {
            black_box(qmf.analyze(black_box(block)));
        }
    }
    for block in &blocks[..remaining] {
        black_box(qmf.analyze(black_box(block)));
    }
    let elapsed = started.elapsed();
    eprintln!(
        "bench: {calls} analyze calls, {} input blocks, {:.3} ms total, {:.3} us/call",
        blocks.len(),
        elapsed.as_secs_f64() * 1_000.0,
        elapsed.as_secs_f64() * 1_000_000.0 / calls as f64,
    );
}
