//! Private transactional file output for the CLI renderer.
//! Serialization and publication are kept separate from speaker/binaural DSP.

use super::{JocRenderError, RenderedBlock};
use openjoc_scene::{SemanticChannelLayout, SpeakerLayout, SpeakerLayoutPreset};
use openjoc_wave::{
    CafChannelDescription, CafWriter, Clipping, Dither, SampleFormat, WaveEncodeOptions, WaveWriter,
};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

fn replacement_backup_path(output: &Path) -> Result<PathBuf, io::Error> {
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = output
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "output has no filename"))?
        .to_string_lossy();
    Ok(parent.join(format!(".{name}.openjoc-replace-{}", std::process::id())))
}

pub(crate) fn replace_existing_file(staging: &Path, output: &Path) -> io::Result<()> {
    match fs::rename(staging, output) {
        Ok(()) => Ok(()),
        Err(_rename_error) if output.exists() => {
            let backup = replacement_backup_path(output)?;
            if backup.exists() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!(
                        "temporary replacement path already exists: {}",
                        backup.display()
                    ),
                ));
            }
            fs::rename(output, &backup)?;
            match fs::rename(staging, output) {
                Ok(()) => {
                    let _ = fs::remove_file(backup);
                    Ok(())
                }
                Err(replace_error) => {
                    if let Err(restore_error) = fs::rename(&backup, output) {
                        return Err(io::Error::other(format!(
                            "could not replace {} ({replace_error}); restoring the previous output also failed ({restore_error})",
                            output.display()
                        )));
                    }
                    Err(replace_error)
                }
            }
        }
        Err(error) => Err(error),
    }
}

/// The output container selected by the destination extension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutputContainer {
    Wav,
    Caf,
}

impl OutputContainer {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Wav => "WAV",
            Self::Caf => "CAF",
        }
    }
}

pub(super) fn output_container_for_path(path: &Path) -> Result<OutputContainer, JocRenderError> {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some(extension) if extension.eq_ignore_ascii_case("wav") => Ok(OutputContainer::Wav),
        Some(extension) if extension.eq_ignore_ascii_case("caf") => Ok(OutputContainer::Caf),
        _ => Err(JocRenderError::UnsupportedOutputExtension(path.to_owned())),
    }
}

pub fn validate_output_path(path: &Path) -> Result<OutputContainer, JocRenderError> {
    output_container_for_path(path)
}

struct StagedOutput {
    output: PathBuf,
    staging: PathBuf,
    overwrite: bool,
}

impl StagedOutput {
    fn new(output: &Path, overwrite: bool) -> Result<Self, JocRenderError> {
        if output.exists() && !overwrite {
            return Err(JocRenderError::OutputExists(output.to_owned()));
        }
        let parent = output
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let name = output
            .file_name()
            .ok_or_else(|| JocRenderError::InvalidControl("output has no filename".to_owned()))?
            .to_string_lossy();
        let staging = parent.join(format!(".{name}.openjoc-partial"));
        if staging.exists() {
            return Err(JocRenderError::OutputExists(staging));
        }
        Ok(Self {
            output: output.to_owned(),
            staging,
            overwrite,
        })
    }

    fn create(&self) -> Result<fs::File, JocRenderError> {
        Ok(fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.staging)?)
    }

    fn finish(&self) -> Result<(), JocRenderError> {
        let replacement = if self.overwrite {
            replace_existing_file(&self.staging, &self.output)
        } else if self.output.exists() {
            Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("refusing to overwrite output {}", self.output.display()),
            ))
        } else {
            fs::rename(&self.staging, &self.output)
        };
        replacement.map_err(Into::into)
    }

    fn abort(&self) {
        let _ = fs::remove_file(&self.staging);
    }
}

pub struct JocWavOutput {
    transaction: StagedOutput,
    format: SampleFormat,
    writer: Option<WaveWriter<fs::File>>,
    sample_rate: Option<u32>,
    channels: Option<usize>,
    speaker_mask: Option<u32>,
}

