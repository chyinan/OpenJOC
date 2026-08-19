use gst::glib;
use gst::prelude::*;
use gst::subclass::prelude::*;
use gst_audio::prelude::*;
use gst_audio::subclass::prelude::*;
use openjoc_api::{
    BinauralConfig, BinauralLfePolicy, DialnormMode, DownmixPolicy, DrcPolicy, OpenJocConfig,
    OpenJocPacket, OpenJocPcmFrame, OpenJocSession, OpenJocStatus, RenderMode, ValidationProfile,
};
use openjoc_eac3::{
    StreamType, SyncframeHeader, group_access_units, index_syncframes, parse_joc_access_unit,
    parse_syncframe_header,
};
use std::sync::{Mutex, MutexGuard, OnceLock};

const SAMPLE_RATE: u32 = 48_000;
const MAX_SYNCFRAME_BYTES: usize = 4_096;

static CAT: OnceLock<gst::DebugCategory> = OnceLock::new();

fn category() -> &'static gst::DebugCategory {
    CAT.get_or_init(|| {
        gst::DebugCategory::new(
            "openjocdec",
            gst::DebugColorFlags::empty(),
            Some("OpenJOC native GStreamer decoder"),
        )
    })
}

#[derive(Clone, Debug)]
struct Settings {
    render_mode: String,
    speaker_layout: String,
    drc: String,
    dialnorm: String,
    validation_profile: String,
    downmix: String,
    lfe_policy: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            render_mode: "speaker".to_owned(),
            speaker_layout: "5.1".to_owned(),
            drc: "line".to_owned(),
            dialnorm: "default".to_owned(),
            validation_profile: "auto".to_owned(),
            downmix: "auto".to_owned(),
            lfe_policy: "exclude".to_owned(),
        }
    }
}

impl Settings {
    fn into_config(self) -> Result<OpenJocConfig, String> {
        let render_mode = match self.render_mode.as_str() {
            "speaker" => RenderMode::Speaker,
            "stereo" => RenderMode::Stereo,
            "binaural" => RenderMode::Binaural,
            value => return Err(format!("unsupported render-mode {value:?}")),
        };
        let drc = match self.drc.as_str() {
            "disabled" => DrcPolicy::Disabled,
            "line" => DrcPolicy::Line,
            "rf" => DrcPolicy::Rf,
            value => return Err(format!("unsupported drc policy {value:?}")),
        };
        let dialnorm = match self.dialnorm.as_str() {
            "default" => DialnormMode::Default,
            "digital" => DialnormMode::Digital,
            "analog" => DialnormMode::Analog,
            value => return Err(format!("unsupported dialnorm mode {value:?}")),
        };
        let validation_profile = match self.validation_profile.as_str() {
            "auto" => ValidationProfile::Auto,
            "etsi-strict" => ValidationProfile::EtsiStrict,
            "observed-vendor-compat" => ValidationProfile::ObservedVendorCompat,
            value => return Err(format!("unsupported validation-profile {value:?}")),
        };
        let downmix = match self.downmix.as_str() {
            "auto" => DownmixPolicy::Auto,
            "loro" => DownmixPolicy::LoRo,
            "ltrt" => DownmixPolicy::LtRt,
            value => return Err(format!("unsupported downmix policy {value:?}")),
        };
        let lfe_policy = match self.lfe_policy.as_str() {
            "exclude" => BinauralLfePolicy::Exclude,
            "equal-power-dual-mono" => BinauralLfePolicy::EqualPowerDualMono,
            value => return Err(format!("unsupported lfe-policy {value:?}")),
        };
        let binaural = (render_mode == RenderMode::Binaural).then(|| BinauralConfig {
            sofa_bytes: Vec::new(),
            virtual_layout: self.speaker_layout.clone(),
            lfe_policy,
        });

        Ok(OpenJocConfig {
            render_mode,
            speaker_layout: self.speaker_layout,
            downmix,
            drc,
            dialnorm,
            validation_profile,
            binaural,
            ..OpenJocConfig::default()
        })
    }
}

