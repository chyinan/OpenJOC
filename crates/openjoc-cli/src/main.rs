// pattern: Imperative Shell

mod banner;
mod comparison;
mod eac3_decode;
mod fixture_census;
mod oamd_forensics;
mod oamd_oracle;
mod terminal;

use banner::{package_metadata, render_banner};
use openjoc_container::{DEFAULT_MAX_EAC3_BYTES, InputMediaKind, detect_media, load_eac3};
use openjoc_eac3::{
    DecodedAccessUnitPcm, InternalBasePolicy, emit_coding_tool_inventory,
    extract_joc_addbsi_access_unit,
};
use openjoc_emdf::JocValidationProfile;
use openjoc_oamd::{OamdDecoderConfig, OamdParseProfile, Position3, ReferenceScreen};
use openjoc_scene::{JocFrameInput, PayloadDecoder, PayloadDecoderConfig};
use openjoc_wave::{
    Clipping, Dither, SampleFormat, WaveEncodeOptions, WavePcm, WaveWriter, decode, encode_channels,
};
use std::{
    env,
    error::Error,
    fmt::Write as _,
    fs, io,
    io::Read,
    num::NonZeroU8,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};
use terminal::TerminalCapabilities;

const USAGE: &str = "usage: openjoc inspect FILE [--trim-config-count N]\n       openjoc decode FILE -o DIR [--downmix FILE | --internal-base] [--streaming] [--internal-base-policy current-default|codec-core] [--validation-profile etsi-strict|dolby-vendor-compat] [--trim-config-count N] [--reference-f64]\n       openjoc diagnose-tools FILE --vector-id ID --json OUTPUT\n       openjoc census [MANIFEST] -o DIR\n       openjoc diagnose-oamd FILE [-o DIR] [--access-unit N | --au START..END | --all-access-units] [--trim-config-count N] [--diff-payload-11] [--warp-hypotheses] [--adm-reference PATH] [--json PATH] [--force]\n       openjoc decode-payload --downmix FILE --joc FILE --oamd FILE -o DIR [--validation-profile etsi-strict|dolby-vendor-compat] [--reference-f64] [--trim-config-count N] [--screen-origin-x X --screen-origin-y Y --screen-origin-z Z --screen-width W --screen-height H]";

struct DecodePayloadArgs {
    downmix: PathBuf,
    joc: PathBuf,
    oamd: PathBuf,
    output: PathBuf,
    trim_count: Option<NonZeroU8>,
    validation_profile: JocValidationProfile,
    reference_screen: Option<ReferenceScreen>,
    output_format: SampleFormat,
}

