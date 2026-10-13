//! Immutable provenance for one successful JOC syntax parse.

use crate::{JocFrame, JocParseError, parse_joc_payload};

/// A successful JOC syntax parse tied to every original input byte.
///
/// This proves syntax only, not reconstruction, programme-layout or binding
/// admission. The retained result and byte provenance cannot be mutated or
/// replaced by a caller-supplied value. Only metadata bytes are copied.
#[derive(Debug)]
pub struct ParsedJocProof {
    raw: Box<[u8]>,
    parsed: JocFrame,
}

impl ParsedJocProof {
    /// Runs the existing parser and retains a proof only after success.
    ///
    /// # Errors
    /// Returns the existing parser's error unchanged.
    pub fn parse(raw: &[u8]) -> Result<Self, JocParseError> {
        let parsed = parse_joc_payload(raw)?;
        Ok(Self {
            raw: raw.into(),
            parsed,
        })
    }

    /// Borrows the original parsed result without exposing mutable provenance.
    #[must_use]
    pub fn parsed(&self) -> &JocFrame {
        &self.parsed
    }

    /// Moves the parsed result only when the complete byte slice matches.
    /// A mismatch is a cache miss: callers must use their original parser.
    #[must_use]
    pub fn into_matching_frame(self, raw: &[u8]) -> Option<JocFrame> {
        if self.raw.as_ref() == raw {
            Some(self.parsed)
        } else {
            None
        }
    }
}
