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
    AppliedBinauralUpdate, BinauralConfig, BinauralLfePolicy, BuiltinHrtf, DialnormMode,
    DownmixPolicy, DrcPolicy, ListenerOrientation, OpenJocConfig, OpenJocError, OpenJocPacket,
    OpenJocPcmFrame, OpenJocSession, OpenJocStatus, PreparedBinauralKernel, PreparedBinauralUpdate,
    RenderMode, ValidationProfile,
};
use openjoc_ffmpeg::{
    BridgeError, BridgeErrorKind, BridgeStatus, FfmpegDecoder, FfmpegFrame, JocClassification,
    JocClassifier, LiveInspectionSnapshot, PacketRef, Rational, ReceiveOutcome,
};
use std::{
    ffi::{CStr, CString},
    os::raw::c_char,
    panic::{AssertUnwindSafe, catch_unwind},
    ptr, slice,
    sync::Mutex,
};

/// Major C ABI version. It is intentionally independent from the package
/// version and follows the compatibility policy in `docs/C_API.md`.
pub const OPENJOC_ABI_VERSION_MAJOR: u32 = 1;
/// Experimental ABI minor version.
pub const OPENJOC_ABI_VERSION_MINOR: u32 = 7;
const NO_PTS: i64 = i64::MIN;
const NO_ORIENTATION_SEQUENCE: u64 = u64::MAX;
const LIVE_TEXT_CAPACITY: usize = 512;
const LIVE_SHORT_TEXT_CAPACITY: usize = 128;
const LIVE_TINY_TEXT_CAPACITY: usize = 64;
const LIVE_FORMAT_CAPACITY: usize = 32;

#[repr(C)]
pub struct openjoc_decoder {
    session: OpenJocSession,
    poisoned: bool,
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
    live_snapshot: Mutex<LiveInspectionSnapshot>,
}

/// Opaque prepared pose handle. A failed apply leaves this handle usable;
/// destroy it on a control thread if the caller abandons the update.
pub struct openjoc_listener_orientation_update {
    update: Option<PreparedBinauralUpdate>,
    retired_kernels: Vec<PreparedBinauralKernel>,
}

/// Successful apply reuses the update allocation as the retired-buffer
/// handle, avoiding a new allocation or HRIR drop on the apply path.
pub type openjoc_listener_orientation_retired = openjoc_listener_orientation_update;

/// Immutable, shareable HRIR-preparation context captured from a decoder.
pub struct openjoc_listener_orientation_preparer {
    preparer: openjoc_api::ListenerOrientationPreparer,
    last_error: Mutex<CString>,
}

/// Snapshot of the opt-in listener-orientation control state.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct openjoc_listener_orientation_state {
    pub struct_size: u32,
    pub has_last_applied: u32,
    pub has_pending_update: u32,
    pub reserved: u32,
    pub stream_epoch: u64,
    pub last_applied_sequence: u64,
    pub last_applied_logical_start_sample: u64,
    pub pending_sequence: u64,
    pub pending_binaural_input_samples: usize,
}

/// ABI-stable quaternion input to the orientation preparer.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct openjoc_listener_orientation {
    pub struct_size: u32,
    pub reserved: u32,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct openjoc_live_inspection_snapshot {
    pub struct_size: u32,
    pub schema_version: u32,
    pub observation_epoch: u64,
    pub stream_present: u8,
    pub joc_present: u8,
    pub dynamic_scene_observed: u8,
    pub lfe_presence: u8,
    pub has_sample_rate: u8,
    pub has_timestamp: u8,
    pub has_object_count: u8,
    pub has_complexity: u8,
    pub has_first_change: u8,
    pub reserved: [u8; 3],
    pub sample_rate_hz: u32,
    pub object_count: u16,
    pub complexity: u16,
    pub observed_au_count: u64,
    pub malformed_observed_count: u64,
    pub current_decode_sequence: u64,
    pub current_timestamp_seconds: f64,
    pub first_change_au: u64,
    pub first_change_sample: u64,
    pub first_change_seconds: f64,
    pub profile_index: i32,
    pub inspection_kind: [c_char; LIVE_TINY_TEXT_CAPACITY],
    pub observation_scope: [c_char; LIVE_TINY_TEXT_CAPACITY],
    pub coverage: [c_char; LIVE_FORMAT_CAPACITY],
    pub format: [c_char; LIVE_FORMAT_CAPACITY],
    pub profile_display_name: [c_char; LIVE_SHORT_TEXT_CAPACITY],
    pub reconstruction_carriers: [c_char; LIVE_TEXT_CAPACITY],
    pub programme_topology: [c_char; LIVE_TEXT_CAPACITY],
    pub dependent_ids: [c_char; LIVE_SHORT_TEXT_CAPACITY],
    pub block_partition: [c_char; LIVE_SHORT_TEXT_CAPACITY],
    pub lfe_owner: [c_char; LIVE_TINY_TEXT_CAPACITY],
    pub lfe_semantics: [c_char; LIVE_SHORT_TEXT_CAPACITY],
    pub joc_owner: [c_char; LIVE_TINY_TEXT_CAPACITY],
    pub carriage_locations: [c_char; LIVE_TEXT_CAPACITY],
    pub etsi_strict: [c_char; LIVE_FORMAT_CAPACITY],
    pub deployed_compatibility: [c_char; LIVE_FORMAT_CAPACITY],
    pub emdf_payloads: [c_char; LIVE_SHORT_TEXT_CAPACITY],
    pub last_error_summary: [c_char; LIVE_TEXT_CAPACITY],
    pub programme_layout: [c_char; LIVE_SHORT_TEXT_CAPACITY],
}

const LIVE_SNAPSHOT_SIZE: u32 = std::mem::size_of::<openjoc_live_inspection_snapshot>() as u32;

#[allow(clippy::cast_possible_wrap)]
fn copy_live_text<const N: usize>(destination: &mut [c_char; N], value: &str) {
    destination.fill(0);
    let length = value.len().min(N.saturating_sub(1));
    for (target, source) in destination
        .iter_mut()
        .take(length)
        .zip(value.as_bytes().iter().copied())
    {
        *target = source as c_char;
    }
}

