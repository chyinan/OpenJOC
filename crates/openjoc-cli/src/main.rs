// pattern: Imperative Shell

mod banner;
mod comparison;
mod eac3_decode;
mod fixture_census;
mod joc_render;
mod oamd_forensics;
mod oamd_oracle;
mod performance;
mod progress;
mod render_scene;
mod terminal;

use banner::{package_metadata, render_banner};
use eac3_decode::ValidationProfileRequest;
use openjoc_container::{
    DEFAULT_MAX_EAC3_BYTES, InputMediaError, InputMediaKind, detect_media, load_eac3,
    open_seekable_iso_bmff,
};
use openjoc_eac3::{
    ChannelLocation, DecodedAccessUnitPcm, Eac3Error, InternalBasePolicy,
    emit_coding_tool_inventory, extract_joc_addbsi_access_unit,
};
use openjoc_emdf::{JocProfileDeviation, JocValidationProfile};
use openjoc_oamd::{OamdDecoderConfig, OamdError, OamdParseProfile, Position3, ReferenceScreen};
use openjoc_scene::{
    JocFrameInput, PayloadDecodeError, PayloadDecoder, PayloadDecoderConfig,
    SpatialContributionMode,
};
use openjoc_wave::{
    Clipping, Dither, SampleFormat, WaveEncodeOptions, WaveError, WavePcm, WaveWriter, decode,
    encode_channels,
};
use std::{
    cell::RefCell,
    env,
    error::Error,
    fmt::Write as _,
    fs, io,
    io::{BufRead, IsTerminal, Read, Write as _},
    num::NonZeroU8,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    time::{SystemTime, UNIX_EPOCH},
};
use terminal::TerminalCapabilities;

const USAGE: &str = "usage: openjoc --version\n       openjoc inspect FILE [--trim-config-count N]\n       openjoc decode FILE -o DIR [--downmix FILE | --internal-base] [--streaming] [--internal-base-policy current-default|codec-core] [--validation-profile auto|etsi-strict|observed-vendor-compat] [--trim-config-count N] [--reference-f64]\n       openjoc sofa inspect FILE [--json]\n       openjoc render-scene SCENE --binaural-sofa FILE --output DIR --backend direct|partitioned [--partition-size N] [--block-size N] [--json]\n       openjoc render-joc FILE [--topology TOPOLOGY.json] --layout LAYOUT --output OUTPUT.wav|OUTPUT.caf [--binaural-sofa HRTF.sofa --backend direct|partitioned --partition-size N --lfe-policy exclude|equal-power-dual-mono] [--validation-profile auto|etsi-strict|observed-vendor-compat] [--trim-config-count N] [--internal-base-policy current-default|codec-core] [--reference-f64] [--diagnostic-contribution full|base-only|reconstruction-only] [--no-progress] [--performance-report FILE.json] [--overwrite]\n       openjoc diagnose-tools FILE --vector-id ID --json OUTPUT\n       openjoc census [MANIFEST] -o DIR\n       openjoc diagnose-oamd FILE [-o DIR] [--access-unit N | --au START..END | --all-access-units] [--trim-config-count N] [--diff-payload-11] [--warp-hypotheses] [--adm-reference PATH] [--json PATH] [--force]\n       openjoc decode-payload --downmix FILE --joc FILE --oamd FILE -o DIR [--validation-profile auto|etsi-strict|observed-vendor-compat] [--reference-f64] [--trim-config-count N] [--screen-origin-x X --screen-origin-y Y --screen-origin-z Z --screen-width W --screen-height H]";

// Capture diagnostics are deliberately bounded. Full sample arrays belong in
// the explicit row WAV artifacts; per-frame Debug output must never duplicate
// them and turn a routine decode into an unbounded disk consumer.
const MAX_RETAINED_DEBUG_FRAMES: usize = 64;
const MAX_DEBUG_ARTIFACT_BYTES: usize = 64 * 1024;
const MAX_RETAINED_CAPTURE_PCM_BYTES: u64 = 128 * 1024 * 1024;
const MAX_RECONSTRUCTION_BASIS_JSON_BYTES: usize = 128 * 1024 * 1024;
const ESTIMATED_JSON_BYTES_PER_SAMPLE: u64 = 32;
const MAX_STREAMING_OUTPUT_CHUNKS: usize = 1;

struct DecodePayloadArgs {
    downmix: PathBuf,
    joc: PathBuf,
    oamd: PathBuf,
    output: PathBuf,
    trim_count: Option<NonZeroU8>,
    validation_profile: ValidationProfileRequest,
    reference_screen: Option<ReferenceScreen>,
    output_format: SampleFormat,
}

struct DecodeEac3Args {
    input: PathBuf,
    downmix: Option<PathBuf>,
    internal_base: bool,
    output: PathBuf,
    output_format: SampleFormat,
    validation_profile: ValidationProfileRequest,
    trim_configuration_count: Option<NonZeroU8>,
    internal_base_policy: InternalBasePolicy,
    streaming: bool,
}

struct RenderJocArgs {
    input: PathBuf,
    topology: Option<PathBuf>,
    layout: String,
    output: PathBuf,
    binaural_sofa: Option<PathBuf>,
    binaural_backend: joc_render::BinauralBackend,
    binaural_backend_requested: bool,
    lfe_policy: Option<joc_render::BinauralLfePolicy>,
    validation_profile: ValidationProfileRequest,
    trim_configuration_count: Option<NonZeroU8>,
    internal_base_policy: InternalBasePolicy,
    output_format: SampleFormat,
    no_progress: bool,
    performance_report: Option<PathBuf>,
    diagnostic_contribution: SpatialContributionMode,
    overwrite: bool,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let category = classify_cli_error(error.as_ref());
            eprintln!("openjoc[{}]: {error}", category.as_str());
            if category == CliErrorCategory::ProfileRejection {
                eprintln!(
                    "hint: the requested profile was not relaxed; inspect reports both profiles, and observed-vendor-compat preserves only its documented partial/opaque scope"
                );
            }
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
        Some("-V" | "--version") if arguments.len() == 1 => print_version(),
        Some(command)
            if arguments.len() == 2 && matches!(arguments[1].as_str(), "-h" | "--help") =>
        {
            print_command_help(command)
        }
        Some("inspect") => {
            let (input, trim_configuration_count) = parse_inspect(&arguments[1..])?;
            inspect(&input, trim_configuration_count)
        }
        Some("sofa") => render_scene::run_sofa(&arguments[1..]),
        Some("render-scene") => render_scene::run_render_scene(&arguments[1..]),
        Some("render-joc") => render_joc(&parse_render_joc(&arguments[1..])?, terminal),
        Some("decode-payload") => decode_payload(&arguments[1..]),
        Some("decode") => decode_eac3(&parse_decode_eac3(&arguments[1..])?),
        Some("diagnose-tools") => diagnose_tools(&arguments[1..]),
        Some("census") => run_census(&arguments[1..]),
        Some("diagnose-oamd") => oamd_forensics::run(&arguments[1..]),
        _ => Err(usage_error().into()),
    }
}

fn print_version() -> Result<(), Box<dyn Error>> {
    let version = format!("OpenJOC {}\n", package_metadata().version);
    io::Write::write_all(&mut io::stdout().lock(), version.as_bytes())?;
    Ok(())
}

fn print_root_page(terminal: TerminalCapabilities, help: bool) -> Result<(), Box<dyn Error>> {
    let metadata = package_metadata();
    let context = terminal.banner_context(help, !help);
    let mut output = render_banner(context, metadata);
    if output.is_empty() {
        writeln!(output, "OpenJOC {}", metadata.version)?;
        writeln!(output, "{}", metadata.description)?;
        writeln!(
            output,
            "Inspect metadata. Decode the reconstruction basis.\n"
        )?;
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
        "  openjoc decode <FILE> -o <DIR> [--validation-profile <PROFILE>] [--internal-base] [--streaming]\n",
        "  openjoc census [MANIFEST] -o <DIR>\n",
        "  openjoc diagnose-oamd <FILE> -o <DIR> [--access-unit N | --all-access-units] [--trim-config-count N]\n",
        "  openjoc decode-payload [OPTIONS]\n",
        "  openjoc sofa inspect <FILE> [--json]\n",
        "  openjoc render-scene <SCENE> --binaural-sofa <FILE> --output <DIR> --backend direct|partitioned\n",
        "  openjoc render-joc <FILE> [--topology <TOPOLOGY.json>] --layout <5.1|5.1.2|5.1.4|7.1|7.1.2|7.1.4|7.1.6|9.1|9.1.2|9.1.4|9.1.6> --output <OUTPUT.wav|OUTPUT.caf> [--binaural-sofa <HRTF.sofa> --lfe-policy exclude|equal-power-dual-mono] [--diagnostic-contribution full|base-only|reconstruction-only] [--no-progress] [--performance-report <FILE.json>] [--overwrite]\n",
        "  render-joc supported presets: 5.1, 5.1.2, 5.1.4, 7.1, 7.1.2, 7.1.4, 7.1.6, 9.1, 9.1.2, 9.1.4, 9.1.6\n",
        "  openjoc --help\n",
        "  openjoc --version\n",
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
        "                         [--validation-profile auto|etsi-strict|observed-vendor-compat]\n",
        "                         [--internal-base-policy current-default|codec-core]\n",
        "                         [--trim-config-count N]\n",
        "                         [--reference-f64]\n",
        "  openjoc diagnose-tools <FILE> --vector-id <ID> --json <OUTPUT>\n",
        "  openjoc census [MANIFEST] -o <DIR>\n",
        "  openjoc diagnose-oamd <FILE> [-o <DIR>] [--access-unit N | --au START..END | --all-access-units]\n",
        "                         [--trim-config-count N] [--diff-payload-11] [--warp-hypotheses]\n",
        "                         [--adm-reference PATH] [--json PATH] [--force]\n",
        "  openjoc render-joc <FILE> [--topology <TOPOLOGY.json>] --layout <5.1|5.1.2|5.1.4|7.1|7.1.2|7.1.4|7.1.6|9.1|9.1.2|9.1.4|9.1.6> --output <OUTPUT.wav|OUTPUT.caf> [--binaural-sofa <HRTF.sofa> --backend direct|partitioned --partition-size N --lfe-policy exclude|equal-power-dual-mono]\n",
        "                         [--validation-profile auto|etsi-strict|observed-vendor-compat]\n",
        "                         [--trim-config-count N] [--internal-base-policy current-default|codec-core]\n",
        "                         [--reference-f64] [--diagnostic-contribution full|base-only|reconstruction-only]\n",
        "                         [--no-progress] [--performance-report <FILE.json>] [--overwrite]\n",
        "  openjoc decode-payload --downmix <FILE> --joc <FILE> --oamd <FILE>\n",
        "                         -o <DIR> [--validation-profile auto|etsi-strict|observed-vendor-compat]\n",
        "                         [OPTIONS]\n",
        "\n",
    ));
    append_heading(output, "COMMANDS", color)?;
    output.push_str(concat!(
        "  inspect         Inspect E-AC-3 access units and JOC metadata\n",
        "  decode          Decode metadata plus diagnostic ReconstructionBasis rows\n",
        "  diagnose-tools  Emit diagnostic-only E-AC-3 coding-tool inventory JSON\n",
        "  census          Census bounded metadata carriers from external fixtures\n",
        "  diagnose-oamd   Emit bit-exact EMDF/OAMD entry evidence\n",
        "  decode-payload  Decode supplied downmix, JOC, and OAMD payloads\n",
        "  sofa            Inspect strict SimpleFreeFieldHRIR/CDF-1 SOFA files\n",
        "  render-scene    Render caller-bound static sources to transactional binaural WAV\n",
        "  render-joc      Render decoded JOC through the experimental speaker bridge or SOFA binaural virtualization\n",
        "\n",
    ));
    append_heading(output, "OPTIONS", color)?;
    output.push_str(concat!(
        "  openjoc --version\n",
        "  -h, --help       Print root command help\n",
        "  -V, --version    Print the package version and exit\n",
        "      --no-banner Disable the interactive startup banner\n",
        "      --validation-profile Select AUTO (default for decode), ETSI strict, or explicit observed-vendor compatibility\n",
        "      --internal-base-policy Select current default or codec-core gain policy\n",
        "      --streaming      Bounded AU decode from raw EC3 or seekable ordinary ISO BMFF; requires --internal-base\n",
        "      --trim-config-count Override the normative OAMD trim configuration count (default: 9)\n",
        "      --reference-f64 Use explicit reference f64 reconstruction-row output (default: f32)\n",
        "\n",
        "OUTPUT CONTRACT\n",
        "  capture decode writes a metadata-only scene manifest, a truthful decoded-component manifest, and diagnostic ReconstructionBasis row WAVs\n",
        "  streaming decode writes bounded component WAVs, internal-base diagnostics, and a summary; it does not capture ObjectScene\n",
        "  ReconstructionBasis rows are not authored-object PCM; semantic binding remains unresolved\n",
        "\n",
        "PROFILE / CONTAINER BOUNDARIES\n",
        "  ETSI strict is never auto-downgraded; explicit ETSI_STRICT is never downgraded; reserved syntax is an expected non-zero profile rejection\n",
        "  observed-vendor compatibility is explicit, partial, preserves opaque continuation, and assigns no semantics\n",
        "  non-seekable or fragmented MP4 streaming is not admitted; use a seekable ordinary MP4/M4A file\n",
        "  render-scene accepts only explicit static sources and strict SimpleFreeFieldHRIR/CDF-1 SOFA; no interpolation or JOC bridge\n",
        "  render-joc SUPPORTED PRESETS: 5.1, 5.1.2, 5.1.4, 7.1, 7.1.2, 7.1.4, 7.1.6, 9.1, 9.1.2, 9.1.4, and 9.1.6; bridge control is automatic by default\n",
        "  GENERIC/CUSTOM LIBRARY CAPABILITY: use openjoc_scene::SpatialLayout + JocSpatialBridge; no custom CLI file format\n",
    ));
    Ok(())
}