struct DecodeEac3Args {
    input: PathBuf,
    downmix: Option<PathBuf>,
    internal_base: bool,
    output: PathBuf,
    output_format: SampleFormat,
    validation_profile: JocValidationProfile,
    trim_configuration_count: Option<NonZeroU8>,
    internal_base_policy: InternalBasePolicy,
    streaming: bool,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("openjoc: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args().skip(1).collect::<Vec<_>>();
    let no_banner = arguments.iter().any(|argument| argument == "--no-banner");
    arguments.retain(|argument| argument != "--no-banner");
    let mut terminal = TerminalCapabilities::detect();
    terminal.no_banner |= no_banner;

    match arguments.first().map(String::as_str) {
        None if terminal.is_tty => print_root_page(terminal, false),
        Some("-h" | "--help") if arguments.len() == 1 => print_root_page(terminal, true),
        Some("inspect") => {
            let (input, trim_configuration_count) = parse_inspect(&arguments[1..])?;
            inspect(&input, trim_configuration_count)
        }
        Some("decode-payload") => decode_payload(&arguments[1..]),
        Some("decode") => decode_eac3(&parse_decode_eac3(&arguments[1..])?),
        Some("diagnose-tools") => diagnose_tools(&arguments[1..]),
        Some("census") => run_census(&arguments[1..]),
        Some("diagnose-oamd") => oamd_forensics::run(&arguments[1..]),
        _ => Err(usage_error().into()),
    }
}

fn print_root_page(terminal: TerminalCapabilities, help: bool) -> Result<(), Box<dyn Error>> {
    let metadata = package_metadata();
    let context = terminal.banner_context(help, !help);
    let mut output = render_banner(context, metadata);
    if output.is_empty() {
        writeln!(output, "OpenJOC {}", metadata.version)?;
        writeln!(output, "{}", metadata.description)?;
        writeln!(output, "Open the objects. Rebuild the space.\n")?;
    } else {
        output.push('\n');
    }

    if help {
        append_help(&mut output, terminal.color_enabled())?;
    } else {
        append_home(&mut output, terminal.color_enabled())?;
    }
    io::Write::write_all(&mut io::stdout().lock(), output.as_bytes())?;
    Ok(())
}

fn append_home(output: &mut String, color: bool) -> Result<(), std::fmt::Error> {
    append_heading(output, "USAGE", color)?;
    output.push_str(concat!(
        "  openjoc inspect <FILE> [--trim-config-count N]\n",
        "  openjoc decode <FILE> -o <DIR> [--validation-profile <PROFILE>] [--internal-base-policy current-default|codec-core] [--trim-config-count N] [--reference-f64]\n",
        "  openjoc census [MANIFEST] -o <DIR>\n",
        "  openjoc diagnose-oamd <FILE> -o <DIR> [--access-unit N | --all-access-units] [--trim-config-count N]\n",
        "  openjoc decode-payload [OPTIONS]\n",
        "  openjoc --help\n",
        "\n",
        "Run 'openjoc --help' for all commands and options.\n",
    ));
    Ok(())
}

fn append_help(output: &mut String, color: bool) -> Result<(), std::fmt::Error> {
    append_heading(output, "USAGE", color)?;
    output.push_str(concat!(
        "  openjoc inspect <FILE> [--trim-config-count N]\n",
        "  openjoc decode <FILE> -o <DIR> [--downmix <FILE> | --internal-base] [--streaming]\n",
        "                         [--validation-profile etsi-strict|dolby-vendor-compat]\n",
        "                         [--internal-base-policy current-default|codec-core]\n",
        "                         [--trim-config-count N]\n",
        "                         [--reference-f64]\n",
        "  openjoc census [MANIFEST] -o <DIR>\n",
        "  openjoc diagnose-oamd <FILE> [-o <DIR>] [--access-unit N | --au START..END | --all-access-units]\n",
        "                         [--trim-config-count N] [--diff-payload-11] [--warp-hypotheses]\n",
        "                         [--adm-reference PATH] [--json PATH] [--force]\n",
        "  openjoc decode-payload --downmix <FILE> --joc <FILE> --oamd <FILE>\n",
        "                         -o <DIR> [--validation-profile etsi-strict|dolby-vendor-compat]\n",
        "                         [OPTIONS]\n",
        "\n",
    ));
    append_heading(output, "COMMANDS", color)?;
    output.push_str(concat!(
        "  inspect         Inspect E-AC-3 access units and JOC metadata\n",
        "  decode          Decode an E-AC-3 JOC stream into an object scene\n",
        "  census          Census bounded metadata carriers from external fixtures\n",
        "  diagnose-oamd   Emit bit-exact EMDF/OAMD entry evidence\n",
        "  decode-payload  Decode supplied downmix, JOC, and OAMD payloads\n",
        "\n",
    ));
    append_heading(output, "OPTIONS", color)?;
    output.push_str(concat!(
        "  -h, --help       Print root command help\n",
        "      --no-banner Disable the interactive startup banner\n",
        "      --validation-profile Select ETSI strict (default) or explicit Dolby vendor compatibility\n",
        "      --internal-base-policy Select current default or codec-core gain policy\n",
        "      --streaming      Use the direct sequential raw E-AC-3 AU consumer (with --internal-base)\n",
        "      --trim-config-count Supply the caller-defined OAMD trim configuration count\n",
        "      --reference-f64 Use explicit reference f64 reconstruction-row output (default: f32)\n",
    ));
    Ok(())
}

fn append_heading(output: &mut String, heading: &str, color: bool) -> Result<(), std::fmt::Error> {
    if color {
        writeln!(output, "\x1b[38;2;32;214;181m{heading}\x1b[0m")
    } else {
        writeln!(output, "{heading}")
    }
}

fn parse_inspect(values: &[String]) -> Result<(PathBuf, Option<NonZeroU8>), Box<dyn Error>> {
    let input = values.first().filter(|value| !value.starts_with('-'));
    let mut trim_configuration_count = None;
    let mut index = 1;
    while index < values.len() {
        let flag = &values[index];
        if flag != "--trim-config-count" {
            return Err(usage_error().into());
        }
        let value = values.get(index + 1).ok_or_else(usage_error)?;
        trim_configuration_count = Some(parse_trim_configuration_count(value)?);
        index += 2;
    }
    Ok((
        PathBuf::from(input.ok_or_else(usage_error)?),
        trim_configuration_count,
    ))
}

fn inspect(
    input: &Path,
    trim_configuration_count: Option<NonZeroU8>,
) -> Result<(), Box<dyn Error>> {
    let media = load_eac3(input)?;
    let frames = openjoc_eac3::index_syncframes(&media.bytes)?;
    let units = openjoc_eac3::group_access_units(&frames)?;
    println!("input: {}", media_kind_name(media.kind));
    println!("frames: {}", frames.len());
    println!("access units: {}", units.len());
    let mut aux_present = 0_usize;
    let mut aux_absent = 0_usize;
    let mut emdf_attempts = 0_usize;
    let mut emdf_parsed = 0_usize;
    let mut skip_examined = 0_usize;
    let mut skip_observed = 0_usize;
    let mut skip_unresolved = 0_usize;
    let mut skip_non_emdf = 0_usize;
    let mut skip_valid_emdf = 0_usize;
    let mut skip_malformed_emdf = 0_usize;
    let mut frame_end_non_emdf = 0_usize;
    let mut frame_end_malformed_emdf = 0_usize;
    let mut skip_errors = Vec::new();
    for entry in &frames {
        let end = entry
            .offset
            .checked_add(entry.header.frame_size)
            .ok_or_else(|| io::Error::other("E-AC-3 frame offset overflow"))?;
        let frame = media.bytes.get(entry.offset..end).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "truncated indexed E-AC-3 frame",
            )
        })?;
        match openjoc_eac3::classify_aux_emdf(frame)? {
            Some(classification) => {
                aux_present += 1;
                emdf_attempts += 1;
                match classification {
                    openjoc_emdf::CarrierClassification::NonEmdf => frame_end_non_emdf += 1,
                    openjoc_emdf::CarrierClassification::Parsed(_) => emdf_parsed += 1,
                    openjoc_emdf::CarrierClassification::Malformed(_)
                    | openjoc_emdf::CarrierClassification::TrailingData { .. } => {
                        frame_end_malformed_emdf += 1;
                    }
                }
            }
            None => aux_absent += 1,
        }
        match openjoc_eac3::inspect_audio_block_carriers(frame, |value| {
            skip_examined += 1;
            if let Some(skip) = value.skip_field.as_ref() {
                skip_observed += 1;
                match openjoc_eac3::classify_skip_field_emdf(skip) {
                    openjoc_emdf::CarrierClassification::NonEmdf => skip_non_emdf += 1,
                    openjoc_emdf::CarrierClassification::Parsed(_) => skip_valid_emdf += 1,
                    openjoc_emdf::CarrierClassification::Malformed(_)
                    | openjoc_emdf::CarrierClassification::TrailingData { .. } => {
                        skip_malformed_emdf += 1;
                    }
                }
            }
        }) {
            Ok(carrier) => skip_unresolved += carrier.unresolved_blocks,
            Err(error) => {
                skip_unresolved += usize::from(entry.header.audio_blocks);
                skip_errors.push(error.to_string());
            }
        }
    }
    println!("carrier paths examined:");
    println!("  frame-end auxdatae: {aux_present} present, {aux_absent} absent");
    println!(
        "  frame-end EMDF: {emdf_parsed} parsed, {frame_end_non_emdf} non-EMDF, {frame_end_malformed_emdf} malformed from {emdf_attempts} bounded attempts"
    );
    println!(
        "  audio-block skipfld: {skip_observed} observed in {skip_examined} reached prefixes; {skip_unresolved} blocks unresolved"
    );
    println!(
        "  skipfld EMDF candidates: {skip_valid_emdf} parsed, {skip_non_emdf} non-EMDF, {skip_malformed_emdf} malformed"
    );
    if !skip_errors.is_empty() {
        println!(
            "  audio-block carrier traversal errors: {}",
            skip_errors.len()
        );
        for error in skip_errors.iter().take(3) {
            println!("    {error}");
        }
    }
    for (unit_index, unit) in units.iter().copied().enumerate() {
        println!("access unit {unit_index}:");
        println!("  sample rate: {} Hz", unit.sample_rate);
        println!("  samples: {}", unit.samples);
        match openjoc_eac3::parse_joc_access_unit(&media.bytes, &frames, unit) {
            Ok(Some(parsed)) => {
                println!("  carrier frame: {}", parsed.carrier_frame);
                println!("  complexity index: {}", parsed.complexity_index);
                for profile in [
                    JocValidationProfile::EtsiStrict,
                    JocValidationProfile::DolbyVendorCompat,
                ] {
                    print_profile_validation(&parsed, profile, trim_configuration_count);
                }
            }
            Ok(None) => {
                if let Some(extension) =
                    extract_joc_addbsi_access_unit(&media.bytes, &frames, unit)?
                {
                    println!(
                        "  JOC extension signaled: complexity index {}; EMDF profile absent (examined frame-end/skipfld carrier candidates)",
                        extension.complexity_index
                    );
                } else {
                    println!("  JOC profile: absent");
                }
            }
            Err(error) => {
                println!("  JOC profile candidate parsing failed in examined carriers: {error}");
            }
        }
    }
    Ok(())
}

