use super::{
    AppliedBinauralUpdate, BinauralState, BinauralUpdateAcceptance, BinauralUpdateApplyFailure,
    DEFAULT_DYNAMIC_BINAURAL_TRANSITION_SAMPLES, Duration, FINAL_LINKED_GAIN_LATENCY_SAMPLES,
    JocAccessUnitPcmDecoder, JocFrameInput, JocMetadataFrame, JocValidationProfile,
    LegacyCoreConfiguration, ListenerOrientationPreparer, MAX_DEFERRED_PULL_AU_SAMPLES,
    MAX_DYNAMIC_BINAURAL_BLOCK_SAMPLES, MAX_DYNAMIC_HRIR_TAPS,
    MAX_LISTENER_ORIENTATION_PULL_SAMPLES, OAMD_PAYLOAD_ID, OamdParseProfile, OpenJocConfig,
    OpenJocDiagnostics, OpenJocError, OpenJocOutputInfo, OpenJocPacket, OpenJocPcmFrame,
    OpenJocSession, OpenJocStageTiming, OpenJocStatus, PCM_SAMPLE_FORMAT, ParsedJocProof,
    ParsedOamdProof, PreparedBinauralUpdate, QMF_LATENCY_SAMPLES, RenderMode, RenderedBlock,
    SofaLoadLimits, SpeakerRenderer, StreamType, ValidationProfile, VecDeque,
    classify_binding_codec_profile_for_frame, clock_now_ms, dither_values, elapsed_since_ms,
    extract_joc_access_unit_for_profile, group_access_units, index_syncframes, joc_profile_name,
    new_payload_decoder, parse_bsi, parse_joc_access_unit, sha256_hex, validate_complexity_index,
    validate_joc_access_unit,
};
#[cfg(not(target_arch = "wasm32"))]
use super::{
    OpenJocDecodeStageTiming, OpenJocRenderOperation, OpenJocRenderStageTiming, decode_timing,
    render_timing,
};
use std::fmt::Write as _;

#[derive(Default)]
struct SessionOptions<'a> {
    external_asset: Option<&'a [u8]>,
    custom_sofa_load_limits: Option<SofaLoadLimits>,
    listener_orientation_enabled: bool,
    binaural_pull_samples: Option<usize>,
    initial_stream_epoch: u64,
    orientation_preparer: Option<ListenerOrientationPreparer>,
}

fn validate_binaural_pull_samples(max_pull_samples: usize) -> Result<(), OpenJocError> {
    if max_pull_samples == 0 || max_pull_samples > MAX_LISTENER_ORIENTATION_PULL_SAMPLES {
        return Err(OpenJocError::InvalidConfig(format!(
            "binaural pull block must be in 1..={MAX_LISTENER_ORIENTATION_PULL_SAMPLES} samples"
        )));
    }
    Ok(())
}

// Selection proves this payload parsed under the chosen profile; later
// topology, complexity and scene admission checks still run in their old order.
struct SelectedMetadata {
    metadata: JocMetadataFrame,
    profile: JocValidationProfile,
    oamd_profile: OamdParseProfile,
    oamd_proof: ParsedOamdProof,
}

impl OpenJocSession {
    /// Creates a validated, independent session. No process-global state is
    /// touched; separate sessions can run concurrently on separate threads.
    pub fn new(config: OpenJocConfig) -> Result<Self, OpenJocError> {
        Self::new_with_builtin_hrtf_asset(config, None)
    }

    /// Creates an explicitly opted-in binaural session with device-independent
    /// listener-orientation preparation and bounded dynamic FIR updates.
    ///
    /// The default [`Self::new`] path remains static. This session expects the
    /// host to prepare poses away from rendering and submit them between calls.
    pub fn new_with_listener_orientation(config: OpenJocConfig) -> Result<Self, OpenJocError> {
        Self::new_with_session_options(
            config,
            SessionOptions {
                listener_orientation_enabled: true,
                ..SessionOptions::default()
            },
        )
    }

    /// Creates an opt-in orientation session that exposes projected virtual-speaker
    /// PCM through bounded pull-sized binaural output blocks.
    ///
    /// The session retains at most one decoded access unit of pre-binaural PCM;
    /// `receive_binaural_frame` renders no more than `max_pull_samples` per call.
    /// This makes pose updates eligible between those pulls without changing the
    /// virtual-speaker projection, gain policy, or FIR kernels. It is an experimental
    /// scheduling interface, not a sensor-to-sound latency guarantee.
    pub fn new_with_listener_orientation_pull(
        config: OpenJocConfig,
        max_pull_samples: usize,
    ) -> Result<Self, OpenJocError> {
        Self::new_with_listener_orientation_pull_at_epoch(config, max_pull_samples, 0)
    }

    /// Creates an orientation pull session at a caller-maintained stream epoch.
    /// This supports adapters that lazily discard and recreate their session
    /// while continuing to reject pose updates prepared before a reset.
    pub fn new_with_listener_orientation_pull_at_epoch(
        config: OpenJocConfig,
        max_pull_samples: usize,
        stream_epoch: u64,
    ) -> Result<Self, OpenJocError> {
        validate_binaural_pull_samples(max_pull_samples)?;
        Self::new_with_session_options(
            config,
            SessionOptions {
                listener_orientation_enabled: true,
                binaural_pull_samples: Some(max_pull_samples),
                initial_stream_epoch: stream_epoch,
                ..SessionOptions::default()
            },
        )
    }

