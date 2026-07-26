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

fn joc_emdf() -> Vec<u8> {
    let mut container = Bits::default();
    container.push(0, 2);
    container.push(0, 3);
    for (id, payload) in [(11, 0xa5), (14, 0x5a)] {
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
        container.push(1, 8);
        container.push(0, 1);
        container.push(payload, 8);
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

fn joc_frame(emdf: &[u8]) -> Vec<u8> {
    let size = 64;
    let mut bits = Bits::default();
    for (value, width) in [
        (0x0b77, 16),
        (0, 2),
        (0, 3),
        (31, 11),
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
        (2, 8),
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
    fs::write(&input, joc_frame(&joc_emdf())).expect("write input");

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
