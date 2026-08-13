use crate::{
    Capacity, CapacityProbeProfile, CapacitySupportProbeProfile, Energy, FIXED_ONE, Rational,
    WireId,
};
use thiserror::Error;

/// One Wire's deterministic share of the global overcapacity-support demand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WireCapacitySupportShare {
    wire: WireId,
    length: Capacity,
    demand: Energy,
}

impl WireCapacitySupportShare {
    pub const fn wire(self) -> WireId {
        self.wire
    }

    pub const fn length(self) -> Capacity {
        self.length
    }

    pub const fn demand(self) -> Energy {
        self.demand
    }
}

/// Fail-closed errors from the pure capacity-support arithmetic kernel.
#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum CapacitySupportError {
    #[error("overcapacity linear coefficient must be nonnegative")]
    NegativeLinearCoefficient,

    #[error("overcapacity quadratic coefficient must be positive")]
    NonPositiveQuadraticCoefficient,

    #[error("support power per NCU must be positive")]
    NonPositiveSupportPowerPerNcu,

    #[error("capacity denominator floor must be positive")]
    ZeroCapacityDenominatorFloor,

    #[error("capacity denominator floor does not fit canonical raw Capacity")]
    CapacityDenominatorFloorOverflow,

    #[error("capacity-support exact u128 arithmetic overflow")]
    ArithmeticOverflow,

    #[error("capacity-support demand does not fit canonical Energy")]
    DemandOutOfRange,

    #[error("positive capacity-support accounting requires at least one Wire")]
    EmptyWireSet,

    #[error("Wire {wire:?} has zero capacity length")]
    ZeroWireLength { wire: WireId },

    #[error("Wire {wire:?} appears more than once in capacity-support distribution")]
    DuplicateWire { wire: WireId },

    #[error(
        "declared used Capacity {declared:?} does not equal distributed Wire length {actual:?}"
    )]
    UsedCapacityMismatch {
        declared: Capacity,
        actual: Capacity,
    },
}

/// Returns canonical raw excess Capacity, `max(0, used - supported)`.
pub const fn capacity_excess(used: Capacity, supported: Capacity) -> Capacity {
    Capacity(used.0.saturating_sub(supported.0))
}

/// Computes total overcapacity-support Energy with one, and only one, final ceiling.
///
/// `Capacity` arguments are canonical 16.16 raw NCU. `capacity_denominator_floor` remains a
/// whole-NCU Balance value and is converted to raw Capacity before it enters the exact rational.
/// Rational reductions below are exact; they never round an intermediate term.
pub fn calculate_capacity_support_demand(
    used: Capacity,
    supported: Capacity,
    capacity: &CapacityProbeProfile,
    support: &CapacitySupportProbeProfile,
) -> Result<Energy, CapacitySupportError> {
    let excess = capacity_excess(used, supported);
    if excess.0 == 0 {
        return Ok(Energy(0));
    }

    validate_active_coefficients(capacity, support)?;
    let denominator_floor_raw = capacity
        .capacity_denominator_floor
        .checked_mul(FIXED_ONE as u64)
        .ok_or(CapacitySupportError::CapacityDenominatorFloorOverflow)?;
    let curve_denominator = supported.0.max(denominator_floor_raw);

    let excess = ExactRatio::integer(u128::from(excess.0));
    let linear = ExactRatio::from_rational(capacity.overcap_linear_k)?.checked_mul(excess)?;
    let quadratic = ExactRatio::from_rational(capacity.overcap_quadratic_k)?
        .checked_mul(excess)?
        .checked_mul(excess)?
        .checked_div_integer(u128::from(curve_denominator))?;
    let raw_ncu_curve = linear.checked_add(quadratic)?;
    let energy = ExactRatio::from_rational(support.support_power_per_ncu)?
        .checked_mul(raw_ncu_curve)?
        .checked_div_integer(FIXED_ONE as u128)?
        .ceil()?;

    u64::try_from(energy)
        .map(Energy)
        .map_err(|_| CapacitySupportError::DemandOutOfRange)
}

/// Distributes integer Energy in ascending WireId order after proportional floor shares.
///
/// An empty zero-accounting input is the canonical no-Wire/no-demand result. Every nonempty input
/// must contain positive lengths whose exact sum equals `used`.
pub fn distribute_capacity_support_demand(
    used: Capacity,
    total_demand: Energy,
    wire_lengths: &[(WireId, Capacity)],
) -> Result<Vec<WireCapacitySupportShare>, CapacitySupportError> {
    if wire_lengths.is_empty() {
        return if used.0 == 0 && total_demand.0 == 0 {
            Ok(Vec::new())
        } else {
            Err(CapacitySupportError::EmptyWireSet)
        };
    }

    let mut wires = wire_lengths.to_vec();
    wires.sort_unstable_by_key(|(wire, _)| *wire);
    for pair in wires.windows(2) {
        if pair[0].0 == pair[1].0 {
            return Err(CapacitySupportError::DuplicateWire { wire: pair[0].0 });
        }
    }
    if let Some((wire, _)) = wires.iter().find(|(_, length)| length.0 == 0) {
        return Err(CapacitySupportError::ZeroWireLength { wire: *wire });
    }

    let actual_used_raw = wires.iter().try_fold(0_u64, |sum, (_, length)| {
        sum.checked_add(length.0)
            .ok_or(CapacitySupportError::ArithmeticOverflow)
    })?;
    let actual_used = Capacity(actual_used_raw);
    if actual_used != used {
        return Err(CapacitySupportError::UsedCapacityMismatch {
            declared: used,
            actual: actual_used,
        });
    }

    let denominator = u128::from(used.0);
    debug_assert!(
        denominator > 0,
        "positive Wire lengths imply positive used Capacity"
    );
    let mut allocated = 0_u64;
    let mut shares = wires
        .into_iter()
        .map(|(wire, length)| {
            let numerator = u128::from(total_demand.0)
                .checked_mul(u128::from(length.0))
                .ok_or(CapacitySupportError::ArithmeticOverflow)?;
            let floor = numerator / denominator;
            let floor =
                u64::try_from(floor).map_err(|_| CapacitySupportError::ArithmeticOverflow)?;
            allocated = allocated
                .checked_add(floor)
                .ok_or(CapacitySupportError::ArithmeticOverflow)?;
            Ok(WireCapacitySupportShare {
                wire,
                length,
                demand: Energy(floor),
            })
        })
        .collect::<Result<Vec<_>, CapacitySupportError>>()?;

    let remainder = total_demand
        .0
        .checked_sub(allocated)
        .ok_or(CapacitySupportError::ArithmeticOverflow)?;
    let remainder =
        usize::try_from(remainder).map_err(|_| CapacitySupportError::ArithmeticOverflow)?;
    if remainder > shares.len() {
        return Err(CapacitySupportError::ArithmeticOverflow);
    }
    for share in shares.iter_mut().take(remainder) {
        share.demand = Energy(
            share
                .demand
                .0
                .checked_add(1)
                .ok_or(CapacitySupportError::ArithmeticOverflow)?,
        );
    }

    debug_assert_eq!(
        shares.iter().map(|share| share.demand.0).sum::<u64>(),
        total_demand.0
    );
    Ok(shares)
}

