// pattern: Functional Core

use crate::{ContainerSummary, Diagnostic, StreamInspection};

/// Maps container declarations only; codec recognition remains in-band.
pub(crate) fn apply_declarations(report: &mut StreamInspection, value: &serde_json::Value) {
    let Some(stream) = value["streams"]
        .as_array()
        .and_then(|streams| streams.iter().find(|s| s["codec_name"] == "eac3"))
    else {
        return;
    };
    let text = |key: &str| stream[key].as_str().map(str::to_owned);
    let sample_entry = text("codec_tag_string");
    let timescale = stream["time_base"]
        .as_str()
        .and_then(|s| s.strip_prefix("1/"))
        .and_then(|s| s.parse().ok());
    report.container = ContainerSummary {
        kind: "iso_bmff".into(),
        ec3_present: sample_entry.as_ref().map(|s| s == "ec-3"),
        sample_entry,
        track_id: text("id"),
        timescale,
        duration_seconds: text("duration")
            .and_then(|s| s.parse().ok())
            .filter(|d: &f64| d.is_finite()),
        sample_count: text("nb_frames").and_then(|s| s.parse().ok()),
        ..ContainerSummary::default()
    };
    // FFprobe's packet demux is the existing container backend. It may expose
    // the dec3 body as extradata. Never infer its presence from codec_name.
    if let Some(hex) = stream["extradata"].as_str() {
        let bytes = probe_hex_bytes(hex);
        if let Ok(config) = openjoc_container::cmaf::parse_ec3_specific_box(&bytes) {
            report.container.dec3_present = Some(true);
            report.container.declared_joc = Some(config.flag_ec3_extension_type_a);
            report.container.declared_complexity_index = config
                .flag_ec3_extension_type_a
                .then_some(config.complexity_index_type_a);
        }
    }
    let definitive_presence = matches!(
        report.joc.presence_status.as_str(),
        "present" | "not_present"
    );
    let mismatch = (definitive_presence
        && report
            .container
            .declared_joc
            .is_some_and(|declared| declared != report.joc.present))
        || report.container.declared_complexity_index.is_some_and(|c| {
            !report.joc.complexity_indices.is_empty()
                && report
                    .joc
                    .complexity_indices
                    .iter()
                    .any(|observed| *observed != c)
        });
    if mismatch {
        let issue = Diagnostic {
            code: "CONTAINER_STREAM_MISMATCH".into(),
            au: 0,
            elementary_byte_offset: 0,
            message: "Container dec3 declaration disagrees with parsed in-band JOC signaling"
                .into(),
        };
        report
            .diagnostics
            .first_failure
            .get_or_insert_with(|| issue.clone());
        report.diagnostics.issue_count += 1;
        if report.diagnostics.issues.len() < 64 {
            report.diagnostics.issues.push(issue);
        }
    }
}

fn probe_hex_bytes(value: &str) -> Vec<u8> {
    value
        .lines()
        .filter_map(|line| line.split_once(':').map(|(_, hex)| hex.trim_start()))
        .flat_map(|line| line.split("  ").next().unwrap_or("").split_whitespace())
        .flat_map(|word| word.as_bytes().chunks_exact(2))
        .filter_map(|pair| {
            std::str::from_utf8(pair)
                .ok()
                .and_then(|s| u8::from_str_radix(s, 16).ok())
        })
        .take(4096)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn declared_codec_is_not_joc_authority() {
        let mut r = crate::InspectionAccumulator::new(crate::InspectionOptions::default()).finish();
        apply_declarations(
            &mut r,
            &serde_json::json!({"streams":[{"codec_name":"eac3", "codec_tag_string":"ec-3", "id":"0x1", "time_base":"1/48000", "nb_frames":"2"}]}),
        );
        assert!(!r.joc.present);
        assert_eq!(r.container.timescale, Some(48000));
        assert_eq!(r.container.dec3_present, None);
    }

    #[test]
    fn container_mismatch_requires_definitive_in_band_evidence() {
        let declaration = serde_json::json!({"streams":[{"codec_name":"eac3", "codec_tag_string":"ec-3", "extradata":"\n00000000: 0000 0010 6465 6333 1800 200f 0202 0110  ....dec3.. .....\n"}]});
        let mut unknown =
            crate::InspectionAccumulator::new(crate::InspectionOptions::default()).finish();
        apply_declarations(&mut unknown, &declaration);
        assert_eq!(unknown.container.declared_joc, Some(true));
        assert!(
            !unknown
                .diagnostics
                .issues
                .iter()
                .any(|d| d.code == "CONTAINER_STREAM_MISMATCH")
        );
        unknown.joc.presence_status = "not_present".into();
        apply_declarations(&mut unknown, &declaration);
        assert!(
            unknown
                .diagnostics
                .issues
                .iter()
                .any(|d| d.code == "CONTAINER_STREAM_MISMATCH")
        );
        assert!(!unknown.joc.present);
    }
}
