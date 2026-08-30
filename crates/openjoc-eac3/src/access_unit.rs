// pattern: Functional Core

//! TS 103 420 E-AC-3 access-unit audio assembly.

use crate::{
    AccessUnitIndex, AudioPcmSynthesizer, BitstreamInformation, DecodedAudioPcm, DialnormMode,
    DialnormState, DownmixMetadata, Eac3DecodeStageTiming, Eac3Error, InternalBasePolicy,
    StreamType, SyncframeIndexEntry,
    audio_block::{DynamicRangeOverride, decode_audio_frame_pcm_with_policy_override_and_timing},
    inspect_audio_block_carriers, parse_audio_frame, parse_bsi,
};
use std::time::Instant;

/// Channel-major PCM and timing emitted by one JOC elementary-stream access unit.
///
/// Full-bandwidth channels are ordered as TS 103 420 Table 47: L, R, C, Ls,
/// Rs, followed by the optional 7.X or 5.X+2 pair. LFE is retained separately
/// because the JOC tool bypasses it.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedAccessUnitPcm {
    pub sample_rate: u32,
    pub samples: u16,
    /// Canonical logical location for each entry in [`Self::channels`].
    pub channel_locations: Vec<ChannelLocation>,
    pub channels: Vec<Vec<f64>>,
    /// Logical LFE location retained in [`Self::lfe`], when present.
    pub lfe_location: Option<ChannelLocation>,
    pub lfe: Option<Vec<f64>>,
    /// E-AC-3 mixing metadata owned by this decoded programme frame.
    pub downmix: DownmixMetadata,
    /// One calibrated program scalar prepared from the independent BSI.
    pub dialnorm: DialnormState,
}

/// The two distinct PCM meanings carried by one decoded JOC access unit.
///
/// `compatibility_pcm` is decoded only from the independent I0 presentation.
/// `joc_input_pcm` is the Table-47 reconstruction input assembled from I0 and
/// the optional D0. Keeping them as separately owned values prevents a 7.X
/// reconstruction input from being mistaken for a compatibility downmix.
#[derive(Clone, Debug, PartialEq)]
#[doc(hidden)]
pub struct DecodedJocAccessUnitPcm {
    pub compatibility_pcm: DecodedAccessUnitPcm,
    pub joc_input_pcm: DecodedAccessUnitPcm,
}

impl DecodedAccessUnitPcm {
    /// Returns a copy with the frame's prepared dialnorm scalar applied to
    /// every decoded Base plane, including the retained LFE plane.
    ///
    /// ReconstructionBasis rows are owned by the JOC frame and are scaled by
    /// the shared renderer beside this copy so the complete program receives
    /// one common scalar exactly once.
    #[must_use]
    pub fn with_dialnorm_applied(&self) -> Self {
        let mut calibrated = self.clone();
        for channel in &mut calibrated.channels {
            self.dialnorm.apply_to_samples(channel);
        }
        if let Some(lfe) = calibrated.lfe.as_mut() {
            self.dialnorm.apply_to_samples(lfe);
        }
        calibrated
    }

    /// Validates the channel sets admitted by TS 103 420 Table 47.
    ///
    /// The lower-level E-AC-3 assembler intentionally remains able to
    /// represent every public Table E.1.4 location for diagnostics. The
    /// complete JOC payload path calls this stricter admission boundary.
    ///
    /// # Errors
    /// Returns [`Eac3Error::UnsupportedJocChannelTopology`] for a channel set
    /// outside 5.X, 7.X, or 5.X+2, including an LFE2-only presentation.
    pub fn validate_joc_topology(&self) -> Result<(), Eac3Error> {
        const FIVE: &[ChannelLocation] = &[
            ChannelLocation::Left,
            ChannelLocation::Right,
            ChannelLocation::Centre,
            ChannelLocation::LeftSurround,
            ChannelLocation::RightSurround,
        ];
        const SEVEN_REAR: &[ChannelLocation] = &[
            ChannelLocation::Left,
            ChannelLocation::Right,
            ChannelLocation::Centre,
            ChannelLocation::LeftSurround,
            ChannelLocation::RightSurround,
            ChannelLocation::LeftBack,
            ChannelLocation::RightBack,
        ];
        const SEVEN_HEIGHT: &[ChannelLocation] = &[
            ChannelLocation::Left,
            ChannelLocation::Right,
            ChannelLocation::Centre,
            ChannelLocation::LeftSurround,
            ChannelLocation::RightSurround,
            ChannelLocation::TopFrontLeft,
            ChannelLocation::TopFrontRight,
        ];
        let full_band_valid = matches!(
            self.channel_locations.as_slice(),
            FIVE | SEVEN_REAR | SEVEN_HEIGHT
        );
        let lfe_valid = self
            .lfe_location
            .is_none_or(|location| location == ChannelLocation::Lfe(0))
            && self.lfe.is_some() == self.lfe_location.is_some();
        if !full_band_valid || !lfe_valid || self.channel_locations.len() != self.channels.len() {
            return Err(Eac3Error::UnsupportedJocChannelTopology {
                full_band_channels: self.channel_locations.len(),
                lfe_present: self.lfe.is_some(),
            });
        }
        Ok(())
    }

    /// Binds the standards-defined flat-7.X JOC identity to its exact rear
    /// Table-47 input topology. Other valid JOC downmix indices retain their
    /// existing count-based decoder behavior and are not labeled flat-7.X.
    ///
    /// # Errors
    /// Returns [`Eac3Error::UnsupportedJocChannelTopology`] when index 1 is
    /// paired with anything other than `L R C Ls Rs Lrs Rrs`, or when that
    /// rear topology is paired with a different JOC index.
    pub fn validate_joc_downmix_topology(&self, downmix_index: u8) -> Result<(), Eac3Error> {
        let rear_seven = self.has_flat7x_rear_topology();
        if (downmix_index == 1 && !rear_seven) || (rear_seven && downmix_index != 1) {
            return Err(Eac3Error::UnsupportedJocChannelTopology {
                full_band_channels: self.channel_locations.len(),
                lfe_present: self.lfe.is_some(),
            });
        }
        Ok(())
    }

    /// Whether this PCM plane has the exact standards-defined idx=1 flat-7.X
    /// identity and rear-channel ordering.
    #[must_use]
    pub fn is_standard_flat7x_joc_input(&self, downmix_index: u8) -> bool {
        downmix_index == 1 && self.has_flat7x_rear_topology()
    }

    fn has_flat7x_rear_topology(&self) -> bool {
        const FLAT_SEVEN: &[ChannelLocation] = &[
            ChannelLocation::Left,
            ChannelLocation::Right,
            ChannelLocation::Centre,
            ChannelLocation::LeftSurround,
            ChannelLocation::RightSurround,
            ChannelLocation::LeftBack,
            ChannelLocation::RightBack,
        ];
        self.channel_locations.as_slice() == FLAT_SEVEN
    }
}

