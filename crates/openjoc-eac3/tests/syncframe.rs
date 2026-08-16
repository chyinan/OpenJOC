use openjoc_bitio::BitError;
use openjoc_eac3::{
    AudioPcmSynthesizer, ChannelLocation, CouplingInformation, Eac3DecodeStageTiming, Eac3Error,
    InternalBasePolicy, JocAccessUnitPcmDecoder, JocAddbsi, StreamType,
    block_start_information_length, channel_end_mantissa, channel_exponent_group_count,
    classify_aux_emdf, classify_skip_field_emdf, decode_audio_blocks,
    decode_audio_blocks_with_parsed_frame, decode_audio_blocks_with_policy, decode_audio_frame_pcm,
    decode_exponents, decode_first_audio_block, decode_frame_exponent_strategy, dynamic_range_gain,
    extract_aux_emdf, extract_aux_joc_access_unit, extract_auxdata, extract_joc_addbsi_access_unit,
    group_access_units, index_syncframes, inspect_audio_block_carriers, parse_audio_frame,
    parse_bsi, parse_first_audio_block_prefix, parse_joc_access_unit, parse_joc_addbsi,
    parse_syncframe_header, reconstruct_enhanced_coupling, spx_subband_range,
    synthesize_audio_blocks, validate_complexity_index, validate_joc_access_unit,
};
use openjoc_emdf::{JocProfileField, JocValidationProfile, JocValidationStatus};
use sha2::{Digest, Sha256};
use std::{hint::black_box, time::Instant};

#[derive(Clone, Default)]
struct Bits(Vec<bool>);

impl Bits {
    fn push(&mut self, value: u64, width: u8) {
        for shift in (0..width).rev() {
            self.0.push((value >> shift) & 1 != 0);
        }
    }

    fn bytes(mut self, length: usize) -> Vec<u8> {
        self.0.resize(length * 8, false);
        self.0
            .chunks(8)
            .map(|chunk| {
                chunk
                    .iter()
                    .fold(0_u8, |value, bit| (value << 1) | u8::from(*bit))
            })
            .collect()
    }

    fn set(&mut self, position: usize, value: u64, width: u8) {
        for (index, shift) in (0..width).rev().enumerate() {
            self.0[position + index] = (value >> shift) & 1 != 0;
        }
    }

    fn padded_bytes(self) -> Vec<u8> {
        let length = self.0.len().div_ceil(8);
        self.bytes(length)
    }
}

fn frame(stream_type: u8, substream_id: u8, size: usize, fscod: u8, blocks: u8) -> Vec<u8> {
    assert_eq!(size % 2, 0);
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(u64::from(stream_type), 2);
    bits.push(u64::from(substream_id), 3);
    bits.push(u64::try_from(size / 2 - 1).expect("frame words"), 11);
    bits.push(u64::from(fscod), 2);
    bits.push(u64::from(blocks), 2);
    bits.bytes(size)
}

fn six_block_mono_frame(
    stream_type: u8,
    channel_map: Option<u16>,
    dynamic_range: Option<u8>,
) -> Vec<u8> {
    six_block_mono_frame_with_aht(stream_type, channel_map, dynamic_range, true)
}

fn push_conventional_mono_mantissas(bits: &mut Bits) {
    // The shared exponent/allocation controls used by this fixture produce
    // [9, 8, 8, 8, 4, ...]. Conventional bap 9/8 words occupy 8/7 bits;
    // the remaining 69 bap-4 mantissas are paired into 35 seven-bit groups.
    bits.push(0, 8);
    for _ in 0..3 {
        bits.push(0, 7);
    }
    for _ in 0..35 {
        bits.push(0, 7);
    }
}

fn six_block_mono_frame_with_aht(
    stream_type: u8,
    channel_map: Option<u16>,
    dynamic_range: Option<u8>,
    aht: bool,
) -> Vec<u8> {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(u64::from(stream_type), 2);
    bits.push(0, 3);
    bits.push(2047, 11); // 4096-byte frame
    bits.push(0, 2); // 48 kHz
    bits.push(3, 2); // six audio blocks
    bits.push(1, 3); // mono
    bits.push(0, 1); // no LFE
    bits.push(16, 5); // E-AC-3 version
    bits.push(31, 5); // dialnorm
    bits.push(0, 1); // no compression metadata
    if stream_type == 1 {
        bits.push(u64::from(channel_map.is_some()), 1);
        if let Some(channel_map) = channel_map {
            bits.push(u64::from(channel_map), 16);
        }
    }
    bits.push(0, 1); // no mixing metadata
    bits.push(0, 1); // no informational metadata
    bits.push(0, 1); // no addbsi

    bits.push(1, 1); // per-block exponent strategies
    bits.push(u64::from(aht), 1); // AHT syntax
    bits.push(1, 2); // SNR strategy 1
    bits.push(0, 1); // transient processing disabled
    bits.push(16, 7); // bit-allocation syntax enabled
    for block in 0..6 {
        bits.push(u64::from(block == 0), 2); // D15 then reuse
    }
    if stream_type == 0 {
        bits.push(0, 5); // converter exponent strategy
    }
    if aht {
        bits.push(1, 1); // mono channel uses AHT
    }
    bits.push(0, 1); // no block-start information

    bits.push(u64::from(dynamic_range.is_some()), 1);
    if let Some(dynamic_range) = dynamic_range {
        bits.push(u64::from(dynamic_range), 8);
    }
    bits.push(0, 1); // no SPX
    bits.push(0, 6); // chbwcod = 0 => endmant = 73
    bits.push(15, 4); // initial exponent
    bits.push(87, 7); // deltas +1, 0, 0
    bits.push(87, 7); // deltas +1, 0, 0
    for _ in 0..22 {
        bits.push(62, 7); // grouped exponent deltas 0, 0, 0
    }
    bits.push(0, 2); // gain range
    bits.push(1, 1); // new bit-allocation parameters
    bits.push(0, 11); // all parameter codes zero
    bits.push(63, 6); // coarse SNR
    bits.push(15, 4); // fine SNR
    if stream_type == 0 {
        bits.push(0, 1); // converter SNR offset absent
    }
    if aht {
        bits.push(1, 2); // AHT mode 1
        bits.push(0, 1); // hebap 9 gain 1
        bits.push(1, 1); // hebap 8 gain 2
        bits.push(0, 1); // hebap 8 gain 1
        bits.push(1, 1); // hebap 8 gain 2
        for width in [4_u8; 6] {
            bits.push(0, width);
        }
        for width in [2_u8; 6] {
            bits.push(0, width);
        }
        for width in [3_u8; 6] {
            bits.push(0, width);
        }
        for width in [2_u8; 6] {
            bits.push(0, width);
        }
        for _ in 0..69 {
            bits.push(0, 5);
        }
    } else {
        push_conventional_mono_mantissas(&mut bits);
    }
    for _ in 1..6 {
        bits.push(0, 1); // dynamic range absent/reused
        bits.push(0, 1); // SPX strategy reused
        bits.push(0, 6); // chbwcod remains explicit
        bits.push(0, 1); // bit-allocation parameters reused
        bits.push(0, 1); // block fine SNR offset absent
        if stream_type == 0 {
            bits.push(0, 1); // converter SNR offset absent
        }
        if !aht {
            push_conventional_mono_mantissas(&mut bits);
        }
    }
    bits.bytes(4096)
}

fn benchmark_percentile(sorted: &[f64], percentile: usize) -> f64 {
    let maximum = sorted.len() - 1;
    let index = (maximum * percentile + 50) / 100;
    sorted[index.min(maximum)]
}

#[cfg(debug_assertions)]
fn require_release_benchmark() {
    panic!("run this performance harness with --release");
}

#[cfg(not(debug_assertions))]
fn require_release_benchmark() {}

