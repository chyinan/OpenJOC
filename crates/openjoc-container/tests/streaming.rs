use openjoc_container::{InputMediaError, RawEac3AccessUnitReader, RawEac3FrameReader};
use openjoc_eac3::{group_access_units, index_syncframes};
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
    push(&mut bits, 2, 3); // stereo
    push(&mut bits, 0, 1); // no LFE
    push(&mut bits, 16, 5); // E-AC-3 syntax
    let mut bytes = vec![0_u8; size];
    for (index, bit) in bits.into_iter().enumerate() {
        if bit {
            bytes[index / 8] |= 0x80 >> (index % 8);
        }
    }
    bytes
}

fn legacy_frame() -> Vec<u8> {
    let mut bits = Vec::new();
    push(&mut bits, 0x0b77, 16);
    push(&mut bits, 0, 16);
    push(&mut bits, 0, 2);
    push(&mut bits, 0, 6); // 128 bytes
    push(&mut bits, 8, 5);
    let mut bytes = vec![0_u8; 128];
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
fn access_unit_reader_groups_annex_j_legacy_i0_with_dependent_d0() {
    let stream = [legacy_frame(), frame(1, 0, 128)].concat();
    let mut reader = RawEac3AccessUnitReader::new(stream.as_slice(), 4096);

    let access_unit = reader
        .next_access_unit()
        .expect("mixed reader")
        .expect("mixed access unit");

    assert_eq!(access_unit.frames.len(), 2);
    assert_eq!(access_unit.unit.frame_count, 2);
    assert_eq!(reader.next_access_unit().expect("EOF"), None);
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

#[test]
fn raw_reader_rejects_short_and_bad_sync_input_without_panicking() {
    for bytes in [vec![], vec![0x0b], vec![0; 8]] {
        let mut reader = RawEac3FrameReader::new(
            Chunked {
                bytes: &bytes,
                offset: 0,
                chunk: 1,
            },
            64,
        );
        let result = reader.next_frame();
        if bytes.is_empty() {
            assert_eq!(result.expect("empty input is an exact EOF"), None);
        } else {
            assert!(result.is_err(), "malformed input must fail: {bytes:?}");
        }
    }
}

#[test]
fn raw_reader_rejects_reserved_header_values() {
    let mut bytes = frame(0, 0, 16);
    bytes[4] |= 0xc0; // reserved sample-rate code 3
    let mut reader = RawEac3FrameReader::new(
        Chunked {
            bytes: &bytes,
            offset: 0,
            chunk: 8,
        },
        64,
    );
    assert!(matches!(
        reader.next_frame(),
        Err(InputMediaError::InvalidDemuxedEac3(
            openjoc_eac3::Eac3Error::ReservedSampleRate
        ))
    ));
}

#[test]
fn raw_reader_rejects_absurd_declared_size_before_allocating_it() {
    let mut bytes = frame(0, 0, 16);
    let declared_words = 0x7ff_u16;
    for index in 0..11 {
        let bit = (declared_words >> (10 - index)) & 1;
        let position = 21 + index;
        if bit != 0 {
            bytes[position / 8] |= 0x80 >> (position % 8);
        } else {
            bytes[position / 8] &= !(0x80 >> (position % 8));
        }
    }
    let mut reader = RawEac3FrameReader::new(
        Chunked {
            bytes: &bytes,
            offset: 0,
            chunk: 8,
        },
        64,
    );
    assert!(matches!(
        reader.next_frame(),
        Err(InputMediaError::DemuxOutputTooLarge { limit: 64 })
    ));
}

#[test]
fn access_unit_reader_emits_local_indices_with_one_frame_lookahead() {
    let expected = vec![frame(0, 0, 16), frame(1, 0, 18), frame(0, 0, 20)];
    let mut stream = Vec::new();
    for bytes in &expected {
        stream.extend_from_slice(bytes);
    }
    let mut reader = RawEac3AccessUnitReader::new(
        Chunked {
            bytes: &stream,
            offset: 0,
            chunk: 3,
        },
        64,
    );

    let first = reader
        .next_access_unit()
        .expect("first AU")
        .expect("present");
    assert_eq!(
        first.bytes,
        [expected[0].clone(), expected[1].clone()].concat()
    );
    assert_eq!(first.frames.len(), 2);
    assert_eq!(first.frames[0].offset, 0);
    assert_eq!(first.frames[1].offset, expected[0].len());
    assert_eq!(first.unit.first_frame, 0);
    assert_eq!(first.unit.frame_count, 2);

    let second = reader
        .next_access_unit()
        .expect("second AU")
        .expect("present");
    assert_eq!(second.bytes, expected[2]);
    assert_eq!(second.frames.len(), 1);
    assert_eq!(second.frames[0].offset, 0);
    assert_eq!(second.unit.first_frame, 0);
    assert_eq!(second.unit.frame_count, 1);
    assert_eq!(reader.next_access_unit().expect("exact EOF"), None);

    let stats = reader.stats();
    assert_eq!(stats.frames_emitted, 3);
    assert_eq!(stats.access_units_emitted, 2);
    assert_eq!(stats.max_au_frames, 2);
    assert_eq!(stats.max_au_bytes, 34);
    assert_eq!(stats.max_lookahead_frames, 1);
    assert!(stats.max_complete_frames_retained <= 3);
}

#[test]
fn access_unit_reader_rejects_non_independent_start() {
    let bytes = frame(1, 0, 16);
    let mut reader = RawEac3AccessUnitReader::new(
        Chunked {
            bytes: &bytes,
            offset: 0,
            chunk: 4,
        },
        64,
    );
    assert!(matches!(
        reader.next_access_unit(),
        Err(InputMediaError::InvalidDemuxedEac3(
            openjoc_eac3::Eac3Error::MissingIndependentSubstreamZero { frame: 0 }
        ))
    ));
}

#[test]
fn access_unit_reader_high_watermark_plateaus_over_long_sequence() {
    let one = frame(0, 0, 16);
    let mut stream = Vec::new();
    for _ in 0..128 {
        stream.extend_from_slice(&one);
    }
    let mut reader = RawEac3AccessUnitReader::new(
        Chunked {
            bytes: &stream,
            offset: 0,
            chunk: 5,
        },
        64,
    );
    let mut count = 0;
    while reader.next_access_unit().expect("valid AU").is_some() {
        count += 1;
    }
    assert_eq!(count, 128);
    let stats = reader.stats();
    assert_eq!(stats.frames_emitted, 128);
    assert_eq!(stats.access_units_emitted, 128);
    assert_eq!(stats.max_au_bytes, 16);
    assert_eq!(stats.max_au_frames, 1);
    assert!(stats.max_complete_frames_retained <= 2);
    assert!(stats.max_input_carry_bytes <= 16);
}

#[test]
fn dependent_access_units_match_capture_grouping_with_bounded_lookahead() {
    let independent = frame(0, 0, 16);
    let dependent = frame(1, 0, 18);
    let mut stream = Vec::new();
    for _ in 0..128 {
        stream.extend_from_slice(&independent);
        stream.extend_from_slice(&dependent);
    }

    let captured_frames = index_syncframes(&stream).expect("capture frame index");
    let captured_units = group_access_units(&captured_frames).expect("capture AU grouping");
    assert_eq!(captured_units.len(), 128);

    let mut reader = RawEac3AccessUnitReader::new(
        Chunked {
            bytes: &stream,
            offset: 0,
            chunk: 3,
        },
        64,
    );
    for captured in captured_units {
        let streamed = reader
            .next_access_unit()
            .expect("streamed dependent AU")
            .expect("present dependent AU");
        assert_eq!(streamed.unit.first_frame, 0);
        assert_eq!(streamed.unit.frame_count, captured.frame_count);
        assert_eq!(streamed.unit.sample_rate, captured.sample_rate);
        assert_eq!(streamed.unit.samples, captured.samples);

        let capture_start = captured_frames[captured.first_frame].offset;
        let capture_last = captured_frames[captured.first_frame + captured.frame_count - 1];
        let capture_end = capture_last.offset + capture_last.header.frame_size;
        assert_eq!(streamed.bytes, stream[capture_start..capture_end]);
        assert_eq!(
            streamed
                .frames
                .iter()
                .map(|entry| entry.header)
                .collect::<Vec<_>>(),
            captured_frames[captured.first_frame..captured.first_frame + captured.frame_count]
                .iter()
                .map(|entry| entry.header)
                .collect::<Vec<_>>()
        );
    }
    assert_eq!(reader.next_access_unit().expect("exact EOF"), None);
    let stats = reader.stats();
    assert_eq!(stats.access_units_emitted, 128);
    assert_eq!(stats.frames_emitted, 256);
    assert_eq!(stats.max_au_frames, 2);
    assert_eq!(stats.max_au_bytes, 34);
    assert_eq!(stats.max_lookahead_frames, 1);
    assert!(stats.max_complete_frames_retained <= 3);
    assert!(stats.max_input_carry_bytes <= 18);
}
