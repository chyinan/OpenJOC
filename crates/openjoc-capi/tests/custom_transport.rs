#![allow(unsafe_code)]
#![allow(clippy::borrow_as_ptr)]

use openjoc_capi::*;
use std::{ffi::CStr, mem, ptr};

#[test]
fn custom_stream_transport_preserves_order_and_rejects_unknown_labels_early() {
    for known in [true, false] {
        let names = if known { [c"FR", c"FL"] } else { [c"A", c"B"] };
        let speakers = [0, 1].map(|index| openjoc_custom_speaker {
            struct_size: mem::size_of::<openjoc_custom_speaker>() as u32,
            name: names[index].as_ptr(),
            azimuth: if index == 0 { 45.0 } else { -45.0 },
            elevation: 0.0,
            role: openjoc_speaker_role::OPENJOC_SPEAKER_FULL_RANGE as u32,
        });
        let layout = openjoc_custom_speaker_layout {
            struct_size: mem::size_of::<openjoc_custom_speaker_layout>() as u32,
            version: 1,
            name: c"studio".as_ptr(),
            speakers: speakers.as_ptr(),
            speaker_count: speakers.len(),
        };
        let mut backing = mem::MaybeUninit::<openjoc_decoder_config>::uninit();
        assert_eq!(
            openjoc_decoder_config_init_v1_7(backing.as_mut_ptr()),
            openjoc_status::OPENJOC_STATUS_OK
        );
        // SAFETY: the current-version initializer populated the entire config.
        let mut config = unsafe { backing.assume_init() };
        config.custom_speaker_layout = &layout;
        let mut direct = ptr::null_mut();
        assert_eq!(
            openjoc_decoder_create(&config, &mut direct),
            openjoc_status::OPENJOC_STATUS_OK
        );
        openjoc_decoder_destroy(direct);
        let mut stream = ptr::null_mut();
        let status = openjoc_stream_decoder_create(&config, &mut stream);
        if known {
            assert_eq!(status, openjoc_status::OPENJOC_STATUS_OK);
            for (index, name) in names.iter().enumerate() {
                // SAFETY: successful construction owns stable non-null labels.
                let label = unsafe {
                    CStr::from_ptr(openjoc_stream_decoder_get_channel_label(stream, index))
                };
                assert_eq!(label, *name);
            }
            openjoc_stream_decoder_destroy(stream);
        } else {
            assert_eq!(status, openjoc_status::OPENJOC_STATUS_INVALID_ARGUMENT);
            assert!(stream.is_null());
        }
    }
}
