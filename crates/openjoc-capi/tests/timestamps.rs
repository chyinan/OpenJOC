#![allow(unsafe_code)]
#![allow(clippy::borrow_as_ptr)]

use openjoc_capi::*;
use std::mem::MaybeUninit;

const FIXTURE: &[u8] = include_bytes!("fixtures/timestamps.ec3");

#[test]
fn direct_packet_api_supports_late_pts_and_retries_unrepresentable_origin() {
    let mut config = MaybeUninit::uninit();
    assert_eq!(
        openjoc_decoder_config_init_v1_7(config.as_mut_ptr()),
        openjoc_status::OPENJOC_STATUS_OK
    );
    // SAFETY: the successful current-version initializer wrote the full config.
    let config = unsafe { config.assume_init() };
    let mut handle = MaybeUninit::uninit();
    assert_eq!(
        openjoc_decoder_create(&config, handle.as_mut_ptr()),
        openjoc_status::OPENJOC_STATUS_OK
    );
    // SAFETY: successful creation initialized the owned decoder handle.
    let handle = unsafe { handle.assume_init() };
    let send = |index: usize, pts| {
        let bytes = &FIXTURE[index * 4096..(index + 1) * 4096];
        openjoc_decoder_send_packet(handle, bytes.as_ptr(), bytes.len(), pts, 0)
    };
    assert_eq!(
        send(0, OPENJOC_NO_PTS),
        openjoc_status::OPENJOC_STATUS_NEED_MORE_INPUT
    );
    // INT64_MIN itself is the C sentinel, so use MIN+1 as the real late PTS.
    assert_eq!(
        send(1, i64::MIN + 1),
        openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT
    );
    assert_eq!(
        send(1, 1536),
        openjoc_status::OPENJOC_STATUS_FRAME_AVAILABLE
    );
    let mut frame = MaybeUninit::uninit();
    assert_eq!(
        openjoc_pcm_frame_init(frame.as_mut_ptr()),
        openjoc_status::OPENJOC_STATUS_OK
    );
    assert_eq!(
        openjoc_decoder_receive_frame(handle, frame.as_mut_ptr()),
        openjoc_status::OPENJOC_STATUS_FRAME_AVAILABLE
    );
    // SAFETY: the initialized frame was filled by a successful receive.
    assert_eq!(unsafe { frame.assume_init_ref() }.pts_samples, 0);
    assert_eq!(
        openjoc_decoder_receive_frame(handle, frame.as_mut_ptr()),
        openjoc_status::OPENJOC_STATUS_NEED_MORE_INPUT
    );
    assert_eq!(
        send(2, 3072),
        openjoc_status::OPENJOC_STATUS_FRAME_AVAILABLE
    );
    assert_eq!(
        openjoc_decoder_receive_frame(handle, frame.as_mut_ptr()),
        openjoc_status::OPENJOC_STATUS_FRAME_AVAILABLE
    );
    // SAFETY: the successful receive filled the frame again.
    assert_eq!(unsafe { frame.assume_init_ref() }.pts_samples, 1536);
    openjoc_decoder_destroy(handle);
}

#[test]
fn inferred_c_sentinel_timestamp_fails_closed_and_reset_recovers() {
    let mut config = MaybeUninit::uninit();
    assert_eq!(
        openjoc_decoder_config_init_v1_7(config.as_mut_ptr()),
        openjoc_status::OPENJOC_STATUS_OK
    );
    // SAFETY: successful initialization wrote the whole current config.
    let config = unsafe { config.assume_init() };
    let mut handle = MaybeUninit::uninit();
    assert_eq!(
        openjoc_decoder_create(&config, handle.as_mut_ptr()),
        openjoc_status::OPENJOC_STATUS_OK
    );
    // SAFETY: successful creation initialized the owned handle.
    let handle = unsafe { handle.assume_init() };
    let send = |index: usize, pts| {
        let bytes = &FIXTURE[index * 4096..(index + 1) * 4096];
        openjoc_decoder_send_packet(handle, bytes.as_ptr(), bytes.len(), pts, 0)
    };
    assert_eq!(
        send(0, OPENJOC_NO_PTS),
        openjoc_status::OPENJOC_STATUS_NEED_MORE_INPUT
    );
    assert_eq!(
        send(1, i64::MIN + 1536),
        openjoc_status::OPENJOC_STATUS_FRAME_AVAILABLE
    );
    let mut output = MaybeUninit::uninit();
    assert_eq!(
        openjoc_pcm_frame_init(output.as_mut_ptr()),
        openjoc_status::OPENJOC_STATUS_OK
    );
    // SAFETY: initialization wrote every field; install observable caller state.
    let mut output = unsafe { output.assume_init() };
    output.pts_samples = 42;
    output.sample_count = 17;
    let original_data = output.data;
    assert_eq!(
        openjoc_decoder_receive_frame(handle, &mut output),
        openjoc_status::OPENJOC_STATUS_RENDER_ERROR
    );
    assert_eq!(output.pts_samples, 42);
    assert_eq!(output.sample_count, 17);
    assert_eq!(output.data, original_data);
    // SAFETY: the live handle owns this error string until the next mutation.
    let error = unsafe { std::ffi::CStr::from_ptr(openjoc_decoder_last_error(handle)) };
    assert!(error.to_str().unwrap().contains("reserved OPENJOC_NO_PTS"));
    assert_eq!(
        openjoc_decoder_receive_frame(handle, &mut output),
        openjoc_status::OPENJOC_STATUS_REQUIRE_RESET
    );
    assert_eq!(
        send(2, i64::MIN + 3072),
        openjoc_status::OPENJOC_STATUS_REQUIRE_RESET
    );
    assert_eq!(
        openjoc_decoder_drain(handle),
        openjoc_status::OPENJOC_STATUS_REQUIRE_RESET
    );
    assert_eq!(
        openjoc_decoder_reset(handle),
        openjoc_status::OPENJOC_STATUS_OK
    );
    assert_eq!(send(0, 0), openjoc_status::OPENJOC_STATUS_NEED_MORE_INPUT);
    assert_eq!(
        send(1, 1536),
        openjoc_status::OPENJOC_STATUS_FRAME_AVAILABLE
    );
    assert_eq!(
        openjoc_decoder_receive_frame(handle, &mut output),
        openjoc_status::OPENJOC_STATUS_FRAME_AVAILABLE
    );
    assert_eq!(output.pts_samples, 0);
    openjoc_decoder_destroy(handle);
}
