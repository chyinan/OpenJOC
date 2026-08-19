use super::{MAX_SYNCFRAME_BYTES, SAMPLE_RATE, category};
use gst::glib;
use gst::prelude::*;
use gst::subclass::prelude::*;
use gst_base::prelude::*;
use gst_base::subclass::prelude::*;
use openjoc_eac3::{
    StreamType, group_access_units, index_syncframes, parse_audio_frame, parse_joc_access_unit,
    parse_syncframe_header,
};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Instant;

/// OpenJOC-specific caps discriminator. This is intentionally not presented
/// as an upstream GStreamer convention.
pub(super) const JOC_CAPS_FIELD: &str = "openjoc-joc";
pub(super) const JOC_CAPS_FEATURE: &str = "openjoc:joc";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum JocClassification {
    Unknown,
    ConfirmedJoc,
    ConfirmedNonJoc,
    InvalidOrUnsupported,
}

impl JocClassification {
    fn name(self) -> &'static str {
        match self {
            Self::Unknown => "UNKNOWN",
            Self::ConfirmedJoc => "CONFIRMED_JOC",
            Self::ConfirmedNonJoc => "CONFIRMED_NON_JOC",
            Self::InvalidOrUnsupported => "INVALID_OR_UNSUPPORTED",
        }
    }
}

pub(super) enum AccessUnitParse {
    NeedMore,
    Complete(usize),
}

/// Finds exactly one admitted I0/[D0] access unit without owning an adapter.
/// The decoder and classifier both use this function so AU boundaries cannot
/// drift between the pre-decoder and decoder stages.
pub(super) fn parse_access_unit(bytes: &[u8], eos: bool) -> Result<AccessUnitParse, String> {
    if bytes.len() < 8 {
        return if eos {
            Err("truncated E-AC-3 syncframe header at EOS".to_owned())
        } else {
            Ok(AccessUnitParse::NeedMore)
        };
    }

    let first = parse_syncframe_header(bytes).map_err(|error| error.to_string())?;
    if first.stream_type != StreamType::Independent || first.substream_id != 0 {
        return Err("access unit does not start with independent substream zero".to_owned());
    }
    if first.frame_size > MAX_SYNCFRAME_BYTES || first.sample_rate != SAMPLE_RATE {
        return Err(format!(
            "unsupported E-AC-3 frame size/rate: {} bytes at {} Hz",
            first.frame_size, first.sample_rate
        ));
    }
    if bytes.len() < first.frame_size {
        return if eos {
            Err("truncated first E-AC-3 syncframe at EOS".to_owned())
        } else {
            Ok(AccessUnitParse::NeedMore)
        };
    }

    if bytes.len() == first.frame_size {
        return if eos {
            Ok(AccessUnitParse::Complete(first.frame_size))
        } else {
            Ok(AccessUnitParse::NeedMore)
        };
    }

    let second_header_offset = first.frame_size;
    if bytes.len() < second_header_offset + 8 {
        return if eos {
            Err("partial second E-AC-3 syncframe header at EOS".to_owned())
        } else {
            Ok(AccessUnitParse::NeedMore)
        };
    }

    let second = parse_syncframe_header(&bytes[second_header_offset..])
        .map_err(|error| error.to_string())?;
    if second.stream_type != StreamType::Dependent && second.substream_id == 0 {
        return Ok(AccessUnitParse::Complete(first.frame_size));
    }
    if second.stream_type != StreamType::Dependent || second.substream_id != 0 {
        return Err("unsupported substream order after independent substream zero".to_owned());
    }
    if second.frame_size > MAX_SYNCFRAME_BYTES
        || second.sample_rate != SAMPLE_RATE
        || second.audio_blocks != first.audio_blocks
    {
        return Err("dependent substream timing or size does not match I0".to_owned());
    }

    let total = first
        .frame_size
        .checked_add(second.frame_size)
        .ok_or_else(|| "access unit size overflow".to_owned())?;
    if bytes.len() < total {
        return if eos {
            Err("truncated dependent-zero syncframe at EOS".to_owned())
        } else {
            Ok(AccessUnitParse::NeedMore)
        };
    }
    Ok(AccessUnitParse::Complete(total))
}

