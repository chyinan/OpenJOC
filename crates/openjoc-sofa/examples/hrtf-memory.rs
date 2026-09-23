// pattern: Mixed (unavoidable)
// Reason: this release benchmark combines measured Rust heap allocation
// accounting with the production packed-asset loader and binaural renderer.

#![allow(unsafe_code)]

use std::{
    alloc::{GlobalAlloc, Layout, System},
    fs,
    mem::size_of,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};

use openjoc_render::{
    BinauralRenderer, BinauralSourceBlock, CartesianPosition, HrirBank, HrirEntry, HrirEntryId,
    SourceId, StaticBinauralSource,
};
use openjoc_sofa::{BuiltinHrtf, load_builtin_hrir_f32_from_asset, resolve_hrir_f32};

struct CountingAllocator;

static CURRENT_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_BYTES: AtomicUsize = AtomicUsize::new(0);

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            account_add(layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            account_add(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
        account_sub(layout.size());
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let replacement = unsafe { System.realloc(pointer, layout, new_size) };
        if !replacement.is_null() {
            if new_size >= layout.size() {
                account_add(new_size - layout.size());
            } else {
                account_sub(layout.size() - new_size);
            }
        }
        replacement
    }
}

fn account_add(bytes: usize) {
    let current = CURRENT_BYTES.fetch_add(bytes, Ordering::Relaxed) + bytes;
    PEAK_BYTES.fetch_max(current, Ordering::Relaxed);
}

fn account_sub(bytes: usize) {
    CURRENT_BYTES.fetch_sub(bytes, Ordering::Relaxed);
}

fn reset_peak() {
    PEAK_BYTES.store(CURRENT_BYTES.load(Ordering::Relaxed), Ordering::Relaxed);
}

