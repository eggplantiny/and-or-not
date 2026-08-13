use crate::{Fixed, FixedVec2, WireId};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use thiserror::Error;

/// One deterministic hostile circle sampled by Wire sensing.
///
/// `id` is deliberately independent from dense storage order. The broad phase and every public
/// observation use it as the stable candidate-order key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostileCollider {
    pub id: u64,
    pub center: FixedVec2,
    pub radius: Fixed,
}

/// The physical centerline of one active Wire Body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WireSensingInput<'a> {
    pub id: WireId,
    pub points: &'a [FixedVec2],
}

/// The canonical one-bit occupancy observation for one Wire Body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WireSensingOutput {
    pub id: WireId,
    pub occupied: bool,
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum SensingError {
    #[error("sensing chunk size must be positive")]
    NonPositiveChunkSize,

    #[error("sense radius must be nonnegative")]
    NegativeSenseRadius,

    #[error("hostile collider {id} has a negative radius")]
    NegativeHostileRadius { id: u64 },

    #[error("hostile collider ID {id} is duplicated")]
    DuplicateHostileId { id: u64 },

    #[error("Wire {wire:?} appears more than once in one sensing sample")]
    DuplicateWireId { wire: WireId },

    #[error("a sensing polyline must contain at least two points")]
    PolylineTooShort,

    #[error("a sensing polyline contains a zero-length segment at index {segment_index}")]
    ZeroLengthSegment { segment_index: usize },

    #[error("canonical sensing arithmetic overflow")]
    NumericOverflow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ChunkKey {
    x: i128,
    y: i128,
}

/// A deterministic sparse broad phase.
///
/// Each hostile is stored exactly once, in the chunk containing its center. Queries expand the
/// Wire AABB by `sense_radius + maximum_hostile_radius`, so a true circle/capsule intersection can
/// never be omitted. This center-only layout is important: a collider or Wire spanning the entire
/// signed-coordinate domain still touches only the actually occupied `BTreeMap` buckets instead of
/// materializing every empty chunk in between.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SparseOrderedChunkGrid {
    chunk_size: Fixed,
    maximum_hostile_radius: Fixed,
    hostiles: Vec<HostileCollider>,
    buckets: BTreeMap<ChunkKey, Vec<usize>>,
}

impl SparseOrderedChunkGrid {
    pub fn new(chunk_size: Fixed, hostiles: &[HostileCollider]) -> Result<Self, SensingError> {
        if chunk_size.0 <= 0 {
            return Err(SensingError::NonPositiveChunkSize);
        }

        let mut hostiles = hostiles.to_vec();
        hostiles.sort_unstable_by_key(|hostile| hostile.id);
        for hostile in &hostiles {
            if hostile.radius.0 < 0 {
                return Err(SensingError::NegativeHostileRadius { id: hostile.id });
            }
        }
        for pair in hostiles.windows(2) {
            if pair[0].id == pair[1].id {
                return Err(SensingError::DuplicateHostileId { id: pair[0].id });
            }
        }

        let maximum_hostile_radius = hostiles
            .iter()
            .map(|hostile| hostile.radius)
            .max_by_key(|radius| radius.0)
            .unwrap_or(Fixed::ZERO);
        let mut buckets: BTreeMap<ChunkKey, Vec<usize>> = BTreeMap::new();
        for (index, hostile) in hostiles.iter().enumerate() {
            let key = chunk_key(hostile.center, chunk_size)?;
            buckets.entry(key).or_default().push(index);
        }

        Ok(Self {
            chunk_size,
            maximum_hostile_radius,
            hostiles,
            buckets,
        })
    }

    pub const fn chunk_size(&self) -> Fixed {
        self.chunk_size
    }

    pub fn hostile_count(&self) -> usize {
        self.hostiles.len()
    }

