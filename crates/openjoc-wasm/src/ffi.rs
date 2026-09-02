// pattern: Imperative Shell

//! Small raw-WASM ABI. JavaScript owns the wrapper; this module only translates
//! bounded memory and status operations into the existing Rust bridge.

#![allow(unsafe_code)]

use super::{Decoder, DecoderStatus, performance::PerformanceSummary};
use openjoc_api::DialnormMode;
use std::{
    alloc::{Layout, alloc, dealloc},
    cell::RefCell,
    panic::{AssertUnwindSafe, catch_unwind},
    slice,
};

const STATUS_NEED_MORE_INPUT: i32 = 0;
const STATUS_FRAME_AVAILABLE: i32 = 1;
const STATUS_OUTPUT_PENDING: i32 = 2;
const STATUS_END_OF_STREAM: i32 = 3;
const STATUS_ERROR: i32 = -1;
const NO_PTS_SAMPLES: i64 = i64::MIN;
const MAX_WASM_ALLOCATION_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy)]
struct WasmAllocation {
    pointer: u32,
    length: usize,
}

thread_local! {
    static DECODERS: RefCell<Vec<Option<Decoder>>> = const { RefCell::new(Vec::new()) };
    static ALLOCATIONS: RefCell<Vec<WasmAllocation>> = const { RefCell::new(Vec::new()) };
}

fn with_decoder<R>(handle: u32, operation: impl FnOnce(&mut Decoder) -> R) -> Option<R> {
    let index = usize::try_from(handle).ok()?.checked_sub(1)?;
    DECODERS.with(|decoders| {
        decoders
            .borrow_mut()
            .get_mut(index)
            .and_then(Option::as_mut)
            .map(operation)
    })
}

fn guarded_status(handle: u32, operation: impl FnOnce(&mut Decoder) -> DecoderStatus) -> i32 {
    match catch_unwind(AssertUnwindSafe(|| with_decoder(handle, operation))) {
        Ok(Some(status)) => with_decoder(handle, |decoder| {
            if decoder.last_error().is_some() {
                STATUS_ERROR
            } else {
                status_code(status)
            }
        })
        .unwrap_or(STATUS_ERROR),
        Ok(None) => STATUS_ERROR,
        Err(_) => {
            let _ = with_decoder(handle, Decoder::mark_internal_failure);
            STATUS_ERROR
        }
    }
}

