use crate::{
    AbsoluteModuleGeometry, ArtifactHash, BindPortCommand, Command, CommandAcceptance,
    CommandEncodingError, CommandEnvelope, EndpointTarget, EnemyId, EntityId, Fixed, FixedAabb,
    FixedVec2, GateBlueprint, GateId, GatePort, GatePortRef, GateType, HashAlgorithmId,
    HashParseError, JsonErrorCategory, JunctionBlueprint, JunctionId, MainCoreId, MobileId,
    MobilePort, MobilePortRef, ModuleBlueprint, ModuleContract, ModuleEndpoint, ModuleError,
    ModuleFormatVersion, ModuleLocalId, ModuleProvenance, ModuleRoutingDomain,
    PhysicalScaleProfile, PlaceFixedSubstrateCommand, PlaceGateCommand, PlaceJunctionCommand,
    PlaceMobileSubstrateCommand, PlaceWireCommand, PowerSourceId, ProfileHash, RoutingDomain,
    RunStatus, SEMANTICS_VERSION_V1, SemanticsVersion, Simulation, SimulationContract,
    SimulationError, SubstrateBlueprint, Tick, WireBlueprint, WireEnd, WireId, WireSensePortRef,
    validate_module_against,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::ops::Range;
use thiserror::Error;

pub const REFERENCE_ARCHITECTURE_FORMAT_VERSION_V1: u32 = 1;
pub const REFERENCE_ARCHITECTURE_FORMAT_VERSION_V2: u32 = 2;
pub const REFERENCE_ARCHITECTURE_MAX_BINDING_BATCHES_V2: usize = 16;
pub const REFERENCE_ARCHITECTURE_MAX_BARRIER_TICKS_V2: u16 = 256;
const REFERENCE_ARCHITECTURE_HASH_DOMAIN_V1: &[u8] = b"AON\0REFERENCE-ARCHITECTURE\0V1\0";
const REFERENCE_ARCHITECTURE_HASH_DOMAIN_V2: &[u8] = b"AON\0REFERENCE-ARCHITECTURE\0V2\0";
const REFERENCE_ARCHITECTURE_CANONICAL_ENCODER_VERSION_V1: u16 = 1;
const REFERENCE_ARCHITECTURE_CANONICAL_ENCODER_VERSION_V2: u16 = 2;
const REFERENCE_COMMAND_LOG_HASH_DOMAIN: &[u8] = b"AON\0REFERENCE-COMMAND-LOG\0V1\0";
const REFERENCE_COMMAND_LOG_CANONICAL_ENCODER_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReferenceArchitectureFormatVersion {
    #[default]
    V1,
    V2,
}

impl ReferenceArchitectureFormatVersion {
    pub const fn as_u32(self) -> u32 {
        match self {
            Self::V1 => REFERENCE_ARCHITECTURE_FORMAT_VERSION_V1,
            Self::V2 => REFERENCE_ARCHITECTURE_FORMAT_VERSION_V2,
        }
    }

    fn parse(value: u32) -> Result<Self, ReferenceArchitectureError> {
        match value {
            REFERENCE_ARCHITECTURE_FORMAT_VERSION_V1 => Ok(Self::V1),
            REFERENCE_ARCHITECTURE_FORMAT_VERSION_V2 => Ok(Self::V2),
            actual => Err(ReferenceArchitectureError::UnsupportedFormatVersion {
                expected: REFERENCE_ARCHITECTURE_FORMAT_VERSION_V2,
                actual,
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReferenceArchitectureBindingEndpoint {
    pub wire: ReferenceArchitectureLocalId,
    pub end: WireEnd,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceArchitectureMaterializationSchedule {
    /// Ordered binding stages. Empty intermediate batches are meaningful and hash-bound.
    pub binding_batches: Vec<Vec<ReferenceArchitectureBindingEndpoint>>,
}

/// Artifact-local identity. `0` is reserved so an omitted/default identity can never bind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReferenceArchitectureLocalId(u32);

impl ReferenceArchitectureLocalId {
    pub const fn new(value: u32) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for ReferenceArchitectureLocalId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReferenceArchitectureRoutingDomain {
    OpenWorld,
    FixedSubstrate(ReferenceArchitectureLocalId),
    MobileSubstrate(ReferenceArchitectureLocalId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReferenceArchitectureEndpoint {
    Free,
    Junction(ReferenceArchitectureLocalId),
    GatePort {
        gate: ReferenceArchitectureLocalId,
        port: GatePort,
    },
    MobilePort {
        mobile: ReferenceArchitectureLocalId,
        port: MobilePort,
    },
    MainCore,
    PowerSource {
        ordinal: u32,
    },
    WireSensePort {
        wire: ReferenceArchitectureLocalId,
        end: WireEnd,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReferenceFixedSubstrate {
    pub id: ReferenceArchitectureLocalId,
    pub origin: FixedVec2,
    pub routing_area: FixedAabb,
    pub footprint: FixedAabb,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReferenceMobileSubstrate {
    pub id: ReferenceArchitectureLocalId,
    /// Exact initial world position on an existing open-world track.
    pub origin: FixedVec2,
    pub routing_area: FixedAabb,
    pub footprint: FixedAabb,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReferenceGate {
    pub id: ReferenceArchitectureLocalId,
    pub routing_domain: ReferenceArchitectureRoutingDomain,
    pub gate_type: GateType,
    /// World coordinates for a fixed substrate and substrate-local coordinates for a mobile one.
    pub origin: FixedVec2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReferenceJunction {
    pub id: ReferenceArchitectureLocalId,
    pub routing_domain: ReferenceArchitectureRoutingDomain,
    pub position: FixedVec2,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceWire {
    pub id: ReferenceArchitectureLocalId,
    pub routing_domain: ReferenceArchitectureRoutingDomain,
    pub points: Vec<FixedVec2>,
    pub endpoint_a: ReferenceArchitectureEndpoint,
    pub endpoint_b: ReferenceArchitectureEndpoint,
}

/// The closed set of Command-v1 placement primitives admitted by an architecture artifact.
/// Removal, construction, and direct driver mutation are deliberately not artifact operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReferenceArchitectureOperation {
    PlaceFixedSubstrate(ReferenceFixedSubstrate),
    PlaceMobileSubstrate(ReferenceMobileSubstrate),
    PlaceGate(ReferenceGate),
    PlaceJunction(ReferenceJunction),
    PlaceWire(ReferenceWire),
}

impl ReferenceArchitectureOperation {
    pub const fn local_id(&self) -> ReferenceArchitectureLocalId {
        match self {
            Self::PlaceFixedSubstrate(value) => value.id,
            Self::PlaceMobileSubstrate(value) => value.id,
            Self::PlaceGate(value) => value.id,
            Self::PlaceJunction(value) => value.id,
            Self::PlaceWire(value) => value.id,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReferenceArchitectureSemanticTarget {
    LocalEntity(ReferenceArchitectureLocalId),
    GatePort {
        gate: ReferenceArchitectureLocalId,
        port: GatePort,
    },
    MobilePort {
        mobile: ReferenceArchitectureLocalId,
        port: MobilePort,
    },
    WireSensePort {
        wire: ReferenceArchitectureLocalId,
        end: WireEnd,
    },
    MainCore,
    PowerSource {
        ordinal: u32,
    },
    Enemy {
        ordinal: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceArchitectureRoleBinding {
    pub name: String,
    pub target: ReferenceArchitectureSemanticTarget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceArchitectureObservationBinding {
    pub name: String,
    pub target: ReferenceArchitectureSemanticTarget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceArchitectureArtifact {
    pub format_version: ReferenceArchitectureFormatVersion,
    pub hash_algorithm_id: HashAlgorithmId,
    /// Human-facing only; excluded from the semantic artifact hash.
    pub display_name: String,
    pub contract: SimulationContract,
    pub operations: Vec<ReferenceArchitectureOperation>,
    pub role_bindings: Vec<ReferenceArchitectureRoleBinding>,
    pub observation_bindings: Vec<ReferenceArchitectureObservationBinding>,
    /// Required by format v2 and forbidden by format v1.
    pub materialization_schedule: Option<ReferenceArchitectureMaterializationSchedule>,
}

impl ReferenceArchitectureArtifact {
    pub fn semantic_hash(&self) -> Result<ArtifactHash, ReferenceArchitectureError> {
        let canonical = CanonicalReferenceArchitecture::new(self)?;
        let mut encoder = CanonicalEncoder::new();
        match self.format_version {
            ReferenceArchitectureFormatVersion::V1 => {
                encoder.bytes(REFERENCE_ARCHITECTURE_HASH_DOMAIN_V1);
                encoder.u16(REFERENCE_ARCHITECTURE_CANONICAL_ENCODER_VERSION_V1);
            }
            ReferenceArchitectureFormatVersion::V2 => {
                encoder.bytes(REFERENCE_ARCHITECTURE_HASH_DOMAIN_V2);
                encoder.u16(REFERENCE_ARCHITECTURE_CANONICAL_ENCODER_VERSION_V2);
            }
        }
        encoder.u32(self.format_version.as_u32());
        encoder.text(self.hash_algorithm_id.as_str())?;
        encoder.text(self.contract.semantics_version.as_str())?;
        encoder.bytes(self.contract.numeric_profile_hash.as_bytes());
        encoder.bytes(self.contract.physical_scale_profile_hash.as_bytes());
        encoder.bytes(self.contract.balance_profile_hash.as_bytes());
        canonical.encode(&mut encoder)?;
        Ok(ArtifactHash::from_bytes(
            *blake3::hash(&encoder.finish()).as_bytes(),
        ))
    }

    pub fn materialization_plan(
        &self,
    ) -> Result<ReferenceArchitectureMaterializationPlan, ReferenceArchitectureError> {
        let canonical = CanonicalReferenceArchitecture::new(self)?;
        Ok(canonical.materialization_plan())
    }

    /// Preflights every ordinal anchor before a caller begins an atomic materialization attempt.
    pub fn validate_scenario_resolution(
        &self,
        scenario: &ReferenceArchitectureScenarioResolution,
    ) -> Result<(), ReferenceArchitectureError> {
        let canonical = CanonicalReferenceArchitecture::new(self)?;
        canonical.validate_scenario_resolution(scenario)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReferenceArchitectureMaterializationStep {
    Placement(ReferenceArchitectureOperation),
    BindWireEnd {
        wire: ReferenceArchitectureLocalId,
        end: WireEnd,
        target: ReferenceArchitectureEndpoint,
    },
}

impl ReferenceArchitectureMaterializationStep {
    pub const fn created_local_id(&self) -> Option<ReferenceArchitectureLocalId> {
        match self {
            Self::Placement(operation) => Some(operation.local_id()),
            Self::BindWireEnd { .. } => None,
        }
    }

    pub fn resolve_command(
        &self,
        target_tick: Tick,
        ordinal: u64,
        local_entities: &BTreeMap<ReferenceArchitectureLocalId, EntityId>,
        scenario: &ReferenceArchitectureScenarioResolution,
    ) -> Result<CommandEnvelope, ReferenceArchitectureError> {
        let command = match self {
            Self::Placement(operation) => resolve_placement(operation, local_entities)?,
            Self::BindWireEnd { wire, end, target } => Command::BindPort(BindPortCommand {
                wire: WireId(resolve_local(*wire, local_entities)?),
                end: *end,
                target: resolve_endpoint(*target, local_entities, scenario)?,
            }),
        };
        Ok(CommandEnvelope {
            target_tick,
            ordinal,
            command,
        })
    }

    /// Records the one entity identity returned by a successfully accepted placement step.
    pub fn record_acceptance(
        &self,
        acceptance: CommandAcceptance,
        local_entities: &mut BTreeMap<ReferenceArchitectureLocalId, EntityId>,
    ) -> Result<(), ReferenceArchitectureError> {
        match (self.created_local_id(), acceptance.created_entity) {
            (Some(id), Some(entity)) => {
                if let Some((first_id, _)) = local_entities
                    .iter()
                    .find(|(_, existing)| **existing == entity)
                {
                    return Err(ReferenceArchitectureError::DuplicateResolvedEntity {
                        entity,
                        first_id: *first_id,
                        second_id: id,
                    });
                }
                if local_entities.insert(id, entity).is_some() {
                    return Err(ReferenceArchitectureError::DuplicateResolvedLocalId { id });
                }
                Ok(())
            }
            (Some(id), None) => Err(ReferenceArchitectureError::MissingCreatedEntity { id }),
            (None, Some(entity)) => {
                Err(ReferenceArchitectureError::UnexpectedCreatedEntity { entity })
            }
            (None, None) => Ok(()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceArchitectureMaterializationPlan {
    steps: Vec<ReferenceArchitectureMaterializationStep>,
    batch_kinds: Vec<ReferenceArchitectureMaterializationBatchKind>,
    batch_ranges: Vec<Range<usize>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceArchitectureMaterializationBatchKind {
    Placement { phase: u8 },
    Binding { stage: u8 },
}

impl ReferenceArchitectureMaterializationBatchKind {
    pub const fn diagnostic_phase(self) -> u8 {
        match self {
            Self::Placement { phase } => phase,
            Self::Binding { stage } => 6 + stage,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReferenceArchitectureExecutedBatchEvidence {
    pub kind: ReferenceArchitectureMaterializationBatchKind,
    pub command_tick: Tick,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceArchitectureBindingStageEvidence {
    pub stage: u8,
    pub command_tick: Tick,
    /// Completed Ticks advanced with an empty command batch while crossing the barrier.
    pub barrier_ticks: Vec<Tick>,
    /// The earliest boundary after this stage at which the required candidate set was quiescent.
    pub quiescent_tick: Tick,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterializedReferenceArchitecture {
    pub local_entities: BTreeMap<ReferenceArchitectureLocalId, EntityId>,
    pub commands: Vec<CommandEnvelope>,
    pub acceptances: Vec<CommandAcceptance>,
    pub command_log_hash: ArtifactHash,
    /// Boundary after the final required v1 batch or v2 quiescence barrier.
    pub build_end_tick: Tick,
    /// Empty for v1. V2 records every executed materialization batch, including empty batches.
    pub executed_batch_evidence: Vec<ReferenceArchitectureExecutedBatchEvidence>,
    /// Empty for v1. V2 records every hash-bound binding stage and its exact quiet barrier.
    pub binding_stage_evidence: Vec<ReferenceArchitectureBindingStageEvidence>,
}

/// Materializes into an owned private candidate. Callers construct this candidate from the same
/// package as their retained original, publish the returned `Simulation` only on success, and
/// discard it on error; no partially materialized value is ever returned.
pub fn materialize_reference_architecture(
    candidate: Simulation,
    artifact: &ReferenceArchitectureArtifact,
    scenario: &ReferenceArchitectureScenarioResolution,
) -> Result<(Simulation, MaterializedReferenceArchitecture), ReferenceArchitectureError> {
    validate_reference_architecture_against(
        artifact,
        candidate.contract(),
        &candidate.profiles().physical_scale,
    )?;
    artifact.validate_scenario_resolution(scenario)?;
    validate_scenario_resolution_against_simulation(scenario, &candidate)?;
    let plan = artifact.materialization_plan()?;
    if plan.steps().is_empty() {
        return Err(ReferenceArchitectureError::EmptyMaterializationPlan);
    }
    match artifact.format_version {
        ReferenceArchitectureFormatVersion::V1 => {
            execute_reference_architecture_v1(candidate, artifact, scenario, &plan)
        }
        ReferenceArchitectureFormatVersion::V2 => {
            execute_reference_architecture_v2(candidate, artifact, scenario, &plan)
        }
    }
}

#[derive(Default)]
struct MaterializationAccumulator {
    local_entities: BTreeMap<ReferenceArchitectureLocalId, EntityId>,
    commands: Vec<CommandEnvelope>,
    acceptances: Vec<CommandAcceptance>,
    executed_batch_evidence: Vec<ReferenceArchitectureExecutedBatchEvidence>,
    binding_stage_evidence: Vec<ReferenceArchitectureBindingStageEvidence>,
}

fn execute_materialization_batch(
    candidate: &mut Simulation,
    scenario: &ReferenceArchitectureScenarioResolution,
    kind: ReferenceArchitectureMaterializationBatchKind,
    batch: &[ReferenceArchitectureMaterializationStep],
    accumulator: &mut MaterializationAccumulator,
    fail_on_runtime_effects: bool,
) -> Result<(), ReferenceArchitectureError> {
    let phase = kind.diagnostic_phase();
    let target_tick = candidate.next_tick();
    let batch_commands = batch
        .iter()
        .enumerate()
        .map(|(index, step)| {
            let ordinal = u64::try_from(index)
                .map_err(|_| ReferenceArchitectureError::MaterializationOrdinalOverflow)?;
            step.resolve_command(target_tick, ordinal, &accumulator.local_entities, scenario)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let report = candidate.step(&batch_commands)?;
    validate_materialization_report(
        &report,
        phase,
        target_tick,
        batch_commands.len(),
        fail_on_runtime_effects,
    )?;
    for ((step, command), acceptance) in batch
        .iter()
        .zip(&batch_commands)
        .zip(&report.command_acceptances)
    {
        if acceptance.target_tick != command.target_tick || acceptance.ordinal != command.ordinal {
            return Err(
                ReferenceArchitectureError::MaterializationAcceptanceMismatch {
                    phase,
                    expected_target_tick: command.target_tick,
                    expected_ordinal: command.ordinal,
                    actual_target_tick: acceptance.target_tick,
                    actual_ordinal: acceptance.ordinal,
                },
            );
        }
        step.record_acceptance(*acceptance, &mut accumulator.local_entities)?;
    }
    accumulator.commands.extend(batch_commands);
    accumulator.acceptances.extend(report.command_acceptances);
    Ok(())
}

fn validate_materialization_report(
    report: &crate::StepReport,
    phase: u8,
    target_tick: Tick,
    expected_acceptances: usize,
    fail_on_runtime_effects: bool,
) -> Result<(), ReferenceArchitectureError> {
    if !report.command_rejections.is_empty()
        || report.command_acceptances.len() != expected_acceptances
    {
        return Err(ReferenceArchitectureError::MaterializationBatchRejected {
            phase,
            target_tick,
            expected_acceptances,
            rejections: report.command_rejections.len(),
            acceptances: report.command_acceptances.len(),
        });
    }
    if fail_on_runtime_effects && (!report.contacts.is_empty() || !report.destructions.is_empty()) {
        return Err(
            ReferenceArchitectureError::MaterializationForbiddenRuntimeEffect {
                phase,
                target_tick,
                contacts: report.contacts.len(),
                destructions: report.destructions.len(),
            },
        );
    }
    if fail_on_runtime_effects && report.run_status != RunStatus::Running {
        return Err(ReferenceArchitectureError::MaterializationRunEnded { phase, target_tick });
    }
    Ok(())
}

fn execute_reference_architecture_v1(
    mut candidate: Simulation,
    artifact: &ReferenceArchitectureArtifact,
    scenario: &ReferenceArchitectureScenarioResolution,
    plan: &ReferenceArchitectureMaterializationPlan,
) -> Result<(Simulation, MaterializedReferenceArchitecture), ReferenceArchitectureError> {
    let mut accumulator = MaterializationAccumulator::default();
    for (kind, batch) in plan.execution_batches() {
        execute_materialization_batch(
            &mut candidate,
            scenario,
            kind,
            batch,
            &mut accumulator,
            false,
        )?;
    }
    finish_materialization(candidate, artifact, accumulator)
}

fn execute_reference_architecture_v2(
    mut candidate: Simulation,
    artifact: &ReferenceArchitectureArtifact,
    scenario: &ReferenceArchitectureScenarioResolution,
    plan: &ReferenceArchitectureMaterializationPlan,
) -> Result<(Simulation, MaterializedReferenceArchitecture), ReferenceArchitectureError> {
    let mut accumulator = MaterializationAccumulator::default();
    for (kind, batch) in plan.execution_batches() {
        let command_tick = candidate.next_tick();
        execute_materialization_batch(
            &mut candidate,
            scenario,
            kind,
            batch,
            &mut accumulator,
            true,
        )?;
        accumulator
            .executed_batch_evidence
            .push(ReferenceArchitectureExecutedBatchEvidence { kind, command_tick });
        if let ReferenceArchitectureMaterializationBatchKind::Binding { stage } = kind {
            let barrier_ticks = advance_single_quiescence_barrier(&mut candidate, kind)?;
            accumulator
                .binding_stage_evidence
                .push(ReferenceArchitectureBindingStageEvidence {
                    stage,
                    command_tick,
                    barrier_ticks,
                    quiescent_tick: candidate.next_tick(),
                });
        }
    }
    finish_materialization(candidate, artifact, accumulator)
}

fn advance_single_quiescence_barrier(
    candidate: &mut Simulation,
    kind: ReferenceArchitectureMaterializationBatchKind,
) -> Result<Vec<Tick>, ReferenceArchitectureError> {
    let mut barrier_ticks = Vec::new();
    while !candidate.signal_quiescence_snapshot()?.is_quiescent() {
        if barrier_ticks.len() >= usize::from(REFERENCE_ARCHITECTURE_MAX_BARRIER_TICKS_V2) {
            return Err(ReferenceArchitectureError::MaterializationBarrierExceeded {
                stage: binding_stage(kind),
                maximum: REFERENCE_ARCHITECTURE_MAX_BARRIER_TICKS_V2,
            });
        }
        let target_tick = candidate.next_tick();
        let report = candidate.step(&[])?;
        validate_materialization_report(&report, kind.diagnostic_phase(), target_tick, 0, true)?;
        barrier_ticks.push(report.completed_tick);
    }
    Ok(barrier_ticks)
}

fn binding_stage(kind: ReferenceArchitectureMaterializationBatchKind) -> u8 {
    match kind {
        ReferenceArchitectureMaterializationBatchKind::Binding { stage } => stage,
        ReferenceArchitectureMaterializationBatchKind::Placement { .. } => {
            unreachable!("barriers only follow binding stages")
        }
    }
}

fn finish_materialization(
    candidate: Simulation,
    artifact: &ReferenceArchitectureArtifact,
    accumulator: MaterializationAccumulator,
) -> Result<(Simulation, MaterializedReferenceArchitecture), ReferenceArchitectureError> {
    if accumulator.local_entities.len() != artifact.operations.len() {
        return Err(ReferenceArchitectureError::IncompleteLocalIdMap {
            expected: artifact.operations.len(),
            actual: accumulator.local_entities.len(),
        });
    }
    let command_log_hash = reference_architecture_command_log_hash(&accumulator.commands)?;
    let build_end_tick = candidate.next_tick();
    Ok((
        candidate,
        MaterializedReferenceArchitecture {
            local_entities: accumulator.local_entities,
            commands: accumulator.commands,
            acceptances: accumulator.acceptances,
            command_log_hash,
            build_end_tick,
            executed_batch_evidence: accumulator.executed_batch_evidence,
            binding_stage_evidence: accumulator.binding_stage_evidence,
        },
    ))
}

pub type ReferenceArchitectureMaterializationInput<'a> = (
    Simulation,
    &'a ReferenceArchitectureArtifact,
    &'a ReferenceArchitectureScenarioResolution,
);

pub type MaterializedReferenceArchitecturePair = (
    (Simulation, MaterializedReferenceArchitecture),
    (Simulation, MaterializedReferenceArchitecture),
);

/// Atomically materializes two v2 designs on one shared Tick schedule.
///
/// Every fixed placement phase and every binding stage consumes a Tick on both candidates, even
/// when one side's batch is empty. After each binding stage, both candidates advance empty Ticks
/// until the earliest boundary at which both signal queues are quiescent.
pub fn materialize_reference_architecture_pair(
    left: ReferenceArchitectureMaterializationInput<'_>,
    right: ReferenceArchitectureMaterializationInput<'_>,
) -> Result<MaterializedReferenceArchitecturePair, ReferenceArchitectureError> {
    let (mut left_candidate, left_artifact, left_scenario) = left;
    let (mut right_candidate, right_artifact, right_scenario) = right;
    for (candidate, artifact, scenario) in [
        (&left_candidate, left_artifact, left_scenario),
        (&right_candidate, right_artifact, right_scenario),
    ] {
        validate_reference_architecture_against(
            artifact,
            candidate.contract(),
            &candidate.profiles().physical_scale,
        )?;
        artifact.validate_scenario_resolution(scenario)?;
        validate_scenario_resolution_against_simulation(scenario, candidate)?;
        if artifact.format_version != ReferenceArchitectureFormatVersion::V2 {
            return Err(ReferenceArchitectureError::PairMaterializationRequiresV2);
        }
    }
    if left_candidate.next_tick() != right_candidate.next_tick() {
        return Err(
            ReferenceArchitectureError::PairMaterializationTickMismatch {
                left: left_candidate.next_tick(),
                right: right_candidate.next_tick(),
            },
        );
    }
    let left_plan = left_artifact.materialization_plan()?;
    let right_plan = right_artifact.materialization_plan()?;
    let left_binding_count = left_artifact
        .materialization_schedule
        .as_ref()
        .expect("validated v2 schedule")
        .binding_batches
        .len();
    let right_binding_count = right_artifact
        .materialization_schedule
        .as_ref()
        .expect("validated v2 schedule")
        .binding_batches
        .len();
    if left_binding_count != right_binding_count {
        return Err(ReferenceArchitectureError::PairBindingBatchCountMismatch {
            left: left_binding_count,
            right: right_binding_count,
        });
    }

    let mut left_accumulator = MaterializationAccumulator::default();
    let mut right_accumulator = MaterializationAccumulator::default();
    for phase in 0..6 {
        let kind = ReferenceArchitectureMaterializationBatchKind::Placement { phase };
        let left_batch = left_plan.batch(kind).unwrap_or(&[]);
        let right_batch = right_plan.batch(kind).unwrap_or(&[]);
        if left_batch.is_empty() && right_batch.is_empty() {
            continue;
        }
        let command_tick = left_candidate.next_tick();
        execute_materialization_batch(
            &mut left_candidate,
            left_scenario,
            kind,
            left_batch,
            &mut left_accumulator,
            true,
        )?;
        execute_materialization_batch(
            &mut right_candidate,
            right_scenario,
            kind,
            right_batch,
            &mut right_accumulator,
            true,
        )?;
        let evidence = ReferenceArchitectureExecutedBatchEvidence { kind, command_tick };
        left_accumulator.executed_batch_evidence.push(evidence);
        right_accumulator.executed_batch_evidence.push(evidence);
    }
    for stage in 0..left_binding_count {
        let stage = u8::try_from(stage)
            .map_err(|_| ReferenceArchitectureError::MaterializationOrdinalOverflow)?;
        let kind = ReferenceArchitectureMaterializationBatchKind::Binding { stage };
        let left_batch = left_plan.batch(kind).unwrap_or(&[]);
        let right_batch = right_plan.batch(kind).unwrap_or(&[]);
        let command_tick = left_candidate.next_tick();
        execute_materialization_batch(
            &mut left_candidate,
            left_scenario,
            kind,
            left_batch,
            &mut left_accumulator,
            true,
        )?;
        execute_materialization_batch(
            &mut right_candidate,
            right_scenario,
            kind,
            right_batch,
            &mut right_accumulator,
            true,
        )?;
        let batch_evidence = ReferenceArchitectureExecutedBatchEvidence { kind, command_tick };
        left_accumulator
            .executed_batch_evidence
            .push(batch_evidence);
        right_accumulator
            .executed_batch_evidence
            .push(batch_evidence);
        let barrier_ticks =
            advance_pair_quiescence_barrier(&mut left_candidate, &mut right_candidate, kind)?;
        let evidence = ReferenceArchitectureBindingStageEvidence {
            stage,
            command_tick,
            barrier_ticks,
            quiescent_tick: left_candidate.next_tick(),
        };
        left_accumulator
            .binding_stage_evidence
            .push(evidence.clone());
        right_accumulator.binding_stage_evidence.push(evidence);
    }
    if left_candidate.next_tick() != right_candidate.next_tick() {
        return Err(
            ReferenceArchitectureError::PairMaterializationTickMismatch {
                left: left_candidate.next_tick(),
                right: right_candidate.next_tick(),
            },
        );
    }
    let left = finish_materialization(left_candidate, left_artifact, left_accumulator)?;
    let right = finish_materialization(right_candidate, right_artifact, right_accumulator)?;
    Ok((left, right))
}

fn advance_pair_quiescence_barrier(
    left: &mut Simulation,
    right: &mut Simulation,
    kind: ReferenceArchitectureMaterializationBatchKind,
) -> Result<Vec<Tick>, ReferenceArchitectureError> {
    let mut barrier_ticks = Vec::new();
    loop {
        if left.next_tick() != right.next_tick() {
            return Err(
                ReferenceArchitectureError::PairMaterializationTickMismatch {
                    left: left.next_tick(),
                    right: right.next_tick(),
                },
            );
        }
        if left.signal_quiescence_snapshot()?.is_quiescent()
            && right.signal_quiescence_snapshot()?.is_quiescent()
        {
            return Ok(barrier_ticks);
        }
        if barrier_ticks.len() >= usize::from(REFERENCE_ARCHITECTURE_MAX_BARRIER_TICKS_V2) {
            return Err(ReferenceArchitectureError::MaterializationBarrierExceeded {
                stage: binding_stage(kind),
                maximum: REFERENCE_ARCHITECTURE_MAX_BARRIER_TICKS_V2,
            });
        }
        let target_tick = left.next_tick();
        let left_report = left.step(&[])?;
        validate_materialization_report(
            &left_report,
            kind.diagnostic_phase(),
            target_tick,
            0,
            true,
        )?;
        let right_report = right.step(&[])?;
        validate_materialization_report(
            &right_report,
            kind.diagnostic_phase(),
            target_tick,
            0,
            true,
        )?;
        if left_report.completed_tick != right_report.completed_tick {
            return Err(
                ReferenceArchitectureError::PairMaterializationTickMismatch {
                    left: left.next_tick(),
                    right: right.next_tick(),
                },
            );
        }
        barrier_ticks.push(left_report.completed_tick);
    }
}

fn validate_scenario_resolution_against_simulation(
    scenario: &ReferenceArchitectureScenarioResolution,
    simulation: &Simulation,
) -> Result<(), ReferenceArchitectureError> {
    let actual_core = simulation
        .main_core_state()
        .ok_or(ReferenceArchitectureError::MissingMainCoreAnchor)?
        .id();
    if actual_core != scenario.main_core {
        return Err(ReferenceArchitectureError::MainCoreAnchorMismatch {
            expected: scenario.main_core,
            actual: actual_core,
        });
    }

    let actual_sources: Vec<_> = simulation
        .power_sources()
        .map(|source| source.id())
        .collect();
    if actual_sources.len() != scenario.power_sources.len() {
        return Err(ReferenceArchitectureError::ScenarioAnchorCountMismatch {
            kind: "powerSource",
            expected: scenario.power_sources.len(),
            actual: actual_sources.len(),
        });
    }
    for (ordinal, (expected, actual)) in scenario
        .power_sources
        .iter()
        .copied()
        .zip(actual_sources)
        .enumerate()
    {
        if expected != actual {
            return Err(ReferenceArchitectureError::PowerSourceAnchorMismatch {
                ordinal: ordinal as u32,
                expected,
                actual,
            });
        }
    }

    let actual_enemies: Vec<_> = simulation
        .enemies()
        .iter()
        .map(|enemy| enemy.id())
        .collect();
    if actual_enemies.len() != scenario.enemies.len() {
        return Err(ReferenceArchitectureError::ScenarioAnchorCountMismatch {
            kind: "enemy",
            expected: scenario.enemies.len(),
            actual: actual_enemies.len(),
        });
    }
    for (ordinal, (expected, actual)) in scenario
        .enemies
        .iter()
        .copied()
        .zip(actual_enemies)
        .enumerate()
    {
        if expected != actual {
            return Err(ReferenceArchitectureError::EnemyAnchorMismatch {
                ordinal: ordinal as u32,
                expected,
                actual,
            });
        }
    }
    Ok(())
}

impl ReferenceArchitectureMaterializationPlan {
    pub fn steps(&self) -> &[ReferenceArchitectureMaterializationStep] {
        &self.steps
    }

    pub fn phase_batches(
        &self,
    ) -> impl ExactSizeIterator<Item = (u8, &[ReferenceArchitectureMaterializationStep])> {
        self.batch_kinds
            .iter()
            .map(|kind| kind.diagnostic_phase())
            .zip(
                self.batch_ranges
                    .iter()
                    .map(|range| &self.steps[range.clone()]),
            )
    }

    pub fn execution_batches(
        &self,
    ) -> impl ExactSizeIterator<
        Item = (
            ReferenceArchitectureMaterializationBatchKind,
            &[ReferenceArchitectureMaterializationStep],
        ),
    > {
        self.batch_kinds.iter().copied().zip(
            self.batch_ranges
                .iter()
                .map(|range| &self.steps[range.clone()]),
        )
    }

    pub fn batch(
        &self,
        kind: ReferenceArchitectureMaterializationBatchKind,
    ) -> Option<&[ReferenceArchitectureMaterializationStep]> {
        self.execution_batches()
            .find_map(|(actual, batch)| (actual == kind).then_some(batch))
    }
}

/// Scenario identities must be supplied in Scenario-v4 canonical order, never registry order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReferenceArchitectureScenarioResolution {
    pub main_core: MainCoreId,
    pub power_sources: Vec<PowerSourceId>,
    pub enemies: Vec<EnemyId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolvedReferenceArchitectureSemanticTarget {
    LocalEntity(EntityId),
    GatePort(GatePortRef),
    MobilePort(MobilePortRef),
    WireSensePort(WireSensePortRef),
    MainCore(MainCoreId),
    PowerSource(PowerSourceId),
    Enemy(EnemyId),
}

pub fn resolve_reference_architecture_semantic_target(
    target: ReferenceArchitectureSemanticTarget,
    local_entities: &BTreeMap<ReferenceArchitectureLocalId, EntityId>,
    scenario: &ReferenceArchitectureScenarioResolution,
) -> Result<ResolvedReferenceArchitectureSemanticTarget, ReferenceArchitectureError> {
    Ok(match target {
        ReferenceArchitectureSemanticTarget::LocalEntity(id) => {
            ResolvedReferenceArchitectureSemanticTarget::LocalEntity(resolve_local(
                id,
                local_entities,
            )?)
        }
        ReferenceArchitectureSemanticTarget::GatePort { gate, port } => {
            ResolvedReferenceArchitectureSemanticTarget::GatePort(GatePortRef {
                gate: GateId(resolve_local(gate, local_entities)?),
                port,
            })
        }
        ReferenceArchitectureSemanticTarget::MobilePort { mobile, port } => {
            ResolvedReferenceArchitectureSemanticTarget::MobilePort(MobilePortRef {
                mobile: MobileId(resolve_local(mobile, local_entities)?),
                port,
            })
        }
        ReferenceArchitectureSemanticTarget::WireSensePort { wire, end } => {
            ResolvedReferenceArchitectureSemanticTarget::WireSensePort(WireSensePortRef {
                wire: WireId(resolve_local(wire, local_entities)?),
                end,
            })
        }
        ReferenceArchitectureSemanticTarget::MainCore => {
            ResolvedReferenceArchitectureSemanticTarget::MainCore(scenario.main_core)
        }
        ReferenceArchitectureSemanticTarget::PowerSource { ordinal } => {
            ResolvedReferenceArchitectureSemanticTarget::PowerSource(
                *scenario.power_sources.get(ordinal as usize).ok_or(
                    ReferenceArchitectureError::MissingScenarioAnchor {
                        kind: "powerSource",
                        ordinal,
                    },
                )?,
            )
        }
        ReferenceArchitectureSemanticTarget::Enemy { ordinal } => {
            ResolvedReferenceArchitectureSemanticTarget::Enemy(
                *scenario.enemies.get(ordinal as usize).ok_or(
                    ReferenceArchitectureError::MissingScenarioAnchor {
                        kind: "enemy",
                        ordinal,
                    },
                )?,
            )
        }
    })
}

pub fn validate_reference_architecture_against(
    artifact: &ReferenceArchitectureArtifact,
    target: &SimulationContract,
    physical: &PhysicalScaleProfile,
) -> Result<(), ReferenceArchitectureError> {
    let canonical = CanonicalReferenceArchitecture::new(artifact)?;
    compare_contract(artifact.contract, *target)?;
    physical.validate()?;
    let actual = physical.canonical_hash()?;
    if actual != target.physical_scale_profile_hash {
        return Err(ReferenceArchitectureError::TargetPhysicalProfileMismatch {
            expected: target.physical_scale_profile_hash,
            actual,
        });
    }
    canonical.validate_geometry(physical, target)
}

/// Hashes the exact complete Command-v1 streams in materialization execution order.
pub fn reference_architecture_command_log_hash(
    commands: &[CommandEnvelope],
) -> Result<ArtifactHash, ReferenceArchitectureError> {
    let mut encoder = CanonicalEncoder::new();
    encoder.bytes(REFERENCE_COMMAND_LOG_HASH_DOMAIN);
    encoder.u16(REFERENCE_COMMAND_LOG_CANONICAL_ENCODER_VERSION);
    encoder.count("commands", commands.len())?;
    for command in commands {
        encoder.bytes(&command.canonical_bytes()?);
    }
    Ok(ArtifactHash::from_bytes(
        *blake3::hash(&encoder.finish()).as_bytes(),
    ))
}

fn compare_contract(
    artifact: SimulationContract,
    target: SimulationContract,
) -> Result<(), ReferenceArchitectureError> {
    if artifact.semantics_version != target.semantics_version {
        return Err(ReferenceArchitectureError::SemanticsMismatch {
            expected: artifact.semantics_version,
            actual: target.semantics_version,
        });
    }
    if artifact.numeric_profile_hash != target.numeric_profile_hash {
        return Err(ReferenceArchitectureError::NumericProfileMismatch {
            expected: artifact.numeric_profile_hash,
            actual: target.numeric_profile_hash,
        });
    }
    if artifact.physical_scale_profile_hash != target.physical_scale_profile_hash {
        return Err(ReferenceArchitectureError::PhysicalScaleProfileMismatch {
            expected: artifact.physical_scale_profile_hash,
            actual: target.physical_scale_profile_hash,
        });
    }
    if artifact.balance_profile_hash != target.balance_profile_hash {
        return Err(ReferenceArchitectureError::BalanceProfileMismatch {
            expected: artifact.balance_profile_hash,
            actual: target.balance_profile_hash,
        });
    }
    Ok(())
}

fn resolve_placement(
    operation: &ReferenceArchitectureOperation,
    local_entities: &BTreeMap<ReferenceArchitectureLocalId, EntityId>,
) -> Result<Command, ReferenceArchitectureError> {
    Ok(match operation {
        ReferenceArchitectureOperation::PlaceFixedSubstrate(value) => {
            Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
                origin: value.origin,
                routing_area: value.routing_area,
                footprint: value.footprint,
            })
        }
        ReferenceArchitectureOperation::PlaceMobileSubstrate(value) => {
            Command::PlaceMobileSubstrate(PlaceMobileSubstrateCommand {
                origin: value.origin,
                routing_area: value.routing_area,
                footprint: value.footprint,
            })
        }
        ReferenceArchitectureOperation::PlaceGate(value) => Command::PlaceGate(PlaceGateCommand {
            gate_type: value.gate_type,
            origin: value.origin,
            routing_domain: resolve_routing_domain(value.routing_domain, local_entities)?,
        }),
        ReferenceArchitectureOperation::PlaceJunction(value) => {
            Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: resolve_routing_domain(value.routing_domain, local_entities)?,
                position: value.position,
            })
        }
        ReferenceArchitectureOperation::PlaceWire(value) => Command::PlaceWire(PlaceWireCommand {
            routing_domain: resolve_routing_domain(value.routing_domain, local_entities)?,
            points: value.points.clone(),
            // Binding is a deterministic second pass, so all symbolic targets may be forward refs.
            endpoint_a: EndpointTarget::Free,
            endpoint_b: EndpointTarget::Free,
        }),
    })
}

fn resolve_routing_domain(
    domain: ReferenceArchitectureRoutingDomain,
    local_entities: &BTreeMap<ReferenceArchitectureLocalId, EntityId>,
) -> Result<RoutingDomain, ReferenceArchitectureError> {
    Ok(match domain {
        ReferenceArchitectureRoutingDomain::OpenWorld => RoutingDomain::OpenWorld,
        ReferenceArchitectureRoutingDomain::FixedSubstrate(id) => {
            RoutingDomain::FixedSubstrate(resolve_local(id, local_entities)?)
        }
        ReferenceArchitectureRoutingDomain::MobileSubstrate(id) => {
            RoutingDomain::MobileSubstrate(resolve_local(id, local_entities)?)
        }
    })
}

fn resolve_endpoint(
    endpoint: ReferenceArchitectureEndpoint,
    local_entities: &BTreeMap<ReferenceArchitectureLocalId, EntityId>,
    scenario: &ReferenceArchitectureScenarioResolution,
) -> Result<EndpointTarget, ReferenceArchitectureError> {
    Ok(match endpoint {
        ReferenceArchitectureEndpoint::Free => EndpointTarget::Free,
        ReferenceArchitectureEndpoint::Junction(id) => {
            EndpointTarget::Junction(JunctionId(resolve_local(id, local_entities)?))
        }
        ReferenceArchitectureEndpoint::GatePort { gate, port } => {
            EndpointTarget::GatePort(GatePortRef {
                gate: GateId(resolve_local(gate, local_entities)?),
                port,
            })
        }
        ReferenceArchitectureEndpoint::MobilePort { mobile, port } => {
            EndpointTarget::MobilePort(MobilePortRef {
                mobile: MobileId(resolve_local(mobile, local_entities)?),
                port,
            })
        }
        ReferenceArchitectureEndpoint::MainCore => {
            EndpointTarget::MainCoreAnchor(scenario.main_core)
        }
        ReferenceArchitectureEndpoint::PowerSource { ordinal } => {
            EndpointTarget::PowerSourceAnchor(*scenario.power_sources.get(ordinal as usize).ok_or(
                ReferenceArchitectureError::MissingScenarioAnchor {
                    kind: "powerSource",
                    ordinal,
                },
            )?)
        }
        ReferenceArchitectureEndpoint::WireSensePort { wire, end } => {
            EndpointTarget::WireSensePort(WireSensePortRef {
                wire: WireId(resolve_local(wire, local_entities)?),
                end,
            })
        }
    })
}

fn resolve_local(
    id: ReferenceArchitectureLocalId,
    local_entities: &BTreeMap<ReferenceArchitectureLocalId, EntityId>,
) -> Result<EntityId, ReferenceArchitectureError> {
    local_entities
        .get(&id)
        .copied()
        .ok_or(ReferenceArchitectureError::UnresolvedLocalId { id })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LocalKind {
    FixedSubstrate,
    MobileSubstrate,
    Gate,
    Junction,
    Wire,
}

impl LocalKind {
    const fn label(self) -> &'static str {
        match self {
            Self::FixedSubstrate => "fixed substrate",
            Self::MobileSubstrate => "mobile substrate",
            Self::Gate => "gate",
            Self::Junction => "junction",
            Self::Wire => "wire",
        }
    }
}

struct CanonicalReferenceArchitecture<'a> {
    artifact: &'a ReferenceArchitectureArtifact,
    operations: BTreeMap<ReferenceArchitectureLocalId, &'a ReferenceArchitectureOperation>,
    kinds: BTreeMap<ReferenceArchitectureLocalId, LocalKind>,
}

impl<'a> CanonicalReferenceArchitecture<'a> {
    fn new(
        artifact: &'a ReferenceArchitectureArtifact,
    ) -> Result<Self, ReferenceArchitectureError> {
        canonical_count("operations", artifact.operations.len())?;
        canonical_count("roleBindings", artifact.role_bindings.len())?;
        canonical_count("observationBindings", artifact.observation_bindings.len())?;
        canonical_text(&artifact.display_name)?;
        if artifact.operations.is_empty() {
            return Err(ReferenceArchitectureError::EmptyArchitecture);
        }

        let mut operations = BTreeMap::new();
        let mut kinds = BTreeMap::new();
        for operation in &artifact.operations {
            let id = operation.local_id();
            let kind = match operation {
                ReferenceArchitectureOperation::PlaceFixedSubstrate(_) => LocalKind::FixedSubstrate,
                ReferenceArchitectureOperation::PlaceMobileSubstrate(_) => {
                    LocalKind::MobileSubstrate
                }
                ReferenceArchitectureOperation::PlaceGate(_) => LocalKind::Gate,
                ReferenceArchitectureOperation::PlaceJunction(_) => LocalKind::Junction,
                ReferenceArchitectureOperation::PlaceWire(wire) => {
                    canonical_count("wire.points", wire.points.len())?;
                    LocalKind::Wire
                }
            };
            if operations.insert(id, operation).is_some() {
                return Err(ReferenceArchitectureError::DuplicateLocalId { id });
            }
            kinds.insert(id, kind);
        }

        let canonical = Self {
            artifact,
            operations,
            kinds,
        };
        canonical.validate_references()?;
        canonical.validate_bindings()?;
        canonical.validate_materialization_schedule()?;
        Ok(canonical)
    }

    fn validate_materialization_schedule(&self) -> Result<(), ReferenceArchitectureError> {
        let Some(schedule) = self.artifact.materialization_schedule.as_ref() else {
            return match self.artifact.format_version {
                ReferenceArchitectureFormatVersion::V1 => Ok(()),
                ReferenceArchitectureFormatVersion::V2 => {
                    Err(ReferenceArchitectureError::MissingMaterializationSchedule)
                }
            };
        };
        if self.artifact.format_version == ReferenceArchitectureFormatVersion::V1 {
            return Err(ReferenceArchitectureError::UnexpectedMaterializationSchedule);
        }
        if schedule.binding_batches.is_empty()
            || schedule.binding_batches.len() > REFERENCE_ARCHITECTURE_MAX_BINDING_BATCHES_V2
        {
            return Err(ReferenceArchitectureError::InvalidBindingBatchCount {
                actual: schedule.binding_batches.len(),
                maximum: REFERENCE_ARCHITECTURE_MAX_BINDING_BATCHES_V2,
            });
        }
        if schedule.binding_batches.first().is_some_and(Vec::is_empty) {
            return Err(ReferenceArchitectureError::EmptyRequiredBindingBatch { stage: 0 });
        }
        let final_stage = schedule.binding_batches.len() - 1;
        if schedule.binding_batches[final_stage].is_empty() {
            return Err(ReferenceArchitectureError::EmptyRequiredBindingBatch {
                stage: final_stage as u8,
            });
        }

        let mut scheduled = BTreeSet::new();
        for (stage, batch) in schedule.binding_batches.iter().enumerate() {
            let mut previous = None;
            for &binding in batch {
                let key = (binding.wire.get(), wire_end_tag(binding.end));
                if previous.is_some_and(|previous| previous >= key) {
                    return Err(ReferenceArchitectureError::NonCanonicalBindingBatch {
                        stage: stage as u8,
                    });
                }
                previous = Some(key);
                let Some(operation) = self.operations.get(&binding.wire) else {
                    return Err(ReferenceArchitectureError::DanglingReference { id: binding.wire });
                };
                let ReferenceArchitectureOperation::PlaceWire(wire) = operation else {
                    return Err(ReferenceArchitectureError::WrongKindReference {
                        id: binding.wire,
                        expected: LocalKind::Wire.label(),
                    });
                };
                let target = wire_endpoint(wire, binding.end);
                if target == ReferenceArchitectureEndpoint::Free {
                    return Err(ReferenceArchitectureError::ScheduledFreeEndpoint {
                        wire: binding.wire,
                        end: binding.end,
                    });
                }
                if stage > 0 && !matches!(target, ReferenceArchitectureEndpoint::PowerSource { .. })
                {
                    return Err(ReferenceArchitectureError::LateNonPowerSourceBinding {
                        stage: stage as u8,
                        wire: binding.wire,
                        end: binding.end,
                    });
                }
                if !scheduled.insert(binding) {
                    return Err(ReferenceArchitectureError::DuplicateScheduledEndpoint {
                        wire: binding.wire,
                        end: binding.end,
                    });
                }
            }
        }

        for operation in self.operations.values() {
            let ReferenceArchitectureOperation::PlaceWire(wire) = operation else {
                continue;
            };
            for (end, target) in [(WireEnd::A, wire.endpoint_a), (WireEnd::B, wire.endpoint_b)] {
                let binding = ReferenceArchitectureBindingEndpoint { wire: wire.id, end };
                if target != ReferenceArchitectureEndpoint::Free && !scheduled.contains(&binding) {
                    return Err(ReferenceArchitectureError::MissingScheduledEndpoint {
                        wire: wire.id,
                        end,
                    });
                }
            }
        }
        Ok(())
    }

    fn validate_references(&self) -> Result<(), ReferenceArchitectureError> {
        for operation in self.operations.values() {
            match operation {
                ReferenceArchitectureOperation::PlaceFixedSubstrate(_)
                | ReferenceArchitectureOperation::PlaceMobileSubstrate(_) => {}
                ReferenceArchitectureOperation::PlaceGate(gate) => match gate.routing_domain {
                    ReferenceArchitectureRoutingDomain::OpenWorld => {
                        return Err(ReferenceArchitectureError::UnsupportedGateDomain {
                            gate: gate.id,
                        });
                    }
                    other => self.validate_domain(other)?,
                },
                ReferenceArchitectureOperation::PlaceJunction(junction) => {
                    self.validate_domain(junction.routing_domain)?;
                }
                ReferenceArchitectureOperation::PlaceWire(wire) => {
                    self.validate_domain(wire.routing_domain)?;
                    self.validate_endpoint(wire.id, wire.endpoint_a)?;
                    self.validate_endpoint(wire.id, wire.endpoint_b)?;
                }
            }
        }
        Ok(())
    }

    fn validate_domain(
        &self,
        domain: ReferenceArchitectureRoutingDomain,
    ) -> Result<(), ReferenceArchitectureError> {
        match domain {
            ReferenceArchitectureRoutingDomain::OpenWorld => Ok(()),
            ReferenceArchitectureRoutingDomain::FixedSubstrate(id) => {
                self.require_kind(id, LocalKind::FixedSubstrate)
            }
            ReferenceArchitectureRoutingDomain::MobileSubstrate(id) => {
                self.require_kind(id, LocalKind::MobileSubstrate)
            }
        }
    }

    fn validate_endpoint(
        &self,
        owner: ReferenceArchitectureLocalId,
        endpoint: ReferenceArchitectureEndpoint,
    ) -> Result<(), ReferenceArchitectureError> {
        match endpoint {
            ReferenceArchitectureEndpoint::Free
            | ReferenceArchitectureEndpoint::MainCore
            | ReferenceArchitectureEndpoint::PowerSource { .. } => Ok(()),
            ReferenceArchitectureEndpoint::Junction(id) => {
                self.require_kind(id, LocalKind::Junction)
            }
            ReferenceArchitectureEndpoint::GatePort { gate, port } => {
                self.require_kind(gate, LocalKind::Gate)?;
                self.validate_gate_port(gate, port)
            }
            ReferenceArchitectureEndpoint::MobilePort { mobile, .. } => {
                self.require_kind(mobile, LocalKind::MobileSubstrate)
            }
            ReferenceArchitectureEndpoint::WireSensePort { wire, .. } => {
                self.require_kind(wire, LocalKind::Wire)?;
                if owner == wire {
                    Err(ReferenceArchitectureError::SelfWireSenseBinding { wire })
                } else {
                    Ok(())
                }
            }
        }
    }

    fn validate_bindings(&self) -> Result<(), ReferenceArchitectureError> {
        let mut roles = BTreeSet::new();
        for binding in &self.artifact.role_bindings {
            canonical_text(&binding.name)?;
            if binding.name.is_empty() {
                return Err(ReferenceArchitectureError::EmptyRoleName);
            }
            if !roles.insert(binding.name.as_str()) {
                return Err(ReferenceArchitectureError::DuplicateRoleName {
                    name: binding.name.clone(),
                });
            }
            self.validate_semantic_target(binding.target)?;
        }

        let mut observations = BTreeSet::new();
        for binding in &self.artifact.observation_bindings {
            canonical_text(&binding.name)?;
            if binding.name.is_empty() {
                return Err(ReferenceArchitectureError::EmptyObservationName);
            }
            if !observations.insert(binding.name.as_str()) {
                return Err(ReferenceArchitectureError::DuplicateObservationName {
                    name: binding.name.clone(),
                });
            }
            self.validate_semantic_target(binding.target)?;
        }
        Ok(())
    }

    fn validate_scenario_resolution(
        &self,
        scenario: &ReferenceArchitectureScenarioResolution,
    ) -> Result<(), ReferenceArchitectureError> {
        for operation in self.operations.values() {
            let ReferenceArchitectureOperation::PlaceWire(wire) = operation else {
                continue;
            };
            for endpoint in [wire.endpoint_a, wire.endpoint_b] {
                if let ReferenceArchitectureEndpoint::PowerSource { ordinal } = endpoint {
                    require_scenario_ordinal("powerSource", ordinal, scenario.power_sources.len())?;
                }
            }
        }
        for target in self
            .artifact
            .role_bindings
            .iter()
            .map(|binding| binding.target)
            .chain(
                self.artifact
                    .observation_bindings
                    .iter()
                    .map(|binding| binding.target),
            )
        {
            match target {
                ReferenceArchitectureSemanticTarget::PowerSource { ordinal } => {
                    require_scenario_ordinal("powerSource", ordinal, scenario.power_sources.len())?;
                }
                ReferenceArchitectureSemanticTarget::Enemy { ordinal } => {
                    require_scenario_ordinal("enemy", ordinal, scenario.enemies.len())?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn validate_semantic_target(
        &self,
        target: ReferenceArchitectureSemanticTarget,
    ) -> Result<(), ReferenceArchitectureError> {
        match target {
            ReferenceArchitectureSemanticTarget::LocalEntity(id) => self.require_existing(id),
            ReferenceArchitectureSemanticTarget::GatePort { gate, port } => {
                self.require_kind(gate, LocalKind::Gate)?;
                self.validate_gate_port(gate, port)
            }
            ReferenceArchitectureSemanticTarget::MobilePort { mobile, .. } => {
                self.require_kind(mobile, LocalKind::MobileSubstrate)
            }
            ReferenceArchitectureSemanticTarget::WireSensePort { wire, .. } => {
                self.require_kind(wire, LocalKind::Wire)
            }
            ReferenceArchitectureSemanticTarget::MainCore
            | ReferenceArchitectureSemanticTarget::PowerSource { .. }
            | ReferenceArchitectureSemanticTarget::Enemy { .. } => Ok(()),
        }
    }

    fn validate_gate_port(
        &self,
        gate: ReferenceArchitectureLocalId,
        port: GatePort,
    ) -> Result<(), ReferenceArchitectureError> {
        let ReferenceArchitectureOperation::PlaceGate(record) = self.operations[&gate] else {
            return Err(ReferenceArchitectureError::WrongKindReference {
                id: gate,
                expected: LocalKind::Gate.label(),
            });
        };
        if record.gate_type == GateType::Not && port == GatePort::InputB {
            Err(ReferenceArchitectureError::InvalidGatePort { gate, port })
        } else {
            Ok(())
        }
    }

    fn require_existing(
        &self,
        id: ReferenceArchitectureLocalId,
    ) -> Result<(), ReferenceArchitectureError> {
        if self.kinds.contains_key(&id) {
            Ok(())
        } else {
            Err(ReferenceArchitectureError::DanglingReference { id })
        }
    }

    fn require_kind(
        &self,
        id: ReferenceArchitectureLocalId,
        expected: LocalKind,
    ) -> Result<(), ReferenceArchitectureError> {
        match self.kinds.get(&id).copied() {
            Some(actual) if actual == expected => Ok(()),
            Some(_) => Err(ReferenceArchitectureError::WrongKindReference {
                id,
                expected: expected.label(),
            }),
            None => Err(ReferenceArchitectureError::DanglingReference { id }),
        }
    }

    fn materialization_plan(&self) -> ReferenceArchitectureMaterializationPlan {
        const BINDING_PHASE: u8 = 6;
        let mut phased_steps: BTreeMap<u8, Vec<ReferenceArchitectureMaterializationStep>> =
            BTreeMap::new();
        for operation in self.operations.values().copied().cloned() {
            phased_steps
                .entry(self.materialization_phase(&operation))
                .or_default()
                .push(ReferenceArchitectureMaterializationStep::Placement(
                    operation,
                ));
        }
        let mut steps = Vec::new();
        let mut batch_kinds = Vec::new();
        let mut batch_ranges = Vec::new();
        match self.artifact.format_version {
            ReferenceArchitectureFormatVersion::V1 => {
                for operation in self.operations.values() {
                    let ReferenceArchitectureOperation::PlaceWire(wire) = operation else {
                        continue;
                    };
                    for (end, target) in
                        [(WireEnd::A, wire.endpoint_a), (WireEnd::B, wire.endpoint_b)]
                    {
                        if target != ReferenceArchitectureEndpoint::Free {
                            phased_steps.entry(BINDING_PHASE).or_default().push(
                                ReferenceArchitectureMaterializationStep::BindWireEnd {
                                    wire: wire.id,
                                    end,
                                    target,
                                },
                            );
                        }
                    }
                }
                for (phase, mut batch) in phased_steps {
                    batch.sort_by_key(materialization_step_sort_key);
                    push_materialization_batch(
                        &mut steps,
                        &mut batch_kinds,
                        &mut batch_ranges,
                        ReferenceArchitectureMaterializationBatchKind::Placement { phase },
                        batch,
                    );
                }
                if let Some(last) = batch_kinds.last_mut()
                    && last.diagnostic_phase() == BINDING_PHASE
                {
                    *last = ReferenceArchitectureMaterializationBatchKind::Binding { stage: 0 };
                }
            }
            ReferenceArchitectureFormatVersion::V2 => {
                for phase in 0..BINDING_PHASE {
                    let mut batch = phased_steps.remove(&phase).unwrap_or_default();
                    if batch.is_empty() {
                        continue;
                    }
                    batch.sort_by_key(materialization_step_sort_key);
                    push_materialization_batch(
                        &mut steps,
                        &mut batch_kinds,
                        &mut batch_ranges,
                        ReferenceArchitectureMaterializationBatchKind::Placement { phase },
                        batch,
                    );
                }
                let schedule = self
                    .artifact
                    .materialization_schedule
                    .as_ref()
                    .expect("validated v2 schedule");
                for (stage, bindings) in schedule.binding_batches.iter().enumerate() {
                    let batch = bindings
                        .iter()
                        .map(|binding| {
                            let ReferenceArchitectureOperation::PlaceWire(wire) =
                                self.operations[&binding.wire]
                            else {
                                unreachable!("validated wire reference")
                            };
                            ReferenceArchitectureMaterializationStep::BindWireEnd {
                                wire: binding.wire,
                                end: binding.end,
                                target: wire_endpoint(wire, binding.end),
                            }
                        })
                        .collect();
                    push_materialization_batch(
                        &mut steps,
                        &mut batch_kinds,
                        &mut batch_ranges,
                        ReferenceArchitectureMaterializationBatchKind::Binding {
                            stage: stage as u8,
                        },
                        batch,
                    );
                }
            }
        }
        ReferenceArchitectureMaterializationPlan {
            steps,
            batch_kinds,
            batch_ranges,
        }
    }

    fn materialization_phase(&self, operation: &ReferenceArchitectureOperation) -> u8 {
        match operation {
            ReferenceArchitectureOperation::PlaceFixedSubstrate(_) => 0,
            ReferenceArchitectureOperation::PlaceGate(gate) => match gate.routing_domain {
                ReferenceArchitectureRoutingDomain::FixedSubstrate(_) => 1,
                ReferenceArchitectureRoutingDomain::MobileSubstrate(_) => 4,
                ReferenceArchitectureRoutingDomain::OpenWorld => {
                    unreachable!("validated gate domain")
                }
            },
            ReferenceArchitectureOperation::PlaceJunction(junction) => {
                match junction.routing_domain {
                    ReferenceArchitectureRoutingDomain::OpenWorld
                    | ReferenceArchitectureRoutingDomain::FixedSubstrate(_) => 1,
                    ReferenceArchitectureRoutingDomain::MobileSubstrate(_) => 4,
                }
            }
            ReferenceArchitectureOperation::PlaceWire(wire) => match wire.routing_domain {
                ReferenceArchitectureRoutingDomain::OpenWorld
                | ReferenceArchitectureRoutingDomain::FixedSubstrate(_) => 2,
                ReferenceArchitectureRoutingDomain::MobileSubstrate(_) => 5,
            },
            ReferenceArchitectureOperation::PlaceMobileSubstrate(_) => 3,
        }
    }

    fn validate_geometry(
        &self,
        physical: &PhysicalScaleProfile,
        target: &SimulationContract,
    ) -> Result<(), ReferenceArchitectureError> {
        for operation in self.operations.values() {
            if let ReferenceArchitectureOperation::PlaceMobileSubstrate(mobile) = operation
                && !point_is_quantized(mobile.origin, physical.wire_geometry_quantum)
            {
                return Err(ReferenceArchitectureError::NotQuantized { id: mobile.id });
            }
        }

        let fixed_module = self.module_for_fixed_domains(target)?;
        validate_module_against(&fixed_module, target, physical)?;

        for operation in self.operations.values() {
            let ReferenceArchitectureOperation::PlaceMobileSubstrate(mobile) = operation else {
                continue;
            };
            let mobile_module = self.module_for_mobile_domain(*mobile, target)?;
            validate_module_against(&mobile_module, target, physical)?;
        }

        self.validate_deferred_endpoints(physical)
    }

    fn module_for_fixed_domains(
        &self,
        target: &SimulationContract,
    ) -> Result<ModuleBlueprint, ReferenceArchitectureError> {
        let mut geometry = AbsoluteModuleGeometry::default();
        for operation in self.operations.values() {
            match operation {
                ReferenceArchitectureOperation::PlaceFixedSubstrate(value) => {
                    geometry.substrates.push(SubstrateBlueprint {
                        id: module_id(value.id)?,
                        origin: value.origin,
                        routing_area: value.routing_area,
                        footprint: value.footprint,
                    });
                }
                ReferenceArchitectureOperation::PlaceGate(value)
                    if matches!(
                        value.routing_domain,
                        ReferenceArchitectureRoutingDomain::FixedSubstrate(_)
                    ) =>
                {
                    let ReferenceArchitectureRoutingDomain::FixedSubstrate(substrate) =
                        value.routing_domain
                    else {
                        unreachable!()
                    };
                    geometry.gates.push(GateBlueprint {
                        id: module_id(value.id)?,
                        substrate: module_id(substrate)?,
                        gate_type: value.gate_type,
                        origin: value.origin,
                    });
                }
                ReferenceArchitectureOperation::PlaceJunction(value)
                    if !matches!(
                        value.routing_domain,
                        ReferenceArchitectureRoutingDomain::MobileSubstrate(_)
                    ) =>
                {
                    geometry.junctions.push(JunctionBlueprint {
                        id: module_id(value.id)?,
                        routing_domain: module_domain(value.routing_domain)?,
                        position: value.position,
                    });
                }
                ReferenceArchitectureOperation::PlaceWire(value)
                    if !matches!(
                        value.routing_domain,
                        ReferenceArchitectureRoutingDomain::MobileSubstrate(_)
                    ) =>
                {
                    geometry.wires.push(WireBlueprint {
                        id: module_id(value.id)?,
                        routing_domain: module_domain(value.routing_domain)?,
                        points: value.points.clone(),
                        endpoint_a: self
                            .module_endpoint_in_domain(value.routing_domain, value.endpoint_a)?,
                        endpoint_b: self
                            .module_endpoint_in_domain(value.routing_domain, value.endpoint_b)?,
                    });
                }
                _ => {}
            }
        }
        Ok(reference_module(geometry, target))
    }

    fn module_for_mobile_domain(
        &self,
        mobile: ReferenceMobileSubstrate,
        target: &SimulationContract,
    ) -> Result<ModuleBlueprint, ReferenceArchitectureError> {
        let mobile_id = module_id(mobile.id)?;
        let mut geometry = AbsoluteModuleGeometry {
            substrates: vec![SubstrateBlueprint {
                id: mobile_id,
                origin: FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
                routing_area: mobile.routing_area,
                footprint: mobile.footprint,
            }],
            ..AbsoluteModuleGeometry::default()
        };
        for operation in self.operations.values() {
            match operation {
                ReferenceArchitectureOperation::PlaceGate(value)
                    if value.routing_domain
                        == ReferenceArchitectureRoutingDomain::MobileSubstrate(mobile.id) =>
                {
                    geometry.gates.push(GateBlueprint {
                        id: module_id(value.id)?,
                        substrate: mobile_id,
                        gate_type: value.gate_type,
                        origin: value.origin,
                    });
                }
                ReferenceArchitectureOperation::PlaceJunction(value)
                    if value.routing_domain
                        == ReferenceArchitectureRoutingDomain::MobileSubstrate(mobile.id) =>
                {
                    geometry.junctions.push(JunctionBlueprint {
                        id: module_id(value.id)?,
                        routing_domain: ModuleRoutingDomain::Substrate(mobile_id),
                        position: value.position,
                    });
                }
                ReferenceArchitectureOperation::PlaceWire(value)
                    if value.routing_domain
                        == ReferenceArchitectureRoutingDomain::MobileSubstrate(mobile.id) =>
                {
                    geometry.wires.push(WireBlueprint {
                        id: module_id(value.id)?,
                        routing_domain: ModuleRoutingDomain::Substrate(mobile_id),
                        points: value.points.clone(),
                        endpoint_a: self
                            .module_endpoint_in_domain(value.routing_domain, value.endpoint_a)?,
                        endpoint_b: self
                            .module_endpoint_in_domain(value.routing_domain, value.endpoint_b)?,
                    });
                }
                _ => {}
            }
        }
        Ok(reference_module(geometry, target))
    }

    fn module_endpoint_in_domain(
        &self,
        domain: ReferenceArchitectureRoutingDomain,
        endpoint: ReferenceArchitectureEndpoint,
    ) -> Result<ModuleEndpoint, ReferenceArchitectureError> {
        Ok(match endpoint {
            ReferenceArchitectureEndpoint::Junction(id)
                if self.domain_for_local(id) == Some(domain) =>
            {
                ModuleEndpoint::Junction(module_id(id)?)
            }
            ReferenceArchitectureEndpoint::GatePort { gate, port }
                if self.domain_for_local(gate) == Some(domain) =>
            {
                ModuleEndpoint::GatePort {
                    gate: module_id(gate)?,
                    port,
                }
            }
            _ => ModuleEndpoint::Free,
        })
    }

    fn validate_deferred_endpoints(
        &self,
        physical: &PhysicalScaleProfile,
    ) -> Result<(), ReferenceArchitectureError> {
        for operation in self.operations.values() {
            let ReferenceArchitectureOperation::PlaceWire(wire) = operation else {
                continue;
            };
            for (end, endpoint) in [(WireEnd::A, wire.endpoint_a), (WireEnd::B, wire.endpoint_b)] {
                let actual = if end == WireEnd::A {
                    wire.points.first().copied()
                } else {
                    wire.points.last().copied()
                };
                let Some(actual) = actual else {
                    continue;
                };
                match endpoint {
                    ReferenceArchitectureEndpoint::Free
                    | ReferenceArchitectureEndpoint::PowerSource { .. } => {}
                    ReferenceArchitectureEndpoint::MainCore => {
                        if wire.routing_domain != ReferenceArchitectureRoutingDomain::OpenWorld {
                            return Err(ReferenceArchitectureError::EndpointDomainMismatch {
                                wire: wire.id,
                                end,
                            });
                        }
                    }
                    ReferenceArchitectureEndpoint::Junction(id) => {
                        self.require_endpoint_match(
                            wire,
                            end,
                            actual,
                            self.domain_for_local(id).expect("validated junction"),
                            self.position_for_local(id, physical)?,
                        )?;
                    }
                    ReferenceArchitectureEndpoint::GatePort { gate, port } => {
                        self.require_endpoint_match(
                            wire,
                            end,
                            actual,
                            self.domain_for_local(gate).expect("validated gate"),
                            self.gate_port_position(gate, port, physical)?,
                        )?;
                    }
                    ReferenceArchitectureEndpoint::MobilePort { mobile, .. } => {
                        let expected_domain =
                            ReferenceArchitectureRoutingDomain::MobileSubstrate(mobile);
                        if wire.routing_domain != expected_domain {
                            return Err(ReferenceArchitectureError::EndpointDomainMismatch {
                                wire: wire.id,
                                end,
                            });
                        }
                        let ReferenceArchitectureOperation::PlaceMobileSubstrate(substrate) =
                            self.operations[&mobile]
                        else {
                            unreachable!("validated mobile reference")
                        };
                        if !substrate.routing_area.contains_point(actual) {
                            return Err(ReferenceArchitectureError::EndpointPositionMismatch {
                                wire: wire.id,
                                end,
                            });
                        }
                    }
                    ReferenceArchitectureEndpoint::WireSensePort {
                        wire: owner,
                        end: owner_end,
                    } => {
                        let ReferenceArchitectureOperation::PlaceWire(owner) =
                            self.operations[&owner]
                        else {
                            unreachable!("validated wire reference")
                        };
                        let expected = match owner_end {
                            WireEnd::A => owner.points.first().copied(),
                            WireEnd::B => owner.points.last().copied(),
                        };
                        self.require_endpoint_match(
                            wire,
                            end,
                            actual,
                            owner.routing_domain,
                            expected.ok_or(ReferenceArchitectureError::InvalidPolyline {
                                id: owner.id,
                            })?,
                        )?;
                    }
                }
            }
        }
        Ok(())
    }

    fn require_endpoint_match(
        &self,
        wire: &ReferenceWire,
        end: WireEnd,
        actual_position: FixedVec2,
        target_domain: ReferenceArchitectureRoutingDomain,
        target_position: FixedVec2,
    ) -> Result<(), ReferenceArchitectureError> {
        if wire.routing_domain != target_domain {
            return Err(ReferenceArchitectureError::EndpointDomainMismatch { wire: wire.id, end });
        }
        if actual_position != target_position {
            return Err(ReferenceArchitectureError::EndpointPositionMismatch {
                wire: wire.id,
                end,
            });
        }
        Ok(())
    }

    fn domain_for_local(
        &self,
        id: ReferenceArchitectureLocalId,
    ) -> Option<ReferenceArchitectureRoutingDomain> {
        match self.operations.get(&id).copied()? {
            ReferenceArchitectureOperation::PlaceGate(gate) => Some(gate.routing_domain),
            ReferenceArchitectureOperation::PlaceJunction(junction) => {
                Some(junction.routing_domain)
            }
            ReferenceArchitectureOperation::PlaceWire(wire) => Some(wire.routing_domain),
            _ => None,
        }
    }

    fn position_for_local(
        &self,
        id: ReferenceArchitectureLocalId,
        _physical: &PhysicalScaleProfile,
    ) -> Result<FixedVec2, ReferenceArchitectureError> {
        match self.operations[&id] {
            ReferenceArchitectureOperation::PlaceJunction(junction) => Ok(junction.position),
            _ => Err(ReferenceArchitectureError::WrongKindReference {
                id,
                expected: LocalKind::Junction.label(),
            }),
        }
    }

    fn gate_port_position(
        &self,
        id: ReferenceArchitectureLocalId,
        port: GatePort,
        physical: &PhysicalScaleProfile,
    ) -> Result<FixedVec2, ReferenceArchitectureError> {
        let ReferenceArchitectureOperation::PlaceGate(gate) = self.operations[&id] else {
            return Err(ReferenceArchitectureError::WrongKindReference {
                id,
                expected: LocalKind::Gate.label(),
            });
        };
        let anchor = gate_port_anchor(gate.gate_type, port, physical)
            .ok_or(ReferenceArchitectureError::InvalidGatePort { gate: id, port })?;
        Ok(FixedVec2::new(
            gate.origin.x.checked_add(anchor.x)?,
            gate.origin.y.checked_add(anchor.y)?,
        ))
    }

    fn encode(&self, encoder: &mut CanonicalEncoder) -> Result<(), ReferenceArchitectureError> {
        encoder.count("operations", self.operations.len())?;
        for operation in self.operations.values() {
            encoder.operation(operation)?;
        }

        let mut roles: Vec<_> = self.artifact.role_bindings.iter().collect();
        roles.sort_by(|left, right| left.name.as_bytes().cmp(right.name.as_bytes()));
        encoder.count("roleBindings", roles.len())?;
        for binding in roles {
            encoder.text(&binding.name)?;
            encoder.semantic_target(binding.target);
        }

        let mut observations: Vec<_> = self.artifact.observation_bindings.iter().collect();
        observations.sort_by(|left, right| left.name.as_bytes().cmp(right.name.as_bytes()));
        encoder.count("observationBindings", observations.len())?;
        for binding in observations {
            encoder.text(&binding.name)?;
            encoder.semantic_target(binding.target);
        }
        if self.artifact.format_version == ReferenceArchitectureFormatVersion::V2 {
            let schedule = self
                .artifact
                .materialization_schedule
                .as_ref()
                .expect("validated v2 schedule");
            encoder.count(
                "materializationSchedule.bindingBatches",
                schedule.binding_batches.len(),
            )?;
            for batch in &schedule.binding_batches {
                encoder.count("materializationSchedule.bindingBatch", batch.len())?;
                for binding in batch {
                    encoder.local_id(binding.wire);
                    encoder.u8(wire_end_tag(binding.end));
                }
            }
        }
        Ok(())
    }
}

fn materialization_step_sort_key(step: &ReferenceArchitectureMaterializationStep) -> (u32, u8) {
    match step {
        ReferenceArchitectureMaterializationStep::Placement(operation) => {
            (operation.local_id().get(), 0)
        }
        ReferenceArchitectureMaterializationStep::BindWireEnd { wire, end, .. } => {
            (wire.get(), wire_end_tag(*end))
        }
    }
}

fn wire_endpoint(wire: &ReferenceWire, end: WireEnd) -> ReferenceArchitectureEndpoint {
    match end {
        WireEnd::A => wire.endpoint_a,
        WireEnd::B => wire.endpoint_b,
    }
}

fn push_materialization_batch(
    steps: &mut Vec<ReferenceArchitectureMaterializationStep>,
    batch_kinds: &mut Vec<ReferenceArchitectureMaterializationBatchKind>,
    batch_ranges: &mut Vec<Range<usize>>,
    kind: ReferenceArchitectureMaterializationBatchKind,
    batch: Vec<ReferenceArchitectureMaterializationStep>,
) {
    let start = steps.len();
    steps.extend(batch);
    let end = steps.len();
    batch_kinds.push(kind);
    batch_ranges.push(start..end);
}

fn reference_module(
    geometry: AbsoluteModuleGeometry,
    target: &SimulationContract,
) -> ModuleBlueprint {
    ModuleBlueprint {
        format_version: ModuleFormatVersion::V1,
        hash_algorithm_id: HashAlgorithmId::Blake3V1,
        name: "reference-architecture-geometry-preflight".to_owned(),
        contract: ModuleContract {
            semantics_version: target.semantics_version,
            numeric_profile_hash: target.numeric_profile_hash,
            physical_scale_profile_hash: target.physical_scale_profile_hash,
        },
        balance_profile_hash: Some(target.balance_profile_hash),
        geometry,
        io_bindings: Vec::new(),
        provenance: ModuleProvenance::default(),
    }
}

fn module_id(
    id: ReferenceArchitectureLocalId,
) -> Result<ModuleLocalId, ReferenceArchitectureError> {
    ModuleLocalId::new(id.get())
        .ok_or(ReferenceArchitectureError::InvalidLocalId { actual: id.get() })
}

fn module_domain(
    domain: ReferenceArchitectureRoutingDomain,
) -> Result<ModuleRoutingDomain, ReferenceArchitectureError> {
    Ok(match domain {
        ReferenceArchitectureRoutingDomain::OpenWorld => ModuleRoutingDomain::OpenWorld,
        ReferenceArchitectureRoutingDomain::FixedSubstrate(id) => {
            ModuleRoutingDomain::Substrate(module_id(id)?)
        }
        ReferenceArchitectureRoutingDomain::MobileSubstrate(_) => {
            return Err(ReferenceArchitectureError::InternalInvalidCanonicalState);
        }
    })
}

fn point_is_quantized(point: FixedVec2, quantum: Fixed) -> bool {
    quantum.0 > 0 && point.x.0.rem_euclid(quantum.0) == 0 && point.y.0.rem_euclid(quantum.0) == 0
}

fn require_scenario_ordinal(
    kind: &'static str,
    ordinal: u32,
    count: usize,
) -> Result<(), ReferenceArchitectureError> {
    if usize::try_from(ordinal).is_ok_and(|ordinal| ordinal < count) {
        Ok(())
    } else {
        Err(ReferenceArchitectureError::MissingScenarioAnchor { kind, ordinal })
    }
}

fn gate_port_anchor(
    gate_type: GateType,
    port: GatePort,
    physical: &PhysicalScaleProfile,
) -> Option<crate::PortAnchor> {
    Some(match gate_type {
        GateType::And => match port {
            GatePort::InputA => physical.gate_port_anchors.and_gate.input_a,
            GatePort::InputB => physical.gate_port_anchors.and_gate.input_b,
            GatePort::Output => physical.gate_port_anchors.and_gate.output,
            GatePort::Power => physical.gate_port_anchors.and_gate.power,
        },
        GateType::Or => match port {
            GatePort::InputA => physical.gate_port_anchors.or_gate.input_a,
            GatePort::InputB => physical.gate_port_anchors.or_gate.input_b,
            GatePort::Output => physical.gate_port_anchors.or_gate.output,
            GatePort::Power => physical.gate_port_anchors.or_gate.power,
        },
        GateType::Not => match port {
            GatePort::InputA => physical.gate_port_anchors.not_gate.input,
            GatePort::InputB => return None,
            GatePort::Output => physical.gate_port_anchors.not_gate.output,
            GatePort::Power => physical.gate_port_anchors.not_gate.power,
        },
    })
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ReferenceArchitectureError {
    #[error(
        "invalid Reference Architecture JSON: category={category:?}, line={line}, column={column}"
    )]
    InvalidJson {
        category: JsonErrorCategory,
        line: usize,
        column: usize,
    },
    #[error("unable to encode canonical Reference Architecture JSON")]
    JsonEncoding,
    #[error("unsupported Reference Architecture format: expected {expected}, got {actual}")]
    UnsupportedFormatVersion { expected: u32, actual: u32 },
    #[error("unsupported Reference Architecture hash algorithm `{actual}`")]
    UnsupportedHashAlgorithm { actual: String },
    #[error("unsupported Reference Architecture semantics version `{actual}`")]
    UnsupportedSemanticsVersion { actual: String },
    #[error("invalid Reference Architecture hash field `{field}`: {error}")]
    InvalidHash {
        field: &'static str,
        error: HashParseError,
    },
    #[error("reference-architecture-local id must be nonzero, got {actual}")]
    InvalidLocalId { actual: u32 },
    #[error("Reference Architecture must contain at least one operation")]
    EmptyArchitecture,
    #[error("Reference Architecture materialization plan must contain at least one step")]
    EmptyMaterializationPlan,
    #[error("Reference Architecture v2 requires materializationSchedule")]
    MissingMaterializationSchedule,
    #[error("Reference Architecture v1 forbids materializationSchedule")]
    UnexpectedMaterializationSchedule,
    #[error("Reference Architecture v2 binding batch count must be in 1..={maximum}, got {actual}")]
    InvalidBindingBatchCount { actual: usize, maximum: usize },
    #[error("Reference Architecture v2 binding stage {stage} must be nonempty")]
    EmptyRequiredBindingBatch { stage: u8 },
    #[error("Reference Architecture v2 binding stage {stage} is not in canonical endpoint order")]
    NonCanonicalBindingBatch { stage: u8 },
    #[error("wire {wire} endpoint {end:?} is Free and cannot appear in a binding schedule")]
    ScheduledFreeEndpoint {
        wire: ReferenceArchitectureLocalId,
        end: WireEnd,
    },
    #[error("wire {wire} endpoint {end:?} appears more than once in the binding schedule")]
    DuplicateScheduledEndpoint {
        wire: ReferenceArchitectureLocalId,
        end: WireEnd,
    },
    #[error("wire {wire} endpoint {end:?} is absent from the binding schedule")]
    MissingScheduledEndpoint {
        wire: ReferenceArchitectureLocalId,
        end: WireEnd,
    },
    #[error(
        "binding stage {stage} contains non-PowerSource wire {wire} endpoint {end:?}; these must be in stage 0"
    )]
    LateNonPowerSourceBinding {
        stage: u8,
        wire: ReferenceArchitectureLocalId,
        end: WireEnd,
    },
    #[error("Reference Architecture collection `{collection}` exceeds the canonical u32 limit")]
    CollectionTooLong { collection: &'static str },
    #[error("Reference Architecture text field is too long for canonical encoding")]
    TextTooLong,
    #[error("Reference Architecture semantics mismatch: expected {expected}, got {actual}")]
    SemanticsMismatch {
        expected: SemanticsVersion,
        actual: SemanticsVersion,
    },
    #[error("Reference Architecture numeric profile mismatch: expected {expected}, got {actual}")]
    NumericProfileMismatch {
        expected: ProfileHash,
        actual: ProfileHash,
    },
    #[error(
        "Reference Architecture physical-scale profile mismatch: expected {expected}, got {actual}"
    )]
    PhysicalScaleProfileMismatch {
        expected: ProfileHash,
        actual: ProfileHash,
    },
    #[error("Reference Architecture balance profile mismatch: expected {expected}, got {actual}")]
    BalanceProfileMismatch {
        expected: ProfileHash,
        actual: ProfileHash,
    },
    #[error(
        "physical profile bytes do not match target contract: expected {expected}, got {actual}"
    )]
    TargetPhysicalProfileMismatch {
        expected: ProfileHash,
        actual: ProfileHash,
    },
    #[error("duplicate reference-architecture-local id {id}")]
    DuplicateLocalId { id: ReferenceArchitectureLocalId },
    #[error("dangling reference-architecture-local reference {id}")]
    DanglingReference { id: ReferenceArchitectureLocalId },
    #[error("reference-architecture-local reference {id} does not refer to a {expected}")]
    WrongKindReference {
        id: ReferenceArchitectureLocalId,
        expected: &'static str,
    },
    #[error("gate {gate} cannot be placed in the open-world routing domain")]
    UnsupportedGateDomain { gate: ReferenceArchitectureLocalId },
    #[error("gate {gate} does not expose port {port:?}")]
    InvalidGatePort {
        gate: ReferenceArchitectureLocalId,
        port: GatePort,
    },
    #[error("wire {wire} cannot bind an endpoint to its own WireSense port")]
    SelfWireSenseBinding { wire: ReferenceArchitectureLocalId },
    #[error("semantic role name must not be empty")]
    EmptyRoleName,
    #[error("duplicate semantic role name {name:?}")]
    DuplicateRoleName { name: String },
    #[error("observation binding name must not be empty")]
    EmptyObservationName,
    #[error("duplicate observation binding name {name:?}")]
    DuplicateObservationName { name: String },
    #[error("geometry for reference-architecture-local id {id} is not quantized")]
    NotQuantized { id: ReferenceArchitectureLocalId },
    #[error("wire {id} must contain at least two distinct points")]
    InvalidPolyline { id: ReferenceArchitectureLocalId },
    #[error("wire {wire} endpoint {end:?} uses a different routing domain than its target")]
    EndpointDomainMismatch {
        wire: ReferenceArchitectureLocalId,
        end: WireEnd,
    },
    #[error("wire {wire} endpoint {end:?} does not equal its target position")]
    EndpointPositionMismatch {
        wire: ReferenceArchitectureLocalId,
        end: WireEnd,
    },
    #[error("reference-architecture-local id {id} has not been materialized")]
    UnresolvedLocalId { id: ReferenceArchitectureLocalId },
    #[error("reference-architecture-local id {id} was resolved more than once")]
    DuplicateResolvedLocalId { id: ReferenceArchitectureLocalId },
    #[error(
        "runtime entity {entity:?} was assigned to both reference-architecture-local ids {first_id} and {second_id}"
    )]
    DuplicateResolvedEntity {
        entity: EntityId,
        first_id: ReferenceArchitectureLocalId,
        second_id: ReferenceArchitectureLocalId,
    },
    #[error("accepted placement for reference-architecture-local id {id} created no entity")]
    MissingCreatedEntity { id: ReferenceArchitectureLocalId },
    #[error("accepted non-placement step unexpectedly created entity {entity:?}")]
    UnexpectedCreatedEntity { entity: EntityId },
    #[error("scenario does not contain {kind} ordinal {ordinal}")]
    MissingScenarioAnchor { kind: &'static str, ordinal: u32 },
    #[error("Scenario has no Main Core anchor")]
    MissingMainCoreAnchor,
    #[error("Scenario {kind} anchor count mismatch: expected {expected}, got {actual}")]
    ScenarioAnchorCountMismatch {
        kind: &'static str,
        expected: usize,
        actual: usize,
    },
    #[error("Scenario Main Core identity mismatch: expected {expected:?}, got {actual:?}")]
    MainCoreAnchorMismatch {
        expected: MainCoreId,
        actual: MainCoreId,
    },
    #[error(
        "Scenario Power Source ordinal {ordinal} identity mismatch: expected {expected:?}, got {actual:?}"
    )]
    PowerSourceAnchorMismatch {
        ordinal: u32,
        expected: PowerSourceId,
        actual: PowerSourceId,
    },
    #[error(
        "Scenario Enemy ordinal {ordinal} identity mismatch: expected {expected:?}, got {actual:?}"
    )]
    EnemyAnchorMismatch {
        ordinal: u32,
        expected: EnemyId,
        actual: EnemyId,
    },
    #[error("Reference Architecture materialization batch ordinal exceeds u64")]
    MaterializationOrdinalOverflow,
    #[error(
        "Reference Architecture materialization batch failed: phase={phase}, targetTick={target_tick}, expectedAcceptances={expected_acceptances}, rejections={rejections}, acceptances={acceptances}"
    )]
    MaterializationBatchRejected {
        phase: u8,
        target_tick: Tick,
        expected_acceptances: usize,
        rejections: usize,
        acceptances: usize,
    },
    #[error(
        "Reference Architecture acceptance mismatch: phase={phase}, expected=({expected_target_tick},{expected_ordinal}), actual=({actual_target_tick},{actual_ordinal})"
    )]
    MaterializationAcceptanceMismatch {
        phase: u8,
        expected_target_tick: Tick,
        expected_ordinal: u64,
        actual_target_tick: Tick,
        actual_ordinal: u64,
    },
    #[error(
        "Reference Architecture materialization caused forbidden runtime effects: phase={phase}, targetTick={target_tick}, contacts={contacts}, destructions={destructions}"
    )]
    MaterializationForbiddenRuntimeEffect {
        phase: u8,
        target_tick: Tick,
        contacts: usize,
        destructions: usize,
    },
    #[error(
        "Reference Architecture materialization ended the run: phase={phase}, targetTick={target_tick}"
    )]
    MaterializationRunEnded { phase: u8, target_tick: Tick },
    #[error(
        "Reference Architecture v2 binding stage {stage} did not quiesce within {maximum} empty Ticks"
    )]
    MaterializationBarrierExceeded { stage: u8, maximum: u16 },
    #[error("paired Reference Architecture materialization requires two format-v2 artifacts")]
    PairMaterializationRequiresV2,
    #[error(
        "paired Reference Architecture binding batch count mismatch: left={left}, right={right}"
    )]
    PairBindingBatchCountMismatch { left: usize, right: usize },
    #[error("paired Reference Architecture Tick mismatch: left={left}, right={right}")]
    PairMaterializationTickMismatch { left: Tick, right: Tick },
    #[error("Reference Architecture local-ID map is incomplete: expected {expected}, got {actual}")]
    IncompleteLocalIdMap { expected: usize, actual: usize },
    #[error("invalid internal Reference Architecture canonical state")]
    InternalInvalidCanonicalState,
    #[error(transparent)]
    Module(#[from] ModuleError),
    #[error(transparent)]
    CommandEncoding(#[from] CommandEncodingError),
    #[error(transparent)]
    Simulation(#[from] SimulationError),
    #[error(transparent)]
    Numeric(#[from] crate::NumericError),
    #[error(transparent)]
    Profile(#[from] crate::ProfileValidationError),
}

fn canonical_count(
    collection: &'static str,
    count: usize,
) -> Result<u32, ReferenceArchitectureError> {
    u32::try_from(count).map_err(|_| ReferenceArchitectureError::CollectionTooLong { collection })
}

fn canonical_text(value: &str) -> Result<(), ReferenceArchitectureError> {
    u32::try_from(value.len())
        .map(|_| ())
        .map_err(|_| ReferenceArchitectureError::TextTooLong)
}

struct CanonicalEncoder {
    bytes: Vec<u8>,
}

impl CanonicalEncoder {
    fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }

    fn bytes(&mut self, value: &[u8]) {
        self.bytes.extend_from_slice(value);
    }

    fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn u16(&mut self, value: u16) {
        self.bytes(value.to_le_bytes().as_slice());
    }

    fn u32(&mut self, value: u32) {
        self.bytes(value.to_le_bytes().as_slice());
    }

    fn i64(&mut self, value: i64) {
        self.bytes(value.to_le_bytes().as_slice());
    }

    fn count(
        &mut self,
        collection: &'static str,
        count: usize,
    ) -> Result<(), ReferenceArchitectureError> {
        self.u32(canonical_count(collection, count)?);
        Ok(())
    }

    fn text(&mut self, value: &str) -> Result<(), ReferenceArchitectureError> {
        canonical_text(value)?;
        self.u32(value.len() as u32);
        self.bytes(value.as_bytes());
        Ok(())
    }

    fn local_id(&mut self, id: ReferenceArchitectureLocalId) {
        self.u32(id.get());
    }

    fn point(&mut self, point: FixedVec2) {
        self.i64(point.x.0);
        self.i64(point.y.0);
    }

    fn aabb(&mut self, aabb: FixedAabb) {
        self.point(aabb.min);
        self.point(aabb.max);
    }

    fn domain(&mut self, domain: ReferenceArchitectureRoutingDomain) {
        match domain {
            ReferenceArchitectureRoutingDomain::OpenWorld => self.u8(0),
            ReferenceArchitectureRoutingDomain::FixedSubstrate(id) => {
                self.u8(1);
                self.local_id(id);
            }
            ReferenceArchitectureRoutingDomain::MobileSubstrate(id) => {
                self.u8(2);
                self.local_id(id);
            }
        }
    }

    fn endpoint(&mut self, endpoint: ReferenceArchitectureEndpoint) {
        match endpoint {
            ReferenceArchitectureEndpoint::Free => self.u8(0),
            ReferenceArchitectureEndpoint::Junction(id) => {
                self.u8(1);
                self.local_id(id);
            }
            ReferenceArchitectureEndpoint::GatePort { gate, port } => {
                self.u8(2);
                self.local_id(gate);
                self.u8(gate_port_tag(port));
            }
            ReferenceArchitectureEndpoint::MobilePort { mobile, port } => {
                self.u8(3);
                self.local_id(mobile);
                self.u8(mobile_port_tag(port));
            }
            ReferenceArchitectureEndpoint::MainCore => self.u8(4),
            ReferenceArchitectureEndpoint::PowerSource { ordinal } => {
                self.u8(5);
                self.u32(ordinal);
            }
            ReferenceArchitectureEndpoint::WireSensePort { wire, end } => {
                self.u8(6);
                self.local_id(wire);
                self.u8(wire_end_tag(end));
            }
        }
    }

    fn semantic_target(&mut self, target: ReferenceArchitectureSemanticTarget) {
        match target {
            ReferenceArchitectureSemanticTarget::LocalEntity(id) => {
                self.u8(0);
                self.local_id(id);
            }
            ReferenceArchitectureSemanticTarget::GatePort { gate, port } => {
                self.u8(1);
                self.local_id(gate);
                self.u8(gate_port_tag(port));
            }
            ReferenceArchitectureSemanticTarget::MobilePort { mobile, port } => {
                self.u8(2);
                self.local_id(mobile);
                self.u8(mobile_port_tag(port));
            }
            ReferenceArchitectureSemanticTarget::WireSensePort { wire, end } => {
                self.u8(3);
                self.local_id(wire);
                self.u8(wire_end_tag(end));
            }
            ReferenceArchitectureSemanticTarget::MainCore => self.u8(4),
            ReferenceArchitectureSemanticTarget::PowerSource { ordinal } => {
                self.u8(5);
                self.u32(ordinal);
            }
            ReferenceArchitectureSemanticTarget::Enemy { ordinal } => {
                self.u8(6);
                self.u32(ordinal);
            }
        }
    }

    fn operation(
        &mut self,
        operation: &ReferenceArchitectureOperation,
    ) -> Result<(), ReferenceArchitectureError> {
        match operation {
            ReferenceArchitectureOperation::PlaceFixedSubstrate(value) => {
                self.u8(0);
                self.local_id(value.id);
                self.point(value.origin);
                self.aabb(value.routing_area);
                self.aabb(value.footprint);
            }
            ReferenceArchitectureOperation::PlaceMobileSubstrate(value) => {
                self.u8(1);
                self.local_id(value.id);
                self.point(value.origin);
                self.aabb(value.routing_area);
                self.aabb(value.footprint);
            }
            ReferenceArchitectureOperation::PlaceGate(value) => {
                self.u8(2);
                self.local_id(value.id);
                self.domain(value.routing_domain);
                self.u8(gate_type_tag(value.gate_type));
                self.point(value.origin);
            }
            ReferenceArchitectureOperation::PlaceJunction(value) => {
                self.u8(3);
                self.local_id(value.id);
                self.domain(value.routing_domain);
                self.point(value.position);
            }
            ReferenceArchitectureOperation::PlaceWire(value) => {
                self.u8(4);
                self.local_id(value.id);
                self.domain(value.routing_domain);
                self.count("wire.points", value.points.len())?;
                for &point in &value.points {
                    self.point(point);
                }
                self.endpoint(value.endpoint_a);
                self.endpoint(value.endpoint_b);
            }
        }
        Ok(())
    }
}

const fn gate_type_tag(gate_type: GateType) -> u8 {
    match gate_type {
        GateType::And => 0,
        GateType::Or => 1,
        GateType::Not => 2,
    }
}

const fn gate_port_tag(port: GatePort) -> u8 {
    match port {
        GatePort::InputA => 0,
        GatePort::InputB => 1,
        GatePort::Output => 2,
        GatePort::Power => 3,
    }
}

const fn mobile_port_tag(port: MobilePort) -> u8 {
    match port {
        MobilePort::Stop => 0,
        MobilePort::Left => 1,
        MobilePort::Right => 2,
        MobilePort::Build => 3,
    }
}

const fn wire_end_tag(end: WireEnd) -> u8 {
    match end {
        WireEnd::A => 0,
        WireEnd::B => 1,
    }
}

pub fn decode_reference_architecture_artifact(
    source: &str,
) -> Result<ReferenceArchitectureArtifact, ReferenceArchitectureError> {
    let envelope: ReferenceArchitectureFormatEnvelope =
        serde_json::from_str(source).map_err(invalid_json)?;
    ReferenceArchitectureFormatVersion::parse(envelope.format_version)?;
    if envelope.hash_algorithm_id != crate::HASH_ALGORITHM_ID_BLAKE3_V1 {
        return Err(ReferenceArchitectureError::UnsupportedHashAlgorithm {
            actual: envelope.hash_algorithm_id,
        });
    }
    let wire: ReferenceArchitectureArtifactWire =
        serde_json::from_str(source).map_err(invalid_json)?;
    wire.try_into()
}

fn invalid_json(error: serde_json::Error) -> ReferenceArchitectureError {
    ReferenceArchitectureError::InvalidJson {
        category: JsonErrorCategory::from(error.classify()),
        line: error.line(),
        column: error.column(),
    }
}

pub fn encode_reference_architecture_artifact(
    artifact: &ReferenceArchitectureArtifact,
) -> Result<String, ReferenceArchitectureError> {
    let canonical = CanonicalReferenceArchitecture::new(artifact)?;
    let wire = ReferenceArchitectureArtifactWire::from_canonical(&canonical);
    let mut encoded = serde_json::to_string_pretty(&wire)
        .map_err(|_| ReferenceArchitectureError::JsonEncoding)?;
    encoded.push('\n');
    Ok(encoded)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReferenceArchitectureArtifactWire {
    format_version: u32,
    hash_algorithm_id: String,
    display_name: String,
    contract: ReferenceArchitectureContractWire,
    operations: Vec<ReferenceArchitectureOperationWire>,
    role_bindings: Vec<ReferenceArchitectureRoleBindingWire>,
    observation_bindings: Vec<ReferenceArchitectureObservationBindingWire>,
    #[serde(skip_serializing_if = "Option::is_none")]
    materialization_schedule: Option<ReferenceArchitectureMaterializationScheduleWire>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReferenceArchitectureMaterializationScheduleWire {
    binding_batches: Vec<Vec<ReferenceArchitectureBindingEndpointWire>>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReferenceArchitectureBindingEndpointWire {
    wire: u32,
    end: WireEndWire,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReferenceArchitectureFormatEnvelope {
    format_version: u32,
    hash_algorithm_id: String,
}

impl ReferenceArchitectureArtifactWire {
    fn from_canonical(canonical: &CanonicalReferenceArchitecture<'_>) -> Self {
        let artifact = canonical.artifact;
        let operations = canonical
            .operations
            .values()
            .map(|operation| ReferenceArchitectureOperationWire::from(*operation))
            .collect();
        let mut role_bindings: Vec<_> = artifact
            .role_bindings
            .iter()
            .map(ReferenceArchitectureRoleBindingWire::from)
            .collect();
        role_bindings.sort_by(|left, right| left.name.as_bytes().cmp(right.name.as_bytes()));
        let mut observation_bindings: Vec<_> = artifact
            .observation_bindings
            .iter()
            .map(ReferenceArchitectureObservationBindingWire::from)
            .collect();
        observation_bindings.sort_by(|left, right| left.name.as_bytes().cmp(right.name.as_bytes()));
        Self {
            format_version: artifact.format_version.as_u32(),
            hash_algorithm_id: artifact.hash_algorithm_id.as_str().to_owned(),
            display_name: artifact.display_name.clone(),
            contract: artifact.contract.into(),
            operations,
            role_bindings,
            observation_bindings,
            materialization_schedule: artifact.materialization_schedule.as_ref().map(|schedule| {
                ReferenceArchitectureMaterializationScheduleWire {
                    binding_batches: schedule
                        .binding_batches
                        .iter()
                        .map(|batch| {
                            batch
                                .iter()
                                .map(|binding| ReferenceArchitectureBindingEndpointWire {
                                    wire: binding.wire.get(),
                                    end: binding.end.into(),
                                })
                                .collect()
                        })
                        .collect(),
                }
            }),
        }
    }
}

impl TryFrom<ReferenceArchitectureArtifactWire> for ReferenceArchitectureArtifact {
    type Error = ReferenceArchitectureError;

    fn try_from(wire: ReferenceArchitectureArtifactWire) -> Result<Self, Self::Error> {
        let format_version = ReferenceArchitectureFormatVersion::parse(wire.format_version)?;
        let hash_algorithm_id = match wire.hash_algorithm_id.as_str() {
            crate::HASH_ALGORITHM_ID_BLAKE3_V1 => HashAlgorithmId::Blake3V1,
            _ => {
                return Err(ReferenceArchitectureError::UnsupportedHashAlgorithm {
                    actual: wire.hash_algorithm_id,
                });
            }
        };
        let artifact = Self {
            format_version,
            hash_algorithm_id,
            display_name: wire.display_name,
            contract: wire.contract.try_into()?,
            operations: wire
                .operations
                .into_iter()
                .map(ReferenceArchitectureOperation::try_from)
                .collect::<Result<Vec<_>, _>>()?,
            role_bindings: wire
                .role_bindings
                .into_iter()
                .map(ReferenceArchitectureRoleBinding::try_from)
                .collect::<Result<Vec<_>, _>>()?,
            observation_bindings: wire
                .observation_bindings
                .into_iter()
                .map(ReferenceArchitectureObservationBinding::try_from)
                .collect::<Result<Vec<_>, _>>()?,
            materialization_schedule: wire
                .materialization_schedule
                .map(|schedule| {
                    schedule
                        .binding_batches
                        .into_iter()
                        .map(|batch| {
                            batch
                                .into_iter()
                                .map(|binding| {
                                    Ok(ReferenceArchitectureBindingEndpoint {
                                        wire: parse_local_id(binding.wire)?,
                                        end: binding.end.into(),
                                    })
                                })
                                .collect::<Result<Vec<_>, ReferenceArchitectureError>>()
                        })
                        .collect::<Result<Vec<_>, ReferenceArchitectureError>>()
                        .map(
                            |binding_batches| ReferenceArchitectureMaterializationSchedule {
                                binding_batches,
                            },
                        )
                })
                .transpose()?,
        };
        CanonicalReferenceArchitecture::new(&artifact)?;
        Ok(artifact)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReferenceArchitectureContractWire {
    semantics_version: String,
    numeric_profile_hash: String,
    physical_scale_profile_hash: String,
    balance_profile_hash: String,
}

impl From<SimulationContract> for ReferenceArchitectureContractWire {
    fn from(contract: SimulationContract) -> Self {
        Self {
            semantics_version: contract.semantics_version.as_str().to_owned(),
            numeric_profile_hash: contract.numeric_profile_hash.to_string(),
            physical_scale_profile_hash: contract.physical_scale_profile_hash.to_string(),
            balance_profile_hash: contract.balance_profile_hash.to_string(),
        }
    }
}

impl TryFrom<ReferenceArchitectureContractWire> for SimulationContract {
    type Error = ReferenceArchitectureError;

    fn try_from(wire: ReferenceArchitectureContractWire) -> Result<Self, Self::Error> {
        let semantics_version = match wire.semantics_version.as_str() {
            SEMANTICS_VERSION_V1 => SemanticsVersion::AonV1,
            _ => {
                return Err(ReferenceArchitectureError::UnsupportedSemanticsVersion {
                    actual: wire.semantics_version,
                });
            }
        };
        Ok(Self {
            semantics_version,
            numeric_profile_hash: parse_profile_hash(
                "contract.numericProfileHash",
                &wire.numeric_profile_hash,
            )?,
            physical_scale_profile_hash: parse_profile_hash(
                "contract.physicalScaleProfileHash",
                &wire.physical_scale_profile_hash,
            )?,
            balance_profile_hash: parse_profile_hash(
                "contract.balanceProfileHash",
                &wire.balance_profile_hash,
            )?,
        })
    }
}

fn parse_profile_hash(
    field: &'static str,
    value: &str,
) -> Result<ProfileHash, ReferenceArchitectureError> {
    ProfileHash::from_hex(value)
        .map_err(|error| ReferenceArchitectureError::InvalidHash { field, error })
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum ReferenceArchitectureOperationWire {
    #[serde(rename = "placeFixedSubstrate")]
    FixedSubstrate {
        id: u32,
        origin: FixedVec2Wire,
        routing_area: FixedAabbWire,
        footprint: FixedAabbWire,
    },
    #[serde(rename = "placeMobileSubstrate")]
    MobileSubstrate {
        id: u32,
        origin: FixedVec2Wire,
        routing_area: FixedAabbWire,
        footprint: FixedAabbWire,
    },
    #[serde(rename = "placeGate")]
    Gate {
        id: u32,
        routing_domain: ReferenceArchitectureRoutingDomainWire,
        gate_type: GateTypeWire,
        origin: FixedVec2Wire,
    },
    #[serde(rename = "placeJunction")]
    Junction {
        id: u32,
        routing_domain: ReferenceArchitectureRoutingDomainWire,
        position: FixedVec2Wire,
    },
    #[serde(rename = "placeWire")]
    Wire {
        id: u32,
        routing_domain: ReferenceArchitectureRoutingDomainWire,
        points: Vec<FixedVec2Wire>,
        endpoint_a: ReferenceArchitectureEndpointWire,
        endpoint_b: ReferenceArchitectureEndpointWire,
    },
}

impl From<&ReferenceArchitectureOperation> for ReferenceArchitectureOperationWire {
    fn from(operation: &ReferenceArchitectureOperation) -> Self {
        match operation {
            ReferenceArchitectureOperation::PlaceFixedSubstrate(value) => Self::FixedSubstrate {
                id: value.id.get(),
                origin: value.origin.into(),
                routing_area: value.routing_area.into(),
                footprint: value.footprint.into(),
            },
            ReferenceArchitectureOperation::PlaceMobileSubstrate(value) => Self::MobileSubstrate {
                id: value.id.get(),
                origin: value.origin.into(),
                routing_area: value.routing_area.into(),
                footprint: value.footprint.into(),
            },
            ReferenceArchitectureOperation::PlaceGate(value) => Self::Gate {
                id: value.id.get(),
                routing_domain: value.routing_domain.into(),
                gate_type: value.gate_type.into(),
                origin: value.origin.into(),
            },
            ReferenceArchitectureOperation::PlaceJunction(value) => Self::Junction {
                id: value.id.get(),
                routing_domain: value.routing_domain.into(),
                position: value.position.into(),
            },
            ReferenceArchitectureOperation::PlaceWire(value) => Self::Wire {
                id: value.id.get(),
                routing_domain: value.routing_domain.into(),
                points: value.points.iter().copied().map(Into::into).collect(),
                endpoint_a: value.endpoint_a.into(),
                endpoint_b: value.endpoint_b.into(),
            },
        }
    }
}

impl TryFrom<ReferenceArchitectureOperationWire> for ReferenceArchitectureOperation {
    type Error = ReferenceArchitectureError;

    fn try_from(wire: ReferenceArchitectureOperationWire) -> Result<Self, Self::Error> {
        Ok(match wire {
            ReferenceArchitectureOperationWire::FixedSubstrate {
                id,
                origin,
                routing_area,
                footprint,
            } => Self::PlaceFixedSubstrate(ReferenceFixedSubstrate {
                id: parse_local_id(id)?,
                origin: origin.into(),
                routing_area: routing_area.into(),
                footprint: footprint.into(),
            }),
            ReferenceArchitectureOperationWire::MobileSubstrate {
                id,
                origin,
                routing_area,
                footprint,
            } => Self::PlaceMobileSubstrate(ReferenceMobileSubstrate {
                id: parse_local_id(id)?,
                origin: origin.into(),
                routing_area: routing_area.into(),
                footprint: footprint.into(),
            }),
            ReferenceArchitectureOperationWire::Gate {
                id,
                routing_domain,
                gate_type,
                origin,
            } => Self::PlaceGate(ReferenceGate {
                id: parse_local_id(id)?,
                routing_domain: routing_domain.try_into()?,
                gate_type: gate_type.into(),
                origin: origin.into(),
            }),
            ReferenceArchitectureOperationWire::Junction {
                id,
                routing_domain,
                position,
            } => Self::PlaceJunction(ReferenceJunction {
                id: parse_local_id(id)?,
                routing_domain: routing_domain.try_into()?,
                position: position.into(),
            }),
            ReferenceArchitectureOperationWire::Wire {
                id,
                routing_domain,
                points,
                endpoint_a,
                endpoint_b,
            } => Self::PlaceWire(ReferenceWire {
                id: parse_local_id(id)?,
                routing_domain: routing_domain.try_into()?,
                points: points.into_iter().map(Into::into).collect(),
                endpoint_a: endpoint_a.try_into()?,
                endpoint_b: endpoint_b.try_into()?,
            }),
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReferenceArchitectureRoleBindingWire {
    name: String,
    target: ReferenceArchitectureSemanticTargetWire,
}

impl From<&ReferenceArchitectureRoleBinding> for ReferenceArchitectureRoleBindingWire {
    fn from(binding: &ReferenceArchitectureRoleBinding) -> Self {
        Self {
            name: binding.name.clone(),
            target: binding.target.into(),
        }
    }
}

impl TryFrom<ReferenceArchitectureRoleBindingWire> for ReferenceArchitectureRoleBinding {
    type Error = ReferenceArchitectureError;

    fn try_from(wire: ReferenceArchitectureRoleBindingWire) -> Result<Self, Self::Error> {
        Ok(Self {
            name: wire.name,
            target: wire.target.try_into()?,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReferenceArchitectureObservationBindingWire {
    name: String,
    target: ReferenceArchitectureSemanticTargetWire,
}

impl From<&ReferenceArchitectureObservationBinding>
    for ReferenceArchitectureObservationBindingWire
{
    fn from(binding: &ReferenceArchitectureObservationBinding) -> Self {
        Self {
            name: binding.name.clone(),
            target: binding.target.into(),
        }
    }
}

impl TryFrom<ReferenceArchitectureObservationBindingWire>
    for ReferenceArchitectureObservationBinding
{
    type Error = ReferenceArchitectureError;

    fn try_from(wire: ReferenceArchitectureObservationBindingWire) -> Result<Self, Self::Error> {
        Ok(Self {
            name: wire.name,
            target: wire.target.try_into()?,
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum ReferenceArchitectureRoutingDomainWire {
    OpenWorld,
    FixedSubstrate { substrate: u32 },
    MobileSubstrate { substrate: u32 },
}

impl From<ReferenceArchitectureRoutingDomain> for ReferenceArchitectureRoutingDomainWire {
    fn from(domain: ReferenceArchitectureRoutingDomain) -> Self {
        match domain {
            ReferenceArchitectureRoutingDomain::OpenWorld => Self::OpenWorld,
            ReferenceArchitectureRoutingDomain::FixedSubstrate(substrate) => Self::FixedSubstrate {
                substrate: substrate.get(),
            },
            ReferenceArchitectureRoutingDomain::MobileSubstrate(substrate) => {
                Self::MobileSubstrate {
                    substrate: substrate.get(),
                }
            }
        }
    }
}

impl TryFrom<ReferenceArchitectureRoutingDomainWire> for ReferenceArchitectureRoutingDomain {
    type Error = ReferenceArchitectureError;

    fn try_from(domain: ReferenceArchitectureRoutingDomainWire) -> Result<Self, Self::Error> {
        Ok(match domain {
            ReferenceArchitectureRoutingDomainWire::OpenWorld => Self::OpenWorld,
            ReferenceArchitectureRoutingDomainWire::FixedSubstrate { substrate } => {
                Self::FixedSubstrate(parse_local_id(substrate)?)
            }
            ReferenceArchitectureRoutingDomainWire::MobileSubstrate { substrate } => {
                Self::MobileSubstrate(parse_local_id(substrate)?)
            }
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum ReferenceArchitectureEndpointWire {
    Free,
    Junction { junction: u32 },
    GatePort { gate: u32, port: GatePortWire },
    MobilePort { mobile: u32, port: MobilePortWire },
    MainCore,
    PowerSource { ordinal: u32 },
    WireSensePort { wire: u32, end: WireEndWire },
}

impl From<ReferenceArchitectureEndpoint> for ReferenceArchitectureEndpointWire {
    fn from(endpoint: ReferenceArchitectureEndpoint) -> Self {
        match endpoint {
            ReferenceArchitectureEndpoint::Free => Self::Free,
            ReferenceArchitectureEndpoint::Junction(junction) => Self::Junction {
                junction: junction.get(),
            },
            ReferenceArchitectureEndpoint::GatePort { gate, port } => Self::GatePort {
                gate: gate.get(),
                port: port.into(),
            },
            ReferenceArchitectureEndpoint::MobilePort { mobile, port } => Self::MobilePort {
                mobile: mobile.get(),
                port: port.into(),
            },
            ReferenceArchitectureEndpoint::MainCore => Self::MainCore,
            ReferenceArchitectureEndpoint::PowerSource { ordinal } => Self::PowerSource { ordinal },
            ReferenceArchitectureEndpoint::WireSensePort { wire, end } => Self::WireSensePort {
                wire: wire.get(),
                end: end.into(),
            },
        }
    }
}

impl TryFrom<ReferenceArchitectureEndpointWire> for ReferenceArchitectureEndpoint {
    type Error = ReferenceArchitectureError;

    fn try_from(endpoint: ReferenceArchitectureEndpointWire) -> Result<Self, Self::Error> {
        Ok(match endpoint {
            ReferenceArchitectureEndpointWire::Free => Self::Free,
            ReferenceArchitectureEndpointWire::Junction { junction } => {
                Self::Junction(parse_local_id(junction)?)
            }
            ReferenceArchitectureEndpointWire::GatePort { gate, port } => Self::GatePort {
                gate: parse_local_id(gate)?,
                port: port.into(),
            },
            ReferenceArchitectureEndpointWire::MobilePort { mobile, port } => Self::MobilePort {
                mobile: parse_local_id(mobile)?,
                port: port.into(),
            },
            ReferenceArchitectureEndpointWire::MainCore => Self::MainCore,
            ReferenceArchitectureEndpointWire::PowerSource { ordinal } => {
                Self::PowerSource { ordinal }
            }
            ReferenceArchitectureEndpointWire::WireSensePort { wire, end } => Self::WireSensePort {
                wire: parse_local_id(wire)?,
                end: end.into(),
            },
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum ReferenceArchitectureSemanticTargetWire {
    LocalEntity { entity: u32 },
    GatePort { gate: u32, port: GatePortWire },
    MobilePort { mobile: u32, port: MobilePortWire },
    WireSensePort { wire: u32, end: WireEndWire },
    MainCore,
    PowerSource { ordinal: u32 },
    Enemy { ordinal: u32 },
}

impl From<ReferenceArchitectureSemanticTarget> for ReferenceArchitectureSemanticTargetWire {
    fn from(target: ReferenceArchitectureSemanticTarget) -> Self {
        match target {
            ReferenceArchitectureSemanticTarget::LocalEntity(entity) => Self::LocalEntity {
                entity: entity.get(),
            },
            ReferenceArchitectureSemanticTarget::GatePort { gate, port } => Self::GatePort {
                gate: gate.get(),
                port: port.into(),
            },
            ReferenceArchitectureSemanticTarget::MobilePort { mobile, port } => Self::MobilePort {
                mobile: mobile.get(),
                port: port.into(),
            },
            ReferenceArchitectureSemanticTarget::WireSensePort { wire, end } => {
                Self::WireSensePort {
                    wire: wire.get(),
                    end: end.into(),
                }
            }
            ReferenceArchitectureSemanticTarget::MainCore => Self::MainCore,
            ReferenceArchitectureSemanticTarget::PowerSource { ordinal } => {
                Self::PowerSource { ordinal }
            }
            ReferenceArchitectureSemanticTarget::Enemy { ordinal } => Self::Enemy { ordinal },
        }
    }
}

impl TryFrom<ReferenceArchitectureSemanticTargetWire> for ReferenceArchitectureSemanticTarget {
    type Error = ReferenceArchitectureError;

    fn try_from(target: ReferenceArchitectureSemanticTargetWire) -> Result<Self, Self::Error> {
        Ok(match target {
            ReferenceArchitectureSemanticTargetWire::LocalEntity { entity } => {
                Self::LocalEntity(parse_local_id(entity)?)
            }
            ReferenceArchitectureSemanticTargetWire::GatePort { gate, port } => Self::GatePort {
                gate: parse_local_id(gate)?,
                port: port.into(),
            },
            ReferenceArchitectureSemanticTargetWire::MobilePort { mobile, port } => {
                Self::MobilePort {
                    mobile: parse_local_id(mobile)?,
                    port: port.into(),
                }
            }
            ReferenceArchitectureSemanticTargetWire::WireSensePort { wire, end } => {
                Self::WireSensePort {
                    wire: parse_local_id(wire)?,
                    end: end.into(),
                }
            }
            ReferenceArchitectureSemanticTargetWire::MainCore => Self::MainCore,
            ReferenceArchitectureSemanticTargetWire::PowerSource { ordinal } => {
                Self::PowerSource { ordinal }
            }
            ReferenceArchitectureSemanticTargetWire::Enemy { ordinal } => Self::Enemy { ordinal },
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
enum GateTypeWire {
    And,
    Or,
    Not,
}

impl From<GateType> for GateTypeWire {
    fn from(value: GateType) -> Self {
        match value {
            GateType::And => Self::And,
            GateType::Or => Self::Or,
            GateType::Not => Self::Not,
        }
    }
}

impl From<GateTypeWire> for GateType {
    fn from(value: GateTypeWire) -> Self {
        match value {
            GateTypeWire::And => Self::And,
            GateTypeWire::Or => Self::Or,
            GateTypeWire::Not => Self::Not,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
enum GatePortWire {
    InputA,
    InputB,
    Output,
    Power,
}

impl From<GatePort> for GatePortWire {
    fn from(value: GatePort) -> Self {
        match value {
            GatePort::InputA => Self::InputA,
            GatePort::InputB => Self::InputB,
            GatePort::Output => Self::Output,
            GatePort::Power => Self::Power,
        }
    }
}

impl From<GatePortWire> for GatePort {
    fn from(value: GatePortWire) -> Self {
        match value {
            GatePortWire::InputA => Self::InputA,
            GatePortWire::InputB => Self::InputB,
            GatePortWire::Output => Self::Output,
            GatePortWire::Power => Self::Power,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
enum MobilePortWire {
    Stop,
    Left,
    Right,
    Build,
}

impl From<MobilePort> for MobilePortWire {
    fn from(value: MobilePort) -> Self {
        match value {
            MobilePort::Stop => Self::Stop,
            MobilePort::Left => Self::Left,
            MobilePort::Right => Self::Right,
            MobilePort::Build => Self::Build,
        }
    }
}

impl From<MobilePortWire> for MobilePort {
    fn from(value: MobilePortWire) -> Self {
        match value {
            MobilePortWire::Stop => Self::Stop,
            MobilePortWire::Left => Self::Left,
            MobilePortWire::Right => Self::Right,
            MobilePortWire::Build => Self::Build,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
enum WireEndWire {
    A,
    B,
}

impl From<WireEnd> for WireEndWire {
    fn from(value: WireEnd) -> Self {
        match value {
            WireEnd::A => Self::A,
            WireEnd::B => Self::B,
        }
    }
}

impl From<WireEndWire> for WireEnd {
    fn from(value: WireEndWire) -> Self {
        match value {
            WireEndWire::A => Self::A,
            WireEndWire::B => Self::B,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FixedVec2Wire {
    x: i64,
    y: i64,
}

impl From<FixedVec2> for FixedVec2Wire {
    fn from(value: FixedVec2) -> Self {
        Self {
            x: value.x.0,
            y: value.y.0,
        }
    }
}

impl From<FixedVec2Wire> for FixedVec2 {
    fn from(value: FixedVec2Wire) -> Self {
        Self::new(Fixed(value.x), Fixed(value.y))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FixedAabbWire {
    min: FixedVec2Wire,
    max: FixedVec2Wire,
}

impl From<FixedAabb> for FixedAabbWire {
    fn from(value: FixedAabb) -> Self {
        Self {
            min: value.min.into(),
            max: value.max.into(),
        }
    }
}

impl From<FixedAabbWire> for FixedAabb {
    fn from(value: FixedAabbWire) -> Self {
        Self::new(value.min.into(), value.max.into())
    }
}

fn parse_local_id(value: u32) -> Result<ReferenceArchitectureLocalId, ReferenceArchitectureError> {
    ReferenceArchitectureLocalId::new(value)
        .ok_or(ReferenceArchitectureError::InvalidLocalId { actual: value })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BalanceProfile, Energy, FIXED_ONE, InitialWorld, Integrity, NumericProfile,
        PowerSourceInitialState, ProfileBundle, SimulationPackage, StageFeatureSet, WireEnd,
    };

    fn id(value: u32) -> ReferenceArchitectureLocalId {
        ReferenceArchitectureLocalId::new(value).expect("nonzero test id")
    }

    fn profiles() -> ProfileBundle {
        ProfileBundle {
            numeric: NumericProfile::reference_v1("reference-architecture-test-numeric"),
            physical_scale: PhysicalScaleProfile::stage0_alpha(
                "reference-architecture-test-physical",
            ),
            balance: BalanceProfile::stage0_alpha("reference-architecture-test-balance"),
        }
    }

    fn materializer_package() -> SimulationPackage {
        let profiles = ProfileBundle {
            numeric: NumericProfile::reference_v1("materializer-numeric"),
            physical_scale: PhysicalScaleProfile::stage0_alpha("materializer-physical"),
            balance: BalanceProfile::construction_contact_damage_alpha("materializer-balance"),
        };
        let contract = SimulationContract::from_profiles(&profiles).expect("valid contract");
        SimulationPackage::new(
            "materializer",
            InitialWorld::MainCorePowerV1 {
                main_core_position: FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
                main_core_integrity: Integrity(100),
                main_core_heat_energy: crate::HeatEnergy(0),
                power_sources: vec![PowerSourceInitialState::new(
                    FixedVec2::new(Fixed(4 * FIXED_ONE), Fixed::ZERO),
                    Energy(1_000),
                )],
            },
            StageFeatureSet {
                signal: true,
                mobility: true,
                capacity: true,
                sensing: true,
                power: true,
                ..StageFeatureSet::none()
            },
            contract,
            profiles,
        )
    }

    fn v2_materializer_package() -> SimulationPackage {
        let profiles = ProfileBundle {
            numeric: NumericProfile::reference_v1("v2-materializer-numeric"),
            physical_scale: PhysicalScaleProfile::stage0_alpha("v2-materializer-physical"),
            balance: BalanceProfile::construction_contact_damage_alpha("v2-materializer-balance"),
        };
        let contract = SimulationContract::from_profiles(&profiles).expect("valid contract");
        SimulationPackage::new(
            "v2-materializer",
            InitialWorld::MainCorePowerV1 {
                main_core_position: FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
                main_core_integrity: Integrity(100),
                main_core_heat_energy: crate::HeatEnergy(0),
                power_sources: vec![
                    PowerSourceInitialState::new(
                        FixedVec2::new(Fixed(4 * FIXED_ONE), Fixed::ZERO),
                        Energy(1_000),
                    ),
                    PowerSourceInitialState::new(
                        FixedVec2::new(Fixed::ZERO, Fixed(4 * FIXED_ONE)),
                        Energy(1_000),
                    ),
                ],
            },
            StageFeatureSet {
                signal: true,
                mobility: true,
                capacity: true,
                sensing: true,
                power: true,
                ..StageFeatureSet::none()
            },
            contract,
            profiles,
        )
    }

    fn scenario_resolution(simulation: &Simulation) -> ReferenceArchitectureScenarioResolution {
        ReferenceArchitectureScenarioResolution {
            main_core: simulation.main_core_state().expect("main core").id(),
            power_sources: simulation
                .power_sources()
                .map(|source| source.id())
                .collect(),
            enemies: simulation
                .enemies()
                .iter()
                .map(|enemy| enemy.id())
                .collect(),
        }
    }

    fn fixture() -> ReferenceArchitectureArtifact {
        let profiles = profiles();
        let contract = SimulationContract::from_profiles(&profiles).expect("valid test profiles");
        let pitch = profiles.physical_scale.world_routing_pitch;
        ReferenceArchitectureArtifact {
            format_version: ReferenceArchitectureFormatVersion::V1,
            hash_algorithm_id: HashAlgorithmId::Blake3V1,
            display_name: "Brute reference".to_owned(),
            contract,
            operations: vec![
                ReferenceArchitectureOperation::PlaceWire(ReferenceWire {
                    id: id(2),
                    routing_domain: ReferenceArchitectureRoutingDomain::OpenWorld,
                    points: vec![
                        FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
                        FixedVec2::new(pitch, Fixed::ZERO),
                    ],
                    endpoint_a: ReferenceArchitectureEndpoint::Junction(id(1)),
                    endpoint_b: ReferenceArchitectureEndpoint::MainCore,
                }),
                ReferenceArchitectureOperation::PlaceJunction(ReferenceJunction {
                    id: id(1),
                    routing_domain: ReferenceArchitectureRoutingDomain::OpenWorld,
                    position: FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
                }),
            ],
            role_bindings: vec![ReferenceArchitectureRoleBinding {
                name: "defense-wire".to_owned(),
                target: ReferenceArchitectureSemanticTarget::LocalEntity(id(2)),
            }],
            observation_bindings: vec![
                ReferenceArchitectureObservationBinding {
                    name: "hostile-entry".to_owned(),
                    target: ReferenceArchitectureSemanticTarget::Enemy { ordinal: 0 },
                },
                ReferenceArchitectureObservationBinding {
                    name: "defense-activation".to_owned(),
                    target: ReferenceArchitectureSemanticTarget::WireSensePort {
                        wire: id(2),
                        end: WireEnd::A,
                    },
                },
            ],
            materialization_schedule: None,
        }
    }

    fn v2_fixture(simulation: &Simulation) -> ReferenceArchitectureArtifact {
        let pitch = simulation.profiles().physical_scale.world_routing_pitch;
        let point = |x: i64, y: i64| FixedVec2::new(Fixed(x * pitch.0), Fixed(y * pitch.0));
        ReferenceArchitectureArtifact {
            format_version: ReferenceArchitectureFormatVersion::V2,
            hash_algorithm_id: HashAlgorithmId::Blake3V1,
            display_name: "v2 staged fixture".to_owned(),
            contract: *simulation.contract(),
            operations: vec![
                ReferenceArchitectureOperation::PlaceJunction(ReferenceJunction {
                    id: id(1),
                    routing_domain: ReferenceArchitectureRoutingDomain::OpenWorld,
                    position: point(1, 0),
                }),
                ReferenceArchitectureOperation::PlaceWire(ReferenceWire {
                    id: id(2),
                    routing_domain: ReferenceArchitectureRoutingDomain::OpenWorld,
                    points: vec![point(0, 0), point(1, 0)],
                    endpoint_a: ReferenceArchitectureEndpoint::MainCore,
                    endpoint_b: ReferenceArchitectureEndpoint::Junction(id(1)),
                }),
                ReferenceArchitectureOperation::PlaceJunction(ReferenceJunction {
                    id: id(3),
                    routing_domain: ReferenceArchitectureRoutingDomain::OpenWorld,
                    position: point(3, 0),
                }),
                ReferenceArchitectureOperation::PlaceWire(ReferenceWire {
                    id: id(4),
                    routing_domain: ReferenceArchitectureRoutingDomain::OpenWorld,
                    points: vec![point(4, 0), point(3, 0)],
                    endpoint_a: ReferenceArchitectureEndpoint::PowerSource { ordinal: 0 },
                    endpoint_b: ReferenceArchitectureEndpoint::Junction(id(3)),
                }),
                ReferenceArchitectureOperation::PlaceJunction(ReferenceJunction {
                    id: id(5),
                    routing_domain: ReferenceArchitectureRoutingDomain::OpenWorld,
                    position: point(0, 3),
                }),
                ReferenceArchitectureOperation::PlaceWire(ReferenceWire {
                    id: id(6),
                    routing_domain: ReferenceArchitectureRoutingDomain::OpenWorld,
                    points: vec![point(0, 4), point(0, 3)],
                    endpoint_a: ReferenceArchitectureEndpoint::PowerSource { ordinal: 1 },
                    endpoint_b: ReferenceArchitectureEndpoint::Junction(id(5)),
                }),
            ],
            role_bindings: Vec::new(),
            observation_bindings: Vec::new(),
            materialization_schedule: Some(ReferenceArchitectureMaterializationSchedule {
                binding_batches: vec![
                    vec![
                        ReferenceArchitectureBindingEndpoint {
                            wire: id(2),
                            end: WireEnd::A,
                        },
                        ReferenceArchitectureBindingEndpoint {
                            wire: id(2),
                            end: WireEnd::B,
                        },
                        ReferenceArchitectureBindingEndpoint {
                            wire: id(4),
                            end: WireEnd::B,
                        },
                        ReferenceArchitectureBindingEndpoint {
                            wire: id(6),
                            end: WireEnd::B,
                        },
                    ],
                    vec![ReferenceArchitectureBindingEndpoint {
                        wire: id(4),
                        end: WireEnd::A,
                    }],
                    vec![ReferenceArchitectureBindingEndpoint {
                        wire: id(6),
                        end: WireEnd::A,
                    }],
                ],
            }),
        }
    }

    fn v2_wire_only_fixture(simulation: &Simulation) -> ReferenceArchitectureArtifact {
        let pitch = simulation.profiles().physical_scale.world_routing_pitch;
        let point = |x: i64, y: i64| FixedVec2::new(Fixed(x * pitch.0), Fixed(y * pitch.0));
        ReferenceArchitectureArtifact {
            format_version: ReferenceArchitectureFormatVersion::V2,
            hash_algorithm_id: HashAlgorithmId::Blake3V1,
            display_name: "v2 empty-placement-side fixture".to_owned(),
            contract: *simulation.contract(),
            operations: vec![
                ReferenceArchitectureOperation::PlaceWire(ReferenceWire {
                    id: id(2),
                    routing_domain: ReferenceArchitectureRoutingDomain::OpenWorld,
                    points: vec![point(0, 0), point(4, 0)],
                    endpoint_a: ReferenceArchitectureEndpoint::MainCore,
                    endpoint_b: ReferenceArchitectureEndpoint::PowerSource { ordinal: 0 },
                }),
                ReferenceArchitectureOperation::PlaceWire(ReferenceWire {
                    id: id(4),
                    routing_domain: ReferenceArchitectureRoutingDomain::OpenWorld,
                    points: vec![point(0, 0), point(0, 4)],
                    endpoint_a: ReferenceArchitectureEndpoint::MainCore,
                    endpoint_b: ReferenceArchitectureEndpoint::PowerSource { ordinal: 1 },
                }),
            ],
            role_bindings: Vec::new(),
            observation_bindings: Vec::new(),
            materialization_schedule: Some(ReferenceArchitectureMaterializationSchedule {
                binding_batches: vec![
                    vec![
                        ReferenceArchitectureBindingEndpoint {
                            wire: id(2),
                            end: WireEnd::A,
                        },
                        ReferenceArchitectureBindingEndpoint {
                            wire: id(4),
                            end: WireEnd::A,
                        },
                    ],
                    Vec::new(),
                    vec![
                        ReferenceArchitectureBindingEndpoint {
                            wire: id(2),
                            end: WireEnd::B,
                        },
                        ReferenceArchitectureBindingEndpoint {
                            wire: id(4),
                            end: WireEnd::B,
                        },
                    ],
                ],
            }),
        }
    }

    #[test]
    fn semantic_hash_normalizes_order_and_excludes_display_name() {
        let baseline = fixture();
        let mut reordered = baseline.clone();
        reordered.operations.reverse();
        reordered.observation_bindings.reverse();
        reordered.display_name = "Computed display-only label".to_owned();

        assert_eq!(
            baseline.semantic_hash().expect("baseline hash"),
            reordered.semantic_hash().expect("reordered hash")
        );
    }

    #[test]
    fn strict_json_round_trips_to_canonical_order() {
        let artifact = fixture();
        let encoded = encode_reference_architecture_artifact(&artifact).expect("encode fixture");
        let decoded = decode_reference_architecture_artifact(&encoded).expect("decode fixture");
        let reencoded =
            encode_reference_architecture_artifact(&decoded).expect("re-encode fixture");

        const V1_GOLDEN: &str = r#"{
  "formatVersion": 1,
  "hashAlgorithmId": "blake3-v1",
  "displayName": "Brute reference",
  "contract": {
    "semanticsVersion": "aon-semantics-v1",
    "numericProfileHash": "fe92f0c723660040a3200254890c8a34ec3ed9e65fc242de1c0951e4ecd00469",
    "physicalScaleProfileHash": "0e0f7fe8c9ccbf0b159d44e4e53d05417cf558c37e796e5f8bccd8221aec6490",
    "balanceProfileHash": "b1540d6ad19c616ce60e96523108264355311168c51a0b92de2fdf596e2646fd"
  },
  "operations": [
    {
      "kind": "placeJunction",
      "id": 1,
      "routing_domain": {
        "kind": "openWorld"
      },
      "position": {
        "x": 0,
        "y": 0
      }
    },
    {
      "kind": "placeWire",
      "id": 2,
      "routing_domain": {
        "kind": "openWorld"
      },
      "points": [
        {
          "x": 0,
          "y": 0
        },
        {
          "x": 65536,
          "y": 0
        }
      ],
      "endpoint_a": {
        "kind": "junction",
        "junction": 1
      },
      "endpoint_b": {
        "kind": "mainCore"
      }
    }
  ],
  "roleBindings": [
    {
      "name": "defense-wire",
      "target": {
        "kind": "localEntity",
        "entity": 2
      }
    }
  ],
  "observationBindings": [
    {
      "name": "defense-activation",
      "target": {
        "kind": "wireSensePort",
        "wire": 2,
        "end": "a"
      }
    },
    {
      "name": "hostile-entry",
      "target": {
        "kind": "enemy",
        "ordinal": 0
      }
    }
  ]
}
"#;
        assert_eq!(encoded, V1_GOLDEN);
        assert_eq!(
            artifact.semantic_hash().expect("fixture hash").to_string(),
            "5b5a55dcde9160076528c669785a5e6f7c3896f0a2aef23c47eca75be5851c1b"
        );

        assert_eq!(encoded, reencoded);
        assert_eq!(artifact.semantic_hash(), decoded.semantic_hash());
        let unknown = encoded.replacen(
            "\"displayName\":",
            "\"unexpected\": true,\n  \"displayName\":",
            1,
        );
        assert!(matches!(
            decode_reference_architecture_artifact(&unknown),
            Err(ReferenceArchitectureError::InvalidJson { .. })
        ));

        let unsupported_version =
            encoded.replacen("\"formatVersion\": 1", "\"formatVersion\": 3", 1);
        assert_eq!(
            decode_reference_architecture_artifact(&unsupported_version),
            Err(ReferenceArchitectureError::UnsupportedFormatVersion {
                expected: 2,
                actual: 3,
            })
        );
        let unsupported_algorithm = encoded.replacen(
            "\"hashAlgorithmId\": \"blake3-v1\"",
            "\"hashAlgorithmId\": \"future-hash\"",
            1,
        );
        assert_eq!(
            decode_reference_architecture_artifact(&unsupported_algorithm),
            Err(ReferenceArchitectureError::UnsupportedHashAlgorithm {
                actual: "future-hash".to_owned(),
            })
        );
    }

    #[test]
    fn validation_rejects_duplicate_dangling_wrong_kind_and_duplicate_bindings() {
        let mut duplicate = fixture();
        duplicate
            .operations
            .push(ReferenceArchitectureOperation::PlaceJunction(
                ReferenceJunction {
                    id: id(1),
                    routing_domain: ReferenceArchitectureRoutingDomain::OpenWorld,
                    position: FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
                },
            ));
        assert_eq!(
            duplicate.semantic_hash(),
            Err(ReferenceArchitectureError::DuplicateLocalId { id: id(1) })
        );

        let mut dangling = fixture();
        let ReferenceArchitectureOperation::PlaceWire(wire) = &mut dangling.operations[0] else {
            panic!("fixture wire")
        };
        wire.endpoint_a = ReferenceArchitectureEndpoint::Junction(id(99));
        assert_eq!(
            dangling.semantic_hash(),
            Err(ReferenceArchitectureError::DanglingReference { id: id(99) })
        );

        let mut wrong_kind = fixture();
        let ReferenceArchitectureOperation::PlaceWire(wire) = &mut wrong_kind.operations[0] else {
            panic!("fixture wire")
        };
        wire.endpoint_a = ReferenceArchitectureEndpoint::Junction(id(2));
        assert_eq!(
            wrong_kind.semantic_hash(),
            Err(ReferenceArchitectureError::WrongKindReference {
                id: id(2),
                expected: "junction",
            })
        );

        let mut duplicate_binding = fixture();
        duplicate_binding
            .observation_bindings
            .push(duplicate_binding.observation_bindings[0].clone());
        assert_eq!(
            duplicate_binding.semantic_hash(),
            Err(ReferenceArchitectureError::DuplicateObservationName {
                name: "hostile-entry".to_owned(),
            })
        );
    }

    #[test]
    fn validation_binds_the_full_profile_contract() {
        let artifact = fixture();
        let profiles = profiles();
        let target = SimulationContract::from_profiles(&profiles).expect("valid test contract");
        validate_reference_architecture_against(&artifact, &target, &profiles.physical_scale)
            .expect("fixture validates");

        let mut mismatched = target;
        mismatched.balance_profile_hash = artifact.contract.numeric_profile_hash;
        assert!(matches!(
            validate_reference_architecture_against(
                &artifact,
                &mismatched,
                &profiles.physical_scale
            ),
            Err(ReferenceArchitectureError::BalanceProfileMismatch { .. })
        ));
    }

    #[test]
    fn v2_strict_json_round_trip_and_hash_bind_the_schedule() {
        let simulation = Simulation::new(v2_materializer_package()).expect("fresh simulation");
        let artifact = v2_fixture(&simulation);
        let encoded = encode_reference_architecture_artifact(&artifact).expect("encode v2");
        assert!(encoded.contains("\"formatVersion\": 2"));
        assert!(encoded.contains("\"materializationSchedule\""));
        assert!(encoded.contains("\"bindingBatches\""));
        let decoded = decode_reference_architecture_artifact(&encoded).expect("decode v2");
        assert_eq!(decoded, artifact);
        assert_eq!(
            encode_reference_architecture_artifact(&decoded).expect("reencode v2"),
            encoded
        );

        let mut reordered_stages = artifact.clone();
        reordered_stages
            .materialization_schedule
            .as_mut()
            .expect("v2 schedule")
            .binding_batches
            .swap(1, 2);
        assert_ne!(
            artifact.semantic_hash().expect("baseline hash"),
            reordered_stages.semantic_hash().expect("reordered hash")
        );
        assert!(matches!(
            decode_reference_architecture_artifact(&encoded.replacen(
                "\"materializationSchedule\":",
                "\"unexpected\": true,\n  \"materializationSchedule\":",
                1,
            )),
            Err(ReferenceArchitectureError::InvalidJson { .. })
        ));
    }

    #[test]
    fn v2_schedule_validation_rejects_missing_duplicate_free_and_late_nonpower_bindings() {
        let simulation = Simulation::new(v2_materializer_package()).expect("fresh simulation");

        let mut missing_schedule = v2_fixture(&simulation);
        missing_schedule.materialization_schedule = None;
        assert_eq!(
            missing_schedule.semantic_hash(),
            Err(ReferenceArchitectureError::MissingMaterializationSchedule)
        );

        let mut missing_endpoint = v2_fixture(&simulation);
        missing_endpoint
            .materialization_schedule
            .as_mut()
            .expect("schedule")
            .binding_batches[0]
            .remove(0);
        assert_eq!(
            missing_endpoint.semantic_hash(),
            Err(ReferenceArchitectureError::MissingScheduledEndpoint {
                wire: id(2),
                end: WireEnd::A,
            })
        );

        let mut duplicate = v2_fixture(&simulation);
        duplicate
            .materialization_schedule
            .as_mut()
            .expect("schedule")
            .binding_batches[2]
            .insert(
                0,
                ReferenceArchitectureBindingEndpoint {
                    wire: id(4),
                    end: WireEnd::A,
                },
            );
        assert_eq!(
            duplicate.semantic_hash(),
            Err(ReferenceArchitectureError::DuplicateScheduledEndpoint {
                wire: id(4),
                end: WireEnd::A,
            })
        );

        let mut free = v2_fixture(&simulation);
        let ReferenceArchitectureOperation::PlaceWire(wire) = &mut free.operations[5] else {
            panic!("wire fixture")
        };
        wire.endpoint_b = ReferenceArchitectureEndpoint::Free;
        assert_eq!(
            free.semantic_hash(),
            Err(ReferenceArchitectureError::ScheduledFreeEndpoint {
                wire: id(6),
                end: WireEnd::B,
            })
        );

        let mut late_nonpower = v2_fixture(&simulation);
        let schedule = late_nonpower
            .materialization_schedule
            .as_mut()
            .expect("schedule");
        let binding = schedule.binding_batches[0].remove(0);
        schedule.binding_batches[1].insert(0, binding);
        assert_eq!(
            late_nonpower.semantic_hash(),
            Err(ReferenceArchitectureError::LateNonPowerSourceBinding {
                stage: 1,
                wire: id(2),
                end: WireEnd::A,
            })
        );
    }

    #[test]
    fn v2_materializer_records_each_earliest_post_stage_quiescence_barrier() {
        let simulation = Simulation::new(v2_materializer_package()).expect("fresh simulation");
        let scenario = scenario_resolution(&simulation);
        let artifact = v2_fixture(&simulation);
        let (simulation, evidence) =
            materialize_reference_architecture(simulation, &artifact, &scenario)
                .expect("materialize v2 fixture");

        assert_eq!(evidence.binding_stage_evidence.len(), 3);
        assert_eq!(
            evidence
                .executed_batch_evidence
                .iter()
                .map(|evidence| evidence.kind)
                .collect::<Vec<_>>(),
            artifact
                .materialization_plan()
                .expect("validated plan")
                .execution_batches()
                .map(|(kind, _)| kind)
                .collect::<Vec<_>>()
        );
        assert_eq!(simulation.next_tick(), evidence.build_end_tick);
        assert!(
            simulation
                .signal_quiescence_snapshot()
                .expect("quiescence snapshot")
                .is_quiescent()
        );
        for (expected_stage, stage) in evidence.binding_stage_evidence.iter().enumerate() {
            assert_eq!(stage.stage, expected_stage as u8);
            assert!(stage.command_tick < stage.quiescent_tick);
            assert_eq!(
                stage.barrier_ticks.first().copied(),
                (stage.command_tick.checked_add(Tick(1)).expect("next tick")
                    < stage.quiescent_tick)
                    .then_some(stage.command_tick.checked_add(Tick(1)).expect("next tick"))
            );
            assert_eq!(
                stage.barrier_ticks.last().copied(),
                (!stage.barrier_ticks.is_empty()).then_some(Tick(stage.quiescent_tick.0 - 1))
            );
        }
        assert_eq!(
            evidence
                .binding_stage_evidence
                .last()
                .expect("final stage")
                .quiescent_tick,
            evidence.build_end_tick
        );
    }

    #[test]
    fn v2_pair_materializer_preserves_empty_stages_and_equal_build_end_atomically() {
        let package = v2_materializer_package();
        let left_candidate = Simulation::new(package.clone()).expect("left candidate");
        let right_candidate = Simulation::new(package.clone()).expect("right candidate");
        let left_scenario = scenario_resolution(&left_candidate);
        let right_scenario = scenario_resolution(&right_candidate);
        let left_artifact = v2_fixture(&left_candidate);
        let right_artifact = v2_wire_only_fixture(&right_candidate);

        let ((left_simulation, left), (right_simulation, right)) =
            materialize_reference_architecture_pair(
                (left_candidate, &left_artifact, &left_scenario),
                (right_candidate, &right_artifact, &right_scenario),
            )
            .expect("lockstep pair");
        assert_eq!(left.build_end_tick, right.build_end_tick);
        assert_eq!(left_simulation.next_tick(), right_simulation.next_tick());
        assert_eq!(left.executed_batch_evidence, right.executed_batch_evidence);
        assert_eq!(left.binding_stage_evidence, right.binding_stage_evidence);
        assert_eq!(left.binding_stage_evidence.len(), 3);
        let right_plan = right_artifact.materialization_plan().expect("right plan");
        assert_eq!(
            right_plan.batch(ReferenceArchitectureMaterializationBatchKind::Placement { phase: 1 }),
            None
        );
        assert!(right.executed_batch_evidence.iter().any(|evidence| {
            evidence.kind == ReferenceArchitectureMaterializationBatchKind::Placement { phase: 1 }
        }));
        assert_eq!(
            right_plan.execution_batches().find_map(|(kind, batch)| {
                (kind == ReferenceArchitectureMaterializationBatchKind::Binding { stage: 1 })
                    .then_some(batch.len())
            }),
            Some(0)
        );

        let retained = Simulation::new(package.clone()).expect("retained original");
        let retained_tick = retained.next_tick();
        let retained_hash = retained.state_hash();
        let failed_left = Simulation::new(package.clone()).expect("failed left candidate");
        let failed_right = Simulation::new(package).expect("failed right candidate");
        let failed_left_scenario = scenario_resolution(&failed_left);
        let mut invalid_right_scenario = scenario_resolution(&failed_right);
        invalid_right_scenario.power_sources.reverse();
        assert!(matches!(
            materialize_reference_architecture_pair(
                (failed_left, &left_artifact, &failed_left_scenario),
                (failed_right, &right_artifact, &invalid_right_scenario),
            ),
            Err(ReferenceArchitectureError::PowerSourceAnchorMismatch { .. })
        ));
        assert_eq!(retained.next_tick(), retained_tick);
        assert_eq!(retained.state_hash(), retained_hash);
    }

    #[test]
    fn plan_places_unbound_then_resolves_bindings_without_raw_artifact_ids() {
        let artifact = fixture();
        let plan = artifact.materialization_plan().expect("valid plan");
        assert_eq!(plan.steps().len(), 4);
        let batches: Vec<_> = plan
            .phase_batches()
            .map(|(phase, steps)| (phase, steps.len()))
            .collect();
        assert_eq!(batches, vec![(1, 1), (2, 1), (6, 2)]);
        assert!(matches!(
            plan.steps()[0],
            ReferenceArchitectureMaterializationStep::Placement(
                ReferenceArchitectureOperation::PlaceJunction(_)
            )
        ));
        assert!(matches!(
            plan.steps()[1],
            ReferenceArchitectureMaterializationStep::Placement(
                ReferenceArchitectureOperation::PlaceWire(_)
            )
        ));

        let mut locals = BTreeMap::new();
        plan.steps()[0]
            .record_acceptance(
                CommandAcceptance {
                    target_tick: Tick(8),
                    ordinal: 5,
                    created_entity: Some(EntityId(41)),
                },
                &mut locals,
            )
            .expect("record junction creation");
        locals.insert(id(2), EntityId(42));
        let scenario = ReferenceArchitectureScenarioResolution {
            main_core: MainCoreId(EntityId(1)),
            power_sources: vec![PowerSourceId(EntityId(2))],
            enemies: vec![EnemyId(EntityId(3))],
        };
        let resolved = plan.steps()[2]
            .resolve_command(Tick(10), 7, &locals, &scenario)
            .expect("resolve local junction binding");
        assert_eq!(
            resolved.command,
            Command::BindPort(BindPortCommand {
                wire: WireId(EntityId(42)),
                end: WireEnd::A,
                target: EndpointTarget::Junction(JunctionId(EntityId(41))),
            })
        );
    }

    #[test]
    fn plan_uses_all_seven_dependency_phases_and_sorts_each_batch() {
        let mut artifact = fixture();
        let point = FixedVec2::new(Fixed::ZERO, Fixed::ZERO);
        let bounds = FixedAabb::new(
            FixedVec2::new(Fixed(-FIXED_ONE), Fixed(-FIXED_ONE)),
            FixedVec2::new(Fixed(FIXED_ONE), Fixed(FIXED_ONE)),
        );
        artifact.operations = vec![
            ReferenceArchitectureOperation::PlaceFixedSubstrate(ReferenceFixedSubstrate {
                id: id(70),
                origin: point,
                routing_area: bounds,
                footprint: bounds,
            }),
            ReferenceArchitectureOperation::PlaceWire(ReferenceWire {
                id: id(50),
                routing_domain: ReferenceArchitectureRoutingDomain::OpenWorld,
                points: vec![point, point],
                endpoint_a: ReferenceArchitectureEndpoint::Junction(id(60)),
                endpoint_b: ReferenceArchitectureEndpoint::MainCore,
            }),
            ReferenceArchitectureOperation::PlaceMobileSubstrate(ReferenceMobileSubstrate {
                id: id(40),
                origin: point,
                routing_area: bounds,
                footprint: bounds,
            }),
            ReferenceArchitectureOperation::PlaceGate(ReferenceGate {
                id: id(35),
                routing_domain: ReferenceArchitectureRoutingDomain::MobileSubstrate(id(40)),
                gate_type: GateType::Or,
                origin: point,
            }),
            ReferenceArchitectureOperation::PlaceJunction(ReferenceJunction {
                id: id(30),
                routing_domain: ReferenceArchitectureRoutingDomain::MobileSubstrate(id(40)),
                position: point,
            }),
            ReferenceArchitectureOperation::PlaceWire(ReferenceWire {
                id: id(25),
                routing_domain: ReferenceArchitectureRoutingDomain::FixedSubstrate(id(10)),
                points: vec![point, point],
                endpoint_a: ReferenceArchitectureEndpoint::GatePort {
                    gate: id(20),
                    port: GatePort::Output,
                },
                endpoint_b: ReferenceArchitectureEndpoint::Junction(id(15)),
            }),
            ReferenceArchitectureOperation::PlaceGate(ReferenceGate {
                id: id(20),
                routing_domain: ReferenceArchitectureRoutingDomain::FixedSubstrate(id(10)),
                gate_type: GateType::Or,
                origin: point,
            }),
            ReferenceArchitectureOperation::PlaceJunction(ReferenceJunction {
                id: id(15),
                routing_domain: ReferenceArchitectureRoutingDomain::FixedSubstrate(id(10)),
                position: point,
            }),
            ReferenceArchitectureOperation::PlaceFixedSubstrate(ReferenceFixedSubstrate {
                id: id(10),
                origin: point,
                routing_area: bounds,
                footprint: bounds,
            }),
            ReferenceArchitectureOperation::PlaceJunction(ReferenceJunction {
                id: id(60),
                routing_domain: ReferenceArchitectureRoutingDomain::OpenWorld,
                position: point,
            }),
            ReferenceArchitectureOperation::PlaceWire(ReferenceWire {
                id: id(45),
                routing_domain: ReferenceArchitectureRoutingDomain::MobileSubstrate(id(40)),
                points: vec![point, point],
                endpoint_a: ReferenceArchitectureEndpoint::GatePort {
                    gate: id(35),
                    port: GatePort::Output,
                },
                endpoint_b: ReferenceArchitectureEndpoint::Junction(id(30)),
            }),
        ];
        artifact.role_bindings.clear();
        artifact.observation_bindings.clear();

        let plan = artifact
            .materialization_plan()
            .expect("valid dependency plan");
        let batch_keys = plan
            .phase_batches()
            .map(|(phase, steps)| {
                (
                    phase,
                    steps
                        .iter()
                        .map(materialization_step_sort_key)
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            batch_keys,
            vec![
                (0, vec![(10, 0), (70, 0)]),
                (1, vec![(15, 0), (20, 0), (60, 0)]),
                (2, vec![(25, 0), (50, 0)]),
                (3, vec![(40, 0)]),
                (4, vec![(30, 0), (35, 0)]),
                (5, vec![(45, 0)]),
                (
                    6,
                    vec![(25, 0), (25, 1), (45, 0), (45, 1), (50, 0), (50, 1)]
                ),
            ]
        );

        let mut permuted = artifact;
        permuted.operations.reverse();
        assert_eq!(
            plan,
            permuted
                .materialization_plan()
                .expect("permuted dependency plan")
        );
    }

    #[test]
    fn scenario_ordinals_are_resolved_explicitly() {
        let locals = BTreeMap::new();
        let scenario = ReferenceArchitectureScenarioResolution {
            main_core: MainCoreId(EntityId(10)),
            power_sources: vec![PowerSourceId(EntityId(20))],
            enemies: vec![EnemyId(EntityId(30))],
        };
        assert_eq!(
            resolve_reference_architecture_semantic_target(
                ReferenceArchitectureSemanticTarget::Enemy { ordinal: 0 },
                &locals,
                &scenario,
            ),
            Ok(ResolvedReferenceArchitectureSemanticTarget::Enemy(EnemyId(
                EntityId(30)
            )))
        );
        assert_eq!(
            resolve_reference_architecture_semantic_target(
                ReferenceArchitectureSemanticTarget::Enemy { ordinal: 1 },
                &locals,
                &scenario,
            ),
            Err(ReferenceArchitectureError::MissingScenarioAnchor {
                kind: "enemy",
                ordinal: 1,
            })
        );

        let artifact = fixture();
        artifact
            .validate_scenario_resolution(&scenario)
            .expect("fixture ordinals exist");
        let missing = ReferenceArchitectureScenarioResolution {
            enemies: Vec::new(),
            ..scenario
        };
        assert_eq!(
            artifact.validate_scenario_resolution(&missing),
            Err(ReferenceArchitectureError::MissingScenarioAnchor {
                kind: "enemy",
                ordinal: 0,
            })
        );
    }

    #[test]
    fn command_log_hash_is_complete_and_order_sensitive() {
        let first = CommandEnvelope {
            target_tick: Tick(1),
            ordinal: 0,
            command: Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: RoutingDomain::OpenWorld,
                position: FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
            }),
        };
        let second = CommandEnvelope {
            target_tick: Tick(2),
            ordinal: 0,
            command: Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: RoutingDomain::OpenWorld,
                position: FixedVec2::new(Fixed(FIXED_ONE), Fixed::ZERO),
            }),
        };
        let forward = reference_architecture_command_log_hash(&[first.clone(), second.clone()])
            .expect("forward hash");
        let reverse =
            reference_architecture_command_log_hash(&[second, first]).expect("reverse hash");
        assert_ne!(forward, reverse);
        assert_ne!(
            forward,
            reference_architecture_command_log_hash(&[]).expect("empty hash")
        );
    }

    #[test]
    fn materializer_executes_one_dependency_batch_per_tick_and_returns_complete_evidence() {
        let simulation = Simulation::new(materializer_package()).expect("fresh simulation");
        let scenario = scenario_resolution(&simulation);
        let contract = *simulation.contract();
        let pitch = simulation.profiles().physical_scale.world_routing_pitch;
        let artifact = ReferenceArchitectureArtifact {
            format_version: ReferenceArchitectureFormatVersion::V1,
            hash_algorithm_id: HashAlgorithmId::Blake3V1,
            display_name: "atomic materializer test".to_owned(),
            contract,
            operations: vec![
                ReferenceArchitectureOperation::PlaceJunction(ReferenceJunction {
                    id: id(1),
                    routing_domain: ReferenceArchitectureRoutingDomain::OpenWorld,
                    position: FixedVec2::new(pitch, Fixed::ZERO),
                }),
                ReferenceArchitectureOperation::PlaceWire(ReferenceWire {
                    id: id(2),
                    routing_domain: ReferenceArchitectureRoutingDomain::OpenWorld,
                    points: vec![
                        FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
                        FixedVec2::new(pitch, Fixed::ZERO),
                    ],
                    endpoint_a: ReferenceArchitectureEndpoint::MainCore,
                    endpoint_b: ReferenceArchitectureEndpoint::Junction(id(1)),
                }),
                ReferenceArchitectureOperation::PlaceJunction(ReferenceJunction {
                    id: id(3),
                    routing_domain: ReferenceArchitectureRoutingDomain::OpenWorld,
                    position: FixedVec2::new(Fixed(2 * pitch.0), Fixed::ZERO),
                }),
            ],
            role_bindings: Vec::new(),
            observation_bindings: Vec::new(),
            materialization_schedule: None,
        };

        let (simulation, result) =
            materialize_reference_architecture(simulation, &artifact, &scenario)
                .expect("materialize fixture");
        assert_eq!(result.local_entities.len(), 3);
        assert_eq!(result.commands.len(), 5);
        assert_eq!(result.acceptances.len(), 5);
        assert_eq!(result.build_end_tick, Tick(3));
        assert!(result.executed_batch_evidence.is_empty());
        assert!(result.binding_stage_evidence.is_empty());
        assert_eq!(simulation.next_tick(), result.build_end_tick);
        assert_eq!(
            result.command_log_hash,
            reference_architecture_command_log_hash(&result.commands).expect("command log hash")
        );
        assert_eq!(
            result
                .commands
                .iter()
                .map(|command| (command.target_tick, command.ordinal))
                .collect::<Vec<_>>(),
            vec![
                (Tick(0), 0),
                (Tick(0), 1),
                (Tick(1), 0),
                (Tick(2), 0),
                (Tick(2), 1),
            ]
        );
        assert!(
            result
                .commands
                .iter()
                .zip(&result.acceptances)
                .all(
                    |(command, acceptance)| command.target_tick == acceptance.target_tick
                        && command.ordinal == acceptance.ordinal
                )
        );
    }

    #[test]
    fn failed_late_batch_returns_no_partial_candidate_or_evidence() {
        let package = materializer_package();
        let original = Simulation::new(package.clone()).expect("retained original");
        let candidate = Simulation::new(package).expect("private candidate");
        let original_tick = original.next_tick();
        let original_hash = original.state_hash();
        let scenario = scenario_resolution(&candidate);
        let contract = *candidate.contract();
        let pitch = candidate.profiles().physical_scale.world_routing_pitch;
        let artifact = ReferenceArchitectureArtifact {
            format_version: ReferenceArchitectureFormatVersion::V1,
            hash_algorithm_id: HashAlgorithmId::Blake3V1,
            display_name: "late binding rejection".to_owned(),
            contract,
            operations: vec![
                ReferenceArchitectureOperation::PlaceJunction(ReferenceJunction {
                    id: id(1),
                    routing_domain: ReferenceArchitectureRoutingDomain::OpenWorld,
                    position: FixedVec2::new(pitch, Fixed::ZERO),
                }),
                ReferenceArchitectureOperation::PlaceWire(ReferenceWire {
                    id: id(2),
                    routing_domain: ReferenceArchitectureRoutingDomain::OpenWorld,
                    points: vec![
                        FixedVec2::new(Fixed(2 * pitch.0), Fixed::ZERO),
                        FixedVec2::new(pitch, Fixed::ZERO),
                    ],
                    // Static validation cannot substitute the Scenario-owned Core coordinate;
                    // phase 6 rejects this deliberately wrong runtime binding position.
                    endpoint_a: ReferenceArchitectureEndpoint::MainCore,
                    endpoint_b: ReferenceArchitectureEndpoint::Junction(id(1)),
                }),
            ],
            role_bindings: Vec::new(),
            observation_bindings: Vec::new(),
            materialization_schedule: None,
        };

        let error = match materialize_reference_architecture(candidate, &artifact, &scenario) {
            Ok(_) => panic!("late invalid binding must not publish a candidate"),
            Err(error) => error,
        };
        assert!(matches!(
            error,
            ReferenceArchitectureError::MaterializationBatchRejected {
                phase: 6,
                target_tick: Tick(2),
                ..
            }
        ));
        assert_eq!(original.next_tick(), original_tick);
        assert_eq!(original.state_hash(), original_hash);
    }
}