    /// Pull-session constructor that reuses an immutable preparer captured by
    /// an adapter or worker. Resource identity is validated before sharing.
    pub fn new_with_listener_orientation_pull_using_preparer(
        config: OpenJocConfig,
        max_pull_samples: usize,
        stream_epoch: u64,
        preparer: ListenerOrientationPreparer,
    ) -> Result<Self, OpenJocError> {
        validate_binaural_pull_samples(max_pull_samples)?;
        Self::new_with_session_options(
            config,
            SessionOptions {
                listener_orientation_enabled: true,
                binaural_pull_samples: Some(max_pull_samples),
                initial_stream_epoch: stream_epoch,
                orientation_preparer: Some(preparer),
                ..SessionOptions::default()
            },
        )
    }

    /// Creates a session from a borrowed, external built-in HRTF asset.
    /// The asset is validated synchronously and is not retained.
    pub fn new_with_hrtf_asset(config: OpenJocConfig, asset: &[u8]) -> Result<Self, OpenJocError> {
        Self::new_with_builtin_hrtf_asset(config, Some(asset))
    }

    /// Creates a session while applying caller-selected limits to explicit
    /// custom SOFA parsing. Built-in HRTF asset loading is unchanged.
    pub fn new_with_sofa_load_limits(
        config: OpenJocConfig,
        sofa_load_limits: SofaLoadLimits,
    ) -> Result<Self, OpenJocError> {
        Self::new_with_session_options(
            config,
            SessionOptions {
                custom_sofa_load_limits: Some(sofa_load_limits),
                ..SessionOptions::default()
            },
        )
    }

    fn new_with_builtin_hrtf_asset(
        config: OpenJocConfig,
        external_asset: Option<&[u8]>,
    ) -> Result<Self, OpenJocError> {
        Self::new_with_session_options(
            config,
            SessionOptions {
                external_asset,
                ..SessionOptions::default()
            },
        )
    }

    fn new_with_session_options(
        config: OpenJocConfig,
        options: SessionOptions<'_>,
    ) -> Result<Self, OpenJocError> {
        let SessionOptions {
            external_asset,
            custom_sofa_load_limits,
            listener_orientation_enabled,
            binaural_pull_samples,
            initial_stream_epoch,
            orientation_preparer,
        } = options;
        config.validate()?;
        if listener_orientation_enabled && config.render_mode != RenderMode::Binaural {
            return Err(OpenJocError::InvalidConfig(
                "listener orientation is only valid for binaural rendering".to_owned(),
            ));
        }
        if binaural_pull_samples.is_some() && !listener_orientation_enabled {
            return Err(OpenJocError::InvalidConfig(
                "binaural pull rendering requires listener-orientation support".to_owned(),
            ));
        }
        let speaker_layout = config.effective_speaker_layout()?;
        let speaker = SpeakerRenderer::new_with_linked_gain(
            speaker_layout,
            config.downmix,
            config.render_mode != RenderMode::Binaural,
            config.render_mode != RenderMode::Binaural,
        );
        let binaural = config
            .binaural
            .as_ref()
            .map(|binaural| {
                if listener_orientation_enabled {
                    BinauralState::new_with_listener_orientation_at_epoch(
                        binaural,
                        external_asset,
                        custom_sofa_load_limits,
                        initial_stream_epoch,
                        orientation_preparer,
                    )
                } else {
                    BinauralState::new(binaural, external_asset, custom_sofa_load_limits)
                }
            })
            .transpose()?;
        let mut audio_decoder = JocAccessUnitPcmDecoder::new();
        audio_decoder.set_dialnorm_mode(config.dialnorm);
        Ok(Self {
            payload_decoder: new_payload_decoder(&config),
            audio_decoder,
            speaker,
            binaural,
            output_queue: VecDeque::new(),
            pending_binaural_inputs: VecDeque::new(),
            current_binaural_input: None,
            pending_binaural_input_samples: 0,
            binaural_pull_samples,
            selected_profile: None,
            core_stream_type: None,
            legacy_core_configuration: None,
            dither_values: dither_values(),
            sample_rate: None,
            segment_pts: None,
            next_input_sample: 0,
            last_output_end: 0,
            downmix_index: None,
            object_count: None,
            complexity_index: None,
            stage_timing_enabled: false,
            last_stage_timing: OpenJocStageTiming::default(),
            #[cfg(not(target_arch = "wasm32"))]
            decode_timing: decode_timing::DecodeTimingState::default(),
            #[cfg(not(target_arch = "wasm32"))]
            render_timing: render_timing::RenderTimingState::default(),
            drained: false,
            terminal_error: None,
            config,
        })
    }

    /// Returns output semantics without exposing internal decoder structs.
    #[must_use]
    pub fn output_info(&self) -> OpenJocOutputInfo {
        let (layout_name, channel_labels) = self.output_layout_info();
        OpenJocOutputInfo {
            sample_format: PCM_SAMPLE_FORMAT,
            sample_rate: self.sample_rate,
            channel_count: channel_labels.len(),
            channel_labels,
            layout_name,
            render_mode: self.config.render_mode,
            latency_samples: self.latency_samples(),
        }
    }

    /// Returns the known decoder, reconstruction, and added HRIR conversion delay.
    #[must_use]
    pub fn latency_samples(&self) -> usize {
        if self.config.render_mode == RenderMode::Binaural {
            QMF_LATENCY_SAMPLES
                + self
                    .binaural
                    .as_ref()
                    .map_or(0, |state| state.lfe_delay.delay_samples())
        } else {
            QMF_LATENCY_SAMPLES + FINAL_LINKED_GAIN_LATENCY_SAMPLES
        }
    }

