#![allow(unsafe_code)]
#![allow(clippy::borrow_as_ptr)]

use openjoc_capi::*;
use std::ptr;

#[test]
fn version_and_struct_initialization_are_stable() {
    assert_eq!(openjoc_get_abi_version(), 0x0001_0000);
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
