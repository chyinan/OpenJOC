#![allow(unsafe_code)]
#![allow(clippy::borrow_as_ptr)]

use openjoc_capi::*;
use std::ptr;

fn pose(x: f64, y: f64, z: f64, w: f64) -> openjoc_listener_orientation {
    openjoc_listener_orientation {
        struct_size: std::mem::size_of::<openjoc_listener_orientation>() as u32,
        reserved: 0,
        x,
        y,
        z,
        w,
    }
}

fn zeroed_config_backing() -> openjoc_decoder_config {
    // ABI 1.3/1.4 initializers write only their historical prefixes. Every
    // field in this C config is an integer scalar or nullable raw pointer, so
    // the all-zero representation is valid for the unappended tail as well.
    unsafe { std::mem::zeroed() }
}

#[repr(C)]
struct LegacyDecoderConfigV1_6 {
    struct_size: u32,
    render_mode: u32,
    speaker_layout: *const std::ffi::c_char,
    downmix: u32,
    drc: u32,
    drc_boost_percent: u8,
    drc_cut_percent: u8,
    validation_profile: u32,
    sofa_data: *const u8,
    sofa_size: usize,
    virtual_layout: *const std::ffi::c_char,
    lfe_policy: u32,
    dialnorm_mode: u32,
    custom_speaker_layout: *const openjoc_custom_speaker_layout,
    hrtf_preset: u32,
}

#[repr(C, align(8))]
struct ShortOrientationState {
    struct_size: u32,
    canary: [u8; 8],
}

#[repr(C, align(8))]
struct ShortOrientationPose {
    struct_size: u32,
    canary: [u8; 8],
}

#[test]
fn version_and_struct_initialization_are_stable() {
    assert_eq!(openjoc_get_abi_version(), 0x0001_0007);
    let current_size = {
        let mut config = std::mem::MaybeUninit::uninit();
        assert_eq!(
            openjoc_decoder_config_init_v1_7(config.as_mut_ptr()),
            openjoc_status::OPENJOC_STATUS_OK
        );
        let initialized = unsafe { config.assume_init() };
        assert_eq!(
            initialized.struct_size as usize,
            std::mem::size_of::<openjoc_decoder_config>()
        );
        assert_eq!(initialized.listener_orientation_pull_samples, 0);
        initialized.struct_size
    };
    assert_eq!(
        current_size as usize,
        std::mem::size_of::<openjoc_decoder_config>()
    );
    assert!(current_size > std::mem::size_of::<LegacyDecoderConfigV1_6>() as u32);
    let old_v1_6_size = std::mem::size_of::<LegacyDecoderConfigV1_6>();
    let canary_len = old_v1_6_size + 16;
    // The historical config is pointer-aligned. Keep that property while
    // leaving guard bytes after its exact v1.6 extent.
    let mut canary_storage = vec![u64::MAX; canary_len.div_ceil(std::mem::size_of::<u64>())];
    let canary = unsafe {
        std::slice::from_raw_parts_mut(canary_storage.as_mut_ptr().cast::<u8>(), canary_len)
    };
    canary.fill(0xA5);
    assert_eq!(
        openjoc_decoder_config_init_v1_6(
            canary_storage.as_mut_ptr().cast::<openjoc_decoder_config>()
        ),
        openjoc_status::OPENJOC_STATUS_OK
    );
    assert_eq!(
        unsafe { ptr::read_unaligned(canary.as_ptr().cast::<u32>()) } as usize,
        old_v1_6_size
    );
    assert!(canary[old_v1_6_size..].iter().all(|byte| *byte == 0xA5));
    {
        // A legacy caller may leave the old struct's trailing pad nonzero. It
        // must never be interpreted as the ABI 1.7 opt-in field.
        let padding_start =
            std::mem::offset_of!(LegacyDecoderConfigV1_6, hrtf_preset) + std::mem::size_of::<u32>();
        canary[padding_start..old_v1_6_size].fill(0xFF);
        let mut decoder = ptr::null_mut();
        assert_eq!(
            openjoc_stream_decoder_create(
                canary_storage.as_ptr().cast::<openjoc_decoder_config>(),
                &mut decoder
            ),
            openjoc_status::OPENJOC_STATUS_OK
        );
        let descriptor = unsafe {
            std::ffi::CStr::from_ptr(openjoc_stream_decoder_get_config_descriptor(decoder))
        }
        .to_str()
        .unwrap();
        assert!(!descriptor.contains("listener_orientation_pull=enabled"));
        openjoc_stream_decoder_destroy(decoder);
    }
    {
        let mut config = std::mem::MaybeUninit::uninit();
        assert_eq!(
            openjoc_decoder_config_init(config.as_mut_ptr()),
            openjoc_status::OPENJOC_STATUS_OK
        );
        // The legacy initializer writes only the ABI 1.3-sized prefix.
        assert_eq!(
            unsafe { ptr::read_unaligned(config.as_ptr().cast::<u32>()) },
            std::mem::offset_of!(openjoc_decoder_config, custom_speaker_layout) as u32
        );
        let mut config = std::mem::MaybeUninit::uninit();
        assert_eq!(
            openjoc_decoder_config_init_v1_4(config.as_mut_ptr()),
            openjoc_status::OPENJOC_STATUS_OK
        );
        assert_eq!(
            unsafe { ptr::read_unaligned(config.as_ptr().cast::<u32>()) },
            std::mem::offset_of!(openjoc_decoder_config, hrtf_preset) as u32
        );
        assert_eq!(
            old_v1_6_size as u32,
            std::mem::size_of::<LegacyDecoderConfigV1_6>() as u32
        );
    }
}

