// pattern: Mixed (bounded extraction shell around pure bit-span diagnostics)

use openjoc_container::{InputMediaKind, load_eac3};
use openjoc_eac3::{
    AccessUnitIndex, Eac3Error, SyncframeIndexEntry, classify_skip_field_emdf, group_access_units,
    index_syncframes, inspect_audio_block_carriers,
};
use openjoc_emdf::{EmdfPayload, EmdfPayloadBitTrace, parse_emdf_sync_with_bit_trace};
use openjoc_oamd::{OamdBitTrace, OamdDecoderConfig, trace_oamd_payload};
use serde::Serialize;
use std::{
    fs, io,
    num::NonZeroU8,
    path::PathBuf,
    process::{Command, Stdio},
};

#[derive(Clone, Debug, Serialize)]
struct BitSpan {
    start_bit: usize,
    end_bit: usize,
    length_bits: usize,
}

#[derive(Clone, Copy, Debug)]
struct IsoPacket {
    sample_index: usize,
    file_start_byte: usize,
    stream_start_byte: usize,
    size_bytes: usize,
}

struct OamdTraceContext<'a> {
    stream: &'a [u8],
    original_file: &'a [u8],
    emdf_start_stream: usize,
    unit_start_stream: usize,
    original_file_bit_offset: Option<usize>,
}

struct ForensicInput<'a> {
    stream: &'a [u8],
    original_file: &'a [u8],
    frames: &'a [SyncframeIndexEntry],
    units: &'a [AccessUnitIndex],
    input_media: InputMediaKind,
    iso_packets: Option<&'a [IsoPacket]>,
}

#[derive(Clone, Debug, Serialize)]
struct PayloadConfigEvidence {
    payload_id: u64,
    sample_offset: Option<u16>,
    duration: Option<u64>,
    group_id: Option<u64>,
    codec_data_present: bool,
    discard_unknown_payload: bool,
    payload_frame_aligned: Option<bool>,
    create_duplicate: Option<bool>,
    remove_duplicate: Option<bool>,
    priority: Option<u8>,
    processing_allowed: Option<u8>,
}

#[derive(Clone, Debug, Serialize)]
struct PayloadEvidence {
    payload_id: u64,
    id_span_emdf: BitSpan,
    config_span_emdf: BitSpan,
    size_span_emdf: BitSpan,
    body_span_emdf: BitSpan,
    coordinate_spans: PayloadCoordinateSpans,
    config: PayloadConfigEvidence,
    body_bytes_hex: String,
}

#[derive(Clone, Debug, Serialize)]
struct PayloadCoordinateSpans {
    bounded_skip_field: PayloadFieldSpans,
    elementary_stream: PayloadFieldSpans,
    access_unit: PayloadFieldSpans,
    original_file: Option<PayloadFieldSpans>,
}

#[derive(Clone, Debug, Serialize)]
struct PayloadFieldSpans {
    id: BitSpan,
    config: BitSpan,
    size: BitSpan,
    body: BitSpan,
}

#[derive(Clone, Debug, Serialize)]
struct OamdEvidence {
    payload_span_emdf: BitSpan,
    payload_span_skip_field: BitSpan,
    payload_span_elementary_stream: BitSpan,
    payload_span_access_unit: BitSpan,
    payload_span_original_file: Option<BitSpan>,
    payload_bits: usize,
    prefix_end_bit: usize,
    object_count: u16,
    element_count: u8,
    elements: Vec<openjoc_oamd::OamdElementBitTrace>,
    trim_element_index: Option<usize>,
    trim_element_body_span_oamd: Option<BitSpan>,
    warp_mode_span_oamd: Option<BitSpan>,
    warp_mode_span_emdf: Option<BitSpan>,
    warp_mode_span_skip_field: Option<BitSpan>,
    warp_mode_span_elementary_stream: Option<BitSpan>,
    warp_mode_span_access_unit: Option<BitSpan>,
    warp_mode_span_original_file: Option<BitSpan>,
    warp_mode_raw: Option<u8>,
    warp_window_start_bit_oamd: Option<usize>,
    warp_window_end_bit_oamd: Option<usize>,
    warp_window_before_bits: Option<usize>,
    warp_window_after_bits: Option<usize>,
    warp_window_bits: Option<String>,
    warp_window_bytes_start_byte_oamd: Option<usize>,
    warp_window_bytes_hex: Option<String>,
    warp_window_bytes_start_byte_elementary_stream: Option<usize>,
    warp_window_bytes_hex_elementary_stream: Option<String>,
    warp_window_bytes_start_byte_original_file: Option<usize>,
    warp_window_bytes_hex_original_file: Option<String>,
    trim_config_count: Option<u8>,
    validator_result: String,
}