/// Focused release-mode E-AC-3 core harness. This deliberately excludes JOC
/// reconstruction, spatial rendering, progress reporting, and file output.
/// The bounded public-syntax I0/D0 pair exercises two stateful six-block
/// decode and TDAC paths; it is not a substitute for a real-media retest.
#[test]
#[ignore = "manual release E-AC-3 core performance harness"]
fn eac3_core_release_benchmark() {
    require_release_benchmark();
    let access_units = std::env::var("OPENJOC_EAC3_BENCH_AUS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(200);
    let runs = std::env::var("OPENJOC_EAC3_BENCH_RUNS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(7);
    let collect_stages = std::env::var_os("OPENJOC_EAC3_BENCH_STAGE_TIMING").is_some();
    assert!(access_units > 0 && runs > 0);

    let independent = six_block_mono_frame(0, None, Some(0x00));
    let dependent = six_block_mono_frame(1, Some(0x8000), Some(0xa5));
    let bytes = [independent, dependent].concat();
    let frames = index_syncframes(&bytes).expect("benchmark frame index");
    let unit = group_access_units(&frames).expect("benchmark access unit")[0];
    let dither = [0.5; 512];
    let mut measurements = Vec::with_capacity(runs);

    for run in 0..runs {
        let mut decoder = JocAccessUnitPcmDecoder::new();
        for _ in 0..8 {
            black_box(
                decoder
                    .decode(&bytes, &frames, unit, &dither)
                    .expect("benchmark warmup decode"),
            );
        }
        if collect_stages {
            decoder.enable_stage_timing();
        }
        let mut frame_ms = Vec::with_capacity(access_units);
        let mut checksum = Sha256::new();
        let mut stages = Eac3DecodeStageTiming::default();
        let started = Instant::now();
        for _ in 0..access_units {
            let frame_started = Instant::now();
            let pcm = decoder
                .decode(&bytes, &frames, unit, &dither)
                .expect("benchmark decode");
            if collect_stages {
                stages.add_assign(&decoder.take_stage_timing());
            }
            frame_ms.push(frame_started.elapsed().as_secs_f64() * 1_000.0);
            for channel in &pcm.channels {
                for sample in channel {
                    checksum.update(sample.to_bits().to_le_bytes());
                }
            }
            if let Some(lfe) = &pcm.lfe {
                for sample in lfe {
                    checksum.update(sample.to_bits().to_le_bytes());
                }
            }
            black_box(pcm);
        }
        let elapsed = started.elapsed();
        frame_ms.sort_by(f64::total_cmp);
        measurements.push((
            elapsed.as_secs_f64() * 1_000.0 / access_units as f64,
            benchmark_percentile(&frame_ms, 50),
            benchmark_percentile(&frame_ms, 95),
            benchmark_percentile(&frame_ms, 99),
            *frame_ms.last().expect("frame timing"),
            format!("{:x}", checksum.finalize()),
            stages,
        ));
        eprintln!(
            "eac3-core run={} access_units={} elapsed_ms={:.3} ms_per_au={:.6} realtime_factor={:.3}",
            run + 1,
            access_units,
            elapsed.as_secs_f64() * 1_000.0,
            measurements[run].0,
            (access_units as f64 * f64::from(unit.samples) / 48_000.0) / elapsed.as_secs_f64()
        );
    }
    measurements.sort_by(|left, right| left.0.total_cmp(&right.0));
    let median = &measurements[measurements.len() / 2];
    assert!(
        measurements
            .iter()
            .all(|measurement| measurement.5 == median.5),
        "benchmark output changed across identical runs"
    );
    eprintln!(
        "eac3-core median runs={} access_units={} ms_per_au={:.6} p50_ms={:.6} p95_ms={:.6} p99_ms={:.6} max_ms={:.6} checksum={}",
        runs, access_units, median.0, median.1, median.2, median.3, median.4, median.5
    );
    if collect_stages {
        let milliseconds = |duration: std::time::Duration| duration.as_secs_f64() * 1_000.0;
        let stages = &median.6;
        eprintln!(
            "eac3-core stages_ms total={:.3} syncframe_header={:.3} block_syntax_exponents={:.3} bit_allocation={:.3} mantissa_dequantization={:.3} coupling_rematrix_spx={:.3} inverse_transform={:.3} window_overlap={:.3} pcm_assembly={:.3} allocation_copy={:.3} state_commit={:.3}",
            milliseconds(stages.total),
            milliseconds(stages.syncframe_and_header_parsing),
            milliseconds(stages.audio_block_syntax_and_exponents),
            milliseconds(stages.bit_allocation),
            milliseconds(stages.mantissa_unpack_and_dequantization),
            milliseconds(stages.coupling_rematrix_and_spx),
            milliseconds(stages.inverse_transform),
            milliseconds(stages.window_and_overlap_add),
            milliseconds(stages.pcm_assembly),
            milliseconds(stages.allocation_and_copy),
            milliseconds(stages.decoder_state_commit),
        );
    }
}

fn skip_field_joc_frame(emdf: &[u8]) -> Vec<u8> {
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
    bits.push(0, 1); // convsync
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

#[test]
fn parse_only_carrier_inspection_reports_reached_prefixes_and_unresolved_blocks() {
    let bytes = six_block_mono_frame(0, None, None);
    let mut callbacks = Vec::new();
    let report = inspect_audio_block_carriers(&bytes, |carrier| {
        callbacks.push(carrier.block_index);
    })
    .expect("all synthetic zero-bap blocks are bounded");
    assert_eq!(callbacks, (0..6).collect::<Vec<_>>());
    assert_eq!(report.examined_blocks, 6);
    assert_eq!(report.unresolved_blocks, 0);
}

#[test]
fn parse_only_carrier_inspection_consumes_reachable_mantissa_cursors() {
    let bytes = six_block_mono_frame(0, None, None);
    let mut callbacks = Vec::new();
    let report = inspect_audio_block_carriers(&bytes, |carrier| {
        callbacks.push((
            carrier.block_index,
            carrier.prefix_start_offset_bits,
            carrier.next_offset_bits,
            carrier.skip_field.is_some(),
        ));
    })
    .expect("all synthetic zero-bap blocks are bounded");

    assert_eq!(report.examined_blocks, 6);
    assert_eq!(report.unresolved_blocks, 0);
    assert_eq!(callbacks.len(), 6);
    assert_eq!(
        callbacks.iter().map(|entry| entry.0).collect::<Vec<_>>(),
        (0..6).collect::<Vec<_>>()
    );
    assert!(callbacks.iter().all(|entry| !entry.3));
    assert!(
        callbacks
            .windows(2)
            .all(|window| window[0].2 <= window[1].1)
    );
}

#[test]
fn parses_every_stream_rate_and_block_code() {
    let rates = [48_000, 44_100, 32_000];
    let block_counts = [1, 2, 3, 6];
    for (fscod, sample_rate) in rates.into_iter().enumerate() {
        for (code, blocks) in block_counts.into_iter().enumerate() {
            let bytes = frame(
                1,
                5,
                64,
                u8::try_from(fscod).expect("rate code"),
                u8::try_from(code).expect("block code"),
            );
            let header = parse_syncframe_header(&bytes).expect("valid header");
            assert_eq!(header.stream_type, StreamType::Dependent);
            assert_eq!(header.substream_id, 5);
            assert_eq!(header.frame_size, 64);
            assert_eq!(header.sample_rate, sample_rate);
            assert_eq!(header.audio_blocks, blocks);
            assert_eq!(header.samples, u16::from(blocks) * 256);
        }
    }
}

#[test]
fn indexes_complete_frames_without_scanning_inside_payloads() {
    let first = frame(0, 0, 32, 0, 3);
    let second = frame(1, 0, 48, 0, 3);
    let bytes = [first, second].concat();
    let entries = index_syncframes(&bytes).expect("two frames");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].offset, 0);
    assert_eq!(entries[1].offset, 32);
    assert_eq!(entries[1].header.frame_size, 48);
}

#[test]
fn rejects_reserved_headers_bad_sync_and_declared_truncation() {
    assert_eq!(
        parse_syncframe_header(&frame(3, 0, 16, 0, 0)),
        Err(Eac3Error::ReservedStreamType)
    );
    assert_eq!(
        parse_syncframe_header(&frame(0, 0, 16, 3, 0)),
        Err(Eac3Error::ReservedSampleRate)
    );
    let mut bad_sync = frame(0, 0, 16, 0, 0);
    bad_sync[0] = 0;
    assert_eq!(
        parse_syncframe_header(&bad_sync),
        Err(Eac3Error::InvalidSyncword { actual: 0x0077 })
    );
    let truncated = frame(0, 0, 32, 0, 0);
    assert_eq!(
        index_syncframes(&truncated[..16]),
        Err(Eac3Error::TruncatedFrame {
            offset: 0,
            declared: 32,
            available: 16,
        })
    );
}

#[test]
fn parses_and_bounds_the_type_a_addbsi_extension() {
    assert_eq!(
        parse_joc_addbsi(&[0x01, 0x10]),
        Ok(JocAddbsi {
            complexity_index: 16,
        })
    );
    assert_eq!(
        parse_joc_addbsi(&[0x00, 0x10]),
        Err(Eac3Error::MissingJocExtensionFlag)
    );
    assert_eq!(
        parse_joc_addbsi(&[0x03, 0x10]),
        Err(Eac3Error::NonzeroReservedData)
    );
    assert_eq!(
        parse_joc_addbsi(&[0x01, 0x11]),
        Err(Eac3Error::ComplexityIndexOutOfRange { actual: 17 })
    );
    assert_eq!(
        parse_joc_addbsi(&[0x01]),
        Err(Eac3Error::InvalidAddbsiLength { actual: 1 })
    );
}

#[test]
fn complexity_index_equals_the_oamd_program_object_count() {
    assert_eq!(validate_complexity_index(0, 0), Ok(()));
    assert_eq!(validate_complexity_index(16, 16), Ok(()));
    assert_eq!(
        validate_complexity_index(7, 8),
        Err(Eac3Error::ComplexityIndexMismatch {
            complexity: 7,
            objects: 8,
        })
    );
    assert_eq!(
        validate_complexity_index(0, 17),
        Err(Eac3Error::ComplexityIndexMismatch {
            complexity: 0,
            objects: 17,
        })
    );
}

#[test]
fn parses_custom_channel_map_on_a_dependent_substream() {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(1, 2); // dependent
    bits.push(0, 3); // dependent substream zero
    bits.push(7, 11); // 16-byte frame
    bits.push(0, 2); // 48 kHz
    bits.push(0, 2); // one block
    bits.push(2, 3); // 2/0: two full-bandwidth channels
    bits.push(0, 1); // no LFE
    bits.push(16, 5); // E-AC-3 version
    bits.push(0, 5); // dialnorm
    bits.push(0, 1); // no compression word
    bits.push(1, 1); // custom channel map exists
    bits.push(1 << 9, 16); // chanmap bit 6: Lb/Rb pair
    bits.push(0, 1); // no mixing metadata
    bits.push(0, 1); // no informational metadata
    bits.push(0, 1); // no addbsi

    let bsi = parse_bsi(&bits.bytes(16)).expect("valid dependent BSI");
    assert_eq!(bsi.header.stream_type, StreamType::Dependent);
    assert_eq!(bsi.channel_map, Some(1 << 9));
}

#[test]
fn decodes_an_indexed_independent_joc_access_unit_to_pcm() {
    let bytes = six_block_mono_frame(0, None, Some(0xa5));
    let frames = index_syncframes(&bytes).expect("indexed frame");
    let units = group_access_units(&frames).expect("access unit");
    let mut decoder = JocAccessUnitPcmDecoder::new();
    let pcm = decoder
        .decode(&bytes, &frames, units[0], &[0.5; 49])
        .expect("independent access-unit PCM");
    assert_eq!(pcm.sample_rate, 48_000);
    assert_eq!(pcm.samples, 1536);
    assert_eq!(pcm.channels.len(), 1);
    assert_eq!(pcm.channel_locations, vec![ChannelLocation::Centre]);
    assert_eq!(pcm.channels[0].len(), 1536);
    assert!(pcm.channels[0].iter().all(|sample| sample.is_finite()));
}

#[test]
fn decodes_a_raw_dependent_d0_custom_map_through_pcm_and_replaces_i0() {
    let independent = six_block_mono_frame(0, None, Some(0x00));
    let dependent = six_block_mono_frame(1, Some(0x4000), Some(0xa5)); // Centre replaces I0
    let bytes = [independent.clone(), dependent].concat();
    let frames = index_syncframes(&bytes).expect("indexed I0/D0 frames");
    let units = group_access_units(&frames).expect("JOC access unit");
    let mut decoder = JocAccessUnitPcmDecoder::new();
    let pcm = decoder
        .decode(&bytes, &frames, units[0], &[0.5; 512])
        .expect("dependent D0 PCM");
    assert_eq!(pcm.samples, 1536);
    assert_eq!(pcm.channels.len(), 1);
    assert_eq!(pcm.channel_locations, vec![ChannelLocation::Centre]);
    assert_eq!(pcm.channels[0].len(), 1536);

    let mut independent_decoder = JocAccessUnitPcmDecoder::new();
    let independent_pcm = independent_decoder
        .decode(
            &independent,
            &index_syncframes(&independent).expect("I0 index"),
            openjoc_eac3::AccessUnitIndex {
                first_frame: 0,
                frame_count: 1,
                sample_rate: 48_000,
                samples: 1536,
            },
            &[0.5; 512],
        )
        .expect("independent PCM");
    assert_ne!(pcm.channels[0], independent_pcm.channels[0]);
    assert!(pcm.channels[0].iter().all(|sample| sample.is_finite()));
}

#[test]
fn dependent_configuration_change_resets_only_dependent_tdac_history() {
    let independent = six_block_mono_frame(0, None, Some(0x00));
    let centre_dependent = six_block_mono_frame(1, Some(0x4000), Some(0xa5));
    let left_dependent = six_block_mono_frame(1, Some(0x8000), Some(0xa5));

    let centre_bytes = [independent.clone(), centre_dependent].concat();
    let centre_frames = index_syncframes(&centre_bytes).expect("centre I0/D0 frames");
    let centre_unit = group_access_units(&centre_frames).expect("centre AU")[0];
    let mut continuing = JocAccessUnitPcmDecoder::new();
    continuing
        .decode(&centre_bytes, &centre_frames, centre_unit, &[0.5; 512])
        .expect("prime dependent TDAC state");
    let mut independent_control = JocAccessUnitPcmDecoder::new();
    independent_control
        .decode(&centre_bytes, &centre_frames, centre_unit, &[0.5; 512])
        .expect("prime independent control state");

    let left_bytes = [independent, left_dependent].concat();
    let left_frames = index_syncframes(&left_bytes).expect("left I0/D0 frames");
    let left_unit = group_access_units(&left_frames).expect("left AU")[0];
    let transitioned = continuing
        .decode(&left_bytes, &left_frames, left_unit, &[0.5; 512])
        .expect("changed dependent map");
    let fresh = JocAccessUnitPcmDecoder::new()
        .decode(&left_bytes, &left_frames, left_unit, &[0.5; 512])
        .expect("fresh changed-map decode");
    let independent_frames =
        index_syncframes(&left_bytes[..left_frames[1].offset]).expect("independent-only frame");
    let independent_unit = group_access_units(&independent_frames).expect("independent AU")[0];
    let continued_independent = independent_control
        .decode(
            &left_bytes[..left_frames[1].offset],
            &independent_frames,
            independent_unit,
            &[0.5; 512],
        )
        .expect("continued independent TDAC state");

    // The first canonical output is dependent Left. Its coded-channel TDAC
    // history must not inherit the previous Centre identity.
    assert_eq!(transitioned.channels[0], fresh.channels[0]);
    assert_eq!(transitioned.channels.len(), 2);
    assert_eq!(
        transitioned.channel_locations,
        vec![ChannelLocation::Left, ChannelLocation::Centre]
    );
    assert_eq!(transitioned.channels[1], continued_independent.channels[0]);
}

#[test]
fn capture_and_access_unit_local_decode_have_identical_dependent_pcm() {
    let independent = six_block_mono_frame(0, None, Some(0x00));
    let dependent = six_block_mono_frame(1, Some(0x8000), Some(0xa5));
    let one_unit = [independent, dependent].concat();
    let stream = [one_unit.clone(), one_unit.clone(), one_unit.clone()].concat();
    let frames = index_syncframes(&stream).expect("capture frames");
    let units = group_access_units(&frames).expect("capture AUs");

    let mut capture_decoder = JocAccessUnitPcmDecoder::new();
    let capture = units
        .iter()
        .copied()
        .map(|unit| {
            capture_decoder
                .decode(&stream, &frames, unit, &[0.5; 512])
                .expect("capture decode")
        })
        .collect::<Vec<_>>();

    let mut local_decoder = JocAccessUnitPcmDecoder::new();
    let local = stream
        .chunks_exact(one_unit.len())
        .map(|bytes| {
            let local_frames = index_syncframes(bytes).expect("local frames");
            let local_unit = group_access_units(&local_frames).expect("local AU")[0];
            local_decoder
                .decode(bytes, &local_frames, local_unit, &[0.5; 512])
                .expect("AU-local decode")
        })
        .collect::<Vec<_>>();

    assert_eq!(local, capture);
}

#[test]
fn failed_dependent_decode_is_atomic_for_both_substream_histories() {
    let independent = six_block_mono_frame(0, None, Some(0x00));
    let dependent = six_block_mono_frame(1, Some(0x4000), Some(0xa5));
    let valid = [independent, dependent].concat();
    let frames = index_syncframes(&valid).expect("valid I0/D0 frames");
    let unit = group_access_units(&frames).expect("valid AU")[0];
    let mut subject = JocAccessUnitPcmDecoder::new();
    let mut control = JocAccessUnitPcmDecoder::new();
    subject
        .decode(&valid, &frames, unit, &[0.5; 512])
        .expect("subject first AU");
    control
        .decode(&valid, &frames, unit, &[0.5; 512])
        .expect("control first AU");

    let truncated = &valid[..valid.len() - 1];
    assert!(matches!(
        subject.decode(truncated, &frames, unit, &[0.5; 512]),
        Err(Eac3Error::TruncatedFrame { .. })
    ));

    let subject_next = subject
        .decode(&valid, &frames, unit, &[0.5; 512])
        .expect("subject retry after failed D0");
    let control_next = control
        .decode(&valid, &frames, unit, &[0.5; 512])
        .expect("control uninterrupted second AU");
    assert_eq!(subject_next, control_next);
}

#[test]
fn explicit_access_unit_decoder_reset_matches_a_fresh_decoder() {
    let independent = six_block_mono_frame(0, None, Some(0x00));
    let dependent = six_block_mono_frame(1, Some(0x4000), Some(0xa5));
    let bytes = [independent, dependent].concat();
    let frames = index_syncframes(&bytes).expect("I0/D0 frames");
    let unit = group_access_units(&frames).expect("AU")[0];
    let mut reset = JocAccessUnitPcmDecoder::new();
    reset
        .decode(&bytes, &frames, unit, &[0.5; 512])
        .expect("prime decoder");
    reset.reset();
    let reset_output = reset
        .decode(&bytes, &frames, unit, &[0.5; 512])
        .expect("decode after reset");
    let fresh_output = JocAccessUnitPcmDecoder::new()
        .decode(&bytes, &frames, unit, &[0.5; 512])
        .expect("fresh decode");
    assert_eq!(reset_output, fresh_output);
}

#[test]
fn joc_decoder_rejects_more_than_one_dependent_substream() {
    let independent = six_block_mono_frame(0, None, Some(0x00));
    let dependent_zero = six_block_mono_frame(1, Some(0x4000), Some(0xa5));
    let mut dependent_one = six_block_mono_frame(1, Some(0x8000), Some(0xa5));
    // substreamid is the three bits immediately after strmtyp.
    dependent_one[2] = (dependent_one[2] & 0xc7) | 0x08;
    let bytes = [independent, dependent_zero, dependent_one].concat();
    let frames = index_syncframes(&bytes).expect("I0/D0/D1 frames");
    let unit = group_access_units(&frames).expect("general E-AC-3 grouping")[0];
    assert_eq!(unit.frame_count, 3);
    assert_eq!(
        JocAccessUnitPcmDecoder::new().decode(&bytes, &frames, unit, &[0.5; 512]),
        Err(Eac3Error::UnsupportedJocAccessUnitFrameCount { actual: 3 })
    );
}

#[test]
fn truncated_dependent_chanmap_is_a_bounded_parser_error() {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(1, 2); // dependent
    bits.push(0, 3); // D0
    bits.push(3, 11); // eight-byte frame
    bits.push(0, 2); // 48 kHz
    bits.push(3, 2); // six blocks
    bits.push(1, 3); // mono
    bits.push(0, 1); // no LFE
    bits.push(16, 5); // bsid
    bits.push(31, 5); // dialnorm
    bits.push(0, 1); // no compression word
    bits.push(1, 1); // chanmape; fewer than 16 bits remain
    assert!(matches!(
        parse_bsi(&bits.bytes(8)),
        Err(Eac3Error::Bit(BitError::EndOfInput { requested: 16, .. }))
    ));
}

#[test]
fn rejects_non_six_block_joc_access_units() {
    let bytes = frame(0, 0, 16, 0, 0);
    let frames = index_syncframes(&bytes).expect("indexed one-block frame");
    let units = group_access_units(&frames).expect("grouped one-block frame");
    let mut decoder = JocAccessUnitPcmDecoder::new();
    assert_eq!(
        decoder.decode(&bytes, &frames, units[0], &[]),
        Err(Eac3Error::UnsupportedJocAudioBlockCount { actual: 1 })
    );
}

#[test]
fn parses_bsi_conditionals_to_extract_addbsi_without_scanning() {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 2); // independent
    bits.push(0, 3);
    bits.push(31, 11); // 64-byte frame
    bits.push(0, 2); // 48 kHz
    bits.push(3, 2); // 6 blocks
    bits.push(2, 3); // stereo
    bits.push(0, 1); // no LFE
    bits.push(16, 5); // E-AC-3 version
    bits.push(31, 5); // dialnorm
    bits.push(0, 1); // no compression word
    bits.push(0, 1); // no mixing metadata
    bits.push(0, 1); // no informational metadata
    bits.push(1, 1); // addbsi exists
    bits.push(1, 6); // 2 bytes
    bits.push(0x01, 8);
    bits.push(0x05, 8);
    let bytes = bits.bytes(64);

    let bsi = parse_bsi(&bytes).expect("valid complete BSI");
    assert_eq!(bsi.audio_coding_mode, 2);
    assert!(!bsi.lfe_on);
    assert_eq!(bsi.bitstream_id, 16);
    assert_eq!(bsi.addbsi.as_deref(), Some(&[0x01, 0x05][..]));
    assert_eq!(
        parse_joc_addbsi(bsi.addbsi.as_deref().expect("addbsi")),
        Ok(JocAddbsi {
            complexity_index: 5,
        })
    );
}