fn joined<T: ToString>(values: &[T], separator: &str) -> String {
    values
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(separator)
}

fn c_snapshot(snapshot: &LiveInspectionSnapshot) -> openjoc_live_inspection_snapshot {
    let mut output = openjoc_live_inspection_snapshot {
        struct_size: LIVE_SNAPSHOT_SIZE,
        schema_version: snapshot.schema_version,
        observation_epoch: snapshot.observation_epoch,
        stream_present: u8::from(snapshot.stream_present),
        joc_present: u8::from(snapshot.joc_present),
        dynamic_scene_observed: match snapshot.dynamic_scene_observed {
            Some(false) => 1,
            Some(true) => 2,
            None => 0,
        },
        lfe_presence: match snapshot.lfe_presence {
            Some(false) => 1,
            Some(true) => 2,
            None => 0,
        },
        has_sample_rate: u8::from(snapshot.sample_rate_hz.is_some()),
        has_timestamp: u8::from(snapshot.current_timestamp_seconds.is_some()),
        has_object_count: u8::from(snapshot.object_count.is_some()),
        has_complexity: u8::from(snapshot.complexity.is_some()),
        has_first_change: u8::from(snapshot.first_observed_metadata_change.is_some()),
        reserved: [0; 3],
        sample_rate_hz: snapshot.sample_rate_hz.unwrap_or(0),
        object_count: u16::from(snapshot.object_count.unwrap_or(0)),
        complexity: u16::from(snapshot.complexity.unwrap_or(0)),
        observed_au_count: snapshot.observed_au_count,
        malformed_observed_count: snapshot.malformed_observed_count,
        current_decode_sequence: snapshot.current_decode_sequence,
        current_timestamp_seconds: snapshot.current_timestamp_seconds.unwrap_or(0.0),
        first_change_au: snapshot
            .first_observed_metadata_change
            .as_ref()
            .map_or(0, |value| value.au),
        first_change_sample: snapshot
            .first_observed_metadata_change
            .as_ref()
            .map_or(0, |value| value.sample),
        first_change_seconds: snapshot
            .first_observed_metadata_change
            .as_ref()
            .map_or(0.0, |value| value.seconds),
        profile_index: snapshot
            .current_profile
            .as_ref()
            .map_or(-1, |value| i32::from(value.profile_index)),
        inspection_kind: [0; LIVE_TINY_TEXT_CAPACITY],
        observation_scope: [0; LIVE_TINY_TEXT_CAPACITY],
        coverage: [0; LIVE_FORMAT_CAPACITY],
        format: [0; LIVE_FORMAT_CAPACITY],
        profile_display_name: [0; LIVE_SHORT_TEXT_CAPACITY],
        reconstruction_carriers: [0; LIVE_TEXT_CAPACITY],
        programme_topology: [0; LIVE_TEXT_CAPACITY],
        dependent_ids: [0; LIVE_SHORT_TEXT_CAPACITY],
        block_partition: [0; LIVE_SHORT_TEXT_CAPACITY],
        lfe_owner: [0; LIVE_TINY_TEXT_CAPACITY],
        lfe_semantics: [0; LIVE_SHORT_TEXT_CAPACITY],
        joc_owner: [0; LIVE_TINY_TEXT_CAPACITY],
        carriage_locations: [0; LIVE_TEXT_CAPACITY],
        etsi_strict: [0; LIVE_FORMAT_CAPACITY],
        deployed_compatibility: [0; LIVE_FORMAT_CAPACITY],
        emdf_payloads: [0; LIVE_SHORT_TEXT_CAPACITY],
        last_error_summary: [0; LIVE_TEXT_CAPACITY],
        programme_layout: [0; LIVE_SHORT_TEXT_CAPACITY],
    };
    copy_live_text(&mut output.inspection_kind, &snapshot.inspection_kind);
    copy_live_text(&mut output.observation_scope, &snapshot.observation_scope);
    copy_live_text(&mut output.coverage, &snapshot.coverage);
    copy_live_text(&mut output.format, &snapshot.format);
    copy_live_text(
        &mut output.profile_display_name,
        snapshot
            .current_profile
            .as_ref()
            .map_or("", |value| value.display_name.as_str()),
    );
    copy_live_text(
        &mut output.reconstruction_carriers,
        &snapshot.reconstruction_carriers.join(" "),
    );
    copy_live_text(
        &mut output.programme_topology,
        &snapshot.programme_topology.join(" + "),
    );
    copy_live_text(
        &mut output.dependent_ids,
        &joined(&snapshot.dependent_ids, ","),
    );
    copy_live_text(
        &mut output.block_partition,
        &joined(&snapshot.block_partition, "+"),
    );
    copy_live_text(
        &mut output.lfe_owner,
        snapshot.lfe_owner.as_deref().unwrap_or(""),
    );
    copy_live_text(
        &mut output.lfe_semantics,
        snapshot.lfe_semantics.as_deref().unwrap_or(""),
    );
    copy_live_text(
        &mut output.joc_owner,
        snapshot.joc_owner.as_deref().unwrap_or(""),
    );
    copy_live_text(
        &mut output.carriage_locations,
        &snapshot
            .carriage_locations
            .iter()
            .map(|value| format!("{}@{}", value.owner, value.location))
            .collect::<Vec<_>>()
            .join(","),
    );
    copy_live_text(&mut output.etsi_strict, &snapshot.etsi_strict);
    copy_live_text(
        &mut output.deployed_compatibility,
        &snapshot.deployed_compatibility,
    );
    copy_live_text(
        &mut output.emdf_payloads,
        &joined(&snapshot.emdf_payloads, ","),
    );
    copy_live_text(
        &mut output.last_error_summary,
        snapshot.last_error_summary.as_deref().unwrap_or(""),
    );
    copy_live_text(
        &mut output.programme_layout,
        snapshot.programme_layout.as_deref().unwrap_or(""),
    );
    output
}

fn refresh_live_snapshot(decoder: &mut openjoc_stream_decoder) {
    let mut snapshot = decoder.decoder.live_inspection_snapshot();
    if !decoder.last_error.as_bytes().is_empty() {
        snapshot.last_error_summary = Some(decoder.last_error.to_string_lossy().into_owned());
    }
    if let Ok(mut target) = decoder.live_snapshot.lock() {
        *target = snapshot;
    }
}

