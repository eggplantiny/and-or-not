#![forbid(unsafe_code)]

mod artifact;
mod canonical;
mod contract;
mod error;
mod geometry;
mod hash;
mod identity;
mod numeric;
mod profile;
mod simulation;
mod snapshot;

pub use artifact::{
    ArtifactBytes, ArtifactKind, InitialWorld, ProfileKind, ProfileReference, ProfileReferences,
    ScenarioManifest, StageFeatureSet, decode_package, decode_scenario_manifest,
};
pub use contract::{
    ContractValidationError, HASH_ALGORITHM_ID_BLAKE3_V1, HashAlgorithmId, SEMANTICS_VERSION_V1,
    SemanticsVersion, SimulationContract,
};
pub use error::{JsonErrorCategory, PackageError, SimulationError};
pub use geometry::{
    FixedVec2, GeometryError, cell_coordinate, polyline_length, segment_length, validate_quantized,
};
pub use hash::{HashParseError, ProfileHash, StateHash};
pub use identity::{
    ConnectionGeneration, ConnectionGenerationError, ConstructionSiteIndex, DepositIndex, DriverId,
    EnemyIndex, EntityLocation, EntityRegistry, EntityRegistryError, FIRST_ENTITY_ID,
    FixedSubstrateIndex, GateId, GateIndex, JunctionId, JunctionIndex, MobileId,
    MobileSubstrateIndex, PowerSourceIndex, QuartzIndex, RESERVED_ENTITY_ID, RelaySiteId,
    RelaySiteIndex, SinkId, WireId, WireIndex,
};
pub use numeric::{
    Capacity, DriveStrength, Energy, EntityId, FIXED_ONE, Fixed, HeatEnergy, Integrity,
    NumericError, Revision, Tick, ceil_div_nonnegative, ceil_isqrt, floor_div,
    round_div_nearest_even,
};
pub use profile::{
    BalanceProfile, BinaryGatePortAnchors, CapacityProbeProfile, DivisionProfile, GateFootprint,
    GateFootprintTable, GatePortTable, GeometryLengthProfile, NumericProfile,
    OrientationBoundaryMultipliers, OrientationWeightTable, OverflowPolicy,
    PROFILE_SCHEMA_VERSION_V1, PhysicalScaleProfile, PortAnchor, ProfileBundle, ProfileHashes,
    ProfileValidationError, REFERENCE_CIRCUIT_ROUTING_PITCH, REFERENCE_GATE_MINIMUM_EXTENT,
    REFERENCE_WIRE_BODY_RADIUS, REFERENCE_WIRE_GEOMETRY_QUANTUM, REFERENCE_WORLD_ROUTING_PITCH,
    RadiationReferenceProfile, Rational, UnaryGatePortAnchors,
};
pub use simulation::{CommandEnvelope, Simulation, SimulationPackage, StepReport};
pub use snapshot::RenderSnapshot;