    /// Returns the effective session descriptor, including the opt-in dynamic
    /// orientation contract when this session was explicitly enabled.
    #[must_use]
    pub fn effective_config_descriptor(&self) -> String {
        let mut descriptor = self.config.effective_config_descriptor();
        if self
            .binaural
            .as_ref()
            .is_some_and(BinauralState::listener_orientation_enabled)
        {
            let transition_samples = DEFAULT_DYNAMIC_BINAURAL_TRANSITION_SAMPLES;
            let max_hrir_taps = MAX_DYNAMIC_HRIR_TAPS;
            let max_block_samples = MAX_DYNAMIC_BINAURAL_BLOCK_SAMPLES;
            let _ = write!(
                descriptor,
                "\nlistener_orientation=enabled\nlistener_orientation_transition_samples={transition_samples}\nlistener_orientation_max_hrir_taps={max_hrir_taps}\nlistener_orientation_max_block_samples={max_block_samples}",
            );
            if let Some(pull_samples) = self.binaural_pull_samples {
                let _ = write!(
                    descriptor,
                    "\nlistener_orientation_pull=enabled\nlistener_orientation_pull_max_samples={pull_samples}"
                );
            }
        }
        descriptor
    }

    /// Returns a deterministic fingerprint of this session's effective
    /// configuration, including orientation support only when opted in.
    #[must_use]
    pub fn effective_config_fingerprint(&self) -> String {
        sha256_hex(self.effective_config_descriptor().as_bytes())
    }

    /// Returns a cloneable immutable preparation context when orientation was enabled.
    #[must_use]
    pub fn listener_orientation_preparer(&self) -> Option<ListenerOrientationPreparer> {
        self.binaural
            .as_ref()
            .and_then(BinauralState::listener_orientation_preparer)
    }

    /// Returns the current stream epoch for updates prepared on a worker thread.
    #[must_use]
    pub fn listener_orientation_stream_epoch(&self) -> Option<u64> {
        self.binaural
            .as_ref()
            .and_then(BinauralState::listener_orientation_stream_epoch)
    }

    /// Queues a complete prepared pose for the next not-yet-rendered binaural block.
    ///
    /// On validation failure, the returned error retains the caller's original
    /// update. Successful receipts may also contain retired kernel buffers;
    /// release those away from any audio callback.
    pub fn apply_prepared_listener_orientation(
        &mut self,
        update: PreparedBinauralUpdate,
    ) -> Result<BinauralUpdateAcceptance, BinauralUpdateApplyFailure> {
        if self.terminal_error.is_some() {
            return Err(BinauralUpdateApplyFailure {
                error: openjoc_render::RenderError::BinauralRequiresReset,
                update,
            });
        }
        if self.drained {
            return Err(BinauralUpdateApplyFailure {
                error: openjoc_render::RenderError::BinauralInputAfterTailStart,
                update,
            });
        }
        match self.binaural.as_mut() {
            Some(state) => state.apply_prepared_listener_orientation(update),
            None => Err(BinauralUpdateApplyFailure {
                error: openjoc_render::RenderError::BinauralOrientationNotEnabled,
                update,
            }),
        }
    }

    /// Returns the most recently applied pose and its actual rendered sample boundary.
    #[must_use]
    pub fn last_applied_listener_orientation(&self) -> Option<AppliedBinauralUpdate> {
        self.binaural
            .as_ref()
            .and_then(BinauralState::last_applied_listener_orientation)
    }

    /// Returns a target sequence that has not started affecting rendered PCM.
    #[must_use]
    pub fn pending_listener_orientation_sequence(&self) -> Option<u64> {
        self.binaural
            .as_ref()
            .and_then(BinauralState::pending_listener_orientation_sequence)
    }

    /// Returns the number of projected virtual-speaker input samples still
    /// waiting for pull-side binauralization. `None` means pull mode is disabled.
    #[must_use]
    pub fn pending_binaural_input_samples(&self) -> Option<usize> {
        self.binaural_pull_samples
            .map(|_| self.pending_binaural_input_samples)
    }

    /// Returns the bounded profile/topology facts observed in decoded metadata.
    #[must_use]
    pub fn diagnostics(&self) -> OpenJocDiagnostics {
        OpenJocDiagnostics {
            profile: self.selected_profile.map(joc_profile_name),
            downmix_index: self.downmix_index,
            object_count: self.object_count,
            complexity_index: self.complexity_index,
        }
    }

    /// Enables opt-in decode/render timing for later successful AU pushes.
    pub fn enable_stage_timing(&mut self) {
        self.stage_timing_enabled = true;
        self.last_stage_timing = OpenJocStageTiming::default();
    }

    /// Takes the most recent successful AU stage timing record.
    pub fn take_stage_timing(&mut self) -> OpenJocStageTiming {
        std::mem::take(&mut self.last_stage_timing)
    }

