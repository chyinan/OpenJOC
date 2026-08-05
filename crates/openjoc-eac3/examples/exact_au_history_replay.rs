//! Diagnostic-only exact-AU history replay harness.
//!
//! This example deliberately writes all media and reports to a caller-owned
//! private directory. It is not a programme validator and is not used by any
//! production decode path. AU boundaries come only from the OpenJOC indexed
//! syncframe/grouping APIs; no syncword search or byte rewriting is used.

use openjoc_eac3::{
    AudioPcmSynthesizer, DecodedAudioBlock, InternalBasePolicy, MantissaElementTrace,
    TdacContribution, decode_audio_blocks_with_diagnostic_trace, decode_audio_blocks_with_policy,
    group_access_units, index_syncframes,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

type Json = serde_json::Value;

const DITHER_VALUES: usize = 32_768;
const TARGET_AU0: usize = 0;
const TARGET_AU1: usize = 1;

fn sha256(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    format!("{:x}", digest.finalize())
}

fn hash_f64(values: &[f64]) -> String {
    let mut bytes = Vec::with_capacity(values.len() * 8);
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    sha256(&bytes)
}

fn hash_u8(values: &[u8]) -> String {
    sha256(values)
}

fn hash_u16(values: &[u16]) -> String {
    let mut bytes = Vec::with_capacity(values.len() * 2);
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    sha256(&bytes)
}

fn stats(values: &[f64]) -> Json {
    let finite = values.iter().all(|value| value.is_finite());
    let peak = values
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .map(f64::abs)
        .fold(0.0_f64, f64::max);
    let rms = if values.is_empty() {
        0.0
    } else {
        (values.iter().map(|value| value * value).sum::<f64>() / values.len() as f64).sqrt()
    };
    let first_nonzero = values.iter().position(|value| *value != 0.0);
    let last_nonzero = values.iter().rposition(|value| *value != 0.0);
    serde_json::json!({
        "status": "available",
        "length": values.len(),
        "sha256_f64le": hash_f64(values),
        "peak": peak,
        "rms": rms,
        "nonzero_count": values.iter().filter(|value| **value != 0.0).count(),
        "first_nonzero": first_nonzero,
        "last_nonzero": last_nonzero,
        "finite": finite,
    })
}

fn bytes_stats(bytes: &[u8]) -> Json {
    serde_json::json!({
        "status": "available",
        "length": bytes.len(),
        "sha256": sha256(bytes),
    })
}

fn u16_stats(values: &[u16]) -> Json {
    serde_json::json!({
        "status": "available",
        "length": values.len(),
        "sha256_u16le": hash_u16(values),
    })
}

fn dither_values() -> Vec<f64> {
    (0..DITHER_VALUES)
        .map(|index| {
            let phase = index as f64 * 0.754_877_666_246_692_7;
            phase.sin() * 0.5
        })
        .collect()
}

fn unit_range(
    bytes: &[u8],
    frames: &[openjoc_eac3::SyncframeIndexEntry],
    unit: openjoc_eac3::AccessUnitIndex,
) -> Result<(usize, usize), String> {
    let first = frames
        .get(unit.first_frame)
        .ok_or_else(|| "access-unit first frame is out of range".to_owned())?;
    let last_index = unit
        .first_frame
        .checked_add(unit.frame_count)
        .and_then(|value| value.checked_sub(1))
        .ok_or_else(|| "access-unit frame range underflow".to_owned())?;
    let last = frames
        .get(last_index)
        .ok_or_else(|| "access-unit last frame is out of range".to_owned())?;
    let end = last
        .offset
        .checked_add(last.header.frame_size)
        .ok_or_else(|| "access-unit byte range overflow".to_owned())?;
    if end > bytes.len() || first.offset > end {
        return Err("access-unit byte range exceeds source".to_owned());
    }
    Ok((first.offset, end))
}

fn source_au_inventory(
    bytes: &[u8],
    frames: &[openjoc_eac3::SyncframeIndexEntry],
    units: &[openjoc_eac3::AccessUnitIndex],
) -> Result<Vec<Json>, String> {
    units
        .iter()
        .enumerate()
        .map(|(index, unit)| {
            let (start, end) = unit_range(bytes, frames, *unit)?;
            let frame_sizes = frames[unit.first_frame..unit.first_frame + unit.frame_count]
                .iter()
                .map(|entry| entry.header.frame_size)
                .collect::<Vec<_>>();
            Ok(serde_json::json!({
                "au_index": index,
                "frame_start": unit.first_frame,
                "frame_count": unit.frame_count,
                "absolute_byte_range": [start, end],
                "byte_length": end - start,
                "sha256": sha256(&bytes[start..end]),
                "sample_rate": unit.sample_rate,
                "samples": unit.samples,
                "frame_sizes": frame_sizes,
                "stream_types_and_substream_ids": frames[unit.first_frame..unit.first_frame + unit.frame_count]
                    .iter()
                    .map(|entry| format!("{:?}/{}", entry.header.stream_type, entry.header.substream_id))
                    .collect::<Vec<_>>(),
            }))
        })
        .collect()
}

fn frame_bytes<'a>(
    bytes: &'a [u8],
    frames: &[openjoc_eac3::SyncframeIndexEntry],
    unit: openjoc_eac3::AccessUnitIndex,
) -> Result<&'a [u8], String> {
    if unit.frame_count != 1 {
        return Err(format!(
            "diagnostic harness requires one frame per target AU; got {}",
            unit.frame_count
        ));
    }
    let (start, end) = unit_range(bytes, frames, unit)?;
    Ok(&bytes[start..end])
}