/// Classifies one complete AU using the existing public OpenJOC admission
/// parser. No filename, container, channel-count, or metadata heuristic is
/// used. A complete valid E-AC-3 AU without an admitted JOC carrier is the
/// positive ordinary-stream evidence used by the normal decoder path.
pub(super) fn classify_access_unit(bytes: &[u8]) -> JocClassification {
    let Ok(frames) = index_syncframes(bytes) else {
        return JocClassification::InvalidOrUnsupported;
    };
    let Ok(units) = group_access_units(&frames) else {
        return JocClassification::InvalidOrUnsupported;
    };
    let Some(unit) = units.first().copied() else {
        return JocClassification::InvalidOrUnsupported;
    };
    if units.len() != 1 || unit.first_frame != 0 || unit.frame_count != frames.len() {
        return JocClassification::InvalidOrUnsupported;
    }

    for entry in &frames {
        let Some(frame) = bytes.get(entry.offset..) else {
            return JocClassification::InvalidOrUnsupported;
        };
        if parse_audio_frame(frame).is_err() {
            return JocClassification::InvalidOrUnsupported;
        }
    }

    match parse_joc_access_unit(bytes, &frames, unit) {
        Ok(Some(_)) => JocClassification::ConfirmedJoc,
        Ok(None) => JocClassification::ConfirmedNonJoc,
        Err(_) => JocClassification::InvalidOrUnsupported,
    }
}

pub(super) fn classified_caps(joc: bool) -> gst::Caps {
    let builder = gst::Caps::builder("audio/x-eac3")
        .field("framed", true)
        .field("alignment", "frame")
        .field(JOC_CAPS_FIELD, joc);
    if joc {
        builder.features([JOC_CAPS_FEATURE]).build()
    } else {
        builder.build()
    }
}

fn classifier_src_caps() -> gst::Caps {
    let mut caps = classified_caps(false);
    caps.merge(classified_caps(true));
    caps
}

#[derive(Debug)]
struct ClassifierState {
    classification: JocClassification,
    classification_bytes: usize,
    classification_micros: u128,
}

impl Default for ClassifierState {
    fn default() -> Self {
        Self {
            classification: JocClassification::Unknown,
            classification_bytes: 0,
            classification_micros: 0,
        }
    }
}

#[derive(Default)]
pub(super) struct OpenJocClassifyImp {
    state: Mutex<ClassifierState>,
}

#[glib::object_subclass]
impl ObjectSubclass for OpenJocClassifyImp {
    const NAME: &'static str = "GstOpenJocClassify";
    type Type = OpenJocClassify;
    type ParentType = gst_base::BaseParse;
}

impl ObjectImpl for OpenJocClassifyImp {}
impl GstObjectImpl for OpenJocClassifyImp {}

impl ElementImpl for OpenJocClassifyImp {
    fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
        static METADATA: OnceLock<gst::subclass::ElementMetadata> = OnceLock::new();
        Some(METADATA.get_or_init(|| {
            gst::subclass::ElementMetadata::new(
                "OpenJOC-aware E-AC-3 classifier",
                "Codec/Parser/Audio",
                "Frames E-AC-3 and emits an experimental pre-decoder JOC caps discriminator",
                "OpenJOC contributors",
            )
        }))
    }

    fn pad_templates() -> &'static [gst::PadTemplate] {
        static TEMPLATES: OnceLock<Vec<gst::PadTemplate>> = OnceLock::new();
        TEMPLATES.get_or_init(|| {
            vec![
                gst::PadTemplate::new(
                    "sink",
                    gst::PadDirection::Sink,
                    gst::PadPresence::Always,
                    &gst::Caps::new_empty_simple("audio/x-eac3"),
                )
                .expect("valid OpenJOC classifier sink caps"),
                gst::PadTemplate::new(
                    "src",
                    gst::PadDirection::Src,
                    gst::PadPresence::Always,
                    &classifier_src_caps(),
                )
                .expect("valid OpenJOC classifier source caps"),
            ]
        })
    }
}

impl BaseParseImpl for OpenJocClassifyImp {
    fn start(&self) -> Result<(), gst::ErrorMessage> {
        self.parent_start()?;
        self.obj().set_min_frame_size(8);
        self.obj().set_pts_interpolation(false);
        self.obj().set_syncable(true);
        *lock(&self.state) = ClassifierState::default();
        Ok(())
    }

    fn stop(&self) -> Result<(), gst::ErrorMessage> {
        *lock(&self.state) = ClassifierState::default();
        self.parent_stop()
    }

    fn set_sink_caps(&self, caps: &gst::Caps) -> Result<(), gst::LoggableError> {
        let Some(structure) = caps.structure(0) else {
            return Err(gst::loggable_error!(
                gst::CAT_RUST,
                "OpenJOC classifier requires one audio/x-eac3 structure"
            ));
        };
        if structure.name() != "audio/x-eac3" {
            return Err(gst::loggable_error!(
                gst::CAT_RUST,
                "OpenJOC classifier requires audio/x-eac3 input, got {}",
                structure.name()
            ));
        }
        self.parent_set_sink_caps(caps)?;
        *lock(&self.state) = ClassifierState::default();
        Ok(())
    }

