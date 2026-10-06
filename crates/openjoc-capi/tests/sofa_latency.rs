#![allow(unsafe_code)]
#![allow(clippy::borrow_as_ptr)]
use openjoc_capi::*;

#[test]
fn stream_output_info_includes_custom_sofa_resampling_latency() {
    for (rate, expected, bytes) in [
        (
            24_000,
            610,
            include_bytes!("../../openjoc-ffmpeg/tests/fixtures/sofa-latency/24000.sofa"),
        ),
        (
            44_100,
            596,
            include_bytes!("../../openjoc-ffmpeg/tests/fixtures/sofa-latency/44100.sofa"),
        ),
        (
            48_000,
            577,
            include_bytes!("../../openjoc-ffmpeg/tests/fixtures/sofa-latency/48000.sofa"),
        ),
        (
            96_000,
            594,
            include_bytes!("../../openjoc-ffmpeg/tests/fixtures/sofa-latency/96000.sofa"),
        ),
    ] {
        for pull in [0, 128] {
            let mut config = std::mem::MaybeUninit::uninit();
            assert_eq!(
                openjoc_decoder_config_init_v1_7(config.as_mut_ptr()),
                openjoc_status::OPENJOC_STATUS_OK
            );
            // SAFETY: the initializer populated the complete configuration.
            let mut config = unsafe { config.assume_init() };
            config.render_mode = openjoc_render_mode::OPENJOC_RENDER_BINAURAL as u32;
            config.speaker_layout = c"5.1".as_ptr();
            config.virtual_layout = c"5.1".as_ptr();
            config.sofa_data = bytes.as_ptr();
            config.sofa_size = bytes.len();
            config.listener_orientation_pull_samples = pull;
            let mut stream = std::ptr::null_mut();
            assert_eq!(
                openjoc_stream_decoder_create(&config, &mut stream),
                openjoc_status::OPENJOC_STATUS_OK
            );
            for reset in [false, true] {
                if reset {
                    assert_eq!(
                        openjoc_stream_decoder_reset(stream),
                        openjoc_status::OPENJOC_STATUS_OK
                    );
                }
                let mut info = std::mem::MaybeUninit::uninit();
                assert_eq!(
                    openjoc_output_info_init(info.as_mut_ptr()),
                    openjoc_status::OPENJOC_STATUS_OK
                );
                assert_eq!(
                    openjoc_stream_decoder_get_output_info(stream, info.as_mut_ptr()),
                    openjoc_status::OPENJOC_STATUS_OK
                );
                // SAFETY: successful getter populated the initialized output structure.
                assert_eq!(
                    unsafe { info.assume_init() }.latency_samples,
                    expected,
                    "rate={rate} pull={pull}"
                );
            }
            openjoc_stream_decoder_destroy(stream);
        }
    }
}
