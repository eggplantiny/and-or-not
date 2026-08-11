use crate::{Fixed, FixedAabb, FixedVec2, NumericError};
use std::cmp::Ordering;

pub(crate) fn segments_have_positive_collinear_overlap(
    first_a: FixedVec2,
    first_b: FixedVec2,
    second_a: FixedVec2,
    second_b: FixedVec2,
) -> Result<bool, NumericError> {
    if !cross(first_a, first_b, second_a)?.is_zero()
        || !cross(first_a, first_b, second_b)?.is_zero()
    {
        return Ok(false);
    }

    let first_dx = i128::from(first_b.x.0) - i128::from(first_a.x.0);
    let second_dx = i128::from(second_b.x.0) - i128::from(second_a.x.0);
    let use_x = first_dx != 0 || second_dx != 0;
    let (first_start, first_end, second_start, second_end) = if use_x {
        (first_a.x.0, first_b.x.0, second_a.x.0, second_b.x.0)
    } else {
        (first_a.y.0, first_b.y.0, second_a.y.0, second_b.y.0)
    };
    let first_min = first_start.min(first_end);
    let first_max = first_start.max(first_end);
    let second_min = second_start.min(second_end);
    let second_max = second_start.max(second_end);
    Ok(first_min.max(second_min) < first_max.min(second_max))
}

pub(crate) fn point_is_strict_segment_interior(
    point: FixedVec2,
    start: FixedVec2,
    end: FixedVec2,
) -> Result<bool, NumericError> {
    Ok(point != start
        && point != end
        && cross(start, end, point)?.is_zero()
        && coordinate_between(point.x.0, start.x.0, end.x.0)
        && coordinate_between(point.y.0, start.y.0, end.y.0))
}

pub(crate) fn parallel_segments_are_too_close(
    first_a: FixedVec2,
    first_b: FixedVec2,
    second_a: FixedVec2,
    second_b: FixedVec2,
    pitch: Fixed,
) -> Result<bool, NumericError> {
    if pitch.0 <= 0 {
        return Err(NumericError::NonPositiveDivisor);
    }

    let first = delta(first_a, first_b);
    let second = delta(second_a, second_b);
    if first == (0, 0) || second == (0, 0) {
        return Ok(false);
    }
    if !cross_delta(first, second)?.is_zero() {
        return Ok(false);
    }

    let length_squared = dot_delta(first, first)?;
    if !length_squared.is_positive() {
        return Err(NumericError::Overflow);
    }
    let from_first_to_second_a = delta(first_a, second_a);
    let from_first_to_second_b = delta(first_a, second_b);
    let projection_a = dot_delta(from_first_to_second_a, first)?;
    let projection_b = dot_delta(from_first_to_second_b, first)?;
    let (projected_min, projected_max) = if projection_a.cmp_signed(projection_b).is_le() {
        (projection_a, projection_b)
    } else {
        (projection_b, projection_a)
    };
    let projections_overlap = projected_min.cmp_signed(length_squared).is_le()
        && SignedWide::ZERO.cmp_signed(projected_max).is_le();

    let pitch = u128::try_from(pitch.0).map_err(|_| NumericError::Overflow)?;
    let pitch_squared = U256::multiply_u128(pitch, pitch);
    if projections_overlap {
        let line_cross = cross_delta(first, from_first_to_second_a)?.magnitude;
        let left = U512::multiply(line_cross, line_cross);
        let right = U512::multiply(pitch_squared, length_squared.magnitude);
        Ok(left < right)
    } else {
        let minimum = [
            squared_distance(first_a, second_a)?,
            squared_distance(first_a, second_b)?,
            squared_distance(first_b, second_a)?,
            squared_distance(first_b, second_b)?,
        ]
        .into_iter()
        .min()
        .ok_or(NumericError::Overflow)?;
        Ok(minimum < pitch_squared)
    }
}

pub(crate) fn segment_intersects_aabb_interior(
    start: FixedVec2,
    end: FixedVec2,
    aabb: FixedAabb,
) -> Result<bool, NumericError> {
    if !aabb.is_nonempty() {
        return Ok(false);
    }

    let mut interval = ParameterInterval::unit();
    if !interval.clip_open_axis(start.x.0, end.x.0, aabb.min.x.0, aabb.max.x.0)? {
        return Ok(false);
    }
    if !interval.clip_open_axis(start.y.0, end.y.0, aabb.min.y.0, aabb.max.y.0)? {
        return Ok(false);
    }
    interval.is_nonempty()
}