fn diagnose_tools(values: &[String]) -> Result<(), Box<dyn Error>> {
    let input = values
        .first()
        .filter(|value| !value.starts_with('-'))
        .ok_or_else(usage_error)?;
    let mut vector_id = None;
    let mut output = None;
    let mut index = 1;
    while index < values.len() {
        match values[index].as_str() {
            "--vector-id" => {
                vector_id = values.get(index + 1).cloned();
                index += 2;
            }
            "--json" => {
                output = values.get(index + 1).map(PathBuf::from);
                index += 2;
            }
            _ => return Err(usage_error().into()),
        }
    }
    let vector_id = vector_id.ok_or_else(usage_error)?;
    let output = output.ok_or_else(usage_error)?;
    let media = load_eac3(Path::new(input))?;
    let frames = openjoc_eac3::index_syncframes(&media.bytes)?;
    let units = openjoc_eac3::group_access_units(&frames)?;
    let dither = deterministic_dither_values();
    let mut inventories = Vec::new();
    let mut failures = Vec::new();
    for (au_index, unit) in units.iter().enumerate() {
        let entry = frames[unit.first_frame];
        let end = entry.offset + entry.header.frame_size;
        let bytes = media
            .bytes
            .get(entry.offset..end)
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "truncated frame"))?;
        let parsed = openjoc_eac3::parse_audio_frame(bytes)?;
        match openjoc_eac3::decode_audio_blocks_with_parsed_frame(
            bytes,
            &parsed,
            &dither,
            InternalBasePolicy::CurrentDefault,
        )
        .and_then(|blocks| emit_coding_tool_inventory(&vector_id, au_index, &parsed, &blocks))
        {
            Ok(inventory) => inventories.push(inventory),
            Err(error) => {
                failures
                    .push(serde_json::json!({"au_index": au_index, "error": error.to_string()}));
            }
        }
    }
    let document = serde_json::json!({
        "schema": "openjoc.coding-tool-inventory.v1",
        "vector_id": vector_id,
        "source_kind": media_kind_name(media.kind),
        "au_count": units.len(),
        "inventory_count": inventories.len(),
        "failed_access_units": failures,
        "inventories": inventories,
        "diagnostic_only": true,
        "production_pcm_unchanged": true,
    });
    if output.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("refusing to overwrite {}", output.display()),
        )
        .into());
    }
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output, serde_json::to_vec_pretty(&document)?)?;
    Ok(())
}

fn print_profile_validation(
    parsed: &openjoc_eac3::ParsedJocAccessUnit,
    profile: JocValidationProfile,
    trim_configuration_count: Option<NonZeroU8>,
) {
    println!("  profile: {}", profile.as_str());
    match openjoc_eac3::validate_joc_access_unit(parsed, profile) {
        Ok(metadata) => {
            println!("    result: {}", metadata.validation_status.as_str());
            println!("    OAMD bytes: {}", metadata.oamd.len());
            println!("    JOC bytes: {}", metadata.joc.len());
            for deviation in &metadata.deviations {
                println!(
                    "    deviation: payload {} {}={} expected_by_etsi={}",
                    deviation.payload_id,
                    deviation.field,
                    deviation.actual,
                    deviation.expected_by_etsi
                );
            }
            match trim_configuration_count {
                Some(count) => print_oamd_profile_status(
                    &metadata.oamd,
                    profile,
                    OamdDecoderConfig {
                        trim_configuration_count: Some(count),
                    },
                ),
                None => println!("    OAMD partial: not_attempted_without_trim_config_count"),
            }
        }
        Err(openjoc_eac3::Eac3Error::JocProfileValidation(failure)) => {
            println!("    result: failed");
            println!("    reason: {failure}");
            for deviation in &failure.deviations {
                println!(
                    "    deviation: payload {} {}={} expected_by_etsi={}",
                    deviation.payload_id,
                    deviation.field,
                    deviation.actual,
                    deviation.expected_by_etsi
                );
            }
        }
        Err(error) => {
            println!("    result: failed");
            println!("    reason: {error}");
        }
    }
}

fn print_oamd_profile_status(
    payload: &[u8],
    profile: JocValidationProfile,
    config: OamdDecoderConfig,
) {
    let parsed = match profile {
        JocValidationProfile::EtsiStrict => {
            openjoc_oamd::parse_oamd_payload_with_config(payload, config)
        }
        JocValidationProfile::DolbyVendorCompat => openjoc_oamd::parse_oamd_payload_with_profile(
            payload,
            config,
            OamdParseProfile::DolbyVendorCompat,
            openjoc_oamd::OAMD_PAYLOAD_ID,
        ),
    };
    match parsed {
        Ok(parsed) => {
            let object_element = parsed
                .elements
                .iter()
                .find(|metadata| matches!(metadata.element, openjoc_oamd::OamdElement::Objects(_)));
            let opaque_trim = parsed.elements.iter().find_map(|metadata| {
                if let openjoc_oamd::OamdElement::OpaqueObservedKnownElement(opaque) =
                    &metadata.element
                {
                    Some(opaque)
                } else {
                    None
                }
            });
            println!(
                "    OAMD result: {}",
                if opaque_trim.is_some() {
                    "accepted_with_deviation"
                } else {
                    "accepted"
                }
            );
            println!(
                "    OAMD object element: {}",
                if object_element.is_some() {
                    "parsed"
                } else {
                    "blocked"
                }
            );
            if let Some(opaque) = opaque_trim {
                println!(
                    "    OAMD trim element: opaque unresolved; raw warp={} payload-relative bits=[{},{}] deviation={}",
                    opaque.raw_warp,
                    opaque.warp_payload_start_bit,
                    opaque.warp_payload_end_bit,
                    opaque.deviation_code,
                );
                println!(
                    "    vendor continuation: status={} payload-relative bits=[{},{}] length_bits={} sha256={} provenance={} interpretation={}",
                    opaque.preservation_status,
                    opaque.continuation_payload_start_bit,
                    opaque.continuation_payload_end_bit,
                    opaque
                        .continuation_element_relative_end_bit
                        .saturating_sub(opaque.continuation_element_relative_start_bit),
                    opaque.continuation_sha256,
                    opaque.provenance,
                    opaque.interpretation_status,
                );
                println!("    OAMD trim timeline: unavailable");
                println!("    OAMD renderer fidelity: ineligible");
            } else {
                println!("    OAMD trim element: parsed or absent");
            }
        }
        Err(error) => {
            println!("    OAMD result: failed");
            println!("    OAMD reason: {error}");
        }
    }
}

fn run_census(values: &[String]) -> Result<(), Box<dyn Error>> {
    let mut manifest = None;
    let mut output = None;
    let mut index = 0;
    while index < values.len() {
        let value = &values[index];
        if value == "-o" || value == "--output" {
            let path = values.get(index + 1).ok_or_else(usage_error)?;
            output = Some(PathBuf::from(path));
            index += 2;
        } else if value.starts_with('-') {
            return Err(usage_error().into());
        } else if manifest.is_none() {
            manifest = Some(PathBuf::from(value));
            index += 1;
        } else {
            return Err(usage_error().into());
        }
    }
    let manifest = manifest
        .or_else(|| env::var_os("OPENJOC_REAL_FIXTURE_MANIFEST").map(PathBuf::from))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "missing fixture manifest; pass MANIFEST or set OPENJOC_REAL_FIXTURE_MANIFEST",
            )
        })?;
    let output = output.ok_or_else(usage_error)?;
    let report = fixture_census::run_census(&manifest)?;
    fixture_census::write_reports(&report, &output)?;
    println!(
        "census: {} fixtures written to {}",
        report.fixtures.len(),
        output.display()
    );
    Ok(())
}

