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
    BinauralConfig, BinauralLfePolicy, DialnormMode, DownmixPolicy, DrcPolicy, OpenJocConfig,
    OpenJocError, OpenJocPacket, OpenJocPcmFrame, OpenJocSession, OpenJocStatus, RenderMode,
    ValidationProfile,
};
use openjoc_ffmpeg::{
    BridgeError, BridgeErrorKind, BridgeStatus, FfmpegDecoder, FfmpegFrame, JocClassification,
    JocClassifier, PacketRef, Rational, ReceiveOutcome,
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
pub const OPENJOC_ABI_VERSION_MINOR: u32 = 4;
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

/// Framework-neutral packet/chunk bridge used by native media adapters.
///
/// The contained bridge owns the single proven bounded AU assembler. It does
/// not create its `OpenJocSession` until a complete access unit is positively
/// classified as JOC.
#[repr(C)]
pub struct openjoc_stream_decoder {
    decoder: FfmpegDecoder,
    last_error: CString,
    layout_name: CString,
    channel_labels: Vec<CString>,
    channel_label_ptrs: Vec<*const c_char>,
    config_descriptor: CString,
    config_fingerprint: CString,
    last_frame: Option<FfmpegFrame>,
}

/// Framework-neutral compressed-stream classifier. It never creates a
/// renderer session or emits PCM.
#[repr(C)]
pub struct openjoc_classifier {
    classifier: JocClassifier,
    last_error: CString,
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
    OPENJOC_STATUS_NOT_JOC = 11,
    OPENJOC_STATUS_OUT_OF_MEMORY = 12,
    OPENJOC_STATUS_EXTERNAL_ERROR = 13,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum openjoc_classification {
    OPENJOC_CLASSIFICATION_UNKNOWN = 0,
    OPENJOC_CLASSIFICATION_CONFIRMED_JOC = 1,
    OPENJOC_CLASSIFICATION_CONFIRMED_NON_JOC = 2,
    OPENJOC_CLASSIFICATION_INVALID_OR_UNSUPPORTED = 3,
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
pub enum openjoc_dialnorm_mode {
    OPENJOC_DIALNORM_DEFAULT = 0,
    OPENJOC_DIALNORM_DIGITAL = 1,
    OPENJOC_DIALNORM_ANALOG = 2,
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

/// Role values for [`openjoc_custom_speaker`].
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum openjoc_speaker_role {
    OPENJOC_SPEAKER_FULL_RANGE = 0,
    OPENJOC_SPEAKER_LFE = 1,
}

pub const OPENJOC_PACKET_FLAG_DISCONTINUITY: u32 = 1;
pub const OPENJOC_PACKET_FLAG_PREROLL: u32 = 2;
pub const OPENJOC_NO_PTS: i64 = NO_PTS;

/// One caller-owned custom speaker geometry entry. The strings and array are
/// borrowed only during decoder creation; the validated Rust layout owns its
/// copies after creation returns.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct openjoc_custom_speaker {
    pub struct_size: u32,
    pub name: *const c_char,
    pub azimuth: f64,
    pub elevation: f64,
    pub role: u32,
}

/// Caller-owned ordered custom speaker layout descriptor.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct openjoc_custom_speaker_layout {
    pub struct_size: u32,
    pub version: u32,
    pub name: *const c_char,
    pub speakers: *const openjoc_custom_speaker,
    pub speaker_count: usize,
}

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
    /// Appended in ABI minor 1; older `struct_size` callers use Default.
    pub dialnorm_mode: u32,
    /// Appended in ABI minor 4; null retains preset-name behavior.
    pub custom_speaker_layout: *const openjoc_custom_speaker_layout,
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
const CONFIG_SIZE_BEFORE_CUSTOM: u32 = CONFIG_SIZE - std::mem::size_of::<*const u8>() as u32;
const CONFIG_SIZE_BEFORE_DIALNORM: u32 =
    CONFIG_SIZE_BEFORE_CUSTOM - std::mem::size_of::<u32>() as u32;
const FRAME_SIZE: u32 = std::mem::size_of::<openjoc_pcm_frame>() as u32;
const INFO_SIZE: u32 = std::mem::size_of::<openjoc_output_info>() as u32;
const CUSTOM_LAYOUT_SIZE: u32 = std::mem::size_of::<openjoc_custom_speaker_layout>() as u32;
const CUSTOM_SPEAKER_SIZE: u32 = std::mem::size_of::<openjoc_custom_speaker>() as u32;

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

fn config_from_c(config: *const openjoc_decoder_config) -> Result<OpenJocConfig, OpenJocError> {
    // Read only the prefix advertised by the caller before constructing a
    // Rust value. This keeps ABI 1.0 callers, whose allocation ends before
    // `dialnorm_mode`, from being treated as a reference to the larger 1.1
    // struct.
    let struct_size = unsafe { ptr::read_unaligned(ptr::addr_of!((*config).struct_size)) };
    if struct_size < CONFIG_SIZE_BEFORE_DIALNORM {
        return Err(OpenJocError::InvalidConfig(
            "config.struct_size is too small".to_owned(),
        ));
    }
    let mut owned: openjoc_decoder_config = unsafe { std::mem::zeroed() };
    unsafe {
        ptr::copy_nonoverlapping(
            config.cast::<u8>(),
            (&raw mut owned).cast::<u8>(),
            CONFIG_SIZE_BEFORE_DIALNORM as usize,
        );
        if struct_size >= CONFIG_SIZE_BEFORE_CUSTOM {
            owned.dialnorm_mode = ptr::read_unaligned(ptr::addr_of!((*config).dialnorm_mode));
        }
        if struct_size >= CONFIG_SIZE {
            owned.custom_speaker_layout =
                ptr::read_unaligned(ptr::addr_of!((*config).custom_speaker_layout));
        }
    }
    owned.struct_size = struct_size;
    config_from_c_fields(&owned)
}

fn custom_layout_from_c(
    descriptor: *const openjoc_custom_speaker_layout,
) -> Result<openjoc_scene::SpeakerLayout, OpenJocError> {
    if descriptor.is_null() {
        return Err(OpenJocError::InvalidConfig(
            "custom_speaker_layout is null".to_owned(),
        ));
    }
    let descriptor_size = unsafe { ptr::read_unaligned(ptr::addr_of!((*descriptor).struct_size)) };
    if descriptor_size < CUSTOM_LAYOUT_SIZE {
        return Err(OpenJocError::InvalidConfig(
            "custom speaker layout struct_size is too small".to_owned(),
        ));
    }
    let descriptor = unsafe { ptr::read_unaligned(descriptor) };
    if descriptor.version != openjoc_scene::SPEAKER_LAYOUT_JSON_VERSION {
        return Err(OpenJocError::InvalidConfig(format!(
            "unsupported custom speaker layout version {}; expected {}",
            descriptor.version,
            openjoc_scene::SPEAKER_LAYOUT_JSON_VERSION
        )));
    }
    let name = c_string(descriptor.name, "custom_speaker_layout.name")?;
    if descriptor.speaker_count > openjoc_scene::MAX_CUSTOM_SPEAKERS {
        return Err(OpenJocError::InvalidConfig(format!(
            "custom speaker layout contains {}; maximum is {}",
            descriptor.speaker_count,
            openjoc_scene::MAX_CUSTOM_SPEAKERS
        )));
    }
    if descriptor.speaker_count > 0 && descriptor.speakers.is_null() {
        return Err(OpenJocError::InvalidConfig(
            "custom_speaker_layout.speakers is null".to_owned(),
        ));
    }
    let entries = if descriptor.speaker_count == 0 {
        Vec::new()
    } else {
        // SAFETY: the caller promises a readable array for the duration of
        // create; the count is bounded before constructing the slice.
        unsafe { slice::from_raw_parts(descriptor.speakers, descriptor.speaker_count) }
            .iter()
            .map(|entry| {
                if entry.struct_size < CUSTOM_SPEAKER_SIZE {
                    return Err(OpenJocError::InvalidConfig(
                        "custom speaker struct_size is too small".to_owned(),
                    ));
                }
                let role = match entry.role {
                    value if value == openjoc_speaker_role::OPENJOC_SPEAKER_FULL_RANGE as u32 => {
                        openjoc_scene::SpeakerRole::FullRange
                    }
                    value if value == openjoc_speaker_role::OPENJOC_SPEAKER_LFE as u32 => {
                        openjoc_scene::SpeakerRole::Lfe
                    }
                    _ => {
                        return Err(OpenJocError::InvalidConfig(
                            "unknown custom speaker role".to_owned(),
                        ));
                    }
                };
                Ok(openjoc_scene::SpeakerGeometry {
                    name: c_string(entry.name, "custom_speaker.name")?,
                    azimuth: entry.azimuth,
                    elevation: entry.elevation,
                    role,
                })
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    openjoc_scene::SpeakerLayout::custom(name, entries)
        .map_err(|error| OpenJocError::InvalidConfig(error.to_string()))
}

fn config_from_c_fields(config: &openjoc_decoder_config) -> Result<OpenJocConfig, OpenJocError> {
    if config.struct_size < CONFIG_SIZE_BEFORE_DIALNORM {
        return Err(OpenJocError::InvalidConfig(
            "config.struct_size is too small".to_owned(),
        ));
    }
    let dialnorm = if config.struct_size >= CONFIG_SIZE {
        match config.dialnorm_mode {
            value if value == openjoc_dialnorm_mode::OPENJOC_DIALNORM_DEFAULT as u32 => {
                DialnormMode::Default
            }
            value if value == openjoc_dialnorm_mode::OPENJOC_DIALNORM_DIGITAL as u32 => {
                DialnormMode::Digital
            }
            value if value == openjoc_dialnorm_mode::OPENJOC_DIALNORM_ANALOG as u32 => {
                DialnormMode::Analog
            }
            _ => {
                return Err(OpenJocError::InvalidConfig(
                    "unknown dialnorm mode".to_owned(),
                ));
            }
        }
    } else {
        DialnormMode::Default
    };
    let custom_layout =
        if config.struct_size >= CONFIG_SIZE && !config.custom_speaker_layout.is_null() {
            Some(custom_layout_from_c(config.custom_speaker_layout)?)
        } else {
            None
        };
    let speaker_layout = if let Some(layout) = &custom_layout {
        layout.name().to_owned()
    } else if config.speaker_layout.is_null() {
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
        // A null/empty SOFA selects the bundled generic HRTF. A non-empty
        // buffer retains the strict user-SOFA path and its validation gates.
        let bytes = if config.sofa_data.is_null() || config.sofa_size == 0 {
            Vec::new()
        } else {
            // SAFETY: the caller owns a readable buffer for the duration of create.
            unsafe { slice::from_raw_parts(config.sofa_data, config.sofa_size) }.to_vec()
        };
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
        speaker_layout_definition: custom_layout,
        downmix,
        drc,
        dialnorm,
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

fn stream_status(status: BridgeStatus) -> openjoc_status {
    match status {
        BridgeStatus::Ok => openjoc_status::OPENJOC_STATUS_OK,
        BridgeStatus::NeedMoreInput => openjoc_status::OPENJOC_STATUS_NEED_MORE_INPUT,
        BridgeStatus::FrameAvailable => openjoc_status::OPENJOC_STATUS_FRAME_AVAILABLE,
        BridgeStatus::WouldBlock => openjoc_status::OPENJOC_STATUS_OUTPUT_PENDING,
        BridgeStatus::EndOfStream => openjoc_status::OPENJOC_STATUS_END_OF_STREAM,
        BridgeStatus::NotJoc => openjoc_status::OPENJOC_STATUS_NOT_JOC,
    }
}

fn stream_error_status(error: &BridgeError) -> openjoc_status {
    match error.kind {
        BridgeErrorKind::InvalidConfig | BridgeErrorKind::InvalidTimestamp => {
            openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT
        }
        BridgeErrorKind::Unsupported => openjoc_status::OPENJOC_STATUS_UNSUPPORTED,
        BridgeErrorKind::OutputPending => openjoc_status::OPENJOC_STATUS_OUTPUT_PENDING,
        BridgeErrorKind::EndOfStream => openjoc_status::OPENJOC_STATUS_END_OF_STREAM,
        BridgeErrorKind::Ffmpeg | BridgeErrorKind::InternalPanic => {
            openjoc_status::OPENJOC_STATUS_EXTERNAL_ERROR
        }
        BridgeErrorKind::InvalidData => openjoc_status::OPENJOC_STATUS_DECODE_ERROR,
    }
}

fn set_stream_error(decoder: &mut openjoc_stream_decoder, error: BridgeError) -> openjoc_status {
    let result = stream_error_status(&error);
    decoder.last_error = CString::new(error.to_string())
        .unwrap_or_else(|_| CString::new("OpenJOC error contains NUL").expect("static error"));
    result
}

fn set_stream_message(
    decoder: &mut openjoc_stream_decoder,
    message: impl ToString,
) -> openjoc_status {
    decoder.last_error = CString::new(message.to_string())
        .unwrap_or_else(|_| CString::new("OpenJOC error contains NUL").expect("static error"));
    openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT
}

fn stream_panic_status(decoder: &mut openjoc_stream_decoder) -> openjoc_status {
    decoder.last_error =
        CString::new("panic contained at OpenJOC C ABI boundary").expect("static error");
    openjoc_status::OPENJOC_STATUS_EXTERNAL_ERROR
}

fn classifier_value(classification: JocClassification) -> openjoc_classification {
    match classification {
        JocClassification::Unknown => openjoc_classification::OPENJOC_CLASSIFICATION_UNKNOWN,
        JocClassification::ConfirmedJoc => {
            openjoc_classification::OPENJOC_CLASSIFICATION_CONFIRMED_JOC
        }
        JocClassification::ConfirmedNonJoc => {
            openjoc_classification::OPENJOC_CLASSIFICATION_CONFIRMED_NON_JOC
        }
        JocClassification::InvalidOrUnsupported => {
            openjoc_classification::OPENJOC_CLASSIFICATION_INVALID_OR_UNSUPPORTED
        }
    }
}

fn set_classifier_error(classifier: &mut openjoc_classifier, error: BridgeError) -> openjoc_status {
    let result = stream_error_status(&error);
    classifier.last_error = CString::new(error.to_string())
        .unwrap_or_else(|_| CString::new("OpenJOC error contains NUL").expect("static error"));
    result
}

fn classifier_panic_status(classifier: &mut openjoc_classifier) -> openjoc_status {
    classifier.last_error =
        CString::new("panic contained at OpenJOC C ABI boundary").expect("static error");
    openjoc_status::OPENJOC_STATUS_EXTERNAL_ERROR
}

/// Returns the packed ABI version `(major << 16) | minor`.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_get_abi_version() -> u32 {
    (OPENJOC_ABI_VERSION_MAJOR << 16) | OPENJOC_ABI_VERSION_MINOR
}

fn default_decoder_config(struct_size: u32) -> openjoc_decoder_config {
    openjoc_decoder_config {
        struct_size,
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
        dialnorm_mode: openjoc_dialnorm_mode::OPENJOC_DIALNORM_DEFAULT as u32,
        custom_speaker_layout: ptr::null(),
    }
}

/// Initializes only the ABI 1.3 configuration prefix.
///
/// This symbol is intentionally safe for a caller compiled against the old
/// ABI 1.3 header: that caller allocated only the prefix-sized struct, so the
/// function never writes the ABI 1.4 appended field. ABI 1.4 callers that
/// need the complete descriptor should use [`openjoc_decoder_config_init_v1_4`].
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_decoder_config_init(
    config: *mut openjoc_decoder_config,
) -> openjoc_status {
    if config.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    let defaults = default_decoder_config(CONFIG_SIZE_BEFORE_CUSTOM);
    // SAFETY: the legacy ABI 1.3 prefix is the largest allocation that this
    // symbol is permitted to touch. ABI 1.4 callers use the full initializer.
    unsafe {
        ptr::copy_nonoverlapping(
            (&raw const defaults).cast::<u8>(),
            config.cast::<u8>(),
            CONFIG_SIZE_BEFORE_CUSTOM as usize,
        );
    }
    openjoc_status::OPENJOC_STATUS_OK
}

/// Initializes the complete ABI 1.4 configuration structure, including the
/// appended in-memory custom speaker-layout field.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_decoder_config_init_v1_4(
    config: *mut openjoc_decoder_config,
) -> openjoc_status {
    if config.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: null was checked and the ABI 1.4 caller supplied full writable
    // storage for the current configuration structure.
    unsafe {
        *config = default_decoder_config(CONFIG_SIZE);
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

/// Creates a framework-neutral compressed-stream bridge.
///
/// Unlike `openjoc_decoder`, this bridge accepts arbitrary byte chunks and
/// owns bounded packet-to-access-unit staging. The render session remains
/// lazy until the first complete access unit is positively admitted as JOC;
/// Binaural configurations are preflighted at creation so SOFA/layout errors
/// are returned before an adapter persists or starts a stream.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_stream_decoder_create(
    config: *const openjoc_decoder_config,
    output: *mut *mut openjoc_stream_decoder,
) -> openjoc_status {
    if config.is_null() || output.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let config = config_from_c(config)
            .map_err(|error| BridgeError::new(BridgeErrorKind::InvalidConfig, error.to_string()))?;
        let decoder = FfmpegDecoder::new(config)?;
        let layout_name = CString::new(
            decoder
                .channel_layout()
                .standard_layout
                .as_deref()
                .unwrap_or(decoder.channel_layout().name.as_str()),
        )
        .map_err(|_| BridgeError::new(BridgeErrorKind::InvalidConfig, "layout contains NUL"))?;
        let channel_labels = decoder
            .channel_layout()
            .ffmpeg_order
            .iter()
            .map(|label| {
                CString::new(label.as_str()).map_err(|_| {
                    BridgeError::new(BridgeErrorKind::InvalidConfig, "channel label contains NUL")
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let channel_label_ptrs = channel_labels
            .iter()
            .map(|label| label.as_c_str().as_ptr())
            .collect();
        let config_descriptor =
            CString::new(decoder.effective_config_descriptor()).map_err(|_| {
                BridgeError::new(
                    BridgeErrorKind::InvalidConfig,
                    "config descriptor contains NUL",
                )
            })?;
        let config_fingerprint =
            CString::new(decoder.effective_config_fingerprint()).map_err(|_| {
                BridgeError::new(
                    BridgeErrorKind::InvalidConfig,
                    "config fingerprint contains NUL",
                )
            })?;
        let stream = Box::new(openjoc_stream_decoder {
            decoder,
            last_error: CString::new("").expect("empty CString"),
            layout_name,
            channel_labels,
            channel_label_ptrs,
            config_descriptor,
            config_fingerprint,
            last_frame: None,
        });
        // SAFETY: output was checked and receives ownership of the allocation.
        unsafe { *output = Box::into_raw(stream) };
        Ok::<openjoc_status, BridgeError>(openjoc_status::OPENJOC_STATUS_OK)
    }));
    match result {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => stream_error_status(&error),
        Err(_) => openjoc_status::OPENJOC_STATUS_EXTERNAL_ERROR,
    }
}

/// Destroys a framework-neutral compressed-stream bridge.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_stream_decoder_destroy(decoder: *mut openjoc_stream_decoder) {
    if decoder.is_null() {
        return;
    }
    // SAFETY: the pointer came from `openjoc_stream_decoder_create` and is consumed once.
    unsafe { drop(Box::from_raw(decoder)) };
}

/// Sends one borrowed compressed chunk. Bytes needed after this call are
/// copied into the bridge's bounded staging allocation.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_stream_decoder_send_chunk(
    decoder: *mut openjoc_stream_decoder,
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
        // SAFETY: caller guarantees a readable chunk buffer for this call.
        let bytes = unsafe { slice::from_raw_parts(data, data_len) };
        decoder.last_frame = None;
        decoder
            .decoder
            .send_packet(PacketRef {
                data: bytes,
                pts: (pts_samples != NO_PTS).then_some(pts_samples),
                dts: None,
                duration: None,
                time_base: Rational::SAMPLE_TIME_BASE,
                stream_index: 0,
                discontinuity: flags & OPENJOC_PACKET_FLAG_DISCONTINUITY != 0,
                preroll: flags & OPENJOC_PACKET_FLAG_PREROLL != 0,
            })
            .map(stream_status)
    }));
    match result {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => set_stream_error(decoder, error),
        Err(_) => stream_panic_status(decoder),
    }
}

/// Receives one packed float32 PCM frame in the advertised semantic order.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_stream_decoder_receive_frame(
    decoder: *mut openjoc_stream_decoder,
    output: *mut openjoc_pcm_frame,
) -> openjoc_status {
    if decoder.is_null() || output.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: pointers were checked and remain caller-owned.
    let decoder = unsafe { &mut *decoder };
    let result = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: output is valid for the advertised structure size.
        let output = unsafe { &mut *output };
        if output.struct_size < FRAME_SIZE {
            return Err(BridgeError::new(
                BridgeErrorKind::InvalidConfig,
                "pcm_frame.struct_size is too small",
            ));
        }
        match decoder.decoder.receive_frame()? {
            ReceiveOutcome::Frame(frame) => {
                decoder.last_frame = Some(frame);
                let frame = decoder.last_frame.as_ref().expect("stored frame");
                output.sample_format = 1;
                output.sample_rate = frame.sample_rate;
                output.channel_count = decoder.channel_labels.len() as u32;
                output.sample_count = frame.nb_samples;
                output.pts_samples = frame.pts.unwrap_or(NO_PTS);
                output.data = frame.interleaved_f32.as_ptr();
                output.data_len = frame.interleaved_f32.len();
                output.layout_name = decoder.layout_name.as_c_str().as_ptr();
                output.channel_labels = decoder.channel_label_ptrs.as_ptr();
                output.channel_label_count = decoder.channel_labels.len();
                Ok(openjoc_status::OPENJOC_STATUS_FRAME_AVAILABLE)
            }
            ReceiveOutcome::NeedMoreInput => Ok(openjoc_status::OPENJOC_STATUS_NEED_MORE_INPUT),
            ReceiveOutcome::EndOfStream => Ok(openjoc_status::OPENJOC_STATUS_END_OF_STREAM),
            ReceiveOutcome::NotJoc => Ok(openjoc_status::OPENJOC_STATUS_NOT_JOC),
        }
    }));
    match result {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => set_stream_error(decoder, error),
        Err(_) => stream_panic_status(decoder),
    }
}

