use openjoc_api::{BinauralConfig, OpenJocConfig, RenderMode};
use openjoc_ffmpeg::{
    BridgeStatus, Demuxer, FfmpegDecoder, FfmpegLibraryVersions, PacketRef, ReceiveAvOutcome,
};
use openjoc_wave::{Clipping, Dither, SampleFormat, WaveEncodeOptions, WaveWriter};
use sha2::{Digest, Sha256};
use std::{env, fs::File, process::ExitCode, time::Instant};

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug)]
struct Arguments {
    input: String,
    output: Option<String>,
    layout: String,
    binaural: bool,
    null: bool,
    checksum: bool,
    semantic_checksum: bool,
    trace: bool,
}

impl Arguments {
    fn parse() -> Result<Self, String> {
        let mut input = None;
        let mut output = None;
        let mut layout = "5.1".to_owned();
        let mut binaural = false;
        let mut null = false;
        let mut checksum = false;
        let mut semantic_checksum = false;
        let mut trace = false;
        let mut values = env::args().skip(1);
        while let Some(value) = values.next() {
            match value.as_str() {
                "--output" | "-o" => {
                    output = Some(values.next().ok_or("--output requires a path")?);
                }
                "--layout" => layout = values.next().ok_or("--layout requires a name")?,
                "--binaural" => {
                    binaural = true;
                    if layout == "5.1" {
                        "7.1.4".clone_into(&mut layout);
                    }
                }
                "--null" => null = true,
                "--checksum" => checksum = true,
                "--semantic-checksum" => {
                    checksum = true;
                    semantic_checksum = true;
                }
                "--trace" => trace = true,
                "--help" | "-h" => return Err(usage()),
                option if option.starts_with('-') => {
                    return Err(format!("unknown option {option}\n{}", usage()));
                }
                path if input.is_none() => input = Some(path.to_owned()),
                path => return Err(format!("unexpected positional argument {path}")),
            }
        }
        let input = input.ok_or_else(usage)?;
        if output.is_none() && !null && !checksum {
            return Err(format!(
                "select --output PATH, --null, or --checksum\n{}",
                usage()
            ));
        }
        Ok(Self {
            input,
            output,
            layout,
            binaural,
            null,
            checksum,
            semantic_checksum,
            trace,
        })
    }
}

fn usage() -> String {
    "usage: openjoc-avdecode INPUT [--binaural | --layout NAME] [--output FILE.wav] [--null] [--checksum | --semantic-checksum] [--trace]".to_owned()
}

struct Sink {
    writer: Option<WaveWriter<File>>,
    digest: Sha256,
    checksum: bool,
    semantic_checksum: bool,
    inverse_permutation: Vec<usize>,
    frames: u64,
    samples: u64,
    channels: usize,
}

impl Sink {
    fn new(
        arguments: &Arguments,
        channels: usize,
        inverse_permutation: Vec<usize>,
    ) -> Result<Self, String> {
        let writer = arguments
            .output
            .as_ref()
            .map(|path| {
                let file = File::create(path).map_err(|error| error.to_string())?;
                WaveWriter::new(
                    file,
                    48_000,
                    channels,
                    WaveEncodeOptions {
                        sample_format: SampleFormat::F32,
                        clipping: Clipping::Reject,
                        dither: Dither::None,
                    },
                )
                .map_err(|error| error.to_string())
            })
            .transpose()?;
        Ok(Self {
            writer,
            digest: Sha256::new(),
            checksum: arguments.checksum,
            semantic_checksum: arguments.semantic_checksum,
            inverse_permutation,
            frames: 0,
            samples: 0,
            channels,
        })
    }