fn print_command_help(command: &str) -> Result<(), Box<dyn Error>> {
    let help = match command {
        "inspect" => concat!(
            "usage: openjoc inspect <FILE> [--trim-config-count N]\n\n",
            "Inspects raw EC3 or a seekable ordinary MP4/M4A E-AC-3 track. Reports both\n",
            "ETSI_STRICT and OBSERVED_VENDOR_COMPAT validation outcomes without fallback.\n",
        ),
        "decode" => concat!(
            "usage: openjoc decode <FILE> -o <DIR> [--downmix <FILE> | --internal-base]\n",
            "       [--streaming] [--validation-profile auto|etsi-strict|observed-vendor-compat]\n",
            "       [--internal-base-policy current-default|codec-core]\n",
            "       [--trim-config-count N] [--reference-f64]\n\n",
            "Capture mode writes a metadata-only scene plus diagnostic ReconstructionBasis rows.\n",
            "--streaming requires --internal-base, accepts raw EC3 or seekable ordinary ISO BMFF,\n",
            "and writes bounded component WAVs, internal-base diagnostics, and a summary without ObjectScene capture.\n",
            "Rows are not authored-object PCM. AUTO reports strict status and any compatibility selection; explicit ETSI_STRICT never falls back; it is never downgraded.\n",
        ),
        "decode-payload" => concat!(
            "usage: openjoc decode-payload --downmix <FILE> --joc <FILE> --oamd <FILE> -o <DIR>\n",
            "       [--validation-profile auto|etsi-strict|observed-vendor-compat] [--reference-f64]\n",
            "       [--trim-config-count N] [reference-screen options]\n\n",
            "Diagnostic/API-level payload path. Output is a metadata-only scene and separately\n",
            "named ReconstructionBasis rows, never verified authored-object PCM.\n",
        ),
        "sofa" => concat!(
            "usage: openjoc sofa inspect <FILE> [--json]\n\n",
            "Inspects the strict SimpleFreeFieldHRIR / NetCDF classic CDF-1 subset.\n",
        ),
        "render-scene" => concat!(
            "usage: openjoc render-scene <SCENE> --binaural-sofa <FILE> --output <DIR>\n",
            "       --backend direct|partitioned [--partition-size N] [--block-size N] [--json]\n\n",
            "Renders explicit static sources transactionally to stereo float32 WAV.\n",
        ),
        "render-joc" => concat!(
            "usage: openjoc render-joc <FILE> [--topology <TOPOLOGY.json>] --layout <LAYOUT> --output <OUTPUT.wav|OUTPUT.caf>\n",
            "       [--binaural-sofa <HRTF.sofa> --backend direct|partitioned --partition-size N]\n",
            "       [--lfe-policy exclude|equal-power-dual-mono]\n",
            "       [--validation-profile auto|etsi-strict|observed-vendor-compat]\n",
            "       [--trim-config-count N] [--internal-base-policy current-default|codec-core]\n",
            "       [--reference-f64] [--diagnostic-contribution full|base-only|reconstruction-only]\n",
            "       [--no-progress] [--performance-report <FILE.json>] [--overwrite]\n\n",
            "Renders a real supported JOC stream through the experimental JocSpatialBridge.\n",
            "--diagnostic-contribution is expert-only fidelity isolation; FULL is the default.\n",
            "SUPPORTED PRESETS: 5.1, 5.1.2, 5.1.4, 7.1, 7.1.2, 7.1.4, 7.1.6, 9.1, 9.1.2, 9.1.4, and 9.1.6.\n",
            "GENERIC/CUSTOM LIBRARY CAPABILITY: openjoc_scene::SpatialLayout + JocSpatialBridge; no custom CLI file format.\n",
            "Without --topology, bridge control is assembled from decoded real JOC/OAMD state.\n",
            "With --topology, the complete sidecar is an explicit override/test input; sources are not merged.\n",
            "With --binaural-sofa, the selected layout is virtualized to stereo through exact SOFA HRIR directions.\n",
            "Binaural layouts require an explicit LFE policy; direct is the default backend and no vendor-fidelity claim is made.\n",
            "The output extension selects the container: .wav for WAVEFORMATEXTENSIBLE or .caf for Core Audio Format.\n",
            "CAF preserves semantic channel descriptions; no new public speaker preset is introduced here.\n",
            "Progress is enabled on interactive stderr, throttled, and disabled for non-TTY output; --no-progress opts out.\n",
            "--performance-report writes diagnostic JSON with stage timings and realtime metrics.\n",
            "Existing output files prompt once on an interactive terminal ([y/N]); --overwrite skips the prompt.\n",
            "Non-interactive renders refuse existing outputs unless --overwrite is given; replacements remain transactional.\n",
        ),
        "diagnose-tools" => concat!(
            "usage: openjoc diagnose-tools <FILE> --vector-id <ID> --json <OUTPUT>\n\n",
            "Emits diagnostic-only coding-tool activation/state inventory; production PCM is unchanged.\n",
        ),
        "census" => concat!(
            "usage: openjoc census [MANIFEST] -o <DIR>\n\n",
            "Creates deterministic bounded carrier/profile reports from an external fixture manifest.\n",
        ),
        "diagnose-oamd" => concat!(
            "usage: openjoc diagnose-oamd <FILE> [-o <DIR>]\n",
            "       [--access-unit N | --au START..END | --all-access-units]\n",
            "       [--trim-config-count N] [--diff-payload-11] [--warp-hypotheses]\n",
            "       [--adm-reference PATH] [--json PATH] [--force]\n\n",
            "Forensic-only bit evidence. Warp hypotheses are diagnostic and do not change strict/vendor semantics.\n",
        ),
        _ => return Err(usage_error().into()),
    };
    io::Write::write_all(&mut io::stdout().lock(), help.as_bytes())?;
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
    let oamd_config = OamdDecoderConfig::with_trim_configuration_count(trim_configuration_count);
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
                    JocValidationProfile::ObservedVendorCompat,
                ] {
                    print_profile_validation(&parsed, profile, oamd_config);
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
    oamd_config: OamdDecoderConfig,
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
            print_oamd_profile_status(&metadata.oamd, profile, oamd_config);
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
        JocValidationProfile::ObservedVendorCompat => {
            openjoc_oamd::parse_oamd_payload_with_profile(
                payload,
                config,
                OamdParseProfile::ObservedVendorCompat,
                openjoc_oamd::OAMD_PAYLOAD_ID,
            )
        }
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
    ensure_output_directory_available(&arguments.output)?;
    let downmix = read_input_wave(&arguments.downmix)?;
    let joc_payload = fs::read(&arguments.joc)?;
    let oamd_payload = fs::read(&arguments.oamd)?;
    let oamd_config = OamdDecoderConfig::with_trim_configuration_count(arguments.trim_count);
    let oamd_profile = eac3_decode::resolve_profile_for_oamd(
        &oamd_payload,
        oamd_config,
        arguments.validation_profile,
    )?;
    let mut decoder = PayloadDecoder::with_oamd_profile(
        PayloadDecoderConfig {
            reference_screen: arguments.reference_screen,
            oamd: oamd_config,
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
        |frame| {
            let selected_profile = match oamd_profile {
                OamdParseProfile::EtsiStrict => JocValidationProfile::EtsiStrict,
                OamdParseProfile::ObservedVendorCompat => {
                    JocValidationProfile::ObservedVendorCompat
                }
            };
            write_debug(
                &arguments.output,
                0,
                frame,
                arguments.validation_profile,
                selected_profile,
            )
        },
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
    let mut validation_profile = ValidationProfileRequest::Auto;
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

fn parse_render_joc(values: &[String]) -> Result<RenderJocArgs, Box<dyn Error>> {
    let input = values.first().filter(|value| !value.starts_with('-'));
    let mut topology = None;
    let mut layout = None;
    let mut output = None;
    let mut binaural_sofa = None;
    let mut binaural_backend = joc_render::BinauralBackend::Direct;
    let mut binaural_backend_requested = false;
    let mut lfe_policy = None;
    let mut reference_f64 = false;
    let mut validation_profile = ValidationProfileRequest::Auto;
    let mut trim_configuration_count = None;
    let mut internal_base_policy = InternalBasePolicy::CurrentDefault;
    let mut no_progress = false;
    let mut performance_report = None;
    let mut diagnostic_contribution = SpatialContributionMode::Full;
    let mut overwrite = false;
    let mut index = 1;
    while index < values.len() {
        let flag = &values[index];
        if flag == "--reference-f64" {
            reference_f64 = true;
            index += 1;
            continue;
        }
        if flag == "--no-progress" {
            no_progress = true;
            index += 1;
            continue;
        }
        if flag == "--overwrite" {
            overwrite = true;
            index += 1;
            continue;
        }
        let value = values.get(index + 1).ok_or_else(usage_error)?;
        match flag.as_str() {
            "--topology" => topology = Some(PathBuf::from(value)),
            "--layout" => layout = Some(value.clone()),
            "-o" | "--output" => output = Some(PathBuf::from(value)),
            "--binaural-sofa" => binaural_sofa = Some(PathBuf::from(value)),
            "--backend" => {
                binaural_backend_requested = true;
                binaural_backend = match value.as_str() {
                    "direct" => joc_render::BinauralBackend::Direct,
                    "partitioned" => joc_render::BinauralBackend::Partitioned {
                        partition_size: 256,
                    },
                    _ => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "binaural backend must be direct or partitioned",
                        )
                        .into());
                    }
                };
            }
            "--partition-size" => {
                binaural_backend_requested = true;
                let partition_size = value.parse::<usize>()?;
                binaural_backend = joc_render::BinauralBackend::Partitioned { partition_size };
            }
            "--lfe-policy" => {
                lfe_policy = Some(match value.as_str() {
                    "exclude" => joc_render::BinauralLfePolicy::Exclude,
                    "equal-power-dual-mono" => joc_render::BinauralLfePolicy::EqualPowerDualMono,
                    _ => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "LFE policy must be exclude or equal-power-dual-mono",
                        )
                        .into());
                    }
                });
            }
            "--validation-profile" => validation_profile = parse_validation_profile(value)?,
            "--trim-config-count" => {
                trim_configuration_count = Some(parse_trim_configuration_count(value)?);
            }
            "--internal-base-policy" => internal_base_policy = parse_internal_base_policy(value)?,
            "--performance-report" => performance_report = Some(PathBuf::from(value)),
            "--diagnostic-contribution" => {
                diagnostic_contribution = match value.as_str() {
                    "full" => SpatialContributionMode::Full,
                    "base-only" => SpatialContributionMode::BaseOnly,
                    "reconstruction-only" => SpatialContributionMode::ReconstructionOnly,
                    _ => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "diagnostic contribution must be full, base-only, or reconstruction-only",
                        )
                        .into());
                    }
                };
            }
            _ => return Err(usage_error().into()),
        }
        index += 2;
    }
    Ok(RenderJocArgs {
        input: PathBuf::from(input.ok_or_else(usage_error)?),
        topology,
        layout: layout.ok_or_else(usage_error)?,
        output: output.ok_or_else(usage_error)?,
        binaural_sofa,
        binaural_backend,
        binaural_backend_requested,
        lfe_policy,
        validation_profile,
        trim_configuration_count,
        internal_base_policy,
        output_format: if reference_f64 {
            SampleFormat::F64
        } else {
            SampleFormat::F32
        },
        no_progress,
        performance_report,
        diagnostic_contribution,
        overwrite,
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

fn parse_validation_profile(value: &str) -> Result<ValidationProfileRequest, io::Error> {
    match value {
        "auto" | "AUTO" => Ok(ValidationProfileRequest::Auto),
        "etsi-strict" | "ETSI_STRICT" => Ok(ValidationProfileRequest::EtsiStrict),
        // Keep the former spelling as a cheap input-only migration alias. It
        // is never shown in help or emitted in diagnostics.
        "observed-vendor-compat"
        | "OBSERVED_VENDOR_COMPAT"
        | "dolby-vendor-compat"
        | "DOLBY_VENDOR_COMPAT" => Ok(ValidationProfileRequest::ObservedVendorCompat),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "unknown validation profile {value}; expected auto, etsi-strict, or observed-vendor-compat"
            ),
        )),
    }
}

