use gst::glib;
use gst::prelude::*;
use gst::subclass::prelude::*;
use gst_audio::prelude::*;
use gst_audio::subclass::prelude::*;
use openjoc_api::{
    BinauralConfig, BinauralLfePolicy, DialnormMode, DownmixPolicy, DrcPolicy, OpenJocConfig,
    OpenJocPacket, OpenJocPcmFrame, OpenJocSession, OpenJocStatus, RenderMode, ValidationProfile,
};
use openjoc_eac3::{StreamType, parse_syncframe_header};
use std::sync::{Mutex, MutexGuard, OnceLock};

#[path = "autoplug.rs"]
mod autoplug;

const SAMPLE_RATE: u32 = 48_000;
const MAX_SYNCFRAME_BYTES: usize = 4_096;
// GStreamer 1.28 resolves a negative finish-frame count relative to the
// pending compressed-input queue.  -1 therefore finishes all pending input
// frames; during forced drain the queue is empty because every admitted AU
// was already closed with finish_subframe(NULL), so the delayed PCM is not
// falsely attributed to a new compressed frame.
const DRAIN_FINISH_ALL_PENDING: i32 = -1;

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
    virtual_layout: String,
    drc: String,
    drc_boost_percent: u8,
    drc_cut_percent: u8,
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
            virtual_layout: "7.1.4".to_owned(),
            drc: "line".to_owned(),
            drc_boost_percent: 100,
            drc_cut_percent: 100,
            dialnorm: "default".to_owned(),
            validation_profile: "auto".to_owned(),
            downmix: "auto".to_owned(),
            lfe_policy: "exclude".to_owned(),
        }
    }
}

impl Settings {
    fn into_config(self) -> Result<OpenJocConfig, String> {
        let target = match self.render_mode.as_str() {
            "speaker" => OutputTarget::Speaker {
                layout: self.speaker_layout.clone(),
                render_mode: RenderMode::Speaker,
            },
            "stereo" => OutputTarget::Speaker {
                layout: "2.0".to_owned(),
                render_mode: RenderMode::Stereo,
            },
            "binaural" => OutputTarget::Binaural {
                virtual_layout: self.virtual_layout.clone(),
            },
            "auto" => {
                return Err(
                    "render-mode=auto requires a fixed semantic downstream target".to_owned(),
                );
            }
            value => return Err(format!("unsupported render-mode {value:?}")),
        };
        self.config_for_target(target)
    }