/// Validates that a complete compressed AU is consumable by the native audio
/// frontend and that its assembled Table-47 topology matches the admitted JOC
/// header, without synthesizing PCM or creating renderer state.
#[doc(hidden)]
pub fn validate_joc_access_unit_decoder_contract(
    stream: &[u8],
    frames: &[SyncframeIndexEntry],
    unit: AccessUnitIndex,
    downmix_index: u8,
    joc_channel_count: u8,
) -> Result<(), Eac3Error> {
    let unit_end = unit
        .first_frame
        .checked_add(unit.frame_count)
        .ok_or(Eac3Error::InvalidAccessUnitRange)?;
    if unit.frame_count == 0 || unit_end > frames.len() || unit.frame_count > 2 {
        return Err(Eac3Error::InvalidAccessUnitRange);
    }
    let first = frames[unit.first_frame];
    if !matches!(
        first.header.stream_type,
        StreamType::LegacyIndependent | StreamType::Independent
    ) || first.header.substream_id != 0
    {
        return Err(Eac3Error::MissingIndependentSubstreamZero {
            frame: unit.first_frame,
        });
    }
    let dependent = (unit.frame_count == 2).then(|| frames[unit.first_frame + 1]);
    if dependent.is_some_and(|entry| {
        entry.header.stream_type != StreamType::Dependent || entry.header.substream_id != 0
    }) {
        return Err(Eac3Error::UnsupportedJocAccessUnitFrameCount {
            actual: unit.frame_count,
        });
    }
    if first.header.stream_type == StreamType::LegacyIndependent
        && (dependent.is_none()
            || !matches!(first.header.bitstream_id, 6 | 8)
            || first.header.sample_rate != 48_000)
    {
        return Err(Eac3Error::UnsupportedJocAccessUnitFrameCount {
            actual: unit.frame_count,
        });
    }

    let mut information = Vec::with_capacity(unit.frame_count);
    for (relative, entry) in frames[unit.first_frame..unit_end]
        .iter()
        .copied()
        .enumerate()
    {
        if entry.header.sample_rate != unit.sample_rate
            || entry.header.samples != unit.samples
            || entry.header.audio_blocks != 6
        {
            return Err(Eac3Error::SubstreamTimingMismatch {
                frame: unit.first_frame + relative,
            });
        }
        let end = entry
            .offset
            .checked_add(entry.header.frame_size)
            .ok_or(Eac3Error::FrameSizeOverflow)?;
        let bytes = stream
            .get(entry.offset..end)
            .ok_or(Eac3Error::TruncatedFrame {
                offset: entry.offset,
                declared: entry.header.frame_size,
                available: stream.len().saturating_sub(entry.offset),
            })?;
        let frame = parse_audio_frame(bytes)?;
        let report = inspect_audio_block_carriers(bytes, |_| {})?;
        if report.examined_blocks != usize::from(entry.header.audio_blocks)
            || report.unresolved_blocks != 0
        {
            return Err(Eac3Error::AudioBlockCarrierTraversalUnresolved {
                examined_blocks: report.examined_blocks,
                unresolved_blocks: report.unresolved_blocks,
            });
        }
        validate_channel_description(&frame.bsi, frame.full_bandwidth_channels)?;
        information.push(frame.bsi);
    }
    if first.header.stream_type == StreamType::LegacyIndependent
        && information[0].audio_coding_mode == 0
    {
        return Err(Eac3Error::UnsupportedAc3CodingTool {
            tool: "Annex-J dual-mono core",
        });
    }

    let (channel_locations, lfe_location) =
        assembled_channel_topology(&information[0], information.get(1))?;
    let topology = DecodedAccessUnitPcm {
        sample_rate: unit.sample_rate,
        samples: unit.samples,
        channels: vec![Vec::new(); channel_locations.len()],
        channel_locations,
        lfe: lfe_location.map(|_| Vec::new()),
        lfe_location,
        downmix: DownmixMetadata::default(),
        dialnorm: DialnormState::default(),
    };
    topology.validate_joc_topology()?;
    topology.validate_joc_downmix_topology(downmix_index)?;
    if topology.channel_locations.len() != usize::from(joc_channel_count) {
        return Err(Eac3Error::UnsupportedJocChannelTopology {
            full_band_channels: topology.channel_locations.len(),
            lfe_present: topology.lfe.is_some(),
        });
    }
    Ok(())
}

/// Logical channel location from TS 102 366 Table E.1.4.
///
/// OpenJOC keeps the table's paired locations distinct after expanding a
/// `chanmap` bit. `Other(n)` is a stable internal ordering identifier whose
/// exact public table label is returned by [`Self::label`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ChannelLocation {
    Left,
    Right,
    Centre,
    LeftSurround,
    RightSurround,
    LeftBack,
    RightBack,
    TopFrontLeft,
    TopFrontRight,
    Other(u8),
    Lfe(u8),
}

impl ChannelLocation {
    /// Returns the exact short label used by the public channel-map table.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Left => "L",
            Self::Right => "R",
            Self::Centre => "C",
            Self::LeftSurround => "Ls",
            Self::RightSurround => "Rs",
            Self::LeftBack => "Lrs",
            Self::RightBack => "Rrs",
            Self::TopFrontLeft => "Vhl",
            Self::TopFrontRight => "Vhr",
            Self::Other(1) => "Lc",
            Self::Other(2) => "Rc",
            Self::Other(3) => "Cs",
            Self::Other(4) => "Ts",
            Self::Other(5) => "Lsd",
            Self::Other(6) => "Rsd",
            Self::Other(7) => "Lw",
            Self::Other(8) => "Rw",
            Self::Other(9) => "Vhc",
            Self::Other(10) => "Lts",
            Self::Other(11) => "Rts",
            Self::Other(_) => "unknown",
            Self::Lfe(0) => "LFE",
            Self::Lfe(1) => "LFE2",
            Self::Lfe(_) => "unknown-LFE",
        }
    }
}

