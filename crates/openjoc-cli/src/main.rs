// pattern: Imperative Shell

mod banner;
mod eac3_decode;
mod fixture_census;
mod terminal;

use banner::{package_metadata, render_banner};
use openjoc_container::{InputMediaKind, load_eac3};
use openjoc_eac3::extract_joc_addbsi_access_unit;
use openjoc_oamd::{OamdDecoderConfig, Position3, ReferenceScreen};
use openjoc_scene::{JocFrameInput, PayloadDecoder, PayloadDecoderConfig};
use openjoc_wave::{Clipping, Dither, SampleFormat, WaveEncodeOptions, decode, encode_channels};
use std::{
    env,
    error::Error,
    fmt::Write as _,
    fs, io,
    num::NonZeroU8,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};
use terminal::TerminalCapabilities;

const USAGE: &str = "usage: openjoc inspect FILE\n       openjoc decode FILE -o DIR [--downmix FILE | --internal-base] [--reference-f64]\n       openjoc census [MANIFEST] -o DIR\n       openjoc decode-payload --downmix FILE --joc FILE --oamd FILE -o DIR [--reference-f64] [--trim-config-count N] [--screen-origin-x X --screen-origin-y Y --screen-origin-z Z --screen-width W --screen-height H]";

struct DecodePayloadArgs {
    downmix: PathBuf,
    joc: PathBuf,
    oamd: PathBuf,
    output: PathBuf,
    trim_count: Option<NonZeroU8>,
    reference_screen: Option<ReferenceScreen>,
    output_format: SampleFormat,
}

