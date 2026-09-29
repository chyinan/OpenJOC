// pattern: Imperative Shell

//! Reopenable, bounded input for the CLI's preflight and render passes.

use crate::eac3_decode::{
    DecodeEac3Error, StreamTiming, ValidationProfileRequest, rebase_frame_error,
    resolve_profile_for_stream,
};
use openjoc_container::{
    DEFAULT_MAX_EAC3_BYTES, InputMediaError, InputMediaKind, RawEac3AccessUnitReader, detect_media,
    open_seekable_iso_bmff,
};
use openjoc_eac3::MAX_SYNCFRAME_BYTES;
use openjoc_emdf::JocValidationProfile;
use openjoc_scene::PayloadDecoderConfig;
use std::{
    fs::File,
    io::{BufReader, Read},
    path::{Path, PathBuf},
};

pub(crate) struct RenderInput {
    path: PathBuf,
    kind: InputMediaKind,
}

impl RenderInput {
    pub(crate) fn new(path: &Path) -> Result<Self, InputMediaError> {
        let mut file = open_file(path)?;
        let mut prefix = [0_u8; 12];
        let mut count = 0;
        while count < prefix.len() {
            let read = file
                .read(&mut prefix[count..])
                .map_err(|source| InputMediaError::Io {
                    operation: "read input signature",
                    source,
                })?;
            if read == 0 {
                break;
            }
            count += read;
        }
        if count == 0 {
            return Err(InputMediaError::EmptyInput);
        }
        let kind = detect_media(&prefix[..count]);
        if kind == InputMediaKind::Unknown {
            return Err(InputMediaError::UnsupportedSignature);
        }
        Ok(Self {
            path: path.to_owned(),
            kind,
        })
    }

    pub(crate) fn open(&self) -> Result<Box<dyn Read>, InputMediaError> {
        match self.kind {
            InputMediaKind::RawEac3 => Ok(Box::new(BufReader::new(open_file(&self.path)?))),
            // This is a per-container-sample bound, not a programme length cap.
            InputMediaKind::IsoBmff => Ok(Box::new(open_seekable_iso_bmff(
                &self.path,
                Path::new("ffprobe"),
                DEFAULT_MAX_EAC3_BYTES,
            )?)),
            InputMediaKind::Unknown => Err(InputMediaError::UnsupportedSignature),
        }
    }

    pub(crate) fn preflight(
        &self,
        config: PayloadDecoderConfig,
        request: ValidationProfileRequest,
    ) -> Result<(StreamTiming, JocValidationProfile), DecodeEac3Error> {
        scan(self.open()?, config, request)
    }
}

fn open_file(path: &Path) -> Result<File, InputMediaError> {
    File::open(path).map_err(|source| InputMediaError::Io {
        operation: "open input file",
        source,
    })
}

