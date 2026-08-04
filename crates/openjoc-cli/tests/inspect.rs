use openjoc_wave::{decode, encode_f64_channels};
use std::{fs, process::Command, time::SystemTime};

#[derive(Default)]
struct Bits(Vec<bool>);

impl Bits {
    fn push(&mut self, value: u64, width: u8) {
        for shift in (0..width).rev() {
            self.0.push(value & (1_u64 << shift) != 0);
        }
    }

    fn set(&mut self, position: usize, value: u64, width: u8) {
        for index in 0..usize::from(width) {
            let shift = usize::from(width) - index - 1;
            self.0[position + index] = (value >> shift) & 1 != 0;
        }
    }

    fn padded_bytes(mut self) -> Vec<u8> {
        while self.0.len() % 8 != 0 {
            self.0.push(false);
        }
        let size = self.0.len() / 8;
        self.bytes(size)
    }

    fn bytes(self, size: usize) -> Vec<u8> {
        let mut bytes = vec![0_u8; size];
        for (index, bit) in self.0.into_iter().enumerate() {
            if bit {
                bytes[index / 8] |= 0x80 >> (index % 8);
            }
        }
        bytes
    }
}

fn joc_emdf_for_profile(oamd: &[u8], joc: &[u8], vendor_compat: bool) -> Vec<u8> {
    let mut container = Bits::default();
    container.push(0, 2);
    container.push(0, 3);
    for (id, payload) in [(11, oamd), (14, joc)] {
        container.push(id, 5);
        container.push(0, 1);
        container.push(0, 1);
        container.push(1, 1);
        container.push(1, 2);
        container.push(0, 1);
        container.push(u64::from(!vendor_compat), 1);
        if !vendor_compat {
            container.push(0, 8);
        }
        container.push(0, 1);
        if vendor_compat && id == 11 {
            container.push(0, 1);
        } else {
            container.push(1, 1);
            container.push(0, 1);
            container.push(0, 1);
            container.push(0, 5);
            container.push(0, 2);
        }
        container.push(u64::try_from(payload.len()).expect("payload length"), 8);
        container.push(0, 1);
        for byte in payload {
            container.push(u64::from(*byte), 8);
        }
    }
    container.push(0, 5);
    container.push(1, 2);
    container.push(0, 2);
    container.push(0, 8);
    let container = container.padded_bytes();
    let mut emdf = vec![0x58, 0x38];
    emdf.extend_from_slice(
        &u16::try_from(container.len())
            .expect("container length")
            .to_be_bytes(),
    );
    emdf.extend_from_slice(&container);
    emdf
}

fn joc_emdf(oamd: &[u8], joc: &[u8]) -> Vec<u8> {
    joc_emdf_for_profile(oamd, joc, false)
}

fn joc_frame(emdf: &[u8], complexity: u8) -> Vec<u8> {
    let size = 128;
    let mut bits = Bits::default();
    for (value, width) in [
        (0x0b77, 16),
        (0, 2),
        (0, 3),
        (63, 11),
        (0, 2),
        (3, 2),
        (2, 3),
        (0, 1),
        (16, 5),
        (31, 5),
        (0, 1),
        (0, 1),
        (0, 1),
        (1, 1),
        (1, 6),
        (0x01, 8),
        (u64::from(complexity), 8),
    ] {
        bits.push(value, width);
    }
    bits.0.resize(size * 8, false);
    let length_position = size * 8 - 32;
    bits.set(
        length_position,
        u64::try_from(emdf.len() * 8).expect("EMDF bits"),
        14,
    );
    bits.set(size * 8 - 18, 1, 1);
    let start = length_position - emdf.len() * 8;
    for (index, byte) in emdf.iter().copied().enumerate() {
        bits.set(start + index * 8, u64::from(byte), 8);
    }
    bits.bytes(size)
}

fn absent_joc() -> Vec<u8> {
    let mut bits = Vec::new();
    push(&mut bits, 0, 3); // joc_dmx_config_idx: 5.X
    push(&mut bits, 0, 6); // object count
    push(&mut bits, 0, 3); // extension count
    push(&mut bits, 0, 3 + 5 + 10); // reserved/header fields
    push(&mut bits, 0, 1); // no matrix data
    pack(bits)
}