#[derive(Debug, Default)]
struct State {
    session: Option<OpenJocSession>,
    configured_rate: Option<u32>,
    configured_channels: Option<usize>,
    pending_discontinuity: bool,
}

#[derive(Default)]
pub struct OpenJocDecImp {
    settings: Mutex<Settings>,
    state: Mutex<State>,
}

#[glib::object_subclass]
impl ObjectSubclass for OpenJocDecImp {
    const NAME: &'static str = "GstOpenJocDec";
    type Type = OpenJocDec;
    type ParentType = gst_audio::AudioDecoder;
}

impl ObjectImpl for OpenJocDecImp {
    fn properties() -> &'static [glib::ParamSpec] {
        static PROPERTIES: OnceLock<Vec<glib::ParamSpec>> = OnceLock::new();
        PROPERTIES.get_or_init(|| {
            vec![
                glib::ParamSpecString::builder("render-mode")
                    .nick("Render mode")
                    .blurb("speaker, stereo, or binaural")
                    .default_value(Some("speaker"))
                    .build(),
                glib::ParamSpecString::builder("speaker-layout")
                    .nick("Speaker layout")
                    .blurb("OpenJOC speaker preset, for example 5.1 or 7.1.4")
                    .default_value(Some("5.1"))
                    .build(),
                glib::ParamSpecString::builder("drc")
                    .nick("Dynamic range control")
                    .blurb("disabled, line, or rf")
                    .default_value(Some("line"))
                    .build(),
                glib::ParamSpecString::builder("dialnorm")
                    .nick("Dialnorm mode")
                    .blurb("default, digital, or analog")
                    .default_value(Some("default"))
                    .build(),
                glib::ParamSpecString::builder("validation-profile")
                    .nick("Validation profile")
                    .blurb("auto, etsi-strict, or observed-vendor-compat")
                    .default_value(Some("auto"))
                    .build(),
                glib::ParamSpecString::builder("downmix")
                    .nick("Stereo downmix")
                    .blurb("auto, loro, or ltrt; valid for stereo output")
                    .default_value(Some("auto"))
                    .build(),
                glib::ParamSpecString::builder("lfe-policy")
                    .nick("Binaural LFE policy")
                    .blurb("exclude or equal-power-dual-mono")
                    .default_value(Some("exclude"))
                    .build(),
            ]
        })
    }

    fn set_property(&self, id: usize, value: &glib::Value, _pspec: &glib::ParamSpec) {
        let mut settings = lock(&self.settings);
        let target = match id {
            1 => &mut settings.render_mode,
            2 => &mut settings.speaker_layout,
            3 => &mut settings.drc,
            4 => &mut settings.dialnorm,
            5 => &mut settings.validation_profile,
            6 => &mut settings.downmix,
            7 => &mut settings.lfe_policy,
            _ => return,
        };
        if let Ok(value) = value.get::<String>() {
            *target = value;
        }
    }

    fn property(&self, id: usize, _pspec: &glib::ParamSpec) -> glib::Value {
        let settings = lock(&self.settings);
        match id {
            1 => settings.render_mode.to_value(),
            2 => settings.speaker_layout.to_value(),
            3 => settings.drc.to_value(),
            4 => settings.dialnorm.to_value(),
            5 => settings.validation_profile.to_value(),
            6 => settings.downmix.to_value(),
            7 => settings.lfe_policy.to_value(),
            _ => unreachable!("invalid openjocdec property id"),
        }
    }
}

impl GstObjectImpl for OpenJocDecImp {}