fn decode_payload(values: &[String]) -> Result<(), Box<dyn Error>> {
    let arguments = parse_decode_payload(values)?;
    let downmix = decode(&fs::read(&arguments.downmix)?)?;
    let joc_payload = fs::read(&arguments.joc)?;
    let oamd_payload = fs::read(&arguments.oamd)?;
    let oamd_profile = match arguments.validation_profile {
        JocValidationProfile::EtsiStrict => OamdParseProfile::EtsiStrict,
        JocValidationProfile::DolbyVendorCompat => OamdParseProfile::DolbyVendorCompat,
    };
    let mut decoder = PayloadDecoder::with_oamd_profile(
        PayloadDecoderConfig {
            reference_screen: arguments.reference_screen,
            oamd: OamdDecoderConfig {
                trim_configuration_count: arguments.trim_count,
            },
        },
        oamd_profile,
    );
    decoder.decode_frame_with(
        JocFrameInput {
            sample_rate: downmix.sample_rate,
            downmix_pcm: &downmix.channels,
            base_lfe_pcm: None,
            joc_payload: &joc_payload,
            oamd_payload: &oamd_payload,
            frame_index: 0,
        },
        |frame| write_debug(&arguments.output, 0, frame, arguments.validation_profile),
    )?;
    let scene = decoder.finish()?;
    write_scene(&arguments.output, &scene, arguments.output_format)?;
    Ok(())
}

fn parse_decode_eac3(values: &[String]) -> Result<DecodeEac3Args, Box<dyn Error>> {
    let input = values.first().filter(|value| !value.starts_with('-'));
    let mut downmix = None;
    let mut internal_base = false;
    let mut reference_f64 = false;
    let mut validation_profile = JocValidationProfile::EtsiStrict;
    let mut trim_configuration_count = None;
    let mut internal_base_policy = InternalBasePolicy::CurrentDefault;
    let mut streaming = false;
    let mut output = None;
    let mut index = 1;
    while index < values.len() {
        let flag = &values[index];
        if flag == "--internal-base" {
            internal_base = true;
            index += 1;
            continue;
        }
        if flag == "--streaming" {
            streaming = true;
            index += 1;
            continue;
        }
        if flag == "--reference-f64" {
            reference_f64 = true;
            index += 1;
            continue;
        }
        let value = values.get(index + 1).ok_or_else(usage_error)?;
        match flag.as_str() {
            "--downmix" => downmix = Some(PathBuf::from(value)),
            "-o" | "--output" => output = Some(PathBuf::from(value)),
            "--validation-profile" => validation_profile = parse_validation_profile(value)?,
            "--internal-base-policy" => internal_base_policy = parse_internal_base_policy(value)?,
            "--trim-config-count" => {
                trim_configuration_count = Some(parse_trim_configuration_count(value)?);
            }
            _ => return Err(usage_error().into()),
        }
        index += 2;
    }
    if internal_base && downmix.is_some() {
        return Err(usage_error().into());
    }
    Ok(DecodeEac3Args {
        input: PathBuf::from(input.ok_or_else(usage_error)?),
        downmix,
        internal_base,
        output: output.ok_or_else(usage_error)?,
        output_format: if reference_f64 {
            SampleFormat::F64
        } else {
            SampleFormat::F32
        },
        validation_profile,
        trim_configuration_count,
        internal_base_policy,
        streaming,
    })
}

fn parse_internal_base_policy(value: &str) -> Result<InternalBasePolicy, io::Error> {
    match value {
        "current-default" | "CURRENT_DEFAULT" => Ok(InternalBasePolicy::CurrentDefault),
        "codec-core" | "CODEC_CORE" => Ok(InternalBasePolicy::CodecCore),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown internal base policy {value}; expected current-default or codec-core"),
        )),
    }
}

fn parse_trim_configuration_count(value: &str) -> Result<NonZeroU8, io::Error> {
    let count = value.parse::<u8>().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid OAMD trim configuration count {value}; expected 1..=255"),
        )
    })?;
    NonZeroU8::new(count).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid OAMD trim configuration count 0; expected 1..=255",
        )
    })
}

fn parse_validation_profile(value: &str) -> Result<JocValidationProfile, io::Error> {
    match value {
        "etsi-strict" | "ETSI_STRICT" => Ok(JocValidationProfile::EtsiStrict),
        "dolby-vendor-compat" | "DOLBY_VENDOR_COMPAT" => {
            Ok(JocValidationProfile::DolbyVendorCompat)
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "unknown validation profile {value}; expected etsi-strict or dolby-vendor-compat"
            ),
        )),
    }
}

fn decode_eac3(arguments: &DecodeEac3Args) -> Result<(), Box<dyn Error>> {
    if arguments.streaming {
        return decode_eac3_streaming(arguments);
    }
    let media = load_eac3(&arguments.input)?;
    let stream = &media.bytes;
    let config = PayloadDecoderConfig {
        reference_screen: None,
        oamd: OamdDecoderConfig {
            trim_configuration_count: arguments.trim_configuration_count,
        },
    };
    let sink_output = arguments.output.clone();
    let scene = if arguments.internal_base {
        let dither = deterministic_dither_values();
        let mut base_capture = InternalBasePcm {
            base_policy: arguments.internal_base_policy,
            ..InternalBasePcm::default()
        };
        let scene = eac3_decode::decode_internal_eac3_with_base_sink_and_policy(
            stream,
            config,
            arguments.validation_profile,
            &dither,
            arguments.internal_base_policy,
            |frame_index, metadata, frame| {
                write_validation_debug(&sink_output, frame_index, metadata)
                    .and_then(|()| {
                        write_debug(
                            &sink_output,
                            frame_index,
                            frame,
                            metadata.validation_profile,
                        )
                    })
                    .map_err(|error| eac3_decode::DecodeEac3Error::Sink(error.to_string()))
            },
            |access_unit, pcm| {
                base_capture
                    .append(access_unit, pcm)
                    .map_err(eac3_decode::DecodeEac3Error::Sink)
            },
        )?;
        base_capture.write(&sink_output)?;
        scene
    } else {
        let base_paths = match &arguments.downmix {
            Some(path) => CompatibleBasePaths {
                full: None,
                downmix: path.clone(),
                lfe: None,
            },
            None => decode_base_audio(&arguments.input, &arguments.output)?,
        };
        let downmix = decode(&fs::read(&base_paths.downmix)?)?;
        let lfe = base_paths
            .lfe
            .as_ref()
            .map(|path| -> Result<_, Box<dyn Error>> { Ok(decode(&fs::read(path)?)?) })
            .transpose()?;
        write_compatible_base_inventory(&arguments.output, &base_paths, &downmix, lfe.as_ref())?;
        eac3_decode::decode_aligned_eac3_with_sink_and_lfe(
            stream,
            &downmix,
            lfe.as_ref(),
            config,
            arguments.validation_profile,
            |frame_index, metadata, frame| {
                write_validation_debug(&sink_output, frame_index, metadata)
                    .and_then(|()| {
                        write_debug(
                            &sink_output,
                            frame_index,
                            frame,
                            metadata.validation_profile,
                        )
                    })
                    .map_err(|error| eac3_decode::DecodeEac3Error::Sink(error.to_string()))
            },
        )?
    };
    write_scene(&arguments.output, &scene, arguments.output_format)
}