fn current_live_snapshot(decoder: &openjoc_stream_decoder) -> LiveInspectionSnapshot {
    decoder.live_snapshot.lock().map_or_else(
        |poisoned| poisoned.into_inner().clone(),
        |snapshot| snapshot.clone(),
    )
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

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum openjoc_hrtf_preset {
    OPENJOC_HRTF_SADIE_D1_KU100 = 0,
    OPENJOC_HRTF_SADIE_D2_KEMAR = 1,
}

const RETIRED_AACHEN_HRTF_PRESET_CODE: u32 = 2;

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
    /// Appended in ABI minor 6; zero retains the SADIE II D1 default.
    pub hrtf_preset: u32,
    /// Reserves the ABI 1.6 trailing alignment bytes so the new field below
    /// begins strictly after the old struct's sizeof on 64-bit targets.
    pub reserved_v1_6_padding: u32,
    /// Appended in ABI minor 7; 0 disables pull mode, 1..=256 selects the
    /// maximum binaural samples returned per receive call.
    pub listener_orientation_pull_samples: u32,
}

/// Exact ABI 1.6 prefix layout, retained to compute its historical sizeof on
/// both 32-bit and 64-bit targets. Do not add orientation fields here.
#[repr(C)]
struct openjoc_decoder_config_v1_6 {
    struct_size: u32,
    render_mode: u32,
    speaker_layout: *const c_char,
    downmix: u32,
    drc: u32,
    drc_boost_percent: u8,
    drc_cut_percent: u8,
    validation_profile: u32,
    sofa_data: *const u8,
    sofa_size: usize,
    virtual_layout: *const c_char,
    lfe_policy: u32,
    dialnorm_mode: u32,
    custom_speaker_layout: *const openjoc_custom_speaker_layout,
    hrtf_preset: u32,
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
const CONFIG_SIZE_BEFORE_LISTENER_ORIENTATION: u32 =
    std::mem::size_of::<openjoc_decoder_config_v1_6>() as u32;
const CONFIG_SIZE_BEFORE_HRTF: u32 =
    std::mem::offset_of!(openjoc_decoder_config, hrtf_preset) as u32;
const CONFIG_SIZE_BEFORE_CUSTOM: u32 =
    std::mem::offset_of!(openjoc_decoder_config, custom_speaker_layout) as u32;
const CONFIG_SIZE_BEFORE_DIALNORM: u32 =
    std::mem::offset_of!(openjoc_decoder_config, dialnorm_mode) as u32;
const FRAME_SIZE: u32 = std::mem::size_of::<openjoc_pcm_frame>() as u32;
const ORIENTATION_STATE_SIZE: u32 =
    std::mem::size_of::<openjoc_listener_orientation_state>() as u32;
const ORIENTATION_SIZE: u32 = std::mem::size_of::<openjoc_listener_orientation>() as u32;
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

fn prepare_listener_orientation_update(
    preparer: &openjoc_api::ListenerOrientationPreparer,
    epoch: u64,
    pose: openjoc_listener_orientation,
    sequence: u64,
) -> Result<PreparedBinauralUpdate, OpenJocError> {
    let orientation = ListenerOrientation::new(pose.x, pose.y, pose.z, pose.w)
        .map_err(|error| OpenJocError::InvalidConfig(error.to_string()))?;
    preparer
        .prepare(orientation, epoch, sequence)
        .map_err(|error| OpenJocError::Render(error.to_string()))
}

fn fill_listener_orientation_state(
    output: &mut openjoc_listener_orientation_state,
    epoch: Option<u64>,
    last_applied: Option<AppliedBinauralUpdate>,
    pending_sequence: Option<u64>,
    pending_samples: Option<usize>,
) -> Result<(), OpenJocError> {
    let stream_epoch = epoch.ok_or_else(|| {
        OpenJocError::Unsupported("listener-orientation pull mode is not enabled".to_owned())
    })?;
    let pending_samples = pending_samples.ok_or_else(|| {
        OpenJocError::Unsupported("listener-orientation pull mode is not enabled".to_owned())
    })?;
    output.has_last_applied = u32::from(last_applied.is_some());
    output.has_pending_update = u32::from(pending_sequence.is_some());
    output.reserved = 0;
    output.stream_epoch = stream_epoch;
    output.last_applied_sequence =
        last_applied.map_or(NO_ORIENTATION_SEQUENCE, |receipt| receipt.sequence);
    output.last_applied_logical_start_sample =
        last_applied.map_or(0, |receipt| receipt.logical_start_sample);
    output.pending_sequence = pending_sequence.unwrap_or(NO_ORIENTATION_SEQUENCE);
    output.pending_binaural_input_samples = pending_samples;
    Ok(())
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

fn decoder_requires_reset(decoder: &mut openjoc_decoder) -> openjoc_status {
    decoder.last_error = CString::new("OpenJOC decoder requires reset after a terminal error")
        .expect("static error");
    openjoc_status::OPENJOC_STATUS_REQUIRE_RESET
}

fn set_preparer_error(
    preparer: &openjoc_listener_orientation_preparer,
    error: &OpenJocError,
) -> openjoc_status {
    let mut last_error = preparer
        .last_error
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *last_error = CString::new(error.to_string())
        .unwrap_or_else(|_| CString::new("OpenJOC error contains NUL").expect("static error"));
    error_status(error)
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
        if struct_size >= CONFIG_SIZE_BEFORE_HRTF {
            owned.custom_speaker_layout =
                ptr::read_unaligned(ptr::addr_of!((*config).custom_speaker_layout));
        }
        if struct_size >= CONFIG_SIZE_BEFORE_LISTENER_ORIENTATION {
            owned.hrtf_preset = ptr::read_unaligned(ptr::addr_of!((*config).hrtf_preset));
        }
        if struct_size >= CONFIG_SIZE {
            owned.listener_orientation_pull_samples =
                ptr::read_unaligned(ptr::addr_of!((*config).listener_orientation_pull_samples));
        }
    }
    owned.struct_size = struct_size;
    config_from_c_fields(&owned)
}

fn orientation_pull_samples_from_c(config: *const openjoc_decoder_config) -> Option<usize> {
    let struct_size = unsafe { ptr::read_unaligned(ptr::addr_of!((*config).struct_size)) };
    if struct_size < CONFIG_SIZE {
        return None;
    }
    let max_samples =
        unsafe { ptr::read_unaligned(ptr::addr_of!((*config).listener_orientation_pull_samples)) };
    (max_samples > 0).then(|| usize::try_from(max_samples).unwrap_or(usize::MAX))
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
    let dialnorm = if config.struct_size >= CONFIG_SIZE_BEFORE_CUSTOM {
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
    let custom_layout = if config.struct_size >= CONFIG_SIZE_BEFORE_HRTF
        && !config.custom_speaker_layout.is_null()
    {
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
        let hrtf_preset = if config.struct_size >= CONFIG_SIZE_BEFORE_LISTENER_ORIENTATION {
            config.hrtf_preset
        } else {
            openjoc_hrtf_preset::OPENJOC_HRTF_SADIE_D1_KU100 as u32
        };
        let builtin_hrtf = match hrtf_preset {
            value if value == openjoc_hrtf_preset::OPENJOC_HRTF_SADIE_D1_KU100 as u32 => {
                BuiltinHrtf::SadieD1Ku100
            }
            value if value == openjoc_hrtf_preset::OPENJOC_HRTF_SADIE_D2_KEMAR as u32 => {
                BuiltinHrtf::SadieD2Kemar
            }
            RETIRED_AACHEN_HRTF_PRESET_CODE => BuiltinHrtf::SadieD1Ku100,
            _ => {
                return Err(OpenJocError::InvalidConfig(
                    "unknown HRTF preset".to_owned(),
                ));
            }
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
            builtin_hrtf,
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
    decoder.poisoned = true;
    decoder.last_error =
        CString::new("panic contained at OpenJOC C ABI boundary").expect("static error");
    openjoc_status::OPENJOC_STATUS_REQUIRE_RESET
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
        BridgeErrorKind::ResetRequired => openjoc_status::OPENJOC_STATUS_REQUIRE_RESET,
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
    refresh_live_snapshot(decoder);
    result
}

fn set_stream_message(
    decoder: &mut openjoc_stream_decoder,
    message: impl ToString,
) -> openjoc_status {
    decoder.last_error = CString::new(message.to_string())
        .unwrap_or_else(|_| CString::new("OpenJOC error contains NUL").expect("static error"));
    refresh_live_snapshot(decoder);
    openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT
}

fn stream_panic_status(decoder: &mut openjoc_stream_decoder) -> openjoc_status {
    decoder.decoder.poison_after_outer_panic();
    decoder.last_error =
        CString::new("panic contained at OpenJOC C ABI boundary").expect("static error");
    refresh_live_snapshot(decoder);
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

/// Initializes a listener-orientation state output structure.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_listener_orientation_state_init(
    output: *mut openjoc_listener_orientation_state,
) -> openjoc_status {
    if output.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: null was checked and the caller supplies writable current ABI storage.
    unsafe {
        *output = openjoc_listener_orientation_state {
            struct_size: ORIENTATION_STATE_SIZE,
            has_last_applied: 0,
            has_pending_update: 0,
            reserved: 0,
            stream_epoch: 0,
            last_applied_sequence: NO_ORIENTATION_SEQUENCE,
            last_applied_logical_start_sample: 0,
            pending_sequence: NO_ORIENTATION_SEQUENCE,
            pending_binaural_input_samples: 0,
        };
    }
    openjoc_status::OPENJOC_STATUS_OK
}

/// Initializes a quaternion input structure to identity.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_listener_orientation_init(
    output: *mut openjoc_listener_orientation,
) -> openjoc_status {
    if output.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: null was checked and the caller supplies current ABI storage.
    unsafe {
        *output = openjoc_listener_orientation {
            struct_size: ORIENTATION_SIZE,
            reserved: 0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 1.0,
        };
    }
    openjoc_status::OPENJOC_STATUS_OK
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
        hrtf_preset: openjoc_hrtf_preset::OPENJOC_HRTF_SADIE_D1_KU100 as u32,
        reserved_v1_6_padding: 0,
        listener_orientation_pull_samples: 0,
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
    // SAFETY: null was checked and the ABI 1.4 caller supplied writable
    // storage through the custom-layout field. The HRTF field was appended
    // later and is initialized by `openjoc_decoder_config_init_v1_6`.
    unsafe {
        let defaults = default_decoder_config(CONFIG_SIZE_BEFORE_HRTF);
        ptr::copy_nonoverlapping(
            (&raw const defaults).cast::<u8>(),
            config.cast::<u8>(),
            CONFIG_SIZE_BEFORE_HRTF as usize,
        );
    }
    openjoc_status::OPENJOC_STATUS_OK
}

/// Initializes the complete ABI 1.6 configuration structure.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_decoder_config_init_v1_6(
    config: *mut openjoc_decoder_config,
) -> openjoc_status {
    if config.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    let defaults = default_decoder_config(CONFIG_SIZE_BEFORE_LISTENER_ORIENTATION);
    // SAFETY: null was checked. Copy exactly the historical ABI 1.6 size; the
    // caller may own only that prefix even though the current Rust type grew.
    unsafe {
        ptr::copy_nonoverlapping(
            (&raw const defaults).cast::<u8>(),
            config.cast::<u8>(),
            CONFIG_SIZE_BEFORE_LISTENER_ORIENTATION as usize,
        );
    }
    openjoc_status::OPENJOC_STATUS_OK
}

/// Initializes the complete ABI 1.7 configuration structure, including the
/// experimental bounded listener-orientation pull block size.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_decoder_config_init_v1_7(
    config: *mut openjoc_decoder_config,
) -> openjoc_status {
    if config.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: null was checked and the caller supplies the current structure.
    unsafe { *config = default_decoder_config(CONFIG_SIZE) };
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
        let effective_config = config_from_c(config)?;
        let session = if let Some(max_pull_samples) = orientation_pull_samples_from_c(config) {
            OpenJocSession::new_with_listener_orientation_pull(effective_config, max_pull_samples)?
        } else {
            OpenJocSession::new(effective_config)?
        };
        let layout_name =
            CString::new(session.output_info().layout_name).expect("layout name contains no NUL");
        let channel_labels = labels_for(&session);
        let channel_label_ptrs = channel_labels
            .iter()
            .map(|label| label.as_c_str().as_ptr())
            .collect();
        let decoder = Box::new(openjoc_decoder {
            session,
            poisoned: false,
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
    if decoder.poisoned {
        return decoder_requires_reset(decoder);
    }
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
    if decoder.poisoned {
        return decoder_requires_reset(decoder);
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: output is valid for the advertised structure size.
        let output = unsafe { &mut *output };
        if output.struct_size < FRAME_SIZE {
            return Err(OpenJocError::InvalidConfig(
                "pcm_frame.struct_size is too small".to_owned(),
            ));
        }
        let frame = if decoder.session.pending_binaural_input_samples().is_some() {
            decoder.session.receive_binaural_frame()?
        } else {
            decoder.session.receive_frame()
        };
        let Some(frame) = frame else {
            return Ok(if decoder.session.is_drained() {
                openjoc_status::OPENJOC_STATUS_END_OF_STREAM
            } else {
                openjoc_status::OPENJOC_STATUS_NEED_MORE_INPUT
            });
        };
        if frame.pts_samples == Some(NO_PTS) {
            // This is a real timestamp in Rust, but the C ABI reserves the
            // value for absence. Do not return ambiguous PCM or allow a gap
            // after consuming this unrepresentable frame.
            decoder.last_frame = None;
            decoder.poisoned = true;
            return Err(OpenJocError::Render(
                "output timestamp equals reserved OPENJOC_NO_PTS; reset required".to_owned(),
            ));
        }
        decoder.last_frame = Some(frame);
        let frame = decoder.last_frame.as_ref().expect("stored frame");
        output.sample_format = 1;
        output.sample_rate = frame.sample_rate;
        output.channel_count = frame.channel_count as u32;
        output.sample_count = frame.sample_count;
        output.pts_samples = frame.pts_samples.unwrap_or(NO_PTS);
        output.data = frame.interleaved_f32.as_ptr();
        output.data_len = frame.interleaved_f32.len() * std::mem::size_of::<f32>();
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

/// Reads the opt-in orientation control state for an independent decoder.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_decoder_get_listener_orientation_state(
    decoder: *mut openjoc_decoder,
    output: *mut openjoc_listener_orientation_state,
) -> openjoc_status {
    if decoder.is_null() || output.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: pointers were checked and remain caller-owned.
    let decoder = unsafe { &mut *decoder };
    let output_size = unsafe { ptr::read_unaligned(ptr::addr_of!((*output).struct_size)) };
    if output_size < ORIENTATION_STATE_SIZE {
        return set_message(
            decoder,
            "listener_orientation_state.struct_size is too small",
        );
    }
    let output = unsafe { &mut *output };
    if decoder.poisoned {
        return decoder_requires_reset(decoder);
    }
    match fill_listener_orientation_state(
        output,
        decoder.session.listener_orientation_stream_epoch(),
        decoder.session.last_applied_listener_orientation(),
        decoder.session.pending_listener_orientation_sequence(),
        decoder.session.pending_binaural_input_samples(),
    ) {
        Ok(()) => openjoc_status::OPENJOC_STATUS_OK,
        Err(error) => set_error(decoder, error),
    }
}

/// Captures a cloneable immutable preparer before sharing the decoder with a
/// render thread. Pose preparation then uses the separate handle.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_decoder_get_listener_orientation_preparer(
    decoder: *mut openjoc_decoder,
    output: *mut *mut openjoc_listener_orientation_preparer,
) -> openjoc_status {
    if decoder.is_null() || output.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    unsafe { *output = ptr::null_mut() };
    // SAFETY: pointer was checked and remains caller-owned.
    let decoder = unsafe { &mut *decoder };
    match decoder.session.listener_orientation_preparer() {
        Some(preparer) => {
            let handle = openjoc_listener_orientation_preparer {
                preparer,
                last_error: Mutex::new(CString::new("").expect("empty CString")),
            };
            unsafe { *output = Box::into_raw(Box::new(handle)) };
            openjoc_status::OPENJOC_STATUS_OK
        }
        None => set_error(
            decoder,
            OpenJocError::Unsupported("listener-orientation pull mode is not enabled".to_owned()),
        ),
    }
}

/// Captures the orientation preparer from an opt-in stream bridge.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_stream_decoder_get_listener_orientation_preparer(
    decoder: *mut openjoc_stream_decoder,
    output: *mut *mut openjoc_listener_orientation_preparer,
) -> openjoc_status {
    if decoder.is_null() || output.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    unsafe { *output = ptr::null_mut() };
    let decoder = unsafe { &mut *decoder };
    match decoder.decoder.listener_orientation_preparer() {
        Some(preparer) => {
            let handle = openjoc_listener_orientation_preparer {
                preparer,
                last_error: Mutex::new(CString::new("").expect("empty CString")),
            };
            unsafe { *output = Box::into_raw(Box::new(handle)) };
            openjoc_status::OPENJOC_STATUS_OK
        }
        None => set_stream_message(decoder, "listener-orientation pull mode is not enabled"),
    }
}

/// Prepares a size-versioned quaternion using an immutable preparer handle.
/// Supply the current stream epoch obtained from the decoder's state snapshot.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_listener_orientation_prepare(
    preparer: *const openjoc_listener_orientation_preparer,
    orientation: *const openjoc_listener_orientation,
    stream_epoch: u64,
    sequence: u64,
    output: *mut *mut openjoc_listener_orientation_update,
) -> openjoc_status {
    if preparer.is_null() || orientation.is_null() || output.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    unsafe { *output = ptr::null_mut() };
    let preparer = unsafe { &*preparer };
    let pose_size = unsafe { ptr::read_unaligned(ptr::addr_of!((*orientation).struct_size)) };
    if pose_size < ORIENTATION_SIZE {
        return set_preparer_error(
            preparer,
            &OpenJocError::InvalidConfig(
                "listener_orientation.struct_size is too small".to_owned(),
            ),
        );
    }
    let pose = unsafe { ptr::read_unaligned(orientation) };
    let result = catch_unwind(AssertUnwindSafe(|| {
        prepare_listener_orientation_update(&preparer.preparer, stream_epoch, pose, sequence)
    }));
    match result {
        Ok(Ok(update)) => {
            let handle = openjoc_listener_orientation_update {
                update: Some(update),
                retired_kernels: Vec::new(),
            };
            unsafe { *output = Box::into_raw(Box::new(handle)) };
            *preparer
                .last_error
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                CString::new("").expect("empty CString");
            openjoc_status::OPENJOC_STATUS_OK
        }
        Ok(Err(error)) => set_preparer_error(preparer, &error),
        Err(_) => {
            let error =
                OpenJocError::Render("panic contained during orientation preparation".to_owned());
            set_preparer_error(preparer, &error)
        }
    }
}

/// Returns the last preparation error. The pointer remains valid until the
/// next prepare call or preparer destruction.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_listener_orientation_preparer_last_error(
    preparer: *const openjoc_listener_orientation_preparer,
) -> *const c_char {
    if preparer.is_null() {
        return c"invalid null listener-orientation preparer".as_ptr();
    }
    let preparer = unsafe { &*preparer };
    let error = preparer
        .last_error
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    error.as_ptr()
}