fn decode_eac3(arguments: &DecodeEac3Args) -> Result<(), Box<dyn Error>> {
    ensure_output_directory_available(&arguments.output)?;
    if arguments.streaming {
        return decode_eac3_streaming(arguments);
    }
    let media = load_eac3(&arguments.input)?;
    let stream = &media.bytes;
    let config = PayloadDecoderConfig {
        reference_screen: None,
        oamd: OamdDecoderConfig::with_trim_configuration_count(arguments.trim_configuration_count),
    };
    let selected_profile =
        eac3_decode::resolve_profile_for_stream(stream, config, arguments.validation_profile)?;
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
            selected_profile,
            &dither,
            arguments.internal_base_policy,
            |frame_index, metadata, frame| {
                write_frame_debug(
                    &sink_output,
                    frame_index,
                    metadata,
                    frame,
                    arguments.validation_profile,
                )
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
        let downmix = if arguments.downmix.is_some() {
            read_input_wave(&base_paths.downmix)?
        } else {
            decode(&fs::read(&base_paths.downmix)?)?
        };
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
            selected_profile,
            |frame_index, metadata, frame| {
                write_frame_debug(
                    &sink_output,
                    frame_index,
                    metadata,
                    frame,
                    arguments.validation_profile,
                )
                .map_err(|error| eac3_decode::DecodeEac3Error::Sink(error.to_string()))
            },
        )?
    };
    write_scene(&arguments.output, &scene, arguments.output_format)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OverwriteDecision {
    Proceed,
    Cancelled,
    Refused,
}

fn planned_render_outputs(arguments: &RenderJocArgs) -> Vec<PathBuf> {
    let mut outputs = vec![arguments.output.clone()];
    if let Some(report) = &arguments.performance_report {
        outputs.push(report.clone());
    }
    outputs
}

fn normalize_path(path: &Path) -> io::Result<PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        env::current_dir()?.join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                let _ = normalized.pop();
            }
            _ => normalized.push(component.as_os_str()),
        }
    }
    Ok(normalized)
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

fn paths_alias(left: &Path, right: &Path) -> io::Result<bool> {
    if paths_equal(left, right) {
        return Ok(true);
    }
    if paths_equal(&normalize_path(left)?, &normalize_path(right)?) {
        return Ok(true);
    }
    match (
        canonicalize_path_or_parent(left),
        canonicalize_path_or_parent(right),
    ) {
        (Ok(left), Ok(right)) => Ok(paths_equal(&left, &right)),
        _ => Ok(false),
    }
}

fn canonicalize_path_or_parent(path: &Path) -> io::Result<PathBuf> {
    if path.exists() {
        return fs::canonicalize(path);
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no filename"))?;
    Ok(fs::canonicalize(parent)?.join(file_name))
}

fn overwrite_answer_is_affirmative(answer: &str) -> bool {
    matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}

fn should_prompt_for_overwrite(existing: &[PathBuf], promptable: bool) -> bool {
    promptable && !existing.is_empty()
}

fn decide_overwrite(
    existing: &[PathBuf],
    overwrite: bool,
    promptable: bool,
    answer: Option<&str>,
) -> OverwriteDecision {
    if existing.is_empty() || overwrite {
        return OverwriteDecision::Proceed;
    }
    if promptable && answer.is_some_and(overwrite_answer_is_affirmative) {
        OverwriteDecision::Proceed
    } else if promptable {
        OverwriteDecision::Cancelled
    } else {
        OverwriteDecision::Refused
    }
}

fn prompt_for_overwrite(existing: &[PathBuf]) -> io::Result<bool> {
    let mut stderr = io::stderr().lock();
    io::Write::write_all(&mut stderr, b"The following output files already exist:\n")?;
    for path in existing {
        writeln!(stderr, "  {}", path.display())?;
    }
    io::Write::write_all(&mut stderr, b"\nOverwrite? [y/N]: ")?;
    io::Write::flush(&mut stderr)?;
    drop(stderr);

    let mut answer = String::new();
    let bytes_read = io::stdin().lock().read_line(&mut answer)?;
    Ok(bytes_read > 0 && overwrite_answer_is_affirmative(&answer))
}

fn render_joc_preflight(
    arguments: &RenderJocArgs,
    terminal: TerminalCapabilities,
) -> Result<bool, Box<dyn Error>> {
    let outputs = planned_render_outputs(arguments);
    for (index, output) in outputs.iter().enumerate() {
        for other in outputs.iter().skip(index + 1) {
            if paths_alias(output, other)? {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "render-joc output paths alias each other: {} and {}",
                        output.display(),
                        other.display()
                    ),
                )
                .into());
            }
        }
        if paths_alias(&arguments.input, output)? {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "render-joc input path aliases output path {}; refusing to overwrite input",
                    output.display()
                ),
            )
            .into());
        }
    }
    // Validate semantic layout/output capability before checking overwrite
    // state or opening/decoding the input stream. In particular, a blocked
    // speaker-WAV mapping must never prompt about an existing target first.
    joc_render::validate_speaker_output(&arguments.layout, &arguments.output)?;
    if arguments.binaural_sofa.is_some() {
        joc_render::validate_binaural_layout(&arguments.layout)?;
    } else if arguments.lfe_policy.is_some() || arguments.binaural_backend_requested {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--backend, --partition-size, and --lfe-policy require --binaural-sofa",
        )
        .into());
    }
    joc_render::validate_output_path(&arguments.output)?;

    let existing = outputs
        .into_iter()
        .filter(|path| path.exists())
        .collect::<Vec<_>>();
    let promptable = terminal.stderr_is_tty && io::stdin().is_terminal();
    let decision = if arguments.overwrite {
        decide_overwrite(&existing, true, false, None)
    } else if should_prompt_for_overwrite(&existing, promptable) {
        let confirmed = prompt_for_overwrite(&existing)?;
        decide_overwrite(&existing, false, true, confirmed.then_some("yes"))
    } else {
        decide_overwrite(&existing, false, false, None)
    };
    match decision {
        OverwriteDecision::Proceed => Ok(arguments.overwrite || !existing.is_empty()),
        OverwriteDecision::Cancelled => Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "render cancelled; existing output files were not overwritten",
        )
        .into()),
        OverwriteDecision::Refused => {
            let paths = existing
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "refusing to overwrite existing render output(s): {paths}; rerun with --overwrite"
                ),
            )
            .into())
        }
    }
}

