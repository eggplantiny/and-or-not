#![forbid(unsafe_code)]

mod artifact;
mod canonical;
mod capacity;
mod command;
mod contract;
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
mod profile;
mod replay;
mod signal;
mod signal_topology;
mod simulation;
mod snapshot;
mod structural;
mod structural_geometry;
mod topology;

pub use artifact::{
    ArtifactBytes, ArtifactKind, InitialWorld, PhysicalScaleProfileArtifactError, ProfileKind,
    ProfileReference, ProfileReferences, SCENARIO_SCHEMA_VERSION_V1, SCENARIO_SCHEMA_VERSION_V2,
    ScenarioHashError, ScenarioManifest, StageFeatureSet, decode_balance_profile,
    decode_numeric_profile, decode_package, decode_physical_scale_profile,
    decode_scenario_manifest, encode_physical_scale_profile,
};
pub use capacity::{
    MainCoreCapacityContribution, NetworkAccounting, NetworkAnalyzerSnapshot, WireCapacityUsage,
};
pub use command::{
    BindPortCommand, Command, CommandAcceptance, CommandEncodingError, CommandEnvelope,
    CommandRejection, CommandRejectionReason, LogicLevel, PlaceFixedSubstrateCommand,
    PlaceGateCommand, PlaceJunctionCommand, PlaceMobileSubstrateCommand, PlaceWireCommand,
    RemoveEntityCommand, SetExternalDriverCommand,
};
pub use contract::{
    ContractValidationError, HASH_ALGORITHM_ID_BLAKE3_V1, HashAlgorithmId, SEMANTICS_VERSION_V1,
    SemanticsVersion, SimulationContract,
};
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
    ConnectionGeneration, ConnectionGenerationError, ConstructionSiteIndex, DepositIndex, DriverId,
    EnemyIndex, EntityLocation, EntityRegistry, EntityRegistryError, FIRST_ENTITY_ID,
    FixedSubstrateIndex, GateId, GateIndex, JunctionId, JunctionIndex, MainCoreId, MobileId,
    MobileSubstrateIndex, PowerSourceIndex, QuartzIndex, RESERVED_ENTITY_ID, RelaySiteId,
    RelaySiteIndex, SinkId, WireId, WireIndex,
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
pub use profile::{
    BalanceProfile, BinaryGatePortAnchors, CapacityProbeProfile, DivisionProfile, GateFootprint,
    GateFootprintTable, GatePortTable, GeometryLengthProfile,
    MAX_STAGE0_WORLD_PITCH_GEOMETRY_QUANTA, NumericProfile, OrientationBoundaryMultipliers,
    OrientationWeightTable, OverflowPolicy, PROFILE_SCHEMA_VERSION_V1, PROFILE_SCHEMA_VERSION_V2,
    PhysicalScaleProfile, PortAnchor, ProfileBundle, ProfileHashes, ProfileValidationError,
    REFERENCE_CIRCUIT_ROUTING_PITCH, REFERENCE_GATE_MINIMUM_EXTENT, REFERENCE_WIRE_BODY_RADIUS,
    REFERENCE_WIRE_GEOMETRY_QUANTUM, REFERENCE_WORLD_ROUTING_PITCH, RadiationReferenceProfile,
    Rational, UnaryGatePortAnchors,
};
pub use replay::{
    HashCheckpoint, REPLAY_FORMAT_VERSION_V1, Replay, ReplayArtifact, ReplayContractField,
    ReplayError, ReplayFormatVersion, ReplayHeader, STATE_HASH_VERSION_V3, STATE_HASH_VERSION_V4,
    STATE_HASH_VERSION_V5, Seed, SeedParseError, StateHashVersion,
    WORLD_GENERATOR_VERSION_EMPTY_V1, WORLD_GENERATOR_VERSION_MAIN_CORE_V1, WorldGeneratorVersion,
    WorldInputEvent, decode_replay_artifact, encode_replay_artifact,
};
pub use signal::{
    DriveVector, DriverChangeRecord, DriverRole, GateInputSignalPort, GateSignalPorts,
    GateSignalSnapshot, SignalChangeRecord, SignalStepCounters, SinkRole, WireSignalSnapshot,
};
pub use simulation::{SignalArrivalObservation, Simulation, SimulationPackage, StepReport};
pub use snapshot::{
    FixedSubstrateRenderRecord, GateRenderRecord, JunctionRenderRecord, MainCoreRenderRecord,
    MobileRenderRecord, RenderSnapshot, SignalProbeSample, SignalProbeTarget, SignalProbeValue,
    WireRenderRecord,
};
pub use topology::{
    EndpointTarget, FixedAabb, GatePort, GatePortRef, GateType, RoutingDomain, WireEnd,
};