/// Requests complete reconstruction, gain, and binaural tail drain.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_stream_decoder_drain(
    decoder: *mut openjoc_stream_decoder,
) -> openjoc_status {
    if decoder.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: pointer was checked and remains caller-owned.
    let decoder = unsafe { &mut *decoder };
    let result = catch_unwind(AssertUnwindSafe(|| {
        decoder.decoder.drain().map(stream_status)
    }));
    match result {
        Ok(Ok(value)) => value,
        Ok(Err(error)) => set_stream_error(decoder, error),
        Err(_) => stream_panic_status(decoder),
    }
}

/// Discards compressed staging, PCM, DSP history, and timestamp state.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_stream_decoder_flush(
    decoder: *mut openjoc_stream_decoder,
) -> openjoc_status {
    if decoder.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: pointer was checked and remains caller-owned.
    let decoder = unsafe { &mut *decoder };
    let result = catch_unwind(AssertUnwindSafe(|| {
        decoder.last_frame = None;
        decoder.decoder.flush();
        openjoc_status::OPENJOC_STATUS_OK
    }));
    result.unwrap_or_else(|_| stream_panic_status(decoder))
}

/// Alias with explicit new-stream/seek intent.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_stream_decoder_reset(
    decoder: *mut openjoc_stream_decoder,
) -> openjoc_status {
    openjoc_stream_decoder_flush(decoder)
}