fn inactive_oamd() -> Vec<u8> {
    let mut bits = Vec::new();
    for (value, width) in [
        (0, 2),
        (0, 5),
        (1, 1),
        (0, 1),
        (0, 1),
        (1, 4),
        (1, 4),
        (2, 4),
        (0, 1),
        (0, 1),
        (0, 2),
        (0, 3),
        (0, 6),
        (0, 2),
        (1, 1),
        (1, 1),
        (0, 1),
        (0, 7),
    ] {
        push(&mut bits, value, width);
    }
    pack(bits)
}

fn five_channel_audio_frame(emdf: &[u8]) -> Vec<u8> {
    let size = 4096;
    let mut bits = Bits::default();
    for (value, width) in [
        (0x0b77, 16),
        (0, 2), // independent I0
        (0, 3),
        (2047, 11),
        (0, 2), // 48 kHz
        (3, 2), // six blocks
        (7, 3), // 3/2: five full-bandwidth channels
        (0, 1), // no LFE
        (16, 5),
        (31, 5),
        (0, 1),    // no compression metadata
        (0, 1),    // no mixing metadata
        (0, 1),    // no informational metadata
        (1, 1),    // addbsi exists
        (1, 6),    // two addbsi bytes
        (0x01, 8), // JOC extension flag
        (1, 8),    // complexity index: one inactive object
    ] {
        bits.push(value, width);
    }

    bits.push(1, 1); // per-block exponent strategies
    bits.push(0, 1); // no AHT syntax
    bits.push(0, 2); // frame SNR strategy
    bits.push(0, 1); // no transient processing
    bits.push(0, 7); // all optional frame syntax disabled
    bits.push(0, 1); // coupling strategy absent in block 0
    for _ in 1..6 {
        bits.push(0, 1); // coupling strategy absent
    }
    for block in 0..6 {
        for _ in 0..5 {
            bits.push(u64::from(block == 0), 2); // D15, then reuse
        }
    }
    for _ in 0..5 {
        bits.push(0, 5); // converter exponent strategy
    }
    bits.push(0, 6); // frame coarse SNR
    bits.push(0, 4); // frame fine SNR
    bits.push(0, 1); // no block-start information

    // First audio block: all BAPs become zero because the frame SNR offsets are
    // zero. The exponents remain valid and provide the 73-bin channel extent.
    bits.push(0, 1); // dynamic range absent
    bits.push(0, 1); // no SPX
    for _ in 0..5 {
        bits.push(0, 6); // channel bandwidth code
    }
    for _ in 0..5 {
        bits.push(0, 4); // initial exponent
        for _ in 0..24 {
            bits.push(62, 7); // zero D15 exponent deltas
        }
        bits.push(0, 2); // gain range
    }
    bits.push(0, 1); // converter SNR offset absent

    // Following blocks reuse exponents and all optional state. They contain no
    // mantissa words because the zero-SNR special case keeps every BAP zero.
    for _ in 1..6 {
        bits.push(0, 1); // dynamic range absent
        bits.push(0, 1); // SPX strategy reused
        bits.push(0, 1); // converter SNR offset absent
    }

    bits.0.resize(size * 8, false);
    let auxdatae_position = size * 8 - 18;
    let length_position = auxdatae_position - 14;
    bits.set(
        length_position,
        u64::try_from(emdf.len() * 8).expect("EMDF bit length"),
        14,
    );
    bits.set(auxdatae_position, 1, 1);
    let start = length_position - emdf.len() * 8;
    for (index, byte) in emdf.iter().copied().enumerate() {
        bits.set(start + index * 8, u64::from(byte), 8);
    }
    bits.bytes(size)
}