#[test]
fn retired_aachen_preset_code_falls_back_to_default_d1() {
    let mut config = std::mem::MaybeUninit::uninit();
    assert_eq!(
        openjoc_decoder_config_init_v1_7(config.as_mut_ptr()),
        openjoc_status::OPENJOC_STATUS_OK
    );
    let mut config = unsafe { config.assume_init() };
    config.render_mode = openjoc_render_mode::OPENJOC_RENDER_BINAURAL as u32;
    config.hrtf_preset = 2;

    let mut decoder = ptr::null_mut();
    assert_eq!(
        openjoc_stream_decoder_create(&config, &mut decoder),
        openjoc_status::OPENJOC_STATUS_OK
    );
    let descriptor =
        unsafe { std::ffi::CStr::from_ptr(openjoc_stream_decoder_get_config_descriptor(decoder)) }
            .to_str()
            .expect("configuration descriptor");
    assert!(descriptor.contains("binaural_hrtf_source=builtin:SADIE_II_D1_KU100_v2-2"));
    openjoc_stream_decoder_destroy(decoder);
}

#[test]
fn c_orientation_handles_preserve_failures_and_fence_epochs() {
    let layout = std::ffi::CString::new("7.1.4").unwrap();
    let mut config = std::mem::MaybeUninit::uninit();
    assert_eq!(
        openjoc_decoder_config_init_v1_7(config.as_mut_ptr()),
        openjoc_status::OPENJOC_STATUS_OK
    );
    let mut config = unsafe { config.assume_init() };
    config.render_mode = openjoc_render_mode::OPENJOC_RENDER_BINAURAL as u32;
    config.speaker_layout = layout.as_ptr();
    config.listener_orientation_pull_samples = 128;

    let mut decoder = ptr::null_mut();
    assert_eq!(
        openjoc_decoder_create(&config, &mut decoder),
        openjoc_status::OPENJOC_STATUS_OK
    );
    let mut state = std::mem::MaybeUninit::uninit();
    assert_eq!(
        openjoc_listener_orientation_state_init(state.as_mut_ptr()),
        openjoc_status::OPENJOC_STATUS_OK
    );
    let mut state = unsafe { state.assume_init() };
    assert_eq!(
        openjoc_decoder_get_listener_orientation_state(decoder, &mut state),
        openjoc_status::OPENJOC_STATUS_OK
    );
    assert_eq!(state.stream_epoch, 0);
    assert_eq!(state.pending_binaural_input_samples, 0);

    // A C caller may provide only the size prefix. Reject it before forming a
    // reference to the full state object or touching adjacent canary bytes.
    let mut short_state = ShortOrientationState {
        struct_size: 4,
        canary: [0xA5; 8],
    };
    assert_eq!(
        openjoc_decoder_get_listener_orientation_state(
            decoder,
            ptr::from_mut(&mut short_state).cast::<openjoc_listener_orientation_state>()
        ),
        openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT
    );
    assert_eq!(short_state.canary, [0xA5; 8]);

    let mut preparer = ptr::null_mut();
    assert_eq!(
        openjoc_decoder_get_listener_orientation_preparer(decoder, &mut preparer),
        openjoc_status::OPENJOC_STATUS_OK
    );
    let short_pose = ShortOrientationPose {
        struct_size: 4,
        canary: [0x5A; 8],
    };
    let mut update = ptr::null_mut();
    assert_eq!(
        openjoc_listener_orientation_prepare(
            preparer,
            ptr::from_ref(&short_pose).cast::<openjoc_listener_orientation>(),
            0,
            1,
            &mut update
        ),
        openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT
    );
    assert!(update.is_null());
    assert_eq!(short_pose.canary, [0x5A; 8]);
    assert_eq!(
        unsafe {
            std::ffi::CStr::from_ptr(openjoc_listener_orientation_preparer_last_error(preparer))
        }
        .to_str()
        .unwrap(),
        "invalid OpenJOC configuration: listener_orientation.struct_size is too small"
    );
    assert_eq!(
        openjoc_listener_orientation_prepare(
            preparer,
            &pose(0.0, 0.0, 0.0, 1.0),
            0,
            1,
            &mut update
        ),
        openjoc_status::OPENJOC_STATUS_OK
    );
    let mut accepted = 0;
    let mut superseded = 0;
    let mut retired = ptr::null_mut();
    assert_eq!(
        openjoc_decoder_apply_listener_orientation(
            decoder,
            &mut update,
            &mut accepted,
            &mut superseded,
            &mut retired
        ),
        openjoc_status::OPENJOC_STATUS_OK
    );
    assert_eq!(accepted, 1);
    assert_eq!(superseded, u64::MAX);
    assert!(update.is_null());
    assert!(!retired.is_null());
    assert_eq!(openjoc_listener_orientation_retired_count(retired), 0);
    openjoc_listener_orientation_retired_destroy(retired);

    let yaw = 9.0_f64.to_radians();
    assert_eq!(
        openjoc_listener_orientation_prepare(
            preparer,
            &pose(0.0, 0.0, (yaw * 0.5).sin(), (yaw * 0.5).cos()),
            0,
            2,
            &mut update
        ),
        openjoc_status::OPENJOC_STATUS_OK
    );
    assert_eq!(
        openjoc_decoder_apply_listener_orientation(
            decoder,
            &mut update,
            &mut accepted,
            &mut superseded,
            &mut retired
        ),
        openjoc_status::OPENJOC_STATUS_OK
    );
    assert_eq!(accepted, 2);
    assert_eq!(superseded, 1);
    assert_eq!(openjoc_listener_orientation_retired_count(retired), 11);
    openjoc_listener_orientation_retired_destroy(retired);

    assert_eq!(
        openjoc_listener_orientation_prepare(
            preparer,
            &pose(0.0, 0.0, 0.0, 1.0),
            0,
            3,
            &mut update
        ),
        openjoc_status::OPENJOC_STATUS_OK
    );
    assert_eq!(
        openjoc_decoder_flush(decoder),
        openjoc_status::OPENJOC_STATUS_OK
    );
    assert_eq!(
        openjoc_decoder_get_listener_orientation_state(decoder, &mut state),
        openjoc_status::OPENJOC_STATUS_OK
    );
    assert_eq!(state.stream_epoch, 1);
    assert_eq!(state.has_pending_update, 0);
    assert_eq!(
        openjoc_decoder_apply_listener_orientation(
            decoder,
            &mut update,
            &mut accepted,
            &mut superseded,
            &mut retired
        ),
        openjoc_status::OPENJOC_STATUS_RENDER_ERROR
    );
    assert!(
        !update.is_null(),
        "failed apply keeps the prepared update usable"
    );
    assert!(!openjoc_decoder_last_error(decoder).is_null());
    openjoc_listener_orientation_update_destroy(update);
    openjoc_listener_orientation_preparer_destroy(preparer);
    openjoc_decoder_destroy(decoder);

    // Stream mode shares its immutable preparer with the lazily-created
    // decoder, and its epoch advances even before the first positive JOC AU.
    let mut stream = ptr::null_mut();
    assert_eq!(
        openjoc_stream_decoder_create(&config, &mut stream),
        openjoc_status::OPENJOC_STATUS_OK
    );
    let mut stream_preparer = ptr::null_mut();
    assert_eq!(
        openjoc_stream_decoder_get_listener_orientation_preparer(stream, &mut stream_preparer),
        openjoc_status::OPENJOC_STATUS_OK
    );
    assert_eq!(
        openjoc_listener_orientation_prepare(
            stream_preparer,
            &pose(0.0, 0.0, 0.0, 1.0),
            0,
            1,
            &mut update
        ),
        openjoc_status::OPENJOC_STATUS_OK
    );
    assert_eq!(
        openjoc_stream_decoder_apply_listener_orientation(
            stream,
            &mut update,
            &mut accepted,
            &mut superseded,
            &mut retired
        ),
        openjoc_status::OPENJOC_STATUS_OK
    );
    assert_eq!(superseded, u64::MAX);
    openjoc_listener_orientation_retired_destroy(retired);
    assert_eq!(
        openjoc_listener_orientation_prepare(
            stream_preparer,
            &pose(0.0, 0.0, 0.0, 1.0),
            0,
            2,
            &mut update
        ),
        openjoc_status::OPENJOC_STATUS_OK
    );
    assert_eq!(
        openjoc_stream_decoder_reset(stream),
        openjoc_status::OPENJOC_STATUS_OK
    );
    assert_eq!(
        openjoc_stream_decoder_get_listener_orientation_state(stream, &mut state),
        openjoc_status::OPENJOC_STATUS_OK
    );
    assert_eq!(state.stream_epoch, 1);
    assert_eq!(state.has_pending_update, 0);
    assert_eq!(
        openjoc_stream_decoder_apply_listener_orientation(
            stream,
            &mut update,
            &mut accepted,
            &mut superseded,
            &mut retired
        ),
        openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT
    );
    assert!(!update.is_null());
    openjoc_listener_orientation_update_destroy(update);
    openjoc_listener_orientation_preparer_destroy(stream_preparer);
    openjoc_stream_decoder_destroy(stream);
}