struct DecodeEac3Args {
    input: PathBuf,
    downmix: Option<PathBuf>,
    internal_base: bool,
    output: PathBuf,
    output_format: SampleFormat,
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
            let input = arguments.get(1).ok_or_else(usage_error)?;
            if arguments.len() != 2 {
                return Err(usage_error().into());
            }
            inspect(Path::new(&input))
        }
        Some("decode-payload") => decode_payload(&arguments[1..]),
        Some("decode") => decode_eac3(&parse_decode_eac3(&arguments[1..])?),
        Some("census") => run_census(&arguments[1..]),
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
        "  openjoc inspect <FILE>\n",
        "  openjoc decode <FILE> -o <DIR> [--reference-f64]\n",
        "  openjoc census [MANIFEST] -o <DIR>\n",
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
        "  openjoc inspect <FILE>\n",
        "  openjoc decode <FILE> -o <DIR> [--downmix <FILE> | --internal-base] [--reference-f64]\n",
        "  openjoc census [MANIFEST] -o <DIR>\n",
        "  openjoc decode-payload --downmix <FILE> --joc <FILE> --oamd <FILE>\n",
        "                         -o <DIR> [OPTIONS]\n",
        "\n",
    ));
    append_heading(output, "COMMANDS", color)?;
    output.push_str(concat!(
        "  inspect         Inspect E-AC-3 access units and JOC metadata\n",
        "  decode          Decode an E-AC-3 JOC stream into an object scene\n",
        "  census          Census bounded metadata carriers from external fixtures\n",
        "  decode-payload  Decode supplied downmix, JOC, and OAMD payloads\n",
        "\n",
    ));
    append_heading(output, "OPTIONS", color)?;
    output.push_str(concat!(
        "  -h, --help       Print root command help\n",
        "      --no-banner Disable the interactive startup banner\n",
        "      --reference-f64 Use explicit reference f64 object-stem output (default: f32)\n",
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

fn inspect(input: &Path) -> Result<(), Box<dyn Error>> {
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
        match openjoc_eac3::extract_auxdata(frame)? {
            Some(_) => {
                aux_present += 1;
                emdf_attempts += 1;
                if openjoc_eac3::extract_aux_emdf(frame)?.is_some() {
                    emdf_parsed += 1;
                }
            }
            None => aux_absent += 1,
        }
        match openjoc_eac3::inspect_audio_block_carriers(frame, |value| {
            skip_examined += 1;
            if value.skip_field.is_some() {
                skip_observed += 1;
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
    println!("  frame-end EMDF: {emdf_parsed} parsed from {emdf_attempts} bounded attempts");
    println!(
        "  audio-block skipfld: {skip_observed} observed in {skip_examined} reached first-block prefixes; {skip_unresolved} later blocks unresolved pending mantissa cursor traversal"
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
        match openjoc_eac3::extract_joc_access_unit(&media.bytes, &frames, unit) {
            Ok(Some(metadata)) => {
                println!("  carrier frame: {}", metadata.carrier_frame);
                println!("  complexity index: {}", metadata.complexity_index);
                println!("  OAMD bytes: {}", metadata.oamd.len());
                println!("  JOC bytes: {}", metadata.joc.len());
            }
            Ok(None) => {
                if let Some(extension) =
                    extract_joc_addbsi_access_unit(&media.bytes, &frames, unit)?
                {
                    println!(
                        "  JOC extension signaled: complexity index {}; no profile in examined frame-end/skipfld carriers",
                        extension.complexity_index
                    );
                } else {
                    println!("  JOC profile: absent");
                }
            }
            Err(error) => {
                println!(
                    "  JOC profile candidate found but validation failed in examined carriers: {error}"
                );
            }
        }
    }
    Ok(())
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
    let mut decoder = PayloadDecoder::new(PayloadDecoderConfig {
        reference_screen: arguments.reference_screen,
        oamd: OamdDecoderConfig {
            trim_configuration_count: arguments.trim_count,
        },
    });
    decoder.decode_frame_with(
        JocFrameInput {
            sample_rate: downmix.sample_rate,
            downmix_pcm: &downmix.channels,
            joc_payload: &joc_payload,
            oamd_payload: &oamd_payload,
            frame_index: 0,
        },
        |frame| write_debug(&arguments.output, 0, frame),
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
    let mut output = None;
    let mut index = 1;
    while index < values.len() {
        let flag = &values[index];
        if flag == "--internal-base" {
            internal_base = true;
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
    })
}

fn decode_eac3(arguments: &DecodeEac3Args) -> Result<(), Box<dyn Error>> {
    let media = load_eac3(&arguments.input)?;
    let stream = &media.bytes;
    let config = PayloadDecoderConfig {
        reference_screen: None,
        oamd: OamdDecoderConfig {
            trim_configuration_count: None,
        },
    };
    let sink_output = arguments.output.clone();
    let scene = if arguments.internal_base {
        let dither = deterministic_dither_values();
        eac3_decode::decode_internal_eac3_with_sink(
            stream,
            config,
            &dither,
            |frame_index, frame| {
                write_debug(&sink_output, frame_index, frame)
                    .map_err(|error| eac3_decode::DecodeEac3Error::Sink(error.to_string()))
            },
        )?
    } else {
        let downmix_path = match &arguments.downmix {
            Some(path) => path.clone(),
            None => decode_base_audio(&arguments.input, &arguments.output)?,
        };
        let downmix = decode(&fs::read(downmix_path)?)?;
        eac3_decode::decode_aligned_eac3_with_sink(
            stream,
            &downmix,
            config,
            |frame_index, frame| {
                write_debug(&sink_output, frame_index, frame)
                    .map_err(|error| eac3_decode::DecodeEac3Error::Sink(error.to_string()))
            },
        )?
    };
    write_scene(&arguments.output, &scene, arguments.output_format)
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

fn decode_base_audio(input: &Path, output: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let debug = output.join("debug");
    fs::create_dir_all(&debug)?;
    // pcm_f64le is an explicit reference/debug format. It is compatible
    // base-channel PCM, not a final speaker or binaural render.
    let base_pcm = debug.join("compatible_base.wav");
    let result = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-i"])
        .arg(input)
        .args(["-map", "0:a:0", "-c:a", "pcm_f64le"])
        .arg(&base_pcm)
        .output()?;
    if !result.status.success() {
        return Err(io::Error::other(format!(
            "base E-AC-3 decode failed: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        ))
        .into());
    }
    Ok(base_pcm)
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
    let objects = output.join("objects");
    let metadata = output.join("metadata");
    fs::create_dir_all(&objects)?;
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
    for object in &scene.objects {
        let filename = format!("object_{:03}.wav", object.object_id);
        fs::write(
            objects.join(filename),
            encode_channels(
                scene.sample_rate,
                std::slice::from_ref(&object.pcm),
                WaveEncodeOptions {
                    sample_format,
                    clipping: Clipping::Reject,
                    dither: Dither::None,
                },
            )?,
        )?;
    }
    Ok(())
}

fn write_debug(
    output: &Path,
    frame_index: usize,
    decoded: &openjoc_scene::DecodedPayloadFrame,
) -> Result<(), Box<dyn Error>> {
    let frame = output.join(format!("debug/frame_{frame_index:03}"));
    fs::create_dir_all(&frame)?;
    fs::write(frame.join("joc.txt"), format!("{:#?}\n", decoded.joc))?;
    fs::write(frame.join("oamd.txt"), format!("{:#?}\n", decoded.oamd))?;
    fs::write(
        frame.join("reconstruction.txt"),
        format!("{:#?}\n", decoded.decoded),
    )?;
    Ok(())
}

fn usage_error() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, USAGE)
}
