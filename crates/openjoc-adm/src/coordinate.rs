// pattern: Functional Core

use openjoc_scene::Position3 as ScenePosition3;
use std::fmt;

const OAMD_XY_RANGE: std::ops::RangeInclusive<f64> = 0.0..=1.0;
const OAMD_Z_RANGE: std::ops::RangeInclusive<f64> = -1.0..=1.0;
const ADM_RANGE: std::ops::RangeInclusive<f64> = -1.0..=1.0;

/// Coordinate validation failures at the OAMD-to-ADM semantic boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CoordinateError {
    NonFinite,
    OamdOutOfRange,
    AdmOutOfRange,
}

impl fmt::Display for CoordinateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinite => formatter.write_str("coordinate contains a non-finite value"),
            Self::OamdOutOfRange => {
                formatter.write_str("OAMD coordinate is outside the supported normalized range")
            }
            Self::AdmOutOfRange => {
                formatter.write_str("ADM coordinate is outside the normalized range")
            }
        }
    }
}

impl std::error::Error for CoordinateError {}

/// A decoded OAMD room-coordinate position in the admitted in-room profile.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct OamdCartesianPosition {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) z: f64,
}

impl OamdCartesianPosition {
    fn new(x: f64, y: f64, z: f64) -> Result<Self, CoordinateError> {
        if ![x, y, z].into_iter().all(f64::is_finite) {
            return Err(CoordinateError::NonFinite);
        }
        if !OAMD_XY_RANGE.contains(&x) || !OAMD_XY_RANGE.contains(&y) || !OAMD_Z_RANGE.contains(&z)
        {
            return Err(CoordinateError::OamdOutOfRange);
        }
        Ok(Self { x, y, z })
    }
}

impl TryFrom<ScenePosition3> for OamdCartesianPosition {
    type Error = CoordinateError;

    fn try_from(position: ScenePosition3) -> Result<Self, Self::Error> {
        Self::new(position.x, position.y, position.z)
    }
}

/// A normalized ADM Cartesian position ready for `cartesian=1` XML output.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AdmCartesianPosition {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) z: f64,
}

impl AdmCartesianPosition {
    fn new(x: f64, y: f64, z: f64) -> Result<Self, CoordinateError> {
        if ![x, y, z].into_iter().all(f64::is_finite) {
            return Err(CoordinateError::NonFinite);
        }
        if !ADM_RANGE.contains(&x) || !ADM_RANGE.contains(&y) || !ADM_RANGE.contains(&z) {
            return Err(CoordinateError::AdmOutOfRange);
        }
        Ok(Self { x, y, z })
    }
}

impl TryFrom<OamdCartesianPosition> for AdmCartesianPosition {
    type Error = CoordinateError;

    fn try_from(position: OamdCartesianPosition) -> Result<Self, Self::Error> {
        Self::new(
            2.0_f64.mul_add(position.x, -1.0),
            1.0 - 2.0 * position.y,
            position.z,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn convert(x: f64, y: f64, z: f64) -> AdmCartesianPosition {
        let oamd = OamdCartesianPosition::new(x, y, z).expect("valid OAMD coordinate");
        AdmCartesianPosition::try_from(oamd).expect("valid ADM coordinate")
    }

    #[test]
    fn maps_center_front() {
        assert_eq!(
            convert(0.5, 0.0, 0.0),
            AdmCartesianPosition {
                x: 0.0,
                y: 1.0,
                z: 0.0
            }
        );
    }

    #[test]
    fn maps_front_left_and_front_right() {
        assert_eq!(
            convert(0.0, 0.0, 0.0),
            AdmCartesianPosition {
                x: -1.0,
                y: 1.0,
                z: 0.0
            }
        );
        assert_eq!(
            convert(1.0, 0.0, 0.0),
            AdmCartesianPosition {
                x: 1.0,
                y: 1.0,
                z: 0.0
            }
        );
    }

    #[test]
    fn maps_rear_and_horizontal_midpoint() {
        assert_eq!(
            convert(0.5, 1.0, 0.0),
            AdmCartesianPosition {
                x: 0.0,
                y: -1.0,
                z: 0.0
            }
        );
        assert_eq!(
            convert(0.25, 0.5, 0.0),
            AdmCartesianPosition {
                x: -0.5,
                y: 0.0,
                z: 0.0
            }
        );
    }

    #[test]
    fn preserves_height_endpoints_and_intermediate_value() {
        assert_eq!(convert(0.5, 0.0, -1.0).z, -1.0);
        assert_eq!(convert(0.5, 0.0, 0.4).z, 0.4);
        assert_eq!(convert(0.5, 0.0, 1.0).z, 1.0);
    }

    #[test]
    fn rejects_non_finite_and_unsupported_ranges() {
        assert_eq!(
            OamdCartesianPosition::new(f64::NAN, 0.0, 0.0),
            Err(CoordinateError::NonFinite)
        );
        assert_eq!(
            OamdCartesianPosition::new(-0.1, 0.0, 0.0),
            Err(CoordinateError::OamdOutOfRange)
        );
        assert_eq!(
            AdmCartesianPosition::new(1.1, 0.0, 0.0),
            Err(CoordinateError::AdmOutOfRange)
        );
    }
}