#[test]
fn custom_geometry_descriptor_is_owned_and_exposes_ordered_semantics() {
    let names = [
        std::ffi::CString::new("Left").unwrap(),
        std::ffi::CString::new("Right").unwrap(),
        std::ffi::CString::new("Subwoofer").unwrap(),
    ];
    let speakers = [
        openjoc_custom_speaker {
            struct_size: std::mem::size_of::<openjoc_custom_speaker>() as u32,
            name: names[0].as_ptr(),
            azimuth: -35.0,
            elevation: 0.0,
            role: openjoc_speaker_role::OPENJOC_SPEAKER_FULL_RANGE as u32,
        },
        openjoc_custom_speaker {
            struct_size: std::mem::size_of::<openjoc_custom_speaker>() as u32,
            name: names[1].as_ptr(),
            azimuth: 35.0,
            elevation: 0.0,
            role: openjoc_speaker_role::OPENJOC_SPEAKER_FULL_RANGE as u32,
        },
        openjoc_custom_speaker {
            struct_size: std::mem::size_of::<openjoc_custom_speaker>() as u32,
            name: names[2].as_ptr(),
            azimuth: 0.0,
            elevation: -20.0,
            role: openjoc_speaker_role::OPENJOC_SPEAKER_LFE as u32,
        },
    ];
    let layout_name = std::ffi::CString::new("c-api-studio").unwrap();
    let layout = openjoc_custom_speaker_layout {
        struct_size: std::mem::size_of::<openjoc_custom_speaker_layout>() as u32,
        version: openjoc_scene::SPEAKER_LAYOUT_JSON_VERSION,
        name: layout_name.as_ptr(),
        speakers: speakers.as_ptr(),
        speaker_count: speakers.len(),
    };
    let mut config = zeroed_config_backing();
    assert_eq!(
        openjoc_decoder_config_init_v1_4(&mut config),
        openjoc_status::OPENJOC_STATUS_OK
    );
    config.custom_speaker_layout = &layout;
    let mut decoder = ptr::null_mut();
    assert_eq!(
        openjoc_decoder_create(&config, &mut decoder),
        openjoc_status::OPENJOC_STATUS_OK
    );
    let mut info = std::mem::MaybeUninit::uninit();
    openjoc_output_info_init(info.as_mut_ptr());
    assert_eq!(
        openjoc_decoder_get_output_info(decoder, info.as_mut_ptr()),
        openjoc_status::OPENJOC_STATUS_OK
    );
    let info = unsafe { info.assume_init() };
    assert_eq!(
        unsafe { std::ffi::CStr::from_ptr(info.layout_name) },
        layout_name.as_c_str()
    );
    assert_eq!(info.channel_count, 3);
    for (index, name) in names.iter().enumerate() {
        assert_eq!(
            unsafe { std::ffi::CStr::from_ptr(openjoc_decoder_get_channel_label(decoder, index)) },
            name.as_c_str()
        );
    }
    openjoc_decoder_destroy(decoder);
}