/// Destroys an immutable preparer handle.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_listener_orientation_preparer_destroy(
    preparer: *mut openjoc_listener_orientation_preparer,
) {
    if !preparer.is_null() {
        unsafe { drop(Box::from_raw(preparer)) };
    }
}

/// Applies one prepared update. Validation/lifecycle failures preserve the
/// update handle unchanged. On success `*update` becomes NULL and the same
/// allocation is returned as a retired handle (even if it contains no retired
/// kernels). An unexpected panic poisons the decoder; the update may be empty
/// and must be destroyed rather than retried.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_decoder_apply_listener_orientation(
    decoder: *mut openjoc_decoder,
    update: *mut *mut openjoc_listener_orientation_update,
    accepted_sequence: *mut u64,
    superseded_sequence: *mut u64,
    retired: *mut *mut openjoc_listener_orientation_retired,
) -> openjoc_status {
    if decoder.is_null()
        || update.is_null()
        || accepted_sequence.is_null()
        || superseded_sequence.is_null()
        || retired.is_null()
    {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    let decoder = unsafe { &mut *decoder };
    if decoder.poisoned {
        return decoder_requires_reset(decoder);
    }
    unsafe {
        *accepted_sequence = 0;
        *superseded_sequence = NO_ORIENTATION_SEQUENCE;
        *retired = ptr::null_mut();
    }
    let handle = unsafe { *update };
    if handle.is_null() {
        return set_message(decoder, "listener-orientation update handle is null");
    }
    let Some(prepared) = (unsafe { &mut *handle }).update.take() else {
        return set_message(decoder, "listener-orientation update handle is empty");
    };
    let result = catch_unwind(AssertUnwindSafe(|| {
        decoder
            .session
            .apply_prepared_listener_orientation(prepared)
    }));
    match result {
        Ok(Ok(acceptance)) => {
            unsafe {
                *accepted_sequence = acceptance.accepted_sequence;
                *superseded_sequence = acceptance
                    .superseded_sequence
                    .unwrap_or(NO_ORIENTATION_SEQUENCE);
                *update = ptr::null_mut();
                let handle_ref = &mut *handle;
                handle_ref.update = None;
                handle_ref.retired_kernels = acceptance.retired_kernels;
                *retired = handle.cast::<openjoc_listener_orientation_retired>();
            }
            // Keep the previous diagnostic unchanged; clearing it would
            // allocate a CString on the application path.
            openjoc_status::OPENJOC_STATUS_OK
        }
        Ok(Err(failure)) => {
            let error = OpenJocError::Render(failure.error.to_string());
            unsafe { (&mut *handle).update = Some(failure.update) };
            set_error(decoder, error)
        }
        Err(_) => {
            decoder.poisoned = true;
            panic_status(decoder)
        }
    }
}

/// Destroys an unapplied prepared update on the caller's control thread.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_listener_orientation_update_destroy(
    update: *mut openjoc_listener_orientation_update,
) {
    if !update.is_null() {
        // SAFETY: the handle was returned by a prepare call and is consumed once.
        unsafe { drop(Box::from_raw(update)) };
    }
}