impl ElementImpl for OpenJocDecImp {
    fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
        static METADATA: OnceLock<gst::subclass::ElementMetadata> = OnceLock::new();
        Some(METADATA.get_or_init(|| {
            gst::subclass::ElementMetadata::new(
                "OpenJOC E-AC-3 JOC decoder",
                "Decoder/Audio",
                "Explicit rank-none decoder for framed E-AC-3 JOC access units",
                "OpenJOC contributors",
            )
        }))
    }

    fn pad_templates() -> &'static [gst::PadTemplate] {
        static TEMPLATES: OnceLock<Vec<gst::PadTemplate>> = OnceLock::new();
        TEMPLATES.get_or_init(|| {
            let sink_caps = gst::Caps::builder("audio/x-eac3")
                .field("framed", true)
                .field("alignment", "frame")
                .build();
            let src_caps = gst_audio::AudioCapsBuilder::new_interleaved()
                .format(gst_audio::AudioFormat::F32le)
                .rate(i32::try_from(SAMPLE_RATE).expect("OpenJOC sample rate fits i32"))
                .channels_range(2..=24)
                .build();
            vec![
                gst::PadTemplate::new(
                    "sink",
                    gst::PadDirection::Sink,
                    gst::PadPresence::Always,
                    &sink_caps,
                )
                .expect("valid OpenJOC sink caps"),
                gst::PadTemplate::new(
                    "src",
                    gst::PadDirection::Src,
                    gst::PadPresence::Always,
                    &src_caps,
                )
                .expect("valid OpenJOC source caps"),
            ]
        })
    }
}

impl AudioDecoderImpl for OpenJocDecImp {
    fn start(&self) -> Result<(), gst::ErrorMessage> {
        let settings = lock(&self.settings).clone();
        let config = settings
            .into_config()
            .map_err(|error| gst::error_msg!(gst::CoreError::Failed, ["{error}"]))?;
        let mut state = lock(&self.state);
        state.session = Some(OpenJocSession::new(config).map_err(|error| {
            gst::error_msg!(
                gst::CoreError::Failed,
                ["failed to create OpenJOC session: {error}"]
            )
        })?);
        state.configured_rate = None;
        state.configured_channels = None;
        state.pending_discontinuity = false;
        drop(state);

        self.obj().set_drainable(true);
        gst::debug!(category(), imp = self, "started OpenJOC session");
        Ok(())
    }

    fn stop(&self) -> Result<(), gst::ErrorMessage> {
        let mut state = lock(&self.state);
        state.session = None;
        state.configured_rate = None;
        state.configured_channels = None;
        state.pending_discontinuity = false;
        Ok(())
    }

    fn set_format(&self, caps: &gst::Caps) -> Result<(), gst::LoggableError> {
        let Some(structure) = caps.structure(0) else {
            return Err(gst::loggable_error!(
                gst::CAT_RUST,
                "OpenJOC requires one fixed audio/x-eac3 structure"
            ));
        };
        if structure.name() != "audio/x-eac3" {
            return Err(gst::loggable_error!(
                gst::CAT_RUST,
                "OpenJOC requires audio/x-eac3 input, got {}",
                structure.name()
            ));
        }
        if structure.get::<bool>("framed") != Ok(true)
            || structure.get::<String>("alignment") != Ok("frame".to_owned())
        {
            return Err(gst::loggable_error!(
                gst::CAT_RUST,
                "OpenJOC requires framed=true, alignment=frame E-AC-3 input"
            ));
        }
        let mut state = lock(&self.state);
        let Some(session) = state.session.as_mut() else {
            return Err(gst::loggable_error!(
                gst::CAT_RUST,
                "OpenJOC session is not started"
            ));
        };
        session.flush();
        state.configured_rate = None;
        state.configured_channels = None;
        state.pending_discontinuity = false;
        Ok(())
    }