#[test]
fn mixing_option_four_length_includes_the_mixdeflen_field() {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 2);
    bits.push(0, 3);
    bits.push(31, 11);
    bits.push(0, 2);
    bits.push(3, 2);
    bits.push(2, 3);
    bits.push(0, 1);
    bits.push(16, 5);
    bits.push(31, 5);
    bits.push(0, 1); // compre
    bits.push(1, 1); // mixmdate
    bits.push(0, 1); // pgmscle
    bits.push(0, 1); // extpgmscle
    bits.push(3, 2); // mixdef option 4
    bits.push(0, 5); // 2-byte mixdata, including these five bits
    bits.push(0, 11); // mixdata2e, mixdata3e, and zero fill
    bits.push(0, 1); // frmmixcfginfoe
    bits.push(0, 1); // infomdate
    bits.push(1, 1); // addbsie
    bits.push(1, 6);
    bits.push(0x01, 8);
    bits.push(0x04, 8);

    let bsi = parse_bsi(&bits.bytes(64)).expect("bounded option-four mixdata");
    assert_eq!(bsi.addbsi, Some(vec![0x01, 0x04]));
}

#[test]
fn parses_one_block_audio_frame_state_and_exact_block_offset() {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 2); // independent
    bits.push(0, 3);
    bits.push(63, 11); // 128-byte frame
    bits.push(0, 2); // 48 kHz
    bits.push(0, 2); // one block
    bits.push(1, 3); // mono
    bits.push(0, 1); // no LFE
    bits.push(16, 5);
    bits.push(31, 5);
    bits.push(0, 1); // compre
    bits.push(0, 1); // mixmdate
    bits.push(0, 1); // infomdate
    bits.push(0, 1); // convsync
    bits.push(0, 1); // addbsie

    bits.push(0, 2); // frame SNR strategy
    bits.push(0, 1); // transient processing
    bits.push(1, 1); // block-switch syntax
    bits.push(0, 1); // dither syntax
    bits.push(0, 1); // bit-allocation syntax
    bits.push(1, 1); // frame fast-gain syntax
    bits.push(1, 1); // delta-bit-allocation syntax
    bits.push(1, 1); // skip-field syntax
    bits.push(0, 1); // SPX attenuation syntax
    bits.push(1, 2); // channel exponent strategy D15
    bits.push(0, 1); // converter exponent strategy absent
    bits.push(32, 6); // frame coarse SNR offset
    bits.push(7, 4); // frame fine SNR offset
    let expected_offset = bits.0.len();
    let bytes = bits.bytes(128);

    let frame = parse_audio_frame(&bytes).expect("valid audio-frame syntax");
    assert_eq!(frame.full_bandwidth_channels, 1);
    assert_eq!(frame.snr_offset_strategy, 0);
    assert!(frame.syntax.block_switch());
    assert!(!frame.syntax.dither());
    assert!(!frame.syntax.bit_allocation());
    assert!(frame.syntax.frame_fast_gain());
    assert!(frame.syntax.delta_bit_allocation());
    assert!(frame.syntax.skip_field());
    assert!(!frame.syntax.spx_attenuation());
    assert_eq!(frame.coupling_in_use, [false]);
    assert_eq!(frame.channel_exponent_strategy, vec![vec![1]]);
    assert_eq!(frame.frame_coarse_snr_code, Some(32));
    assert_eq!(frame.frame_fine_snr_code, Some(7));
    assert_eq!(frame.audio_blocks_offset_bits, expected_offset);
}

#[test]
fn parses_first_audio_block_through_spectral_extension_coordinates() {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 2); // independent
    bits.push(0, 3);
    bits.push(63, 11); // 128-byte frame
    bits.push(0, 2); // 48 kHz
    bits.push(0, 2); // one block
    bits.push(1, 3); // mono
    bits.push(0, 1); // no LFE
    bits.push(16, 5);
    bits.push(31, 5);
    bits.push(0, 1); // compre
    bits.push(0, 1); // mixmdate
    bits.push(0, 1); // infomdate
    bits.push(0, 1); // convsync
    bits.push(0, 1); // addbsie
    bits.push(0, 2); // frame SNR strategy
    bits.push(0, 1); // transient processing
    bits.push(1, 1); // block-switch syntax
    bits.push(0, 1); // dither syntax disabled
    bits.push(0, 5); // remaining syntax flags
    bits.push(1, 2); // channel D15
    bits.push(0, 1); // converter exponent strategy absent
    bits.push(0, 10); // frame SNR offsets

    bits.push(1, 1); // block switch
    bits.push(1, 1); // dynamic range exists
    bits.push(0xa5, 8); // dynamic range
    bits.push(1, 1); // SPX in use (strategy is implicit in block zero)
    bits.push(2, 2); // start copy frequency code
    bits.push(0, 3); // begin subband 2
    bits.push(3, 3); // end subband 9
    bits.push(1, 1); // band structure exists
    for value in [false, true, false, true, true, false] {
        bits.push(u64::from(value), 1); // subbands 3 through 8
    }
    bits.push(17, 5); // blend
    bits.push(2, 2); // master coordinate
    for (exponent, mantissa) in [(1, 0), (2, 1), (3, 2), (4, 3)] {
        bits.push(exponent, 4);
        bits.push(mantissa, 2);
    }
    bits.push(10, 4); // channel absolute exponent
    for _ in 0..16 {
        bits.push(62, 7); // neutral D15 groups through SPX begin bin 49
    }
    bits.push(1, 2); // gain range
    bits.push(0, 1); // converter SNR offset absent
    let expected_offset = bits.0.len();

    let bytes = bits.bytes(128);
    let prefix = parse_first_audio_block_prefix(&bytes).expect("valid block prefix");
    assert_eq!(prefix.block_switch, vec![true]);
    assert_eq!(prefix.dither, vec![true]);
    assert_eq!(prefix.dynamic_range, Some(0xa5));
    assert_eq!(prefix.dynamic_range_2, None);
    let spx = prefix.spectral_extension.expect("SPX state");
    assert_eq!(spx.channel_in_use, vec![true]);
    assert_eq!(spx.start_copy_frequency_code, 2);
    assert_eq!((spx.begin_subband, spx.end_subband), (2, 9));
    assert_eq!(spx.band_count, 4);
    let coordinate = spx.coordinates[0].as_ref().expect("channel coordinate");
    assert_eq!(coordinate.blend, 17);
    assert_eq!(coordinate.master, 2);
    assert_eq!(coordinate.bands, vec![(1, 0), (2, 1), (3, 2), (4, 3)]);
    assert_eq!(prefix.channel_bandwidth_codes, vec![None]);
    assert!(prefix.snr_offsets.is_none());
    let exponents = prefix.channel_exponents[0]
        .as_ref()
        .expect("channel exponents");
    assert_eq!((exponents.start_mantissa, exponents.end_mantissa), (0, 49));
    assert_eq!(exponents.decoded, vec![10; 49]);
    assert_eq!(exponents.gain_range, Some(1));
    assert_eq!(prefix.next_offset_bits, expected_offset);

    let decoded =
        decode_first_audio_block(&bytes, &[0.5; 49]).expect("dithered zero-bit mantissas");
    assert_eq!(decoded.channel_baps[0], vec![0; 49]);
    assert_eq!(
        decoded.channel_mantissas[0][0],
        (0.5 / 1024.0) * dynamic_range_gain(Some(0xa5))
    );
    assert_eq!(decoded.mantissa_end_offset_bits, expected_offset);
    let pcm = synthesize_audio_blocks(&[decoded]).expect("block-switched PCM synthesis");
    assert_eq!(pcm.channels.len(), 1);
    assert_eq!(pcm.channels[0].len(), 256);
    assert!(pcm.channels[0].iter().all(|sample| sample.is_finite()));
}

#[test]
fn codec_core_policy_disables_only_optional_dynamic_range_gain() {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 2); // independent
    bits.push(0, 3);
    bits.push(255, 11); // 512-byte frame
    bits.push(0, 2); // 48 kHz
    bits.push(0, 2); // one block
    bits.push(1, 3); // mono
    bits.push(0, 1); // no LFE
    bits.push(16, 5);
    bits.push(31, 5);
    bits.push(0, 1); // compre
    bits.push(0, 1); // mixmdate
    bits.push(0, 1); // infomdate
    bits.push(0, 1); // convsync
    bits.push(0, 1); // addbsie
    bits.push(0, 2); // frame SNR strategy
    bits.push(0, 1); // transient processing
    bits.push(1, 1); // block-switch syntax
    bits.push(0, 1); // dither syntax disabled
    bits.push(0, 5); // remaining syntax flags
    bits.push(1, 2); // channel D15
    bits.push(0, 1); // converter exponent strategy absent
    bits.push(0, 10); // frame SNR offsets
    bits.push(1, 1); // block switch
    bits.push(1, 1); // dynamic range present
    bits.push(0x60, 8);
    bits.push(0, 1); // no SPX
    bits.push(0, 6); // channel bandwidth: 73 bins
    bits.push(15, 4); // channel absolute exponent
    for _ in 0..24 {
        bits.push(62, 7);
    }
    bits.push(0, 2); // gain range
    bits.push(0, 1); // converter SNR offset absent
    let bytes = bits.bytes(512);
    let dither = [0.5; 73];
    let current =
        decode_audio_blocks_with_policy(&bytes, &dither, InternalBasePolicy::CurrentDefault)
            .expect("current policy");
    let core = decode_audio_blocks_with_policy(&bytes, &dither, InternalBasePolicy::CodecCore)
        .expect("codec-core policy");
    assert_eq!(current[0].prefix.dynamic_range, Some(0x60));
    let source = core[0].channel_mantissas[0][0];
    assert_ne!(source, 0.0);
    assert_eq!(
        current[0].channel_mantissas[0][0],
        source * dynamic_range_gain(Some(0x60))
    );
    assert_ne!(current[0].channel_mantissas, core[0].channel_mantissas);
    assert_eq!(current[0].channel_baps, core[0].channel_baps);
    assert_eq!(
        current[0].prefix.channel_exponents,
        core[0].prefix.channel_exponents
    );
}