/// Returns the instance-owned diagnostic for the most recent stream failure.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_stream_decoder_last_error(
    decoder: *const openjoc_stream_decoder,
) -> *const c_char {
    if decoder.is_null() {
        return c"invalid null OpenJOC stream decoder handle".as_ptr();
    }
    // SAFETY: pointer was checked and remains valid for the caller.
    unsafe { (&*decoder).last_error.as_ptr() }
}

/// Returns deterministic output semantics before compressed input is sent.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_stream_decoder_get_output_info(
    decoder: *mut openjoc_stream_decoder,
    output: *mut openjoc_output_info,
) -> openjoc_status {
    if decoder.is_null() || output.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: pointers were checked and remain caller-owned.
    let decoder = unsafe { &mut *decoder };
    let output = unsafe { &mut *output };
    if output.struct_size < INFO_SIZE {
        return set_stream_message(decoder, "output_info.struct_size is too small");
    }
    output.sample_format = 1;
    output.sample_rate = 48_000;
    output.channel_count = decoder.channel_labels.len() as u32;
    output.latency_samples = decoder.decoder.latency_samples();
    output.layout_name = decoder.layout_name.as_c_str().as_ptr();
    output.channel_labels = decoder.channel_label_ptrs.as_ptr();
    output.channel_label_count = decoder.channel_labels.len();
    openjoc_status::OPENJOC_STATUS_OK
}

