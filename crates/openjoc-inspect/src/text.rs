// pattern: Functional Core

use crate::{InspectionOptions, ProfileValidation, StreamInspection};
use std::fmt::{Display, Write};

fn optional<T: Display>(value: Option<T>) -> String {
    value.map_or_else(|| String::from("unavailable"), |v| v.to_string())
}
fn position(value: Option<[f64; 3]>) -> String {
    value.map_or_else(
        || String::from("unavailable"),
        |v| format!("({:.4}, {:.4}, {:.4})", v[0], v[1], v[2]),
    )
}
fn yes_no(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "Yes",
        Some(false) => "No",
        None => "Unknown",
    }
}

fn status(value: &str) -> &str {
    match value {
        "pass" => "PASS",
        "fail" => "FAIL",
        "not_applicable" => "N/A",
        other => other,
    }
}

/// Formats a deliberate human summary rather than serializing parser internals.
pub fn format_summary(r: &StreamInspection, options: InspectionOptions) -> String {
    let mut out = String::from("OpenJOC Stream Inspector\n\nInput\n");
    let format = match r.input.format.as_str() {
        "eac3_joc" => "E-AC-3 JOC",
        "eac3_joc_legacy_ac3_core" => "E-AC-3 JOC with legacy/original-syntax AC-3 core",
        "eac3" => "E-AC-3",
        "malformed_joc_metadata" => "Malformed JOC metadata",
        _ => "Unknown / malformed E-AC-3",
    };
    let _ = writeln!(
        out,
        "  Filename               {}\n  Container              {}\n  Format                 {format}\n  Sample rates           {:?} Hz\n  Duration               {:.6} s\n  Access units           {}\n  Total samples          {}\n  Total bitrate          {:.3} kbps",
        r.input.filename.as_deref().unwrap_or("(reader)"),
        if r.container.kind == "raw_eac3" {
            "Raw E-AC-3"
        } else {
            "ISO BMFF"
        },
        r.eac3.sample_rates_hz,
        r.eac3.duration_seconds,
        r.eac3.access_unit_count,
        r.eac3.total_samples,
        r.eac3.bitrate_bps.unwrap_or(0.0) / 1000.0
    );
    out.push_str("\nE-AC-3 topology\n");
    for topology in &r.eac3.topologies {
        let _ = writeln!(
            out,
            "  Programme              {} ({} AUs; first {})",
            topology.value.join(" + "),
            topology.occurrences,
            topology.first_au
        );
    }
    let _ = writeln!(
        out,
        "  Frames per AU          {:?}\n  Dependent IDs          {:?}\n  Sequential IDs         {}\n  Channel conflicts      {}",
        r.eac3.frames_per_au,
        r.eac3.dependent_ids,
        yes_no(r.eac3.dependent_ids_sequential),
        r.eac3.channel_conflicts
    );
    for partition in &r.eac3.block_partitions {
        let _ = writeln!(
            out,
            "  Block partition        {} ({} AUs)",
            partition
                .value
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(" + "),
            partition.occurrences
        );
    }
    for lfe in &r.eac3.lfe_ownership {
        let _ = writeln!(
            out,
            "  LFE ownership          {}",
            lfe.value.replace('_', " ")
        );
    }
    out.push_str("\nJOC\n");
    let _ = writeln!(
        out,
        "  Present                {}",
        if r.joc.present {
            "Yes"
        } else if r.joc.payload_occurrences > 0 {
            "Payload present; syntax invalid/unsupported"
        } else if r.joc.presence_status == "unavailable" {
            "Not observed; carrier traversal incomplete"
        } else {
            "Not present"
        }
    );
    for p in &r.joc.profiles {
        let _ = writeln!(
            out,
            "  Profile                {}\n  Reconstruction inputs  {}",
            p.value.display_name,
            p.value.carriers.join(" ")
        );
    }
    let _ = writeln!(
        out,
        "  Coded objects          {:?}\n  Complexity index       {:?}\n  JOC owner              {}",
        r.joc.coded_object_counts,
        r.joc.complexity_indices,
        r.carriage.joc_owners.join(", ")
    );
    out.push_str("\nCarriage / EMDF\n");
    let _ = writeln!(
        out,
        "  frame-end auxdatae: {} present, {} absent\n  frame-end EMDF: {} parsed, {} non-EMDF, {} malformed\n  audio-block skipfld: {} observed in {} reached prefixes; {} blocks unresolved",
        r.carriage.aux_present,
        r.carriage.aux_absent,
        r.carriage.aux_parsed,
        r.carriage.aux_non_emdf,
        r.carriage.aux_malformed,
        r.carriage.skip_observed,
        r.carriage.skip_examined,
        r.carriage.skip_unresolved
    );
    for location in &r.carriage.locations {
        let _ = writeln!(
            out,
            "  {}  {} ({} occurrences)",
            location.value.owner,
            match location.value.location.as_str() {
                "frame_end_auxdatae" => "frame-end auxdatae",
                "audio_block_skipfld" => "audio-block skipfld",
                other => other,
            },
            location.occurrences
        );
    }
    for p in &r.emdf.payloads {
        let _ = writeln!(
            out,
            "  Payload {:<3} {:<6}     {} occurrences / {} AUs; {}..{} bytes; {} unique lengths",
            p.id,
            p.name.as_deref().unwrap_or("other"),
            p.occurrences,
            p.affected_aus,
            p.length_min,
            p.length_max,
            p.unique_lengths
        );
    }
    for order in &r.emdf.payload_orders {
        let _ = writeln!(
            out,
            "  Payload order          {:?} ({}; first AU {})",
            order.value, order.occurrences, order.first_au
        );
    }
    out.push_str("\nValidation\n");
    let _ = writeln!(
        out,
        "  Stream parse           {}\n  ETSI Strict            {}\n  Deployed Compatibility {}\n  Malformed AUs          {}\n  Decoder-admissible     {}\n  PCM render verified    No\n  Frame timing           {}\n  Metadata timing        {}",
        status(&r.validation.stream_parse),
        status(&r.validation.etsi_strict.status),
        status(&r.validation.deployed_compatibility.status),
        r.validation.malformed_aus,
        match r.validation.decoder_admissible {
            Some(true) => "Yes (structural)",
            Some(false) => "No",
            None => "Unknown",
        },
        r.validation.frame_timing_continuity,
        r.validation.metadata_timing_continuity
    );
    deviations(&mut out, &r.validation.etsi_strict);
    out.push_str("\nObject scene\n");
    let _ = writeln!(
        out,
        "  Object metadata        {}\n  Dynamic metadata       {}\n  Metadata updates       {}",
        yes_no(Some(r.scene.object_metadata_present)),
        match r.scene.dynamic_metadata_detected {
            Some(true) => "Yes",
            Some(false) => "No change observed",
            None => "Unknown",
        },
        r.scene.metadata_update_count
    );
    if let Some(change) = &r.scene.first_change {
        let _ = writeln!(
            out,
            "  First change           AU {} / {:.6} s",
            change.au, change.seconds
        );
    }
    if r.scene.opaque_trim_present {
        out.push_str("  OAMD trim element: opaque unresolved\n");
    }
    if options.aus {
        out.push_str("\nAU  Time(s)  Frames  Partition  Topology  JOC owner  Profiles  Objects  Complexity  Payloads  Status\n");
        for au in &r.access_units {
            let _ = writeln!(
                out,
                "{} {:.6} {} {:?} {} {} {:?} {:?} {:?} {:?} {}",
                au.au,
                au.timestamp_seconds,
                au.frames,
                au.block_partition,
                au.topology.join("+"),
                au.joc_owner.as_deref().unwrap_or("-"),
                au.profile_indices,
                au.object_counts,
                au.complexity_indices,
                au.payload_ids,
                au.status
            );
        }
    }
    if options.objects {
        out.push_str("\nOAMD object slots (not authored identities)\n");
        for object in &r.scene.objects {
            let _ = writeln!(
                out,
                "  {} active {}..{}; updates {}; dynamic {}; position {}..{}",
                object.object_index,
                optional(object.first_active_au),
                optional(object.last_active_au),
                object.metadata_update_count,
                object.dynamic,
                position(object.position_min),
                position(object.position_max)
            );
        }
    }
    if options.emdf || options.verbose {
        out.push_str("\nEMDF configuration\n");
        for c in &r.emdf.configurations {
            let v = &c.value;
            let _ = writeln!(
                out,
                "  payload {} group_id={} codecdatae={} alignment={} create_duplicate={} remove_duplicate={} priority={} proc_allowed={}; {} occurrences; first AU {}",
                v.payload_id,
                optional(v.group_id),
                v.codecdatae,
                optional(v.payload_frame_aligned),
                optional(v.create_duplicate),
                optional(v.remove_duplicate),
                optional(v.priority),
                optional(v.proc_allowed),
                c.occurrences,
                c.first_au
            );
        }
    }
    if !r.diagnostics.issues.is_empty() {
        out.push_str("\nDiagnostics\n");
        for issue in r
            .diagnostics
            .issues
            .iter()
            .take(if options.verbose { 64 } else { 5 })
        {
            let _ = writeln!(
                out,
                "  {}: AU {} / elementary byte {}: {}",
                issue.code, issue.au, issue.elementary_byte_offset, issue.message
            );
        }
    }
    if r.diagnostics.aggregation_truncated {
        out.push_str(
            "  Summary variant limit reached; counts for retained variants remain exact.\n",
        );
    }
    if options.verbose {
        out.push_str("\nComponent configuration\n");
        for c in &r.eac3.components {
            let maps = c
                .chanmap
                .iter()
                .map(|m| m.map_or_else(|| "default".into(), |v| format!("0x{v:04x}")))
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(
                out,
                "  {} {}: frames {}, bytes {:?}, bitrate {:.3} kbps; blocks {:?}; numblkscod {:?}; acmod {:?}; lfeon {:?}; chanmap {}; channels {}",
                c.owner,
                c.stream_type,
                c.frame_count,
                c.frame_bytes,
                c.bitrate_bps / 1000.0,
                c.block_counts,
                c.numblkscod,
                c.acmod,
                c.lfeon,
                maps,
                c.channel_locations
                    .iter()
                    .map(|v| v.join(" "))
                    .collect::<Vec<_>>()
                    .join(" / ")
            );
        }
        for limitation in &r.diagnostics.limitations {
            let _ = writeln!(out, "  Note: {limitation}");
        }
    }
    out
}

fn deviations(out: &mut String, validation: &ProfileValidation) {
    for d in validation.deviations.iter().take(12) {
        let _ = writeln!(
            out,
            "  payload {} {}: observed {}, expected {}; {} AUs, first AU {}",
            d.payload_id, d.field, d.observed, d.expected, d.affected_aus, d.first_au
        );
    }
}
