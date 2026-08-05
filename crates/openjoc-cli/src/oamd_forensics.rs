// pattern: Mixed (bounded extraction shell around pure bit-span diagnostics)

use openjoc_container::{InputMediaKind, load_eac3};
use openjoc_eac3::{
    AccessUnitIndex, Eac3Error, SyncframeIndexEntry, classify_skip_field_emdf, group_access_units,
    index_syncframes, inspect_audio_block_carriers,
};
use openjoc_emdf::{EmdfPayload, EmdfPayloadBitTrace, parse_emdf_sync_with_bit_trace};
use openjoc_joc::{JocPayloadData, parse_joc_payload};
use openjoc_oamd::{
    OamdBitTrace, OamdDecoderConfig, OamdElement, OamdParseProfile, OpaqueObservedKnownElement,
    parse_oamd_payload_with_profile, trace_oamd_payload,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fmt::Write as _,
    fs, io,
    num::NonZeroU8,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
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
    oracle: Option<crate::oamd_oracle::OracleTrace>,
    oracle_error: Option<String>,
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
    start_sample: u64,
    end_sample: u64,
    start_seconds: f64,
    end_seconds: f64,
    payload_11_sha256: Option<String>,
    payload_11_changed_from_previous: Option<bool>,
    oamd_parse_stage: String,
    warp_raw: Option<u8>,
    vendor_oamd: Option<VendorOamdEvidence>,
    joc: Option<JocEvidence>,
}

#[derive(Clone, Debug, Serialize)]
struct JocEvidence {
    payload_size_bytes: usize,
    payload_sha256: String,
    result: &'static str,
    error: Option<String>,
    downmix_channel_count: Option<u8>,
    output_object_count: Option<u8>,
    sequence_count: Option<u16>,
    present_object_count: Option<usize>,
    data_point_count: Option<usize>,
    sparse_object_count: Option<usize>,
    full_object_count: Option<usize>,
    codeword_count: Option<usize>,
    nonzero_codeword_count: Option<usize>,
    steep_data_point_count: Option<usize>,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Serialize)]
struct VendorOamdEvidence {
    profile: &'static str,
    result: &'static str,
    error: Option<String>,
    oamd_payload_structurally_accepted: bool,
    oamd_semantically_complete: bool,
    object_metadata_status: &'static str,
    trim_metadata_status: &'static str,
    trim_timeline_available: bool,
    renderer_fidelity_eligible: bool,
    object_count: u16,
    dynamic_object_count: usize,
    element_count: u8,
    metadata_block_count: Option<usize>,
    object_update_count: Option<usize>,
    active_update_count: Option<usize>,
    position_field_count: Option<usize>,
    first_update_sample: Option<u16>,
    positive_ramp_count: Option<usize>,
    opaque_elements: Vec<OpaqueElementEvidence>,
}

#[derive(Clone, Debug, Serialize)]
struct OpaqueElementEvidence {
    element_id: u8,
    declared_bits: usize,
    declared_bytes: usize,
    valid_bits_in_last_byte: u8,
    raw_body_sha256: String,
    raw_warp: u8,
    warp_element_relative_span: BitSpan,
    warp_payload_relative_span: BitSpan,
    first_parser_error: String,
    deviation_code: &'static str,
}

#[derive(Clone, Debug, Serialize)]
struct Payload11Unique {
    sha256: String,
    first_au: usize,
    last_au: usize,
    occurrence_count: usize,
    payload_size_bytes: usize,
    changed_bit_intervals_compared_with_previous_unique: Vec<BitSpan>,
    changed_bytes: Vec<usize>,
    warp_raw: Option<u8>,
    warp_changed_from_previous_unique: Option<bool>,
    element_body_hashes: Vec<String>,
    element_boundaries: Vec<BitSpan>,
    metadata_block_count: Option<usize>,
    parse_stage: String,
}

#[derive(Clone, Debug, Serialize)]
struct Payload11Transition {
    from_au: usize,
    to_au: usize,
    from_sha256: String,
    to_sha256: String,
    changed_bit_intervals: Vec<BitSpan>,
    changed_bytes: Vec<usize>,
    warp_changed: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
struct Payload11DiffReport {
    unique_payload_count: usize,
    unique_payloads: Vec<Payload11Unique>,
    transitions: Vec<Payload11Transition>,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Serialize)]