#[test]
fn decodes_following_audio_block_with_normative_reuse_state() {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 2); // independent
    bits.push(0, 3);
    bits.push(255, 11); // 512-byte frame
    bits.push(0, 2); // 48 kHz
    bits.push(1, 2); // two blocks
    bits.push(1, 3); // mono
    bits.push(0, 1); // no LFE
    bits.push(16, 5);
    bits.push(31, 5);
    bits.push(0, 1); // compre
    bits.push(0, 1); // mixmdate
    bits.push(0, 1); // infomdate
    bits.push(0, 1); // convsync
    bits.push(0, 1); // addbsie

    bits.push(0, 2); // frame SNR strategy
    bits.push(0, 1); // transient processing
    bits.push(0, 7); // compact syntax flags
    bits.push(1, 2); // block 0 channel D15
    bits.push(0, 2); // block 1 channel reuse
    bits.push(0, 1); // converter exponent strategy absent
    bits.push(0, 6); // frame coarse SNR offset
    bits.push(0, 4); // frame fine SNR offset
    bits.push(0, 1); // no block-start information

    // Block 0: all conventional side information, no SPX, high exponents,
    // default allocation, and no mantissa bits allocated. A non-unity
    // dynamic-range word is reused by block 1.
    bits.push(1, 1); // dynamic range present
    bits.push(0x60, 8); // +18.06 dB arithmetic-shift term
    bits.push(0, 1); // SPX not in use
    bits.push(0, 6); // channel bandwidth code: end mantissa 73
    bits.push(15, 4); // channel absolute exponent
    for _ in 0..24 {
        bits.push(62, 7);
    }
    bits.push(0, 2); // gain range
    bits.push(0, 1); // converter SNR offset absent

    // Block 1: dynamic range absent, SPX strategy reused, exponent and
    // channel bandwidth state reused, then only converter syntax remains.
    bits.push(0, 1); // dynamic range absent
    bits.push(0, 1); // spxstre = 0, reuse previous SPX state
    bits.push(0, 1); // converter SNR offset absent

    let bytes = bits.bytes(512);
    let blocks = decode_audio_blocks(&bytes, &[0.5; 146]).expect("two-block conventional frame");
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].block_index, 0);
    assert_eq!(blocks[1].block_index, 1);
    assert_eq!(blocks[0].prefix.dynamic_range, Some(0x60));
    assert_eq!(blocks[1].prefix.dynamic_range, None);
    assert_eq!(
        blocks[0].channel_mantissas[0][0],
        blocks[1].channel_mantissas[0][0]
    );
    assert_eq!(blocks[0].channel_baps[0], vec![0; 73]);
    assert_eq!(blocks[1].channel_baps[0], vec![0; 73]);
    assert_eq!(
        blocks[1].prefix.channel_exponents[0],
        blocks[0].prefix.channel_exponents[0]
    );
    assert!(blocks[1].prefix.channel_bandwidth_codes[0].is_none());
    assert!(blocks[1].prefix.spectral_extension.is_none());
    assert!(blocks[1].mantissa_end_offset_bits > blocks[0].mantissa_end_offset_bits);

    let pcm = synthesize_audio_blocks(&blocks).expect("two-block PCM synthesis");
    assert_eq!(pcm.channels.len(), 1);
    assert_eq!(pcm.channels[0].len(), 512);
    assert!(pcm.lfe.is_none());
    assert!(pcm.channels[0].iter().all(|sample| sample.is_finite()));

    let mut direct_synthesizer = AudioPcmSynthesizer::new();
    let direct = decode_audio_frame_pcm(&bytes, &[0.5; 146], &mut direct_synthesizer)
        .expect("direct audio-frame PCM synthesis");
    assert_eq!(direct, pcm);

    let mut stateful = AudioPcmSynthesizer::new();
    let first = stateful
        .synthesize(&blocks)
        .expect("stateful first PCM synthesis");
    let second = stateful
        .synthesize(&blocks)
        .expect("stateful second PCM synthesis");
    assert!(
        first.channels[0]
            .iter()
            .zip(&second.channels[0])
            .any(|(left, right)| left != right)
    );
    stateful.reset();
    assert_eq!(
        stateful
            .synthesize(&blocks)
            .expect("stateful reset PCM synthesis"),
        first
    );

    let mut expected_state = AudioPcmSynthesizer::new();
    expected_state
        .synthesize(&blocks)
        .expect("expected state seed");
    let expected_after_failure = expected_state
        .synthesize(&blocks)
        .expect("expected state continuation");
    let mut malformed = blocks.clone();
    malformed[1].channel_mantissas.clear();
    let mut atomic_state = AudioPcmSynthesizer::new();
    atomic_state.synthesize(&blocks).expect("atomic state seed");
    assert!(atomic_state.synthesize(&malformed).is_err());
    assert_eq!(
        atomic_state
            .synthesize(&blocks)
            .expect("atomic state continuation"),
        expected_after_failure
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn parses_first_audio_block_standard_coupling_coordinates() {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 2); // independent
    bits.push(0, 3);
    bits.push(63, 11); // 128-byte frame
    bits.push(0, 2); // 48 kHz
    bits.push(0, 2); // one block
    bits.push(2, 3); // stereo
    bits.push(0, 1); // no LFE
    bits.push(16, 5);
    bits.push(31, 5);
    bits.push(0, 1); // compre
    bits.push(0, 1); // mixmdate
    bits.push(0, 1); // infomdate
    bits.push(0, 1); // convsync
    bits.push(0, 1); // addbsie
    bits.push(0, 2); // frame SNR strategy
    bits.push(0, 1); // transient processing
    bits.push(0, 7); // compact syntax flags
    bits.push(1, 1); // coupling in use
    bits.push(1, 2); // coupling D15
    bits.push(1, 2); // left D15
    bits.push(1, 2); // right D15
    bits.push(0, 1); // converter exponent strategy absent
    bits.push(0, 10); // frame SNR offsets

    bits.push(0, 1); // dynamic range absent
    bits.push(0, 1); // SPX not in use
    bits.push(0, 1); // standard coupling
    bits.push(1, 1); // phase flags in use
    bits.push(0, 4); // coupling begin frequency
    bits.push(2, 4); // coupling end frequency: five subbands
    bits.push(1, 1); // band structure exists
    for value in [false, true, false, true] {
        bits.push(u64::from(value), 1);
    }
    for (master, coordinates) in [
        (1, [(1, 2), (3, 4), (5, 6)]),
        (2, [(7, 8), (9, 10), (11, 12)]),
    ] {
        bits.push(master, 2);
        for (exponent, mantissa) in coordinates {
            bits.push(exponent, 4);
            bits.push(mantissa, 4);
        }
    }
    bits.push(0b101, 3); // one phase flag per coupling band
    bits.push(0b10, 2); // two rematrix flags for standard cplbegf zero
    bits.push(5, 4); // coupling absolute exponent, decoded as 10
    for _ in 0..20 {
        bits.push(62, 7); // 60 coupled mantissas
    }
    for gain_range in [1, 2] {
        bits.push(10, 4);
        for _ in 0..12 {
            bits.push(62, 7); // channel end mantissa is coupling start 37
        }
        bits.push(gain_range, 2);
    }
    bits.push(0, 1); // converter SNR offset absent
    bits.push(3, 3); // first coupling fast leak
    bits.push(5, 3); // first coupling slow leak
    let expected_offset = bits.0.len();

    let bytes = bits.bytes(128);
    let prefix = parse_first_audio_block_prefix(&bytes).expect("standard coupling");
    let coupling = match prefix.coupling.expect("coupling state") {
        CouplingInformation::Standard(value) => value,
        CouplingInformation::Enhanced(_) => panic!("expected standard coupling"),
    };
    assert_eq!(coupling.channel_in_use, vec![true, true]);
    assert!(coupling.phase_flags_in_use);
    assert_eq!(coupling.begin_frequency_code, 0);
    assert_eq!(coupling.end_frequency_code, 2);
    assert_eq!(coupling.subband_count, 5);
    assert_eq!(coupling.band_count, 3);
    let left = coupling.coordinates[0].as_ref().expect("left coordinates");
    assert_eq!(left.master, 1);
    assert_eq!(left.bands, vec![(1, 2), (3, 4), (5, 6)]);
    let right = coupling.coordinates[1].as_ref().expect("right coordinates");
    assert_eq!(right.master, 2);
    assert_eq!(right.bands, vec![(7, 8), (9, 10), (11, 12)]);
    assert_eq!(coupling.phase_flags, vec![true, false, true]);
    assert_eq!(prefix.rematrix_flags, vec![true, false]);
    let coupling_exponents = prefix
        .coupling_exponents
        .as_ref()
        .expect("coupling exponents");
    assert_eq!(
        (
            coupling_exponents.start_mantissa,
            coupling_exponents.end_mantissa
        ),
        (37, 97)
    );
    assert_eq!(coupling_exponents.decoded, vec![10; 60]);
    assert_eq!(
        prefix.channel_exponents[0].as_ref().expect("left").decoded,
        vec![10; 37]
    );
    assert_eq!(
        prefix.channel_exponents[1].as_ref().expect("right").decoded,
        vec![10; 37]
    );
    let leakage = prefix.coupling_leak.expect("coupling leakage");
    assert_eq!((leakage.fast_code, leakage.slow_code), (3, 5));
    assert_eq!(prefix.next_offset_bits, expected_offset);

    let decoded =
        decode_first_audio_block(&bytes, &[0.0; 74]).expect("standard coupling mantissas");
    assert_eq!(
        decoded
            .channel_baps
            .iter()
            .map(Vec::len)
            .collect::<Vec<_>>(),
        vec![37, 37]
    );
    assert_eq!(
        decoded.coupling_bap.as_ref().expect("coupling BAP").len(),
        60
    );
    assert_eq!(
        decoded
            .coupling_mantissas
            .as_ref()
            .expect("coupling mantissas")
            .len(),
        60
    );
    assert_eq!(
        decoded
            .channel_mantissas
            .iter()
            .map(Vec::len)
            .collect::<Vec<_>>(),
        vec![256, 256]
    );
    assert_eq!(decoded.mantissa_end_offset_bits, expected_offset);
}

#[test]
#[allow(clippy::too_many_lines)]
fn parses_first_audio_block_enhanced_coupling_coordinates() {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 2); // independent
    bits.push(0, 3);
    bits.push(127, 11); // 256-byte frame
    bits.push(0, 2); // 48 kHz
    bits.push(0, 2); // one block
    bits.push(3, 3); // three front channels
    bits.push(0, 1); // no LFE
    bits.push(16, 5);
    bits.push(31, 5);
    bits.push(0, 1); // compre
    bits.push(0, 1); // mixmdate
    bits.push(0, 1); // infomdate
    bits.push(0, 1); // convsync
    bits.push(0, 1); // addbsie
    bits.push(0, 2); // frame SNR strategy
    bits.push(0, 1); // transient processing
    bits.push(0, 7); // compact syntax flags
    bits.push(1, 1); // coupling in use
    bits.push(1, 2); // coupling D15
    for _ in 0..3 {
        bits.push(1, 2); // channel D15
    }
    bits.push(0, 1); // converter exponent strategy absent
    bits.push(0, 10); // frame SNR offsets

    bits.push(0, 1); // dynamic range absent
    bits.push(0, 1); // SPX not in use
    bits.push(1, 1); // enhanced coupling
    bits.push(0b101, 3); // channels 0 and 2 participate
    bits.push(3, 4); // begin subband 5
    bits.push(4, 4); // end subband 11
    bits.push(1, 1); // band structure exists
    bits.push(0, 1); // subband 9 starts a band
    bits.push(1, 1); // subband 10 merges: five bands total
    bits.push(0, 1); // leading reserved bit
    for amplitude in [1, 2, 3, 4, 5] {
        bits.push(amplitude, 5); // first participating channel
    }
    for amplitude in [6, 7, 8, 9, 10] {
        bits.push(amplitude, 5); // later participating channel
    }
    bits.push(0, 36); // 9 * (necplbnd - 1) reserved bits
    bits.push(0, 1); // trailing later-channel reserved bit
    bits.push(0, 6); // bandwidth code for uncoupled channel 1
    bits.push(5, 4); // coupling absolute exponent, decoded as 10
    for _ in 0..24 {
        bits.push(62, 7); // enhanced coupling bins 49 through 120
    }
    for (groups, gain_range) in [(16, 0), (24, 1), (16, 2)] {
        bits.push(10, 4);
        for _ in 0..groups {
            bits.push(62, 7);
        }
        bits.push(gain_range, 2);
    }
    bits.push(0, 1); // converter SNR offset absent
    bits.push(2, 3); // first coupling fast leak
    bits.push(6, 3); // first coupling slow leak
    let expected_offset = bits.0.len();

    let bytes = bits.clone().bytes(256);
    let prefix = parse_first_audio_block_prefix(&bytes).expect("enhanced coupling");
    let coupling = match prefix.coupling.expect("coupling state") {
        CouplingInformation::Enhanced(value) => value,
        CouplingInformation::Standard(_) => panic!("expected enhanced coupling"),
    };
    assert_eq!(coupling.channel_in_use, vec![true, false, true]);
    assert_eq!(coupling.begin_subband, 5);
    assert_eq!(coupling.end_subband, 11);
    assert_eq!(coupling.band_count, 5);
    assert_eq!(coupling.amplitudes[0], Some(vec![1, 2, 3, 4, 5]));
    assert_eq!(coupling.amplitudes[1], None);
    assert_eq!(coupling.amplitudes[2], Some(vec![6, 7, 8, 9, 10]));
    assert_eq!(prefix.channel_bandwidth_codes, vec![None, Some(0), None]);
    let coupling_exponents = prefix
        .coupling_exponents
        .as_ref()
        .expect("coupling exponents");
    assert_eq!(
        (
            coupling_exponents.start_mantissa,
            coupling_exponents.end_mantissa
        ),
        (49, 121)
    );
    assert_eq!(coupling_exponents.decoded, vec![10; 72]);
    assert_eq!(
        prefix.channel_exponents[0].as_ref().expect("left").decoded,
        vec![10; 49]
    );
    assert_eq!(
        prefix.channel_exponents[1]
            .as_ref()
            .expect("centre")
            .decoded,
        vec![10; 73]
    );
    assert_eq!(
        prefix.channel_exponents[2].as_ref().expect("right").decoded,
        vec![10; 49]
    );
    let leakage = prefix.coupling_leak.expect("coupling leakage");
    assert_eq!((leakage.fast_code, leakage.slow_code), (2, 6));
    assert_eq!(prefix.next_offset_bits, expected_offset);

    let decoded =
        decode_first_audio_block(&bytes, &[0.0; 256]).expect("enhanced coupling mantissas");
    let reconstructed = decoded
        .enhanced_coupling
        .expect("enhanced coupling coefficients");
    assert_eq!(
        (reconstructed.begin_mantissa, reconstructed.end_mantissa),
        (49, 121)
    );
    assert_eq!(reconstructed.channels[0].as_ref().expect("left").len(), 72);
    assert!(reconstructed.channels[1].is_none());
    assert_eq!(reconstructed.channels[2].as_ref().expect("right").len(), 72);
    assert_eq!(
        decoded
            .channel_mantissas
            .iter()
            .map(Vec::len)
            .collect::<Vec<_>>(),
        vec![256, 256, 256]
    );
}

