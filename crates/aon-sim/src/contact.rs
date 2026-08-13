use crate::{ContactDamageProbeProfile, EnemyId, Energy, FIXED_ONE, Fixed, FixedVec2, HeatEnergy};
use std::cmp::Ordering;
use thiserror::Error;

/// One external damageable collider eligible to absorb a Wire's granted Live Energy.
///
/// S1-M4 has one contact type, so canonical duration and contact measure are both one and the
/// resulting allocation weight is exactly the positive `conductivity` value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContactCandidate {
    pub target: EnemyId,
    pub weight: u128,
}

/// One Enemy's deterministic share of a Wire's granted Live Energy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContactAllocation {
    pub target: EnemyId,
    pub weight: u128,
    pub absorbed: Energy,
}

/// Descriptive alias for one target allocation row.
pub type ContactAbsorption = ContactAllocation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiveWireInput {
    pub wire: crate::WireId,
    pub length: Fixed,
    pub high_drive_strength: u128,
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum ContactError {
    #[error("Live Wire length must be nonnegative")]
    NegativeLength,

    #[error("a positive Live Wire demand cannot have zero HIGH Drive Strength")]
    ZeroDriveWithDemand,

    #[error("Live Wire Energy coefficient must be positive")]
    NonPositiveCoefficient,

    #[error("a contact Wire polyline must contain at least two points")]
    PolylineTooShort,

    #[error("a contact Wire polyline contains a zero-length segment at index {segment_index}")]
    ZeroLengthWireSegment { segment_index: usize },

    #[error("Wire Body radius must be nonnegative")]
    NegativeWireBodyRadius,

    #[error("hostile collider radius must be nonnegative")]
    NegativeHostileRadius,

    #[error("world leak weight must be positive")]
    ZeroWorldLeakWeight,

    #[error("contact candidate {target:?} has zero weight")]
    ZeroCandidateWeight { target: EnemyId },

    #[error("contact target {target:?} appears more than once in one allocation")]
    DuplicateTarget { target: EnemyId },

    #[error("canonical contact arithmetic overflow")]
    ArithmeticOverflow,

    #[error("Live Wire demand does not fit canonical Energy")]
    DemandOutOfRange,

    #[error("contact target remainder is inconsistent with the candidate set")]
    InvalidRemainder,
}

/// Computes one armed Wire's nominal Live Energy demand with exactly one final ceiling.
pub fn calculate_live_wire_demand(
    input: LiveWireInput,
    probe: &ContactDamageProbeProfile,
) -> Result<Energy, ContactError> {
    if input.length.0 < 0 {
        return Err(ContactError::NegativeLength);
    }
    if probe.live_energy_per_strength_wu.numerator() <= 0
        || probe.live_energy_per_strength_wu.denominator() <= 0
    {
        return Err(ContactError::NonPositiveCoefficient);
    }
    if input.length.0 == 0 {
        return Ok(Energy(0));
    }
    if input.high_drive_strength == 0 {
        return Err(ContactError::ZeroDriveWithDemand);
    }

    let mut factors = [
        u128::try_from(probe.live_energy_per_strength_wu.numerator())
            .map_err(|_| ContactError::NonPositiveCoefficient)?,
        input.high_drive_strength,
        u128::try_from(input.length.0).map_err(|_| ContactError::NegativeLength)?,
    ];
    let mut divisors = [
        u128::try_from(probe.live_energy_per_strength_wu.denominator())
            .map_err(|_| ContactError::NonPositiveCoefficient)?,
        u128::try_from(FIXED_ONE).map_err(|_| ContactError::ArithmeticOverflow)?,
    ];
    for factor in &mut factors {
        for divisor in &mut divisors {
            let common = gcd(*factor, *divisor);
            *factor /= common;
            *divisor /= common;
        }
    }
    let numerator = factors.into_iter().try_fold(1_u128, |product, factor| {
        product
            .checked_mul(factor)
            .ok_or(ContactError::ArithmeticOverflow)
    })?;
    let denominator = divisors.into_iter().try_fold(1_u128, |product, divisor| {
        product
            .checked_mul(divisor)
            .ok_or(ContactError::ArithmeticOverflow)
    })?;
    let quotient = numerator / denominator;
    let demand = if numerator.is_multiple_of(denominator) {
        quotient
    } else {
        quotient
            .checked_add(1)
            .ok_or(ContactError::ArithmeticOverflow)?
    };
    if demand == 0 {
        return Err(ContactError::ArithmeticOverflow);
    }
    u64::try_from(demand)
        .map(Energy)
        .map_err(|_| ContactError::DemandOutOfRange)
}

/// Tests a swept hostile circle against the closed capsule of every actual Wire Body segment.
///
/// The hostile center moves on the closed segment `movement_start..=movement_end`. The compared
/// radius is the exact sum of `hostile_radius` and the physical `wire_body_radius`; sensing radius
/// is deliberately absent from this API. All projection, cross-product, and squared-distance
/// comparisons use widened integer limbs and never project to floating point.
pub fn swept_circle_intersects_wire_body(
    movement_start: FixedVec2,
    movement_end: FixedVec2,
    hostile_radius: Fixed,
    wire_points: &[FixedVec2],
    wire_body_radius: Fixed,
) -> Result<bool, ContactError> {
    validate_wire_polyline(wire_points)?;
    if wire_body_radius.0 < 0 {
        return Err(ContactError::NegativeWireBodyRadius);
    }
    if hostile_radius.0 < 0 {
        return Err(ContactError::NegativeHostileRadius);
    }

    let combined_radius = u128::try_from(wire_body_radius.0)
        .map_err(|_| ContactError::ArithmeticOverflow)?
        .checked_add(
            u128::try_from(hostile_radius.0).map_err(|_| ContactError::ArithmeticOverflow)?,
        )
        .ok_or(ContactError::ArithmeticOverflow)?;
    let radius_squared = U256::multiply_u128(combined_radius, combined_radius);

    for wire_segment in wire_points.windows(2) {
        if segments_within_radius(
            movement_start,
            movement_end,
            wire_segment[0],
            wire_segment[1],
            radius_squared,
        )? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Tests a swept hostile circle against one closed point target.
///
/// This is the exact Main Core contact primitive. It shares the widened segment-capsule distance
/// law with Wire contact while deliberately accepting a zero-length target geometry.
pub fn swept_circle_intersects_point(
    movement_start: FixedVec2,
    movement_end: FixedVec2,
    hostile_radius: Fixed,
    target: FixedVec2,
) -> Result<bool, ContactError> {
    if hostile_radius.0 < 0 {
        return Err(ContactError::NegativeHostileRadius);
    }
    let radius = u128::try_from(hostile_radius.0).map_err(|_| ContactError::ArithmeticOverflow)?;
    point_within_segment_capsule(
        target,
        movement_start,
        movement_end,
        U256::multiply_u128(radius, radius),
    )
}

/// Allocates one Wire's granted Live Energy across unique Enemy contacts.
///
/// Inputs are canonicalized by ascending `EnemyId`. The aggregate target and every individual
/// base share are floored independently using exact checked `u128` arithmetic. Their difference
/// is distributed one Energy unit at a time from the lowest Enemy ID, cycling only if necessary.
/// Every unabsorbed unit becomes Wire Heat, including the explicit world-leak share.
pub fn allocate_contact_energy(
    granted_live_energy: Energy,
    candidates: &[ContactCandidate],
    world_leak_weight: u64,
) -> Result<(Vec<ContactAllocation>, HeatEnergy), ContactError> {
    if world_leak_weight == 0 {
        return Err(ContactError::ZeroWorldLeakWeight);
    }

    let mut candidates = candidates.to_vec();
    candidates.sort_unstable_by_key(|candidate| candidate.target);

    if let Some(candidate) = candidates.iter().find(|candidate| candidate.weight == 0) {
        return Err(ContactError::ZeroCandidateWeight {
            target: candidate.target,
        });
    }
    for pair in candidates.windows(2) {
        if pair[0].target == pair[1].target {
            return Err(ContactError::DuplicateTarget {
                target: pair[0].target,
            });
        }
    }

    if granted_live_energy.0 == 0 {
        return Ok((Vec::new(), HeatEnergy(0)));
    }
    if candidates.is_empty() {
        return Ok((Vec::new(), HeatEnergy(granted_live_energy.0)));
    }

    let total_contact_weight = candidates.iter().try_fold(0_u128, |sum, candidate| {
        sum.checked_add(candidate.weight)
            .ok_or(ContactError::ArithmeticOverflow)
    })?;
    let denominator = u128::from(world_leak_weight)
        .checked_add(total_contact_weight)
        .ok_or(ContactError::ArithmeticOverflow)?;

    let granted = u128::from(granted_live_energy.0);
    let target_budget = granted
        .checked_mul(total_contact_weight)
        .ok_or(ContactError::ArithmeticOverflow)?
        / denominator;

    let mut base_sum = 0_u128;
    let mut allocations = candidates
        .into_iter()
        .map(|candidate| {
            let absorbed = granted
                .checked_mul(candidate.weight)
                .ok_or(ContactError::ArithmeticOverflow)?
                / denominator;
            base_sum = base_sum
                .checked_add(absorbed)
                .ok_or(ContactError::ArithmeticOverflow)?;
            let absorbed = u64::try_from(absorbed).map_err(|_| ContactError::ArithmeticOverflow)?;
            Ok(ContactAllocation {
                target: candidate.target,
                weight: candidate.weight,
                absorbed: Energy(absorbed),
            })
        })
        .collect::<Result<Vec<_>, ContactError>>()?;

    let mut remainder = target_budget
        .checked_sub(base_sum)
        .ok_or(ContactError::InvalidRemainder)?;
    let candidate_count =
        u128::try_from(allocations.len()).map_err(|_| ContactError::ArithmeticOverflow)?;
    if remainder >= candidate_count {
        return Err(ContactError::InvalidRemainder);
    }
    for allocation in &mut allocations {
        if remainder == 0 {
            break;
        }
        allocation.absorbed = allocation
            .absorbed
            .checked_add(Energy(1))
            .map_err(|_| ContactError::ArithmeticOverflow)?;
        remainder -= 1;
    }

    let absorbed_sum = allocations.iter().try_fold(0_u64, |sum, allocation| {
        sum.checked_add(allocation.absorbed.0)
            .ok_or(ContactError::ArithmeticOverflow)
    })?;
    let wire_heat = granted_live_energy
        .0
        .checked_sub(absorbed_sum)
        .map(HeatEnergy)
        .ok_or(ContactError::ArithmeticOverflow)?;

    Ok((allocations, wire_heat))
}

fn validate_wire_polyline(points: &[FixedVec2]) -> Result<(), ContactError> {
    if points.len() < 2 {
        return Err(ContactError::PolylineTooShort);
    }
    if let Some(segment_index) = points
        .windows(2)
        .position(|segment| segment[0] == segment[1])
    {
        return Err(ContactError::ZeroLengthWireSegment { segment_index });
    }
    Ok(())
}

const fn gcd(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

fn segments_within_radius(
    first_a: FixedVec2,
    first_b: FixedVec2,
    second_a: FixedVec2,
    second_b: FixedVec2,
    radius_squared: U256,
) -> Result<bool, ContactError> {
    if segments_intersect_closed(first_a, first_b, second_a, second_b)? {
        return Ok(true);
    }

    Ok(
        point_within_segment_capsule(first_a, second_a, second_b, radius_squared)?
            || point_within_segment_capsule(first_b, second_a, second_b, radius_squared)?
            || point_within_segment_capsule(second_a, first_a, first_b, radius_squared)?
            || point_within_segment_capsule(second_b, first_a, first_b, radius_squared)?,
    )
}

fn point_within_segment_capsule(
    point: FixedVec2,
    start: FixedVec2,
    end: FixedVec2,
    radius_squared: U256,
) -> Result<bool, ContactError> {
    let segment = delta(start, end);
    let from_start = delta(start, point);
    let length_squared = squared_distance(start, end)?;
    if length_squared.is_zero() {
        return Ok(squared_distance(point, start)? <= radius_squared);
    }

    let projection = dot_delta(from_start, segment)?;
    if projection.is_negative() || projection.is_zero() {
        return Ok(squared_distance(point, start)? <= radius_squared);
    }
    if projection.magnitude >= length_squared {
        return Ok(squared_distance(point, end)? <= radius_squared);
    }

    let cross = cross_delta(segment, from_start)?;
    let cross_squared = U512::multiply(cross.magnitude, cross.magnitude);
    let scaled_radius_squared = U512::multiply(radius_squared, length_squared);
    Ok(cross_squared <= scaled_radius_squared)
}

fn segments_intersect_closed(
    first_a: FixedVec2,
    first_b: FixedVec2,
    second_a: FixedVec2,
    second_b: FixedVec2,
) -> Result<bool, ContactError> {
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
) -> Result<SignedWide, ContactError> {
    cross_delta(delta(origin, first), delta(origin, second))
}

fn cross_delta(left: (i128, i128), right: (i128, i128)) -> Result<SignedWide, ContactError> {
    SignedWide::product(left.0, right.1)
        .checked_sub(SignedWide::product(left.1, right.0))
        .ok_or(ContactError::ArithmeticOverflow)
}

fn dot_delta(left: (i128, i128), right: (i128, i128)) -> Result<SignedWide, ContactError> {
    SignedWide::product(left.0, right.0)
        .checked_add(SignedWide::product(left.1, right.1))
        .ok_or(ContactError::ArithmeticOverflow)
}

fn delta(start: FixedVec2, end: FixedVec2) -> (i128, i128) {
    (
        i128::from(end.x.0) - i128::from(start.x.0),
        i128::from(end.y.0) - i128::from(start.y.0),
    )
}

fn squared_distance(first: FixedVec2, second: FixedVec2) -> Result<U256, ContactError> {
    let difference = delta(first, second);
    U256::multiply_u128(difference.0.unsigned_abs(), difference.0.unsigned_abs())
        .checked_add(U256::multiply_u128(
            difference.1.unsigned_abs(),
            difference.1.unsigned_abs(),
        ))
        .ok_or(ContactError::ArithmeticOverflow)
}

// Coordinate deltas have at most 64 magnitude bits. Squared distances, cross products, and dot
// products can need 129 bits, so signed i128 is insufficient for the full canonical i64 domain.
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
            if left_index + right.0.len() < output.len() {
                output[left_index + right.0.len()] = carry as u64;
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

// Canonical zero is always nonnegative.
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

    const fn is_zero(self) -> bool {
        self.magnitude.is_zero()
    }

    const fn is_negative(self) -> bool {
        self.negative
    }

    const fn is_positive(self) -> bool {
        !self.negative && !self.magnitude.is_zero()
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BalanceProfile, EntityId};

    fn point(x: i64, y: i64) -> FixedVec2 {
        FixedVec2::new(Fixed(x), Fixed(y))
    }

    fn enemy(id: u64) -> EnemyId {
        EnemyId(EntityId(id))
    }

    fn candidate(id: u64, conductivity: u64) -> ContactCandidate {
        ContactCandidate {
            target: enemy(id),
            weight: u128::from(conductivity),
        }
    }

    #[test]
    fn live_wire_demand_uses_one_final_ceil_and_reference_strength_length() {
        let probe = BalanceProfile::construction_contact_damage_alpha("contact-test")
            .contact_damage_probe
            .expect("v5 probe");
        let input = LiveWireInput {
            wire: crate::WireId(EntityId(20)),
            length: Fixed(FIXED_ONE),
            high_drive_strength: 400,
        };
        assert_eq!(calculate_live_wire_demand(input, &probe), Ok(Energy(1)));
        let aggregate_above_u64 = u128::from(u64::MAX) + 1;
        assert_eq!(
            calculate_live_wire_demand(
                LiveWireInput {
                    high_drive_strength: aggregate_above_u64,
                    ..input
                },
                &probe,
            ),
            Ok(Energy(46_116_860_184_273_880))
        );
        assert_eq!(
            calculate_live_wire_demand(
                LiveWireInput {
                    length: Fixed(FIXED_ONE + 1),
                    ..input
                },
                &probe,
            ),
            Ok(Energy(2))
        );
        assert_eq!(
            calculate_live_wire_demand(
                LiveWireInput {
                    length: Fixed::ZERO,
                    high_drive_strength: 0,
                    ..input
                },
                &probe,
            ),
            Ok(Energy(0))
        );
        assert_eq!(
            calculate_live_wire_demand(
                LiveWireInput {
                    high_drive_strength: 0,
                    ..input
                },
                &probe,
            ),
            Err(ContactError::ZeroDriveWithDemand)
        );
    }

    #[test]
    fn c10_equal_weights_conserve_granted_energy_and_leak_to_heat() {
        let (allocations, heat) =
            allocate_contact_energy(Energy(20), &[candidate(12, 1), candidate(11, 1)], 2)
                .expect("C-10 allocation is valid");

        assert_eq!(
            allocations,
            vec![
                ContactAllocation {
                    target: enemy(11),
                    weight: 1,
                    absorbed: Energy(5),
                },
                ContactAllocation {
                    target: enemy(12),
                    weight: 1,
                    absorbed: Energy(5),
                },
            ]
        );
        assert_eq!(heat, HeatEnergy(10));
        assert_eq!(
            allocations
                .iter()
                .map(|allocation| allocation.absorbed.0)
                .sum::<u64>()
                + heat.0,
            20
        );
    }

    #[test]
    fn swept_circle_point_contact_is_closed_exact_and_allows_stationary_motion() {
        assert_eq!(
            swept_circle_intersects_point(point(0, 0), point(10, 0), Fixed(3), point(5, 3)),
            Ok(true)
        );
        assert_eq!(
            swept_circle_intersects_point(point(0, 0), point(10, 0), Fixed(3), point(5, 4)),
            Ok(false)
        );
        assert_eq!(
            swept_circle_intersects_point(point(7, -2), point(7, -2), Fixed(0), point(7, -2)),
            Ok(true)
        );
        assert_eq!(
            swept_circle_intersects_point(point(0, 0), point(1, 0), Fixed(-1), point(0, 0)),
            Err(ContactError::NegativeHostileRadius)
        );
    }

    #[test]
    fn odd_remainder_goes_to_the_lowest_enemy_id() {
        let (allocations, heat) =
            allocate_contact_energy(Energy(11), &[candidate(9, 1), candidate(3, 1)], 1)
                .expect("odd allocation is valid");

        assert_eq!(allocations[0].target, enemy(3));
        assert_eq!(allocations[0].absorbed, Energy(4));
        assert_eq!(allocations[1].target, enemy(9));
        assert_eq!(allocations[1].absorbed, Energy(3));
        assert_eq!(heat, HeatEnergy(4));
    }

    #[test]
    fn zero_grant_and_no_contacts_have_explicit_conservative_results() {
        let zero = allocate_contact_energy(Energy(0), &[candidate(2, u64::MAX)], 2)
            .expect("zero source avoids multiplication artifacts");
        assert_eq!(zero, (Vec::new(), HeatEnergy(0)));

        let no_contacts = allocate_contact_energy(Energy(17), &[], 2)
            .expect("world leak is the entire no-contact denominator");
        assert_eq!(no_contacts, (Vec::new(), HeatEnergy(17)));
    }

    #[test]
    fn candidate_permutation_does_not_change_output() {
        let forward = [candidate(2, 5), candidate(9, 3), candidate(4, 7)];
        let mut reverse = forward;
        reverse.reverse();

        assert_eq!(
            allocate_contact_energy(Energy(997), &forward, 11),
            allocate_contact_energy(Energy(997), &reverse, 11)
        );
    }

    #[test]
    fn invalid_candidates_denominator_and_u128_overflow_are_typed() {
        assert_eq!(
            allocate_contact_energy(Energy(1), &[candidate(7, 0)], 1),
            Err(ContactError::ZeroCandidateWeight { target: enemy(7) })
        );
        assert_eq!(
            allocate_contact_energy(Energy(1), &[candidate(7, 1), candidate(7, 2)], 1),
            Err(ContactError::DuplicateTarget { target: enemy(7) })
        );
        assert_eq!(
            allocate_contact_energy(Energy(1), &[], 0),
            Err(ContactError::ZeroWorldLeakWeight)
        );
        assert_eq!(
            allocate_contact_energy(
                Energy(u64::MAX),
                &[
                    ContactCandidate {
                        target: enemy(7),
                        weight: u128::MAX,
                    },
                    candidate(8, 1),
                ],
                1,
            ),
            Err(ContactError::ArithmeticOverflow)
        );
    }

    #[test]
    fn swept_path_crossing_tangency_static_contact_and_reversal_are_exact() {
        let body = [point(0, 0), point(10, 0)];
        for (start, end, radius, expected) in [
            (point(5, -10), point(5, 10), 0, true),
            (point(-5, 3), point(15, 3), 2, true),
            (point(-5, 4), point(15, 4), 2, false),
            (point(5, 3), point(5, 3), 2, true),
            (point(5, 4), point(5, 4), 2, false),
        ] {
            let forward =
                swept_circle_intersects_wire_body(start, end, Fixed(1), &body, Fixed(radius));
            let reverse =
                swept_circle_intersects_wire_body(end, start, Fixed(1), &body, Fixed(radius));
            assert_eq!(forward, Ok(expected));
            assert_eq!(reverse, forward);
        }
    }

    #[test]
    fn polyline_segment_order_and_bends_do_not_change_contact() {
        let forward = [point(-10, 0), point(0, 0), point(0, 10)];
        let reverse = [point(0, 10), point(0, 0), point(-10, 0)];
        let query = |points: &[FixedVec2]| {
            swept_circle_intersects_wire_body(
                point(5, 5),
                point(-5, 5),
                Fixed::ZERO,
                points,
                Fixed::ZERO,
            )
        };
        assert_eq!(query(&forward), Ok(true));
        assert_eq!(query(&reverse), query(&forward));
    }

    #[test]
    fn full_i64_domain_is_widened_and_symmetric() {
        let body = [point(i64::MIN, i64::MIN), point(i64::MAX, i64::MAX)];
        let reverse_body = [body[1], body[0]];
        let movement_start = point(i64::MIN, i64::MAX);
        let movement_end = point(i64::MAX, i64::MIN);

        let forward = swept_circle_intersects_wire_body(
            movement_start,
            movement_end,
            Fixed(i64::MAX),
            &body,
            Fixed(i64::MAX),
        );
        let reversed = swept_circle_intersects_wire_body(
            movement_end,
            movement_start,
            Fixed(i64::MAX),
            &reverse_body,
            Fixed(i64::MAX),
        );
        assert_eq!(forward, Ok(true));
        assert_eq!(reversed, forward);
    }

    #[test]
    fn endpoint_cap_and_diagonal_near_miss_are_distinguished() {
        let body = [point(0, 0), point(4, 3)];
        assert_eq!(
            swept_circle_intersects_wire_body(point(8, 3), point(7, 3), Fixed(1), &body, Fixed(2),),
            Ok(true)
        );
        assert_eq!(
            swept_circle_intersects_wire_body(
                point(-4, 5),
                point(-3, 4),
                Fixed(1),
                &body,
                Fixed(2),
            ),
            Ok(false)
        );
    }

    #[test]
    fn geometry_input_errors_are_typed() {
        assert_eq!(
            swept_circle_intersects_wire_body(
                point(0, 0),
                point(1, 1),
                Fixed::ZERO,
                &[point(0, 0)],
                Fixed::ZERO,
            ),
            Err(ContactError::PolylineTooShort)
        );
        assert_eq!(
            swept_circle_intersects_wire_body(
                point(0, 0),
                point(1, 1),
                Fixed::ZERO,
                &[point(0, 0), point(0, 0)],
                Fixed::ZERO,
            ),
            Err(ContactError::ZeroLengthWireSegment { segment_index: 0 })
        );
        assert_eq!(
            swept_circle_intersects_wire_body(
                point(0, 0),
                point(1, 1),
                Fixed::ZERO,
                &[point(0, 0), point(1, 0)],
                Fixed(-1),
            ),
            Err(ContactError::NegativeWireBodyRadius)
        );
        assert_eq!(
            swept_circle_intersects_wire_body(
                point(0, 0),
                point(1, 1),
                Fixed(-1),
                &[point(0, 0), point(1, 0)],
                Fixed::ZERO,
            ),
            Err(ContactError::NegativeHostileRadius)
        );
    }
}