struct WarpHypothesisEvidence {
    raw_warp: u8,
    assumed_semantics: u8,
    diagnostic_only: bool,
    bounded_element_closed: bool,
    payload_closed: bool,
    reserved_after_warp_raw: Option<u8>,
    object_count: Option<u16>,
    element_count: Option<u8>,
    metadata_block_count: Option<usize>,
    update_count: Option<usize>,
    position_count: Option<usize>,
    jump_count: Option<usize>,
    ramp_count: Option<usize>,
    non_finite_values: bool,
    adm_timing_correspondence: String,
    adm_movement_correspondence: String,
    status: String,
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
    timing_grid_seconds: Option<f64>,
    payload_11_diff: Option<Payload11DiffReport>,
    adm_reference: Option<String>,
    warp_hypotheses: Option<Vec<WarpHypothesisEvidence>>,
}

pub fn run(values: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let input = values.first().filter(|value| !value.starts_with('-'));
    let mut output = None;
    let mut requested_access_unit = None;
    let mut requested_range = None;
    let mut trim_config_count = None;
    let mut all_access_units = false;
    let mut diff_payload_11 = false;
    let mut json_output = None;
    let mut warp_hypotheses = false;
    let mut adm_reference = None;
    let mut force = false;
    let mut index = 1;
    while index < values.len() {
        let flag = &values[index];
        if flag == "--all-access-units" {
            all_access_units = true;
            index += 1;
            continue;
        }
        if flag == "--diff-payload-11" {
            diff_payload_11 = true;
            index += 1;
            continue;
        }
        if flag == "--warp-hypotheses" {
            warp_hypotheses = true;
            index += 1;
            continue;
        }
        if flag == "--force" {
            force = true;
            index += 1;
            continue;
        }
        let value = values.get(index + 1).ok_or_else(usage_error)?;
        match flag.as_str() {
            "-o" | "--output" => output = Some(PathBuf::from(value)),
            "--json" => json_output = Some(PathBuf::from(value)),
            "--adm-reference" => adm_reference = Some(value.to_owned()),
            "--access-unit" => {
                requested_access_unit = Some(value.parse::<usize>().map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidInput, "invalid access-unit index")
                })?);
            }
            "--au" => requested_range = Some(parse_access_unit_range(value)?),
            "--trim-config-count" => trim_config_count = Some(parse_trim_count(value)?),
            _ => return Err(usage_error().into()),
        }
        index += 2;
    }
    let input = PathBuf::from(input.ok_or_else(usage_error)?);
    let output = output
        .or_else(|| {
            json_output
                .as_ref()
                .and_then(|path| path.parent().map(PathBuf::from))
        })
        .ok_or_else(usage_error)?;
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
    let selected = selected_units(
        units.len(),
        requested_access_unit,
        requested_range,
        all_access_units,
    )?;
    let mut observations = selected
        .iter()
        .map(|&unit_index| observe_access_unit(&forensic_input, unit_index, trim_config_count))
        .collect::<Result<Vec<_>, _>>()?;
    annotate_payload_11_changes(&mut observations);
    let timing_grid_seconds = units
        .first()
        .map(|unit| f64::from(unit.samples) / f64::from(unit.sample_rate));
    let payload_11_diff = diff_payload_11.then(|| build_payload_11_diff(&observations));
    let warp_hypothesis_report = warp_hypotheses.then(|| build_warp_hypotheses(&observations));
    let report = ForensicReport {
        input: input.display().to_string(),
        input_media: media_kind_name(media.kind).to_owned(),
        coordinate_convention: "All bit spans are MSB-first and half-open [start,end). PayloadEvidence.coordinate_spans names bounded skip-field, elementary-stream, access-unit, and original-file coordinates. OamdEvidence payload spans use the same coordinates; OamdEvidence warp_mode_span_oamd is relative to the OAMD payload start, while its other warp spans name the parent coordinate directly.".to_owned(),
        syncframe_count: frames.len(),
        access_unit_count: units.len(),
        selected_access_units: selected,
        observations,
        timing_grid_seconds,
        payload_11_diff,
        adm_reference,
        warp_hypotheses: warp_hypothesis_report,
    };
    let (json_path, text_path) = if let Some(path) = json_output {
        (path.clone(), path.with_extension("txt"))
    } else {
        (
            output.join("oamd_forensics.json"),
            output.join("oamd_forensics.txt"),
        )
    };
    fs::create_dir_all(&output)?;
    if let Some(parent) = json_path.parent() {
        fs::create_dir_all(parent)?;
    }
    ensure_report_targets_available(&[&json_path, &text_path], force)?;
    let json_text = format!("{}\n", serde_json::to_string_pretty(&report)?);
    fs::write(&json_path, &json_text)?;
    fs::write(&text_path, render_text(&report))?;
    println!(
        "oamd-forensics: {} observations written to {}",
        report.observations.len(),
        output.display()
    );
    Ok(())
}