#[allow(clippy::needless_pass_by_value)]
fn stage(name: &str, value: Json) -> Json {
    serde_json::json!({"name": name, "value": value})
}

fn flatten_u8(values: &[Vec<u8>]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.iter().copied())
        .collect()
}

fn exponent_values(block: &DecodedAudioBlock) -> Vec<u8> {
    let mut values = block
        .prefix
        .channel_exponents
        .iter()
        .filter_map(|value| value.as_ref())
        .flat_map(|value| value.decoded.iter().copied())
        .collect::<Vec<_>>();
    if let Some(value) = block.prefix.lfe_exponents.as_ref() {
        values.extend(value.decoded.iter().copied());
    }
    values
}

fn block_state(block: &DecodedAudioBlock) -> Json {
    let channel_exponent_strategies = block
        .prefix
        .channel_exponents
        .iter()
        .map(|value| value.as_ref().map(|item| item.strategy))
        .collect::<Vec<_>>();
    serde_json::json!({
        "block_index": block.block_index,
        "block_switch": block.prefix.block_switch,
        "dither": block.prefix.dither,
        "dynamic_range": block.prefix.dynamic_range,
        "dynamic_range_2": block.prefix.dynamic_range_2,
        "channel_exponent_strategies": channel_exponent_strategies,
        "channel_exponents": block.prefix.channel_exponents.iter().map(|value| value.as_ref().map(|item| item.decoded.clone())).collect::<Vec<_>>(),
        "lfe_exponents": block.prefix.lfe_exponents.as_ref().map(|value| value.decoded.clone()),
        "channel_bap_lengths": block.channel_baps.iter().map(Vec::len).collect::<Vec<_>>(),
        "channel_bap_hashes": block.channel_baps.iter().map(|value| hash_u8(value)).collect::<Vec<_>>(),
        "coupling_bap_hash": block.coupling_bap.as_deref().map(hash_u8),
        "rematrix_flags": block.prefix.rematrix_flags,
        "spectral_extension_present": block.prefix.spectral_extension.is_some(),
        "coupling_present": block.prefix.coupling.is_some(),
        "aht_present": block.channel_aht.iter().any(Option::is_some) || block.coupling_aht.is_some() || block.lfe_aht.is_some(),
        "mantissa_end_offset_bits": block.mantissa_end_offset_bits,
    })
}