#[test]
fn reconstructs_enhanced_coupling_coefficients_from_band_amplitudes() {
    let coupling = openjoc_eac3::EnhancedCouplingInformation {
        channel_in_use: vec![true, false, true],
        begin_frequency_code: 3,
        begin_subband: 5,
        end_subband: 8,
        band_structure: {
            let mut value = [false; 22];
            value[6] = true;
            value
        },
        band_count: 2,
        amplitudes: vec![Some(vec![0, 31]), None, Some(vec![1, 5])],
    };
    let coupling_mantissas = vec![1.0; 36];

    let reconstructed = reconstruct_enhanced_coupling(&coupling, &coupling_mantissas)
        .expect("enhanced coupling reconstruction");

    assert_eq!(
        (reconstructed.begin_mantissa, reconstructed.end_mantissa),
        (49, 85)
    );
    let channel0 = reconstructed.channels[0].as_ref().expect("channel 0");
    assert_eq!(&channel0[..24], vec![1.0; 24].as_slice());
    assert_eq!(&channel0[24..], vec![0.0; 12].as_slice());
    let channel2 = reconstructed.channels[2].as_ref().expect("channel 2");
    assert_eq!(&channel2[..24], vec![27.0 / 32.0; 24].as_slice());
    assert_eq!(&channel2[24..], vec![27.0 / 64.0; 12].as_slice());
    assert!(reconstructed.channels[1].is_none());
}

#[test]
#[allow(clippy::too_many_lines)]
fn parses_uncoupled_channel_and_lfe_exponents() {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 2); // independent
    bits.push(0, 3);
    bits.push(127, 11); // 256-byte frame
    bits.push(0, 2); // 48 kHz
    bits.push(0, 2); // one block
    bits.push(1, 3); // mono
    bits.push(1, 1); // LFE
    bits.push(16, 5);
    bits.push(31, 5);
    bits.push(0, 1); // compre
    bits.push(0, 1); // mixmdate
    bits.push(0, 1); // infomdate
    bits.push(0, 1); // convsync
    bits.push(0, 1); // addbsie
    bits.push(2, 2); // per-element block SNR strategy
    bits.push(0, 1); // transient processing
    bits.push(0, 1); // block-switch syntax
    bits.push(0, 1); // dither syntax
    bits.push(1, 1); // bit-allocation syntax
    bits.push(1, 1); // frame fast-gain syntax
    bits.push(1, 1); // delta-bit-allocation syntax
    bits.push(1, 1); // skip-field syntax
    bits.push(0, 1); // SPX attenuation syntax
    bits.push(1, 2); // channel D15
    bits.push(1, 1); // LFE D15
    bits.push(0, 1); // converter exponent strategy absent

    bits.push(0, 1); // dynamic range absent
    bits.push(0, 1); // SPX not in use
    bits.push(0, 6); // channel bandwidth code: end mantissa 73
    bits.push(10, 4); // channel absolute exponent
    for _ in 0..24 {
        bits.push(62, 7);
    }
    bits.push(3, 2); // channel gain range
    bits.push(8, 4); // LFE absolute exponent
    bits.push(62, 7);
    bits.push(62, 7);
    bits.push(1, 1); // new bit-allocation parameters
    bits.push(3, 2); // slow decay
    bits.push(2, 2); // fast decay
    bits.push(1, 2); // slow gain
    bits.push(0, 2); // dB per bit
    bits.push(5, 3); // floor
    bits.push(33, 6); // coarse SNR offset
    bits.push(5, 4); // channel fine SNR offset
    bits.push(7, 4); // LFE fine SNR offset
    bits.push(1, 1); // new fast-gain codes
    bits.push(3, 3); // channel fast gain
    bits.push(6, 3); // LFE fast gain
    bits.push(1, 1); // converter SNR offset exists
    bits.push(0x155, 10); // converter SNR offset
    bits.push(1, 1); // delta-bit-allocation information exists
    bits.push(1, 2); // channel: new information follows
    bits.push(1, 3); // two channel delta segments
    for (offset, length, delta) in [(3, 4, 5), (17, 9, 2)] {
        bits.push(offset, 5);
        bits.push(length, 4);
        bits.push(delta, 3);
    }
    bits.push(1, 1); // skip length exists
    bits.push(2, 9); // two skipped bytes
    let expected_skip_start = bits.0.len();
    bits.push(0xabcd, 16); // skipped data
    let expected_offset = bits.0.len();

    let bytes = bits.bytes(256);
    let prefix = parse_first_audio_block_prefix(&bytes).expect("channel and LFE exponents");
    assert_eq!(prefix.channel_bandwidth_codes, vec![Some(0)]);
    let channel = prefix.channel_exponents[0]
        .as_ref()
        .expect("channel exponents");
    assert_eq!((channel.start_mantissa, channel.end_mantissa), (0, 73));
    assert_eq!(channel.decoded, vec![10; 73]);
    assert_eq!(channel.gain_range, Some(3));
    let lfe = prefix.lfe_exponents.as_ref().expect("LFE exponents");
    assert_eq!((lfe.start_mantissa, lfe.end_mantissa), (0, 7));
    assert_eq!(lfe.decoded, vec![8; 7]);
    assert_eq!(lfe.gain_range, None);
    let bit_allocation = prefix
        .bit_allocation_parameters
        .expect("bit-allocation parameters");
    assert_eq!(bit_allocation.slow_decay_code, 3);
    assert_eq!(bit_allocation.fast_decay_code, 2);
    assert_eq!(bit_allocation.slow_gain_code, 1);
    assert_eq!(bit_allocation.db_per_bit_code, 0);
    assert_eq!(bit_allocation.floor_code, 5);
    let snr = prefix.snr_offsets.expect("SNR offsets");
    assert_eq!(snr.coarse_code, 33);
    assert_eq!(snr.coupling_fine_code, None);
    assert_eq!(snr.channel_fine_codes, vec![5]);
    assert_eq!(snr.lfe_fine_code, Some(7));
    let fast_gain = prefix.fast_gain_codes.expect("fast-gain codes");
    assert_eq!(fast_gain.coupling, None);
    assert_eq!(fast_gain.channels, vec![3]);
    assert_eq!(fast_gain.lfe, Some(6));
    assert_eq!(prefix.converter_snr_offset, Some(0x155));
    let delta = prefix.delta_bit_allocation.expect("delta allocation");
    assert_eq!(delta.coupling, None);
    assert_eq!(delta.channels[0].strategy, 1);
    assert_eq!(delta.channels[0].segments.len(), 2);
    assert_eq!(
        (
            delta.channels[0].segments[0].offset,
            delta.channels[0].segments[0].length,
            delta.channels[0].segments[0].delta
        ),
        (3, 4, 5)
    );
    assert_eq!(
        (
            delta.channels[0].segments[1].offset,
            delta.channels[0].segments[1].length,
            delta.channels[0].segments[1].delta
        ),
        (17, 9, 2)
    );
    assert_eq!(
        prefix.skip_field,
        Some(openjoc_eac3::AuxiliaryData {
            bit_len: 16,
            bytes: vec![0xab, 0xcd],
        })
    );
    assert_eq!(
        prefix.skip_field_start_offset_bits,
        Some(expected_skip_start)
    );
    assert_eq!(prefix.next_offset_bits, expected_offset);

    let decoded =
        decode_first_audio_block(&bytes, &[0.0; 73]).expect("conventional first-block mantissas");
    assert_eq!(decoded.channel_baps[0].len(), 73);
    assert_eq!(decoded.channel_mantissas[0].len(), 73);
    assert_eq!(decoded.lfe_bap.as_ref().expect("LFE BAP").len(), 7);
    assert_eq!(
        decoded.lfe_mantissas.as_ref().expect("LFE mantissas").len(),
        7
    );
    assert!(decoded.mantissa_end_offset_bits > decoded.prefix.next_offset_bits);
}

#[test]
fn parses_six_block_coupling_lfe_converter_and_optional_frame_data() {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 2);
    bits.push(0, 3);
    bits.push(127, 11); // 256-byte frame
    bits.push(0, 2);
    bits.push(3, 2); // six blocks
    bits.push(7, 3); // 3/2 mode, five full-bandwidth channels
    bits.push(1, 1); // LFE
    bits.push(16, 5);
    bits.push(31, 5);
    bits.push(0, 1); // compre
    bits.push(0, 1); // mixmdate
    bits.push(0, 1); // infomdate
    bits.push(0, 1); // addbsie

    bits.push(1, 1); // per-block exponent strategies
    bits.push(0, 1); // no AHT
    bits.push(2, 2); // channel-specific SNR strategy
    bits.push(1, 1); // transient processing
    bits.push(0, 1); // block switch syntax
    bits.push(1, 1); // dither syntax
    bits.push(1, 1); // bit allocation syntax
    bits.push(0, 1); // frame fast gain syntax
    bits.push(1, 1); // delta bit allocation syntax
    bits.push(1, 1); // skip field syntax
    bits.push(1, 1); // SPX attenuation syntax
    bits.push(1, 1); // coupling in block 0
    for _ in 1..6 {
        bits.push(0, 1); // reuse coupling-in-use state
    }
    for block in 0..6 {
        bits.push(u64::from(block == 0), 2); // coupling D15 then reuse
        for _ in 0..5 {
            bits.push(u64::from(block == 0), 2); // channel D15 then reuse
        }
    }
    bits.push(0b10_0000, 6); // LFE D15 then reuse
    for _ in 0..5 {
        bits.push(0, 5); // converter frame strategy
    }
    bits.push(1, 1); // channel 0 transient data
    bits.push(341, 10);
    bits.push(85, 8);
    for _ in 1..5 {
        bits.push(0, 1);
    }
    bits.push(1, 1); // channel 0 SPX attenuation
    bits.push(17, 5);
    for _ in 1..5 {
        bits.push(0, 1);
    }
    bits.push(1, 1); // block start information present
    bits.push((1_u64 << 55) - 1, 55);
    let expected_offset = bits.0.len();
    let bytes = bits.bytes(256);

    let frame = parse_audio_frame(&bytes).expect("complete six-block frame state");
    assert_eq!(frame.full_bandwidth_channels, 5);
    assert_eq!(frame.coupling_in_use, [true; 6]);
    assert_eq!(frame.coupling_exponent_strategy, vec![1, 0, 0, 0, 0, 0]);
    assert_eq!(frame.channel_exponent_strategy[0], [1; 5]);
    assert!(
        frame.channel_exponent_strategy[1..]
            .iter()
            .all(|strategies| strategies == &[0; 5])
    );
    assert_eq!(
        frame.lfe_exponent_strategy,
        [true, false, false, false, false, false]
    );
    assert_eq!(
        frame.spx_attenuation_codes,
        vec![Some(17), None, None, None, None]
    );
    assert_eq!(
        frame.block_start_information,
        Some(openjoc_eac3::AuxiliaryData {
            bit_len: 55,
            bytes: vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xfe],
        })
    );
    assert_eq!(frame.audio_blocks_offset_bits, expected_offset);
}