/// Returns one semantic channel label in packed PCM order.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_stream_decoder_get_channel_label(
    decoder: *const openjoc_stream_decoder,
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

/// Returns the exact shared effective configuration descriptor.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_stream_decoder_get_config_descriptor(
    decoder: *const openjoc_stream_decoder,
) -> *const c_char {
    if decoder.is_null() {
        return ptr::null();
    }
    // SAFETY: pointer was checked and remains valid for the caller.
    unsafe { (&*decoder).config_descriptor.as_ptr() }
}

/// Returns the exact shared effective configuration SHA-256 fingerprint.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_stream_decoder_get_config_fingerprint(
    decoder: *const openjoc_stream_decoder,
) -> *const c_char {
    if decoder.is_null() {
        return ptr::null();
    }
    // SAFETY: pointer was checked and remains valid for the caller.
    unsafe { (&*decoder).config_fingerprint.as_ptr() }
}

/// Returns the current bounded compressed staging size for diagnostics.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_stream_decoder_get_staged_bytes(
    decoder: *const openjoc_stream_decoder,
) -> usize {
    if decoder.is_null() {
        return 0;
    }
    // SAFETY: pointer was checked and remains valid for the caller.
    unsafe { (&*decoder).decoder.staged_bytes() }
}

/// Creates a bounded, decode-free compressed-stream classifier.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_classifier_create(
    output: *mut *mut openjoc_classifier,
) -> openjoc_status {
    if output.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let classifier = Box::new(openjoc_classifier {
            classifier: JocClassifier::new(),
            last_error: CString::new("").expect("empty CString"),
        });
        // SAFETY: output was checked and receives ownership of the allocation.
        unsafe { *output = Box::into_raw(classifier) };
        openjoc_status::OPENJOC_STATUS_OK
    }));
    result.unwrap_or(openjoc_status::OPENJOC_STATUS_EXTERNAL_ERROR)
}