impl JocWavOutput {
    pub fn new_with_overwrite(
        output: &Path,
        format: SampleFormat,
        overwrite: bool,
    ) -> Result<Self, JocRenderError> {
        Self::new_with_mask(output, format, overwrite, None)
    }

    /// Creates transactional output for one of the admitted speaker layouts.
    /// The WAV header carries the standard speaker mask matching the preset's
    /// explicit public channel identities and order.
    #[allow(dead_code)]
    pub fn new_for_speaker_layout(
        output: &Path,
        format: SampleFormat,
        overwrite: bool,
        layout: &str,
    ) -> Result<Self, JocRenderError> {
        let preset = SpeakerLayoutPreset::for_name(layout)?;
        Self::new_for_semantic_layout(output, format, overwrite, &preset.semantic_channel_layout())
    }

    pub fn new_for_semantic_layout(
        output: &Path,
        format: SampleFormat,
        overwrite: bool,
        layout: &SemanticChannelLayout,
    ) -> Result<Self, JocRenderError> {
        let speaker_mask = match layout.wav_channel_mask() {
            Some(mask) => Some(mask),
            None if layout.name == "22.2" => None,
            None => {
                return Err(JocRenderError::WavLayoutNotExactlyRepresentable {
                    layout: layout.name.clone(),
                });
            }
        };
        Self::new_with_mask(output, format, overwrite, speaker_mask)
    }

    pub fn new_for_canonical_layout(
        output: &Path,
        format: SampleFormat,
        overwrite: bool,
        layout: &SpeakerLayout,
    ) -> Result<Self, JocRenderError> {
        let semantic = layout.semantic_channel_layout();
        let speaker_mask = match semantic.wav_channel_mask() {
            Some(mask) => Some(mask),
            None if semantic.name == "22.2" || layout.unmasked_wav_allowed() => None,
            None => {
                return Err(JocRenderError::WavLayoutNotExactlyRepresentable {
                    layout: semantic.name,
                });
            }
        };
        Self::new_with_mask(output, format, overwrite, speaker_mask)
    }

    fn new_with_mask(
        output: &Path,
        format: SampleFormat,
        overwrite: bool,
        speaker_mask: Option<u32>,
    ) -> Result<Self, JocRenderError> {
        Ok(Self {
            transaction: StagedOutput::new(output, overwrite)?,
            format,
            writer: None,
            sample_rate: None,
            channels: None,
            speaker_mask,
        })
    }

    pub fn write_block(&mut self, block: &RenderedBlock) -> Result<(), JocRenderError> {
        if block.channels.is_empty() {
            return Err(JocRenderError::NoRenderedFrames);
        }
        let channels = block.channels.len();
        if let Some(expected) = self.sample_rate {
            if expected != block.sample_rate || self.channels != Some(channels) {
                return Err(JocRenderError::InvalidControl(
                    "render output format changed during stream".to_owned(),
                ));
            }
        } else {
            let file = self.transaction.create()?;
            let options = WaveEncodeOptions {
                sample_format: self.format,
                clipping: Clipping::Reject,
                dither: Dither::None,
            };
            let writer = match self.speaker_mask {
                Some(mask) => WaveWriter::new_with_speaker_mask(
                    file,
                    block.sample_rate,
                    channels,
                    options,
                    mask,
                )?,
                None => WaveWriter::new(file, block.sample_rate, channels, options)?,
            };
            self.writer = Some(writer);
            self.sample_rate = Some(block.sample_rate);
            self.channels = Some(channels);
        }
        let references = block.channels.iter().map(Vec::as_slice).collect::<Vec<_>>();
        self.writer
            .as_mut()
            .ok_or(JocRenderError::NoRenderedFrames)?
            .write_channels(&references)?;
        Ok(())
    }

    pub fn finish(&mut self) -> Result<(), JocRenderError> {
        let writer = self.writer.take().ok_or(JocRenderError::NoRenderedFrames)?;
        if let Err(error) = writer.finish() {
            self.transaction.abort();
            return Err(error.into());
        }
        if let Err(error) = self.transaction.finish() {
            self.transaction.abort();
            return Err(error);
        }
        Ok(())
    }