fn ensure_report_targets_available(paths: &[&Path], force: bool) -> io::Result<()> {
    if force {
        return Ok(());
    }
    if let Some(path) = paths.iter().find(|path| path.exists()) {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "report output already exists: {}; pass --force to overwrite",
                path.display()
            ),
        ));
    }
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
    requested_range: Option<(usize, usize)>,
    all: bool,
) -> Result<Vec<usize>, io::Error> {
    if all && (requested.is_some() || requested_range.is_some()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--all-access-units cannot be combined with --access-unit or --au",
        ));
    }
    if requested.is_some() && requested_range.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--access-unit cannot be combined with --au",
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
    if let Some((start, end)) = requested_range {
        if start > end || end >= count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("access-unit range {start}..{end} is outside 0..{count}"),
            ));
        }
        return Ok((start..=end).collect());
    }
    let mut selected = vec![0, count / 2, count - 1];
    selected.sort_unstable();
    selected.dedup();
    Ok(selected)
}

fn parse_access_unit_range(value: &str) -> Result<(usize, usize), io::Error> {
    let (start, end) = value.split_once("..").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid --au range; expected START..END (inclusive)",
        )
    })?;
    let start = start
        .parse::<usize>()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid --au range start"))?;
    let end = end
        .parse::<usize>()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid --au range end"))?;
    Ok((start, end))
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
    let payload_11_sha256 = Some(sha256_hex(&payload.data));
    let oamd_parse_stage = classify_oamd_parse_stage(trim_config_count, &validator_result);
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
    let oracle_result = crate::oamd_oracle::trace_observed_payload(&payload.data);
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
            oracle_result.as_ref().ok(),
            oracle_result.as_ref().err().map(ToString::to_string),
        )
    });
    let vendor_oamd = trim_config_count.map(|count| {
        build_vendor_oamd_evidence(
            &payload.data,
            OamdDecoderConfig {
                trim_configuration_count: Some(count),
            },
        )
    });
    let joc = emdf_trace
        .parsed
        .container
        .payloads
        .iter()
        .find(|candidate| candidate.id == 14)
        .map(build_joc_evidence);
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
        start_sample: units_sample_start(input.units, unit_index),
        end_sample: units_sample_start(input.units, unit_index)
            .saturating_add(u64::from(unit.samples)),
        start_seconds: units_sample_start(input.units, unit_index) as f64
            / f64::from(unit.sample_rate),
        end_seconds: (units_sample_start(input.units, unit_index)
            .saturating_add(u64::from(unit.samples))) as f64
            / f64::from(unit.sample_rate),
        payload_11_sha256,
        payload_11_changed_from_previous: None,
        oamd_parse_stage,
        warp_raw: oamd_trace.as_ref().and_then(|trace| {
            trace
                .elements
                .iter()
                .find(|element| element.id == 2)
                .and_then(|element| element.warp_mode_raw)
        }),
        vendor_oamd,
        joc,
    })
}