#[test]
fn decodes_every_frame_exponent_strategy_table_row() {
    let rows = [
        [1, 0, 0, 0, 0, 0],
        [1, 0, 0, 0, 0, 3],
        [1, 0, 0, 0, 2, 0],
        [1, 0, 0, 0, 3, 3],
        [2, 0, 0, 2, 0, 0],
        [2, 0, 0, 2, 0, 3],
        [2, 0, 0, 3, 2, 0],
        [2, 0, 0, 3, 3, 3],
        [2, 0, 1, 0, 0, 0],
        [2, 0, 2, 0, 0, 3],
        [2, 0, 2, 0, 2, 0],
        [2, 0, 2, 0, 3, 3],
        [2, 0, 3, 2, 0, 0],
        [2, 0, 3, 2, 0, 3],
        [2, 0, 3, 3, 2, 0],
        [2, 0, 3, 3, 3, 3],
        [3, 1, 0, 0, 0, 0],
        [3, 1, 0, 0, 0, 3],
        [3, 2, 0, 0, 2, 0],
        [3, 2, 0, 0, 3, 3],
        [3, 2, 0, 2, 0, 0],
        [3, 2, 0, 2, 0, 3],
        [3, 2, 0, 3, 2, 0],
        [3, 2, 0, 3, 3, 3],
        [3, 3, 1, 0, 0, 0],
        [3, 3, 2, 0, 0, 3],
        [3, 3, 2, 0, 2, 0],
        [3, 3, 2, 0, 3, 3],
        [3, 3, 3, 2, 0, 0],
        [3, 3, 3, 2, 0, 3],
        [3, 3, 3, 3, 2, 0],
        [3, 3, 3, 3, 3, 3],
    ];
    for (code, expected) in rows.into_iter().enumerate() {
        assert_eq!(
            decode_frame_exponent_strategy(u8::try_from(code).expect("code")),
            Ok(expected)
        );
    }
    assert_eq!(
        decode_frame_exponent_strategy(32),
        Err(Eac3Error::InvalidFrameExponentStrategy { actual: 32 })
    );
}

#[test]
fn derives_six_block_channel_strategies_from_frame_code() {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 2);
    bits.push(0, 3);
    bits.push(63, 11);
    bits.push(0, 2);
    bits.push(3, 2);
    bits.push(1, 3); // mono: no coupling syntax
    bits.push(0, 1);
    bits.push(16, 5);
    bits.push(31, 5);
    bits.push(0, 1); // compre
    bits.push(0, 1); // mixmdate
    bits.push(0, 1); // infomdate
    bits.push(0, 1); // addbsie

    bits.push(0, 1); // frame-based exponent strategy
    bits.push(0, 1); // no AHT
    let snr_position = bits.0.len();
    bits.push(2, 2); // channel-specific SNR strategy
    bits.push(0, 7); // transient through skip-field syntax disabled
    bits.push(0, 1); // SPX attenuation
    bits.push(30, 5); // channel frame strategy
    bits.push(0, 5); // converter frame strategy
    bits.push(0, 1); // no block-start information
    let expected_offset = bits.0.len();
    let mut reserved = bits.clone();
    reserved.set(snr_position, 3, 2);
    let bytes = bits.bytes(128);

    let frame = parse_audio_frame(&bytes).expect("frame-based strategies");
    assert_eq!(
        frame.channel_exponent_strategy,
        vec![vec![3], vec![3], vec![3], vec![3], vec![2], vec![0]]
    );
    assert_eq!(frame.audio_blocks_offset_bits, expected_offset);
    assert_eq!(
        parse_audio_frame(&reserved.bytes(128)),
        Err(Eac3Error::ReservedSnrOffsetStrategy)
    );
}

#[test]
fn computes_the_normative_block_start_information_length() {
    assert_eq!(block_start_information_length(128, 1), Ok(0));
    assert_eq!(block_start_information_length(128, 6), Ok(50));
    assert_eq!(block_start_information_length(130, 6), Ok(55));
    assert_eq!(block_start_information_length(256, 3), Ok(22));
    assert_eq!(
        block_start_information_length(0, 6),
        Err(Eac3Error::InvalidBlockStartDimensions {
            frame_size: 0,
            audio_blocks: 6,
        })
    );
    assert_eq!(
        block_start_information_length(128, 4),
        Err(Eac3Error::InvalidBlockStartDimensions {
            frame_size: 128,
            audio_blocks: 4,
        })
    );
}

#[test]
fn derives_channel_mantissa_and_exponent_group_counts() {
    assert_eq!(channel_end_mantissa(0), Ok(73));
    assert_eq!(channel_end_mantissa(60), Ok(253));
    assert_eq!(
        channel_end_mantissa(61),
        Err(Eac3Error::InvalidChannelBandwidthCode { actual: 61 })
    );

    assert_eq!(channel_exponent_group_count(73, 1), Ok(24));
    assert_eq!(channel_exponent_group_count(73, 2), Ok(12));
    assert_eq!(channel_exponent_group_count(73, 3), Ok(6));
    assert_eq!(
        channel_exponent_group_count(73, 0),
        Err(Eac3Error::InvalidExponentStrategy { actual: 0 })
    );
}

#[test]
fn decodes_grouped_exponents_for_every_strategy() {
    assert_eq!(decode_exponents(10, &[62, 62], 1, 7), Ok(vec![10; 7]));
    assert_eq!(decode_exponents(10, &[62], 2, 7), Ok(vec![10; 7]));
    assert_eq!(decode_exponents(10, &[62], 3, 13), Ok(vec![10; 13]));

    assert_eq!(decode_exponents(6, &[0], 1, 4), Ok(vec![6, 4, 2, 0]));
    assert_eq!(decode_exponents(0, &[124], 1, 4), Ok(vec![0, 2, 4, 6]));
}

#[test]
fn rejects_malformed_grouped_exponents() {
    assert_eq!(
        decode_exponents(10, &[], 1, 0),
        Err(Eac3Error::InvalidExponentDimensions { end_mantissa: 0 })
    );
    assert_eq!(
        decode_exponents(10, &[125], 1, 4),
        Err(Eac3Error::InvalidGroupedExponent { actual: 125 })
    );
    assert_eq!(
        decode_exponents(1, &[0], 1, 4),
        Err(Eac3Error::ExponentOutOfRange { actual: -1 })
    );
    assert_eq!(
        decode_exponents(10, &[], 1, 4),
        Err(Eac3Error::ExponentGroupCountMismatch {
            expected: 1,
            actual: 0,
        })
    );
}

#[test]
fn derives_every_spectral_extension_subband_boundary() {
    let expected_begin = [2, 3, 4, 5, 6, 7, 9, 11];
    let expected_end = [5, 6, 7, 9, 11, 13, 15, 17];
    for (begin_code, &begin) in expected_begin.iter().enumerate() {
        for (end_code, &end) in expected_end.iter().enumerate() {
            let begin_code = u8::try_from(begin_code).expect("three-bit begin code");
            let end_code = u8::try_from(end_code).expect("three-bit end code");
            let actual = spx_subband_range(begin_code, end_code);
            if begin < end {
                assert_eq!(actual, Ok((begin, end)));
            } else {
                assert_eq!(
                    actual,
                    Err(Eac3Error::InvalidSpectralExtensionRange { begin, end })
                );
            }
        }
    }
}

#[test]
fn rejects_spectral_extension_codes_wider_than_the_normative_fields() {
    assert_eq!(
        spx_subband_range(8, 0),
        Err(Eac3Error::InvalidSpectralExtensionCode {
            begin_code: 8,
            end_code: 0,
        })
    );
    assert_eq!(
        spx_subband_range(0, 8),
        Err(Eac3Error::InvalidSpectralExtensionCode {
            begin_code: 0,
            end_code: 8,
        })
    );
}

#[test]
fn parses_aht_flags_only_for_single_exponent_regions() {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 2);
    bits.push(0, 3);
    bits.push(63, 11);
    bits.push(0, 2);
    bits.push(3, 2);
    bits.push(1, 3); // mono
    bits.push(0, 1);
    bits.push(16, 5);
    bits.push(31, 5);
    bits.push(0, 1); // compre
    bits.push(0, 1); // mixmdate
    bits.push(0, 1); // infomdate
    bits.push(0, 1); // addbsie

    bits.push(1, 1); // per-block exponent strategies
    bits.push(1, 1); // AHT syntax
    bits.push(1, 2); // SNR strategy
    bits.push(0, 8); // frame syntax flags
    for block in 0..6 {
        bits.push(u64::from(block == 0), 2);
    }
    bits.push(0, 5); // converter exponent strategy
    bits.push(1, 1); // mono channel uses AHT (one exponent region)
    bits.push(0, 1); // no block-start information
    let expected_offset = bits.0.len();
    let bytes = bits.bytes(128);

    let frame = parse_audio_frame(&bytes).expect("AHT frame flags");
    assert!(!frame.coupling_aht_in_use);
    assert_eq!(frame.channel_aht_in_use, [true]);
    assert!(!frame.lfe_aht_in_use);
    assert_eq!(frame.audio_blocks_offset_bits, expected_offset);
}

#[test]
fn decodes_aht_channel_across_all_six_blocks_without_repeating_payload() {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 2); // independent
    bits.push(0, 3); // substream id
    bits.push(1023, 11); // 2048-byte frame
    bits.push(0, 2); // 48 kHz
    bits.push(3, 2); // six audio blocks
    bits.push(1, 3); // mono
    bits.push(0, 1); // no LFE
    bits.push(16, 5); // bitstream id
    bits.push(0, 5); // dialnorm
    bits.push(0, 1); // no compression metadata
    bits.push(0, 1); // no mixing metadata
    bits.push(0, 1); // no informational metadata
    bits.push(0, 1); // no addbsi

    bits.push(1, 1); // per-block exponent strategies
    bits.push(1, 1); // AHT syntax
    bits.push(1, 2); // SNR strategy 1
    bits.push(0, 1); // transient processing disabled
    bits.push(16, 7); // bit-allocation syntax enabled (syntax bit index 2)
    for block in 0..6 {
        bits.push(u64::from(block == 0), 2); // D15 then reuse
    }
    bits.push(0, 5); // converter exponent strategy
    bits.push(1, 1); // mono channel uses AHT
    bits.push(0, 1); // no block-start information

    // First audio block prefix: no SPX, 73 channel bins, exponents step from
    // 15 through 17, then remain constant,
    // explicit allocation parameters, and no converter SNR offset.
    bits.push(0, 1); // dynamic range absent
    bits.push(0, 1); // no SPX
    bits.push(0, 6); // chbwcod = 0 => endmant = 73
    bits.push(15, 4); // initial exponent
    bits.push(87, 7); // deltas +1, 0, 0
    bits.push(87, 7); // deltas +1, 0, 0
    for _ in 0..22 {
        bits.push(62, 7); // grouped exponent deltas 0, 0, 0
    }
    bits.push(0, 2); // gain range
    bits.push(1, 1); // new bit-allocation parameters
    bits.push(0, 11); // all parameter codes zero
    bits.push(63, 6); // coarse SNR
    bits.push(15, 4); // fine SNR
    bits.push(0, 1); // converter SNR offset absent
    bits.push(1, 2); // AHT mode 1, one-bit GAQ gains for hebap 8..11
    bits.push(0, 1); // hebap 9 gain 1
    bits.push(1, 1); // hebap 8 gain 2
    bits.push(0, 1); // hebap 8 gain 1
    bits.push(1, 1); // hebap 8 gain 2

    // With this exponent/SNR combination the normative hebap sequence starts
    // [9, 8, 8, 8, 4, ...].  The first four bins carry six GAQ symbols,
    // followed by 69 five-bit VQ indices.  Zero indices still reconstruct a
    // non-zero ETSI Table E.3 vector, exercising both integrated paths.
    for width in [4_u8; 6] {
        bits.push(0, width); // hebap 9, gain 1
    }
    for width in [2_u8; 6] {
        bits.push(0, width); // hebap 8, gain 2
    }
    for width in [3_u8; 6] {
        bits.push(0, width); // hebap 8, gain 1
    }
    for width in [2_u8; 6] {
        bits.push(0, width); // hebap 8, gain 2
    }
    for _ in 0..69 {
        bits.push(0, 5);
    }

    // Following block prefixes reuse exponents and SNR offsets.  The AHT
    // mantissa payload must not be repeated after block zero.
    for _ in 1..6 {
        bits.push(0, 1); // dynamic range absent
        bits.push(0, 1); // SPX strategy reused
        bits.push(0, 6); // chbwcod remains explicit in this traversal
        bits.push(0, 1); // bit-allocation parameters reused
        bits.push(0, 1); // block fine SNR offset absent
        bits.push(0, 1); // converter SNR offset absent
    }

    let bytes = bits.bytes(2048);
    let blocks = decode_audio_blocks(&bytes, &[]).expect("AHT channel traversal");
    assert_eq!(blocks.len(), 6);
    assert_eq!(
        blocks[0].channel_aht[0].as_ref().map(|info| info.mode),
        Some(1)
    );
    assert_eq!(
        blocks[0].channel_aht[0]
            .as_ref()
            .map(|info| (&info.gain_words, &info.gains)),
        Some((&vec![0, 1, 0, 1], &vec![1, 2, 1, 2]))
    );
    assert!(
        blocks[1..]
            .iter()
            .all(|block| block.channel_aht[0].is_none())
    );
    for block in blocks {
        assert_eq!(block.channel_baps[0].len(), 73);
        assert_eq!(&block.channel_baps[0][..5], &[9, 8, 8, 8, 4]);
        assert!(
            block.channel_mantissas[0]
                .iter()
                .any(|mantissa| mantissa.abs() > 1.0e-12)
        );
    }
}