/// Destroys a compressed-stream classifier.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_classifier_destroy(classifier: *mut openjoc_classifier) {
    if classifier.is_null() {
        return;
    }
    // SAFETY: the pointer came from openjoc_classifier_create and is consumed once.
    unsafe { drop(Box::from_raw(classifier)) };
}

/// Supplies borrowed compressed bytes and returns the current positive
/// classification. No OpenJOC rendering or PCM decode occurs here.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_classifier_send_chunk(
    classifier: *mut openjoc_classifier,
    data: *const u8,
    data_len: usize,
    output: *mut openjoc_classification,
) -> openjoc_status {
    if classifier.is_null() || data.is_null() || data_len == 0 || output.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: classifier was checked and remains owned by the caller.
    let classifier = unsafe { &mut *classifier };
    let result = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: caller guarantees a readable chunk buffer for this call.
        let bytes = unsafe { slice::from_raw_parts(data, data_len) };
        match classifier.classifier.send_chunk(bytes) {
            Ok(value) => {
                // SAFETY: output was checked and is caller-owned.
                unsafe { *output = classifier_value(value) };
                openjoc_status::OPENJOC_STATUS_OK
            }
            Err(error) => set_classifier_error(classifier, error),
        }
    }));
    result.unwrap_or_else(|_| classifier_panic_status(classifier))
}