fn scan(
    reader: impl Read,
    config: PayloadDecoderConfig,
    request: ValidationProfileRequest,
) -> Result<(StreamTiming, JocValidationProfile), DecodeEac3Error> {
    let mut reader = RawEac3AccessUnitReader::new(reader, MAX_SYNCFRAME_BYTES);
    let mut timing = StreamTiming {
        access_units: 0,
        samples: 0,
        sample_rate: 0,
    };
    let mut selected = match request {
        ValidationProfileRequest::ObservedVendorCompat => {
            JocValidationProfile::ObservedVendorCompat
        }
        _ => JocValidationProfile::EtsiStrict,
    };
    let mut frame_offset = 0_usize;
    while let Some(au) = reader
        .next_access_unit()
        .map_err(|error| rebase_frame_error(error.into(), frame_offset))?
    {
        if request == ValidationProfileRequest::Auto {
            // Keep the existing whole-stream AUTO contract: a deviation in any
            // AU promotes the legacy render pass, including preceding AUs.
            let access_unit = usize::try_from(timing.access_units)
                .map_err(|_| DecodeEac3Error::FrameIndexOverflow)?;
            let profile = resolve_profile_for_stream(&au.bytes, config, request).map_err(
                |error| match error {
                    DecodeEac3Error::MissingMetadata { .. } => {
                        DecodeEac3Error::MissingMetadata { access_unit }
                    }
                    DecodeEac3Error::JocExtensionWithoutMetadata {
                        complexity_index, ..
                    } => DecodeEac3Error::JocExtensionWithoutMetadata {
                        access_unit,
                        complexity_index,
                    },
                    error => rebase_frame_error(error, frame_offset),
                },
            )?;
            if profile == JocValidationProfile::ObservedVendorCompat {
                selected = profile;
            }
        }
        if timing.access_units == 0 {
            timing.sample_rate = au.unit.sample_rate;
        }
        timing.access_units = timing
            .access_units
            .checked_add(1)
            .ok_or(DecodeEac3Error::FrameIndexOverflow)?;
        timing.samples = timing
            .samples
            .checked_add(u64::from(au.unit.samples))
            .ok_or(DecodeEac3Error::SampleCountOverflow)?;
        frame_offset = frame_offset
            .checked_add(au.unit.frame_count)
            .ok_or(DecodeEac3Error::FrameIndexOverflow)?;
    }
    if timing.access_units == 0 {
        return Err(DecodeEac3Error::EmptyStream);
    }
    Ok((timing, selected))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;

    fn config() -> PayloadDecoderConfig {
        PayloadDecoderConfig {
            reference_screen: None,
            oamd: openjoc_oamd::OamdDecoderConfig::with_trim_configuration_count(None),
        }
    }

    struct RepeatedFrames {
        frame: Vec<u8>,
        remaining: usize,
        offset: usize,
        chunk: usize,
        max_request: usize,
    }

    impl RepeatedFrames {
        fn new(frames: usize, chunk: usize) -> Self {
            let mut frame = vec![0; MAX_SYNCFRAME_BYTES];
            // 48 kHz E-AC-3 I0, six blocks, stereo, 4096-byte syncframe.
            frame[..6].copy_from_slice(&[0x0b, 0x77, 0x07, 0xff, 0x34, 0x80]);
            Self {
                remaining: frames * frame.len(),
                frame,
                offset: 0,
                chunk,
                max_request: 0,
            }
        }
    }

    impl Read for RepeatedFrames {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            self.max_request = self.max_request.max(buffer.len());
            let count = self
                .remaining
                .min(buffer.len())
                .min(self.chunk)
                .min(self.frame.len() - self.offset);
            buffer[..count].copy_from_slice(&self.frame[self.offset..self.offset + count]);
            self.remaining -= count;
            self.offset = (self.offset + count) % self.frame.len();
            Ok(count)
        }
    }

    #[test]
    fn preflight_counts_a_programme_larger_than_the_old_cap_with_bounded_reads() {
        let frames = DEFAULT_MAX_EAC3_BYTES / MAX_SYNCFRAME_BYTES + 1;
        let mut reader = RepeatedFrames::new(frames, MAX_SYNCFRAME_BYTES);
        let (timing, profile) =
            scan(&mut reader, config(), ValidationProfileRequest::EtsiStrict).unwrap();
        assert_eq!(timing.access_units, frames as u64);
        assert_eq!(timing.samples, frames as u64 * 1536);
        assert_eq!(timing.sample_rate, 48_000);
        assert_eq!(profile, JocValidationProfile::EtsiStrict);
        assert_eq!(reader.remaining, 0);
        assert!(reader.max_request <= MAX_SYNCFRAME_BYTES);
    }

    #[test]
    fn fragmented_preflight_matches_the_original_slice_timing() {
        let bytes = RepeatedFrames::new(1, 1).frame.repeat(3);
        let expected = crate::eac3_decode::stream_timing(&bytes).unwrap();
        for chunk in [1, 7, 4096] {
            let (actual, profile) = scan(
                RepeatedFrames::new(3, chunk),
                config(),
                ValidationProfileRequest::ObservedVendorCompat,
            )
            .unwrap();
            assert_eq!(actual, expected);
            assert_eq!(profile, JocValidationProfile::ObservedVendorCompat);
        }
    }

    #[test]
    fn preflight_rejects_empty_and_truncated_input() {
        assert!(matches!(
            scan(&[][..], config(), ValidationProfileRequest::Auto),
            Err(DecodeEac3Error::EmptyStream)
        ));
        let frame = RepeatedFrames::new(1, 1).frame;
        assert!(
            scan(
                &frame[..frame.len() - 1],
                config(),
                ValidationProfileRequest::EtsiStrict,
            )
            .is_err()
        );
    }
}
