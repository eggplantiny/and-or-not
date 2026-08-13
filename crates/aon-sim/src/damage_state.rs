use crate::{HeatEnergy, Integrity};

/// Canonical integrity and owned thermal energy for a damageable structural object.
///
/// The component is optional on retained pre-v5 structural records. S1-M4 worlds attach it at
/// creation time, so absence is meaningful and must not be replaced by sentinel values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DamageState {
    pub integrity: Integrity,
    pub heat_energy: HeatEnergy,
}

impl DamageState {
    pub const fn new(integrity: Integrity, heat_energy: HeatEnergy) -> Self {
        Self {
            integrity,
            heat_energy,
        }
    }

    pub const fn pristine(integrity: Integrity) -> Self {
        Self::new(integrity, HeatEnergy(0))
    }
}
