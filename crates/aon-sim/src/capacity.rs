use crate::structural::StructuralWorld;
use crate::{
    Capacity, MainCoreId, MainCoreState, NumericError, SimulationError, Tick, WireId,
    polyline_length,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WireCapacityUsage {
    wire: WireId,
    length: Capacity,
}

impl WireCapacityUsage {
    pub const fn wire(self) -> WireId {
        self.wire
    }

    pub const fn length(self) -> Capacity {
        self.length
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NetworkAccounting {
    used: Capacity,
    supported: Capacity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MainCoreCapacityContribution {
    main_core: MainCoreId,
    capacity: Capacity,
}

impl MainCoreCapacityContribution {
    pub const fn main_core(self) -> MainCoreId {
        self.main_core
    }

    pub const fn capacity(self) -> Capacity {
        self.capacity
    }
}

impl NetworkAccounting {
    pub const fn used(self) -> Capacity {
        self.used
    }

    pub const fn supported(self) -> Capacity {
        self.supported
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkAnalyzerSnapshot {
    next_tick: Tick,
    accounting: NetworkAccounting,
    main_core_contribution: MainCoreCapacityContribution,
    wires: Vec<WireCapacityUsage>,
}

impl NetworkAnalyzerSnapshot {
    pub const fn next_tick(&self) -> Tick {
        self.next_tick
    }

    pub const fn accounting(&self) -> NetworkAccounting {
        self.accounting
    }

    pub const fn main_core_contribution(&self) -> MainCoreCapacityContribution {
        self.main_core_contribution
    }

    pub fn wires(&self) -> &[WireCapacityUsage] {
        &self.wires
    }
}

pub(crate) fn account_network(
    structural: &StructuralWorld,
    main_core: Option<&MainCoreState>,
) -> Result<NetworkAccounting, SimulationError> {
    let wires = wire_capacity_rows(structural)?;
    let used = checked_usage_sum(wires.iter().map(|row| row.length))?;
    Ok(NetworkAccounting {
        used,
        supported: main_core.map_or(Capacity(0), |core| core.capacity()),
    })
}

pub(crate) fn analyzer_snapshot(
    next_tick: Tick,
    structural: &StructuralWorld,
    main_core: &MainCoreState,
) -> Result<NetworkAnalyzerSnapshot, SimulationError> {
    let wires = wire_capacity_rows(structural)?;
    let used = checked_usage_sum(wires.iter().map(|row| row.length))?;
    Ok(NetworkAnalyzerSnapshot {
        next_tick,
        accounting: NetworkAccounting {
            used,
            supported: main_core.capacity(),
        },
        main_core_contribution: MainCoreCapacityContribution {
            main_core: main_core.id(),
            capacity: main_core.capacity(),
        },
        wires,
    })
}

fn wire_capacity_rows(
    structural: &StructuralWorld,
) -> Result<Vec<WireCapacityUsage>, SimulationError> {
    let mut wires = structural
        .wires()
        .iter_alive()
        .map(|(_, wire)| {
            Ok(WireCapacityUsage {
                wire: wire.id,
                length: capacity_from_wire_length(polyline_length(wire.points)?)?,
            })
        })
        .collect::<Result<Vec<_>, SimulationError>>()?;
    wires.sort_unstable_by_key(|row| row.wire);
    Ok(wires)
}

fn capacity_from_wire_length(length: crate::Fixed) -> Result<Capacity, SimulationError> {
    u64::try_from(length.0)
        .map(Capacity)
        .map_err(|_| SimulationError::InvalidCanonicalState)
}

fn checked_usage_sum(
    lengths: impl IntoIterator<Item = Capacity>,
) -> Result<Capacity, NumericError> {
    lengths
        .into_iter()
        .try_fold(Capacity(0), Capacity::checked_add)
}

#[cfg(test)]
mod tests {
    use super::checked_usage_sum;
    use crate::{Capacity, NumericError};

    #[test]
    fn aggregate_usage_checks_u64_overflow_without_saturation() {
        assert_eq!(
            checked_usage_sum([Capacity(u64::MAX - 1), Capacity(1)]),
            Ok(Capacity(u64::MAX))
        );
        assert_eq!(
            checked_usage_sum([Capacity(u64::MAX), Capacity(1)]),
            Err(NumericError::Overflow)
        );
    }
}
