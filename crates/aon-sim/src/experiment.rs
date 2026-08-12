use crate::{
    BinaryGatePortAnchors, Fixed, FixedVec2, GateFootprintTable, GatePortTable, HashParseError,
    PhysicalScaleProfile, PortAnchor, ProfileHash, ProfileValidationError, Seed, SemanticsVersion,
    SimulationContract, UnaryGatePortAnchors,
};
use std::fmt;
use thiserror::Error;

pub const MAX_PHYSICAL_SCALE_PROFILES: usize = 4_096;
pub const MAX_EXPERIMENT_RUNS: usize = 65_536;

const LONG_WIRE_ARTIFACT_HASH_DOMAIN: &[u8] = b"AON\0LONG-WIRE-DESIGN\0V1\0";
const EXPERIMENT_RUN_ID_DOMAIN: &[u8] = b"AON\0EXPERIMENT-RUN\0V1\0";
const CANONICAL_ENCODER_VERSION: u16 = 1;

macro_rules! canonical_hash_type {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name([u8; 32]);

        impl $name {
            pub const fn from_bytes(bytes: [u8; 32]) -> Self {
                Self(bytes)
            }

            pub const fn as_bytes(&self) -> &[u8; 32] {
                &self.0
            }

            pub fn from_hex(value: &str) -> Result<Self, HashParseError> {
                let parsed = ProfileHash::from_hex(value)?;
                Ok(Self(*parsed.as_bytes()))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                for byte in self.0 {
                    write!(formatter, "{byte:02x}")?;
                }
                Ok(())
            }
        }
    };
}

canonical_hash_type!(ArtifactHash);
canonical_hash_type!(ExperimentRunId);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GateGeometryVariant {
    pub gate_footprints: GateFootprintTable,
    pub gate_port_anchors: GatePortTable,
}