/// Releases retired HRIR allocations on the caller's control thread.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_listener_orientation_retired_destroy(
    retired: *mut openjoc_listener_orientation_retired,
) {
    if !retired.is_null() {
        // SAFETY: the handle was returned by apply and is consumed once.
        unsafe { drop(Box::from_raw(retired)) };
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
    if decoder.poisoned {
        return decoder_requires_reset(decoder);
    }
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
        match decoder.session.try_flush() {
            Ok(()) => {
                decoder.poisoned = false;
                decoder.last_error = CString::new("").expect("empty CString");
                openjoc_status::OPENJOC_STATUS_OK
            }
            Err(error) => set_error(decoder, error),
        }
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
        let orientation_pull_samples = orientation_pull_samples_from_c(config);
        let config = config_from_c(config)
            .map_err(|error| BridgeError::new(BridgeErrorKind::InvalidConfig, error.to_string()))?;
        let mut decoder = if let Some(max_pull_samples) = orientation_pull_samples {
            FfmpegDecoder::new_with_listener_orientation_pull(config, max_pull_samples)?
        } else {
            FfmpegDecoder::new(config)?
        };
        // The C stream API exposes aggregate live inspection, not AU traces.
        // Do not retain diagnostic history that its callers cannot consume.
        decoder.set_trace_collection_enabled(false);
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
            live_snapshot: Mutex::new(decoder.live_inspection_snapshot()),
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
        Ok(Ok(value)) => {
            refresh_live_snapshot(decoder);
            value
        }
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
                output.data_len = frame.interleaved_f32.len() * std::mem::size_of::<f32>();
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
        Ok(Ok(value)) => {
            refresh_live_snapshot(decoder);
            value
        }
        Ok(Err(error)) => set_stream_error(decoder, error),
        Err(_) => stream_panic_status(decoder),
    }
}