fn build_joc_evidence(payload: &EmdfPayload) -> JocEvidence {
    let payload_size_bytes = payload.data.len();
    let payload_sha256 = sha256_hex(&payload.data);
    match parse_joc_payload(&payload.data) {
        Ok(frame) => {
            let present_objects = frame.objects.iter().filter(|object| object.present).count();
            let data_point_count = frame
                .objects
                .iter()
                .map(|object| object.data_points.len())
                .sum::<usize>();
            let sparse_object_count = frame
                .objects
                .iter()
                .filter(|object| {
                    object.present
                        && object
                            .data_points
                            .iter()
                            .any(|point| matches!(point.payload, JocPayloadData::Sparse { .. }))
                })
                .count();
            let full_object_count = frame
                .objects
                .iter()
                .filter(|object| {
                    object.present
                        && object
                            .data_points
                            .iter()
                            .any(|point| matches!(point.payload, JocPayloadData::Full { .. }))
                })
                .count();
            let codeword_count = frame
                .objects
                .iter()
                .flat_map(|object| &object.data_points)
                .map(|point| match &point.payload {
                    JocPayloadData::Sparse {
                        channel_deltas,
                        vector_symbols,
                        ..
                    } => channel_deltas.len() + vector_symbols.len(),
                    JocPayloadData::Full { matrix_symbols } => {
                        matrix_symbols.iter().map(Vec::len).sum()
                    }
                })
                .sum::<usize>();
            let nonzero_codeword_count = frame
                .objects
                .iter()
                .flat_map(|object| &object.data_points)
                .flat_map(|point| match &point.payload {
                    JocPayloadData::Sparse {
                        channel_deltas,
                        vector_symbols,
                        ..
                    } => channel_deltas
                        .iter()
                        .chain(vector_symbols)
                        .collect::<Vec<_>>(),
                    JocPayloadData::Full { matrix_symbols } => matrix_symbols
                        .iter()
                        .flat_map(|row| row.iter())
                        .collect::<Vec<_>>(),
                })
                .filter(|codeword| codeword.symbol != 0)
                .count();
            let steep_data_point_count = frame
                .objects
                .iter()
                .flat_map(|object| &object.data_points)
                .filter(|point| point.offset_timeslot.is_some())
                .count();
            JocEvidence {
                payload_size_bytes,
                payload_sha256,
                result: "parsed",
                error: None,
                downmix_channel_count: Some(frame.header.channel_count),
                output_object_count: Some(frame.header.object_count),
                sequence_count: Some(frame.sequence_count),
                present_object_count: Some(present_objects),
                data_point_count: Some(data_point_count),
                sparse_object_count: Some(sparse_object_count),
                full_object_count: Some(full_object_count),
                codeword_count: Some(codeword_count),
                nonzero_codeword_count: Some(nonzero_codeword_count),
                steep_data_point_count: Some(steep_data_point_count),
            }
        }
        Err(error) => JocEvidence {
            payload_size_bytes,
            payload_sha256,
            result: "failed",
            error: Some(error.to_string()),
            downmix_channel_count: None,
            output_object_count: None,
            sequence_count: None,
            present_object_count: None,
            data_point_count: None,
            sparse_object_count: None,
            full_object_count: None,
            codeword_count: None,
            nonzero_codeword_count: None,
            steep_data_point_count: None,
        },
    }
}