    pub fn abort(&mut self) {
        self.writer.take();
        self.transaction.abort();
    }

    pub fn frames(&self) -> u64 {
        self.writer.as_ref().map_or(0, WaveWriter::frames)
    }
}

pub struct JocCafOutput {
    transaction: StagedOutput,
    format: SampleFormat,
    writer: Option<CafWriter<fs::File>>,
    sample_rate: Option<u32>,
    channels: usize,
    descriptions: Vec<CafChannelDescription>,
}

impl JocCafOutput {
    pub fn new_for_semantic_layout(
        output: &Path,
        format: SampleFormat,
        overwrite: bool,
        layout: &SemanticChannelLayout,
    ) -> Result<Self, JocRenderError> {
        let descriptions = layout
            .labels
            .iter()
            .enumerate()
            .map(|(index, label)| caf_description_at(layout, index, label))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            transaction: StagedOutput::new(output, overwrite)?,
            format,
            writer: None,
            sample_rate: None,
            channels: layout.channel_count(),
            descriptions,
        })
    }

    pub fn new_for_canonical_layout(
        output: &Path,
        format: SampleFormat,
        overwrite: bool,
        layout: &SpeakerLayout,
    ) -> Result<Self, JocRenderError> {
        let semantic = layout.semantic_channel_layout();
        let descriptions = semantic
            .labels
            .iter()
            .enumerate()
            .map(|(index, label)| caf_description_at_canonical(layout, index, label))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            transaction: StagedOutput::new(output, overwrite)?,
            format,
            writer: None,
            sample_rate: None,
            channels: semantic.channel_count(),
            descriptions,
        })
    }

    pub fn write_block(&mut self, block: &RenderedBlock) -> Result<(), JocRenderError> {
        if block.channels.is_empty() {
            return Err(JocRenderError::NoRenderedFrames);
        }
        let channels = block.channels.len();
        if channels != self.channels {
            return Err(JocRenderError::InvalidControl(
                "render output channel semantics changed during stream".to_owned(),
            ));
        }
        if let Some(expected) = self.sample_rate {
            if expected != block.sample_rate {
                return Err(JocRenderError::InvalidControl(
                    "render output sample rate changed during stream".to_owned(),
                ));
            }
        } else {
            let file = self.transaction.create()?;
            let options = WaveEncodeOptions {
                sample_format: self.format,
                clipping: Clipping::Reject,
                dither: Dither::None,
            };
            self.writer = Some(CafWriter::new(
                file,
                block.sample_rate,
                channels,
                options,
                &self.descriptions,
            )?);
            self.sample_rate = Some(block.sample_rate);
        }
        let references = block.channels.iter().map(Vec::as_slice).collect::<Vec<_>>();
        self.writer
            .as_mut()
            .ok_or(JocRenderError::NoRenderedFrames)?
            .write_channels(&references)?;
        Ok(())
    }

    pub fn finish(&mut self) -> Result<(), JocRenderError> {
        let writer = self.writer.take().ok_or(JocRenderError::NoRenderedFrames)?;
        if let Err(error) = writer.finish() {
            self.transaction.abort();
            return Err(error.into());
        }
        if let Err(error) = self.transaction.finish() {
            self.transaction.abort();
            return Err(error);
        }
        Ok(())
    }

    pub fn abort(&mut self) {
        self.writer.take();
        self.transaction.abort();
    }

    pub fn frames(&self) -> u64 {
        self.writer.as_ref().map_or(0, CafWriter::frames)
    }
}

/// Container-selected output sink. Renderer blocks are independent of this
/// enum; only the destination extension chooses the serializer.
pub enum JocPcmOutput {
    Wav(JocWavOutput),
    Caf(JocCafOutput),
}

impl JocPcmOutput {
    #[allow(dead_code)]
    pub fn new_for_speaker_layout(
        output: &Path,
        format: SampleFormat,
        overwrite: bool,
        layout: &str,
    ) -> Result<Self, JocRenderError> {
        let preset = SpeakerLayoutPreset::for_name(layout)?;
        let semantic = preset.semantic_channel_layout();
        Self::new_for_semantic_layout(output, format, overwrite, &semantic)
    }