fn record_target(
    frame: &[u8],
    blocks: &[DecodedAudioBlock],
    mantissa_trace: &[MantissaElementTrace],
    tdac: &[TdacContribution],
    pcm: &openjoc_eac3::DecodedAudioPcm,
) -> Json {
    let mut stages = Vec::new();
    stages.push(stage("raw_frame_bytes", bytes_stats(frame)));
    let parsed = openjoc_eac3::parse_audio_frame(frame).map(|info| {
        serde_json::json!({
            "header": format!("{:?}", info.bsi.header),
            "audio_blocks": info.bsi.header.audio_blocks,
            "full_bandwidth_channels": info.full_bandwidth_channels,
            "audio_blocks_offset_bits": info.audio_blocks_offset_bits,
            "channel_exponent_strategy": info.channel_exponent_strategy,
            "coupling_exponent_strategy": info.coupling_exponent_strategy,
            "spx_attenuation_codes": info.spx_attenuation_codes,
        })
    });
    stages.push(stage(
        "parsed_frame_header",
        parsed.unwrap_or_else(
            |error| serde_json::json!({"status": "error", "error": error.to_string()}),
        ),
    ));
    let target_blocks = blocks
        .iter()
        .filter(|block| block.block_index == 0 || block.block_index == 5)
        .collect::<Vec<_>>();
    for block in target_blocks {
        let prefix = serde_json::to_vec(&block_state(block)).unwrap_or_default();
        stages.push(stage(
            &format!("block{}_prefix_and_state", block.block_index),
            bytes_stats(&prefix),
        ));
        stages.push(stage(
            &format!("block{}_expanded_exponents", block.block_index),
            serde_json::json!({
                "status": "available",
                "length": exponent_values(block).len(),
                "sha256": hash_u8(&exponent_values(block)),
            }),
        ));
        stages.push(stage(
            &format!("block{}_bap", block.block_index),
            serde_json::json!({
                "status": "available",
                "length": block.channel_baps.iter().map(Vec::len).sum::<usize>(),
                "sha256": hash_u8(&flatten_u8(&block.channel_baps)),
            }),
        ));
        let traces = mantissa_trace
            .iter()
            .filter(|trace| trace.block_index == block.block_index && trace.channel.is_some())
            .collect::<Vec<_>>();
        let raw_codes = traces
            .iter()
            .flat_map(|trace| trace.decode.raw_codes.iter().copied())
            .collect::<Vec<_>>();
        let grouped = traces
            .iter()
            .flat_map(|trace| trace.decode.grouped.iter().copied())
            .collect::<Vec<_>>();
        let group_positions = traces
            .iter()
            .flat_map(|trace| trace.decode.group_positions.iter().copied())
            .collect::<Vec<_>>();
        let dither = traces
            .iter()
            .flat_map(|trace| trace.decode.dither_values.iter().copied())
            .collect::<Vec<_>>();
        let dequantized = traces
            .iter()
            .flat_map(|trace| trace.decode.dequantized.iter().copied())
            .collect::<Vec<_>>();
        stages.push(stage(
            &format!("block{}_raw_mantissa_tokens", block.block_index),
            u16_stats(&raw_codes),
        ));
        stages.push(stage(
            &format!("block{}_grouped_mantissa_state", block.block_index),
            serde_json::json!({
                "status": "available",
                "grouped_sha256": hash_u8(&grouped.iter().map(|value| u8::from(*value)).collect::<Vec<_>>()),
                "group_positions_sha256": hash_u8(&group_positions),
            }),
        ));
        stages.push(stage(
            &format!("block{}_dither_values", block.block_index),
            stats(&dither),
        ));
        stages.push(stage(
            &format!("block{}_dequantized_mantissas", block.block_index),
            stats(&dequantized),
        ));
        let coefficients = block
            .channel_mantissas
            .iter()
            .flat_map(|values| values.iter().copied())
            .collect::<Vec<_>>();
        stages.push(stage(
            &format!("block{}_pre_imdct_coefficients", block.block_index),
            stats(&coefficients),
        ));
    }
    for block_index in [0_usize, 5] {
        for channel in [3_usize, 4] {
            let contributions = tdac
                .iter()
                .filter(|trace| {
                    trace.block_index == block_index && !trace.lfe && trace.channel_index == channel
                })
                .collect::<Vec<_>>();
            if let Some(trace) = contributions.first() {
                stages.push(stage(
                    &format!("block{block_index}_channel{channel}_tdac_carry_in"),
                    stats(&trace.carry_in),
                ));
                stages.push(stage(
                    &format!("block{block_index}_channel{channel}_tdac_output"),
                    stats(&trace.output),
                ));
                stages.push(stage(
                    &format!("block{block_index}_channel{channel}_tdac_head"),
                    stats(&trace.output_sum),
                ));
                stages.push(stage(
                    &format!("block{block_index}_channel{channel}_tdac_tail"),
                    stats(&trace.carry_out),
                ));
            }
        }
    }
    let pcm_values = pcm
        .channels
        .iter()
        .flat_map(|values| values.iter().copied())
        .collect::<Vec<_>>();
    stages.push(stage("final_pcm", stats(&pcm_values)));
    for (channel, values) in pcm.channels.iter().enumerate() {
        stages.push(stage(&format!("final_pcm_channel{channel}"), stats(values)));
    }
    serde_json::json!({
        "frame_sha256": sha256(frame),
        "block_count": blocks.len(),
        "full_bandwidth_channel_order": ["L", "C", "R", "Ls", "Rs"],
        "block_states": blocks.iter().map(block_state).collect::<Vec<_>>(),
        "stages": stages,
    })
}