#[test]
fn inspect_command_reports_timing_profile_payloads_and_complexity() {
    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("openjoc-inspect-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&root).expect("test directory");
    let input = root.join("profile.ec3");
    fs::write(&input, joc_frame(&joc_emdf(&[0xa5], &[0x5a]), 2)).expect("write input");

    let result = Command::new(env!("CARGO_BIN_EXE_openjoc"))
        .args(["inspect", input.to_str().expect("input path")])
        .output()
        .expect("run openjoc");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).expect("UTF-8 output");
    assert!(output.contains("frames: 1"));
    assert!(output.contains("access units: 1"));
    assert!(output.contains("sample rate: 48000 Hz"));
    assert!(output.contains("samples: 1536"));
    assert!(output.contains("carrier frame: 0"));
    assert!(output.contains("complexity index: 2"));
    assert!(output.contains("OAMD bytes: 1"));
    assert!(output.contains("JOC bytes: 1"));
    assert!(
        output
            .contains("audio-block skipfld: 0 observed in 0 reached prefixes; 6 blocks unresolved")
    );
    assert!(!output.contains("Open the objects"));
    assert!(!output.contains("\x1b["));

    fs::remove_dir_all(&root).expect("remove test directory");
}

#[test]
fn inspect_distinguishes_normative_failure_from_vendor_compatibility() {
    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "openjoc-inspect-vendor-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&root).expect("test directory");
    let input = root.join("vendor-profile.ec3");
    fs::write(
        &input,
        joc_frame(&joc_emdf_for_profile(&[0xa5], &[0x5a], true), 2),
    )
    .expect("write input");

    let result = Command::new(env!("CARGO_BIN_EXE_openjoc"))
        .args(["inspect", input.to_str().expect("input path")])
        .output()
        .expect("run openjoc");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).expect("UTF-8 output");
    assert!(output.contains("profile: ETSI_STRICT"));
    assert!(output.contains("result: failed"));
    assert!(output.contains("payload 11 codecdatae=0 where ETSI requires 1"));
    assert!(output.contains("profile: DOLBY_VENDOR_COMPAT"));
    assert!(output.contains("result: accepted_with_deviation"));
    assert!(output.contains("deviation: payload 14 codecdatae=0 expected_by_etsi=1"));

    fs::remove_dir_all(&root).expect("remove test directory");
}

#[test]
fn inspect_command_reports_non_emdf_frame_end_data_without_rejecting_it() {
    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "openjoc-inspect-non-emdf-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&root).expect("test directory");
    let input = root.join("non-emdf.ec3");
    fs::write(&input, joc_frame(&[0, 0, 0, 0], 2)).expect("write input");

    let result = Command::new(env!("CARGO_BIN_EXE_openjoc"))
        .args(["inspect", input.to_str().expect("input path")])
        .output()
        .expect("run openjoc");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).expect("UTF-8 output");
    assert!(output.contains("frame-end auxdatae: 1 present, 0 absent"));
    assert!(output.contains("frame-end EMDF: 0 parsed, 1 non-EMDF, 0 malformed"));

    fs::remove_dir_all(&root).expect("remove test directory");
}