impl GateGeometryVariant {
    pub const fn from_profile(profile: &PhysicalScaleProfile) -> Self {
        Self {
            gate_footprints: profile.gate_footprints,
            gate_port_anchors: profile.gate_port_anchors,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhysicalScaleMatrix {
    pub base_profile: PhysicalScaleProfile,
    pub gate_geometries: Vec<GateGeometryVariant>,
    pub circuit_routing_pitches: Vec<Fixed>,
    pub world_routing_pitches: Vec<Fixed>,
}

impl PhysicalScaleMatrix {
    pub fn resolve(&self) -> Result<Vec<ResolvedPhysicalScaleProfile>, ExperimentPlanError> {
        self.validate_required_axes()?;
        let axes = self.validate_axes()?;
        let profile_count = checked_product(&[
            axes.gate_geometries.len(),
            axes.circuit_routing_pitches.len(),
            axes.world_routing_pitches.len(),
        ])?;
        if profile_count > MAX_PHYSICAL_SCALE_PROFILES {
            return Err(ExperimentPlanError::TooManyPhysicalScaleProfiles {
                maximum: MAX_PHYSICAL_SCALE_PROFILES,
                actual: profile_count,
            });
        }

        self.generate_profiles(&axes, profile_count)
    }

    fn validate_required_axes(&self) -> Result<(), ExperimentPlanError> {
        require_nonempty(ExperimentAxis::GateGeometry, self.gate_geometries.len())?;
        require_nonempty(
            ExperimentAxis::CircuitRoutingPitch,
            self.circuit_routing_pitches.len(),
        )?;
        require_nonempty(
            ExperimentAxis::WorldRoutingPitch,
            self.world_routing_pitches.len(),
        )?;
        Ok(())
    }

    fn validate_axes(&self) -> Result<ValidatedPhysicalScaleAxes, ExperimentPlanError> {
        self.base_profile.validate()?;

        for geometry in &self.gate_geometries {
            let mut candidate = self.base_profile.clone();
            candidate.gate_footprints = geometry.gate_footprints;
            candidate.gate_port_anchors = geometry.gate_port_anchors;
            candidate.validate()?;
        }
        for circuit_routing_pitch in &self.circuit_routing_pitches {
            let mut candidate = self.base_profile.clone();
            candidate.circuit_routing_pitch = *circuit_routing_pitch;
            candidate.validate()?;
        }
        for world_routing_pitch in &self.world_routing_pitches {
            let mut candidate = self.base_profile.clone();
            candidate.world_routing_pitch = *world_routing_pitch;
            candidate.validate()?;
        }

        let mut gate_geometries = self.gate_geometries.clone();
        gate_geometries.sort_by_key(|geometry| gate_geometry_key(*geometry));
        let mut circuit_routing_pitches = self.circuit_routing_pitches.clone();
        circuit_routing_pitches.sort_unstable();
        let mut world_routing_pitches = self.world_routing_pitches.clone();
        world_routing_pitches.sort_unstable();

        if let Some(duplicate) = gate_geometries
            .windows(2)
            .find(|geometries| geometries[0] == geometries[1])
        {
            return Err(ExperimentPlanError::DuplicatePhysicalScaleProfile {
                profile_hash: self.profile_hash_for_axes(
                    duplicate[0],
                    circuit_routing_pitches[0],
                    world_routing_pitches[0],
                )?,
            });
        }
        if let Some(duplicate) = circuit_routing_pitches
            .windows(2)
            .find(|pitches| pitches[0] == pitches[1])
        {
            return Err(ExperimentPlanError::DuplicatePhysicalScaleProfile {
                profile_hash: self.profile_hash_for_axes(
                    gate_geometries[0],
                    duplicate[0],
                    world_routing_pitches[0],
                )?,
            });
        }
        if let Some(duplicate) = world_routing_pitches
            .windows(2)
            .find(|pitches| pitches[0] == pitches[1])
        {
            return Err(ExperimentPlanError::DuplicatePhysicalScaleProfile {
                profile_hash: self.profile_hash_for_axes(
                    gate_geometries[0],
                    circuit_routing_pitches[0],
                    duplicate[0],
                )?,
            });
        }

        Ok(ValidatedPhysicalScaleAxes {
            gate_geometries,
            circuit_routing_pitches,
            world_routing_pitches,
        })
    }

    fn generate_profiles(
        &self,
        axes: &ValidatedPhysicalScaleAxes,
        profile_count: usize,
    ) -> Result<Vec<ResolvedPhysicalScaleProfile>, ExperimentPlanError> {
        debug_assert!(profile_count <= MAX_PHYSICAL_SCALE_PROFILES);

        let mut resolved = Vec::with_capacity(profile_count);
        for geometry in &axes.gate_geometries {
            for circuit_routing_pitch in &axes.circuit_routing_pitches {
                for world_routing_pitch in &axes.world_routing_pitches {
                    let mut profile = self.base_profile.clone();
                    profile.gate_footprints = geometry.gate_footprints;
                    profile.gate_port_anchors = geometry.gate_port_anchors;
                    profile.circuit_routing_pitch = *circuit_routing_pitch;
                    profile.world_routing_pitch = *world_routing_pitch;
                    let profile_hash = profile.canonical_hash()?;
                    profile.profile_id = format!("s1m0-physical-{profile_hash}");
                    resolved.push(ResolvedPhysicalScaleProfile {
                        profile,
                        profile_hash,
                    });
                }
            }
        }

        resolved.sort_unstable_by_key(|profile| profile.profile_hash);
        if let Some(duplicate) = resolved
            .windows(2)
            .find(|profiles| profiles[0].profile_hash == profiles[1].profile_hash)
        {
            return Err(ExperimentPlanError::DuplicatePhysicalScaleProfile {
                profile_hash: duplicate[0].profile_hash,
            });
        }
        Ok(resolved)
    }

    fn profile_hash_for_axes(
        &self,
        geometry: GateGeometryVariant,
        circuit_routing_pitch: Fixed,
        world_routing_pitch: Fixed,
    ) -> Result<ProfileHash, ExperimentPlanError> {
        let mut profile = self.base_profile.clone();
        profile.gate_footprints = geometry.gate_footprints;
        profile.gate_port_anchors = geometry.gate_port_anchors;
        profile.circuit_routing_pitch = circuit_routing_pitch;
        profile.world_routing_pitch = world_routing_pitch;
        Ok(profile.canonical_hash()?)
    }
}

struct ValidatedPhysicalScaleAxes {
    gate_geometries: Vec<GateGeometryVariant>,
    circuit_routing_pitches: Vec<Fixed>,
    world_routing_pitches: Vec<Fixed>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedPhysicalScaleProfile {
    profile: PhysicalScaleProfile,
    profile_hash: ProfileHash,
}

impl ResolvedPhysicalScaleProfile {
    pub const fn profile(&self) -> &PhysicalScaleProfile {
        &self.profile
    }

    pub const fn profile_hash(&self) -> ProfileHash {
        self.profile_hash
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExperimentPlan {
    pub experiment_id: String,
    pub scenario_artifact_hash: ArtifactHash,
    pub physical_scale_matrix: PhysicalScaleMatrix,
    pub long_wire_distances: Vec<Fixed>,
    pub numeric_profile_hashes: Vec<ProfileHash>,
    pub balance_profile_hashes: Vec<ProfileHash>,
    pub seeds: Vec<Seed>,
    pub max_ticks: u64,
    pub metric_set_id: String,
}

impl ExperimentPlan {
    pub fn resolve(&self) -> Result<ResolvedExperimentPlan, ExperimentPlanError> {
        self.resolve_with_run_limit(MAX_EXPERIMENT_RUNS)
    }

    pub fn resolve_with_run_limit(
        &self,
        maximum_runs: usize,
    ) -> Result<ResolvedExperimentPlan, ExperimentPlanError> {
        let maximum_runs = maximum_runs.min(MAX_EXPERIMENT_RUNS);
        validate_nonempty_text(ExperimentTextField::ExperimentId, &self.experiment_id)?;
        validate_nonempty_text(ExperimentTextField::MetricSetId, &self.metric_set_id)?;
        require_nonempty(
            ExperimentAxis::NumericProfile,
            self.numeric_profile_hashes.len(),
        )?;
        self.physical_scale_matrix.validate_required_axes()?;
        require_nonempty(
            ExperimentAxis::BalanceProfile,
            self.balance_profile_hashes.len(),
        )?;
        require_nonempty(
            ExperimentAxis::LongWireDistance,
            self.long_wire_distances.len(),
        )?;
        require_nonempty(ExperimentAxis::Seed, self.seeds.len())?;

        validate_text_length(ExperimentTextField::ExperimentId, &self.experiment_id)?;
        validate_text_length(ExperimentTextField::MetricSetId, &self.metric_set_id)?;
        if self.max_ticks == 0 {
            return Err(ExperimentPlanError::NonPositiveMaxTicks);
        }

        let physical_axes = self.physical_scale_matrix.validate_axes()?;

        let mut numeric_profile_hashes = self.numeric_profile_hashes.clone();
        numeric_profile_hashes.sort_unstable();
        reject_duplicate_profile_hashes(ExperimentAxis::NumericProfile, &numeric_profile_hashes)?;
        let mut balance_profile_hashes = self.balance_profile_hashes.clone();
        balance_profile_hashes.sort_unstable();
        reject_duplicate_profile_hashes(ExperimentAxis::BalanceProfile, &balance_profile_hashes)?;

        let mut long_wire_distances = self.long_wire_distances.clone();
        long_wire_distances.sort_unstable();
        if let Some(distance) = long_wire_distances.iter().find(|distance| distance.0 <= 0) {
            return Err(ExperimentPlanError::NonPositiveLongWireDistance {
                distance: *distance,
            });
        }
        if let Some(duplicate) = long_wire_distances
            .windows(2)
            .find(|distances| distances[0] == distances[1])
        {
            return Err(ExperimentPlanError::DuplicateLongWireDistance {
                distance: duplicate[0],
            });
        }

        let mut seeds = self.seeds.clone();
        seeds.sort_unstable();
        if let Some(duplicate) = seeds.windows(2).find(|seeds| seeds[0] == seeds[1]) {
            return Err(ExperimentPlanError::DuplicateSeed { seed: duplicate[0] });
        }

        self.validate_long_wire_alignment(&physical_axes, &long_wire_distances)?;

        let profile_count = checked_product(&[
            physical_axes.gate_geometries.len(),
            physical_axes.circuit_routing_pitches.len(),
            physical_axes.world_routing_pitches.len(),
        ])?;
        let run_count = checked_product(&[
            numeric_profile_hashes.len(),
            profile_count,
            balance_profile_hashes.len(),
            long_wire_distances.len(),
            seeds.len(),
        ])?;
        if profile_count > MAX_PHYSICAL_SCALE_PROFILES {
            return Err(ExperimentPlanError::TooManyPhysicalScaleProfiles {
                maximum: MAX_PHYSICAL_SCALE_PROFILES,
                actual: profile_count,
            });
        }
        if run_count > maximum_runs {
            return Err(ExperimentPlanError::TooManyExperimentRuns {
                maximum: maximum_runs,
                actual: run_count,
            });
        }

        let physical_scale_profiles = self
            .physical_scale_matrix
            .generate_profiles(&physical_axes, profile_count)?;
        let mut runs = Vec::with_capacity(run_count);
        for numeric_profile_hash in numeric_profile_hashes {
            for resolved_profile in &physical_scale_profiles {
                let profile = resolved_profile.profile();
                let physical_scale_profile_hash = resolved_profile.profile_hash();
                for balance_profile_hash in &balance_profile_hashes {
                    for distance in &long_wire_distances {
                        let design = LongWireDesign::try_from_distance(*distance)?;
                        let design_artifact_hash = design.canonical_hash();
                        let contract = SimulationContract {
                            semantics_version: SemanticsVersion::AonV1,
                            numeric_profile_hash,
                            physical_scale_profile_hash,
                            balance_profile_hash: *balance_profile_hash,
                        };
                        for seed in &seeds {
                            let run_id = experiment_run_id(ExperimentRunIdentity {
                                experiment_id: &self.experiment_id,
                                scenario_artifact_hash: self.scenario_artifact_hash,
                                design_artifact_hash,
                                semantics_version: contract.semantics_version.as_str(),
                                numeric_profile_hash: contract.numeric_profile_hash,
                                physical_scale_profile_hash: contract.physical_scale_profile_hash,
                                balance_profile_hash: contract.balance_profile_hash,
                                long_wire_distance: *distance,
                                seed: *seed,
                                max_ticks: self.max_ticks,
                                metric_set_id: &self.metric_set_id,
                            });
                            runs.push(ExperimentRunSpec {
                                experiment_id: self.experiment_id.clone(),
                                scenario_artifact_hash: self.scenario_artifact_hash,
                                contract,
                                physical_scale_profile: profile.clone(),
                                design,
                                design_artifact_hash,
                                seed: *seed,
                                max_ticks: self.max_ticks,
                                metric_set_id: self.metric_set_id.clone(),
                                run_id,
                            });
                        }
                    }
                }
            }
        }

        require_unique_run_ids(runs.iter().map(|run| run.run_id))?;

        Ok(ResolvedExperimentPlan {
            physical_scale_profiles,
            runs,
        })
    }

    fn validate_long_wire_alignment(
        &self,
        axes: &ValidatedPhysicalScaleAxes,
        distances: &[Fixed],
    ) -> Result<(), ExperimentPlanError> {
        let geometry = axes.gate_geometries[0];
        let circuit_routing_pitch = axes.circuit_routing_pitches[0];
        for world_routing_pitch in &axes.world_routing_pitches {
            for distance in distances {
                if distance.0.rem_euclid(world_routing_pitch.0) != 0 {
                    return Err(ExperimentPlanError::LongWireDistanceNotWorldPitchAligned {
                        distance: *distance,
                        world_routing_pitch: *world_routing_pitch,
                        physical_scale_profile_hash: self
                            .physical_scale_matrix
                            .profile_hash_for_axes(
                                geometry,
                                circuit_routing_pitch,
                                *world_routing_pitch,
                            )?,
                    });
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedExperimentPlan {
    physical_scale_profiles: Vec<ResolvedPhysicalScaleProfile>,
    runs: Vec<ExperimentRunSpec>,
}

impl ResolvedExperimentPlan {
    pub fn physical_scale_profiles(&self) -> &[ResolvedPhysicalScaleProfile] {
        &self.physical_scale_profiles
    }

    pub fn runs(&self) -> &[ExperimentRunSpec] {
        &self.runs
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LongWireDesign {
    start: FixedVec2,
    end: FixedVec2,
}

impl LongWireDesign {
    pub fn try_from_distance(distance: Fixed) -> Result<Self, ExperimentPlanError> {
        if distance.0 <= 0 {
            return Err(ExperimentPlanError::NonPositiveLongWireDistance { distance });
        }
        Ok(Self {
            start: FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
            end: FixedVec2::new(distance, Fixed::ZERO),
        })
    }

    pub const fn start(self) -> FixedVec2 {
        self.start
    }

    pub const fn end(self) -> FixedVec2 {
        self.end
    }

    pub const fn distance(self) -> Fixed {
        self.end.x
    }

    pub fn canonical_hash(self) -> ArtifactHash {
        let mut hasher = blake3::Hasher::new();
        hasher.update(LONG_WIRE_ARTIFACT_HASH_DOMAIN);
        hasher.update(&CANONICAL_ENCODER_VERSION.to_le_bytes());
        for coordinate in [self.start.x, self.start.y, self.end.x, self.end.y] {
            hasher.update(&coordinate.0.to_le_bytes());
        }
        ArtifactHash::from_bytes(*hasher.finalize().as_bytes())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExperimentRunSpec {
    experiment_id: String,
    scenario_artifact_hash: ArtifactHash,
    contract: SimulationContract,
    physical_scale_profile: PhysicalScaleProfile,
    design: LongWireDesign,
    design_artifact_hash: ArtifactHash,
    seed: Seed,
    max_ticks: u64,
    metric_set_id: String,
    run_id: ExperimentRunId,
}

impl ExperimentRunSpec {
    pub fn experiment_id(&self) -> &str {
        &self.experiment_id
    }

    pub const fn scenario_artifact_hash(&self) -> ArtifactHash {
        self.scenario_artifact_hash
    }

    pub const fn contract(&self) -> SimulationContract {
        self.contract
    }

    pub const fn physical_scale_profile(&self) -> &PhysicalScaleProfile {
        &self.physical_scale_profile
    }

    pub const fn design(&self) -> LongWireDesign {
        self.design
    }

    pub const fn design_artifact_hash(&self) -> ArtifactHash {
        self.design_artifact_hash
    }

    pub const fn seed(&self) -> Seed {
        self.seed
    }

    pub const fn long_wire_distance(&self) -> Fixed {
        self.design.distance()
    }

    pub const fn max_ticks(&self) -> u64 {
        self.max_ticks
    }

    pub fn metric_set_id(&self) -> &str {
        &self.metric_set_id
    }

    pub const fn run_id(&self) -> ExperimentRunId {
        self.run_id
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExperimentAxis {
    NumericProfile,
    GateGeometry,
    CircuitRoutingPitch,
    WorldRoutingPitch,
    BalanceProfile,
    LongWireDistance,
    Seed,
}

impl fmt::Display for ExperimentAxis {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::NumericProfile => "numericProfile",
            Self::GateGeometry => "gateGeometry",
            Self::CircuitRoutingPitch => "circuitRoutingPitch",
            Self::WorldRoutingPitch => "worldRoutingPitch",
            Self::BalanceProfile => "balanceProfile",
            Self::LongWireDistance => "longWireDistance",
            Self::Seed => "seed",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExperimentTextField {
    ExperimentId,
    MetricSetId,
}

impl fmt::Display for ExperimentTextField {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ExperimentId => "experimentId",
            Self::MetricSetId => "metricSetId",
        })
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ExperimentPlanError {
    #[error(transparent)]
    Profile(#[from] ProfileValidationError),

    #[error("experiment axis `{axis}` must not be empty")]
    EmptyAxis { axis: ExperimentAxis },

    #[error("experiment field `{field}` must not be empty")]
    EmptyTextField { field: ExperimentTextField },

    #[error("experiment field `{field}` exceeds the canonical u32 byte-length limit")]
    TextFieldTooLong { field: ExperimentTextField },

    #[error("experiment maxTicks must be positive")]
    NonPositiveMaxTicks,

    #[error("experiment matrix cardinality overflow")]
    CardinalityOverflow,

    #[error("physical-scale matrix has {actual} profiles; maximum is {maximum}")]
    TooManyPhysicalScaleProfiles { maximum: usize, actual: usize },

    #[error("experiment plan has {actual} runs; maximum is {maximum}")]
    TooManyExperimentRuns { maximum: usize, actual: usize },

    #[error("physical-scale matrix contains duplicate semantic profile {profile_hash}")]
    DuplicatePhysicalScaleProfile { profile_hash: ProfileHash },

    #[error("experiment axis `{axis}` contains duplicate profile hash {profile_hash}")]
    DuplicateProfileHash {
        axis: ExperimentAxis,
        profile_hash: ProfileHash,
    },

    #[error("long-wire distance must be positive, got raw fixed value {distance:?}")]
    NonPositiveLongWireDistance { distance: Fixed },

    #[error("long-wire distance axis contains duplicate raw fixed value {distance:?}")]
    DuplicateLongWireDistance { distance: Fixed },

    #[error("experiment seed axis contains duplicate Seed {seed}")]
    DuplicateSeed { seed: Seed },

    #[error(
        "long-wire distance {distance:?} is not aligned to world routing pitch {world_routing_pitch:?} for physical-scale profile {physical_scale_profile_hash}"
    )]
    LongWireDistanceNotWorldPitchAligned {
        distance: Fixed,
        world_routing_pitch: Fixed,
        physical_scale_profile_hash: ProfileHash,
    },

    #[error("experiment matrix contains duplicate Run ID {run_id}")]
    DuplicateExperimentRun { run_id: ExperimentRunId },
}

fn require_nonempty(axis: ExperimentAxis, length: usize) -> Result<(), ExperimentPlanError> {
    if length == 0 {
        Err(ExperimentPlanError::EmptyAxis { axis })
    } else {
        Ok(())
    }
}

fn validate_nonempty_text(
    field: ExperimentTextField,
    value: &str,
) -> Result<(), ExperimentPlanError> {
    if value.trim().is_empty() {
        return Err(ExperimentPlanError::EmptyTextField { field });
    }
    Ok(())
}

fn validate_text_length(
    field: ExperimentTextField,
    value: &str,
) -> Result<(), ExperimentPlanError> {
    validate_text_length_bytes(field, value.len())
}

fn validate_text_length_bytes(
    field: ExperimentTextField,
    length: usize,
) -> Result<(), ExperimentPlanError> {
    u32::try_from(length)
        .map(|_| ())
        .map_err(|_| ExperimentPlanError::TextFieldTooLong { field })
}

fn reject_duplicate_profile_hashes(
    axis: ExperimentAxis,
    hashes: &[ProfileHash],
) -> Result<(), ExperimentPlanError> {
    if let Some(duplicate) = hashes.windows(2).find(|hashes| hashes[0] == hashes[1]) {
        Err(ExperimentPlanError::DuplicateProfileHash {
            axis,
            profile_hash: duplicate[0],
        })
    } else {
        Ok(())
    }
}

fn checked_product(factors: &[usize]) -> Result<usize, ExperimentPlanError> {
    let product = factors.iter().try_fold(1_usize, |product, factor| {
        product
            .checked_mul(*factor)
            .ok_or(ExperimentPlanError::CardinalityOverflow)
    })?;
    if product > u32::MAX as usize {
        Err(ExperimentPlanError::CardinalityOverflow)
    } else {
        Ok(product)
    }
}

fn require_unique_run_ids(
    run_ids: impl IntoIterator<Item = ExperimentRunId>,
) -> Result<(), ExperimentPlanError> {
    let mut unique = std::collections::BTreeSet::new();
    for run_id in run_ids {
        if !unique.insert(run_id) {
            return Err(ExperimentPlanError::DuplicateExperimentRun { run_id });
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct ExperimentRunIdentity<'a> {
    experiment_id: &'a str,
    scenario_artifact_hash: ArtifactHash,
    design_artifact_hash: ArtifactHash,
    semantics_version: &'a str,
    numeric_profile_hash: ProfileHash,
    physical_scale_profile_hash: ProfileHash,
    balance_profile_hash: ProfileHash,
    long_wire_distance: Fixed,
    seed: Seed,
    max_ticks: u64,
    metric_set_id: &'a str,
}

fn experiment_run_id(identity: ExperimentRunIdentity<'_>) -> ExperimentRunId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(EXPERIMENT_RUN_ID_DOMAIN);
    hasher.update(&CANONICAL_ENCODER_VERSION.to_le_bytes());
    update_canonical_text(&mut hasher, identity.experiment_id);
    hasher.update(identity.scenario_artifact_hash.as_bytes());
    hasher.update(identity.design_artifact_hash.as_bytes());
    let semantics = identity.semantics_version.as_bytes();
    hasher.update(&(semantics.len() as u32).to_le_bytes());
    hasher.update(semantics);
    hasher.update(identity.numeric_profile_hash.as_bytes());
    hasher.update(identity.physical_scale_profile_hash.as_bytes());
    hasher.update(identity.balance_profile_hash.as_bytes());
    hasher.update(&identity.long_wire_distance.0.to_le_bytes());
    hasher.update(identity.seed.as_bytes());
    hasher.update(&identity.max_ticks.to_le_bytes());
    update_canonical_text(&mut hasher, identity.metric_set_id);
    ExperimentRunId::from_bytes(*hasher.finalize().as_bytes())
}

fn update_canonical_text(hasher: &mut blake3::Hasher, value: &str) {
    let length = u32::try_from(value.len()).expect("experiment text length was validated");
    hasher.update(&length.to_le_bytes());
    hasher.update(value.as_bytes());
}

pub(crate) fn gate_geometry_key(geometry: GateGeometryVariant) -> [i64; 28] {
    let footprints = geometry.gate_footprints;
    let anchors = geometry.gate_port_anchors;
    let and_gate = binary_anchor_key(anchors.and_gate);
    let or_gate = binary_anchor_key(anchors.or_gate);
    let not_gate = unary_anchor_key(anchors.not_gate);
    [
        footprints.and_gate.width.0,
        footprints.and_gate.height.0,
        footprints.or_gate.width.0,
        footprints.or_gate.height.0,
        footprints.not_gate.width.0,
        footprints.not_gate.height.0,
        and_gate[0],
        and_gate[1],
        and_gate[2],
        and_gate[3],
        and_gate[4],
        and_gate[5],
        and_gate[6],
        and_gate[7],
        or_gate[0],
        or_gate[1],
        or_gate[2],
        or_gate[3],
        or_gate[4],
        or_gate[5],
        or_gate[6],
        or_gate[7],
        not_gate[0],
        not_gate[1],
        not_gate[2],
        not_gate[3],
        not_gate[4],
        not_gate[5],
    ]
}

fn binary_anchor_key(anchors: BinaryGatePortAnchors) -> [i64; 8] {
    let input_a = anchor_key(anchors.input_a);
    let input_b = anchor_key(anchors.input_b);
    let output = anchor_key(anchors.output);
    let power = anchor_key(anchors.power);
    [
        input_a[0], input_a[1], input_b[0], input_b[1], output[0], output[1], power[0], power[1],
    ]
}

fn unary_anchor_key(anchors: UnaryGatePortAnchors) -> [i64; 6] {
    let input = anchor_key(anchors.input);
    let output = anchor_key(anchors.output);
    let power = anchor_key(anchors.power);
    [input[0], input[1], output[0], output[1], power[0], power[1]]
}

const fn anchor_key(anchor: PortAnchor) -> [i64; 2] {
    [anchor.x.0, anchor.y.0]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_identity() -> ExperimentRunIdentity<'static> {
        ExperimentRunIdentity {
            experiment_id: "experiment-v1",
            scenario_artifact_hash: ArtifactHash::from_bytes([0x11; 32]),
            design_artifact_hash: ArtifactHash::from_bytes([0x22; 32]),
            semantics_version: "aon-semantics-v1",
            numeric_profile_hash: ProfileHash::from_bytes([0x33; 32]),
            physical_scale_profile_hash: ProfileHash::from_bytes([0x44; 32]),
            balance_profile_hash: ProfileHash::from_bytes([0x55; 32]),
            long_wire_distance: Fixed(65_536),
            seed: Seed::from_hex(&"66".repeat(32)).expect("test Seed is canonical"),
            max_ticks: 4_096,
            metric_set_id: "metrics-v1",
        }
    }

    fn independently_encode_run_id(identity: ExperimentRunIdentity<'_>) -> ExperimentRunId {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"AON\0EXPERIMENT-RUN\0V1\0");
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&(identity.experiment_id.len() as u32).to_le_bytes());
        bytes.extend_from_slice(identity.experiment_id.as_bytes());
        bytes.extend_from_slice(identity.scenario_artifact_hash.as_bytes());
        bytes.extend_from_slice(identity.design_artifact_hash.as_bytes());
        bytes.extend_from_slice(&(identity.semantics_version.len() as u32).to_le_bytes());
        bytes.extend_from_slice(identity.semantics_version.as_bytes());
        bytes.extend_from_slice(identity.numeric_profile_hash.as_bytes());
        bytes.extend_from_slice(identity.physical_scale_profile_hash.as_bytes());
        bytes.extend_from_slice(identity.balance_profile_hash.as_bytes());
        bytes.extend_from_slice(&identity.long_wire_distance.0.to_le_bytes());
        bytes.extend_from_slice(identity.seed.as_bytes());
        bytes.extend_from_slice(&identity.max_ticks.to_le_bytes());
        bytes.extend_from_slice(&(identity.metric_set_id.len() as u32).to_le_bytes());
        bytes.extend_from_slice(identity.metric_set_id.as_bytes());
        ExperimentRunId::from_bytes(*blake3::hash(&bytes).as_bytes())
    }

    #[test]
    fn run_id_encoder_binds_every_field_independently() {
        let baseline = run_identity();
        let baseline_id = experiment_run_id(baseline);
        assert_eq!(baseline_id, independently_encode_run_id(baseline));

        let mut experiment = baseline;
        experiment.experiment_id = "experiment-v2";
        let mut scenario = baseline;
        scenario.scenario_artifact_hash = ArtifactHash::from_bytes([0x12; 32]);
        let mut design = baseline;
        design.design_artifact_hash = ArtifactHash::from_bytes([0x23; 32]);
        let mut semantics = baseline;
        semantics.semantics_version = "aon-semantics-v2-probe";
        let mut numeric = baseline;
        numeric.numeric_profile_hash = ProfileHash::from_bytes([0x34; 32]);
        let mut physical = baseline;
        physical.physical_scale_profile_hash = ProfileHash::from_bytes([0x45; 32]);
        let mut balance = baseline;
        balance.balance_profile_hash = ProfileHash::from_bytes([0x56; 32]);
        let mut distance = baseline;
        distance.long_wire_distance = Fixed(131_072);
        let mut seed = baseline;
        seed.seed = Seed::from_hex(&"67".repeat(32)).expect("test Seed is canonical");
        let mut max_ticks = baseline;
        max_ticks.max_ticks = 4_097;
        let mut metric = baseline;
        metric.metric_set_id = "metrics-v2";

        for (field, changed) in [
            ("experimentId", experiment),
            ("scenarioArtifactHash", scenario),
            ("designArtifactHash", design),
            ("semanticsVersion", semantics),
            ("numericProfileHash", numeric),
            ("physicalScaleProfileHash", physical),
            ("balanceProfileHash", balance),
            ("longWireDistance", distance),
            ("seed", seed),
            ("maxTicks", max_ticks),
            ("metricSetId", metric),
        ] {
            let changed_id = experiment_run_id(changed);
            assert_eq!(changed_id, independently_encode_run_id(changed), "{field}");
            assert_ne!(changed_id, baseline_id, "{field}");
        }
    }

    #[test]
    fn nonallocating_bound_helpers_cover_unreachable_public_plan_edges() {
        assert_eq!(
            checked_product(&[u32::MAX as usize, 2]),
            Err(ExperimentPlanError::CardinalityOverflow)
        );
        if usize::BITS > u32::BITS {
            assert_eq!(
                validate_text_length_bytes(
                    ExperimentTextField::ExperimentId,
                    u32::MAX as usize + 1,
                ),
                Err(ExperimentPlanError::TextFieldTooLong {
                    field: ExperimentTextField::ExperimentId,
                })
            );
        }
    }

    #[test]
    fn duplicate_run_id_collision_guard_is_typed() {
        let run_id = experiment_run_id(run_identity());
        assert_eq!(
            require_unique_run_ids([run_id, run_id]),
            Err(ExperimentPlanError::DuplicateExperimentRun { run_id })
        );
    }
}