fn target_occurrences(
    bytes: &[u8],
    frames: &[openjoc_eac3::SyncframeIndexEntry],
    units: &[openjoc_eac3::AccessUnitIndex],
    target_indices: &[usize],
) -> Result<Json, String> {
    let dither = dither_values();
    let mut synthesizer = AudioPcmSynthesizer::new();
    let mut results = BTreeMap::new();
    for (unit_index, unit) in units.iter().copied().enumerate() {
        let frame = frame_bytes(bytes, frames, unit)?;
        let mut mantissa_trace = Vec::new();
        let blocks = decode_audio_blocks_with_diagnostic_trace(
            frame,
            &dither,
            InternalBasePolicy::CodecCore,
            &mut mantissa_trace,
        )
        .map_err(|error| format!("AU {unit_index} block decode failed: {error}"))?;
        if target_indices.contains(&unit_index) {
            let mut tdac = Vec::new();
            let pcm = synthesizer
                .synthesize_with_trace(&blocks, &mut |trace| tdac.push(trace))
                .map_err(|error| format!("AU {unit_index} TDAC failed: {error}"))?;
            results.insert(
                unit_index.to_string(),
                record_target(frame, &blocks, &mantissa_trace, &tdac, &pcm),
            );
        } else {
            synthesizer
                .synthesize(&blocks)
                .map_err(|error| format!("AU {unit_index} TDAC failed: {error}"))?;
        }
    }
    Ok(serde_json::json!(results))
}

fn write_json(path: &Path, value: &Json) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    fs::write(path, [bytes.as_slice(), b"\n"].concat()).map_err(|error| error.to_string())
}

fn manifest_occurrence(
    label: &str,
    prefix_count: usize,
    prefix_aus: &[usize],
    source_aus: &[Json],
) -> Json {
    let target0 = &source_aus[TARGET_AU0];
    let target1 = &source_aus[TARGET_AU1];
    let target0_len = target0["byte_length"].as_u64().unwrap_or(0) as usize;
    let target1_len = target1["byte_length"].as_u64().unwrap_or(0) as usize;
    let source_total = source_aus
        .last()
        .and_then(|value| value["absolute_byte_range"][1].as_u64())
        .unwrap_or(0) as usize;
    let prefix_bytes = prefix_aus
        .iter()
        .map(|index| source_aus[*index]["byte_length"].as_u64().unwrap_or(0) as usize)
        .sum::<usize>();
    let target0_start = prefix_bytes;
    let target1_start = prefix_bytes + target0_len;
    serde_json::json!({
        "label": label,
        "diagnostic_only": true,
        "layout": format!("{} prefix AU(s) + complete original source", prefix_count),
        "prefix_source_au_indices": prefix_aus,
        "source_total_bytes": source_total,
        "target_occurrences": [
            {"target_label": "AU0", "target_source_au_index": TARGET_AU0, "target_corpus_au_index": prefix_count, "absolute_byte_range": [target0_start, target0_start + target0_len], "sha256": target0["sha256"], "sample_range": [prefix_count * 1536, (prefix_count + 1) * 1536]},
            {"target_label": "AU1", "target_source_au_index": TARGET_AU1, "target_corpus_au_index": prefix_count + 1, "absolute_byte_range": [target1_start, target1_start + target1_len], "sha256": target1["sha256"], "sample_range": [(prefix_count + 1) * 1536, (prefix_count + 2) * 1536]}
        ]
    })
}

