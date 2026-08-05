//! Evaluation-only decoder comparison contracts.
//!
//! These types describe how decoded samples are measured. They never alter
//! decoding, trimming, presentation, or stateful decoder behavior.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparisonRegion {
    ColdStart,
    Warmup,
    SteadyState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MappingConfidence {
    High,
    Medium,
    Low,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SampleRange {
    pub start: u64,
    pub end: u64,
}

impl SampleRange {
    pub fn validate(self) -> Result<(), &'static str> {
        if self.start >= self.end {
            return Err("sample range must be non-empty and increasing");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TargetAuMapping {
    pub source_au_index: u64,
    pub target_hash: String,
    pub corpus_au_index: u64,
    pub decoded_sample_range: SampleRange,
    pub authored_sample_range: Option<SampleRange>,
    pub packet_pts: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DecoderComparisonContract {
    pub decoder: String,
    pub input_path: String,
    pub elementary_stream_sha256: String,
    pub sample_rate_hz: u32,
    pub samples_per_access_unit: u32,
    pub mapping_confidence: MappingConfidence,
    pub leading_samples: Option<u64>,
    pub trailing_samples: Option<u64>,
    pub warmup_access_unit_count: Option<u64>,
    pub regions: BTreeMap<ComparisonRegion, SampleRange>,
    pub target_au_mappings: Vec<TargetAuMapping>,
    pub evaluation_only: bool,
}

impl DecoderComparisonContract {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.decoder.is_empty() || self.elementary_stream_sha256.is_empty() {
            return Err("decoder and elementary stream hash are required");
        }
        if self.sample_rate_hz == 0 || self.samples_per_access_unit == 0 {
            return Err("sample rate and access-unit size must be non-zero");
        }
        if !self.evaluation_only {
            return Err("comparison contract must be evaluation-only");
        }
        for range in self.regions.values() {
            range.validate()?;
        }
        for mapping in &self.target_au_mappings {
            if mapping.target_hash.is_empty() {
                return Err("target AU hash is required");
            }
            mapping.decoded_sample_range.validate()?;
            if let Some(range) = mapping.authored_sample_range {
                range.validate()?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contract() -> DecoderComparisonContract {
        let mut regions = BTreeMap::new();
        regions.insert(
            ComparisonRegion::ColdStart,
            SampleRange {
                start: 0,
                end: 1536,
            },
        );
        regions.insert(
            ComparisonRegion::Warmup,
            SampleRange {
                start: 1536,
                end: 3072,
            },
        );
        regions.insert(
            ComparisonRegion::SteadyState,
            SampleRange {
                start: 3072,
                end: 4608,
            },
        );
        DecoderComparisonContract {
            decoder: "OpenJOC".to_owned(),
            input_path: "private/history/H0.ec3".to_owned(),
            elementary_stream_sha256: "stream-hash".to_owned(),
            sample_rate_hz: 48_000,
            samples_per_access_unit: 1536,
            mapping_confidence: MappingConfidence::High,
            leading_samples: Some(0),
            trailing_samples: Some(0),
            warmup_access_unit_count: Some(1),
            regions,
            target_au_mappings: vec![TargetAuMapping {
                source_au_index: 1,
                target_hash: "au-hash".to_owned(),
                corpus_au_index: 2,
                decoded_sample_range: SampleRange {
                    start: 3072,
                    end: 4608,
                },
                authored_sample_range: Some(SampleRange {
                    start: 1536,
                    end: 3072,
                }),
                packet_pts: None,
            }],
            evaluation_only: true,
        }
    }

    #[test]
    fn validates_hash_mapping_and_regions() {
        assert!(contract().validate().is_ok());
        let mut invalid = contract();
        invalid.target_au_mappings[0].target_hash.clear();
        assert_eq!(invalid.validate(), Err("target AU hash is required"));
    }

    #[test]
    fn rejects_unknown_or_mutating_contracts() {
        let mut invalid = contract();
        invalid.regions.insert(
            ComparisonRegion::SteadyState,
            SampleRange { start: 10, end: 10 },
        );
        assert_eq!(
            invalid.validate(),
            Err("sample range must be non-empty and increasing")
        );
        let mut mutating = contract();
        mutating.evaluation_only = false;
        assert_eq!(
            mutating.validate(),
            Err("comparison contract must be evaluation-only")
        );
    }

    #[test]
    fn serialization_is_deterministic() {
        let value = contract();
        let first = serde_json::to_vec_pretty(&value).expect("serialize");
        let second = serde_json::to_vec_pretty(&value).expect("serialize");
        assert_eq!(first, second);
    }
}