fn status_code(status: DecoderStatus) -> i32 {
    match status {
        DecoderStatus::NeedMoreInput => STATUS_NEED_MORE_INPUT,
        DecoderStatus::FrameAvailable => STATUS_FRAME_AVAILABLE,
        DecoderStatus::OutputPending => STATUS_OUTPUT_PENDING,
        DecoderStatus::EndOfStream => STATUS_END_OF_STREAM,
        DecoderStatus::Error => STATUS_ERROR,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_create() -> u32 {
    openjoc_wasm_decoder_create_with_dialnorm(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_create_with_dialnorm(mode: u32) -> u32 {
    let dialnorm = match mode {
        0 => DialnormMode::Default,
        1 => DialnormMode::Analog,
        _ => return 0,
    };
    let Ok(decoder) = Decoder::new_with_dialnorm(dialnorm) else {
        return 0;
    };
    DECODERS.with(|decoders| {
        let mut decoders = decoders.borrow_mut();
        if let Some((index, slot)) = decoders
            .iter_mut()
            .enumerate()
            .find(|(_, slot)| slot.is_none())
        {
            *slot = Some(decoder);
            u32::try_from(index + 1).unwrap_or(0)
        } else {
            decoders.push(Some(decoder));
            u32::try_from(decoders.len()).unwrap_or(0)
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_destroy(handle: u32) {
    let index = usize::try_from(handle)
        .ok()
        .and_then(|value| value.checked_sub(1));
    if let Some(index) = index {
        DECODERS.with(|decoders| {
            if let Some(slot) = decoders.borrow_mut().get_mut(index) {
                *slot = None;
            }
        });
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_alloc(length: u32) -> u32 {
    let Ok(length) = usize::try_from(length) else {
        return 0;
    };
    if length == 0 {
        return 0;
    }
    if length > MAX_WASM_ALLOCATION_BYTES {
        return 0;
    }
    let Ok(layout) = Layout::array::<u8>(length) else {
        return 0;
    };
    // SAFETY: `layout` is a valid non-zero allocation layout and the pointer
    // is returned to JavaScript for exactly one matching deallocation.
    let pointer = unsafe { alloc(layout) };
    if pointer.is_null() || pointer as usize > usize::try_from(u32::MAX).unwrap_or(usize::MAX) {
        if !pointer.is_null() {
            unsafe { dealloc(pointer, layout) };
        }
        return 0;
    }
    let pointer = pointer as usize as u32;
    let tracked = ALLOCATIONS.with(|allocations| {
        let mut allocations = allocations.borrow_mut();
        if allocations.try_reserve(1).is_err() {
            return false;
        }
        allocations.push(WasmAllocation { pointer, length });
        true
    });
    if !tracked {
        unsafe { dealloc(pointer as usize as *mut u8, layout) };
        return 0;
    }
    pointer
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn openjoc_wasm_dealloc(pointer: u32, length: u32) {
    if pointer == 0 {
        return;
    }
    let Ok(length) = usize::try_from(length) else {
        return;
    };
    let Some(allocation) = ALLOCATIONS.with(|allocations| {
        let mut allocations = allocations.borrow_mut();
        allocations
            .iter()
            .position(|allocation| allocation.pointer == pointer && allocation.length == length)
            .map(|index| allocations.remove(index))
    }) else {
        return;
    };
    let Ok(layout) = Layout::array::<u8>(allocation.length) else {
        return;
    };
    // SAFETY: The JavaScript wrapper passes the exact pointer, length, and
    // length returned/used by openjoc_wasm_alloc.
    unsafe { dealloc(pointer as usize as *mut u8, layout) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn openjoc_wasm_decoder_push_bytes(
    handle: u32,
    pointer: u32,
    length: u32,
) -> i32 {
    let Ok(length) = usize::try_from(length) else {
        return STATUS_ERROR;
    };
    if length != 0 && pointer == 0 {
        return STATUS_ERROR;
    }
    if length != 0 && !known_allocation(pointer, length) {
        return STATUS_ERROR;
    }
    if !wasm_memory_range_valid(pointer, length) {
        return STATUS_ERROR;
    }
    // SAFETY: The caller guarantees that the pointer references `length`
    // readable bytes in this module's linear memory.
    let bytes = unsafe { slice::from_raw_parts(pointer as usize as *const u8, length) };
    guarded_status(handle, |decoder| decoder.push_bytes(bytes))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn openjoc_wasm_decoder_push_packet(
    handle: u32,
    pointer: u32,
    length: u32,
    pts_samples: i64,
    flags: u32,
) -> i32 {
    let Ok(length) = usize::try_from(length) else {
        return STATUS_ERROR;
    };
    if length == 0 || pointer == 0 || !known_allocation(pointer, length) {
        return STATUS_ERROR;
    }
    if !wasm_memory_range_valid(pointer, length) {
        return STATUS_ERROR;
    }
    // SAFETY: The caller guarantees that the pointer references `length`
    // readable bytes in this module's linear memory.
    let bytes = unsafe { slice::from_raw_parts(pointer as usize as *const u8, length) };
    let pts = (pts_samples != NO_PTS_SAMPLES).then_some(pts_samples);
    let discontinuity = (flags & 1) != 0;
    let preroll = (flags & 2) != 0;
    guarded_status(handle, |decoder| {
        decoder.push_packet(bytes, pts, discontinuity, preroll)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_flush(handle: u32) -> i32 {
    match catch_unwind(AssertUnwindSafe(|| with_decoder(handle, Decoder::flush))) {
        Ok(Some(Ok(status))) => with_decoder(handle, |decoder| {
            if decoder.last_error().is_some() {
                STATUS_ERROR
            } else {
                status_code(status)
            }
        })
        .unwrap_or(STATUS_ERROR),
        Ok(Some(Err(_))) => STATUS_ERROR,
        Ok(None) | Err(_) => {
            let _ = with_decoder(handle, Decoder::mark_internal_failure);
            STATUS_ERROR
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_reset(handle: u32) -> i32 {
    match catch_unwind(AssertUnwindSafe(|| with_decoder(handle, Decoder::reset))) {
        Ok(Some(())) => 0,
        Ok(None) | Err(_) => STATUS_ERROR,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_receive_pcm(handle: u32) -> i32 {
    with_decoder(handle, |decoder| {
        i32::from(decoder.expose_current_pcm().is_some())
    })
    .unwrap_or(STATUS_ERROR)
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_consume_pcm(handle: u32) -> i32 {
    with_decoder(handle, |decoder| i32::from(decoder.consume_current_pcm())).unwrap_or(STATUS_ERROR)
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_pcm_ptr(handle: u32) -> u32 {
    with_decoder(handle, |decoder| {
        decoder
            .expose_current_pcm()
            .map_or(0, |frame| frame.interleaved_f32.as_ptr() as usize as u32)
    })
    .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_pcm_len(handle: u32) -> u32 {
    with_decoder(handle, |decoder| {
        decoder.expose_current_pcm().map_or(0, |frame| {
            u32::try_from(frame.interleaved_f32.len()).unwrap_or(u32::MAX)
        })
    })
    .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_pcm_samples(handle: u32) -> u32 {
    with_decoder(handle, |decoder| {
        decoder.expose_current_pcm().map_or(0, |frame| {
            u32::try_from(frame.sample_count).unwrap_or(u32::MAX)
        })
    })
    .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_pcm_pts_samples(handle: u32) -> i64 {
    with_decoder(handle, |decoder| {
        decoder
            .expose_current_pcm()
            .and_then(|frame| frame.pts_samples)
            .unwrap_or(NO_PTS_SAMPLES)
    })
    .unwrap_or(NO_PTS_SAMPLES)
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_sample_rate(handle: u32) -> u32 {
    with_decoder(handle, |decoder| decoder.status().sample_rate.unwrap_or(0)).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_channel_count(handle: u32) -> u32 {
    with_decoder(handle, |decoder| {
        u32::try_from(decoder.status().output_channels).unwrap_or(0)
    })
    .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_queued_audio_ms(handle: u32) -> f64 {
    with_decoder(handle, |decoder| decoder.status().queued_audio_ms).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_decoded_access_units(handle: u32) -> u32 {
    with_decoder(handle, |decoder| {
        u32::try_from(decoder.status().decoded_access_units).unwrap_or(u32::MAX)
    })
    .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_output_frames(handle: u32) -> u32 {
    with_decoder(handle, |decoder| {
        u32::try_from(decoder.status().output_frames).unwrap_or(u32::MAX)
    })
    .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_output_samples(handle: u32) -> u64 {
    with_decoder(handle, |decoder| decoder.status().output_samples).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_error_ptr(handle: u32) -> u32 {
    with_decoder(handle, |decoder| {
        decoder
            .last_error()
            .map_or(0, |error| error.detail.as_ptr() as usize as u32)
    })
    .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_error_len(handle: u32) -> u32 {
    with_decoder(handle, |decoder| {
        decoder.last_error().map_or(0, |error| {
            u32::try_from(error.detail.len()).unwrap_or(u32::MAX)
        })
    })
    .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_error_category(handle: u32) -> u32 {
    with_decoder(handle, |decoder| {
        decoder.error_category_code().unwrap_or(u32::MAX)
    })
    .unwrap_or(u32::MAX)
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_profile_ptr(handle: u32) -> u32 {
    with_decoder(handle, |decoder| {
        decoder
            .profile_bytes()
            .map_or(0, |profile| profile.as_ptr() as usize as u32)
    })
    .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_profile_len(handle: u32) -> u32 {
    with_decoder(handle, |decoder| {
        decoder.profile_bytes().map_or(0, |profile| {
            u32::try_from(profile.len()).unwrap_or(u32::MAX)
        })
    })
    .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_downmix_index(handle: u32) -> u32 {
    with_decoder(handle, |decoder| {
        decoder.downmix_index().map_or(u32::MAX, u32::from)
    })
    .unwrap_or(u32::MAX)
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_object_count(handle: u32) -> u32 {
    with_decoder(handle, |decoder| {
        decoder.object_count().map_or(u32::MAX, u32::from)
    })
    .unwrap_or(u32::MAX)
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_complexity_index(handle: u32) -> u32 {
    with_decoder(handle, |decoder| {
        decoder.complexity_index().map_or(u32::MAX, u32::from)
    })
    .unwrap_or(u32::MAX)
}

fn performance_value(handle: u32, selector: fn(PerformanceSummary) -> f64) -> f64 {
    with_decoder(handle, |decoder| selector(decoder.performance_summary())).unwrap_or(0.0)
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_decode_mean_ms(handle: u32) -> f64 {
    performance_value(handle, |summary| summary.decode_mean_ms)
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_decode_p95_ms(handle: u32) -> f64 {
    performance_value(handle, |summary| summary.decode_p95_ms)
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_decode_max_ms(handle: u32) -> f64 {
    performance_value(handle, |summary| summary.decode_max_ms)
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_render_mean_ms(handle: u32) -> f64 {
    performance_value(handle, |summary| summary.render_mean_ms)
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_render_p95_ms(handle: u32) -> f64 {
    performance_value(handle, |summary| summary.render_p95_ms)
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_render_max_ms(handle: u32) -> f64 {
    performance_value(handle, |summary| summary.render_max_ms)
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_total_mean_ms(handle: u32) -> f64 {
    performance_value(handle, |summary| summary.total_mean_ms)
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_total_p95_ms(handle: u32) -> f64 {
    performance_value(handle, |summary| summary.total_p95_ms)
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_total_max_ms(handle: u32) -> f64 {
    performance_value(handle, |summary| summary.total_max_ms)
}

#[unsafe(no_mangle)]
pub extern "C" fn openjoc_wasm_decoder_realtime_factor(handle: u32) -> f64 {
    with_decoder(handle, |decoder| {
        decoder.performance_summary().realtime_factor.unwrap_or(0.0)
    })
    .unwrap_or(0.0)
}

fn wasm_memory_range_valid(pointer: u32, length: usize) -> bool {
    let Some(end) = usize::try_from(pointer)
        .ok()
        .and_then(|start| start.checked_add(length))
    else {
        return false;
    };
    #[cfg(target_arch = "wasm32")]
    {
        let pages = usize::try_from(core::arch::wasm32::memory_size(0)).unwrap_or(0);
        return end <= pages.saturating_mul(65_536);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = end;
        true
    }
}

fn known_allocation(pointer: u32, length: usize) -> bool {
    ALLOCATIONS.with(|allocations| {
        allocations
            .borrow()
            .iter()
            .any(|allocation| allocation.pointer == pointer && allocation.length == length)
    })
}

#[cfg(test)]
mod tests {
    use super::{
        known_allocation, openjoc_wasm_dealloc, openjoc_wasm_decoder_create,
        openjoc_wasm_decoder_create_with_dialnorm, openjoc_wasm_decoder_destroy,
        openjoc_wasm_decoder_error_category, openjoc_wasm_decoder_pcm_pts_samples,
        openjoc_wasm_decoder_push_bytes, openjoc_wasm_decoder_push_packet,
        openjoc_wasm_decoder_receive_pcm,
    };

    #[test]
    fn untracked_wasm_allocation_is_rejected() {
        assert!(!known_allocation(0x1000, 64));
    }

    #[test]
    fn raw_abi_rejects_invalid_handles_and_untracked_input() {
        assert_eq!(openjoc_wasm_decoder_receive_pcm(0), -1);
        assert_eq!(openjoc_wasm_decoder_error_category(0), u32::MAX);
        let handle = openjoc_wasm_decoder_create();
        assert_ne!(handle, 0);
        let status = unsafe { openjoc_wasm_decoder_push_bytes(handle, 0x1000, 64) };
        assert_eq!(status, -1);
        unsafe { openjoc_wasm_dealloc(0x1000, 64) };
        unsafe { openjoc_wasm_dealloc(0x1000, 64) };
        openjoc_wasm_decoder_destroy(handle);
    }

    #[test]
    fn timestamped_packet_abi_rejects_missing_input_and_exposes_empty_pts() {
        let handle = openjoc_wasm_decoder_create();
        assert_ne!(handle, 0);
        assert_eq!(
            unsafe { openjoc_wasm_decoder_push_packet(handle, 0, 0, 0, 1) },
            -1
        );
        assert_eq!(openjoc_wasm_decoder_pcm_pts_samples(handle), i64::MIN);
        openjoc_wasm_decoder_destroy(handle);
    }

    #[test]
    fn dialnorm_abi_rejects_unknown_mode() {
        assert_eq!(openjoc_wasm_decoder_create_with_dialnorm(99), 0);
    }
}
