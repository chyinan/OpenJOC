use openjoc_wave::{decode, encode_f64_channels};
use std::{fs, process::Command, time::SystemTime};

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

fn absent_joc() -> Vec<u8> {
    let mut bits = Vec::new();
    push(&mut bits, 0, 3);
    push(&mut bits, 0, 6);
    push(&mut bits, 0, 3);
    push(&mut bits, 0, 3 + 5 + 10);
    push(&mut bits, 0, 1);
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

#[test]
fn decode_payload_command_writes_metadata_and_reconstruction_row_artifacts() {
    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("openjoc-cli-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&root).expect("test directory");
    let downmix_path = root.join("downmix.wav");
    let joc_path = root.join("joc.bin");
    let oamd_path = root.join("oamd.bin");
    let output = root.join("output");
    fs::write(
        &downmix_path,
        encode_f64_channels(48_000, &vec![vec![1.0; 64]; 5]).expect("downmix WAV"),
    )
    .expect("write downmix");
    fs::write(&joc_path, absent_joc()).expect("write JOC");
    fs::write(&oamd_path, inactive_oamd()).expect("write OAMD");

    let result = Command::new(env!("CARGO_BIN_EXE_openjoc"))
        .args([
            "decode-payload",
            "--downmix",
            downmix_path.to_str().expect("downmix path"),
            "--joc",
            joc_path.to_str().expect("JOC path"),
            "--oamd",
            oamd_path.to_str().expect("OAMD path"),
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
    assert!(output.join("debug/frame_000/oamd.txt").is_file());
    assert!(output.join("debug/frame_000/reconstruction.txt").is_file());
    let scene: serde_json::Value = serde_json::from_slice(
        &fs::read(output.join("scene.json")).expect("metadata-only scene manifest"),
    )
    .expect("scene JSON");
    assert_eq!(scene["semantic_binding"], "unresolved");
    assert_eq!(
        scene["reconstruction_basis"],
        "diagnostics/reconstruction_basis.json"
    );
    let basis: serde_json::Value = serde_json::from_slice(
        &fs::read(output.join("diagnostics/reconstruction_basis.json"))
            .expect("reconstruction-basis manifest"),
    )
    .expect("basis JSON");
    assert!(basis.to_string().contains("rows"));
    assert!(!basis.to_string().contains("object_id"));
    assert!(!output.join("object_stems").exists());
    assert!(!output.join("object_pcm").exists());
    let stem_bytes = fs::read(output.join("diagnostics/reconstruction_rows/row_000.wav"))
        .expect("reconstruction row");
    assert_eq!(
        u16::from_le_bytes(stem_bytes[20..22].try_into().unwrap()),
        3
    );
    assert_eq!(
        u16::from_le_bytes(stem_bytes[34..36].try_into().unwrap()),
        32
    );
    let stem = decode(&stem_bytes).expect("decode reconstruction row");
    assert_eq!(stem.channels, vec![vec![0.0; 64]]);

    let reference_output = root.join("reference-output");
    let reference_result = Command::new(env!("CARGO_BIN_EXE_openjoc"))
        .args([
            "decode-payload",
            "--downmix",
            downmix_path.to_str().expect("downmix path"),
            "--joc",
            joc_path.to_str().expect("JOC path"),
            "--oamd",
            oamd_path.to_str().expect("OAMD path"),
            "-o",
            reference_output.to_str().expect("output path"),
            "--reference-f64",
        ])
        .output()
        .expect("run reference output");
    assert!(
        reference_result.status.success(),
        "{}",
        String::from_utf8_lossy(&reference_result.stderr)
    );
    let reference_stem =
        fs::read(reference_output.join("diagnostics/reconstruction_rows/row_000.wav"))
            .expect("reference reconstruction row");
    assert_eq!(
        u16::from_le_bytes(reference_stem[34..36].try_into().unwrap()),
        64
    );

    fs::remove_dir_all(&root).expect("remove test directory");
}

#[test]
fn decode_payload_classifies_malformed_user_wave_as_input() {
    let nonce = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "openjoc-cli-malformed-wave-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&root).expect("test directory");
    let downmix_path = root.join("downmix.wav");
    let joc_path = root.join("joc.bin");
    let oamd_path = root.join("oamd.bin");
    let output = root.join("output");
    fs::write(&downmix_path, b"not a wave file").expect("write malformed WAV");
    fs::write(&joc_path, absent_joc()).expect("write JOC");
    fs::write(&oamd_path, inactive_oamd()).expect("write OAMD");

    let result = Command::new(env!("CARGO_BIN_EXE_openjoc"))
        .args([
            "decode-payload",
            "--downmix",
            downmix_path.to_str().expect("downmix path"),
            "--joc",
            joc_path.to_str().expect("JOC path"),
            "--oamd",
            oamd_path.to_str().expect("OAMD path"),
            "-o",
            output.to_str().expect("output path"),
        ])
        .output()
        .expect("run openjoc");

    assert!(!result.status.success());
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.starts_with("openjoc[malformed-input]:"), "{stderr}");
    assert!(stderr.contains("failed to decode input WAV"), "{stderr}");

    fs::remove_dir_all(&root).expect("remove test directory");
}