    fn config_for_target(&self, target: OutputTarget) -> Result<OpenJocConfig, String> {
        let render_mode = target.render_mode();
        let speaker_layout = match &target {
            OutputTarget::Speaker { layout, .. } => layout.clone(),
            OutputTarget::Binaural { .. } => self.speaker_layout.clone(),
        };
        let drc = match self.drc.as_str() {
            "disabled" => DrcPolicy::Disabled,
            "line" => DrcPolicy::Line,
            "rf" => DrcPolicy::Rf,
            "custom" => DrcPolicy::Custom {
                boost_percent: self.drc_boost_percent,
                cut_percent: self.drc_cut_percent,
            },
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
        let binaural = match target {
            OutputTarget::Binaural { virtual_layout } => Some(BinauralConfig {
                sofa_bytes: Vec::new(),
                virtual_layout,
                lfe_policy,
            }),
            OutputTarget::Speaker { .. } => None,
        };

        Ok(OpenJocConfig {
            render_mode,
            speaker_layout,
            downmix,
            drc,
            dialnorm,
            validation_profile,
            binaural,
            ..OpenJocConfig::default()
        })
    }

    fn auto_config(&self, downstream_caps: &gst::Caps) -> Result<(OpenJocConfig, String), String> {
        let Some(layout) = fixed_downstream_speaker_layout(downstream_caps)? else {
            return Err(
                "render-mode=auto requires one fixed audio/x-raw target with recognized channel positions; broad or positionless caps are ambiguous, and two channels do not imply headphones".to_owned(),
            );
        };
        let target = OutputTarget::Speaker {
            layout: layout.clone(),
            render_mode: RenderMode::Speaker,
        };
        Ok((self.config_for_target(target)?, format!("speaker:{layout}")))
    }

    fn explicit_target_description(&self) -> Result<String, String> {
        match self.render_mode.as_str() {
            "speaker" => Ok(format!("speaker:{}", self.speaker_layout)),
            "stereo" => Ok("speaker:2.0".to_owned()),
            "binaural" => Ok(format!("binaural:virtual={}", self.virtual_layout)),
            "auto" => Err("auto target is selected at downstream negotiation".to_owned()),
            value => Err(format!("unsupported render-mode {value:?}")),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum OutputTarget {
    Speaker {
        layout: String,
        render_mode: RenderMode,
    },
    Binaural {
        virtual_layout: String,
    },
}

impl OutputTarget {
    fn render_mode(&self) -> RenderMode {
        match self {
            Self::Speaker { render_mode, .. } => *render_mode,
            Self::Binaural { .. } => RenderMode::Binaural,
        }
    }
}

#[derive(Debug, Default)]
struct State {
    session: Option<OpenJocSession>,
    configured_rate: Option<u32>,
    configured_channels: Option<usize>,
    configured_labels: Option<Vec<String>>,
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
                    .blurb("speaker, stereo, binaural, or auto")
                    .default_value(Some("speaker"))
                    .build(),
                glib::ParamSpecString::builder("speaker-layout")
                    .nick("Speaker layout")
                    .blurb("OpenJOC speaker preset, for example 5.1 or 7.1.4")
                    .default_value(Some("5.1"))
                    .build(),
                glib::ParamSpecString::builder("virtual-layout")
                    .nick("Binaural virtual layout")
                    .blurb("Virtual speaker layout used by binaural mode")
                    .default_value(Some("7.1.4"))
                    .build(),
                glib::ParamSpecString::builder("drc")
                    .nick("Dynamic range control")
                    .blurb("disabled, line, rf, or custom")
                    .default_value(Some("line"))
                    .build(),
                glib::ParamSpecUChar::builder("drc-boost")
                    .nick("DRC boost percentage")
                    .blurb("Custom DRC positive-range percentage")
                    .minimum(0)
                    .maximum(100)
                    .default_value(100)
                    .build(),
                glib::ParamSpecUChar::builder("drc-cut")
                    .nick("DRC cut percentage")
                    .blurb("Custom DRC negative-range percentage")
                    .minimum(0)
                    .maximum(100)
                    .default_value(100)
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
        if id == 5 || id == 6 {
            let Ok(value) = value.get::<u8>() else {
                return;
            };
            if id == 5 {
                settings.drc_boost_percent = value;
            } else {
                settings.drc_cut_percent = value;
            }
            return;
        }
        let target = match id {
            1 => &mut settings.render_mode,
            2 => &mut settings.speaker_layout,
            3 => &mut settings.virtual_layout,
            4 => &mut settings.drc,
            7 => &mut settings.dialnorm,
            8 => &mut settings.validation_profile,
            9 => &mut settings.downmix,
            10 => &mut settings.lfe_policy,
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
            3 => settings.virtual_layout.to_value(),
            4 => settings.drc.to_value(),
            5 => settings.drc_boost_percent.to_value(),
            6 => settings.drc_cut_percent.to_value(),
            7 => settings.dialnorm.to_value(),
            8 => settings.validation_profile.to_value(),
            9 => settings.downmix.to_value(),
            10 => settings.lfe_policy.to_value(),
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
                "Primary JOC-only decoder for classified framed E-AC-3 access units",
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
                .field(autoplug::JOC_CAPS_FIELD, true)
                .features([autoplug::JOC_CAPS_FEATURE])
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
        let (session, target, config_fingerprint) = if settings.render_mode == "auto" {
            (None, None, None)
        } else {
            let config = settings
                .clone()
                .into_config()
                .map_err(|error| gst::error_msg!(gst::CoreError::Failed, ["{error}"]))?;
            let fingerprint = config.effective_config_fingerprint();
            let target = settings
                .explicit_target_description()
                .map_err(|error| gst::error_msg!(gst::CoreError::Failed, ["{error}"]))?;
            (
                Some(OpenJocSession::new(config).map_err(|error| {
                    gst::error_msg!(
                        gst::CoreError::Failed,
                        ["failed to create OpenJOC session: {error}"]
                    )
                })?),
                Some(target),
                Some(fingerprint),
            )
        };
        let mut state = lock(&self.state);
        state.session = session;
        state.configured_rate = None;
        state.configured_channels = None;
        state.configured_labels = None;
        state.pending_discontinuity = false;
        drop(state);

        self.obj().set_drainable(true);
        if let (Some(target), Some(fingerprint)) = (target, config_fingerprint) {
            gst::info!(
                category(),
                imp = self,
                "started OpenJOC session effective-config-sha256={fingerprint} target={target}"
            );
        } else {
            gst::info!(
                category(),
                imp = self,
                "started OpenJOC auto-target session"
            );
        }
        Ok(())
    }

    fn stop(&self) -> Result<(), gst::ErrorMessage> {
        let mut state = lock(&self.state);
        state.session = None;
        state.configured_rate = None;
        state.configured_channels = None;
        state.configured_labels = None;
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
            || structure.get::<bool>(autoplug::JOC_CAPS_FIELD) != Ok(true)
            || caps
                .features(0)
                .is_none_or(|features| !features.contains(autoplug::JOC_CAPS_FEATURE))
        {
            return Err(gst::loggable_error!(
                gst::CAT_RUST,
                "OpenJOC requires classified JOC caps: framed=true, alignment=frame, {}=true, feature={} ",
                autoplug::JOC_CAPS_FIELD,
                autoplug::JOC_CAPS_FEATURE
            ));
        }
        let mut state = lock(&self.state);
        if let Some(session) = state.session.as_mut() {
            session.flush();
        }
        if lock(&self.settings).render_mode == "auto" {
            state.session = None;
        }
        state.configured_rate = None;
        state.configured_channels = None;
        state.configured_labels = None;
        state.pending_discontinuity = false;
        Ok(())
    }

    fn parse(&self, adapter: &gst_base::Adapter) -> Result<(u32, u32), gst::FlowError> {
        parse_adapter(adapter, self.obj().parse_state().1)
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
        if header.stream_type != StreamType::Independent || header.substream_id != 0 {
            gst::error!(category(), "access unit is not independent substream zero");
            return Err(gst::FlowError::Error);
        }
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

        self.ensure_session_for_output_target()?;

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
        state.configured_labels = None;
        state.pending_discontinuity = true;
    }
}

impl OpenJocDecImp {
    fn ensure_session_for_output_target(&self) -> Result<(), gst::FlowError> {
        if lock(&self.state).session.is_some() {
            return Ok(());
        }
        let settings = lock(&self.settings).clone();
        let downstream_caps = self.obj().src_pad().peer_query_caps(None);
        let (config, target) = settings.auto_config(&downstream_caps).map_err(|error| {
            gst::element_imp_error!(self, gst::StreamError::Format, ("{error}"));
            gst::FlowError::NotNegotiated
        })?;
        let fingerprint = config.effective_config_fingerprint();
        let session = OpenJocSession::new(config).map_err(|error| {
            gst::element_imp_error!(
                self,
                gst::StreamError::Format,
                ("failed to create OpenJOC auto-target session: {error}")
            );
            gst::FlowError::NotNegotiated
        })?;
        let mut state = lock(&self.state);
        if state.session.is_none() {
            state.session = Some(session);
            gst::info!(
                category(),
                imp = self,
                "selected OpenJOC auto target effective-config-sha256={fingerprint} target={target} downstream-caps={downstream_caps}"
            );
        }
        Ok(())
    }

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
            return Ok(gst::FlowSuccess::Ok);
        }
        self.ensure_output_format(frames[0].sample_rate, &frames[0])?;
        for frame in frames {
            self.obj()
                .finish_frame(Some(pcm_buffer(&frame)?), DRAIN_FINISH_ALL_PENDING)?;
        }
        Ok(gst::FlowSuccess::Ok)
    }

    fn ensure_output_format(
        &self,
        sample_rate: u32,
        frame: &OpenJocPcmFrame,
    ) -> Result<(), gst::FlowError> {
        let mut state = lock(&self.state);
        if state.configured_rate == Some(sample_rate)
            && state.configured_channels == Some(frame.channel_count)
            && state.configured_labels.as_ref() == Some(&frame.channel_labels)
        {
            return Ok(());
        }
        let (positions, _) = gst_channel_order(&frame.channel_labels).map_err(|error| {
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
        state.configured_labels = Some(frame.channel_labels.clone());
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

/// Returns one complete I0/[D0] unit length, or `GST_FLOW_EOS` through the
/// Rust `FlowError::Eos` mapping while the adapter needs more bytes. The
/// GstAudioDecoder base class consumes that return as "no frame yet" and keeps
/// the adapter contents; it does not forward EOS downstream.
fn parse_adapter(adapter: &gst_base::Adapter, eos: bool) -> Result<(u32, u32), gst::FlowError> {
    let available = adapter.available();
    let mut bytes = vec![0_u8; available];
    adapter
        .copy(0, &mut bytes)
        .map_err(|_| gst::FlowError::Error)?;
    match autoplug::parse_access_unit(&bytes, eos) {
        Ok(autoplug::AccessUnitParse::NeedMore) => Err(gst::FlowError::Eos),
        Ok(autoplug::AccessUnitParse::Complete(size)) => {
            Ok((0, u32::try_from(size).map_err(|_| gst::FlowError::Error)?))
        }
        Err(_) => Err(gst::FlowError::Error),
    }
}

fn admit_joc(bytes: &[u8]) -> Result<(), String> {
    match autoplug::classify_access_unit(bytes) {
        autoplug::JocClassification::ConfirmedJoc => Ok(()),
        autoplug::JocClassification::ConfirmedNonJoc => {
            Err("JOC metadata is absent; ordinary E-AC-3 is not a fallback".to_owned())
        }
        autoplug::JocClassification::Unknown => Err("access unit is incomplete".to_owned()),
        autoplug::JocClassification::InvalidOrUnsupported => {
            Err("access unit is malformed or unsupported".to_owned())
        }
    }
}

fn pcm_buffer(frame: &OpenJocPcmFrame) -> Result<gst::Buffer, gst::FlowError> {
    if frame.sample_rate == 0
        || frame.channel_count == 0
        || frame.interleaved_f32.len() != frame.sample_count.saturating_mul(frame.channel_count)
    {
        return Err(gst::FlowError::Error);
    }
    let (_, reorder) =
        gst_channel_order(&frame.channel_labels).map_err(|_| gst::FlowError::Error)?;
    let mut bytes = Vec::with_capacity(frame.interleaved_f32.len().saturating_mul(4));
    for sample in 0..frame.sample_count {
        for &input_channel in &reorder {
            let index = sample
                .saturating_mul(frame.channel_count)
                .saturating_add(input_channel);
            bytes.extend_from_slice(&frame.interleaved_f32[index].to_le_bytes());
        }
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

fn gst_channel_order(
    labels: &[String],
) -> Result<(Vec<gst_audio::AudioChannelPosition>, Vec<usize>), String> {
    let semantic_positions = channel_positions(labels)?;
    let mut gst_positions = semantic_positions.clone();
    gst_audio::AudioChannelPosition::positions_to_valid_order(&mut gst_positions).map_err(
        |error| format!("OpenJOC channel order is not representable by GStreamer: {error}"),
    )?;
    let reorder = gst_positions
        .iter()
        .map(|position| {
            semantic_positions
                .iter()
                .position(|candidate| candidate == position)
                .ok_or_else(|| "GStreamer channel order lost a semantic channel".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((gst_positions, reorder))
}

fn fixed_downstream_speaker_layout(caps: &gst::Caps) -> Result<Option<String>, String> {
    if caps.is_any() || caps.is_empty() || caps.size() != 1 {
        return Ok(None);
    }
    let Some(structure) = caps.structure(0) else {
        return Ok(None);
    };
    if structure.name() != "audio/x-raw" {
        return Ok(None);
    }
    let Ok(channels) = structure.get::<i32>("channels") else {
        return Ok(None);
    };
    let Ok(channels) = usize::try_from(channels) else {
        return Ok(None);
    };
    if channels == 0 || channels > 24 {
        return Ok(None);
    }
    let positions = if channels == 2 && structure.get::<gst::Bitmask>("channel-mask").is_err() {
        vec![
            gst_audio::AudioChannelPosition::FrontLeft,
            gst_audio::AudioChannelPosition::FrontRight,
        ]
    } else {
        let Ok(mask) = structure.get::<gst::Bitmask>("channel-mask") else {
            return Ok(None);
        };
        if mask.0.count_ones() as usize != channels {
            return Ok(None);
        }
        let mut positions = vec![gst_audio::AudioChannelPosition::None; channels];
        gst_audio::AudioChannelPosition::positions_from_mask(mask.0, &mut positions)
            .map_err(|error| format!("invalid downstream channel mask: {error}"))?;
        positions
    };

    for (name, labels) in supported_speaker_layouts() {
        let expected = channel_positions(
            &labels
                .iter()
                .map(|label| (*label).to_owned())
                .collect::<Vec<_>>(),
        )?;
        if expected.len() == positions.len()
            && expected.iter().all(|position| positions.contains(position))
        {
            return Ok(Some((*name).to_owned()));
        }
    }
    Ok(None)
}

fn supported_speaker_layouts() -> &'static [(&'static str, &'static [&'static str])] {
    &[
        ("2.0", &["FL", "FR"]),
        ("5.1", &["FL", "FR", "FC", "LFE", "Ls", "Rs"]),
        (
            "5.1.2",
            &["FL", "FR", "FC", "LFE", "Ls", "Rs", "TFL", "TFR"],
        ),
        (
            "5.1.4",
            &[
                "FL", "FR", "FC", "LFE", "Ls", "Rs", "TFL", "TFR", "TBL", "TBR",
            ],
        ),
        ("7.1", &["FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs"]),
        (
            "7.1.2",
            &[
                "FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs", "TFL", "TFR",
            ],
        ),
        (
            "7.1.4",
            &[
                "FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs", "TFL", "TFR", "TBL", "TBR",
            ],
        ),
        (
            "7.1.6",
            &[
                "FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs", "Ltf", "Rtf", "Ltm", "Rtm", "Ltr",
                "Rtr",
            ],
        ),
        (
            "9.1",
            &["FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs", "Lw", "Rw"],
        ),
        (
            "9.1.2",
            &[
                "FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs", "Lw", "Rw", "Ltm", "Rtm",
            ],
        ),
        (
            "9.1.4",
            &[
                "FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs", "Lw", "Rw", "Ltf", "Rtf", "Ltr",
                "Rtr",
            ],
        ),
        (
            "9.1.6",
            &[
                "FL", "FR", "FC", "LFE", "Lb", "Rb", "Ls", "Rs", "Lw", "Rw", "Ltf", "Rtf", "Ltm",
                "Rtm", "Ltr", "Rtr",
            ],
        ),
        (
            "22.2",
            &[
                "FL", "FR", "FC", "LFE1", "BL", "BR", "FLc", "FRc", "BC", "LFE2", "SiL", "SiR",
                "TpFL", "TpFR", "TpFC", "TpC", "TpBL", "TpBR", "TpSiL", "TpSiR", "TpBC", "BtFC",
                "BtFL", "BtFR",
            ],
        ),
    ]
}

pub fn register(plugin: &gst::Plugin) -> Result<(), glib::BoolError> {
    autoplug::register(Some(plugin))?;
    gst::Element::register(
        Some(plugin),
        "openjocdec",
        gst::Rank::PRIMARY,
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

    fn push_bits(bytes: &mut [u8], cursor: &mut usize, value: u64, width: usize) {
        for shift in (0..width).rev() {
            if value & (1_u64 << shift) != 0 {
                bytes[*cursor / 8] |= 0x80 >> (*cursor % 8);
            }
            *cursor += 1;
        }
    }

    fn syncframe(stream_type: u8, substream_id: u8, size: usize) -> Vec<u8> {
        let mut bytes = vec![0_u8; size];
        let mut cursor = 0;
        push_bits(&mut bytes, &mut cursor, 0x0b77, 16);
        push_bits(&mut bytes, &mut cursor, u64::from(stream_type), 2);
        push_bits(&mut bytes, &mut cursor, u64::from(substream_id), 3);
        push_bits(
            &mut bytes,
            &mut cursor,
            u64::try_from(size / 2 - 1).expect("frame words"),
            11,
        );
        push_bits(&mut bytes, &mut cursor, 0, 2);
        push_bits(&mut bytes, &mut cursor, 3, 2);
        bytes
    }

    fn feed(adapter: &gst_base::Adapter, bytes: Vec<u8>) {
        adapter.push(gst::Buffer::from_mut_slice(bytes));
    }

    #[test]
    fn independent_only_unit_waits_for_eos_then_finishes() {
        gst::init().expect("GStreamer initializes");
        let adapter = gst_base::Adapter::new();
        feed(&adapter, syncframe(0, 0, 16));
        assert_eq!(parse_adapter(&adapter, false), Err(gst::FlowError::Eos));
        assert_eq!(parse_adapter(&adapter, true), Ok((0, 16)));
        adapter.flush(16);
        assert_eq!(adapter.available(), 0);
    }

    #[test]
    fn dependent_frame_in_the_next_buffer_is_not_eos() {
        gst::init().expect("GStreamer initializes");
        let adapter = gst_base::Adapter::new();
        feed(&adapter, syncframe(0, 0, 16));
        assert_eq!(parse_adapter(&adapter, false), Err(gst::FlowError::Eos));
        feed(&adapter, syncframe(1, 0, 16));
        assert_eq!(parse_adapter(&adapter, false), Ok((0, 32)));
        adapter.flush(32);
        assert_eq!(adapter.available(), 0);
    }

    #[test]
    fn consecutive_access_units_are_emitted_one_at_a_time() {
        gst::init().expect("GStreamer initializes");
        let adapter = gst_base::Adapter::new();
        feed(
            &adapter,
            [syncframe(0, 0, 16), syncframe(1, 0, 16)].concat(),
        );
        feed(
            &adapter,
            [syncframe(0, 0, 16), syncframe(1, 0, 16)].concat(),
        );
        assert_eq!(parse_adapter(&adapter, false), Ok((0, 32)));
        adapter.flush(32);
        assert_eq!(parse_adapter(&adapter, false), Ok((0, 32)));
        adapter.flush(32);
        assert_eq!(adapter.available(), 0);
    }

    #[test]
    fn partial_dependent_frame_at_eos_fails_closed() {
        gst::init().expect("GStreamer initializes");
        let adapter = gst_base::Adapter::new();
        feed(&adapter, syncframe(0, 0, 16));
        let dependent = syncframe(1, 0, 16);
        feed(&adapter, dependent[..4].to_vec());
        assert_eq!(parse_adapter(&adapter, true), Err(gst::FlowError::Error));
    }

    #[test]
    fn flush_clears_a_partial_access_unit() {
        gst::init().expect("GStreamer initializes");
        let adapter = gst_base::Adapter::new();
        feed(&adapter, syncframe(0, 0, 16));
        assert_eq!(parse_adapter(&adapter, false), Err(gst::FlowError::Eos));
        adapter.clear();
        assert_eq!(adapter.available(), 0);
        feed(&adapter, syncframe(0, 0, 16));
        assert_eq!(parse_adapter(&adapter, true), Ok((0, 16)));
    }

    #[test]
    fn discontinuity_reset_does_not_retain_partial_input() {
        gst::init().expect("GStreamer initializes");
        let adapter = gst_base::Adapter::new();
        feed(&adapter, syncframe(0, 0, 16));
        assert_eq!(parse_adapter(&adapter, false), Err(gst::FlowError::Eos));
        adapter.clear();
        feed(&adapter, syncframe(0, 0, 16));
        assert_eq!(parse_adapter(&adapter, true), Ok((0, 16)));
    }

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
    fn timestamp_round_trip_is_exact_at_long_run_au_boundaries() {
        for au in [0_u64, 1, 17, 426, 8_672] {
            let samples = au.saturating_mul(1_536);
            let nanoseconds = samples_to_gst_time_u64(samples, SAMPLE_RATE)
                .expect("timestamp")
                .nseconds();
            assert_eq!(
                gst_time_to_samples(nanoseconds, SAMPLE_RATE),
                Some(i64::try_from(samples).expect("sample timestamp"))
            );
        }
    }

    #[test]
    fn adapter_au_boundary_matches_common_byte_hash_trace() {
        gst::init().expect("GStreamer initializes");
        let bytes = [syncframe(0, 0, 16), syncframe(1, 0, 16)].concat();
        let trace = openjoc_api::trace_access_units(&bytes, Some(0)).expect("trace access unit");
        assert_eq!(trace.len(), 1);
        let adapter = gst_base::Adapter::new();
        feed(&adapter, bytes);
        assert_eq!(parse_adapter(&adapter, true), Ok((0, 32)));
        assert_eq!(trace[0].byte_length, 32);
        assert_eq!(trace[0].independent_frame_count, 1);
        assert_eq!(trace[0].dependent_frame_count, 1);
        assert_eq!(trace[0].pts_samples, Some(0));
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

    fn exact_audio_caps(layout: &str) -> gst::Caps {
        gst::init().expect("GStreamer initializes");
        let labels = supported_speaker_layouts()
            .iter()
            .find(|(name, _)| *name == layout)
            .map(|(_, labels)| *labels)
            .expect("known speaker layout");
        let labels = labels
            .iter()
            .map(|label| (*label).to_owned())
            .collect::<Vec<_>>();
        let positions = channel_positions(&labels).expect("layout positions");
        let mask = gst_audio::AudioChannelPosition::positions_to_mask(&positions, false)
            .expect("layout channel mask");
        gst_audio::AudioCapsBuilder::new_interleaved()
            .format(gst_audio::AudioFormat::F32le)
            .rate(i32::try_from(SAMPLE_RATE).expect("sample rate fits"))
            .channels(i32::try_from(labels.len()).expect("channel count fits"))
            .channel_mask(mask)
            .build()
    }

    #[test]
    fn fixed_caps_recognize_every_current_native_speaker_layout() {
        for (layout, _) in supported_speaker_layouts() {
            assert_eq!(
                fixed_downstream_speaker_layout(&exact_audio_caps(layout)),
                Ok(Some((*layout).to_owned())),
                "layout {layout}"
            );
        }
    }

    #[test]
    fn auto_selects_native_speaker_layout_but_never_binaural_from_caps() {
        let settings = Settings {
            render_mode: "auto".to_owned(),
            ..Settings::default()
        };
        let (config, target) = settings
            .auto_config(&exact_audio_caps("7.1.4"))
            .expect("fixed physical target");
        assert_eq!(target, "speaker:7.1.4");
        assert_eq!(config.render_mode, RenderMode::Speaker);
        assert_eq!(config.speaker_layout, "7.1.4");
        assert!(config.binaural.is_none());
        assert!(
            config
                .effective_config_descriptor()
                .contains("render_mode=speaker\nlayout=7.1.4")
        );

        let stereo_caps = gst::Caps::builder("audio/x-raw")
            .field("channels", 2_i32)
            .build();
        let (config, target) = settings
            .auto_config(&stereo_caps)
            .expect("fixed stereo target");
        assert_eq!(target, "speaker:2.0");
        assert_eq!(config.render_mode, RenderMode::Speaker);
        assert_eq!(config.speaker_layout, "2.0");
        assert!(config.binaural.is_none());
    }

    #[test]
    fn broad_or_positionless_multichannel_caps_do_not_select_a_layout() {
        gst::init().expect("GStreamer initializes");
        let broad = gst::Caps::builder("audio/x-raw")
            .field("channels", gst::IntRange::new(1, 64))
            .build();
        assert_eq!(fixed_downstream_speaker_layout(&broad), Ok(None));

        let positionless = gst::Caps::builder("audio/x-raw")
            .field("channels", 12_i32)
            .build();
        assert_eq!(fixed_downstream_speaker_layout(&positionless), Ok(None));
    }

    #[test]
    fn gstreamer_transport_order_is_canonical_without_changing_two_channel_binaural() {
        let labels = supported_speaker_layouts()
            .iter()
            .find(|(name, _)| *name == "9.1.6")
            .expect("9.1.6")
            .1
            .iter()
            .map(|label| (*label).to_owned())
            .collect::<Vec<_>>();
        let (positions, reorder) = gst_channel_order(&labels).expect("9.1.6 mapping");
        assert_eq!(positions.len(), labels.len());
        assert_eq!(reorder.len(), labels.len());
        assert!(
            reorder
                .iter()
                .enumerate()
                .any(|(output, input)| output != *input)
        );

        let (positions, reorder) =
            gst_channel_order(&["Left Ear".to_owned(), "Right Ear".to_owned()])
                .expect("binaural transport mapping");
        assert_eq!(
            positions,
            vec![
                gst_audio::AudioChannelPosition::FrontLeft,
                gst_audio::AudioChannelPosition::FrontRight,
            ]
        );
        assert_eq!(reorder, vec![0, 1]);
    }

    #[test]
    fn pcm_transport_is_a_permutation_only_for_noncanonical_layout_order() {
        let labels = supported_speaker_layouts()
            .iter()
            .find(|(name, _)| *name == "9.1.6")
            .expect("9.1.6")
            .1
            .iter()
            .map(|label| (*label).to_owned())
            .collect::<Vec<_>>();
        let values = (0..labels.len())
            .map(|value| value as f32)
            .collect::<Vec<_>>();
        let frame = OpenJocPcmFrame {
            sample_format: openjoc_api::PcmSampleFormat::F32,
            sample_rate: SAMPLE_RATE,
            channel_count: labels.len(),
            channel_labels: labels.clone(),
            layout_name: "9.1.6".to_owned(),
            render_mode: RenderMode::Speaker,
            sample_count: 1,
            pts_samples: Some(0),
            interleaved_f32: values.clone(),
        };
        let (_, reorder) = gst_channel_order(&labels).expect("9.1.6 order");
        let buffer = pcm_buffer(&frame).expect("PCM buffer");
        let map = buffer.map_readable().expect("readable PCM");
        let transported = map
            .as_slice()
            .chunks_exact(4)
            .map(|sample| f32::from_le_bytes(sample.try_into().expect("f32 bytes")))
            .collect::<Vec<_>>();
        let expected = reorder
            .iter()
            .map(|index| values[*index])
            .collect::<Vec<_>>();
        assert_eq!(transported, expected);
    }

    #[test]
    fn default_settings_are_streaming_safe() {
        let config = Settings::default().into_config().expect("default config");
        assert_eq!(config.render_mode, RenderMode::Speaker);
        assert_eq!(config.speaker_layout, "5.1");
        assert_eq!(config.dialnorm, DialnormMode::Default);
        assert_eq!(config.drc, DrcPolicy::Line);
    }

    #[test]
    fn custom_drc_settings_reach_the_effective_session_config() {
        let settings = Settings {
            drc: "custom".to_owned(),
            drc_boost_percent: 37,
            drc_cut_percent: 63,
            ..Settings::default()
        };
        let config = settings.into_config().expect("custom DRC config");
        assert_eq!(
            config.drc,
            DrcPolicy::Custom {
                boost_percent: 37,
                cut_percent: 63
            }
        );
        assert!(
            config
                .effective_config_descriptor()
                .contains("drc_boost_percent=37")
        );
    }

    #[test]
    fn binaural_defaults_match_the_cli_virtual_layout_and_effective_config() {
        let settings = Settings {
            render_mode: "binaural".to_owned(),
            ..Settings::default()
        };
        let gst_config = settings.into_config().expect("binaural config");
        let cli_config = OpenJocConfig {
            render_mode: RenderMode::Binaural,
            speaker_layout: "7.1.4".to_owned(),
            binaural: Some(BinauralConfig::builtin_generic("7.1.4")),
            ..OpenJocConfig::default()
        };
        assert_eq!(
            gst_config.binaural.as_ref().unwrap().virtual_layout,
            "7.1.4"
        );
        assert_eq!(
            gst_config.effective_config_fingerprint(),
            cli_config.effective_config_fingerprint()
        );
        assert_eq!(gst_config.drc, DrcPolicy::Line);
        assert_eq!(gst_config.dialnorm, DialnormMode::Default);
    }

    #[test]
    fn forced_drain_completion_is_not_a_zero_frame_subframe() {
        assert_eq!(DRAIN_FINISH_ALL_PENDING, -1);
        assert_ne!(DRAIN_FINISH_ALL_PENDING, 0);
    }
}