#[derive(Clone, Debug, Serialize)]
struct ForensicObservation {
    mp4_sample_index: Option<usize>,
    access_unit_index: usize,
    access_unit_frame_start: usize,
    carrier_frame_index: usize,
    substream_id: u8,
    input_media: String,
    original_file_bit_offset: Option<usize>,
    original_file_coordinate_note: String,
    elementary_stream_bit_offset: usize,
    access_unit_relative_bit_offset: usize,
    skip_field_span_frame: BitSpan,
    skip_field_span_original_file: Option<BitSpan>,
    skip_field_span_elementary_stream: BitSpan,
    skip_field_span_access_unit: BitSpan,
    emdf_span_skip_field: BitSpan,
    emdf_span_elementary_stream: BitSpan,
    emdf_span_access_unit: BitSpan,
    emdf_span_original_file: Option<BitSpan>,
    emdf_container_bytes: usize,
    emdf_payloads: Vec<PayloadEvidence>,
    oamd: Option<OamdEvidence>,
    trim_config_count: Option<u8>,
    parse_error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct ForensicReport {
    input: String,
    input_media: String,
    coordinate_convention: String,
    syncframe_count: usize,
    access_unit_count: usize,
    selected_access_units: Vec<usize>,
    observations: Vec<ForensicObservation>,
}

pub fn run(values: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let input = values.first().filter(|value| !value.starts_with('-'));
    let mut output = None;
    let mut requested_access_unit = None;
    let mut trim_config_count = None;
    let mut all_access_units = false;
    let mut index = 1;
    while index < values.len() {
        let flag = &values[index];
        if flag == "--all-access-units" {
            all_access_units = true;
            index += 1;
            continue;
        }
        let value = values.get(index + 1).ok_or_else(usage_error)?;
        match flag.as_str() {
            "-o" | "--output" => output = Some(PathBuf::from(value)),
            "--access-unit" => {
                requested_access_unit = Some(value.parse::<usize>().map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidInput, "invalid access-unit index")
                })?);
            }
            "--trim-config-count" => trim_config_count = Some(parse_trim_count(value)?),
            _ => return Err(usage_error().into()),
        }
        index += 2;
    }
    let input = PathBuf::from(input.ok_or_else(usage_error)?);
    let output = output.ok_or_else(usage_error)?;
    let original_file_bytes = fs::read(&input)?;
    let media = load_eac3(&input)?;
    let iso_packets = if media.kind == InputMediaKind::IsoBmff {
        Some(index_iso_packets(&input, media.bytes.len())?)
    } else {
        None
    };
    let frames = index_syncframes(&media.bytes)?;
    let units = group_access_units(&frames)?;
    if units.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty E-AC-3 stream").into());
    }
    let forensic_input = ForensicInput {
        stream: &media.bytes,
        original_file: &original_file_bytes,
        frames: &frames,
        units: &units,
        input_media: media.kind,
        iso_packets: iso_packets.as_deref(),
    };
    let selected = selected_units(units.len(), requested_access_unit, all_access_units)?;
    let observations = selected
        .iter()
        .map(|&unit_index| observe_access_unit(&forensic_input, unit_index, trim_config_count))
        .collect::<Result<Vec<_>, _>>()?;
    let report = ForensicReport {
        input: input.display().to_string(),
        input_media: media_kind_name(media.kind).to_owned(),
        coordinate_convention: "All bit spans are MSB-first and half-open [start,end). PayloadEvidence.coordinate_spans names bounded skip-field, elementary-stream, access-unit, and original-file coordinates. OamdEvidence payload spans use the same coordinates; OamdEvidence warp_mode_span_oamd is relative to the OAMD payload start, while its other warp spans name the parent coordinate directly.".to_owned(),
        syncframe_count: frames.len(),
        access_unit_count: units.len(),
        selected_access_units: selected,
        observations,
    };
    fs::create_dir_all(&output)?;
    fs::write(
        output.join("oamd_forensics.json"),
        format!("{}\n", serde_json::to_string_pretty(&report)?),
    )?;
    fs::write(output.join("oamd_forensics.txt"), render_text(&report))?;
    println!(
        "oamd-forensics: {} observations written to {}",
        report.observations.len(),
        output.display()
    );
    Ok(())
}

