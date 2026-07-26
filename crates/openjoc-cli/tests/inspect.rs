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

fn joc_emdf(oamd: &[u8], joc: &[u8]) -> Vec<u8> {
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
        container.push(1, 1);
        container.push(0, 8);
        container.push(0, 1);
        container.push(1, 1);
        container.push(0, 1);
        container.push(0, 1);
        container.push(0, 5);
        container.push(0, 2);
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
    assert!(output.join("scene.json").is_file());
    assert!(output.join("metadata/timeline.json").is_file());
    assert!(output.join("debug/frame_000/joc.txt").is_file());
    let stem = decode(&fs::read(output.join("objects/object_000.wav")).expect("stem"))
        .expect("decode stem");
    assert_eq!(stem.sample_rate, 48_000);
    assert_eq!(stem.channels, vec![vec![0.0; 1536]]);

    fs::remove_dir_all(&root).expect("remove test directory");
}
