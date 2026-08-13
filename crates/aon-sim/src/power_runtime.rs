use crate::power::{
    CanonicalPowerRoute, DemandId, DemandKind, PowerDemand, PowerError, PowerLossCoefficient,
    PowerRatio, PowerRegionId, PowerSourceState, solve_power_region,
};
use crate::power_source::PowerSourceStore;
use crate::power_topology::{CompiledPowerTopology, PowerLoadAttachment, PowerNodeKey};
use crate::profile::{CapacityProbeProfile, PowerProbeProfile, Rational};
use crate::{
    DriveStrength, DriverSample, Energy, EntityId, FIXED_ONE, Fixed, GateId, HeatEnergy,
    LogicLevel, MobileId, PowerSourceId, Tick, WireCapacitySupportShare, WireEnd, WireId,
};
use std::collections::BTreeSet;
use thiserror::Error;

/// Phase-3 facts needed to derive the M2 Gate demand categories in Phase 4.
///
/// `switch_energy` is present only for a new or replacement ordinary Gate transition. Keeping an
/// identical pending transition, and scheduling the exceptional retention-expiry LOW, both pass
/// `None` so Power is not charged a second time or retroactively re-solved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GatePowerDemandInput {
    pub gate: GateId,
    pub output_has_reachable_load: bool,
    pub switch_energy: Option<Energy>,
}

/// Geometry needed to attach both intrinsic Wire loads at the orientation-neutral Wire body node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WirePowerDemandInput {
    pub wire: WireId,
    pub length: Fixed,
}

/// One Phase-3 Mobile intent and its immutable Phase-1 Track attachment.
///
/// `offset` uses the Track/Wire stored A-to-B polyline convention. `PowerNodeKey::WireOffset`
/// performs endpoint and midpoint coalescing when the Power topology is compiled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MovementPowerDemandInput {
    pub mobile: MobileId,
    pub wire: WireId,
    pub offset: Fixed,
    pub base_distance: Fixed,
    pub movement_enabled: bool,
}

/// One stable, pre-solve nominal load. It is derived Tick scratch, never canonical state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NominalPowerDemand {
    id: DemandId,
    nominal: Energy,
    node: PowerNodeKey,
}

impl NominalPowerDemand {
    pub(crate) fn new(
        owner: EntityId,
        kind: DemandKind,
        nominal: Energy,
        node: PowerNodeKey,
    ) -> Self {
        Self {
            id: DemandId::new(owner, kind),
            nominal,
            node,
        }
    }

    pub const fn id(&self) -> DemandId {
        self.id
    }

    pub const fn owner(&self) -> EntityId {
        self.id.owner()
    }

    pub const fn kind(&self) -> DemandKind {
        self.id.kind()
    }

    pub const fn nominal(&self) -> Energy {
        self.nominal
    }

    pub const fn node(&self) -> PowerNodeKey {
        self.node
    }

    pub const fn load_attachment(&self) -> PowerLoadAttachment {
        PowerLoadAttachment {
            demand: self.id,
            node: self.node,
        }
    }
}

/// Complete Phase-4 M2 load collection, sorted by `DemandId` and duplicate-free.
///
/// Requiring this value at the solve boundary makes it impossible for the region loop to grant one
/// load before a later load has been collected.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NominalPowerDemandSet {
    demands: Vec<NominalPowerDemand>,
}

impl NominalPowerDemandSet {
    fn from_unsorted(mut demands: Vec<NominalPowerDemand>) -> Result<Self, PowerRuntimeError> {
        demands.sort_unstable_by_key(NominalPowerDemand::id);
        if let Some(pair) = demands.windows(2).find(|pair| pair[0].id == pair[1].id) {
            return Err(PowerRuntimeError::DuplicateDemand { demand: pair[0].id });
        }
        Ok(Self { demands })
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = &NominalPowerDemand> {
        self.demands.iter()
    }

    pub fn load_attachments(&self) -> impl ExactSizeIterator<Item = PowerLoadAttachment> + '_ {
        self.demands.iter().map(NominalPowerDemand::load_attachment)
    }

    pub fn get(&self, id: DemandId) -> Option<&NominalPowerDemand> {
        self.demands
            .binary_search_by_key(&id, NominalPowerDemand::id)
            .ok()
            .map(|index| &self.demands[index])
    }

    pub const fn len(&self) -> usize {
        self.demands.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.demands.is_empty()
    }

    /// Adds S1-M4 interaction loads before the Phase-4 demand set is frozen.
    ///
    /// The complete result is re-canonicalized by `DemandId`, so an interaction load can never
    /// silently shadow an ordinary Power load.
    pub(crate) fn with_additional(
        mut self,
        additional: impl IntoIterator<Item = NominalPowerDemand>,
    ) -> Result<Self, PowerRuntimeError> {
        self.demands.extend(additional);
        Self::from_unsorted(self.demands)
    }
}

/// Stable region observation. `first_node` is the report key; neither it nor `region` is durable
/// canonical identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PowerRegionReport {
    pub region: PowerRegionId,
    pub first_node: PowerNodeKey,
    pub sources: Vec<PowerSourceId>,
    pub generation: Energy,
    pub total_nominal_demand: Energy,
    pub ratio: PowerRatio,
}

/// Stable per-load observation after the common region ratio has been solved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PowerLoadReport {
    pub demand: DemandId,
    pub region: PowerRegionId,
    pub nominal: Energy,
    pub granted: Energy,
    pub ratio: PowerRatio,
    pub source_route: Option<CanonicalPowerRoute>,
    pub transmission_loss: Energy,
    pub source_cost: Energy,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PowerHeatKind {
    LeakageDissipation = 0,
    TransmissionLoss = 1,
    OvercapacitySupport = 2,
}