fn parse_trim_count(value: &str) -> Result<NonZeroU8, io::Error> {
    let count = value.parse::<u8>().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid OAMD trim configuration count; expected 1..=255",
        )
    })?;
    NonZeroU8::new(count).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid OAMD trim configuration count; expected 1..=255",
        )
    })
}

fn selected_units(
    count: usize,
    requested: Option<usize>,
    all: bool,
) -> Result<Vec<usize>, io::Error> {
    if all && requested.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--all-access-units cannot be combined with --access-unit",
        ));
    }
    if all {
        return Ok((0..count).collect());
    }
    if let Some(index) = requested {
        if index >= count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("access-unit index {index} is outside 0..{count}"),
            ));
        }
        return Ok(vec![index]);
    }
    let mut selected = vec![0, count / 2, count - 1];
    selected.sort_unstable();
    selected.dedup();
    Ok(selected)
}

fn index_iso_packets(
    path: &PathBuf,
    stream_len: usize,
) -> Result<Vec<IsoPacket>, Box<dyn std::error::Error>> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "packet=size,pos",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .stdin(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other("ffprobe could not enumerate MP4 audio samples").into());
    }
    let mut stream_start_byte = 0_usize;
    let mut packets = Vec::new();
    for (sample_index, line) in String::from_utf8(output.stdout)?.lines().enumerate() {
        let fields = line
            .split(',')
            .map(str::trim)
            .filter(|field| !field.is_empty())
            .collect::<Vec<_>>();
        let size_bytes = fields.first().and_then(|field| field.parse::<usize>().ok());
        let file_start_byte = fields.get(1).and_then(|field| field.parse::<usize>().ok());
        let (Some(size_bytes), Some(file_start_byte)) = (size_bytes, file_start_byte) else {
            return Err(io::Error::other(format!(
                "ffprobe returned an unusable MP4 packet row: {line}"
            ))
            .into());
        };
        packets.push(IsoPacket {
            sample_index,
            file_start_byte,
            stream_start_byte,
            size_bytes,
        });
        stream_start_byte = stream_start_byte
            .checked_add(size_bytes)
            .ok_or_else(|| io::Error::other("MP4 packet stream offset overflow"))?;
    }
    if packets.is_empty() || stream_start_byte != stream_len {
        return Err(io::Error::other(format!(
            "MP4 packet byte sum {stream_start_byte} does not match demuxed E-AC-3 length {stream_len}"
        ))
        .into());
    }
    Ok(packets)
}

fn iso_packet_for_stream_offset(
    packets: &[IsoPacket],
    stream_offset_bits: usize,
) -> Option<&IsoPacket> {
    let stream_offset_byte = stream_offset_bits / 8;
    packets.iter().find(|packet| {
        stream_offset_byte >= packet.stream_start_byte
            && stream_offset_byte < packet.stream_start_byte + packet.size_bytes
    })
}

fn original_file_bit_offset(packet: &IsoPacket, stream_offset_bits: usize) -> Option<usize> {
    let packet_stream_start = packet.stream_start_byte.checked_mul(8)?;
    let delta = stream_offset_bits.checked_sub(packet_stream_start)?;
    if delta >= packet.size_bytes.checked_mul(8)? {
        return None;
    }
    packet.file_start_byte.checked_mul(8)?.checked_add(delta)
}

