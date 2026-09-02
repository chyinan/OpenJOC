use openjoc_wasm::{Decoder, DecoderStatus};

const FIXTURE: &[u8] = include_bytes!("../testdata/joc.ec3");
const LIFECYCLE_FIXTURE: &[u8] = include_bytes!("../testdata/joc.lifecycle.ec3");

fn collect_pcm(decoder: &mut Decoder, output: &mut Vec<f32>) -> usize {
    let mut frames = 0;
    while let Some(frame) = decoder.receive_pcm() {
        assert_eq!(frame.sample_rate, 48_000);
        assert_eq!(frame.channel_count, 2);
        assert_eq!(frame.interleaved_f32.len(), frame.sample_count * 2);
        assert!(
            frame
                .interleaved_f32
                .iter()
                .all(|sample| sample.is_finite())
        );
        output.extend_from_slice(&frame.interleaved_f32);
        frames += 1;
    }
    frames
}

#[test]
fn public_synthetic_fixture_decodes_to_stereo_float32_pcm() {
    let mut decoder = Decoder::new().expect("stereo decoder");
    let mut pcm = Vec::new();
    let mut frames = 0;

    for chunk in FIXTURE.chunks(97) {
        let status = decoder.push_bytes(chunk);
        assert_ne!(status, DecoderStatus::Error, "{:?}", decoder.status().error);
        frames += collect_pcm(&mut decoder, &mut pcm);
    }

    loop {
        let status = decoder.flush().expect("flush synthetic fixture");
        frames += collect_pcm(&mut decoder, &mut pcm);
        if status == DecoderStatus::EndOfStream {
            break;
        }
    }

    assert!(frames > 0);
    assert_eq!(decoder.status().sample_rate, Some(48_000));
    assert_eq!(decoder.status().decoded_access_units, 8);
    assert_eq!(decoder.status().output_samples, pcm.len() as u64 / 2);
    assert_eq!(decoder.status().profile.as_deref(), Some("etsi-strict"));
    assert_eq!(decoder.status().objects, Some(1));
}

#[test]
fn lifecycle_fixture_conserves_all_programme_samples_plus_declared_tail() {
    let mut decoder = Decoder::new().expect("stereo decoder");
    for chunk in LIFECYCLE_FIXTURE.chunks(4097) {
        assert_ne!(decoder.push_bytes(chunk), DecoderStatus::Error);
        while decoder.receive_pcm().is_some() {}
    }
    loop {
        let status = decoder.flush().expect("flush lifecycle fixture");
        while decoder.receive_pcm().is_some() {}
        if status == DecoderStatus::EndOfStream {
            break;
        }
    }

    assert_eq!(decoder.status().decoded_access_units, 128);
    assert_eq!(decoder.status().output_samples, 128 * 1536 + 32);
}

#[test]
fn reset_before_malformed_input_cannot_leak_previous_pcm() {
    let mut decoder = Decoder::new().expect("stereo decoder");
    for chunk in LIFECYCLE_FIXTURE.chunks(8192) {
        assert_ne!(decoder.push_bytes(chunk), DecoderStatus::Error);
        while decoder.receive_pcm().is_some() {}
    }
    loop {
        let status = decoder.flush().expect("flush lifecycle fixture");
        while decoder.receive_pcm().is_some() {}
        if status == DecoderStatus::EndOfStream {
            break;
        }
    }
    assert!(decoder.status().output_samples > 0);

    decoder.reset();
    assert_eq!(decoder.status().output_samples, 0);
    assert_eq!(
        decoder.push_bytes(&[0x0b, 0x77]),
        DecoderStatus::NeedMoreInput
    );
    let error = decoder.flush().expect_err("truncated input must fail");
    assert_eq!(error.category, openjoc_wasm::FailureCategory::InvalidInput);
    assert_eq!(decoder.status().output_samples, 0);
}