fn report(stage: &str, elapsed_ms: f64, baseline: usize, extra: &str) {
    let live = CURRENT_BYTES
        .load(Ordering::Relaxed)
        .saturating_sub(baseline);
    let peak = PEAK_BYTES.load(Ordering::Relaxed).saturating_sub(baseline);
    println!(
        "stage={stage} elapsed_ms={elapsed_ms:.3} live_heap_bytes={live} stage_peak_heap_bytes={peak} {extra}"
    );
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directions = [
        CartesianPosition::new(0.0, 1.0, 0.0),
        CartesianPosition::new(-1.0, 1.0, 0.0),
        CartesianPosition::new(-1.0, 0.0, 0.0),
        CartesianPosition::new(-1.0, -1.0, 0.0),
        CartesianPosition::new(-1.0, 1.0, 1.0),
        CartesianPosition::new(-1.0, 0.0, 1.0),
        CartesianPosition::new(-1.0, -1.0, 1.0),
        CartesianPosition::new(0.0, 0.0, 1.0),
    ];
    let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");

    for preset in BuiltinHrtf::all() {
        reset_peak();
        let baseline = CURRENT_BYTES.load(Ordering::Relaxed);
        let path = assets.join(preset.asset_metadata().asset_file);
        let started = Instant::now();
        let asset = fs::read(path)?;
        let asset_load_ms = started.elapsed().as_secs_f64() * 1_000.0;
        report(
            "after_asset_load",
            asset_load_ms,
            baseline,
            &format!(
                "preset={} asset_input_bytes={} wasm_rust_copy_bytes=0",
                preset.id(),
                asset.len()
            ),
        );

        reset_peak();
        let started = Instant::now();
        let loaded = load_builtin_hrir_f32_from_asset(*preset, &asset)?;
        let parse_ms = started.elapsed().as_secs_f64() * 1_000.0;
        report(
            "after_packed_asset_parse",
            parse_ms,
            baseline,
            &format!(
                "preset={} directions={} asset_input_bytes={} f32_tap_bytes={} direction_metadata_bytes={} resident_bank_bytes={} full_f64_bank_bytes=0 spatial_index_resident_bytes=0",
                preset.id(),
                loaded.bank.direction_count(),
                asset.len(),
                loaded.bank.tap_storage_bytes(),
                loaded.bank.direction_metadata_storage_bytes(),
                loaded.bank.resident_storage_bytes(),
            ),
        );

        reset_peak();
        let started = Instant::now();
        let mut entries = Vec::with_capacity(directions.len());
        let mut sources = Vec::with_capacity(directions.len());
        let mut prepared_f64_tap_bytes = 0usize;
        let mut max_single_kernel_bytes = 0usize;
        for (index, direction) in directions.into_iter().enumerate() {
            let entry_id = HrirEntryId::new(index as u64 + 1);
            let resolved = resolve_hrir_f32(&loaded.bank, direction)?;
            let pair_bytes = resolved.pair.tap_count() * 2 * size_of::<f64>();
            prepared_f64_tap_bytes += pair_bytes;
            max_single_kernel_bytes = max_single_kernel_bytes.max(pair_bytes);
            entries.push(HrirEntry::new(entry_id, direction, resolved.pair)?);
            sources.push(StaticBinauralSource::new(
                SourceId::new(index as u64 + 1),
                direction,
                1.0,
                entry_id,
            )?);
        }
        let prepared_bank = HrirBank::new(loaded.metadata.sample_rate_hz, entries)?;
        let prepared_kernel_capacity_bytes = prepared_bank
            .entries()
            .iter()
            .map(|entry| {
                (entry.pair().left_taps().len() + entry.pair().right_taps().len())
                    * size_of::<f64>()
            })
            .sum::<usize>();
        drop(loaded);
        drop(asset);
        let conversion_ms = started.elapsed().as_secs_f64() * 1_000.0;
        report(
            "after_internal_conversion",
            conversion_ms,
            baseline,
            &format!(
                "preset={} prepared_directions={} f32_bank_retained_bytes=0 full_f64_bank_bytes=0 prepared_f64_kernel_bytes={prepared_f64_tap_bytes} prepared_kernel_capacity_bytes={prepared_kernel_capacity_bytes} max_single_interpolated_kernel_bytes={max_single_kernel_bytes}",
                preset.id(),
                prepared_bank.entries().len(),
            ),
        );

        reset_peak();
        let started = Instant::now();
        let mut renderer = BinauralRenderer::new(48_000, prepared_bank, sources)?;
        let renderer_init_ms = started.elapsed().as_secs_f64() * 1_000.0;
        report(
            "after_renderer_initialization",
            renderer_init_ms,
            baseline,
            &format!(
                "preset={} renderer_kernel_bytes={} history_state_bytes={} total_renderer_hrir_bytes={}",
                preset.id(),
                renderer.hrir_kernel_storage_bytes(),
                renderer.hrir_history_storage_bytes(),
                renderer.hrir_tap_storage_bytes(),
            ),
        );

        reset_peak();
        let started = Instant::now();
        let mut input = [0.0; 256];
        input[0] = 1.0;
        let blocks = (0..directions.len())
            .map(|index| BinauralSourceBlock::new(SourceId::new(index as u64 + 1), &input))
            .collect::<Vec<_>>();
        let mut left = [0.0; 256];
        let mut right = [0.0; 256];
        renderer.render_block(&blocks, &mut left, &mut right)?;
        let first_render_ms = started.elapsed().as_secs_f64() * 1_000.0;
        let output_is_finite = left.iter().chain(&right).all(|sample| sample.is_finite());
        report(
            "after_first_render",
            first_render_ms,
            baseline,
            &format!("preset={} output_finite={output_is_finite}", preset.id()),
        );

        reset_peak();
        let started = Instant::now();
        for _ in 0..200 {
            renderer.render_block(&blocks, &mut left, &mut right)?;
        }
        let steady_render_ms = started.elapsed().as_secs_f64() * 1_000.0;
        report(
            "steady_render_51200_samples_8_sources",
            steady_render_ms,
            baseline,
            &format!("preset={}", preset.id()),
        );
        drop(blocks);

        reset_peak();
        let started = Instant::now();
        drop(renderer);
        let switch_away_ms = started.elapsed().as_secs_f64() * 1_000.0;
        report(
            "after_switching_away",
            switch_away_ms,
            baseline,
            &format!(
                "preset={} residual_heap_bytes={}",
                preset.id(),
                CURRENT_BYTES
                    .load(Ordering::Relaxed)
                    .saturating_sub(baseline)
            ),
        );
    }

    Ok(())
}
