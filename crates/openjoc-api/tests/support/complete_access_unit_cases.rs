// Test-only complete, syntactically valid AU cases shared by both adapters.
use openjoc_eac3::{group_access_units, index_syncframes, parse_audio_frame};

pub struct Case {
    name: String,
    bytes: Vec<u8>,
    ffmpeg: &'static str,
    gstreamer: &'static str,
}

fn add_stream(cases: &mut Vec<Case>, name: &str, bytes: &[u8], classification: &'static str) {
    let frames = index_syncframes(bytes).unwrap();
    for (index, unit) in group_access_units(&frames).unwrap().iter().enumerate() {
        let first = &frames[unit.first_frame];
        let last = &frames[unit.first_frame + unit.frame_count - 1];
        cases.push(Case {
            name: format!("{name}-{index}"),
            bytes: bytes[first.offset..last.offset + last.header.frame_size].to_vec(),
            ffmpeg: classification,
            gstreamer: classification,
        });
    }
}

#[derive(Clone, Copy)]
enum MetadataConfig {
    SharedGroupTwo,
    ObservedVendor,
    DifferentGroups,
    JocOnly,
    OamdOnly,
}

fn push(bits: &mut Vec<bool>, value: u64, width: usize) {
    for shift in (0..width).rev() {
        bits.push(value & (1 << shift) != 0);
    }
}

fn pack(mut bits: Vec<bool>) -> Vec<u8> {
    while bits.len() % 8 != 0 {
        bits.push(false);
    }
    let mut bytes = vec![0; bits.len() / 8];
    for (index, bit) in bits.into_iter().enumerate() {
        if bit {
            bytes[index / 8] |= 0x80 >> (index % 8);
        }
    }
    bytes
}

fn configured_metadata(frame: &[u8], config: MetadataConfig) -> Vec<u8> {
    let parsed = openjoc_eac3::extract_aux_emdf(frame).unwrap().unwrap();
    let mut bits = Vec::new();
    push(&mut bits, 0, 2); // EMDF version zero
    push(&mut bits, 0, 3); // key identifier zero
    for payload in &parsed.container.payloads {
        if matches!(config, MetadataConfig::JocOnly) && payload.id != 14
            || matches!(config, MetadataConfig::OamdOnly) && payload.id != 11
        {
            continue;
        }
        let group = if matches!(config, MetadataConfig::SharedGroupTwo)
            || matches!(config, MetadataConfig::DifferentGroups) && payload.id == 14
        {
            2
        } else {
            1
        };
        let vendor = matches!(config, MetadataConfig::ObservedVendor);
        let aligned = !vendor || payload.id == 14;
        push(&mut bits, payload.id, 5);
        push(&mut bits, 0, 1); // no sample offset
        push(&mut bits, 0, 1); // no duration
        push(&mut bits, 1, 1); // group present
        push(&mut bits, group, 2);
        push(&mut bits, 0, 1); // no group continuation
        push(&mut bits, u64::from(!vendor), 1); // codec data present
        if !vendor {
            push(&mut bits, 0, 8); // normative reserved codec byte
        }
        push(&mut bits, 0, 1); // retain known payloads
        push(&mut bits, u64::from(aligned), 1);
        if aligned {
            push(&mut bits, 0, 1); // create_duplicate
            push(&mut bits, 0, 1); // remove_duplicate
            push(&mut bits, 0, 5); // priority
            push(&mut bits, 0, 2); // processing_allowed
        }
        push(&mut bits, u64::try_from(payload.data.len()).unwrap(), 8);
        push(&mut bits, 0, 1); // no size continuation
        for &byte in &payload.data {
            push(&mut bits, u64::from(byte), 8);
        }
    }
    push(&mut bits, 0, 5); // end of payload list
    push(&mut bits, 1, 2); // one primary protection byte
    push(&mut bits, 0, 2); // no secondary protection
    push(&mut bits, 0, 8);
    let body = pack(bits);
    let mut emdf = vec![0x58, 0x38];
    emdf.extend_from_slice(&u16::try_from(body.len()).unwrap().to_be_bytes());
    emdf.extend_from_slice(&body);

    // This admitted synthetic frame's auxdata starts/ends on byte boundaries.
    // Replace only the complete auxiliary container and its length, not audio.
    let old = openjoc_eac3::extract_auxdata(frame).unwrap().unwrap();
    assert_eq!(old.bit_len, old.bytes.len() * 8);
    let end = frame.len() - 4;
    let mut bytes = frame.to_vec();
    bytes[end - old.bytes.len()..end].fill(0);
    bytes[end - emdf.len()..end].copy_from_slice(&emdf);
    let length = u16::try_from(emdf.len() * 8).unwrap();
    bytes[end] = u8::try_from(length >> 6).unwrap();
    bytes[end + 1] = u8::try_from((length & 63) << 2).unwrap() | (bytes[end + 1] & 3);
    // Every variant remains a complete legal container; policy rejection is
    // separate from parser validity, and no OAMD payload is decoded here.
    openjoc_eac3::extract_aux_emdf(&bytes).unwrap().unwrap();
    bytes
}