    fn parse(&self, adapter: &gst_base::Adapter) -> Result<(u32, u32), gst::FlowError> {
        let available = adapter.available();
        if available < 8 {
            return Err(gst::FlowError::Eos);
        }

        let first = read_header(adapter, 0)?;
        validate_independent_zero(first, "first syncframe")?;
        if first.frame_size > MAX_SYNCFRAME_BYTES || first.sample_rate != SAMPLE_RATE {
            return Err(gst::FlowError::Error);
        }
        if available < first.frame_size {
            return if self.obj().parse_state().1 {
                Err(gst::FlowError::Error)
            } else {
                Err(gst::FlowError::Eos)
            };
        }

        let eos = self.obj().parse_state().1;
        if available == first.frame_size {
            return if eos {
                Ok((0, first.frame_size as u32))
            } else {
                Err(gst::FlowError::Eos)
            };
        }

        if available < first.frame_size + 8 {
            return if eos {
                Ok((0, first.frame_size as u32))
            } else {
                Err(gst::FlowError::Eos)
            };
        }
        let second_offset = first.frame_size;
        let second = read_header(adapter, second_offset)?;
        if second.stream_type != StreamType::Dependent && second.substream_id == 0 {
            return Ok((0, first.frame_size as u32));
        }
        if second.stream_type != StreamType::Dependent || second.substream_id != 0 {
            return Err(gst::FlowError::Error);
        }
        let total = first
            .frame_size
            .checked_add(second.frame_size)
            .ok_or(gst::FlowError::Error)?;
        if second.frame_size > MAX_SYNCFRAME_BYTES
            || second.sample_rate != SAMPLE_RATE
            || second.audio_blocks != first.audio_blocks
        {
            return Err(gst::FlowError::Error);
        }
        if available < total {
            return if eos {
                Err(gst::FlowError::Error)
            } else {
                Err(gst::FlowError::Eos)
            };
        }
        Ok((0, total as u32))
    }

    fn handle_frame(
        &self,
        input: Option<&gst::Buffer>,
    ) -> Result<gst::FlowSuccess, gst::FlowError> {
        let Some(input) = input else {
            return self.handle_drain();
        };
        let bytes = input
            .map_readable()
            .map_err(|_| gst::FlowError::Error)?
            .as_slice()
            .to_vec();
        let header = parse_syncframe_header(&bytes).map_err(|error| {
            gst::element_imp_error!(
                self,
                gst::StreamError::Decode,
                ("invalid E-AC-3 access unit header: {error}")
            );
            gst::FlowError::Error
        })?;
        validate_independent_zero(header, "access unit")?;
        if header.sample_rate != SAMPLE_RATE {
            gst::element_imp_error!(
                self,
                gst::StreamError::Format,
                (
                    "OpenJOC GStreamer output currently requires 48 kHz, got {} Hz",
                    header.sample_rate
                )
            );
            return Err(gst::FlowError::NotNegotiated);
        }
        if bytes.len() < header.frame_size {
            return Err(gst::FlowError::Error);
        }
        if let Err(error) = admit_joc(&bytes) {
            gst::element_imp_error!(
                self,
                gst::StreamError::Decode,
                ("OpenJOC requires an admitted JOC access unit: {error}")
            );
            return Err(gst::FlowError::Error);
        }

        let discontinuity = input.flags().contains(gst::BufferFlags::DISCONT)
            || lock(&self.state).pending_discontinuity;
        let pts_samples = input
            .pts()
            .and_then(|pts| gst_time_to_samples(pts.nseconds(), header.sample_rate));

        let frames = {
            let mut state = lock(&self.state);
            let Some(session) = state.session.as_mut() else {
                return Err(gst::FlowError::NotNegotiated);
            };
            let result = session.push_packet(OpenJocPacket {
                data: &bytes,
                pts_samples,
                discontinuity,
                preroll: false,
            });
            match result {
                Ok(OpenJocStatus::OutputPending) => {
                    return Err(gst::FlowError::Error);
                }
                Ok(_) => {
                    let frames = collect_frames(session);
                    state.pending_discontinuity = false;
                    frames
                }
                Err(error) => {
                    gst::element_imp_error!(
                        self,
                        gst::StreamError::Decode,
                        ("OpenJOC rejected access unit: {error}")
                    );
                    return Err(gst::FlowError::Error);
                }
            }
        };

        if frames.is_empty() {
            return self.obj().finish_subframe(None);
        }
        self.ensure_output_format(frames[0].sample_rate, &frames[0])?;
        self.push_frames(frames)
    }

