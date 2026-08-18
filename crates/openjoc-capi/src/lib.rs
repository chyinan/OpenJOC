//! Stable C-compatible adapter for [`openjoc_api`].
//!
//! This crate deliberately contains no decoder implementation. Every handle
//! owns one `OpenJocSession`, and every exported function catches panics before
//! returning across the ABI boundary.

#![allow(unsafe_code)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::needless_pass_by_value)]
#![allow(clippy::not_unsafe_ptr_arg_deref)]
#![allow(clippy::too_many_lines)]
#![allow(clippy::default_trait_access)]
#![allow(non_camel_case_types)]

use openjoc_api::{
    BinauralConfig, BinauralLfePolicy, DownmixPolicy, DrcPolicy, OpenJocConfig, OpenJocError,
    OpenJocPacket, OpenJocPcmFrame, OpenJocSession, OpenJocStatus, RenderMode, ValidationProfile,
};
use std::{
    ffi::{CStr, CString},
    os::raw::c_char,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr, slice,
};

/// Major C ABI version. It is intentionally independent from the package
/// version and follows the compatibility policy in `docs/C_API.md`.
pub const OPENJOC_ABI_VERSION_MAJOR: u32 = 1;
/// Experimental ABI minor version.
pub const OPENJOC_ABI_VERSION_MINOR: u32 = 0;
const NO_PTS: i64 = i64::MIN;

