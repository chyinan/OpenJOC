// pattern: Functional Core
// Project-owned synthetic bitstreams, shared syntax with existing codec tests.
#[derive(Default)]
struct Bits(Vec<bool>);

/// BSI/aux fixture with configurable programme ownership and channel map.
pub fn topology_frame(
    kind: u8,
    id: u8,
    acmod: u8,
    lfe: bool,
    map: Option<u16>,
    emdf: Option<&[u8]>,
) -> Vec<u8> {
    let mut bits = Bits::default();
    for (value, width) in [
        (0x0b77, 16),
        (u64::from(kind), 2),
        (u64::from(id), 3),
        (127, 11),
        (0, 2),
        (3, 2),
        (u64::from(acmod), 3),
        (u64::from(lfe), 1),
        (16, 5),
        (31, 5),
        (0, 1),
    ] {
        bits.push(value, width);
    }
    if kind == 1 {
        bits.push(u64::from(map.is_some()), 1);
        if let Some(map) = map {
            bits.push(u64::from(map), 16);
        }
    }
    bits.push(0, 1);
    bits.push(0, 1);
    bits.push(u64::from(emdf.is_some()), 1);
    if emdf.is_some() {
        bits.push(1, 6);
        bits.push(1, 8);
        bits.push(1, 8);
    }
    bits.0.resize(256 * 8, false);
    if let Some(emdf) = emdf {
        let length = 256 * 8 - 32;
        bits.set(length, (emdf.len() * 8) as u64, 14);
        bits.set(256 * 8 - 18, 1, 1);
        for (i, b) in emdf.iter().enumerate() {
            bits.set(length - emdf.len() * 8 + i * 8, u64::from(*b), 8);
        }
    }
    bits.bytes(256)
}

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

pub fn joc_emdf_for_profile(oamd: &[u8], joc: &[u8], vendor_compat: bool) -> Vec<u8> {
    emdf_payloads(&[(11, oamd), (14, joc)], vendor_compat)
}