/// Positive Phase-8 heat contribution. It is report scratch and does not mutate canonical thermal
/// state before S1-M4.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerHeatReport {
    pub owner: WireId,
    pub kind: PowerHeatKind,
    pub demand: DemandId,
    pub energy: HeatEnergy,
}

/// One stable Sense-end observation after Phase 6 has selected this Tick's intended response.
///
/// `current_driver` is the Phase-2 driver sample that was current while the intent was selected;
/// the intended fields are the delayed target recorded during Phase 6. A Wire contributes exactly
/// two rows, ordered A then B.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerSenseReport {
    pub wire: WireId,
    pub end: WireEnd,
    pub sampled_presence: bool,
    pub intended_level: LogicLevel,
    pub intended_strength: DriveStrength,
    pub current_driver: DriverSample,
}

/// One stable Gate brownout observation after Phase 6 has updated its retention counter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerGateReport {
    pub gate: GateId,
    pub ratio: PowerRatio,
    pub effective_delay: Tick,
    pub effective_drive: DriveStrength,
    pub unpowered_ticks: u64,
}

/// One stable Mobile budget observation for the Phase-3 intent staged in this Tick.
///
/// STOP/disabled intents have zero nominal and granted budgets and no Movement-demand ratio.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerMobileReport {
    pub mobile: MobileId,
    pub nominal_budget: Fixed,
    pub granted_budget: Fixed,
    pub ratio: Option<PowerRatio>,
}

/// Read-only recomputation of persistent and current-state-derived Power/Sense facts.
///
/// Movement grants, hostile geometry, and Phase-8 heat are intentionally Tick-local and therefore
/// absent. Rows use the same stable ordering as `PowerStepReport`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PowerSenseAnalyzerSnapshot {
    pub next_tick: Tick,
    pub regions: Vec<PowerRegionReport>,
    pub loads: Vec<PowerLoadReport>,
    pub senses: Vec<PowerSenseReport>,
    pub gates: Vec<PowerGateReport>,
}

/// Complete derived Power output for one Tick.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PowerStepReport {
    pub regions: Vec<PowerRegionReport>,
    pub loads: Vec<PowerLoadReport>,
    pub sense: Vec<PowerSenseReport>,
    pub gates: Vec<PowerGateReport>,
    pub mobiles: Vec<PowerMobileReport>,
    pub heat_contributions: Vec<PowerHeatReport>,
}

impl PowerStepReport {
    pub fn region(&self, id: PowerRegionId) -> Option<&PowerRegionReport> {
        self.regions
            .binary_search_by_key(&id, |region| region.region)
            .ok()
            .map(|index| &self.regions[index])
    }

    pub fn load(&self, id: DemandId) -> Option<&PowerLoadReport> {
        self.loads
            .binary_search_by_key(&id, |load| load.demand)
            .ok()
            .map(|index| &self.loads[index])
    }

    pub fn ratio_for(&self, id: DemandId) -> Option<PowerRatio> {
        self.load(id).map(|load| load.ratio)
    }

    pub fn sense(&self, wire: WireId, end: WireEnd) -> Option<&PowerSenseReport> {
        self.sense
            .binary_search_by_key(&(wire, end), |sense| (sense.wire, sense.end))
            .ok()
            .map(|index| &self.sense[index])
    }

    pub fn gate(&self, id: GateId) -> Option<&PowerGateReport> {
        self.gates
            .binary_search_by_key(&id, |gate| gate.gate)
            .ok()
            .map(|index| &self.gates[index])
    }

    pub fn mobile(&self, id: MobileId) -> Option<&PowerMobileReport> {
        self.mobiles
            .binary_search_by_key(&id, |mobile| mobile.mobile)
            .ok()
            .map(|index| &self.mobiles[index])
    }
}

/// Builds GateIdle, conditional GateDrive, and new/replacement GateSwitch loads for one Gate.
pub fn build_gate_nominal_demands(
    probe: PowerProbeProfile,
    input: GatePowerDemandInput,
) -> Result<Vec<NominalPowerDemand>, PowerRuntimeError> {
    validate_gate_probe(probe)?;
    let node = PowerNodeKey::GatePower(input.gate);
    let mut demands = vec![NominalPowerDemand::new(
        input.gate.entity_id(),
        DemandKind::GateIdle,
        Energy(probe.gate_idle_demand),
        node,
    )];
    if input.output_has_reachable_load {
        demands.push(NominalPowerDemand::new(
            input.gate.entity_id(),
            DemandKind::GateDrive,
            Energy(probe.gate_drive_demand),
            node,
        ));
    }
    if let Some(switch_energy) = input.switch_energy {
        if switch_energy.0 == 0 {
            return Err(PowerRuntimeError::ZeroOrdinaryGateSwitchEnergy { gate: input.gate });
        }
        let nominal = ceil_scaled_energy(
            probe.gate_switch_demand_per_energy,
            switch_energy,
            "powerProbe.gateSwitchDemandPerEnergy",
        )?;
        demands.push(NominalPowerDemand::new(
            input.gate.entity_id(),
            DemandKind::GateSwitch,
            nominal,
            node,
        ));
    }
    Ok(demands)
}

/// Builds the intrinsic WireLeakage and WireSensing loads at `WireBody`.
pub fn build_wire_nominal_demands(
    probe: PowerProbeProfile,
    input: WirePowerDemandInput,
) -> Result<Vec<NominalPowerDemand>, PowerRuntimeError> {
    validate_positive_rational(probe.wire_leakage_per_wu, "powerProbe.wireLeakagePerWU")?;
    validate_positive_rational(
        probe.wire_sense_demand_per_wu,
        "powerProbe.wireSenseDemandPerWU",
    )?;
    if input.length.0 <= 0 {
        return Err(PowerRuntimeError::NonPositiveFixedInput {
            field: "wire.length",
            raw: input.length.0,
        });
    }
    let node = PowerNodeKey::WireBody(input.wire);
    Ok(vec![
        NominalPowerDemand::new(
            input.wire.entity_id(),
            DemandKind::WireLeakage,
            ceil_scaled_world_distance(
                probe.wire_leakage_per_wu,
                input.length,
                "powerProbe.wireLeakagePerWU",
            )?,
            node,
        ),
        NominalPowerDemand::new(
            input.wire.entity_id(),
            DemandKind::WireSensing,
            ceil_scaled_world_distance(
                probe.wire_sense_demand_per_wu,
                input.length,
                "powerProbe.wireSenseDemandPerWU",
            )?,
            node,
        ),
    ])
}