fn render_joc(
    arguments: &RenderJocArgs,
    terminal: TerminalCapabilities,
) -> Result<(), Box<dyn Error>> {
    let overwrite_authorized = render_joc_preflight(arguments, terminal)?;
    let mut performance = arguments
        .performance_report
        .as_ref()
        .map(|_| performance::RenderPerformance::new());
    let input_start = std::time::Instant::now();
    let media = load_eac3(&arguments.input)?;
    if let Some(report) = performance.as_mut() {
        report.input_container += input_start.elapsed();
    }
    let config = PayloadDecoderConfig {
        reference_screen: None,
        oamd: OamdDecoderConfig::with_trim_configuration_count(arguments.trim_configuration_count),
    };
    let profile_start = std::time::Instant::now();
    let selected_profile = eac3_decode::resolve_profile_for_stream(
        &media.bytes,
        config,
        arguments.validation_profile,
    )?;
    let progress_enabled = terminal.progress_is_tty() && !arguments.no_progress;
    let collect_timing = performance.is_some();
    let stream_timing = if progress_enabled || collect_timing {
        Some(eac3_decode::stream_timing(&media.bytes)?)
    } else {
        None
    };
    if let (Some(report), Some(stream_timing)) = (performance.as_mut(), stream_timing) {
        report.profile_validation += profile_start.elapsed();
        report.processed_access_units = stream_timing.access_units;
        report.total_audio_samples = stream_timing.samples;
        report.sample_rate_hz = Some(stream_timing.sample_rate);
    }
    let (total_access_units, total_samples, sample_rate) = stream_timing
        .map_or((0, 0, 0), |timing| {
            (timing.access_units, timing.samples, timing.sample_rate)
        });
    let mut progress = progress::ProgressReporter::new(
        progress_enabled,
        &arguments.layout,
        total_access_units,
        total_samples,
        sample_rate,
    );
    if let Some(sofa_path) = &arguments.binaural_sofa {
        let sofa = openjoc_sofa::load_simple_free_field_hrir(
            sofa_path,
            openjoc_sofa::SofaLoadLimits::default(),
        )
        .map_err(joc_render::JocRenderError::from)?;
        let control = arguments
            .topology
            .as_ref()
            .map(|path| joc_render::RenderControl::from_path(path))
            .transpose()?;
        let mut renderer = if arguments.diagnostic_contribution == SpatialContributionMode::Full {
            joc_render::JocBinauralRenderer::new(
                &arguments.layout,
                sofa.bank,
                arguments.binaural_backend,
                arguments.lfe_policy,
                control,
            )?
        } else {
            joc_render::JocBinauralRenderer::new_with_contribution(
                &arguments.layout,
                sofa.bank,
                arguments.binaural_backend,
                arguments.lfe_policy,
                control,
                arguments.diagnostic_contribution,
            )?
        };
        if performance.is_some() {
            renderer.enable_stage_timing();
        }
        let mut output = joc_render::JocPcmOutput::new_for_binaural(
            &arguments.output,
            arguments.output_format,
            overwrite_authorized,
        )?;
        let mut decode_timing = performance::DecodeStageTiming::new(performance.is_some());
        let mut render_timing = performance::RenderStageTiming::default();
        let dither = deterministic_dither_values();
        let result = eac3_decode::decode_internal_eac3_streaming_with_render_sink_and_policy(
            &media.bytes,
            config,
            selected_profile,
            &dither,
            arguments.internal_base_policy,
            |frame_index, metadata, frame, base| {
                let blocks = renderer
                    .render_frame_aligned(frame_index, frame, base)
                    .map_err(|error| eac3_decode::DecodeEac3Error::Sink(error.to_string()))?;
                for block in blocks {
                    let output_start = collect_timing.then(std::time::Instant::now);
                    output
                        .write_block(&joc_render::RenderedBlock {
                            sample_rate: block.sample_rate,
                            channels: vec![block.left, block.right],
                        })
                        .map_err(|error| eac3_decode::DecodeEac3Error::Sink(error.to_string()))?;
                    if let Some(output_start) = output_start {
                        render_timing.output_conversion_wav_write += output_start.elapsed();
                    }
                }
                let stage_timings = renderer.take_stage_timings();
                render_timing.bridge_control_assembly += stage_timings.bridge_control_assembly;
                render_timing.spatial_bridge_render += stage_timings.spatial_bridge_render;
                render_timing.binaural_render += stage_timings.binaural_render;
                render_timing.rendered_frames += 1;
                render_timing.rendered_samples += u64::from(base.samples);
                progress.update(frame_index, render_timing.rendered_samples);
                renderer
                    .record_profile(metadata)
                    .map_err(|error| eac3_decode::DecodeEac3Error::Sink(error.to_string()))
            },
            |_access_unit, _pcm| Ok(()),
            performance.as_ref().map(|_| &mut decode_timing),
        );
        match result {
            Ok((summary, reconstruction_tail)) => {
                let tail = match renderer.finish_with_reconstruction_tail(&reconstruction_tail) {
                    Ok(tail) => tail,
                    Err(error) => {
                        progress.finish();
                        output.abort();
                        return Err(error.into());
                    }
                };
                for block in tail {
                    let output_start = collect_timing.then(std::time::Instant::now);
                    if let Err(error) = output.write_block(&joc_render::RenderedBlock {
                        sample_rate: block.sample_rate,
                        channels: vec![block.left, block.right],
                    }) {
                        progress.finish();
                        output.abort();
                        return Err(error.into());
                    }
                    if let Some(output_start) = output_start {
                        render_timing.output_conversion_wav_write += output_start.elapsed();
                    }
                }
                let output_frames = output.frames();
                if let Err(error) = output.finish() {
                    progress.finish();
                    return Err(error.into());
                }
                progress.finish();
                if let Some(report) = performance.as_mut() {
                    report.merge_decode(decode_timing);
                    report.merge_render(&render_timing);
                    report.output_frames = output_frames;
                    report.output_bytes = fs::metadata(&arguments.output)?.len();
                    report.progress_enabled = progress.enabled();
                    report.progress_updates = progress.updates();
                    report.progress_overhead = progress.overhead();
                    performance::write_report(
                        arguments
                            .performance_report
                            .as_deref()
                            .expect("report path"),
                        report,
                        &arguments.layout,
                        selected_profile,
                        arguments.output_format,
                        overwrite_authorized,
                    )?;
                }
                println!(
                    "{}",
                    renderer.diagnostics(
                        sofa_path,
                        arguments.validation_profile,
                        selected_profile,
                        &summary,
                        &arguments.output,
                        arguments.output_format,
                    )
                );
                Ok(())
            }
            Err(error) => {
                progress.finish();
                output.abort();
                Err(error.into())
            }
        }
    } else if arguments.lfe_policy.is_some() || arguments.binaural_backend_requested {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--backend, --partition-size, and --lfe-policy require --binaural-sofa",
        )
        .into())
    } else {
        let mut renderer = if let Some(topology) = &arguments.topology {
            let control = joc_render::RenderControl::from_path(topology)?;
            if arguments.diagnostic_contribution == SpatialContributionMode::Full {
                joc_render::JocSpeakerRenderer::new(&arguments.layout, control)?
            } else {
                joc_render::JocSpeakerRenderer::new_with_contribution(
                    &arguments.layout,
                    control,
                    arguments.diagnostic_contribution,
                )?
            }
        } else if arguments.diagnostic_contribution == SpatialContributionMode::Full {
            joc_render::JocSpeakerRenderer::new_automatic(&arguments.layout)?
        } else {
            joc_render::JocSpeakerRenderer::new_automatic_with_contribution(
                &arguments.layout,
                arguments.diagnostic_contribution,
            )?
        };
        if performance.is_some() {
            renderer.enable_stage_timing();
        }
        let semantic_layout = renderer.semantic_channel_layout();
        let mut output = joc_render::JocPcmOutput::new_for_semantic_layout(
            &arguments.output,
            arguments.output_format,
            overwrite_authorized,
            &semantic_layout,
        )?;
        let output_container = output.container().name();
        let mut decode_timing = performance::DecodeStageTiming::new(performance.is_some());
        let mut render_timing = performance::RenderStageTiming::default();
        let dither = deterministic_dither_values();
        let result = eac3_decode::decode_internal_eac3_streaming_with_render_sink_and_policy(
            &media.bytes,
            config,
            selected_profile,
            &dither,
            arguments.internal_base_policy,
            |frame_index, metadata, frame, base| {
                let blocks = renderer
                    .render_frame_aligned(frame_index, frame, base)
                    .map_err(|error| eac3_decode::DecodeEac3Error::Sink(error.to_string()))?;
                for block in blocks {
                    let output_start = collect_timing.then(std::time::Instant::now);
                    output
                        .write_block(&block)
                        .map_err(|error| eac3_decode::DecodeEac3Error::Sink(error.to_string()))?;
                    if let Some(output_start) = output_start {
                        render_timing.output_conversion_wav_write += output_start.elapsed();
                    }
                }
                let stage_timings = renderer.take_stage_timings();
                render_timing.bridge_control_assembly += stage_timings.bridge_control_assembly;
                render_timing.spatial_bridge_render += stage_timings.spatial_bridge_render;
                render_timing.binaural_render += stage_timings.binaural_render;
                render_timing.rendered_frames += 1;
                render_timing.rendered_samples += u64::from(base.samples);
                progress.update(frame_index, render_timing.rendered_samples);
                renderer
                    .record_profile(metadata)
                    .map_err(|error| eac3_decode::DecodeEac3Error::Sink(error.to_string()))
            },
            |_access_unit, _pcm| Ok(()),
            performance.as_ref().map(|_| &mut decode_timing),
        );
        match result {
            Ok((summary, reconstruction_tail)) => {
                let tail = match renderer.finish_with_reconstruction_tail(&reconstruction_tail) {
                    Ok(tail) => tail,
                    Err(error) => {
                        progress.finish();
                        output.abort();
                        return Err(error.into());
                    }
                };
                for block in tail {
                    if let Err(error) = output.write_block(&block) {
                        progress.finish();
                        output.abort();
                        return Err(error.into());
                    }
                }
                let output_frames = output.frames();
                if let Err(error) = output.finish() {
                    progress.finish();
                    return Err(error.into());
                }
                progress.finish();
                if let Some(report) = performance.as_mut() {
                    report.merge_decode(decode_timing);
                    report.merge_render(&render_timing);
                    report.output_frames = output_frames;
                    report.output_bytes = fs::metadata(&arguments.output)?.len();
                    report.progress_enabled = progress.enabled();
                    report.progress_updates = progress.updates();
                    report.progress_overhead = progress.overhead();
                    performance::write_report(
                        arguments
                            .performance_report
                            .as_deref()
                            .expect("report path"),
                        report,
                        &arguments.layout,
                        selected_profile,
                        arguments.output_format,
                        overwrite_authorized,
                    )?;
                }
                println!(
                    "{}\noutput container: {}\noutput format: {}",
                    renderer.diagnostics(
                        &arguments.layout,
                        arguments.validation_profile,
                        selected_profile,
                        &summary,
                        &arguments.output,
                    ),
                    output_container,
                    match arguments.output_format {
                        SampleFormat::F32 => "IEEE float32",
                        SampleFormat::F64 => "IEEE float64",
                        SampleFormat::S24 => "signed PCM24",
                        SampleFormat::S16 => "signed PCM16",
                    }
                );
                Ok(())
            }
            Err(error) => {
                progress.finish();
                output.abort();
                Err(error.into())
            }
        }
    }
}

fn ensure_output_directory_available(output: &Path) -> Result<(), Box<dyn Error>> {
    if output.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "refusing to overwrite output directory {}",
                output.display()
            ),
        )
        .into());
    }
    Ok(())
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
    let input_kind = detect_media(&prefix[..prefix_len]);
    let reader: Box<dyn Read> = match input_kind {
        InputMediaKind::RawEac3 => Box::new(fs::File::open(&arguments.input)?),
        InputMediaKind::IsoBmff => Box::new(open_seekable_iso_bmff(
            &arguments.input,
            Path::new("ffprobe"),
            DEFAULT_MAX_EAC3_BYTES,
        )?),
        InputMediaKind::Unknown => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "--streaming requires raw E-AC-3 or seekable ISO BMFF input",
            )
            .into());
        }
    };
    let staging = create_streaming_stage(&arguments.output)?;
    let result = (|| -> Result<(), Box<dyn Error>> {
        let config = PayloadDecoderConfig {
            reference_screen: None,
            oamd: OamdDecoderConfig::with_trim_configuration_count(
                arguments.trim_configuration_count,
            ),
        };
        let sink_output = staging.clone();
        let component_export = RefCell::new(StreamingComponentExport::new(
            &staging,
            arguments.output_format,
        )?);
        let dither = deterministic_dither_values();
        let summary = eac3_decode::decode_internal_eac3_reader_with_base_sink_and_policy_request(
            reader,
            DEFAULT_MAX_EAC3_BYTES,
            config,
            arguments.validation_profile,
            &dither,
            arguments.internal_base_policy,
            |frame_index, metadata, frame| {
                component_export
                    .borrow_mut()
                    .write_frame(frame_index, metadata, frame, arguments.validation_profile)
                    .map_err(|error| eac3_decode::DecodeEac3Error::Sink(error.to_string()))
            },
            |access_unit, pcm| {
                component_export
                    .borrow_mut()
                    .write_base(access_unit, pcm)
                    .map_err(|error| eac3_decode::DecodeEac3Error::Sink(error.to_string()))
            },
        )?;
        component_export.into_inner().finish(&summary)?;
        write_streaming_summary(&sink_output, input_kind, &summary)?;
        Ok(())
    })();
    match result {
        Ok(()) => match fs::rename(&staging, &arguments.output) {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = fs::remove_dir_all(&staging);
                Err(error.into())
            }
        },
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            Err(error)
        }
    }
}

fn create_streaming_stage(output: &Path) -> Result<PathBuf, Box<dyn Error>> {
    if output.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("streaming output already exists: {}", output.display()),
        )
        .into());
    }
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let name = output
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "output path has no filename")
        })?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| io::Error::other("system clock is before UNIX epoch"))?
        .as_nanos();
    for attempt in 0..100_u32 {
        let candidate = parent.join(format!(
            ".{name}.partial-{}-{}",
            std::process::id(),
            stamp + u128::from(attempt)
        ));
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate unique streaming staging directory",
    )
    .into())
}

fn write_streaming_summary(
    output: &Path,
    input_kind: InputMediaKind,
    summary: &openjoc_scene::StreamingSceneSummary,
) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(output.join("debug"))?;
    let (source, input_delivery) = match input_kind {
        InputMediaKind::RawEac3 => (
            "OpenJOC raw E-AC-3 incremental AU consumer",
            "direct sequential raw E-AC-3",
        ),
        InputMediaKind::IsoBmff => (
            "OpenJOC seekable ISO BMFF E-AC-3 sample consumer",
            "seekable ordinary ISO BMFF packet cursor; one sample at a time",
        ),
        InputMediaKind::Unknown => unreachable!("unsupported input rejected before decode"),
    };
    let value = serde_json::json!({
        "schema": "openjoc.streaming-summary.v1",
        "source": source,
        "input_kind": media_kind_name(input_kind),
        "input_delivery": input_delivery,
        "sample_rate": summary.sample_rate,
        "duration_samples": summary.duration_samples,
        "frames": summary.frames,
        "object_count": summary.object_count,
        "max_reconstruction_rows": summary.max_reconstruction_rows,
        "max_frame_samples": summary.max_frame_samples,
        "metadata_events": summary.metadata_events,
        "trim_events": summary.trim_events,
        "retention": "streaming component export and summary only; no ObjectScene or full-duration scene capture",
        "semantic_binding_state": "unresolved",
        "authored_object_pcm_admissible": false,
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

struct StreamingComponentExport {
    output: PathBuf,
    sample_format: SampleFormat,
    rows: Option<Vec<WaveWriter<fs::File>>>,
    base: Option<StreamingBaseWriters>,
    frame_count: usize,
}

impl StreamingComponentExport {
    fn new(output: &Path, sample_format: SampleFormat) -> Result<Self, Box<dyn Error>> {
        fs::create_dir_all(output.join("debug"))?;
        Ok(Self {
            output: output.to_owned(),
            sample_format,
            rows: None,
            base: None,
            frame_count: 0,
        })
    }

    fn write_frame(
        &mut self,
        frame_index: usize,
        metadata: &openjoc_eac3::JocMetadataFrame,
        frame: &openjoc_scene::DecodedPayloadFrame,
        requested_profile: ValidationProfileRequest,
    ) -> Result<(), Box<dyn Error>> {
        if frame_index != self.frame_count {
            return Err(io::Error::other(format!(
                "streaming component frame sequence expected {}, received {}",
                self.frame_count, frame_index
            ))
            .into());
        }
        if self.rows.is_none() {
            let row_dir = self.output.join("diagnostics/reconstruction_rows");
            fs::create_dir_all(&row_dir)?;
            let mut writers = Vec::with_capacity(frame.decoded.reconstruction_basis.rows.len());
            for row_index in 0..frame.decoded.reconstruction_basis.rows.len() {
                let path = row_dir.join(format!("row_{row_index:03}.wav"));
                let writer = WaveWriter::new(
                    fs::File::create(path)?,
                    metadata.sample_rate,
                    1,
                    WaveEncodeOptions {
                        sample_format: self.sample_format,
                        clipping: Clipping::Reject,
                        dither: Dither::None,
                    },
                )?;
                writers.push(writer);
            }
            self.rows = Some(writers);
        }
        let rows = &mut self.rows.as_mut().expect("initialized above")[..];
        if rows.len() != frame.decoded.reconstruction_basis.rows.len() {
            return Err(io::Error::other(
                "reconstruction-basis row count changed during streaming decode",
            )
            .into());
        }
        for (writer, row) in rows
            .iter_mut()
            .zip(&frame.decoded.reconstruction_basis.rows)
        {
            writer.write_channels(&[row])?;
        }
        if frame_index < MAX_RETAINED_DEBUG_FRAMES {
            write_frame_debug(
                &self.output,
                frame_index,
                metadata,
                frame,
                requested_profile,
            )?;
        } else if frame_index == MAX_RETAINED_DEBUG_FRAMES {
            fs::write(
                self.output.join("debug/retention.json"),
                serde_json::to_vec_pretty(&serde_json::json!({
                    "schema": "openjoc.retention.v1",
                    "status": "DEBUG_FRAME_RETENTION_TRUNCATED",
                    "max_retained_frames": MAX_RETAINED_DEBUG_FRAMES,
                    "first_omitted_frame": frame_index,
                    "per_artifact_max_bytes": MAX_DEBUG_ARTIFACT_BYTES,
                    "overflow_classification": "bounded_debug_retention",
                }))?,
            )?;
        }
        self.frame_count = self.frame_count.saturating_add(1);
        Ok(())
    }

    fn write_base(
        &mut self,
        access_unit: usize,
        pcm: &DecodedAccessUnitPcm,
    ) -> Result<(), Box<dyn Error>> {
        if self.base.is_none() {
            self.base = Some(StreamingBaseWriters::new(
                &self.output,
                self.sample_format,
                pcm,
            )?);
        }
        self.base
            .as_mut()
            .expect("initialized above")
            .write(access_unit, pcm)
    }

    fn finish(
        mut self,
        summary: &openjoc_scene::StreamingSceneSummary,
    ) -> Result<(), Box<dyn Error>> {
        if self.frame_count != usize::try_from(summary.frames).unwrap_or(usize::MAX) {
            return Err(io::Error::other(
                "streaming summary frame count disagrees with component writer",
            )
            .into());
        }
        let row_count = self.rows.as_ref().map_or(0, Vec::len);
        if self.rows.as_ref().is_some_and(|rows| {
            rows.iter()
                .any(|writer| writer.frames() != summary.duration_samples)
        }) {
            return Err(io::Error::other(
                "streaming component row sample count disagrees with scene summary",
            )
            .into());
        }
        if let Some(rows) = self.rows.take() {
            for writer in rows {
                writer.finish()?;
            }
        }
        let base_inventory = self
            .base
            .take()
            .map(StreamingBaseWriters::finish)
            .transpose()?;
        let base_full_band = base_inventory
            .as_ref()
            .map_or_else(Vec::new, |inventory| inventory.full_order.clone());
        let base_lfe = base_inventory
            .as_ref()
            .and_then(|inventory| inventory.lfe.as_ref());
        let reconstruction_basis = (0..row_count)
            .map(|row_index| {
                serde_json::json!({
                    "component_role": "reconstruction_basis",
                    "row_index": row_index,
                    "pcm_artifact": format!("diagnostics/reconstruction_rows/row_{row_index:03}.wav"),
                    "semantic_binding": "unresolved",
                })
            })
            .collect::<Vec<_>>();
        fs::write(
            self.output.join("diagnostics/components.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema": "openjoc.components.v1",
                "base_full_band": base_full_band,
                "base_lfe": base_lfe.map(|_| serde_json::json!({
                    "component_role": "base_lfe",
                    "pcm_artifact": "debug/internal_base_lfe.wav",
                    "reconstruction_basis_member": false,
                    "semantic_binding": "unresolved",
                })),
                "reconstruction_basis": reconstruction_basis,
                "semantic_binding": "unresolved",
                "retention": "streaming; one decoded frame and one bounded PCM chunk at a time",
            }))?,
        )?;
        fs::write(
            self.output.join("diagnostics/retention.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema": "openjoc.retention.v1",
                "mode": "streaming",
                "max_buffered_output_chunks": MAX_STREAMING_OUTPUT_CHUNKS,
                "full_duration_pcm_retained": false,
                "codec_history_retained": true,
                "debug_frame_limit": MAX_RETAINED_DEBUG_FRAMES,
            }))?,
        )?;
        Ok(())
    }
}