pub fn emdf_payloads(payloads: &[(u64, &[u8])], vendor_compat: bool) -> Vec<u8> {
    let mut container = Bits::default();
    container.push(0, 2);
    container.push(0, 3);
    for &(id, payload) in payloads {
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

pub fn joc_emdf(oamd: &[u8], joc: &[u8]) -> Vec<u8> {
    joc_emdf_for_profile(oamd, joc, false)
}

pub fn joc_frame(emdf: &[u8], complexity: u8) -> Vec<u8> {
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

pub fn absent_joc() -> Vec<u8> {
    let mut bits = Vec::new();
    push(&mut bits, 0, 3); // joc_dmx_config_idx: 5.X
    push(&mut bits, 0, 6); // object count
    push(&mut bits, 0, 3); // extension count
    push(&mut bits, 0, 3 + 5 + 10); // reserved/header fields
    push(&mut bits, 0, 1); // no matrix data
    pack(bits)
}

pub fn inactive_oamd() -> Vec<u8> {
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

pub fn active_object_oamd(active: bool) -> Vec<u8> {
    let mut bits = Vec::new();
    push(&mut bits, 0, 2); // syntax version
    push(&mut bits, 0, 5); // one object
    push(&mut bits, 1, 1); // dynamic-only program
    push(&mut bits, 0, 1); // no LFE
    push(&mut bits, 0, 1); // no alternate object data
    push(&mut bits, 1, 4); // one element
    let mut content = vec![false]; // discard unknown false
    push(&mut content, 0, 2); // sample offset 0
    push(&mut content, 0, 3); // one update block
    push(&mut content, 0, 6); // block offset 0
    push(&mut content, 0, 2); // no ramp
    push(&mut content, 1, 1); // reserved data absent
    if active {
        push(&mut content, 1, 1); // object active
        push(&mut content, 2, 2); // gain index
        push(&mut content, 20, 6); // gain bits
        push(&mut content, 0, 1); // explicit priority
        push(&mut content, 16, 5); // standard position X
        push(&mut content, 31, 6); // standard position Y
        push(&mut content, 1, 1); // positive Z
        push(&mut content, 15, 4); // standard position Z
        push(&mut content, 0, 1); // no distance
        push(&mut content, 0, 3); // side zone excluded
        push(&mut content, 0, 1); // elevation excluded
        push(&mut content, 0, 2); // independent size
        push(&mut content, 0, 1); // no screen anchor
        push(&mut content, 0, 1); // no additional table data
    } else {
        push(&mut content, 0, 1); // object inactive
        push(&mut content, 0, 1); // no additional table data
    }
    push_element(&mut bits, 1, 10, &content);
    pack(bits)
}

pub fn five_channel_audio_frame(emdf: &[u8]) -> Vec<u8> {
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
        (1, 8),    // complexity index: one object
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

pub fn push(bits: &mut Vec<bool>, value: u64, width: u8) {
    for shift in (0..width).rev() {
        bits.push(value & (1_u64 << shift) != 0);
    }
}

pub fn pack(mut bits: Vec<bool>) -> Vec<u8> {
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

pub fn short_mono_frame_with_options(
    stream_type: u8,
    blocks: u8,
    convsync: bool,
    channel_map: Option<u16>,
    addbsi: Option<&[u8]>,
    emdf: Option<&[u8]>,
) -> Vec<u8> {
    let num_blocks_code = match blocks {
        1 => 0,
        2 => 1,
        3 => 2,
        _ => panic!("short fixture block count"),
    };
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(u64::from(stream_type), 2);
    bits.push(0, 3);
    bits.push(255, 11); // 512-byte frame
    bits.push(0, 2); // 48 kHz
    bits.push(num_blocks_code, 2);
    bits.push(1, 3); // mono
    bits.push(0, 1); // no LFE
    bits.push(16, 5); // E-AC-3 version
    bits.push(31, 5); // dialnorm
    bits.push(0, 1); // no compression word
    if stream_type == 1 {
        bits.push(u64::from(channel_map.is_some()), 1);
        if let Some(channel_map) = channel_map {
            bits.push(u64::from(channel_map), 16);
        }
    }
    bits.push(0, 1); // no mixing metadata
    bits.push(0, 1); // no informational metadata
    if stream_type == 0 {
        bits.push(u64::from(convsync), 1);
    }
    if let Some(addbsi) = addbsi {
        assert!((1..=64).contains(&addbsi.len()));
        bits.push(1, 1);
        bits.push(u64::try_from(addbsi.len() - 1).expect("addbsi length"), 6);
        for byte in addbsi {
            bits.push(u64::from(*byte), 8);
        }
    } else {
        bits.push(0, 1); // no addbsi
    }
    bits.push(0, 2); // frame SNR strategy
    bits.push(0, 1); // no transient processing
    bits.push(0, 7); // compact syntax flags
    for block in 0..blocks {
        bits.push(u64::from(block == 0), 2); // D15 then reuse
    }
    if stream_type == 0 {
        bits.push(0, 1); // converter exponent strategy absent
    }
    bits.push(0, 6); // frame coarse SNR offset
    bits.push(0, 4); // frame fine SNR offset
    if blocks > 1 {
        bits.push(0, 1); // no block-start information
    }
    bits.push(0, 1); // dynamic range absent
    bits.push(0, 1); // SPX not in use
    bits.push(0, 6); // channel bandwidth code
    bits.push(15, 4); // channel absolute exponent
    for _ in 0..24 {
        bits.push(62, 7); // zero-allocated mantissas
    }
    bits.push(0, 2); // gain range
    bits.push(0, 1); // converter SNR offset absent
    for _ in 1..blocks {
        bits.push(0, 1); // dynamic range absent
        bits.push(0, 1); // SPX strategy reused
        bits.push(0, 1); // converter SNR offset absent
    }
    if let Some(emdf) = emdf {
        let frame_bits = 512 * 8;
        let length_position = frame_bits - 32;
        bits.0.resize(frame_bits, false);
        bits.set(
            length_position,
            u64::try_from(emdf.len() * 8).expect("EMDF bits"),
            14,
        );
        bits.set(frame_bits - 18, 1, 1);
        let start = length_position - emdf.len() * 8;
        for (index, byte) in emdf.iter().copied().enumerate() {
            bits.set(start + index * 8, u64::from(byte), 8);
        }
    }
    bits.bytes(512)
}

pub fn skip_field_joc_frame(emdf: &[u8]) -> Vec<u8> {
    let mut bytes = skip_field_joc_frame_single(emdf, true);
    for _ in 1..6 {
        bytes.extend(skip_field_joc_frame_single(&[], false));
    }
    bytes
}

fn skip_field_joc_frame_single(emdf: &[u8], convsync: bool) -> Vec<u8> {
    let size = 512;
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 2); // independent
    bits.push(0, 3);
    bits.push(u64::try_from(size / 2 - 1).expect("frame words"), 11);
    bits.push(0, 2); // 48 kHz
    bits.push(0, 2); // one block
    bits.push(1, 3); // mono
    bits.push(0, 1); // no LFE
    bits.push(16, 5);
    bits.push(31, 5);
    bits.push(0, 1); // no compression metadata
    bits.push(0, 1); // no mixing metadata
    bits.push(0, 1); // no informational metadata
    bits.push(u64::from(convsync), 1); // convsync
    bits.push(1, 1); // addbsie
    bits.push(1, 6); // two addbsi bytes
    bits.push(0x01, 8); // JOC extension flag
    bits.push(1, 8); // complexity index

    bits.push(0, 2); // frame SNR strategy
    bits.push(0, 1); // transient processing
    bits.push(2, 7); // skip-field syntax only
    bits.push(1, 2); // channel D15
    bits.push(0, 1); // converter exponent strategy absent
    bits.push(0, 6); // frame coarse SNR offset
    bits.push(0, 4); // frame fine SNR offset

    bits.push(0, 1); // dynamic range absent
    bits.push(0, 1); // SPX not in use
    bits.push(0, 6); // channel bandwidth code: end mantissa 73
    bits.push(15, 4); // channel absolute exponent
    for _ in 0..24 {
        bits.push(62, 7); // neutral D15 groups
    }
    bits.push(0, 2); // gain range
    bits.push(0, 1); // converter SNR offset absent
    bits.push(1, 1); // skiple exists
    bits.push(u64::try_from(emdf.len()).expect("skip-field length"), 9);
    for byte in emdf {
        bits.push(u64::from(*byte), 8);
    }

    bits.bytes(size)
}
fn push_element(bits: &mut Vec<bool>, id: u8, size_bytes: u8, content: &[bool]) {
    push(bits, u64::from(id), 4);
    push(bits, u64::from(size_bytes - 1), 4);
    push(bits, 0, 1); // variable_bits_max continuation false
    bits.extend_from_slice(content);
    let target = usize::from(size_bytes) * 8;
    assert!(content.len() <= target);
    bits.resize(bits.len() + target - content.len(), false);
}

pub fn crc16_update(mut register: u16, input: bool) -> u16 {
    let feedback = ((register >> 15) != 0) ^ input;
    register <<= 1;
    if feedback {
        register ^= 0x8005;
    }
    register
}

pub fn crc16(bytes: &[u8]) -> u16 {
    bytes.iter().fold(0_u16, |mut register, byte| {
        for shift in (0..8).rev() {
            register = crc16_update(register, byte & (1 << shift) != 0);
        }
        register
    })
}

pub fn crc16_reverse(register: u16, input: bool) -> u16 {
    for top in [false, true] {
        let feedback = top ^ input;
        let shifted = register ^ if feedback { 0x8005 } else { 0 };
        if shifted & 1 != 0 {
            continue;
        }
        let previous = (shifted >> 1) | (u16::from(top) << 15);
        if crc16_update(previous, input) == register {
            return previous;
        }
    }
    panic!("CRC reverse transition must exist");
}

pub fn crc16_reverse_bytes(bytes: &[u8], mut target: u16) -> u16 {
    for byte in bytes.iter().rev() {
        for shift in 0..8 {
            target = crc16_reverse(target, byte & (1 << shift) != 0);
        }
    }
    target
}

pub fn two_bytes_for_target(target: u16) -> [u8; 2] {
    for value in 0..=u16::MAX {
        let bytes = value.to_be_bytes();
        if crc16(&bytes) == target {
            return bytes;
        }
    }
    panic!("two-byte CRC preimage must exist");
}

pub fn finalize_ac3_crc(frame: &mut [u8]) {
    let frame_words = frame.len() / 2;
    let five_eighth_bytes = ((frame_words >> 1) + (frame_words >> 3)) * 2;
    frame[2] = 0;
    frame[3] = 0;
    let required_after_crc1 = crc16_reverse_bytes(&frame[4..five_eighth_bytes], 0);
    frame[2..4].copy_from_slice(&two_bytes_for_target(required_after_crc1));
    assert_eq!(crc16(&frame[2..five_eighth_bytes]), 0);

    let end = frame.len();
    frame[end - 2] = 0;
    frame[end - 1] = 0;
    let prefix_state = frame[2..end - 2].iter().fold(0_u16, |mut register, byte| {
        for shift in (0..8).rev() {
            register = crc16_update(register, byte & (1 << shift) != 0);
        }
        register
    });
    let crc2 = (0..=u16::MAX)
        .find(|value| {
            value
                .to_be_bytes()
                .iter()
                .fold(prefix_state, |mut register, byte| {
                    for shift in (0..8).rev() {
                        register = crc16_update(register, byte & (1 << shift) != 0);
                    }
                    register
                })
                == 0
        })
        .expect("CRC2 preimage");
    frame[end - 2..].copy_from_slice(&crc2.to_be_bytes());
    assert_eq!(crc16(&frame[2..]), 0);
}

pub fn decodable_ac3_frame_for(
    acmod: u8,
    lfe_on: bool,
    dynamic_range: Option<u8>,
    delta_reuse: bool,
) -> Vec<u8> {
    const SIZE: usize = 2560;
    let channels = [2_usize, 1, 2, 3, 3, 4, 4, 5][usize::from(acmod)];
    let exponent_codes = [62_u8, 82, 86, 102, 106];
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 16); // patched crc1
    bits.push(0, 2); // 48 kHz
    bits.push(36, 6); // 640 kbit/s, 2,560 bytes
    bits.push(8, 5);
    bits.push(0, 3); // complete main
    bits.push(u64::from(acmod), 3);
    if acmod & 1 != 0 && acmod != 1 {
        bits.push(0, 2); // centre -3 dB
    }
    if acmod & 4 != 0 {
        bits.push(0, 2); // surround -3 dB
    }
    if acmod == 2 {
        bits.push(0, 2); // matrix-surround mode not indicated
    }
    bits.push(u64::from(lfe_on), 1);
    bits.push(31, 5); // dialnorm
    bits.push(0, 1); // compr absent
    bits.push(0, 1); // language absent
    bits.push(0, 1); // production info absent
    bits.push(0, 1); // copyright
    bits.push(1, 1); // original bitstream
    bits.push(0, 1); // timecode 1 absent
    bits.push(0, 1); // timecode 2 absent
    bits.push(0, 1); // addbsi absent

    for block in 0..6 {
        for channel in 0..channels {
            bits.push(u64::from(block == 0 && channel == 0), 1);
        }
        for _ in 0..channels {
            bits.push(1, 1); // dither enabled
        }
        bits.push(u64::from(block == 0 && dynamic_range.is_some()), 1);
        if block == 0 {
            if let Some(code) = dynamic_range {
                bits.push(u64::from(code), 8);
            }
        }
        bits.push(u64::from(block == 0), 1); // coupling strategy
        if block == 0 {
            bits.push(0, 1); // coupling off
        }
        if acmod == 2 {
            bits.push(u64::from(block == 0), 1);
            if block == 0 {
                for _ in 0..4 {
                    bits.push(0, 1);
                }
            }
        }
        for _ in 0..channels {
            bits.push(u64::from(block == 0), 2); // D15 then reuse
        }
        if lfe_on {
            bits.push(u64::from(block == 0), 1);
        }
        if block == 0 {
            for _ in 0..channels {
                bits.push(0, 6); // chbwcod
            }
            for &code in &exponent_codes[..channels] {
                bits.push(15, 4);
                for _ in 0..24 {
                    bits.push(u64::from(code), 7);
                }
                bits.push(0, 2);
            }
            if lfe_on {
                bits.push(0, 4);
                bits.push(62, 7);
                bits.push(62, 7);
            }
        }
        bits.push(u64::from(block == 0), 1); // bit allocation info
        if block == 0 {
            bits.push(2, 2);
            bits.push(1, 2);
            bits.push(1, 2);
            bits.push(2, 2);
            bits.push(7, 3);
        }
        bits.push(u64::from(block == 0), 1); // SNR offsets
        if block == 0 {
            bits.push(5, 6);
            for _ in 0..channels {
                bits.push(0, 4);
                bits.push(4, 3);
            }
            if lfe_on {
                bits.push(15, 4);
                bits.push(4, 3);
            }
        }
        if delta_reuse && block <= 1 {
            bits.push(1, 1);
            for _ in 0..channels {
                bits.push(if block == 0 { 2 } else { 0 }, 2);
            }
        } else {
            bits.push(0, 1);
        }
        bits.push(0, 1); // skip field absent
        if lfe_on {
            for group in 0..3 {
                bits.push(u64::try_from((block * 3 + group) % 27).unwrap(), 5);
            }
        }
    }
    let mut frame = bits.bytes(SIZE);
    finalize_ac3_crc(&mut frame);
    frame
}

pub fn cmaf_bmff_box(kind: [u8; 4], payload: &[u8]) -> Vec<u8> {
    let size = u32::try_from(payload.len() + 8).expect("BMFF box size");
    let mut output = size.to_be_bytes().to_vec();
    output.extend_from_slice(&kind);
    output.extend_from_slice(payload);
    output
}

pub fn cmaf_bmff_full_box(kind: [u8; 4], flags: u32, payload: &[u8]) -> Vec<u8> {
    let mut full = flags.to_be_bytes().to_vec();
    full.extend_from_slice(payload);
    cmaf_bmff_box(kind, &full)
}

pub fn cmaf_dec3_box() -> Vec<u8> {
    let mut bits = Bits::default();
    for (value, width) in [
        (768, 13), // data_rate
        (0, 3),    // num_ind_sub: one independent substream
        (0, 2),    // fscod: 48 kHz
        (16, 5),   // bsid
        (0, 1),    // reserved
        (0, 1),    // asvc
        (0, 3),    // bsmod
        (7, 3),    // acmod
        (1, 1),    // lfeon
        (0, 3),    // reserved
        (1, 4),    // num_dep_sub: D0
        (2, 9),    // chan_loc: Lrs/Rrs
        (0, 7),    // reserved
        (1, 1),    // flag_ec3_extension_type_a
        (16, 8),   // complexity_index_type_a
    ] {
        bits.push(value, width);
    }
    let payload = bits.padded_bytes();
    let size = u32::try_from(payload.len() + 8).expect("dec3 size");
    [size.to_be_bytes().as_slice(), b"dec3", payload.as_slice()].concat()
}

pub fn cmaf_init_segment_for_e2e(dec3: &[u8]) -> Vec<u8> {
    cmaf_init_segment_for_e2e_with_entry(Some(dec3))
}

pub fn cmaf_audio_sample_entry(dec3: Option<&[u8]>) -> Vec<u8> {
    let mut payload = vec![0_u8; 6];
    payload.extend_from_slice(&1_u16.to_be_bytes());
    payload.extend_from_slice(&[0_u8; 8]);
    payload.extend_from_slice(&2_u16.to_be_bytes());
    payload.extend_from_slice(&16_u16.to_be_bytes());
    payload.extend_from_slice(&[0_u8; 4]);
    payload.extend_from_slice(&(48_000_u32 << 16).to_be_bytes());
    if let Some(dec3) = dec3 {
        payload.extend_from_slice(dec3);
    }
    cmaf_bmff_box(*b"ec-3", &payload)
}

pub fn cmaf_init_segment_for_e2e_with_entry(dec3: Option<&[u8]>) -> Vec<u8> {
    let mut ftyp = Vec::new();
    ftyp.extend_from_slice(b"cmfc");
    ftyp.extend_from_slice(&0_u32.to_be_bytes());
    for brand in [b"isom", b"iso6", b"cmfc", b"ceao"] {
        ftyp.extend_from_slice(brand);
    }

    let mut movie_fields = vec![0_u8; 96];
    movie_fields[8..12].copy_from_slice(&48_000_u32.to_be_bytes());
    let movie_header_box = cmaf_bmff_full_box(*b"mvhd", 0, &movie_fields);
    let mut track_fields = vec![0_u8; 80];
    track_fields[8..12].copy_from_slice(&1_u32.to_be_bytes());
    let tkhd = cmaf_bmff_full_box(*b"tkhd", 7, &track_fields);
    let mut media_fields = vec![0_u8; 20];
    media_fields[8..12].copy_from_slice(&48_000_u32.to_be_bytes());
    let media_header_box = cmaf_bmff_full_box(*b"mdhd", 0, &media_fields);
    let mut handler_fields = vec![0_u8; 20];
    handler_fields[4..8].copy_from_slice(b"soun");
    handler_fields.extend_from_slice(b"SoundHandler\0");
    let hdlr = cmaf_bmff_full_box(*b"hdlr", 0, &handler_fields);
    let smhd = cmaf_bmff_full_box(*b"smhd", 0, &[0_u8; 4]);
    let url = cmaf_bmff_full_box(*b"url ", 1, &[]);
    let dref = cmaf_bmff_full_box(
        *b"dref",
        0,
        &[1_u32.to_be_bytes().as_slice(), &url].concat(),
    );
    let dinf = cmaf_bmff_box(*b"dinf", &dref);
    let entry = cmaf_audio_sample_entry(dec3);
    let stsd = cmaf_bmff_full_box(
        *b"stsd",
        0,
        &[1_u32.to_be_bytes().as_slice(), &entry].concat(),
    );
    let stbl = cmaf_bmff_box(*b"stbl", &stsd);
    let minf = cmaf_bmff_box(*b"minf", &[smhd, dinf, stbl].concat());
    let mdia = cmaf_bmff_box(*b"mdia", &[media_header_box, hdlr, minf].concat());
    let trak = cmaf_bmff_box(*b"trak", &[tkhd, mdia].concat());
    let trex_fields = [
        1_u32.to_be_bytes().as_slice(),
        1_u32.to_be_bytes().as_slice(),
        1536_u32.to_be_bytes().as_slice(),
        0_u32.to_be_bytes().as_slice(),
        0_u32.to_be_bytes().as_slice(),
    ]
    .concat();
    let trex = cmaf_bmff_full_box(*b"trex", 0, &trex_fields);
    let mvex = cmaf_bmff_box(*b"mvex", &trex);
    let moov = cmaf_bmff_box(*b"moov", &[movie_header_box, trak, mvex].concat());
    [cmaf_bmff_box(*b"ftyp", &ftyp), moov].concat()
}

pub fn cmaf_fragment_for_e2e(sample: &[u8], sequence: u32, decode_time: u64) -> Vec<u8> {
    let mfhd = cmaf_bmff_full_box(*b"mfhd", 0, &sequence.to_be_bytes());
    let tfhd_fields = [
        1_u32.to_be_bytes().as_slice(),
        1536_u32.to_be_bytes().as_slice(),
    ]
    .concat();
    let tfhd = cmaf_bmff_full_box(*b"tfhd", 0x0002_0008, &tfhd_fields);
    let tfdt = cmaf_bmff_full_box(*b"tfdt", 0x0100_0000, &decode_time.to_be_bytes());
    let mut trun_fields = Vec::new();
    trun_fields.extend_from_slice(&1_u32.to_be_bytes());
    trun_fields.extend_from_slice(&0_u32.to_be_bytes());
    trun_fields.extend_from_slice(&1536_u32.to_be_bytes());
    trun_fields.extend_from_slice(
        &u32::try_from(sample.len())
            .expect("sample size")
            .to_be_bytes(),
    );
    let trun = cmaf_bmff_full_box(*b"trun", 0x0000_0301, &trun_fields);
    let traf = cmaf_bmff_box(*b"traf", &[tfhd, tfdt, trun].concat());
    let moof = cmaf_bmff_box(*b"moof", &[mfhd, traf].concat());
    let data_offset = u32::try_from(moof.len() + 8).expect("data offset");
    let mut patched_trun_fields = Vec::new();
    patched_trun_fields.extend_from_slice(&1_u32.to_be_bytes());
    patched_trun_fields.extend_from_slice(&data_offset.to_be_bytes());
    patched_trun_fields.extend_from_slice(&1536_u32.to_be_bytes());
    patched_trun_fields.extend_from_slice(
        &u32::try_from(sample.len())
            .expect("sample size")
            .to_be_bytes(),
    );
    let patched_trun = cmaf_bmff_full_box(*b"trun", 0x0000_0301, &patched_trun_fields);
    let patched_tfhd_fields = [
        1_u32.to_be_bytes().as_slice(),
        1536_u32.to_be_bytes().as_slice(),
    ]
    .concat();
    let patched_tfhd = cmaf_bmff_full_box(*b"tfhd", 0x0002_0008, &patched_tfhd_fields);
    let patched_tfdt = cmaf_bmff_full_box(*b"tfdt", 0x0100_0000, &decode_time.to_be_bytes());
    let patched_traf = cmaf_bmff_box(
        *b"traf",
        &[patched_tfhd, patched_tfdt, patched_trun].concat(),
    );
    let patched_moof = cmaf_bmff_box(
        *b"moof",
        &[
            cmaf_bmff_full_box(*b"mfhd", 0, &sequence.to_be_bytes()),
            patched_traf,
        ]
        .concat(),
    );
    [patched_moof, cmaf_bmff_box(*b"mdat", sample)].concat()
}