    pub fn new_for_binaural(
        output: &Path,
        format: SampleFormat,
        overwrite: bool,
    ) -> Result<Self, JocRenderError> {
        let semantic =
            SemanticChannelLayout::without_wav_mapping("binaural", ["Left", "Right"], None);
        match output_container_for_path(output)? {
            OutputContainer::Wav => Ok(Self::Wav(JocWavOutput::new_with_overwrite(
                output, format, overwrite,
            )?)),
            OutputContainer::Caf => Ok(Self::Caf(JocCafOutput::new_for_semantic_layout(
                output, format, overwrite, &semantic,
            )?)),
        }
    }

    pub(crate) fn new_for_semantic_layout(
        output: &Path,
        format: SampleFormat,
        overwrite: bool,
        layout: &SemanticChannelLayout,
    ) -> Result<Self, JocRenderError> {
        match output_container_for_path(output)? {
            OutputContainer::Wav => Ok(Self::Wav(JocWavOutput::new_for_semantic_layout(
                output, format, overwrite, layout,
            )?)),
            OutputContainer::Caf => Ok(Self::Caf(JocCafOutput::new_for_semantic_layout(
                output, format, overwrite, layout,
            )?)),
        }
    }

    pub(crate) fn new_for_canonical_layout(
        output: &Path,
        format: SampleFormat,
        overwrite: bool,
        layout: &SpeakerLayout,
    ) -> Result<Self, JocRenderError> {
        match output_container_for_path(output)? {
            OutputContainer::Wav => Ok(Self::Wav(JocWavOutput::new_for_canonical_layout(
                output, format, overwrite, layout,
            )?)),
            OutputContainer::Caf => Ok(Self::Caf(JocCafOutput::new_for_canonical_layout(
                output, format, overwrite, layout,
            )?)),
        }
    }

    pub fn write_block(&mut self, block: &RenderedBlock) -> Result<(), JocRenderError> {
        match self {
            Self::Wav(output) => output.write_block(block),
            Self::Caf(output) => output.write_block(block),
        }
    }

    pub fn finish(&mut self) -> Result<(), JocRenderError> {
        match self {
            Self::Wav(output) => output.finish(),
            Self::Caf(output) => output.finish(),
        }
    }

    pub fn abort(&mut self) {
        match self {
            Self::Wav(output) => output.abort(),
            Self::Caf(output) => output.abort(),
        }
    }

    pub fn frames(&self) -> u64 {
        match self {
            Self::Wav(output) => output.frames(),
            Self::Caf(output) => output.frames(),
        }
    }

    pub const fn container(&self) -> OutputContainer {
        match self {
            Self::Wav(_) => OutputContainer::Wav,
            Self::Caf(_) => OutputContainer::Caf,
        }
    }
}

const CAF_LABEL_USE_COORDINATES: u32 = 100;
const CAF_FLAG_RECTANGULAR_COORDINATES: u32 = 1;
const TOP_MIDDLE_X_LEFT: f64 = 7_928.0 / 32_768.0;
const TOP_MIDDLE_X_RIGHT: f64 = 24_840.0 / 32_768.0;
const TOP_MIDDLE_Z: f64 = 32_767.0 / 32_768.0;
const OPENJOC_QMAX: f64 = 32_767.0 / 32_768.0;