fn observe_access_unit(
    input: &ForensicInput<'_>,
    unit_index: usize,
    trim_config_count: Option<NonZeroU8>,
) -> Result<ForensicObservation, Box<dyn std::error::Error>> {
    let unit = input.units[unit_index];
    let unit_start_offset = input.frames[unit.first_frame].offset;
    let mut found = None;
    for (frame_index, entry) in input
        .frames
        .iter()
        .copied()
        .enumerate()
        .skip(unit.first_frame)
        .take(unit.frame_count)
    {
        let frame = frame_slice(input.stream, &entry)?;
        let mut callback_error = None;
        inspect_audio_block_carriers(frame, |carrier| {
            if found.is_some() || callback_error.is_some() {
                return;
            }
            let Some(skip) = carrier.skip_field.as_ref() else {
                return;
            };
            let Some(_skip_start) = carrier.skip_field_start_offset_bits else {
                return;
            };
            if !matches!(
                classify_skip_field_emdf(skip),
                openjoc_emdf::CarrierClassification::Parsed(_)
            ) {
                return;
            }
            match parse_emdf_sync_with_bit_trace(&skip.bytes) {
                Ok(trace)
                    if trace
                        .parsed
                        .container
                        .payloads
                        .iter()
                        .any(|payload| payload.id == 11) =>
                {
                    found = Some((frame_index, entry, carrier.clone(), trace));
                }
                Ok(_) => {}
                Err(error) => callback_error = Some(error.to_string()),
            }
        })?;
        if let Some(error) = callback_error {
            return Err(error.into());
        }
        if found.is_some() {
            break;
        }
    }
    let Some((carrier_frame, entry, carrier, emdf_trace)) = found else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("no payload-11 skip-field candidate in access unit {unit_index}"),
        )
        .into());
    };
    let skip_start_frame = carrier.skip_field_start_offset_bits.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "candidate lost skip-field start",
        )
    })?;
    let skip_len = carrier
        .skip_field
        .as_ref()
        .map(|skip| skip.bit_len)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "candidate lost skip-field bytes",
            )
        })?;
    let skip_start_stream = entry
        .offset
        .checked_mul(8)
        .and_then(|value| value.checked_add(skip_start_frame))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "skip offset overflow"))?;
    let unit_start_stream = unit_start_offset
        .checked_mul(8)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "access-unit offset overflow"))?;
    let payload = emdf_trace
        .parsed
        .container
        .payloads
        .iter()
        .find(|payload| payload.id == 11)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "payload 11 disappeared"))?;
    let payload_trace = emdf_trace
        .payloads
        .iter()
        .find(|trace| trace.payload_id == 11)
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "payload 11 trace disappeared")
        })?;
    let validator_result = match trim_config_count {
        Some(count) => openjoc_oamd::parse_oamd_payload_with_config(
            &payload.data,
            OamdDecoderConfig {
                trim_configuration_count: Some(count),
            },
        )
        .map_or_else(|error| error.to_string(), |_| "accepted".to_owned()),
        None => "not_attempted_without_trim_config_count".to_owned(),
    };
    let emdf_start_stream = skip_start_stream;
    let emdf_end_stream = emdf_start_stream
        .checked_add(
            emdf_trace
                .parsed
                .bytes_consumed
                .checked_mul(8)
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "EMDF length overflow")
                })?,
        )
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "EMDF end overflow"))?;
    let iso_packet = input
        .iso_packets
        .and_then(|packets| iso_packet_for_stream_offset(packets, emdf_start_stream));
    let mp4_sample_index = iso_packet.map(|packet| packet.sample_index);
    let source_file_bit_offset = match input.input_media {
        InputMediaKind::RawEac3 => Some(emdf_start_stream),
        InputMediaKind::IsoBmff => {
            iso_packet.and_then(|packet| original_file_bit_offset(packet, emdf_start_stream))
        }
        InputMediaKind::Unknown => None,
    };
    let source_file_coordinate_note = match input.input_media {
        InputMediaKind::RawEac3 => "raw EC-3 file bytes equal the elementary-stream bytes".to_owned(),
        InputMediaKind::IsoBmff if iso_packet.is_some() => "MP4 sample index and original-file packet position mapped by ffprobe packet size/pos against the exact demuxed E-AC-3 byte sequence".to_owned(),
        InputMediaKind::IsoBmff => "MP4 demux packet mapping was unavailable; elementary-stream offsets remain explicit".to_owned(),
        InputMediaKind::Unknown => "container classifier returned unknown; only elementary-stream offsets are asserted".to_owned(),
    };
    let oamd_trace = trace_oamd_payload(&payload.data).ok();
    let trace_context = OamdTraceContext {
        stream: input.stream,
        original_file: input.original_file,
        emdf_start_stream,
        unit_start_stream,
        original_file_bit_offset: source_file_bit_offset,
    };
    let oamd = oamd_trace.as_ref().map(|trace| {
        build_oamd_evidence(
            trace,
            payload_trace,
            payload,
            trim_config_count,
            &trace_context,
        )
    });
    let payload_evidence = emdf_trace
        .payloads
        .iter()
        .filter_map(|trace| {
            emdf_trace
                .parsed
                .container
                .payloads
                .iter()
                .find(|payload| payload.id == trace.payload_id)
                .map(|payload| {
                    payload_evidence(
                        trace,
                        payload,
                        emdf_start_stream,
                        unit_start_stream,
                        source_file_bit_offset,
                    )
                })
        })
        .collect();
    Ok(ForensicObservation {
        mp4_sample_index,
        access_unit_index: unit_index,
        access_unit_frame_start: unit.first_frame,
        carrier_frame_index: carrier_frame,
        substream_id: entry.header.substream_id,
        input_media: media_kind_name(input.input_media).to_owned(),
        original_file_bit_offset: source_file_bit_offset,
        original_file_coordinate_note: source_file_coordinate_note,
        elementary_stream_bit_offset: emdf_start_stream,
        access_unit_relative_bit_offset: emdf_start_stream
            .checked_sub(unit_start_stream)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "AU offset underflow"))?,
        skip_field_span_frame: span(skip_start_frame, skip_start_frame + skip_len),
        skip_field_span_original_file: source_file_bit_offset
            .map(|start| span(start, start + skip_len)),
        skip_field_span_elementary_stream: span(skip_start_stream, skip_start_stream + skip_len),
        skip_field_span_access_unit: span(
            skip_start_stream - unit_start_stream,
            skip_start_stream - unit_start_stream + skip_len,
        ),
        emdf_span_skip_field: span(0, emdf_trace.parsed.bytes_consumed * 8),
        emdf_span_elementary_stream: span(emdf_start_stream, emdf_end_stream),
        emdf_span_access_unit: span(
            emdf_start_stream - unit_start_stream,
            emdf_end_stream - unit_start_stream,
        ),
        emdf_span_original_file: source_file_bit_offset
            .map(|start| span(start, start + emdf_trace.parsed.bytes_consumed * 8)),
        emdf_container_bytes: emdf_trace.parsed.bytes_consumed,
        emdf_payloads: payload_evidence,
        oamd,
        trim_config_count: trim_config_count.map(NonZeroU8::get),
        parse_error: Some(validator_result),
    })
}