    /// Returns the broad-phase candidate IDs in ascending stable ID order.
    ///
    /// The result is a conservative superset. Callers must still use the exact narrow phase.
    pub fn candidate_ids_for_polyline(
        &self,
        points: &[FixedVec2],
        sense_radius: Fixed,
    ) -> Result<Vec<u64>, SensingError> {
        let indices = self.candidate_indices_for_polyline(points, sense_radius)?;
        Ok(indices
            .into_iter()
            .map(|index| self.hostiles[index].id)
            .collect())
    }

    fn candidate_indices_for_polyline(
        &self,
        points: &[FixedVec2],
        sense_radius: Fixed,
    ) -> Result<Vec<usize>, SensingError> {
        validate_polyline(points)?;
        if sense_radius.0 < 0 {
            return Err(SensingError::NegativeSenseRadius);
        }

        let expansion = i128::from(sense_radius.0)
            .checked_add(i128::from(self.maximum_hostile_radius.0))
            .ok_or(SensingError::NumericOverflow)?;
        let first = points[0];
        let mut minimum_x = i128::from(first.x.0);
        let mut maximum_x = minimum_x;
        let mut minimum_y = i128::from(first.y.0);
        let mut maximum_y = minimum_y;
        for point in &points[1..] {
            let x = i128::from(point.x.0);
            let y = i128::from(point.y.0);
            minimum_x = minimum_x.min(x);
            maximum_x = maximum_x.max(x);
            minimum_y = minimum_y.min(y);
            maximum_y = maximum_y.max(y);
        }

        minimum_x = minimum_x
            .checked_sub(expansion)
            .ok_or(SensingError::NumericOverflow)?;
        maximum_x = maximum_x
            .checked_add(expansion)
            .ok_or(SensingError::NumericOverflow)?;
        minimum_y = minimum_y
            .checked_sub(expansion)
            .ok_or(SensingError::NumericOverflow)?;
        maximum_y = maximum_y
            .checked_add(expansion)
            .ok_or(SensingError::NumericOverflow)?;

        let divisor = i128::from(self.chunk_size.0);
        let minimum_chunk_x = minimum_x.div_euclid(divisor);
        let maximum_chunk_x = maximum_x.div_euclid(divisor);
        let minimum_chunk_y = minimum_y.div_euclid(divisor);
        let maximum_chunk_y = maximum_y.div_euclid(divisor);
        let lower = ChunkKey {
            x: minimum_chunk_x,
            y: i128::MIN,
        };
        let upper = ChunkKey {
            x: maximum_chunk_x,
            y: i128::MAX,
        };

        let mut candidates = Vec::new();
        for (key, indices) in self.buckets.range(lower..=upper) {
            if minimum_chunk_y <= key.y && key.y <= maximum_chunk_y {
                candidates.extend(indices.iter().copied());
            }
        }
        candidates.sort_unstable_by_key(|index| self.hostiles[*index].id);
        Ok(candidates)
    }
}