#[test]
fn packet_stream_bridge_is_independent_bounded_and_reports_semantics() {
    let mut config = zeroed_config_backing();
    assert_eq!(
        openjoc_decoder_config_init(&mut config),
        openjoc_status::OPENJOC_STATUS_OK
    );
    let mut first = ptr::null_mut();
    let mut second = ptr::null_mut();
    assert_eq!(
        openjoc_stream_decoder_create(&config, &mut first),
        openjoc_status::OPENJOC_STATUS_OK
    );
    assert_eq!(
        openjoc_stream_decoder_create(&config, &mut second),
        openjoc_status::OPENJOC_STATUS_OK
    );
    assert!(!first.is_null());
    assert!(!second.is_null());
    assert_ne!(first, second);

    let mut info = std::mem::MaybeUninit::uninit();
    assert_eq!(
        openjoc_output_info_init(info.as_mut_ptr()),
        openjoc_status::OPENJOC_STATUS_OK
    );
    assert_eq!(
        openjoc_stream_decoder_get_output_info(first, info.as_mut_ptr()),
        openjoc_status::OPENJOC_STATUS_OK
    );
    let info = unsafe { info.assume_init() };
    assert_eq!(info.sample_format, 1);
    assert_eq!(info.sample_rate, 48_000);
    assert_eq!(info.channel_count, 6);
    assert_eq!(info.latency_samples, 609);
    assert!(!openjoc_stream_decoder_get_channel_label(first, 0).is_null());
    assert!(!openjoc_stream_decoder_get_config_descriptor(first).is_null());
    assert!(!openjoc_stream_decoder_get_config_fingerprint(first).is_null());

    let fragment = [0x0b_u8, 0x77_u8];
    assert_eq!(
        openjoc_stream_decoder_send_chunk(
            first,
            fragment.as_ptr(),
            fragment.len(),
            OPENJOC_NO_PTS,
            0,
        ),
        openjoc_status::OPENJOC_STATUS_NEED_MORE_INPUT
    );
    assert_eq!(
        openjoc_stream_decoder_flush(first),
        openjoc_status::OPENJOC_STATUS_OK
    );
    assert_eq!(
        openjoc_stream_decoder_drain(first),
        openjoc_status::OPENJOC_STATUS_END_OF_STREAM
    );

    openjoc_stream_decoder_destroy(first);
    openjoc_stream_decoder_destroy(second);
}