    fn flush(&self, _hard: bool) {
        let mut state = lock(&self.state);
        if let Some(session) = state.session.as_mut() {
            session.flush();
        }
        state.configured_rate = None;
        state.configured_channels = None;
        state.pending_discontinuity = true;
    }
}

impl OpenJocDecImp {
    fn handle_drain(&self) -> Result<gst::FlowSuccess, gst::FlowError> {
        let frames = {
            let mut state = lock(&self.state);
            let Some(session) = state.session.as_mut() else {
                return Ok(gst::FlowSuccess::Ok);
            };
            match session.drain() {
                Ok(_) => collect_frames(session),
                Err(error) => {
                    gst::element_imp_error!(
                        self,
                        gst::StreamError::Decode,
                        ("OpenJOC drain failed: {error}")
                    );
                    return Err(gst::FlowError::Error);
                }
            }
        };
        if frames.is_empty() {
            return self.obj().finish_frame(None, 1);
        }
        self.ensure_output_format(frames[0].sample_rate, &frames[0])?;
        for frame in frames {
            self.obj().finish_frame(Some(pcm_buffer(&frame)?), 0)?;
        }
        self.obj().finish_frame(None, 1)
    }

    fn ensure_output_format(
        &self,
        sample_rate: u32,
        frame: &OpenJocPcmFrame,
    ) -> Result<(), gst::FlowError> {
        let mut state = lock(&self.state);
        if state.configured_rate == Some(sample_rate)
            && state.configured_channels == Some(frame.channel_count)
        {
            return Ok(());
        }
        let positions = channel_positions(&frame.channel_labels).map_err(|error| {
            gst::element_imp_error!(self, gst::StreamError::Format, ("{error}"));
            gst::FlowError::NotNegotiated
        })?;
        let info = gst_audio::AudioInfo::builder(
            gst_audio::AudioFormat::F32le,
            sample_rate,
            frame.channel_count as u32,
        )
        .layout(gst_audio::AudioLayout::Interleaved)
        .positions(&positions)
        .build()
        .map_err(|error| {
            gst::element_imp_error!(
                self,
                gst::StreamError::Format,
                ("failed to create output AudioInfo: {error}")
            );
            gst::FlowError::NotNegotiated
        })?;
        self.obj().set_output_format(&info)?;
        self.obj().negotiate()?;

        let latency_samples = state
            .session
            .as_ref()
            .map_or(0, OpenJocSession::latency_samples);
        let latency =
            samples_to_gst_time(latency_samples, sample_rate).ok_or(gst::FlowError::Error)?;
        self.obj().set_latency(latency, latency);
        state.configured_rate = Some(sample_rate);
        state.configured_channels = Some(frame.channel_count);
        Ok(())
    }

    fn push_frames(
        &self,
        frames: Vec<OpenJocPcmFrame>,
    ) -> Result<gst::FlowSuccess, gst::FlowError> {
        for frame in frames {
            self.obj().finish_subframe(Some(pcm_buffer(&frame)?))?;
        }
        self.obj().finish_subframe(None)
    }
}

