// Functional Core: frozen pre-fix interpolation arithmetic for the valid
// sparse fixtures. Source: acaaf740, openjoc-sofa/src/lib.rs blob 2d186ea64633.
// Platform sin/cos implementations can produce different parsed directions.
// Compare exact coefficient bits against the same-host legacy arithmetic,
// rather than treating one platform's trigonometric results as universal.
use openjoc_render::{CartesianPosition, HrirBank, HrirEar};

pub fn valid_triangle_coefficient_bits(bank: &HrirBank, direction: CartesianPosition) -> Vec<u64> {
    // The public resolver normalizes Cartesian requests with component
    // division; its tangent-basis helper uses reciprocal multiplication.
    let direction = [direction.x, direction.y, direction.z];
    let length =
        (direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2])
            .sqrt();
    let target = direction.map(|axis| axis / length);
    let mut indices = (0..bank.entries().len()).collect::<Vec<_>>();
    indices.sort_by(|&first, &second| {
        let first_dot = dot(target, bank.entries()[first].direction()).clamp(-1.0, 1.0);
        let second_dot = dot(target, bank.entries()[second].direction()).clamp(-1.0, 1.0);
        if first_dot == second_dot {
            first.cmp(&second)
        } else {
            second_dot
                .partial_cmp(&first_dot)
                .unwrap_or(std::cmp::Ordering::Equal)
        }
    });
    // These covered cap/wide-triangle fixtures contain the target in their
    // first three ranked measurements. Pin that selection independently of
    // the new production containment predicate and candidate search.
    let selected = [indices[0], indices[1], indices[2]];
    let vertices = selected.map(|index| bank.entries()[index].direction());
    assert!(
        vertices
            .iter()
            .all(|vertex| dot(target, *vertex).clamp(-1.0, 1.0).acos()
                <= 2.0 * std::f64::consts::PI / 3.0)
    );
    let reference = if target[2].abs() < 0.9 {
        [0.0, 0.0, 1.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let basis_x = normalize(cross(reference, target));
    let basis_y = cross(target, basis_x);
    let projected = vertices.map(|vertex| {
        let radial = dot(vertex, target);
        let tangent = std::array::from_fn(|axis| vertex[axis] - target[axis] * radial);
        [dot(tangent, basis_x), dot(tangent, basis_y)]
    });
    let a = projected[0][0] - projected[2][0];
    let b = projected[1][0] - projected[2][0];
    let c = projected[0][1] - projected[2][1];
    let d = projected[1][1] - projected[2][1];
    let determinant = a.mul_add(d, -b * c);
    assert!(determinant.is_finite() && determinant.abs() > 1.0e-10);
    let alpha = ((-projected[2][0]).mul_add(d, -b * -projected[2][1])) / determinant;
    let beta = (a.mul_add(-projected[2][1], projected[2][0] * c)) / determinant;
    let weights = [alpha, beta, 1.0 - alpha - beta];
    assert!(
        weights
            .iter()
            .all(|weight| weight.is_finite() && *weight >= -1.0e-8)
    );
    let weights = weights.map(|weight| weight.max(0.0));
    let mut sum = 0.0;
    for weight in weights {
        sum += weight;
    }
    let weights = weights.map(|weight| weight.max(0.0) / sum);
    let mut output = [vec![0.0; 2], vec![0.0; 2]];
    for (index, weight) in selected.into_iter().zip(weights) {
        let pair = bank.entries()[index].pair();
        for (ear_index, ear) in [HrirEar::Left, HrirEar::Right].into_iter().enumerate() {
            assert_eq!(pair.delay_samples(ear), 0);
            let taps = if ear == HrirEar::Left {
                pair.left_taps()
            } else {
                pair.right_taps()
            };
            assert_eq!(taps.len(), 2);
            for (tap_index, tap) in taps.iter().enumerate() {
                output[ear_index][tap_index] += weight * *tap;
            }
        }
    }
    output.into_iter().flatten().map(f64::to_bits).collect()
}

fn dot(first: [f64; 3], second: [f64; 3]) -> f64 {
    first[0].mul_add(second[0], first[1].mul_add(second[1], first[2] * second[2]))
}

fn cross(first: [f64; 3], second: [f64; 3]) -> [f64; 3] {
    [
        first[1] * second[2] - first[2] * second[1],
        first[2] * second[0] - first[0] * second[2],
        first[0] * second[1] - first[1] * second[0],
    ]
}

fn normalize(value: [f64; 3]) -> [f64; 3] {
    let norm = dot(value, value).sqrt();
    value.map(|axis| axis * (1.0 / norm))
}