    fn handle_frame(
        &self,
        frame: gst_base::BaseParseFrame,
    ) -> Result<(gst::FlowSuccess, u32), gst::FlowError> {
        let Some(buffer) = frame.buffer() else {
            return Err(gst::FlowError::Error);
        };
        let bytes = buffer
            .map_readable()
            .map_err(|_| gst::FlowError::Error)?
            .as_slice()
            .to_vec();
        let eos = self.obj().is_draining();
        let size = match parse_access_unit(&bytes, eos) {
            Ok(AccessUnitParse::NeedMore) => {
                return Ok((gst::FlowSuccess::Ok, 0));
            }
            Ok(AccessUnitParse::Complete(size)) => size,
            Err(error) => {
                gst::element_imp_error!(
                    self,
                    gst::StreamError::Decode,
                    ("OpenJOC classifier rejected E-AC-3 input: {error}")
                );
                return Err(gst::FlowError::Error);
            }
        };

        let (classification, newly_classified, classification_micros) = {
            let mut state = lock(&self.state);
            if state.classification == JocClassification::Unknown {
                let started = Instant::now();
                state.classification = classify_access_unit(&bytes[..size]);
                state.classification_bytes = size;
                state.classification_micros = started.elapsed().as_micros();
                (state.classification, true, state.classification_micros)
            } else {
                (state.classification, false, state.classification_micros)
            }
        };
        if classification == JocClassification::InvalidOrUnsupported {
            gst::element_imp_error!(
                self,
                gst::StreamError::Decode,
                ("OpenJOC classifier could not establish a supported E-AC-3 classification")
            );
            return Err(gst::FlowError::Error);
        }
        if classification == JocClassification::Unknown {
            return Err(gst::FlowError::Error);
        }

        if newly_classified {
            let caps = classified_caps(classification == JocClassification::ConfirmedJoc);
            if !self
                .obj()
                .src_pad()
                .push_event(gst::event::Caps::new(&caps))
            {
                return Err(gst::FlowError::NotNegotiated);
            }
            gst::info!(
                category(),
                imp = self,
                "classified E-AC-3 as {} after {} bytes classification-us={classification_micros}",
                classification.name(),
                lock(&self.state).classification_bytes
            );
        }
        let flow = self.obj().finish_frame(frame, size as u32)?;
        Ok((flow, 0))
    }
}

glib::wrapper! {
    pub(super) struct OpenJocClassify(ObjectSubclass<OpenJocClassifyImp>) @extends gst_base::BaseParse, gst::Element, gst::Object;
}

pub(super) fn register(plugin: Option<&gst::Plugin>) -> Result<(), glib::BoolError> {
    gst::Element::register(
        plugin,
        "openjocclassify",
        gst::Rank::PRIMARY + 2,
        OpenJocClassify::static_type(),
    )
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

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

    fn header_only_frame(size: usize) -> Vec<u8> {
        let mut bytes = vec![0_u8; size];
        let mut cursor = 0;
        push_bits(&mut bytes, &mut cursor, 0x0b77, 16);
        push_bits(&mut bytes, &mut cursor, 0, 2);
        push_bits(&mut bytes, &mut cursor, 0, 3);
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

    #[test]
    fn classification_waits_for_a_complete_access_unit() {
        let bytes = header_only_frame(16);
        assert!(matches!(
            parse_access_unit(&bytes[..7], false),
            Ok(AccessUnitParse::NeedMore)
        ));
        assert!(matches!(
            parse_access_unit(&bytes, false),
            Ok(AccessUnitParse::NeedMore)
        ));
        assert!(matches!(
            parse_access_unit(&bytes, true),
            Ok(AccessUnitParse::Complete(16))
        ));
    }

    #[test]
    fn malformed_complete_data_is_not_a_non_joc_positive() {
        let mut bytes = header_only_frame(16);
        bytes[0] = 0;
        assert_eq!(
            classify_access_unit(&bytes),
            JocClassification::InvalidOrUnsupported
        );
    }

    #[test]
    fn classified_caps_use_the_project_feature_only_for_positive_joc() {
        gst::init().expect("GStreamer initializes");
        let ordinary = classified_caps(false);
        let joc = classified_caps(true);
        assert!(
            ordinary
                .features(0)
                .is_none_or(|features| { !features.contains(JOC_CAPS_FEATURE) })
        );
        assert!(
            joc.features(0)
                .is_some_and(|features| features.contains(JOC_CAPS_FEATURE))
        );
        assert_eq!(
            ordinary
                .structure(0)
                .and_then(|s| s.get::<bool>(JOC_CAPS_FIELD).ok()),
            Some(false)
        );
        assert_eq!(
            joc.structure(0)
                .and_then(|s| s.get::<bool>(JOC_CAPS_FIELD).ok()),
            Some(true)
        );
    }
}