/// Samples all Wires through the ordered sparse broad phase and exact capsule narrow phase.
///
/// Results are always ordered by ascending `WireId`, independent of the storage order of either
/// input slice. Multiple hostile intersections still produce one `occupied` bit.
pub fn sample_wire_sensing(
    wires: &[WireSensingInput<'_>],
    hostiles: &[HostileCollider],
    sense_radius: Fixed,
    chunk_size: Fixed,
) -> Result<Vec<WireSensingOutput>, SensingError> {
    let grid = SparseOrderedChunkGrid::new(chunk_size, hostiles)?;
    sample_wire_sensing_with_grid(wires, &grid, sense_radius)
}

/// Samples Wires against a reusable, immutable broad-phase grid.
pub fn sample_wire_sensing_with_grid(
    wires: &[WireSensingInput<'_>],
    grid: &SparseOrderedChunkGrid,
    sense_radius: Fixed,
) -> Result<Vec<WireSensingOutput>, SensingError> {
    if sense_radius.0 < 0 {
        return Err(SensingError::NegativeSenseRadius);
    }

    let mut ordered_wires = wires.to_vec();
    ordered_wires.sort_unstable_by_key(|wire| wire.id);
    for pair in ordered_wires.windows(2) {
        if pair[0].id == pair[1].id {
            return Err(SensingError::DuplicateWireId { wire: pair[0].id });
        }
    }

    let mut output = Vec::with_capacity(ordered_wires.len());
    for wire in ordered_wires {
        validate_polyline(wire.points)?;
        let candidates = grid.candidate_indices_for_polyline(wire.points, sense_radius)?;
        let mut occupied = false;
        for index in candidates {
            if circle_intersects_polyline_capsule(grid.hostiles[index], wire.points, sense_radius)?
            {
                occupied = true;
                break;
            }
        }
        output.push(WireSensingOutput {
            id: wire.id,
            occupied,
        });
    }
    Ok(output)
}

/// Exact closed-set intersection between a hostile circle and a polyline expanded by
/// `sense_radius`.
///
/// The comparison never projects to floating point. Interior segment distance is compared as
/// `cross² <= combined_radius² * segment_length²` in a 512-bit unsigned domain, while endpoint
/// caps use an exact 256-bit squared-distance comparison.
pub fn circle_intersects_polyline_capsule(
    hostile: HostileCollider,
    points: &[FixedVec2],
    sense_radius: Fixed,
) -> Result<bool, SensingError> {
    validate_polyline(points)?;
    if sense_radius.0 < 0 {
        return Err(SensingError::NegativeSenseRadius);
    }
    if hostile.radius.0 < 0 {
        return Err(SensingError::NegativeHostileRadius { id: hostile.id });
    }

    let combined_radius = u128::try_from(sense_radius.0)
        .map_err(|_| SensingError::NumericOverflow)?
        .checked_add(u128::try_from(hostile.radius.0).map_err(|_| SensingError::NumericOverflow)?)
        .ok_or(SensingError::NumericOverflow)?;
    let radius_squared = U256::multiply_u128(combined_radius, combined_radius);

    for segment in points.windows(2) {
        if circle_intersects_segment_capsule(
            hostile.center,
            segment[0],
            segment[1],
            radius_squared,
        )? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn validate_polyline(points: &[FixedVec2]) -> Result<(), SensingError> {
    if points.len() < 2 {
        return Err(SensingError::PolylineTooShort);
    }
    if let Some(segment_index) = points
        .windows(2)
        .position(|segment| segment[0] == segment[1])
    {
        return Err(SensingError::ZeroLengthSegment { segment_index });
    }
    Ok(())
}

fn chunk_key(point: FixedVec2, chunk_size: Fixed) -> Result<ChunkKey, SensingError> {
    if chunk_size.0 <= 0 {
        return Err(SensingError::NonPositiveChunkSize);
    }
    let divisor = i128::from(chunk_size.0);
    Ok(ChunkKey {
        x: i128::from(point.x.0).div_euclid(divisor),
        y: i128::from(point.y.0).div_euclid(divisor),
    })
}

fn circle_intersects_segment_capsule(
    center: FixedVec2,
    start: FixedVec2,
    end: FixedVec2,
    radius_squared: U256,
) -> Result<bool, SensingError> {
    let segment = delta(start, end);
    let from_start = delta(start, center);
    let length_squared = squared_distance(start, end)?;
    if length_squared.is_zero() {
        return Err(SensingError::ZeroLengthSegment { segment_index: 0 });
    }

    let projection = dot_delta(from_start, segment)?;
    if projection.negative || projection.magnitude.is_zero() {
        return Ok(squared_distance(center, start)? <= radius_squared);
    }
    if projection.magnitude >= length_squared {
        return Ok(squared_distance(center, end)? <= radius_squared);
    }

    let cross = cross_delta(segment, from_start)?;
    let cross_squared = U512::multiply(cross.magnitude, cross.magnitude)?;
    let scaled_radius_squared = U512::multiply(radius_squared, length_squared)?;
    Ok(cross_squared <= scaled_radius_squared)
}

fn delta(start: FixedVec2, end: FixedVec2) -> (i128, i128) {
    (
        i128::from(end.x.0) - i128::from(start.x.0),
        i128::from(end.y.0) - i128::from(start.y.0),
    )
}

fn squared_distance(first: FixedVec2, second: FixedVec2) -> Result<U256, SensingError> {
    let difference = delta(first, second);
    U256::multiply_u128(difference.0.unsigned_abs(), difference.0.unsigned_abs())
        .checked_add(U256::multiply_u128(
            difference.1.unsigned_abs(),
            difference.1.unsigned_abs(),
        ))
        .ok_or(SensingError::NumericOverflow)
}

fn dot_delta(left: (i128, i128), right: (i128, i128)) -> Result<SignedWide, SensingError> {
    SignedWide::product(left.0, right.0)
        .checked_add(SignedWide::product(left.1, right.1))
        .ok_or(SensingError::NumericOverflow)
}

fn cross_delta(left: (i128, i128), right: (i128, i128)) -> Result<SignedWide, SensingError> {
    SignedWide::product(left.0, right.1)
        .checked_sub(SignedWide::product(left.1, right.0))
        .ok_or(SensingError::NumericOverflow)
}

// Coordinate deltas have at most 64 magnitude bits. Squared distance, dot, and cross can need 129
// bits, so signed i128 is not sufficient for the complete i64 coordinate domain. Limbs are stored
// least-significant first.
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
    fn multiply(left: U256, right: U256) -> Result<Self, SensingError> {
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
                let slot = output
                    .get_mut(output_index)
                    .ok_or(SensingError::NumericOverflow)?;
                let total = u128::from(*slot) + carry;
                *slot = total as u64;
                carry = total >> 64;
                output_index = output_index
                    .checked_add(1)
                    .ok_or(SensingError::NumericOverflow)?;
            }
        }
        Ok(Self(output))
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
    use super::{
        HostileCollider, SensingError, SparseOrderedChunkGrid, WireSensingInput, WireSensingOutput,
        chunk_key, circle_intersects_polyline_capsule, sample_wire_sensing,
        sample_wire_sensing_with_grid,
    };
    use crate::{EntityId, Fixed, FixedVec2, WireId};

    fn point(x: i64, y: i64) -> FixedVec2 {
        FixedVec2::new(Fixed(x), Fixed(y))
    }

    fn hostile(id: u64, x: i64, y: i64, radius: i64) -> HostileCollider {
        HostileCollider {
            id,
            center: point(x, y),
            radius: Fixed(radius),
        }
    }

    fn wire(id: u64, points: &[FixedVec2]) -> WireSensingInput<'_> {
        WireSensingInput {
            id: WireId(EntityId(id)),
            points,
        }
    }

    #[test]
    fn closed_capsule_boundary_covers_segment_side_and_both_endpoint_caps() {
        let points = [point(0, 0), point(10, 0)];
        for collider in [
            hostile(1, 5, 3, 1),
            hostile(2, -3, 0, 1),
            hostile(3, 13, 0, 1),
            hostile(4, -2, 2, 1),
        ] {
            assert_eq!(
                circle_intersects_polyline_capsule(collider, &points, Fixed(2)),
                Ok(true),
                "closed boundary should include {collider:?}"
            );
        }
        for collider in [
            hostile(5, 5, 4, 1),
            hostile(6, -4, 0, 1),
            hostile(7, 14, 0, 1),
            hostile(8, -3, 3, 1),
        ] {
            assert_eq!(
                circle_intersects_polyline_capsule(collider, &points, Fixed(2)),
                Ok(false),
                "outside point should exclude {collider:?}"
            );
        }
    }

    #[test]
    fn diagonal_three_four_five_boundary_is_exact_and_reversal_invariant() {
        let forward = [point(0, 0), point(4, 3)];
        let reverse = [point(4, 3), point(0, 0)];
        let touching = hostile(1, 0, 5, 3);
        let outside = hostile(2, 0, 5, 2);
        assert_eq!(
            circle_intersects_polyline_capsule(touching, &forward, Fixed(1)),
            Ok(true)
        );
        assert_eq!(
            circle_intersects_polyline_capsule(touching, &reverse, Fixed(1)),
            Ok(true)
        );
        assert_eq!(
            circle_intersects_polyline_capsule(outside, &forward, Fixed(1)),
            Ok(false)
        );
        assert_eq!(
            circle_intersects_polyline_capsule(outside, &reverse, Fixed(1)),
            Ok(false)
        );
    }

    #[test]
    fn bent_polyline_uses_each_straight_segment_without_filling_the_inside_corner() {
        let points = [point(0, 0), point(10, 0), point(10, 10)];
        assert_eq!(
            circle_intersects_polyline_capsule(hostile(1, 5, 1, 0), &points, Fixed(1)),
            Ok(true)
        );
        assert_eq!(
            circle_intersects_polyline_capsule(hostile(2, 9, 5, 0), &points, Fixed(1)),
            Ok(true)
        );
        assert_eq!(
            circle_intersects_polyline_capsule(hostile(3, 7, 3, 0), &points, Fixed(1)),
            Ok(false)
        );
    }

    #[test]
    fn full_i64_domain_uses_widened_cross_dot_and_squared_distance() {
        let diagonal = [point(i64::MIN, i64::MIN), point(i64::MAX, i64::MAX)];
        assert_eq!(
            circle_intersects_polyline_capsule(hostile(1, 0, 0, 0), &diagonal, Fixed(0)),
            Ok(true)
        );
        assert_eq!(
            circle_intersects_polyline_capsule(
                hostile(2, i64::MAX, i64::MIN, 0),
                &diagonal,
                Fixed(i64::MAX),
            ),
            Ok(false)
        );

        let horizontal = [point(i64::MIN, 0), point(i64::MAX, 0)];
        assert_eq!(
            circle_intersects_polyline_capsule(
                hostile(3, 0, i64::MAX, i64::MAX),
                &horizontal,
                Fixed(i64::MAX),
            ),
            Ok(true),
            "combined radii may exceed signed i64 without overflowing"
        );
    }

    #[test]
    fn negative_coordinates_use_mathematical_floor_chunking() {
        let chunk = Fixed(4);
        assert_eq!(chunk_key(point(-5, -4), chunk).unwrap().x, -2);
        assert_eq!(chunk_key(point(-5, -4), chunk).unwrap().y, -1);
        assert_eq!(chunk_key(point(-1, -1), chunk).unwrap().x, -1);
        assert_eq!(chunk_key(point(-1, -1), chunk).unwrap().y, -1);
        assert_eq!(chunk_key(point(0, 0), chunk).unwrap().x, 0);
        assert_eq!(chunk_key(point(3, 3), chunk).unwrap().x, 0);
        assert_eq!(chunk_key(point(4, 4), chunk).unwrap().x, 1);
    }

    #[test]
    fn broad_phase_candidates_are_sorted_and_are_a_conservative_geometric_superset() {
        let grid = SparseOrderedChunkGrid::new(
            Fixed(4),
            &[
                hostile(30, 100, 100, 1),
                hostile(10, 5, 3, 1),
                hostile(20, -2, 0, 0),
                hostile(40, 40, 31, 30),
            ],
        )
        .unwrap();
        let points = [point(0, 0), point(10, 0)];
        let candidates = grid.candidate_ids_for_polyline(&points, Fixed(2)).unwrap();
        assert_eq!(candidates, vec![10, 20, 40]);
        assert!(!candidates.contains(&30));
        for collider in [hostile(10, 5, 3, 1), hostile(20, -2, 0, 0)] {
            assert!(circle_intersects_polyline_capsule(collider, &points, Fixed(2)).unwrap());
            assert!(candidates.contains(&collider.id));
        }
    }

    #[test]
    fn sparse_grid_does_not_enumerate_empty_chunks_across_the_i64_domain() {
        let grid = SparseOrderedChunkGrid::new(
            Fixed(1),
            &[hostile(2, i64::MAX, 0, 0), hostile(1, i64::MIN, 0, 0)],
        )
        .unwrap();
        let entire_domain = [point(i64::MIN, 0), point(i64::MAX, 0)];
        assert_eq!(
            grid.candidate_ids_for_polyline(&entire_domain, Fixed(0)),
            Ok(vec![1, 2])
        );
    }

    #[test]
    fn output_is_wire_id_sorted_and_input_permutations_do_not_change_results() {
        let first = [point(0, 0), point(10, 0)];
        let second = [point(0, 20), point(10, 20)];
        let wires_a = [wire(9, &second), wire(3, &first)];
        let wires_b = [wire(3, &first), wire(9, &second)];
        let hostiles_a = [hostile(8, 5, 0, 0), hostile(2, 6, 0, 0)];
        let hostiles_b = [hostiles_a[1], hostiles_a[0]];
        let expected = vec![
            WireSensingOutput {
                id: WireId(EntityId(3)),
                occupied: true,
            },
            WireSensingOutput {
                id: WireId(EntityId(9)),
                occupied: false,
            },
        ];
        assert_eq!(
            sample_wire_sensing(&wires_a, &hostiles_a, Fixed(1), Fixed(4)),
            Ok(expected.clone())
        );
        assert_eq!(
            sample_wire_sensing(&wires_b, &hostiles_b, Fixed(1), Fixed(4)),
            Ok(expected)
        );
    }

    #[test]
    fn hostile_multiplicity_collapses_to_one_occupancy_bit() {
        let points = [point(0, 0), point(10, 0)];
        let wires = [wire(1, &points)];
        let one = [hostile(1, 5, 0, 0)];
        let three = [
            hostile(1, 5, 0, 0),
            hostile(2, 6, 0, 0),
            hostile(3, 7, 0, 0),
        ];
        let one_output = sample_wire_sensing(&wires, &one, Fixed(1), Fixed(4)).unwrap();
        let three_output = sample_wire_sensing(&wires, &three, Fixed(1), Fixed(4)).unwrap();
        assert_eq!(one_output, three_output);
        assert!(one_output[0].occupied);
    }

    #[test]
    fn reusable_grid_is_read_only_across_repeated_samples() {
        let points = [point(0, 0), point(10, 0)];
        let wires = [wire(1, &points)];
        let grid = SparseOrderedChunkGrid::new(Fixed(4), &[hostile(1, 5, 0, 0)]).unwrap();
        let before = grid.clone();
        let first = sample_wire_sensing_with_grid(&wires, &grid, Fixed(0)).unwrap();
        let second = sample_wire_sensing_with_grid(&wires, &grid, Fixed(0)).unwrap();
        assert_eq!(first, second);
        assert_eq!(grid, before);
    }

    #[test]
    fn malformed_inputs_fail_closed_before_sampling() {
        assert_eq!(
            SparseOrderedChunkGrid::new(Fixed(0), &[]),
            Err(SensingError::NonPositiveChunkSize)
        );
        assert_eq!(
            SparseOrderedChunkGrid::new(Fixed(-1), &[]),
            Err(SensingError::NonPositiveChunkSize)
        );
        assert_eq!(
            SparseOrderedChunkGrid::new(Fixed(1), &[hostile(7, 0, 0, -1)]),
            Err(SensingError::NegativeHostileRadius { id: 7 })
        );
        assert_eq!(
            SparseOrderedChunkGrid::new(Fixed(1), &[hostile(7, 0, 0, 0), hostile(7, 1, 0, 0)],),
            Err(SensingError::DuplicateHostileId { id: 7 })
        );

        let point_only = [point(0, 0)];
        assert_eq!(
            circle_intersects_polyline_capsule(hostile(1, 0, 0, 0), &point_only, Fixed(0)),
            Err(SensingError::PolylineTooShort)
        );
        let duplicate = [point(0, 0), point(0, 0)];
        assert_eq!(
            circle_intersects_polyline_capsule(hostile(1, 0, 0, 0), &duplicate, Fixed(0)),
            Err(SensingError::ZeroLengthSegment { segment_index: 0 })
        );
        let line = [point(0, 0), point(1, 0)];
        assert_eq!(
            circle_intersects_polyline_capsule(hostile(1, 0, 0, 0), &line, Fixed(-1)),
            Err(SensingError::NegativeSenseRadius)
        );

        let duplicate_wires = [wire(2, &line), wire(2, &line)];
        assert_eq!(
            sample_wire_sensing(&duplicate_wires, &[], Fixed(0), Fixed(1)),
            Err(SensingError::DuplicateWireId {
                wire: WireId(EntityId(2))
            })
        );
    }

    #[test]
    fn broad_phase_matches_brute_force_for_exhaustive_small_integer_geometry() {
        let mut hostiles = Vec::new();
        let mut id = 1_u64;
        for x in -3_i64..=3 {
            for y in -3_i64..=3 {
                for radius in 0_i64..=2 {
                    hostiles.push(hostile(id, x, y, radius));
                    id += 1;
                }
            }
        }
        let grid = SparseOrderedChunkGrid::new(Fixed(2), &hostiles).unwrap();

        for start_x in -2_i64..=2 {
            for start_y in -2_i64..=2 {
                for end_x in -2_i64..=2 {
                    for end_y in -2_i64..=2 {
                        if start_x == end_x && start_y == end_y {
                            continue;
                        }
                        let points = [point(start_x, start_y), point(end_x, end_y)];
                        for sense_radius in 0_i64..=2 {
                            let candidate_ids = grid
                                .candidate_ids_for_polyline(&points, Fixed(sense_radius))
                                .unwrap();
                            let actual = sample_wire_sensing_with_grid(
                                &[wire(1, &points)],
                                &grid,
                                Fixed(sense_radius),
                            )
                            .unwrap()[0]
                                .occupied;
                            let expected = hostiles.iter().copied().any(|collider| {
                                circle_intersects_polyline_capsule(
                                    collider,
                                    &points,
                                    Fixed(sense_radius),
                                )
                                .unwrap()
                            });
                            assert_eq!(
                                actual, expected,
                                "start=({start_x},{start_y}) end=({end_x},{end_y}) radius={sense_radius}"
                            );
                            for collider in hostiles.iter().copied().filter(|collider| {
                                circle_intersects_polyline_capsule(
                                    *collider,
                                    &points,
                                    Fixed(sense_radius),
                                )
                                .unwrap()
                            }) {
                                assert!(
                                    candidate_ids.binary_search(&collider.id).is_ok(),
                                    "broad phase omitted collider {} for {points:?} at radius {sense_radius}",
                                    collider.id
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn translation_and_endpoint_reversal_preserve_the_exact_narrow_phase() {
        let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
        for case in 0_u64..2_048 {
            let mut next = || {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                i64::try_from(seed % 101).unwrap() - 50
            };
            let start = point(next(), next());
            let mut end = point(next(), next());
            if end == start {
                end.x = Fixed(end.x.0 + 1);
            }
            let center = point(next(), next());
            let sense_radius = next().unsigned_abs() as i64 % 8;
            let hostile_radius = next().unsigned_abs() as i64 % 8;
            let collider = HostileCollider {
                id: case + 1,
                center,
                radius: Fixed(hostile_radius),
            };
            let forward = [start, end];
            let reverse = [end, start];
            let expected =
                circle_intersects_polyline_capsule(collider, &forward, Fixed(sense_radius))
                    .unwrap();
            assert_eq!(
                circle_intersects_polyline_capsule(collider, &reverse, Fixed(sense_radius)),
                Ok(expected)
            );

            let translation = point(100, -75);
            let translate =
                |value: FixedVec2| point(value.x.0 + translation.x.0, value.y.0 + translation.y.0);
            let translated = [translate(start), translate(end)];
            let translated_collider = HostileCollider {
                center: translate(center),
                ..collider
            };
            assert_eq!(
                circle_intersects_polyline_capsule(
                    translated_collider,
                    &translated,
                    Fixed(sense_radius),
                ),
                Ok(expected)
            );
        }
    }
}