/// Builds a Movement load for an enabled intent, or no load for STOP/disabled movement.
pub fn build_movement_nominal_demand(
    probe: PowerProbeProfile,
    input: MovementPowerDemandInput,
) -> Result<Option<NominalPowerDemand>, PowerRuntimeError> {
    validate_positive_rational(
        probe.movement_demand_per_wu,
        "powerProbe.movementDemandPerWU",
    )?;
    if input.offset.0 < 0 {
        return Err(PowerRuntimeError::NegativeFixedInput {
            field: "movement.offset",
            raw: input.offset.0,
        });
    }
    if input.base_distance.0 <= 0 {
        return Err(PowerRuntimeError::NonPositiveFixedInput {
            field: "movement.baseDistance",
            raw: input.base_distance.0,
        });
    }
    if !input.movement_enabled {
        return Ok(None);
    }
    Ok(Some(NominalPowerDemand::new(
        input.mobile.entity_id(),
        DemandKind::Movement,
        ceil_scaled_world_distance(
            probe.movement_demand_per_wu,
            input.base_distance,
            "powerProbe.movementDemandPerWU",
        )?,
        PowerNodeKey::WireOffset(input.wire, input.offset),
    )))
}

/// Collects every M2 nominal category before any region is solved.
///
/// Input order is deliberately irrelevant; output order is exactly `DemandId` order.
pub fn collect_nominal_power_demands(
    probe: PowerProbeProfile,
    gates: &[GatePowerDemandInput],
    wires: &[WirePowerDemandInput],
    movements: &[MovementPowerDemandInput],
) -> Result<NominalPowerDemandSet, PowerRuntimeError> {
    collect_nominal_power_demands_with_capacity_support(probe, gates, wires, movements, &[])
}

/// Collects every nominal category, including positive per-Wire capacity-support shares, before
/// any region is solved. Zero shares remain analyzer/accounting observations and are not loads.
pub fn collect_nominal_power_demands_with_capacity_support(
    probe: PowerProbeProfile,
    gates: &[GatePowerDemandInput],
    wires: &[WirePowerDemandInput],
    movements: &[MovementPowerDemandInput],
    capacity_support: &[WireCapacitySupportShare],
) -> Result<NominalPowerDemandSet, PowerRuntimeError> {
    validate_power_probe(probe)?;
    if !capacity_support.is_empty() {
        if capacity_support.len() != wires.len() {
            return Err(PowerRuntimeError::CapacitySupportWireSetMismatch);
        }
        for share in capacity_support {
            let matching_wire = wires
                .iter()
                .find(|wire| wire.wire == share.wire())
                .ok_or(PowerRuntimeError::CapacitySupportWireMismatch { wire: share.wire() })?;
            let input_length = u64::try_from(matching_wire.length.0).map_err(|_| {
                PowerRuntimeError::CapacitySupportWireMismatch { wire: share.wire() }
            })?;
            if input_length != share.length().0 {
                return Err(PowerRuntimeError::CapacitySupportWireMismatch { wire: share.wire() });
            }
        }
    }
    let mut demands = Vec::new();
    for gate in gates {
        demands.extend(build_gate_nominal_demands(probe, *gate)?);
    }
    for wire in wires {
        demands.extend(build_wire_nominal_demands(probe, *wire)?);
    }
    for movement in movements {
        if let Some(demand) = build_movement_nominal_demand(probe, *movement)? {
            demands.push(demand);
        }
    }
    for share in capacity_support {
        if share.demand().0 > 0 {
            demands.push(NominalPowerDemand::new(
                share.wire().entity_id(),
                DemandKind::OvercapacitySupport,
                share.demand(),
                PowerNodeKey::WireBody(share.wire()),
            ));
        }
    }
    NominalPowerDemandSet::from_unsorted(demands)
}

/// Binds the complete nominal set to the routes compiled from its own load attachments.
///
/// The topology must contain exactly the collected load IDs: neither a missing load nor an extra
/// stale load is accepted.
pub fn bind_nominal_power_demands(
    topology: &CompiledPowerTopology,
    nominal: &NominalPowerDemandSet,
) -> Result<Vec<PowerDemand>, PowerRuntimeError> {
    for compiled in topology.loads() {
        if nominal.get(compiled.demand()).is_none() {
            return Err(PowerRuntimeError::UnexpectedCompiledLoad {
                demand: compiled.demand(),
            });
        }
    }

    let mut bound = Vec::with_capacity(nominal.len());
    for demand in nominal.iter() {
        let compiled = topology
            .load(demand.id)
            .ok_or(PowerRuntimeError::MissingCompiledLoad { demand: demand.id })?;
        bound.push(PowerDemand::new(
            demand.owner(),
            demand.kind(),
            compiled.region(),
            demand.nominal,
            compiled.source_route().cloned(),
        ));
    }
    bound.sort_unstable_by_key(PowerDemand::id);
    Ok(bound)
}

/// Converts the Balance coefficient to the validated kernel representation.
pub fn power_loss_coefficient(
    probe: PowerProbeProfile,
) -> Result<PowerLossCoefficient, PowerRuntimeError> {
    let (numerator, denominator) =
        validate_nonnegative_rational(probe.power_loss_k, "powerProbe.powerLossK")?;
    Ok(PowerLossCoefficient::new(numerator, denominator)?)
}