pub(super) fn caf_description(
    layout: &SemanticChannelLayout,
    label: &str,
) -> Result<CafChannelDescription, JocRenderError> {
    let description = match label {
        "FL" | "front-left" | "Left" => caf_label(1),
        "FR" | "front-right" | "Right" => caf_label(2),
        "FC" | "front-center" | "Center" => caf_label(3),
        "LFE" | "LFE1" | "low-frequency" => caf_label(4),
        "LFE2" => caf_label(37),
        "Lb" | "BL" | "back-left" => caf_label(5),
        "Rb" | "BR" | "back-right" => caf_label(6),
        "FLc" => caf_label(7),
        "FRc" => caf_label(8),
        "BC" => caf_label(9),
        "Ls" | "SiL" | "SL" | "side-left" => caf_label(10),
        "Rs" | "SiR" | "SR" | "side-right" => caf_label(11),
        "TFL" | "Ltf" | "TpFL" | "top-front-left" => caf_label(13),
        "TFR" | "Rtf" | "TpFR" | "top-front-right" => caf_label(15),
        "TBL" | "Ltr" | "TpBL" | "top-back-left" => caf_label(16),
        "TBR" | "Rtr" | "TpBR" | "top-back-right" => caf_label(18),
        "TpFC" => caf_label(14),
        "TpC" => caf_label(12),
        "TpBC" => caf_label(17),
        "TpSiL" => caf_22_2_coordinate(90.0, 37.5),
        "TpSiR" => caf_22_2_coordinate(-90.0, 37.5),
        "BtFC" => caf_22_2_coordinate(0.0, -22.5),
        "BtFL" => caf_22_2_coordinate(52.5, -22.5),
        "BtFR" => caf_22_2_coordinate(-52.5, -22.5),
        "Lw" | "left-wide" => caf_label(35),
        "Rw" | "right-wide" => caf_label(36),
        "Ltm" => caf_coordinate(TOP_MIDDLE_X_LEFT),
        "Rtm" => caf_coordinate(TOP_MIDDLE_X_RIGHT),
        _ => {
            return Err(JocRenderError::UnsupportedCafSpeaker {
                layout: layout.name.clone(),
                label: label.to_owned(),
            });
        }
    };
    Ok(description)
}

fn caf_description_at(
    layout: &SemanticChannelLayout,
    _index: usize,
    label: &str,
) -> Result<CafChannelDescription, JocRenderError> {
    caf_description(layout, label)
}

pub(super) fn caf_description_at_canonical(
    layout: &SpeakerLayout,
    index: usize,
    label: &str,
) -> Result<CafChannelDescription, JocRenderError> {
    if layout.lfe_indices().contains(&index) {
        let lfe_ordinal = layout
            .lfe_indices()
            .iter()
            .position(|candidate| *candidate == index)
            .unwrap_or(0);
        return Ok(caf_label(if lfe_ordinal == 0 { 4 } else { 37 }));
    }
    if let Some(coordinates) = layout.channel_coordinates().get(index) {
        return Ok(caf_coordinate_xyz(
            f64::from(coordinates[0]) * 2.0 - 1.0,
            1.0 - f64::from(coordinates[1]) * 2.0,
            f64::from(coordinates[2]) / OPENJOC_QMAX,
        ));
    }
    caf_description(&layout.semantic_channel_layout(), label)
}

const fn caf_label(label: u32) -> CafChannelDescription {
    CafChannelDescription {
        label,
        flags: 0,
        coordinates: [0.0; 3],
    }
}

fn caf_coordinate(openjoc_x: f64) -> CafChannelDescription {
    CafChannelDescription {
        label: CAF_LABEL_USE_COORDINATES,
        flags: CAF_FLAG_RECTANGULAR_COORDINATES,
        coordinates: [(2.0 * openjoc_x - 1.0) as f32, 0.0, TOP_MIDDLE_Z as f32],
    }
}

fn caf_22_2_coordinate(azimuth_degrees: f64, elevation_degrees: f64) -> CafChannelDescription {
    let azimuth = azimuth_degrees.to_radians();
    let elevation = elevation_degrees.to_radians();
    caf_coordinate_xyz(
        -azimuth.sin() * elevation.cos(),
        azimuth.cos() * elevation.cos(),
        elevation.sin(),
    )
}

fn caf_coordinate_xyz(x: f64, y: f64, z: f64) -> CafChannelDescription {
    CafChannelDescription {
        label: CAF_LABEL_USE_COORDINATES,
        flags: CAF_FLAG_RECTANGULAR_COORDINATES,
        coordinates: [x as f32, y as f32, z as f32],
    }
}
