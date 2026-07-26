// pattern: Imperative Shell

mod eac3_decode;

use openjoc_oamd::{OamdDecoderConfig, Position3, ReferenceScreen};
use openjoc_scene::{JocFrameInput, PayloadDecoder, PayloadDecoderConfig};
use openjoc_wave::{decode, encode_f64_mono};
use std::{
    env,
    error::Error,
    fs, io,
    num::NonZeroU8,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

const USAGE: &str = "usage: openjoc inspect FILE\n       openjoc decode FILE -o DIR [--downmix FILE]\n       openjoc decode-payload --downmix FILE --joc FILE --oamd FILE -o DIR [--trim-config-count N] [--screen-origin-x X --screen-origin-y Y --screen-origin-z Z --screen-width W --screen-height H]";

struct DecodePayloadArgs {
    downmix: PathBuf,
    joc: PathBuf,
    oamd: PathBuf,
    output: PathBuf,
    trim_count: Option<NonZeroU8>,
    reference_screen: Option<ReferenceScreen>,
}

struct DecodeEac3Args {
    input: PathBuf,
    downmix: Option<PathBuf>,
    output: PathBuf,
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
    let mut arguments = env::args().skip(1);
    match arguments.next().as_deref() {
        Some("inspect") => {
            let input = arguments.next().ok_or_else(usage_error)?;
            if arguments.next().is_some() {
                return Err(usage_error().into());
            }
            inspect(Path::new(&input))
        }
        Some("decode-payload") => {
            let values = arguments.collect::<Vec<_>>();
            decode_payload(&values)
        }
        Some("decode") => {
            let values = arguments.collect::<Vec<_>>();
            decode_eac3(&parse_decode_eac3(&values)?)
        }
        _ => Err(usage_error().into()),
    }
}

fn inspect(input: &Path) -> Result<(), Box<dyn Error>> {
    let stream = fs::read(input)?;
    let frames = openjoc_eac3::index_syncframes(&stream)?;
    let units = openjoc_eac3::group_access_units(&frames)?;
    println!("frames: {}", frames.len());
    println!("access units: {}", units.len());
    for (unit_index, unit) in units.iter().copied().enumerate() {
        println!("access unit {unit_index}:");
        println!("  sample rate: {} Hz", unit.sample_rate);
        println!("  samples: {}", unit.samples);
        if let Some(metadata) = openjoc_eac3::extract_aux_joc_access_unit(&stream, &frames, unit)? {
            println!("  carrier frame: {}", metadata.carrier_frame);
            println!("  complexity index: {}", metadata.complexity_index);
            println!("  OAMD bytes: {}", metadata.oamd.len());
            println!("  JOC bytes: {}", metadata.joc.len());
        } else {
            println!("  JOC profile: absent");
        }
    }
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
    let frame_output = decoder.decode_frame(JocFrameInput {
        sample_rate: downmix.sample_rate,
        downmix_pcm: &downmix.channels,
        joc_payload: &joc_payload,
        oamd_payload: &oamd_payload,
        frame_index: 0,
    })?;
    let scene = decoder.finish()?;
    write_scene(&arguments.output, &scene)?;
    write_debug(&arguments.output, 0, &frame_output)?;
    Ok(())
}

fn parse_decode_eac3(values: &[String]) -> Result<DecodeEac3Args, Box<dyn Error>> {
    let input = values.first().filter(|value| !value.starts_with('-'));
    let mut downmix = None;
    let mut output = None;
    let mut index = 1;
    while index < values.len() {
        let flag = &values[index];
        let value = values.get(index + 1).ok_or_else(usage_error)?;
        match flag.as_str() {
            "--downmix" => downmix = Some(PathBuf::from(value)),
            "-o" | "--output" => output = Some(PathBuf::from(value)),
            _ => return Err(usage_error().into()),
        }
        index += 2;
    }
    Ok(DecodeEac3Args {
        input: PathBuf::from(input.ok_or_else(usage_error)?),
        downmix,
        output: output.ok_or_else(usage_error)?,
    })
}

fn decode_eac3(arguments: &DecodeEac3Args) -> Result<(), Box<dyn Error>> {
    let stream = fs::read(&arguments.input)?;
    let downmix_path = match &arguments.downmix {
        Some(path) => path.clone(),
        None => decode_base_audio(&arguments.input, &arguments.output)?,
    };
    let downmix = decode(&fs::read(downmix_path)?)?;
    let config = PayloadDecoderConfig {
        reference_screen: None,
        oamd: OamdDecoderConfig {
            trim_configuration_count: None,
        },
    };
    let decoded = eac3_decode::decode_aligned_eac3(&stream, &downmix, config)?;
    for (frame_index, frame) in decoded.frames.iter().enumerate() {
        write_debug(&arguments.output, frame_index, frame)?;
    }
    write_scene(&arguments.output, &decoded.scene)
}

fn decode_base_audio(input: &Path, output: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let debug = output.join("debug");
    fs::create_dir_all(&debug)?;
    let downmix = debug.join("downmix.wav");
    let result = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-i"])
        .arg(input)
        .args(["-map", "0:a:0", "-c:a", "pcm_f64le"])
        .arg(&downmix)
        .output()?;
    if !result.status.success() {
        return Err(io::Error::other(format!(
            "base E-AC-3 decode failed: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        ))
        .into());
    }
    Ok(downmix)
}

fn parse_decode_payload(values: &[String]) -> Result<DecodePayloadArgs, Box<dyn Error>> {
    let mut downmix = None;
    let mut joc = None;
    let mut oamd = None;
    let mut output = None;
    let mut trim_count = None;
    let mut screen = [None; 5];
    let mut index = 0;
    while index < values.len() {
        let flag = &values[index];
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
    })
}

fn write_scene(output: &Path, scene: &openjoc_scene::ObjectScene) -> Result<(), Box<dyn Error>> {
    let objects = output.join("objects");
    let metadata = output.join("metadata");
    fs::create_dir_all(&objects)?;
    fs::create_dir_all(&metadata)?;
    fs::write(output.join("scene.json"), scene.to_manifest_json_pretty()?)?;
    fs::write(
        metadata.join("timeline.json"),
        scene.to_timeline_json_pretty()?,
    )?;
    for object in &scene.objects {
        let filename = format!("object_{:03}.wav", object.object_id);
        fs::write(
            objects.join(filename),
            encode_f64_mono(scene.sample_rate, &object.pcm)?,
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