/// Solves every compiled region from the already-complete nominal set and creates stable reports.
///
/// This function is read-only over topology, sources, and canonical inputs. Leakage and
/// transmission heat are positive derived observations only; no thermal or other canonical state is
/// mutated here.
pub fn solve_power_step(
    topology: &CompiledPowerTopology,
    sources: &PowerSourceStore,
    nominal: &NominalPowerDemandSet,
    probe: PowerProbeProfile,
) -> Result<PowerStepReport, PowerRuntimeError> {
    solve_power_step_with_capacity_support_heat(topology, sources, nominal, probe, None)
}

/// Solves a complete nominal set and, when the v4 Capacity probe is supplied, converts actually
/// granted support Energy into per-Wire report-only Heat using nearest-ties-even rounding.
pub fn solve_power_step_with_capacity_support_heat(
    topology: &CompiledPowerTopology,
    sources: &PowerSourceStore,
    nominal: &NominalPowerDemandSet,
    probe: PowerProbeProfile,
    capacity_probe: Option<CapacityProbeProfile>,
) -> Result<PowerStepReport, PowerRuntimeError> {
    validate_power_probe(probe)?;
    let contains_capacity_support = nominal
        .iter()
        .any(|demand| demand.kind() == DemandKind::OvercapacitySupport);
    if contains_capacity_support && capacity_probe.is_none() {
        return Err(PowerRuntimeError::MissingCapacityProbeForSupport);
    }
    if let Some(capacity_probe) = capacity_probe {
        validate_positive_unit_rational(
            capacity_probe.support_heat_fraction,
            "capacityProbe.supportHeatFraction",
        )?;
    }
    validate_source_topology(topology, sources)?;
    let coefficient = power_loss_coefficient(probe)?;
    let bound = bind_nominal_power_demands(topology, nominal)?;

    let mut report = PowerStepReport::default();
    for region in topology.regions() {
        let region_sources = region
            .sources()
            .iter()
            .map(|source| {
                sources
                    .get(*source)
                    .copied()
                    .ok_or(PowerRuntimeError::MissingPowerSource {
                        power_source_id: *source,
                    })
            })
            .collect::<Result<Vec<PowerSourceState>, PowerRuntimeError>>()?;
        let region_demands = region
            .loads()
            .iter()
            .map(|id| {
                bound
                    .binary_search_by_key(id, PowerDemand::id)
                    .ok()
                    .map(|index| bound[index].clone())
                    .ok_or(PowerRuntimeError::MissingBoundDemand { demand: *id })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let solution =
            solve_power_region(region.id(), &region_sources, &region_demands, coefficient)?;

        let total_nominal_demand = region_demands.iter().try_fold(Energy(0), |total, demand| {
            total
                .checked_add(demand.nominal())
                .map_err(|_| PowerRuntimeError::NumericOverflow)
        })?;
        report.regions.push(PowerRegionReport {
            region: region.id(),
            first_node: region.first_node(),
            sources: region.sources().to_vec(),
            generation: solution.generation(),
            total_nominal_demand,
            ratio: solution.ratio(),
        });

        for grant in solution.grants() {
            let demand =
                nominal
                    .get(grant.demand_id())
                    .ok_or(PowerRuntimeError::MissingBoundDemand {
                        demand: grant.demand_id(),
                    })?;
            let compiled =
                topology
                    .load(grant.demand_id())
                    .ok_or(PowerRuntimeError::MissingCompiledLoad {
                        demand: grant.demand_id(),
                    })?;
            report.loads.push(PowerLoadReport {
                demand: grant.demand_id(),
                region: region.id(),
                nominal: demand.nominal,
                granted: grant.granted(),
                ratio: grant.ratio(),
                source_route: compiled.source_route().cloned(),
                transmission_loss: grant.transmission_loss(),
                source_cost: grant.source_cost(),
            });

            if demand.kind() == DemandKind::WireLeakage && grant.granted().0 > 0 {
                let wire = leakage_owner(demand)?;
                report.heat_contributions.push(PowerHeatReport {
                    owner: wire,
                    kind: PowerHeatKind::LeakageDissipation,
                    demand: demand.id,
                    energy: HeatEnergy(grant.granted().0),
                });
            }
            if demand.kind() == DemandKind::OvercapacitySupport && grant.granted().0 > 0 {
                let wire = capacity_support_owner(demand)?;
                let heat = round_scaled_energy_nearest_even(
                    capacity_probe
                        .ok_or(PowerRuntimeError::MissingCapacityProbeForSupport)?
                        .support_heat_fraction,
                    grant.granted(),
                    "capacityProbe.supportHeatFraction",
                )?;
                if heat.0 > 0 {
                    report.heat_contributions.push(PowerHeatReport {
                        owner: wire,
                        kind: PowerHeatKind::OvercapacitySupport,
                        demand: demand.id,
                        energy: heat,
                    });
                }
            }
        }
        report.heat_contributions.extend(
            solution
                .transmission_heat()
                .iter()
                .filter(|heat| heat.heat_energy().0 > 0)
                .map(|heat| PowerHeatReport {
                    owner: heat.wire(),
                    kind: PowerHeatKind::TransmissionLoss,
                    demand: heat.demand_id(),
                    energy: heat.heat_energy(),
                }),
        );
    }

    report.regions.sort_unstable_by_key(|region| region.region);
    report.loads.sort_unstable_by_key(|load| load.demand);
    report
        .heat_contributions
        .sort_unstable_by_key(|heat| (heat.owner, heat.kind, heat.demand));
    Ok(report)
}

fn capacity_support_owner(demand: &NominalPowerDemand) -> Result<WireId, PowerRuntimeError> {
    match demand.node {
        PowerNodeKey::WireBody(wire) if wire.entity_id() == demand.owner() => Ok(wire),
        _ => Err(PowerRuntimeError::InvalidCapacitySupportAttachment { demand: demand.id }),
    }
}

fn leakage_owner(demand: &NominalPowerDemand) -> Result<WireId, PowerRuntimeError> {
    match demand.node {
        PowerNodeKey::WireBody(wire) if wire.entity_id() == demand.owner() => Ok(wire),
        _ => Err(PowerRuntimeError::InvalidLeakageAttachment { demand: demand.id }),
    }
}

fn validate_source_topology(
    topology: &CompiledPowerTopology,
    sources: &PowerSourceStore,
) -> Result<(), PowerRuntimeError> {
    for source in sources.iter() {
        if topology.region_for_source(source.id()).is_none() {
            return Err(PowerRuntimeError::MissingSourceAttachment {
                power_source_id: source.id(),
            });
        }
    }
    let mut topology_sources = BTreeSet::new();
    for region in topology.regions() {
        for source in region.sources() {
            if !topology_sources.insert(*source) {
                return Err(PowerRuntimeError::DuplicateTopologySource {
                    power_source_id: *source,
                });
            }
            if sources.get(*source).is_none() {
                return Err(PowerRuntimeError::MissingPowerSource {
                    power_source_id: *source,
                });
            }
            if topology.region_for_source(*source) != Some(region.id()) {
                return Err(PowerRuntimeError::SourceRegionMismatch {
                    power_source_id: *source,
                });
            }
        }
    }
    Ok(())
}

fn validate_power_probe(probe: PowerProbeProfile) -> Result<(), PowerRuntimeError> {
    validate_gate_probe(probe)?;
    validate_positive_rational(probe.wire_leakage_per_wu, "powerProbe.wireLeakagePerWU")?;
    validate_positive_rational(
        probe.wire_sense_demand_per_wu,
        "powerProbe.wireSenseDemandPerWU",
    )?;
    validate_positive_rational(
        probe.movement_demand_per_wu,
        "powerProbe.movementDemandPerWU",
    )?;
    validate_nonnegative_rational(probe.power_loss_k, "powerProbe.powerLossK")?;
    if probe.sense_nominal_drive == 0 {
        return Err(PowerRuntimeError::InvalidPowerProbe {
            field: "powerProbe.senseNominalDrive",
        });
    }
    if probe.gate_state_retention_ticks == 0 {
        return Err(PowerRuntimeError::InvalidPowerProbe {
            field: "powerProbe.gateStateRetentionTicks",
        });
    }
    Ok(())
}

fn validate_gate_probe(probe: PowerProbeProfile) -> Result<(), PowerRuntimeError> {
    if probe.gate_idle_demand == 0 {
        return Err(PowerRuntimeError::InvalidPowerProbe {
            field: "powerProbe.gateIdleDemand",
        });
    }
    if probe.gate_drive_demand == 0 {
        return Err(PowerRuntimeError::InvalidPowerProbe {
            field: "powerProbe.gateDriveDemand",
        });
    }
    validate_positive_rational(
        probe.gate_switch_demand_per_energy,
        "powerProbe.gateSwitchDemandPerEnergy",
    )?;
    Ok(())
}

fn validate_positive_rational(
    value: Rational,
    field: &'static str,
) -> Result<(u64, u64), PowerRuntimeError> {
    let (numerator, denominator) = rational_parts(value, field)?;
    if numerator == 0 {
        return Err(PowerRuntimeError::InvalidPowerProbe { field });
    }
    Ok((numerator, denominator))
}

fn validate_nonnegative_rational(
    value: Rational,
    field: &'static str,
) -> Result<(u64, u64), PowerRuntimeError> {
    rational_parts(value, field)
}

fn validate_positive_unit_rational(
    value: Rational,
    field: &'static str,
) -> Result<(u64, u64), PowerRuntimeError> {
    let (numerator, denominator) = validate_positive_rational(value, field)?;
    if numerator > denominator {
        return Err(PowerRuntimeError::InvalidPowerProbe { field });
    }
    Ok((numerator, denominator))
}

fn rational_parts(value: Rational, field: &'static str) -> Result<(u64, u64), PowerRuntimeError> {
    if value.numerator() < 0 || value.denominator() <= 0 {
        return Err(PowerRuntimeError::InvalidPowerProbe { field });
    }
    Ok((
        u64::try_from(value.numerator())
            .map_err(|_| PowerRuntimeError::InvalidPowerProbe { field })?,
        u64::try_from(value.denominator())
            .map_err(|_| PowerRuntimeError::InvalidPowerProbe { field })?,
    ))
}

fn ceil_scaled_energy(
    coefficient: Rational,
    value: Energy,
    field: &'static str,
) -> Result<Energy, PowerRuntimeError> {
    let (numerator, denominator) = validate_positive_rational(coefficient, field)?;
    ceil_product(numerator, value.0, denominator).map(Energy)
}

fn ceil_scaled_world_distance(
    coefficient: Rational,
    distance: Fixed,
    field: &'static str,
) -> Result<Energy, PowerRuntimeError> {
    let (numerator, denominator) = validate_positive_rational(coefficient, field)?;
    let distance_raw =
        u64::try_from(distance.0).map_err(|_| PowerRuntimeError::NonPositiveFixedInput {
            field: "worldDistance",
            raw: distance.0,
        })?;
    if distance_raw == 0 {
        return Err(PowerRuntimeError::NonPositiveFixedInput {
            field: "worldDistance",
            raw: distance.0,
        });
    }
    let scaled_denominator = denominator
        .checked_mul(FIXED_ONE as u64)
        .ok_or(PowerRuntimeError::NumericOverflow)?;
    ceil_product(numerator, distance_raw, scaled_denominator).map(Energy)
}

fn round_scaled_energy_nearest_even(
    coefficient: Rational,
    value: Energy,
    field: &'static str,
) -> Result<HeatEnergy, PowerRuntimeError> {
    let (numerator, denominator) = validate_nonnegative_rational(coefficient, field)?;
    let product = u128::from(numerator)
        .checked_mul(u128::from(value.0))
        .ok_or(PowerRuntimeError::NumericOverflow)?;
    let denominator = u128::from(denominator);
    let quotient = product / denominator;
    let remainder = product % denominator;
    let twice_remainder = remainder
        .checked_mul(2)
        .ok_or(PowerRuntimeError::NumericOverflow)?;
    let rounds_up = twice_remainder > denominator
        || (twice_remainder == denominator && !quotient.is_multiple_of(2));
    let rounded = if rounds_up {
        quotient
            .checked_add(1)
            .ok_or(PowerRuntimeError::NumericOverflow)?
    } else {
        quotient
    };
    u64::try_from(rounded)
        .map(HeatEnergy)
        .map_err(|_| PowerRuntimeError::NumericOverflow)
}

fn ceil_product(left: u64, right: u64, denominator: u64) -> Result<u64, PowerRuntimeError> {
    if denominator == 0 {
        return Err(PowerRuntimeError::NumericOverflow);
    }
    let product = u128::from(left)
        .checked_mul(u128::from(right))
        .ok_or(PowerRuntimeError::NumericOverflow)?;
    let quotient = product / u128::from(denominator);
    let rounded = if product.is_multiple_of(u128::from(denominator)) {
        quotient
    } else {
        quotient
            .checked_add(1)
            .ok_or(PowerRuntimeError::NumericOverflow)?
    };
    u64::try_from(rounded).map_err(|_| PowerRuntimeError::NumericOverflow)
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum PowerRuntimeError {
    #[error("invalid S1-M2 Power probe field `{field}`")]
    InvalidPowerProbe { field: &'static str },

    #[error("ordinary Gate {gate:?} switching Energy must be positive")]
    ZeroOrdinaryGateSwitchEnergy { gate: GateId },

    #[error("Power runtime field `{field}` must be positive, got raw {raw}")]
    NonPositiveFixedInput { field: &'static str, raw: i64 },

    #[error("Power runtime field `{field}` must be nonnegative, got raw {raw}")]
    NegativeFixedInput { field: &'static str, raw: i64 },

    #[error("duplicate derived Power DemandId {demand:?}")]
    DuplicateDemand { demand: DemandId },

    #[error("capacity-support rows do not cover exactly the collected Wire set")]
    CapacitySupportWireSetMismatch,

    #[error("capacity-support row for {wire:?} does not match the collected Wire length")]
    CapacitySupportWireMismatch { wire: WireId },

    #[error("compiled Power topology is missing load {demand:?}")]
    MissingCompiledLoad { demand: DemandId },

    #[error("compiled Power topology contains stale load {demand:?}")]
    UnexpectedCompiledLoad { demand: DemandId },

    #[error("compiled Power region is missing bound load {demand:?}")]
    MissingBoundDemand { demand: DemandId },

    #[error("Power Source {power_source_id:?} is missing from the canonical Source store")]
    MissingPowerSource { power_source_id: PowerSourceId },

    #[error("Power Source {power_source_id:?} is missing its compiled topology attachment")]
    MissingSourceAttachment { power_source_id: PowerSourceId },

    #[error("Power Source {power_source_id:?} appears in more than one compiled region")]
    DuplicateTopologySource { power_source_id: PowerSourceId },

    #[error(
        "Power Source {power_source_id:?} region lookup disagrees with compiled region membership"
    )]
    SourceRegionMismatch { power_source_id: PowerSourceId },

    #[error("WireLeakage demand {demand:?} is not attached to its owner WireBody")]
    InvalidLeakageAttachment { demand: DemandId },

    #[error("OvercapacitySupport demand {demand:?} is not attached to its owner WireBody")]
    InvalidCapacitySupportAttachment { demand: DemandId },

    #[error("OvercapacitySupport demand requires the Balance Capacity probe")]
    MissingCapacityProbeForSupport,

    #[error("Power runtime numeric overflow")]
    NumericOverflow,

    #[error(transparent)]
    Power(#[from] PowerError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::power::PowerRatio;
    use crate::power_topology::{
        CompiledPowerTopology, PowerBodyEdge, PowerSourceAttachment, PowerTopologyInput,
    };
    use crate::profile::BalanceProfile;
    use crate::{Capacity, FixedVec2, WireEnd, distribute_capacity_support_demand};

    fn probe() -> PowerProbeProfile {
        BalanceProfile::power_probe_alpha("runtime-test")
            .power_probe
            .expect("reference v3 has a Power probe")
    }

    fn gate(id: u64) -> GateId {
        GateId(EntityId(id))
    }

    fn wire(id: u64) -> WireId {
        WireId(EntityId(id))
    }

    fn mobile(id: u64) -> MobileId {
        MobileId(EntityId(id))
    }

    fn source(id: u64, generation: u64) -> PowerSourceState {
        PowerSourceState::new(
            PowerSourceId(EntityId(id)),
            FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
            Energy(generation),
        )
    }

    #[test]
    fn capacity_support_heat_boundary_is_positive_unit_interval_and_legacy_none_is_identical() {
        let topology = CompiledPowerTopology::compile(&PowerTopologyInput::default())
            .expect("empty topology compiles");
        let sources = PowerSourceStore::default();
        let nominal = NominalPowerDemandSet::default();
        let legacy = solve_power_step(&topology, &sources, &nominal, probe())
            .expect("legacy empty solve succeeds");
        assert_eq!(
            solve_power_step_with_capacity_support_heat(
                &topology,
                &sources,
                &nominal,
                probe(),
                None,
            ),
            Ok(legacy)
        );

        let mut capacity = BalanceProfile::capacity_support_probe_alpha("runtime-v4")
            .capacity_probe
            .expect("v4 has Capacity probe");
        for fraction in [Rational::new(0, 1).unwrap(), Rational::new(5, 4).unwrap()] {
            capacity.support_heat_fraction = fraction;
            assert_eq!(
                solve_power_step_with_capacity_support_heat(
                    &topology,
                    &sources,
                    &nominal,
                    probe(),
                    Some(capacity),
                ),
                Err(PowerRuntimeError::InvalidPowerProbe {
                    field: "capacityProbe.supportHeatFraction"
                })
            );
        }

        assert_eq!(
            round_scaled_energy_nearest_even(Rational::new(1, 4).unwrap(), Energy(2), "test",),
            Ok(HeatEnergy(0)),
            "one half rounds to the even zero"
        );
        assert_eq!(
            round_scaled_energy_nearest_even(Rational::new(1, 4).unwrap(), Energy(6), "test",),
            Ok(HeatEnergy(2)),
            "one and one half rounds to the even two"
        );

        let support_wire = wire(10);
        let support = distribute_capacity_support_demand(
            Capacity(FIXED_ONE as u64),
            Energy(1),
            &[(support_wire, Capacity(FIXED_ONE as u64))],
        )
        .expect("one-Wire support distribution succeeds");
        let matching_wire = [WirePowerDemandInput {
            wire: support_wire,
            length: Fixed(FIXED_ONE),
        }];
        let extra_wire = [
            matching_wire[0],
            WirePowerDemandInput {
                wire: wire(11),
                length: Fixed(FIXED_ONE),
            },
        ];
        assert_eq!(
            collect_nominal_power_demands_with_capacity_support(
                probe(),
                &[],
                &extra_wire,
                &[],
                &support,
            ),
            Err(PowerRuntimeError::CapacitySupportWireSetMismatch)
        );

        let wrong_length = [WirePowerDemandInput {
            wire: support_wire,
            length: Fixed(FIXED_ONE + 1),
        }];
        assert_eq!(
            collect_nominal_power_demands_with_capacity_support(
                probe(),
                &[],
                &wrong_length,
                &[],
                &support,
            ),
            Err(PowerRuntimeError::CapacitySupportWireMismatch { wire: support_wire })
        );

        let support_nominal = collect_nominal_power_demands_with_capacity_support(
            probe(),
            &[],
            &matching_wire,
            &[],
            &support,
        )
        .expect("matching support rows collect");
        assert_eq!(
            solve_power_step_with_capacity_support_heat(
                &topology,
                &sources,
                &support_nominal,
                probe(),
                None,
            ),
            Err(PowerRuntimeError::MissingCapacityProbeForSupport)
        );

        let malformed = NominalPowerDemand::new(
            support_wire.entity_id(),
            DemandKind::OvercapacitySupport,
            Energy(1),
            PowerNodeKey::WireBody(wire(11)),
        );
        assert_eq!(
            capacity_support_owner(&malformed),
            Err(PowerRuntimeError::InvalidCapacitySupportAttachment {
                demand: DemandId::new(support_wire.entity_id(), DemandKind::OvercapacitySupport,),
            })
        );
    }

    #[test]
    fn nominal_collection_is_exact_ceiling_sorted_and_complete_before_solve() {
        let gates = [GatePowerDemandInput {
            gate: gate(20),
            output_has_reachable_load: true,
            switch_energy: Some(Energy(2)),
        }];
        let wires = [WirePowerDemandInput {
            wire: wire(10),
            length: Fixed(FIXED_ONE + 1),
        }];
        let movements = [
            MovementPowerDemandInput {
                mobile: mobile(30),
                wire: wire(10),
                offset: Fixed(FIXED_ONE / 2),
                base_distance: Fixed(FIXED_ONE / 2),
                movement_enabled: true,
            },
            MovementPowerDemandInput {
                mobile: mobile(31),
                wire: wire(10),
                offset: Fixed::ZERO,
                base_distance: Fixed::ONE,
                movement_enabled: false,
            },
        ];

        let collected = collect_nominal_power_demands(probe(), &gates, &wires, &movements)
            .expect("valid nominal inputs collect");
        assert_eq!(collected.len(), 6);
        assert_eq!(
            collected
                .iter()
                .map(NominalPowerDemand::id)
                .collect::<Vec<_>>(),
            vec![
                DemandId::new(EntityId(10), DemandKind::WireLeakage),
                DemandId::new(EntityId(10), DemandKind::WireSensing),
                DemandId::new(EntityId(20), DemandKind::GateIdle),
                DemandId::new(EntityId(20), DemandKind::GateSwitch),
                DemandId::new(EntityId(20), DemandKind::GateDrive),
                DemandId::new(EntityId(30), DemandKind::Movement),
            ]
        );
        assert_eq!(
            collected
                .get(DemandId::new(EntityId(10), DemandKind::WireLeakage))
                .expect("leakage exists")
                .nominal(),
            Energy(2)
        );
        assert_eq!(
            collected
                .get(DemandId::new(EntityId(30), DemandKind::Movement))
                .expect("movement exists")
                .nominal(),
            Energy(1)
        );
        assert_eq!(
            collected
                .get(DemandId::new(EntityId(20), DemandKind::GateSwitch))
                .expect("switch exists")
                .nominal(),
            Energy(2)
        );
    }

    #[test]
    fn duplicate_owner_kind_and_zero_ordinary_switch_fail_closed() {
        let duplicated = [
            GatePowerDemandInput {
                gate: gate(20),
                output_has_reachable_load: false,
                switch_energy: None,
            },
            GatePowerDemandInput {
                gate: gate(20),
                output_has_reachable_load: false,
                switch_energy: None,
            },
        ];
        assert!(matches!(
            collect_nominal_power_demands(probe(), &duplicated, &[], &[]),
            Err(PowerRuntimeError::DuplicateDemand { .. })
        ));
        assert_eq!(
            build_gate_nominal_demands(
                probe(),
                GatePowerDemandInput {
                    gate: gate(20),
                    output_has_reachable_load: false,
                    switch_energy: Some(Energy(0)),
                }
            ),
            Err(PowerRuntimeError::ZeroOrdinaryGateSwitchEnergy { gate: gate(20) })
        );
    }

    #[test]
    fn region_solve_reports_common_ratio_routes_and_only_real_heat() {
        let source = source(1, 100);
        let sources = PowerSourceStore::new(vec![source]).expect("Source store is valid");
        let collected = collect_nominal_power_demands(
            probe(),
            &[GatePowerDemandInput {
                gate: gate(20),
                output_has_reachable_load: true,
                switch_energy: Some(Energy(1)),
            }],
            &[WirePowerDemandInput {
                wire: wire(10),
                length: Fixed::ONE,
            }],
            &[MovementPowerDemandInput {
                mobile: mobile(30),
                wire: wire(10),
                offset: Fixed(FIXED_ONE / 4),
                base_distance: Fixed::ONE,
                movement_enabled: true,
            }],
        )
        .expect("nominal demands collect");
        let topology = CompiledPowerTopology::compile(&PowerTopologyInput {
            bodies: vec![PowerBodyEdge {
                wire: wire(10),
                a: PowerNodeKey::SourceAnchor(source.id()),
                b: PowerNodeKey::GatePower(gate(20)),
                length: Fixed::ONE,
                segment_lengths: vec![Fixed::ONE],
                canonical_lower_end: WireEnd::A,
            }],
            sources: vec![PowerSourceAttachment {
                source: source.id(),
                node: PowerNodeKey::SourceAnchor(source.id()),
            }],
            loads: collected.load_attachments().collect(),
        })
        .expect("Power topology compiles");

        let report = solve_power_step(&topology, &sources, &collected, probe())
            .expect("Power runtime solves");
        assert_eq!(report.regions.len(), 1);
        assert_eq!(report.regions[0].ratio, PowerRatio::ONE);
        assert_eq!(report.regions[0].total_nominal_demand, Energy(6));
        assert_eq!(report.loads.len(), 6);
        assert!(
            report
                .loads
                .iter()
                .all(|load| load.ratio == PowerRatio::ONE && load.source_route.is_some())
        );
        assert_eq!(
            report.heat_contributions,
            vec![PowerHeatReport {
                owner: wire(10),
                kind: PowerHeatKind::LeakageDissipation,
                demand: DemandId::new(EntityId(10), DemandKind::WireLeakage),
                energy: HeatEnergy(1),
            }]
        );

        let empty_sources = PowerSourceStore::default();
        let source_less = CompiledPowerTopology::compile(&PowerTopologyInput {
            bodies: vec![PowerBodyEdge {
                wire: wire(10),
                a: PowerNodeKey::WireEnd(wire(10), WireEnd::A),
                b: PowerNodeKey::GatePower(gate(20)),
                length: Fixed::ONE,
                segment_lengths: vec![Fixed::ONE],
                canonical_lower_end: WireEnd::A,
            }],
            sources: vec![],
            loads: collected.load_attachments().collect(),
        })
        .expect("source-less topology compiles");
        let report = solve_power_step(&source_less, &empty_sources, &collected, probe())
            .expect("source-less runtime solves");
        assert!(
            report
                .loads
                .iter()
                .all(|load| load.ratio == PowerRatio::ZERO)
        );
        assert!(report.heat_contributions.is_empty());
    }

    #[test]
    fn nonzero_loss_is_derived_and_stably_allocated_without_state_mutation() {
        let source = source(1, 100);
        let sources = PowerSourceStore::new(vec![source]).expect("Source store is valid");
        let collected = collect_nominal_power_demands(
            probe(),
            &[GatePowerDemandInput {
                gate: gate(20),
                output_has_reachable_load: false,
                switch_energy: None,
            }],
            &[],
            &[],
        )
        .expect("nominal demands collect");
        let topology = CompiledPowerTopology::compile(&PowerTopologyInput {
            bodies: vec![PowerBodyEdge {
                wire: wire(10),
                a: PowerNodeKey::SourceAnchor(source.id()),
                b: PowerNodeKey::GatePower(gate(20)),
                length: Fixed::ONE,
                segment_lengths: vec![Fixed::ONE],
                canonical_lower_end: WireEnd::A,
            }],
            sources: vec![PowerSourceAttachment {
                source: source.id(),
                node: PowerNodeKey::SourceAnchor(source.id()),
            }],
            loads: collected.load_attachments().collect(),
        })
        .expect("Power topology compiles");
        let mut nonzero_loss = probe();
        nonzero_loss.power_loss_k = Rational::new(1, 1).expect("unit Rational is valid");

        let first = solve_power_step(&topology, &sources, &collected, nonzero_loss)
            .expect("nonzero-loss solve succeeds");
        let second = solve_power_step(&topology, &sources, &collected, nonzero_loss)
            .expect("repeated read-only solve succeeds");
        assert_eq!(first, second);
        assert_eq!(first.loads[0].transmission_loss, Energy(1));
        assert_eq!(
            first.heat_contributions,
            vec![PowerHeatReport {
                owner: wire(10),
                kind: PowerHeatKind::TransmissionLoss,
                demand: DemandId::new(EntityId(20), DemandKind::GateIdle),
                energy: HeatEnergy(1),
            }]
        );
    }
}