pub(crate) fn segment_touches_aabb_boundary(
    start: FixedVec2,
    end: FixedVec2,
    aabb: FixedAabb,
) -> Result<bool, NumericError> {
    for (edge_a, edge_b) in aabb_edges(aabb) {
        if segments_intersect_closed(start, end, edge_a, edge_b)? {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) fn segment_overlaps_aabb_boundary(
    start: FixedVec2,
    end: FixedVec2,
    aabb: FixedAabb,
) -> Result<bool, NumericError> {
    for (edge_a, edge_b) in aabb_edges(aabb) {
        if segments_have_positive_collinear_overlap(start, end, edge_a, edge_b)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn aabb_edges(aabb: FixedAabb) -> [(FixedVec2, FixedVec2); 4] {
    let bottom_right = FixedVec2::new(aabb.max.x, aabb.min.y);
    let top_left = FixedVec2::new(aabb.min.x, aabb.max.y);
    [
        (aabb.min, bottom_right),
        (bottom_right, aabb.max),
        (aabb.max, top_left),
        (top_left, aabb.min),
    ]
}

fn segments_intersect_closed(
    first_a: FixedVec2,
    first_b: FixedVec2,
    second_a: FixedVec2,
    second_b: FixedVec2,
) -> Result<bool, NumericError> {
    let first_second_a = cross(first_a, first_b, second_a)?;
    let first_second_b = cross(first_a, first_b, second_b)?;
    let second_first_a = cross(second_a, second_b, first_a)?;
    let second_first_b = cross(second_a, second_b, first_b)?;

    if signs_are_opposite(first_second_a, first_second_b)
        && signs_are_opposite(second_first_a, second_first_b)
    {
        return Ok(true);
    }
    Ok(
        (first_second_a.is_zero() && point_is_on_segment(second_a, first_a, first_b))
            || (first_second_b.is_zero() && point_is_on_segment(second_b, first_a, first_b))
            || (second_first_a.is_zero() && point_is_on_segment(first_a, second_a, second_b))
            || (second_first_b.is_zero() && point_is_on_segment(first_b, second_a, second_b)),
    )
}

const fn signs_are_opposite(left: SignedWide, right: SignedWide) -> bool {
    (left.is_negative() && right.is_positive()) || (left.is_positive() && right.is_negative())
}

const fn point_is_on_segment(point: FixedVec2, start: FixedVec2, end: FixedVec2) -> bool {
    coordinate_between(point.x.0, start.x.0, end.x.0)
        && coordinate_between(point.y.0, start.y.0, end.y.0)
}

const fn coordinate_between(value: i64, first: i64, second: i64) -> bool {
    (first <= value && value <= second) || (second <= value && value <= first)
}

fn cross(
    origin: FixedVec2,
    first: FixedVec2,
    second: FixedVec2,
) -> Result<SignedWide, NumericError> {
    cross_delta(delta(origin, first), delta(origin, second))
}

fn cross_delta(left: (i128, i128), right: (i128, i128)) -> Result<SignedWide, NumericError> {
    SignedWide::product(left.0, right.1)
        .checked_sub(SignedWide::product(left.1, right.0))
        .ok_or(NumericError::Overflow)
}

fn dot_delta(left: (i128, i128), right: (i128, i128)) -> Result<SignedWide, NumericError> {
    SignedWide::product(left.0, right.0)
        .checked_add(SignedWide::product(left.1, right.1))
        .ok_or(NumericError::Overflow)
}

fn delta(start: FixedVec2, end: FixedVec2) -> (i128, i128) {
    (
        i128::from(end.x.0) - i128::from(start.x.0),
        i128::from(end.y.0) - i128::from(start.y.0),
    )
}

fn squared_distance(first: FixedVec2, second: FixedVec2) -> Result<U256, NumericError> {
    let difference = delta(first, second);
    U256::multiply_u128(difference.0.unsigned_abs(), difference.0.unsigned_abs())
        .checked_add(U256::multiply_u128(
            difference.1.unsigned_abs(),
            difference.1.unsigned_abs(),
        ))
        .ok_or(NumericError::Overflow)
}

// Coordinate deltas have at most 64 magnitude bits. A cross, dot product, or squared distance can
// require 129 magnitude bits, so signed i128 intermediates are not sufficient for the full i64
// coordinate domain. Limbs are stored least-significant first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct U256([u64; 4]);

impl U256 {
    const ZERO: Self = Self([0; 4]);

    fn multiply_u128(left: u128, right: u128) -> Self {
        let left_low = left as u64;
        let left_high = (left >> 64) as u64;
        let right_low = right as u64;
        let right_high = (right >> 64) as u64;

        let low_low = u128::from(left_low) * u128::from(right_low);
        let low_high = u128::from(left_low) * u128::from(right_high);
        let high_low = u128::from(left_high) * u128::from(right_low);
        let high_high = u128::from(left_high) * u128::from(right_high);

        let middle = (low_low >> 64) + u128::from(low_high as u64) + u128::from(high_low as u64);
        let upper =
            (low_high >> 64) + (high_low >> 64) + u128::from(high_high as u64) + (middle >> 64);
        let highest = (high_high >> 64) + (upper >> 64);

        Self([low_low as u64, middle as u64, upper as u64, highest as u64])
    }

    fn checked_add(self, right: Self) -> Option<Self> {
        let mut output = [0_u64; 4];
        let mut carry = false;
        for (index, slot) in output.iter_mut().enumerate() {
            let (partial, first_carry) = self.0[index].overflowing_add(right.0[index]);
            let (sum, second_carry) = partial.overflowing_add(u64::from(carry));
            *slot = sum;
            carry = first_carry || second_carry;
        }
        (!carry).then_some(Self(output))
    }

    fn checked_sub(self, right: Self) -> Option<Self> {
        let mut output = [0_u64; 4];
        let mut borrow = false;
        for (index, slot) in output.iter_mut().enumerate() {
            let (partial, first_borrow) = self.0[index].overflowing_sub(right.0[index]);
            let (difference, second_borrow) = partial.overflowing_sub(u64::from(borrow));
            *slot = difference;
            borrow = first_borrow || second_borrow;
        }
        (!borrow).then_some(Self(output))
    }

    const fn is_zero(self) -> bool {
        self.0[0] == 0 && self.0[1] == 0 && self.0[2] == 0 && self.0[3] == 0
    }
}

impl Ord for U256 {
    fn cmp(&self, other: &Self) -> Ordering {
        for index in (0..self.0.len()).rev() {
            match self.0[index].cmp(&other.0[index]) {
                Ordering::Equal => {}
                ordering => return ordering,
            }
        }
        Ordering::Equal
    }
}

impl PartialOrd for U256 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// Full products are needed for the exact spacing comparison cross^2 < pitch^2 * length^2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct U512([u64; 8]);

impl U512 {
    fn multiply(left: U256, right: U256) -> Self {
        let mut output = [0_u64; 8];
        for (left_index, &left_limb) in left.0.iter().enumerate() {
            let mut carry = 0_u128;
            for (right_index, &right_limb) in right.0.iter().enumerate() {
                let output_index = left_index + right_index;
                let total = u128::from(left_limb) * u128::from(right_limb)
                    + u128::from(output[output_index])
                    + carry;
                output[output_index] = total as u64;
                carry = total >> 64;
            }

            let mut output_index = left_index + right.0.len();
            while carry != 0 {
                let total = u128::from(output[output_index]) + carry;
                output[output_index] = total as u64;
                carry = total >> 64;
                output_index += 1;
            }
        }
        Self(output)
    }
}

impl Ord for U512 {
    fn cmp(&self, other: &Self) -> Ordering {
        for index in (0..self.0.len()).rev() {
            match self.0[index].cmp(&other.0[index]) {
                Ordering::Equal => {}
                ordering => return ordering,
            }
        }
        Ordering::Equal
    }
}

impl PartialOrd for U512 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// Canonical zero always has a nonnegative sign.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SignedWide {
    negative: bool,
    magnitude: U256,
}

impl SignedWide {
    const ZERO: Self = Self {
        negative: false,
        magnitude: U256::ZERO,
    };

    fn product(left: i128, right: i128) -> Self {
        Self::from_parts(
            (left < 0) != (right < 0),
            U256::multiply_u128(left.unsigned_abs(), right.unsigned_abs()),
        )
    }

    const fn from_parts(negative: bool, magnitude: U256) -> Self {
        Self {
            negative: negative && !magnitude.is_zero(),
            magnitude,
        }
    }

    fn checked_add(self, right: Self) -> Option<Self> {
        if self.negative == right.negative {
            self.magnitude
                .checked_add(right.magnitude)
                .map(|magnitude| Self::from_parts(self.negative, magnitude))
        } else {
            match self.magnitude.cmp(&right.magnitude) {
                Ordering::Less => right
                    .magnitude
                    .checked_sub(self.magnitude)
                    .map(|magnitude| Self::from_parts(right.negative, magnitude)),
                Ordering::Equal => Some(Self::ZERO),
                Ordering::Greater => self
                    .magnitude
                    .checked_sub(right.magnitude)
                    .map(|magnitude| Self::from_parts(self.negative, magnitude)),
            }
        }
    }

    fn checked_sub(self, right: Self) -> Option<Self> {
        self.checked_add(Self::from_parts(!right.negative, right.magnitude))
    }

    fn cmp_signed(self, other: Self) -> Ordering {
        match (self.negative, other.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => self.magnitude.cmp(&other.magnitude),
            (true, true) => other.magnitude.cmp(&self.magnitude),
        }
    }

    const fn is_zero(self) -> bool {
        self.magnitude.is_zero()
    }

    const fn is_negative(self) -> bool {
        self.negative
    }

    const fn is_positive(self) -> bool {
        !self.negative && !self.magnitude.is_zero()
    }
}

#[derive(Clone, Copy)]
struct Fraction {
    numerator: i128,
    denominator: i128,
}

impl Fraction {
    const ZERO: Self = Self {
        numerator: 0,
        denominator: 1,
    };
    const ONE: Self = Self {
        numerator: 1,
        denominator: 1,
    };

    fn new(mut numerator: i128, mut denominator: i128) -> Result<Self, NumericError> {
        if denominator == 0 {
            return Err(NumericError::NonPositiveDivisor);
        }
        if denominator < 0 {
            numerator = numerator.checked_neg().ok_or(NumericError::Overflow)?;
            denominator = denominator.checked_neg().ok_or(NumericError::Overflow)?;
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }

    fn compare(self, other: Self) -> Result<Ordering, NumericError> {
        let left = SignedWide::product(self.numerator, other.denominator);
        let right = SignedWide::product(other.numerator, self.denominator);
        Ok(left.cmp_signed(right))
    }
}

#[derive(Clone, Copy)]
struct Bound {
    value: Fraction,
    inclusive: bool,
}

struct ParameterInterval {
    lower: Bound,
    upper: Bound,
}

impl ParameterInterval {
    const fn unit() -> Self {
        Self {
            lower: Bound {
                value: Fraction::ZERO,
                inclusive: true,
            },
            upper: Bound {
                value: Fraction::ONE,
                inclusive: true,
            },
        }
    }

    fn clip_open_axis(
        &mut self,
        start: i64,
        end: i64,
        minimum: i64,
        maximum: i64,
    ) -> Result<bool, NumericError> {
        let start = i128::from(start);
        let delta = i128::from(end) - start;
        if delta == 0 {
            return Ok(i128::from(minimum) < start && start < i128::from(maximum));
        }

        let first = Fraction::new(i128::from(minimum) - start, delta)?;
        let second = Fraction::new(i128::from(maximum) - start, delta)?;
        let (lower, upper) = if first.compare(second)? == Ordering::Less {
            (first, second)
        } else {
            (second, first)
        };
        self.raise_lower(Bound {
            value: lower,
            inclusive: false,
        })?;
        self.lower_upper(Bound {
            value: upper,
            inclusive: false,
        })?;
        self.is_nonempty()
    }

    fn raise_lower(&mut self, candidate: Bound) -> Result<(), NumericError> {
        match self.lower.value.compare(candidate.value)? {
            Ordering::Less => self.lower = candidate,
            Ordering::Equal => self.lower.inclusive &= candidate.inclusive,
            Ordering::Greater => {}
        }
        Ok(())
    }

    fn lower_upper(&mut self, candidate: Bound) -> Result<(), NumericError> {
        match self.upper.value.compare(candidate.value)? {
            Ordering::Greater => self.upper = candidate,
            Ordering::Equal => self.upper.inclusive &= candidate.inclusive,
            Ordering::Less => {}
        }
        Ok(())
    }

    fn is_nonempty(&self) -> Result<bool, NumericError> {
        Ok(match self.lower.value.compare(self.upper.value)? {
            Ordering::Less => true,
            Ordering::Equal => self.lower.inclusive && self.upper.inclusive,
            Ordering::Greater => false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: i64, y: i64) -> FixedVec2 {
        FixedVec2::new(Fixed(x), Fixed(y))
    }

    fn box_0_10() -> FixedAabb {
        FixedAabb::new(point(0, 0), point(10, 10))
    }

    #[test]
    fn positive_collinear_overlap_excludes_point_contact() {
        assert_eq!(
            segments_have_positive_collinear_overlap(
                point(0, 0),
                point(5, 0),
                point(4, 0),
                point(9, 0)
            ),
            Ok(true)
        );
        assert_eq!(
            segments_have_positive_collinear_overlap(
                point(0, 0),
                point(5, 0),
                point(5, 0),
                point(9, 0)
            ),
            Ok(false)
        );
    }

    #[test]
    fn parallel_spacing_is_exact_for_overlapping_and_disjoint_projections() {
        assert_eq!(
            parallel_segments_are_too_close(
                point(0, 0),
                point(10, 0),
                point(0, 2),
                point(10, 2),
                Fixed(3)
            ),
            Ok(true)
        );
        assert_eq!(
            parallel_segments_are_too_close(
                point(0, 0),
                point(10, 0),
                point(0, 3),
                point(10, 3),
                Fixed(3)
            ),
            Ok(false)
        );
        assert_eq!(
            parallel_segments_are_too_close(
                point(0, 0),
                point(2, 0),
                point(4, 0),
                point(6, 0),
                Fixed(3)
            ),
            Ok(true)
        );
    }

    #[test]
    fn open_aabb_interior_distinguishes_crossing_tangent_and_boundary_run() {
        assert_eq!(
            segment_intersects_aabb_interior(point(-1, 5), point(11, 5), box_0_10()),
            Ok(true)
        );
        assert_eq!(
            segment_intersects_aabb_interior(point(-1, -1), point(0, 0), box_0_10()),
            Ok(false)
        );
        assert_eq!(
            segment_intersects_aabb_interior(point(0, 0), point(10, 0), box_0_10()),
            Ok(false)
        );
        assert_eq!(
            segment_touches_aabb_boundary(point(-1, -1), point(0, 0), box_0_10()),
            Ok(true)
        );
        assert_eq!(
            segment_overlaps_aabb_boundary(point(0, 0), point(10, 0), box_0_10()),
            Ok(true)
        );
    }

    #[test]
    fn wide_products_cover_the_full_delta_range_without_truncation() {
        assert_eq!(
            U256::multiply_u128(u128::MAX, u128::MAX),
            U256([1, 0, u64::MAX - 1, u64::MAX])
        );
        assert_eq!(
            U512::multiply(U256([u64::MAX; 4]), U256([u64::MAX; 4])),
            U512([1, 0, 0, 0, u64::MAX - 1, u64::MAX, u64::MAX, u64::MAX,])
        );

        let maximum_delta = i128::from(u64::MAX);
        let doubled_square = SignedWide::product(maximum_delta, maximum_delta)
            .checked_add(SignedWide::product(maximum_delta, maximum_delta))
            .expect("two full-width delta products fit the signed wide accumulator");
        assert_eq!(
            doubled_square,
            SignedWide::from_parts(false, U256([2, u64::MAX - 3, 1, 0]))
        );
        assert_eq!(
            cross_delta(
                (maximum_delta, maximum_delta),
                (maximum_delta, -maximum_delta)
            ),
            Ok(SignedWide::from_parts(true, U256([2, u64::MAX - 3, 1, 0])))
        );
    }

    #[test]
    fn extreme_orientation_is_invariant_under_operand_swap_and_reversal() {
        let long = (point(i64::MIN, i64::MIN), point(i64::MAX, i64::MIN));
        let short = (point(0, i64::MAX - 1), point(0, i64::MAX));
        let long_directions = [long, (long.1, long.0)];
        let short_directions = [short, (short.1, short.0)];

        for first in long_directions {
            for second in short_directions {
                assert_eq!(
                    segments_have_positive_collinear_overlap(first.0, first.1, second.0, second.1),
                    Ok(false)
                );
                assert_eq!(
                    segments_have_positive_collinear_overlap(second.0, second.1, first.0, first.1),
                    Ok(false)
                );
            }
        }

        let diagonal = (point(i64::MIN, i64::MIN), point(i64::MAX, i64::MAX));
        let diagonal_subset = (point(-1, -1), point(1, 1));
        for first in [diagonal, (diagonal.1, diagonal.0)] {
            for second in [diagonal_subset, (diagonal_subset.1, diagonal_subset.0)] {
                assert_eq!(
                    segments_have_positive_collinear_overlap(first.0, first.1, second.0, second.1),
                    Ok(true)
                );
                assert_eq!(
                    segments_have_positive_collinear_overlap(second.0, second.1, first.0, first.1),
                    Ok(true)
                );
            }
        }
    }

    #[test]
    fn extreme_parallel_spacing_is_exact_and_argument_invariant() {
        let baseline = (point(i64::MIN, 0), point(i64::MAX, 0));
        for (offset, expected) in [(1_023, true), (1_024, false)] {
            let parallel = (point(i64::MIN, offset), point(i64::MAX, offset));
            for first in [baseline, (baseline.1, baseline.0)] {
                for second in [parallel, (parallel.1, parallel.0)] {
                    assert_eq!(
                        parallel_segments_are_too_close(
                            first.0,
                            first.1,
                            second.0,
                            second.1,
                            Fixed(1_024)
                        ),
                        Ok(expected)
                    );
                    assert_eq!(
                        parallel_segments_are_too_close(
                            second.0,
                            second.1,
                            first.0,
                            first.1,
                            Fixed(1_024)
                        ),
                        Ok(expected)
                    );
                }
            }
        }

        let low = (point(i64::MIN, i64::MIN), point(i64::MIN + 1, i64::MIN));
        let high = (point(i64::MAX - 1, i64::MAX), point(i64::MAX, i64::MAX));
        assert_eq!(
            parallel_segments_are_too_close(low.0, low.1, high.0, high.1, Fixed(i64::MAX)),
            Ok(false)
        );
        assert_eq!(
            parallel_segments_are_too_close(high.1, high.0, low.1, low.0, Fixed(i64::MAX)),
            Ok(false)
        );
    }

    #[test]
    fn extreme_aabb_clipping_and_fraction_order_are_exact() {
        let near_minimum = FixedAabb::new(
            point(i64::MIN + 1, i64::MIN + 1),
            point(i64::MIN + 3, i64::MIN + 3),
        );
        let maximum = point(i64::MAX, i64::MAX);
        let minimum = point(i64::MIN, i64::MIN);

        assert_eq!(
            segment_intersects_aabb_interior(maximum, minimum, near_minimum),
            Ok(true)
        );
        assert_eq!(
            segment_intersects_aabb_interior(minimum, maximum, near_minimum),
            Ok(true)
        );
        assert_eq!(
            segment_touches_aabb_boundary(maximum, minimum, near_minimum),
            Ok(true)
        );
        assert_eq!(
            segment_touches_aabb_boundary(minimum, maximum, near_minimum),
            Ok(true)
        );

        let maximum_delta = i128::from(u64::MAX);
        let larger = Fraction::new(maximum_delta, maximum_delta - 1)
            .expect("nonzero positive denominator is valid");
        let smaller = Fraction::new(maximum_delta - 1, maximum_delta)
            .expect("nonzero positive denominator is valid");
        assert_eq!(larger.compare(smaller), Ok(Ordering::Greater));
        assert_eq!(smaller.compare(larger), Ok(Ordering::Less));
    }
}