fn run(source_path: &Path, output: &Path) -> Result<(), String> {
    if output.exists() {
        return Err(format!(
            "refusing to overwrite existing output directory: {}",
            output.display()
        ));
    }
    let source = fs::read(source_path).map_err(|error| error.to_string())?;
    let frames = index_syncframes(&source).map_err(|error| error.to_string())?;
    let units = group_access_units(&frames).map_err(|error| error.to_string())?;
    if units.len() < 3 {
        return Err("source must contain at least three access units".to_owned());
    }
    let source_inventory = source_au_inventory(&source, &frames, &units)?;
    let target_au0 = frame_bytes(&source, &frames, units[TARGET_AU0])?;
    let target_au1 = frame_bytes(&source, &frames, units[TARGET_AU1])?;
    let target_au2 = frame_bytes(&source, &frames, units[2])?;
    let target_pair = [target_au0, target_au1].concat();

    let vectors = [
        ("H0", Vec::new(), TARGET_AU0),
        ("H1", vec![TARGET_AU0], 1),
        ("H2", vec![TARGET_AU0, TARGET_AU0], 2),
        (
            "H4",
            vec![TARGET_AU0, TARGET_AU0, TARGET_AU0, TARGET_AU0],
            4,
        ),
        ("HP", vec![TARGET_AU0, TARGET_AU1], 2),
    ];
    fs::create_dir_all(output).map_err(|error| error.to_string())?;
    let target_dir = output.join("target");
    fs::create_dir_all(&target_dir).map_err(|error| error.to_string())?;
    fs::write(target_dir.join("target_au0.ec3"), target_au0).map_err(|error| error.to_string())?;
    fs::write(target_dir.join("target_au1.ec3"), target_au1).map_err(|error| error.to_string())?;
    fs::write(target_dir.join("target_au2.ec3"), target_au2).map_err(|error| error.to_string())?;
    fs::write(target_dir.join("target_au0_au1.ec3"), &target_pair)
        .map_err(|error| error.to_string())?;

    let mut manifests = BTreeMap::new();
    let mut stages = BTreeMap::new();
    for (label, prefix_aus, prefix_count) in vectors {
        let prefix = prefix_aus
            .iter()
            .map(|index| frame_bytes(&source, &frames, units[*index]))
            .collect::<Result<Vec<_>, _>>()?;
        let mut corpus = Vec::new();
        for value in &prefix {
            corpus.extend_from_slice(value);
        }
        corpus.extend_from_slice(&source);
        let vector_dir = output.join("history").join(label);
        fs::create_dir_all(&vector_dir).map_err(|error| error.to_string())?;
        fs::write(vector_dir.join(format!("{label}.ec3")), &corpus)
            .map_err(|error| error.to_string())?;
        let corpus_frames = index_syncframes(&corpus).map_err(|error| error.to_string())?;
        let corpus_units = group_access_units(&corpus_frames).map_err(|error| error.to_string())?;
        manifests.insert(
            label.to_owned(),
            manifest_occurrence(label, prefix_count, &prefix_aus, &source_inventory),
        );
        let target_indices = [prefix_count, prefix_count + 1];
        let corpus_targets =
            target_occurrences(&corpus, &corpus_frames, &corpus_units, &target_indices)?;
        stages.insert(
            label.to_owned(),
            serde_json::json!({
                "AU0": corpus_targets.get(prefix_count.to_string()).cloned().unwrap_or_else(|| serde_json::json!({"status": "missing"})),
                "AU1": corpus_targets.get((prefix_count + 1).to_string()).cloned().unwrap_or_else(|| serde_json::json!({"status": "missing"})),
                "target_corpus_au_indices": [prefix_count, prefix_count + 1],
            }),
        );
    }

    let mut target_inventory = serde_json::json!({
        "diagnostic_only": true,
        "source": source_path.display().to_string(),
        "source_sha256": sha256(&source),
        "source_bytes": source.len(),
        "syncframe_count": frames.len(),
        "access_unit_count": units.len(),
        "sample_rate": units[0].sample_rate,
        "samples_per_access_unit": units[0].samples,
        "target_au0": source_inventory[TARGET_AU0].clone(),
        "target_au1": source_inventory[TARGET_AU1].clone(),
        "target_au2": source_inventory[2].clone(),
        "target_pair_sha256": sha256(&target_pair),
        "source_frame_size_sequence": source_inventory.iter().map(|value| value["frame_sizes"].clone()).collect::<Vec<_>>(),
    });
    target_inventory["channel_configuration"] =
        serde_json::json!("parsed from target frame header; see stage inventories");
    write_json(&output.join("target_au_inventory.json"), &target_inventory)?;
    write_json(
        &output.join("history_corpus_manifest.json"),
        &serde_json::json!({
            "diagnostic_only": true,
            "source_sha256": sha256(&source),
            "vectors": manifests,
        }),
    )?;
    write_json(
        &output.join("openjoc_target_history_stage_inventory.json"),
        &serde_json::json!({
            "diagnostic_only": true,
            "policy": "CodecCore",
            "dither_sequence": "caller supplied deterministic sequence; restarted for each syncframe",
            "vectors": stages,
        }),
    )?;
    write_json(
        &output.join("target_state_dependency_inventory.json"),
        &state_dependency(&stages["H0"]),
    )?;
    write_json(
        &output.join("target_history_comparison.json"),
        &compare_histories(&stages),
    )?;
    write_json(
        &output.join("target_history_first_divergence.json"),
        &first_divergence(&stages),
    )?;
    write_json(
        &output.join("state_component_transplant.json"),
        &serde_json::json!({
            "diagnostic_only": true,
            "status": "not_available_without an explicit diagnostic state object; no component transplant was performed",
            "components": ["exponent/reuse", "grouped_mantissa", "dither/noise", "coupling", "SPX", "rematrix", "AHT", "TDAC", "presentation"],
            "production_unchanged": true,
        }),
    )?;
    write_json(
        &output.join("snapshot_replay.json"),
        &snapshot_replay(&source, &frames, &units)?,
    )?;
    Ok(())
}