fn build_oamd_evidence(
    trace: &OamdBitTrace,
    payload_trace: &EmdfPayloadBitTrace,
    payload: &EmdfPayload,
    trim_config_count: Option<NonZeroU8>,
    context: &OamdTraceContext<'_>,
) -> OamdEvidence {
    let trim = trace.elements.iter().find(|element| element.id == 2);
    let warp = trim.and_then(|element| element.warp_mode_start_bit.zip(element.warp_mode_raw));
    let payload_span_emdf = span(
        payload_trace.payload_body_start_bit,
        payload_trace.payload_body_end_bit,
    );
    let payload_span_skip_field = payload_span_emdf.clone();
    let payload_span_elementary_stream = shift_span(&payload_span_emdf, context.emdf_start_stream);
    let payload_span_access_unit = span(
        payload_span_elementary_stream
            .start_bit
            .saturating_sub(context.unit_start_stream),
        payload_span_elementary_stream
            .end_bit
            .saturating_sub(context.unit_start_stream),
    );
    let payload_span_original_file = context
        .original_file_bit_offset
        .map(|start| shift_span(&payload_span_emdf, start));
    let (window_start, window_end, before, after, bits, bytes_start, bytes_hex) = warp.map_or(
        (None, None, None, None, None, None, None),
        |(warp_start, _)| {
            let start = warp_start.saturating_sub(64);
            let end = (warp_start + 2 + 64).min(trace.payload_bits);
            (
                Some(start),
                Some(end),
                Some(warp_start - start),
                Some(end.saturating_sub(warp_start + 2)),
                Some(bit_window(&payload.data, start, end)),
                Some(start / 8),
                Some(hex_window(&payload.data, start, end)),
            )
        },
    );
    let (es_window_start_byte, es_window_hex) = warp.map_or((None, None), |(warp_start, _)| {
        let start_bit = payload_trace
            .payload_body_start_bit
            .checked_add(warp_start.saturating_sub(64))
            .and_then(|value| context.emdf_start_stream.checked_add(value));
        let end_bit = payload_trace
            .payload_body_start_bit
            .checked_add((warp_start + 2 + 64).min(trace.payload_bits))
            .and_then(|value| context.emdf_start_stream.checked_add(value));
        let (Some(start_bit), Some(end_bit)) = (start_bit, end_bit) else {
            return (None, None);
        };
        let start_byte = start_bit / 8;
        let end_byte = end_bit.saturating_add(7) / 8;
        (
            Some(start_byte),
            Some(hex_bytes(
                context.stream.get(start_byte..end_byte).unwrap_or_default(),
            )),
        )
    });
    let (original_window_start_byte, original_window_hex) =
        warp.map_or((None, None), |(warp_start, _)| {
            let Some(original_start) = context.original_file_bit_offset else {
                return (None, None);
            };
            let start_bit = payload_trace
                .payload_body_start_bit
                .checked_add(warp_start.saturating_sub(64))
                .and_then(|value| original_start.checked_add(value));
            let end_bit = payload_trace
                .payload_body_start_bit
                .checked_add((warp_start + 2 + 64).min(trace.payload_bits))
                .and_then(|value| original_start.checked_add(value));
            let (Some(start_bit), Some(end_bit)) = (start_bit, end_bit) else {
                return (None, None);
            };
            let start_byte = start_bit / 8;
            let end_byte = end_bit.saturating_add(7) / 8;
            (
                Some(start_byte),
                Some(hex_bytes(
                    context
                        .original_file
                        .get(start_byte..end_byte)
                        .unwrap_or_default(),
                )),
            )
        });
    let warp_mode_span_oamd = warp.map(|(start, _)| span(start, start + 2));
    let warp_mode_span_emdf = warp_mode_span_oamd
        .as_ref()
        .map(|value| shift_span(value, payload_trace.payload_body_start_bit));
    let warp_mode_span_skip_field = warp_mode_span_emdf.clone();
    let warp_mode_span_elementary_stream = warp_mode_span_emdf
        .as_ref()
        .map(|value| shift_span(value, context.emdf_start_stream));
    let warp_mode_span_access_unit = warp_mode_span_elementary_stream.as_ref().map(|value| {
        span(
            value.start_bit.saturating_sub(context.unit_start_stream),
            value.end_bit.saturating_sub(context.unit_start_stream),
        )
    });
    let warp_mode_span_original_file = context
        .original_file_bit_offset
        .zip(warp_mode_span_emdf.as_ref())
        .map(|(start, value)| shift_span(value, start));
    OamdEvidence {
        payload_span_emdf,
        payload_span_skip_field,
        payload_span_elementary_stream,
        payload_span_access_unit,
        payload_span_original_file,
        payload_bits: trace.payload_bits,
        prefix_end_bit: trace.prefix_end_bit,
        object_count: trace.object_count,
        element_count: trace.element_count,
        elements: trace.elements.clone(),
        trim_element_index: trim.map(|element| element.index),
        trim_element_body_span_oamd: trim
            .map(|element| span(element.body_start_bit, element.body_end_bit)),
        warp_mode_span_oamd,
        warp_mode_span_emdf,
        warp_mode_span_skip_field,
        warp_mode_span_elementary_stream,
        warp_mode_span_access_unit,
        warp_mode_span_original_file,
        warp_mode_raw: warp.map(|(_, raw)| raw),
        warp_window_start_bit_oamd: window_start,
        warp_window_end_bit_oamd: window_end,
        warp_window_before_bits: before,
        warp_window_after_bits: after,
        warp_window_bits: bits,
        warp_window_bytes_start_byte_oamd: bytes_start,
        warp_window_bytes_hex: bytes_hex,
        warp_window_bytes_start_byte_elementary_stream: es_window_start_byte,
        warp_window_bytes_hex_elementary_stream: es_window_hex,
        warp_window_bytes_start_byte_original_file: original_window_start_byte,
        warp_window_bytes_hex_original_file: original_window_hex,
        trim_config_count: trim_config_count.map(NonZeroU8::get),
        validator_result: "trace_only".to_owned(),
    }
}