#[test]
fn live_inspection_snapshot_abi_is_versioned_and_has_explicit_live_json() {
    let mut config = zeroed_config_backing();
    assert_eq!(
        openjoc_decoder_config_init_v1_4(&mut config),
        openjoc_status::OPENJOC_STATUS_OK
    );
    let mut decoder = ptr::null_mut();
    assert_eq!(
        openjoc_stream_decoder_create(&config, &mut decoder),
        openjoc_status::OPENJOC_STATUS_OK
    );

    let mut snapshot = std::mem::MaybeUninit::uninit();
    assert_eq!(
        openjoc_live_inspection_snapshot_init(snapshot.as_mut_ptr()),
        openjoc_status::OPENJOC_STATUS_OK
    );
    let mut snapshot = unsafe { snapshot.assume_init() };
    assert_eq!(
        openjoc_stream_decoder_get_live_inspection_snapshot(decoder, &mut snapshot),
        openjoc_status::OPENJOC_STATUS_OK
    );
    assert_eq!(snapshot.schema_version, 1);
    assert_eq!(snapshot.observation_epoch, 1);
    assert_eq!(snapshot.stream_present, 0);

    let mut json = vec![0_i8; 4096];
    let mut required = 0_usize;
    assert_eq!(
        openjoc_stream_decoder_copy_live_inspection_json(
            decoder,
            json.as_mut_ptr(),
            json.len(),
            &mut required,
        ),
        openjoc_status::OPENJOC_STATUS_OK
    );
    assert!(required > 1);
    let json = unsafe { std::ffi::CStr::from_ptr(json.as_ptr()) }
        .to_str()
        .expect("live JSON");
    assert!(json.contains("\"inspection_kind\":\"live_decode_snapshot\""));
    assert!(json.contains("\"observation_scope\":\"live_decode\""));
    assert!(json.contains("\"coverage\":\"partial\""));

    openjoc_stream_decoder_destroy(decoder);
}

