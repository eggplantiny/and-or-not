use crate::contract::{ContractValidationError, SimulationContract};
use crate::event::{
    DRIVER_TRANSITION_KIND_ORDER, DriverTransition, DriverTransitionCause, EventCalendar,
    EventCalendarError, EventPayloadAllocator, SIGNAL_ARRIVAL_KIND_ORDER, SignalArrival,
    SignalArrivalKind, SignalArrivalStagingError, UncertifiedSignalArrival, stage_signal_arrivals,
};
use crate::mobility::{
    MobileControlSample, MobileMovementObservation, TrackGraph, TrackGraphError, TrackPosition,
};
use crate::path_certificate::{PathCertificateArena, PathCertificateError};
use crate::profile::{BalanceProfile, PhysicalScaleProfile, ProfileBundle, ProfileValidationError};
use crate::replay::{
    ReplayFormatVersion, ReplayHeader, Seed, StateHashVersion, WorldGeneratorVersion,
};
use crate::signal::{
    DriverChangeRecord, DriverRole, GateSignalPorts, GateSignalSnapshot, SignalChangeRecord,
    SignalError, SignalStepCounters, SignalWorld, SinkRole, SlotApplyOutcome, WireSignalSnapshot,
};
use crate::signal_topology::{
    CompiledSignalTopology, RouteDiff, SignalTopologyError, switch_energy,
};
use crate::snapshot::{RenderSnapshotSource, SignalProbeSample, SignalProbeTarget, sample_signal};
use crate::structural::{StructuralError, StructuralPhaseReport, StructuralWorld};
use crate::{
    CommandAcceptance, CommandEnvelope, CommandRejection, DriveStrength, DriverId, DriverSample,
    Fixed, FixedVec2, GateId, GateType, InitialWorld, LogicLevel, MobileId, MobileSubstrateIndex,
    RenderSnapshot, Revision, ScenarioManifest, SimulationError, SinkId, StageFeatureSet,
    StateHash, Tick, WireId, canonical,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimulationPackage {
    scenario_id: String,
    initial_world: InitialWorld,
    required_features: StageFeatureSet,
    contract: SimulationContract,
    profiles: ProfileBundle,
}

impl SimulationPackage {
    pub fn new(
        scenario_id: impl Into<String>,
        initial_world: InitialWorld,
        required_features: StageFeatureSet,
        contract: SimulationContract,
        profiles: ProfileBundle,
    ) -> Self {
        Self {
            scenario_id: scenario_id.into(),
            initial_world,
            required_features,
            contract,
            profiles,
        }
    }

    pub(crate) fn from_artifacts(scenario: ScenarioManifest, profiles: ProfileBundle) -> Self {
        let contract = SimulationContract {
            semantics_version: scenario.semantics_version(),
            numeric_profile_hash: scenario.profiles().numeric().profile_hash(),
            physical_scale_profile_hash: scenario.profiles().physical_scale().profile_hash(),
            balance_profile_hash: scenario.profiles().balance().profile_hash(),
        };
        Self::new(
            scenario.scenario_id(),
            scenario.initial_world().clone(),
            scenario.required_features(),
            contract,
            profiles,
        )
    }

    pub fn scenario_id(&self) -> &str {
        &self.scenario_id
    }

    pub const fn contract(&self) -> &SimulationContract {
        &self.contract
    }

    pub const fn semantics_version(&self) -> crate::SemanticsVersion {
        self.contract.semantics_version
    }

    pub const fn profiles(&self) -> &ProfileBundle {
        &self.profiles
    }

    pub const fn required_features(&self) -> StageFeatureSet {
        self.required_features
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignalArrivalObservation {
    pub due_tick: Tick,
    pub source_driver: DriverId,
    pub sink: SinkId,
    pub sample: DriverSample,
    pub kind: SignalArrivalKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepReport {
    pub completed_tick: Tick,
    pub next_tick: Tick,
    pub state_hash: StateHash,
    pub command_acceptances: Vec<CommandAcceptance>,
    pub command_rejections: Vec<CommandRejection>,
    pub topology_changed: bool,
    pub driver_changes: Vec<DriverChangeRecord>,
    pub signal_changes: Vec<SignalChangeRecord>,
    pub signal_arrivals: Vec<SignalArrivalObservation>,
    pub signal_counters: SignalStepCounters,
    pub mobile_movements: Vec<MobileMovementObservation>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MobileIntent {
    index: MobileSubstrateIndex,
    mobile: MobileId,
    start: TrackPosition,
    start_world_point: FixedVec2,
    controls: MobileControlSample,
    granted_budget: Fixed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MobilePhase1Snapshot {
    index: MobileSubstrateIndex,
    mobile: MobileId,
    start: TrackPosition,
    world_point: FixedVec2,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Phase1Snapshot {
    mobiles: Vec<MobilePhase1Snapshot>,
}

struct Phase0Output {
    structural_report: StructuralPhaseReport,
    topology: CompiledSignalTopology,
    track_graph: TrackGraph,
    signal_counters: SignalStepCounters,
}

struct Phase11Output {
    state_hash: StateHash,
    mobile_movements: Vec<MobileMovementObservation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StagedMobileMovement {
    index: MobileSubstrateIndex,
    observation: MobileMovementObservation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum TickPhase {
    StructuralCommit = 0,
    SnapshotAndWorldSample = 1,
    DriverAndSignalArrival = 2,
    IntentEvaluation = 3,
    GlobalAccountingAndNominalDemand = 4,
    PowerSolveAndBrownout = 5,
    SchedulingAndGrantedWork = 6,
    Trajectory = 7,
    Interaction = 8,
    ThermalIntegration = 9,
    DamageResolution = 10,
    ProgressCommit = 11,
}

impl TickPhase {
    const ORDER: [Self; 12] = [
        Self::StructuralCommit,
        Self::SnapshotAndWorldSample,
        Self::DriverAndSignalArrival,
        Self::IntentEvaluation,
        Self::GlobalAccountingAndNominalDemand,
        Self::PowerSolveAndBrownout,
        Self::SchedulingAndGrantedWork,
        Self::Trajectory,
        Self::Interaction,
        Self::ThermalIntegration,
        Self::DamageResolution,
        Self::ProgressCommit,
    ];
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TickPhaseSequence {
    next: usize,
}

impl TickPhaseSequence {
    fn enter(&mut self, phase: TickPhase) -> Result<(), SimulationError> {
        if TickPhase::ORDER.get(self.next) != Some(&phase) {
            return Err(SimulationError::InvalidCanonicalState);
        }
        self.next = self
            .next
            .checked_add(1)
            .ok_or(SimulationError::NumericOverflow)?;
        Ok(())
    }

    fn finish(self) -> Result<(), SimulationError> {
        if self.next == TickPhase::ORDER.len() {
            Ok(())
        } else {
            Err(SimulationError::InvalidCanonicalState)
        }
    }
}

#[derive(Clone)]
struct CanonicalWorld {
    next_tick: Tick,
    topology_revision: Revision,
    contract: SimulationContract,
    structural: StructuralWorld,
    signal: SignalWorld,
    event_payloads: EventPayloadAllocator,
    driver_events: EventCalendar<DriverTransition>,
    signal_events: EventCalendar<SignalArrival>,
    path_certificates: PathCertificateArena,
}

impl CanonicalWorld {
    fn state_view(&self) -> canonical::StateView<'_> {
        canonical::StateView {
            contract: &self.contract,
            next_tick: self.next_tick,
            topology_revision: self.topology_revision,
            structural: &self.structural,
            signal: &self.signal,
            event_payloads: &self.event_payloads,
            driver_events: &self.driver_events,
            signal_events: &self.signal_events,
            path_certificates: &self.path_certificates,
        }
    }
}

pub struct Simulation {
    scenario_id: String,
    canonical: CanonicalWorld,
    profiles: ProfileBundle,
    initial_state_hash: StateHash,
}

impl Simulation {
    pub fn new(package: SimulationPackage) -> Result<Self, SimulationError> {
        if let Some(feature) = package.required_features.first_enabled() {
            return Err(SimulationError::UnsupportedStageFeature { feature });
        }

        package.profiles.validate().map_err(SimulationError::from)?;
        package
            .contract
            .validate_profiles(&package.profiles)
            .map_err(SimulationError::from)?;

        let structural = match package.initial_world {
            InitialWorld::Empty => StructuralWorld::new(),
        };

        let canonical = CanonicalWorld {
            next_tick: Tick(0),
            topology_revision: Revision(0),
            contract: package.contract,
            structural,
            signal: SignalWorld::new(),
            event_payloads: EventPayloadAllocator::new(),
            driver_events: EventCalendar::new(),
            signal_events: EventCalendar::new(),
            path_certificates: PathCertificateArena::new(),
        };
        validate_canonical_world(&canonical)?;
        let initial_state_hash = canonical::state_hash(canonical.state_view());
        Ok(Self {
            scenario_id: package.scenario_id,
            canonical,
            profiles: package.profiles,
            initial_state_hash,
        })
    }

    pub fn step(&mut self, commands: &[CommandEnvelope]) -> Result<StepReport, SimulationError> {
        let mut candidate = self.canonical.clone();
        let completed_tick = candidate.next_tick;
        let next_tick = completed_tick.checked_add(Tick(1))?;
        let mut phases = TickPhaseSequence::default();

        phases.enter(TickPhase::StructuralCommit)?;
        let mut phase0 = run_phase0_structural_commit(
            &mut candidate,
            commands,
            completed_tick,
            &self.profiles.physical_scale,
            &self.profiles.balance,
        )?;

        phases.enter(TickPhase::SnapshotAndWorldSample)?;
        let phase1 = run_phase1_snapshot_and_world_sample(&candidate, &phase0.track_graph)?;

        phases.enter(TickPhase::DriverAndSignalArrival)?;
        let phase2 = run_phase2(
            &mut candidate,
            &phase0.topology,
            completed_tick,
            self.profiles.balance.logic_threshold,
            &mut phase0.signal_counters,
        )?;

        phases.enter(TickPhase::IntentEvaluation)?;
        let mut mobile_intents = run_phase3(&mut candidate, &phase1)?;

        phases.enter(TickPhase::GlobalAccountingAndNominalDemand)?;
        run_phase4_global_accounting_and_nominal_demand(&candidate, &mobile_intents);

        phases.enter(TickPhase::PowerSolveAndBrownout)?;
        run_phase5_power_solve_and_brownout();

        phases.enter(TickPhase::SchedulingAndGrantedWork)?;
        run_phase6(
            &mut candidate,
            &phase0.topology,
            completed_tick,
            &self.profiles.balance,
            &mut mobile_intents,
            self.profiles.physical_scale.world_routing_pitch,
        )?;

        phases.enter(TickPhase::Trajectory)?;
        let staged_mobiles = run_phase7(&phase0.track_graph, &mobile_intents)?;

        phases.enter(TickPhase::Interaction)?;
        run_phase8_interaction(&staged_mobiles);

        phases.enter(TickPhase::ThermalIntegration)?;
        run_phase9_thermal_integration();

        phases.enter(TickPhase::DamageResolution)?;
        run_phase10_damage_resolution();

        phases.enter(TickPhase::ProgressCommit)?;
        let phase11 = run_phase11_progress_commit(&mut candidate, next_tick, staged_mobiles)?;
        phases.finish()?;

        self.canonical = candidate;

        Ok(StepReport {
            completed_tick,
            next_tick,
            state_hash: phase11.state_hash,
            command_acceptances: phase0.structural_report.acceptances,
            command_rejections: phase0.structural_report.rejections,
            topology_changed: phase0.structural_report.topology_changed,
            driver_changes: phase2.driver_changes,
            signal_changes: phase2.signal_changes,
            signal_arrivals: phase2.signal_arrivals,
            signal_counters: phase0.signal_counters,
            mobile_movements: phase11.mobile_movements,
        })
    }

    pub fn write_render_snapshot(&self, output: &mut RenderSnapshot) {
        output.write(RenderSnapshotSource {
            scenario_id: &self.scenario_id,
            next_tick: self.canonical.next_tick,
            topology_revision: self.canonical.topology_revision,
            contract: self.canonical.contract,
            state_hash: self.state_hash(),
            structural: &self.canonical.structural,
            signal: &self.canonical.signal,
            logic_threshold: self.profiles.balance.logic_threshold,
        });
    }

    pub fn signal_probe(&self, target: SignalProbeTarget) -> Option<SignalProbeSample> {
        sample_signal(
            &self.canonical.signal,
            self.profiles.balance.logic_threshold,
            self.canonical.next_tick,
            target,
        )
    }

    pub fn state_hash(&self) -> StateHash {
        canonical::state_hash(self.canonical.state_view())
    }

    pub const fn next_tick(&self) -> Tick {
        self.canonical.next_tick
    }

    pub const fn topology_revision(&self) -> Revision {
        self.canonical.topology_revision
    }

    pub const fn contract(&self) -> &SimulationContract {
        &self.canonical.contract
    }

    pub fn replay_header(&self) -> ReplayHeader {
        ReplayHeader {
            format_version: ReplayFormatVersion::V1,
            semantics_version: self.canonical.contract.semantics_version,
            numeric_profile_hash: self.canonical.contract.numeric_profile_hash,
            physical_scale_profile_hash: self.canonical.contract.physical_scale_profile_hash,
            balance_profile_hash: self.canonical.contract.balance_profile_hash,
            state_hash_version: StateHashVersion::current(),
            world_generator_version: WorldGeneratorVersion::EmptyV1,
            seed: Seed::ZERO,
            initial_state_hash: self.initial_state_hash,
            hash_algorithm_id: self.canonical.contract.hash_algorithm_id(),
        }
    }

    pub const fn profiles(&self) -> &ProfileBundle {
        &self.profiles
    }

    pub fn scenario_id(&self) -> &str {
        &self.scenario_id
    }

    pub fn gate_signal_ports(&self, gate: GateId) -> Option<GateSignalPorts> {
        self.canonical.signal.gate_ports(gate)
    }

    pub fn driver_sample(&self, driver: DriverId) -> Option<DriverSample> {
        self.canonical.signal.driver_sample(driver)
    }

    pub fn sink_level(&self, sink: SinkId) -> Option<LogicLevel> {
        self.canonical.signal.sink_level(sink)
    }

    pub fn sink_driver_sample(&self, sink: SinkId, driver: DriverId) -> Option<DriverSample> {
        self.canonical.signal.sink_driver_sample(sink, driver)
    }

    pub fn gate_signal_state(&self, gate: GateId) -> Option<GateSignalSnapshot> {
        self.canonical.signal.gate_snapshot(gate)
    }

    pub fn wire_signal_state(&self, wire: WireId) -> Option<WireSignalSnapshot> {
        self.canonical.signal.wire_snapshot(wire)
    }
}

fn validate_canonical_world(world: &CanonicalWorld) -> Result<(), SimulationError> {
    validate_structural_registry_links(&world.structural)?;
    let signal = &world.signal;
    let driver_frontier = signal.driver_frontier().entity_id().0;
    let sink_frontier = signal.sink_frontier().entity_id().0;
    if driver_frontier == 0 || sink_frontier == 0 {
        return Err(SimulationError::InvalidCanonicalState);
    }

    let driver_slots: Vec<_> = signal.canonical_driver_slots().collect();
    let sink_slots: Vec<_> = signal.canonical_sink_slots().collect();
    if u64::try_from(driver_slots.len()).map_err(|_| SimulationError::NumericOverflow)?
        != signal.allocated_driver_count()
        || u64::try_from(sink_slots.len()).map_err(|_| SimulationError::NumericOverflow)?
            != signal.allocated_sink_count()
        || driver_frontier
            != signal
                .allocated_driver_count()
                .checked_add(1)
                .ok_or(SimulationError::NumericOverflow)?
        || sink_frontier
            != signal
                .allocated_sink_count()
                .checked_add(1)
                .ok_or(SimulationError::NumericOverflow)?
    {
        return Err(SimulationError::InvalidCanonicalState);
    }

    let mut structural_gates = BTreeMap::new();
    for (_, gate) in world.structural.gates().iter_alive() {
        if structural_gates.insert(gate.id, gate.gate_type).is_some() {
            return Err(SimulationError::InvalidCanonicalState);
        }
    }

    let signal_gates: Vec<_> = signal.iter_gate_entries().collect();
    if signal_gates.len() != structural_gates.len() {
        return Err(SimulationError::InvalidCanonicalState);
    }
    let mut referenced_drivers = BTreeSet::new();
    let mut referenced_sinks = BTreeSet::new();
    for (key, gate) in signal_gates {
        if key != gate.gate || structural_gates.get(&gate.gate) != Some(&gate.gate_type) {
            return Err(SimulationError::InvalidCanonicalState);
        }
        let expects_input_b = matches!(gate.gate_type, GateType::And | GateType::Or);
        if gate.ports.input_b.is_some() != expects_input_b {
            return Err(SimulationError::InvalidCanonicalState);
        }
        validate_gate_driver(
            signal,
            gate.gate,
            gate.ports.input_a.external_driver,
            DriverRole::ExternalInputA,
            &mut referenced_drivers,
        )?;
        validate_gate_sink(
            signal,
            gate.gate,
            gate.ports.input_a.sink,
            SinkRole::InputA,
            &mut referenced_sinks,
        )?;
        if let Some(input_b) = gate.ports.input_b {
            validate_gate_driver(
                signal,
                gate.gate,
                input_b.external_driver,
                DriverRole::ExternalInputB,
                &mut referenced_drivers,
            )?;
            validate_gate_sink(
                signal,
                gate.gate,
                input_b.sink,
                SinkRole::InputB,
                &mut referenced_sinks,
            )?;
        }
        validate_gate_driver(
            signal,
            gate.gate,
            gate.ports.output,
            DriverRole::GateOutput,
            &mut referenced_drivers,
        )?;

        let output = signal
            .driver_record(gate.ports.output)
            .ok_or(SimulationError::InvalidCanonicalState)?;
        if output.sample.level != gate.current_output {
            return Err(SimulationError::InvalidCanonicalState);
        }
        let input_a = signal
            .sink_level(gate.ports.input_a.sink)
            .ok_or(SimulationError::InvalidCanonicalState)?;
        let input_b = match gate.ports.input_b {
            Some(port) => Some(
                signal
                    .sink_level(port.sink)
                    .ok_or(SimulationError::InvalidCanonicalState)?,
            ),
            None => None,
        };
        if crate::signal::gate_output(gate.gate_type, input_a, input_b)? != gate.desired_output {
            return Err(SimulationError::InvalidCanonicalState);
        }

        match (
            gate.pending_due_tick,
            gate.pending_level,
            gate.pending_switch_energy,
        ) {
            (None, None, None) => {}
            (Some(due_tick), Some(level), Some(energy)) => {
                if gate.pending_generation == 0
                    || due_tick < world.next_tick
                    || level != gate.desired_output
                    || level == gate.current_output
                    || energy.0 == 0
                    || !world.driver_events.canonical_view().any(|event| {
                        event.cause == DriverTransitionCause::GateOutput
                            && event.driver_id == gate.ports.output
                            && event.level == level
                            && event.pending_generation == gate.pending_generation
                            && event.key.due_tick == due_tick
                    })
                {
                    return Err(SimulationError::InvalidCanonicalState);
                }
            }
            _ => return Err(SimulationError::InvalidCanonicalState),
        }
    }

    let structural_mobiles: BTreeSet<_> = world
        .structural
        .mobile_substrates()
        .iter_alive()
        .map(|(_, record)| record.id)
        .collect();
    let signal_mobiles: Vec<_> = signal.iter_mobile_entries().collect();
    if signal_mobiles.len() != structural_mobiles.len() {
        return Err(SimulationError::InvalidCanonicalState);
    }
    for (mobile, ports) in signal_mobiles {
        if !structural_mobiles.contains(&mobile) {
            return Err(SimulationError::InvalidCanonicalState);
        }
        validate_mobile_sink(
            signal,
            mobile,
            ports.stop,
            SinkRole::MobileStop,
            &mut referenced_sinks,
        )?;
        validate_mobile_sink(
            signal,
            mobile,
            ports.left,
            SinkRole::MobileLeft,
            &mut referenced_sinks,
        )?;
        validate_mobile_sink(
            signal,
            mobile,
            ports.right,
            SinkRole::MobileRight,
            &mut referenced_sinks,
        )?;
    }

    let mut live_drivers = BTreeSet::new();
    for (slot_id, record) in driver_slots {
        if slot_id.entity_id().0 == 0 || slot_id.entity_id().0 >= driver_frontier {
            return Err(SimulationError::InvalidCanonicalState);
        }
        let Some(record) = record else {
            continue;
        };
        if record.id != slot_id
            || record.sample.driver_id != slot_id
            || record.sample.emitted_at >= world.next_tick
            || !live_drivers.insert(slot_id)
        {
            return Err(SimulationError::InvalidCanonicalState);
        }
    }
    if live_drivers != referenced_drivers {
        return Err(SimulationError::InvalidCanonicalState);
    }

    let mut live_sinks = BTreeSet::new();
    for (slot_id, record) in sink_slots {
        if slot_id.entity_id().0 == 0 || slot_id.entity_id().0 >= sink_frontier {
            return Err(SimulationError::InvalidCanonicalState);
        }
        let Some(record) = record else {
            continue;
        };
        if record.id != slot_id || record.dirty || !live_sinks.insert(slot_id) {
            return Err(SimulationError::InvalidCanonicalState);
        }
    }
    if live_sinks != referenced_sinks {
        return Err(SimulationError::InvalidCanonicalState);
    }

    let structural_wires: BTreeSet<_> = world
        .structural
        .wires()
        .iter_alive()
        .map(|(_, wire)| wire.id)
        .collect();
    let signal_wires: BTreeSet<_> = signal.iter_wires().map(|(wire, _)| wire).collect();
    if structural_wires != signal_wires {
        return Err(SimulationError::InvalidCanonicalState);
    }

    let mut slot_keys = BTreeSet::new();
    for ((key_sink, key_driver), slot) in signal.iter_slot_entries() {
        let driver = signal
            .driver_record(slot.driver)
            .ok_or(SimulationError::InvalidCanonicalState)?;
        if key_sink != slot.sink
            || key_driver != slot.driver
            || slot.emitted_at >= world.next_tick
            || !live_drivers.contains(&slot.driver)
            || !live_sinks.contains(&slot.sink)
            || !slot_keys.insert((key_sink, key_driver))
        {
            return Err(SimulationError::InvalidCanonicalState);
        }
        if slot.revision > driver.sample.revision {
            return Err(SimulationError::DriverRevisionInvariantViolation);
        }
    }

    validate_event_state(world, driver_frontier, sink_frontier, &live_drivers)
}

fn validate_structural_registry_links(structural: &StructuralWorld) -> Result<(), SimulationError> {
    let registry = structural.entities();
    let frontier = registry.next_id().0;
    if frontier == 0 {
        return Err(SimulationError::InvalidCanonicalState);
    }
    let slots: Vec<_> = registry.canonical_slots().collect();
    let allocated = registry.allocated_count();
    if u64::try_from(slots.len()).map_err(|_| SimulationError::NumericOverflow)? != allocated
        || frontier
            != allocated
                .checked_add(1)
                .ok_or(SimulationError::NumericOverflow)?
    {
        return Err(SimulationError::InvalidCanonicalState);
    }

    let mut registry_gates = BTreeSet::new();
    let mut registry_wires = BTreeSet::new();
    let mut registry_junctions = BTreeSet::new();
    let mut registry_substrates = BTreeSet::new();
    let mut registry_mobiles = BTreeSet::new();
    for (id, location) in slots {
        if id.0 == 0 || id.0 >= frontier {
            return Err(SimulationError::InvalidCanonicalState);
        }
        let Some(location) = location else {
            continue;
        };
        match location {
            crate::EntityLocation::Gate(index) => {
                let record = structural
                    .gates()
                    .get(index)
                    .ok_or(SimulationError::InvalidCanonicalState)?;
                if record.id.entity_id() != id || !registry_gates.insert(id) {
                    return Err(SimulationError::InvalidCanonicalState);
                }
            }
            crate::EntityLocation::Wire(index) => {
                let record = structural
                    .wires()
                    .get(index)
                    .ok_or(SimulationError::InvalidCanonicalState)?;
                if record.id.entity_id() != id || !registry_wires.insert(id) {
                    return Err(SimulationError::InvalidCanonicalState);
                }
            }
            crate::EntityLocation::Junction(index) => {
                let record = structural
                    .junctions()
                    .get(index)
                    .ok_or(SimulationError::InvalidCanonicalState)?;
                if record.id.entity_id() != id || !registry_junctions.insert(id) {
                    return Err(SimulationError::InvalidCanonicalState);
                }
            }
            crate::EntityLocation::FixedSubstrate(index) => {
                let record = structural
                    .fixed_substrates()
                    .get(index)
                    .ok_or(SimulationError::InvalidCanonicalState)?;
                if record.id != id || !registry_substrates.insert(id) {
                    return Err(SimulationError::InvalidCanonicalState);
                }
            }
            crate::EntityLocation::MobileSubstrate(index) => {
                let record = structural
                    .mobile_substrates()
                    .get(index)
                    .ok_or(SimulationError::InvalidCanonicalState)?;
                if record.id.entity_id() != id || !registry_mobiles.insert(id) {
                    return Err(SimulationError::InvalidCanonicalState);
                }
            }
            _ => return Err(SimulationError::InvalidCanonicalState),
        }
    }

    let mut store_gates = BTreeSet::new();
    for (index, record) in structural.gates().iter_alive() {
        let id = record.id.entity_id();
        if registry.location(id) != Some(&crate::EntityLocation::Gate(index))
            || !store_gates.insert(id)
        {
            return Err(SimulationError::InvalidCanonicalState);
        }
    }
    let mut store_wires = BTreeSet::new();
    for (index, record) in structural.wires().iter_alive() {
        let id = record.id.entity_id();
        if registry.location(id) != Some(&crate::EntityLocation::Wire(index))
            || !store_wires.insert(id)
        {
            return Err(SimulationError::InvalidCanonicalState);
        }
    }
    let mut store_junctions = BTreeSet::new();
    for (index, record) in structural.junctions().iter_alive() {
        let id = record.id.entity_id();
        if registry.location(id) != Some(&crate::EntityLocation::Junction(index))
            || !store_junctions.insert(id)
        {
            return Err(SimulationError::InvalidCanonicalState);
        }
    }
    let mut store_substrates = BTreeSet::new();
    for (index, record) in structural.fixed_substrates().iter_alive() {
        if registry.location(record.id) != Some(&crate::EntityLocation::FixedSubstrate(index))
            || !store_substrates.insert(record.id)
        {
            return Err(SimulationError::InvalidCanonicalState);
        }
    }
    let mut store_mobiles = BTreeSet::new();
    for (index, record) in structural.mobile_substrates().iter_alive() {
        let id = record.id.entity_id();
        if registry.location(id) != Some(&crate::EntityLocation::MobileSubstrate(index))
            || !store_mobiles.insert(id)
        {
            return Err(SimulationError::InvalidCanonicalState);
        }
    }

    if registry_gates != store_gates
        || registry_wires != store_wires
        || registry_junctions != store_junctions
        || registry_substrates != store_substrates
        || registry_mobiles != store_mobiles
        || u64::try_from(store_gates.len()).map_err(|_| SimulationError::NumericOverflow)?
            != structural.gates().live_count()
        || u64::try_from(store_wires.len()).map_err(|_| SimulationError::NumericOverflow)?
            != structural.wires().live_count()
        || u64::try_from(store_junctions.len()).map_err(|_| SimulationError::NumericOverflow)?
            != structural.junctions().live_count()
        || u64::try_from(store_substrates.len()).map_err(|_| SimulationError::NumericOverflow)?
            != structural.fixed_substrates().live_count()
        || u64::try_from(store_mobiles.len()).map_err(|_| SimulationError::NumericOverflow)?
            != structural.mobile_substrates().live_count()
    {
        return Err(SimulationError::InvalidCanonicalState);
    }
    let track_graph = TrackGraph::compile(structural.wires(), structural.junctions())?;
    for (_, mobile) in structural.mobile_substrates().iter_alive() {
        track_graph.world_position(mobile.track_position)?;
    }
    Ok(())
}

fn validate_gate_driver(
    signal: &SignalWorld,
    owner: GateId,
    driver: DriverId,
    role: DriverRole,
    referenced: &mut BTreeSet<DriverId>,
) -> Result<(), SimulationError> {
    let record = signal
        .driver_record(driver)
        .ok_or(SimulationError::InvalidCanonicalState)?;
    if record.owner != owner.entity_id() || record.role != role || !referenced.insert(driver) {
        return Err(SimulationError::InvalidCanonicalState);
    }
    Ok(())
}

fn validate_gate_sink(
    signal: &SignalWorld,
    owner: GateId,
    sink: SinkId,
    role: SinkRole,
    referenced: &mut BTreeSet<SinkId>,
) -> Result<(), SimulationError> {
    let record = signal
        .sink_record(sink)
        .ok_or(SimulationError::InvalidCanonicalState)?;
    if record.owner != owner.entity_id() || record.role != role || !referenced.insert(sink) {
        return Err(SimulationError::InvalidCanonicalState);
    }
    Ok(())
}

fn validate_mobile_sink(
    signal: &SignalWorld,
    owner: crate::MobileId,
    sink: SinkId,
    role: SinkRole,
    referenced: &mut BTreeSet<SinkId>,
) -> Result<(), SimulationError> {
    let record = signal
        .sink_record(sink)
        .ok_or(SimulationError::InvalidCanonicalState)?;
    if record.owner != owner.entity_id() || record.role != role || !referenced.insert(sink) {
        return Err(SimulationError::InvalidCanonicalState);
    }
    Ok(())
}

fn validate_event_state(
    world: &CanonicalWorld,
    driver_frontier: u64,
    sink_frontier: u64,
    live_drivers: &BTreeSet<DriverId>,
) -> Result<(), SimulationError> {
    let payload_frontier = world.event_payloads.next_payload_order();
    if payload_frontier == 0 {
        return Err(SimulationError::EventQueueInvariantViolation);
    }
    world.path_certificates.validate_shape()?;
    let certificate_frontier = world.path_certificates.frontier().0;
    if certificate_frontier == 0 {
        return Err(SimulationError::PathCertificateInvariantViolation);
    }
    let mut live_certificates = BTreeSet::new();
    let structural_frontier = world.structural.entities().next_id().0;
    for (certificate, record) in world.path_certificates.canonical_slots() {
        let Some(_) = record else {
            continue;
        };
        if !live_certificates.insert(certificate) {
            return Err(SimulationError::PathCertificateInvariantViolation);
        }
        for element in world.path_certificates.elements(certificate)? {
            let entity = element.entity_id().0;
            if entity == 0 || entity >= structural_frontier {
                return Err(SimulationError::PathCertificateInvariantViolation);
            }
        }
    }

    let mut payloads = BTreeSet::new();

    for (calendar_key, event) in world.driver_events.canonical_entries() {
        let driver_raw = event.driver_id.entity_id().0;
        if *calendar_key != event.key
            || event.key.due_tick < world.next_tick
            || event.key.kind_order != DRIVER_TRANSITION_KIND_ORDER
            || event.key.target_id != driver_raw
            || event.key.source_id != driver_raw
            || event.key.revision != Revision(0)
            || event.key.generation != event.pending_generation
            || driver_raw == 0
            || driver_raw >= driver_frontier
            || event.key.payload_order == 0
            || event.key.payload_order >= payload_frontier
            || !payloads.insert(event.key.payload_order)
        {
            return Err(SimulationError::EventQueueInvariantViolation);
        }
        match event.cause {
            DriverTransitionCause::ExternalDriver | DriverTransitionCause::GateStrengthResponse
                if event.pending_generation != 0 =>
            {
                return Err(SimulationError::EventQueueInvariantViolation);
            }
            DriverTransitionCause::GateOutput if event.pending_generation == 0 => {
                return Err(SimulationError::EventQueueInvariantViolation);
            }
            _ => {}
        }
        if live_drivers.contains(&event.driver_id) {
            let record = world
                .signal
                .driver_record(event.driver_id)
                .ok_or(SimulationError::InvalidCanonicalState)?;
            let valid_role = match event.cause {
                DriverTransitionCause::ExternalDriver => record.role.is_external(),
                DriverTransitionCause::GateOutput | DriverTransitionCause::GateStrengthResponse => {
                    record.role == DriverRole::GateOutput
                }
            };
            if !valid_role {
                return Err(SimulationError::InvalidCanonicalState);
            }
        }
    }

    let mut referenced_certificates = BTreeSet::new();
    for (calendar_key, event) in world.signal_events.canonical_entries() {
        let driver_raw = event.source_driver.entity_id().0;
        let sink_raw = event.sink.entity_id().0;
        if *calendar_key != event.key
            || event.key.due_tick < world.next_tick
            || event.key.kind_order != SIGNAL_ARRIVAL_KIND_ORDER
            || event.key.target_id != sink_raw
            || event.key.source_id != driver_raw
            || event.key.revision != event.sample.revision
            || event.key.generation != 0
            || event.sample.driver_id != event.source_driver
            || event.sample.emitted_at >= event.key.due_tick
            || !matches!(
                event.kind,
                SignalArrivalKind::Propagation | SignalArrivalKind::TopologySync
            )
            || driver_raw == 0
            || driver_raw >= driver_frontier
            || sink_raw == 0
            || sink_raw >= sink_frontier
            || event.key.payload_order == 0
            || event.key.payload_order >= payload_frontier
            || !payloads.insert(event.key.payload_order)
        {
            return Err(SimulationError::EventQueueInvariantViolation);
        }
        if let Some(driver) = world.signal.driver_record(event.source_driver)
            && event.sample.revision > driver.sample.revision
        {
            return Err(SimulationError::DriverRevisionInvariantViolation);
        }
        let certificate = event
            .path_certificate
            .ok_or(SimulationError::PathCertificateInvariantViolation)?;
        if certificate.0 == 0
            || certificate.0 >= certificate_frontier
            || !live_certificates.contains(&certificate)
            || !referenced_certificates.insert(certificate)
        {
            return Err(SimulationError::PathCertificateInvariantViolation);
        }
    }
    if referenced_certificates != live_certificates {
        return Err(SimulationError::PathCertificateInvariantViolation);
    }
    Ok(())
}

fn apply_route_diff(
    world: &mut CanonicalWorld,
    diff: &RouteDiff,
    topology: &CompiledSignalTopology,
    tick: Tick,
) -> Result<SignalStepCounters, SimulationError> {
    let mut counters = SignalStepCounters {
        routes_added: count_items(&diff.added)?,
        routes_removed: count_items(&diff.removed)?,
        routes_retained: count_items(&diff.retained)?,
        routes_replaced: count_items(&diff.replaced)?,
        ..SignalStepCounters::default()
    };

    for pair in &diff.removed {
        world.signal.remove_route_slot(pair.sink, pair.driver)?;
    }

    let mut candidates = Vec::new();
    for pair in diff.added.iter().chain(&diff.replaced) {
        let route = topology
            .route(*pair)
            .ok_or(SimulationError::InvalidCanonicalState)?;
        let sample = world
            .signal
            .driver_sample(pair.driver)
            .ok_or(SimulationError::InvalidCanonicalState)?;
        candidates.push(UncertifiedSignalArrival::topology_sync(
            tick.checked_add(route.delay)?,
            pair.driver,
            pair.sink,
            sample,
            route.path_stamps.clone(),
        ));
    }

    let expected = counters
        .routes_added
        .checked_add(counters.routes_replaced)
        .ok_or(SimulationError::NumericOverflow)?;
    let inserted = stage_signal_arrivals(
        &mut world.signal_events,
        &mut world.event_payloads,
        &mut world.path_certificates,
        candidates,
    )?;
    counters.topology_sync_arrivals_staged =
        u64::try_from(inserted).map_err(|_| SimulationError::NumericOverflow)?;
    if counters.topology_sync_arrivals_staged != expected {
        return Err(SimulationError::EventQueueInvariantViolation);
    }
    Ok(counters)
}

fn count_items<T>(items: &[T]) -> Result<u64, SimulationError> {
    u64::try_from(items.len()).map_err(|_| SimulationError::NumericOverflow)
}

fn stage_external_driver_updates(
    world: &mut CanonicalWorld,
    updates: &[crate::structural::ExternalDriverUpdate],
    tick: Tick,
) -> Result<(), SimulationError> {
    let mut candidates = Vec::new();
    for update in updates {
        let record = world
            .signal
            .driver_record(update.driver)
            .ok_or(SimulationError::InvalidCanonicalState)?;
        if !record.role.is_external() {
            return Err(SimulationError::InvalidCanonicalState);
        }
        if record.sample.level == update.level && record.sample.strength == update.strength {
            continue;
        }
        candidates.push(DriverTransition::s0m3(
            tick,
            update.driver,
            update.level,
            update.strength,
            0,
            DriverTransitionCause::ExternalDriver,
        ));
    }
    world
        .driver_events
        .stage(&mut world.event_payloads, candidates)?;
    Ok(())
}

fn run_phase0_structural_commit(
    world: &mut CanonicalWorld,
    commands: &[CommandEnvelope],
    tick: Tick,
    physical: &PhysicalScaleProfile,
    balance: &BalanceProfile,
) -> Result<Phase0Output, SimulationError> {
    let old_topology = CompiledSignalTopology::compile(&world.structural, &world.signal, balance)?;
    let structural_report =
        world
            .structural
            .apply_phase0_with_signal(&mut world.signal, tick, commands, physical)?;
    world.topology_revision = if structural_report.topology_changed {
        world.topology_revision.checked_add(Revision(1))?
    } else {
        world.topology_revision
    };

    let track_graph = TrackGraph::compile(world.structural.wires(), world.structural.junctions())?;
    let topology = CompiledSignalTopology::compile(&world.structural, &world.signal, balance)?;
    let signal_counters = if structural_report.topology_changed {
        apply_route_diff(world, &old_topology.route_diff(&topology), &topology, tick)?
    } else {
        SignalStepCounters::default()
    };
    stage_external_driver_updates(world, &structural_report.external_driver_updates, tick)?;

    Ok(Phase0Output {
        structural_report,
        topology,
        track_graph,
        signal_counters,
    })
}

fn run_phase1_snapshot_and_world_sample(
    world: &CanonicalWorld,
    track_graph: &TrackGraph,
) -> Result<Phase1Snapshot, SimulationError> {
    let mut mobiles = Vec::new();
    for (index, record) in world.structural.mobile_substrates().iter_alive() {
        let start = record.track_position;
        mobiles.push(MobilePhase1Snapshot {
            index,
            mobile: record.id,
            start,
            world_point: track_graph.world_position(start)?,
        });
    }
    mobiles.sort_unstable_by_key(|snapshot| snapshot.mobile.entity_id());
    if mobiles
        .windows(2)
        .any(|pair| pair[0].mobile >= pair[1].mobile)
    {
        return Err(SimulationError::InvalidCanonicalState);
    }
    Ok(Phase1Snapshot { mobiles })
}

#[derive(Clone, Copy)]
struct ValidDriverTransition {
    event: DriverTransition,
    clear_pending_gate: Option<GateId>,
}

struct Phase2Report {
    driver_changes: Vec<DriverChangeRecord>,
    signal_changes: Vec<SignalChangeRecord>,
    signal_arrivals: Vec<SignalArrivalObservation>,
}

fn run_phase2(
    world: &mut CanonicalWorld,
    topology: &CompiledSignalTopology,
    tick: Tick,
    logic_threshold: u64,
    counters: &mut SignalStepCounters,
) -> Result<Phase2Report, SimulationError> {
    let due_drivers = world.driver_events.drain_due(tick)?;
    let mut valid = BTreeMap::<DriverId, ValidDriverTransition>::new();
    for event in due_drivers {
        let Some(candidate) = validate_driver_transition(world, event, tick)? else {
            counters.stale_driver_transitions = counters
                .stale_driver_transitions
                .checked_add(1)
                .ok_or(SimulationError::NumericOverflow)?;
            continue;
        };
        match valid.get(&event.driver_id) {
            None => {
                valid.insert(event.driver_id, candidate);
            }
            Some(existing)
                if existing.event.level == event.level
                    && existing.event.strength == event.strength
                    && existing.clear_pending_gate == candidate.clear_pending_gate => {}
            Some(_) => return Err(SimulationError::InvalidCanonicalState),
        }
    }

    let mut driver_changes = Vec::new();
    for (driver, transition) in valid {
        if let Some(change) = world.signal.apply_driver_sample(
            driver,
            transition.event.level,
            transition.event.strength,
            tick,
        )? {
            driver_changes.push(change);
            counters.driver_transitions_applied = counters
                .driver_transitions_applied
                .checked_add(1)
                .ok_or(SimulationError::NumericOverflow)?;
        }
        if let Some(gate) = transition.clear_pending_gate {
            world.signal.clear_pending(gate)?;
        }
    }

    let mut arrivals = Vec::new();
    for change in &driver_changes {
        for route in topology.routes_from(change.driver) {
            let due_tick = tick.checked_add(route.delay)?;
            arrivals.push(UncertifiedSignalArrival::propagation(
                due_tick,
                change.driver,
                route.sink,
                change.current,
                route.path_stamps.clone(),
            ));
        }
    }
    stage_signal_arrivals(
        &mut world.signal_events,
        &mut world.event_payloads,
        &mut world.path_certificates,
        arrivals,
    )?;

    let due_arrivals = world.signal_events.drain_due(tick)?;
    let signal_arrivals = due_arrivals
        .iter()
        .map(|arrival| SignalArrivalObservation {
            due_tick: arrival.key.due_tick,
            source_driver: arrival.source_driver,
            sink: arrival.sink,
            sample: arrival.sample,
            kind: arrival.kind,
        })
        .collect();
    apply_due_signal_arrivals(world, due_arrivals, counters)?;
    let (signal_changes, sinks_resolved) = world.signal.resolve_dirty(logic_threshold)?;
    counters.sinks_resolved = sinks_resolved;
    let excitations = topology.wire_excitations(&world.signal)?;
    world.signal.set_wire_excitations(&excitations)?;

    Ok(Phase2Report {
        driver_changes,
        signal_changes,
        signal_arrivals,
    })
}

#[derive(Clone, Copy)]
struct RevisionBucket {
    sample: DriverSample,
    count: u64,
}

fn apply_due_signal_arrivals(
    world: &mut CanonicalWorld,
    arrivals: Vec<SignalArrival>,
    counters: &mut SignalStepCounters,
) -> Result<(), SimulationError> {
    let mut groups = BTreeMap::<(SinkId, DriverId), BTreeMap<Revision, RevisionBucket>>::new();

    for arrival in arrivals {
        validate_due_signal_arrival_shape(arrival)?;
        let certificate = arrival
            .path_certificate
            .ok_or(SimulationError::PathCertificateInvariantViolation)?;
        let path = world.path_certificates.consume(certificate)?;
        let endpoints_live = world.signal.driver_record(arrival.source_driver).is_some()
            && world.signal.sink_record(arrival.sink).is_some();
        let path_current = path
            .iter()
            .copied()
            .all(|stamp| world.structural.path_element_is_current(stamp));
        if !endpoints_live || !path_current {
            add_counter(&mut counters.invalid_path_arrivals, 1)?;
            continue;
        }

        let buckets = groups
            .entry((arrival.sink, arrival.source_driver))
            .or_default();
        match buckets.entry(arrival.sample.revision) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(RevisionBucket {
                    sample: arrival.sample,
                    count: 1,
                });
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                let bucket = entry.get_mut();
                if bucket.sample != arrival.sample {
                    return Err(SimulationError::DriverRevisionInvariantViolation);
                }
                bucket.count = bucket
                    .count
                    .checked_add(1)
                    .ok_or(SimulationError::NumericOverflow)?;
            }
        }
    }

    for ((sink, driver), buckets) in groups {
        let stored = world.signal.sink_driver_sample(sink, driver);
        if let Some(stored) = stored
            && let Some(bucket) = buckets.get(&stored.revision)
            && bucket.sample != stored
        {
            return Err(SimulationError::DriverRevisionInvariantViolation);
        }

        let (&winner_revision, winner) = buckets
            .last_key_value()
            .ok_or(SimulationError::DriverRevisionInvariantViolation)?;
        let lower_count = buckets
            .iter()
            .filter(|(revision, _)| **revision != winner_revision)
            .try_fold(0_u64, |total, (_, bucket)| {
                total
                    .checked_add(bucket.count)
                    .ok_or(SimulationError::NumericOverflow)
            })?;

        match stored.map(|sample| sample.revision.cmp(&winner_revision)) {
            Some(std::cmp::Ordering::Greater) => {
                let total = lower_count
                    .checked_add(winner.count)
                    .ok_or(SimulationError::NumericOverflow)?;
                add_counter(&mut counters.stale_revision_arrivals, total)?;
            }
            Some(std::cmp::Ordering::Equal) => {
                add_counter(&mut counters.stale_revision_arrivals, lower_count)?;
                add_counter(&mut counters.idempotent_signal_arrivals, winner.count)?;
            }
            Some(std::cmp::Ordering::Less) | None => {
                add_counter(&mut counters.stale_revision_arrivals, lower_count)?;
                match world.signal.apply_slot_sample(sink, winner.sample)? {
                    SlotApplyOutcome::Applied => {}
                    SlotApplyOutcome::Idempotent | SlotApplyOutcome::Stale => {
                        return Err(SimulationError::DriverRevisionInvariantViolation);
                    }
                }
                add_counter(&mut counters.signal_arrivals_applied, 1)?;
                add_counter(
                    &mut counters.idempotent_signal_arrivals,
                    winner
                        .count
                        .checked_sub(1)
                        .ok_or(SimulationError::DriverRevisionInvariantViolation)?,
                )?;
            }
        }
    }
    Ok(())
}

fn validate_due_signal_arrival_shape(arrival: SignalArrival) -> Result<(), SimulationError> {
    if arrival.key.kind_order != SIGNAL_ARRIVAL_KIND_ORDER
        || arrival.key.target_id != arrival.sink.entity_id().0
        || arrival.key.source_id != arrival.source_driver.entity_id().0
        || arrival.key.revision != arrival.sample.revision
        || arrival.key.generation != 0
        || arrival.sample.driver_id != arrival.source_driver
        || !matches!(
            arrival.kind,
            SignalArrivalKind::Propagation | SignalArrivalKind::TopologySync
        )
    {
        return Err(SimulationError::EventQueueInvariantViolation);
    }
    if arrival.path_certificate.is_none() {
        return Err(SimulationError::PathCertificateInvariantViolation);
    }
    Ok(())
}

fn add_counter(counter: &mut u64, amount: u64) -> Result<(), SimulationError> {
    *counter = counter
        .checked_add(amount)
        .ok_or(SimulationError::NumericOverflow)?;
    Ok(())
}

fn validate_driver_transition(
    world: &CanonicalWorld,
    event: DriverTransition,
    tick: Tick,
) -> Result<Option<ValidDriverTransition>, SimulationError> {
    if event.key.due_tick != tick
        || event.key.target_id != event.driver_id.entity_id().0
        || event.key.source_id != event.driver_id.entity_id().0
        || event.key.generation != event.pending_generation
    {
        return Err(SimulationError::EventQueueInvariantViolation);
    }
    let Some(driver) = world.signal.driver_record(event.driver_id) else {
        return Ok(None);
    };
    match event.cause {
        DriverTransitionCause::ExternalDriver => {
            if !driver.role.is_external() || event.pending_generation != 0 {
                return Err(SimulationError::InvalidCanonicalState);
            }
            Ok(Some(ValidDriverTransition {
                event,
                clear_pending_gate: None,
            }))
        }
        DriverTransitionCause::GateOutput => {
            if driver.role != DriverRole::GateOutput {
                return Err(SimulationError::InvalidCanonicalState);
            }
            let gate = world
                .signal
                .gate_record(GateId(driver.owner))
                .ok_or(SimulationError::InvalidCanonicalState)?;
            if gate.ports.output != event.driver_id {
                return Err(SimulationError::InvalidCanonicalState);
            }
            if gate.pending_generation != event.pending_generation
                || gate.pending_due_tick != Some(tick)
                || gate.pending_level != Some(event.level)
            {
                return Ok(None);
            }
            Ok(Some(ValidDriverTransition {
                event,
                clear_pending_gate: Some(GateId(driver.owner)),
            }))
        }
        DriverTransitionCause::GateStrengthResponse => {
            if driver.role != DriverRole::GateOutput || event.pending_generation != 0 {
                return Err(SimulationError::InvalidCanonicalState);
            }
            let gate = world
                .signal
                .gate_record(GateId(driver.owner))
                .ok_or(SimulationError::InvalidCanonicalState)?;
            if gate.current_output != event.level || gate.pending_due_tick.is_some() {
                return Ok(None);
            }
            Ok(Some(ValidDriverTransition {
                event,
                clear_pending_gate: None,
            }))
        }
    }
}

fn run_phase3(
    world: &mut CanonicalWorld,
    snapshot: &Phase1Snapshot,
) -> Result<Vec<MobileIntent>, SimulationError> {
    let gates: Vec<_> = world
        .signal
        .iter_gates()
        .map(|record| record.gate)
        .collect();
    for gate in gates {
        world.signal.set_gate_desired_from_inputs(gate)?;
    }
    snapshot
        .mobiles
        .iter()
        .copied()
        .map(|mobile_snapshot| {
            let ports = world
                .signal
                .mobile_ports(mobile_snapshot.mobile)
                .ok_or(SimulationError::InvalidCanonicalState)?;
            let level = |sink| {
                world
                    .signal
                    .sink_level(sink)
                    .ok_or(SimulationError::InvalidCanonicalState)
            };
            Ok(MobileIntent {
                index: mobile_snapshot.index,
                mobile: mobile_snapshot.mobile,
                start: mobile_snapshot.start,
                start_world_point: mobile_snapshot.world_point,
                controls: MobileControlSample {
                    stop: level(ports.stop)?,
                    left: level(ports.left)?,
                    right: level(ports.right)?,
                },
                granted_budget: Fixed::ZERO,
            })
        })
        .collect()
}

fn run_phase4_global_accounting_and_nominal_demand(
    _world: &CanonicalWorld,
    _mobile_intents: &[MobileIntent],
) {
    // Stage 0 has no Capacity, Power, or economy stores. The fixed movement demand represented by
    // each MobileIntent is granted at unity in Phase 6; the phase remains explicit and ordered.
}

fn run_phase5_power_solve_and_brownout() {
    // Stage 0 freezes the power ratio at one, so there is no Power Region solve yet.
}

fn grant_stage0_mobile_budgets(intents: &mut [MobileIntent], budget: Fixed) {
    for intent in intents {
        intent.granted_budget = if intent.controls.grants_stage0_movement() {
            budget
        } else {
            Fixed::ZERO
        };
    }
}

fn run_phase7(
    track_graph: &TrackGraph,
    intents: &[MobileIntent],
) -> Result<Vec<StagedMobileMovement>, SimulationError> {
    intents
        .iter()
        .map(|intent| {
            if track_graph.world_position(intent.start)? != intent.start_world_point {
                return Err(SimulationError::InvalidCanonicalState);
            }
            Ok(StagedMobileMovement {
                index: intent.index,
                observation: track_graph.stage_movement(
                    intent.mobile,
                    intent.start,
                    intent.controls,
                    intent.granted_budget,
                )?,
            })
        })
        .collect()
}

fn run_phase6(
    world: &mut CanonicalWorld,
    topology: &CompiledSignalTopology,
    tick: Tick,
    balance: &BalanceProfile,
    mobile_intents: &mut [MobileIntent],
    movement_budget: Fixed,
) -> Result<(), SimulationError> {
    let gates: Vec<_> = world.signal.iter_gates().collect();
    let mut candidates = Vec::new();
    for gate in gates {
        let mut replacement_generation = None;
        match (
            gate.pending_due_tick,
            gate.pending_level,
            gate.pending_switch_energy,
        ) {
            (Some(_), Some(level), Some(_))
                if level == gate.desired_output && gate.desired_output != gate.current_output =>
            {
                continue;
            }
            (Some(_), Some(_), Some(energy)) => {
                world.signal.add_cancelled_heat(gate.gate, energy)?;
                replacement_generation = Some(world.signal.advance_pending_generation(gate.gate)?);
                world.signal.clear_pending(gate.gate)?;
            }
            (None, None, None) => {}
            _ => return Err(SimulationError::InvalidCanonicalState),
        }

        if gate.desired_output != gate.current_output {
            let generation = match replacement_generation {
                Some(generation) => generation,
                None => world.signal.advance_pending_generation(gate.gate)?,
            };
            let load = topology
                .driver_load(gate.ports.output)
                .ok_or(SimulationError::InvalidCanonicalState)?;
            let due_tick = tick.checked_add(load.gate_delay)?;
            let energy = switch_energy(load.total_load, balance)?;
            world
                .signal
                .set_pending(gate.gate, due_tick, gate.desired_output, energy)?;
            candidates.push(DriverTransition::s0m3(
                due_tick,
                gate.ports.output,
                gate.desired_output,
                DriveStrength(balance.nominal_gate_drive),
                generation,
                DriverTransitionCause::GateOutput,
            ));
            continue;
        }

        let output = world
            .signal
            .driver_sample(gate.ports.output)
            .ok_or(SimulationError::InvalidCanonicalState)?;
        if output.level != gate.current_output {
            return Err(SimulationError::InvalidCanonicalState);
        }
        if output.strength != DriveStrength(balance.nominal_gate_drive) {
            let due_tick = tick.checked_add(Tick(1))?;
            candidates.push(DriverTransition::s0m3(
                due_tick,
                gate.ports.output,
                gate.current_output,
                DriveStrength(balance.nominal_gate_drive),
                0,
                DriverTransitionCause::GateStrengthResponse,
            ));
        }
    }
    world
        .driver_events
        .stage(&mut world.event_payloads, candidates)?;
    grant_stage0_mobile_budgets(mobile_intents, movement_budget);
    Ok(())
}

fn run_phase8_interaction(_staged_mobiles: &[StagedMobileMovement]) {
    // Stage 0 has no collision, payload, construction, radiation, or heat contribution stores.
}

fn run_phase9_thermal_integration() {
    // Stage 0 has no thermal state.
}

fn run_phase10_damage_resolution() {
    // Stage 0 has no Integrity, exposure, or pending-destruction state.
}

fn run_phase11_progress_commit(
    world: &mut CanonicalWorld,
    next_tick: Tick,
    staged_mobiles: Vec<StagedMobileMovement>,
) -> Result<Phase11Output, SimulationError> {
    let committed_positions = staged_mobiles
        .iter()
        .map(|staged| {
            (
                staged.index,
                staged.observation.mobile,
                staged.observation.end,
            )
        })
        .collect::<Vec<_>>();
    world
        .structural
        .commit_mobile_positions(&committed_positions)?;
    world.next_tick = next_tick;
    validate_canonical_world(world)?;
    let state_hash = canonical::state_hash(world.state_view());
    let mobile_movements = staged_mobiles
        .into_iter()
        .map(|staged| staged.observation)
        .collect();
    Ok(Phase11Output {
        state_hash,
        mobile_movements,
    })
}

impl From<ProfileValidationError> for SimulationError {
    fn from(error: ProfileValidationError) -> Self {
        Self::InvalidProfile { error }
    }
}

impl From<StructuralError> for SimulationError {
    fn from(error: StructuralError) -> Self {
        match error {
            StructuralError::NumericOverflow => Self::NumericOverflow,
            StructuralError::InvalidCanonicalState => Self::InvalidCanonicalState,
        }
    }
}

impl From<SignalError> for SimulationError {
    fn from(error: SignalError) -> Self {
        match error {
            SignalError::NumericOverflow => Self::NumericOverflow,
            SignalError::InvalidCanonicalState => Self::InvalidCanonicalState,
            SignalError::DriverRevisionInvariantViolation => Self::DriverRevisionInvariantViolation,
        }
    }
}

impl From<SignalTopologyError> for SimulationError {
    fn from(error: SignalTopologyError) -> Self {
        match error {
            SignalTopologyError::NumericOverflow => Self::NumericOverflow,
            SignalTopologyError::InvalidCanonicalState => Self::InvalidCanonicalState,
        }
    }
}

impl From<TrackGraphError> for SimulationError {
    fn from(error: TrackGraphError) -> Self {
        match error {
            TrackGraphError::NumericOverflow => Self::NumericOverflow,
            TrackGraphError::InvalidCanonicalState => Self::InvalidCanonicalState,
        }
    }
}

impl From<EventCalendarError> for SimulationError {
    fn from(error: EventCalendarError) -> Self {
        match error {
            EventCalendarError::PayloadOrderExhausted => Self::NumericOverflow,
            EventCalendarError::ReservedPayloadOrder
            | EventCalendarError::AssignedStagedPayload { .. }
            | EventCalendarError::InvalidKindOrder { .. }
            | EventCalendarError::DuplicateEventKey { .. }
            | EventCalendarError::OverdueEvent { .. } => Self::EventQueueInvariantViolation,
        }
    }
}

impl From<PathCertificateError> for SimulationError {
    fn from(error: PathCertificateError) -> Self {
        match error {
            PathCertificateError::CertificateIdExhausted
            | PathCertificateError::CertificateSlotIndexExhausted
            | PathCertificateError::ElementRangeExhausted => Self::NumericOverflow,
            PathCertificateError::ReservedCertificateId
            | PathCertificateError::UnknownCertificate { .. }
            | PathCertificateError::ConsumedCertificate { .. }
            | PathCertificateError::StaleBatchPlan
            | PathCertificateError::InvalidSlotLayout
            | PathCertificateError::CertificateIdMismatch { .. }
            | PathCertificateError::InvalidElementRange { .. }
            | PathCertificateError::OverlappingElementRange { .. } => {
                Self::PathCertificateInvariantViolation
            }
        }
    }
}

impl From<SignalArrivalStagingError> for SimulationError {
    fn from(error: SignalArrivalStagingError) -> Self {
        match error {
            SignalArrivalStagingError::PathCertificate(error) => error.into(),
            SignalArrivalStagingError::EventCalendar(error) => error.into(),
            SignalArrivalStagingError::ReservedPathElement => {
                Self::PathCertificateInvariantViolation
            }
            SignalArrivalStagingError::ReservedSourceDriver
            | SignalArrivalStagingError::ReservedSink
            | SignalArrivalStagingError::SampleDriverMismatch => Self::EventQueueInvariantViolation,
        }
    }
}

impl From<ContractValidationError> for SimulationError {
    fn from(error: ContractValidationError) -> Self {
        match error {
            ContractValidationError::Profile(error) => Self::InvalidProfile { error },
            ContractValidationError::ProfileHashMismatch {
                profile,
                expected,
                actual,
            } => Self::ProfileHashMismatch {
                profile,
                expected,
                actual,
            },
            ContractValidationError::UnsupportedSemanticsVersion { actual } => {
                Self::UnsupportedSemanticsVersion { actual }
            }
            ContractValidationError::UnsupportedHashAlgorithm { actual } => {
                Self::UnsupportedHashAlgorithm { actual }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{BalanceProfile, NumericProfile, PhysicalScaleProfile};
    use crate::{ProfileKind, SemanticsVersion};

    fn package() -> SimulationPackage {
        let profiles = ProfileBundle {
            numeric: NumericProfile::reference_v1("numeric"),
            physical_scale: PhysicalScaleProfile::stage0_alpha("physical"),
            balance: BalanceProfile::stage0_alpha("balance"),
        };
        let contract = SimulationContract::from_profiles(&profiles).expect("profiles are valid");
        SimulationPackage::new(
            "empty",
            InitialWorld::Empty,
            StageFeatureSet::none(),
            contract,
            profiles,
        )
    }

    #[test]
    fn explicit_twelve_phase_sequence_is_total_ordered_and_enforced_by_step() {
        assert_eq!(TickPhase::ORDER.len(), 12);
        for (ordinal, phase) in TickPhase::ORDER.into_iter().enumerate() {
            assert_eq!(usize::from(phase as u8), ordinal);
        }

        let mut sequence = TickPhaseSequence::default();
        for phase in TickPhase::ORDER {
            sequence.enter(phase).expect("canonical phase order");
        }
        assert_eq!(sequence.finish(), Ok(()));

        let mut skipped = TickPhaseSequence::default();
        assert_eq!(
            skipped.enter(TickPhase::SnapshotAndWorldSample),
            Err(SimulationError::InvalidCanonicalState)
        );
        assert_eq!(
            skipped.finish(),
            Err(SimulationError::InvalidCanonicalState)
        );

        let mut duplicate = TickPhaseSequence::default();
        duplicate
            .enter(TickPhase::StructuralCommit)
            .expect("Phase 0 starts the Tick");
        assert_eq!(
            duplicate.enter(TickPhase::StructuralCommit),
            Err(SimulationError::InvalidCanonicalState)
        );

        let mut simulation = Simulation::new(package()).expect("test package is valid");
        let report = simulation
            .step(&[])
            .expect("Simulation::step completes all twelve phases");
        assert_eq!(report.completed_tick, Tick(0));
        assert_eq!(report.next_tick, Tick(1));
        assert_eq!(report.state_hash, simulation.state_hash());
    }

    #[test]
    fn phase1_snapshots_mobile_start_and_world_point_without_observable_mutation() {
        let mut simulation = Simulation::new(package()).expect("test package is valid");
        let pitch = simulation.profiles().physical_scale.world_routing_pitch;
        let circuit_pitch = simulation.profiles().physical_scale.circuit_routing_pitch;
        let point = |x, y| FixedVec2::new(Fixed(x), Fixed(y));
        simulation
            .step(&[crate::CommandEnvelope {
                target_tick: Tick(0),
                ordinal: 0,
                command: crate::Command::PlaceWire(crate::PlaceWireCommand {
                    routing_domain: crate::RoutingDomain::OpenWorld,
                    points: vec![point(0, 0), point(3 * pitch.0, 4 * pitch.0)],
                    endpoint_a: crate::EndpointTarget::Free,
                    endpoint_b: crate::EndpointTarget::Free,
                }),
            }])
            .expect("diagonal Track placement succeeds");
        let local_bounds = crate::FixedAabb::new(
            point(-4 * circuit_pitch.0, -4 * circuit_pitch.0),
            point(4 * circuit_pitch.0, 4 * circuit_pitch.0),
        );
        simulation
            .step(&[crate::CommandEnvelope {
                target_tick: Tick(1),
                ordinal: 0,
                command: crate::Command::PlaceMobileSubstrate(crate::PlaceMobileSubstrateCommand {
                    origin: point(0, 0),
                    routing_area: local_bounds,
                    footprint: local_bounds,
                }),
            }])
            .expect("Mobile placement succeeds");

        let graph = TrackGraph::compile(
            simulation.canonical.structural.wires(),
            simulation.canonical.structural.junctions(),
        )
        .expect("canonical Track compiles");
        let tick_before = simulation.next_tick();
        let hash_before = simulation.state_hash();
        let snapshot = run_phase1_snapshot_and_world_sample(&simulation.canonical, &graph)
            .expect("Phase 1 samples canonical Mobile state");
        assert_eq!(snapshot.mobiles.len(), 1);
        assert_eq!(
            snapshot.mobiles[0].start,
            TrackPosition::Edge {
                edge: WireId(crate::EntityId(1)),
                offset: Fixed(pitch.0),
                heading: crate::Heading::Forward,
            }
        );
        assert_eq!(
            snapshot.mobiles[0].world_point,
            point(
                i64::try_from(
                    crate::round_div_nearest_even(i128::from(3 * pitch.0), 5)
                        .expect("3:4 projection rounds"),
                )
                .expect("projected x fits Fixed"),
                i64::try_from(
                    crate::round_div_nearest_even(i128::from(4 * pitch.0), 5)
                        .expect("3:4 projection rounds"),
                )
                .expect("projected y fits Fixed"),
            )
        );
        assert_eq!(simulation.next_tick(), tick_before);
        assert_eq!(simulation.state_hash(), hash_before);

        let mut control = Simulation {
            scenario_id: simulation.scenario_id.clone(),
            canonical: simulation.canonical.clone(),
            profiles: simulation.profiles.clone(),
            initial_state_hash: simulation.initial_state_hash,
        };
        let sampled_report = simulation.step(&[]).expect("sampled replica advances");
        let control_report = control.step(&[]).expect("control replica advances");
        assert_eq!(sampled_report, control_report);
        assert_eq!(simulation.state_hash(), control.state_hash());
    }

    fn place_test_not(simulation: &mut Simulation) -> GateId {
        let bounds = crate::FixedAabb::new(
            crate::FixedVec2::new(
                crate::Fixed(-4 * crate::FIXED_ONE),
                crate::Fixed(-4 * crate::FIXED_ONE),
            ),
            crate::FixedVec2::new(
                crate::Fixed(4 * crate::FIXED_ONE),
                crate::Fixed(4 * crate::FIXED_ONE),
            ),
        );
        let substrate = simulation
            .step(&[crate::CommandEnvelope {
                target_tick: simulation.next_tick(),
                ordinal: 0,
                command: crate::Command::PlaceFixedSubstrate(crate::PlaceFixedSubstrateCommand {
                    origin: crate::FixedVec2::new(crate::Fixed(0), crate::Fixed(0)),
                    routing_area: bounds,
                    footprint: bounds,
                }),
            }])
            .expect("test Substrate placement succeeds");
        let substrate = substrate.command_acceptances[0]
            .created_entity
            .expect("Substrate placement allocates an EntityId");
        let report = simulation
            .step(&[crate::CommandEnvelope {
                target_tick: simulation.next_tick(),
                ordinal: 0,
                command: crate::Command::PlaceGate(crate::PlaceGateCommand {
                    gate_type: GateType::Not,
                    origin: crate::FixedVec2::new(crate::Fixed(0), crate::Fixed(0)),
                    routing_domain: crate::RoutingDomain::FixedSubstrate(substrate),
                }),
            }])
            .expect("test NOT placement succeeds");
        GateId(
            report.command_acceptances[0]
                .created_entity
                .expect("Gate placement allocates an EntityId"),
        )
    }

    #[test]
    fn tick_overflow_is_typed_and_does_not_wrap() {
        let mut simulation = Simulation::new(package()).expect("test package is valid");
        simulation.canonical.next_tick = Tick(u64::MAX);
        let before_hash = simulation.state_hash();

        assert_eq!(simulation.step(&[]), Err(SimulationError::NumericOverflow));
        assert_eq!(simulation.next_tick(), Tick(u64::MAX));
        assert_eq!(simulation.state_hash(), before_hash);
    }

    #[test]
    fn mobility_ratio_cap_preserves_profiles_and_bounds_mobile_placement() {
        fn point(x: i64) -> crate::FixedVec2 {
            crate::FixedVec2::new(Fixed(x), Fixed::ZERO)
        }

        fn simulation_with_one_unit_track(world_routing_pitch: i64) -> Simulation {
            let mut profiles = ProfileBundle {
                numeric: NumericProfile::reference_v1("extreme-mobility"),
                physical_scale: PhysicalScaleProfile::stage0_alpha("extreme-mobility"),
                balance: BalanceProfile::stage0_alpha("extreme-mobility"),
            };
            profiles.physical_scale.wire_geometry_quantum = Fixed(1);
            profiles.physical_scale.world_routing_pitch = Fixed(world_routing_pitch);
            profiles
                .validate()
                .expect("pre-Mobility Physical Scale v1 profiles remain valid");
            let contract =
                SimulationContract::from_profiles(&profiles).expect("valid profile contract");
            let package = SimulationPackage::new(
                "extreme-mobility",
                InitialWorld::Empty,
                StageFeatureSet::none(),
                contract,
                profiles,
            );
            let mut simulation = Simulation::new(package).expect("extreme simulation");
            let track = simulation
                .step(&[crate::CommandEnvelope {
                    target_tick: Tick(0),
                    ordinal: 0,
                    command: crate::Command::PlaceWire(crate::PlaceWireCommand {
                        routing_domain: crate::RoutingDomain::OpenWorld,
                        points: vec![point(0), point(1)],
                        endpoint_a: crate::EndpointTarget::Free,
                        endpoint_b: crate::EndpointTarget::Free,
                    }),
                }])
                .expect("one-unit Track placement");
            assert!(track.command_rejections.is_empty());
            simulation
        }

        let local_extent = 4 * crate::REFERENCE_CIRCUIT_ROUTING_PITCH.0;
        let bounds = crate::FixedAabb::new(
            crate::FixedVec2::new(Fixed(-local_extent), Fixed(-local_extent)),
            crate::FixedVec2::new(Fixed(local_extent), Fixed(local_extent)),
        );
        let unsupported_ratio = crate::MAX_STAGE0_WORLD_PITCH_GEOMETRY_QUANTA
            .checked_add(1)
            .expect("test ratio remains in range");
        let mut unsupported = simulation_with_one_unit_track(unsupported_ratio);
        let before_placement = unsupported.state_hash();
        let before_entity_count = unsupported
            .canonical
            .structural
            .entities()
            .allocated_count();
        let before_sink_frontier = unsupported.canonical.signal.sink_frontier();
        let rejected = unsupported
            .step(&[crate::CommandEnvelope {
                target_tick: Tick(1),
                ordinal: 0,
                command: crate::Command::PlaceMobileSubstrate(crate::PlaceMobileSubstrateCommand {
                    origin: point(0),
                    routing_area: bounds,
                    footprint: bounds,
                }),
            }])
            .expect("unsupported Mobility ratio is an ordinary command rejection");
        assert!(rejected.command_acceptances.is_empty());
        assert_eq!(
            rejected.command_rejections[0].reason,
            crate::CommandRejectionReason::UnsupportedPlacement
        );
        assert!(rejected.mobile_movements.is_empty());
        assert_ne!(
            rejected.state_hash, before_placement,
            "the rejected command still completes its Tick"
        );
        assert_eq!(
            unsupported
                .canonical
                .structural
                .entities()
                .allocated_count(),
            before_entity_count,
            "unsupported placement consumes no structural identity"
        );
        assert_eq!(
            unsupported.canonical.signal.sink_frontier(),
            before_sink_frontier,
            "unsupported placement consumes no intrinsic control Sink identity"
        );
        assert_eq!(
            unsupported
                .canonical
                .structural
                .mobile_substrates()
                .live_count(),
            0
        );

        let mut boundary =
            simulation_with_one_unit_track(crate::MAX_STAGE0_WORLD_PITCH_GEOMETRY_QUANTA);
        let before_boundary_sink = boundary.canonical.signal.sink_frontier();
        let placement = boundary
            .step(&[crate::CommandEnvelope {
                target_tick: boundary.next_tick(),
                ordinal: 0,
                command: crate::Command::PlaceMobileSubstrate(crate::PlaceMobileSubstrateCommand {
                    origin: point(0),
                    routing_area: bounds,
                    footprint: bounds,
                }),
            }])
            .expect("maximum supported Mobility ratio completes");
        assert!(placement.command_rejections.is_empty());
        let mobile = MobileId(
            placement.command_acceptances[0]
                .created_entity
                .expect("supported placement allocates a Mobile identity"),
        );
        let ports = boundary
            .canonical
            .signal
            .mobile_ports(mobile)
            .expect("supported placement activates all Mobile control ports");
        assert_ne!(ports.stop, ports.left);
        assert_ne!(ports.stop, ports.right);
        assert_ne!(ports.left, ports.right);
        assert_eq!(
            boundary.canonical.signal.sink_frontier().entity_id().0
                - before_boundary_sink.entity_id().0,
            3,
            "supported placement consumes exactly STOP, LEFT, and RIGHT Sink identities"
        );
        assert_eq!(placement.mobile_movements.len(), 1);
        assert_eq!(
            placement.mobile_movements[0].granted_budget,
            Fixed(crate::MAX_STAGE0_WORLD_PITCH_GEOMETRY_QUANTA)
        );
        assert_eq!(
            placement.mobile_movements[0].end,
            TrackPosition::Edge {
                edge: WireId(crate::EntityId(1)),
                offset: Fixed::ZERO,
                heading: crate::Heading::Reverse,
            }
        );
    }

    #[test]
    fn topology_revision_overflow_rolls_back_tick_and_structural_changes() {
        let mut simulation = Simulation::new(package()).expect("test package is valid");
        simulation.canonical.topology_revision = Revision(u64::MAX);
        let before_hash = simulation.state_hash();
        let command = crate::CommandEnvelope {
            target_tick: Tick(0),
            ordinal: 0,
            command: crate::Command::PlaceJunction(crate::PlaceJunctionCommand {
                routing_domain: crate::RoutingDomain::OpenWorld,
                position: crate::FixedVec2::new(crate::Fixed(0), crate::Fixed(0)),
            }),
        };

        assert_eq!(
            simulation.step(&[command]),
            Err(SimulationError::NumericOverflow)
        );
        assert_eq!(simulation.next_tick(), Tick(0));
        assert_eq!(simulation.topology_revision(), Revision(u64::MAX));
        assert_eq!(simulation.state_hash(), before_hash);
    }

    #[test]
    fn pending_generation_overflow_in_step_rolls_back_the_complete_canonical_world() {
        let mut simulation = Simulation::new(package()).expect("test package is valid");
        let gate = place_test_not(&mut simulation);
        simulation
            .step(&[])
            .expect("the initial NOT transition settles");
        let ports = simulation
            .gate_signal_ports(gate)
            .expect("test Gate signal ports exist");
        simulation
            .canonical
            .signal
            .force_pending_generation_for_test(gate, u32::MAX)
            .expect("test-only generation seed succeeds");
        validate_canonical_world(&simulation.canonical)
            .expect("maximum generation with no pending event is canonical");

        let mut control = Simulation {
            scenario_id: simulation.scenario_id.clone(),
            canonical: simulation.canonical.clone(),
            profiles: simulation.profiles.clone(),
            initial_state_hash: simulation.initial_state_hash,
        };
        let before_hash = simulation.state_hash();
        let before_driver = simulation
            .driver_sample(ports.input_a.external_driver)
            .expect("external Driver is observable");
        let before_sink = simulation.sink_level(ports.input_a.sink);
        let before_gate = simulation
            .gate_signal_state(gate)
            .expect("Gate state is observable");
        let before_payload_frontier = simulation.canonical.event_payloads.next_payload_order();

        let command = crate::CommandEnvelope {
            target_tick: simulation.next_tick(),
            ordinal: 0,
            command: crate::Command::SetExternalDriver(crate::SetExternalDriverCommand {
                driver: ports.input_a.external_driver,
                level: LogicLevel::High,
                strength: DriveStrength(100),
            }),
        };
        assert_eq!(
            simulation.step(&[command]),
            Err(SimulationError::NumericOverflow)
        );

        assert_eq!(simulation.state_hash(), before_hash);
        assert_eq!(simulation.next_tick(), control.next_tick());
        assert_eq!(simulation.topology_revision(), control.topology_revision());
        assert_eq!(
            simulation.canonical.structural,
            control.canonical.structural
        );
        assert_eq!(simulation.canonical.signal, control.canonical.signal);
        assert_eq!(
            simulation.canonical.event_payloads,
            control.canonical.event_payloads
        );
        assert_eq!(
            simulation.canonical.driver_events,
            control.canonical.driver_events
        );
        assert_eq!(
            simulation.canonical.signal_events,
            control.canonical.signal_events
        );
        assert_eq!(
            simulation.canonical.path_certificates,
            control.canonical.path_certificates
        );
        assert_eq!(
            simulation.canonical.event_payloads.next_payload_order(),
            before_payload_frontier
        );
        assert_eq!(
            simulation.driver_sample(ports.input_a.external_driver),
            Some(before_driver)
        );
        assert_eq!(simulation.sink_level(ports.input_a.sink), before_sink);
        assert_eq!(simulation.gate_signal_state(gate), Some(before_gate));

        // Matching event-producing allocations prove that the Entity, Driver, Sink, and payload
        // frontiers all resume from exactly the pre-failure values.
        let retry_command = |target_tick| crate::CommandEnvelope {
            target_tick,
            ordinal: 0,
            command: crate::Command::PlaceGate(crate::PlaceGateCommand {
                gate_type: GateType::Not,
                origin: crate::FixedVec2::new(crate::Fixed(2 * crate::FIXED_ONE), crate::Fixed(0)),
                routing_domain: crate::RoutingDomain::FixedSubstrate(crate::EntityId(1)),
            }),
        };
        let failed_report = simulation
            .step(&[retry_command(simulation.next_tick())])
            .expect("retry after rollback succeeds");
        let control_report = control
            .step(&[retry_command(control.next_tick())])
            .expect("untouched control retry succeeds");
        assert_eq!(failed_report, control_report);
        assert_eq!(
            failed_report.command_acceptances[0].created_entity,
            Some(crate::EntityId(3))
        );
        let retry_gate = GateId(crate::EntityId(3));
        let retry_ports = simulation
            .gate_signal_ports(retry_gate)
            .expect("retry Gate endpoints exist");
        assert_eq!(
            retry_ports.input_a.external_driver,
            DriverId(crate::EntityId(3))
        );
        assert_eq!(retry_ports.output, DriverId(crate::EntityId(4)));
        assert_eq!(retry_ports.input_a.sink, SinkId(crate::EntityId(2)));
        assert_eq!(simulation.state_hash(), control.state_hash());
        assert_eq!(
            simulation.canonical.event_payloads,
            control.canonical.event_payloads
        );
        assert!(simulation.canonical.event_payloads.next_payload_order() > before_payload_frontier);
    }

    #[test]
    fn driver_revision_overflow_rolls_back_event_and_certificate_frontiers() {
        let mut simulation = Simulation::new(package()).expect("test package is valid");
        let gate = place_test_not(&mut simulation);
        simulation
            .step(&[])
            .expect("the initial NOT transition settles");
        let ports = simulation
            .gate_signal_ports(gate)
            .expect("test Gate signal ports exist");
        simulation
            .canonical
            .signal
            .force_driver_revision_for_test(ports.input_a.external_driver, Revision(u64::MAX))
            .expect("test-only Revision seed succeeds");
        validate_canonical_world(&simulation.canonical)
            .expect("maximum live Driver Revision is canonical before mutation");

        let before_hash = simulation.state_hash();
        let before = simulation.canonical.clone();
        let command = crate::CommandEnvelope {
            target_tick: simulation.next_tick(),
            ordinal: 0,
            command: crate::Command::SetExternalDriver(crate::SetExternalDriverCommand {
                driver: ports.input_a.external_driver,
                level: LogicLevel::High,
                strength: DriveStrength(100),
            }),
        };

        assert_eq!(
            simulation.step(&[command]),
            Err(SimulationError::NumericOverflow)
        );
        assert_eq!(simulation.state_hash(), before_hash);
        assert_eq!(simulation.canonical.next_tick, before.next_tick);
        assert_eq!(
            simulation.canonical.topology_revision,
            before.topology_revision
        );
        assert_eq!(simulation.canonical.structural, before.structural);
        assert_eq!(simulation.canonical.signal, before.signal);
        assert_eq!(simulation.canonical.event_payloads, before.event_payloads);
        assert_eq!(simulation.canonical.driver_events, before.driver_events);
        assert_eq!(simulation.canonical.signal_events, before.signal_events);
        assert_eq!(
            simulation.canonical.path_certificates,
            before.path_certificates
        );
    }

    #[test]
    fn topology_sync_due_tick_overflow_rolls_back_phase0_and_both_allocators() {
        const CIRCUIT_PITCH: i64 = 16_384;
        let mut simulation = Simulation::new(package()).expect("test package is valid");
        let bounds = crate::FixedAabb::new(
            crate::FixedVec2::new(
                crate::Fixed(-128 * crate::FIXED_ONE),
                crate::Fixed(-128 * crate::FIXED_ONE),
            ),
            crate::FixedVec2::new(
                crate::Fixed(128 * crate::FIXED_ONE),
                crate::Fixed(128 * crate::FIXED_ONE),
            ),
        );
        let substrate_report = simulation
            .step(&[crate::CommandEnvelope {
                target_tick: simulation.next_tick(),
                ordinal: 0,
                command: crate::Command::PlaceFixedSubstrate(crate::PlaceFixedSubstrateCommand {
                    origin: crate::FixedVec2::new(crate::Fixed(0), crate::Fixed(0)),
                    routing_area: bounds,
                    footprint: bounds,
                }),
            }])
            .expect("test Substrate placement succeeds");
        let substrate = substrate_report.command_acceptances[0]
            .created_entity
            .expect("Substrate placement allocates an EntityId");
        let place_gate = |tick, origin| crate::CommandEnvelope {
            target_tick: tick,
            ordinal: 0,
            command: crate::Command::PlaceGate(crate::PlaceGateCommand {
                gate_type: GateType::Not,
                origin,
                routing_domain: crate::RoutingDomain::FixedSubstrate(substrate),
            }),
        };
        let source_report = simulation
            .step(&[place_gate(
                simulation.next_tick(),
                crate::FixedVec2::new(crate::Fixed(0), crate::Fixed(0)),
            )])
            .expect("source Gate placement succeeds");
        let source = GateId(
            source_report.command_acceptances[0]
                .created_entity
                .expect("source Gate allocates an EntityId"),
        );
        let downstream_report = simulation
            .step(&[place_gate(
                simulation.next_tick(),
                crate::FixedVec2::new(crate::Fixed(34 * CIRCUIT_PITCH), crate::Fixed(0)),
            )])
            .expect("downstream Gate placement succeeds");
        let downstream = GateId(
            downstream_report.command_acceptances[0]
                .created_entity
                .expect("downstream Gate allocates an EntityId"),
        );
        for _ in 0..8 {
            let source_state = simulation
                .gate_signal_state(source)
                .expect("source Gate remains live");
            let downstream_state = simulation
                .gate_signal_state(downstream)
                .expect("downstream Gate remains live");
            if source_state.current_output == LogicLevel::High
                && source_state.pending_due_tick.is_none()
                && downstream_state.current_output == LogicLevel::High
                && downstream_state.pending_due_tick.is_none()
            {
                break;
            }
            simulation.step(&[]).expect("Gate settling Tick succeeds");
        }
        assert!(
            [source, downstream].into_iter().all(|gate| {
                let state = simulation
                    .gate_signal_state(gate)
                    .expect("test Gate remains live");
                state.current_output == LogicLevel::High && state.pending_due_tick.is_none()
            }),
            "both NOT Gates must be settled before the near-maximum Tick seam"
        );

        simulation.canonical.next_tick = Tick(u64::MAX - 1);
        validate_canonical_world(&simulation.canonical)
            .expect("the settled near-maximum Tick world is canonical");
        let before_hash = simulation.state_hash();
        let before = simulation.canonical.clone();
        let wire = crate::CommandEnvelope {
            target_tick: simulation.next_tick(),
            ordinal: 0,
            command: crate::Command::PlaceWire(crate::PlaceWireCommand {
                routing_domain: crate::RoutingDomain::FixedSubstrate(substrate),
                points: vec![
                    crate::FixedVec2::new(crate::Fixed(CIRCUIT_PITCH), crate::Fixed(0)),
                    crate::FixedVec2::new(crate::Fixed(33 * CIRCUIT_PITCH), crate::Fixed(0)),
                ],
                endpoint_a: crate::EndpointTarget::GatePort(crate::GatePortRef {
                    gate: source,
                    port: crate::GatePort::Output,
                }),
                endpoint_b: crate::EndpointTarget::GatePort(crate::GatePortRef {
                    gate: downstream,
                    port: crate::GatePort::InputA,
                }),
            }),
        };

        assert_eq!(
            simulation.step(&[wire]),
            Err(SimulationError::NumericOverflow)
        );
        assert_eq!(simulation.state_hash(), before_hash);
        assert_eq!(simulation.canonical.next_tick, before.next_tick);
        assert_eq!(
            simulation.canonical.topology_revision,
            before.topology_revision
        );
        assert_eq!(simulation.canonical.structural, before.structural);
        assert_eq!(simulation.canonical.signal, before.signal);
        assert_eq!(simulation.canonical.event_payloads, before.event_payloads);
        assert_eq!(simulation.canonical.driver_events, before.driver_events);
        assert_eq!(simulation.canonical.signal_events, before.signal_events);
        assert_eq!(
            simulation.canonical.path_certificates,
            before.path_certificates
        );
    }

    #[test]
    fn stored_revision_conflict_is_fatal_even_with_a_higher_winner_and_permutations() {
        fn seeded(reverse: bool) -> Simulation {
            let mut simulation = Simulation::new(package()).expect("test package is valid");
            let gate = place_test_not(&mut simulation);
            simulation
                .step(&[])
                .expect("the initial NOT transition settles");
            let ports = simulation
                .gate_signal_ports(gate)
                .expect("test Gate signal ports exist");
            simulation
                .canonical
                .signal
                .force_driver_revision_for_test(ports.input_a.external_driver, Revision(4))
                .expect("test-only Revision seed succeeds");
            let current = simulation
                .driver_sample(ports.input_a.external_driver)
                .expect("external Driver sample exists");
            let stored = DriverSample {
                revision: Revision(3),
                emitted_at: Tick(0),
                ..current
            };
            assert_eq!(
                simulation
                    .canonical
                    .signal
                    .apply_slot_sample(ports.input_a.sink, stored),
                Ok(SlotApplyOutcome::Applied)
            );
            simulation
                .canonical
                .signal
                .resolve_dirty(simulation.profiles.balance.logic_threshold)
                .expect("test Slot settles");

            let conflicting = DriverSample {
                strength: DriveStrength(1),
                ..stored
            };
            let mut candidates = vec![
                UncertifiedSignalArrival::propagation(
                    simulation.next_tick(),
                    ports.input_a.external_driver,
                    ports.input_a.sink,
                    conflicting,
                    Vec::new(),
                ),
                UncertifiedSignalArrival::topology_sync(
                    simulation.next_tick(),
                    ports.input_a.external_driver,
                    ports.input_a.sink,
                    current,
                    Vec::new(),
                ),
            ];
            if reverse {
                candidates.reverse();
            }
            stage_signal_arrivals(
                &mut simulation.canonical.signal_events,
                &mut simulation.canonical.event_payloads,
                &mut simulation.canonical.path_certificates,
                candidates,
            )
            .expect("test Arrivals stage");
            validate_canonical_world(&simulation.canonical)
                .expect("the conflict is a due-time, not commit-shape, invariant");
            simulation
        }

        let mut first = seeded(false);
        let mut reversed = seeded(true);
        let first_before = first.state_hash();
        let reversed_before = reversed.state_hash();
        assert_eq!(first_before, reversed_before);

        assert_eq!(
            first.step(&[]),
            Err(SimulationError::DriverRevisionInvariantViolation)
        );
        assert_eq!(
            reversed.step(&[]),
            Err(SimulationError::DriverRevisionInvariantViolation)
        );
        assert_eq!(first.state_hash(), first_before);
        assert_eq!(reversed.state_hash(), reversed_before);
        assert_eq!(first.state_hash(), reversed.state_hash());
    }

    #[test]
    fn committed_validator_rejects_orphan_consumed_and_duplicate_certificates() {
        fn pending_simulation() -> Simulation {
            let mut simulation = Simulation::new(package()).expect("test package is valid");
            let gate = place_test_not(&mut simulation);
            simulation
                .step(&[])
                .expect("the initial NOT transition settles");
            let ports = simulation
                .gate_signal_ports(gate)
                .expect("test Gate signal ports exist");
            let sample = simulation
                .driver_sample(ports.input_a.external_driver)
                .expect("external Driver sample exists");
            let due_tick = simulation
                .next_tick()
                .checked_add(Tick(1))
                .expect("test due Tick fits");
            stage_signal_arrivals(
                &mut simulation.canonical.signal_events,
                &mut simulation.canonical.event_payloads,
                &mut simulation.canonical.path_certificates,
                [UncertifiedSignalArrival::propagation(
                    due_tick,
                    ports.input_a.external_driver,
                    ports.input_a.sink,
                    sample,
                    Vec::new(),
                )],
            )
            .expect("test Arrival stages");
            validate_canonical_world(&simulation.canonical)
                .expect("single pending certified Arrival is canonical");
            simulation
        }

        let mut orphan = pending_simulation();
        orphan
            .canonical
            .path_certificates
            .allocate_batch(&[&[]])
            .expect("orphan test Certificate allocates");
        assert_eq!(
            validate_canonical_world(&orphan.canonical),
            Err(SimulationError::PathCertificateInvariantViolation)
        );

        let mut consumed = pending_simulation();
        let certificate = consumed
            .canonical
            .signal_events
            .canonical_view()
            .next()
            .and_then(|event| event.path_certificate)
            .expect("test Arrival owns a Certificate");
        consumed
            .canonical
            .path_certificates
            .consume(certificate)
            .expect("test Certificate consumes");
        assert_eq!(
            validate_canonical_world(&consumed.canonical),
            Err(SimulationError::PathCertificateInvariantViolation)
        );

        let mut duplicate = pending_simulation();
        let original = *duplicate
            .canonical
            .signal_events
            .canonical_view()
            .next()
            .expect("test Arrival exists");
        let payload_order = duplicate.canonical.event_payloads.next_payload_order();
        duplicate.canonical.event_payloads = EventPayloadAllocator::from_next_payload_order(
            payload_order
                .checked_add(1)
                .expect("test payload frontier fits"),
        )
        .expect("test payload frontier is valid");
        let mut second = original;
        second.key.due_tick = second
            .key
            .due_tick
            .checked_add(Tick(1))
            .expect("test due Tick fits");
        second.key.payload_order = payload_order;
        duplicate
            .canonical
            .signal_events
            .insert_assigned(second)
            .expect("calendar keys remain distinct");
        assert_eq!(
            validate_canonical_world(&duplicate.canonical),
            Err(SimulationError::PathCertificateInvariantViolation)
        );
    }

    #[test]
    fn certificate_and_payload_exhaustion_roll_back_phase0_allocations() {
        fn substrate_only() -> Simulation {
            let mut simulation = Simulation::new(package()).expect("test package is valid");
            let bounds = crate::FixedAabb::new(
                crate::FixedVec2::new(
                    crate::Fixed(-4 * crate::FIXED_ONE),
                    crate::Fixed(-4 * crate::FIXED_ONE),
                ),
                crate::FixedVec2::new(
                    crate::Fixed(4 * crate::FIXED_ONE),
                    crate::Fixed(4 * crate::FIXED_ONE),
                ),
            );
            simulation
                .step(&[crate::CommandEnvelope {
                    target_tick: Tick(0),
                    ordinal: 0,
                    command: crate::Command::PlaceFixedSubstrate(
                        crate::PlaceFixedSubstrateCommand {
                            origin: crate::FixedVec2::new(crate::Fixed(0), crate::Fixed(0)),
                            routing_area: bounds,
                            footprint: bounds,
                        },
                    ),
                }])
                .expect("test Substrate placement succeeds");
            simulation
        }

        fn gate_command(tick: Tick) -> crate::CommandEnvelope {
            crate::CommandEnvelope {
                target_tick: tick,
                ordinal: 0,
                command: crate::Command::PlaceGate(crate::PlaceGateCommand {
                    gate_type: GateType::Not,
                    origin: crate::FixedVec2::new(crate::Fixed(0), crate::Fixed(0)),
                    routing_domain: crate::RoutingDomain::FixedSubstrate(crate::EntityId(1)),
                }),
            }
        }

        fn assert_rollback(simulation: &Simulation, before: &CanonicalWorld, hash: StateHash) {
            assert_eq!(simulation.state_hash(), hash);
            assert_eq!(simulation.canonical.next_tick, before.next_tick);
            assert_eq!(
                simulation.canonical.topology_revision,
                before.topology_revision
            );
            assert_eq!(simulation.canonical.structural, before.structural);
            assert_eq!(simulation.canonical.signal, before.signal);
            assert_eq!(simulation.canonical.event_payloads, before.event_payloads);
            assert_eq!(simulation.canonical.driver_events, before.driver_events);
            assert_eq!(simulation.canonical.signal_events, before.signal_events);
            assert_eq!(
                simulation.canonical.path_certificates,
                before.path_certificates
            );
        }

        let mut certificate_exhausted = substrate_only();
        let certificate_frontier = certificate_exhausted
            .canonical
            .path_certificates
            .frontier()
            .0;
        certificate_exhausted
            .canonical
            .path_certificates
            .set_frontier_limits_for_test(certificate_frontier, u32::MAX);
        validate_canonical_world(&certificate_exhausted.canonical)
            .expect("the bounded allocator seam does not alter canonical shape");
        let before = certificate_exhausted.canonical.clone();
        let hash = certificate_exhausted.state_hash();
        assert_eq!(
            certificate_exhausted.step(&[gate_command(certificate_exhausted.next_tick())]),
            Err(SimulationError::NumericOverflow)
        );
        assert_rollback(&certificate_exhausted, &before, hash);

        let mut payload_exhausted = substrate_only();
        payload_exhausted.canonical.event_payloads =
            EventPayloadAllocator::from_next_payload_order(u64::MAX)
                .expect("maximum payload frontier is representable");
        validate_canonical_world(&payload_exhausted.canonical)
            .expect("the exhausted payload frontier is canonical before allocation");
        let before = payload_exhausted.canonical.clone();
        let hash = payload_exhausted.state_hash();
        assert_eq!(
            payload_exhausted.step(&[gate_command(payload_exhausted.next_tick())]),
            Err(SimulationError::NumericOverflow)
        );
        assert_rollback(&payload_exhausted, &before, hash);
    }

    #[test]
    fn contract_hash_mismatch_rejects_simulation_start() {
        let mut package = package();
        package.contract.balance_profile_hash = crate::ProfileHash::default();

        assert!(matches!(
            Simulation::new(package),
            Err(SimulationError::ProfileHashMismatch {
                profile: ProfileKind::Balance,
                ..
            })
        ));
    }

    #[test]
    fn committed_validator_rejects_dirty_sinks_and_orphan_wire_signal_records() {
        let mut dirty = Simulation::new(package()).expect("test package is valid");
        let gate = place_test_not(&mut dirty);
        let ports = dirty
            .gate_signal_ports(gate)
            .expect("test Gate signal ports exist");
        assert!(
            dirty
                .canonical
                .signal
                .remove_route_slot(ports.input_a.sink, ports.input_a.external_driver)
                .expect("test slot removal succeeds")
        );
        assert_eq!(
            validate_canonical_world(&dirty.canonical),
            Err(SimulationError::InvalidCanonicalState)
        );

        let mut orphan = Simulation::new(package()).expect("test package is valid");
        orphan
            .canonical
            .signal
            .activate_wire(WireId(crate::EntityId(99)))
            .expect("test-only orphan Wire signal record inserts");
        assert_eq!(
            validate_canonical_world(&orphan.canonical),
            Err(SimulationError::InvalidCanonicalState)
        );
    }

    #[test]
    fn committed_validator_rejects_sink_driver_slot_key_mismatch() {
        let mut simulation = Simulation::new(package()).expect("test package is valid");
        let gate = place_test_not(&mut simulation);
        let ports = simulation
            .gate_signal_ports(gate)
            .expect("test Gate signal ports exist");
        simulation
            .canonical
            .signal
            .move_slot_key_for_test(
                (ports.input_a.sink, ports.input_a.external_driver),
                (ports.input_a.sink, ports.output),
            )
            .expect("test-only key corruption succeeds");

        assert_eq!(
            validate_canonical_world(&simulation.canonical),
            Err(SimulationError::InvalidCanonicalState)
        );
    }

    #[test]
    fn committed_validator_rejects_gate_map_key_mismatch() {
        let mut simulation = Simulation::new(package()).expect("test package is valid");
        let gate = place_test_not(&mut simulation);
        simulation
            .canonical
            .signal
            .move_gate_key_for_test(gate, GateId(crate::EntityId(gate.entity_id().0 + 1)))
            .expect("test-only Gate key corruption succeeds");

        assert_eq!(
            validate_canonical_world(&simulation.canonical),
            Err(SimulationError::InvalidCanonicalState)
        );
    }

    #[test]
    fn committed_validator_rejects_registry_to_store_index_mismatch() {
        const WORLD_PITCH: i64 = 65_536;
        let mut simulation = Simulation::new(package()).expect("test package is valid");
        let wire = |ordinal, y| crate::CommandEnvelope {
            target_tick: Tick(0),
            ordinal,
            command: crate::Command::PlaceWire(crate::PlaceWireCommand {
                routing_domain: crate::RoutingDomain::OpenWorld,
                points: vec![
                    crate::FixedVec2::new(crate::Fixed(0), crate::Fixed(y)),
                    crate::FixedVec2::new(crate::Fixed(WORLD_PITCH), crate::Fixed(y)),
                ],
                endpoint_a: crate::EndpointTarget::Free,
                endpoint_b: crate::EndpointTarget::Free,
            }),
        };
        simulation
            .step(&[wire(0, 0), wire(1, 2 * WORLD_PITCH)])
            .expect("two disjoint OpenWorld Wires place");
        validate_canonical_world(&simulation.canonical)
            .expect("the uncorrupted structural links are canonical");
        let canonical_hash = simulation.state_hash();

        simulation
            .canonical
            .structural
            .swap_wire_registry_locations_for_test(crate::WireIndex(0), crate::WireIndex(1))
            .expect("test-only registry index corruption succeeds");
        assert_eq!(
            simulation.state_hash(),
            canonical_hash,
            "dense registry indices are intentionally excluded from canonical bytes"
        );
        assert_eq!(
            validate_canonical_world(&simulation.canonical),
            Err(SimulationError::InvalidCanonicalState)
        );
    }

    #[test]
    fn committed_validator_rejects_calendar_map_key_payload_key_mismatch() {
        let mut driver_mismatch = Simulation::new(package()).expect("test package is valid");
        place_test_not(&mut driver_mismatch);
        let original = *driver_mismatch
            .canonical
            .driver_events
            .canonical_keys()
            .next()
            .expect("NOT startup retains one Driver event");
        let mut moved = original;
        moved.due_tick = moved
            .due_tick
            .checked_add(Tick(1))
            .expect("test due Tick fits");
        driver_mismatch
            .canonical
            .driver_events
            .move_map_key_for_test(original, moved);
        assert_eq!(
            validate_canonical_world(&driver_mismatch.canonical),
            Err(SimulationError::EventQueueInvariantViolation)
        );

        let mut signal_mismatch = Simulation::new(package()).expect("test package is valid");
        let gate = place_test_not(&mut signal_mismatch);
        let ports = signal_mismatch
            .gate_signal_ports(gate)
            .expect("test Gate signal ports exist");
        let sample = signal_mismatch
            .driver_sample(ports.input_a.external_driver)
            .expect("external Driver sample exists");
        let due_tick = signal_mismatch
            .next_tick()
            .checked_add(Tick(1))
            .expect("test due Tick fits");
        stage_signal_arrivals(
            &mut signal_mismatch.canonical.signal_events,
            &mut signal_mismatch.canonical.event_payloads,
            &mut signal_mismatch.canonical.path_certificates,
            [UncertifiedSignalArrival::propagation(
                due_tick,
                ports.input_a.external_driver,
                ports.input_a.sink,
                sample,
                Vec::new(),
            )],
        )
        .expect("test SignalArrival stages");
        validate_canonical_world(&signal_mismatch.canonical)
            .expect("the uncorrupted pending SignalArrival is canonical");
        let original = *signal_mismatch
            .canonical
            .signal_events
            .canonical_keys()
            .next()
            .expect("one SignalArrival is pending");
        let mut moved = original;
        moved.due_tick = moved
            .due_tick
            .checked_add(Tick(1))
            .expect("test due Tick fits");
        signal_mismatch
            .canonical
            .signal_events
            .move_map_key_for_test(original, moved);
        assert_eq!(
            validate_canonical_world(&signal_mismatch.canonical),
            Err(SimulationError::EventQueueInvariantViolation)
        );
    }

    #[test]
    fn committed_validator_rejects_cross_calendar_payload_reuse_and_invalid_event_keys() {
        let mut duplicate = Simulation::new(package()).expect("test package is valid");
        let gate = place_test_not(&mut duplicate);
        let ports = duplicate
            .gate_signal_ports(gate)
            .expect("test Gate signal ports exist");
        let sample = duplicate
            .driver_sample(ports.input_a.external_driver)
            .expect("external Driver sample exists");
        let certificate = duplicate
            .canonical
            .path_certificates
            .allocate_batch(&[&[]])
            .expect("empty test certificate allocates")[0];
        let mut arrival = SignalArrival::propagation(
            duplicate
                .next_tick()
                .checked_add(Tick(1))
                .expect("test due Tick fits"),
            ports.input_a.external_driver,
            ports.input_a.sink,
            sample,
            certificate,
        );
        let reused_payload = duplicate
            .canonical
            .driver_events
            .canonical_keys()
            .next()
            .expect("NOT startup retains a pending Driver event")
            .payload_order;
        arrival.key = arrival.key.with_payload_order(reused_payload);
        duplicate
            .canonical
            .signal_events
            .insert_assigned(arrival)
            .expect("each calendar accepts its own assigned key");
        assert_eq!(
            validate_canonical_world(&duplicate.canonical),
            Err(SimulationError::EventQueueInvariantViolation)
        );

        let mut invalid_key = Simulation::new(package()).expect("test package is valid");
        let gate = place_test_not(&mut invalid_key);
        let ports = invalid_key
            .gate_signal_ports(gate)
            .expect("test Gate signal ports exist");
        invalid_key.canonical.event_payloads =
            EventPayloadAllocator::from_next_payload_order(3).expect("frontier is nonzero");
        let mut transition = DriverTransition::s0m3(
            invalid_key
                .next_tick()
                .checked_add(Tick(1))
                .expect("test due Tick fits"),
            ports.input_a.external_driver,
            LogicLevel::High,
            DriveStrength(1),
            0,
            DriverTransitionCause::ExternalDriver,
        );
        transition.key = transition.key.with_payload_order(2);
        transition.key.target_id = u64::MAX;
        invalid_key
            .canonical
            .driver_events
            .insert_assigned(transition)
            .expect("calendar-level shape accepts the assigned event");
        assert_eq!(
            validate_canonical_world(&invalid_key.canonical),
            Err(SimulationError::EventQueueInvariantViolation)
        );

        let mut invalid_role = Simulation::new(package()).expect("test package is valid");
        let gate = place_test_not(&mut invalid_role);
        let ports = invalid_role
            .gate_signal_ports(gate)
            .expect("test Gate signal ports exist");
        let transition = DriverTransition::s0m3(
            invalid_role
                .next_tick()
                .checked_add(Tick(1))
                .expect("test due Tick fits"),
            ports.output,
            LogicLevel::High,
            DriveStrength(1),
            0,
            DriverTransitionCause::ExternalDriver,
        );
        invalid_role
            .canonical
            .driver_events
            .stage(&mut invalid_role.canonical.event_payloads, [transition])
            .expect("calendar-level shape accepts the cross-store role mismatch");
        assert_eq!(
            validate_canonical_world(&invalid_role.canonical),
            Err(SimulationError::InvalidCanonicalState)
        );
    }

    #[test]
    fn due_driver_transition_key_shape_uses_the_event_queue_error_taxonomy() {
        let mut simulation = Simulation::new(package()).expect("test package is valid");
        let gate = place_test_not(&mut simulation);
        let ports = simulation
            .gate_signal_ports(gate)
            .expect("test Gate signal ports exist");
        let tick = simulation.next_tick();
        let valid = DriverTransition::s0m3(
            tick,
            ports.input_a.external_driver,
            LogicLevel::High,
            DriveStrength(1),
            0,
            DriverTransitionCause::ExternalDriver,
        );
        let mut malformed = [valid; 4];
        malformed[0].key.due_tick = tick.checked_add(Tick(1)).expect("test Tick fits");
        malformed[1].key.target_id = u64::MAX;
        malformed[2].key.source_id = u64::MAX;
        malformed[3].key.generation = 1;

        for event in malformed {
            assert!(matches!(
                validate_driver_transition(&simulation.canonical, event, tick),
                Err(SimulationError::EventQueueInvariantViolation)
            ));
        }
    }

    #[test]
    fn unsupported_later_stage_feature_rejects_simulation_start() {
        let mut package = package();
        package.required_features.mobility = true;
        package.required_features.capacity = true;

        assert_eq!(
            Simulation::new(package).err(),
            Some(SimulationError::UnsupportedStageFeature {
                feature: "capacity"
            })
        );
    }

    #[test]
    fn mobile_commit_order_and_hash_are_independent_of_mobile_store_layout() {
        let mut canonical = Simulation::new(package()).expect("test package is valid");
        let pitch = canonical.profiles().physical_scale.world_routing_pitch;
        let circuit_pitch = canonical.profiles().physical_scale.circuit_routing_pitch;
        let point = |x: i64, y: i64| crate::FixedVec2::new(crate::Fixed(x), crate::Fixed(y));
        canonical
            .step(&[crate::CommandEnvelope {
                target_tick: canonical.next_tick(),
                ordinal: 0,
                command: crate::Command::PlaceWire(crate::PlaceWireCommand {
                    routing_domain: crate::RoutingDomain::OpenWorld,
                    points: vec![point(0, 0), point(8 * pitch.0, 0)],
                    endpoint_a: crate::EndpointTarget::Free,
                    endpoint_b: crate::EndpointTarget::Free,
                }),
            }])
            .expect("track placement succeeds");
        let local_bounds = crate::FixedAabb::new(
            point(-4 * circuit_pitch.0, -4 * circuit_pitch.0),
            point(4 * circuit_pitch.0, 4 * circuit_pitch.0),
        );
        canonical
            .step(&[
                crate::CommandEnvelope {
                    target_tick: canonical.next_tick(),
                    ordinal: 0,
                    command: crate::Command::PlaceMobileSubstrate(
                        crate::PlaceMobileSubstrateCommand {
                            origin: point(pitch.0, 0),
                            routing_area: local_bounds,
                            footprint: local_bounds,
                        },
                    ),
                },
                crate::CommandEnvelope {
                    target_tick: canonical.next_tick(),
                    ordinal: 1,
                    command: crate::Command::PlaceMobileSubstrate(
                        crate::PlaceMobileSubstrateCommand {
                            origin: point(2 * pitch.0, 0),
                            routing_area: local_bounds,
                            footprint: local_bounds,
                        },
                    ),
                },
            ])
            .expect("two Mobile placements succeed");

        let mut reordered = Simulation {
            scenario_id: canonical.scenario_id.clone(),
            canonical: canonical.canonical.clone(),
            profiles: canonical.profiles.clone(),
            initial_state_hash: canonical.initial_state_hash,
        };
        reordered
            .canonical
            .structural
            .reserve_layout_capacity_for_test(128);
        reordered
            .canonical
            .structural
            .swap_mobile_substrate_slots_for_test(MobileSubstrateIndex(0), MobileSubstrateIndex(1))
            .expect("test-only Mobile slots swap");
        validate_canonical_world(&reordered.canonical).expect("reordered layout remains valid");
        assert_eq!(canonical.state_hash(), reordered.state_hash());

        let canonical_report = canonical.step(&[]).expect("canonical layout moves");
        let reordered_report = reordered.step(&[]).expect("reordered layout moves");
        assert_eq!(canonical_report, reordered_report);
        assert_eq!(canonical.state_hash(), reordered.state_hash());
        assert!(
            canonical_report
                .mobile_movements
                .windows(2)
                .all(|pair| pair[0].mobile < pair[1].mobile),
            "Phase 11 commits and reports Mobiles in stable MobileId order"
        );
    }

    #[test]
    fn valid_simulation_exposes_immutable_contract_and_profiles() {
        let simulation = Simulation::new(package()).expect("test package is valid");

        assert_eq!(
            simulation.contract().semantics_version,
            SemanticsVersion::AonV1
        );
        assert_eq!(simulation.profiles().numeric.fixed_one, crate::FIXED_ONE);
        assert_eq!(simulation.topology_revision(), Revision(0));
    }
}