/// Reads the opt-in orientation control state for a compressed-stream bridge.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_stream_decoder_get_listener_orientation_state(
    decoder: *mut openjoc_stream_decoder,
    output: *mut openjoc_listener_orientation_state,
) -> openjoc_status {
    if decoder.is_null() || output.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: pointers were checked and remain caller-owned.
    let decoder = unsafe { &mut *decoder };
    let output_size = unsafe { ptr::read_unaligned(ptr::addr_of!((*output).struct_size)) };
    if output_size < ORIENTATION_STATE_SIZE {
        return set_stream_message(
            decoder,
            "listener_orientation_state.struct_size is too small",
        );
    }
    let output = unsafe { &mut *output };
    match fill_listener_orientation_state(
        output,
        decoder.decoder.listener_orientation_stream_epoch(),
        decoder.decoder.last_applied_listener_orientation(),
        decoder.decoder.pending_listener_orientation_sequence(),
        decoder.decoder.pending_binaural_input_samples(),
    ) {
        Ok(()) => openjoc_status::OPENJOC_STATUS_OK,
        Err(error) => set_stream_error(
            decoder,
            BridgeError::new(BridgeErrorKind::Unsupported, error.to_string()),
        ),
    }
}

/// Applies one prepared update. Validation/lifecycle failures preserve the
/// update handle. Success reuses its allocation for a retired handle, avoiding
/// an extra allocation or filter destruction on the calling path. A panic
/// poisons the stream decoder; the handle remains allocated but its update
/// may be empty and should be destroyed rather than retried.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_stream_decoder_apply_listener_orientation(
    decoder: *mut openjoc_stream_decoder,
    update: *mut *mut openjoc_listener_orientation_update,
    accepted_sequence: *mut u64,
    superseded_sequence: *mut u64,
    retired: *mut *mut openjoc_listener_orientation_retired,
) -> openjoc_status {
    if decoder.is_null()
        || update.is_null()
        || accepted_sequence.is_null()
        || superseded_sequence.is_null()
        || retired.is_null()
    {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    let decoder = unsafe { &mut *decoder };
    unsafe {
        *accepted_sequence = 0;
        *superseded_sequence = NO_ORIENTATION_SEQUENCE;
        *retired = ptr::null_mut();
    }
    let handle = unsafe { *update };
    if handle.is_null() {
        return set_stream_message(decoder, "listener-orientation update handle is null");
    }
    let Some(prepared) = (unsafe { &mut *handle }).update.take() else {
        return set_stream_message(decoder, "listener-orientation update handle is empty");
    };
    let result = catch_unwind(AssertUnwindSafe(|| {
        decoder
            .decoder
            .apply_prepared_listener_orientation(prepared)
    }));
    match result {
        Ok(Ok(acceptance)) => {
            unsafe {
                *accepted_sequence = acceptance.accepted_sequence;
                *superseded_sequence = acceptance
                    .superseded_sequence
                    .unwrap_or(NO_ORIENTATION_SEQUENCE);
                *update = ptr::null_mut();
                let handle_ref = &mut *handle;
                handle_ref.update = None;
                handle_ref.retired_kernels = acceptance.retired_kernels;
                *retired = handle.cast::<openjoc_listener_orientation_retired>();
            }
            openjoc_status::OPENJOC_STATUS_OK
        }
        Ok(Err(failure)) => {
            unsafe { (&mut *handle).update = Some(failure.update) };
            let error_kind = if matches!(
                &failure.error,
                openjoc_api::RenderError::BinauralRequiresReset
            ) {
                BridgeErrorKind::ResetRequired
            } else {
                BridgeErrorKind::InvalidConfig
            };
            set_stream_error(
                decoder,
                BridgeError::new(error_kind, failure.error.to_string()),
            )
        }
        Err(_) => stream_panic_status(decoder),
    }
}

/// Returns the number of retired source kernels in a handle.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_listener_orientation_retired_count(
    retired: *const openjoc_listener_orientation_retired,
) -> usize {
    if retired.is_null() {
        return 0;
    }
    // SAFETY: pointer was checked and remains valid for the caller.
    unsafe { (&*retired).retired_kernels.len() }
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
        Ok(Ok(value)) => {
            refresh_live_snapshot(decoder);
            value
        }
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
        match decoder.decoder.try_reset() {
            Ok(()) => {
                decoder.last_error = CString::new("").expect("empty CString");
                refresh_live_snapshot(decoder);
                openjoc_status::OPENJOC_STATUS_OK
            }
            Err(error) => set_stream_error(decoder, error),
        }
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

/// Initializes the versioned caller-owned live inspection snapshot.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_live_inspection_snapshot_init(
    output: *mut openjoc_live_inspection_snapshot,
) -> openjoc_status {
    if output.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: output was checked and is caller-owned for the advertised size.
    unsafe {
        ptr::write_bytes(output, 0, 1);
        (*output).struct_size = LIVE_SNAPSHOT_SIZE;
    }
    openjoc_status::OPENJOC_STATUS_OK
}

