use crate::{Fixed, NumericError, ceil_isqrt, floor_div};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FixedVec2 {
    pub x: Fixed,
    pub y: Fixed,
}

impl FixedVec2 {
    pub const fn new(x: Fixed, y: Fixed) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum GeometryError {
    #[error("geometry quantum must be positive")]
    NonPositiveQuantum,

    #[error("point is not aligned to the canonical geometry quantum")]
    NotQuantized,
}

pub fn validate_quantized(point: FixedVec2, quantum: Fixed) -> Result<(), GeometryError> {
    if quantum.0 <= 0 {
        return Err(GeometryError::NonPositiveQuantum);
    }
    if point.x.0.rem_euclid(quantum.0) == 0 && point.y.0.rem_euclid(quantum.0) == 0 {
        Ok(())
    } else {
        Err(GeometryError::NotQuantized)
    }
}

pub fn segment_length(a: FixedVec2, b: FixedVec2) -> Result<Fixed, NumericError> {
    let dx = i128::from(b.x.0) - i128::from(a.x.0);
    let dy = i128::from(b.y.0) - i128::from(a.y.0);
    let dx = dx.unsigned_abs();
    let dy = dy.unsigned_abs();
    let squared = dx
        .checked_mul(dx)
        .and_then(|x| dy.checked_mul(dy).and_then(|y| x.checked_add(y)))
        .ok_or(NumericError::Overflow)?;
    let length = ceil_isqrt(squared)?;
    i64::try_from(length)
        .map(Fixed)
        .map_err(|_| NumericError::Overflow)
}

pub fn polyline_length(points: &[FixedVec2]) -> Result<Fixed, NumericError> {
    let Some((&first, remaining)) = points.split_first() else {
        return Ok(Fixed::ZERO);
    };
    let Some((&second, remaining)) = remaining.split_first() else {
        return Ok(Fixed::ZERO);
    };

    let mut total = Fixed::ZERO;
    let mut run_start = first;
    let mut run_end = second;
    for &next in remaining {
        if next == run_end {
            continue;
        }
        if run_end == run_start || same_direction_collinear(run_start, run_end, next) {
            run_end = next;
            continue;
        }
        total = total.checked_add(segment_length(run_start, run_end)?)?;
        run_start = run_end;
        run_end = next;
    }
    total.checked_add(segment_length(run_start, run_end)?)
}

pub(crate) fn canonical_polyline_points(points: &[FixedVec2]) -> Vec<FixedVec2> {
    let mut canonical = Vec::with_capacity(points.len());
    for &point in points {
        if canonical.last() == Some(&point) {
            continue;
        }
        if canonical.len() >= 2 {
            let previous = canonical[canonical.len() - 2];
            let middle = canonical[canonical.len() - 1];
            if same_direction_collinear(previous, middle, point) {
                let last = canonical.len() - 1;
                canonical[last] = point;
                continue;
            }
        }
        canonical.push(point);
    }
    canonical
}

fn same_direction_collinear(start: FixedVec2, middle: FixedVec2, end: FixedVec2) -> bool {
    let first_x = i128::from(middle.x.0) - i128::from(start.x.0);
    let first_y = i128::from(middle.y.0) - i128::from(start.y.0);
    let second_x = i128::from(end.x.0) - i128::from(middle.x.0);
    let second_y = i128::from(end.y.0) - i128::from(middle.y.0);

    signed_products_equal(first_x, second_y, first_y, second_x)
        && directions_are_compatible(first_x, second_x)
        && directions_are_compatible(first_y, second_y)
}

fn signed_products_equal(left_a: i128, left_b: i128, right_a: i128, right_b: i128) -> bool {
    let left_sign = left_a.signum() * left_b.signum();
    let right_sign = right_a.signum() * right_b.signum();
    if left_sign != right_sign {
        return false;
    }

    products_equal_without_overflow(
        left_a.unsigned_abs(),
        left_b.unsigned_abs(),
        right_a.unsigned_abs(),
        right_b.unsigned_abs(),
    )
}

fn products_equal_without_overflow(mut a: u128, b: u128, mut c: u128, d: u128) -> bool {
    if a == 0 || b == 0 || c == 0 || d == 0 {
        return (a == 0 || b == 0) && (c == 0 || d == 0);
    }

    // Cancel the common part of a/c. The remaining values are coprime, so equality requires
    // c to divide b and a to divide d. Comparing the two exact quotients avoids a 256-bit
    // intermediate while preserving the full i128-delta input range.
    let common = gcd_u128(a, c);
    a /= common;
    c /= common;
    b.is_multiple_of(c) && d.is_multiple_of(a) && b / c == d / a
}

const fn gcd_u128(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

const fn directions_are_compatible(first: i128, second: i128) -> bool {
    first == 0 || second == 0 || (first < 0) == (second < 0)
}

pub fn cell_coordinate(coordinate: Fixed, cell_size: Fixed) -> Result<i64, NumericError> {
    let coordinate = i128::from(coordinate.0);
    let cell_size = i128::from(cell_size.0);
    let cell = floor_div(coordinate, cell_size)?;
    i64::try_from(cell).map_err(|_| NumericError::Overflow)
}

#[cfg(test)]
mod tests {
    use super::{
        FixedVec2, GeometryError, canonical_polyline_points, cell_coordinate, polyline_length,
        segment_length, validate_quantized,
    };
    use crate::{FIXED_ONE, Fixed, NumericError};

    #[test]
    fn c17_three_four_five_length_is_exact() {
        let start = FixedVec2::new(Fixed::ZERO, Fixed::ZERO);
        let end = FixedVec2::new(Fixed(3 * FIXED_ONE), Fixed(4 * FIXED_ONE));

        assert_eq!(segment_length(start, end), Ok(Fixed(5 * FIXED_ONE)));
    }

    #[test]
    fn non_integer_euclidean_length_rounds_up_in_fixed_units() {
        let start = FixedVec2::new(Fixed::ZERO, Fixed::ZERO);
        let end = FixedVec2::new(Fixed(1), Fixed(1));

        assert_eq!(segment_length(start, end), Ok(Fixed(2)));
    }

    #[test]
    fn polyline_uses_the_same_segment_length_function() {
        let points = [
            FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
            FixedVec2::new(Fixed(3 * FIXED_ONE), Fixed(4 * FIXED_ONE)),
            FixedVec2::new(Fixed(3 * FIXED_ONE), Fixed(5 * FIXED_ONE)),
        ];

        assert_eq!(polyline_length(&points), Ok(Fixed(6 * FIXED_ONE)));
    }

    #[test]
    fn redundant_collinear_vertex_does_not_change_rounded_length() {
        let direct = [
            FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
            FixedVec2::new(Fixed(2), Fixed(2)),
        ];
        let split = [
            FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
            FixedVec2::new(Fixed(1), Fixed(1)),
            FixedVec2::new(Fixed(2), Fixed(2)),
        ];

        assert_eq!(polyline_length(&direct), Ok(Fixed(3)));
        assert_eq!(polyline_length(&split), polyline_length(&direct));
        assert_eq!(canonical_polyline_points(&split), direct);
    }

    #[test]
    fn reversal_is_not_collapsed_as_a_redundant_vertex() {
        let points = [
            FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
            FixedVec2::new(Fixed(2), Fixed::ZERO),
            FixedVec2::new(Fixed(1), Fixed::ZERO),
        ];

        assert_eq!(polyline_length(&points), Ok(Fixed(3)));
        assert_eq!(canonical_polyline_points(&points), points);
    }

    #[test]
    fn c17_negative_fixed_coordinate_maps_to_cell_minus_one() {
        assert_eq!(cell_coordinate(Fixed(-1), Fixed(FIXED_ONE)), Ok(-1));
    }

    #[test]
    fn quantization_is_validation_not_rounding() {
        let quantum = Fixed(FIXED_ONE / 64);
        assert_eq!(
            validate_quantized(FixedVec2::new(Fixed(quantum.0), Fixed(-quantum.0)), quantum),
            Ok(())
        );
        assert_eq!(
            validate_quantized(FixedVec2::new(Fixed(1), Fixed::ZERO), quantum),
            Err(GeometryError::NotQuantized)
        );
    }

    #[test]
    fn invalid_size_and_overflow_are_typed() {
        assert_eq!(
            cell_coordinate(Fixed(1), Fixed::ZERO),
            Err(NumericError::NonPositiveDivisor)
        );
        assert_eq!(
            segment_length(
                FixedVec2::new(Fixed(i64::MIN), Fixed(i64::MIN)),
                FixedVec2::new(Fixed(i64::MAX), Fixed(i64::MAX))
            ),
            Err(NumericError::Overflow)
        );
    }

    #[test]
    fn same_direction_collinear_splits_preserve_length_across_quantized_directions() {
        let directions = [
            (1_i64, 0_i64),
            (-1, 0),
            (0, 1),
            (0, -1),
            (1, 1),
            (-1, 1),
            (1, -1),
            (-1, -1),
            (3, 4),
            (-3, 4),
            (2, -5),
        ];
        let start_units = [(0_i64, 0_i64), (3, -5), (-7, 11)];
        let split_factors = [1_i64, 2, 4, 7];
        let end_factor = 11_i64;

        for quantum in [1_i64, 2, FIXED_ONE / 64, FIXED_ONE] {
            for (start_x, start_y) in start_units {
                let start = FixedVec2::new(Fixed(start_x * quantum), Fixed(start_y * quantum));
                for (direction_x, direction_y) in directions {
                    let point_at = |factor: i64| {
                        FixedVec2::new(
                            Fixed((start_x + direction_x * factor) * quantum),
                            Fixed((start_y + direction_y * factor) * quantum),
                        )
                    };
                    let end = point_at(end_factor);
                    let direct = polyline_length(&[start, end]).expect("direct length is valid");

                    for split_mask in 0_u8..(1 << split_factors.len()) {
                        let mut points = vec![start];
                        for (index, factor) in split_factors.iter().copied().enumerate() {
                            if split_mask & (1 << index) != 0 {
                                points.push(point_at(factor));
                            }
                        }
                        points.push(end);

                        for &point in &points {
                            assert_eq!(
                                validate_quantized(point, Fixed(quantum)),
                                Ok(()),
                                "generated point must remain quantum-aligned"
                            );
                        }
                        assert_eq!(
                            polyline_length(&points),
                            Ok(direct),
                            "split changed length for q={quantum}, start=({start_x},{start_y}), \
                             direction=({direction_x},{direction_y}), mask={split_mask:04b}"
                        );
                    }
                }
            }
        }
    }
}
