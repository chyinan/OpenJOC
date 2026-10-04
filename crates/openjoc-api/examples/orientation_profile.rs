//! Reproducible control-path benchmark. Run release, optionally with orientation-profile.
//! Arguments: iterations (default 1000), optional trajectory name.
//! CSV times are microseconds per complete pose; allocation and CPU columns
//! describe the entire prepare call, including on stage rows. CPU uses the Linux
//! 64-bit thread clock (zero means unavailable on other targets). Quantiles
//! include rejected calls, reported explicitly in the errors column. Destruction
//! of returned updates, initialization, warmup and pose generation are excluded.
//! Use no profiling feature for final latency measurements.
#![allow(unsafe_code)]
use openjoc_api::{BinauralConfig, ListenerOrientation, ListenerOrientationPreparer};
use openjoc_sofa::BuiltinHrtf;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
    time::Instant,
};
thread_local! { static ALLOC: Cell<Option<(u64,u64)>> = const { Cell::new(None) }; }
struct Counter;
// SAFETY: Every operation delegates to System with unchanged pointer/layout.
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOC.with(|c| {
            if let Some((n, b)) = c.get() {
                c.set(Some((n + 1, b + layout.size() as u64)));
            }
        });
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        ALLOC.with(|c| {
            if let Some((n, b)) = c.get() {
                c.set(Some((n + 1, b + size as u64)));
            }
        });
        unsafe { System.realloc(ptr, layout, size) }
    }
}
#[global_allocator]
static COUNTER: Counter = Counter;
fn pose(i: usize, mode: &str) -> ListenerOrientation {
    if mode == "smooth" {
        let phase = std::f64::consts::TAU * (i % 120) as f64 / 120.0;
        let (y, p, r) = (
            (30.0 * phase.sin()).to_radians() / 2.0,
            (15.0 * (phase * 2.0).sin()).to_radians() / 2.0,
            (10.0 * phase.cos()).to_radians() / 2.0,
        );
        // q_z(yaw) * q_x(pitch) * q_y(roll), matching the original PR probe.
        return ListenerOrientation::new(
            y.cos() * p.sin() * r.cos() - y.sin() * p.cos() * r.sin(),
            y.cos() * p.cos() * r.sin() + y.sin() * p.sin() * r.cos(),
            y.cos() * p.sin() * r.sin() + y.sin() * p.cos() * r.cos(),
            y.cos() * p.cos() * r.cos() - y.sin() * p.sin() * r.sin(),
        )
        .unwrap();
    }
    let angle = match mode {
        "identity" => 0.,
        "nearby" => 0.3 + (i % 4) as f64 * 0.00001,
        _ => ((i * 137) % 360) as f64 * std::f64::consts::PI / 180.,
    };
    // Non-axis-aligned rotation exercises yaw, pitch, roll and repeated adjacent poses.
    let h = angle / 2.;
    ListenerOrientation::new(
        h.sin() * 0.2,
        h.sin() * 0.3,
        h.sin() * (0.87_f64).sqrt(),
        h.cos(),
    )
    .unwrap()
}
#[cfg(all(target_os = "linux", target_pointer_width = "64"))]
fn cpu_ns() -> u64 {
    #[repr(C)]
    struct Timespec {
        seconds: std::ffi::c_long,
        nanoseconds: std::ffi::c_long,
    }
    unsafe extern "C" {
        fn clock_gettime(clock: std::ffi::c_int, time: *mut Timespec) -> std::ffi::c_int;
    }
    let mut time = Timespec {
        seconds: 0,
        nanoseconds: 0,
    };
    // SAFETY: valid writable timespec, Linux CLOCK_THREAD_CPUTIME_ID = 3.
    assert_eq!(unsafe { clock_gettime(3, &raw mut time) }, 0);
    u64::try_from(time.seconds).unwrap() * 1_000_000_000 + u64::try_from(time.nanoseconds).unwrap()
}
#[cfg(not(all(target_os = "linux", target_pointer_width = "64")))]
fn cpu_ns() -> u64 {
    0
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let count = std::env::args()
        .nth(1)
        .map_or(Ok(1000), |s| s.parse::<usize>())?
        .max(20);
    println!(
        "preset,layout,trajectory,stage,p50_us,p95_us,worst_us,mean_allocs,mean_alloc_bytes,cpu_mean_us,errors"
    );
    for preset in BuiltinHrtf::all() {
        for layout in ["2.0", "5.1", "7.1.4", "9.1.6", "22.2"] {
            let prep =
                match ListenerOrientationPreparer::new(&BinauralConfig::builtin(*preset, layout)) {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("unsupported {preset:?} {layout}: {e}");
                        continue;
                    }
                };
            for mode in ["identity", "rapid", "nearby", "smooth"] {
                if std::env::args().nth(2).is_some_and(|filter| filter != mode) {
                    continue;
                }
                for i in 0..30 {
                    black_box(prep.prepare(pose(i, mode), 0, i as u64)).ok();
                }
                let mut samples = (0..7)
                    .map(|_| Vec::with_capacity(count))
                    .collect::<Vec<_>>();
                let mut allocs = 0;
                let mut bytes = 0;
                let mut errors = 0;
                let mut cpu_total = 0;
                for i in 0..count {
                    let orientation = pose(i, mode);
                    #[cfg(feature = "orientation-profile")]
                    let _ = openjoc_sofa::orientation_profile::take();
                    ALLOC.with(|c| c.set(Some((0, 0))));
                    let cpu_start = cpu_ns();
                    let start = Instant::now();
                    let result = black_box(prep.prepare(black_box(orientation), 0, i as u64));
                    let elapsed = start.elapsed().as_nanos() as u64;
                    cpu_total += cpu_ns() - cpu_start;
                    let (n, b) = ALLOC.with(|c| c.replace(None).unwrap());
                    allocs += n;
                    bytes += b;
                    samples[0].push(elapsed);
                    #[cfg(feature = "orientation-profile")]
                    for (j, t) in openjoc_sofa::orientation_profile::take()
                        .into_iter()
                        .enumerate()
                    {
                        samples[j + 1].push(t);
                    }
                    if result.is_err() {
                        errors += 1;
                    }
                    black_box(result).ok();
                }
                for (stage, values) in [
                    "total",
                    "transform",
                    "exact",
                    "ranking",
                    "geometry",
                    "taps",
                    "update",
                ]
                .into_iter()
                .zip(&mut samples)
                {
                    if values.is_empty() {
                        continue;
                    }
                    values.sort_unstable();
                    println!(
                        "{preset:?},{layout},{mode},{stage},{:.3},{:.3},{:.3},{:.2},{:.2},{:.3},{errors}",
                        values[count / 2] as f64 / 1000.,
                        values[(count * 95 / 100).min(count - 1)] as f64 / 1000.,
                        values[count - 1] as f64 / 1000.,
                        allocs as f64 / count as f64,
                        bytes as f64 / count as f64,
                        cpu_total as f64 / count as f64 / 1000.
                    );
                }
            }
        }
    }
    Ok(())
}
