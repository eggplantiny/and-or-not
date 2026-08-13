use crate::capacity::{account_network_with_support, analyzer_snapshot};
use crate::construction::{
    ConstructionSiteStore, ConstructionWorkContribution, apply_construction_work,
    construction_nominal_demand_for_work, grant_construction_work,
};
use crate::contact::{
    ContactCandidate, LiveWireInput, allocate_contact_energy, calculate_live_wire_demand,
    swept_circle_intersects_point, swept_circle_intersects_wire_body,
};
use crate::contract::{ContractValidationError, SimulationContract};
use crate::enemy::{EnemyState, EnemyStore};
use crate::event::{
    DRIVER_TRANSITION_KIND_ORDER, DriverTransition, DriverTransitionCause, EventCalendar,
    EventCalendarError, EventPayloadAllocator, SIGNAL_ARRIVAL_KIND_ORDER, SignalArrival,
    SignalArrivalKind, SignalArrivalStagingError, UncertifiedSignalArrival, stage_signal_arrivals,
};
use crate::main_core::MainCoreState;
use crate::mobility::{
    MobileControlSample, MobileMovementObservation, TrackGraph, TrackGraphError, TrackPosition,
};
use crate::path_certificate::{PathCertificateArena, PathCertificateError};
use crate::power::{
    DemandId, DemandKind, PowerError, PowerRatio, PowerSourceState, brownout_gate_delay,
    scale_drive, scale_movement,
};
use crate::power_adapter::{PowerAdapterError, compile_power_topology_with_loads};
use crate::power_runtime::{
    GatePowerDemandInput, MovementPowerDemandInput, NominalPowerDemandSet, PowerGateReport,
    PowerHeatReport, PowerMobileReport, PowerRuntimeError, PowerSenseAnalyzerSnapshot,
    PowerSenseReport, PowerStepReport, WirePowerDemandInput,
    collect_nominal_power_demands_with_capacity_support,
    solve_power_step_with_capacity_support_heat,
};
use crate::power_source::PowerSourceStore;
use crate::profile::{
    BalanceProfile, PhysicalScaleProfile, ProfileBundle, ProfileValidationError, Rational,
};
use crate::replay::{
    ReplayFormatVersion, ReplayHeader, Seed, StateHashVersion, WorldGeneratorVersion,
    WorldInputEvent,
};
use crate::sensing::{HostileCollider, WireSensingInput, WireSensingOutput, sample_wire_sensing};
use crate::signal::{
    DriverChangeRecord, DriverRole, GateSignalPorts, GateSignalSnapshot, SignalChangeRecord,
    SignalError, SignalStepCounters, SignalWorld, SinkRole, SlotApplyOutcome, WireSignalSnapshot,
};
use crate::signal_topology::{
    CompiledSignalTopology, RouteDiff, SignalTopologyError, switch_energy,
};
use crate::snapshot::{RenderSnapshotSource, SignalProbeSample, SignalProbeTarget, sample_signal};
use crate::structural::{
    PowerSourceAnchorView, StructuralCommandContext, StructuralDestructionKind, StructuralError,
    StructuralPhaseReport, StructuralWorld,
};
use crate::thermal_damage::{
    DamageSnapshot, ElectricalExposure, HeatContributionInput, HeatContributionKey,
    InteractionHeatKind, ThermalObjectKind, integrate_heat, resolve_damage, thermal_capacity_for,
};
use crate::{
    CommandAcceptance, CommandEnvelope, CommandRejection, DriveStrength, DriverId, DriverSample,
    Energy, EntityId, Fixed, FixedVec2, GateId, GateType, HeatEnergy, InitialWorld, Integrity,
    LogicLevel, MobileId, MobileSubstrateIndex, RenderSnapshot, Revision, ScenarioManifest,
    SimulationError, SinkId, StageFeatureSet, StateHash, Tick, WireEnd, WireId, canonical,
    polyline_length,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RunStatus {
    #[default]
    Running,
    Ended {
        completed_tick: Tick,
        cause: RunEndCause,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RunEndCause {
    MainCoreDestroyed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConstructionWorkReport {
    pub site: crate::ConstructionSiteId,
    pub builder: MobileId,
    pub requested: Energy,
    pub nominal_power: Energy,
    pub granted_work: Energy,
    pub applied_work: Energy,
    pub completed_work: Energy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContactEnergyReport {
    pub wire: WireId,
    pub target: crate::EnemyId,
    pub weight: u128,
    pub absorbed: Energy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InteractionHeatReport {
    pub owner: EntityId,
    pub kind: crate::InteractionHeatKind,
    pub demand: Option<DemandId>,
    pub energy: HeatEnergy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DamageReport {
    pub target: EntityId,
    pub electrical_exposure: Energy,
    pub electrical_damage: Integrity,
    pub thermal_damage: Integrity,
    pub integrity_before: Integrity,
    pub integrity_after: Integrity,
    pub pending_destruction: bool,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DestructionKind {
    Damage = 0,
    TrackSupportLost = 1,
    SubstrateSupportLost = 2,
    ConstructionDependencyLost = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DestructionReport {
    pub target: EntityId,
    pub kind: DestructionKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DamageAnalyzerRecord {
    pub target: EntityId,
    pub kind: ThermalObjectKind,
    pub integrity: Integrity,
    pub heat_energy: HeatEnergy,
    pub temperature: Fixed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArmedWireAnalyzerRecord {
    pub wire: WireId,
    pub nominal_live_energy: Energy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstructionContactDamageAnalyzerSnapshot {
    pub next_tick: Tick,
    pub construction_sites: Vec<crate::ConstructionSite>,
    pub enemies: Vec<EnemyState>,
    pub damage: Vec<DamageAnalyzerRecord>,
    pub armed_wires: Vec<ArmedWireAnalyzerRecord>,
    pub run_status: RunStatus,
}

/// Read-only counts that distinguish a stable signal state from one with deferred work.
///
/// This is derived observation data: reading it cannot advance a Tick or mutate canonical State.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SignalQuiescenceSnapshot {
    pub next_tick: Tick,
    pub pending_driver_transitions: u64,
    pub pending_signal_arrivals: u64,
    pub pending_gate_transitions: u64,
}

impl SignalQuiescenceSnapshot {
    pub const fn is_quiescent(self) -> bool {
        self.pending_driver_transitions == 0
            && self.pending_signal_arrivals == 0
            && self.pending_gate_transitions == 0
    }
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
    pub network_accounting: Option<crate::NetworkAccounting>,
    pub power: Option<PowerStepReport>,
    pub construction_work: Vec<ConstructionWorkReport>,
    pub contacts: Vec<ContactEnergyReport>,
    pub interaction_heat: Vec<InteractionHeatReport>,
    pub damage: Vec<DamageReport>,
    pub destructions: Vec<DestructionReport>,
    pub run_status: RunStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MobileIntent {
    index: MobileSubstrateIndex,
    mobile: MobileId,
    start: TrackPosition,
    start_world_point: FixedVec2,
    controls: MobileControlSample,
    granted_budget: Fixed,
    power_attachment: Option<(WireId, Fixed)>,
    construction: Option<ConstructionIntent>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ConstructionIntent {
    site: crate::ConstructionSiteId,
    builder: MobileId,
    requested: Energy,
    nominal_power: Energy,
    granted_work: Energy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MobilePhase1Snapshot {
    index: MobileSubstrateIndex,
    mobile: MobileId,
    start: TrackPosition,
    world_point: FixedVec2,
    power_attachment: Option<(WireId, Fixed)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EnemyPhase1Snapshot {
    enemy: crate::EnemyId,
    position: FixedVec2,
    velocity_per_tick: FixedVec2,
    radius: Fixed,
    integrity: Integrity,
    heat_energy: HeatEnergy,
    temperature: Fixed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CoreDamagePhase1Snapshot {
    target: EntityId,
    integrity: Integrity,
    temperature: Fixed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StagedEnemyTrajectory {
    enemy: crate::EnemyId,
    start: FixedVec2,
    end: FixedVec2,
    radius: Fixed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LiveWireIntent {
    wire: WireId,
    nominal: Energy,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Phase3Output {
    mobile_intents: Vec<MobileIntent>,
    live_wires: Vec<LiveWireIntent>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Phase1Snapshot {
    mobiles: Vec<MobilePhase1Snapshot>,
    wire_sensing: Vec<WireSensingOutput>,
    enemies: Vec<EnemyPhase1Snapshot>,
    core: Option<CoreDamagePhase1Snapshot>,
    structural_damage: Vec<DamageSnapshot>,
}

struct Phase0Output {
    structural_report: StructuralPhaseReport,
    destructions: Vec<DestructionReport>,
    topology: CompiledSignalTopology,
    track_graph: TrackGraph,
    signal_counters: SignalStepCounters,
}

struct Phase4Output {
    network_accounting: Option<crate::NetworkAccounting>,
    nominal_power: Option<NominalPowerDemandSet>,
}

/// Phase-5 Power output before Phase 8 publishes derived Heat Contributions.
///
/// Keeping heat outside `report` makes the Phase-8 ownership boundary explicit even though the
/// pure Power solver also remains useful as a complete standalone kernel.
#[derive(Default)]
struct Phase5Output {
    report: Option<PowerStepReport>,
    private_heat: Vec<PowerHeatReport>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Phase8Output {
    contacts: Vec<ContactEnergyReport>,
    interaction_heat: Vec<InteractionHeatReport>,
    exposures: Vec<ElectricalExposure>,
    construction: Vec<ConstructionIntent>,
    construction_contributions: Vec<ConstructionWorkContribution>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Phase10Output {
    damage: Vec<DamageReport>,
    core_destroyed: bool,
}

struct Phase6Inputs<'a> {
    topology: &'a CompiledSignalTopology,
    tick: Tick,
    balance: &'a BalanceProfile,
    movement_budget: Fixed,
    wire_sensing: &'a [WireSensingOutput],
    power_report: Option<&'a mut PowerStepReport>,
}

struct Phase8Inputs<'a> {
    staged_mobiles: &'a [StagedMobileMovement],
    mobile_intents: &'a [MobileIntent],
    staged_enemies: &'a [StagedEnemyTrajectory],
    live_wires: &'a [LiveWireIntent],
    track_graph: &'a TrackGraph,
    physical: &'a PhysicalScaleProfile,
    balance: &'a BalanceProfile,
}

struct Phase11Inputs<'a> {
    completed_tick: Tick,
    next_tick: Tick,
    staged_mobiles: Vec<StagedMobileMovement>,
    staged_enemies: Vec<StagedEnemyTrajectory>,
    construction: &'a [ConstructionIntent],
    construction_contributions: &'a [ConstructionWorkContribution],
    core_destroyed: bool,
}

struct Phase11Output {
    state_hash: StateHash,
    mobile_movements: Vec<MobileMovementObservation>,
    run_status: RunStatus,
    construction_work: Vec<ConstructionWorkReport>,
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
    main_core: Option<MainCoreState>,
    power_sources: PowerSourceStore,
    enemies: EnemyStore,
    construction_sites: ConstructionSiteStore,
    pending_destructions: BTreeSet<EntityId>,
    run_status: RunStatus,
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
            main_core: self.main_core.as_ref(),
            power_sources: &self.power_sources,
            enemies: &self.enemies,
            construction_sites: &self.construction_sites,
            pending_destructions: &self.pending_destructions,
            run_status: self.run_status,
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
    required_features: StageFeatureSet,
    initial_state_hash: StateHash,
    world_generator_version: WorldGeneratorVersion,
}

fn validate_s1m4_initial_world_precedence(
    initial_world: &InitialWorld,
    physical_scale: &PhysicalScaleProfile,
) -> Result<(), SimulationError> {
    let InitialWorld::MainCorePowerEnemyV1 {
        main_core_position,
        main_core_integrity,
        power_sources,
        enemies,
        ..
    } = initial_world
    else {
        return Ok(());
    };

    if main_core_integrity.0 == 0 {
        return Err(SimulationError::InvalidMainCoreIntegrity);
    }

    if power_sources
        .iter()
        .any(|source| source.generation_per_tick().0 == 0)
    {
        return Err(SimulationError::InvalidCanonicalState);
    }
    let source_positions = power_sources
        .iter()
        .map(|source| (source.position().x.0, source.position().y.0))
        .collect::<BTreeSet<_>>();
    if source_positions.len() != power_sources.len() {
        return Err(SimulationError::InvalidCanonicalState);
    }

    if enemies.is_empty() {
        return Err(SimulationError::InvalidCanonicalState);
    }
    for enemy in enemies {
        if enemy.radius().0 <= 0 || enemy.integrity().0 == 0 {
            return Err(SimulationError::InvalidCanonicalState);
        }
        enemy.checked_next_position()?;
    }
    let enemy_keys = enemies
        .iter()
        .map(crate::artifact::enemy_semantic_key)
        .collect::<BTreeSet<_>>();
    if enemy_keys.len() != enemies.len() {
        return Err(SimulationError::InvalidCanonicalState);
    }

    // Profile validation owns a non-positive quantum. With a usable quantum, however, Scenario-v4
    // World alignment is an earlier package fault than any later Profile body/hash fault.
    let quantum = physical_scale.wire_geometry_quantum;
    if quantum.0 <= 0 {
        return Ok(());
    }
    if crate::validate_quantized(*main_core_position, quantum).is_err() {
        return Err(SimulationError::InvalidMainCoreGeometryQuantum);
    }
    if power_sources
        .iter()
        .any(|source| crate::validate_quantized(source.position(), quantum).is_err())
    {
        return Err(SimulationError::InvalidPowerSourceGeometryQuantum);
    }
    if enemies.iter().any(|enemy| {
        let Ok(next_position) = enemy.checked_next_position() else {
            return true;
        };
        crate::validate_quantized(enemy.position(), quantum).is_err()
            || crate::validate_quantized(enemy.velocity_per_tick(), quantum).is_err()
            || enemy.radius().0.rem_euclid(quantum.0) != 0
            || crate::validate_quantized(next_position, quantum).is_err()
    }) {
        return Err(SimulationError::InvalidCanonicalState);
    }

    Ok(())
}

impl Simulation {
    pub fn new(package: SimulationPackage) -> Result<Self, SimulationError> {
        if let Some(feature) = package.required_features.first_unsupported() {
            return Err(SimulationError::UnsupportedStageFeature { feature });
        }

        validate_s1m4_initial_world_precedence(
            &package.initial_world,
            &package.profiles.physical_scale,
        )?;

        package.profiles.validate().map_err(SimulationError::from)?;
        package
            .contract
            .validate_profiles(&package.profiles)
            .map_err(SimulationError::from)?;

        let (structural, main_core, power_sources, enemies, world_generator_version) = match package
            .initial_world
        {
            InitialWorld::Empty => {
                if package.required_features.construction
                    || package.required_features.contact
                    || package.required_features.damage
                {
                    return Err(SimulationError::PowerFeaturesRequireMainCorePowerWorld);
                }
                if package.required_features.capacity {
                    return Err(SimulationError::CapacityRequiresMainCore);
                }
                if package.required_features.sensing || package.required_features.power {
                    return Err(SimulationError::PowerFeaturesRequireMainCorePowerWorld);
                }
                if package.profiles.balance.power_probe.is_some() {
                    return Err(SimulationError::PowerProbeRequiresMainCorePowerWorld);
                }
                (
                    StructuralWorld::new(),
                    None,
                    PowerSourceStore::default(),
                    EnemyStore::default(),
                    WorldGeneratorVersion::EmptyV1,
                )
            }
            InitialWorld::MainCoreV1 {
                position,
                integrity,
                heat_energy,
            } => {
                if package.required_features.construction
                    || package.required_features.contact
                    || package.required_features.damage
                {
                    return Err(SimulationError::PowerFeaturesRequireMainCorePowerWorld);
                }
                if !package.required_features.capacity {
                    return Err(SimulationError::MainCoreRequiresCapacity);
                }
                if package.required_features.sensing || package.required_features.power {
                    return Err(SimulationError::PowerFeaturesRequireMainCorePowerWorld);
                }
                if package.profiles.balance.power_probe.is_some() {
                    return Err(SimulationError::PowerProbeRequiresMainCorePowerWorld);
                }
                let capacity_profile = package
                    .profiles
                    .balance
                    .capacity_probe
                    .ok_or(SimulationError::CapacityRequiresProfile)?;
                if integrity.0 == 0 {
                    return Err(SimulationError::InvalidMainCoreIntegrity);
                }
                if crate::validate_quantized(
                    position,
                    package.profiles.physical_scale.wire_geometry_quantum,
                )
                .is_err()
                {
                    return Err(SimulationError::InvalidMainCoreGeometryQuantum);
                }
                let capacity =
                    crate::Capacity::from_whole_ncu(capacity_profile.main_core_capacity)?;
                let (structural, id) = StructuralWorld::new_with_main_core_registry_entry()?;
                (
                    structural,
                    Some(MainCoreState::new(
                        id,
                        position,
                        capacity,
                        integrity,
                        heat_energy,
                    )),
                    PowerSourceStore::default(),
                    EnemyStore::default(),
                    WorldGeneratorVersion::MainCoreV1,
                )
            }
            InitialWorld::MainCorePowerV1 {
                main_core_position,
                main_core_integrity,
                main_core_heat_energy,
                power_sources,
            } => {
                if package.required_features.construction
                    || package.required_features.contact
                    || package.required_features.damage
                {
                    return Err(SimulationError::PowerFeaturesRequireMainCorePowerWorld);
                }
                if !package.required_features.capacity
                    || !package.required_features.sensing
                    || !package.required_features.power
                {
                    return Err(SimulationError::MainCorePowerRequiresFeatures);
                }
                let capacity_profile = package
                    .profiles
                    .balance
                    .capacity_probe
                    .ok_or(SimulationError::MainCorePowerRequiresProfiles)?;
                package
                    .profiles
                    .balance
                    .power_probe
                    .ok_or(SimulationError::MainCorePowerRequiresProfiles)?;
                if main_core_integrity.0 == 0 {
                    return Err(SimulationError::InvalidMainCoreIntegrity);
                }
                let quantum = package.profiles.physical_scale.wire_geometry_quantum;
                if crate::validate_quantized(main_core_position, quantum).is_err() {
                    return Err(SimulationError::InvalidMainCoreGeometryQuantum);
                }
                if power_sources
                    .iter()
                    .any(|source| crate::validate_quantized(source.position(), quantum).is_err())
                {
                    return Err(SimulationError::InvalidPowerSourceGeometryQuantum);
                }
                let capacity =
                    crate::Capacity::from_whole_ncu(capacity_profile.main_core_capacity)?;
                let (structural, core_id, source_ids) =
                    StructuralWorld::new_with_main_core_and_power_source_registry_entries(
                        power_sources.len(),
                    )?;
                let source_states = source_ids
                    .into_iter()
                    .zip(power_sources)
                    .map(|(id, source)| {
                        PowerSourceState::new(id, source.position(), source.generation_per_tick())
                    })
                    .collect();
                let power_sources = PowerSourceStore::new(source_states)
                    .map_err(|_| SimulationError::InvalidCanonicalState)?;
                (
                    structural,
                    Some(MainCoreState::new(
                        core_id,
                        main_core_position,
                        capacity,
                        main_core_integrity,
                        main_core_heat_energy,
                    )),
                    power_sources,
                    EnemyStore::default(),
                    WorldGeneratorVersion::MainCorePowerV1,
                )
            }
            InitialWorld::MainCorePowerEnemyV1 {
                main_core_position,
                main_core_integrity,
                main_core_heat_energy,
                mut power_sources,
                mut enemies,
            } => {
                if !package.required_features.signal
                    || !package.required_features.mobility
                    || !package.required_features.capacity
                    || !package.required_features.sensing
                    || !package.required_features.power
                    || !package.required_features.construction
                    || !package.required_features.contact
                    || !package.required_features.damage
                {
                    return Err(SimulationError::MainCorePowerRequiresFeatures);
                }
                let capacity_profile = package
                    .profiles
                    .balance
                    .capacity_probe
                    .ok_or(SimulationError::MainCorePowerRequiresProfiles)?;
                package
                    .profiles
                    .balance
                    .power_probe
                    .ok_or(SimulationError::MainCorePowerRequiresProfiles)?;
                package
                    .profiles
                    .balance
                    .construction_probe
                    .ok_or(SimulationError::MainCorePowerRequiresProfiles)?;
                let contact_damage_probe = package
                    .profiles
                    .balance
                    .contact_damage_probe
                    .ok_or(SimulationError::MainCorePowerRequiresProfiles)?;
                if main_core_integrity.0 == 0 {
                    return Err(SimulationError::InvalidMainCoreIntegrity);
                }
                power_sources.sort_unstable_by_key(crate::artifact::power_source_semantic_key);
                if power_sources
                    .iter()
                    .any(|source| source.generation_per_tick().0 == 0)
                    || power_sources
                        .windows(2)
                        .any(|pair| pair[0].position() == pair[1].position())
                {
                    return Err(SimulationError::InvalidCanonicalState);
                }
                if enemies.is_empty() {
                    return Err(SimulationError::InvalidCanonicalState);
                }
                for enemy in &enemies {
                    if enemy.radius().0 <= 0 || enemy.integrity().0 == 0 {
                        return Err(SimulationError::InvalidCanonicalState);
                    }
                    enemy.checked_next_position()?;
                }
                enemies.sort_unstable_by_key(crate::artifact::enemy_semantic_key);
                if enemies.windows(2).any(|pair| pair[0] == pair[1]) {
                    return Err(SimulationError::InvalidCanonicalState);
                }
                let quantum = package.profiles.physical_scale.wire_geometry_quantum;
                if crate::validate_quantized(main_core_position, quantum).is_err() {
                    return Err(SimulationError::InvalidMainCoreGeometryQuantum);
                }
                if power_sources
                    .iter()
                    .any(|source| crate::validate_quantized(source.position(), quantum).is_err())
                {
                    return Err(SimulationError::InvalidPowerSourceGeometryQuantum);
                }
                if enemies.iter().any(|enemy| {
                    let Ok(next_position) = enemy.checked_next_position() else {
                        return true;
                    };
                    crate::validate_quantized(enemy.position(), quantum).is_err()
                        || crate::validate_quantized(enemy.velocity_per_tick(), quantum).is_err()
                        || enemy.radius().0.rem_euclid(quantum.0) != 0
                        || crate::validate_quantized(next_position, quantum).is_err()
                }) {
                    return Err(SimulationError::InvalidCanonicalState);
                }
                let expected_core_integrity =
                    Integrity(contact_damage_probe.initial_integrity.main_core);
                let expected_enemy_integrity =
                    Integrity(contact_damage_probe.initial_integrity.enemy);
                if main_core_integrity != expected_core_integrity {
                    return Err(SimulationError::InitialIntegrityProfileMismatch {
                        entity_kind: "mainCore",
                        expected: expected_core_integrity,
                        actual: main_core_integrity,
                    });
                }
                if let Some(enemy) = enemies
                    .iter()
                    .find(|enemy| enemy.integrity() != expected_enemy_integrity)
                {
                    return Err(SimulationError::InitialIntegrityProfileMismatch {
                        entity_kind: "enemy",
                        expected: expected_enemy_integrity,
                        actual: enemy.integrity(),
                    });
                }
                let capacity =
                    crate::Capacity::from_whole_ncu(capacity_profile.main_core_capacity)?;
                let (structural, core_id, source_ids, enemy_ids) =
                    StructuralWorld::new_with_main_core_power_source_and_enemy_registry_entries(
                        power_sources.len(),
                        enemies.len(),
                    )?;
                let source_states = source_ids
                    .into_iter()
                    .zip(power_sources)
                    .map(|(id, source)| {
                        PowerSourceState::new(id, source.position(), source.generation_per_tick())
                    })
                    .collect();
                let power_sources = PowerSourceStore::new(source_states)
                    .map_err(|_| SimulationError::InvalidCanonicalState)?;
                let enemy_states = enemy_ids
                    .into_iter()
                    .zip(enemies)
                    .map(|(id, enemy)| {
                        EnemyState::new(
                            id,
                            enemy.position(),
                            enemy.velocity_per_tick(),
                            enemy.radius(),
                            enemy.integrity(),
                            enemy.heat_energy(),
                        )
                    })
                    .collect();
                let enemies = EnemyStore::new(enemy_states)
                    .map_err(|_| SimulationError::InvalidCanonicalState)?;
                (
                    structural,
                    Some(MainCoreState::new(
                        core_id,
                        main_core_position,
                        capacity,
                        main_core_integrity,
                        main_core_heat_energy,
                    )),
                    power_sources,
                    enemies,
                    WorldGeneratorVersion::MainCorePowerEnemyV1,
                )
            }
        };

        let canonical = CanonicalWorld {
            next_tick: Tick(0),
            topology_revision: Revision(0),
            contract: package.contract,
            main_core,
            power_sources,
            enemies,
            construction_sites: ConstructionSiteStore::default(),
            pending_destructions: BTreeSet::new(),
            run_status: RunStatus::Running,
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
            required_features: package.required_features,
            initial_state_hash,
            world_generator_version,
        })
    }

    pub fn step(&mut self, commands: &[CommandEnvelope]) -> Result<StepReport, SimulationError> {
        self.step_with_world_inputs(commands, &[])
    }

    pub fn step_with_world_inputs(
        &mut self,
        commands: &[CommandEnvelope],
        world_inputs: &[WorldInputEvent],
    ) -> Result<StepReport, SimulationError> {
        if matches!(self.canonical.run_status, RunStatus::Ended { .. }) {
            return Err(SimulationError::RunEnded);
        }
        let hostiles = validate_world_inputs(world_inputs, self.canonical.next_tick)?;
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
        let phase1 = run_phase1_snapshot_and_world_sample(
            &candidate,
            &phase0.track_graph,
            hostiles,
            self.required_features.sensing,
            self.profiles.balance.sense_radius,
            self.profiles.physical_scale.world_routing_pitch,
            self.profiles.balance.contact_damage_probe.as_ref(),
        )?;

        phases.enter(TickPhase::DriverAndSignalArrival)?;
        let phase2 = run_phase2(
            &mut candidate,
            &phase0.topology,
            completed_tick,
            self.profiles.balance.logic_threshold,
            &mut phase0.signal_counters,
        )?;

        phases.enter(TickPhase::IntentEvaluation)?;
        let mut phase3 = run_phase3(
            &mut candidate,
            &phase1,
            &phase0.track_graph,
            &self.profiles.physical_scale,
            &self.profiles.balance,
        )?;

        phases.enter(TickPhase::GlobalAccountingAndNominalDemand)?;
        let phase4 = run_phase4_global_accounting_and_nominal_demand(
            &candidate,
            &phase0.topology,
            &phase3.mobile_intents,
            &phase3.live_wires,
            &self.profiles.balance,
            self.profiles.physical_scale.world_routing_pitch,
        )?;

        phases.enter(TickPhase::PowerSolveAndBrownout)?;
        let mut phase5 = run_phase5_power_solve_and_brownout(
            &candidate,
            phase4.nominal_power.as_ref(),
            &self.profiles.balance,
        )?;

        phases.enter(TickPhase::SchedulingAndGrantedWork)?;
        run_phase6(
            &mut candidate,
            &mut phase3.mobile_intents,
            Phase6Inputs {
                topology: &phase0.topology,
                tick: completed_tick,
                balance: &self.profiles.balance,
                movement_budget: self.profiles.physical_scale.world_routing_pitch,
                wire_sensing: &phase1.wire_sensing,
                power_report: phase5.report.as_mut(),
            },
        )?;

        phases.enter(TickPhase::Trajectory)?;
        let (staged_mobiles, staged_enemies) = run_phase7(
            &phase0.track_graph,
            &phase3.mobile_intents,
            &phase1.enemies,
            phase5.report.as_ref(),
        )?;

        phases.enter(TickPhase::Interaction)?;
        let phase8 = run_phase8_interaction(
            &candidate,
            &mut phase5,
            Phase8Inputs {
                staged_mobiles: &staged_mobiles,
                mobile_intents: &phase3.mobile_intents,
                staged_enemies: &staged_enemies,
                live_wires: &phase3.live_wires,
                track_graph: &phase0.track_graph,
                physical: &self.profiles.physical_scale,
                balance: &self.profiles.balance,
            },
        )?;

        phases.enter(TickPhase::ThermalIntegration)?;
        run_phase9_thermal_integration(
            &mut candidate,
            &phase8.interaction_heat,
            phase5
                .report
                .as_ref()
                .map(|report| report.heat_contributions.as_slice())
                .unwrap_or(&[]),
            &self.profiles.balance,
        )?;

        phases.enter(TickPhase::DamageResolution)?;
        let phase10 = run_phase10_damage_resolution(
            &mut candidate,
            &phase1,
            &phase8.exposures,
            &self.profiles.balance,
        )?;

        phases.enter(TickPhase::ProgressCommit)?;
        let phase11 = run_phase11_progress_commit(
            &mut candidate,
            Phase11Inputs {
                completed_tick,
                next_tick,
                staged_mobiles,
                staged_enemies,
                construction: &phase8.construction,
                construction_contributions: &phase8.construction_contributions,
                core_destroyed: phase10.core_destroyed,
            },
        )?;
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
            network_accounting: phase4.network_accounting,
            power: phase5.report,
            construction_work: phase11.construction_work,
            contacts: phase8.contacts,
            interaction_heat: phase8.interaction_heat,
            damage: phase10.damage,
            destructions: phase0.destructions,
            run_status: phase11.run_status,
        })
    }

    pub fn write_render_snapshot(&self, output: &mut RenderSnapshot) {
        output.write(RenderSnapshotSource {
            scenario_id: &self.scenario_id,
            next_tick: self.canonical.next_tick,
            topology_revision: self.canonical.topology_revision,
            contract: self.canonical.contract,
            state_hash: self.state_hash(),
            main_core: self.canonical.main_core.as_ref(),
            enemies: &self.canonical.enemies,
            construction_sites: &self.canonical.construction_sites,
            run_status: self.canonical.run_status,
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

    pub const fn run_status(&self) -> RunStatus {
        self.canonical.run_status
    }

    pub const fn construction_sites(&self) -> &ConstructionSiteStore {
        &self.canonical.construction_sites
    }

    pub const fn enemies(&self) -> &EnemyStore {
        &self.canonical.enemies
    }

    pub const fn contract(&self) -> &SimulationContract {
        &self.canonical.contract
    }

    pub fn replay_header(&self) -> ReplayHeader {
        ReplayHeader {
            format_version: ReplayFormatVersion::V2,
            semantics_version: self.canonical.contract.semantics_version,
            numeric_profile_hash: self.canonical.contract.numeric_profile_hash,
            physical_scale_profile_hash: self.canonical.contract.physical_scale_profile_hash,
            balance_profile_hash: self.canonical.contract.balance_profile_hash,
            state_hash_version: StateHashVersion::current(),
            world_generator_version: self.world_generator_version,
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

    pub const fn main_core_state(&self) -> Option<&MainCoreState> {
        self.canonical.main_core.as_ref()
    }

    pub fn power_sources(&self) -> impl ExactSizeIterator<Item = &PowerSourceState> {
        self.canonical.power_sources.iter()
    }

    pub fn power_source_state(&self, id: crate::PowerSourceId) -> Option<&PowerSourceState> {
        self.canonical.power_sources.get(id)
    }

    pub fn network_analyzer_snapshot(
        &self,
    ) -> Result<Option<crate::NetworkAnalyzerSnapshot>, SimulationError> {
        self.canonical
            .main_core
            .as_ref()
            .map(|core| {
                analyzer_snapshot(
                    self.canonical.next_tick,
                    &self.canonical.structural,
                    core,
                    self.profiles.balance.capacity_probe,
                    self.profiles.balance.capacity_support_probe,
                )
            })
            .transpose()
    }

    /// Recomputes persistent Power routes and current Sense/Gate observations without mutation.
    ///
    /// Tick-local Movement intents, hostile samples, and Phase-8 Heat Contributions are excluded.
    pub fn power_sense_analyzer_snapshot(
        &self,
    ) -> Result<Option<PowerSenseAnalyzerSnapshot>, SimulationError> {
        if !self.required_features.power {
            return Ok(None);
        }
        let balance = &self.profiles.balance;
        let probe = balance
            .power_probe
            .ok_or(SimulationError::InvalidCanonicalState)?;
        let signal_topology = CompiledSignalTopology::compile(
            &self.canonical.structural,
            &self.canonical.signal,
            balance,
        )?;
        let gates = collect_gate_power_inputs(&self.canonical, &signal_topology, balance)?;
        let core = self
            .canonical
            .main_core
            .as_ref()
            .ok_or(SimulationError::InvalidCanonicalState)?;
        let accounted = account_network_with_support(
            &self.canonical.structural,
            Some(core),
            balance.capacity_probe,
            balance.capacity_support_probe,
        )?;
        let wires = collect_wire_power_inputs(accounted.wires())?;
        let nominal = collect_nominal_power_demands_with_capacity_support(
            probe,
            &gates,
            &wires,
            &[],
            accounted.support_shares(),
        )?;
        let topology = compile_power_topology_with_loads(
            &self.canonical.structural,
            &self.canonical.power_sources,
            nominal.load_attachments(),
        )?;
        let solved = solve_power_step_with_capacity_support_heat(
            &topology,
            &self.canonical.power_sources,
            &nominal,
            probe,
            active_capacity_probe(balance)?,
        )?;
        let mut senses = collect_power_sense_reports(&self.canonical, &solved, probe)?;
        let mut gates =
            collect_power_gate_reports(&self.canonical, &signal_topology, &solved, balance)?;
        senses.sort_unstable_by_key(|sense| (sense.wire, sense.end));
        gates.sort_unstable_by_key(|gate| gate.gate);
        Ok(Some(PowerSenseAnalyzerSnapshot {
            next_tick: self.canonical.next_tick,
            regions: solved.regions,
            loads: solved.loads,
            senses,
            gates,
        }))
    }

    pub fn construction_contact_damage_analyzer_snapshot(
        &self,
    ) -> Result<Option<ConstructionContactDamageAnalyzerSnapshot>, SimulationError> {
        let Some(probe) = self.profiles.balance.contact_damage_probe.as_ref() else {
            return Ok(None);
        };
        let mut damage = Vec::new();
        if let Some(core) = self.canonical.main_core {
            damage.push(DamageAnalyzerRecord {
                target: core.id().entity_id(),
                kind: ThermalObjectKind::MainCore,
                integrity: core.integrity(),
                heat_energy: core.heat_energy(),
                temperature: phase1_temperature(
                    core.heat_energy(),
                    thermal_capacity_for(ThermalObjectKind::MainCore, probe),
                )?,
            });
        }
        damage.extend(
            self.canonical
                .structural
                .damageable_structural_states()
                .map(|(target, kind, state)| {
                    Ok(DamageAnalyzerRecord {
                        target,
                        kind,
                        integrity: state.integrity,
                        heat_energy: state.heat_energy,
                        temperature: phase1_temperature(
                            state.heat_energy,
                            thermal_capacity_for(kind, probe),
                        )?,
                    })
                })
                .collect::<Result<Vec<_>, SimulationError>>()?,
        );
        damage.extend(
            self.canonical
                .enemies
                .iter()
                .map(|enemy| {
                    Ok(DamageAnalyzerRecord {
                        target: enemy.id().entity_id(),
                        kind: ThermalObjectKind::Enemy,
                        integrity: enemy.integrity(),
                        heat_energy: enemy.heat_energy(),
                        temperature: phase1_temperature(
                            enemy.heat_energy(),
                            thermal_capacity_for(ThermalObjectKind::Enemy, probe),
                        )?,
                    })
                })
                .collect::<Result<Vec<_>, SimulationError>>()?,
        );
        damage.sort_unstable_by_key(|row| row.target);

        let mut armed_wires = self
            .canonical
            .structural
            .wires()
            .iter_alive()
            .filter_map(|(_, wire)| {
                let state = self.canonical.signal.wire_snapshot(wire.id)?;
                let resolved = crate::signal::resolve_drive(
                    state.active,
                    self.profiles.balance.logic_threshold,
                );
                (resolved == LogicLevel::High && state.active.high > 0)
                    .then_some((wire, state.active.high))
            })
            .map(|(wire, strength)| {
                Ok(ArmedWireAnalyzerRecord {
                    wire: wire.id,
                    nominal_live_energy: calculate_live_wire_demand(
                        LiveWireInput {
                            wire: wire.id,
                            length: polyline_length(wire.points)?,
                            high_drive_strength: strength,
                        },
                        probe,
                    )
                    .map_err(|_| SimulationError::InvalidCanonicalState)?,
                })
            })
            .collect::<Result<Vec<_>, SimulationError>>()?;
        armed_wires.sort_unstable_by_key(|row| row.wire);
        Ok(Some(ConstructionContactDamageAnalyzerSnapshot {
            next_tick: self.canonical.next_tick,
            construction_sites: self.canonical.construction_sites.iter().cloned().collect(),
            enemies: self.canonical.enemies.iter().copied().collect(),
            damage,
            armed_wires,
            run_status: self.canonical.run_status,
        }))
    }

    /// Reports every deferred signal mechanism without changing canonical State.
    pub fn signal_quiescence_snapshot(&self) -> Result<SignalQuiescenceSnapshot, SimulationError> {
        Ok(SignalQuiescenceSnapshot {
            next_tick: self.canonical.next_tick,
            pending_driver_transitions: u64::try_from(self.canonical.driver_events.len())
                .map_err(|_| SimulationError::NumericOverflow)?,
            pending_signal_arrivals: u64::try_from(self.canonical.signal_events.len())
                .map_err(|_| SimulationError::NumericOverflow)?,
            pending_gate_transitions: u64::try_from(
                self.canonical
                    .signal
                    .iter_gates()
                    .filter(|gate| gate.pending_due_tick.is_some())
                    .count(),
            )
            .map_err(|_| SimulationError::NumericOverflow)?,
        })
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

    pub fn wire_sense_state(&self, wire: WireId) -> Option<crate::WireSenseSnapshot> {
        self.canonical.signal.wire_sense_snapshot(wire)
    }
}

fn validate_canonical_world(world: &CanonicalWorld) -> Result<(), SimulationError> {
    validate_structural_registry_links(
        &world.structural,
        world.main_core.as_ref(),
        &world.power_sources,
        &world.enemies,
        &world.construction_sites,
    )?;
    validate_damage_lifecycle(world)?;
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
        let ordinary_desired = crate::signal::gate_output(gate.gate_type, input_a, input_b)?;
        let retention_reset = gate.unpowered_ticks > 0
            && gate.desired_output == LogicLevel::Low
            && gate.pending_level == Some(LogicLevel::Low)
            && gate.pending_switch_energy == Some(Energy(0));
        if ordinary_desired != gate.desired_output && !retention_reset {
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
                    || (energy.0 == 0 && level != LogicLevel::Low)
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
        let build_drivers: Vec<_> = driver_slots
            .iter()
            .filter_map(|(driver, record)| {
                record
                    .as_ref()
                    .filter(|record| {
                        record.owner == mobile.entity_id()
                            && record.role == DriverRole::ExternalMobileBuild
                    })
                    .map(|_| *driver)
            })
            .collect();
        match (ports.build, build_drivers.as_slice()) {
            (None, []) => {}
            (Some(build), [driver]) => {
                validate_mobile_sink(
                    signal,
                    mobile,
                    build,
                    SinkRole::MobileBuild,
                    &mut referenced_sinks,
                )?;
                validate_mobile_driver(
                    signal,
                    mobile,
                    *driver,
                    DriverRole::ExternalMobileBuild,
                    &mut referenced_drivers,
                )?;
            }
            _ => return Err(SimulationError::InvalidCanonicalState),
        }
    }

    for (wire, sense) in signal.iter_wire_sensing() {
        if !world
            .structural
            .wires()
            .iter_alive()
            .any(|(_, record)| record.id == wire)
            || sense.ports.a == sense.ports.b
            || !matches!(sense.intended_level, LogicLevel::Low | LogicLevel::High)
        {
            return Err(SimulationError::InvalidCanonicalState);
        }
        let a = signal
            .driver_record(sense.ports.a)
            .ok_or(SimulationError::InvalidCanonicalState)?;
        let b = signal
            .driver_record(sense.ports.b)
            .ok_or(SimulationError::InvalidCanonicalState)?;
        if a.owner != wire.entity_id()
            || b.owner != wire.entity_id()
            || a.role != DriverRole::WireSenseA
            || b.role != DriverRole::WireSenseB
            || !referenced_drivers.insert(sense.ports.a)
            || !referenced_drivers.insert(sense.ports.b)
        {
            return Err(SimulationError::InvalidCanonicalState);
        }
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

fn validate_damage_lifecycle(world: &CanonicalWorld) -> Result<(), SimulationError> {
    let mut expected_pending = BTreeSet::new();
    for enemy in world.enemies.iter() {
        if enemy.integrity().0 == 0 {
            expected_pending.insert(enemy.id().entity_id());
        }
    }
    for (target, _, state) in world.structural.damageable_structural_states() {
        if state.integrity.0 == 0 {
            expected_pending.insert(target);
        }
    }
    if expected_pending != world.pending_destructions {
        return Err(SimulationError::InvalidCanonicalState);
    }
    let core = world.main_core.as_ref();
    if core.is_some_and(|core| world.pending_destructions.contains(&core.id().entity_id())) {
        return Err(SimulationError::InvalidCanonicalState);
    }
    match world.run_status {
        RunStatus::Running => {
            if core.is_some_and(|core| core.integrity().0 == 0) {
                return Err(SimulationError::InvalidCanonicalState);
            }
        }
        RunStatus::Ended {
            completed_tick,
            cause: RunEndCause::MainCoreDestroyed,
        } => {
            if !core.is_some_and(|core| core.integrity().0 == 0)
                || completed_tick.checked_add(Tick(1))? != world.next_tick
            {
                return Err(SimulationError::InvalidCanonicalState);
            }
        }
    }
    Ok(())
}

fn validate_structural_registry_links(
    structural: &StructuralWorld,
    main_core: Option<&MainCoreState>,
    power_sources: &PowerSourceStore,
    enemies: &EnemyStore,
    construction_sites: &ConstructionSiteStore,
) -> Result<(), SimulationError> {
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
    let mut registry_power_sources = BTreeMap::new();
    let mut registry_enemies = BTreeMap::new();
    let mut registry_construction_sites = BTreeSet::new();
    let mut registry_main_core = None;
    for (id, location) in slots {
        if id.0 == 0 || id.0 >= frontier {
            return Err(SimulationError::InvalidCanonicalState);
        }
        let Some(location) = location else {
            continue;
        };
        match location {
            crate::EntityLocation::MainCore => {
                if registry_main_core.replace(id).is_some() {
                    return Err(SimulationError::InvalidCanonicalState);
                }
            }
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
            crate::EntityLocation::PowerSource(index) => {
                if registry_power_sources.insert(id, index).is_some() {
                    return Err(SimulationError::InvalidCanonicalState);
                }
            }
            crate::EntityLocation::Enemy(index) => {
                if registry_enemies.insert(id, index).is_some() {
                    return Err(SimulationError::InvalidCanonicalState);
                }
            }
            crate::EntityLocation::ConstructionSite(_) => {
                if !registry_construction_sites.insert(id) {
                    return Err(SimulationError::InvalidCanonicalState);
                }
            }
            _ => return Err(SimulationError::InvalidCanonicalState),
        }
    }

    if registry_main_core != main_core.map(|core| core.id().entity_id()) {
        return Err(SimulationError::InvalidCanonicalState);
    }
    if main_core.is_some_and(|core| core.id().entity_id() != crate::FIRST_ENTITY_ID) {
        return Err(SimulationError::InvalidCanonicalState);
    }
    let store_power_sources: BTreeMap<_, _> = power_sources
        .iter()
        .enumerate()
        .map(|(index, source)| {
            let index = u32::try_from(index).map_err(|_| SimulationError::NumericOverflow)?;
            Ok((source.id().entity_id(), crate::PowerSourceIndex(index)))
        })
        .collect::<Result<_, SimulationError>>()?;
    if registry_power_sources != store_power_sources {
        return Err(SimulationError::InvalidCanonicalState);
    }
    let store_enemies: BTreeMap<_, _> = enemies
        .iter_alive()
        .map(|(index, enemy)| (enemy.id().entity_id(), index))
        .collect();
    if registry_enemies != store_enemies {
        return Err(SimulationError::InvalidCanonicalState);
    }
    let store_construction_sites = construction_sites
        .iter()
        .map(|site| site.id.entity_id())
        .collect::<BTreeSet<_>>();
    if registry_construction_sites != store_construction_sites {
        return Err(SimulationError::InvalidCanonicalState);
    }
    for (index, source) in power_sources.iter().enumerate() {
        let expected = 2_u64
            .checked_add(u64::try_from(index).map_err(|_| SimulationError::NumericOverflow)?)
            .ok_or(SimulationError::NumericOverflow)?;
        if source.id().entity_id().0 != expected {
            return Err(SimulationError::InvalidCanonicalState);
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
        for (target, point) in [
            (record.endpoint_a, record.points.first().copied()),
            (record.endpoint_b, record.points.last().copied()),
        ] {
            if let crate::EndpointTarget::MainCoreAnchor(id) = target
                && (record.routing_domain != crate::RoutingDomain::OpenWorld
                    || point != main_core.map(|core| core.position())
                    || main_core.map(|core| core.id()) != Some(id))
            {
                return Err(SimulationError::InvalidCanonicalState);
            }
            if let crate::EndpointTarget::PowerSourceAnchor(id) = target
                && point != power_sources.get(id).map(|source| source.position())
            {
                return Err(SimulationError::InvalidCanonicalState);
            }
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

fn validate_mobile_driver(
    signal: &SignalWorld,
    owner: crate::MobileId,
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
            DriverTransitionCause::WireSense if event.pending_generation != 0 => {
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
                DriverTransitionCause::WireSense => {
                    matches!(record.role, DriverRole::WireSenseA | DriverRole::WireSenseB)
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
    let (mut destructions, destruction_topology_changed) =
        apply_pending_structural_and_enemy_destructions(world)?;
    let main_core = world.main_core.map(MainCoreState::anchor_view);
    let power_sources = world
        .power_sources
        .iter()
        .map(|source| PowerSourceAnchorView {
            id: source.id(),
            position: source.position(),
        })
        .collect::<Vec<_>>();
    let mut structural_report = world.structural.apply_phase0_s1m4(
        &mut world.signal,
        &mut world.construction_sites,
        tick,
        commands,
        StructuralCommandContext {
            physical,
            main_core,
            power_sources: &power_sources,
            sensing_enabled: balance.power_probe.is_some(),
            construction_probe: balance.construction_probe.as_ref(),
            initial_integrity: balance
                .contact_damage_probe
                .as_ref()
                .map(|probe| &probe.initial_integrity),
        },
    )?;
    structural_report.topology_changed |= destruction_topology_changed;
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

    destructions.sort_unstable_by_key(|row| row.target);
    Ok(Phase0Output {
        structural_report,
        destructions,
        topology,
        track_graph,
        signal_counters,
    })
}

fn apply_pending_structural_and_enemy_destructions(
    world: &mut CanonicalWorld,
) -> Result<(Vec<DestructionReport>, bool), SimulationError> {
    let structural_batch = world.structural.apply_pending_structural_destructions(
        &mut world.signal,
        &mut world.construction_sites,
        &mut world.pending_destructions,
    )?;
    let mut reports = structural_batch
        .records
        .into_iter()
        .map(|row| DestructionReport {
            target: row.target,
            kind: match row.kind {
                StructuralDestructionKind::Damage => DestructionKind::Damage,
                StructuralDestructionKind::TrackSupportLost => DestructionKind::TrackSupportLost,
                StructuralDestructionKind::SubstrateSupportLost => {
                    DestructionKind::SubstrateSupportLost
                }
                StructuralDestructionKind::ConstructionDependencyLost => {
                    DestructionKind::ConstructionDependencyLost
                }
            },
        })
        .collect::<Vec<_>>();
    let pending_enemies = world
        .pending_destructions
        .iter()
        .filter_map(|&target| {
            let crate::EntityLocation::Enemy(index) =
                world.structural.entities().location(target).copied()?
            else {
                return None;
            };
            Some((target, index))
        })
        .collect::<Vec<_>>();
    for (target, index) in pending_enemies {
        let enemy = crate::EnemyId(target);
        world
            .enemies
            .remove_by_index(index)
            .map_err(|_| SimulationError::InvalidCanonicalState)?;
        world.structural.remove_enemy_registry_entry(enemy, index)?;
        if !world.pending_destructions.remove(&target) {
            return Err(SimulationError::InvalidCanonicalState);
        }
        reports.push(DestructionReport {
            target,
            kind: DestructionKind::Damage,
        });
    }
    Ok((reports, structural_batch.topology_changed))
}

fn run_phase1_snapshot_and_world_sample(
    world: &CanonicalWorld,
    track_graph: &TrackGraph,
    hostiles: &[HostileCollider],
    sensing_enabled: bool,
    sense_radius: Fixed,
    chunk_size: Fixed,
    damage_probe: Option<&crate::ContactDamageProbeProfile>,
) -> Result<Phase1Snapshot, SimulationError> {
    let mut mobiles = Vec::new();
    for (index, record) in world.structural.mobile_substrates().iter_alive() {
        let start = record.track_position;
        mobiles.push(MobilePhase1Snapshot {
            index,
            mobile: record.id,
            start,
            world_point: track_graph.world_position(start)?,
            power_attachment: sensing_enabled
                .then(|| track_graph.power_attachment_position(start))
                .transpose()?,
        });
    }
    mobiles.sort_unstable_by_key(|snapshot| snapshot.mobile.entity_id());
    if mobiles
        .windows(2)
        .any(|pair| pair[0].mobile >= pair[1].mobile)
    {
        return Err(SimulationError::InvalidCanonicalState);
    }
    let wire_sensing = if sensing_enabled {
        let wire_world_points = world
            .structural
            .wires()
            .iter_alive()
            .map(|(_, wire)| {
                Ok((
                    wire.id,
                    world.structural.routing_domain_points_world(
                        wire.routing_domain,
                        wire.points,
                        track_graph,
                    )?,
                ))
            })
            .collect::<Result<Vec<_>, SimulationError>>()?;
        let wires = wire_world_points
            .iter()
            .map(|(id, points)| WireSensingInput { id: *id, points })
            .collect::<Vec<_>>();
        let input_presence = sample_wire_sensing(&wires, hostiles, sense_radius, chunk_size)
            .map_err(|_| SimulationError::InvalidCanonicalState)?;
        let enemies = world
            .enemies
            .iter()
            .map(|enemy| HostileCollider {
                id: enemy.id().entity_id().0,
                center: enemy.position(),
                radius: enemy.radius(),
            })
            .collect::<Vec<_>>();
        let enemy_presence = sample_wire_sensing(&wires, &enemies, sense_radius, chunk_size)
            .map_err(|_| SimulationError::InvalidCanonicalState)?;
        if input_presence.len() != enemy_presence.len() {
            return Err(SimulationError::InvalidCanonicalState);
        }
        input_presence
            .into_iter()
            .zip(enemy_presence)
            .map(|(input, enemy)| {
                if input.id != enemy.id {
                    return Err(SimulationError::InvalidCanonicalState);
                }
                Ok(crate::WireSensingOutput {
                    id: input.id,
                    occupied: input.occupied || enemy.occupied,
                })
            })
            .collect::<Result<Vec<_>, SimulationError>>()?
    } else {
        Vec::new()
    };
    let temperature = |heat: HeatEnergy, kind: ThermalObjectKind| {
        damage_probe
            .map(|probe| phase1_temperature(heat, thermal_capacity_for(kind, probe)))
            .transpose()
            .map(|value| value.unwrap_or(Fixed::ZERO))
    };
    Ok(Phase1Snapshot {
        mobiles,
        wire_sensing,
        enemies: world
            .enemies
            .iter()
            .map(|enemy| {
                Ok(EnemyPhase1Snapshot {
                    enemy: enemy.id(),
                    position: enemy.position(),
                    velocity_per_tick: enemy.velocity_per_tick(),
                    radius: enemy.radius(),
                    integrity: enemy.integrity(),
                    heat_energy: enemy.heat_energy(),
                    temperature: temperature(enemy.heat_energy(), ThermalObjectKind::Enemy)?,
                })
            })
            .collect::<Result<Vec<_>, SimulationError>>()?,
        core: world
            .main_core
            .map(
                |core| -> Result<CoreDamagePhase1Snapshot, SimulationError> {
                    Ok(CoreDamagePhase1Snapshot {
                        target: core.id().entity_id(),
                        integrity: core.integrity(),
                        temperature: temperature(core.heat_energy(), ThermalObjectKind::MainCore)?,
                    })
                },
            )
            .transpose()?,
        structural_damage: world
            .structural
            .damageable_structural_states()
            .map(|(target, kind, damage)| {
                Ok(DamageSnapshot {
                    target,
                    kind,
                    integrity: damage.integrity,
                    phase1_temperature: temperature(damage.heat_energy, kind)?,
                })
            })
            .collect::<Result<Vec<_>, SimulationError>>()?,
    })
}

fn phase1_temperature(heat: HeatEnergy, thermal_capacity: u64) -> Result<Fixed, SimulationError> {
    if thermal_capacity == 0 {
        return Err(SimulationError::InvalidCanonicalState);
    }
    let raw = u128::from(heat.0)
        .checked_mul(
            u128::try_from(crate::FIXED_ONE).map_err(|_| SimulationError::NumericOverflow)?,
        )
        .ok_or(SimulationError::NumericOverflow)?
        / u128::from(thermal_capacity);
    let raw = i64::try_from(raw).map_err(|_| SimulationError::NumericOverflow)?;
    Ok(Fixed(raw))
}

fn validate_world_inputs(
    inputs: &[WorldInputEvent],
    tick: Tick,
) -> Result<&[HostileCollider], SimulationError> {
    if inputs.len() > 1 {
        return Err(SimulationError::DuplicateWorldInputFrame);
    }
    let Some(input) = inputs.first() else {
        return Ok(&[]);
    };
    let WorldInputEvent::HostileFrame {
        target_tick,
        hostiles,
    } = input;
    if *target_tick != tick {
        return Err(SimulationError::WorldInputTickMismatch);
    }
    let mut previous = None;
    for hostile in hostiles {
        if hostile.id == 0 {
            return Err(SimulationError::InvalidHostileId);
        }
        if hostile.radius.0 < 0 {
            return Err(SimulationError::NegativeHostileRadius { id: hostile.id });
        }
        if previous.is_some_and(|id| id >= hostile.id) {
            return if previous == Some(hostile.id) {
                Err(SimulationError::DuplicateHostileId { id: hostile.id })
            } else {
                Err(SimulationError::InvalidCanonicalState)
            };
        }
        previous = Some(hostile.id);
    }
    Ok(hostiles)
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
        match valid.get(&event.driver_id).copied() {
            None => {
                valid.insert(event.driver_id, candidate);
            }
            Some(existing)
                if existing.event.level == event.level
                    && existing.event.strength == event.strength
                    && existing.clear_pending_gate == candidate.clear_pending_gate => {}
            Some(existing)
                if matches!(
                    (existing.event.cause, candidate.event.cause),
                    (
                        DriverTransitionCause::GateOutput,
                        DriverTransitionCause::GateStrengthResponse
                    ) | (
                        DriverTransitionCause::GateStrengthResponse,
                        DriverTransitionCause::GateOutput
                    )
                ) =>
            {
                let (level_transition, strength_transition) =
                    if existing.event.cause == DriverTransitionCause::GateOutput {
                        (existing, candidate)
                    } else {
                        (candidate, existing)
                    };
                let mut combined = level_transition;
                combined.event.strength = strength_transition.event.strength;
                valid.insert(event.driver_id, combined);
            }
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
    mut event: DriverTransition,
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
            // Logic delay is frozen when the level transition is scheduled, but Drive strength
            // continues to follow Power independently. A due level event therefore preserves the
            // latest applied strength unless a same-Tick strength response is merged in Phase 2.
            event.strength = driver.sample.strength;
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
            if gate.current_output != event.level {
                return Ok(None);
            }
            Ok(Some(ValidDriverTransition {
                event,
                clear_pending_gate: None,
            }))
        }
        DriverTransitionCause::WireSense => {
            if !matches!(driver.role, DriverRole::WireSenseA | DriverRole::WireSenseB)
                || event.pending_generation != 0
            {
                return Err(SimulationError::InvalidCanonicalState);
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
    track_graph: &TrackGraph,
    physical: &PhysicalScaleProfile,
    balance: &BalanceProfile,
) -> Result<Phase3Output, SimulationError> {
    let gates: Vec<_> = world
        .signal
        .iter_gates()
        .map(|record| record.gate)
        .collect();
    for gate in gates {
        world.signal.set_gate_desired_from_inputs(gate)?;
    }
    let mut mobile_intents = snapshot
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
                power_attachment: mobile_snapshot.power_attachment,
                construction: None,
            })
        })
        .collect::<Result<Vec<_>, SimulationError>>()?;
    if let Some(probe) = balance.construction_probe.as_ref() {
        for intent in &mut mobile_intents {
            let ports = world
                .signal
                .mobile_ports(intent.mobile)
                .ok_or(SimulationError::InvalidCanonicalState)?;
            let Some(build) = ports.build else {
                return Err(SimulationError::InvalidCanonicalState);
            };
            if world.signal.sink_level(build) != Some(LogicLevel::High) {
                continue;
            }
            let footprint = world
                .structural
                .mobile_substrates()
                .get(intent.index)
                .ok_or(SimulationError::InvalidCanonicalState)?
                .footprint;
            let Some(site) = world.structural.smallest_intersecting_site(
                &world.construction_sites,
                intent.start_world_point,
                footprint,
                track_graph,
                physical,
            )?
            else {
                continue;
            };
            let site_state = world
                .construction_sites
                .get(site)
                .ok_or(SimulationError::InvalidCanonicalState)?;
            let remaining = site_state
                .required_work
                .0
                .checked_sub(site_state.completed_work.0)
                .ok_or(SimulationError::InvalidCanonicalState)?;
            let requested = Energy(probe.builder_work_per_tick.min(remaining));
            let (wire, offset) = intent
                .power_attachment
                .ok_or(SimulationError::InvalidCanonicalState)?;
            let demand = construction_nominal_demand_for_work(
                site,
                intent.mobile,
                crate::PowerNodeKey::WireOffset(wire, offset),
                requested,
                probe,
            )
            .map_err(|_| SimulationError::InvalidCanonicalState)?;
            intent.construction = Some(ConstructionIntent {
                site,
                builder: intent.mobile,
                requested,
                nominal_power: demand.nominal(),
                granted_work: Energy(0),
            });
        }
    }
    let live_wires = if let Some(probe) = balance.contact_damage_probe.as_ref() {
        world
            .structural
            .wires()
            .iter_alive()
            .filter_map(|(_, wire)| {
                let signal = world.signal.wire_snapshot(wire.id)?;
                let resolved = crate::signal::resolve_drive(signal.active, balance.logic_threshold);
                (resolved == LogicLevel::High && signal.active.high > 0).then_some((wire, signal))
            })
            .map(|(wire, signal)| {
                let length = polyline_length(wire.points)?;
                let nominal = calculate_live_wire_demand(
                    LiveWireInput {
                        wire: wire.id,
                        length,
                        high_drive_strength: signal.active.high,
                    },
                    probe,
                )
                .map_err(|_| SimulationError::InvalidCanonicalState)?;
                if physical.wire_body_radius.0 < 0 || nominal.0 == 0 {
                    return Err(SimulationError::InvalidCanonicalState);
                }
                Ok(LiveWireIntent {
                    wire: wire.id,
                    nominal,
                })
            })
            .collect::<Result<Vec<_>, SimulationError>>()?
    } else {
        Vec::new()
    };
    Ok(Phase3Output {
        mobile_intents,
        live_wires,
    })
}

fn run_phase4_global_accounting_and_nominal_demand(
    world: &CanonicalWorld,
    topology: &CompiledSignalTopology,
    mobile_intents: &[MobileIntent],
    live_wires: &[LiveWireIntent],
    balance: &BalanceProfile,
    movement_budget: Fixed,
) -> Result<Phase4Output, SimulationError> {
    let accounted_network = world
        .main_core
        .as_ref()
        .map(|core| {
            account_network_with_support(
                &world.structural,
                Some(core),
                balance.capacity_probe,
                balance.capacity_support_probe,
            )
        })
        .transpose()?;
    let network_accounting = accounted_network
        .as_ref()
        .map(crate::capacity::AccountedNetwork::accounting);
    let Some(probe) = balance.power_probe else {
        return Ok(Phase4Output {
            network_accounting,
            nominal_power: None,
        });
    };

    let gates = collect_gate_power_inputs(world, topology, balance)?;
    let accounted_network = accounted_network
        .as_ref()
        .ok_or(SimulationError::InvalidCanonicalState)?;
    let wires = collect_wire_power_inputs(accounted_network.wires())?;
    let movements = mobile_intents
        .iter()
        .map(|intent| {
            let (wire, offset) = intent
                .power_attachment
                .ok_or(SimulationError::InvalidCanonicalState)?;
            Ok(MovementPowerDemandInput {
                mobile: intent.mobile,
                wire,
                offset,
                base_distance: movement_budget,
                movement_enabled: intent.controls.grants_stage0_movement(),
            })
        })
        .collect::<Result<Vec<_>, SimulationError>>()?;
    let nominal_power = collect_nominal_power_demands_with_capacity_support(
        probe,
        &gates,
        &wires,
        &movements,
        accounted_network.support_shares(),
    )
    .map_err(SimulationError::from)?
    .with_additional(mobile_intents.iter().filter_map(|intent| {
        let construction = intent.construction?;
        let (wire, offset) = intent.power_attachment?;
        Some(crate::NominalPowerDemand::new(
            construction.builder.entity_id(),
            DemandKind::Construction,
            construction.nominal_power,
            crate::PowerNodeKey::WireOffset(wire, offset),
        ))
    }))
    .map_err(SimulationError::from)?
    .with_additional(live_wires.iter().map(|live| {
        crate::NominalPowerDemand::new(
            live.wire.entity_id(),
            DemandKind::LiveWire,
            live.nominal,
            crate::PowerNodeKey::WireBody(live.wire),
        )
    }))
    .map_err(SimulationError::from)?;
    Ok(Phase4Output {
        network_accounting,
        nominal_power: Some(nominal_power),
    })
}

fn collect_gate_power_inputs(
    world: &CanonicalWorld,
    topology: &CompiledSignalTopology,
    balance: &BalanceProfile,
) -> Result<Vec<GatePowerDemandInput>, SimulationError> {
    world
        .signal
        .iter_gates()
        .map(|gate| {
            let load = topology
                .driver_load(gate.ports.output)
                .ok_or(SimulationError::InvalidCanonicalState)?;
            let retains_identical_pending = matches!(
                (
                    gate.pending_due_tick,
                    gate.pending_level,
                    gate.pending_switch_energy,
                ),
                (Some(_), Some(level), Some(_))
                    if level == gate.desired_output
                        && gate.desired_output != gate.current_output
            );
            let switch_energy =
                if gate.desired_output != gate.current_output && !retains_identical_pending {
                    Some(switch_energy(load.total_load, balance)?)
                } else {
                    None
                };
            Ok(GatePowerDemandInput {
                gate: gate.gate,
                output_has_reachable_load: topology.routes_from(gate.ports.output).next().is_some(),
                switch_energy,
            })
        })
        .collect()
}

fn collect_wire_power_inputs(
    wires: &[crate::WireCapacityUsage],
) -> Result<Vec<WirePowerDemandInput>, SimulationError> {
    wires
        .iter()
        .map(|wire| {
            let raw =
                i64::try_from(wire.length().0).map_err(|_| SimulationError::NumericOverflow)?;
            Ok(WirePowerDemandInput {
                wire: wire.wire(),
                length: Fixed(raw),
            })
        })
        .collect()
}

fn run_phase5_power_solve_and_brownout(
    world: &CanonicalWorld,
    nominal: Option<&NominalPowerDemandSet>,
    balance: &BalanceProfile,
) -> Result<Phase5Output, SimulationError> {
    let Some(nominal) = nominal else {
        return Ok(Phase5Output::default());
    };
    let probe = balance
        .power_probe
        .ok_or(SimulationError::InvalidCanonicalState)?;
    let topology = compile_power_topology_with_loads(
        &world.structural,
        &world.power_sources,
        nominal.load_attachments(),
    )?;
    let mut report = solve_power_step_with_capacity_support_heat(
        &topology,
        &world.power_sources,
        nominal,
        probe,
        active_capacity_probe(balance)?,
    )?;
    let private_heat = std::mem::take(&mut report.heat_contributions);
    Ok(Phase5Output {
        report: Some(report),
        private_heat,
    })
}

fn active_capacity_probe(
    balance: &BalanceProfile,
) -> Result<Option<crate::CapacityProbeProfile>, SimulationError> {
    match balance.capacity_support_probe {
        Some(_) => balance
            .capacity_probe
            .map(Some)
            .ok_or(SimulationError::InvalidCanonicalState),
        None => Ok(None),
    }
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

fn power_ratio_from_rational(value: Rational) -> Result<PowerRatio, SimulationError> {
    if value.numerator() < 0 || value.denominator() <= 0 {
        return Err(SimulationError::InvalidCanonicalState);
    }
    let numerator = i128::from(value.numerator())
        .checked_mul(i128::from(crate::FIXED_ONE))
        .ok_or(SimulationError::NumericOverflow)?;
    let raw = crate::round_div_nearest_even(numerator, i128::from(value.denominator()))?;
    let raw = i64::try_from(raw).map_err(|_| SimulationError::NumericOverflow)?;
    PowerRatio::new(Fixed(raw)).map_err(SimulationError::from)
}

fn required_power_ratio(
    report: &PowerStepReport,
    owner: crate::EntityId,
    kind: DemandKind,
) -> Result<PowerRatio, SimulationError> {
    report
        .ratio_for(DemandId::new(owner, kind))
        .ok_or(SimulationError::InvalidCanonicalState)
}

fn collect_power_sense_reports(
    world: &CanonicalWorld,
    report: &PowerStepReport,
    probe: crate::PowerProbeProfile,
) -> Result<Vec<PowerSenseReport>, SimulationError> {
    world
        .signal
        .iter_wire_sensing()
        .flat_map(|(wire, sense)| {
            [(WireEnd::A, sense.ports.a), (WireEnd::B, sense.ports.b)]
                .map(move |(end, driver)| (wire, end, driver, sense))
        })
        .map(|(wire, end, driver, sense)| {
            let ratio = required_power_ratio(report, wire.entity_id(), DemandKind::WireSensing)?;
            let current_driver = world
                .signal
                .driver_sample(driver)
                .ok_or(SimulationError::InvalidCanonicalState)?;
            let intended_strength = scale_drive(DriveStrength(probe.sense_nominal_drive), ratio)?;
            Ok(PowerSenseReport {
                wire,
                end,
                sampled_presence: sense.sampled_presence,
                intended_level: sense.intended_level,
                intended_strength,
                current_driver,
            })
        })
        .collect()
}

fn collect_power_gate_reports(
    world: &CanonicalWorld,
    topology: &CompiledSignalTopology,
    report: &PowerStepReport,
    balance: &BalanceProfile,
) -> Result<Vec<PowerGateReport>, SimulationError> {
    let delay_floor = power_ratio_from_rational(balance.brownout_delay_floor)?;
    world
        .signal
        .iter_gates()
        .map(|gate| {
            let ratio = required_power_ratio(report, gate.gate.entity_id(), DemandKind::GateIdle)?;
            let load = topology
                .driver_load(gate.ports.output)
                .ok_or(SimulationError::InvalidCanonicalState)?;
            Ok(PowerGateReport {
                gate: gate.gate,
                ratio,
                effective_delay: brownout_gate_delay(load.gate_delay, ratio, delay_floor)?,
                effective_drive: scale_drive(DriveStrength(balance.nominal_gate_drive), ratio)?,
                unpowered_ticks: gate.unpowered_ticks,
            })
        })
        .collect()
}

fn run_phase7(
    track_graph: &TrackGraph,
    intents: &[MobileIntent],
    enemies: &[EnemyPhase1Snapshot],
    power_report: Option<&PowerStepReport>,
) -> Result<(Vec<StagedMobileMovement>, Vec<StagedEnemyTrajectory>), SimulationError> {
    let powered_edges = power_report
        .map(|report| {
            track_graph
                .edge_ids()
                .filter_map(|edge| {
                    let ratio =
                        required_power_ratio(report, edge.entity_id(), DemandKind::WireLeakage);
                    match ratio {
                        Ok(ratio) if ratio > PowerRatio::ZERO => Some(Ok(edge)),
                        Ok(_) => None,
                        Err(error) => Some(Err(error)),
                    }
                })
                .collect::<Result<BTreeSet<_>, SimulationError>>()
        })
        .transpose()?;
    let mobiles = intents
        .iter()
        .map(|intent| {
            if track_graph.world_position(intent.start)? != intent.start_world_point {
                return Err(SimulationError::InvalidCanonicalState);
            }
            let observation = match powered_edges.as_ref() {
                Some(powered_edges) => track_graph.stage_powered_movement(
                    intent.mobile,
                    intent.start,
                    intent.controls,
                    intent.granted_budget,
                    powered_edges,
                )?,
                None => track_graph.stage_movement(
                    intent.mobile,
                    intent.start,
                    intent.controls,
                    intent.granted_budget,
                )?,
            };
            Ok(StagedMobileMovement {
                index: intent.index,
                observation,
            })
        })
        .collect::<Result<Vec<_>, SimulationError>>()?;
    let enemies = enemies
        .iter()
        .map(|enemy| {
            let end = FixedVec2::new(
                enemy.position.x.checked_add(enemy.velocity_per_tick.x)?,
                enemy.position.y.checked_add(enemy.velocity_per_tick.y)?,
            );
            Ok(StagedEnemyTrajectory {
                enemy: enemy.enemy,
                start: enemy.position,
                end,
                radius: enemy.radius,
            })
        })
        .collect::<Result<Vec<_>, SimulationError>>()?;
    Ok((mobiles, enemies))
}

fn run_phase6(
    world: &mut CanonicalWorld,
    mobile_intents: &mut [MobileIntent],
    inputs: Phase6Inputs<'_>,
) -> Result<(), SimulationError> {
    let Phase6Inputs {
        topology,
        tick,
        balance,
        movement_budget,
        wire_sensing,
        power_report,
    } = inputs;
    let power_enabled = power_report.is_some();
    if let Some(report) = power_report.as_deref()
        && (!report.sense.is_empty()
            || !report.gates.is_empty()
            || !report.mobiles.is_empty()
            || !report.heat_contributions.is_empty())
    {
        return Err(SimulationError::InvalidCanonicalState);
    }
    let gates: Vec<_> = world.signal.iter_gates().collect();
    let mut candidates = Vec::new();
    let mut gate_reports = Vec::with_capacity(gates.len());
    for gate in gates {
        let load = topology
            .driver_load(gate.ports.output)
            .ok_or(SimulationError::InvalidCanonicalState)?;
        let (ratio, effective_strength, operate_threshold, delay_floor, retention_ticks) =
            match (power_report.as_deref(), balance.power_probe) {
                (None, None) => (
                    PowerRatio::ONE,
                    DriveStrength(balance.nominal_gate_drive),
                    PowerRatio::ZERO,
                    PowerRatio::ONE,
                    u64::MAX,
                ),
                (Some(report), Some(probe)) => {
                    let ratio =
                        required_power_ratio(report, gate.gate.entity_id(), DemandKind::GateIdle)?;
                    (
                        ratio,
                        scale_drive(DriveStrength(balance.nominal_gate_drive), ratio)?,
                        power_ratio_from_rational(balance.logic_operate_threshold)?,
                        power_ratio_from_rational(balance.brownout_delay_floor)?,
                        probe.gate_state_retention_ticks,
                    )
                }
                _ => return Err(SimulationError::InvalidCanonicalState),
            };
        let effective_delay = if power_enabled {
            brownout_gate_delay(load.gate_delay, ratio, delay_floor)?
        } else {
            load.gate_delay
        };
        let powered = ratio >= operate_threshold;
        let unpowered_ticks = if !power_enabled || powered {
            0
        } else {
            gate.unpowered_ticks
                .checked_add(1)
                .ok_or(SimulationError::NumericOverflow)?
        };
        world
            .signal
            .set_gate_unpowered_ticks(gate.gate, unpowered_ticks)?;

        let retention_expired = power_enabled
            && !powered
            && unpowered_ticks >= retention_ticks
            && gate.current_output != LogicLevel::Low;
        let target = if retention_expired {
            world
                .signal
                .set_gate_desired_level(gate.gate, LogicLevel::Low)?;
            Some((LogicLevel::Low, Energy(0)))
        } else if powered && gate.desired_output != gate.current_output {
            Some((
                gate.desired_output,
                switch_energy(load.total_load, balance)?,
            ))
        } else {
            None
        };

        let pending = (
            gate.pending_due_tick,
            gate.pending_level,
            gate.pending_switch_energy,
        );
        let pending_conflicts = matches!(
            pending,
            (Some(_), Some(level), Some(_)) if level != gate.desired_output
        );
        if target.is_none() && pending_conflicts {
            let (Some(_), Some(_), Some(energy)) = pending else {
                return Err(SimulationError::InvalidCanonicalState);
            };
            world.signal.add_cancelled_heat(gate.gate, energy)?;
            world.signal.advance_pending_generation(gate.gate)?;
            world.signal.clear_pending(gate.gate)?;
        } else if let Some((target_level, switch_energy)) = target {
            let retains_identical_pending = matches!(
                (
                    gate.pending_due_tick,
                    gate.pending_level,
                    gate.pending_switch_energy,
                ),
                (Some(_), Some(level), Some(_)) if level == target_level
            );
            if !retains_identical_pending {
                let generation = match (
                    gate.pending_due_tick,
                    gate.pending_level,
                    gate.pending_switch_energy,
                ) {
                    (Some(_), Some(_), Some(energy)) => {
                        world.signal.add_cancelled_heat(gate.gate, energy)?;
                        let generation = world.signal.advance_pending_generation(gate.gate)?;
                        world.signal.clear_pending(gate.gate)?;
                        generation
                    }
                    (None, None, None) => world.signal.advance_pending_generation(gate.gate)?,
                    _ => return Err(SimulationError::InvalidCanonicalState),
                };
                let due_tick = tick.checked_add(effective_delay)?;
                world
                    .signal
                    .set_pending(gate.gate, due_tick, target_level, switch_energy)?;
                candidates.push(DriverTransition::s0m3(
                    due_tick,
                    gate.ports.output,
                    target_level,
                    effective_strength,
                    generation,
                    DriverTransitionCause::GateOutput,
                ));
            }
        } else {
            match (
                gate.pending_due_tick,
                gate.pending_level,
                gate.pending_switch_energy,
            ) {
                (Some(_), Some(_), Some(_)) | (None, None, None) => {}
                _ => return Err(SimulationError::InvalidCanonicalState),
            }
        }

        let output = world
            .signal
            .driver_sample(gate.ports.output)
            .ok_or(SimulationError::InvalidCanonicalState)?;
        if output.level != gate.current_output {
            return Err(SimulationError::InvalidCanonicalState);
        }
        if output.strength != effective_strength {
            candidates.push(DriverTransition::s0m3(
                tick.checked_add(Tick(1))?,
                gate.ports.output,
                gate.current_output,
                effective_strength,
                0,
                DriverTransitionCause::GateStrengthResponse,
            ));
        }
        if power_enabled {
            gate_reports.push(PowerGateReport {
                gate: gate.gate,
                ratio,
                effective_delay,
                effective_drive: effective_strength,
                unpowered_ticks,
            });
        }
    }

    let mut sense_reports = Vec::with_capacity(wire_sensing.len().saturating_mul(2));
    match (power_report.as_deref(), balance.power_probe) {
        (None, None) if !wire_sensing.is_empty() => {
            return Err(SimulationError::InvalidCanonicalState);
        }
        (Some(report), Some(probe)) => {
            for sampled in wire_sensing {
                let ratio =
                    required_power_ratio(report, sampled.id.entity_id(), DemandKind::WireSensing)?;
                let level = if sampled.occupied {
                    LogicLevel::High
                } else {
                    LogicLevel::Low
                };
                let strength = scale_drive(DriveStrength(probe.sense_nominal_drive), ratio)?;
                let (ports, changed) = world.signal.set_wire_sense_intent(
                    sampled.id,
                    sampled.occupied,
                    level,
                    strength,
                )?;
                if changed {
                    let due_tick = tick.checked_add(Tick(balance.sense_delay))?;
                    for driver in [ports.a, ports.b] {
                        candidates.push(DriverTransition::s0m3(
                            due_tick,
                            driver,
                            level,
                            strength,
                            0,
                            DriverTransitionCause::WireSense,
                        ));
                    }
                }
                for (end, driver) in [(WireEnd::A, ports.a), (WireEnd::B, ports.b)] {
                    let current_driver = world
                        .signal
                        .driver_sample(driver)
                        .ok_or(SimulationError::InvalidCanonicalState)?;
                    sense_reports.push(PowerSenseReport {
                        wire: sampled.id,
                        end,
                        sampled_presence: sampled.occupied,
                        intended_level: level,
                        intended_strength: strength,
                        current_driver,
                    });
                }
            }
        }
        (None, None) => {}
        _ => return Err(SimulationError::InvalidCanonicalState),
    }

    world
        .driver_events
        .stage(&mut world.event_payloads, candidates)?;
    let mut mobile_reports = Vec::with_capacity(mobile_intents.len());
    if let Some(report) = power_report.as_deref() {
        for intent in mobile_intents {
            let (nominal_budget, granted_budget, ratio) = if intent
                .controls
                .grants_stage0_movement()
            {
                let ratio =
                    required_power_ratio(report, intent.mobile.entity_id(), DemandKind::Movement)?;
                (
                    movement_budget,
                    scale_movement(movement_budget, ratio)?,
                    Some(ratio),
                )
            } else {
                (Fixed::ZERO, Fixed::ZERO, None)
            };
            intent.granted_budget = granted_budget;
            mobile_reports.push(PowerMobileReport {
                mobile: intent.mobile,
                nominal_budget,
                granted_budget,
                ratio,
            });
            if let Some(construction) = intent.construction.as_mut() {
                let ratio = required_power_ratio(
                    report,
                    construction.builder.entity_id(),
                    DemandKind::Construction,
                )?;
                construction.granted_work = grant_construction_work(construction.requested, ratio)
                    .map_err(|_| SimulationError::InvalidCanonicalState)?;
            }
        }
    } else {
        grant_stage0_mobile_budgets(mobile_intents, movement_budget);
    }
    if let Some(report) = power_report {
        gate_reports.sort_unstable_by_key(|gate| gate.gate);
        sense_reports.sort_unstable_by_key(|sense| (sense.wire, sense.end));
        mobile_reports.sort_unstable_by_key(|mobile| mobile.mobile);
        report.gates = gate_reports;
        report.sense = sense_reports;
        report.mobiles = mobile_reports;
    }
    Ok(())
}

fn run_phase8_interaction(
    world: &CanonicalWorld,
    phase5: &mut Phase5Output,
    inputs: Phase8Inputs<'_>,
) -> Result<Phase8Output, SimulationError> {
    let Phase8Inputs {
        staged_mobiles,
        mobile_intents,
        staged_enemies,
        live_wires,
        track_graph,
        physical,
        balance,
    } = inputs;
    let Some(report) = phase5.report.as_mut() else {
        return if phase5.private_heat.is_empty() {
            Ok(Phase8Output::default())
        } else {
            Err(SimulationError::InvalidCanonicalState)
        };
    };
    if !report.heat_contributions.is_empty() || report.mobiles.len() != staged_mobiles.len() {
        return Err(SimulationError::InvalidCanonicalState);
    }

    // Phase 7 is the authority for the budget attached to the staged trajectory observation.
    for staged in staged_mobiles {
        let index = report
            .mobiles
            .binary_search_by_key(&staged.observation.mobile, |mobile| mobile.mobile)
            .map_err(|_| SimulationError::InvalidCanonicalState)?;
        report.mobiles[index].granted_budget = staged.observation.granted_budget;
    }

    // Heat is not public report data until Phase 8 and does not mutate thermal state before S1-M4.
    phase5
        .private_heat
        .sort_unstable_by_key(|heat| (heat.owner, heat.kind, heat.demand));
    report.heat_contributions = std::mem::take(&mut phase5.private_heat);
    let Some(probe) = balance.contact_damage_probe.as_ref() else {
        return Ok(Phase8Output::default());
    };

    let mut wire_world_points = BTreeMap::<WireId, Vec<FixedVec2>>::new();
    for (_, wire) in world.structural.wires().iter_alive() {
        let points = world.structural.routing_domain_points_world(
            wire.routing_domain,
            wire.points,
            track_graph,
        )?;
        if wire_world_points.insert(wire.id, points).is_some() {
            return Err(SimulationError::InvalidCanonicalState);
        }
    }

    let mut contacts = Vec::new();
    let mut interaction_heat = Vec::new();
    let mut exposures = Vec::new();
    let mut construction = Vec::new();
    let mut construction_contributions = Vec::new();
    for load in &report.loads {
        let kind = match load.demand.kind() {
            DemandKind::GateIdle | DemandKind::GateSwitch | DemandKind::GateDrive => Some((
                InteractionHeatKind::GatePowerDissipation,
                probe.gate_power_heat_fraction,
            )),
            DemandKind::Movement => {
                Some((InteractionHeatKind::Movement, probe.movement_heat_fraction))
            }
            _ => None,
        };
        if let Some((kind, fraction)) = kind {
            let heat = fraction_heat(load.granted, fraction)?;
            if heat.0 > 0 {
                interaction_heat.push(InteractionHeatReport {
                    owner: load.demand.owner(),
                    kind,
                    demand: Some(load.demand),
                    energy: heat,
                });
            }
        }
    }
    for gate in world.signal.iter_gates() {
        if gate.cancelled_switching_heat.0 > 0 {
            interaction_heat.push(InteractionHeatReport {
                owner: gate.gate.entity_id(),
                kind: InteractionHeatKind::CancelledGateSwitch,
                demand: None,
                energy: gate.cancelled_switching_heat,
            });
        }
    }
    if let Some(construction_probe) = balance.construction_probe.as_ref() {
        for intent in mobile_intents {
            let Some(build) = intent.construction else {
                continue;
            };
            construction.push(build);
            construction_contributions.push(ConstructionWorkContribution {
                site: build.site,
                builder: build.builder,
                granted_work: build.granted_work,
            });
            let granted_power = report
                .load(DemandId::new(
                    build.builder.entity_id(),
                    DemandKind::Construction,
                ))
                .ok_or(SimulationError::InvalidCanonicalState)?
                .granted;
            let heat = fraction_heat(granted_power, construction_probe.construction_heat_fraction)?;
            if heat.0 > 0 {
                interaction_heat.push(InteractionHeatReport {
                    owner: build.builder.entity_id(),
                    kind: InteractionHeatKind::Construction,
                    demand: Some(DemandId::new(
                        build.builder.entity_id(),
                        DemandKind::Construction,
                    )),
                    energy: heat,
                });
            }
        }
    }
    for live in live_wires {
        let grant = report
            .load(DemandId::new(live.wire.entity_id(), DemandKind::LiveWire))
            .ok_or(SimulationError::InvalidCanonicalState)?
            .granted;
        let points = wire_world_points
            .get(&live.wire)
            .ok_or(SimulationError::InvalidCanonicalState)?;
        let candidates = staged_enemies
            .iter()
            .filter_map(|enemy| {
                match swept_circle_intersects_wire_body(
                    enemy.start,
                    enemy.end,
                    enemy.radius,
                    points,
                    physical.wire_body_radius,
                ) {
                    Ok(true) => Some(Ok(ContactCandidate {
                        target: enemy.enemy,
                        weight: u128::from(probe.enemy_conductivity),
                    })),
                    Ok(false) => None,
                    Err(_) => Some(Err(SimulationError::InvalidCanonicalState)),
                }
            })
            .collect::<Result<Vec<_>, SimulationError>>()?;
        let (allocations, heat) =
            allocate_contact_energy(grant, &candidates, probe.world_leak_weight)
                .map_err(|_| SimulationError::InvalidCanonicalState)?;
        for allocation in allocations {
            contacts.push(ContactEnergyReport {
                wire: live.wire,
                target: allocation.target,
                weight: allocation.weight,
                absorbed: allocation.absorbed,
            });
            if allocation.absorbed.0 > 0 {
                exposures.push(ElectricalExposure {
                    target: allocation.target.entity_id(),
                    source: live.wire.entity_id(),
                    energy: allocation.absorbed,
                });
            }
        }
        if heat.0 > 0 {
            interaction_heat.push(InteractionHeatReport {
                owner: live.wire.entity_id(),
                kind: InteractionHeatKind::LiveWireRemainder,
                demand: Some(DemandId::new(live.wire.entity_id(), DemandKind::LiveWire)),
                energy: heat,
            });
        }
    }

    for enemy in staged_enemies {
        let mut targets = Vec::<EntityId>::new();
        if let Some(core) = world.main_core
            && swept_circle_intersects_point(enemy.start, enemy.end, enemy.radius, core.position())
                .map_err(|_| SimulationError::InvalidCanonicalState)?
        {
            targets.push(core.id().entity_id());
        }
        for (wire, points) in &wire_world_points {
            if swept_circle_intersects_wire_body(
                enemy.start,
                enemy.end,
                enemy.radius,
                points,
                physical.wire_body_radius,
            )
            .map_err(|_| SimulationError::InvalidCanonicalState)?
            {
                targets.push(wire.entity_id());
            }
        }
        if let Some(target) = targets.into_iter().min()
            && probe.enemy_attack_energy_per_tick > 0
        {
            exposures.push(ElectricalExposure {
                target,
                source: enemy.enemy.entity_id(),
                energy: Energy(probe.enemy_attack_energy_per_tick),
            });
        }
    }
    contacts.sort_unstable_by_key(|row| (row.wire, row.target));
    interaction_heat.sort_unstable_by_key(|row| (row.owner, row.kind, row.demand));
    interaction_heat = reduce_interaction_heat(interaction_heat)?;
    exposures.sort_unstable_by_key(|row| (row.target, row.source));
    exposures = reduce_electrical_exposures(exposures)?;
    Ok(Phase8Output {
        contacts,
        interaction_heat,
        exposures,
        construction,
        construction_contributions,
    })
}

fn fraction_heat(energy: Energy, fraction: Rational) -> Result<HeatEnergy, SimulationError> {
    if fraction.numerator() < 0 || fraction.denominator() <= 0 {
        return Err(SimulationError::InvalidCanonicalState);
    }
    let numerator = i128::from(energy.0)
        .checked_mul(i128::from(fraction.numerator()))
        .ok_or(SimulationError::NumericOverflow)?;
    let rounded = crate::round_div_nearest_even(numerator, i128::from(fraction.denominator()))?;
    let rounded = u64::try_from(rounded).map_err(|_| SimulationError::NumericOverflow)?;
    Ok(HeatEnergy(rounded))
}

fn reduce_interaction_heat(
    rows: Vec<InteractionHeatReport>,
) -> Result<Vec<InteractionHeatReport>, SimulationError> {
    let mut reduced = Vec::<InteractionHeatReport>::new();
    for row in rows {
        if let Some(previous) = reduced.last_mut()
            && (previous.owner, previous.kind, previous.demand) == (row.owner, row.kind, row.demand)
        {
            previous.energy = previous
                .energy
                .checked_add(row.energy)
                .map_err(|_| SimulationError::NumericOverflow)?;
        } else {
            reduced.push(row);
        }
    }
    Ok(reduced)
}

fn reduce_electrical_exposures(
    rows: Vec<ElectricalExposure>,
) -> Result<Vec<ElectricalExposure>, SimulationError> {
    let mut reduced = Vec::<ElectricalExposure>::new();
    for row in rows {
        if let Some(previous) = reduced.last_mut()
            && (previous.target, previous.source) == (row.target, row.source)
        {
            previous.energy = previous
                .energy
                .checked_add(row.energy)
                .map_err(|_| SimulationError::NumericOverflow)?;
        } else {
            reduced.push(row);
        }
    }
    Ok(reduced)
}

fn run_phase9_thermal_integration(
    world: &mut CanonicalWorld,
    interaction_heat: &[InteractionHeatReport],
    power_heat: &[PowerHeatReport],
    balance: &BalanceProfile,
) -> Result<(), SimulationError> {
    if balance.contact_damage_probe.is_none() {
        return if interaction_heat.is_empty() {
            Ok(())
        } else {
            Err(SimulationError::InvalidCanonicalState)
        };
    }
    let mut grouped = BTreeMap::<EntityId, BTreeMap<HeatContributionKey, HeatEnergy>>::new();
    for row in interaction_heat {
        let key = HeatContributionKey {
            kind: row.kind,
            source: row.demand.map(DemandId::owner).unwrap_or(row.owner),
            demand: row.demand,
        };
        let slot = grouped
            .entry(row.owner)
            .or_default()
            .entry(key)
            .or_default();
        *slot = slot
            .checked_add(row.energy)
            .map_err(|_| SimulationError::NumericOverflow)?;
    }
    let gate_ids = world
        .signal
        .iter_gates()
        .map(|gate| gate.gate)
        .collect::<Vec<_>>();
    for gate in gate_ids {
        let heat = world.signal.take_cancelled_heat(gate)?;
        if heat.0 == 0 {
            continue;
        }
        let key = HeatContributionKey {
            kind: InteractionHeatKind::CancelledGateSwitch,
            source: gate.entity_id(),
            demand: None,
        };
        if grouped
            .get(&gate.entity_id())
            .and_then(|rows| rows.get(&key))
            != Some(&heat)
        {
            return Err(SimulationError::InvalidCanonicalState);
        }
    }
    let mut power_by_owner = BTreeMap::<EntityId, HeatEnergy>::new();
    for row in power_heat {
        if row.energy.0 == 0
            || !matches!(
                world.structural.damage_state(row.owner.entity_id()),
                Some((ThermalObjectKind::Wire, _))
            )
        {
            return Err(SimulationError::InvalidCanonicalState);
        }
        let slot = power_by_owner.entry(row.owner.entity_id()).or_default();
        *slot = slot
            .checked_add(row.energy)
            .map_err(|_| SimulationError::NumericOverflow)?;
    }
    let owners = grouped
        .keys()
        .chain(power_by_owner.keys())
        .copied()
        .collect::<BTreeSet<_>>();
    let mut integrated = BTreeMap::<EntityId, HeatEnergy>::new();
    for owner in owners {
        let current = thermal_heat_energy(world, owner)?;
        let after_power = current
            .checked_add(power_by_owner.get(&owner).copied().unwrap_or_default())
            .map_err(|_| SimulationError::NumericOverflow)?;
        let rows = grouped
            .remove(&owner)
            .unwrap_or_default()
            .into_iter()
            .map(|(key, energy)| HeatContributionInput { key, energy })
            .collect::<Vec<_>>();
        let heat = if rows.is_empty() {
            after_power
        } else {
            integrate_heat(owner, after_power, &rows)
                .map_err(|_| SimulationError::InvalidCanonicalState)?
        };
        integrated.insert(owner, heat);
    }
    for (owner, heat) in integrated {
        set_thermal_heat_energy(world, owner, heat)?;
    }
    Ok(())
}

fn thermal_heat_energy(
    world: &CanonicalWorld,
    owner: EntityId,
) -> Result<HeatEnergy, SimulationError> {
    if let Some(core) = world
        .main_core
        .filter(|core| core.id().entity_id() == owner)
    {
        Ok(core.heat_energy())
    } else if let Some(enemy) = world.enemies.get(crate::EnemyId(owner)) {
        Ok(enemy.heat_energy())
    } else if let Some((_, damage)) = world.structural.damage_state(owner) {
        Ok(damage.heat_energy)
    } else {
        Err(SimulationError::InvalidCanonicalState)
    }
}

fn set_thermal_heat_energy(
    world: &mut CanonicalWorld,
    owner: EntityId,
    heat: HeatEnergy,
) -> Result<(), SimulationError> {
    if world
        .main_core
        .is_some_and(|core| core.id().entity_id() == owner)
    {
        let core = world
            .main_core
            .as_mut()
            .ok_or(SimulationError::InvalidCanonicalState)?;
        core.set_heat_energy(heat);
    } else if let Some(enemy) = world.enemies.get_mut(crate::EnemyId(owner)) {
        enemy.set_heat_energy(heat);
    } else if let Some((_, damage)) = world.structural.damage_state(owner) {
        world.structural.set_damage_state(
            owner,
            crate::DamageState {
                heat_energy: heat,
                ..damage
            },
        )?;
    } else {
        return Err(SimulationError::InvalidCanonicalState);
    }
    Ok(())
}

fn run_phase10_damage_resolution(
    world: &mut CanonicalWorld,
    snapshot: &Phase1Snapshot,
    exposures: &[ElectricalExposure],
    balance: &BalanceProfile,
) -> Result<Phase10Output, SimulationError> {
    let Some(probe) = balance.contact_damage_probe.as_ref() else {
        return if exposures.is_empty() {
            Ok(Phase10Output::default())
        } else {
            Err(SimulationError::InvalidCanonicalState)
        };
    };
    let mut damageable_targets = BTreeSet::new();
    if let Some(core) = snapshot.core
        && !damageable_targets.insert(core.target)
    {
        return Err(SimulationError::InvalidCanonicalState);
    }
    for enemy in &snapshot.enemies {
        if !damageable_targets.insert(enemy.enemy.entity_id()) {
            return Err(SimulationError::InvalidCanonicalState);
        }
    }
    for structural in &snapshot.structural_damage {
        if !damageable_targets.insert(structural.target) {
            return Err(SimulationError::InvalidCanonicalState);
        }
    }
    if exposures
        .iter()
        .any(|exposure| !damageable_targets.contains(&exposure.target))
    {
        return Err(SimulationError::InvalidCanonicalState);
    }
    let mut damage = Vec::new();
    let mut core_destroyed = false;

    if let Some(core) = snapshot.core {
        let rows = exposure_rows_for(exposures, core.target);
        let resolution = resolve_damage(
            DamageSnapshot {
                target: core.target,
                kind: ThermalObjectKind::MainCore,
                integrity: core.integrity,
                phase1_temperature: core.temperature,
            },
            rows,
            probe,
        )
        .map_err(|_| SimulationError::InvalidCanonicalState)?;
        if resolution.electrical_exposure.0 > 0
            || resolution.electrical_damage.0 > 0
            || resolution.thermal_damage.0 > 0
        {
            damage.push(damage_report(resolution));
        }
        let state = world
            .main_core
            .as_mut()
            .ok_or(SimulationError::InvalidCanonicalState)?;
        state.set_integrity(resolution.integrity_after);
        core_destroyed = resolution.pending_destruction;
    }
    for enemy in &snapshot.enemies {
        let target = enemy.enemy.entity_id();
        let rows = exposure_rows_for(exposures, target);
        let resolution = resolve_damage(
            DamageSnapshot {
                target,
                kind: ThermalObjectKind::Enemy,
                integrity: enemy.integrity,
                phase1_temperature: enemy.temperature,
            },
            rows,
            probe,
        )
        .map_err(|_| SimulationError::InvalidCanonicalState)?;
        if resolution.electrical_exposure.0 > 0
            || resolution.electrical_damage.0 > 0
            || resolution.thermal_damage.0 > 0
        {
            damage.push(damage_report(resolution));
        }
        let state = world
            .enemies
            .get_mut(enemy.enemy)
            .ok_or(SimulationError::InvalidCanonicalState)?;
        state.set_integrity(resolution.integrity_after);
        if resolution.pending_destruction {
            world.pending_destructions.insert(target);
        }
    }
    for structural in &snapshot.structural_damage {
        let rows = exposure_rows_for(exposures, structural.target);
        let resolution = resolve_damage(*structural, rows, probe)
            .map_err(|_| SimulationError::InvalidCanonicalState)?;
        if resolution.electrical_exposure.0 > 0
            || resolution.electrical_damage.0 > 0
            || resolution.thermal_damage.0 > 0
        {
            damage.push(damage_report(resolution));
        }
        let (_, current) = world
            .structural
            .damage_state(structural.target)
            .ok_or(SimulationError::InvalidCanonicalState)?;
        world.structural.set_damage_state(
            structural.target,
            crate::DamageState {
                integrity: resolution.integrity_after,
                ..current
            },
        )?;
        if resolution.pending_destruction {
            world.pending_destructions.insert(structural.target);
        }
    }
    damage.sort_unstable_by_key(|row| row.target);
    Ok(Phase10Output {
        damage,
        core_destroyed,
    })
}

fn exposure_rows_for(exposures: &[ElectricalExposure], target: EntityId) -> &[ElectricalExposure] {
    let start = exposures.partition_point(|row| row.target < target);
    let end = exposures.partition_point(|row| row.target <= target);
    &exposures[start..end]
}

fn damage_report(resolution: crate::DamageResolution) -> DamageReport {
    DamageReport {
        target: resolution.target,
        electrical_exposure: resolution.electrical_exposure,
        electrical_damage: resolution.electrical_damage,
        thermal_damage: resolution.thermal_damage,
        integrity_before: resolution.integrity_before,
        integrity_after: resolution.integrity_after,
        pending_destruction: resolution.pending_destruction,
    }
}

fn run_phase11_progress_commit(
    world: &mut CanonicalWorld,
    inputs: Phase11Inputs<'_>,
) -> Result<Phase11Output, SimulationError> {
    let Phase11Inputs {
        completed_tick,
        next_tick,
        staged_mobiles,
        staged_enemies,
        construction,
        construction_contributions,
        core_destroyed,
    } = inputs;
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
    for staged in staged_enemies {
        world
            .enemies
            .get_mut(staged.enemy)
            .ok_or(SimulationError::InvalidCanonicalState)?
            .set_position(staged.end);
    }
    let construction_progress =
        apply_construction_work(&mut world.construction_sites, construction_contributions)
            .map_err(|_| SimulationError::InvalidCanonicalState)?;
    let construction_by_key = construction
        .iter()
        .map(|row| ((row.site, row.builder), *row))
        .collect::<BTreeMap<_, _>>();
    let construction_work = construction_progress
        .into_iter()
        .map(|progress| {
            let intent = construction_by_key
                .get(&(progress.site, progress.builder))
                .ok_or(SimulationError::InvalidCanonicalState)?;
            Ok(ConstructionWorkReport {
                site: progress.site,
                builder: progress.builder,
                requested: intent.requested,
                nominal_power: intent.nominal_power,
                granted_work: progress.granted_work,
                applied_work: progress.applied_work,
                completed_work: progress.completed_work,
            })
        })
        .collect::<Result<Vec<_>, SimulationError>>()?;
    world.next_tick = next_tick;
    if core_destroyed {
        world.run_status = RunStatus::Ended {
            completed_tick,
            cause: RunEndCause::MainCoreDestroyed,
        };
    }
    validate_canonical_world(world)?;
    let state_hash = canonical::state_hash(world.state_view());
    let mobile_movements = staged_mobiles
        .into_iter()
        .map(|staged| staged.observation)
        .collect();
    Ok(Phase11Output {
        state_hash,
        mobile_movements,
        run_status: world.run_status,
        construction_work,
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

impl From<PowerAdapterError> for SimulationError {
    fn from(error: PowerAdapterError) -> Self {
        match error {
            PowerAdapterError::NumericOverflow => Self::NumericOverflow,
            PowerAdapterError::IndistinguishableWireEndpoints { .. }
            | PowerAdapterError::InvalidCanonicalState => Self::InvalidCanonicalState,
            PowerAdapterError::Topology(error) => match error {
                crate::PowerTopologyError::NumericOverflow => Self::NumericOverflow,
                crate::PowerTopologyError::PowerKernel(PowerError::NumericOverflow) => {
                    Self::NumericOverflow
                }
                _ => Self::InvalidCanonicalState,
            },
        }
    }
}

impl From<PowerRuntimeError> for SimulationError {
    fn from(error: PowerRuntimeError) -> Self {
        match error {
            PowerRuntimeError::NumericOverflow
            | PowerRuntimeError::Power(PowerError::NumericOverflow) => Self::NumericOverflow,
            _ => Self::InvalidCanonicalState,
        }
    }
}

impl From<PowerError> for SimulationError {
    fn from(error: PowerError) -> Self {
        match error {
            PowerError::NumericOverflow => Self::NumericOverflow,
            PowerError::InvalidNumericDivisor => Self::InvalidNumericDivisor,
            _ => Self::InvalidCanonicalState,
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
        let snapshot = run_phase1_snapshot_and_world_sample(
            &simulation.canonical,
            &graph,
            &[],
            false,
            simulation.profiles.balance.sense_radius,
            simulation.profiles.physical_scale.world_routing_pitch,
            None,
        )
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
            required_features: simulation.required_features,
            initial_state_hash: simulation.initial_state_hash,
            world_generator_version: simulation.world_generator_version,
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
            required_features: simulation.required_features,
            initial_state_hash: simulation.initial_state_hash,
            world_generator_version: simulation.world_generator_version,
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
    fn canonical_validator_rejects_x_as_a_wire_sense_intended_level() {
        let profiles = ProfileBundle {
            numeric: NumericProfile::reference_v1("numeric-sense-x-validator"),
            physical_scale: PhysicalScaleProfile::stage0_alpha("physical-sense-x-validator"),
            balance: BalanceProfile::power_probe_alpha("balance-sense-x-validator"),
        };
        let contract = SimulationContract::from_profiles(&profiles).expect("profiles are valid");
        let pitch = profiles.physical_scale.world_routing_pitch;
        let point = |x: i64, y: i64| FixedVec2::new(Fixed(x), Fixed(y));
        let mut simulation = Simulation::new(SimulationPackage::new(
            "sense-x-validator",
            InitialWorld::MainCorePowerV1 {
                main_core_position: point(-16 * pitch.0, -16 * pitch.0),
                main_core_integrity: crate::Integrity(1_000),
                main_core_heat_energy: crate::HeatEnergy(0),
                power_sources: Vec::new(),
            },
            StageFeatureSet {
                capacity: true,
                sensing: true,
                power: true,
                ..StageFeatureSet::none()
            },
            contract,
            profiles,
        ))
        .expect("S1-M2 source-less world starts");
        simulation
            .step(&[crate::CommandEnvelope {
                target_tick: Tick(0),
                ordinal: 0,
                command: crate::Command::PlaceWire(crate::PlaceWireCommand {
                    routing_domain: crate::RoutingDomain::OpenWorld,
                    points: vec![point(0, 0), point(2 * pitch.0, 0)],
                    endpoint_a: crate::EndpointTarget::Free,
                    endpoint_b: crate::EndpointTarget::Free,
                }),
            }])
            .expect("sensing-enabled Wire placement succeeds");
        let wire = simulation
            .canonical
            .signal
            .iter_wire_sensing()
            .next()
            .expect("placed Wire has Sense state")
            .0;
        simulation
            .canonical
            .signal
            .set_wire_sense_intent(wire, false, LogicLevel::X, DriveStrength(0))
            .expect("test injects malformed intended level");

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
    fn committed_validator_rejects_main_core_registry_mismatch() {
        let profiles = ProfileBundle {
            numeric: NumericProfile::reference_v1("numeric-core-validator"),
            physical_scale: PhysicalScaleProfile::stage0_alpha("physical-core-validator"),
            balance: BalanceProfile::capacity_probe_alpha("balance-core-validator"),
        };
        let contract = SimulationContract::from_profiles(&profiles).expect("profiles are valid");
        let mut simulation = Simulation::new(SimulationPackage::new(
            "core-validator",
            InitialWorld::MainCoreV1 {
                position: FixedVec2::new(Fixed(0), Fixed(0)),
                integrity: crate::Integrity(1),
                heat_energy: crate::HeatEnergy(0),
            },
            StageFeatureSet {
                capacity: true,
                ..StageFeatureSet::none()
            },
            contract,
            profiles,
        ))
        .expect("Main Core simulation starts");
        validate_canonical_world(&simulation.canonical).expect("uncorrupted Main Core is valid");
        simulation
            .canonical
            .structural
            .remove_registry_entry_for_test(crate::EntityId(1))
            .expect("test-only Main Core registry corruption succeeds");
        assert_eq!(
            validate_canonical_world(&simulation.canonical),
            Err(SimulationError::InvalidCanonicalState)
        );

        let profiles = ProfileBundle {
            numeric: NumericProfile::reference_v1("numeric-core-id-validator"),
            physical_scale: PhysicalScaleProfile::stage0_alpha("physical-core-id-validator"),
            balance: BalanceProfile::capacity_probe_alpha("balance-core-id-validator"),
        };
        let contract = SimulationContract::from_profiles(&profiles).expect("profiles are valid");
        let mut matching_id_two = Simulation::new(SimulationPackage::new(
            "core-id-validator",
            InitialWorld::MainCoreV1 {
                position: FixedVec2::new(Fixed(0), Fixed(0)),
                integrity: crate::Integrity(1),
                heat_energy: crate::HeatEnergy(0),
            },
            StageFeatureSet {
                capacity: true,
                ..StageFeatureSet::none()
            },
            contract,
            profiles,
        ))
        .expect("Main Core simulation starts");
        let id_two = matching_id_two
            .canonical
            .structural
            .relocate_main_core_registry_for_test()
            .expect("test-only Main Core registry relocation succeeds");
        let relocated = matching_id_two
            .canonical
            .main_core
            .expect("Main Core exists")
            .with_id_for_test(id_two);
        matching_id_two.canonical.main_core = Some(relocated);
        assert_eq!(id_two, crate::MainCoreId(crate::EntityId(2)));
        assert_eq!(
            validate_canonical_world(&matching_id_two.canonical),
            Err(SimulationError::InvalidCanonicalState),
            "a matching registry/Core pair is still invalid unless the Core owns EntityId 1"
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
    fn power_features_require_the_s1m2_world_before_later_runtime_work() {
        let mut package = package();
        package.required_features.mobility = true;
        package.required_features.sensing = true;

        assert_eq!(
            Simulation::new(package).err(),
            Some(SimulationError::PowerFeaturesRequireMainCorePowerWorld)
        );
    }

    #[test]
    fn s1m4_feature_triad_is_rejected_by_retained_worlds_before_tick_zero() {
        for feature in ["construction", "contact", "damage"] {
            let mut package = package();
            match feature {
                "construction" => package.required_features.construction = true,
                "contact" => package.required_features.contact = true,
                "damage" => package.required_features.damage = true,
                _ => unreachable!(),
            }
            assert_eq!(
                Simulation::new(package).err(),
                Some(SimulationError::PowerFeaturesRequireMainCorePowerWorld),
                "retained Empty world must reject S1-M4 feature {feature}"
            );
        }
    }

    fn s1m4_runtime_package(core_integrity: u64, enemy_position: FixedVec2) -> SimulationPackage {
        let mut profiles = ProfileBundle {
            numeric: NumericProfile::reference_v1("numeric-s1m4-runtime"),
            physical_scale: PhysicalScaleProfile::stage0_alpha("physical-s1m4-runtime"),
            balance: BalanceProfile::construction_contact_damage_alpha("balance-s1m4-runtime"),
        };
        profiles
            .balance
            .contact_damage_probe
            .as_mut()
            .expect("v5 probe exists")
            .enemy_attack_energy_per_tick = 100;
        let contract = SimulationContract::from_profiles(&profiles).expect("profiles are valid");
        SimulationPackage::new(
            "s1m4-runtime",
            InitialWorld::MainCorePowerEnemyV1 {
                main_core_position: FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
                main_core_integrity: Integrity(core_integrity),
                main_core_heat_energy: HeatEnergy(0),
                power_sources: vec![],
                enemies: vec![crate::EnemyInitialState::new(
                    enemy_position,
                    FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
                    Fixed(crate::FIXED_ONE),
                    Integrity(10),
                    HeatEnergy(0),
                )],
            },
            StageFeatureSet {
                signal: true,
                mobility: true,
                capacity: true,
                sensing: true,
                power: true,
                construction: true,
                contact: true,
                damage: true,
                ..StageFeatureSet::none()
            },
            contract,
            profiles,
        )
    }

    fn direct_s1m4_package(
        power_sources: Vec<crate::PowerSourceInitialState>,
        enemies: Vec<crate::EnemyInitialState>,
    ) -> SimulationPackage {
        let mut package = s1m4_runtime_package(100, FixedVec2::new(Fixed::ZERO, Fixed::ZERO));
        let InitialWorld::MainCorePowerEnemyV1 {
            power_sources: package_sources,
            enemies: package_enemies,
            ..
        } = &mut package.initial_world
        else {
            unreachable!("the S1-M4 test package selects the v1 Enemy world")
        };
        *package_sources = power_sources;
        *package_enemies = enemies;
        package
    }

    fn direct_enemy(
        position: FixedVec2,
        velocity_per_tick: FixedVec2,
        radius: Fixed,
        integrity: u64,
        heat_energy: u64,
    ) -> crate::EnemyInitialState {
        crate::EnemyInitialState::new(
            position,
            velocity_per_tick,
            radius,
            Integrity(integrity),
            HeatEnergy(heat_energy),
        )
    }

    #[test]
    fn direct_s1m4_package_rejects_empty_nonpositive_overflowing_and_duplicate_enemies() {
        let quantum = crate::REFERENCE_WIRE_GEOMETRY_QUANTUM;
        let point = |x, y| FixedVec2::new(Fixed(x), Fixed(y));
        let valid = direct_enemy(point(0, 0), point(0, 0), quantum, 10, 0);
        let duplicate = direct_enemy(point(0, 0), point(0, 0), quantum, 10, 7);

        for (name, enemies, expected) in [
            ("empty", Vec::new(), SimulationError::InvalidCanonicalState),
            (
                "zero radius",
                vec![direct_enemy(point(0, 0), point(0, 0), Fixed::ZERO, 10, 0)],
                SimulationError::InvalidCanonicalState,
            ),
            (
                "negative radius",
                vec![direct_enemy(point(0, 0), point(0, 0), Fixed(-1), 10, 0)],
                SimulationError::InvalidCanonicalState,
            ),
            (
                "zero integrity",
                vec![direct_enemy(point(0, 0), point(0, 0), quantum, 0, 0)],
                SimulationError::InvalidCanonicalState,
            ),
            (
                "trajectory overflow",
                vec![direct_enemy(
                    point(i64::MAX, 0),
                    point(quantum.0, 0),
                    quantum,
                    10,
                    0,
                )],
                SimulationError::NumericOverflow,
            ),
            (
                "complete duplicate",
                vec![duplicate, duplicate],
                SimulationError::InvalidCanonicalState,
            ),
        ] {
            assert_eq!(
                Simulation::new(direct_s1m4_package(Vec::new(), enemies)).err(),
                Some(expected),
                "direct package case `{name}` returned the wrong typed error"
            );
        }

        Simulation::new(direct_s1m4_package(
            Vec::new(),
            vec![
                valid,
                direct_enemy(point(0, 0), point(0, 0), quantum, 10, 1),
            ],
        ))
        .expect("only a complete semantic-tuple duplicate is rejected");
    }

    #[test]
    fn direct_s1m4_package_rejects_nonpositive_and_duplicate_power_sources() {
        let quantum = crate::REFERENCE_WIRE_GEOMETRY_QUANTUM;
        let point = |x, y| FixedVec2::new(Fixed(x), Fixed(y));
        let enemy = direct_enemy(point(0, 0), point(0, 0), quantum, 10, 0);
        let duplicate_position = point(2 * quantum.0, 0);
        let cases = [
            vec![crate::PowerSourceInitialState::new(
                point(quantum.0, 0),
                Energy(0),
            )],
            vec![
                crate::PowerSourceInitialState::new(duplicate_position, Energy(2)),
                crate::PowerSourceInitialState::new(duplicate_position, Energy(1)),
            ],
        ];

        for sources in cases {
            assert_eq!(
                Simulation::new(direct_s1m4_package(sources, vec![enemy])).err(),
                Some(SimulationError::InvalidCanonicalState)
            );
        }
    }

    #[test]
    fn direct_s1m4_package_requires_the_complete_eight_feature_set() {
        let quantum = crate::REFERENCE_WIRE_GEOMETRY_QUANTUM;
        let point = |x, y| FixedVec2::new(Fixed(x), Fixed(y));
        let enemy = direct_enemy(point(0, 0), point(0, 0), quantum, 10, 0);

        for feature in [
            "signal",
            "mobility",
            "capacity",
            "sensing",
            "power",
            "construction",
            "contact",
            "damage",
        ] {
            let mut package = direct_s1m4_package(Vec::new(), vec![enemy]);
            match feature {
                "signal" => package.required_features.signal = false,
                "mobility" => package.required_features.mobility = false,
                "capacity" => package.required_features.capacity = false,
                "sensing" => package.required_features.sensing = false,
                "power" => package.required_features.power = false,
                "construction" => package.required_features.construction = false,
                "contact" => package.required_features.contact = false,
                "damage" => package.required_features.damage = false,
                _ => unreachable!(),
            }
            assert_eq!(
                Simulation::new(package).err(),
                Some(SimulationError::MainCorePowerRequiresFeatures),
                "missing direct S1-M4 feature `{feature}` was accepted"
            );
        }
    }

    #[test]
    fn direct_s1m4_package_rejects_every_off_quantum_enemy_coordinate() {
        let quantum = crate::REFERENCE_WIRE_GEOMETRY_QUANTUM;
        let point = |x, y| FixedVec2::new(Fixed(x), Fixed(y));
        let cases = [
            ("position.x", point(1, 0), point(0, 0), quantum),
            ("position.y", point(0, 1), point(0, 0), quantum),
            ("velocity.x", point(0, 0), point(1, 0), quantum),
            ("velocity.y", point(0, 0), point(0, 1), quantum),
            ("radius", point(0, 0), point(0, 0), Fixed(quantum.0 + 1)),
        ];

        for (field, position, velocity, radius) in cases {
            let enemy = direct_enemy(position, velocity, radius, 10, 0);
            assert_eq!(
                Simulation::new(direct_s1m4_package(Vec::new(), vec![enemy])).err(),
                Some(SimulationError::InvalidCanonicalState),
                "off-quantum direct Enemy field `{field}` was accepted"
            );
        }
    }

    #[test]
    fn direct_s1m4_enemy_permutations_allocate_the_same_sorted_ids() {
        let quantum = crate::REFERENCE_WIRE_GEOMETRY_QUANTUM;
        let point = |x, y| FixedVec2::new(Fixed(x), Fixed(y));
        let sorted_inputs = vec![
            direct_enemy(point(-quantum.0, 0), point(0, 0), quantum, 10, 0),
            direct_enemy(point(0, -quantum.0), point(0, 0), quantum, 10, 0),
            direct_enemy(point(0, 0), point(-quantum.0, 0), quantum, 10, 0),
            direct_enemy(point(0, 0), point(0, -quantum.0), quantum, 10, 0),
            direct_enemy(point(0, 0), point(0, 0), quantum, 10, 3),
            direct_enemy(point(0, 0), point(0, 0), quantum, 10, 7),
            direct_enemy(point(0, 0), point(0, 0), Fixed(2 * quantum.0), 10, 0),
        ];
        let permuted_inputs = vec![
            sorted_inputs[6],
            sorted_inputs[4],
            sorted_inputs[2],
            sorted_inputs[0],
            sorted_inputs[5],
            sorted_inputs[1],
            sorted_inputs[3],
        ];
        let sorted_sources = vec![
            crate::PowerSourceInitialState::new(point(-2 * quantum.0, 0), Energy(1)),
            crate::PowerSourceInitialState::new(point(2 * quantum.0, 0), Energy(2)),
        ];
        let permuted_sources = vec![sorted_sources[1], sorted_sources[0]];

        let canonical = Simulation::new(direct_s1m4_package(permuted_sources, permuted_inputs))
            .expect("permuted direct Source and Enemy input normalizes");
        let ordered = Simulation::new(direct_s1m4_package(
            sorted_sources.clone(),
            sorted_inputs.clone(),
        ))
        .expect("already ordered direct Source and Enemy input remains valid");

        let rows = |simulation: &Simulation| {
            simulation
                .canonical
                .enemies
                .iter()
                .map(|enemy| {
                    (
                        enemy.id(),
                        enemy.position(),
                        enemy.velocity_per_tick(),
                        enemy.radius(),
                        enemy.integrity(),
                        enemy.heat_energy(),
                    )
                })
                .collect::<Vec<_>>()
        };
        let expected = sorted_inputs
            .into_iter()
            .enumerate()
            .map(|(index, enemy)| {
                (
                    crate::EnemyId(EntityId(4 + index as u64)),
                    enemy.position(),
                    enemy.velocity_per_tick(),
                    enemy.radius(),
                    enemy.integrity(),
                    enemy.heat_energy(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(rows(&canonical), expected);
        assert_eq!(rows(&ordered), expected);
        assert_eq!(canonical.initial_state_hash, ordered.initial_state_hash);
        let source_rows = |simulation: &Simulation| {
            simulation
                .canonical
                .power_sources
                .iter()
                .map(|source| (source.id(), source.position(), source.generation_per_tick()))
                .collect::<Vec<_>>()
        };
        let expected_sources = sorted_sources
            .into_iter()
            .enumerate()
            .map(|(index, source)| {
                (
                    crate::PowerSourceId(EntityId(2 + index as u64)),
                    source.position(),
                    source.generation_per_tick(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            source_rows(&canonical),
            expected_sources,
            "Core and sorted Sources allocate before normalized Enemies"
        );
        assert_eq!(source_rows(&ordered), expected_sources);
    }

    #[test]
    fn direct_s1m4_quantum_error_precedes_profile_integrity_mismatch() {
        let quantum = crate::REFERENCE_WIRE_GEOMETRY_QUANTUM;
        let point = |x, y| FixedVec2::new(Fixed(x), Fixed(y));
        let enemy = direct_enemy(point(0, 0), point(0, 0), quantum, 10, 0);
        let mut package = direct_s1m4_package(Vec::new(), vec![enemy]);
        let InitialWorld::MainCorePowerEnemyV1 {
            main_core_position,
            main_core_integrity,
            ..
        } = &mut package.initial_world
        else {
            unreachable!("the S1-M4 test package selects the v1 Enemy world")
        };
        *main_core_position = point(1, 0);
        *main_core_integrity = Integrity(99);

        assert_eq!(
            Simulation::new(package).err(),
            Some(SimulationError::InvalidMainCoreGeometryQuantum)
        );
    }

    #[test]
    fn direct_s1m4_world_faults_precede_numeric_hash_and_profile_body_faults() {
        let quantum = crate::REFERENCE_WIRE_GEOMETRY_QUANTUM;
        let point = |x, y| FixedVec2::new(Fixed(x), Fixed(y));
        let enemy = direct_enemy(point(0, 0), point(0, 0), quantum, 10, 0);
        let valid = direct_s1m4_package(Vec::new(), vec![enemy]);

        let mut nonpositive_core = valid.clone();
        let InitialWorld::MainCorePowerEnemyV1 {
            main_core_integrity,
            ..
        } = &mut nonpositive_core.initial_world
        else {
            unreachable!("the S1-M4 test package selects the v1 Enemy world")
        };
        *main_core_integrity = Integrity(0);

        let empty_enemies = direct_s1m4_package(Vec::new(), Vec::new());
        let duplicate_position = point(2 * quantum.0, 0);
        let duplicate_sources = direct_s1m4_package(
            vec![
                crate::PowerSourceInitialState::new(duplicate_position, Energy(1)),
                crate::PowerSourceInitialState::new(duplicate_position, Energy(2)),
            ],
            vec![enemy],
        );

        let mut off_quantum_core = valid.clone();
        let InitialWorld::MainCorePowerEnemyV1 {
            main_core_position, ..
        } = &mut off_quantum_core.initial_world
        else {
            unreachable!("the S1-M4 test package selects the v1 Enemy world")
        };
        *main_core_position = point(1, 0);

        for (name, package, expected) in [
            (
                "Main Core positivity",
                nonpositive_core,
                SimulationError::InvalidMainCoreIntegrity,
            ),
            (
                "Enemy shape",
                empty_enemies,
                SimulationError::InvalidCanonicalState,
            ),
            (
                "Source duplicates",
                duplicate_sources,
                SimulationError::InvalidCanonicalState,
            ),
            (
                "World quantum",
                off_quantum_core,
                SimulationError::InvalidMainCoreGeometryQuantum,
            ),
        ] {
            let mut hash_fault = package.clone();
            hash_fault.contract.numeric_profile_hash = crate::ProfileHash::default();
            assert_eq!(
                Simulation::new(hash_fault).err(),
                Some(expected.clone()),
                "{name} must precede a Numeric contract hash mismatch"
            );

            let mut body_fault = package;
            body_fault.profiles.numeric.fixed_one = crate::FIXED_ONE + 1;
            assert_eq!(
                Simulation::new(body_fault).err(),
                Some(expected),
                "{name} must precede an invalid Numeric Profile body"
            );
        }

        let valid_numeric_hash = valid.contract.numeric_profile_hash;
        let mut hash_only = valid.clone();
        hash_only.contract.numeric_profile_hash = crate::ProfileHash::default();
        assert_eq!(
            Simulation::new(hash_only).err(),
            Some(SimulationError::ProfileHashMismatch {
                profile: ProfileKind::Numeric,
                expected: crate::ProfileHash::default(),
                actual: valid_numeric_hash,
            })
        );

        let mut body_only = valid;
        body_only.profiles.numeric.fixed_one = crate::FIXED_ONE + 1;
        assert_eq!(
            Simulation::new(body_only).err(),
            Some(SimulationError::InvalidProfile {
                error: ProfileValidationError::FixedOneMismatch {
                    expected: crate::FIXED_ONE,
                    actual: crate::FIXED_ONE + 1,
                },
            })
        );
    }

    #[test]
    fn core_terminal_tick_commits_then_later_steps_fail_before_input_validation() {
        let mut simulation = Simulation::new(s1m4_runtime_package(
            100,
            FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
        ))
        .expect("S1-M4 world starts");
        let report = simulation.step(&[]).expect("fatal Tick completes");
        assert_eq!(
            report.run_status,
            RunStatus::Ended {
                completed_tick: Tick(0),
                cause: RunEndCause::MainCoreDestroyed,
            }
        );
        assert_eq!(report.state_hash, simulation.state_hash());
        let next_tick = simulation.next_tick();
        let hash = simulation.state_hash();
        let invalid_input = WorldInputEvent::HostileFrame {
            target_tick: Tick(u64::MAX),
            hostiles: vec![],
        };
        assert_eq!(
            simulation.step_with_world_inputs(&[], &[invalid_input]),
            Err(SimulationError::RunEnded)
        );
        assert_eq!(simulation.next_tick(), next_tick);
        assert_eq!(simulation.state_hash(), hash);
    }

    #[test]
    fn construction_contact_damage_analyzer_is_sorted_and_read_only() {
        let simulation = Simulation::new(s1m4_runtime_package(
            100,
            FixedVec2::new(Fixed(4 * crate::FIXED_ONE), Fixed::ZERO),
        ))
        .expect("S1-M4 world starts");
        let tick = simulation.next_tick();
        let hash = simulation.state_hash();
        let first = simulation
            .construction_contact_damage_analyzer_snapshot()
            .expect("analyzer recomputation fits")
            .expect("S1-M4 analyzer is enabled");
        let second = simulation
            .construction_contact_damage_analyzer_snapshot()
            .expect("repeated analyzer recomputation fits")
            .expect("S1-M4 analyzer remains enabled");
        assert_eq!(first, second);
        assert_eq!(first.next_tick, tick);
        assert_eq!(first.run_status, RunStatus::Running);
        assert!(first.damage.is_sorted_by_key(|row| row.target));
        assert!(first.armed_wires.is_sorted_by_key(|row| row.wire));
        assert_eq!(simulation.next_tick(), tick);
        assert_eq!(simulation.state_hash(), hash);
    }

    fn s1m4_runtime_with_wire() -> (Simulation, WireId) {
        let pitch = crate::REFERENCE_WORLD_ROUTING_PITCH.0;
        let mut simulation = Simulation::new(s1m4_runtime_package(
            100,
            FixedVec2::new(Fixed(64 * pitch), Fixed::ZERO),
        ))
        .expect("S1-M4 world starts");
        let placed = simulation
            .step(&[crate::CommandEnvelope {
                target_tick: Tick(0),
                ordinal: 0,
                command: crate::Command::PlaceWire(crate::PlaceWireCommand {
                    routing_domain: crate::RoutingDomain::OpenWorld,
                    points: vec![
                        FixedVec2::new(Fixed(4 * pitch), Fixed::ZERO),
                        FixedVec2::new(Fixed(12 * pitch), Fixed::ZERO),
                    ],
                    endpoint_a: crate::EndpointTarget::Free,
                    endpoint_b: crate::EndpointTarget::Free,
                }),
            }])
            .expect("S1-M4 Wire placement commits");
        assert!(placed.command_rejections.is_empty());
        let wire = WireId(
            placed.command_acceptances[0]
                .created_entity
                .expect("Wire placement allocates an identity"),
        );
        (simulation, wire)
    }

    fn s1m4_runtime_with_mobile_wire() -> (Simulation, MobileId, WireId) {
        let (mut simulation, _) = s1m4_runtime_with_wire();
        let pitch = simulation.profiles.physical_scale.world_routing_pitch.0;
        let circuit = simulation.profiles.physical_scale.circuit_routing_pitch.0;
        let routing_area = crate::FixedAabb::new(
            FixedVec2::new(Fixed(-12 * circuit), Fixed(-12 * circuit)),
            FixedVec2::new(Fixed(12 * circuit), Fixed(12 * circuit)),
        );
        let footprint = routing_area;
        let placed_mobile = simulation
            .step(&[crate::CommandEnvelope {
                target_tick: simulation.next_tick(),
                ordinal: 0,
                command: crate::Command::PlaceMobileSubstrate(crate::PlaceMobileSubstrateCommand {
                    origin: FixedVec2::new(Fixed(5 * pitch), Fixed::ZERO),
                    routing_area,
                    footprint,
                }),
            }])
            .expect("nonzero-origin Mobile placement commits");
        assert!(
            placed_mobile.command_rejections.is_empty(),
            "Mobile placement rejected: {:?}",
            placed_mobile.command_rejections,
        );
        let mobile = MobileId(
            placed_mobile.command_acceptances[0]
                .created_entity
                .expect("Mobile placement allocates an identity"),
        );

        let placed_wire = simulation
            .step(&[crate::CommandEnvelope {
                target_tick: simulation.next_tick(),
                ordinal: 0,
                command: crate::Command::PlaceWire(crate::PlaceWireCommand {
                    routing_domain: crate::RoutingDomain::MobileSubstrate(mobile.entity_id()),
                    points: vec![
                        FixedVec2::new(Fixed(-2 * circuit), Fixed(8 * circuit)),
                        FixedVec2::new(Fixed(2 * circuit), Fixed(8 * circuit)),
                    ],
                    endpoint_a: crate::EndpointTarget::Free,
                    endpoint_b: crate::EndpointTarget::Free,
                }),
            }])
            .expect("Mobile-local Wire placement commits");
        assert!(
            placed_wire.command_rejections.is_empty(),
            "Mobile-local Wire placement rejected: {:?}",
            placed_wire.command_rejections,
        );
        let wire = WireId(
            placed_wire.command_acceptances[0]
                .created_entity
                .expect("Mobile-local Wire placement allocates an identity"),
        );
        (simulation, mobile, wire)
    }

    fn mobile_wire_test_track_graph(simulation: &Simulation) -> TrackGraph {
        TrackGraph::compile(
            simulation.canonical.structural.wires(),
            simulation.canonical.structural.junctions(),
        )
        .expect("Mobile Wire Track graph compiles")
    }

    #[test]
    fn mobile_wire_sensing_uses_world_geometry_for_enemy_and_hostile() {
        let (mut simulation, _, wire) = s1m4_runtime_with_mobile_wire();
        let graph = mobile_wire_test_track_graph(&simulation);
        let pitch = simulation.profiles.physical_scale.world_routing_pitch.0;
        let circuit = simulation.profiles.physical_scale.circuit_routing_pitch.0;
        let world_point = FixedVec2::new(Fixed(5 * pitch), Fixed(8 * circuit));
        let local_ghost = FixedVec2::new(Fixed::ZERO, Fixed(8 * circuit));
        let far = FixedVec2::new(Fixed(64 * pitch), Fixed::ZERO);
        let enemy = simulation
            .canonical
            .enemies
            .iter()
            .next()
            .expect("test Enemy exists")
            .id();
        let occupied = |snapshot: &Phase1Snapshot| {
            snapshot
                .wire_sensing
                .iter()
                .find(|sample| sample.id == wire)
                .expect("Mobile Wire has a sensing row")
                .occupied
        };
        let sample = |simulation: &Simulation, hostiles: &[HostileCollider]| {
            let before = simulation.state_hash();
            let snapshot = run_phase1_snapshot_and_world_sample(
                &simulation.canonical,
                &graph,
                hostiles,
                true,
                simulation.profiles.balance.sense_radius,
                simulation.profiles.physical_scale.world_routing_pitch,
                simulation.profiles.balance.contact_damage_probe.as_ref(),
            )
            .expect("Phase 1 world sensing succeeds");
            assert_eq!(simulation.state_hash(), before, "Phase 1 remains read-only");
            snapshot
        };

        simulation
            .canonical
            .enemies
            .get_mut(enemy)
            .expect("test Enemy remains alive")
            .set_position(world_point);
        assert!(occupied(&sample(&simulation, &[])));
        simulation
            .canonical
            .enemies
            .get_mut(enemy)
            .expect("test Enemy remains alive")
            .set_position(local_ghost);
        assert!(!occupied(&sample(&simulation, &[])));

        simulation
            .canonical
            .enemies
            .get_mut(enemy)
            .expect("test Enemy remains alive")
            .set_position(far);
        assert!(occupied(&sample(
            &simulation,
            &[HostileCollider {
                id: 77,
                center: world_point,
                radius: Fixed::ZERO,
            }],
        )));
        assert!(!occupied(&sample(
            &simulation,
            &[HostileCollider {
                id: 77,
                center: local_ghost,
                radius: Fixed::ZERO,
            }],
        )));
    }

    #[test]
    fn mobile_construction_targets_use_world_geometry_for_all_routed_kinds() {
        let (simulation, mobile, _) = s1m4_runtime_with_mobile_wire();
        let graph = mobile_wire_test_track_graph(&simulation);
        let circuit = simulation.profiles.physical_scale.circuit_routing_pitch.0;
        let mobile_record = simulation
            .canonical
            .structural
            .mobile_substrates()
            .iter_alive()
            .find(|(_, record)| record.id == mobile)
            .map(|(_, record)| record)
            .expect("test Mobile remains alive");
        let world_origin = graph
            .world_position(mobile_record.track_position)
            .expect("Mobile Track position resolves");
        let domain = crate::RoutingDomain::MobileSubstrate(mobile.entity_id());
        let targets = [
            crate::ConstructionTarget::Gate {
                gate_type: GateType::Not,
                origin: FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
                routing_domain: domain,
            },
            crate::ConstructionTarget::Junction {
                routing_domain: domain,
                position: FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
            },
            crate::ConstructionTarget::Wire {
                routing_domain: domain,
                points: vec![
                    FixedVec2::new(Fixed(-circuit), Fixed::ZERO),
                    FixedVec2::new(Fixed(circuit), Fixed::ZERO),
                ],
                endpoint_a: crate::EndpointTarget::Free,
                endpoint_b: crate::EndpointTarget::Free,
            },
        ];
        let before = simulation.state_hash();
        for (index, target) in targets.into_iter().enumerate() {
            let site = crate::ConstructionSiteId(EntityId(
                100 + u64::try_from(index).expect("small test index fits"),
            ));
            let sites = ConstructionSiteStore::new(vec![crate::ConstructionSite {
                id: site,
                target,
                required_work: Energy(1),
                completed_work: Energy(0),
                activation_ready: false,
            }])
            .expect("test Site is structurally valid");
            assert_eq!(
                simulation.canonical.structural.smallest_intersecting_site(
                    &sites,
                    world_origin,
                    mobile_record.footprint,
                    &graph,
                    &simulation.profiles.physical_scale,
                ),
                Ok(Some(site)),
                "world-transformed routed Site kind {index} is reachable",
            );
            assert_eq!(
                simulation.canonical.structural.smallest_intersecting_site(
                    &sites,
                    FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
                    mobile_record.footprint,
                    &graph,
                    &simulation.profiles.physical_scale,
                ),
                Ok(None),
                "raw Mobile-local ghost for Site kind {index} is not world geometry",
            );
        }
        assert_eq!(
            simulation.state_hash(),
            before,
            "Site reach query is read-only"
        );
    }

    #[test]
    fn mobile_wire_contact_and_enemy_attack_use_world_geometry_not_local_ghost() {
        let (simulation, _, wire) = s1m4_runtime_with_mobile_wire();
        let graph = mobile_wire_test_track_graph(&simulation);
        let pitch = simulation.profiles.physical_scale.world_routing_pitch.0;
        let circuit = simulation.profiles.physical_scale.circuit_routing_pitch.0;
        let world_point = FixedVec2::new(Fixed(5 * pitch), Fixed(8 * circuit));
        let local_ghost = FixedVec2::new(Fixed::ZERO, Fixed(8 * circuit));
        let enemy = simulation
            .canonical
            .enemies
            .iter()
            .next()
            .expect("test Enemy exists")
            .id();
        let staged = |point| StagedEnemyTrajectory {
            enemy,
            start: point,
            end: point,
            radius: Fixed(crate::FIXED_ONE),
        };
        let phase5 = || Phase5Output {
            report: Some(PowerStepReport {
                loads: vec![crate::PowerLoadReport {
                    demand: DemandId::new(wire.entity_id(), DemandKind::LiveWire),
                    region: crate::PowerRegionId(1),
                    nominal: Energy(30),
                    granted: Energy(30),
                    ratio: PowerRatio::ONE,
                    source_route: None,
                    transmission_loss: Energy(0),
                    source_cost: Energy(0),
                }],
                ..PowerStepReport::default()
            }),
            private_heat: Vec::new(),
        };
        let run = |point| {
            let mut phase5 = phase5();
            run_phase8_interaction(
                &simulation.canonical,
                &mut phase5,
                Phase8Inputs {
                    staged_mobiles: &[],
                    mobile_intents: &[],
                    staged_enemies: &[staged(point)],
                    live_wires: &[LiveWireIntent {
                        wire,
                        nominal: Energy(30),
                    }],
                    track_graph: &graph,
                    physical: &simulation.profiles.physical_scale,
                    balance: &simulation.profiles.balance,
                },
            )
            .expect("Phase 8 interaction succeeds")
        };
        let before = simulation.state_hash();
        let actual = run(world_point);
        assert_eq!(actual.contacts.len(), 1);
        assert_eq!(actual.contacts[0].wire, wire);
        assert_eq!(actual.contacts[0].target, enemy);
        assert!(actual.exposures.iter().any(|exposure| {
            exposure.target == enemy.entity_id() && exposure.source == wire.entity_id()
        }));
        assert!(actual.exposures.iter().any(|exposure| {
            exposure.target == wire.entity_id() && exposure.source == enemy.entity_id()
        }));

        let ghost = run(local_ghost);
        assert!(ghost.contacts.is_empty());
        assert!(ghost.exposures.is_empty());
        assert_eq!(
            simulation.state_hash(),
            before,
            "Phase 8 geometry is read-only"
        );
    }

    #[test]
    fn canonical_enemy_is_a_wire_sensing_collider_without_a_hostile_frame() {
        let pitch = crate::REFERENCE_WORLD_ROUTING_PITCH.0;
        let mut simulation = Simulation::new(s1m4_runtime_package(
            100,
            FixedVec2::new(Fixed(6 * pitch), Fixed::ZERO),
        ))
        .expect("S1-M4 world starts");
        let placed = simulation
            .step(&[crate::CommandEnvelope {
                target_tick: Tick(0),
                ordinal: 0,
                command: crate::Command::PlaceWire(crate::PlaceWireCommand {
                    routing_domain: crate::RoutingDomain::OpenWorld,
                    points: vec![
                        FixedVec2::new(Fixed(4 * pitch), Fixed::ZERO),
                        FixedVec2::new(Fixed(12 * pitch), Fixed::ZERO),
                    ],
                    endpoint_a: crate::EndpointTarget::Free,
                    endpoint_b: crate::EndpointTarget::Free,
                }),
            }])
            .expect("S1-M4 Wire placement commits");
        let wire = WireId(
            placed.command_acceptances[0]
                .created_entity
                .expect("Wire placement allocates an identity"),
        );

        let sense = simulation
            .wire_sense_state(wire)
            .expect("placed S1-M4 Wire has Sense state");
        assert!(sense.sampled_presence);
        assert_eq!(sense.intended_level, LogicLevel::High);
    }

    #[test]
    fn v5_mobile_build_ports_validate_after_a_real_tick_commit() {
        let (mut simulation, _) = s1m4_runtime_with_wire();
        let pitch = simulation.profiles.physical_scale.world_routing_pitch.0;
        let circuit_pitch = simulation.profiles.physical_scale.circuit_routing_pitch.0;
        let local_bounds = crate::FixedAabb::new(
            FixedVec2::new(Fixed(-4 * circuit_pitch), Fixed(-4 * circuit_pitch)),
            FixedVec2::new(Fixed(4 * circuit_pitch), Fixed(4 * circuit_pitch)),
        );
        let placed = simulation
            .step(&[crate::CommandEnvelope {
                target_tick: Tick(1),
                ordinal: 0,
                command: crate::Command::PlaceMobileSubstrate(crate::PlaceMobileSubstrateCommand {
                    origin: FixedVec2::new(Fixed(5 * pitch), Fixed::ZERO),
                    routing_area: local_bounds,
                    footprint: local_bounds,
                }),
            }])
            .expect("v5 Mobile commit passes final canonical validation");
        assert!(placed.command_rejections.is_empty());
        let mobile = MobileId(
            placed.command_acceptances[0]
                .created_entity
                .expect("Mobile placement allocates an identity"),
        );
        let ports = simulation
            .canonical
            .signal
            .mobile_ports(mobile)
            .expect("Mobile signal lifecycle exists");
        assert!(ports.build.is_some());
        assert_eq!(
            simulation
                .canonical
                .signal
                .canonical_driver_slots()
                .filter_map(|(_, record)| record)
                .filter(|record| {
                    record.owner == mobile.entity_id()
                        && record.role == DriverRole::ExternalMobileBuild
                })
                .count(),
            1
        );
        validate_canonical_world(&simulation.canonical)
            .expect("post-step BUILD sink/driver registry is coherent");
    }

    #[test]
    fn phase9_keeps_retained_power_heat_separate_from_live_wire_remainder() {
        let (mut simulation, wire) = s1m4_runtime_with_wire();
        let live_demand = DemandId::new(wire.entity_id(), DemandKind::LiveWire);
        let interaction = [InteractionHeatReport {
            owner: wire.entity_id(),
            kind: InteractionHeatKind::LiveWireRemainder,
            demand: Some(live_demand),
            energy: HeatEnergy(4),
        }];
        let retained = [
            PowerHeatReport {
                owner: wire,
                kind: crate::PowerHeatKind::LeakageDissipation,
                demand: DemandId::new(wire.entity_id(), DemandKind::WireLeakage),
                energy: HeatEnergy(1),
            },
            PowerHeatReport {
                owner: wire,
                kind: crate::PowerHeatKind::TransmissionLoss,
                demand: live_demand,
                energy: HeatEnergy(2),
            },
            PowerHeatReport {
                owner: wire,
                kind: crate::PowerHeatKind::OvercapacitySupport,
                demand: DemandId::new(wire.entity_id(), DemandKind::OvercapacitySupport),
                energy: HeatEnergy(3),
            },
        ];
        run_phase9_thermal_integration(
            &mut simulation.canonical,
            &interaction,
            &retained,
            &simulation.profiles.balance,
        )
        .expect("all four heat causes integrate exactly once");
        assert_eq!(
            simulation
                .canonical
                .structural
                .damage_state(wire.entity_id())
                .expect("Wire is damageable")
                .1
                .heat_energy,
            HeatEnergy(10)
        );
    }

    #[test]
    fn phase10_rejects_orphan_exposure_before_mutating_canonical_state() {
        let mut simulation = Simulation::new(s1m4_runtime_package(
            100,
            FixedVec2::new(
                Fixed(64 * crate::REFERENCE_WORLD_ROUTING_PITCH.0),
                Fixed::ZERO,
            ),
        ))
        .expect("S1-M4 world starts");
        let core = simulation.canonical.main_core.expect("S1-M4 Core exists");
        let snapshot = Phase1Snapshot {
            core: Some(CoreDamagePhase1Snapshot {
                target: core.id().entity_id(),
                integrity: core.integrity(),
                temperature: Fixed::ZERO,
            }),
            ..Phase1Snapshot::default()
        };
        let before = canonical::state_hash(simulation.canonical.state_view());
        assert_eq!(
            run_phase10_damage_resolution(
                &mut simulation.canonical,
                &snapshot,
                &[ElectricalExposure {
                    target: EntityId(u64::MAX),
                    source: EntityId(1),
                    energy: Energy(1),
                }],
                &simulation.profiles.balance,
            ),
            Err(SimulationError::InvalidCanonicalState)
        );
        assert_eq!(
            canonical::state_hash(simulation.canonical.state_view()),
            before,
            "orphan exposure rejection is atomic"
        );
    }

    fn retained_world_package_with_power_probe(
        scenario_id: &'static str,
        initial_world: InitialWorld,
        required_features: StageFeatureSet,
    ) -> SimulationPackage {
        let profiles = ProfileBundle {
            numeric: NumericProfile::reference_v1("numeric-power-probe-coherence"),
            physical_scale: PhysicalScaleProfile::stage0_alpha("physical-power-probe-coherence"),
            balance: BalanceProfile::power_probe_alpha("balance-power-probe-coherence"),
        };
        let contract = SimulationContract::from_profiles(&profiles).expect("profiles are valid");
        SimulationPackage::new(
            scenario_id,
            initial_world,
            required_features,
            contract,
            profiles,
        )
    }

    #[test]
    fn balance_v3_power_probe_rejects_retained_empty_world_before_tick_zero() {
        let package = retained_world_package_with_power_probe(
            "empty-with-power-probe",
            InitialWorld::Empty,
            StageFeatureSet::none(),
        );

        assert_eq!(
            Simulation::new(package).err(),
            Some(SimulationError::PowerProbeRequiresMainCorePowerWorld)
        );

        let with_power_feature = retained_world_package_with_power_probe(
            "empty-with-power-feature-and-probe",
            InitialWorld::Empty,
            StageFeatureSet {
                power: true,
                ..StageFeatureSet::none()
            },
        );
        assert_eq!(
            Simulation::new(with_power_feature).err(),
            Some(SimulationError::PowerFeaturesRequireMainCorePowerWorld),
            "the retained feature/world coherence error must precede the probe/world error"
        );
    }

    #[test]
    fn balance_v3_power_probe_rejects_retained_main_core_world_before_tick_zero() {
        let package = retained_world_package_with_power_probe(
            "main-core-with-power-probe",
            InitialWorld::MainCoreV1 {
                position: FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
                integrity: crate::Integrity(1),
                heat_energy: crate::HeatEnergy(0),
            },
            StageFeatureSet {
                capacity: true,
                ..StageFeatureSet::none()
            },
        );

        assert_eq!(
            Simulation::new(package).err(),
            Some(SimulationError::PowerProbeRequiresMainCorePowerWorld)
        );

        let without_capacity = retained_world_package_with_power_probe(
            "main-core-without-capacity-and-with-probe",
            InitialWorld::MainCoreV1 {
                position: FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
                integrity: crate::Integrity(1),
                heat_energy: crate::HeatEnergy(0),
            },
            StageFeatureSet::none(),
        );
        assert_eq!(
            Simulation::new(without_capacity).err(),
            Some(SimulationError::MainCoreRequiresCapacity),
            "the retained capacity/world coherence error must precede the probe/world error"
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
            required_features: canonical.required_features,
            initial_state_hash: canonical.initial_state_hash,
            world_generator_version: canonical.world_generator_version,
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
    fn capacity_accounting_and_analyzer_are_invariant_to_wire_store_layout() {
        let mut profiles = ProfileBundle {
            numeric: NumericProfile::reference_v1("numeric-capacity-layout"),
            physical_scale: PhysicalScaleProfile::stage0_alpha("physical-capacity-layout"),
            balance: BalanceProfile::capacity_probe_alpha("balance-capacity-layout"),
        };
        profiles
            .balance
            .capacity_probe
            .as_mut()
            .expect("capacity section exists")
            .main_core_capacity = 1_000;
        let contract = SimulationContract::from_profiles(&profiles).expect("profiles are valid");
        let package = SimulationPackage::new(
            "capacity-layout",
            InitialWorld::MainCoreV1 {
                position: FixedVec2::new(Fixed(0), Fixed(0)),
                integrity: crate::Integrity(1),
                heat_energy: crate::HeatEnergy(0),
            },
            StageFeatureSet {
                capacity: true,
                ..StageFeatureSet::none()
            },
            contract,
            profiles,
        );
        let mut canonical = Simulation::new(package).expect("capacity simulation starts");
        let point = |x: i64, y: i64| FixedVec2::new(Fixed(x), Fixed(y));
        let pitch = crate::FIXED_ONE;
        let commands = [
            crate::CommandEnvelope {
                target_tick: Tick(0),
                ordinal: 0,
                command: crate::Command::PlaceWire(crate::PlaceWireCommand {
                    routing_domain: crate::RoutingDomain::OpenWorld,
                    points: vec![point(0, 0), point(2 * pitch, 0)],
                    endpoint_a: crate::EndpointTarget::MainCoreAnchor(crate::MainCoreId(
                        crate::EntityId(1),
                    )),
                    endpoint_b: crate::EndpointTarget::Free,
                }),
            },
            crate::CommandEnvelope {
                target_tick: Tick(0),
                ordinal: 1,
                command: crate::Command::PlaceWire(crate::PlaceWireCommand {
                    routing_domain: crate::RoutingDomain::OpenWorld,
                    points: vec![point(0, 4 * pitch), point(3 * pitch, 4 * pitch)],
                    endpoint_a: crate::EndpointTarget::Free,
                    endpoint_b: crate::EndpointTarget::Free,
                }),
            },
        ];
        canonical.step(&commands).expect("two capacity Wires place");

        let mut reordered = Simulation {
            scenario_id: canonical.scenario_id.clone(),
            canonical: canonical.canonical.clone(),
            profiles: canonical.profiles.clone(),
            required_features: canonical.required_features,
            initial_state_hash: canonical.initial_state_hash,
            world_generator_version: canonical.world_generator_version,
        };
        reordered
            .canonical
            .structural
            .reserve_layout_capacity_for_test(128);
        reordered
            .canonical
            .structural
            .swap_wire_slots_for_test(crate::WireIndex(0), crate::WireIndex(1))
            .expect("test-only Wire slots swap");
        validate_canonical_world(&reordered.canonical).expect("reordered layout remains valid");
        assert_eq!(canonical.state_hash(), reordered.state_hash());
        assert_eq!(
            canonical
                .network_analyzer_snapshot()
                .expect("canonical Analyzer fits"),
            reordered
                .network_analyzer_snapshot()
                .expect("reordered Analyzer fits")
        );

        let canonical_report = canonical.step(&[]).expect("canonical Tick succeeds");
        let reordered_report = reordered.step(&[]).expect("reordered Tick succeeds");
        assert_eq!(canonical_report, reordered_report);
        assert_eq!(canonical.state_hash(), reordered.state_hash());
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

    #[test]
    fn phase8_is_the_only_seam_that_publishes_phase5_heat_scratch() {
        let heat = PowerHeatReport {
            owner: WireId(crate::EntityId(3)),
            kind: crate::PowerHeatKind::LeakageDissipation,
            demand: DemandId::new(crate::EntityId(3), DemandKind::WireLeakage),
            energy: crate::HeatEnergy(7),
        };
        let mut phase5 = Phase5Output {
            report: Some(PowerStepReport::default()),
            private_heat: vec![heat],
        };
        assert!(
            phase5
                .report
                .as_ref()
                .expect("Power report exists")
                .heat_contributions
                .is_empty()
        );

        let simulation = Simulation::new(package()).expect("test package is valid");
        let track_graph = TrackGraph::compile(
            simulation.canonical.structural.wires(),
            simulation.canonical.structural.junctions(),
        )
        .expect("empty Track graph compiles");
        run_phase8_interaction(
            &simulation.canonical,
            &mut phase5,
            Phase8Inputs {
                staged_mobiles: &[],
                mobile_intents: &[],
                staged_enemies: &[],
                live_wires: &[],
                track_graph: &track_graph,
                physical: &simulation.profiles.physical_scale,
                balance: &simulation.profiles.balance,
            },
        )
        .expect("Phase 8 publishes derived heat");

        assert!(phase5.private_heat.is_empty());
        assert_eq!(
            phase5
                .report
                .expect("Power report remains available")
                .heat_contributions,
            vec![heat]
        );
    }

    #[test]
    fn enabled_power_analyzer_preserves_signal_frontiers_and_state_hash() {
        const NUMERIC: &[u8] = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../profiles/numeric/v1.json"
        ));
        const PHYSICAL: &[u8] = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../profiles/physical-scale/stage0-alpha.json"
        ));
        const BALANCE: &[u8] = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../profiles/balance/s1-m2-power-probe-alpha.json"
        ));
        const SCENARIO: &[u8] = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/scenarios/s1-m2-c08-brownout-full-v1.json"
        ));
        const REPLAY: &[u8] = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/replays/s1-m2-c08-brownout-full-v1.json"
        ));
        let package = crate::decode_package(crate::ArtifactBytes {
            scenario: SCENARIO,
            numeric_profile: NUMERIC,
            physical_scale_profile: PHYSICAL,
            balance_profile: BALANCE,
        })
        .expect("the retained C08 package decodes");
        let (_, replay) = crate::decode_replay_artifact(REPLAY)
            .expect("the retained C08 Replay decodes")
            .into_parts();
        let mut simulation = Simulation::new(package).expect("the retained C08 Simulation starts");
        while simulation.next_tick() < Tick(3) {
            let tick = simulation.next_tick();
            let commands = replay.commands_for_tick(tick).cloned().collect::<Vec<_>>();
            let world_inputs = replay
                .world_inputs_for_tick(tick)
                .cloned()
                .collect::<Vec<_>>();
            simulation
                .step_with_world_inputs(&commands, &world_inputs)
                .expect("the retained construction Tick succeeds");
        }
        let hash_before = simulation.state_hash();
        let driver_frontier_before = simulation.canonical.signal.driver_frontier();
        let sink_frontier_before = simulation.canonical.signal.sink_frontier();

        let first = simulation
            .power_sense_analyzer_snapshot()
            .expect("the first analyzer read succeeds")
            .expect("Power is enabled");
        let second = simulation
            .power_sense_analyzer_snapshot()
            .expect("the repeated analyzer read succeeds")
            .expect("Power remains enabled");

        assert_eq!(first, second);
        assert_eq!(simulation.state_hash(), hash_before);
        assert_eq!(
            simulation.canonical.signal.driver_frontier(),
            driver_frontier_before
        );
        assert_eq!(
            simulation.canonical.signal.sink_frontier(),
            sink_frontier_before
        );
    }

    #[test]
    fn signal_quiescence_snapshot_counts_every_deferred_mechanism_without_mutation() {
        let mut simulation = Simulation::new(package()).expect("test Simulation starts");
        let baseline = simulation
            .signal_quiescence_snapshot()
            .expect("quiescence reads");
        assert_eq!(baseline.next_tick, Tick(0));
        assert!(baseline.is_quiescent());

        let driver = DriverId(EntityId(1));
        let sink = SinkId(EntityId(1));
        simulation
            .canonical
            .driver_events
            .stage(
                &mut simulation.canonical.event_payloads,
                [DriverTransition::s0m3(
                    Tick(2),
                    driver,
                    LogicLevel::High,
                    DriveStrength(1),
                    0,
                    DriverTransitionCause::ExternalDriver,
                )],
            )
            .expect("test Driver event stages");
        stage_signal_arrivals(
            &mut simulation.canonical.signal_events,
            &mut simulation.canonical.event_payloads,
            &mut simulation.canonical.path_certificates,
            [UncertifiedSignalArrival::topology_sync(
                Tick(3),
                driver,
                sink,
                DriverSample::s0m3(driver, LogicLevel::Low, DriveStrength(0), Tick(0)),
                Vec::new(),
            )],
        )
        .expect("test Signal arrival stages");
        let gate = GateId(EntityId(2));
        simulation
            .canonical
            .signal
            .activate_gate(gate, GateType::Not, Tick(0))
            .expect("test Gate activates");
        simulation
            .canonical
            .signal
            .set_pending(gate, Tick(4), LogicLevel::High, Energy(1))
            .expect("test Gate transition stages");
        let hash_before = simulation.state_hash();
        let first = simulation
            .signal_quiescence_snapshot()
            .expect("first read succeeds");
        let second = simulation
            .signal_quiescence_snapshot()
            .expect("second read succeeds");
        assert_eq!(first, second);
        assert_eq!(first.pending_driver_transitions, 1);
        assert_eq!(first.pending_signal_arrivals, 1);
        assert_eq!(first.pending_gate_transitions, 1);
        assert!(!first.is_quiescent());
        assert_eq!(simulation.state_hash(), hash_before);
        assert_eq!(simulation.next_tick(), Tick(0));
    }

    #[test]
    fn signal_quiescence_ignores_unscheduled_unpowered_desired_level() {
        let mut simulation = Simulation::new(package()).expect("test Simulation starts");
        let gate = GateId(EntityId(2));
        simulation
            .canonical
            .signal
            .activate_gate(gate, GateType::Not, Tick(0))
            .expect("test NOT Gate activates");
        simulation
            .canonical
            .signal
            .set_gate_desired_level(gate, LogicLevel::High)
            .expect("test desired level updates");

        let hash_before = simulation.state_hash();
        let snapshot = simulation
            .signal_quiescence_snapshot()
            .expect("quiescence reads");
        assert_eq!(snapshot.pending_driver_transitions, 0);
        assert_eq!(snapshot.pending_signal_arrivals, 0);
        assert_eq!(snapshot.pending_gate_transitions, 0);
        assert!(snapshot.is_quiescent());
        assert_eq!(simulation.state_hash(), hash_before);
        assert_eq!(simulation.next_tick(), Tick(0));
    }
}