fn validate_active_coefficients(
    capacity: &CapacityProbeProfile,
    support: &CapacitySupportProbeProfile,
) -> Result<(), CapacitySupportError> {
    if capacity.overcap_linear_k.numerator() < 0 {
        return Err(CapacitySupportError::NegativeLinearCoefficient);
    }
    if capacity.overcap_quadratic_k.numerator() <= 0 {
        return Err(CapacitySupportError::NonPositiveQuadraticCoefficient);
    }
    if support.support_power_per_ncu.numerator() <= 0 {
        return Err(CapacitySupportError::NonPositiveSupportPowerPerNcu);
    }
    if capacity.capacity_denominator_floor == 0 {
        return Err(CapacitySupportError::ZeroCapacityDenominatorFloor);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ExactRatio {
    numerator: u128,
    denominator: u128,
}

impl ExactRatio {
    const fn integer(value: u128) -> Self {
        Self {
            numerator: value,
            denominator: 1,
        }
    }

    fn from_rational(value: Rational) -> Result<Self, CapacitySupportError> {
        let numerator = u128::try_from(value.numerator())
            .map_err(|_| CapacitySupportError::ArithmeticOverflow)?;
        let denominator = u128::try_from(value.denominator())
            .map_err(|_| CapacitySupportError::ArithmeticOverflow)?;
        Self::new(numerator, denominator)
    }

    fn new(numerator: u128, denominator: u128) -> Result<Self, CapacitySupportError> {
        if denominator == 0 {
            return Err(CapacitySupportError::ArithmeticOverflow);
        }
        if numerator == 0 {
            return Ok(Self::integer(0));
        }
        let divisor = gcd(numerator, denominator);
        Ok(Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        })
    }

    fn checked_mul(self, rhs: Self) -> Result<Self, CapacitySupportError> {
        if self.numerator == 0 || rhs.numerator == 0 {
            return Ok(Self::integer(0));
        }
        let left_cross = gcd(self.numerator, rhs.denominator);
        let right_cross = gcd(rhs.numerator, self.denominator);
        let numerator = (self.numerator / left_cross)
            .checked_mul(rhs.numerator / right_cross)
            .ok_or(CapacitySupportError::ArithmeticOverflow)?;
        let denominator = (self.denominator / right_cross)
            .checked_mul(rhs.denominator / left_cross)
            .ok_or(CapacitySupportError::ArithmeticOverflow)?;
        Self::new(numerator, denominator)
    }

    fn checked_div_integer(self, divisor: u128) -> Result<Self, CapacitySupportError> {
        if divisor == 0 {
            return Err(CapacitySupportError::ArithmeticOverflow);
        }
        if self.numerator == 0 {
            return Ok(Self::integer(0));
        }
        let cancellation = gcd(self.numerator, divisor);
        let denominator = self
            .denominator
            .checked_mul(divisor / cancellation)
            .ok_or(CapacitySupportError::ArithmeticOverflow)?;
        Self::new(self.numerator / cancellation, denominator)
    }

    fn checked_add(self, rhs: Self) -> Result<Self, CapacitySupportError> {
        let common = gcd(self.denominator, rhs.denominator);
        let left_multiplier = rhs.denominator / common;
        let right_multiplier = self.denominator / common;
        let numerator = self
            .numerator
            .checked_mul(left_multiplier)
            .and_then(|left| {
                rhs.numerator
                    .checked_mul(right_multiplier)
                    .and_then(|right| left.checked_add(right))
            })
            .ok_or(CapacitySupportError::ArithmeticOverflow)?;
        let denominator = self
            .denominator
            .checked_mul(left_multiplier)
            .ok_or(CapacitySupportError::ArithmeticOverflow)?;
        Self::new(numerator, denominator)
    }

    fn ceil(self) -> Result<u128, CapacitySupportError> {
        let quotient = self.numerator / self.denominator;
        if self.numerator.is_multiple_of(self.denominator) {
            Ok(quotient)
        } else {
            quotient
                .checked_add(1)
                .ok_or(CapacitySupportError::ArithmeticOverflow)
        }
    }
}

const fn gcd(mut left: u128, mut right: u128) -> u128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}