fn decode_eac3_streaming(arguments: &DecodeEac3Args) -> Result<(), Box<dyn Error>> {
    if !arguments.internal_base {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--streaming currently requires --internal-base for a sequential raw E-AC-3 input",
        )
        .into());
    }
    let mut probe = fs::File::open(&arguments.input)?;
    let mut prefix = [0_u8; 12];
    let prefix_len = probe.read(&mut prefix)?;
    if detect_media(&prefix[..prefix_len]) != InputMediaKind::RawEac3 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--streaming requires a raw E-AC-3 input; ISO BMFF remains an explicit indexed/capture path",
        )
        .into());
    }
    let config = PayloadDecoderConfig {
        reference_screen: None,
        oamd: OamdDecoderConfig {
            trim_configuration_count: arguments.trim_configuration_count,
        },
    };
    let sink_output = arguments.output.clone();
    let mut base_capture = InternalBasePcm {
        base_policy: arguments.internal_base_policy,
        ..InternalBasePcm::default()
    };
    let dither = deterministic_dither_values();
    let summary = eac3_decode::decode_internal_eac3_reader_with_base_sink_and_policy(
        fs::File::open(&arguments.input)?,
        DEFAULT_MAX_EAC3_BYTES,
        config,
        arguments.validation_profile,
        &dither,
        arguments.internal_base_policy,
        |frame_index, metadata, frame| {
            write_validation_debug(&sink_output, frame_index, metadata)
                .and_then(|()| {
                    write_debug(
                        &sink_output,
                        frame_index,
                        frame,
                        metadata.validation_profile,
                    )
                })
                .map_err(|error| eac3_decode::DecodeEac3Error::Sink(error.to_string()))
        },
        |access_unit, pcm| {
            base_capture
                .append(access_unit, pcm)
                .map_err(eac3_decode::DecodeEac3Error::Sink)
        },
    )?;
    base_capture.write(&arguments.output)?;
    write_streaming_summary(&arguments.output, &summary)?;
    Ok(())
}

fn write_streaming_summary(
    output: &Path,
    summary: &openjoc_scene::StreamingSceneSummary,
) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(output.join("debug"))?;
    let value = serde_json::json!({
        "source": "OpenJOC direct raw E-AC-3 incremental AU consumer",
        "sample_rate": summary.sample_rate,
        "duration_samples": summary.duration_samples,
        "frames": summary.frames,
        "object_count": summary.object_count,
        "max_reconstruction_rows": summary.max_reconstruction_rows,
        "max_frame_samples": summary.max_frame_samples,
        "metadata_events": summary.metadata_events,
        "trim_events": summary.trim_events,
        "retention": "streaming summary only; no ObjectScene capture",
    });
    fs::write(
        output.join("debug/streaming_summary.json"),
        serde_json::to_vec_pretty(&value)?,
    )?;
    Ok(())
}

fn deterministic_dither_values() -> Vec<f64> {
    // TS 102 366 clause 6.3.4 permits any reasonably random sequence and
    // accepts a +/-0.5 uniform range. A fixed xorshift stream makes CLI output
    // reproducible while retaining the required non-zero noise behavior.
    let mut state = 0x6d2b_79f5_u32;
    (0..32_768)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (f64::from(state) / f64::from(u32::MAX) - 0.5) * 0.5
        })
        .collect()
}

struct CompatibleBasePaths {
    full: Option<PathBuf>,
    downmix: PathBuf,
    lfe: Option<PathBuf>,
}

#[derive(Default)]
struct InternalBasePcm {
    base_policy: InternalBasePolicy,
    sample_rate: Option<u32>,
    full: Vec<Vec<f64>>,
    joc_input: Vec<Vec<f64>>,
    lfe: Option<Vec<f64>>,
    access_units: usize,
    samples_per_access_unit: Vec<usize>,
}

impl InternalBasePcm {
    fn append(&mut self, access_unit: usize, pcm: &DecodedAccessUnitPcm) -> Result<(), String> {
        if access_unit != self.access_units {
            return Err(format!(
                "internal base access-unit sequence expected {}, received {}",
                self.access_units, access_unit
            ));
        }
        if pcm.channels.len() != 5 {
            return Err(format!(
                "internal base exposes {} full-band channels; expected five JOC inputs",
                pcm.channels.len()
            ));
        }
        let frame_samples = usize::from(pcm.samples);
        if pcm
            .channels
            .iter()
            .any(|channel| channel.len() != frame_samples)
        {
            return Err("internal base channel frame length mismatch".to_owned());
        }
        if let Some(expected) = self.sample_rate
            && expected != pcm.sample_rate
        {
            return Err(format!(
                "internal base sample rate changed from {} to {}",
                expected, pcm.sample_rate
            ));
        }
        self.sample_rate.get_or_insert(pcm.sample_rate);
        if self.joc_input.is_empty() {
            self.joc_input = vec![Vec::new(); pcm.channels.len()];
        }
        for (destination, source) in self.joc_input.iter_mut().zip(&pcm.channels) {
            destination.extend_from_slice(source);
        }
        match (&mut self.lfe, &pcm.lfe) {
            (None, None) => {}
            (None, Some(source)) => {
                if source.len() != frame_samples {
                    return Err("internal base LFE frame length mismatch".to_owned());
                }
                self.lfe = Some(source.clone());
            }
            (Some(destination), Some(source)) => {
                if source.len() != frame_samples {
                    return Err("internal base LFE frame length mismatch".to_owned());
                }
                destination.extend_from_slice(source);
            }
            (Some(_), None) => {
                return Err("internal base LFE presence changed between access units".to_owned());
            }
        }
        let frame_full = if let Some(lfe) = &pcm.lfe {
            if lfe.len() != frame_samples {
                return Err("internal base LFE frame length mismatch".to_owned());
            }
            vec![
                pcm.channels[0].clone(),
                pcm.channels[1].clone(),
                pcm.channels[2].clone(),
                lfe.clone(),
                pcm.channels[3].clone(),
                pcm.channels[4].clone(),
            ]
        } else {
            pcm.channels.clone()
        };
        if self.full.is_empty() {
            self.full = vec![Vec::new(); frame_full.len()];
        }
        for (destination, source) in self.full.iter_mut().zip(frame_full) {
            destination.extend_from_slice(&source);
        }
        self.samples_per_access_unit.push(frame_samples);
        self.access_units += 1;
        Ok(())
    }