#[repr(C)]
pub struct openjoc_decoder {
    session: OpenJocSession,
    last_error: CString,
    layout_name: CString,
    channel_labels: Vec<CString>,
    channel_label_ptrs: Vec<*const c_char>,
    last_frame: Option<OpenJocPcmFrame>,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum openjoc_status {
    OPENJOC_STATUS_OK = 0,
    OPENJOC_STATUS_NEED_MORE_INPUT = 1,
    OPENJOC_STATUS_FRAME_AVAILABLE = 2,
    OPENJOC_STATUS_END_OF_STREAM = 3,
    OPENJOC_STATUS_OUTPUT_PENDING = 4,
    OPENJOC_STATUS_UNSUPPORTED = 5,
    OPENJOC_STATUS_INVALID_ARGUMENT = 6,
    OPENJOC_STATUS_DECODE_ERROR = 7,
    OPENJOC_STATUS_RENDER_ERROR = 8,
    OPENJOC_STATUS_FORMAT_CHANGED = 9,
    OPENJOC_STATUS_REQUIRE_RESET = 10,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum openjoc_render_mode {
    OPENJOC_RENDER_SPEAKER = 0,
    OPENJOC_RENDER_STEREO = 1,
    OPENJOC_RENDER_BINAURAL = 2,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum openjoc_downmix_policy {
    OPENJOC_DOWNMIX_AUTO = 0,
    OPENJOC_DOWNMIX_LORO = 1,
    OPENJOC_DOWNMIX_LTRT = 2,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum openjoc_drc_mode {
    OPENJOC_DRC_DISABLED = 0,
    OPENJOC_DRC_LINE = 1,
    OPENJOC_DRC_RF = 2,
    OPENJOC_DRC_CUSTOM = 3,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum openjoc_validation_profile {
    OPENJOC_VALIDATION_AUTO = 0,
    OPENJOC_VALIDATION_ETSI_STRICT = 1,
    OPENJOC_VALIDATION_OBSERVED_VENDOR_COMPAT = 2,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum openjoc_lfe_policy {
    OPENJOC_LFE_EXCLUDE = 0,
    OPENJOC_LFE_EQUAL_POWER_DUAL_MONO = 1,
}

pub const OPENJOC_PACKET_FLAG_DISCONTINUITY: u32 = 1;
pub const OPENJOC_PACKET_FLAG_PREROLL: u32 = 2;
pub const OPENJOC_NO_PTS: i64 = NO_PTS;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct openjoc_decoder_config {
    pub struct_size: u32,
    pub render_mode: u32,
    pub speaker_layout: *const c_char,
    pub downmix: u32,
    pub drc: u32,
    pub drc_boost_percent: u8,
    pub drc_cut_percent: u8,
    pub validation_profile: u32,
    pub sofa_data: *const u8,
    pub sofa_size: usize,
    pub virtual_layout: *const c_char,
    pub lfe_policy: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct openjoc_pcm_frame {
    pub struct_size: u32,
    pub sample_format: u32,
    pub sample_rate: u32,
    pub channel_count: u32,
    pub sample_count: usize,
    pub pts_samples: i64,
    pub data: *const f32,
    pub data_len: usize,
    pub layout_name: *const c_char,
    pub channel_labels: *const *const c_char,
    pub channel_label_count: usize,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct openjoc_output_info {
    pub struct_size: u32,
    pub sample_format: u32,
    pub sample_rate: u32,
    pub channel_count: u32,
    pub latency_samples: usize,
    pub layout_name: *const c_char,
    pub channel_labels: *const *const c_char,
    pub channel_label_count: usize,
}

const CONFIG_SIZE: u32 = std::mem::size_of::<openjoc_decoder_config>() as u32;
const FRAME_SIZE: u32 = std::mem::size_of::<openjoc_pcm_frame>() as u32;
const INFO_SIZE: u32 = std::mem::size_of::<openjoc_output_info>() as u32;

fn status(status: OpenJocStatus) -> openjoc_status {
    match status {
        OpenJocStatus::Ok => openjoc_status::OPENJOC_STATUS_OK,
        OpenJocStatus::NeedMoreInput => openjoc_status::OPENJOC_STATUS_NEED_MORE_INPUT,
        OpenJocStatus::FrameAvailable => openjoc_status::OPENJOC_STATUS_FRAME_AVAILABLE,
        OpenJocStatus::EndOfStream => openjoc_status::OPENJOC_STATUS_END_OF_STREAM,
        OpenJocStatus::OutputPending => openjoc_status::OPENJOC_STATUS_OUTPUT_PENDING,
    }
}

fn error_status(error: &OpenJocError) -> openjoc_status {
    match error {
        OpenJocError::InvalidConfig(_) | OpenJocError::InvalidPacket(_) => {
            openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT
        }
        OpenJocError::FormatChanged { .. } | OpenJocError::ProfileChanged => {
            openjoc_status::OPENJOC_STATUS_FORMAT_CHANGED
        }
        OpenJocError::Unsupported(_) => openjoc_status::OPENJOC_STATUS_UNSUPPORTED,
        OpenJocError::OutputPending => openjoc_status::OPENJOC_STATUS_OUTPUT_PENDING,
        OpenJocError::Render(_) => openjoc_status::OPENJOC_STATUS_RENDER_ERROR,
        _ => openjoc_status::OPENJOC_STATUS_DECODE_ERROR,
    }
}

fn set_error(decoder: &mut openjoc_decoder, error: OpenJocError) -> openjoc_status {
    let result = error_status(&error);
    decoder.last_error = CString::new(error.to_string())
        .unwrap_or_else(|_| CString::new("OpenJOC error contains NUL").expect("static error"));
    result
}

fn set_message(decoder: &mut openjoc_decoder, message: impl ToString) -> openjoc_status {
    decoder.last_error = CString::new(message.to_string())
        .unwrap_or_else(|_| CString::new("OpenJOC error contains NUL").expect("static error"));
    openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT
}

fn c_string(pointer: *const c_char, name: &str) -> Result<String, OpenJocError> {
    if pointer.is_null() {
        return Err(OpenJocError::InvalidConfig(format!("{name} is null")));
    }
    // SAFETY: callers of the C API must pass a valid NUL-terminated string.
    unsafe { CStr::from_ptr(pointer) }
        .to_str()
        .map(str::to_owned)
        .map_err(|_| OpenJocError::InvalidConfig(format!("{name} is not UTF-8")))
}

fn config_from_c(config: &openjoc_decoder_config) -> Result<OpenJocConfig, OpenJocError> {
    if config.struct_size < CONFIG_SIZE {
        return Err(OpenJocError::InvalidConfig(
            "config.struct_size is too small".to_owned(),
        ));
    }
    let speaker_layout = if config.speaker_layout.is_null() {
        "5.1".to_owned()
    } else {
        c_string(config.speaker_layout, "speaker_layout")?
    };
    let downmix = match config.downmix {
        value if value == openjoc_downmix_policy::OPENJOC_DOWNMIX_AUTO as u32 => {
            DownmixPolicy::Auto
        }
        value if value == openjoc_downmix_policy::OPENJOC_DOWNMIX_LORO as u32 => {
            DownmixPolicy::LoRo
        }
        value if value == openjoc_downmix_policy::OPENJOC_DOWNMIX_LTRT as u32 => {
            DownmixPolicy::LtRt
        }
        _ => {
            return Err(OpenJocError::InvalidConfig(
                "unknown downmix policy".to_owned(),
            ));
        }
    };
    let drc = match config.drc {
        value if value == openjoc_drc_mode::OPENJOC_DRC_DISABLED as u32 => DrcPolicy::Disabled,
        value if value == openjoc_drc_mode::OPENJOC_DRC_LINE as u32 => DrcPolicy::Line,
        value if value == openjoc_drc_mode::OPENJOC_DRC_RF as u32 => DrcPolicy::Rf,
        value if value == openjoc_drc_mode::OPENJOC_DRC_CUSTOM as u32 => DrcPolicy::Custom {
            boost_percent: config.drc_boost_percent,
            cut_percent: config.drc_cut_percent,
        },
        _ => return Err(OpenJocError::InvalidConfig("unknown DRC mode".to_owned())),
    };
    let validation_profile = match config.validation_profile {
        value if value == openjoc_validation_profile::OPENJOC_VALIDATION_AUTO as u32 => {
            ValidationProfile::Auto
        }
        value if value == openjoc_validation_profile::OPENJOC_VALIDATION_ETSI_STRICT as u32 => {
            ValidationProfile::EtsiStrict
        }
        value
            if value
                == openjoc_validation_profile::OPENJOC_VALIDATION_OBSERVED_VENDOR_COMPAT as u32 =>
        {
            ValidationProfile::ObservedVendorCompat
        }
        _ => {
            return Err(OpenJocError::InvalidConfig(
                "unknown validation profile".to_owned(),
            ));
        }
    };
    let render_mode = match config.render_mode {
        value if value == openjoc_render_mode::OPENJOC_RENDER_SPEAKER as u32 => RenderMode::Speaker,
        value if value == openjoc_render_mode::OPENJOC_RENDER_STEREO as u32 => RenderMode::Stereo,
        value if value == openjoc_render_mode::OPENJOC_RENDER_BINAURAL as u32 => {
            RenderMode::Binaural
        }
        _ => {
            return Err(OpenJocError::InvalidConfig(
                "unknown render mode".to_owned(),
            ));
        }
    };
    let binaural = if render_mode == RenderMode::Binaural {
        if config.sofa_data.is_null() || config.sofa_size == 0 {
            return Err(OpenJocError::InvalidConfig(
                "binaural mode requires sofa_data and sofa_size".to_owned(),
            ));
        }
        // SAFETY: the caller owns a readable buffer for the duration of create.
        let bytes = unsafe { slice::from_raw_parts(config.sofa_data, config.sofa_size) }.to_vec();
        let virtual_layout = if config.virtual_layout.is_null() {
            speaker_layout.clone()
        } else {
            c_string(config.virtual_layout, "virtual_layout")?
        };
        Some(BinauralConfig {
            sofa_bytes: bytes,
            virtual_layout,
            lfe_policy: match config.lfe_policy {
                value if value == openjoc_lfe_policy::OPENJOC_LFE_EXCLUDE as u32 => {
                    BinauralLfePolicy::Exclude
                }
                value if value == openjoc_lfe_policy::OPENJOC_LFE_EQUAL_POWER_DUAL_MONO as u32 => {
                    BinauralLfePolicy::EqualPowerDualMono
                }
                _ => return Err(OpenJocError::InvalidConfig("unknown LFE policy".to_owned())),
            },
        })
    } else {
        None
    };
    Ok(OpenJocConfig {
        render_mode,
        speaker_layout,
        downmix,
        drc,
        validation_profile,
        oamd: Default::default(),
        binaural,
    })
}

fn labels_for(session: &OpenJocSession) -> Vec<CString> {
    session
        .output_info()
        .channel_labels
        .into_iter()
        .map(|label| CString::new(label).expect("layout labels contain no NUL"))
        .collect()
}

fn panic_status(decoder: &mut openjoc_decoder) -> openjoc_status {
    decoder.last_error =
        CString::new("panic contained at OpenJOC C ABI boundary").expect("static error");
    openjoc_status::OPENJOC_STATUS_DECODE_ERROR
}

/// Returns the packed ABI version `(major << 16) | minor`.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_get_abi_version() -> u32 {
    (OPENJOC_ABI_VERSION_MAJOR << 16) | OPENJOC_ABI_VERSION_MINOR
}

/// Initializes a forward-compatible configuration structure.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_decoder_config_init(
    config: *mut openjoc_decoder_config,
) -> openjoc_status {
    if config.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: null was checked and the caller supplied writable config storage.
    unsafe {
        *config = openjoc_decoder_config {
            struct_size: CONFIG_SIZE,
            render_mode: openjoc_render_mode::OPENJOC_RENDER_SPEAKER as u32,
            speaker_layout: ptr::null(),
            downmix: openjoc_downmix_policy::OPENJOC_DOWNMIX_AUTO as u32,
            drc: openjoc_drc_mode::OPENJOC_DRC_LINE as u32,
            drc_boost_percent: 100,
            drc_cut_percent: 100,
            validation_profile: openjoc_validation_profile::OPENJOC_VALIDATION_AUTO as u32,
            sofa_data: ptr::null(),
            sofa_size: 0,
            virtual_layout: ptr::null(),
            lfe_policy: openjoc_lfe_policy::OPENJOC_LFE_EXCLUDE as u32,
        };
    }
    openjoc_status::OPENJOC_STATUS_OK
}

/// Creates one independent opaque decoder session.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_decoder_create(
    config: *const openjoc_decoder_config,
    output: *mut *mut openjoc_decoder,
) -> openjoc_status {
    if config.is_null() || output.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: pointers were checked and are valid for this call by the C contract.
        let config = unsafe { &*config };
        let session = OpenJocSession::new(config_from_c(config)?)?;
        let layout_name =
            CString::new(session.output_info().layout_name).expect("layout name contains no NUL");
        let channel_labels = labels_for(&session);
        let channel_label_ptrs = channel_labels
            .iter()
            .map(|label| label.as_c_str().as_ptr())
            .collect();
        let decoder = Box::new(openjoc_decoder {
            session,
            last_error: CString::new("").expect("empty CString"),
            layout_name,
            channel_labels,
            channel_label_ptrs,
            last_frame: None,
        });
        // SAFETY: output was checked and receives ownership of the allocation.
        unsafe { *output = Box::into_raw(decoder) };
        Ok::<openjoc_status, OpenJocError>(openjoc_status::OPENJOC_STATUS_OK)
    }));
    match result {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => error_status(&error),
        Err(_) => openjoc_status::OPENJOC_STATUS_DECODE_ERROR,
    }
}

/// Destroys an opaque decoder handle.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_decoder_destroy(decoder: *mut openjoc_decoder) {
    if decoder.is_null() {
        return;
    }
    // SAFETY: the pointer came from `openjoc_decoder_create` and is consumed once.
    unsafe { drop(Box::from_raw(decoder)) };
}

/// Sends one borrowed complete access unit. The data is never retained.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_decoder_send_packet(
    decoder: *mut openjoc_decoder,
    data: *const u8,
    data_len: usize,
    pts_samples: i64,
    flags: u32,
) -> openjoc_status {
    if decoder.is_null() || data.is_null() || data_len == 0 {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: decoder was checked and remains owned by the caller.
    let decoder = unsafe { &mut *decoder };
    let result = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: caller guarantees a readable packet buffer for this call.
        let bytes = unsafe { slice::from_raw_parts(data, data_len) };
        decoder.last_frame = None;
        decoder
            .session
            .push_packet(OpenJocPacket {
                data: bytes,
                pts_samples: (pts_samples != NO_PTS).then_some(pts_samples),
                discontinuity: flags & OPENJOC_PACKET_FLAG_DISCONTINUITY != 0,
                preroll: flags & OPENJOC_PACKET_FLAG_PREROLL != 0,
            })
            .map(status)
    }));
    match result {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => set_error(decoder, error),
        Err(_) => panic_status(decoder),
    }
}