fn payload_evidence(
    trace: &EmdfPayloadBitTrace,
    payload: &EmdfPayload,
    emdf_start_stream: usize,
    unit_start_stream: usize,
    original_file_bit_offset: Option<usize>,
) -> PayloadEvidence {
    let emdf = payload_field_spans(trace, 0);
    let elementary_stream = payload_field_spans(trace, emdf_start_stream);
    let access_unit =
        payload_field_spans(trace, emdf_start_stream.saturating_sub(unit_start_stream));
    let original_file = original_file_bit_offset.map(|base| payload_field_spans(trace, base));
    PayloadEvidence {
        payload_id: trace.payload_id,
        id_span_emdf: span(trace.payload_id_start_bit, trace.payload_id_end_bit),
        config_span_emdf: span(trace.config_start_bit, trace.config_end_bit),
        size_span_emdf: span(trace.payload_size_start_bit, trace.payload_size_end_bit),
        body_span_emdf: span(trace.payload_body_start_bit, trace.payload_body_end_bit),
        coordinate_spans: PayloadCoordinateSpans {
            bounded_skip_field: emdf,
            elementary_stream,
            access_unit,
            original_file,
        },
        config: PayloadConfigEvidence {
            payload_id: payload.id,
            sample_offset: payload.config.sample_offset,
            duration: payload.config.duration,
            group_id: payload.config.group_id,
            codec_data_present: payload.config.codec_data_present,
            discard_unknown_payload: payload.config.discard_unknown_payload,
            payload_frame_aligned: payload.config.payload_frame_aligned,
            create_duplicate: payload.config.create_duplicate,
            remove_duplicate: payload.config.remove_duplicate,
            priority: payload.config.priority,
            processing_allowed: payload.config.processing_allowed,
        },
        body_bytes_hex: hex_bytes(&payload.data),
    }
}

