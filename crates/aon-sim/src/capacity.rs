use crate::structural::StructuralWorld;
use crate::{
    Capacity, CapacityProbeProfile, CapacitySupportError, CapacitySupportProbeProfile, Energy,
    MainCoreId, MainCoreState, NumericError, SimulationError, Tick, WireCapacitySupportShare,
    WireId, calculate_capacity_support_demand, capacity_excess, distribute_capacity_support_demand,
    polyline_length,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WireCapacityUsage {
    wire: WireId,
    length: Capacity,
    support_demand: Option<Energy>,
}

impl WireCapacityUsage {
    pub const fn wire(self) -> WireId {
        self.wire
    }

    pub const fn length(self) -> Capacity {
        self.length
    }

    /// Returns `None` before Balance v4 and `Some`, including zero, when capacity support is active.
    pub const fn support_demand(self) -> Option<Energy> {
        self.support_demand
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NetworkAccounting {
    used: Capacity,
    supported: Capacity,
    excess: Option<Capacity>,
    total_support_demand: Option<Energy>,
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

    /// Returns `None` before Balance v4 and `Some`, including zero, when capacity support is active.
    pub const fn excess(self) -> Option<Capacity> {
        self.excess
    }

    /// Returns `None` before Balance v4 and `Some`, including zero, when capacity support is active.
    pub const fn total_support_demand(self) -> Option<Energy> {
        self.total_support_demand
    }
}

/// One internally shared Phase-4 accounting result. Wire lengths are measured exactly once and
/// reused for both Capacity reporting and Power demand collection.
pub(crate) struct AccountedNetwork {
    accounting: NetworkAccounting,
    wires: Vec<WireCapacityUsage>,
    support_shares: Vec<WireCapacitySupportShare>,
}

impl AccountedNetwork {
    pub(crate) const fn accounting(&self) -> NetworkAccounting {
        self.accounting
    }

    pub(crate) fn wires(&self) -> &[WireCapacityUsage] {
        &self.wires
    }

    pub(crate) fn support_shares(&self) -> &[WireCapacitySupportShare] {
        &self.support_shares
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

pub(crate) fn account_network_with_support(
    structural: &StructuralWorld,
    main_core: Option<&MainCoreState>,
    capacity_probe: Option<CapacityProbeProfile>,
    support_probe: Option<CapacitySupportProbeProfile>,
) -> Result<AccountedNetwork, SimulationError> {
    let mut wires = wire_capacity_rows(structural)?;
    let used = checked_usage_sum(wires.iter().map(|row| row.length))?;
    let supported = main_core.map_or(Capacity(0), |core| core.capacity());
    let (excess, total_support_demand, support_shares) = match support_probe {
        None => (None, None, Vec::new()),
        Some(support_probe) => {
            let excess = capacity_excess(used, supported);
            let capacity_probe = capacity_probe.ok_or(SimulationError::InvalidCanonicalState)?;
            let total =
                calculate_capacity_support_demand(used, supported, &capacity_probe, &support_probe)
                    .map_err(map_capacity_support_error)?;
            let lengths = wires
                .iter()
                .map(|row| (row.wire, row.length))
                .collect::<Vec<_>>();
            let shares = distribute_capacity_support_demand(used, total, &lengths)
                .map_err(map_capacity_support_error)?;
            for (row, share) in wires.iter_mut().zip(&shares) {
                if row.wire != share.wire() || row.length != share.length() {
                    return Err(SimulationError::InvalidCanonicalState);
                }
                row.support_demand = Some(share.demand());
            }
            (Some(excess), Some(total), shares)
        }
    };
    Ok(AccountedNetwork {
        accounting: NetworkAccounting {
            used,
            supported,
            excess,
            total_support_demand,
        },
        wires,
        support_shares,
    })
}

pub(crate) fn analyzer_snapshot(
    next_tick: Tick,
    structural: &StructuralWorld,
    main_core: &MainCoreState,
    capacity_probe: Option<CapacityProbeProfile>,
    support_probe: Option<CapacitySupportProbeProfile>,
) -> Result<NetworkAnalyzerSnapshot, SimulationError> {
    let accounted =
        account_network_with_support(structural, Some(main_core), capacity_probe, support_probe)?;
    Ok(NetworkAnalyzerSnapshot {
        next_tick,
        accounting: accounted.accounting,
        main_core_contribution: MainCoreCapacityContribution {
            main_core: main_core.id(),
            capacity: main_core.capacity(),
        },
        wires: accounted.wires,
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
                support_demand: None,
            })
        })
        .collect::<Result<Vec<_>, SimulationError>>()?;
    wires.sort_unstable_by_key(|row| row.wire);
    Ok(wires)
}

fn map_capacity_support_error(error: CapacitySupportError) -> SimulationError {
    match error {
        CapacitySupportError::CapacityDenominatorFloorOverflow
        | CapacitySupportError::ArithmeticOverflow
        | CapacitySupportError::DemandOutOfRange => SimulationError::NumericOverflow,
        _ => SimulationError::InvalidCanonicalState,
    }
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