/// Stateful decoder for the one-I0/optional-D0 JOC elementary-stream shape.
///
/// TS 103 420 E.3 restricts a conforming JOC elementary stream to one
/// independent substream (I0) and at most one dependent substream (D0). The
/// dependent channel data replaces matching I0 locations and supplements the
/// base 5.X channels for the 7.X and 5.X+2 configurations. Transform delay is
/// retained independently for I0 and D0 across access units.
#[derive(Clone, Debug, Default)]
pub struct JocAccessUnitPcmDecoder {
    independent: AudioPcmSynthesizer,
    dependent: AudioPcmSynthesizer,
    dependent_present: bool,
    independent_configuration: Option<SubstreamPcmConfiguration>,
    dependent_configuration: Option<SubstreamPcmConfiguration>,
    dialnorm_mode: DialnormMode,
    dialnorm_state: DialnormState,
    stage_timing_enabled: bool,
    last_stage_timing: Eac3DecodeStageTiming,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SubstreamPcmConfiguration {
    stream_type: StreamType,
    bitstream_id: u8,
    bitstream_mode: Option<u8>,
    sample_rate: u32,
    audio_coding_mode: u8,
    lfe_on: bool,
    channel_map: Option<u16>,
}

impl From<&BitstreamInformation> for SubstreamPcmConfiguration {
    fn from(info: &BitstreamInformation) -> Self {
        Self {
            stream_type: info.header.stream_type,
            bitstream_id: info.bitstream_id,
            bitstream_mode: info.bitstream_mode,
            sample_rate: info.header.sample_rate,
            audio_coding_mode: info.audio_coding_mode,
            lfe_on: info.lfe_on,
            channel_map: info.channel_map,
        }
    }
}

impl JocAccessUnitPcmDecoder {
    /// Creates a decoder with zero TDAC history.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears all substream TDAC history.
    pub fn reset(&mut self) {
        let stage_timing_enabled = self.stage_timing_enabled;
        *self = Self::default();
        self.stage_timing_enabled = stage_timing_enabled;
    }

    /// Selects the internal dialnorm branch for conformance tests.
    ///
    /// The public OpenJocSession intentionally keeps the automatic Default
    /// policy and does not expose this as a user configuration option.
    pub fn set_dialnorm_mode(&mut self, mode: DialnormMode) {
        self.dialnorm_mode = mode;
        self.dialnorm_state = DialnormState::new(mode, self.dialnorm_state.encoded_value());
    }

    /// Returns the currently committed semantic dialnorm state.
    #[must_use]
    pub const fn dialnorm_state(&self) -> DialnormState {
        self.dialnorm_state
    }

    /// Enables opt-in core stage timing. Normal decoding performs no timing
    /// clock reads unless this diagnostic mode is selected.
    pub fn enable_stage_timing(&mut self) {
        self.stage_timing_enabled = true;
        self.last_stage_timing = Eac3DecodeStageTiming::default();
    }

    /// Takes the most recent successful access-unit timing record.
    pub fn take_stage_timing(&mut self) -> Eac3DecodeStageTiming {
        std::mem::take(&mut self.last_stage_timing)
    }

    /// Decodes and assembles one indexed JOC access unit.
    ///
    /// `dither_values` is a deterministic sequence supplied by the caller and
    /// is reused from the start for each source substream. This keeps the
    /// normative decoder boundary pure while permitting any ETSI-allowed
    /// random dither sequence to be injected by an application.
    ///
    /// # Errors
    /// Returns a checked indexing, channel-map, audio-block, transform, or
    /// access-unit PCM alignment error. Decoder state is committed only after
    /// both source frames and the channel merge succeed.
    pub fn decode(
        &mut self,
        stream: &[u8],
        frames: &[SyncframeIndexEntry],
        unit: AccessUnitIndex,
        dither_values: &[f64],
    ) -> Result<DecodedAccessUnitPcm, Eac3Error> {
        self.decode_with_policy(
            stream,
            frames,
            unit,
            dither_values,
            InternalBasePolicy::CurrentDefault,
        )
    }

    /// Decodes and assembles one access unit with an explicit internal-base
    /// presentation policy. The default [`Self::decode`] behavior is kept
    /// unchanged for existing callers.
    pub fn decode_with_policy(
        &mut self,
        stream: &[u8],
        frames: &[SyncframeIndexEntry],
        unit: AccessUnitIndex,
        dither_values: &[f64],
        policy: InternalBasePolicy,
    ) -> Result<DecodedAccessUnitPcm, Eac3Error> {
        self.decode_pcm_planes_with_policy(stream, frames, unit, dither_values, policy)
            .map(|planes| planes.joc_input_pcm)
    }