glib::wrapper! {
    pub struct OpenJocDec(ObjectSubclass<OpenJocDecImp>) @extends gst_audio::AudioDecoder, gst::Element, gst::Object;
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn collect_frames(session: &mut OpenJocSession) -> Vec<OpenJocPcmFrame> {
    let mut frames = Vec::new();
    while let Some(frame) = session.receive_frame() {
        frames.push(frame);
    }
    frames
}

fn read_header(
    adapter: &gst_base::Adapter,
    offset: usize,
) -> Result<SyncframeHeader, gst::FlowError> {
    let mut prefix = [0_u8; 8];
    adapter
        .copy(offset, &mut prefix)
        .map_err(|_| gst::FlowError::Error)?;
    parse_syncframe_header(&prefix).map_err(|_| gst::FlowError::Error)
}

fn validate_independent_zero(header: SyncframeHeader, context: &str) -> Result<(), gst::FlowError> {
    if header.stream_type != StreamType::Independent || header.substream_id != 0 {
        gst::error!(category(), "{context} is not independent substream zero");
        return Err(gst::FlowError::Error);
    }
    Ok(())
}

fn admit_joc(bytes: &[u8]) -> Result<(), String> {
    let frames = index_syncframes(bytes).map_err(|error| error.to_string())?;
    let units = group_access_units(&frames).map_err(|error| error.to_string())?;
    let unit = units
        .first()
        .copied()
        .ok_or_else(|| "access unit is empty".to_owned())?;
    if units.len() != 1 || unit.first_frame != 0 || unit.frame_count != frames.len() {
        return Err("input contains more than one OpenJOC access unit".to_owned());
    }
    if parse_joc_access_unit(bytes, &frames, unit)
        .map_err(|error| error.to_string())?
        .is_none()
    {
        return Err("JOC metadata is absent; ordinary E-AC-3 is not a fallback".to_owned());
    }
    Ok(())
}

fn pcm_buffer(frame: &OpenJocPcmFrame) -> Result<gst::Buffer, gst::FlowError> {
    if frame.sample_rate == 0
        || frame.channel_count == 0
        || frame.interleaved_f32.len() != frame.sample_count.saturating_mul(frame.channel_count)
    {
        return Err(gst::FlowError::Error);
    }
    let mut bytes = Vec::with_capacity(frame.interleaved_f32.len().saturating_mul(4));
    for sample in &frame.interleaved_f32 {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    let mut buffer = gst::Buffer::from_mut_slice(bytes);
    buffer.get_mut().unwrap().set_duration(
        samples_to_gst_time(frame.sample_count, frame.sample_rate).ok_or(gst::FlowError::Error)?,
    );
    Ok(buffer)
}

fn gst_time_to_samples(nanoseconds: u64, sample_rate: u32) -> Option<i64> {
    let numerator = u128::from(nanoseconds).checked_mul(u128::from(sample_rate))?;
    let rounded = numerator.checked_add(500_000_000)? / 1_000_000_000;
    i64::try_from(rounded).ok()
}

fn samples_to_gst_time(samples: usize, sample_rate: u32) -> Option<gst::ClockTime> {
    samples_to_gst_time_u64(u64::try_from(samples).ok()?, sample_rate)
}

fn samples_to_gst_time_u64(samples: u64, sample_rate: u32) -> Option<gst::ClockTime> {
    if sample_rate == 0 {
        return None;
    }
    let numerator = u128::from(samples).checked_mul(1_000_000_000)?;
    let rounded = numerator.checked_add(u128::from(sample_rate / 2))? / u128::from(sample_rate);
    gst::ClockTime::from_nseconds(u64::try_from(rounded).ok()?).into()
}

fn channel_positions(labels: &[String]) -> Result<Vec<gst_audio::AudioChannelPosition>, String> {
    labels
        .iter()
        .map(|label| match label.as_str() {
            "FL" | "Left Ear" => Ok(gst_audio::AudioChannelPosition::FrontLeft),
            "FR" | "Right Ear" => Ok(gst_audio::AudioChannelPosition::FrontRight),
            "FC" => Ok(gst_audio::AudioChannelPosition::FrontCenter),
            "LFE" | "LFE1" => Ok(gst_audio::AudioChannelPosition::Lfe1),
            "LFE2" => Ok(gst_audio::AudioChannelPosition::Lfe2),
            "Ls" | "SiL" => Ok(gst_audio::AudioChannelPosition::SideLeft),
            "Rs" | "SiR" => Ok(gst_audio::AudioChannelPosition::SideRight),
            "Lb" | "BL" => Ok(gst_audio::AudioChannelPosition::RearLeft),
            "Rb" | "BR" => Ok(gst_audio::AudioChannelPosition::RearRight),
            "BC" => Ok(gst_audio::AudioChannelPosition::RearCenter),
            "FLc" => Ok(gst_audio::AudioChannelPosition::FrontLeftOfCenter),
            "FRc" => Ok(gst_audio::AudioChannelPosition::FrontRightOfCenter),
            "Lw" => Ok(gst_audio::AudioChannelPosition::WideLeft),
            "Rw" => Ok(gst_audio::AudioChannelPosition::WideRight),
            "TFL" | "Ltf" | "TpFL" => Ok(gst_audio::AudioChannelPosition::TopFrontLeft),
            "TFR" | "Rtf" | "TpFR" => Ok(gst_audio::AudioChannelPosition::TopFrontRight),
            "TBL" | "Ltr" | "TpBL" => Ok(gst_audio::AudioChannelPosition::TopRearLeft),
            "TBR" | "Rtr" | "TpBR" => Ok(gst_audio::AudioChannelPosition::TopRearRight),
            "Ltm" | "TpSiL" => Ok(gst_audio::AudioChannelPosition::TopSideLeft),
            "Rtm" | "TpSiR" => Ok(gst_audio::AudioChannelPosition::TopSideRight),
            "TpFC" => Ok(gst_audio::AudioChannelPosition::TopFrontCenter),
            "TpC" => Ok(gst_audio::AudioChannelPosition::TopCenter),
            "TpBC" => Ok(gst_audio::AudioChannelPosition::TopRearCenter),
            "BtFC" => Ok(gst_audio::AudioChannelPosition::BottomFrontCenter),
            "BtFL" => Ok(gst_audio::AudioChannelPosition::BottomFrontLeft),
            "BtFR" => Ok(gst_audio::AudioChannelPosition::BottomFrontRight),
            other => Err(format!(
                "OpenJOC channel identity {other:?} has no truthful GStreamer position"
            )),
        })
        .collect()
}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    gst::Element::register(
        Some(plugin),
        "openjocdec",
        gst::Rank::NONE,
        OpenJocDec::static_type(),
    )
}

pub fn register_static_plugin() -> Result<(), glib::BoolError> {
    plugin_desc::plugin_register_static()
}

fn plugin_init(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    register(plugin)
}

gst::plugin_define!(
    openjoc,
    env!("CARGO_PKG_DESCRIPTION"),
    plugin_init,
    env!("CARGO_PKG_VERSION"),
    "Apache-2.0",
    env!("CARGO_PKG_NAME"),
    env!("CARGO_PKG_NAME"),
    env!("CARGO_PKG_REPOSITORY")
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_conversion_is_rational_and_bounded() {
        assert_eq!(gst_time_to_samples(1_000_000_000, 48_000), Some(48_000));
        assert_eq!(gst_time_to_samples(1_000_000_000, 44_100), Some(44_100));
        assert_eq!(
            samples_to_gst_time(609, 48_000)
                .expect("speaker latency")
                .nseconds(),
            12_687_500
        );
        assert_eq!(
            samples_to_gst_time(577, 48_000)
                .expect("binaural latency")
                .nseconds(),
            12_020_833
        );
        assert_eq!(gst_time_to_samples(u64::MAX, u32::MAX), None);
    }

    #[test]
    fn channel_identity_mapping_preserves_openjoc_order() {
        let labels = ["FL", "FR", "FC", "LFE1", "BL", "BR", "SiL", "SiR"]
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let positions = channel_positions(&labels).expect("standard positions");
        assert_eq!(positions[0], gst_audio::AudioChannelPosition::FrontLeft);
        assert_eq!(positions[3], gst_audio::AudioChannelPosition::Lfe1);
        assert_eq!(positions[4], gst_audio::AudioChannelPosition::RearLeft);
        assert_eq!(positions[6], gst_audio::AudioChannelPosition::SideLeft);
    }

    #[test]
    fn default_settings_are_streaming_safe() {
        let config = Settings::default().into_config().expect("default config");
        assert_eq!(config.render_mode, RenderMode::Speaker);
        assert_eq!(config.speaker_layout, "5.1");
        assert_eq!(config.dialnorm, DialnormMode::Default);
        assert_eq!(config.drc, DrcPolicy::Line);
    }
}
