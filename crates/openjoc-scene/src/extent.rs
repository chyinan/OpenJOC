//! Ordinary metadata-driven dynamic-object extent target generation.
//!
//! This module is intentionally separate from the generic point projector.
//! It owns only the clean-room ordinary extent field; scheduling, source
//! scaling, accumulation, LFE handling, and constrained Region selection stay
//! at their existing boundaries.

use crate::{
    SpatialDescriptor, SpatialLayout, SpatialLayoutChannel, SpatialLayoutTopology,
    SpatialProjectionError, SpatialSourceClass,
};

const Q: f64 = 32_768.0;
const ZERO_FIELD_THRESHOLD: f64 = 2.5e-5;
const MAX_RADIUS: f64 = 0.7;
const RADIUS_KNOT_COUNT: usize = 20;
const XY_CENTER_KNOT_COUNT: usize = 35;
const Z_CENTER_KNOT_COUNT: usize = 4;
const X_Y_SAMPLE_COUNT: usize = 20;
const Z_SAMPLE_COUNT: usize = 8;
const KERNEL_COEFFICIENT: f64 = 16.8172606265625;
const RESPONSE_SCALE: f64 = 0.5 * 0.6328125;

/// Converts the bridge's quantized semantic extent components to the exact
/// ordinary renderer scalar. The bridge stores normalized Q15 values, while
/// direct callers may supply the equivalent normalized five-bit value.
#[allow(clippy::cast_sign_loss)]
pub(crate) fn extent_scalar(components: [f64; 3]) -> Result<(u32, f64), SpatialProjectionError> {
    let q = components.map(|value| {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(SpatialProjectionError::InvalidExtent);
        }
        Ok((value.mul_add(Q, 0.5).floor().min(32_767.0)) as u32)
    });
    let [ex, ey, ez] = q;
    let ex = ex?;
    let ey = ey?;
    let ez = ez?;
    let mean_q = (ex + ey + ez) / 3;
    Ok((mean_q, f64::from(mean_q) / Q))
}

/// Returns the five-knot scalar-to-radius transfer.
pub(crate) fn extent_radius(scalar: f64) -> f64 {
    const KNOTS: [(f64, f64); 5] = [
        (0.0, 0.0),
        (0.2, 0.075),
        (0.5, 0.25),
        (0.75, 0.45),
        (1.0, 0.7),
    ];
    let scalar = scalar.clamp(0.0, 1.0);
    if scalar <= KNOTS[0].0 {
        return KNOTS[0].1;
    }
    for window in KNOTS.windows(2) {
        let [(e0, s0), (e1, s1)] = window else {
            unreachable!("five-knot windows have two entries");
        };
        if scalar <= *e1 {
            let t = (scalar - e0) / (e1 - e0);
            return s0 + t * (s1 - s0);
        }
    }
    KNOTS[KNOTS.len() - 1].1
}