struct StreamingBaseWriters {
    output: PathBuf,
    full: WaveWriter<fs::File>,
    joc_input: WaveWriter<fs::File>,
    lfe: Option<WaveWriter<fs::File>>,
    channel_locations: Vec<ChannelLocation>,
    lfe_location: Option<ChannelLocation>,
    full_order: Vec<String>,
    joc_order: Vec<String>,
    sample_rate: u32,
    access_units: usize,
    sample_count: u64,
}

#[derive(serde::Serialize)]
struct StreamingBaseInventory {
    sample_rate: u32,
    access_units: usize,
    sample_count: u64,
    full_order: Vec<String>,
    joc_order: Vec<String>,
    lfe: Option<String>,
}

impl StreamingBaseWriters {
    fn new(
        output: &Path,
        sample_format: SampleFormat,
        pcm: &DecodedAccessUnitPcm,
    ) -> Result<Self, Box<dyn Error>> {
        let debug = output.join("debug");
        fs::create_dir_all(&debug)?;
        let options = WaveEncodeOptions {
            sample_format,
            clipping: Clipping::Reject,
            dither: Dither::None,
        };
        let mut full_order = pcm
            .channel_locations
            .iter()
            .map(|location| location.label().to_owned())
            .collect::<Vec<_>>();
        if let Some(lfe_location) = pcm.lfe_location {
            let insertion = pcm
                .channel_locations
                .iter()
                .position(|location| *location == ChannelLocation::Centre)
                .map_or(full_order.len(), |index| index + 1);
            full_order.insert(insertion, lfe_location.label().to_owned());
        }
        let joc_order = pcm
            .channel_locations
            .iter()
            .map(|location| location.label().to_owned())
            .collect::<Vec<_>>();
        Ok(Self {
            output: output.to_owned(),
            full: WaveWriter::new(
                fs::File::create(debug.join("internal_base_full.wav"))?,
                pcm.sample_rate,
                full_order.len(),
                options,
            )?,
            joc_input: WaveWriter::new(
                fs::File::create(debug.join("internal_base_joc_input.wav"))?,
                pcm.sample_rate,
                pcm.channels.len(),
                options,
            )?,
            lfe: if pcm.lfe.is_some() {
                Some(WaveWriter::new(
                    fs::File::create(debug.join("internal_base_lfe.wav"))?,
                    pcm.sample_rate,
                    1,
                    options,
                )?)
            } else {
                None
            },
            channel_locations: pcm.channel_locations.clone(),
            lfe_location: pcm.lfe_location,
            full_order,
            joc_order,
            sample_rate: pcm.sample_rate,
            access_units: 0,
            sample_count: 0,
        })
    }

    fn write(
        &mut self,
        access_unit: usize,
        pcm: &DecodedAccessUnitPcm,
    ) -> Result<(), Box<dyn Error>> {
        if access_unit != self.access_units {
            return Err(io::Error::other(format!(
                "internal base access-unit sequence expected {}, received {}",
                self.access_units, access_unit
            ))
            .into());
        }
        if pcm.sample_rate != self.sample_rate
            || pcm.channel_locations != self.channel_locations
            || pcm.lfe_location != self.lfe_location
        {
            return Err(io::Error::other(
                "internal base channel topology changed during streaming decode",
            )
            .into());
        }
        let expected = usize::from(pcm.samples);
        if pcm.channels.iter().any(|channel| channel.len() != expected)
            || pcm
                .lfe
                .as_ref()
                .is_some_and(|channel| channel.len() != expected)
        {
            return Err(io::Error::other(
                "internal base frame length mismatch during streaming decode",
            )
            .into());
        }
        let mut full = pcm.channels.iter().map(Vec::as_slice).collect::<Vec<_>>();
        if let Some(lfe) = &pcm.lfe {
            let insertion = self
                .channel_locations
                .iter()
                .position(|location| *location == ChannelLocation::Centre)
                .map_or(full.len(), |index| index + 1);
            full.insert(insertion, lfe.as_slice());
        }
        let joc = pcm.channels.iter().map(Vec::as_slice).collect::<Vec<_>>();
        self.full.write_channels(&full)?;
        self.joc_input.write_channels(&joc)?;
        if let (Some(writer), Some(lfe)) = (&mut self.lfe, &pcm.lfe) {
            writer.write_channels(&[lfe.as_slice()])?;
        }
        self.access_units = self
            .access_units
            .checked_add(1)
            .ok_or_else(|| io::Error::other("internal base access-unit count overflow"))?;
        self.sample_count = self
            .sample_count
            .checked_add(u64::from(pcm.samples))
            .ok_or_else(|| io::Error::other("internal base sample count overflow"))?;
        Ok(())
    }

    fn finish(self) -> Result<StreamingBaseInventory, Box<dyn Error>> {
        self.full.finish()?;
        self.joc_input.finish()?;
        if let Some(writer) = self.lfe {
            writer.finish()?;
        }
        let inventory = StreamingBaseInventory {
            sample_rate: self.sample_rate,
            access_units: self.access_units,
            sample_count: self.sample_count,
            full_order: self.full_order,
            joc_order: self.joc_order,
            lfe: self
                .lfe_location
                .map(|location| location.label().to_owned()),
        };
        fs::write(
            self.output.join("debug/internal_base_inventory.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema": "openjoc.internal-base-inventory.v1",
                "source": "OpenJOC internal E-AC-3 decoder",
                "retention": "streaming; one access-unit PCM chunk at a time",
                "sample_rate": inventory.sample_rate,
                "access_units": inventory.access_units,
                "sample_count": inventory.sample_count,
                "full_base": {
                    "wav": "debug/internal_base_full.wav",
                    "channel_order": inventory.full_order.clone(),
                    "channel_count": inventory.full_order.len(),
                },
                "joc_input": {
                    "wav": "debug/internal_base_joc_input.wav",
                    "channel_order": inventory.joc_order.clone(),
                    "channel_count": inventory.joc_order.len(),
                    "lfe_excluded": true,
                },
                "lfe": {
                    "wav": inventory.lfe.as_ref().map(|_| "debug/internal_base_lfe.wav"),
                    "channel_order": inventory.lfe.clone(),
                    "present": inventory.lfe.is_some(),
                },
                "overlap_add_state": "stateful TDAC retained across access units; reset at stream start",
            }))?,
        )?;
        Ok(inventory)
    }
}