#[test]
fn aht_production_path_matches_an_independent_six_point_oracle() {
    let bytes = six_block_mono_frame(0, None, None);
    let parsed = parse_audio_frame(&bytes).expect("AHT frame syntax");
    let direct = decode_audio_blocks(&bytes, &[]).expect("direct AHT traversal");
    let repeated = decode_audio_blocks(&bytes, &[]).expect("repeated AHT traversal");
    let preparsed = decode_audio_blocks_with_parsed_frame(
        &bytes,
        &parsed,
        &[],
        InternalBasePolicy::CurrentDefault,
    )
    .expect("pre-parsed AHT traversal");
    assert_eq!(direct, repeated);
    assert_eq!(direct, preparsed);

    // The fifth spectral bin has hebap 4 and transmitted VQ index zero in the
    // synthetic frame. The following six words are independently transcribed
    // from TS 102 366 V1.4.1 Table E.3.4, then passed through the printed
    // E.2.4.5 inverse-DCT equation without calling an AHT production helper.
    let input = [0x5903_u16, 0x15c0, 0xe9e6, 0xff64, 0xfe06, 0xffdf]
        .map(|word| f64::from(i16::from_be_bytes(word.to_be_bytes())) / 32768.0);
    let mut coefficients = [0.0; 6];
    for (block, coefficient) in coefficients.iter_mut().enumerate() {
        let mut sum = 0.0;
        for (transform, value) in input.iter().enumerate() {
            let r = if transform == 0 {
                1.0 / 2.0_f64.sqrt()
            } else {
                1.0
            };
            let angle = (transform * (2 * block + 1)) as f64 * core::f64::consts::PI / 12.0;
            sum += r * value * angle.cos();
        }
        *coefficient = 2.0_f64.sqrt() * sum;
    }
    let exponent = direct[0].prefix.channel_exponents[0]
        .as_ref()
        .expect("first-block channel exponents")
        .decoded[4];
    let expected = coefficients.map(|value| value / 2_f64.powi(i32::from(exponent)));
    for (block, expected) in direct.iter().zip(expected) {
        let actual = block.channel_mantissas[0][4];
        assert!((actual - expected).abs() <= 1.0e-12);
    }
    assert!(
        direct
            .windows(2)
            .any(|pair| pair[0].channel_mantissas[0][4] != pair[1].channel_mantissas[0][4])
    );
}

#[test]
fn aht_enablement_selects_a_distinct_production_reconstruction_path() {
    let enabled_bytes = six_block_mono_frame_with_aht(0, None, None, true);
    let disabled_bytes = six_block_mono_frame_with_aht(0, None, None, false);
    let enabled_frame = parse_audio_frame(&enabled_bytes).expect("AHT-enabled frame syntax");
    let disabled_frame = parse_audio_frame(&disabled_bytes).expect("AHT-disabled frame syntax");
    assert_eq!(enabled_frame.channel_aht_in_use, [true]);
    assert_eq!(disabled_frame.channel_aht_in_use, [false]);

    let enabled = decode_audio_blocks(&enabled_bytes, &[]).expect("AHT-enabled production path");
    let disabled = decode_audio_blocks(&disabled_bytes, &[]).expect("conventional production path");
    assert_eq!(enabled.len(), 6);
    assert_eq!(disabled.len(), 6);
    assert!(enabled[0].channel_aht[0].is_some());
    assert!(disabled.iter().all(|block| block.channel_aht[0].is_none()));
    assert_ne!(
        enabled
            .iter()
            .map(|block| &block.channel_mantissas[0])
            .collect::<Vec<_>>(),
        disabled
            .iter()
            .map(|block| &block.channel_mantissas[0])
            .collect::<Vec<_>>()
    );
    assert!(enabled.iter().chain(&disabled).all(|block| {
        block.channel_mantissas[0]
            .iter()
            .all(|value| value.is_finite())
    }));
}

#[test]
fn decodes_aht_lfe_after_full_bandwidth_channel_in_syntax_order() {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 2); // independent
    bits.push(0, 3); // substream id
    bits.push(2047, 11); // 4096-byte frame
    bits.push(0, 2); // 48 kHz
    bits.push(3, 2); // six audio blocks
    bits.push(1, 3); // mono full-bandwidth channel
    bits.push(1, 1); // LFE present
    bits.push(16, 5); // bitstream id
    bits.push(0, 5); // dialnorm
    bits.push(0, 1); // no compression metadata
    bits.push(0, 1); // no mixing metadata
    bits.push(0, 1); // no informational metadata
    bits.push(0, 1); // no addbsi

    bits.push(1, 1); // per-block exponent strategies
    bits.push(1, 1); // AHT syntax
    bits.push(1, 2); // SNR strategy 1
    bits.push(0, 1); // transient processing disabled
    bits.push(48, 7); // dither and bit-allocation syntax enabled
    for block in 0..6 {
        bits.push(u64::from(block == 0), 2); // channel D15 then reuse
    }
    for block in 0..6 {
        bits.push(u64::from(block == 0), 1); // LFE D15 then reuse
    }
    bits.push(0, 5); // converter exponent strategy
    bits.push(1, 1); // full-bandwidth channel uses AHT
    bits.push(1, 1); // LFE uses AHT
    bits.push(0, 1); // no block-start information

    // First block: explicit dither-off, no SPX, full-bandwidth exponents,
    // LFE exponents, explicit zero allocation parameters, and high SNR.
    bits.push(0, 1); // channel dither flag
    bits.push(0, 1); // dynamic range absent
    bits.push(0, 1); // no SPX
    bits.push(0, 6); // channel bandwidth code => endmant 73
    bits.push(15, 4); // full-bandwidth channel initial exponent
    bits.push(87, 7);
    bits.push(87, 7);
    for _ in 0..22 {
        bits.push(62, 7);
    }
    bits.push(0, 2); // channel gain range
    bits.push(15, 4); // LFE initial exponent
    bits.push(87, 7);
    bits.push(87, 7);
    bits.push(1, 1); // new bit-allocation parameters
    bits.push(0, 11); // all parameter codes zero
    bits.push(63, 6); // coarse SNR
    bits.push(15, 4); // fine SNR
    bits.push(0, 1); // converter SNR offset absent
    bits.push(0, 2); // channel AHT mode 0
    bits.push(0, 2); // LFE AHT mode 0

    // Zero payload is valid for every resulting VQ/scalar codeword; reserve
    // ample bounded space so the following block prefixes remain zero-filled.
    bits.0.extend(std::iter::repeat_n(false, 12_000));
    for _ in 1..6 {
        bits.push(0, 1); // dynamic range absent
        bits.push(0, 1); // SPX strategy reused
        bits.push(0, 6); // chbwcod remains explicit
        bits.push(0, 1); // bit-allocation parameters reused
        bits.push(0, 1); // block fine SNR offset absent
        bits.push(0, 1); // converter SNR offset absent
    }

    let bytes = bits.bytes(4096);
    let blocks = decode_audio_blocks(&bytes, &[]).expect("AHT LFE traversal");
    assert_eq!(blocks.len(), 6);
    assert!(blocks[0].channel_aht[0].is_some());
    assert!(blocks[0].lfe_aht.is_some());
    assert!(
        blocks[1..]
            .iter()
            .all(|block| block.channel_aht[0].is_none() && block.lfe_aht.is_none())
    );
    assert!(blocks.iter().all(|block| {
        block.lfe_bap.as_ref().is_some_and(|baps| !baps.is_empty())
            && block
                .lfe_mantissas
                .as_ref()
                .is_some_and(|mantissas| mantissas.len() == 7)
    }));
    let pcm = synthesize_audio_blocks(&blocks).expect("LFE PCM synthesis");
    assert_eq!(pcm.channels.len(), 1);
    assert_eq!(pcm.channels[0].len(), 1536);
    assert_eq!(pcm.lfe.as_ref().map(Vec::len), Some(1536));
    assert!(
        pcm.lfe
            .as_ref()
            .is_some_and(|channel| channel.iter().all(|sample| sample.is_finite()))
    );
}

#[test]
fn decodes_aht_coupling_after_the_first_participating_channel() {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 2); // independent
    bits.push(0, 3); // substream id
    bits.push(2047, 11); // 4096-byte frame
    bits.push(0, 2); // 48 kHz
    bits.push(3, 2); // six audio blocks
    bits.push(2, 3); // stereo mode, two full-bandwidth channels
    bits.push(0, 1); // no LFE
    bits.push(16, 5); // bitstream id
    bits.push(0, 5); // dialnorm
    bits.push(0, 1); // no compression metadata
    bits.push(0, 1); // no mixing metadata
    bits.push(0, 1); // no informational metadata
    bits.push(0, 1); // no addbsi

    bits.push(1, 1); // per-block exponent strategies
    bits.push(1, 1); // AHT syntax
    bits.push(1, 2); // SNR strategy 1
    bits.push(0, 1); // transient processing disabled
    bits.push(16, 7); // bit-allocation syntax enabled
    bits.push(1, 1); // coupling in use in block zero
    for _ in 1..6 {
        bits.push(0, 1); // coupling-in-use state reused
    }
    for block in 0..6 {
        bits.push(u64::from(block == 0), 2); // coupling D15 then reuse
        for _ in 0..2 {
            bits.push(u64::from(block == 0), 2); // channel D15 then reuse
        }
    }
    bits.push(0, 10); // converter exponent strategy for two channels
    bits.push(1, 1); // coupling uses AHT
    bits.push(1, 1); // channel 0 uses AHT
    bits.push(1, 1); // channel 1 uses AHT
    bits.push(0, 1); // no block-start information

    // First standard-coupling strategy: phase disabled, [begin,end] = [0,0],
    // default three-band structure, and two coordinate sets.
    bits.push(0, 1); // dynamic range absent
    bits.push(0, 1); // no SPX
    bits.push(0, 1); // standard coupling
    bits.push(0, 1); // phase flags not in use
    bits.push(0, 4); // begin frequency code
    bits.push(0, 4); // end frequency code
    bits.push(0, 1); // default band structure
    for _ in 0..2 {
        bits.push(0, 2); // coupling master
        for _ in 0..3 {
            bits.push(0, 4); // coupling coordinate exponent
            bits.push(0, 4); // coupling coordinate mantissa
        }
    }
    bits.push(0, 1); // rematrix band zero
    bits.push(0, 1); // rematrix band one

    // Coupling spans mantissas 37..73; each full-bandwidth channel spans 0..37.
    bits.push(7, 4); // coupling exponent is doubled by parser
    for _ in 0..12 {
        bits.push(62, 7);
    }
    for _ in 0..2 {
        bits.push(15, 4);
        for _ in 0..12 {
            bits.push(62, 7);
        }
        bits.push(0, 2); // channel gain range
    }
    bits.push(1, 1); // new bit-allocation parameters
    bits.push(0, 11); // all parameter codes zero
    bits.push(63, 6); // coarse SNR
    bits.push(15, 4); // fine SNR
    bits.push(0, 1); // converter SNR offset absent
    bits.push(0, 3); // coupling leak fast/slow codes
    bits.push(0, 2); // coupling AHT mode 0
    bits.push(0, 2); // channel 0 AHT mode 0
    bits.push(0, 2); // channel 1 AHT mode 0

    // Reserve zero mantissa payload and following block side information.
    bits.0.extend(std::iter::repeat_n(false, 12_000));
    for _ in 1..6 {
        bits.push(0, 1); // dynamic range absent
        bits.push(0, 1); // SPX strategy reused
        bits.push(0, 1); // rematrix flags reused
        bits.push(0, 1); // channel 0 coupling coordinates reused
        bits.push(0, 1); // channel 1 coupling coordinates reused
        bits.push(0, 1); // bit-allocation parameters reused
        bits.push(0, 1); // block fine SNR offset absent
        bits.push(0, 1); // converter SNR offset absent
        bits.push(0, 1); // coupling leak reused
    }

    let bytes = bits.bytes(4096);
    let blocks = decode_audio_blocks(&bytes, &[]).expect("AHT coupling traversal");
    assert_eq!(blocks.len(), 6);
    assert!(blocks[0].coupling_aht.is_some());
    assert!(
        blocks[1..]
            .iter()
            .all(|block| block.prefix.coupling == blocks[0].prefix.coupling)
    );
    assert!(blocks[0].channel_aht.iter().all(Option::is_some));
    assert!(blocks[1..].iter().all(|block| {
        block.coupling_aht.is_none() && block.channel_aht.iter().all(Option::is_none)
    }));
    assert!(blocks.iter().all(|block| {
        block
            .coupling_bap
            .as_ref()
            .is_some_and(|baps| !baps.is_empty())
            && block
                .coupling_mantissas
                .as_ref()
                .is_some_and(|mantissas| mantissas.len() == 36)
    }));
}

#[test]
fn groups_sequential_independent_and_dependent_substreams_into_access_units() {
    let frames = [
        frame(0, 0, 16, 0, 3),
        frame(1, 0, 16, 0, 3),
        frame(1, 1, 16, 0, 3),
        frame(0, 1, 16, 0, 3),
        frame(0, 0, 16, 0, 3),
        frame(1, 0, 16, 0, 3),
        frame(1, 1, 16, 0, 3),
        frame(0, 1, 16, 0, 3),
    ]
    .concat();
    let indexed = index_syncframes(&frames).expect("indexed frames");
    let units = group_access_units(&indexed).expect("valid substream sequence");
    assert_eq!(units.len(), 2);
    assert_eq!(units[0].first_frame, 0);
    assert_eq!(units[0].frame_count, 4);
    assert_eq!(units[1].first_frame, 4);
    assert_eq!(units[1].frame_count, 4);
    assert_eq!(units[0].sample_rate, 48_000);
    assert_eq!(units[0].samples, 1536);
}

