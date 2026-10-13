//! Immutable provenance for reusing an existing successful syntax parse.

use crate::{
    OamdDecoderConfig, OamdError, OamdParseProfile, OamdPayload, parse_oamd_payload_with_config,
    parse_oamd_payload_with_profile,
};

/// A successful OAMD syntax parse tied to its exact bytes and parser arguments.
///
/// This is not scene, programme-layout or decoded-binding admission. Callers
/// must still perform those checks. The private provenance cannot be replaced
/// by a caller-supplied parsed value, and the parsed view is immutable.
/// Construction snapshots metadata bytes only; it does not copy audio PCM.
#[derive(Debug)]
pub struct ParsedOamdProof {
    raw: Box<[u8]>,
    config: OamdDecoderConfig,
    profile: OamdParseProfile,
    payload_id: Option<u64>,
    parsed: OamdPayload,
}

impl ParsedOamdProof {
    /// Runs the existing strict-config parser and retains its successful result.
    /// Its errors are returned unchanged; no proof is created on rejection.
    pub fn parse_with_config(raw: &[u8], config: OamdDecoderConfig) -> Result<Self, OamdError> {
        let parsed = parse_oamd_payload_with_config(raw, config)?;
        Ok(Self {
            raw: raw.into(),
            config,
            profile: OamdParseProfile::EtsiStrict,
            payload_id: None,
            parsed,
        })
    }

    /// Runs the existing explicit-profile parser with its original identifier
    /// checks and errors, retaining a proof only after successful parsing.
    pub fn parse_with_profile(
        raw: &[u8],
        config: OamdDecoderConfig,
        profile: OamdParseProfile,
        payload_id: u64,
    ) -> Result<Self, OamdError> {
        let parsed = parse_oamd_payload_with_profile(raw, config, profile, payload_id)?;
        Ok(Self {
            raw: raw.into(),
            config,
            profile,
            payload_id: Some(payload_id),
            parsed,
        })
    }

    /// Borrows the original parsed result without exposing mutable provenance.
    #[must_use]
    pub fn parsed(&self) -> &OamdPayload {
        &self.parsed
    }

    /// Moves out the parsed result only for exactly the same complete bytes,
    /// configuration, profile and entry-point identifier provenance.
    ///
    /// A mismatch is a cache miss, not an admission error: callers should use
    /// their original parser and checks. `None` and `Some` identifiers remain
    /// distinct, as do distinct configurations even when a payload would not
    /// exercise their difference. All bytes, including padding, are compared.
    #[must_use]
    pub fn into_matching_payload(
        self,
        raw: &[u8],
        config: OamdDecoderConfig,
        profile: OamdParseProfile,
        payload_id: Option<u64>,
    ) -> Option<OamdPayload> {
        if self.raw.as_ref() == raw
            && self.config == config
            && self.profile == profile
            && self.payload_id == payload_id
        {
            Some(self.parsed)
        } else {
            None
        }
    }
}
