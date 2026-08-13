#![forbid(unsafe_code)]

mod artifact;
mod canonical;
mod capacity;
mod capacity_support;
mod command;
mod construction;
mod contact;
mod contract;
mod damage_state;
mod enemy;
mod error;
mod event;
mod experiment;
mod experiment_artifact;
mod geometry;
mod hash;
mod identity;
mod main_core;
mod mobility;
mod module;
mod numeric;
mod path_certificate;
mod power;
mod power_adapter;
mod power_runtime;
mod power_source;
mod power_topology;
mod profile;
mod reference_architecture;
mod reference_experiment;
mod reference_metrics;
mod replay;
mod sensing;
mod signal;
mod signal_topology;
mod simulation;
mod snapshot;
mod structural;
mod structural_geometry;
mod thermal_damage;
mod topology;

pub use artifact::{
    ArtifactBytes, ArtifactKind, EnemyInitialState, InitialWorld,
    PhysicalScaleProfileArtifactError, PowerSourceInitialState, ProfileKind, ProfileReference,
    ProfileReferences, SCENARIO_SCHEMA_VERSION_V1, SCENARIO_SCHEMA_VERSION_V2,
    SCENARIO_SCHEMA_VERSION_V3, SCENARIO_SCHEMA_VERSION_V4, ScenarioHashError, ScenarioManifest,
    StageFeatureSet, decode_balance_profile, decode_numeric_profile, decode_package,
    decode_physical_scale_profile, decode_scenario_manifest, encode_physical_scale_profile,
};
pub use capacity::{
    MainCoreCapacityContribution, NetworkAccounting, NetworkAnalyzerSnapshot, WireCapacityUsage,
};
pub use capacity_support::{
    CapacitySupportError, WireCapacitySupportShare, calculate_capacity_support_demand,
    capacity_excess, distribute_capacity_support_demand,
};
pub use command::{
    BindPortCommand, Command, CommandAcceptance, CommandEncodingError, CommandEnvelope,
    CommandRejection, CommandRejectionReason, LogicLevel, PlaceConstructionSiteCommand,
    PlaceFixedSubstrateCommand, PlaceGateCommand, PlaceJunctionCommand,
    PlaceMobileSubstrateCommand, PlaceWireCommand, RemoveEntityCommand, SetExternalDriverCommand,
};
pub use construction::{
    ConstructionError, ConstructionProgressResult, ConstructionSite, ConstructionSiteStore,
    ConstructionTarget, ConstructionWorkContribution, apply_construction_work,
    construction_nominal_demand, construction_nominal_demand_for_work, grant_construction_work,
    required_construction_work,
};
pub use contact::{
    ContactAbsorption, ContactAllocation, ContactCandidate, ContactError, LiveWireInput,
    allocate_contact_energy, calculate_live_wire_demand, swept_circle_intersects_point,
    swept_circle_intersects_wire_body,
};
pub use contract::{
    ContractValidationError, HASH_ALGORITHM_ID_BLAKE3_V1, HashAlgorithmId, SEMANTICS_VERSION_V1,
    SemanticsVersion, SimulationContract,
};
pub use damage_state::DamageState;
pub use enemy::{EnemyState, EnemyStore, EnemyStoreError};
pub use error::{JsonErrorCategory, PackageError, SimulationError};
pub use event::{
    CanonicalEvent, DRIVER_TRANSITION_KIND_ORDER, DriverSample, DriverTransition,
    DriverTransitionCause, EventCalendar, EventCalendarError, EventKey, EventPayloadAllocator,
    FIRST_EVENT_PAYLOAD_ORDER, RESERVED_EVENT_PAYLOAD_ORDER, SIGNAL_ARRIVAL_KIND_ORDER,
    SignalArrival, SignalArrivalKind,
};
pub use experiment::{
    ArtifactHash, ExperimentAxis, ExperimentPlan, ExperimentPlanError, ExperimentRunId,
    ExperimentRunSpec, ExperimentTextField, GateGeometryVariant, LongWireDesign,
    MAX_EXPERIMENT_RUNS, MAX_PHYSICAL_SCALE_PROFILES, PhysicalScaleMatrix, ResolvedExperimentPlan,
    ResolvedPhysicalScaleProfile,
};
pub use experiment_artifact::{
    EXPERIMENT_PLAN_FORMAT_VERSION_V1, EXPERIMENT_STAGE_S1_M0, ExperimentArtifactBytes,
    ExperimentArtifactError, ExperimentArtifactReference, ExperimentPlanArtifact,
    ExperimentProfileReference, ExperimentStage, LONG_WIRE_DESIGN_DERIVATION_VERSION_V1,
    decode_experiment_plan_artifact, encode_experiment_plan_artifact,
    resolve_experiment_plan_artifact,
};
pub use geometry::{
    FixedVec2, GeometryError, cell_coordinate, polyline_length, segment_length, validate_quantized,
};
pub use hash::{HashParseError, ProfileHash, StateHash};
pub use identity::{
    ConnectionGeneration, ConnectionGenerationError, ConstructionSiteId, ConstructionSiteIndex,
    DepositIndex, DriverId, EnemyId, EnemyIndex, EntityLocation, EntityRegistry,
    EntityRegistryError, FIRST_ENTITY_ID, FixedSubstrateIndex, GateId, GateIndex, JunctionId,
    JunctionIndex, MainCoreId, MobileId, MobileSubstrateIndex, PowerSourceId, PowerSourceIndex,
    QuartzIndex, RESERVED_ENTITY_ID, RelaySiteId, RelaySiteIndex, SinkId, WireId, WireIndex,
};
pub use main_core::{MainCoreState, TopologyNodeId};
pub use mobility::{
    Heading, JunctionDecisionKind, MobileControlPorts, MobileControlSample, MobileJunctionDecision,
    MobileMovementObservation, MobilePort, MobilePortRef, TrackPosition,
};
pub use module::{
    AbsoluteModuleGeometry, GateBlueprint, JunctionBlueprint, MODULE_FORMAT_VERSION_V1,
    ModuleBlueprint, ModuleContract, ModuleEndpoint, ModuleError, ModuleFormatVersion,
    ModuleIoBinding, ModuleLocalId, ModuleProvenance, ModuleRoutingDomain, SubstrateBlueprint,
    WireBlueprint, decode_module_artifact, encode_module_artifact, validate_module_against,
};
pub use numeric::{
    Capacity, DriveStrength, Energy, EntityId, FIXED_ONE, Fixed, HeatEnergy, Integrity,
    NumericError, Revision, Tick, ceil_div_nonnegative, ceil_isqrt, floor_div,
    round_div_nearest_even,
};
pub use path_certificate::PathCertificateId;
pub use power::{
    CanonicalPowerRoute, DemandId, DemandKind, POWER_RATIO_SOLVER_COMPARISONS, PowerDemand,
    PowerError, PowerGrant, PowerHeatContribution, PowerLossCoefficient, PowerPathToken,
    PowerRatio, PowerRegionId, PowerRegionSolution, PowerRouteKey, PowerRouteWire,
    PowerSourceState, brownout_gate_delay, distribute_transmission_heat, scale_drive,
    scale_movement, scale_work, select_canonical_source_route, solve_power_region,
    transmission_loss,
};
pub use power_runtime::{
    GatePowerDemandInput, MovementPowerDemandInput, NominalPowerDemand, NominalPowerDemandSet,
    PowerGateReport, PowerHeatKind, PowerHeatReport, PowerLoadReport, PowerMobileReport,
    PowerRegionReport, PowerRuntimeError, PowerSenseAnalyzerSnapshot, PowerSenseReport,
    PowerStepReport, WirePowerDemandInput, bind_nominal_power_demands, build_gate_nominal_demands,
    build_movement_nominal_demand, build_wire_nominal_demands, collect_nominal_power_demands,
    collect_nominal_power_demands_with_capacity_support, power_loss_coefficient, solve_power_step,
    solve_power_step_with_capacity_support_heat,
};
pub use power_source::{PowerSourceStore, PowerSourceStoreError};
pub use power_topology::{
    CompiledPowerLoad, CompiledPowerRegion, CompiledPowerTopology, PowerBodyEdge,
    PowerLoadAttachment, PowerNodeKey, PowerSourceAttachment, PowerTopologyError,
    PowerTopologyInput,
};
pub use profile::{
    BALANCE_SCHEMA_VERSION_V4, BALANCE_SCHEMA_VERSION_V5, BalanceProfile, BinaryGatePortAnchors,
    CapacityProbeProfile, CapacitySupportProbeProfile, ConstructionProbeProfile,
    ContactDamageProbeProfile, DivisionProfile, ElectricalToleranceProfile, GateFootprint,
    GateFootprintTable, GatePortTable, GeometryLengthProfile,
    MAX_STAGE0_WORLD_PITCH_GEOMETRY_QUANTA, NumericProfile, OrientationBoundaryMultipliers,
    OrientationWeightTable, OverflowPolicy, PROFILE_SCHEMA_VERSION_V1, PROFILE_SCHEMA_VERSION_V2,
    PROFILE_SCHEMA_VERSION_V3, PhysicalScaleProfile, PortAnchor, PowerProbeProfile,
    PrimitiveIntegrityProfile, PrimitiveThermalCapacityProfile, ProfileBundle, ProfileHashes,
    ProfileValidationError, REFERENCE_CIRCUIT_ROUTING_PITCH, REFERENCE_GATE_MINIMUM_EXTENT,
    REFERENCE_WIRE_BODY_RADIUS, REFERENCE_WIRE_GEOMETRY_QUANTUM, REFERENCE_WORLD_ROUTING_PITCH,
    RadiationReferenceProfile, Rational, UnaryGatePortAnchors,
};
pub use reference_architecture::{
    MaterializedReferenceArchitecture, MaterializedReferenceArchitecturePair,
    REFERENCE_ARCHITECTURE_FORMAT_VERSION_V1, REFERENCE_ARCHITECTURE_FORMAT_VERSION_V2,
    REFERENCE_ARCHITECTURE_MAX_BARRIER_TICKS_V2, REFERENCE_ARCHITECTURE_MAX_BINDING_BATCHES_V2,
    ReferenceArchitectureArtifact, ReferenceArchitectureBindingEndpoint,
    ReferenceArchitectureBindingStageEvidence, ReferenceArchitectureEndpoint,
    ReferenceArchitectureError, ReferenceArchitectureExecutedBatchEvidence,
    ReferenceArchitectureFormatVersion, ReferenceArchitectureLocalId,
    ReferenceArchitectureMaterializationBatchKind, ReferenceArchitectureMaterializationInput,
    ReferenceArchitectureMaterializationPlan, ReferenceArchitectureMaterializationSchedule,
    ReferenceArchitectureMaterializationStep, ReferenceArchitectureObservationBinding,
    ReferenceArchitectureOperation, ReferenceArchitectureRoleBinding,
    ReferenceArchitectureRoutingDomain, ReferenceArchitectureScenarioResolution,
    ReferenceArchitectureSemanticTarget, ReferenceFixedSubstrate, ReferenceGate, ReferenceJunction,
    ReferenceMobileSubstrate, ReferenceWire, ResolvedReferenceArchitectureSemanticTarget,
    decode_reference_architecture_artifact, encode_reference_architecture_artifact,
    materialize_reference_architecture, materialize_reference_architecture_pair,
    reference_architecture_command_log_hash, resolve_reference_architecture_semantic_target,
    validate_reference_architecture_against,
};
pub use reference_experiment::{
    REFERENCE_EXPERIMENT_FORMAT_VERSION_V2, REFERENCE_EXPERIMENT_STAGE_S1_M5,
    REFERENCE_PAIR_FORMAT_VERSION_V1, ReferenceArchitecturePairManifest, ReferenceArchitectureRole,
    ReferenceArtifactReference, ReferenceDesignBinding, ReferenceExperimentError,
    ReferenceExperimentPlanV2, ReferenceExperimentRunIdentityV2, ReferenceExperimentRunV2,
    ReferencePairFairnessInput, ReferenceProfileReference, ReferenceResponseBinding,
    ReferenceTerritoryAnchor, decode_reference_experiment_plan_v2, decode_reference_pair_manifest,
    encode_reference_experiment_plan_v2, encode_reference_pair_manifest, experiment_run_id_v2,
    reference_empty_shared_command_log_hash, reference_enemy_sequence_hash,
    reference_power_source_sequence_hash, validate_reference_pair_fairness,
};
pub use reference_metrics::{
    REFERENCE_METRIC_ARTIFACT_FORMAT_VERSION_V1, REFERENCE_METRIC_SET_FORMAT_V1,
    REFERENCE_METRIC_SET_ID_V1, REFERENCE_METRICS_V1, ReferenceMetric, ReferenceMetricArtifact,
    ReferenceMetricBoundaries, ReferenceMetricCollector, ReferenceMetricError,
    ReferenceMetricResult, ReferenceMetricResultBoundaries, ReferenceMetricSetArtifact,
    ReferenceMetricTickSample, ReferenceObservationPhase, ReferenceResponseLatency,
    ReferenceResponseObservationSpec, ReferenceRuntimeMetrics, ReferenceStaticInventory,
    ReferenceTerminalStatus, ResolvedReferenceResponseObservation,
    decode_reference_metric_artifact, decode_reference_metric_set_artifact,
    derive_reference_static_inventory, encode_reference_metric_artifact,
    encode_reference_metric_set_artifact, reduce_reference_metrics,
    resolve_reference_response_observations, validate_reference_metric_bindings,
};
pub use replay::{
    HashCheckpoint, REPLAY_FORMAT_VERSION_V1, REPLAY_FORMAT_VERSION_V2, Replay, ReplayArtifact,
    ReplayContractField, ReplayError, ReplayFormatVersion, ReplayHeader, STATE_HASH_VERSION_V3,
    STATE_HASH_VERSION_V4, STATE_HASH_VERSION_V5, STATE_HASH_VERSION_V6, STATE_HASH_VERSION_V7,
    Seed, SeedParseError, StateHashVersion, WORLD_GENERATOR_VERSION_EMPTY_V1,
    WORLD_GENERATOR_VERSION_MAIN_CORE_POWER_ENEMY_V1, WORLD_GENERATOR_VERSION_MAIN_CORE_POWER_V1,
    WORLD_GENERATOR_VERSION_MAIN_CORE_V1, WorldGeneratorVersion, WorldInputEvent,
    decode_replay_artifact, encode_replay_artifact,
};
pub use sensing::{
    HostileCollider, SensingError, SparseOrderedChunkGrid, WireSensingInput, WireSensingOutput,
    circle_intersects_polyline_capsule, sample_wire_sensing, sample_wire_sensing_with_grid,
};
pub use signal::{
    DriveVector, DriverChangeRecord, DriverRole, GateInputSignalPort, GateSignalPorts,
    GateSignalSnapshot, SignalChangeRecord, SignalStepCounters, SinkRole, WireSensePorts,
    WireSenseSnapshot, WireSignalSnapshot,
};
pub use simulation::{
    ArmedWireAnalyzerRecord, ConstructionContactDamageAnalyzerSnapshot, ConstructionWorkReport,
    ContactEnergyReport, DamageAnalyzerRecord, DamageReport, DestructionKind, DestructionReport,
    InteractionHeatReport, RunEndCause, RunStatus, SignalArrivalObservation,
    SignalQuiescenceSnapshot, Simulation, SimulationPackage, StepReport,
};
pub use snapshot::{
    ConstructionSiteRenderRecord, EnemyRenderRecord, FixedSubstrateRenderRecord, GateRenderRecord,
    JunctionRenderRecord, MainCoreRenderRecord, MobileRenderRecord, RenderSnapshot,
    SignalProbeSample, SignalProbeTarget, SignalProbeValue, WireRenderRecord,
};
pub use thermal_damage::{
    DamageKind, DamageResolution, DamageSnapshot, ElectricalExposure, HeatContributionInput,
    HeatContributionKey, InteractionHeatKind, ThermalDamageError, ThermalObjectKind,
    electrical_tolerance_for, integrate_heat, resolve_damage, thermal_capacity_for,
};
pub use topology::{
    EndpointTarget, FixedAabb, GatePort, GatePortRef, GateType, RoutingDomain, WireEnd,
    WireSensePortRef,
};
