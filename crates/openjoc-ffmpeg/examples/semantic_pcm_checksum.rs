use openjoc_api::{BinauralConfig, OpenJocConfig, RenderMode};
use openjoc_ffmpeg::FfmpegDecoder;
use sha2::{Digest, Sha256};
use std::{
    env,
    fmt::Write as _,
    fs::File,
    io::{BufReader, Read},
    process::ExitCode,
};

fn run() -> Result<(), String> {
    let mut arguments = env::args().skip(1);
    let input = arguments
        .next()
        .ok_or("usage: semantic_pcm_checksum INPUT.f32 LAYOUT [--binaural]")?;
    let layout = arguments
        .next()
        .ok_or("usage: semantic_pcm_checksum INPUT.f32 LAYOUT [--binaural]")?;
    let binaural = match arguments.next().as_deref() {
        None => false,
        Some("--binaural") => true,
        Some(_) => return Err("expected --binaural".to_owned()),
    };
    if arguments.next().is_some() {
        return Err("too many arguments".to_owned());
    }

    let config = if binaural {
        OpenJocConfig {
            render_mode: RenderMode::Binaural,
            speaker_layout: layout.clone(),
            binaural: Some(BinauralConfig::builtin_generic(layout)),
            ..OpenJocConfig::default()
        }
    } else {
        OpenJocConfig {
            render_mode: RenderMode::Speaker,
            speaker_layout: layout,
            ..OpenJocConfig::default()
        }
    };
    let decoder = FfmpegDecoder::new(config).map_err(|error| error.to_string())?;
    let channels = decoder.channel_layout().ffmpeg_order.len();
    let inverse = decoder
        .channel_layout()
        .inverse_permutation()
        .map_err(|error| error.to_string())?;
    let frame_bytes = channels
        .checked_mul(std::mem::size_of::<f32>())
        .ok_or("frame size overflow")?;
    let source: Box<dyn Read> = if input == "-" {
        Box::new(std::io::stdin())
    } else {
        Box::new(File::open(input).map_err(|error| error.to_string())?)
    };
    let mut reader = BufReader::new(source);
    let mut buffer = vec![0_u8; frame_bytes * 4096];
    let mut pending = Vec::new();
    let mut digest = Sha256::new();
    let mut sample_frames = 0_u64;

    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        pending.extend_from_slice(&buffer[..read]);
        let complete = pending.len() / frame_bytes * frame_bytes;
        for frame in pending[..complete].chunks_exact(frame_bytes) {
            for &output_channel in &inverse {
                let start = output_channel * std::mem::size_of::<f32>();
                digest.update(&frame[start..start + std::mem::size_of::<f32>()]);
            }
            sample_frames = sample_frames.saturating_add(1);
        }
        pending.drain(..complete);
    }
    if !pending.is_empty() {
        return Err("raw PCM ends with a partial sample frame".to_owned());
    }

    let checksum = digest
        .finalize()
        .iter()
        .fold(String::new(), |mut output, byte| {
            let _ = write!(output, "{byte:02x}");
            output
        });
    println!(
        "samples_per_channel={sample_frames} channels={channels} pcm-openjoc-order-f32le-sha256={checksum}"
    );
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