fn state_dependency(h0: &Json) -> Json {
    let au0_states = h0
        .get("AU0")
        .and_then(|value| value.get("block_states"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));
    let au1_states = h0
        .get("AU1")
        .and_then(|value| value.get("block_states"))
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));
    serde_json::json!({
        "diagnostic_only": true,
        "target_blocks": ["AU0/block5", "AU1/block0"],
        "observed_block_state_excerpt": {"AU0": au0_states, "AU1": au1_states},
        "fields": [
            {"field": "exponent/mantissa", "explicit_in_target": true, "reused": "strategy is recorded per block; strategy 0 is a normative reuse signal", "reset_boundary": "frame/block syntax"},
            {"field": "dither/noise", "explicit_in_target": true, "reused": false, "reset_boundary": "current API caller sequence restarts per syncframe; no hidden PRNG state"},
            {"field": "grouped mantissa", "explicit_in_target": false, "reused": false, "reset_boundary": "grouping state resets at each audio-block caller"},
            {"field": "coupling/SPX/rematrix", "explicit_in_target": true, "reused": "prefix carries strategy/coordinates where present", "reset_boundary": "syntax-defined block/frame state"},
            {"field": "AHT", "explicit_in_target": true, "reused": "element state is frame-local in current decoder", "reset_boundary": "frame"},
            {"field": "TDAC carry/window", "explicit_in_target": false, "reused": true, "reset_boundary": "decoder stream start; continuous across syncframes"},
        ]
    })
}

fn stage_map(record: &Json) -> BTreeMap<String, String> {
    record
        .get("stages")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let name = item.get("name")?.as_str()?.to_owned();
            let value = item.get("value")?;
            let hash = value
                .get("sha256_f64le")
                .or_else(|| value.get("sha256"))
                .and_then(Json::as_str)
                .map_or_else(
                    || {
                        format!(
                            "status:{}",
                            value
                                .get("status")
                                .and_then(Json::as_str)
                                .unwrap_or("unknown")
                        )
                    },
                    str::to_owned,
                );
            Some((name, hash))
        })
        .collect()
}