    /// Decodes one access unit while retaining the independent compatibility
    /// presentation separately from the assembled JOC reconstruction input.
    ///
    /// # Errors
    /// Returns the same checked decode and assembly failures as
    /// [`Self::decode_with_policy`].
    #[doc(hidden)]
    pub fn decode_pcm_planes_with_policy(
        &mut self,
        stream: &[u8],
        frames: &[SyncframeIndexEntry],
        unit: AccessUnitIndex,
        dither_values: &[f64],
        policy: InternalBasePolicy,
    ) -> Result<DecodedJocAccessUnitPcm, Eac3Error> {
        let total_start = self.stage_timing_enabled.then(Instant::now);
        let mut stage_timing = self
            .stage_timing_enabled
            .then(Eac3DecodeStageTiming::default);
        if self.stage_timing_enabled {
            self.last_stage_timing = Eac3DecodeStageTiming::default();
        }
        let unit_end = unit
            .first_frame
            .checked_add(unit.frame_count)
            .ok_or(Eac3Error::InvalidAccessUnitRange)?;
        if unit.frame_count == 0 || unit_end > frames.len() {
            return Err(Eac3Error::InvalidAccessUnitRange);
        }
        let first = frames[unit.first_frame];
        if !matches!(
            first.header.stream_type,
            StreamType::LegacyIndependent | StreamType::Independent
        ) || first.header.substream_id != 0
        {
            return Err(Eac3Error::MissingIndependentSubstreamZero {
                frame: unit.first_frame,
            });
        }
        if unit.frame_count > 2 {
            return Err(Eac3Error::UnsupportedJocAccessUnitFrameCount {
                actual: unit.frame_count,
            });
        }
        let dependent_entry = if unit.frame_count == 2 {
            let entry = frames[unit.first_frame + 1];
            if entry.header.stream_type != StreamType::Dependent || entry.header.substream_id != 0 {
                return Err(Eac3Error::UnsupportedJocAccessUnitFrameCount {
                    actual: unit.frame_count,
                });
            }
            Some(entry)
        } else {
            None
        };
        if first.header.stream_type == StreamType::LegacyIndependent
            && (dependent_entry.is_none()
                || !matches!(first.header.bitstream_id, 6 | 8)
                || first.header.sample_rate != 48_000)
        {
            return Err(Eac3Error::UnsupportedJocAccessUnitFrameCount {
                actual: unit.frame_count,
            });
        }
        if first.header.sample_rate != unit.sample_rate || first.header.samples != unit.samples {
            return Err(Eac3Error::SubstreamTimingMismatch {
                frame: unit.first_frame,
            });
        }
        if first.header.audio_blocks != 6 {
            return Err(Eac3Error::UnsupportedJocAudioBlockCount {
                actual: first.header.audio_blocks,
            });
        }

        let allocation_start = stage_timing.is_some().then(Instant::now);
        let mut independent_synth = self.independent.clone();
        let mut dependent_synth = self.dependent.clone();
        if let (Some(timing), Some(start)) = (stage_timing.as_mut(), allocation_start) {
            timing.allocation_and_copy += start.elapsed();
        }
        if dependent_entry.is_some() != self.dependent_present {
            dependent_synth.reset();
        }
        let dependent_drc = dependent_entry
            .map(|entry| dependent_dynamic_range_override(stream, entry))
            .transpose()?;
        let (independent_info, independent, independent_configuration) = decode_frame(
            stream,
            first,
            dither_values,
            &mut independent_synth,
            self.independent_configuration,
            policy,
            dependent_drc.as_ref(),
            stage_timing.as_mut(),
        )?;
        if first.header.stream_type == StreamType::LegacyIndependent
            && independent_info.audio_coding_mode == 0
        {
            return Err(Eac3Error::UnsupportedAc3CodingTool {
                tool: "Annex-J dual-mono core",
            });
        }
        let dependent = dependent_entry
            .map(|entry| {
                decode_frame(
                    stream,
                    entry,
                    dither_values,
                    &mut dependent_synth,
                    self.dependent_configuration,
                    policy,
                    None,
                    stage_timing.as_mut(),
                )
            })
            .transpose()?;
        if let Some((info, _, _)) = &dependent {
            if info.header.sample_rate != unit.sample_rate || info.header.samples != unit.samples {
                return Err(Eac3Error::SubstreamTimingMismatch {
                    frame: unit.first_frame + 1,
                });
            }
        }
        if let Some((info, _, _)) = &dependent {
            if info.header.audio_blocks != 6 {
                return Err(Eac3Error::UnsupportedJocAudioBlockCount {
                    actual: info.header.audio_blocks,
                });
            }
        }
        let dialnorm = DialnormState::new(self.dialnorm_mode, independent_info.dialnorm);
        let output = Eac3DecodeStageTiming::measure(
            stage_timing.as_mut(),
            |timing| &mut timing.pcm_assembly,
            || {
                merge_substream_pcm_planes(
                    unit,
                    &independent_info,
                    independent,
                    dependent.as_ref().map(|(info, pcm, _)| (info, pcm)),
                    dialnorm,
                )
            },
        )?;

        let commit_start = stage_timing.is_some().then(Instant::now);
        self.independent = independent_synth;
        self.dependent = dependent_synth;
        self.dependent_present = dependent_entry.is_some();
        self.independent_configuration = Some(independent_configuration);
        self.dependent_configuration = dependent.map(|(_, _, configuration)| configuration);
        self.dialnorm_state = dialnorm;
        if let (Some(timing), Some(start)) = (stage_timing.as_mut(), commit_start) {
            timing.decoder_state_commit += start.elapsed();
        }
        if let (Some(mut timing), Some(start)) = (stage_timing, total_start) {
            timing.total = start.elapsed();
            self.last_stage_timing = timing;
        }
        Ok(output)
    }
}

fn decode_frame(
    stream: &[u8],
    entry: SyncframeIndexEntry,
    dither_values: &[f64],
    synthesizer: &mut AudioPcmSynthesizer,
    previous_configuration: Option<SubstreamPcmConfiguration>,
    policy: InternalBasePolicy,
    drc_override: Option<&DynamicRangeOverride>,
    mut timing: Option<&mut Eac3DecodeStageTiming>,
) -> Result<
    (
        BitstreamInformation,
        DecodedAudioPcm,
        SubstreamPcmConfiguration,
    ),
    Eac3Error,
> {
    let end = entry
        .offset
        .checked_add(entry.header.frame_size)
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    let bytes = stream
        .get(entry.offset..end)
        .ok_or(Eac3Error::TruncatedFrame {
            offset: entry.offset,
            declared: entry.header.frame_size,
            available: stream.len().saturating_sub(entry.offset),
        })?;
    let info = Eac3DecodeStageTiming::measure(
        timing.as_deref_mut(),
        |timing| &mut timing.syncframe_and_header_parsing,
        || crate::parse_audio_frame(bytes),
    )?
    .bsi;
    let configuration = SubstreamPcmConfiguration::from(&info);
    if previous_configuration.is_some_and(|previous| previous != configuration) {
        synthesizer.reset();
    }
    let pcm = decode_audio_frame_pcm_with_policy_override_and_timing(
        bytes,
        dither_values,
        synthesizer,
        policy,
        drc_override,
        timing,
    )?;
    Ok((info, pcm, configuration))
}

fn dependent_dynamic_range_override(
    stream: &[u8],
    entry: SyncframeIndexEntry,
) -> Result<DynamicRangeOverride, Eac3Error> {
    let end = entry
        .offset
        .checked_add(entry.header.frame_size)
        .ok_or(Eac3Error::FrameSizeOverflow)?;
    let bytes = stream
        .get(entry.offset..end)
        .ok_or(Eac3Error::TruncatedFrame {
            offset: entry.offset,
            declared: entry.header.frame_size,
            available: stream.len().saturating_sub(entry.offset),
        })?;
    let info = parse_bsi(bytes)?;
    let mut primary = Vec::with_capacity(usize::from(entry.header.audio_blocks));
    let mut secondary = Vec::new();
    let report = inspect_audio_block_carriers(bytes, |carrier| {
        if let Some(code) = carrier.dynamic_range {
            primary.push(code);
        }
        if let Some(code) = carrier.dynamic_range_2 {
            secondary.push(code);
        }
    })?;
    if report.unresolved_blocks != 0 {
        return Err(Eac3Error::AudioBlockCarrierTraversalUnresolved {
            examined_blocks: report.examined_blocks,
            unresolved_blocks: report.unresolved_blocks,
        });
    }
    Ok(DynamicRangeOverride {
        primary,
        secondary,
        compr: info.compr,
        compr_2: info.compr_2,
    })
}

fn standard_channel_locations(
    audio_coding_mode: u8,
    lfe_on: bool,
) -> Result<Vec<ChannelLocation>, Eac3Error> {
    let mut locations = match audio_coding_mode {
        0 => vec![ChannelLocation::Left, ChannelLocation::Right],
        1 => vec![ChannelLocation::Centre],
        2 => vec![ChannelLocation::Left, ChannelLocation::Right],
        3 => vec![
            ChannelLocation::Left,
            ChannelLocation::Centre,
            ChannelLocation::Right,
        ],
        4 => vec![
            ChannelLocation::Left,
            ChannelLocation::Right,
            ChannelLocation::Other(3), // Cs
        ],
        5 => vec![
            ChannelLocation::Left,
            ChannelLocation::Centre,
            ChannelLocation::Right,
            ChannelLocation::Other(3), // Cs
        ],
        6 => vec![
            ChannelLocation::Left,
            ChannelLocation::Right,
            ChannelLocation::LeftSurround,
            ChannelLocation::RightSurround,
        ],
        7 => vec![
            ChannelLocation::Left,
            ChannelLocation::Centre,
            ChannelLocation::Right,
            ChannelLocation::LeftSurround,
            ChannelLocation::RightSurround,
        ],
        _ => return Err(Eac3Error::FrameSizeOverflow),
    };
    if lfe_on {
        locations.push(ChannelLocation::Lfe(0));
    }
    Ok(locations)
}

fn channel_locations(info: &BitstreamInformation) -> Result<Vec<ChannelLocation>, Eac3Error> {
    let Some(map) = info.channel_map else {
        return standard_channel_locations(info.audio_coding_mode, info.lfe_on);
    };
    let mut locations = Vec::new();
    for bit in 0..16_u8 {
        if map & (1_u16 << (15 - bit)) == 0 {
            continue;
        }
        locations.extend(match bit {
            0 => vec![ChannelLocation::Left],
            1 => vec![ChannelLocation::Centre],
            2 => vec![ChannelLocation::Right],
            3 => vec![ChannelLocation::LeftSurround],
            4 => vec![ChannelLocation::RightSurround],
            5 => vec![ChannelLocation::Other(1), ChannelLocation::Other(2)], // Lc/Rc
            6 => vec![ChannelLocation::LeftBack, ChannelLocation::RightBack],
            7 => vec![ChannelLocation::Other(3)], // Cs
            8 => vec![ChannelLocation::Other(4)], // Ts
            9 => vec![ChannelLocation::Other(5), ChannelLocation::Other(6)], // Lsd/Rsd
            10 => vec![ChannelLocation::Other(7), ChannelLocation::Other(8)], // Lw/Rw
            11 => vec![
                ChannelLocation::TopFrontLeft,
                ChannelLocation::TopFrontRight,
            ],
            12 => vec![ChannelLocation::Other(9)], // Vhc
            13 => vec![ChannelLocation::Other(10), ChannelLocation::Other(11)], // Lts/Rts
            14 => vec![ChannelLocation::Lfe(1)],
            15 => vec![ChannelLocation::Lfe(0)],
            _ => unreachable!(),
        });
    }
    Ok(locations)
}

fn validate_channel_description(
    info: &BitstreamInformation,
    full_bandwidth_channels: u8,
) -> Result<(), Eac3Error> {
    let locations = channel_locations(info)?;
    let full_band = locations
        .iter()
        .filter(|location| !matches!(location, ChannelLocation::Lfe(_)))
        .count();
    let lfe = locations
        .iter()
        .filter(|location| matches!(location, ChannelLocation::Lfe(_)))
        .count();
    if lfe > 1 {
        return Err(Eac3Error::MultipleLfeChannels);
    }
    if full_band != usize::from(full_bandwidth_channels) || lfe != usize::from(info.lfe_on) {
        return Err(Eac3Error::InvalidDependentChannelMap {
            expected: usize::from(full_bandwidth_channels) + usize::from(info.lfe_on),
            actual: locations.len(),
        });
    }
    Ok(())
}

fn assembled_channel_topology(
    independent: &BitstreamInformation,
    dependent: Option<&BitstreamInformation>,
) -> Result<(Vec<ChannelLocation>, Option<ChannelLocation>), Eac3Error> {
    let mut channels = Vec::new();
    let mut lfe_location = None;
    for info in std::iter::once(independent).chain(dependent) {
        for location in channel_locations(info)? {
            if matches!(location, ChannelLocation::Lfe(_)) {
                if lfe_location.is_some_and(|current| current != location) {
                    return Err(Eac3Error::MultipleLfeChannels);
                }
                lfe_location = Some(location);
            } else if !channels.contains(&location) {
                channels.push(location);
            }
        }
    }
    channels.sort_by_key(|location| location_rank(*location));
    Ok((channels, lfe_location))
}

#[cfg(test)]
fn merge_substreams(
    unit: AccessUnitIndex,
    independent_info: &BitstreamInformation,
    independent: DecodedAudioPcm,
    dependent: Option<(&BitstreamInformation, &DecodedAudioPcm)>,
) -> Result<DecodedAccessUnitPcm, Eac3Error> {
    merge_substreams_with_dialnorm(
        unit,
        independent_info,
        independent,
        dependent,
        DialnormState::default(),
    )
}

fn merge_substreams_with_dialnorm(
    unit: AccessUnitIndex,
    independent_info: &BitstreamInformation,
    independent: DecodedAudioPcm,
    dependent: Option<(&BitstreamInformation, &DecodedAudioPcm)>,
    dialnorm: DialnormState,
) -> Result<DecodedAccessUnitPcm, Eac3Error> {
    let mut channels = Vec::<(ChannelLocation, Vec<f64>)>::new();
    let independent_locations = channel_locations(independent_info)?;
    insert_channels(&mut channels, independent_locations, &independent)?;
    let mut lfe = independent.lfe.clone();
    let mut lfe_location = lfe_channel_location(independent_info)?;
    if let Some((info, pcm)) = dependent {
        let locations = channel_locations(info)?;
        insert_channels(&mut channels, locations, pcm)?;
        if let Some(dependent_lfe) = &pcm.lfe {
            let dependent_lfe_location =
                lfe_channel_location(info)?.ok_or(Eac3Error::InvalidDependentChannelMap {
                    expected: 1,
                    actual: 0,
                })?;
            if lfe_location.is_some_and(|current| current != dependent_lfe_location) {
                return Err(Eac3Error::MultipleLfeChannels);
            }
            lfe_location = Some(dependent_lfe_location);
            lfe = Some(dependent_lfe.clone());
        }
    }
    if let Some((_, mismatched)) = channels
        .iter()
        .find(|(_, pcm)| pcm.len() != usize::from(unit.samples))
    {
        return Err(Eac3Error::AccessUnitPcmSampleCountMismatch {
            expected: usize::from(unit.samples),
            actual: mismatched.len(),
        });
    }
    if lfe
        .as_ref()
        .is_some_and(|pcm| pcm.len() != usize::from(unit.samples))
    {
        return Err(Eac3Error::AccessUnitPcmSampleCountMismatch {
            expected: usize::from(unit.samples),
            actual: lfe.as_ref().map_or(0, Vec::len),
        });
    }
    channels.sort_by_key(|(location, _)| location_rank(*location));
    let (channel_locations, channels) = channels.into_iter().unzip();
    Ok(DecodedAccessUnitPcm {
        sample_rate: unit.sample_rate,
        samples: unit.samples,
        channel_locations,
        channels,
        lfe_location,
        lfe,
        downmix: if independent_info.downmix == DownmixMetadata::default() {
            dependent.map_or_else(DownmixMetadata::default, |(info, _)| info.downmix)
        } else {
            independent_info.downmix
        },
        dialnorm,
    })
}

fn merge_substream_pcm_planes(
    unit: AccessUnitIndex,
    independent_info: &BitstreamInformation,
    independent: DecodedAudioPcm,
    dependent: Option<(&BitstreamInformation, &DecodedAudioPcm)>,
    dialnorm: DialnormState,
) -> Result<DecodedJocAccessUnitPcm, Eac3Error> {
    let mut compatibility_pcm = merge_substreams_with_dialnorm(
        unit,
        independent_info,
        independent.clone(),
        None,
        dialnorm,
    )?;
    let joc_input_pcm =
        merge_substreams_with_dialnorm(unit, independent_info, independent, dependent, dialnorm)?;
    // Mixing metadata remains programme-scoped: retain the established
    // independent-first, dependent-fallback selection even though the
    // compatibility audio samples themselves are strictly I0-only.
    compatibility_pcm.downmix = joc_input_pcm.downmix;
    Ok(DecodedJocAccessUnitPcm {
        compatibility_pcm,
        joc_input_pcm,
    })
}

fn lfe_channel_location(info: &BitstreamInformation) -> Result<Option<ChannelLocation>, Eac3Error> {
    let locations = channel_locations(info)?;
    let mut lfe_locations = locations
        .into_iter()
        .filter(|location| matches!(location, ChannelLocation::Lfe(_)));
    let first = lfe_locations.next();
    if lfe_locations.next().is_some() {
        return Err(Eac3Error::MultipleLfeChannels);
    }
    Ok(first)
}

fn insert_channels(
    target: &mut Vec<(ChannelLocation, Vec<f64>)>,
    locations: Vec<ChannelLocation>,
    pcm: &DecodedAudioPcm,
) -> Result<(), Eac3Error> {
    let full_locations = locations
        .iter()
        .filter(|location| !matches!(location, ChannelLocation::Lfe(_)))
        .count();
    let lfe_locations = locations
        .iter()
        .filter(|location| matches!(location, ChannelLocation::Lfe(_)))
        .count();
    if lfe_locations > 1 {
        return Err(Eac3Error::MultipleLfeChannels);
    }
    if full_locations != pcm.channels.len() || lfe_locations != usize::from(pcm.lfe.is_some()) {
        return Err(Eac3Error::InvalidDependentChannelMap {
            expected: locations.len(),
            actual: pcm.channels.len() + usize::from(pcm.lfe.is_some()),
        });
    }
    let mut channel_index = 0;
    for location in locations {
        if matches!(location, ChannelLocation::Lfe(_)) {
            continue;
        }
        let data = pcm.channels[channel_index].clone();
        channel_index += 1;
        if let Some(existing) = target.iter_mut().find(|(current, _)| *current == location) {
            existing.1 = data;
        } else {
            target.push((location, data));
        }
    }
    Ok(())
}

fn location_rank(location: ChannelLocation) -> (u8, u8) {
    match location {
        ChannelLocation::Left => (0, 0),
        ChannelLocation::Right => (1, 0),
        ChannelLocation::Centre => (2, 0),
        ChannelLocation::LeftSurround => (3, 0),
        ChannelLocation::RightSurround => (4, 0),
        ChannelLocation::LeftBack => (5, 0),
        ChannelLocation::RightBack => (6, 0),
        ChannelLocation::TopFrontLeft => (7, 0),
        ChannelLocation::TopFrontRight => (8, 0),
        ChannelLocation::Other(value) => (9, value),
        ChannelLocation::Lfe(value) => (10, value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(audio_coding_mode: u8, channel_map: Option<u16>) -> BitstreamInformation {
        info_with(audio_coding_mode, false, channel_map)
    }

    fn info_with(
        audio_coding_mode: u8,
        lfe_on: bool,
        channel_map: Option<u16>,
    ) -> BitstreamInformation {
        BitstreamInformation {
            header: crate::SyncframeHeader {
                stream_type: StreamType::Independent,
                substream_id: 0,
                bitstream_id: 16,
                frame_size: 0,
                sample_rate: 48_000,
                audio_blocks: 1,
                samples: 1,
            },
            bitstream_mode: None,
            audio_coding_mode,
            lfe_on,
            bitstream_id: 16,
            dialnorm: 31,
            dialnorm_2: None,
            compr: None,
            compr_2: None,
            downmix: DownmixMetadata::default(),
            channel_map,
            addbsi: None,
        }
    }

    fn independent_channel_map_oracle(map: u16) -> Vec<ChannelLocation> {
        let mut locations = Vec::new();
        for bit in 0..16_u8 {
            if map & (1_u16 << (15 - bit)) == 0 {
                continue;
            }
            match bit {
                0 => locations.push(ChannelLocation::Left),
                1 => locations.push(ChannelLocation::Centre),
                2 => locations.push(ChannelLocation::Right),
                3 => locations.push(ChannelLocation::LeftSurround),
                4 => locations.push(ChannelLocation::RightSurround),
                5 => locations.extend([ChannelLocation::Other(1), ChannelLocation::Other(2)]),
                6 => locations.extend([ChannelLocation::LeftBack, ChannelLocation::RightBack]),
                7 => locations.push(ChannelLocation::Other(3)),
                8 => locations.push(ChannelLocation::Other(4)),
                9 => locations.extend([ChannelLocation::Other(5), ChannelLocation::Other(6)]),
                10 => locations.extend([ChannelLocation::Other(7), ChannelLocation::Other(8)]),
                11 => locations.extend([
                    ChannelLocation::TopFrontLeft,
                    ChannelLocation::TopFrontRight,
                ]),
                12 => locations.push(ChannelLocation::Other(9)),
                13 => locations.extend([ChannelLocation::Other(10), ChannelLocation::Other(11)]),
                14 => locations.push(ChannelLocation::Lfe(1)),
                15 => locations.push(ChannelLocation::Lfe(0)),
                _ => unreachable!(),
            }
        }
        locations
    }

    #[test]
    fn every_chanmap_value_matches_an_independent_table_e_1_4_transcription() {
        for map in 0..=u16::MAX {
            assert_eq!(
                channel_locations(&info(1, Some(map))).expect("bounded channel map"),
                independent_channel_map_oracle(map),
                "chanmap {map:#06x}"
            );
        }
    }

    #[test]
    fn dependent_pair_supplements_the_independent_five_channel_order() {
        let independent = DecodedAudioPcm {
            channels: vec![vec![1.0], vec![2.0], vec![3.0], vec![4.0], vec![5.0]],
            lfe: None,
        };
        let dependent = DecodedAudioPcm {
            channels: vec![vec![6.0], vec![7.0]],
            lfe: None,
        };
        let dependent_info = info(2, Some(1 << 9)); // custom bit 6: Lb/Rb
        let output = merge_substreams(
            AccessUnitIndex {
                first_frame: 0,
                frame_count: 2,
                sample_rate: 48_000,
                samples: 1,
            },
            &info(7, None),
            independent,
            Some((&dependent_info, &dependent)),
        )
        .expect("valid 7.X channel merge");
        assert_eq!(
            output.channels,
            vec![
                vec![1.0],
                vec![3.0],
                vec![2.0],
                vec![4.0],
                vec![5.0],
                vec![6.0],
                vec![7.0]
            ]
        );
        output.validate_joc_topology().expect("Table 47 7.X");
    }

    #[test]
    fn dual_plane_assembly_preserves_i0_compatibility_and_seven_input_joc_ownership() {
        let independent = DecodedAudioPcm {
            channels: vec![vec![1.0], vec![2.0], vec![3.0], vec![4.0], vec![5.0]],
            lfe: Some(vec![8.0]),
        };
        let dependent = DecodedAudioPcm {
            channels: vec![vec![6.0], vec![7.0]],
            lfe: None,
        };
        let mut dependent_info = info(2, Some(1 << 9));
        dependent_info.downmix.dmixmod = Some(1);
        let planes = merge_substream_pcm_planes(
            AccessUnitIndex {
                first_frame: 0,
                frame_count: 2,
                sample_rate: 48_000,
                samples: 1,
            },
            &info_with(7, true, None),
            independent,
            Some((&dependent_info, &dependent)),
            DialnormState::default(),
        )
        .expect("valid dual-plane 7.X assembly");

        assert_eq!(
            planes.compatibility_pcm.channel_locations,
            vec![
                ChannelLocation::Left,
                ChannelLocation::Right,
                ChannelLocation::Centre,
                ChannelLocation::LeftSurround,
                ChannelLocation::RightSurround,
            ]
        );
        assert_eq!(
            planes.compatibility_pcm.channels,
            vec![vec![1.0], vec![3.0], vec![2.0], vec![4.0], vec![5.0]]
        );
        assert_eq!(
            planes.joc_input_pcm.channel_locations,
            vec![
                ChannelLocation::Left,
                ChannelLocation::Right,
                ChannelLocation::Centre,
                ChannelLocation::LeftSurround,
                ChannelLocation::RightSurround,
                ChannelLocation::LeftBack,
                ChannelLocation::RightBack,
            ]
        );
        assert_eq!(planes.joc_input_pcm.channels[5], vec![6.0]);
        assert_eq!(planes.joc_input_pcm.channels[6], vec![7.0]);
        assert_eq!(planes.compatibility_pcm.downmix.dmixmod, Some(1));
        assert_ne!(
            planes.compatibility_pcm.channels[0].as_ptr(),
            planes.joc_input_pcm.channels[0].as_ptr(),
            "the two semantic planes must not alias storage"
        );
        planes
            .joc_input_pcm
            .validate_joc_topology()
            .expect("Table 47 7.X");
    }

    #[test]
    fn dependent_standard_channels_replace_matching_independent_channels() {
        let independent = DecodedAudioPcm {
            channels: vec![vec![1.0], vec![2.0], vec![3.0], vec![4.0], vec![5.0]],
            lfe: None,
        };
        let dependent = DecodedAudioPcm {
            channels: vec![vec![9.0], vec![8.0]],
            lfe: None,
        };
        let dependent_info = info(2, None);
        let output = merge_substreams(
            AccessUnitIndex {
                first_frame: 0,
                frame_count: 2,
                sample_rate: 48_000,
                samples: 1,
            },
            &info(7, None),
            independent,
            Some((&dependent_info, &dependent)),
        )
        .expect("valid replacement merge");
        assert_eq!(
            output.channels,
            vec![vec![9.0], vec![8.0], vec![2.0], vec![4.0], vec![5.0]]
        );
    }

    #[test]
    fn custom_map_replaces_left_and_supplements_rear_pair_in_canonical_order() {
        let independent = DecodedAudioPcm {
            channels: vec![vec![1.0], vec![2.0], vec![3.0], vec![4.0], vec![5.0]],
            lfe: None,
        };
        let dependent = DecodedAudioPcm {
            channels: vec![vec![10.0], vec![20.0], vec![21.0]],
            lfe: None,
        };
        let output = merge_substreams(
            AccessUnitIndex {
                first_frame: 0,
                frame_count: 2,
                sample_rate: 48_000,
                samples: 1,
            },
            &info(7, None),
            independent,
            Some((&info(3, Some(0x8200)), &dependent)),
        )
        .expect("replacement plus Lrs/Rrs supplement");
        assert_eq!(
            output.channels,
            vec![
                vec![10.0],
                vec![3.0],
                vec![2.0],
                vec![4.0],
                vec![5.0],
                vec![20.0],
                vec![21.0]
            ]
        );
        output.validate_joc_topology().expect("Table 47 7.X");
    }

    #[test]
    fn custom_map_supplements_the_height_pair_in_canonical_order() {
        let independent = DecodedAudioPcm {
            channels: vec![vec![1.0], vec![2.0], vec![3.0], vec![4.0], vec![5.0]],
            lfe: None,
        };
        let dependent = DecodedAudioPcm {
            channels: vec![vec![30.0], vec![31.0]],
            lfe: None,
        };
        let output = merge_substreams(
            AccessUnitIndex {
                first_frame: 0,
                frame_count: 2,
                sample_rate: 48_000,
                samples: 1,
            },
            &info(7, None),
            independent,
            Some((&info(2, Some(0x0010)), &dependent)),
        )
        .expect("Vhl/Vhr supplement");
        assert_eq!(
            output.channels,
            vec![
                vec![1.0],
                vec![3.0],
                vec![2.0],
                vec![4.0],
                vec![5.0],
                vec![30.0],
                vec![31.0]
            ]
        );
        output.validate_joc_topology().expect("Table 47 5.X+2");
    }

    #[test]
    fn dependent_custom_map_replaces_centre_and_lfe_without_touching_other_channels() {
        let independent = DecodedAudioPcm {
            channels: vec![vec![1.0], vec![2.0], vec![3.0], vec![4.0], vec![5.0]],
            lfe: Some(vec![6.0]),
        };
        let dependent = DecodedAudioPcm {
            channels: vec![vec![9.0]],
            lfe: Some(vec![99.0]),
        };
        let output = merge_substreams(
            AccessUnitIndex {
                first_frame: 0,
                frame_count: 2,
                sample_rate: 48_000,
                samples: 1,
            },
            &info_with(7, true, None),
            independent,
            Some((&info_with(1, true, Some(0x4001)), &dependent)),
        )
        .expect("centre and LFE replacement");
        assert_eq!(
            output.channels,
            vec![vec![1.0], vec![3.0], vec![9.0], vec![4.0], vec![5.0]]
        );
        assert_eq!(output.lfe, Some(vec![99.0]));
        output.validate_joc_topology().expect("Table 47 5.1");
    }

    #[test]
    fn sample_count_error_reports_the_actual_mismatched_channel() {
        let independent = DecodedAudioPcm {
            channels: vec![vec![1.0], vec![2.0]],
            lfe: None,
        };
        let dependent = DecodedAudioPcm {
            channels: vec![vec![9.0, 9.5]],
            lfe: None,
        };
        assert_eq!(
            merge_substreams(
                AccessUnitIndex {
                    first_frame: 0,
                    frame_count: 2,
                    sample_rate: 48_000,
                    samples: 1,
                },
                &info(2, None),
                independent,
                Some((&info(1, Some(0x4000)), &dependent)),
            ),
            Err(Eac3Error::AccessUnitPcmSampleCountMismatch {
                expected: 1,
                actual: 2,
            })
        );
    }

    #[test]
    fn two_lfe_locations_are_rejected_instead_of_silently_collapsing() {
        let independent = DecodedAudioPcm {
            channels: vec![vec![1.0]],
            lfe: None,
        };
        let dependent = DecodedAudioPcm {
            channels: Vec::new(),
            lfe: Some(vec![2.0]),
        };
        assert_eq!(
            merge_substreams(
                AccessUnitIndex {
                    first_frame: 0,
                    frame_count: 2,
                    sample_rate: 48_000,
                    samples: 1,
                },
                &info(1, None),
                independent,
                Some((&info_with(1, true, Some(0x0003)), &dependent)),
            ),
            Err(Eac3Error::MultipleLfeChannels)
        );
    }

    #[test]
    fn distinct_lfe_locations_across_substreams_are_not_conflated() {
        let independent = DecodedAudioPcm {
            channels: vec![vec![1.0]],
            lfe: Some(vec![2.0]),
        };
        let dependent = DecodedAudioPcm {
            channels: Vec::new(),
            lfe: Some(vec![3.0]),
        };
        assert_eq!(
            merge_substreams(
                AccessUnitIndex {
                    first_frame: 0,
                    frame_count: 2,
                    sample_rate: 48_000,
                    samples: 1,
                },
                &info_with(1, true, None),
                independent,
                Some((&info_with(1, true, Some(0x0002)), &dependent)),
            ),
            Err(Eac3Error::MultipleLfeChannels)
        );
    }

    #[test]
    fn complete_joc_boundary_rejects_non_table_47_topology() {
        let mono = DecodedAccessUnitPcm {
            sample_rate: 48_000,
            samples: 1,
            channel_locations: vec![ChannelLocation::Centre],
            channels: vec![vec![1.0]],
            lfe_location: None,
            lfe: None,
            downmix: DownmixMetadata::default(),
            dialnorm: DialnormState::default(),
        };
        assert_eq!(
            mono.validate_joc_topology(),
            Err(Eac3Error::UnsupportedJocChannelTopology {
                full_band_channels: 1,
                lfe_present: false,
            })
        );

        let mut lfe2 = DecodedAccessUnitPcm {
            sample_rate: 48_000,
            samples: 1,
            channel_locations: vec![
                ChannelLocation::Left,
                ChannelLocation::Right,
                ChannelLocation::Centre,
                ChannelLocation::LeftSurround,
                ChannelLocation::RightSurround,
            ],
            channels: vec![vec![1.0]; 5],
            lfe_location: Some(ChannelLocation::Lfe(1)),
            lfe: Some(vec![2.0]),
            downmix: DownmixMetadata::default(),
            dialnorm: DialnormState::default(),
        };
        assert!(matches!(
            lfe2.validate_joc_topology(),
            Err(Eac3Error::UnsupportedJocChannelTopology { .. })
        ));
        lfe2.lfe_location = Some(ChannelLocation::Lfe(0));
        lfe2.validate_joc_topology().expect("Table 47 5.1");
    }

    #[test]
    fn idx1_requires_rear_flat7x_while_idx4_is_not_given_that_identity() {
        let mut pcm = DecodedAccessUnitPcm {
            sample_rate: 48_000,
            samples: 1,
            channel_locations: vec![
                ChannelLocation::Left,
                ChannelLocation::Right,
                ChannelLocation::Centre,
                ChannelLocation::LeftSurround,
                ChannelLocation::RightSurround,
                ChannelLocation::TopFrontLeft,
                ChannelLocation::TopFrontRight,
            ],
            channels: vec![vec![0.0]; 7],
            lfe_location: Some(ChannelLocation::Lfe(0)),
            lfe: Some(vec![0.0]),
            downmix: DownmixMetadata::default(),
            dialnorm: DialnormState::default(),
        };
        assert!(pcm.validate_joc_downmix_topology(1).is_err());
        assert!(pcm.validate_joc_downmix_topology(4).is_ok());
        assert!(!pcm.is_standard_flat7x_joc_input(4));
        pcm.channel_locations[5] = ChannelLocation::LeftBack;
        pcm.channel_locations[6] = ChannelLocation::RightBack;
        assert!(pcm.validate_joc_downmix_topology(4).is_err());
        pcm.validate_joc_downmix_topology(1)
            .expect("idx1 exact flat-7.X topology");
        assert!(pcm.is_standard_flat7x_joc_input(1));
    }

    #[test]
    fn standard_mono_surround_and_custom_cs_share_a_location() {
        let independent = DecodedAudioPcm {
            channels: vec![vec![1.0], vec![2.0], vec![3.0], vec![4.0]],
            lfe: None,
        };
        let dependent = DecodedAudioPcm {
            channels: vec![vec![9.0], vec![8.0], vec![7.0]],
            lfe: None,
        };
        let dependent_info = info(4, None);
        let output = merge_substreams(
            AccessUnitIndex {
                first_frame: 0,
                frame_count: 2,
                sample_rate: 48_000,
                samples: 1,
            },
            &info(5, None),
            independent,
            Some((&dependent_info, &dependent)),
        )
        .expect("standard S channel replacement");
        assert_eq!(
            output.channels,
            vec![vec![9.0], vec![8.0], vec![2.0], vec![7.0]]
        );
    }
}