fn cases() -> Vec<Case> {
    let mut result = Vec::new();
    for (name, bytes, classification) in [
        (
            "ordinary",
            include_bytes!(
                "../../../openjoc-cli/tests/fixtures/container/sine-1000hz-48k-3frames.ec3"
            )
            .as_slice(),
            "ConfirmedNonJoc",
        ),
        (
            "static",
            include_bytes!("../../../openjoc-wasm/testdata/joc.ec3").as_slice(),
            "ConfirmedJoc",
        ),
        (
            "lifecycle",
            include_bytes!("../../../openjoc-wasm/testdata/joc.lifecycle.ec3").as_slice(),
            "ConfirmedJoc",
        ),
        (
            "legacy-flat7x",
            include_bytes!("../fixtures/admission/legacy-flat7x.ec3").as_slice(),
            "ConfirmedJoc",
        ),
        (
            "short-flat7x",
            include_bytes!("../fixtures/admission/short-flat7x.ec3").as_slice(),
            "ConfirmedJoc",
        ),
        (
            "idx2",
            include_bytes!("../fixtures/admission/idx2.ec3").as_slice(),
            "ConfirmedJoc",
        ),
        (
            "idx3",
            include_bytes!("../fixtures/admission/idx3.ec3").as_slice(),
            "ConfirmedJoc",
        ),
        (
            "idx4",
            include_bytes!("../fixtures/admission/idx4.ec3").as_slice(),
            "ConfirmedJoc",
        ),
    ] {
        add_stream(&mut result, name, bytes, classification);
    }
    let frame = &include_bytes!("../../../openjoc-wasm/testdata/joc.lifecycle.ec3")[..4096];
    for (name, config, ffmpeg, gstreamer) in [
        (
            "shared-group-two",
            MetadataConfig::SharedGroupTwo,
            "ConfirmedJoc",
            "ConfirmedJoc",
        ),
        (
            "observed-vendor",
            MetadataConfig::ObservedVendor,
            "ConfirmedJoc",
            "ConfirmedJoc",
        ),
        (
            "different-groups",
            MetadataConfig::DifferentGroups,
            "InvalidOrUnsupported",
            "ConfirmedJoc",
        ),
        (
            "joc-only",
            MetadataConfig::JocOnly,
            "InvalidOrUnsupported",
            "ConfirmedJoc",
        ),
        (
            "oamd-only",
            MetadataConfig::OamdOnly,
            "InvalidOrUnsupported",
            "InvalidOrUnsupported",
        ),
    ] {
        result.push(Case {
            name: name.to_owned(),
            bytes: configured_metadata(frame, config),
            ffmpeg,
            gstreamer,
        });
    }
    assert_eq!(result.len(), 149);
    result
}

pub fn verify(adapter: &str, classify: impl Fn(&[u8]) -> String) {
    use std::fmt::Write as _;
    let mut transcript = String::new();
    for case in cases() {
        let frames = index_syncframes(&case.bytes).unwrap();
        let units = group_access_units(&frames).unwrap();
        assert_eq!(units.len(), 1, "{}", case.name);
        assert_eq!(units[0].frame_count, frames.len());
        assert_eq!(units[0].first_frame, 0);
        for frame in &frames {
            parse_audio_frame(&case.bytes[frame.offset..]).unwrap();
        }
        let actual = classify(&case.bytes);
        let expected = match adapter {
            "ffmpeg" => case.ffmpeg,
            "gstreamer" => case.gstreamer,
            _ => panic!("unknown adapter"),
        };
        assert_eq!(actual, expected, "{}", case.name);
        writeln!(
            transcript,
            "{}:bytes={};frames={};samples={};classification={actual}",
            case.name,
            case.bytes.len(),
            frames.len(),
            units[0].samples
        )
        .unwrap();
    }
    let frozen = match adapter {
        "ffmpeg" => include_str!("../fixtures/admission/ffmpeg.txt"),
        "gstreamer" => include_str!("../fixtures/admission/gstreamer.txt"),
        _ => unreachable!(),
    };
    assert_eq!(
        transcript, frozen,
        "complete pre-refactor classification transcript"
    );
    if let Some(directory) = std::env::var_os("OPENJOC_AU_CLASSIFICATION_CAPTURE") {
        use std::io::Write as _;
        std::fs::File::create_new(std::path::Path::new(&directory).join(format!("{adapter}.txt")))
            .unwrap()
            .write_all(transcript.as_bytes())
            .unwrap();
    }
    println!(
        "{adapter}: 149 complete syntactically valid AU cases matched the frozen policy contract"
    );
}