    fn write(&mut self, frame: &openjoc_ffmpeg::AvFrame) -> Result<(), String> {
        if !frame.is_packed_float()
            || frame.sample_rate() != 48_000
            || usize::try_from(frame.channel_count()) != Ok(self.channels)
        {
            return Err("received AVFrame has an unexpected format".to_owned());
        }
        let samples = frame.interleaved_f32();
        if self.checksum {
            if self.semantic_checksum {
                for frame in samples.chunks_exact(self.channels) {
                    for &output_channel in &self.inverse_permutation {
                        self.digest.update(frame[output_channel].to_le_bytes());
                    }
                }
            } else {
                for sample in samples {
                    self.digest.update(sample.to_le_bytes());
                }
            }
        }
        if let Some(writer) = self.writer.as_mut() {
            let converted: Vec<f64> = samples.iter().copied().map(f64::from).collect();
            writer
                .write_interleaved(&converted)
                .map_err(|error| error.to_string())?;
        }
        self.frames = self.frames.saturating_add(1);
        self.samples = self
            .samples
            .saturating_add(u64::try_from(frame.nb_samples()).unwrap_or(0));
        Ok(())
    }

    fn finish(mut self) -> Result<(u64, u64, Option<String>), String> {
        if let Some(writer) = self.writer.take() {
            writer.finish().map_err(|error| error.to_string())?;
        }
        let checksum = self.checksum.then(|| digest_hex(&self.digest.finalize()));
        Ok((self.frames, self.samples, checksum))
    }
}

fn receive_available(
    decoder: &mut FfmpegDecoder,
    sink: &mut Sink,
    trace: bool,
) -> Result<bool, String> {
    loop {
        match decoder
            .receive_avframe()
            .map_err(|error| error.to_string())?
        {
            ReceiveAvOutcome::Frame(frame) => {
                if trace {
                    eprintln!(
                        "AVFrame pts={:?} nb_samples={} duration={} format=AV_SAMPLE_FMT_FLT layout={} latency_samples={}",
                        frame.pts(),
                        frame.nb_samples(),
                        frame.duration(),
                        frame
                            .layout_description()
                            .map_err(|error| error.to_string())?,
                        decoder.latency_samples(),
                    );
                }
                sink.write(&frame)?;
            }
            ReceiveAvOutcome::NeedMoreInput => return Ok(false),
            ReceiveAvOutcome::EndOfStream => return Ok(true),
            ReceiveAvOutcome::NotJoc => {
                return Err("NOT_JOC: ordinary E-AC-3 is not decoded by OpenJOC".to_owned());
            }
        }
    }
}

fn digest_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut output, byte| {
        let _ = write!(output, "{byte:02x}");
        output
    })
}

