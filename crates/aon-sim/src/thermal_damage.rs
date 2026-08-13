use crate::{
    ContactDamageProbeProfile, DemandId, Energy, EntityId, FIXED_ONE, Fixed, HeatEnergy, Integrity,
    Rational, round_div_nearest_even,
};
use thiserror::Error;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ThermalObjectKind {
    MainCore = 0,
    Wire = 1,
    Gate = 2,
    Junction = 3,
    FixedSubstrate = 4,
    MobileSubstrate = 5,
    Enemy = 6,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DamageKind {
    Electrical = 0,
    Thermal = 1,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InteractionHeatKind {
    GatePowerDissipation = 0,
    Movement = 1,
    Construction = 2,
    LiveWireRemainder = 3,
    CancelledGateSwitch = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HeatContributionKey {
    pub kind: InteractionHeatKind,
    pub source: EntityId,
    pub demand: Option<DemandId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeatContributionInput {
    pub key: HeatContributionKey,
    pub energy: HeatEnergy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DamageSnapshot {
    pub target: EntityId,
    pub kind: ThermalObjectKind,
    pub integrity: Integrity,
    pub phase1_temperature: Fixed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ElectricalExposure {
    pub target: EntityId,
    pub source: EntityId,
    pub energy: Energy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DamageResolution {
    pub target: EntityId,
    pub integrity_before: Integrity,
    pub electrical_exposure: Energy,
    pub electrical_damage: Integrity,
    pub thermal_damage: Integrity,
    pub integrity_after: Integrity,
    pub pending_destruction: bool,
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum ThermalDamageError {
    #[error("thermal object {0:?} has no canonical state")]
    UnknownTarget(EntityId),

    #[error("thermal object {0:?} appears more than once")]
    DuplicateTarget(EntityId),

    #[error("thermal object kind is unsupported")]
    UnsupportedTargetKind,

    #[error("thermal capacity for {0:?} must be positive")]
    NonPositiveThermalCapacity(ThermalObjectKind),

    #[error("electrical tolerance for {0:?} must be positive")]
    NonPositiveElectricalTolerance(ThermalObjectKind),

    #[error("thermal coefficient is invalid")]
    InvalidThermalCoefficient,

    #[error("canonical thermal/damage arithmetic overflow")]
    ArithmeticOverflow,

    #[error("temperature is outside the canonical nonnegative range")]
    TemperatureOutOfRange,

    #[error("damage is outside the canonical range")]
    DamageOutOfRange,

    #[error("electrical exposure targets a non-damageable object {0:?}")]
    ExposureToNonDamageable(EntityId),

    #[error("Heat contributions for {owner:?} are not in canonical key order")]
    NonCanonicalHeatOrder { owner: EntityId },

    #[error("Heat contribution key is duplicated for {owner:?}")]
    DuplicateHeatContribution { owner: EntityId },

    #[error("Heat contribution for {owner:?} must be positive")]
    NonPositiveHeatContribution { owner: EntityId },

    #[error("electrical exposures are not in canonical target/source order")]
    NonCanonicalExposureOrder,

    #[error("electrical exposure source {exposure_source:?} is duplicated for {target:?}")]
    DuplicateExposureSource {
        target: EntityId,
        exposure_source: EntityId,
    },

    #[error("electrical exposure target {actual:?} does not match snapshot {expected:?}")]
    ExposureTargetMismatch {
        expected: EntityId,
        actual: EntityId,
    },
}

pub const fn thermal_capacity_for(
    kind: ThermalObjectKind,
    profile: &ContactDamageProbeProfile,
) -> u64 {
    match kind {
        ThermalObjectKind::MainCore => profile.thermal_capacity.main_core,
        ThermalObjectKind::Wire => profile.thermal_capacity.wire,
        ThermalObjectKind::Gate => profile.thermal_capacity.gate,
        ThermalObjectKind::Junction => profile.thermal_capacity.junction,
        ThermalObjectKind::FixedSubstrate => profile.thermal_capacity.fixed_substrate,
        ThermalObjectKind::MobileSubstrate => profile.thermal_capacity.mobile_substrate,
        ThermalObjectKind::Enemy => profile.thermal_capacity.enemy,
    }
}

pub const fn electrical_tolerance_for(
    kind: ThermalObjectKind,
    profile: &ContactDamageProbeProfile,
) -> u64 {
    match kind {
        ThermalObjectKind::MainCore => profile.electrical_tolerance.main_core,
        ThermalObjectKind::Wire => profile.electrical_tolerance.wire,
        ThermalObjectKind::Gate => profile.electrical_tolerance.gate,
        ThermalObjectKind::Junction => profile.electrical_tolerance.junction,
        ThermalObjectKind::FixedSubstrate => profile.electrical_tolerance.fixed_substrate,
        ThermalObjectKind::MobileSubstrate => profile.electrical_tolerance.mobile_substrate,
        ThermalObjectKind::Enemy => profile.electrical_tolerance.enemy,
    }
}

pub fn integrate_heat(
    owner: EntityId,
    current: HeatEnergy,
    contributions: &[HeatContributionInput],
) -> Result<HeatEnergy, ThermalDamageError> {
    for (index, contribution) in contributions.iter().enumerate() {
        if contribution.energy.0 == 0 {
            return Err(ThermalDamageError::NonPositiveHeatContribution { owner });
        }
        if let Some(previous) = index
            .checked_sub(1)
            .map(|previous| contributions[previous].key)
        {
            if previous == contribution.key {
                return Err(ThermalDamageError::DuplicateHeatContribution { owner });
            }
            if previous > contribution.key {
                return Err(ThermalDamageError::NonCanonicalHeatOrder { owner });
            }
        }
    }

    contributions.iter().try_fold(current, |sum, contribution| {
        sum.checked_add(contribution.energy)
            .map_err(|_| ThermalDamageError::ArithmeticOverflow)
    })
}

pub fn resolve_damage(
    snapshot: DamageSnapshot,
    electrical: &[ElectricalExposure],
    probe: &ContactDamageProbeProfile,
) -> Result<DamageResolution, ThermalDamageError> {
    if snapshot.phase1_temperature.0 < 0 {
        return Err(ThermalDamageError::TemperatureOutOfRange);
    }

    let tolerance = electrical_tolerance_for(snapshot.kind, probe);
    if tolerance == 0 {
        return Err(ThermalDamageError::NonPositiveElectricalTolerance(
            snapshot.kind,
        ));
    }
    if thermal_capacity_for(snapshot.kind, probe) == 0 {
        return Err(ThermalDamageError::NonPositiveThermalCapacity(
            snapshot.kind,
        ));
    }

    let mut total_exposure = 0_u64;
    let mut previous: Option<(EntityId, EntityId)> = None;
    for exposure in electrical {
        if exposure.target != snapshot.target {
            return Err(ThermalDamageError::ExposureTargetMismatch {
                expected: snapshot.target,
                actual: exposure.target,
            });
        }
        let key = (exposure.target, exposure.source);
        if let Some(previous) = previous {
            if previous == key {
                return Err(ThermalDamageError::DuplicateExposureSource {
                    target: key.0,
                    exposure_source: key.1,
                });
            }
            if previous > key {
                return Err(ThermalDamageError::NonCanonicalExposureOrder);
            }
        }
        previous = Some(key);
        total_exposure = total_exposure
            .checked_add(exposure.energy.0)
            .ok_or(ThermalDamageError::ArithmeticOverflow)?;
    }

    let electrical_damage = total_exposure / tolerance;
    let thermal_excess = snapshot
        .phase1_temperature
        .0
        .saturating_sub(probe.safe_temperature.0)
        .max(0);
    let thermal_damage = scaled_thermal_damage(probe.thermal_damage_rate, thermal_excess)?;
    let total_damage = electrical_damage
        .checked_add(thermal_damage)
        .ok_or(ThermalDamageError::ArithmeticOverflow)?;
    let integrity_after = snapshot.integrity.0.saturating_sub(total_damage);

    Ok(DamageResolution {
        target: snapshot.target,
        integrity_before: snapshot.integrity,
        electrical_exposure: Energy(total_exposure),
        electrical_damage: Integrity(electrical_damage),
        thermal_damage: Integrity(thermal_damage),
        integrity_after: Integrity(integrity_after),
        pending_destruction: snapshot.integrity.0 > 0 && integrity_after == 0,
    })
}

fn scaled_thermal_damage(
    coefficient: Rational,
    thermal_excess_raw: i64,
) -> Result<u64, ThermalDamageError> {
    if coefficient.numerator() <= 0 || coefficient.denominator() <= 0 {
        return Err(ThermalDamageError::InvalidThermalCoefficient);
    }
    let numerator = i128::from(coefficient.numerator())
        .checked_mul(i128::from(thermal_excess_raw))
        .ok_or(ThermalDamageError::ArithmeticOverflow)?;
    let denominator = i128::from(coefficient.denominator())
        .checked_mul(i128::from(FIXED_ONE))
        .ok_or(ThermalDamageError::ArithmeticOverflow)?;
    let result = round_div_nearest_even(numerator, denominator)
        .map_err(|_| ThermalDamageError::ArithmeticOverflow)?;
    u64::try_from(result).map_err(|_| ThermalDamageError::DamageOutOfRange)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BalanceProfile;

    fn probe() -> ContactDamageProbeProfile {
        BalanceProfile::construction_contact_damage_alpha("thermal-test")
            .contact_damage_probe
            .expect("v5 probe")
    }

    fn key(kind: InteractionHeatKind, source: u64) -> HeatContributionKey {
        HeatContributionKey {
            kind,
            source: EntityId(source),
            demand: None,
        }
    }

    #[test]
    fn all_seven_thermal_object_kind_selectors_use_their_exact_profile_fields() {
        let mut probe = probe();
        probe.thermal_capacity.main_core = 11;
        probe.thermal_capacity.wire = 12;
        probe.thermal_capacity.gate = 13;
        probe.thermal_capacity.junction = 14;
        probe.thermal_capacity.fixed_substrate = 15;
        probe.thermal_capacity.mobile_substrate = 16;
        probe.thermal_capacity.enemy = 17;
        probe.electrical_tolerance.main_core = 21;
        probe.electrical_tolerance.wire = 22;
        probe.electrical_tolerance.gate = 23;
        probe.electrical_tolerance.junction = 24;
        probe.electrical_tolerance.fixed_substrate = 25;
        probe.electrical_tolerance.mobile_substrate = 26;
        probe.electrical_tolerance.enemy = 27;

        let cases = [
            (ThermalObjectKind::MainCore, 0, 11, 21),
            (ThermalObjectKind::Wire, 1, 12, 22),
            (ThermalObjectKind::Gate, 2, 13, 23),
            (ThermalObjectKind::Junction, 3, 14, 24),
            (ThermalObjectKind::FixedSubstrate, 4, 15, 25),
            (ThermalObjectKind::MobileSubstrate, 5, 16, 26),
            (ThermalObjectKind::Enemy, 6, 17, 27),
        ];
        for (kind, tag, capacity, tolerance) in cases {
            assert_eq!(kind as u8, tag);
            assert_eq!(thermal_capacity_for(kind, &probe), capacity);
            assert_eq!(electrical_tolerance_for(kind, &probe), tolerance);
        }
    }

    #[test]
    fn heat_integration_requires_positive_canonical_unique_rows() {
        let owner = EntityId(7);
        let rows = [
            HeatContributionInput {
                key: key(InteractionHeatKind::Movement, 2),
                energy: HeatEnergy(3),
            },
            HeatContributionInput {
                key: key(InteractionHeatKind::Construction, 3),
                energy: HeatEnergy(5),
            },
        ];
        assert_eq!(
            integrate_heat(owner, HeatEnergy(11), &rows),
            Ok(HeatEnergy(19))
        );

        let mut reversed = rows;
        reversed.reverse();
        assert_eq!(
            integrate_heat(owner, HeatEnergy(0), &reversed),
            Err(ThermalDamageError::NonCanonicalHeatOrder { owner })
        );
        assert_eq!(
            integrate_heat(owner, HeatEnergy(0), &[rows[0], rows[0]]),
            Err(ThermalDamageError::DuplicateHeatContribution { owner })
        );
        let zero = HeatContributionInput {
            energy: HeatEnergy(0),
            ..rows[0]
        };
        assert_eq!(
            integrate_heat(owner, HeatEnergy(0), &[zero]),
            Err(ThermalDamageError::NonPositiveHeatContribution { owner })
        );
    }

    #[test]
    fn electrical_and_phase1_thermal_damage_reduce_simultaneously() {
        let probe = probe();
        let target = EntityId(9);
        let snapshot = DamageSnapshot {
            target,
            kind: ThermalObjectKind::Wire,
            integrity: Integrity(10),
            phase1_temperature: Fixed(3 * FIXED_ONE),
        };
        let exposures = [
            ElectricalExposure {
                target,
                source: EntityId(2),
                energy: Energy(3),
            },
            ElectricalExposure {
                target,
                source: EntityId(5),
                energy: Energy(4),
            },
        ];
        let resolution = resolve_damage(snapshot, &exposures, &probe).expect("valid damage");
        assert_eq!(resolution.electrical_exposure, Energy(7));
        assert_eq!(resolution.electrical_damage, Integrity(7));
        assert_eq!(resolution.thermal_damage, Integrity(2));
        assert_eq!(resolution.integrity_after, Integrity(1));
        assert!(!resolution.pending_destruction);

        let lethal = resolve_damage(
            snapshot,
            &[ElectricalExposure {
                target,
                source: EntityId(2),
                energy: Energy(10),
            }],
            &probe,
        )
        .expect("lethal damage still resolves");
        assert_eq!(lethal.integrity_after, Integrity(0));
        assert!(lethal.pending_destruction);
    }

    #[test]
    fn below_safe_temperature_has_zero_thermal_damage() {
        let probe = probe();
        let snapshot = DamageSnapshot {
            target: EntityId(9),
            kind: ThermalObjectKind::Wire,
            integrity: Integrity(10),
            phase1_temperature: Fixed::ZERO,
        };

        let resolution = resolve_damage(snapshot, &[], &probe).expect("below-safe state is valid");
        assert_eq!(resolution.thermal_damage, Integrity(0));
        assert_eq!(resolution.integrity_after, Integrity(10));
        assert!(!resolution.pending_destruction);
    }

    #[test]
    fn exposure_order_duplicate_target_and_temperature_faults_are_typed() {
        let probe = probe();
        let snapshot = DamageSnapshot {
            target: EntityId(4),
            kind: ThermalObjectKind::Enemy,
            integrity: Integrity(10),
            phase1_temperature: Fixed::ZERO,
        };
        let row = ElectricalExposure {
            target: snapshot.target,
            source: EntityId(8),
            energy: Energy(1),
        };
        assert_eq!(
            resolve_damage(snapshot, &[row, row], &probe),
            Err(ThermalDamageError::DuplicateExposureSource {
                target: snapshot.target,
                exposure_source: row.source,
            })
        );
        assert_eq!(
            resolve_damage(
                snapshot,
                &[ElectricalExposure {
                    target: EntityId(5),
                    ..row
                }],
                &probe,
            ),
            Err(ThermalDamageError::ExposureTargetMismatch {
                expected: snapshot.target,
                actual: EntityId(5),
            })
        );
        assert_eq!(
            resolve_damage(
                DamageSnapshot {
                    phase1_temperature: Fixed(-1),
                    ..snapshot
                },
                &[],
                &probe,
            ),
            Err(ThermalDamageError::TemperatureOutOfRange)
        );
    }
}