/// Evaluates the compact quartic kernel used by the sampled response field.
pub(crate) fn compact_quartic_kernel(u: f64, center: f64, radius: f64, z_axis: bool) -> f64 {
    if radius < ZERO_FIELD_THRESHOLD {
        return 0.0;
    }
    let normalized_distance = (u - center) / 4.0;
    if normalized_distance.abs() > radius {
        return 0.0;
    }
    let ratio = normalized_distance / radius;
    let value = exp2a(-KERNEL_COEFFICIENT * ratio * ratio * ratio * ratio);
    if !value.is_finite() || value <= 0.0 {
        return 0.0;
    }
    if z_axis {
        value * (1.365909936 * u).cos()
    } else {
        value
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Axis {
    X,
    Y,
    Z,
}

#[derive(Clone, Debug, PartialEq)]
struct AxisResponseTable {
    centers: Vec<f64>,
    radii: Vec<f64>,
    channels: usize,
    values: Vec<f64>,
    endpoint_responses: [[Vec<f64>; 2]; 1],
    z_axis: bool,
}

impl AxisResponseTable {
    fn new(
        topology: &SpatialLayoutTopology,
        channels: &[SpatialLayoutChannel],
        axis: Axis,
    ) -> Result<Self, SpatialProjectionError> {
        let z_axis = matches!(axis, Axis::Z);
        let centers = if z_axis {
            (0..Z_CENTER_KNOT_COUNT)
                .map(|index| index as f64 / (Z_CENTER_KNOT_COUNT - 1) as f64)
                .collect::<Vec<_>>()
        } else {
            (0..XY_CENTER_KNOT_COUNT)
                .map(|index| index as f64 / (XY_CENTER_KNOT_COUNT - 1) as f64)
                .collect::<Vec<_>>()
        };
        let radii = (0..RADIUS_KNOT_COUNT)
            .map(|index| MAX_RADIUS * index as f64 / (RADIUS_KNOT_COUNT - 1) as f64)
            .collect::<Vec<_>>();
        let sample_count = if z_axis {
            Z_SAMPLE_COUNT
        } else {
            X_Y_SAMPLE_COUNT
        };
        let samples = (0..sample_count)
            .map(|index| {
                let u = index as f64 / (sample_count - 1) as f64;
                let response = axis_response(topology, channels, axis, u)?;
                Ok((u, response))
            })
            .collect::<Result<Vec<_>, SpatialProjectionError>>()?;
        let endpoint_responses = [
            endpoint_response(topology, channels, axis, 0.0)?,
            endpoint_response(topology, channels, axis, 1.0)?,
        ];
        let mut values = vec![0.0; centers.len() * radii.len() * channels.len()];
        for (center_index, &center) in centers.iter().enumerate() {
            for (radius_index, &radius) in radii.iter().enumerate() {
                let offset = (center_index * radii.len() + radius_index) * channels.len();
                let response = sampled_response(&samples, center, radius, z_axis, channels.len());
                values[offset..offset + channels.len()].copy_from_slice(&response);
            }
        }
        Ok(Self {
            centers,
            radii,
            channels: channels.len(),
            values,
            endpoint_responses: [endpoint_responses],
            z_axis,
        })
    }

    fn lookup(&self, center: f64, radius: f64) -> Vec<f64> {
        let center = center.clamp(0.0, 1.0);
        let radius = radius.clamp(0.0, MAX_RADIUS);
        let (center0, center1, center_t) = bracket(&self.centers, center);
        let (radius0, radius1, radius_t) = bracket(&self.radii, radius);
        let mut result = vec![0.0; self.channels];
        for (channel, result) in result.iter_mut().enumerate() {
            let v00 = self.value(center0, radius0, channel);
            let v01 = self.value(center0, radius1, channel);
            let v10 = self.value(center1, radius0, channel);
            let v11 = self.value(center1, radius1, channel);
            let lower = v00 + radius_t * (v01 - v00);
            let upper = v10 + radius_t * (v11 - v10);
            *result = lower + center_t * (upper - lower);
        }
        result
    }

    fn endpoint_lookup(&self, endpoint: usize, center: f64, radius: f64) -> Vec<f64> {
        let response = &self.endpoint_responses[0][endpoint];
        sampled_endpoint_response(response, endpoint as f64, center, radius, self.z_axis)
    }

    fn value(&self, center: usize, radius: usize, channel: usize) -> f64 {
        self.values[(center * self.radii.len() + radius) * self.channels + channel]
    }
}

/// Immutable response-field preparation for one validated layout/topology.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ExtentFieldCache {
    x: AxisResponseTable,
    y: AxisResponseTable,
    z: Option<AxisResponseTable>,
    active_channels: usize,
}

impl ExtentFieldCache {
    pub(crate) fn build(layout: &SpatialLayout) -> Result<Self, SpatialProjectionError> {
        let layers = &layout.topology().layers;
        if layers.is_empty() || layers.len() > 2 {
            return Err(SpatialProjectionError::UnadmittedLayerPolicy);
        }
        let channels = layout
            .channels()
            .iter()
            .filter(|channel| channel.enabled && !channel.lfe)
            .cloned()
            .collect::<Vec<_>>();
        if channels.is_empty() {
            return Err(SpatialProjectionError::InvalidLayout(
                "no active non-LFE channels",
            ));
        }
        let x = AxisResponseTable::new(layout.topology(), &channels, Axis::X)?;
        let y = AxisResponseTable::new(layout.topology(), &channels, Axis::Y)?;
        let z = (layers.len() == 2)
            .then(|| AxisResponseTable::new(layout.topology(), &channels, Axis::Z))
            .transpose()?;
        Ok(Self {
            x,
            y,
            z,
            active_channels: channels.len(),
        })
    }

    pub(crate) fn project(
        &self,
        layout: &SpatialLayout,
        descriptor: &SpatialDescriptor,
    ) -> Result<Vec<f64>, SpatialProjectionError> {
        let Some(components) = descriptor.extent else {
            return layout.project_unconstrained(descriptor);
        };
        let (mean_q, scalar) = extent_scalar(components)?;
        let radius = extent_radius(scalar);
        let mut point_descriptor = descriptor.clone();
        point_descriptor.source_class = SpatialSourceClass::DynamicPoint;
        point_descriptor.extent = None;
        point_descriptor.spread = None;
        let point = layout.project_unconstrained(&point_descriptor)?;
        if mean_q == 0 || radius < ZERO_FIELD_THRESHOLD {
            return Ok(point);
        }
        let center = room_center(layout, descriptor)?;

        let diffuse = self.diffuse(center, radius);
        let mut result = if radius <= 0.05 {
            let a = (20.0 * radius).clamp(0.0, 1.0);
            let point_weight = 0.5 * (std::f64::consts::PI * a / 2.0).cos();
            let diffuse_weight = 0.5 * (std::f64::consts::PI * a / 2.0).sin();
            point
                .iter()
                .zip(diffuse)
                .map(|(point, diffuse)| point_weight * point + diffuse_weight * diffuse)
                .collect::<Vec<_>>()
        } else {
            diffuse.into_iter().map(|value| 0.5 * value).collect()
        };
        normalize(&mut result);
        Ok(result)
    }

    fn diffuse(&self, center: [f64; 3], radius: f64) -> Vec<f64> {
        let x = self.x.lookup(center[0], radius);
        let y = self.y.lookup(center[1], radius);
        let z = self.z.as_ref().map(|table| table.lookup(center[2], radius));
        let mut interior = vec![0.0; self.active_channels];
        for (channel, value) in interior.iter_mut().enumerate() {
            *value = x[channel] * y[channel] * z.as_ref().map_or(1.0, |z| z[channel]);
        }

        let distances = [
            (center[0], Axis::X, 0_usize),
            (1.0 - center[0], Axis::X, 1_usize),
            (center[1], Axis::Y, 0_usize),
            (1.0 - center[1], Axis::Y, 1_usize),
        ];
        let nearest = if self.z.is_some() {
            distances
                .into_iter()
                .chain([(1.0 - center[2], Axis::Z, 1_usize)])
                .min_by(|left, right| left.0.total_cmp(&right.0))
                .expect("boundary distance list is nonempty")
        } else {
            distances
                .into_iter()
                .min_by(|left, right| left.0.total_cmp(&right.0))
                .expect("boundary distance list is nonempty")
        };
        let distance = nearest.0;
        let lambda = if (distance / 4.0 <= radius || distance / 4.0 <= 0.05)
            && radius >= ZERO_FIELD_THRESHOLD
        {
            25.0 * distance * distance * distance / (4.0 * radius)
        } else {
            0.0
        };
        if lambda == 0.0 {
            return normalize_field(interior);
        }

        let endpoint = match nearest.1 {
            Axis::X => self.x.endpoint_lookup(nearest.2, center[0], radius),
            Axis::Y => self.y.endpoint_lookup(nearest.2, center[1], radius),
            Axis::Z => self
                .z
                .as_ref()
                .expect("Z boundary only exists with a Z table")
                .endpoint_lookup(nearest.2, center[2], radius),
        };
        let mut compensated = vec![0.0; self.active_channels];
        for channel in 0..self.active_channels {
            let endpoint_product = match nearest.1 {
                Axis::X => endpoint[channel] * y[channel] * z.as_ref().map_or(1.0, |z| z[channel]),
                Axis::Y => endpoint[channel] * x[channel] * z.as_ref().map_or(1.0, |z| z[channel]),
                Axis::Z => endpoint[channel] * x[channel] * y[channel],
            };
            let field = RESPONSE_SCALE * (interior[channel] + lambda * endpoint_product);
            compensated[channel] = if field > 0.0 {
                exp2a(log2a(field) / shaping_exponent(radius))
            } else {
                0.0
            };
        }
        normalize_field(compensated)
    }
}

fn endpoint_response(
    topology: &SpatialLayoutTopology,
    channels: &[SpatialLayoutChannel],
    axis: Axis,
    endpoint: f64,
) -> Result<Vec<f64>, SpatialProjectionError> {
    axis_response(topology, channels, axis, endpoint)
}

fn axis_response(
    topology: &SpatialLayoutTopology,
    channels: &[SpatialLayoutChannel],
    axis: Axis,
    coordinate: f64,
) -> Result<Vec<f64>, SpatialProjectionError> {
    let mut response = vec![0.0; channels.len()];
    match axis {
        Axis::X => {
            for layer in &topology.layers {
                for row in &layer.rows {
                    add_x_row_response(row, coordinate, channels, &mut response)?;
                }
            }
        }
        Axis::Y => {
            for layer in &topology.layers {
                let rows = selected_rows(&layer.rows, coordinate);
                for (row_index, weight) in rows {
                    for anchor in &layer.rows[row_index].anchors {
                        add_channel_value(
                            anchor.identity.as_str(),
                            weight,
                            channels,
                            &mut response,
                        )?;
                    }
                }
            }
        }
        Axis::Z => {
            let layer_weights = selected_layer_weights(topology, coordinate);
            for (layer_index, weight) in layer_weights {
                for row in &topology.layers[layer_index].rows {
                    for anchor in &row.anchors {
                        add_channel_value(
                            anchor.identity.as_str(),
                            weight,
                            channels,
                            &mut response,
                        )?;
                    }
                }
            }
        }
    }
    Ok(response)
}

fn add_x_row_response(
    row: &crate::SpatialLayoutRow,
    coordinate: f64,
    channels: &[SpatialLayoutChannel],
    response: &mut [f64],
) -> Result<(), SpatialProjectionError> {
    let anchors = &row.anchors;
    let (first, second, first_weight, second_weight) = if coordinate <= anchors[0].x {
        (0, 0, 1.0, 0.0)
    } else if coordinate >= anchors[anchors.len() - 1].x {
        let last = anchors.len() - 1;
        (last, last, 1.0, 0.0)
    } else {
        let upper = anchors.partition_point(|anchor| anchor.x < coordinate);
        let lower = upper - 1;
        let t = (coordinate - anchors[lower].x) / (anchors[upper].x - anchors[lower].x);
        (
            lower,
            upper,
            (std::f64::consts::PI * t / 2.0).cos(),
            (std::f64::consts::PI * t / 2.0).sin(),
        )
    };
    add_channel_value(
        anchors[first].identity.as_str(),
        first_weight,
        channels,
        response,
    )?;
    if first != second {
        add_channel_value(
            anchors[second].identity.as_str(),
            second_weight,
            channels,
            response,
        )?;
    }
    Ok(())
}

fn add_channel_value(
    identity: &str,
    value: f64,
    channels: &[SpatialLayoutChannel],
    response: &mut [f64],
) -> Result<(), SpatialProjectionError> {
    let index = channels
        .iter()
        .position(|channel| channel.identity == identity)
        .ok_or_else(|| SpatialProjectionError::MissingAnchor(identity.to_owned()))?;
    response[index] += value;
    Ok(())
}

fn selected_rows(rows: &[crate::SpatialLayoutRow], coordinate: f64) -> Vec<(usize, f64)> {
    if coordinate <= rows[0].y {
        return vec![(0, 1.0)];
    }
    if coordinate >= rows[rows.len() - 1].y {
        return vec![(rows.len() - 1, 1.0)];
    }
    let upper = rows.partition_point(|row| row.y < coordinate);
    let lower = upper - 1;
    let t = (coordinate - rows[lower].y) / (rows[upper].y - rows[lower].y);
    vec![
        (lower, (std::f64::consts::PI * t / 2.0).cos()),
        (upper, (std::f64::consts::PI * t / 2.0).sin()),
    ]
}

fn selected_layer_weights(topology: &SpatialLayoutTopology, coordinate: f64) -> Vec<(usize, f64)> {
    match topology.layers.len() {
        1 => vec![(0, 1.0)],
        2 if coordinate <= 0.0 => vec![(0, 1.0)],
        2 if (32_768.0 * coordinate).floor() >= 32_767.0 => vec![(1, 1.0)],
        2 => vec![
            (0, (std::f64::consts::PI * coordinate / 2.0).cos()),
            (1, (std::f64::consts::PI * coordinate / 2.0).sin()),
        ],
        _ => Vec::new(),
    }
}

fn sampled_response(
    samples: &[(f64, Vec<f64>)],
    center: f64,
    radius: f64,
    z_axis: bool,
    channels: usize,
) -> Vec<f64> {
    let exponent = shaping_exponent(radius);
    let mut response = vec![0.0; channels];
    for &(u, ref sample) in samples {
        let kernel = compact_quartic_kernel(u, center, radius, z_axis);
        if kernel <= 0.0 {
            continue;
        }
        for (channel, value) in response.iter_mut().enumerate() {
            *value += shaped(kernel * sample[channel], exponent);
        }
    }
    response
}

fn sampled_endpoint_response(
    response: &[f64],
    endpoint: f64,
    center: f64,
    radius: f64,
    z_axis: bool,
) -> Vec<f64> {
    let kernel = compact_quartic_kernel(endpoint, center, radius, z_axis);
    let exponent = shaping_exponent(radius);
    response
        .iter()
        .map(|value| shaped(kernel * *value, exponent))
        .collect()
}

fn shaped(value: f64, exponent: f64) -> f64 {
    if value > 0.0 {
        let result = exp2a(exponent * log2a(value));
        if result.is_finite() && result > 0.0 {
            result
        } else {
            0.0
        }
    } else {
        0.0
    }
}

fn shaping_exponent(radius: f64) -> f64 {
    if radius <= 0.125 {
        6.0
    } else {
        6.86956501 - 6.95652199 * radius
    }
}

fn normalize_field(mut values: Vec<f64>) -> Vec<f64> {
    normalize(&mut values);
    values
}

fn normalize(values: &mut [f64]) {
    let norm = values
        .iter()
        .fold(0.0_f64, |sum, value| sum + value * value)
        .sqrt();
    if norm.is_finite() && norm > 0.0 {
        for value in values {
            *value /= norm;
        }
    } else {
        values.fill(0.0);
    }
}

fn room_center(
    layout: &SpatialLayout,
    descriptor: &SpatialDescriptor,
) -> Result<[f64; 3], SpatialProjectionError> {
    let dimension = layout.coordinate_dimension_count();
    if descriptor.coordinates.len() != dimension {
        return Err(SpatialProjectionError::CoordinateDimension {
            expected: dimension,
            actual: descriptor.coordinates.len(),
        });
    }
    if descriptor
        .coordinates
        .iter()
        .any(|value| !value.is_finite())
    {
        return Err(SpatialProjectionError::NonFiniteCoordinate { axis: 0 });
    }
    let center = match dimension {
        1 => [descriptor.coordinates[0], 0.5, 0.0],
        2 => [
            descriptor.coordinates[0],
            0.5,
            descriptor.coordinates[1] * 2.0 - 1.0,
        ],
        3 => [
            descriptor.coordinates[0],
            descriptor.coordinates[1],
            descriptor.coordinates[2],
        ],
        _ => {
            return Err(SpatialProjectionError::InvalidLayout(
                "invalid coordinate dimension",
            ));
        }
    };
    if center.iter().any(|value| !value.is_finite()) {
        return Err(SpatialProjectionError::NonFiniteCoordinate { axis: 0 });
    }
    Ok([
        center[0].clamp(0.0, 1.0),
        center[1].clamp(0.0, 1.0),
        center[2].clamp(0.0, 1.0),
    ])
}

fn bracket(knots: &[f64], value: f64) -> (usize, usize, f64) {
    if value <= knots[0] {
        return (0, 0, 0.0);
    }
    if value >= knots[knots.len() - 1] {
        let last = knots.len() - 1;
        return (last, last, 0.0);
    }
    let upper = knots.partition_point(|knot| *knot < value);
    let lower = upper - 1;
    let t = (value - knots[lower]) / (knots[upper] - knots[lower]);
    (lower, upper, t)
}

fn exp2a(value: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    let n = value.floor();
    let fraction = value - n;
    let polynomial = 1.0
        + 0.686279297 * fraction
        + 0.254821777 * fraction * fraction
        + 0.0588989258 * fraction * fraction * fraction;
    let n = if n < f64::from(i32::MIN) || n > f64::from(i32::MAX) {
        return 0.0;
    } else {
        n as i32
    };
    let result = polynomial * 2.0_f64.powi(n);
    if result.is_finite() { result } else { 0.0 }
}

fn log2a(value: f64) -> f64 {
    if !value.is_finite() || value <= 0.0 {
        return f64::NEG_INFINITY;
    }
    let bits = value.to_bits();
    let raw_exponent = ((bits >> 52) & 0x7ff) as i32;
    let (mantissa, exponent) = if raw_exponent == 0 {
        let exponent = value.log2().floor() as i32 + 1;
        (value / 2.0_f64.powi(exponent), exponent)
    } else {
        let exponent = raw_exponent - 1022;
        (value / 2.0_f64.powi(exponent), exponent)
    };
    (mantissa * 4.07934570 - 2.69311523) - mantissa * mantissa * 1.38623047 + exponent as f64
}

#[cfg(test)]
mod tests {
    use super::{compact_quartic_kernel, extent_radius, extent_scalar};

    #[test]
    fn scalar_reduction_is_q15_floored_and_axis_agnostic() {
        let xyz =
            extent_scalar([1_057.0 / 32_768.0, 2_114.0 / 32_768.0, 3_171.0 / 32_768.0]).unwrap();
        assert_eq!(xyz.0, 2_114);
        assert_eq!(xyz.1, 2_114.0 / 32_768.0);
        let x = extent_scalar([15_855.0 / 32_768.0, 0.0, 0.0]).unwrap();
        let y = extent_scalar([0.0, 15_855.0 / 32_768.0, 0.0]).unwrap();
        let z = extent_scalar([0.0, 0.0, 15_855.0 / 32_768.0]).unwrap();
        assert_eq!(x, y);
        assert_eq!(x, z);
        assert_eq!(x.0, 5_285);
    }

    #[test]
    fn radius_knots_and_clamps_are_exact() {
        assert_eq!(extent_radius(-1.0), 0.0);
        assert_eq!(extent_radius(0.2), 0.075);
        assert_eq!(extent_radius(0.5), 0.25);
        assert_eq!(extent_radius(0.75), 0.45);
        assert_eq!(extent_radius(1.0), 0.7);
        assert_eq!(extent_radius(2.0), 0.7);
        assert!((extent_radius(0.625) - 0.35).abs() < 1.0e-15);
    }

    #[test]
    fn compact_kernel_has_finite_support_and_exact_origin() {
        assert_eq!(compact_quartic_kernel(0.5, 0.5, 0.25, false), 1.0);
        assert!(compact_quartic_kernel(0.6, 0.5, 0.1, false).is_finite());
        assert!(compact_quartic_kernel(0.6, 0.5, 0.1, false) > 0.0);
        assert!(compact_quartic_kernel(1.0, 0.0, 0.25, false) > 0.0);
        assert_eq!(compact_quartic_kernel(1.0, 0.0, 0.249, false), 0.0);
        assert_eq!(
            compact_quartic_kernel(0.5, 0.5, 2.5e-5 - 1.0e-8, false),
            0.0
        );
    }
}