/// Receives one PCM frame. Returned pointers are valid until the next
/// send/receive/reset/destroy operation on this handle.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_decoder_receive_frame(
    decoder: *mut openjoc_decoder,
    output: *mut openjoc_pcm_frame,
) -> openjoc_status {
    if decoder.is_null() || output.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: pointers were checked; output is caller-owned writable storage.
    let decoder = unsafe { &mut *decoder };
    let result = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: output is valid for the advertised structure size.
        let output = unsafe { &mut *output };
        if output.struct_size < FRAME_SIZE {
            return Err(OpenJocError::InvalidConfig(
                "pcm_frame.struct_size is too small".to_owned(),
            ));
        }
        let Some(frame) = decoder.session.receive_frame() else {
            return Ok(if decoder.session.is_drained() {
                openjoc_status::OPENJOC_STATUS_END_OF_STREAM
            } else {
                openjoc_status::OPENJOC_STATUS_NEED_MORE_INPUT
            });
        };
        decoder.last_frame = Some(frame);
        let frame = decoder.last_frame.as_ref().expect("stored frame");
        output.sample_format = 1;
        output.sample_rate = frame.sample_rate;
        output.channel_count = frame.channel_count as u32;
        output.sample_count = frame.sample_count;
        output.pts_samples = frame.pts_samples.unwrap_or(NO_PTS);
        output.data = frame.interleaved_f32.as_ptr();
        output.data_len = frame.interleaved_f32.len();
        output.layout_name = decoder.layout_name.as_c_str().as_ptr();
        output.channel_labels = decoder.channel_label_ptrs.as_ptr();
        output.channel_label_count = decoder.channel_labels.len();
        Ok(openjoc_status::OPENJOC_STATUS_FRAME_AVAILABLE)
    }));
    match result {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => set_error(decoder, error),
        Err(_) => panic_status(decoder),
    }
}

