// pattern: Imperative Shell

use openjoc_oamd::{OamdDecoderConfig, Position3, ReferenceScreen};
use openjoc_scene::{JocFrameInput, PayloadDecoder, PayloadDecoderConfig};
use openjoc_wave::{decode, encode_f64_mono};
use std::{
    env,
    error::Error,
    fs, io,
    num::NonZeroU8,
    path::{Path, PathBuf},
    process::ExitCode,
};

const USAGE: &str = "usage: openjoc decode-payload --downmix FILE --joc FILE --oamd FILE -o DIR [--trim-config-count N] [--screen-origin-x X --screen-origin-y Y --screen-origin-z Z --screen-width W --screen-height H]";

struct DecodePayloadArgs {
    downmix: PathBuf,
    joc: PathBuf,
    oamd: PathBuf,
    output: PathBuf,
    trim_count: Option<NonZeroU8>,
    reference_screen: Option<ReferenceScreen>,
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
    if arguments.next().as_deref() != Some("decode-payload") {
        return Err(usage_error().into());
    }
    let values = arguments.collect::<Vec<_>>();
    let arguments = parse_decode_payload(&values)?;
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
    write_debug(&arguments.output, &frame_output)?;
    Ok(())
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
    decoded: &openjoc_scene::DecodedPayloadFrame,
) -> Result<(), Box<dyn Error>> {
    let frame = output.join("debug/frame_000");
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