fn build_vendor_oamd_evidence(payload: &[u8], config: OamdDecoderConfig) -> VendorOamdEvidence {
    match parse_oamd_payload_with_profile(
        payload,
        config,
        OamdParseProfile::DolbyVendorCompat,
        openjoc_oamd::OAMD_PAYLOAD_ID,
    ) {
        Ok(parsed) => {
            let opaque_elements = parsed
                .elements
                .iter()
                .filter_map(|metadata| match &metadata.element {
                    OamdElement::OpaqueObservedKnownElement(opaque) => {
                        Some(opaque_element_evidence(opaque))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            VendorOamdEvidence {
                profile: "DOLBY_VENDOR_COMPAT",
                result: if opaque_elements.is_empty() {
                    "accepted"
                } else {
                    "accepted_with_deviation"
                },
                error: None,
                oamd_payload_structurally_accepted: true,
                oamd_semantically_complete: opaque_elements.is_empty(),
                object_metadata_status: if parsed
                    .elements
                    .iter()
                    .any(|metadata| matches!(metadata.element, OamdElement::Objects(_)))
                {
                    "parsed"
                } else {
                    "blocked"
                },
                trim_metadata_status: if opaque_elements.is_empty() {
                    "parsed_or_absent"
                } else {
                    "opaque_unresolved"
                },
                trim_timeline_available: false,
                renderer_fidelity_eligible: false,
                object_count: parsed.prefix.object_count,
                dynamic_object_count: parsed
                    .object_classes
                    .iter()
                    .filter(|class| matches!(class, openjoc_oamd::ObjectClass::Dynamic))
                    .count(),
                element_count: parsed.prefix.element_count,
                metadata_block_count: object_element(&parsed)
                    .map(|element| element.timing.blocks.len()),
                object_update_count: object_element(&parsed)
                    .map(|element| element.objects.iter().map(Vec::len).sum()),
                active_update_count: object_element(&parsed).map(|element| {
                    element
                        .objects
                        .iter()
                        .flatten()
                        .filter(|update| update.active)
                        .count()
                }),
                position_field_count: object_element(&parsed).map(|element| {
                    element
                        .objects
                        .iter()
                        .flatten()
                        .filter(|update| update.active)
                        .count()
                }),
                first_update_sample: object_element(&parsed)
                    .and_then(|element| element.timing.blocks.first())
                    .map(|block| block.start_sample),
                positive_ramp_count: object_element(&parsed).map(|element| {
                    element
                        .timing
                        .blocks
                        .iter()
                        .filter(|block| block.ramp_duration > 0)
                        .count()
                }),
                opaque_elements,
            }
        }
        Err(error) => VendorOamdEvidence {
            profile: "DOLBY_VENDOR_COMPAT",
            result: "failed",
            error: Some(error.to_string()),
            oamd_payload_structurally_accepted: false,
            oamd_semantically_complete: false,
            object_metadata_status: "blocked",
            trim_metadata_status: "blocked",
            trim_timeline_available: false,
            renderer_fidelity_eligible: false,
            object_count: 0,
            dynamic_object_count: 0,
            element_count: 0,
            metadata_block_count: None,
            object_update_count: None,
            active_update_count: None,
            position_field_count: None,
            first_update_sample: None,
            positive_ramp_count: None,
            opaque_elements: Vec::new(),
        },
    }
}

fn object_element(payload: &openjoc_oamd::OamdPayload) -> Option<&openjoc_oamd::ObjectElement> {
    payload.elements.iter().find_map(|metadata| {
        if let OamdElement::Objects(objects) = &metadata.element {
            Some(objects)
        } else {
            None
        }
    })
}

fn opaque_element_evidence(opaque: &OpaqueObservedKnownElement) -> OpaqueElementEvidence {
    OpaqueElementEvidence {
        element_id: opaque.element_id,
        declared_bits: opaque.declared_bits,
        declared_bytes: opaque.declared_bytes,
        valid_bits_in_last_byte: opaque.valid_bits_in_last_byte,
        raw_body_sha256: opaque.raw_body_sha256.clone(),
        raw_warp: opaque.raw_warp,
        warp_element_relative_span: span(
            opaque.warp_element_relative_start_bit,
            opaque.warp_element_relative_end_bit,
        ),
        warp_payload_relative_span: span(
            opaque.warp_payload_start_bit,
            opaque.warp_payload_end_bit,
        ),
        first_parser_error: opaque.first_parser_error.to_string(),
        deviation_code: opaque.deviation_code,
    }
}

fn units_sample_start(units: &[AccessUnitIndex], index: usize) -> u64 {
    units
        .iter()
        .take(index)
        .map(|unit| u64::from(unit.samples))
        .sum()
}

fn classify_oamd_parse_stage(
    trim_config_count: Option<NonZeroU8>,
    validator_result: &str,
) -> String {
    if trim_config_count.is_none() {
        return "not_attempted_without_trim_config_count".to_owned();
    }
    if validator_result == "accepted" {
        "accepted".to_owned()
    } else if validator_result.contains("reserved OAMD warp mode") {
        "trim.warp_mode".to_owned()
    } else {
        validator_result.to_owned()
    }
}

fn build_oamd_evidence(
    trace: &OamdBitTrace,
    payload_trace: &EmdfPayloadBitTrace,
    payload: &EmdfPayload,
    trim_config_count: Option<NonZeroU8>,
    context: &OamdTraceContext<'_>,
    oracle: Option<&crate::oamd_oracle::OracleTrace>,
    oracle_error: Option<String>,
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
        oracle: oracle.cloned(),
        oracle_error,
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

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn decode_hex_bytes(value: &str) -> Vec<u8> {
    value
        .split_whitespace()
        .filter_map(|pair| u8::from_str_radix(pair, 16).ok())
        .collect()
}

fn annotate_payload_11_changes(observations: &mut [ForensicObservation]) {
    let mut previous: Option<String> = None;
    for observation in observations {
        observation.payload_11_changed_from_previous = observation
            .payload_11_sha256
            .as_ref()
            .map(|current| previous.as_ref().is_some_and(|prior| prior != current));
        previous.clone_from(&observation.payload_11_sha256);
    }
}

fn changed_spans(left: &[u8], right: &[u8], bit_len: usize) -> Vec<BitSpan> {
    let mut spans = Vec::new();
    let mut start = None;
    for bit in 0..bit_len {
        let left_bit = left
            .get(bit / 8)
            .map_or(0, |byte| (byte >> (7 - bit % 8)) & 1);
        let right_bit = right
            .get(bit / 8)
            .map_or(0, |byte| (byte >> (7 - bit % 8)) & 1);
        if left_bit != right_bit {
            start.get_or_insert(bit);
        } else if let Some(begin) = start.take() {
            spans.push(span(begin, bit));
        }
    }
    if let Some(begin) = start {
        spans.push(span(begin, bit_len));
    }
    spans
}

fn changed_bytes(left: &[u8], right: &[u8]) -> Vec<usize> {
    (0..left.len().max(right.len()))
        .filter(|&index| left.get(index) != right.get(index))
        .collect()
}

fn payload_11(observation: &ForensicObservation) -> Option<&PayloadEvidence> {
    observation
        .emdf_payloads
        .iter()
        .find(|payload| payload.payload_id == 11)
}

fn build_payload_11_diff(observations: &[ForensicObservation]) -> Payload11DiffReport {
    let mut unique_payloads = Vec::<Payload11Unique>::new();
    let mut transitions = Vec::new();
    let mut previous: Option<(&ForensicObservation, Vec<u8>)> = None;
    for observation in observations {
        let Some(payload) = payload_11(observation) else {
            continue;
        };
        let bytes = decode_hex_bytes(&payload.body_bytes_hex);
        let hash = sha256_hex(&bytes);
        if let Some((prior_observation, prior_bytes)) = previous {
            if hash != sha256_hex(&prior_bytes) {
                transitions.push(Payload11Transition {
                    from_au: prior_observation.access_unit_index,
                    to_au: observation.access_unit_index,
                    from_sha256: sha256_hex(&prior_bytes),
                    to_sha256: hash.clone(),
                    changed_bit_intervals: changed_spans(
                        &prior_bytes,
                        &bytes,
                        prior_bytes.len().min(bytes.len()).saturating_mul(8),
                    ),
                    changed_bytes: changed_bytes(&prior_bytes, &bytes),
                    warp_changed: Some(prior_observation.warp_raw != observation.warp_raw),
                });
            }
        }
        let current_index = unique_payloads
            .iter()
            .position(|entry| entry.sha256 == hash);
        if let Some(index) = current_index {
            unique_payloads[index].last_au = observation.access_unit_index;
            unique_payloads[index].occurrence_count += 1;
        } else {
            let previous_unique = unique_payloads.last();
            let (changed_bit_intervals, changed_byte_indices, warp_changed) = previous_unique
                .map_or((Vec::new(), Vec::new(), None), |entry| {
                    let prior_obs = observations
                        .iter()
                        .find(|candidate| candidate.access_unit_index == entry.last_au);
                    let prior_bytes = prior_obs
                        .and_then(payload_11)
                        .map(|value| decode_hex_bytes(&value.body_bytes_hex))
                        .unwrap_or_default();
                    (
                        changed_spans(
                            &prior_bytes,
                            &bytes,
                            prior_bytes.len().min(bytes.len()).saturating_mul(8),
                        ),
                        changed_bytes(&prior_bytes, &bytes),
                        prior_obs.map(|prior| prior.warp_raw != observation.warp_raw),
                    )
                });
            let element_body_hashes = observation
                .oamd
                .as_ref()
                .map(|oamd| {
                    oamd.elements
                        .iter()
                        .filter_map(|element| {
                            let start = element.body_start_bit;
                            let end = element.body_end_bit;
                            if end <= bytes.len().saturating_mul(8) {
                                Some(sha256_hex(&bit_slice(&bytes, start, end)))
                            } else {
                                None
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            let element_boundaries = observation
                .oamd
                .as_ref()
                .map(|oamd| {
                    oamd.elements
                        .iter()
                        .map(|element| span(element.body_start_bit, element.body_end_bit))
                        .collect()
                })
                .unwrap_or_default();
            unique_payloads.push(Payload11Unique {
                sha256: hash.clone(),
                first_au: observation.access_unit_index,
                last_au: observation.access_unit_index,
                occurrence_count: 1,
                payload_size_bytes: bytes.len(),
                changed_bit_intervals_compared_with_previous_unique: changed_bit_intervals,
                changed_bytes: changed_byte_indices,
                warp_raw: observation.warp_raw,
                warp_changed_from_previous_unique: warp_changed,
                element_body_hashes,
                element_boundaries,
                metadata_block_count: observation.oamd.as_ref().map(|oamd| oamd.elements.len()),
                parse_stage: observation.oamd_parse_stage.clone(),
            });
        }
        previous = Some((observation, bytes));
    }
    Payload11DiffReport {
        unique_payload_count: unique_payloads.len(),
        unique_payloads,
        transitions,
    }
}

fn bit_slice(bytes: &[u8], start: usize, end: usize) -> Vec<u8> {
    let mut result = vec![0_u8; end.saturating_sub(start).saturating_add(7) / 8];
    for (index, bit) in (start..end).enumerate() {
        let value = (bytes.get(bit / 8).copied().unwrap_or_default() >> (7 - bit % 8)) & 1;
        result[index / 8] |= value << (7 - index % 8);
    }
    result
}

fn build_warp_hypotheses(observations: &[ForensicObservation]) -> Vec<WarpHypothesisEvidence> {
    let first = observations.iter().find_map(|observation| {
        let oamd = observation.oamd.as_ref()?;
        let oracle = oamd.oracle.as_ref()?;
        let reserved = oracle
            .fields
            .iter()
            .find(|field| field.name.ends_with("reserved_after_warp"))
            .map(|field| field.integer_value as u8);
        Some((oamd, oracle, reserved))
    });
    let Some((oamd, oracle, reserved_after_warp_raw)) = first else {
        return Vec::new();
    };
    (0..=2)
        .map(|assumed_semantics| WarpHypothesisEvidence {
            raw_warp: oracle.warp_raw,
            assumed_semantics,
            diagnostic_only: true,
            bounded_element_closed: oamd
                .elements
                .iter()
                .all(|element| element.body_end_bit <= oamd.payload_bits),
            payload_closed: oamd
                .elements
                .last()
                .is_some_and(|element| element.body_end_bit <= oamd.payload_bits),
            reserved_after_warp_raw,
            object_count: Some(oamd.object_count),
            element_count: Some(oamd.element_count),
            metadata_block_count: Some(oamd.elements.len()),
            update_count: None,
            position_count: None,
            jump_count: None,
            ramp_count: None,
            non_finite_values: false,
            adm_timing_correspondence: "not_evaluable before normative object-element decode"
                .to_owned(),
            adm_movement_correspondence: "not_evaluable before normative object-element decode"
                .to_owned(),
            status: "bounded syntax closes; semantic hypothesis is non-unique and diagnostic-only"
                .to_owned(),
        })
        .collect()
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
    if let Some(grid) = report.timing_grid_seconds {
        let _ = writeln!(text, "timing_grid_seconds: {grid:.9}");
    }
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
            "  timing samples=[{}, {}) seconds=[{:.9}, {:.9}) payload_11_sha256={} changed_from_previous={:?} parse_stage={} warp_raw={:?}",
            observation.start_sample,
            observation.end_sample,
            observation.start_seconds,
            observation.end_seconds,
            observation.payload_11_sha256.as_deref().unwrap_or("none"),
            observation.payload_11_changed_from_previous,
            observation.oamd_parse_stage,
            observation.warp_raw
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
    if let Some(diff) = &report.payload_11_diff {
        let _ = writeln!(
            text,
            "payload_11_unique_count: {}",
            diff.unique_payload_count
        );
        for unique in &diff.unique_payloads {
            let _ = writeln!(
                text,
                "payload_11 unique sha256={} AU {}..{} occurrences={} bytes={} warp={:?} changed_bits={:?} changed_bytes={:?} elements={:?}",
                unique.sha256,
                unique.first_au,
                unique.last_au,
                unique.occurrence_count,
                unique.payload_size_bytes,
                unique.warp_raw,
                unique.changed_bit_intervals_compared_with_previous_unique,
                unique.changed_bytes,
                unique.element_boundaries
            );
        }
        for transition in &diff.transitions {
            let _ = writeln!(
                text,
                "payload_11 transition AU {} -> {} changed_bits={:?} changed_bytes={:?} warp_changed={:?}",
                transition.from_au,
                transition.to_au,
                transition.changed_bit_intervals,
                transition.changed_bytes,
                transition.warp_changed
            );
        }
    }
    if let Some(reference) = &report.adm_reference {
        let _ = writeln!(text, "adm_reference: {reference}");
    }
    if let Some(hypotheses) = &report.warp_hypotheses {
        for hypothesis in hypotheses {
            let _ = writeln!(
                text,
                "warp_hypothesis raw={} assumed={} diagnostic_only={} element_closed={} payload_closed={} status={}",
                hypothesis.raw_warp,
                hypothesis.assumed_semantics,
                hypothesis.diagnostic_only,
                hypothesis.bounded_element_closed,
                hypothesis.payload_closed,
                hypothesis.status
            );
        }
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
        "usage: openjoc diagnose-oamd FILE [-o DIR] [--access-unit N | --au START..END | --all-access-units] [--trim-config-count N] [--diff-payload-11] [--warp-hypotheses] [--adm-reference PATH] [--json PATH] [--force]",
    )
}

#[cfg(test)]
mod tests {
    use super::{changed_spans, ensure_report_targets_available, parse_access_unit_range, span};
    use std::fs;

    #[test]
    fn parses_inclusive_access_unit_ranges() {
        assert_eq!(parse_access_unit_range("14..17").expect("range"), (14, 17));
        assert!(parse_access_unit_range("14-17").is_err());
    }

    #[test]
    fn payload_diff_reports_exact_bit_intervals_and_bytes() {
        let left = [0b1010_0000_u8, 0];
        let right = [0b1001_0000_u8, 0b0000_0001];
        assert_eq!(
            changed_spans(&left, &right, 16),
            vec![span(2, 4), span(15, 16)]
        );
    }

    #[test]
    fn report_output_refuses_existing_targets_without_force() {
        let root =
            std::env::temp_dir().join(format!("openjoc-oamd-report-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("temporary report directory");
        let json = root.join("report.json");
        let text = root.join("report.txt");
        fs::write(&json, "old json\n").expect("seed json");
        assert_eq!(
            ensure_report_targets_available(&[json.as_path(), text.as_path()], false)
                .expect_err("existing report must be rejected")
                .kind(),
            std::io::ErrorKind::AlreadyExists
        );
        ensure_report_targets_available(&[json.as_path(), text.as_path()], true)
            .expect("force permits overwrite");
        let _ = fs::remove_dir_all(root);
    }
}