fn push(bits: &mut Vec<bool>, value: u64, width: u8) {
    for shift in (0..width).rev() {
        bits.push(value & (1_u64 << shift) != 0);
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

#[test]
fn decode_command_aligns_ec3_metadata_with_supplied_downmix_pcm() {
    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("openjoc-decode-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&root).expect("test directory");
    let input = root.join("profile.ec3");
    let downmix = root.join("downmix.wav");
    let output = root.join("output");
    let oamd = inactive_oamd();
    let joc = absent_joc();
    fs::write(&input, joc_frame(&joc_emdf(&oamd, &joc), 1)).expect("write input");
    fs::write(
        &downmix,
        encode_f64_channels(48_000, &vec![vec![1.0; 1536]; 5]).expect("downmix WAV"),
    )
    .expect("write downmix");

    let result = Command::new(env!("CARGO_BIN_EXE_openjoc"))
        .args([
            "decode",
            input.to_str().expect("input path"),
            "--downmix",
            downmix.to_str().expect("downmix path"),
            "-o",
            output.to_str().expect("output path"),
        ])
        .output()
        .expect("run openjoc");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stdout.is_empty());
    assert!(output.join("scene.json").is_file());
    assert!(output.join("metadata/timeline.json").is_file());
    assert!(output.join("debug/frame_000/joc.txt").is_file());
    let stem = decode(&fs::read(output.join("objects/object_000.wav")).expect("stem"))
        .expect("decode stem");
    assert_eq!(stem.sample_rate, 48_000);
    assert_eq!(stem.channels, vec![vec![0.0; 1536]]);

    fs::remove_dir_all(&root).expect("remove test directory");
}

#[test]
fn decode_requires_explicit_vendor_profile_and_writes_deviation_evidence() {
    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "openjoc-vendor-decode-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&root).expect("test directory");
    let input = root.join("vendor-profile.ec3");
    let downmix = root.join("downmix.wav");
    let strict_output = root.join("strict-output");
    let compat_output = root.join("compat-output");
    let oamd = inactive_oamd();
    let joc = absent_joc();
    fs::write(
        &input,
        joc_frame(&joc_emdf_for_profile(&oamd, &joc, true), 1),
    )
    .expect("write input");
    fs::write(
        &downmix,
        encode_f64_channels(48_000, &vec![vec![1.0; 1536]; 5]).expect("downmix WAV"),
    )
    .expect("write downmix");

    let strict = Command::new(env!("CARGO_BIN_EXE_openjoc"))
        .args([
            "decode",
            input.to_str().expect("input path"),
            "--downmix",
            downmix.to_str().expect("downmix path"),
            "-o",
            strict_output.to_str().expect("output path"),
        ])
        .output()
        .expect("run strict decode");
    assert!(!strict.status.success());
    assert!(String::from_utf8_lossy(&strict.stderr).contains("ETSI_STRICT validation failed"));

    let compatible = Command::new(env!("CARGO_BIN_EXE_openjoc"))
        .args([
            "decode",
            input.to_str().expect("input path"),
            "--downmix",
            downmix.to_str().expect("downmix path"),
            "--validation-profile",
            "dolby-vendor-compat",
            "-o",
            compat_output.to_str().expect("output path"),
        ])
        .output()
        .expect("run compatible decode");
    assert!(
        compatible.status.success(),
        "{}",
        String::from_utf8_lossy(&compatible.stderr)
    );
    let validation_path = compat_output.join("debug/frame_000/profile_validation.json");
    let validation: serde_json::Value =
        serde_json::from_slice(&fs::read(&validation_path).expect("validation report"))
            .expect("validation JSON");
    assert_eq!(validation["profile"], "DOLBY_VENDOR_COMPAT");
    assert_eq!(validation["result"], "accepted_with_deviation");
    assert_eq!(
        validation["deviations"]
            .as_array()
            .expect("deviation array")
            .len(),
        7
    );
    assert!(compat_output.join("debug/frame_000/emdf.txt").is_file());
    assert!(compat_output.join("scene.json").is_file());

    fs::remove_dir_all(&root).expect("remove test directory");
}

#[test]
fn decode_command_internal_base_reaches_object_scene_from_raw_eac3() {
    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "openjoc-internal-decode-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&root).expect("test directory");
    let input = root.join("profile.ec3");
    let output = root.join("output");
    let oamd = inactive_oamd();
    let joc = absent_joc();
    let emdf = joc_emdf(&oamd, &joc);
    let stream = five_channel_audio_frame(&emdf);
    fs::write(&input, stream).expect("write E-AC-3");

    let result = Command::new(env!("CARGO_BIN_EXE_openjoc"))
        .args([
            "decode",
            input.to_str().expect("input path"),
            "--internal-base",
            "-o",
            output.to_str().expect("output path"),
        ])
        .output()
        .expect("run openjoc");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stdout.is_empty());
    assert!(output.join("scene.json").is_file());
    assert!(output.join("metadata/timeline.json").is_file());
    assert!(output.join("debug/frame_000/reconstruction.txt").is_file());
    let stem = decode(&fs::read(output.join("objects/object_000.wav")).expect("stem"))
        .expect("decode reconstructed stem");
    assert_eq!(stem.sample_rate, 48_000);
    assert_eq!(stem.channels, vec![vec![0.0; 1536]]);

    fs::remove_dir_all(&root).expect("remove test directory");
}