    /// Enables native-only detailed observations for later successful AU pushes.
    ///
    /// The two outer durations measure the existing E-AC-3 and owned JOC frame
    /// decoder calls, excluding intervening admission/metadata work and later
    /// rendering or delivery. Existing aggregate stage timing is independent.
    /// Enabling again clears this record but does not reset the audio stream.
    /// Reset/flush/discontinuity clear the record and retain collection.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn enable_decode_stage_timing(&mut self) {
        self.decode_timing.enabled = true;
        self.decode_timing.last = None;
        self.audio_decoder.enable_stage_timing();
        self.payload_decoder.enable_reconstruction_timing();
    }

    /// Takes the latest successful-AU detailed record, at most once.
    ///
    /// Successful pushes replace an untaken record, including `NeedMoreInput`.
    /// Failed or backpressured pushes preserve it unless a stream reset occurred.
    /// Receiving output and draining neither publish nor consume this record.
    /// Disabled collection and an empty slot return `None`; zero clock durations
    /// are still valid observations and are not used to signal an empty slot.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn take_decode_stage_timing(&mut self) -> Option<OpenJocDecodeStageTiming> {
        self.decode_timing.last.take()
    }

    /// Enables independent native renderer observations without resetting audio.
    ///
    /// Collection defaults off and reads no new clock while disabled. Re-enabling
    /// clears the pending renderer record. Reset/flush/discontinuity clear it and
    /// retain enablement. Aggregate and decoder timing collection are independent.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn enable_render_stage_timing(&mut self) {
        self.render_timing.enabled = true;
        self.render_timing.clear();
        self.speaker.render_timing.enabled = true;
        self.speaker.render_timing.clear();
    }

    /// Takes the latest successful push, first drain transition, or PCM-producing
    /// binaural pull record once. An untaken record is replaced by the next such
    /// success; failed/backpressured operations and empty/repeated receives or
    /// drains preserve it unless a stream reset occurred. Ordinary `receive_frame`
    /// neither publishes nor consumes this record. Zero durations are valid.
    ///
    /// Deferred-binaural pushes observe speaker work only; later pulls observe
    /// binaural rendering and PCM conversion. Consumers needing every observation
    /// must take after each successful operation. See [`OpenJocRenderStageTiming`]
    /// for the measured boundaries and the difference from legacy CLI timing.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn take_render_stage_timing(&mut self) -> Option<OpenJocRenderStageTiming> {
        self.render_timing.last.take()
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn begin_render_observation(&mut self) {
        self.render_timing.begin();
        self.speaker.render_timing.clear();
    }

    /// Sends one complete access unit. Caller packet memory is borrowed only
    /// for this call; the session copies only decoded PCM into bounded state.
    pub fn push_packet(
        &mut self,
        packet: OpenJocPacket<'_>,
    ) -> Result<OpenJocStatus, OpenJocError> {
        let total_start = self.stage_timing_enabled.then(clock_now_ms);
        if let Some(error) = &self.terminal_error {
            return Err(error.legacy_error());
        }
        if self.drained {
            return Err(OpenJocError::AlreadyDrained);
        }
        if self.has_pending_output() {
            return Ok(OpenJocStatus::OutputPending);
        }
        if packet.data.is_empty() {
            return Err(OpenJocError::InvalidPacket("packet is empty".to_owned()));
        }
        if packet.data.len() > openjoc_eac3::GENERAL_MAX_ACCESS_UNIT_BYTES {
            return Err(OpenJocError::InvalidPacket(format!(
                "packet exceeds the bounded {0}-byte General JOC access-unit limit",
                openjoc_eac3::GENERAL_MAX_ACCESS_UNIT_BYTES
            )));
        }
        if packet.discontinuity {
            if let Err(error) = self.reset_stream_state() {
                self.fail_closed(&error);
                return Err(error);
            }
        }
        self.check_timestamp(packet.pts_samples)?;
        let frames = index_syncframes(packet.data)?;
        let units = group_access_units(&frames)?;
        let unit = units.first().copied().ok_or(OpenJocError::InvalidPacket(
            "packet does not contain an access unit".to_owned(),
        ))?;
        if units.len() != 1 || unit.first_frame != 0 || unit.frame_count != frames.len() {
            return Err(OpenJocError::InvalidPacket(
                "a packet must contain exactly one complete JOC access unit".to_owned(),
            ));
        }
        let core_stream_type = frames[unit.first_frame].header.stream_type;
        if self
            .core_stream_type
            .is_some_and(|expected| expected != core_stream_type)
        {
            return Err(OpenJocError::ProfileChanged);
        }
        let legacy_core_configuration = if core_stream_type == StreamType::LegacyIndependent {
            let core = frames[unit.first_frame];
            let core_end = core
                .offset
                .checked_add(core.header.frame_size)
                .ok_or_else(|| {
                    OpenJocError::InvalidPacket("legacy core range overflow".to_owned())
                })?;
            let core_bytes = packet.data.get(core.offset..core_end).ok_or_else(|| {
                OpenJocError::InvalidPacket("legacy core range overflow".to_owned())
            })?;
            let bsi = parse_bsi(core_bytes)?;
            Some(LegacyCoreConfiguration {
                bitstream_id: bsi.bitstream_id,
                bitstream_mode: bsi.bitstream_mode,
                audio_coding_mode: bsi.audio_coding_mode,
                lfe_on: bsi.lfe_on,
                sample_rate: bsi.header.sample_rate,
                frame_size: bsi.header.frame_size,
            })
        } else {
            None
        };
        if self.legacy_core_configuration.is_some()
            && self.legacy_core_configuration != legacy_core_configuration
        {
            return Err(OpenJocError::ProfileChanged);
        }
        if let Some(expected) = self.sample_rate {
            if expected != unit.sample_rate {
                return Err(OpenJocError::FormatChanged {
                    expected,
                    actual: unit.sample_rate,
                });
            }
        } else {
            self.sample_rate = Some(unit.sample_rate);
        }

        let decode_start = self.stage_timing_enabled.then(clock_now_ms);
        #[cfg(not(target_arch = "wasm32"))]
        let eac3_start = decode_timing::start(self.decode_timing.enabled);
        let pcm_planes = self.audio_decoder.decode_pcm_planes_with_policy(
            packet.data,
            &frames,
            unit,
            &self.dither_values,
            self.config.drc.internal(),
        )?;
        #[cfg(not(target_arch = "wasm32"))]
        let eac3_timing = decode_timing::finish(eac3_start)
            .map(|elapsed| (elapsed, self.audio_decoder.take_stage_timing()));
        let pcm = &pcm_planes.joc_input_pcm;
        pcm.validate_joc_topology()?;
        let SelectedMetadata {
            metadata,
            profile,
            oamd_profile,
            oamd_proof,
        } = self.select_metadata(packet.data, &frames, unit)?;
        let parsed_oamd = oamd_proof.parsed();
        let joc_proof = ParsedJocProof::parse(&metadata.joc)
            .map_err(|error| OpenJocError::Decode(error.to_string()))?;
        let parsed_joc = joc_proof.parsed();
        pcm.validate_joc_downmix_topology(parsed_joc.header.downmix_index)?;
        if let Some(previous) = self.selected_profile {
            if previous != profile {
                return Err(OpenJocError::ProfileChanged);
            }
        } else {
            self.selected_profile = Some(profile);
        }
        validate_complexity_index(metadata.complexity_index, parsed_oamd.prefix.object_count)?;
        self.downmix_index
            .get_or_insert(parsed_joc.header.downmix_index);
        self.object_count
            .get_or_insert(parsed_oamd.prefix.object_count);
        self.complexity_index
            .get_or_insert(metadata.complexity_index);
        let frame_number = self.next_input_sample / u64::from(unit.samples);
        let input = JocFrameInput {
            sample_rate: unit.sample_rate,
            downmix_pcm: &pcm.channels,
            base_lfe_pcm: pcm_planes.compatibility_pcm.lfe.as_deref(),
            joc_payload: &metadata.joc,
            oamd_payload: &metadata.oamd,
            frame_index: frame_number,
        };
        let binding_profile =
            classify_binding_codec_profile_for_frame(&metadata, parsed_oamd, profile, oamd_profile);
        #[cfg(not(target_arch = "wasm32"))]
        let reconstruction_start = decode_timing::start(self.decode_timing.enabled);
        let frame = self
            .payload_decoder
            .decode_frame_with_parsed_payloads_to_frame(
                input,
                oamd_profile,
                binding_profile,
                Some(joc_proof),
                Some(oamd_proof),
            )?;
        #[cfg(not(target_arch = "wasm32"))]
        let detailed_decode_timing = decode_timing::finish(reconstruction_start)
            .zip(eac3_timing)
            .map(
                |(joc_reconstruction, (eac3_decode, eac3_stages))| OpenJocDecodeStageTiming {
                    input_start_sample: self.next_input_sample,
                    input_sample_count: unit.samples,
                    sample_rate: unit.sample_rate,
                    eac3_decode,
                    joc_reconstruction,
                    eac3_stages,
                    reconstruction_stages: self.payload_decoder.take_reconstruction_timing(),
                },
            );
        let decode_elapsed = decode_start.map(elapsed_since_ms);
        #[cfg(not(target_arch = "wasm32"))]
        let render_operation = OpenJocRenderOperation::Push {
            input_start_sample: self.next_input_sample,
            input_sample_count: unit.samples,
            sample_rate: unit.sample_rate,
        };
        self.next_input_sample = self
            .next_input_sample
            .checked_add(u64::from(unit.samples))
            .ok_or_else(|| OpenJocError::Decode("sample timeline overflow".to_owned()))?;
        #[cfg(not(target_arch = "wasm32"))]
        self.begin_render_observation();
        let render_start = self.stage_timing_enabled.then(clock_now_ms);
        let rendered = self.speaker.render_frame_aligned(&frame, &pcm_planes)?;
        #[cfg(not(target_arch = "wasm32"))]
        self.render_timing.record_speaker(&rendered);
        let render_elapsed = render_start.map(elapsed_since_ms);
        let binaural_start = self.stage_timing_enabled.then(clock_now_ms);
        self.emit_rendered(rendered)?;
        let binaural_elapsed = binaural_start.map(elapsed_since_ms).map(|elapsed| {
            if self.config.render_mode == RenderMode::Binaural {
                elapsed
            } else {
                Duration::ZERO
            }
        });
        if let (Some(total_start), Some(decode), Some(render), Some(binaural)) = (
            total_start,
            decode_elapsed,
            render_elapsed,
            binaural_elapsed,
        ) {
            self.last_stage_timing = OpenJocStageTiming {
                decode,
                render,
                binaural,
                total: elapsed_since_ms(total_start),
            };
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(timing) = detailed_decode_timing {
            self.decode_timing.last = Some(timing);
        }
        self.core_stream_type = Some(core_stream_type);
        self.legacy_core_configuration = legacy_core_configuration;
        #[cfg(not(target_arch = "wasm32"))]
        self.render_timing
            .publish(render_operation, &self.speaker.render_timing);
        // `preroll` is retained as an explicit input fact for future seek
        // adapters. It is decoded normally in this first ABI because the
        // decoder cannot discard a delayed frame without a caller policy.
        let _ = packet.preroll;
        Ok(if self.has_pending_output() {
            OpenJocStatus::FrameAvailable
        } else {
            OpenJocStatus::NeedMoreInput
        })
    }

    /// Receives one owned PCM frame. The queue is bounded to frames produced
    /// by one send/drain operation; callers should receive before pushing.
    /// Returns `None` in pull-binaural sessions; use
    /// [`Self::receive_binaural_frame`] there.
    pub fn receive_frame(&mut self) -> Option<OpenJocPcmFrame> {
        if self.binaural_pull_samples.is_some() {
            return None;
        }
        self.output_queue.pop_front()
    }

    /// Renders and receives at most the configured pull size from a binaural
    /// orientation session. A returned frame is one chunk of the current AU;
    /// apply pose updates between calls to change the next not-yet-rendered chunk.
    /// Reconstruction and FIR tails are also exposed incrementally after `drain`.
    pub fn receive_binaural_frame(&mut self) -> Result<Option<OpenJocPcmFrame>, OpenJocError> {
        if let Some(error) = &self.terminal_error {
            return Err(error.legacy_error());
        }
        let Some(max_pull_samples) = self.binaural_pull_samples else {
            return Err(OpenJocError::InvalidConfig(
                "bounded binaural pull is not enabled for this session".to_owned(),
            ));
        };
        #[cfg(not(target_arch = "wasm32"))]
        self.begin_render_observation();
        if let Some((frame, offset)) = self.current_binaural_input.take().or_else(|| {
            self.pending_binaural_inputs
                .pop_front()
                .map(|frame| (frame, 0))
        }) {
            let count = max_pull_samples.min(frame.sample_count.saturating_sub(offset));
            if count == 0 {
                self.fail_closed(&OpenJocError::Render(
                    "empty deferred binaural input block".to_owned(),
                ));
                return Err(OpenJocError::Render(
                    "empty deferred binaural input block".to_owned(),
                ));
            }
            let binaural = self.binaural.as_mut().ok_or_else(|| {
                OpenJocError::Render("binaural session state is unavailable".to_owned())
            })?;
            #[cfg(not(target_arch = "wasm32"))]
            let binaural_start = decode_timing::start(self.render_timing.enabled);
            let rendered = match binaural.render_range(&frame, offset, count) {
                Ok(rendered) => rendered,
                Err(error) => {
                    self.fail_closed(&error);
                    return Err(error);
                }
            };
            #[cfg(not(target_arch = "wasm32"))]
            self.render_timing
                .record_binaural(decode_timing::finish(binaural_start));
            #[cfg(not(target_arch = "wasm32"))]
            let pcm_start = decode_timing::start(self.render_timing.enabled);
            let result = self.to_pcm_frame(&rendered);
            let pcm = match result {
                Ok(pcm) => pcm,
                Err(error) => {
                    self.fail_closed(&error);
                    return Err(error);
                }
            };
            #[cfg(not(target_arch = "wasm32"))]
            self.render_timing
                .record_pcm(&pcm, decode_timing::finish(pcm_start));
            if offset + count < frame.sample_count {
                self.current_binaural_input = Some((frame, offset + count));
            }
            self.pending_binaural_input_samples =
                self.pending_binaural_input_samples.saturating_sub(count);
            #[cfg(not(target_arch = "wasm32"))]
            self.render_timing.publish(
                OpenJocRenderOperation::BinauralPull,
                &self.speaker.render_timing,
            );
            return Ok(Some(pcm));
        }
        if self.drained {
            let start = self.last_output_end;
            #[cfg(not(target_arch = "wasm32"))]
            let binaural_start = decode_timing::start(self.render_timing.enabled);
            let tail = self
                .binaural
                .as_mut()
                .ok_or_else(|| {
                    OpenJocError::Render("binaural session state is unavailable".to_owned())
                })?
                .drain_tail_chunk(max_pull_samples, start);
            #[cfg(not(target_arch = "wasm32"))]
            self.render_timing
                .record_binaural(decode_timing::finish(binaural_start));
            match tail {
                Ok(Some(frame)) => {
                    self.last_output_end = self.last_output_end.max(
                        frame
                            .logical_start_sample
                            .saturating_add(frame.sample_count as u64),
                    );
                    #[cfg(not(target_arch = "wasm32"))]
                    let pcm_start = decode_timing::start(self.render_timing.enabled);
                    return match self.to_pcm_frame(&frame) {
                        Ok(pcm) => {
                            #[cfg(not(target_arch = "wasm32"))]
                            {
                                self.render_timing
                                    .record_pcm(&pcm, decode_timing::finish(pcm_start));
                                self.render_timing.publish(
                                    OpenJocRenderOperation::BinauralPull,
                                    &self.speaker.render_timing,
                                );
                            }
                            Ok(Some(pcm))
                        }
                        Err(error) => {
                            self.fail_closed(&error);
                            Err(error)
                        }
                    };
                }
                Ok(None) => return Ok(None),
                Err(error) => {
                    self.fail_closed(&error);
                    return Err(error);
                }
            }
        }
        Ok(None)
    }

    /// Whether `drain` has completed and no further frame can be received.
    #[must_use]
    pub fn is_drained(&self) -> bool {
        self.drain_complete()
    }

    /// Flushes delayed QMF/reconstruction and SOFA FIR tail output.
    pub fn drain(&mut self) -> Result<OpenJocStatus, OpenJocError> {
        if let Some(error) = &self.terminal_error {
            return Err(error.legacy_error());
        }
        if self.has_pending_output() {
            return Ok(OpenJocStatus::OutputPending);
        }
        if self.drained {
            if self.has_unrendered_pull_tail() {
                return Ok(OpenJocStatus::FrameAvailable);
            }
            return Ok(OpenJocStatus::EndOfStream);
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.begin_render_observation();
        self.drained = true;
        let result = (|| {
            if self.sample_rate.is_none() {
                if let Some(binaural) = self.binaural.as_mut() {
                    binaural.begin_drain()?;
                }
                return Ok(OpenJocStatus::EndOfStream);
            }
            if let Some(binaural) = self.binaural.as_mut() {
                binaural.begin_drain()?;
            }
            let payload =
                std::mem::replace(&mut self.payload_decoder, new_payload_decoder(&self.config));
            let (_, reconstruction_tail) = payload.finish_streaming_with_reconstruction_tail()?;
            let rendered = self
                .speaker
                .finish_with_reconstruction_tail(&reconstruction_tail)?;
            #[cfg(not(target_arch = "wasm32"))]
            self.render_timing.record_speaker(&rendered);
            self.emit_rendered(rendered)?;
            if self.binaural_pull_samples.is_none() {
                if let Some(binaural) = self.binaural.as_mut() {
                    let mut tail_start = self.last_output_end;
                    #[cfg(not(target_arch = "wasm32"))]
                    let binaural_start = decode_timing::start(self.render_timing.enabled);
                    let tail = binaural.drain_tail(self.sample_rate.unwrap_or(0), tail_start)?;
                    #[cfg(not(target_arch = "wasm32"))]
                    self.render_timing
                        .record_binaural(decode_timing::finish(binaural_start));
                    for frame in tail {
                        tail_start = tail_start.saturating_add(frame.sample_count as u64);
                        #[cfg(not(target_arch = "wasm32"))]
                        let pcm_start = decode_timing::start(self.render_timing.enabled);
                        let pcm = self.to_pcm_frame(&frame)?;
                        #[cfg(not(target_arch = "wasm32"))]
                        self.render_timing
                            .record_pcm(&pcm, decode_timing::finish(pcm_start));
                        self.output_queue.push_back(pcm);
                    }
                }
            }
            Ok(if self.drain_complete() {
                OpenJocStatus::EndOfStream
            } else {
                OpenJocStatus::FrameAvailable
            })
        })();
        match result {
            Ok(status) => {
                #[cfg(not(target_arch = "wasm32"))]
                self.render_timing
                    .publish(OpenJocRenderOperation::Drain, &self.speaker.render_timing);
                Ok(status)
            }
            Err(error) => {
                self.fail_closed(&error);
                Err(error)
            }
        }
    }

    /// Discards pending output and all stream-derived decoder state while
    /// retaining the immutable configuration.
    pub fn flush(&mut self) {
        let _ = self.try_flush();
    }

    /// Fallible form of [`Self::flush`] that reports a fail-closed orientation
    /// epoch overflow or other reset error.
    pub fn try_flush(&mut self) -> Result<(), OpenJocError> {
        self.clear_pending_output();
        match self.reset_stream_state() {
            Ok(()) => {
                self.drained = false;
                self.terminal_error = None;
                Ok(())
            }
            Err(error) => {
                self.fail_closed(&error);
                Err(error)
            }
        }
    }

    /// Resets state for a new timeline/seek. Configuration and SOFA data stay
    /// prepared; no state from the previous stream is reused.
    pub fn reset(&mut self) {
        self.flush();
    }

    /// Fallible form of [`Self::reset`].
    pub fn try_reset(&mut self) -> Result<(), OpenJocError> {
        self.try_flush()
    }

    fn reset_stream_state(&mut self) -> Result<(), OpenJocError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.decode_timing.last = None;
            self.render_timing.clear();
            self.speaker.render_timing.clear();
        }
        self.audio_decoder.reset();
        self.audio_decoder.set_dialnorm_mode(self.config.dialnorm);
        self.payload_decoder = new_payload_decoder(&self.config);
        #[cfg(not(target_arch = "wasm32"))]
        if self.decode_timing.enabled {
            self.payload_decoder.enable_reconstruction_timing();
        }
        self.speaker.reset();
        if let Some(binaural) = self.binaural.as_mut() {
            binaural.reset()?;
        }
        self.selected_profile = None;
        self.core_stream_type = None;
        self.legacy_core_configuration = None;
        self.sample_rate = None;
        self.segment_pts = None;
        self.next_input_sample = 0;
        self.last_output_end = 0;
        self.downmix_index = None;
        self.object_count = None;
        self.complexity_index = None;
        Ok(())
    }

    fn check_timestamp(&mut self, pts: Option<i64>) -> Result<(), OpenJocError> {
        let Some(pts) = pts else { return Ok(()) };
        let offset = i64::try_from(self.next_input_sample).map_err(|_| {
            OpenJocError::InvalidPacket("timestamp sample offset exceeds i64".to_owned())
        })?;
        if let Some(origin) = self.segment_pts {
            let expected = origin.checked_add(offset).ok_or_else(|| {
                OpenJocError::InvalidPacket("expected packet timestamp exceeds i64".to_owned())
            })?;
            if expected != pts {
                return Err(OpenJocError::TimestampDiscontinuity {
                    expected,
                    actual: pts,
                });
            }
        } else {
            let origin = pts.checked_sub(offset).ok_or_else(|| {
                OpenJocError::InvalidPacket("timestamp segment origin exceeds i64".to_owned())
            })?;
            self.segment_pts = Some(origin);
        }
        Ok(())
    }

    fn select_metadata(
        &self,
        stream: &[u8],
        frames: &[openjoc_eac3::SyncframeIndexEntry],
        unit: openjoc_eac3::AccessUnitIndex,
    ) -> Result<SelectedMetadata, OpenJocError> {
        let try_profile = |profile| -> Result<SelectedMetadata, OpenJocError> {
            let metadata = extract_joc_access_unit_for_profile(stream, frames, unit, profile)?
                .ok_or_else(|| OpenJocError::InvalidPacket("JOC metadata is absent".to_owned()))?;
            let oamd_profile = match profile {
                JocValidationProfile::EtsiStrict => OamdParseProfile::EtsiStrict,
                JocValidationProfile::ObservedVendorCompat => {
                    OamdParseProfile::ObservedVendorCompat
                }
            };
            let oamd_proof = match oamd_profile {
                OamdParseProfile::EtsiStrict => {
                    ParsedOamdProof::parse_with_config(&metadata.oamd, self.config.oamd)
                }
                OamdParseProfile::ObservedVendorCompat => ParsedOamdProof::parse_with_profile(
                    &metadata.oamd,
                    self.config.oamd,
                    oamd_profile,
                    OAMD_PAYLOAD_ID,
                ),
            }
            .map_err(|error| OpenJocError::Decode(error.to_string()))?;
            Ok(SelectedMetadata {
                metadata,
                profile,
                oamd_profile,
                oamd_proof,
            })
        };
        match self.config.validation_profile {
            ValidationProfile::EtsiStrict => try_profile(JocValidationProfile::EtsiStrict),
            ValidationProfile::ObservedVendorCompat => {
                try_profile(JocValidationProfile::ObservedVendorCompat)
            }
            ValidationProfile::Auto => {
                let parsed = parse_joc_access_unit(stream, frames, unit)?.ok_or_else(|| {
                    OpenJocError::InvalidPacket("JOC metadata is absent".to_owned())
                })?;
                if let Ok(metadata) =
                    validate_joc_access_unit(&parsed, JocValidationProfile::EtsiStrict)
                {
                    if let Ok(oamd_proof) =
                        ParsedOamdProof::parse_with_config(&metadata.oamd, self.config.oamd)
                    {
                        return Ok(SelectedMetadata {
                            metadata,
                            profile: JocValidationProfile::EtsiStrict,
                            oamd_profile: OamdParseProfile::EtsiStrict,
                            oamd_proof,
                        });
                    }
                }
                try_profile(JocValidationProfile::ObservedVendorCompat)
            }
        }
    }

    fn emit_rendered(&mut self, mut rendered: Vec<RenderedBlock>) -> Result<(), OpenJocError> {
        if self.binaural_pull_samples.is_some() {
            // Push may enqueue only one access unit. Drain runs only after that
            // AU has been pulled, and may then enqueue at most one delayed
            // speaker-frame reconstruction tail. Validate the complete batch
            // before moving any block into the persistent queue.
            rendered.retain(|block| block.sample_count > 0);
            let maximum = MAX_DEFERRED_PULL_AU_SAMPLES;
            let mut batch_samples = 0_usize;
            for block in &rendered {
                let Some(next_samples) = batch_samples.checked_add(block.sample_count) else {
                    let error =
                        OpenJocError::Render("deferred binaural input size overflow".to_owned());
                    self.fail_closed(&error);
                    return Err(error);
                };
                batch_samples = next_samples;
            }
            if batch_samples > maximum || self.pending_binaural_input_samples != 0 {
                let error = OpenJocError::Render(format!(
                    "deferred binaural batch ({batch_samples} samples) exceeds the separate one-AU / one-delayed-frame {maximum}-sample bound or overlaps pending input"
                ));
                self.fail_closed(&error);
                return Err(error);
            }
            self.pending_binaural_input_samples = batch_samples;
            for block in rendered {
                self.last_output_end = self.last_output_end.max(
                    block
                        .logical_start_sample
                        .saturating_add(block.sample_count as u64),
                );
                self.pending_binaural_inputs.push_back(block);
            }
            return Ok(());
        }
        for block in rendered {
            let frame = if let Some(binaural) = self.binaural.as_mut() {
                #[cfg(not(target_arch = "wasm32"))]
                let binaural_start = decode_timing::start(self.render_timing.enabled);
                let frame = binaural.render(&block)?;
                #[cfg(not(target_arch = "wasm32"))]
                self.render_timing
                    .record_binaural(decode_timing::finish(binaural_start));
                frame
            } else {
                block
            };
            self.last_output_end = self.last_output_end.max(
                frame
                    .logical_start_sample
                    .saturating_add(frame.sample_count as u64),
            );
            #[cfg(not(target_arch = "wasm32"))]
            let pcm_start = decode_timing::start(self.render_timing.enabled);
            let pcm = self.to_pcm_frame(&frame)?;
            #[cfg(not(target_arch = "wasm32"))]
            self.render_timing
                .record_pcm(&pcm, decode_timing::finish(pcm_start));
            self.output_queue.push_back(pcm);
        }
        Ok(())
    }

    fn drain_complete(&self) -> bool {
        self.drained
            && (self.terminal_error.is_some()
                || (!self.has_pending_output() && !self.has_unrendered_pull_tail()))
    }

    fn has_pending_output(&self) -> bool {
        !self.output_queue.is_empty()
            || !self.pending_binaural_inputs.is_empty()
            || self.current_binaural_input.is_some()
    }

    fn has_unrendered_pull_tail(&self) -> bool {
        self.binaural_pull_samples.is_some()
            && self.drained
            && self
                .binaural
                .as_ref()
                .is_some_and(|state| !state.has_no_tail())
    }

    fn to_pcm_frame(&self, frame: &RenderedBlock) -> Result<OpenJocPcmFrame, OpenJocError> {
        let channels = frame.channels.len();
        let samples = frame.sample_count;
        let (layout_name, labels) = self.output_layout_info();
        if channels != labels.len()
            || frame
                .channels
                .iter()
                .any(|channel| channel.len() != samples)
        {
            return Err(OpenJocError::Render(
                "renderer returned inconsistent PCM shape".to_owned(),
            ));
        }
        let mut interleaved = Vec::with_capacity(samples.saturating_mul(channels));
        for sample in 0..samples {
            for channel in &frame.channels {
                let value = channel[sample];
                if !value.is_finite() {
                    return Err(OpenJocError::Render(
                        "renderer returned non-finite PCM".to_owned(),
                    ));
                }
                interleaved.push(value as f32);
            }
        }
        let pts_samples = self
            .segment_pts
            .map(|origin| {
                i64::try_from(frame.logical_start_sample)
                    .ok()
                    .and_then(|offset| origin.checked_add(offset))
                    .ok_or_else(|| OpenJocError::Render("output timestamp exceeds i64".to_owned()))
            })
            .transpose()?;
        Ok(OpenJocPcmFrame {
            sample_format: PCM_SAMPLE_FORMAT,
            sample_rate: frame.sample_rate,
            channel_count: channels,
            channel_labels: labels,
            layout_name,
            render_mode: self.config.render_mode,
            sample_count: samples,
            pts_samples,
            interleaved_f32: interleaved,
        })
    }

    fn output_layout_info(&self) -> (String, Vec<String>) {
        if self.config.render_mode == RenderMode::Binaural {
            (
                "Binaural stereo".to_owned(),
                vec!["Left Ear".to_owned(), "Right Ear".to_owned()],
            )
        } else {
            let layout = self.speaker.layout_info();
            (layout.name, layout.labels)
        }
    }
}

#[cfg(test)]
#[path = "selected_metadata_tests.rs"]
mod metadata_tests;

#[cfg(all(test, feature = "embedded-builtin-hrtf"))]
#[path = "session_lifecycle_contract.rs"]
mod lifecycle_contract;
