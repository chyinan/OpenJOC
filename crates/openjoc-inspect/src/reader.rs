// pattern: Imperative Shell

use crate::{ContainerSummary, InspectionAccumulator, InspectionOptions, StreamInspection};
use openjoc_container::{InputMediaError, InputMediaKind, RawEac3FrameReader};
use openjoc_eac3::{StreamType, SyncframeIndexEntry};
use std::{
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom},
    path::Path,
};

/// Reads raw compressed frames once, retaining at most a bounded programme candidate.
/// A failed lookahead does not discard an already complete preceding interval.
pub fn inspect_reader(reader: impl Read, options: InspectionOptions) -> StreamInspection {
    let mut source = RawEac3FrameReader::new(reader, 4096);
    let mut accumulator = InspectionAccumulator::new(options);
    let mut bytes = Vec::new();
    let mut frames = Vec::new();
    let mut blocks = 0_u16;
    loop {
        match source.next_frame() {
            Ok(Some(frame)) => {
                let header = match openjoc_container::raw_frame_header(&frame) {
                    Ok(header) => header,
                    Err(error) => {
                        accumulator.framing_failure(error.to_string());
                        break;
                    }
                };
                let independent =
                    header.stream_type != StreamType::Dependent && header.substream_id == 0;
                if independent && !frames.is_empty() && blocks >= 6 {
                    accumulator.push(&bytes, &frames);
                    bytes.clear();
                    frames.clear();
                    blocks = 0;
                }
                if frames.len() >= 72 || bytes.len() + frame.len() > 72 * 4096 {
                    accumulator.push(&bytes, &frames);
                    accumulator
                        .framing_failure("Programme candidate exceeds supported AU bound".into());
                    break;
                }
                frames.push(SyncframeIndexEntry {
                    offset: bytes.len(),
                    header,
                });
                bytes.extend_from_slice(&frame);
                if independent {
                    blocks += u16::from(header.audio_blocks);
                }
            }
            Ok(None) => {
                if !frames.is_empty() {
                    accumulator.push(&bytes, &frames);
                }
                break;
            }
            Err(error) => {
                if !frames.is_empty() {
                    accumulator.push(&bytes, &frames);
                }
                if !frames.is_empty() && blocks < 6 {
                    accumulator.incomplete_candidate_tail(error.to_string());
                } else {
                    accumulator.framing_failure(error.to_string());
                }
                break;
            }
        }
    }
    accumulator.finish()
}

/// Inspects raw EC3 or the existing lazy ISO BMFF demux reader.
/// Container declarations never determine in-band JOC classification.
pub fn inspect_path(
    path: &Path,
    options: InspectionOptions,
) -> Result<StreamInspection, InputMediaError> {
    let mut file = File::open(path).map_err(|source| InputMediaError::Io {
        operation: "open inspection input",
        source,
    })?;
    let mut signature = [0_u8; 16];
    let read = file
        .read(&mut signature)
        .map_err(|source| InputMediaError::Io {
            operation: "read input signature",
            source,
        })?;
    file.seek(SeekFrom::Start(0))
        .map_err(|source| InputMediaError::Io {
            operation: "rewind inspection input",
            source,
        })?;
    let kind = openjoc_container::detect_media(&signature[..read]);
    let mut report = match kind {
        InputMediaKind::RawEac3 => inspect_reader(BufReader::new(file), options),
        InputMediaKind::IsoBmff => {
            let reader = openjoc_container::open_seekable_iso_bmff(
                path,
                Path::new("ffprobe"),
                openjoc_eac3::GENERAL_MAX_ACCESS_UNIT_BYTES,
            )?;
            let mut report = inspect_reader(reader, options);
            report.container = ContainerSummary {
                kind: "iso_bmff".into(),
                ..ContainerSummary::default()
            };
            if let Ok(output) = std::process::Command::new("ffprobe")
                .args([
                    "-v",
                    "error",
                    "-select_streams",
                    "a",
                    "-show_entries",
                    "stream=codec_name,codec_tag_string,id,time_base,duration,nb_frames,extradata",
                    "-show_data",
                    "-of",
                    "json",
                ])
                .arg(path)
                .output()
            {
                if output.status.success() {
                    if let Ok(value) = serde_json::from_slice(&output.stdout) {
                        crate::container_metadata::apply_declarations(&mut report, &value);
                    }
                }
            }
            report.diagnostics.limitations.push("ISO BMFF packet selection uses the existing ffprobe cursor; unexposed container declarations remain unavailable.".into());
            report
        }
        InputMediaKind::Unknown => return Err(InputMediaError::UnsupportedSignature),
    };
    report.input.filename = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned());
    Ok(report)
}