#[test]
fn rejects_nonsequential_substreams_and_timing_mismatch() {
    let bad_dependent = [frame(0, 0, 16, 0, 3), frame(1, 1, 16, 0, 3)].concat();
    assert_eq!(
        group_access_units(&index_syncframes(&bad_dependent).expect("headers")),
        Err(Eac3Error::NonsequentialDependentSubstream {
            expected: 0,
            actual: 1,
        })
    );

    let bad_independent = [frame(0, 0, 16, 0, 3), frame(0, 2, 16, 0, 3)].concat();
    assert_eq!(
        group_access_units(&index_syncframes(&bad_independent).expect("headers")),
        Err(Eac3Error::NonsequentialIndependentSubstream {
            expected: 1,
            actual: 2,
        })
    );

    let bad_timing = [frame(0, 0, 16, 0, 3), frame(1, 0, 16, 1, 3)].concat();
    assert_eq!(
        group_access_units(&index_syncframes(&bad_timing).expect("headers")),
        Err(Eac3Error::SubstreamTimingMismatch { frame: 1 })
    );
}

#[test]
fn rejects_orphan_and_dependent_after_converted_independent_substreams() {
    let orphan = frame(1, 0, 16, 0, 3);
    assert_eq!(
        group_access_units(&index_syncframes(&orphan).expect("orphan header")),
        Err(Eac3Error::MissingIndependentSubstreamZero { frame: 0 })
    );

    let converted_then_dependent = [frame(2, 0, 16, 0, 3), frame(1, 0, 16, 0, 3)].concat();
    assert_eq!(
        group_access_units(
            &index_syncframes(&converted_then_dependent).expect("converted headers")
        ),
        Err(Eac3Error::DependentAfterConvertedSubstream { frame: 1 })
    );
}

fn auxdata_frame(auxdatae: bool, declared_bits: u16, payload: &[u8]) -> Vec<u8> {
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(0, 2);
    bits.push(0, 3);
    bits.push(7, 11); // 16 bytes
    bits.push(0, 2);
    bits.push(0, 2);
    bits.0.resize(128, false);
    if auxdatae {
        bits.set(96, u64::from(declared_bits), 14);
        bits.set(110, 1, 1);
        let payload_start = 96 - payload.len() * 8;
        for (index, byte) in payload.iter().copied().enumerate() {
            bits.set(payload_start + index * 8, u64::from(byte), 8);
        }
    }
    bits.bytes(16)
}

#[test]
fn extracts_forward_ordered_auxdata_from_the_frame_end() {
    let payload = [0x58, 0x38, 0x00, 0x00];
    let extracted = extract_auxdata(&auxdata_frame(true, 32, &payload))
        .expect("valid auxdata")
        .expect("present auxdata");
    assert_eq!(extracted.bit_len, 32);
    assert_eq!(extracted.bytes, payload);

    assert_eq!(extract_auxdata(&auxdata_frame(false, 0, &[])), Ok(None));
    assert_eq!(
        extract_auxdata(&auxdata_frame(true, 100, &[])),
        Err(Eac3Error::AuxDataLengthOutOfRange {
            declared: 100,
            available: 96,
        })
    );
}

#[test]
fn parses_a_bounded_emdf_container_directly_from_auxdata() {
    let mut container = Bits::default();
    container.push(0, 2); // EMDF version
    container.push(0, 3); // key
    container.push(0, 5); // terminator
    container.push(1, 2); // primary protection: 8 bits
    container.push(0, 2); // no secondary protection
    container.push(0, 8);
    let container = container.bytes(3);
    let mut emdf = vec![0x58, 0x38, 0, 3];
    emdf.extend_from_slice(&container);
    let frame = auxdata_frame(true, 56, &emdf);

    let parsed = extract_aux_emdf(&frame)
        .expect("valid carrier")
        .expect("EMDF present");
    assert_eq!(parsed.container.version, 0);
    assert!(parsed.container.payloads.is_empty());
    assert_eq!(parsed.bytes_consumed, 7);
}

#[test]
fn classifies_frame_end_auxdata_without_scanning_or_accepting_trailing_bytes() {
    let non_emdf = auxdata_frame(true, 32, &[0x00, 0x00, 0x00, 0x00]);
    assert_eq!(
        classify_aux_emdf(&non_emdf).expect("bounded auxiliary data"),
        Some(openjoc_emdf::CarrierClassification::NonEmdf)
    );

    let mut container = Bits::default();
    container.push(0, 2); // EMDF version
    container.push(0, 3); // key
    container.push(0, 5); // terminator
    container.push(1, 2); // primary protection: 8 bits
    container.push(0, 2); // no secondary protection
    container.push(0, 8);
    let container = container.bytes(3);
    let mut emdf = vec![0x58, 0x38, 0, 3];
    emdf.extend_from_slice(&container);
    emdf.push(0);
    let frame = auxdata_frame(true, 64, &emdf);
    assert_eq!(
        classify_aux_emdf(&frame).expect("bounded auxiliary data"),
        Some(openjoc_emdf::CarrierClassification::TrailingData {
            container_bytes: 7,
            carrier_bytes: 8,
        })
    );
    assert!(matches!(
        extract_aux_emdf(&frame),
        Err(Eac3Error::EmdfCarrierTrailingData {
            container_bytes: 7,
            carrier_bytes: 8,
        })
    ));
}

#[test]
fn classifies_exact_skip_field_ranges_without_scanning_or_padding() {
    let non_emdf = openjoc_eac3::AuxiliaryData {
        bit_len: 32,
        bytes: vec![0, 0, 0, 0],
    };
    assert_eq!(
        classify_skip_field_emdf(&non_emdf),
        openjoc_emdf::CarrierClassification::NonEmdf
    );

    let emdf = joc_emdf();
    let exact = openjoc_eac3::AuxiliaryData {
        bit_len: emdf.len() * 8,
        bytes: emdf.clone(),
    };
    assert!(matches!(
        classify_skip_field_emdf(&exact),
        openjoc_emdf::CarrierClassification::Parsed(_)
    ));

    let mut trailing = emdf;
    trailing.push(0);
    assert_eq!(
        classify_skip_field_emdf(&openjoc_eac3::AuxiliaryData {
            bit_len: trailing.len() * 8,
            bytes: trailing,
        }),
        openjoc_emdf::CarrierClassification::TrailingData {
            container_bytes: exact.bytes.len(),
            carrier_bytes: exact.bytes.len() + 1,
        }
    );
}

fn joc_emdf_for_profile(vendor_compat: bool) -> Vec<u8> {
    let mut container = Bits::default();
    container.push(0, 2);
    container.push(0, 3);
    for (id, payload) in [(11, 0xa5), (14, 0x5a)] {
        container.push(id, 5);
        container.push(0, 1); // no sample offset
        container.push(0, 1); // no duration
        container.push(1, 1); // group ID
        container.push(1, 2);
        container.push(0, 1); // variable-bits stop
        container.push(u64::from(!vendor_compat), 1); // codec data present
        if !vendor_compat {
            container.push(0, 8); // reserved codec data
        }
        container.push(0, 1); // retain unknown payload
        if vendor_compat && id == 11 {
            container.push(0, 1); // observed Logic OAMD is not frame aligned
        } else {
            container.push(1, 1); // frame aligned
            container.push(0, 1); // create duplicate
            container.push(0, 1); // remove duplicate
            container.push(0, 5); // priority
            container.push(0, 2); // proc_allowed
        }
        container.push(1, 8); // one payload byte
        container.push(0, 1); // variable-bits stop
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

fn joc_emdf() -> Vec<u8> {
    joc_emdf_for_profile(false)
}

fn joc_carrier_frame(stream_type: u8, substream_id: u8, emdf: Option<&[u8]>) -> Vec<u8> {
    let size = 64;
    let mut bits = Bits::default();
    bits.push(0x0b77, 16);
    bits.push(u64::from(stream_type), 2);
    bits.push(u64::from(substream_id), 3);
    bits.push(31, 11);
    bits.push(0, 2);
    bits.push(3, 2);
    bits.push(2, 3);
    bits.push(0, 1);
    bits.push(16, 5);
    bits.push(31, 5);
    bits.push(0, 1);
    if stream_type == 1 {
        bits.push(0, 1); // no custom channel map
    }
    bits.push(0, 1);
    bits.push(0, 1);
    bits.push(u64::from(emdf.is_some()), 1);
    if emdf.is_some() {
        bits.push(1, 6);
        bits.push(0x01, 8);
        bits.push(2, 8);
    }
    bits.0.resize(size * 8, false);
    if let Some(emdf) = emdf {
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
    }
    bits.bytes(size)
}

#[test]
fn extracts_joc_profile_from_the_last_dependent_substream() {
    let emdf = joc_emdf();
    let bytes = [
        joc_carrier_frame(0, 0, None),
        joc_carrier_frame(1, 0, None),
        joc_carrier_frame(1, 1, Some(&emdf)),
    ]
    .concat();
    let frames = index_syncframes(&bytes).expect("frames");
    let units = group_access_units(&frames).expect("unit");
    let metadata = extract_aux_joc_access_unit(&bytes, &frames, units[0])
        .expect("valid JOC carrier")
        .expect("JOC metadata");
    assert_eq!(metadata.carrier_frame, 2);
    assert_eq!(metadata.complexity_index, 2);
    assert_eq!(metadata.oamd, [0xa5]);
    assert_eq!(metadata.joc, [0x5a]);
}

#[test]
fn extracts_joc_profile_from_an_exact_audio_block_skip_field() {
    let emdf = joc_emdf();
    let bytes = skip_field_joc_frame(&emdf);
    let frames = index_syncframes(&bytes).expect("frame");
    let unit = group_access_units(&frames).expect("unit")[0];
    let metadata = openjoc_eac3::extract_joc_access_unit(&bytes, &frames, unit)
        .expect("valid bounded skip-field profile")
        .expect("JOC metadata");
    assert_eq!(metadata.carrier_frame, 0);
    assert_eq!(metadata.complexity_index, 1);
    assert_eq!(metadata.oamd, [0xa5]);
    assert_eq!(metadata.joc, [0x5a]);

    let mut carriers = Vec::new();
    let report = inspect_audio_block_carriers(&bytes, |carrier| {
        carriers.push((
            carrier.block_index,
            carrier.skip_field_start_offset_bits,
            carrier.skip_field.clone(),
        ));
    })
    .expect("bounded audio-block traversal");
    assert_eq!(report.examined_blocks, 1);
    assert_eq!(report.unresolved_blocks, 0);
    assert_eq!(carriers.len(), 1);
    assert_eq!(carriers[0].0, 0);
    assert_eq!(
        carriers[0].2,
        Some(openjoc_eac3::AuxiliaryData {
            bit_len: emdf.len() * 8,
            bytes: emdf,
        })
    );
}

#[test]
fn parser_validation_and_decoder_metadata_are_explicit_for_vendor_signaling() {
    let emdf = joc_emdf_for_profile(true);
    let bytes = skip_field_joc_frame(&emdf);
    let frames = index_syncframes(&bytes).expect("frame");
    let unit = group_access_units(&frames).expect("unit")[0];

    let parsed = parse_joc_access_unit(&bytes, &frames, unit)
        .expect("bounded parser")
        .expect("parsed JOC candidate");
    assert!(!parsed.emdf.payloads[0].config.codec_data_present);
    assert_eq!(
        parsed.emdf.payloads[0].config.payload_frame_aligned,
        Some(false)
    );

    let strict = validate_joc_access_unit(&parsed, JocValidationProfile::EtsiStrict)
        .expect_err("strict validation must preserve the normative failure");
    let Eac3Error::JocProfileValidation(strict) = strict else {
        panic!("expected structured profile evidence");
    };
    assert_eq!(strict.profile, JocValidationProfile::EtsiStrict);
    assert!(
        strict
            .deviations
            .iter()
            .any(|deviation| deviation.field == JocProfileField::CodecDataPresent)
    );

    let compatible = validate_joc_access_unit(&parsed, JocValidationProfile::ObservedVendorCompat)
        .expect("documented vendor profile");
    assert_eq!(
        compatible.validation_status,
        JocValidationStatus::AcceptedWithDeviation
    );
    assert_eq!(
        compatible.validation_profile,
        JocValidationProfile::ObservedVendorCompat
    );
    assert_eq!(compatible.deviations, strict.deviations);
    assert_eq!(compatible.emdf, parsed.emdf);
    assert_eq!(compatible.oamd, [0xa5]);
    assert_eq!(compatible.joc, [0x5a]);
}

#[test]
fn extracts_joc_addbsi_when_profile_payload_is_unavailable() {
    let bytes = joc_carrier_frame(0, 0, None);
    let frames = index_syncframes(&bytes).expect("frames");
    let units = group_access_units(&frames).expect("unit");
    assert_eq!(
        extract_joc_addbsi_access_unit(&bytes, &frames, units[0])
            .expect("addbsi")
            .map(|value| value.complexity_index),
        None
    );

    let emdf = joc_emdf();
    let bytes = joc_carrier_frame(0, 0, Some(&emdf));
    let frames = index_syncframes(&bytes).expect("frames");
    let units = group_access_units(&frames).expect("unit");
    assert_eq!(
        extract_joc_addbsi_access_unit(&bytes, &frames, units[0])
            .expect("addbsi")
            .map(|value| value.complexity_index),
        Some(2)
    );
}

#[test]
fn rejects_joc_profile_before_the_last_dependent_substream() {
    let emdf = joc_emdf();
    let bytes = [
        joc_carrier_frame(0, 0, None),
        joc_carrier_frame(1, 0, Some(&emdf)),
        joc_carrier_frame(1, 1, None),
    ]
    .concat();
    let frames = index_syncframes(&bytes).expect("frames");
    let unit = group_access_units(&frames).expect("unit")[0];
    assert_eq!(
        extract_aux_joc_access_unit(&bytes, &frames, unit),
        Err(Eac3Error::InvalidJocCarrierPlacement {
            carrier_frame: 1,
            required_frame: 2,
        })
    );
}
