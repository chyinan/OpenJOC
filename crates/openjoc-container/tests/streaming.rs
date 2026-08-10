use openjoc_container::{InputMediaError, RawEac3FrameReader};
use std::io::{self, Read};

fn push(bits: &mut Vec<bool>, value: u64, width: usize) {
    for shift in (0..width).rev() {
        bits.push(value & (1_u64 << shift) != 0);
    }
}

fn frame(stream_type: u8, substream_id: u8, size: usize) -> Vec<u8> {
    assert_eq!(size % 2, 0);
    let mut bits = Vec::new();
    push(&mut bits, 0x0b77, 16);
    push(&mut bits, u64::from(stream_type), 2);
    push(&mut bits, u64::from(substream_id), 3);
    push(
        &mut bits,
        u64::try_from(size / 2 - 1).expect("frame words"),
        11,
    );
    push(&mut bits, 0, 2); // 48 kHz
    push(&mut bits, 3, 2); // six blocks
    let mut bytes = vec![0_u8; size];
    for (index, bit) in bits.into_iter().enumerate() {
        if bit {
            bytes[index / 8] |= 0x80 >> (index % 8);
        }
    }
    bytes
}

struct Chunked<'a> {
    bytes: &'a [u8],
    offset: usize,
    chunk: usize,
}

impl Read for Chunked<'_> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if self.offset == self.bytes.len() {
            return Ok(0);
        }
        let count = self
            .chunk
            .min(output.len())
            .min(self.bytes.len() - self.offset);
        output[..count].copy_from_slice(&self.bytes[self.offset..self.offset + count]);
        self.offset += count;
        Ok(count)
    }
}

#[test]
fn raw_reader_is_identical_across_chunk_boundaries() {
    let expected = vec![frame(0, 0, 16), frame(1, 0, 18), frame(0, 0, 20)];
    let mut stream = Vec::new();
    for bytes in &expected {
        stream.extend_from_slice(bytes);
    }

    for chunk in [1, 2, 3, 7, 31, 257, 4096] {
        let mut reader = RawEac3FrameReader::new(
            Chunked {
                bytes: &stream,
                offset: 0,
                chunk,
            },
            64,
        );
        let mut actual = Vec::new();
        while let Some(frame) = reader.next_frame().expect("valid frame stream") {
            actual.push(frame);
        }
        assert_eq!(actual, expected, "chunk size {chunk}");
        assert_eq!(reader.stats().frames_emitted, expected.len());
        assert!(reader.stats().max_input_carry_bytes <= 20);
    }
}

#[test]
fn raw_reader_reports_truncated_final_frame() {
    let first = frame(0, 0, 16);
    let second = frame(0, 0, 20);
    let mut stream = first.clone();
    stream.extend_from_slice(&second[..9]);
    let mut reader = RawEac3FrameReader::new(
        Chunked {
            bytes: &stream,
            offset: 0,
            chunk: 2,
        },
        64,
    );
    assert_eq!(reader.next_frame().expect("first frame"), Some(first));
    assert!(matches!(
        reader.next_frame(),
        Err(InputMediaError::InvalidDemuxedEac3(
            openjoc_eac3::Eac3Error::TruncatedFrame { .. }
        ))
    ));
}

#[test]
fn raw_reader_accepts_eof_on_exact_frame_boundary() {
    let bytes = frame(0, 0, 16);
    let mut reader = RawEac3FrameReader::new(
        Chunked {
            bytes: &bytes,
            offset: 0,
            chunk: 1,
        },
        64,
    );
    assert_eq!(reader.next_frame().expect("frame"), Some(bytes.clone()));
    assert_eq!(reader.next_frame().expect("exact EOF"), None);
}

#[test]
fn raw_reader_high_watermark_plateaus_over_logical_sequence() {
    let one = frame(0, 0, 16);
    let mut stream = Vec::new();
    for _ in 0..128 {
        stream.extend_from_slice(&one);
    }
    let mut reader = RawEac3FrameReader::new(
        Chunked {
            bytes: &stream,
            offset: 0,
            chunk: 7,
        },
        64,
    );
    let mut count = 0;
    while reader.next_frame().expect("logical frame").is_some() {
        count += 1;
    }
    assert_eq!(count, 128);
    assert_eq!(reader.stats().frames_emitted, 128);
    assert!(reader.stats().max_input_carry_bytes <= 16);
    assert_eq!(reader.stats().max_frame_bytes, 16);
}

#[test]
fn raw_reader_enforces_declared_frame_bound() {
    let bytes = frame(0, 0, 16);
    let mut reader = RawEac3FrameReader::new(
        Chunked {
            bytes: &bytes,
            offset: 0,
            chunk: 8,
        },
        8,
    );
    assert!(matches!(
        reader.next_frame(),
        Err(InputMediaError::DemuxOutputTooLarge { limit: 8 })
    ));
}
