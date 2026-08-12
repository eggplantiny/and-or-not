use std::fmt;
use thiserror::Error;

pub const FIXED_ONE: i64 = 65_536;

macro_rules! canonical_unsigned_type {
    ($name:ident) => {
        #[repr(transparent)]
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub u64);

        impl $name {
            pub fn checked_add(self, rhs: Self) -> Result<Self, NumericError> {
                self.0
                    .checked_add(rhs.0)
                    .map(Self)
                    .ok_or(NumericError::Overflow)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

canonical_unsigned_type!(Tick);
canonical_unsigned_type!(Revision);
canonical_unsigned_type!(Energy);
canonical_unsigned_type!(HeatEnergy);
canonical_unsigned_type!(Integrity);
canonical_unsigned_type!(DriveStrength);
canonical_unsigned_type!(Capacity);

impl Capacity {
    /// Converts a whole-NCU Balance value to canonical Fixed-scale raw capacity.
    pub fn from_whole_ncu(value: u64) -> Result<Self, NumericError> {
        value
            .checked_mul(FIXED_ONE as u64)
            .map(Self)
            .ok_or(NumericError::Overflow)
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityId(pub u64);

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fixed(pub i64);

impl Fixed {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(FIXED_ONE);

    pub fn checked_add(self, rhs: Self) -> Result<Self, NumericError> {
        self.0
            .checked_add(rhs.0)
            .map(Self)
            .ok_or(NumericError::Overflow)
    }

    pub fn checked_sub(self, rhs: Self) -> Result<Self, NumericError> {
        self.0
            .checked_sub(rhs.0)
            .map(Self)
            .ok_or(NumericError::Overflow)
    }

    pub fn checked_mul(self, rhs: Self) -> Result<Self, NumericError> {
        let product = i128::from(self.0) * i128::from(rhs.0);
        let rounded = round_div_nearest_even(product, i128::from(FIXED_ONE))?;
        i64::try_from(rounded)
            .map(Self)
            .map_err(|_| NumericError::Overflow)
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum NumericError {
    #[error("canonical divisor must be positive")]
    NonPositiveDivisor,

    #[error("canonical numeric overflow")]
    Overflow,
}

pub fn floor_div(numerator: i128, denominator: i128) -> Result<i128, NumericError> {
    if denominator <= 0 {
        return Err(NumericError::NonPositiveDivisor);
    }
    Ok(numerator.div_euclid(denominator))
}

pub fn ceil_div_nonnegative(numerator: u128, denominator: u128) -> Result<u128, NumericError> {
    if denominator == 0 {
        return Err(NumericError::NonPositiveDivisor);
    }

    let quotient = numerator / denominator;
    if numerator.is_multiple_of(denominator) {
        Ok(quotient)
    } else {
        quotient.checked_add(1).ok_or(NumericError::Overflow)
    }
}

pub fn round_div_nearest_even(numerator: i128, denominator: i128) -> Result<i128, NumericError> {
    if denominator <= 0 {
        return Err(NumericError::NonPositiveDivisor);
    }

    let floor = numerator.div_euclid(denominator);
    let remainder = numerator.rem_euclid(denominator);
    let distance_to_floor = remainder;
    let distance_to_ceil = denominator - remainder;

    match distance_to_floor.cmp(&distance_to_ceil) {
        std::cmp::Ordering::Less => Ok(floor),
        std::cmp::Ordering::Greater => floor.checked_add(1).ok_or(NumericError::Overflow),
        std::cmp::Ordering::Equal if floor % 2 == 0 => Ok(floor),
        std::cmp::Ordering::Equal => floor.checked_add(1).ok_or(NumericError::Overflow),
    }
}

pub fn ceil_isqrt(value: u128) -> Result<u64, NumericError> {
    if value == 0 {
        return Ok(0);
    }

    let mut lower = 0_u64;
    let mut upper = u64::MAX;
    while lower < upper {
        let midpoint = lower + (upper - lower).div_ceil(2);
        if u128::from(midpoint) <= value / u128::from(midpoint) {
            lower = midpoint;
        } else {
            upper = midpoint - 1;
        }
    }

    let floor = lower;
    if u128::from(floor) * u128::from(floor) == value {
        Ok(floor)
    } else {
        floor.checked_add(1).ok_or(NumericError::Overflow)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Capacity, Fixed, NumericError, ceil_div_nonnegative, ceil_isqrt, floor_div,
        round_div_nearest_even,
    };

    #[test]
    fn floor_div_uses_mathematical_floor_for_negative_values() {
        assert_eq!(floor_div(-1, 65_536), Ok(-1));
        assert_eq!(floor_div(-65_536, 65_536), Ok(-1));
        assert_eq!(floor_div(-65_537, 65_536), Ok(-2));
    }

    #[test]
    fn ceil_div_nonnegative_never_erases_a_positive_remainder() {
        assert_eq!(ceil_div_nonnegative(0, 7), Ok(0));
        assert_eq!(ceil_div_nonnegative(1, 7), Ok(1));
        assert_eq!(ceil_div_nonnegative(7, 7), Ok(1));
        assert_eq!(ceil_div_nonnegative(8, 7), Ok(2));
    }

    #[test]
    fn nearest_even_is_symmetric_across_zero() {
        for (numerator, expected) in [
            (-7, -4),
            (-5, -2),
            (-3, -2),
            (-1, 0),
            (1, 0),
            (3, 2),
            (5, 2),
            (7, 4),
        ] {
            assert_eq!(round_div_nearest_even(numerator, 2), Ok(expected));
        }
    }

    #[test]
    fn all_division_helpers_reject_a_non_positive_divisor() {
        assert_eq!(floor_div(1, 0), Err(NumericError::NonPositiveDivisor));
        assert_eq!(floor_div(1, -1), Err(NumericError::NonPositiveDivisor));
        assert_eq!(
            ceil_div_nonnegative(1, 0),
            Err(NumericError::NonPositiveDivisor)
        );
        assert_eq!(
            round_div_nearest_even(1, 0),
            Err(NumericError::NonPositiveDivisor)
        );
    }

    #[test]
    fn ceil_isqrt_handles_exact_non_exact_and_maximum_values() {
        assert_eq!(ceil_isqrt(0), Ok(0));
        assert_eq!(ceil_isqrt(1), Ok(1));
        assert_eq!(ceil_isqrt(2), Ok(2));
        assert_eq!(ceil_isqrt(9), Ok(3));
        assert_eq!(ceil_isqrt(10), Ok(4));
        assert_eq!(ceil_isqrt(u128::from(u64::MAX).pow(2)), Ok(u64::MAX));
        assert_eq!(ceil_isqrt(u128::MAX), Err(NumericError::Overflow));
    }

    #[test]
    fn fixed_multiplication_uses_nearest_even_and_checked_output() {
        assert_eq!(Fixed(65_536).checked_mul(Fixed(32_768)), Ok(Fixed(32_768)));
        assert_eq!(
            Fixed(i64::MAX).checked_mul(Fixed(i64::MAX)),
            Err(NumericError::Overflow)
        );
    }

    #[test]
    fn whole_ncu_capacity_conversion_uses_fixed_raw_units_and_checks_overflow() {
        assert_eq!(Capacity::from_whole_ncu(1_000), Ok(Capacity(65_536_000)));
        assert_eq!(
            Capacity::from_whole_ncu(u64::MAX / 65_536 + 1),
            Err(NumericError::Overflow)
        );
    }

    #[test]
    fn division_helpers_satisfy_exhaustive_quotient_and_symmetry_invariants() {
        for denominator in 1_i128..=64 {
            for numerator in -4_096_i128..=4_096 {
                let floor = floor_div(numerator, denominator).expect("positive divisor");
                assert!(
                    floor * denominator <= numerator && numerator < (floor + 1) * denominator,
                    "floor bound failed for {numerator}/{denominator}: {floor}"
                );

                let negated_floor = floor_div(-numerator, denominator).expect("positive divisor");
                let expected_negated_floor = if numerator.rem_euclid(denominator) == 0 {
                    -floor
                } else {
                    -floor - 1
                };
                assert_eq!(
                    negated_floor, expected_negated_floor,
                    "floor symmetry failed for {numerator}/{denominator}"
                );

                let rounded =
                    round_div_nearest_even(numerator, denominator).expect("positive divisor");
                let residual = numerator - rounded * denominator;
                let twice_distance = residual.unsigned_abs() * 2;
                assert!(
                    twice_distance <= denominator as u128,
                    "nearest bound failed for {numerator}/{denominator}: {rounded}"
                );
                if twice_distance == denominator as u128 {
                    assert_eq!(
                        rounded.rem_euclid(2),
                        0,
                        "tie did not select even quotient for {numerator}/{denominator}"
                    );
                }
                assert_eq!(
                    round_div_nearest_even(-numerator, denominator),
                    Ok(-rounded),
                    "nearest-even symmetry failed for {numerator}/{denominator}"
                );
            }
        }

        for denominator in 1_u128..=64 {
            for numerator in 0_u128..=8_192 {
                let ceil = ceil_div_nonnegative(numerator, denominator).expect("positive divisor");
                if numerator == 0 {
                    assert_eq!(ceil, 0);
                } else {
                    assert!(
                        (ceil - 1) * denominator < numerator && numerator <= ceil * denominator,
                        "ceil bound failed for {numerator}/{denominator}: {ceil}"
                    );
                }
                assert_eq!(
                    floor_div(-(numerator as i128), denominator as i128),
                    Ok(-(ceil as i128)),
                    "floor/ceil reflection failed for {numerator}/{denominator}"
                );
            }
        }
    }

    #[test]
    fn division_helpers_preserve_extreme_boundary_values() {
        assert_eq!(floor_div(i128::MIN, 1), Ok(i128::MIN));
        assert_eq!(floor_div(i128::MAX, 1), Ok(i128::MAX));
        assert_eq!(round_div_nearest_even(i128::MIN, 1), Ok(i128::MIN));
        assert_eq!(round_div_nearest_even(i128::MAX, 1), Ok(i128::MAX));
        assert_eq!(ceil_div_nonnegative(u128::MAX, 1), Ok(u128::MAX));
        assert_eq!(ceil_div_nonnegative(u128::MAX, u128::MAX), Ok(1));
        assert_eq!(ceil_div_nonnegative(u128::MAX, u128::MAX - 1), Ok(2));
    }

    fn assert_ceil_isqrt_bounds(value: u128) {
        let root = ceil_isqrt(value).expect("value has a representable ceiling square root");
        let root = u128::from(root);
        assert!(
            value <= root * root,
            "upper square-root bound failed for {value}: {root}"
        );
        if value == 0 {
            assert_eq!(root, 0);
        } else {
            assert!(
                (root - 1) * (root - 1) < value,
                "lower square-root bound failed for {value}: {root}"
            );
        }
    }

    #[test]
    fn ceil_isqrt_satisfies_exhaustive_and_seeded_boundary_invariants() {
        for value in 0_u128..=65_536 {
            assert_ceil_isqrt_bounds(value);
        }

        for root in 0_u128..=4_096 {
            let square = root * root;
            assert_eq!(ceil_isqrt(square), Ok(root as u64));
            if root < u128::from(u64::MAX) {
                assert_eq!(ceil_isqrt(square + 1), Ok(root as u64 + 1));
            }
        }

        let maximum_square = u128::from(u64::MAX).pow(2);
        let mut seed = 0x8a5c_d789_635d_2dff_u64;
        for _ in 0..4_096 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let high = u128::from(seed);
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let value = ((high << 64) | u128::from(seed)) % (maximum_square + 1);
            assert_ceil_isqrt_bounds(value);
        }

        assert_eq!(ceil_isqrt(maximum_square), Ok(u64::MAX));
        assert_eq!(ceil_isqrt(maximum_square + 1), Err(NumericError::Overflow));
        assert_eq!(ceil_isqrt(u128::MAX), Err(NumericError::Overflow));
    }
}