    fn write(&self, output: &Path) -> Result<(), Box<dyn Error>> {
        let sample_rate = self
            .sample_rate
            .ok_or_else(|| io::Error::other("internal base produced no PCM"))?;
        let options = WaveEncodeOptions {
            sample_format: SampleFormat::F64,
            clipping: Clipping::Reject,
            dither: Dither::None,
        };
        let debug = output.join("debug");
        fs::create_dir_all(&debug)?;
        fs::write(
            debug.join("internal_base_full.wav"),
            encode_channels(sample_rate, &self.full, options)?,
        )?;
        fs::write(
            debug.join("internal_base_joc_input.wav"),
            encode_channels(sample_rate, &self.joc_input, options)?,
        )?;
        if let Some(lfe) = &self.lfe {
            fs::write(
                debug.join("internal_base_lfe.wav"),
                encode_channels(sample_rate, std::slice::from_ref(lfe), options)?,
            )?;
        }
        let full_order = if self.lfe.is_some() {
            vec!["FL", "FR", "FC", "LFE", "SL", "SR"]
        } else {
            vec!["FL", "FR", "FC", "SL", "SR"]
        };
        let inventory = serde_json::json!({
            "source": "OpenJOC internal E-AC-3 decoder",
            "base_policy": format!("{:?}", self.base_policy),
            "sample_rate": sample_rate,
            "access_units": self.access_units,
            "samples_per_access_unit": self.samples_per_access_unit,
            "sample_count": self.full.first().map_or(0, Vec::len),
            "full_base": {
                "wav": "debug/internal_base_full.wav",
                "channel_order": full_order,
                "channel_count": self.full.len(),
            },
            "joc_input": {
                "wav": "debug/internal_base_joc_input.wav",
                "channel_order": ["FL", "FR", "FC", "SL", "SR"],
                "channel_count": self.joc_input.len(),
                "lfe_excluded": true,
            },
            "lfe": {
                "wav": self.lfe.as_ref().map(|_| "debug/internal_base_lfe.wav"),
                "channel_order": ["LFE"],
                "present": self.lfe.is_some(),
            },
            "decoder_delay": "not independently exposed; compare-base report estimates bounded alignment",
            "overlap_add_state": "stateful TDAC retained across access units; reset at stream start",
            "dynrng_policy": "OpenJOC internal decoder path; no FFmpeg presentation normalization",
            "dither_policy": "deterministic injected dither sequence",
        });
        fs::write(
            debug.join("internal_base_inventory.json"),
            serde_json::to_vec_pretty(&inventory)?,
        )?;
        Ok(())
    }
}