/// Closes the bounded classifier probe and classifies a final complete AU.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_classifier_finish(
    classifier: *mut openjoc_classifier,
    output: *mut openjoc_classification,
) -> openjoc_status {
    if classifier.is_null() || output.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: classifier was checked and remains owned by the caller.
    let classifier = unsafe { &mut *classifier };
    let result = catch_unwind(AssertUnwindSafe(|| match classifier.classifier.finish() {
        Ok(value) => {
            // SAFETY: output was checked and is caller-owned.
            unsafe { *output = classifier_value(value) };
            openjoc_status::OPENJOC_STATUS_OK
        }
        Err(error) => set_classifier_error(classifier, error),
    }));
    result.unwrap_or_else(|_| classifier_panic_status(classifier))
}

/// Resets the classifier for a new stream or seek re-probe.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_classifier_reset(classifier: *mut openjoc_classifier) -> openjoc_status {
    if classifier.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: classifier was checked and remains owned by the caller.
    let classifier = unsafe { &mut *classifier };
    let result = catch_unwind(AssertUnwindSafe(|| {
        classifier.classifier.reset();
        classifier.last_error = CString::new("").expect("empty CString");
        openjoc_status::OPENJOC_STATUS_OK
    }));
    result.unwrap_or_else(|_| classifier_panic_status(classifier))
}

/// Returns the classifier's latest diagnostic string.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_classifier_last_error(
    classifier: *const openjoc_classifier,
) -> *const c_char {
    if classifier.is_null() {
        return c"invalid null OpenJOC classifier handle".as_ptr();
    }
    // SAFETY: classifier was checked and remains valid for the caller.
    unsafe { (&*classifier).last_error.as_ptr() }
}

/// Returns bytes retained while waiting for a complete access unit.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_classifier_get_staged_bytes(
    classifier: *const openjoc_classifier,
) -> usize {
    if classifier.is_null() {
        return 0;
    }
    // SAFETY: classifier was checked and remains valid for the caller.
    unsafe { (&*classifier).classifier.staged_bytes() }
}

/// Returns compressed bytes inspected by the classifier.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_classifier_get_inspected_bytes(
    classifier: *const openjoc_classifier,
) -> usize {
    if classifier.is_null() {
        return 0;
    }
    // SAFETY: classifier was checked and remains valid for the caller.
    unsafe { (&*classifier).classifier.inspected_bytes() }
}