#[derive(Default)]
struct InternalBasePcm {
    base_policy: InternalBasePolicy,
    sample_rate: Option<u32>,
    channel_locations: Option<Vec<ChannelLocation>>,
    lfe_location: Option<ChannelLocation>,
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
        if pcm.channels.is_empty() || pcm.channel_locations.len() != pcm.channels.len() {
            return Err(format!(
                "internal base exposes {} full-band channels but {} channel locations",
                pcm.channels.len(),
                pcm.channel_locations.len()
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
        if let Some(expected) = self.sample_rate {
            if expected != pcm.sample_rate {
                return Err(format!(
                    "internal base sample rate changed from {} to {}",
                    expected, pcm.sample_rate
                ));
            }
        }
        if pcm.lfe.is_some() != pcm.lfe_location.is_some() {
            return Err("internal base LFE PCM/location presence mismatch".to_owned());
        }
        if let Some(lfe) = &pcm.lfe {
            if lfe.len() != frame_samples {
                return Err("internal base LFE frame length mismatch".to_owned());
            }
        }
        if self.access_units > 0 {
            if self.channel_locations.as_deref() != Some(pcm.channel_locations.as_slice()) {
                return Err(
                    "internal base channel topology changed between access units".to_owned(),
                );
            }
            if self.lfe_location != pcm.lfe_location {
                return Err("internal base LFE topology changed between access units".to_owned());
            }
        }

        let mut frame_full = pcm.channels.clone();
        if let Some(lfe) = &pcm.lfe {
            let insertion = pcm
                .channel_locations
                .iter()
                .position(|location| *location == ChannelLocation::Centre)
                .map_or(frame_full.len(), |index| index + 1);
            frame_full.insert(insertion, lfe.clone());
        }
        if !self.full.is_empty() && self.full.len() != frame_full.len() {
            return Err(
                "internal base full channel topology changed between access units".to_owned(),
            );
        }

        self.sample_rate.get_or_insert(pcm.sample_rate);
        if self.channel_locations.is_none() {
            self.channel_locations = Some(pcm.channel_locations.clone());
            self.lfe_location = pcm.lfe_location;
        }
        if self.joc_input.is_empty() {
            self.joc_input = vec![Vec::new(); pcm.channels.len()];
        }
        for (destination, source) in self.joc_input.iter_mut().zip(&pcm.channels) {
            destination.extend_from_slice(source);
        }
        match (&mut self.lfe, &pcm.lfe) {
            (None, None) => {}
            (None, Some(source)) => {
                self.lfe = Some(source.clone());
            }
            (Some(destination), Some(source)) => {
                destination.extend_from_slice(source);
            }
            (Some(_), None) => {
                return Err("internal base LFE presence changed between access units".to_owned());
            }
        }
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
        let retained_samples = self
            .full
            .iter()
            .chain(self.joc_input.iter())
            .map(Vec::len)
            .try_fold(0_usize, usize::checked_add)
            .and_then(|total| total.checked_add(self.lfe.as_ref().map_or(0, Vec::len)))
            .ok_or_else(|| io::Error::other("internal-base diagnostic sample count overflow"))?;
        ensure_retained_pcm_bytes(retained_samples, 8, "internal-base diagnostic")?;
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
        let joc_locations = self.channel_locations.as_deref().unwrap_or(&[]);
        let joc_order = joc_locations
            .iter()
            .map(|location| location.label())
            .collect::<Vec<_>>();
        let mut full_order = joc_order.clone();
        if let Some(lfe_location) = self.lfe_location {
            let insertion = joc_locations
                .iter()
                .position(|location| *location == ChannelLocation::Centre)
                .map_or(full_order.len(), |index| index + 1);
            full_order.insert(insertion, lfe_location.label());
        }
        let inventory = serde_json::json!({
            "schema": "openjoc.internal-base-inventory.v1",
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
                "channel_order": joc_order,
                "channel_count": self.joc_input.len(),
                "lfe_excluded": true,
            },
            "lfe": {
                "wav": self.lfe.as_ref().map(|_| "debug/internal_base_lfe.wav"),
                "channel_order": self.lfe_location.map(ChannelLocation::label),
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

fn read_input_wave(path: &Path) -> Result<WavePcm, Box<dyn Error>> {
    let bytes = fs::read(path)?;
    decode(&bytes).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("failed to decode input WAV {}: {error}", path.display()),
        )
        .into()
    })
}

fn parse_decode_payload(values: &[String]) -> Result<DecodePayloadArgs, Box<dyn Error>> {
    let mut downmix = None;
    let mut joc = None;
    let mut oamd = None;
    let mut output = None;
    let mut trim_count = None;
    let mut validation_profile = ValidationProfileRequest::Auto;
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
    let basis_samples = scene
        .reconstruction_basis
        .as_ref()
        .map_or(0, |basis| basis.rows.iter().map(Vec::len).sum());
    let lfe_samples = scene.base_lfe_pcm.as_ref().map_or(0, Vec::len);
    ensure_capture_retention_budget(basis_samples, lfe_samples, sample_format)?;
    let reconstruction_basis_json = scene.to_reconstruction_basis_json_pretty()?;
    if reconstruction_basis_json.len() > MAX_RECONSTRUCTION_BASIS_JSON_BYTES {
        return Err(io::Error::other(format!(
            "reconstruction-basis JSON is {} bytes, above the {}-byte retained diagnostic limit",
            reconstruction_basis_json.len(),
            MAX_RECONSTRUCTION_BASIS_JSON_BYTES,
        ))
        .into());
    }
    let rows = output.join("diagnostics/reconstruction_rows");
    let metadata = output.join("metadata");
    fs::create_dir_all(&rows)?;
    fs::create_dir_all(&metadata)?;
    fs::write(output.join("scene.json"), scene.to_manifest_json_pretty()?)?;
    let component_layout = scene.decoded_component_layout(vec![
        openjoc_scene::BaseFullBandChannel::FrontLeft,
        openjoc_scene::BaseFullBandChannel::FrontRight,
        openjoc_scene::BaseFullBandChannel::FrontCentre,
        openjoc_scene::BaseFullBandChannel::SideLeft,
        openjoc_scene::BaseFullBandChannel::SideRight,
    ]);
    fs::write(
        output.join("diagnostics/components.json"),
        serde_json::to_vec_pretty(&component_layout)?,
    )?;
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
        reconstruction_basis_json,
    )?;
    fs::write(
        output.join("diagnostics/retention.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema": "openjoc.retention.v1",
            "max_debug_log_bytes": MAX_DEBUG_ARTIFACT_BYTES,
            "max_diagnostic_records": MAX_RETAINED_DEBUG_FRAMES,
            "max_retained_pcm_bytes": MAX_RETAINED_CAPTURE_PCM_BYTES,
            "max_reconstruction_basis_json_bytes": MAX_RECONSTRUCTION_BASIS_JSON_BYTES,
            "streaming_or_ring_buffer_behavior": "frame debug stops after the retained frame limit; a truncation marker is written once",
            "truncation_marker": "DEBUG_FRAME_RETENTION_TRUNCATED",
            "overflow_classification": "output-failure before retained PCM/JSON is written",
        }))?,
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

fn ensure_capture_retention_budget(
    basis_samples: usize,
    lfe_samples: usize,
    sample_format: SampleFormat,
) -> io::Result<()> {
    let sample_width = match sample_format {
        SampleFormat::F32 => 4_u64,
        SampleFormat::F64 => 8_u64,
        SampleFormat::S24 => 3_u64,
        SampleFormat::S16 => 2_u64,
    };
    let retained_samples = u64::try_from(basis_samples)
        .and_then(|basis| u64::try_from(lfe_samples).map(|lfe| basis.saturating_add(lfe)))
        .map_err(|_| io::Error::other("capture diagnostic sample count overflow"))?;
    ensure_retained_pcm_bytes(
        usize::try_from(retained_samples)
            .map_err(|_| io::Error::other("capture diagnostic sample count overflow"))?,
        sample_width,
        "capture diagnostic",
    )?;
    let estimated_json_bytes = u64::try_from(basis_samples)
        .ok()
        .and_then(|samples| samples.checked_mul(ESTIMATED_JSON_BYTES_PER_SAMPLE))
        .ok_or_else(|| io::Error::other("reconstruction-basis JSON estimate overflow"))?;
    if estimated_json_bytes > u64::try_from(MAX_RECONSTRUCTION_BASIS_JSON_BYTES).unwrap() {
        return Err(io::Error::other(format!(
            "reconstruction-basis JSON estimate would be {estimated_json_bytes} bytes, above the {MAX_RECONSTRUCTION_BASIS_JSON_BYTES}-byte limit",
        )));
    }
    Ok(())
}

fn ensure_retained_pcm_bytes(
    samples: usize,
    bytes_per_sample: u64,
    artifact: &str,
) -> io::Result<()> {
    let pcm_bytes = u64::try_from(samples)
        .ok()
        .and_then(|count| count.checked_mul(bytes_per_sample))
        .ok_or_else(|| io::Error::other("capture diagnostic PCM byte count overflow"))?;
    if pcm_bytes > MAX_RETAINED_CAPTURE_PCM_BYTES {
        return Err(io::Error::other(format!(
            "{artifact} PCM would be {pcm_bytes} bytes, above the {MAX_RETAINED_CAPTURE_PCM_BYTES}-byte limit",
        )));
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
    requested_profile: ValidationProfileRequest,
    selected_profile: JocValidationProfile,
) -> Result<(), Box<dyn Error>> {
    let frame = output.join(format!("debug/frame_{frame_index:03}"));
    fs::create_dir_all(&frame)?;
    write_bounded_debug_text(&frame.join("joc.txt"), format!("{:#?}\n", decoded.joc))?;
    write_bounded_debug_text(&frame.join("oamd.txt"), format!("{:#?}\n", decoded.oamd))?;
    fs::write(
        frame.join("programme_layout.json"),
        serde_json::to_vec_pretty(&decoded.programme_layout)?,
    )?;
    let reconstruction = &decoded.decoded.reconstruction_basis;
    let samples_per_row = reconstruction.rows.iter().map(Vec::len).collect::<Vec<_>>();
    let reconstruction_summary = serde_json::json!({
        "retention": "bounded summary; full per-sample Debug trace suppressed",
        "row_count": reconstruction.rows.len(),
        "samples_per_row": samples_per_row,
        "state_reset": decoded.decoded.state_reset,
        "qmf_row_count": decoded.decoded.reconstruction_qmf.len(),
        "stage_count": decoded.decoded.stages.len(),
        "max_debug_artifact_bytes": MAX_DEBUG_ARTIFACT_BYTES,
    });
    write_bounded_debug_text(
        &frame.join("reconstruction.txt"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&reconstruction_summary)?
        ),
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
    let profile_selection = profile_selection_artifact(
        requested_profile,
        selected_profile,
        opaque_elements
            .iter()
            .map(|opaque| opaque.deviation_code.to_owned())
            .collect(),
    );
    let status = OamdPartialStatusArtifact {
        profile: selected_profile.as_str(),
        requested_profile: profile_selection.requested_profile,
        selected_profile: profile_selection.selected_profile,
        strict_status: profile_selection.strict_status,
        compatibility_deviations: profile_selection.compatibility_deviations.clone(),
        selection_reason: profile_selection.selection_reason,
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
        "profile: {}\nrequested_profile: {}\nselected_profile: {}\nstrict_status: {}\nselection_reason: {}\ncompatibility_deviations: {:?}\naccepted_with_deviation: {}\noamd_payload_structurally_accepted: {}\noamd_semantically_complete: {}\nobject_metadata_status: {}\ntrim_metadata_status: {}\ntrim_timeline_available: {}\nsemantic_object_audio_binding: {}\nsemantic_binding_state: {}\nmetadata_scene_available: {}\nreconstruction_rows_available: {}\nreconstruction_audio_status: {}\naudio_bound_objectscene_admissible: {}\nverified_authored_object_pcm_admissible: {}\nrenderer_fidelity_eligible: {}\n",
        status.profile,
        status.requested_profile,
        status.selected_profile,
        status.strict_status,
        status.selection_reason,
        status.compatibility_deviations,
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

fn write_frame_debug(
    output: &Path,
    frame_index: usize,
    metadata: &openjoc_eac3::JocMetadataFrame,
    decoded: &openjoc_scene::DecodedPayloadFrame,
    requested_profile: ValidationProfileRequest,
) -> Result<(), Box<dyn Error>> {
    if frame_index < MAX_RETAINED_DEBUG_FRAMES {
        return write_validation_debug(output, frame_index, metadata, requested_profile).and_then(
            |()| {
                write_debug(
                    output,
                    frame_index,
                    decoded,
                    requested_profile,
                    metadata.validation_profile,
                )
            },
        );
    }
    if frame_index == MAX_RETAINED_DEBUG_FRAMES {
        fs::create_dir_all(output.join("debug"))?;
        fs::write(
            output.join("debug/retention.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "status": "DEBUG_FRAME_RETENTION_TRUNCATED",
                "max_retained_frames": MAX_RETAINED_DEBUG_FRAMES,
                "first_omitted_frame": frame_index,
                "per_artifact_max_bytes": MAX_DEBUG_ARTIFACT_BYTES,
                "overflow_classification": "bounded_debug_retention",
            }))?,
        )?;
    }
    Ok(())
}

fn write_bounded_debug_text(path: &Path, value: String) -> io::Result<()> {
    if value.len() <= MAX_DEBUG_ARTIFACT_BYTES {
        return fs::write(path, value);
    }
    let mut cutoff = MAX_DEBUG_ARTIFACT_BYTES;
    while !value.is_char_boundary(cutoff) {
        cutoff -= 1;
    }
    let mut retained = value[..cutoff].to_owned();
    retained.push_str("\n[OpenJOC debug artifact truncated by retention policy]\n");
    fs::write(path, retained)
}

#[allow(clippy::struct_excessive_bools)]
#[derive(serde::Serialize)]
struct OamdPartialStatusArtifact {
    profile: &'static str,
    requested_profile: &'static str,
    selected_profile: &'static str,
    strict_status: &'static str,
    compatibility_deviations: Vec<String>,
    selection_reason: &'static str,
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
    profile_selection: ProfileSelectionArtifact,
}

#[derive(serde::Serialize)]
struct ProfileSelectionArtifact {
    requested_profile: &'static str,
    selected_profile: &'static str,
    strict_status: &'static str,
    compatibility_deviations: Vec<String>,
    selection_reason: &'static str,
}

fn profile_deviation_text(deviation: &JocProfileDeviation) -> String {
    format!(
        "payload {} {}={} expected_by_etsi={}",
        deviation.payload_id, deviation.field, deviation.actual, deviation.expected_by_etsi
    )
}

fn profile_selection_artifact(
    requested_profile: ValidationProfileRequest,
    selected_profile: JocValidationProfile,
    compatibility_deviations: Vec<String>,
) -> ProfileSelectionArtifact {
    let strict_status = match requested_profile {
        ValidationProfileRequest::Auto => {
            if selected_profile == JocValidationProfile::EtsiStrict {
                "passed"
            } else {
                "failed"
            }
        }
        ValidationProfileRequest::EtsiStrict => "passed",
        ValidationProfileRequest::ObservedVendorCompat => "not_evaluated",
    };
    let selection_reason = match requested_profile {
        ValidationProfileRequest::Auto if selected_profile == JocValidationProfile::EtsiStrict => {
            "AUTO selected ETSI_STRICT because strict validation passed"
        }
        ValidationProfileRequest::Auto => {
            "AUTO selected OBSERVED_VENDOR_COMPAT because strict failed and the existing compatibility whitelist accepted every deviation"
        }
        ValidationProfileRequest::EtsiStrict => "explicit ETSI_STRICT request",
        ValidationProfileRequest::ObservedVendorCompat => "explicit OBSERVED_VENDOR_COMPAT request",
    };
    ProfileSelectionArtifact {
        requested_profile: requested_profile.as_str(),
        selected_profile: selected_profile.as_str(),
        strict_status,
        compatibility_deviations,
        selection_reason,
    }
}

fn write_validation_debug(
    output: &Path,
    frame_index: usize,
    metadata: &openjoc_eac3::JocMetadataFrame,
    requested_profile: ValidationProfileRequest,
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
        profile_selection: profile_selection_artifact(
            requested_profile,
            metadata.validation_profile,
            metadata
                .deviations
                .iter()
                .map(profile_deviation_text)
                .collect(),
        ),
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
    write_bounded_debug_text(&frame.join("emdf.txt"), format!("{:#?}\n", metadata.emdf))?;
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CliErrorCategory {
    Usage,
    InvalidArgument,
    UnsupportedInput,
    MalformedInput,
    ProfileRejection,
    UnsupportedFeature,
    DecodeFailure,
    OutputFailure,
    IoFailure,
}

impl CliErrorCategory {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Usage => "usage",
            Self::InvalidArgument => "invalid-argument",
            Self::UnsupportedInput => "unsupported-input",
            Self::MalformedInput => "malformed-input",
            Self::ProfileRejection => "profile-rejection",
            Self::UnsupportedFeature => "unsupported-feature",
            Self::DecodeFailure => "decode-failure",
            Self::OutputFailure => "output-failure",
            Self::IoFailure => "io-failure",
        }
    }
}

fn classify_cli_error(error: &(dyn Error + 'static)) -> CliErrorCategory {
    if let Some(error) = error.downcast_ref::<eac3_decode::DecodeEac3Error>() {
        return classify_decode_error(error);
    }
    if let Some(error) = error.downcast_ref::<InputMediaError>() {
        return classify_input_error(error);
    }
    if let Some(error) = error.downcast_ref::<Eac3Error>() {
        return classify_eac3_error(error);
    }
    if let Some(error) = error.downcast_ref::<PayloadDecodeError>() {
        return classify_payload_error(error);
    }
    if let Some(error) = error.downcast_ref::<OamdError>() {
        return classify_oamd_error(error);
    }
    if let Some(error) = error.downcast_ref::<joc_render::JocRenderError>() {
        return classify_joc_render_error(error);
    }
    if error.downcast_ref::<WaveError>().is_some() {
        return CliErrorCategory::OutputFailure;
    }
    if let Some(error) = error.downcast_ref::<io::Error>() {
        if error.to_string() == USAGE {
            return CliErrorCategory::Usage;
        }
        return match error.kind() {
            io::ErrorKind::InvalidInput => CliErrorCategory::InvalidArgument,
            io::ErrorKind::UnexpectedEof | io::ErrorKind::InvalidData => {
                CliErrorCategory::MalformedInput
            }
            _ => CliErrorCategory::IoFailure,
        };
    }
    CliErrorCategory::DecodeFailure
}

const fn classify_joc_render_error(error: &joc_render::JocRenderError) -> CliErrorCategory {
    match error {
        joc_render::JocRenderError::Io(_) => CliErrorCategory::IoFailure,
        joc_render::JocRenderError::Json(_) => CliErrorCategory::MalformedInput,
        joc_render::JocRenderError::OutputExists(_)
        | joc_render::JocRenderError::Wave(_)
        | joc_render::JocRenderError::Caf(_)
        | joc_render::JocRenderError::WavLayoutNotExactlyRepresentable { .. }
        | joc_render::JocRenderError::UnsupportedCafSpeaker { .. }
        | joc_render::JocRenderError::NoRenderedFrames
        | joc_render::JocRenderError::BinauralOutput(_) => CliErrorCategory::OutputFailure,
        joc_render::JocRenderError::UnsupportedOutputExtension(_) => {
            CliErrorCategory::InvalidArgument
        }
        joc_render::JocRenderError::InvalidControl(_)
        | joc_render::JocRenderError::UnsupportedLayout(_)
        | joc_render::JocRenderError::EmptyTopology
        | joc_render::JocRenderError::TopologyCoordinateCount { .. }
        | joc_render::JocRenderError::BaseTopologyChanged
        | joc_render::JocRenderError::BaseCoordinate(_)
        | joc_render::JocRenderError::BridgeControl(_)
        | joc_render::JocRenderError::UnusedUpdate { .. }
        | joc_render::JocRenderError::Sofa(_)
        | joc_render::JocRenderError::BinauralHrirCoverage { .. }
        | joc_render::JocRenderError::BinauralLayoutNotReady { .. }
        | joc_render::JocRenderError::BinauralLfePolicyRequired { .. } => {
            CliErrorCategory::UnsupportedFeature
        }
        joc_render::JocRenderError::FrameIndex { .. }
        | joc_render::JocRenderError::SampleTimeline { .. }
        | joc_render::JocRenderError::SampleRateMismatch { .. }
        | joc_render::JocRenderError::FrameSampleCount
        | joc_render::JocRenderError::ProfileChanged
        | joc_render::JocRenderError::Bridge(_)
        | joc_render::JocRenderError::Spatial(_)
        | joc_render::JocRenderError::Timeline(_)
        | joc_render::JocRenderError::Binaural(_)
        | joc_render::JocRenderError::BinauralSampleRateMismatch { .. } => {
            CliErrorCategory::DecodeFailure
        }
    }
}

const fn classify_input_error(error: &InputMediaError) -> CliErrorCategory {
    match error {
        InputMediaError::UnsupportedSignature
        | InputMediaError::MissingAudioTrack
        | InputMediaError::MultipleAudioTracks { .. }
        | InputMediaError::NoMatchingAudioTrack { .. } => CliErrorCategory::UnsupportedInput,
        InputMediaError::EmptyInput
        | InputMediaError::TruncatedRawEac3 { .. }
        | InputMediaError::InvalidDemuxedEac3(_)
        | InputMediaError::ProbeFailed { .. }
        | InputMediaError::MalformedProbeRow { .. }
        | InputMediaError::DemuxFailed { .. }
        | InputMediaError::MalformedPacketProbeRow { .. }
        | InputMediaError::EmptyDemuxOutput => CliErrorCategory::MalformedInput,
        InputMediaError::DemuxOutputTooLarge { .. } => CliErrorCategory::UnsupportedFeature,
        InputMediaError::Io { .. } => CliErrorCategory::IoFailure,
    }
}

const fn classify_eac3_error(error: &Eac3Error) -> CliErrorCategory {
    match error {
        Eac3Error::JocProfileValidation(_) => CliErrorCategory::ProfileRejection,
        Eac3Error::TruncatedFrame { .. } | Eac3Error::Bit(_) => CliErrorCategory::MalformedInput,
        Eac3Error::UnsupportedJocAccessUnitFrameCount { .. }
        | Eac3Error::UnsupportedJocAudioBlockCount { .. }
        | Eac3Error::UnsupportedJocChannelTopology { .. }
        | Eac3Error::UnsupportedAdaptiveHybridTransform => CliErrorCategory::UnsupportedFeature,
        _ => CliErrorCategory::DecodeFailure,
    }
}

const fn classify_oamd_error(error: &OamdError) -> CliErrorCategory {
    match error {
        OamdError::ReservedIntermediateSpatialFormat { .. }
        | OamdError::ReservedSampleOffsetCode
        | OamdError::ReservedSizeIndex
        | OamdError::ReservedZoneIndex { .. }
        | OamdError::ReservedAlternateObjectData { .. }
        | OamdError::ReservedWarpMode { .. }
        | OamdError::ReservedGlobalTrimMode
        | OamdError::ReservedTrimCode { .. }
        | OamdError::ReservedObjectDivergenceMode
        | OamdError::ReservedObjectDivergenceCode => CliErrorCategory::ProfileRejection,
        OamdError::UnsupportedKnownElement { .. } | OamdError::VendorProfilePayloadId { .. } => {
            CliErrorCategory::UnsupportedFeature
        }
        OamdError::MissingTrimConfigurationCount => CliErrorCategory::InvalidArgument,
        OamdError::Bit(_) => CliErrorCategory::MalformedInput,
        _ => CliErrorCategory::DecodeFailure,
    }
}

const fn classify_payload_error(error: &PayloadDecodeError) -> CliErrorCategory {
    match error {
        PayloadDecodeError::Oamd(error) => classify_oamd_error(error),
        PayloadDecodeError::EmptyStream => CliErrorCategory::MalformedInput,
        PayloadDecodeError::ProgrammeLayout(_) => CliErrorCategory::UnsupportedFeature,
        PayloadDecodeError::Joc(_)
        | PayloadDecodeError::Scene(_)
        | PayloadDecodeError::UnexpectedFrameIndex { .. }
        | PayloadDecodeError::SampleRateChanged { .. }
        | PayloadDecodeError::FrameIndexOverflow
        | PayloadDecodeError::SampleRangeOverflow => CliErrorCategory::DecodeFailure,
    }
}

const fn classify_decode_error(error: &eac3_decode::DecodeEac3Error) -> CliErrorCategory {
    match error {
        eac3_decode::DecodeEac3Error::Input(error) => classify_input_error(error),
        eac3_decode::DecodeEac3Error::Eac3(error) => classify_eac3_error(error),
        eac3_decode::DecodeEac3Error::Oamd(error) => classify_oamd_error(error),
        eac3_decode::DecodeEac3Error::Payload(error) => classify_payload_error(error),
        eac3_decode::DecodeEac3Error::EmptyStream => CliErrorCategory::MalformedInput,
        eac3_decode::DecodeEac3Error::MissingMetadata { .. }
        | eac3_decode::DecodeEac3Error::JocExtensionWithoutMetadata { .. } => {
            CliErrorCategory::UnsupportedFeature
        }
        eac3_decode::DecodeEac3Error::Sink(_) => CliErrorCategory::OutputFailure,
        eac3_decode::DecodeEac3Error::SampleCountOverflow
        | eac3_decode::DecodeEac3Error::InvalidPcmLength { .. }
        | eac3_decode::DecodeEac3Error::SampleRateMismatch { .. }
        | eac3_decode::DecodeEac3Error::FrameIndexOverflow => CliErrorCategory::DecodeFailure,
    }
}

fn usage_error() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, USAGE)
}

#[cfg(test)]
mod profile_name_tests {
    use super::{
        OverwriteDecision, SpatialContributionMode, TerminalCapabilities, ValidationProfileRequest,
        decide_overwrite, overwrite_answer_is_affirmative, parse_render_joc,
        parse_validation_profile, planned_render_outputs, render_joc_preflight,
        should_prompt_for_overwrite,
    };
    use crate::joc_render;
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn render_joc_omitted_profile_matches_explicit_auto() {
        let omitted = [
            "input.m4a".to_owned(),
            "--layout".to_owned(),
            "7.1.4".to_owned(),
            "--output".to_owned(),
            "output.wav".to_owned(),
        ];
        let explicit = [
            "input.m4a".to_owned(),
            "--layout".to_owned(),
            "7.1.4".to_owned(),
            "--output".to_owned(),
            "output.wav".to_owned(),
            "--validation-profile".to_owned(),
            "auto".to_owned(),
        ];
        let omitted_profile = parse_render_joc(&omitted)
            .expect("omitted profile")
            .validation_profile;
        let explicit_profile = parse_render_joc(&explicit)
            .expect("explicit auto profile")
            .validation_profile;
        assert_eq!(omitted_profile, ValidationProfileRequest::Auto);
        assert_eq!(omitted_profile, explicit_profile);
    }

