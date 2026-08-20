#![allow(unsafe_code)]
#![allow(clippy::borrow_as_ptr)]

use openjoc_capi::*;
use std::ptr;

#[test]
fn version_and_struct_initialization_are_stable() {
    assert_eq!(openjoc_get_abi_version(), 0x0001_0003);
    assert_eq!(std::mem::size_of::<openjoc_decoder_config>() as u32, {
        let mut config = std::mem::MaybeUninit::uninit();
        assert_eq!(
            openjoc_decoder_config_init(config.as_mut_ptr()),
            openjoc_status::OPENJOC_STATUS_OK
        );
        // The public init call writes the complete descriptor, including its size.
        unsafe { config.assume_init().struct_size }
    });
}

#[test]
fn packet_stream_bridge_is_independent_bounded_and_reports_semantics() {
    let mut config = std::mem::MaybeUninit::uninit();
    assert_eq!(
        openjoc_decoder_config_init(config.as_mut_ptr()),
        openjoc_status::OPENJOC_STATUS_OK
    );
    let config = unsafe { config.assume_init() };
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
fn pre_dialnorm_config_size_keeps_the_calibrated_default() {
    let mut config = std::mem::MaybeUninit::uninit();
    assert_eq!(
        openjoc_decoder_config_init(config.as_mut_ptr()),
        openjoc_status::OPENJOC_STATUS_OK
    );
    let mut config = unsafe { config.assume_init() };
    config.struct_size = std::mem::size_of::<openjoc_decoder_config>() as u32 - 4;
    let mut decoder = ptr::null_mut();
    assert_eq!(
        openjoc_decoder_create(&config, &mut decoder),
        openjoc_status::OPENJOC_STATUS_OK
    );
    openjoc_decoder_destroy(decoder);
}

#[test]
fn create_destroy_multiple_instances_and_invalid_config() {
    let mut config = std::mem::MaybeUninit::uninit();
    assert_eq!(
        openjoc_decoder_config_init(config.as_mut_ptr()),
        openjoc_status::OPENJOC_STATUS_OK
    );
    let config = unsafe { config.assume_init() };

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
    let mut config = std::mem::MaybeUninit::uninit();
    assert_eq!(
        openjoc_decoder_config_init(config.as_mut_ptr()),
        openjoc_status::OPENJOC_STATUS_OK
    );
    let config = unsafe { config.assume_init() };
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