fn decode_base_audio(input: &Path, output: &Path) -> Result<CompatibleBasePaths, Box<dyn Error>> {
    let debug = output.join("debug");
    fs::create_dir_all(&debug)?;
    // The JOC matrix consumes five non-LFE channels. Keep the base LFE in a
    // separate file so it can be bound to an OAMD speaker entry without
    // entering the JOC row matrix.
    let full_pcm = debug.join("compatible_base_full.wav");
    let base_pcm = debug.join("compatible_base.wav");
    let layout_probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "a:0",
            "-show_entries",
            "stream=channels,channel_layout",
            "-of",
            "default=nw=1",
        ])
        .arg(input)
        .output()?;
    if !layout_probe.status.success() {
        return Err(io::Error::other("could not inspect base E-AC-3 channel layout").into());
    }
    let layout = String::from_utf8_lossy(&layout_probe.stdout).to_ascii_lowercase();
    let has_lfe = layout.contains("5.1") || layout.contains("lfe");
    let result = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-i"])
        .arg(input)
        .args([
            "-map",
            "0:a:0",
            "-af",
            "pan=6c|c0=FL|c1=FR|c2=FC|c3=LFE|c4=SL|c5=SR",
            "-c:a",
            "pcm_f64le",
        ])
        .arg(&full_pcm)
        .output()?;
    if !result.status.success() {
        return Err(io::Error::other(format!(
            "full base E-AC-3 decode failed: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        ))
        .into());
    }
    let result = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-i"])
        .arg(input)
        .args([
            "-map",
            "0:a:0",
            "-af",
            "pan=5c|c0=FL|c1=FR|c2=FC|c3=SL|c4=SR",
            "-c:a",
            "pcm_f64le",
        ])
        .arg(&base_pcm)
        .output()?;
    if !result.status.success() {
        return Err(io::Error::other(format!(
            "base E-AC-3 decode failed: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        ))
        .into());
    }
    let lfe = if has_lfe {
        let lfe_path = debug.join("compatible_base_lfe.wav");
        let result = Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-i"])
            .arg(input)
            .args([
                "-map",
                "0:a:0",
                "-af",
                "pan=mono|c0=LFE",
                "-c:a",
                "pcm_f64le",
            ])
            .arg(&lfe_path)
            .output()?;
        if !result.status.success() {
            return Err(io::Error::other(format!(
                "base LFE decode failed: {}",
                String::from_utf8_lossy(&result.stderr).trim()
            ))
            .into());
        }
        Some(lfe_path)
    } else {
        None
    };
    Ok(CompatibleBasePaths {
        full: Some(full_pcm),
        downmix: base_pcm,
        lfe,
    })
}

fn write_compatible_base_inventory(
    output: &Path,
    paths: &CompatibleBasePaths,
    downmix: &WavePcm,
    lfe: Option<&WavePcm>,
) -> Result<(), Box<dyn Error>> {
    let full = paths
        .full
        .as_ref()
        .map(|path| -> Result<WavePcm, Box<dyn Error>> { Ok(decode(&fs::read(path)?)?) })
        .transpose()?;
    let sample_count = downmix.channels.first().map_or(0, Vec::len);
    let inventory = serde_json::json!({
        "source": "FFmpeg compatible-base decode",
        "ffmpeg_command": {
            "full": "ffmpeg -v error -y -i INPUT -map 0:a:0 -af pan=6c|c0=FL|c1=FR|c2=FC|c3=LFE|c4=SL|c5=SR -c:a pcm_f64le compatible_base_full.wav",
            "joc_input": "ffmpeg -v error -y -i INPUT -map 0:a:0 -af pan=5c|c0=FL|c1=FR|c2=FC|c3=SL|c4=SR -c:a pcm_f64le compatible_base.wav",
            "lfe": "ffmpeg -v error -y -i INPUT -map 0:a:0 -af pan=mono|c0=LFE -c:a pcm_f64le compatible_base_lfe.wav",
        },
        "input_track": {
            "selection": "0:a:0",
            "ffprobe_layout": "5.1(side) observed on controlled Logic stream",
            "sample_rate": downmix.sample_rate,
        },
        "full_base": {
            "wav": paths.full.as_ref().map(|_| "debug/compatible_base_full.wav"),
            "channel_order": ["FL", "FR", "FC", "LFE", "SL", "SR"],
            "channel_count": full.as_ref().map_or(0, |pcm| pcm.channels.len()),
            "sample_count": full.as_ref().and_then(|pcm| pcm.channels.first()).map_or(0, Vec::len),
        },
        "joc_input": {
            "wav": "debug/compatible_base.wav",
            "channel_order": ["FL", "FR", "FC", "SL", "SR"],
            "channel_count": downmix.channels.len(),
            "sample_count": sample_count,
            "lfe_excluded": true,
        },
        "lfe": {
            "wav": lfe.map(|_| "debug/compatible_base_lfe.wav"),
            "channel_order": ["LFE"],
            "present": lfe.is_some(),
            "sample_count": lfe.and_then(|pcm| pcm.channels.first()).map_or(0, Vec::len),
        },
        "dialnorm_policy": "FFmpeg defaults; not independently normalized by OpenJOC",
        "dynrng_policy": "FFmpeg defaults; exact presentation policy must be verified separately",
        "resampling": false,
        "decoder_delay": "not independently exposed; compare-base report estimates bounded alignment",
    });
    fs::create_dir_all(output.join("debug"))?;
    fs::write(
        output.join("debug/compatible_base_inventory.json"),
        serde_json::to_vec_pretty(&inventory)?,
    )?;
    Ok(())
}

fn media_kind_name(kind: InputMediaKind) -> &'static str {
    match kind {
        InputMediaKind::RawEac3 => "raw E-AC-3",
        InputMediaKind::IsoBmff => "ISO BMFF (stream-copied E-AC-3)",
        InputMediaKind::Unknown => "unknown",
    }
}

fn parse_decode_payload(values: &[String]) -> Result<DecodePayloadArgs, Box<dyn Error>> {
    let mut downmix = None;
    let mut joc = None;
    let mut oamd = None;
    let mut output = None;
    let mut trim_count = None;
    let mut validation_profile = JocValidationProfile::EtsiStrict;
    let mut reference_f64 = false;
    let mut screen = [None; 5];
    let mut index = 0;
    while index < values.len() {
        let flag = &values[index];
        if flag == "--reference-f64" {
            reference_f64 = true;
            index += 1;
            continue;
        }
        let value = values.get(index + 1).ok_or_else(usage_error)?;
        match flag.as_str() {
            "--downmix" => downmix = Some(PathBuf::from(value)),
            "--joc" => joc = Some(PathBuf::from(value)),
            "--oamd" => oamd = Some(PathBuf::from(value)),
            "-o" | "--output" => output = Some(PathBuf::from(value)),
            "--trim-config-count" => {
                let parsed = value.parse::<u8>()?;
                trim_count = Some(NonZeroU8::new(parsed).ok_or_else(usage_error)?);
            }
            "--validation-profile" => validation_profile = parse_validation_profile(value)?,
            "--screen-origin-x" => screen[0] = Some(value.parse::<f64>()?),
            "--screen-origin-y" => screen[1] = Some(value.parse::<f64>()?),
            "--screen-origin-z" => screen[2] = Some(value.parse::<f64>()?),
            "--screen-width" => screen[3] = Some(value.parse::<f64>()?),
            "--screen-height" => screen[4] = Some(value.parse::<f64>()?),
            _ => return Err(usage_error().into()),
        }
        index += 2;
    }
    let reference_screen = if screen.iter().all(Option::is_none) {
        None
    } else if let [Some(x), Some(y), Some(z), Some(width), Some(height)] = screen {
        Some(ReferenceScreen {
            bottom_left: Position3 { x, y, z },
            width,
            height,
        })
    } else {
        return Err(usage_error().into());
    };
    Ok(DecodePayloadArgs {
        downmix: downmix.ok_or_else(usage_error)?,
        joc: joc.ok_or_else(usage_error)?,
        oamd: oamd.ok_or_else(usage_error)?,
        output: output.ok_or_else(usage_error)?,
        trim_count,
        validation_profile,
        reference_screen,
        output_format: if reference_f64 {
            SampleFormat::F64
        } else {
            SampleFormat::F32
        },
    })
}

fn write_scene(
    output: &Path,
    scene: &openjoc_scene::ObjectScene,
    sample_format: SampleFormat,
) -> Result<(), Box<dyn Error>> {
    let rows = output.join("diagnostics/reconstruction_rows");
    let metadata = output.join("metadata");
    fs::create_dir_all(&rows)?;
    fs::create_dir_all(&metadata)?;
    fs::write(output.join("scene.json"), scene.to_manifest_json_pretty()?)?;
    fs::write(
        metadata.join("timeline.json"),
        scene.to_timeline_json_pretty()?,
    )?;
    fs::write(
        metadata.join("trim_timeline.json"),
        scene.to_trim_timeline_json_pretty()?,
    )?;
    fs::write(
        output.join("diagnostics/reconstruction_basis.json"),
        scene.to_reconstruction_basis_json_pretty()?,
    )?;
    if let Some(basis) = &scene.reconstruction_basis {
        for (row_index, row) in basis.rows.iter().enumerate() {
            let filename = format!("row_{row_index:03}.wav");
            write_incremental_wave(&rows.join(filename), scene.sample_rate, row, sample_format)?;
        }
    }
    if let Some(base_lfe) = &scene.base_lfe_pcm {
        write_incremental_wave(
            &output.join("diagnostics/base_lfe.wav"),
            scene.sample_rate,
            base_lfe,
            sample_format,
        )?;
    }
    Ok(())
}

fn write_incremental_wave(
    path: &Path,
    sample_rate: u32,
    samples: &[f64],
    sample_format: SampleFormat,
) -> Result<(), Box<dyn Error>> {
    let options = WaveEncodeOptions {
        sample_format,
        clipping: Clipping::Reject,
        dither: Dither::None,
    };
    let file = fs::File::create(path)?;
    let mut writer = WaveWriter::new(file, sample_rate, 1, options)?;
    for chunk in samples.chunks(4096) {
        writer.write_channels(&[chunk])?;
    }
    let _file = writer.finish()?;
    Ok(())
}

fn write_debug(
    output: &Path,
    frame_index: usize,
    decoded: &openjoc_scene::DecodedPayloadFrame,
    validation_profile: JocValidationProfile,
) -> Result<(), Box<dyn Error>> {
    let frame = output.join(format!("debug/frame_{frame_index:03}"));
    fs::create_dir_all(&frame)?;
    fs::write(frame.join("joc.txt"), format!("{:#?}\n", decoded.joc))?;
    fs::write(frame.join("oamd.txt"), format!("{:#?}\n", decoded.oamd))?;
    fs::write(
        frame.join("programme_layout.json"),
        serde_json::to_vec_pretty(&decoded.programme_layout)?,
    )?;
    fs::write(
        frame.join("reconstruction.txt"),
        format!("{:#?}\n", decoded.decoded),
    )?;
    let opaque_elements = decoded
        .oamd
        .elements
        .iter()
        .filter_map(|metadata| match &metadata.element {
            openjoc_oamd::OamdElement::OpaqueObservedKnownElement(opaque) => Some(opaque),
            _ => None,
        })
        .collect::<Vec<_>>();
    let status = OamdPartialStatusArtifact {
        profile: validation_profile.as_str(),
        accepted_with_deviation: !opaque_elements.is_empty(),
        oamd_payload_structurally_accepted: true,
        oamd_semantically_complete: opaque_elements.is_empty(),
        object_metadata_status: if decoded
            .oamd
            .elements
            .iter()
            .any(|metadata| matches!(metadata.element, openjoc_oamd::OamdElement::Objects(_)))
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
        semantic_object_audio_binding: "unresolved",
        semantic_binding_state: "unresolved",
        metadata_scene_available: true,
        reconstruction_rows_available: true,
        reconstruction_audio_status: "diagnostic_rows_only",
        audio_bound_objectscene_admissible: false,
        verified_authored_object_pcm_admissible: false,
        renderer_fidelity_eligible: false,
        opaque_elements: opaque_elements
            .iter()
            .map(|opaque| OpaqueTrimArtifact {
                element_id: opaque.element_id,
                declared_bits: opaque.declared_bits,
                declared_bytes: opaque.declared_bytes,
                raw_body_sha256: opaque.raw_body_sha256.clone(),
                raw_warp: opaque.raw_warp,
                warp_payload_bits: [opaque.warp_payload_start_bit, opaque.warp_payload_end_bit],
                body_payload_bits: [opaque.body_payload_start_bit, opaque.body_payload_end_bit],
                continuation_element_relative_bits: [
                    opaque.continuation_element_relative_start_bit,
                    opaque.continuation_element_relative_end_bit,
                ],
                continuation_payload_bits: [
                    opaque.continuation_payload_start_bit,
                    opaque.continuation_payload_end_bit,
                ],
                continuation_bit_length: opaque
                    .continuation_element_relative_end_bit
                    .saturating_sub(opaque.continuation_element_relative_start_bit),
                continuation_sha256: opaque.continuation_sha256.clone(),
                raw_bits_available: true,
                preservation_status: opaque.preservation_status,
                provenance: opaque.provenance,
                interpretation_status: opaque.interpretation_status,
                deviation_code: opaque.deviation_code,
            })
            .collect(),
    };
    fs::write(
        frame.join("oamd_partial_status.json"),
        serde_json::to_vec_pretty(&status)?,
    )?;
    let mut status_text = format!(
        "profile: {}\naccepted_with_deviation: {}\noamd_payload_structurally_accepted: {}\noamd_semantically_complete: {}\nobject_metadata_status: {}\ntrim_metadata_status: {}\ntrim_timeline_available: {}\nsemantic_object_audio_binding: {}\nsemantic_binding_state: {}\nmetadata_scene_available: {}\nreconstruction_rows_available: {}\nreconstruction_audio_status: {}\naudio_bound_objectscene_admissible: {}\nverified_authored_object_pcm_admissible: {}\nrenderer_fidelity_eligible: {}\n",
        status.profile,
        status.accepted_with_deviation,
        status.oamd_payload_structurally_accepted,
        status.oamd_semantically_complete,
        status.object_metadata_status,
        status.trim_metadata_status,
        status.trim_timeline_available,
        status.semantic_object_audio_binding,
        status.semantic_binding_state,
        status.metadata_scene_available,
        status.reconstruction_rows_available,
        status.reconstruction_audio_status,
        status.audio_bound_objectscene_admissible,
        status.verified_authored_object_pcm_admissible,
        status.renderer_fidelity_eligible,
    );
    for opaque in &status.opaque_elements {
        writeln!(
            status_text,
            "opaque_element id={} declared_bits={} raw_body_sha256={} raw_warp={} payload_bits=[{},{}] deviation={}",
            opaque.element_id,
            opaque.declared_bits,
            opaque.raw_body_sha256,
            opaque.raw_warp,
            opaque.warp_payload_bits[0],
            opaque.warp_payload_bits[1],
            opaque.deviation_code,
        )?;
    }
    fs::write(frame.join("oamd_partial_status.txt"), status_text)?;
    Ok(())
}

#[allow(clippy::struct_excessive_bools)]
#[derive(serde::Serialize)]
struct OamdPartialStatusArtifact {
    profile: &'static str,
    accepted_with_deviation: bool,
    oamd_payload_structurally_accepted: bool,
    oamd_semantically_complete: bool,
    object_metadata_status: &'static str,
    trim_metadata_status: &'static str,
    trim_timeline_available: bool,
    semantic_object_audio_binding: &'static str,
    semantic_binding_state: &'static str,
    metadata_scene_available: bool,
    reconstruction_rows_available: bool,
    reconstruction_audio_status: &'static str,
    audio_bound_objectscene_admissible: bool,
    verified_authored_object_pcm_admissible: bool,
    renderer_fidelity_eligible: bool,
    opaque_elements: Vec<OpaqueTrimArtifact>,
}

#[derive(serde::Serialize)]
struct OpaqueTrimArtifact {
    element_id: u8,
    declared_bits: usize,
    declared_bytes: usize,
    raw_body_sha256: String,
    raw_warp: u8,
    warp_payload_bits: [usize; 2],
    body_payload_bits: [usize; 2],
    continuation_element_relative_bits: [usize; 2],
    continuation_payload_bits: [usize; 2],
    continuation_bit_length: usize,
    continuation_sha256: String,
    raw_bits_available: bool,
    preservation_status: &'static str,
    provenance: &'static str,
    interpretation_status: &'static str,
    deviation_code: &'static str,
}

#[derive(serde::Serialize)]
struct ValidationDeviationArtifact {
    payload_id: u64,
    field: &'static str,
    actual: String,
    expected_by_etsi: String,
}

#[derive(serde::Serialize)]
struct ValidationArtifact {
    profile: &'static str,
    result: &'static str,
    deviations: Vec<ValidationDeviationArtifact>,
}

fn write_validation_debug(
    output: &Path,
    frame_index: usize,
    metadata: &openjoc_eac3::JocMetadataFrame,
) -> Result<(), Box<dyn Error>> {
    let frame = output.join(format!("debug/frame_{frame_index:03}"));
    fs::create_dir_all(&frame)?;
    let deviations = metadata
        .deviations
        .iter()
        .map(|deviation| ValidationDeviationArtifact {
            payload_id: deviation.payload_id,
            field: deviation.field.as_str(),
            actual: deviation.actual.to_string(),
            expected_by_etsi: deviation.expected_by_etsi.to_string(),
        })
        .collect::<Vec<_>>();
    let report = ValidationArtifact {
        profile: metadata.validation_profile.as_str(),
        result: metadata.validation_status.as_str(),
        deviations,
    };
    fs::write(
        frame.join("profile_validation.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    let mut text = format!("profile: {}\nresult: {}\n", report.profile, report.result);
    for deviation in &report.deviations {
        writeln!(
            text,
            "deviation: payload {} {}={} expected_by_etsi={}",
            deviation.payload_id, deviation.field, deviation.actual, deviation.expected_by_etsi
        )?;
    }
    fs::write(frame.join("profile_validation.txt"), text)?;
    fs::write(frame.join("emdf.txt"), format!("{:#?}\n", metadata.emdf))?;
    Ok(())
}

fn usage_error() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, USAGE)
}

#[cfg(test)]
mod internal_base_tests {
    use super::InternalBasePcm;
    use openjoc_eac3::DecodedAccessUnitPcm;

    fn frame(value: f64, lfe: Option<f64>) -> DecodedAccessUnitPcm {
        DecodedAccessUnitPcm {
            sample_rate: 48_000,
            samples: 2,
            channels: (0..5)
                .map(|channel| vec![value + channel as f64, value + channel as f64 + 0.5])
                .collect(),
            lfe: lfe.map(|value| vec![value, value + 0.5]),
        }
    }

    #[test]
    fn preserves_full_joc_and_separate_lfe_channel_order() {
        let mut capture = InternalBasePcm::default();
        capture.append(0, &frame(1.0, Some(10.0))).unwrap();
        capture.append(1, &frame(2.0, Some(20.0))).unwrap();

        assert_eq!(capture.sample_rate, Some(48_000));
        assert_eq!(capture.access_units, 2);
        assert_eq!(capture.samples_per_access_unit, vec![2, 2]);
        assert_eq!(capture.joc_input[0], vec![1.0, 1.5, 2.0, 2.5]);
        assert_eq!(capture.joc_input[4], vec![5.0, 5.5, 6.0, 6.5]);
        assert_eq!(capture.lfe, Some(vec![10.0, 10.5, 20.0, 20.5]));
        assert_eq!(capture.full[3], vec![10.0, 10.5, 20.0, 20.5]);
        assert_eq!(capture.full.len(), 6);
    }

    #[test]
    fn rejects_non_sequential_access_units_and_channel_shape_changes() {
        let mut sequence = InternalBasePcm::default();
        let error = sequence.append(1, &frame(0.0, None)).unwrap_err();
        assert!(error.contains("expected 0, received 1"));

        let mut shape = InternalBasePcm::default();
        let mut invalid = frame(0.0, None);
        invalid.channels.pop();
        let error = shape.append(0, &invalid).unwrap_err();
        assert!(error.contains("expected five JOC inputs"));
    }
}