#[test]
fn pre_dialnorm_config_size_keeps_the_calibrated_default() {
    let mut config = zeroed_config_backing();
    assert_eq!(
        openjoc_decoder_config_init(&mut config),
        openjoc_status::OPENJOC_STATUS_OK
    );
    // ABI 1.0 ends immediately before dialnorm_mode. The sentinel is outside
    // the advertised prefix and must be ignored in favor of the default.
    config.struct_size = std::mem::offset_of!(openjoc_decoder_config, dialnorm_mode) as u32;
    config.dialnorm_mode = u32::MAX;
    let mut decoder = ptr::null_mut();
    assert_eq!(
        openjoc_decoder_create(&config, &mut decoder),
        openjoc_status::OPENJOC_STATUS_OK
    );
    openjoc_decoder_destroy(decoder);
}

#[test]
fn create_destroy_multiple_instances_and_invalid_config() {
    let mut config = zeroed_config_backing();
    assert_eq!(
        openjoc_decoder_config_init(&mut config),
        openjoc_status::OPENJOC_STATUS_OK
    );

    let mut first = ptr::null_mut();
    let mut second = ptr::null_mut();
    assert_eq!(
        openjoc_decoder_create(&config, &mut first),
        openjoc_status::OPENJOC_STATUS_OK
    );
    assert_eq!(
        openjoc_decoder_create(&config, &mut second),
        openjoc_status::OPENJOC_STATUS_OK
    );
    assert!(!first.is_null());
    assert!(!second.is_null());
    assert_ne!(first, second);

    let mut analog = config;
    analog.dialnorm_mode = openjoc_dialnorm_mode::OPENJOC_DIALNORM_ANALOG as u32;
    let mut analog_decoder = ptr::null_mut();
    assert_eq!(
        openjoc_decoder_create(&analog, &mut analog_decoder),
        openjoc_status::OPENJOC_STATUS_OK
    );
    openjoc_decoder_destroy(analog_decoder);

    let mut invalid = config;
    invalid.downmix = 999;
    let mut no_decoder = ptr::null_mut();
    assert_eq!(
        openjoc_decoder_create(&invalid, &mut no_decoder),
        openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT
    );
    assert!(no_decoder.is_null());
    openjoc_decoder_destroy(first);
    openjoc_decoder_destroy(second);
}

#[test]
fn malformed_packet_drain_flush_and_panic_containment() {
    let mut config = zeroed_config_backing();
    assert_eq!(
        openjoc_decoder_config_init(&mut config),
        openjoc_status::OPENJOC_STATUS_OK
    );
    let mut decoder = ptr::null_mut();
    assert_eq!(
        openjoc_decoder_create(&config, &mut decoder),
        openjoc_status::OPENJOC_STATUS_OK
    );

    let packet = [0x0b_u8, 0x77_u8];
    let send =
        openjoc_decoder_send_packet(decoder, packet.as_ptr(), packet.len(), OPENJOC_NO_PTS, 0);
    assert!(matches!(
        send,
        openjoc_status::OPENJOC_STATUS_DECODE_ERROR
            | openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT
    ));
    assert!(!openjoc_decoder_last_error(decoder).is_null());
    assert_eq!(
        openjoc_decoder_flush(decoder),
        openjoc_status::OPENJOC_STATUS_OK
    );
    assert_eq!(
        openjoc_decoder_drain(decoder),
        openjoc_status::OPENJOC_STATUS_END_OF_STREAM
    );

    let mut frame = std::mem::MaybeUninit::uninit();
    assert_eq!(
        openjoc_pcm_frame_init(frame.as_mut_ptr()),
        openjoc_status::OPENJOC_STATUS_OK
    );
    assert_eq!(
        openjoc_decoder_receive_frame(decoder, frame.as_mut_ptr()),
        openjoc_status::OPENJOC_STATUS_END_OF_STREAM
    );
    openjoc_decoder_destroy(decoder);
}