    #[test]
    fn legacy_cli_profile_name_is_input_only_alias() {
        assert_eq!(
            parse_validation_profile("dolby-vendor-compat").expect("legacy alias"),
            ValidationProfileRequest::ObservedVendorCompat
        );
        assert_eq!(
            parse_validation_profile("observed-vendor-compat").expect("canonical name"),
            ValidationProfileRequest::ObservedVendorCompat
        );
    }

    #[test]
    fn render_joc_binaural_options_are_explicit_and_direct_is_the_default() {
        let base = [
            "input.m4a".to_owned(),
            "--layout".to_owned(),
            "7.1.4".to_owned(),
            "--output".to_owned(),
            "output.wav".to_owned(),
        ];
        let parsed = parse_render_joc(&base).expect("base render-joc options");
        assert_eq!(parsed.binaural_backend, joc_render::BinauralBackend::Direct);
        assert!(parsed.binaural_sofa.is_none());
        let mut binaural = base.to_vec();
        binaural.extend([
            "--binaural-sofa".to_owned(),
            "HRTF.sofa".to_owned(),
            "--backend".to_owned(),
            "partitioned".to_owned(),
            "--partition-size".to_owned(),
            "128".to_owned(),
            "--lfe-policy".to_owned(),
            "equal-power-dual-mono".to_owned(),
        ]);
        let parsed = parse_render_joc(&binaural).expect("binaural render-joc options");
        assert_eq!(parsed.binaural_sofa, Some(PathBuf::from("HRTF.sofa")));
        assert_eq!(
            parsed.binaural_backend,
            joc_render::BinauralBackend::Partitioned {
                partition_size: 128
            }
        );
        assert_eq!(
            parsed.lfe_policy,
            Some(joc_render::BinauralLfePolicy::EqualPowerDualMono)
        );
    }