fn payload_field_spans(trace: &EmdfPayloadBitTrace, base_bit: usize) -> PayloadFieldSpans {
    PayloadFieldSpans {
        id: shift_span(
            &span(trace.payload_id_start_bit, trace.payload_id_end_bit),
            base_bit,
        ),
        config: shift_span(
            &span(trace.config_start_bit, trace.config_end_bit),
            base_bit,
        ),
        size: shift_span(
            &span(trace.payload_size_start_bit, trace.payload_size_end_bit),
            base_bit,
        ),
        body: shift_span(
            &span(trace.payload_body_start_bit, trace.payload_body_end_bit),
            base_bit,
        ),
    }
}

fn frame_slice<'a>(stream: &'a [u8], entry: &SyncframeIndexEntry) -> Result<&'a [u8], Eac3Error> {
    let end = entry
        .offset
        .checked_add(entry.header.frame_size)
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    stream
        .get(entry.offset..end)
        .ok_or(Eac3Error::TruncatedFrame {
            offset: entry.offset,
            declared: entry.header.frame_size,
            available: stream.len().saturating_sub(entry.offset),
        })
}

fn span(start_bit: usize, end_bit: usize) -> BitSpan {
    BitSpan {
        start_bit,
        end_bit,
        length_bits: end_bit.saturating_sub(start_bit),
    }
}

fn shift_span(value: &BitSpan, base_bit: usize) -> BitSpan {
    span(
        base_bit.saturating_add(value.start_bit),
        base_bit.saturating_add(value.end_bit),
    )
}