/// Drains QMF reconstruction and SOFA FIR tail state.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_decoder_drain(decoder: *mut openjoc_decoder) -> openjoc_status {
    if decoder.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: pointer was checked and remains caller-owned.
    let decoder = unsafe { &mut *decoder };
    let result = catch_unwind(AssertUnwindSafe(|| decoder.session.drain()));
    match result {
        Ok(Ok(value)) => status(value),
        Ok(Err(error)) => set_error(decoder, error),
        Err(_) => panic_status(decoder),
    }
}

/// Discards pending PCM and resets stream-derived state.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_decoder_flush(decoder: *mut openjoc_decoder) -> openjoc_status {
    if decoder.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: pointer was checked and remains caller-owned.
    let decoder = unsafe { &mut *decoder };
    let result = catch_unwind(AssertUnwindSafe(|| {
        decoder.last_frame = None;
        decoder.session.flush();
        openjoc_status::OPENJOC_STATUS_OK
    }));
    result.unwrap_or_else(|_| panic_status(decoder))
}

/// Resets semantic/timeline state for a new stream or seek.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_decoder_reset(decoder: *mut openjoc_decoder) -> openjoc_status {
    openjoc_decoder_flush(decoder)
}

/// Returns the instance-owned diagnostic string for the most recent failure.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_decoder_last_error(decoder: *const openjoc_decoder) -> *const c_char {
    if decoder.is_null() {
        return c"invalid null OpenJOC decoder handle".as_ptr();
    }
    // SAFETY: pointer was checked and remains valid for the caller.
    unsafe { (&*decoder).last_error.as_ptr() }
}