    #[test]
    fn render_joc_performance_and_progress_options_are_diagnostic() {
        let values = [
            "input.m4a".to_owned(),
            "--layout".to_owned(),
            "7.1.4".to_owned(),
            "--output".to_owned(),
            "output.wav".to_owned(),
            "--no-progress".to_owned(),
            "--performance-report".to_owned(),
            "report.json".to_owned(),
        ];
        let parsed = parse_render_joc(&values).expect("diagnostic render options");
        assert!(parsed.no_progress);
        assert_eq!(
            parsed.performance_report,
            Some(PathBuf::from("report.json"))
        );
    }

    #[test]
    fn render_joc_contribution_diagnostic_is_typed_and_full_by_default() {
        let base = [
            "input.m4a".to_owned(),
            "--layout".to_owned(),
            "7.1.4".to_owned(),
            "--output".to_owned(),
            "output.wav".to_owned(),
        ];
        assert_eq!(
            parse_render_joc(&base)
                .expect("default contribution")
                .diagnostic_contribution,
            SpatialContributionMode::Full
        );
        for (value, expected) in [
            ("full", SpatialContributionMode::Full),
            ("base-only", SpatialContributionMode::BaseOnly),
            (
                "reconstruction-only",
                SpatialContributionMode::ReconstructionOnly,
            ),
        ] {
            let mut values = base.to_vec();
            values.extend(["--diagnostic-contribution".to_owned(), value.to_owned()]);
            assert_eq!(
                parse_render_joc(&values)
                    .expect("valid contribution")
                    .diagnostic_contribution,
                expected
            );
        }
        let mut invalid = base.to_vec();
        invalid.extend(["--diagnostic-contribution".to_owned(), "rb-only".to_owned()]);
        let error = parse_render_joc(&invalid)
            .err()
            .expect("invalid contribution");
        assert!(
            error
                .to_string()
                .contains("full, base-only, or reconstruction-only")
        );
    }

    #[test]
    fn render_joc_overwrite_is_explicit_and_plans_both_files() {
        let values = [
            "input.m4a".to_owned(),
            "--layout".to_owned(),
            "7.1.4".to_owned(),
            "--output".to_owned(),
            "output.wav".to_owned(),
            "--performance-report".to_owned(),
            "report.json".to_owned(),
            "--overwrite".to_owned(),
        ];
        let parsed = parse_render_joc(&values).expect("overwrite render options");
        assert!(parsed.overwrite);
        assert_eq!(
            planned_render_outputs(&parsed),
            vec![PathBuf::from("output.wav"), PathBuf::from("report.json")]
        );
    }

    #[test]
    fn seven_one_six_wav_preflight_rejects_before_overwrite_prompt_or_input_decode() {
        let root = std::env::temp_dir().join(format!(
            "openjoc-716-preflight-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let input = root.join("missing.m4a");
        let output = root.join("existing.wav");
        fs::write(&output, b"previous-valid-output").unwrap();
        let values = [
            input.to_string_lossy().into_owned(),
            "--layout".to_owned(),
            "7.1.6".to_owned(),
            "--output".to_owned(),
            output.to_string_lossy().into_owned(),
        ];
        let parsed = parse_render_joc(&values).unwrap();
        let terminal = TerminalCapabilities::from_inputs(false, true, None, false, None, None);
        let error = render_joc_preflight(&parsed, terminal).expect_err("WAV must be blocked");
        assert!(error.to_string().contains("7.1.6"));
        assert!(
            error
                .to_string()
                .contains("no channel identities were substituted")
        );
        assert!(!error.to_string().contains("overwrite"));
        assert_eq!(fs::read(&output).unwrap(), b"previous-valid-output");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn seven_one_six_binaural_preflight_is_independent_of_caf_acceptance() {
        let values = [
            "missing.m4a".to_owned(),
            "--layout".to_owned(),
            "7.1.6".to_owned(),
            "--binaural-sofa".to_owned(),
            "missing.sofa".to_owned(),
            "--output".to_owned(),
            "output.caf".to_owned(),
        ];
        let parsed = parse_render_joc(&values).unwrap();
        let terminal = TerminalCapabilities::from_inputs(false, false, None, false, None, None);
        let error = render_joc_preflight(&parsed, terminal).expect_err("binaural must be blocked");
        assert!(error.to_string().contains("not currently admitted"));
        assert!(error.to_string().contains("Ltm"));
        assert!(!error.to_string().contains("failed to open input"));
    }

    #[test]
    fn nine_one_family_wav_preflight_rejects_before_overwrite_or_decode() {
        for layout in ["9.1", "9.1.2", "9.1.4", "9.1.6"] {
            let root = std::env::temp_dir().join(format!(
                "openjoc-{layout}-preflight-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(&root).unwrap();
            let input = root.join("missing.m4a");
            let output = root.join("existing.wav");
            fs::write(&output, b"previous-valid-output").unwrap();
            let values = [
                input.to_string_lossy().into_owned(),
                "--layout".to_owned(),
                layout.to_owned(),
                "--output".to_owned(),
                output.to_string_lossy().into_owned(),
            ];
            let parsed = parse_render_joc(&values).unwrap();
            let terminal = TerminalCapabilities::from_inputs(false, true, None, false, None, None);
            let error = render_joc_preflight(&parsed, terminal).expect_err("WAV must be blocked");
            assert!(error.to_string().contains(layout));
            assert!(error.to_string().contains("use .caf"));
            assert!(!error.to_string().contains("overwrite"));
            assert_eq!(fs::read(&output).unwrap(), b"previous-valid-output");
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn overwrite_prompt_accepts_only_y_or_yes_case_insensitively() {
        for answer in ["y", "Y", "yes", "YES", "Yes"] {
            assert!(overwrite_answer_is_affirmative(answer), "{answer}");
        }
        for answer in ["n", "no", "", "maybe", " yes later "] {
            assert!(!overwrite_answer_is_affirmative(answer), "{answer}");
        }
    }

    #[test]
    fn overwrite_prompt_requires_existing_output_paths() {
        let existing = vec![PathBuf::from("output.wav")];
        assert!(!should_prompt_for_overwrite(&[], true));
        assert!(should_prompt_for_overwrite(&existing, true));
        assert!(!should_prompt_for_overwrite(&existing, false));
    }

    #[test]
    fn overwrite_decision_is_single_and_safe_by_default() {
        let existing = vec![PathBuf::from("output.wav"), PathBuf::from("report.json")];
        assert_eq!(
            decide_overwrite(&[], false, false, None),
            OverwriteDecision::Proceed
        );
        assert_eq!(
            decide_overwrite(&existing, false, true, Some("yes")),
            OverwriteDecision::Proceed
        );
        assert_eq!(
            decide_overwrite(&existing, false, true, Some("n")),
            OverwriteDecision::Cancelled
        );
        assert_eq!(
            decide_overwrite(&existing, false, true, Some("")),
            OverwriteDecision::Cancelled
        );
        assert_eq!(
            decide_overwrite(&existing, false, true, None),
            OverwriteDecision::Cancelled
        );
        assert_eq!(
            decide_overwrite(&existing, false, false, None),
            OverwriteDecision::Refused
        );
        assert_eq!(
            decide_overwrite(&existing, true, false, None),
            OverwriteDecision::Proceed
        );
    }

    #[test]
    fn input_output_alias_is_rejected_before_render_even_with_overwrite() {
        let input = std::env::temp_dir().join(format!(
            "openjoc-overwrite-alias-{}-{}.ec3",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        std::fs::write(&input, b"input").expect("input");
        let values = [
            input.to_string_lossy().into_owned(),
            "--layout".to_owned(),
            "7.1.4".to_owned(),
            "--output".to_owned(),
            input.to_string_lossy().into_owned(),
            "--overwrite".to_owned(),
        ];
        let parsed = parse_render_joc(&values).expect("alias options");
        let terminal = TerminalCapabilities::from_inputs(false, false, None, false, None, None);
        let error = render_joc_preflight(&parsed, terminal).expect_err("input alias");
        assert!(error.to_string().contains("aliases output path"));
        assert_eq!(std::fs::read(&input).expect("input remains"), b"input");
        std::fs::remove_file(input).expect("cleanup");
    }
}

#[cfg(test)]
mod retention_tests {
    use super::{
        ESTIMATED_JSON_BYTES_PER_SAMPLE, MAX_RECONSTRUCTION_BASIS_JSON_BYTES,
        MAX_RETAINED_DEBUG_FRAMES, MAX_STREAMING_OUTPUT_CHUNKS, SampleFormat,
        ensure_capture_retention_budget,
    };

    #[test]
    fn capture_retention_fails_closed_before_large_diagnostic_serialization() {
        let maximum_samples = MAX_RECONSTRUCTION_BASIS_JSON_BYTES
            / usize::try_from(ESTIMATED_JSON_BYTES_PER_SAMPLE).unwrap();
        assert!(ensure_capture_retention_budget(maximum_samples, 0, SampleFormat::F32).is_ok());
        assert!(
            ensure_capture_retention_budget(maximum_samples + 1, 0, SampleFormat::F32)
                .unwrap_err()
                .to_string()
                .contains("JSON estimate")
        );
    }

    #[test]
    fn debug_frame_retention_has_an_explicit_first_omitted_frame() {
        assert_eq!(MAX_RETAINED_DEBUG_FRAMES, 64);
    }

    #[test]
    fn streaming_retention_bound_does_not_scale_with_simulated_duration() {
        let mut max_buffered = 0_usize;
        for _ in 0..100_000 {
            let current_chunk = 1_usize;
            max_buffered = max_buffered.max(current_chunk);
            assert_eq!(current_chunk, MAX_STREAMING_OUTPUT_CHUNKS);
        }
        assert_eq!(max_buffered, MAX_STREAMING_OUTPUT_CHUNKS);
    }
}

#[cfg(test)]
mod internal_base_tests {
    use super::InternalBasePcm;
    use openjoc_eac3::{ChannelLocation, DecodedAccessUnitPcm};

    fn frame(value: f64, lfe: Option<f64>) -> DecodedAccessUnitPcm {
        DecodedAccessUnitPcm {
            sample_rate: 48_000,
            samples: 2,
            channel_locations: vec![
                ChannelLocation::Left,
                ChannelLocation::Right,
                ChannelLocation::Centre,
                ChannelLocation::LeftSurround,
                ChannelLocation::RightSurround,
            ],
            channels: (0..5)
                .map(|channel| vec![value + channel as f64, value + channel as f64 + 0.5])
                .collect(),
            lfe_location: lfe.map(|_| ChannelLocation::Lfe(0)),
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
        assert!(error.contains("5 channel locations"));
    }

    #[test]
    fn captures_seven_channel_dependent_topology_without_losing_labels() {
        let mut capture = InternalBasePcm::default();
        let mut seven = frame(1.0, Some(10.0));
        seven
            .channel_locations
            .extend([ChannelLocation::LeftBack, ChannelLocation::RightBack]);
        seven.channels.extend([vec![20.0, 20.5], vec![21.0, 21.5]]);
        capture.append(0, &seven).expect("seven-channel topology");

        assert_eq!(capture.joc_input.len(), 7);
        assert_eq!(capture.full.len(), 8);
        assert_eq!(capture.channel_locations, Some(seven.channel_locations));
        assert_eq!(capture.lfe_location, Some(ChannelLocation::Lfe(0)));
        assert_eq!(capture.full[3], vec![10.0, 10.5]);
        assert_eq!(capture.full[6], vec![20.0, 20.5]);
        assert_eq!(capture.full[7], vec![21.0, 21.5]);
    }

    #[test]
    fn rejects_topology_changes_before_mutating_accumulated_pcm() {
        let mut capture = InternalBasePcm::default();
        capture.append(0, &frame(1.0, None)).expect("first frame");
        let before = capture.joc_input.clone();
        let mut changed = frame(2.0, None);
        changed.channel_locations[4] = ChannelLocation::RightBack;

        assert!(capture.append(1, &changed).is_err());
        assert_eq!(capture.joc_input, before);
        assert_eq!(capture.access_units, 1);
    }
}