/// Copies the latest bounded live semantic snapshot without touching the
/// decoder or media source. The snapshot is safe to read while decode updates
/// the underlying observer.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_stream_decoder_get_live_inspection_snapshot(
    decoder: *const openjoc_stream_decoder,
    output: *mut openjoc_live_inspection_snapshot,
) -> openjoc_status {
    if decoder.is_null() || output.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: pointers were checked and remain caller-owned.
    let output = unsafe { &mut *output };
    if output.struct_size < LIVE_SNAPSHOT_SIZE {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    // SAFETY: decoder was checked and remains valid for the caller.
    let snapshot = unsafe { &*decoder };
    *output = c_snapshot(&current_live_snapshot(snapshot));
    openjoc_status::OPENJOC_STATUS_OK
}

/// Copies a sanitized, versioned JSON representation of the live snapshot.
/// Serialization happens on the caller's thread and is never performed by the
/// decode observer update path.
#[unsafe(no_mangle)]
pub extern "C" fn openjoc_stream_decoder_copy_live_inspection_json(
    decoder: *const openjoc_stream_decoder,
    output: *mut c_char,
    output_capacity: usize,
    required_size: *mut usize,
) -> openjoc_status {
    if decoder.is_null() {
        return openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT;
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: decoder was checked and remains valid for the caller.
        let decoder = unsafe { &*decoder };
        let json = serde_json::to_string(&current_live_snapshot(decoder))
            .map_err(|_| openjoc_status::OPENJOC_STATUS_EXTERNAL_ERROR)?;
        let required = json.len().saturating_add(1);
        if !required_size.is_null() {
            // SAFETY: the caller provided storage for one size value.
            unsafe { *required_size = required };
        }
        if output.is_null() || output_capacity < required {
            return Err(openjoc_status::OPENJOC_STATUS_OUTPUT_PENDING);
        }
        // SAFETY: the caller provided an output buffer of output_capacity bytes.
        let bytes = unsafe { slice::from_raw_parts_mut(output.cast::<u8>(), output_capacity) };
        bytes[..json.len()].copy_from_slice(json.as_bytes());
        bytes[json.len()] = 0;
        Ok(openjoc_status::OPENJOC_STATUS_OK)
    }));
    match result {
        Ok(Ok(value) | Err(value)) => value,
        Err(_) => openjoc_status::OPENJOC_STATUS_EXTERNAL_ERROR,
    }
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