fn bit_window(bytes: &[u8], start_bit: usize, end_bit: usize) -> String {
    (start_bit..end_bit)
        .map(|bit| {
            let byte = bytes[bit / 8];
            char::from(b'0' + ((byte >> (7 - bit % 8)) & 1))
        })
        .collect()
}

fn hex_window(bytes: &[u8], start_bit: usize, end_bit: usize) -> String {
    let start = start_bit / 8;
    let end = end_bit.saturating_add(7) / 8;
    hex_bytes(bytes.get(start..end).unwrap_or_default())
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn render_text(report: &ForensicReport) -> String {
    use std::fmt::Write as _;
    let mut text = String::new();
    let _ = writeln!(text, "input: {}", report.input);
    let _ = writeln!(text, "input_media: {}", report.input_media);
    let _ = writeln!(
        text,
        "coordinate_convention: {}",
        report.coordinate_convention
    );
    let _ = writeln!(
        text,
        "syncframes/access_units: {}/{}",
        report.syncframe_count, report.access_unit_count
    );
    for observation in &report.observations {
        let _ = writeln!(
            text,
            "access_unit={} mp4_sample_index={:?} carrier_frame={} substream_id={}",
            observation.access_unit_index,
            observation.mp4_sample_index,
            observation.carrier_frame_index,
            observation.substream_id
        );
        let _ = writeln!(
            text,
            "  original_file_bit_offset={:?} ({})",
            observation.original_file_bit_offset, observation.original_file_coordinate_note
        );
        let _ = writeln!(
            text,
            "  elementary_stream_bit_offset={} access_unit_relative_bit_offset={}",
            observation.elementary_stream_bit_offset, observation.access_unit_relative_bit_offset
        );
        let _ = writeln!(
            text,
            "  skip_field frame={:?} original_file={:?} elementary_stream={:?} access_unit={:?}",
            observation.skip_field_span_frame,
            observation.skip_field_span_original_file,
            observation.skip_field_span_elementary_stream,
            observation.skip_field_span_access_unit
        );
        let _ = writeln!(
            text,
            "  emdf skip_field={:?} elementary_stream={:?} access_unit={:?} original_file={:?} bytes={}",
            observation.emdf_span_skip_field,
            observation.emdf_span_elementary_stream,
            observation.emdf_span_access_unit,
            observation.emdf_span_original_file,
            observation.emdf_container_bytes
        );
        for payload in &observation.emdf_payloads {
            let _ = writeln!(
                text,
                "  payload {} id={:?} config={:?} size={:?} body={:?}",
                payload.payload_id,
                payload.id_span_emdf,
                payload.config_span_emdf,
                payload.size_span_emdf,
                payload.body_span_emdf
            );
        }
        if let Some(oamd) = &observation.oamd {
            let _ = writeln!(
                text,
                "  oamd payload={:?} object_count={} element_count={} trim_index={:?}",
                oamd.payload_span_emdf,
                oamd.object_count,
                oamd.element_count,
                oamd.trim_element_index
            );
            let _ = writeln!(
                text,
                "  warp span={:?} raw={:?} window_bits={:?} before={} after={} bytes={:?}",
                oamd.warp_mode_span_oamd,
                oamd.warp_mode_raw,
                oamd.warp_window_bits,
                oamd.warp_window_before_bits.unwrap_or_default(),
                oamd.warp_window_after_bits.unwrap_or_default(),
                oamd.warp_window_bytes_hex
            );
        }
        let _ = writeln!(
            text,
            "  trim_config_count={:?} validator={}",
            observation.trim_config_count,
            observation.parse_error.as_deref().unwrap_or("none")
        );
    }
    text
}

fn media_kind_name(kind: InputMediaKind) -> &'static str {
    match kind {
        InputMediaKind::RawEac3 => "raw_eac3",
        InputMediaKind::IsoBmff => "iso_bmff",
        InputMediaKind::Unknown => "unknown",
    }
}

fn usage_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "usage: openjoc diagnose-oamd FILE -o DIR [--access-unit N | --all-access-units] [--trim-config-count N]",
    )
}