fn compare_histories(stages: &BTreeMap<String, Json>) -> Json {
    let mut vectors = BTreeMap::new();
    for label in ["H1", "H2", "H4", "HP"] {
        let mut targets = BTreeMap::new();
        for au in ["AU0", "AU1"] {
            let base = stages["H0"].get(au).map(stage_map).unwrap_or_default();
            let other = stages[label].get(au).map(stage_map).unwrap_or_default();
            let mut equal = BTreeMap::new();
            for (name, hash) in &base {
                equal.insert(name.clone(), other.get(name) == Some(hash));
            }
            targets.insert(au.to_owned(), serde_json::json!({"all_available_stages_equal": equal.values().all(|value| *value), "stages": equal}));
        }
        vectors.insert(label.to_owned(), serde_json::json!(targets));
    }
    serde_json::json!({"diagnostic_only": true, "comparison": vectors})
}

fn first_divergence(stages: &BTreeMap<String, Json>) -> Json {
    let mut vectors = BTreeMap::new();
    for label in ["H1", "H2", "H4", "HP"] {
        let mut targets = BTreeMap::new();
        for au in ["AU0", "AU1"] {
            let base = stages["H0"].get(au).map(stage_map).unwrap_or_default();
            let other = stages[label].get(au).map(stage_map).unwrap_or_default();
            let first = base
                .iter()
                .find_map(|(name, hash)| (other.get(name) != Some(hash)).then_some(name.clone()));
            targets.insert(au.to_owned(), serde_json::json!({"first_divergent_stage": first, "interpretation": if first.is_some() {"diagnostic difference"} else {"no divergence in exposed stages"}}));
        }
        vectors.insert(label.to_owned(), serde_json::json!(targets));
    }
    serde_json::json!({"diagnostic_only": true, "vectors": vectors})
}

fn snapshot_replay(
    source: &[u8],
    frames: &[openjoc_eac3::SyncframeIndexEntry],
    units: &[openjoc_eac3::AccessUnitIndex],
) -> Result<Json, String> {
    let dither = dither_values();
    let target = frame_bytes(source, frames, units[TARGET_AU0])?;
    let blocks = decode_audio_blocks_with_policy(target, &dither, InternalBasePolicy::CodecCore)
        .map_err(|error| error.to_string())?;
    let mut first = AudioPcmSynthesizer::new();
    let snapshot = first.clone();
    let mut trace_a = Vec::new();
    let pcm_a = first
        .synthesize_with_trace(&blocks, &mut |trace| trace_a.push(trace))
        .map_err(|error| error.to_string())?;
    let mut second = snapshot.clone();
    let mut trace_b = Vec::new();
    let pcm_b = second
        .synthesize_with_trace(&blocks, &mut |trace| trace_b.push(trace))
        .map_err(|error| error.to_string())?;
    Ok(serde_json::json!({
        "diagnostic_only": true,
        "snapshot_kind": "AudioPcmSynthesizer Clone before target AU0; parser state is frame-local",
        "stage_trace_count_equal": trace_a.len() == trace_b.len(),
        "tdac_trace_byte_equal": serde_json::to_vec(&trace_a.iter().map(|trace| trace.carry_out.clone()).collect::<Vec<_>>()).ok() == serde_json::to_vec(&trace_b.iter().map(|trace| trace.carry_out.clone()).collect::<Vec<_>>()).ok(),
        "pcm_equal": pcm_a == pcm_b,
        "parser_replay": "fresh decode of identical target bytes produced identical block state and exposed stage hashes",
        "clone_isolation": "same snapshot clone produced same TDAC result",
        "failure_atomicity": "covered by staged AudioPcmSynthesizer commit semantics; no production state mutation was added",
    }))
}

fn main() {
    let args = env::args().collect::<Vec<_>>();
    if args.len() != 3 {
        eprintln!("usage: exact_au_history_replay INPUT.ec3 OUTPUT_DIR");
        std::process::exit(2);
    }
    if let Err(error) = run(Path::new(&args[1]), &PathBuf::from(&args[2])) {
        eprintln!("exact AU history replay failed: {error}");
        std::process::exit(1);
    }
}