#[cfg(test)]
mod orientation_status_tests {
    use super::*;

    #[test]
    fn poisoned_stream_apply_requires_reset_and_preserves_update() {
        let layout = CString::new("7.1.4").expect("layout name");
        let mut config = std::mem::MaybeUninit::uninit();
        assert_eq!(
            openjoc_decoder_config_init_v1_7(config.as_mut_ptr()),
            openjoc_status::OPENJOC_STATUS_OK
        );
        let mut config = unsafe { config.assume_init() };
        config.render_mode = openjoc_render_mode::OPENJOC_RENDER_BINAURAL as u32;
        config.speaker_layout = layout.as_ptr();
        config.listener_orientation_pull_samples = 128;

        let mut stream = ptr::null_mut();
        assert_eq!(
            openjoc_stream_decoder_create(&raw const config, &raw mut stream),
            openjoc_status::OPENJOC_STATUS_OK
        );
        let mut preparer = ptr::null_mut();
        assert_eq!(
            openjoc_stream_decoder_get_listener_orientation_preparer(stream, &raw mut preparer),
            openjoc_status::OPENJOC_STATUS_OK
        );
        let pose = openjoc_listener_orientation {
            struct_size: ORIENTATION_SIZE,
            reserved: 0,
            x: 0.0,
            y: 0.0,
            z: 0.0,
            w: 1.0,
        };
        let mut update = ptr::null_mut();
        assert_eq!(
            openjoc_listener_orientation_prepare(preparer, &raw const pose, 0, 1, &raw mut update),
            openjoc_status::OPENJOC_STATUS_OK
        );

        // Model a panic contained by the outer C boundary. A subsequent apply
        // must report REQUIRE_RESET and return the original opaque update.
        unsafe { (&mut *stream).decoder.poison_after_outer_panic() };
        let mut accepted = 0;
        let mut superseded = 0;
        let mut retired = ptr::null_mut();
        assert_eq!(
            openjoc_stream_decoder_apply_listener_orientation(
                stream,
                &raw mut update,
                &raw mut accepted,
                &raw mut superseded,
                &raw mut retired
            ),
            openjoc_status::OPENJOC_STATUS_REQUIRE_RESET
        );
        assert!(!update.is_null());
        assert!(retired.is_null());
        openjoc_listener_orientation_update_destroy(update);
        openjoc_listener_orientation_preparer_destroy(preparer);
        openjoc_stream_decoder_destroy(stream);
    }
}

#[cfg(test)]
mod stream_lifecycle_tests {
    use super::*;

    #[test]
    fn c_stream_does_not_retain_unconsumable_traces() {
        let mut config = std::mem::MaybeUninit::uninit();
        assert_eq!(
            openjoc_decoder_config_init_v1_7(config.as_mut_ptr()),
            openjoc_status::OPENJOC_STATUS_OK
        );
        // SAFETY: the current initializer populated the full configuration.
        let config = unsafe { config.assume_init() };
        let mut stream = ptr::null_mut();
        assert_eq!(
            openjoc_stream_decoder_create(&raw const config, &raw mut stream),
            openjoc_status::OPENJOC_STATUS_OK
        );
        let fixture = include_bytes!("../../openjoc-wasm/testdata/joc.lifecycle.ec3");
        // SAFETY: successful creation returned a live exclusively owned handle.
        let decoder = unsafe { &mut (*stream).decoder };
        for cycle in 0..2 {
            for (index, bytes) in fixture.chunks_exact(4096).cycle().take(256).enumerate() {
                decoder
                    .send_packet(PacketRef {
                        data: bytes,
                        pts: Some(i64::try_from((index + cycle) * 1536).unwrap()),
                        dts: None,
                        duration: None,
                        time_base: Rational::SAMPLE_TIME_BASE,
                        stream_index: 0,
                        discontinuity: false,
                        preroll: false,
                    })
                    .unwrap();
                while matches!(decoder.receive_frame().unwrap(), ReceiveOutcome::Frame(_)) {}
            }
            decoder.drain().unwrap();
            while matches!(decoder.receive_frame().unwrap(), ReceiveOutcome::Frame(_)) {}
            assert!(matches!(
                decoder.receive_frame().unwrap(),
                ReceiveOutcome::EndOfStream
            ));
            assert_eq!(
                decoder.take_traces(),
                [] as [openjoc_ffmpeg::AccessUnitTrace; 0]
            );
            let snapshot = decoder.live_inspection_snapshot();
            assert_eq!(snapshot.observed_au_count, 256);
            assert_eq!(
                snapshot.coverage,
                if cycle == 0 {
                    "complete_continuous"
                } else {
                    "partial"
                }
            );
            decoder.reset();
        }
        openjoc_stream_decoder_destroy(stream);
    }
}
