//! Stateless complete-AU inspection shared by adapter classifiers.
//!
//! Classification does not replace session admission. The explicit policy
//! preserves each adapter's existing metadata rules; no stream, timestamp,
//! staging, caps, negotiation, or end-of-stream state belongs here.

use openjoc_eac3::{
    AccessUnitIndex, StreamType, SyncframeIndexEntry, group_access_units, index_syncframes,
    parse_audio_frame, parse_joc_access_unit, validate_joc_access_unit,
    validate_joc_access_unit_decoder_contract,
};
use openjoc_emdf::JocValidationProfile;

/// Existing metadata admission choices for complete-AU classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdmissionPolicy {
    /// FFmpeg's strict profile, falling back only to the observed-vendor profile.
    StrictOrObservedVendor,
    /// GStreamer's first JOC payload, without EMDF profile validation.
    /// Payload syntax and decoder topology are still checked.
    FirstJocPayload,
}

/// Result for a complete AU; an adapter's not-yet-classified state is separate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JocClassification {
    ConfirmedJoc,
    ConfirmedNonJoc,
    InvalidOrUnsupported,
}

/// Parser facts retained by the FFmpeg bridge after classification.
#[derive(Debug)]
pub struct AccessUnitInspection {
    pub classification: JocClassification,
    pub unit: AccessUnitIndex,
    pub frames: Vec<SyncframeIndexEntry>,
    pub independent_frame_count: usize,
    pub dependent_frame_count: usize,
}

/// Inspect exactly one complete AU using an explicit adapter metadata policy.
///
/// Profile rejection remains an `InvalidOrUnsupported` classification, while
/// parser/decoder-contract failures retain their original display text as an
/// error. Adapters decide how to expose that distinction.
pub fn inspect_complete_access_unit(
    bytes: &[u8],
    policy: AdmissionPolicy,
) -> Result<AccessUnitInspection, String> {
    let frames = index_syncframes(bytes).map_err(|error| error.to_string())?;
    let units = group_access_units(&frames).map_err(|error| error.to_string())?;
    let Some(unit) = units.first().copied() else {
        return Err("input contains no E-AC-3 access unit".to_owned());
    };
    if units.len() != 1 || unit.first_frame != 0 || unit.frame_count != frames.len() {
        return Err("classification requires exactly one complete access unit".to_owned());
    }
    for entry in &frames {
        let frame = bytes
            .get(entry.offset..)
            .ok_or_else(|| "syncframe offset is outside input".to_owned())?;
        parse_audio_frame(frame).map_err(|error| error.to_string())?;
    }
    let parsed = parse_joc_access_unit(bytes, &frames, unit).map_err(|error| error.to_string())?;
    let accepted_metadata = match policy {
        AdmissionPolicy::StrictOrObservedVendor => parsed.as_ref().and_then(|parsed| {
            validate_joc_access_unit(parsed, JocValidationProfile::EtsiStrict)
                .or_else(|_| {
                    validate_joc_access_unit(parsed, JocValidationProfile::ObservedVendorCompat)
                })
                .ok()
        }),
        AdmissionPolicy::FirstJocPayload => None,
    };
    let joc_payload = match policy {
        AdmissionPolicy::StrictOrObservedVendor => accepted_metadata
            .as_ref()
            .map(|metadata| metadata.joc.as_slice()),
        AdmissionPolicy::FirstJocPayload => parsed.as_ref().and_then(|metadata| {
            metadata
                .emdf
                .payloads
                .iter()
                .find(|payload| payload.id == 14)
                .map(|payload| payload.data.as_slice())
        }),
    };
    let classification = if parsed.is_none() {
        JocClassification::ConfirmedNonJoc
    } else if joc_payload.is_some() {
        JocClassification::ConfirmedJoc
    } else {
        JocClassification::InvalidOrUnsupported
    };
    if classification == JocClassification::ConfirmedJoc {
        let payload = joc_payload.ok_or_else(|| "missing admitted JOC metadata".to_owned())?;
        let joc = openjoc_joc::parse_joc_payload(payload).map_err(|error| error.to_string())?;
        validate_joc_access_unit_decoder_contract(
            bytes,
            &frames,
            unit,
            joc.header.downmix_index,
            joc.header.channel_count,
        )
        .map_err(|error| error.to_string())?;
    }
    let independent_frame_count = frames
        .iter()
        .filter(|entry| {
            matches!(
                entry.header.stream_type,
                StreamType::LegacyIndependent | StreamType::Independent
            )
        })
        .count();
    let dependent_frame_count = frames.len().saturating_sub(independent_frame_count);
    Ok(AccessUnitInspection {
        classification,
        unit,
        frames,
        independent_frame_count,
        dependent_frame_count,
    })
}