fn run(arguments: &Arguments) -> Result<(), String> {
    let config = if arguments.binaural {
        OpenJocConfig {
            render_mode: RenderMode::Binaural,
            speaker_layout: arguments.layout.clone(),
            binaural: Some(BinauralConfig::builtin_generic(arguments.layout.clone())),
            ..OpenJocConfig::default()
        }
    } else {
        OpenJocConfig {
            render_mode: RenderMode::Speaker,
            speaker_layout: arguments.layout.clone(),
            ..OpenJocConfig::default()
        }
    };
    let mut decoder = FfmpegDecoder::new(config).map_err(|error| error.to_string())?;
    let mut demuxer = Demuxer::open(&arguments.input).map_err(|error| error.to_string())?;
    let versions = FfmpegLibraryVersions::current();
    eprintln!(
        "libavutil={}.{}.{} libavcodec={}.{}.{} libavformat={}.{}.{} stream={} time_base={}/{} effective-config-sha256={}",
        versions.avutil.major,
        versions.avutil.minor,
        versions.avutil.micro,
        versions.avcodec.major,
        versions.avcodec.minor,
        versions.avcodec.micro,
        versions.avformat.major,
        versions.avformat.minor,
        versions.avformat.micro,
        demuxer.target_stream_index(),
        demuxer.time_base().numerator,
        demuxer.time_base().denominator,
        decoder.effective_config_fingerprint(),
    );
    let inverse = decoder
        .channel_layout()
        .inverse_permutation()
        .map_err(|error| error.to_string())?;
    let mut sink = Sink::new(
        arguments,
        decoder.channel_layout().ffmpeg_order.len(),
        inverse,
    )?;
    let mut packet_index = 0_u64;
    let mut demux_nanos = 0_u128;
    let target_stream = demuxer.target_stream_index();
    loop {
        let demux_started = Instant::now();
        let packet = demuxer.read_packet().map_err(|error| error.to_string())?;
        demux_nanos = demux_nanos.saturating_add(demux_started.elapsed().as_nanos());
        let Some(packet) = packet else { break };
        if packet.stream_index != target_stream {
            continue;
        }
        if arguments.trace {
            eprintln!(
                "AVPacket index={packet_index} stream={} size={} pts={:?} dts={:?} duration={:?} time_base={}/{}",
                packet.stream_index,
                packet.data.len(),
                packet.pts,
                packet.dts,
                packet.duration,
                packet.time_base.numerator,
                packet.time_base.denominator,
            );
        }
        let input = PacketRef {
            data: packet.data,
            pts: packet.pts,
            dts: packet.dts,
            duration: packet.duration,
            time_base: packet.time_base,
            stream_index: packet.stream_index,
            discontinuity: false,
            preroll: false,
        };
        loop {
            match decoder
                .send_packet(input)
                .map_err(|error| error.to_string())?
            {
                BridgeStatus::WouldBlock => {
                    let _ = receive_available(&mut decoder, &mut sink, arguments.trace)?;
                }
                BridgeStatus::NotJoc => {
                    return Err("NOT_JOC: ordinary E-AC-3 is not decoded by OpenJOC".to_owned());
                }
                _ => break,
            }
        }
        let _ = receive_available(&mut decoder, &mut sink, arguments.trace)?;
        for au in decoder.take_traces() {
            if arguments.trace {
                eprintln!(
                    "OpenJOC AU index={} bytes={} sha256={} pts_samples={:?} timestamp={:?} samples={} I={} D={}",
                    au.index,
                    au.byte_length,
                    au.sha256,
                    au.pts_samples,
                    au.timestamp_source,
                    au.sample_count,
                    au.independent_frame_count,
                    au.dependent_frame_count,
                );
            }
        }
        packet_index = packet_index.saturating_add(1);
    }
    let _ = decoder.drain().map_err(|error| error.to_string())?;
    while !receive_available(&mut decoder, &mut sink, arguments.trace)? {}
    let classification = decoder.classification();
    let (frames, samples, checksum) = sink.finish()?;
    eprintln!(
        "classification={} frames={} samples_per_channel={} channels={} latency_samples={} output_time_base=1/48000",
        classification.as_str(),
        frames,
        samples,
        decoder.channel_layout().ffmpeg_order.len(),
        decoder.latency_samples(),
    );
    let timings = decoder.timings();
    eprintln!(
        "stage-ms demux={:.3} packet-staging={:.3} au-assembly-admission={:.3} openjoc-session={:.3} channel-reorder={:.3} avframe-allocation={:.3}",
        demux_nanos as f64 / 1_000_000.0,
        timings.packet_staging_nanos as f64 / 1_000_000.0,
        timings.au_assembly_and_admission_nanos as f64 / 1_000_000.0,
        timings.openjoc_session_nanos as f64 / 1_000_000.0,
        timings.channel_reorder_nanos as f64 / 1_000_000.0,
        timings.avframe_allocation_nanos as f64 / 1_000_000.0,
    );
    if let Some(checksum) = checksum {
        if arguments.semantic_checksum {
            println!("pcm-openjoc-order-f32le-sha256={checksum}");
        } else {
            println!("pcm-f32le-sha256={checksum}");
        }
    }
    let _ = arguments.null;
    Ok(())
}

fn main() -> ExitCode {
    match Arguments::parse().and_then(|arguments| run(&arguments)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