/// Initializes an output frame descriptor.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_pcm_frame_init(output: *mut openjoc_pcm_frame) -> openjoc_status {
    if output.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: null was checked and caller supplied writable storage.
    unsafe {
        *output = openjoc_pcm_frame {
            struct_size: FRAME_SIZE,
            sample_format: 1,
            sample_rate: 0,
            channel_count: 0,
            sample_count: 0,
            pts_samples: NO_PTS,
            data: ptr::null(),
            data_len: 0,
            layout_name: ptr::null(),
            channel_labels: ptr::null(),
            channel_label_count: 0,
        };
    }
    openjoc_status::OPENJOC_STATUS_OK
}

/// Initializes an output-info descriptor.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_output_info_init(output: *mut openjoc_output_info) -> openjoc_status {
    if output.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: null was checked and caller supplied writable storage.
    unsafe {
        *output = openjoc_output_info {
            struct_size: INFO_SIZE,
            sample_format: 1,
            sample_rate: 0,
            channel_count: 0,
            latency_samples: 0,
            layout_name: ptr::null(),
            channel_labels: ptr::null(),
            channel_label_count: 0,
        };
    }
    openjoc_status::OPENJOC_STATUS_OK
}

/// Returns semantic output information. The descriptor is valid until the
/// next configuration/state operation on the handle.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_decoder_get_output_info(
    decoder: *mut openjoc_decoder,
    output: *mut openjoc_output_info,
) -> openjoc_status {
    if decoder.is_null() || output.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: pointers were checked and remain caller-owned.
    let decoder = unsafe { &mut *decoder };
    let output = unsafe { &mut *output };
    if output.struct_size < INFO_SIZE {
        return set_message(decoder, "output_info.struct_size is too small");
    }
    let info = decoder.session.output_info();
    output.sample_format = 1;
    output.sample_rate = info.sample_rate.unwrap_or(0);
    output.channel_count = info.channel_count as u32;
    output.latency_samples = info.latency_samples;
    output.layout_name = decoder.layout_name.as_c_str().as_ptr();
    output.channel_labels = decoder.channel_label_ptrs.as_ptr();
    output.channel_label_count = decoder.channel_labels.len();
    openjoc_status::OPENJOC_STATUS_OK
}

/// Returns one channel's stable semantic label.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_decoder_get_channel_label(
    decoder: *const openjoc_decoder,
    index: usize,
) -> *const c_char {
    if decoder.is_null() {
        return ptr::null();
    }
    // SAFETY: pointer was checked and remains valid for the caller.
    unsafe {
        (&*decoder)
            .channel_labels
            .get(index)
            .map_or(ptr::null(), |label| label.as_c_str().as_ptr())
    }
}
