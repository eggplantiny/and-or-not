use crate::{
    BindPortCommand, Command, CommandEncodingError, CommandEnvelope, DriveStrength, DriverId,
    EndpointTarget, EntityId, Fixed, FixedAabb, FixedVec2, GateId, GatePort, GatePortRef, GateType,
    HashAlgorithmId, HashParseError, JsonErrorCategory, LogicLevel, PlaceFixedSubstrateCommand,
    PlaceGateCommand, PlaceJunctionCommand, PlaceMobileSubstrateCommand, PlaceWireCommand,
    ProfileHash, RemoveEntityCommand, RoutingDomain, SemanticsVersion, SetExternalDriverCommand,
    Simulation, StateHash, Tick, WireEnd, WireId,
};
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

pub const REPLAY_FORMAT_VERSION_V1: u32 = 1;
pub const STATE_HASH_VERSION_V3: &str = "aon-state-v3";
pub const WORLD_GENERATOR_VERSION_EMPTY_V1: &str = "aon-empty-v1";

const SEED_BYTE_LENGTH: usize = 32;
const SEED_HEX_LENGTH: usize = SEED_BYTE_LENGTH * 2;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReplayFormatVersion {
    #[default]
    V1,
}

impl ReplayFormatVersion {
    pub const fn as_u32(self) -> u32 {
        match self {
            Self::V1 => REPLAY_FORMAT_VERSION_V1,
        }
    }

    fn parse(actual: u32) -> Result<Self, ReplayError> {
        match actual {
            REPLAY_FORMAT_VERSION_V1 => Ok(Self::V1),
            actual => Err(ReplayError::UnsupportedFormatVersion {
                expected: REPLAY_FORMAT_VERSION_V1,
                actual,
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StateHashVersion {
    #[default]
    V3,
}

impl StateHashVersion {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::V3 => STATE_HASH_VERSION_V3,
        }
    }

    fn parse(actual: &str) -> Result<Self, ReplayError> {
        match actual {
            STATE_HASH_VERSION_V3 => Ok(Self::V3),
            actual => Err(ReplayError::UnsupportedStateHashVersion {
                expected: STATE_HASH_VERSION_V3,
                actual: actual.to_owned(),
            }),
        }
    }

    pub(crate) const fn current() -> Self {
        Self::V3
    }
}

impl fmt::Display for StateHashVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WorldGeneratorVersion {
    #[default]
    EmptyV1,
}

impl WorldGeneratorVersion {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EmptyV1 => WORLD_GENERATOR_VERSION_EMPTY_V1,
        }
    }

    fn parse(actual: &str) -> Result<Self, ReplayError> {
        match actual {
            WORLD_GENERATOR_VERSION_EMPTY_V1 => Ok(Self::EmptyV1),
            actual => Err(ReplayError::UnsupportedWorldGeneratorVersion {
                expected: WORLD_GENERATOR_VERSION_EMPTY_V1,
                actual: actual.to_owned(),
            }),
        }
    }
}

impl fmt::Display for WorldGeneratorVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum SeedParseError {
    #[error("canonical Seed must contain exactly 64 lowercase hexadecimal characters")]
    InvalidLength,

    #[error("canonical Seed contains a non-lowercase-hexadecimal character at byte {index}")]
    InvalidCharacter { index: usize },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Seed([u8; SEED_BYTE_LENGTH]);

impl Seed {
    pub const ZERO: Self = Self([0; SEED_BYTE_LENGTH]);

    pub const fn as_bytes(&self) -> &[u8; SEED_BYTE_LENGTH] {
        &self.0
    }

    pub fn from_hex(value: &str) -> Result<Self, SeedParseError> {
        let encoded = value.as_bytes();
        if encoded.len() != SEED_HEX_LENGTH {
            return Err(SeedParseError::InvalidLength);
        }
        let mut decoded = [0_u8; SEED_BYTE_LENGTH];
        for (index, pair) in encoded.chunks_exact(2).enumerate() {
            let high = decode_lower_hex(pair[0], index * 2)?;
            let low = decode_lower_hex(pair[1], index * 2 + 1)?;
            decoded[index] = (high << 4) | low;
        }
        Ok(Self(decoded))
    }
}

impl fmt::Display for Seed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

fn decode_lower_hex(value: u8, index: usize) -> Result<u8, SeedParseError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(SeedParseError::InvalidCharacter { index }),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplayHeader {
    pub format_version: ReplayFormatVersion,
    pub semantics_version: SemanticsVersion,
    pub numeric_profile_hash: ProfileHash,
    pub physical_scale_profile_hash: ProfileHash,
    pub balance_profile_hash: ProfileHash,
    pub state_hash_version: StateHashVersion,
    pub world_generator_version: WorldGeneratorVersion,
    pub seed: Seed,
    pub initial_state_hash: StateHash,
    pub hash_algorithm_id: HashAlgorithmId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HashCheckpoint {
    pub next_tick: Tick,
    pub state_hash: StateHash,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorldInputEvent {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Replay {
    header: ReplayHeader,
    commands: Vec<CommandEnvelope>,
    world_inputs: Vec<WorldInputEvent>,
    checkpoints: Vec<HashCheckpoint>,
}

impl Replay {
    pub fn new(
        header: ReplayHeader,
        commands: Vec<CommandEnvelope>,
        checkpoints: Vec<HashCheckpoint>,
    ) -> Result<Self, ReplayError> {
        let replay = Self {
            header,
            commands: normalize_commands(commands)?,
            world_inputs: Vec::new(),
            checkpoints,
        };
        replay.validate_shape()?;
        Ok(replay)
    }

    pub const fn header(&self) -> &ReplayHeader {
        &self.header
    }

    pub fn commands(&self) -> &[CommandEnvelope] {
        &self.commands
    }

    pub fn world_inputs(&self) -> &[WorldInputEvent] {
        &self.world_inputs
    }

    pub fn checkpoints(&self) -> &[HashCheckpoint] {
        &self.checkpoints
    }

    pub fn commands_for_tick(&self, tick: Tick) -> impl Iterator<Item = &CommandEnvelope> + '_ {
        self.commands
            .iter()
            .filter(move |command| command.target_tick == tick)
    }

    pub fn final_next_tick(&self) -> Tick {
        self.checkpoints
            .last()
            .expect("validated Replay always has a checkpoint")
            .next_tick
    }

    pub fn validate_against(&self, simulation: &Simulation) -> Result<(), ReplayError> {
        self.validate_shape()?;
        let actual = simulation.replay_header();
        compare_header_field(
            ReplayContractField::FormatVersion,
            self.header.format_version.as_u32().to_string(),
            actual.format_version.as_u32().to_string(),
        )?;
        compare_header_field(
            ReplayContractField::SemanticsVersion,
            self.header.semantics_version.to_string(),
            actual.semantics_version.to_string(),
        )?;
        compare_header_field(
            ReplayContractField::NumericProfileHash,
            self.header.numeric_profile_hash.to_string(),
            actual.numeric_profile_hash.to_string(),
        )?;
        compare_header_field(
            ReplayContractField::PhysicalScaleProfileHash,
            self.header.physical_scale_profile_hash.to_string(),
            actual.physical_scale_profile_hash.to_string(),
        )?;
        compare_header_field(
            ReplayContractField::BalanceProfileHash,
            self.header.balance_profile_hash.to_string(),
            actual.balance_profile_hash.to_string(),
        )?;
        compare_header_field(
            ReplayContractField::StateHashVersion,
            self.header.state_hash_version.to_string(),
            actual.state_hash_version.to_string(),
        )?;
        compare_header_field(
            ReplayContractField::WorldGeneratorVersion,
            self.header.world_generator_version.to_string(),
            actual.world_generator_version.to_string(),
        )?;
        compare_header_field(
            ReplayContractField::Seed,
            self.header.seed.to_string(),
            actual.seed.to_string(),
        )?;
        compare_header_field(
            ReplayContractField::InitialStateHash,
            self.header.initial_state_hash.to_string(),
            actual.initial_state_hash.to_string(),
        )?;
        if self.header.hash_algorithm_id != actual.hash_algorithm_id {
            return Err(ReplayError::HashAlgorithmMismatch {
                expected: self.header.hash_algorithm_id.to_string(),
                actual: actual.hash_algorithm_id.to_string(),
            });
        }
        if simulation.next_tick() != Tick(0) {
            return Err(ReplayError::ReplayRequiresFreshSimulation {
                actual: simulation.next_tick(),
            });
        }
        let current_hash = simulation.state_hash();
        if current_hash != self.header.initial_state_hash {
            return Err(ReplayError::CurrentStateMismatch {
                expected: self.header.initial_state_hash,
                actual: current_hash,
            });
        }
        Ok(())
    }

    pub fn verify_trace(&self, trace: &[StateHash]) -> Result<(), ReplayError> {
        self.validate_shape()?;
        let expected_len = expected_trace_len(self.final_next_tick())?;
        if trace.len() != expected_len {
            return Err(ReplayError::TraceLengthMismatch {
                expected: u64::try_from(expected_len)
                    .map_err(|_| ReplayError::TraceLengthOverflow)?,
                actual: u64::try_from(trace.len()).unwrap_or(u64::MAX),
            });
        }
        for checkpoint in &self.checkpoints {
            let index = usize::try_from(checkpoint.next_tick.0)
                .map_err(|_| ReplayError::TraceLengthOverflow)?;
            let actual = trace[index];
            if actual != checkpoint.state_hash {
                return Err(ReplayError::CheckpointDivergence {
                    next_tick: checkpoint.next_tick,
                    expected: checkpoint.state_hash,
                    actual,
                });
            }
        }
        Ok(())
    }

    fn validate_shape(&self) -> Result<(), ReplayError> {
        if self.header.world_generator_version == WorldGeneratorVersion::EmptyV1
            && self.header.seed != Seed::ZERO
        {
            return Err(ReplayError::NonzeroEmptyWorldSeed);
        }
        if !self.world_inputs.is_empty() {
            return Err(ReplayError::UnsupportedWorldInputs {
                count: self.world_inputs.len(),
            });
        }
        let Some(first) = self.checkpoints.first() else {
            return Err(ReplayError::MissingInitialCheckpoint);
        };
        if first.next_tick != Tick(0) {
            return Err(ReplayError::InitialCheckpointTick {
                actual: first.next_tick,
            });
        }
        if first.state_hash != self.header.initial_state_hash {
            return Err(ReplayError::InitialCheckpointHashMismatch {
                header: self.header.initial_state_hash,
                checkpoint: first.state_hash,
            });
        }
        for pair in self.checkpoints.windows(2) {
            if pair[0].next_tick >= pair[1].next_tick {
                return Err(ReplayError::CheckpointOrder {
                    previous: pair[0].next_tick,
                    actual: pair[1].next_tick,
                });
            }
        }
        let final_next_tick = self
            .checkpoints
            .last()
            .expect("nonempty checkpoint list was checked")
            .next_tick;
        expected_trace_len(final_next_tick)?;
        if let Some(command) = self
            .commands
            .iter()
            .find(|command| command.target_tick >= final_next_tick)
        {
            return Err(ReplayError::CommandOutsideRunBoundary {
                target_tick: command.target_tick,
                final_next_tick,
            });
        }
        Ok(())
    }
}

fn expected_trace_len(final_next_tick: Tick) -> Result<usize, ReplayError> {
    let length = final_next_tick
        .0
        .checked_add(1)
        .ok_or(ReplayError::TraceLengthOverflow)?;
    usize::try_from(length).map_err(|_| ReplayError::TraceLengthOverflow)
}

fn normalize_commands(commands: Vec<CommandEnvelope>) -> Result<Vec<CommandEnvelope>, ReplayError> {
    let mut encoded = commands
        .into_iter()
        .map(|command| {
            let bytes = command.canonical_bytes().map_err(ReplayError::from)?;
            Ok((command, bytes))
        })
        .collect::<Result<Vec<_>, ReplayError>>()?;
    encoded.sort_by(|left, right| {
        left.0
            .target_tick
            .cmp(&right.0.target_tick)
            .then_with(|| left.0.ordinal.cmp(&right.0.ordinal))
            .then_with(|| left.1.cmp(&right.1))
    });
    Ok(encoded.into_iter().map(|(command, _)| command).collect())
}

fn compare_header_field(
    field: ReplayContractField,
    expected: String,
    actual: String,
) -> Result<(), ReplayError> {
    if expected == actual {
        Ok(())
    } else {
        Err(ReplayError::ContractMismatch {
            field,
            expected,
            actual,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayArtifact {
    scenario_path: String,
    replay: Replay,
}

impl ReplayArtifact {
    pub fn new(scenario_path: impl Into<String>, replay: Replay) -> Result<Self, ReplayError> {
        let scenario_path = scenario_path.into();
        validate_scenario_path(&scenario_path)?;
        Ok(Self {
            scenario_path,
            replay,
        })
    }

    pub fn scenario_path(&self) -> &str {
        &self.scenario_path
    }

    pub const fn replay(&self) -> &Replay {
        &self.replay
    }

    pub fn into_parts(self) -> (String, Replay) {
        (self.scenario_path, self.replay)
    }
}

fn validate_scenario_path(path: &str) -> Result<(), ReplayError> {
    let bytes = path.as_bytes();
    let windows_drive = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';
    if path.is_empty()
        || path.contains('\0')
        || path.contains('\\')
        || path.starts_with('/')
        || windows_drive
    {
        Err(ReplayError::InvalidScenarioPath)
    } else {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayContractField {
    FormatVersion,
    SemanticsVersion,
    NumericProfileHash,
    PhysicalScaleProfileHash,
    BalanceProfileHash,
    StateHashVersion,
    WorldGeneratorVersion,
    Seed,
    InitialStateHash,
}

impl fmt::Display for ReplayContractField {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::FormatVersion => "formatVersion",
            Self::SemanticsVersion => "semanticsVersion",
            Self::NumericProfileHash => "numericProfileHash",
            Self::PhysicalScaleProfileHash => "physicalScaleProfileHash",
            Self::BalanceProfileHash => "balanceProfileHash",
            Self::StateHashVersion => "stateHashVersion",
            Self::WorldGeneratorVersion => "worldGeneratorVersion",
            Self::Seed => "seed",
            Self::InitialStateHash => "initialStateHash",
        })
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ReplayError {
    #[error("invalid Replay JSON: category={category:?}, line={line}, column={column}")]
    InvalidJson {
        category: JsonErrorCategory,
        line: usize,
        column: usize,
    },

    #[error("unable to encode canonical Replay JSON")]
    JsonEncoding,

    #[error("unsupported Replay format: expected {expected}, got {actual}")]
    UnsupportedFormatVersion { expected: u32, actual: u32 },

    #[error("unsupported State hash version: expected {expected}, got {actual}")]
    UnsupportedStateHashVersion {
        expected: &'static str,
        actual: String,
    },

    #[error("unsupported world generator version: expected {expected}, got {actual}")]
    UnsupportedWorldGeneratorVersion {
        expected: &'static str,
        actual: String,
    },

    #[error("unsupported Replay semantics version `{actual}`")]
    UnsupportedSemanticsVersion { actual: String },

    #[error("unsupported Replay hash algorithm `{actual}`")]
    UnsupportedHashAlgorithm { actual: String },

    #[error("invalid Replay hash field `{field}`: {error}")]
    InvalidHash {
        field: &'static str,
        error: HashParseError,
    },

    #[error("invalid Replay Seed: {error}")]
    InvalidSeed { error: SeedParseError },

    #[error("aon-empty-v1 requires the all-zero Seed")]
    NonzeroEmptyWorldSeed,

    #[error("Replay v1 does not support WorldInput events (got {count})")]
    UnsupportedWorldInputs { count: usize },

    #[error("Replay scenarioPath must be a nonempty portable relative path")]
    InvalidScenarioPath,

    #[error("Replay requires checkpoint nextTick 0")]
    MissingInitialCheckpoint,

    #[error("first Replay checkpoint must use nextTick 0, got {actual}")]
    InitialCheckpointTick { actual: Tick },

    #[error(
        "initial checkpoint hash differs from Replay Header: header={header}, checkpoint={checkpoint}"
    )]
    InitialCheckpointHashMismatch {
        header: StateHash,
        checkpoint: StateHash,
    },

    #[error("Replay checkpoints must increase strictly: previous={previous}, actual={actual}")]
    CheckpointOrder { previous: Tick, actual: Tick },

    #[error("Replay command targets Tick {target_tick} outside final nextTick {final_next_tick}")]
    CommandOutsideRunBoundary {
        target_tick: Tick,
        final_next_tick: Tick,
    },

    #[error("Replay contract mismatch for {field}: expected {expected}, got {actual}")]
    ContractMismatch {
        field: ReplayContractField,
        expected: String,
        actual: String,
    },

    #[error("Replay hash algorithm mismatch: expected {expected}, got {actual}")]
    HashAlgorithmMismatch { expected: String, actual: String },

    #[error("Replay execution requires nextTick 0, got {actual}")]
    ReplayRequiresFreshSimulation { actual: Tick },

    #[error("Replay current initial state mismatch: expected {expected}, got {actual}")]
    CurrentStateMismatch {
        expected: StateHash,
        actual: StateHash,
    },

    #[error(
        "Replay checkpoint diverged at nextTick {next_tick}: expected {expected}, got {actual}"
    )]
    CheckpointDivergence {
        next_tick: Tick,
        expected: StateHash,
        actual: StateHash,
    },

    #[error("Replay trace length overflow")]
    TraceLengthOverflow,

    #[error("Replay trace length mismatch: expected {expected}, got {actual}")]
    TraceLengthMismatch { expected: u64, actual: u64 },

    #[error(transparent)]
    CommandEncoding(#[from] CommandEncodingError),
}

pub fn decode_replay_artifact(bytes: &[u8]) -> Result<ReplayArtifact, ReplayError> {
    let wire: ReplayArtifactWire =
        serde_json::from_slice(bytes).map_err(|error| ReplayError::InvalidJson {
            category: JsonErrorCategory::from(error.classify()),
            line: error.line(),
            column: error.column(),
        })?;
    wire.try_into()
}

pub fn encode_replay_artifact(artifact: &ReplayArtifact) -> Result<Vec<u8>, ReplayError> {
    let wire = ReplayArtifactWire::from(artifact);
    let mut encoded = serde_json::to_vec_pretty(&wire).map_err(|_| ReplayError::JsonEncoding)?;
    encoded.push(b'\n');
    Ok(encoded)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReplayArtifactWire {
    scenario_path: String,
    header: ReplayHeaderWire,
    commands: Vec<CommandEnvelopeWire>,
    world_inputs: Vec<serde_json::Value>,
    checkpoints: Vec<HashCheckpointWire>,
}

impl TryFrom<ReplayArtifactWire> for ReplayArtifact {
    type Error = ReplayError;

    fn try_from(wire: ReplayArtifactWire) -> Result<Self, Self::Error> {
        if !wire.world_inputs.is_empty() {
            return Err(ReplayError::UnsupportedWorldInputs {
                count: wire.world_inputs.len(),
            });
        }
        let replay = Replay::new(
            wire.header.try_into()?,
            wire.commands
                .into_iter()
                .map(CommandEnvelope::from)
                .collect(),
            wire.checkpoints
                .into_iter()
                .map(HashCheckpoint::try_from)
                .collect::<Result<Vec<_>, _>>()?,
        )?;
        ReplayArtifact::new(wire.scenario_path, replay)
    }
}

impl From<&ReplayArtifact> for ReplayArtifactWire {
    fn from(artifact: &ReplayArtifact) -> Self {
        Self {
            scenario_path: artifact.scenario_path.clone(),
            header: ReplayHeaderWire::from(artifact.replay.header),
            commands: artifact
                .replay
                .commands
                .iter()
                .map(CommandEnvelopeWire::from)
                .collect(),
            world_inputs: Vec::new(),
            checkpoints: artifact
                .replay
                .checkpoints
                .iter()
                .copied()
                .map(HashCheckpointWire::from)
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReplayHeaderWire {
    format_version: u32,
    semantics_version: String,
    numeric_profile_hash: String,
    physical_scale_profile_hash: String,
    balance_profile_hash: String,
    state_hash_version: String,
    world_generator_version: String,
    seed: String,
    initial_state_hash: String,
    hash_algorithm_id: String,
}

impl TryFrom<ReplayHeaderWire> for ReplayHeader {
    type Error = ReplayError;

    fn try_from(wire: ReplayHeaderWire) -> Result<Self, Self::Error> {
        Ok(Self {
            format_version: ReplayFormatVersion::parse(wire.format_version)?,
            semantics_version: SemanticsVersion::parse(&wire.semantics_version).map_err(|_| {
                ReplayError::UnsupportedSemanticsVersion {
                    actual: wire.semantics_version.clone(),
                }
            })?,
            numeric_profile_hash: parse_profile_hash(
                "numericProfileHash",
                &wire.numeric_profile_hash,
            )?,
            physical_scale_profile_hash: parse_profile_hash(
                "physicalScaleProfileHash",
                &wire.physical_scale_profile_hash,
            )?,
            balance_profile_hash: parse_profile_hash(
                "balanceProfileHash",
                &wire.balance_profile_hash,
            )?,
            state_hash_version: StateHashVersion::parse(&wire.state_hash_version)?,
            world_generator_version: WorldGeneratorVersion::parse(&wire.world_generator_version)?,
            seed: Seed::from_hex(&wire.seed).map_err(|error| ReplayError::InvalidSeed { error })?,
            initial_state_hash: StateHash::from_hex(&wire.initial_state_hash).map_err(|error| {
                ReplayError::InvalidHash {
                    field: "initialStateHash",
                    error,
                }
            })?,
            hash_algorithm_id: HashAlgorithmId::parse(&wire.hash_algorithm_id).map_err(|_| {
                ReplayError::UnsupportedHashAlgorithm {
                    actual: wire.hash_algorithm_id,
                }
            })?,
        })
    }
}

impl From<ReplayHeader> for ReplayHeaderWire {
    fn from(header: ReplayHeader) -> Self {
        Self {
            format_version: header.format_version.as_u32(),
            semantics_version: header.semantics_version.to_string(),
            numeric_profile_hash: header.numeric_profile_hash.to_string(),
            physical_scale_profile_hash: header.physical_scale_profile_hash.to_string(),
            balance_profile_hash: header.balance_profile_hash.to_string(),
            state_hash_version: header.state_hash_version.to_string(),
            world_generator_version: header.world_generator_version.to_string(),
            seed: header.seed.to_string(),
            initial_state_hash: header.initial_state_hash.to_string(),
            hash_algorithm_id: header.hash_algorithm_id.to_string(),
        }
    }
}

fn parse_profile_hash(field: &'static str, value: &str) -> Result<ProfileHash, ReplayError> {
    ProfileHash::from_hex(value).map_err(|error| ReplayError::InvalidHash { field, error })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HashCheckpointWire {
    next_tick: u64,
    state_hash: String,
}

impl TryFrom<HashCheckpointWire> for HashCheckpoint {
    type Error = ReplayError;

    fn try_from(wire: HashCheckpointWire) -> Result<Self, Self::Error> {
        Ok(Self {
            next_tick: Tick(wire.next_tick),
            state_hash: StateHash::from_hex(&wire.state_hash).map_err(|error| {
                ReplayError::InvalidHash {
                    field: "checkpoints.stateHash",
                    error,
                }
            })?,
        })
    }
}

impl From<HashCheckpoint> for HashCheckpointWire {
    fn from(checkpoint: HashCheckpoint) -> Self {
        Self {
            next_tick: checkpoint.next_tick.0,
            state_hash: checkpoint.state_hash.to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CommandEnvelopeWire {
    target_tick: u64,
    ordinal: u64,
    command: CommandWire,
}

impl From<CommandEnvelopeWire> for CommandEnvelope {
    fn from(wire: CommandEnvelopeWire) -> Self {
        Self {
            target_tick: Tick(wire.target_tick),
            ordinal: wire.ordinal,
            command: wire.command.into(),
        }
    }
}

impl From<&CommandEnvelope> for CommandEnvelopeWire {
    fn from(envelope: &CommandEnvelope) -> Self {
        Self {
            target_tick: envelope.target_tick.0,
            ordinal: envelope.ordinal,
            command: CommandWire::from(&envelope.command),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum CommandWire {
    PlaceGate {
        gate_type: GateTypeWire,
        origin: PointWire,
        routing_domain: RoutingDomainWire,
    },
    PlaceWire {
        routing_domain: RoutingDomainWire,
        points: Vec<PointWire>,
        endpoint_a: EndpointTargetWire,
        endpoint_b: EndpointTargetWire,
    },
    PlaceJunction {
        routing_domain: RoutingDomainWire,
        position: PointWire,
    },
    PlaceFixedSubstrate {
        origin: PointWire,
        routing_area: AabbWire,
        footprint: AabbWire,
    },
    PlaceMobileSubstrate {
        origin: PointWire,
        routing_area: AabbWire,
        footprint: AabbWire,
    },
    RemoveEntity {
        target: u64,
    },
    BindPort {
        wire: u64,
        end: WireEndWire,
        target: EndpointTargetWire,
    },
    SetExternalDriver {
        driver: u64,
        level: LogicLevelWire,
        strength: u64,
    },
}

impl From<CommandWire> for Command {
    fn from(wire: CommandWire) -> Self {
        match wire {
            CommandWire::PlaceGate {
                gate_type,
                origin,
                routing_domain,
            } => Self::PlaceGate(PlaceGateCommand {
                gate_type: gate_type.into(),
                origin: origin.into(),
                routing_domain: routing_domain.into(),
            }),
            CommandWire::PlaceWire {
                routing_domain,
                points,
                endpoint_a,
                endpoint_b,
            } => Self::PlaceWire(PlaceWireCommand {
                routing_domain: routing_domain.into(),
                points: points.into_iter().map(FixedVec2::from).collect(),
                endpoint_a: endpoint_a.into(),
                endpoint_b: endpoint_b.into(),
            }),
            CommandWire::PlaceJunction {
                routing_domain,
                position,
            } => Self::PlaceJunction(PlaceJunctionCommand {
                routing_domain: routing_domain.into(),
                position: position.into(),
            }),
            CommandWire::PlaceFixedSubstrate {
                origin,
                routing_area,
                footprint,
            } => Self::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
                origin: origin.into(),
                routing_area: routing_area.into(),
                footprint: footprint.into(),
            }),
            CommandWire::PlaceMobileSubstrate {
                origin,
                routing_area,
                footprint,
            } => Self::PlaceMobileSubstrate(PlaceMobileSubstrateCommand {
                origin: origin.into(),
                routing_area: routing_area.into(),
                footprint: footprint.into(),
            }),
            CommandWire::RemoveEntity { target } => Self::RemoveEntity(RemoveEntityCommand {
                target: EntityId(target),
            }),
            CommandWire::BindPort { wire, end, target } => Self::BindPort(BindPortCommand {
                wire: WireId(EntityId(wire)),
                end: end.into(),
                target: target.into(),
            }),
            CommandWire::SetExternalDriver {
                driver,
                level,
                strength,
            } => Self::SetExternalDriver(SetExternalDriverCommand {
                driver: DriverId(EntityId(driver)),
                level: level.into(),
                strength: DriveStrength(strength),
            }),
        }
    }
}

impl From<&Command> for CommandWire {
    fn from(command: &Command) -> Self {
        match command {
            Command::PlaceGate(command) => Self::PlaceGate {
                gate_type: GateTypeWire::from(command.gate_type),
                origin: PointWire::from(command.origin),
                routing_domain: RoutingDomainWire::from(command.routing_domain),
            },
            Command::PlaceWire(command) => Self::PlaceWire {
                routing_domain: RoutingDomainWire::from(command.routing_domain),
                points: command
                    .points
                    .iter()
                    .copied()
                    .map(PointWire::from)
                    .collect(),
                endpoint_a: EndpointTargetWire::from(command.endpoint_a),
                endpoint_b: EndpointTargetWire::from(command.endpoint_b),
            },
            Command::PlaceJunction(command) => Self::PlaceJunction {
                routing_domain: RoutingDomainWire::from(command.routing_domain),
                position: PointWire::from(command.position),
            },
            Command::PlaceFixedSubstrate(command) => Self::PlaceFixedSubstrate {
                origin: PointWire::from(command.origin),
                routing_area: AabbWire::from(command.routing_area),
                footprint: AabbWire::from(command.footprint),
            },
            Command::PlaceMobileSubstrate(command) => Self::PlaceMobileSubstrate {
                origin: PointWire::from(command.origin),
                routing_area: AabbWire::from(command.routing_area),
                footprint: AabbWire::from(command.footprint),
            },
            Command::RemoveEntity(command) => Self::RemoveEntity {
                target: command.target.0,
            },
            Command::BindPort(command) => Self::BindPort {
                wire: command.wire.entity_id().0,
                end: WireEndWire::from(command.end),
                target: EndpointTargetWire::from(command.target),
            },
            Command::SetExternalDriver(command) => Self::SetExternalDriver {
                driver: command.driver.entity_id().0,
                level: LogicLevelWire::from(command.level),
                strength: command.strength.0,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum GateTypeWire {
    And,
    Or,
    Not,
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

impl From<GateType> for GateTypeWire {
    fn from(value: GateType) -> Self {
        match value {
            GateType::And => Self::And,
            GateType::Or => Self::Or,
            GateType::Not => Self::Not,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum GatePortWire {
    InputA,
    InputB,
    Output,
    Power,
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

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum WireEndWire {
    A,
    B,
}

impl From<WireEndWire> for WireEnd {
    fn from(value: WireEndWire) -> Self {
        match value {
            WireEndWire::A => Self::A,
            WireEndWire::B => Self::B,
        }
    }
}

impl From<WireEnd> for WireEndWire {
    fn from(value: WireEnd) -> Self {
        match value {
            WireEnd::A => Self::A,
            WireEnd::B => Self::B,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum LogicLevelWire {
    Low,
    High,
    X,
}

impl From<LogicLevelWire> for LogicLevel {
    fn from(value: LogicLevelWire) -> Self {
        match value {
            LogicLevelWire::Low => Self::Low,
            LogicLevelWire::High => Self::High,
            LogicLevelWire::X => Self::X,
        }
    }
}

impl From<LogicLevel> for LogicLevelWire {
    fn from(value: LogicLevel) -> Self {
        match value {
            LogicLevel::Low => Self::Low,
            LogicLevel::High => Self::High,
            LogicLevel::X => Self::X,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PointWire {
    x: i64,
    y: i64,
}

impl From<PointWire> for FixedVec2 {
    fn from(point: PointWire) -> Self {
        Self::new(Fixed(point.x), Fixed(point.y))
    }
}

impl From<FixedVec2> for PointWire {
    fn from(point: FixedVec2) -> Self {
        Self {
            x: point.x.0,
            y: point.y.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AabbWire {
    min: PointWire,
    max: PointWire,
}

impl From<AabbWire> for FixedAabb {
    fn from(aabb: AabbWire) -> Self {
        Self::new(aabb.min.into(), aabb.max.into())
    }
}

impl From<FixedAabb> for AabbWire {
    fn from(aabb: FixedAabb) -> Self {
        Self {
            min: aabb.min.into(),
            max: aabb.max.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum RoutingDomainWire {
    OpenWorld,
    FixedSubstrate { substrate: u64 },
    MobileSubstrate { substrate: u64 },
}

impl From<RoutingDomainWire> for RoutingDomain {
    fn from(domain: RoutingDomainWire) -> Self {
        match domain {
            RoutingDomainWire::OpenWorld => Self::OpenWorld,
            RoutingDomainWire::FixedSubstrate { substrate } => {
                Self::FixedSubstrate(EntityId(substrate))
            }
            RoutingDomainWire::MobileSubstrate { substrate } => {
                Self::MobileSubstrate(EntityId(substrate))
            }
        }
    }
}

impl From<RoutingDomain> for RoutingDomainWire {
    fn from(domain: RoutingDomain) -> Self {
        match domain {
            RoutingDomain::OpenWorld => Self::OpenWorld,
            RoutingDomain::FixedSubstrate(entity) => Self::FixedSubstrate {
                substrate: entity.0,
            },
            RoutingDomain::MobileSubstrate(entity) => Self::MobileSubstrate {
                substrate: entity.0,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum EndpointTargetWire {
    Free,
    Junction { junction: u64 },
    GatePort { gate: u64, port: GatePortWire },
}

impl From<EndpointTargetWire> for EndpointTarget {
    fn from(target: EndpointTargetWire) -> Self {
        match target {
            EndpointTargetWire::Free => Self::Free,
            EndpointTargetWire::Junction { junction } => {
                Self::Junction(crate::JunctionId(EntityId(junction)))
            }
            EndpointTargetWire::GatePort { gate, port } => Self::GatePort(GatePortRef {
                gate: GateId(EntityId(gate)),
                port: port.into(),
            }),
        }
    }
}

impl From<EndpointTarget> for EndpointTargetWire {
    fn from(target: EndpointTarget) -> Self {
        match target {
            EndpointTarget::Free => Self::Free,
            EndpointTarget::Junction(junction) => Self::Junction {
                junction: junction.entity_id().0,
            },
            EndpointTarget::GatePort(reference) => Self::GatePort {
                gate: reference.gate.entity_id().0,
                port: reference.port.into(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        BalanceProfile, CommandRejectionReason, InitialWorld, NumericProfile, PhysicalScaleProfile,
        ProfileBundle, RenderSnapshot, SimulationContract, SimulationPackage, StageFeatureSet,
    };

    type HeaderMutation = fn(&mut ReplayHeader);

    fn simulation() -> Simulation {
        let profiles = ProfileBundle {
            numeric: NumericProfile::reference_v1("replay"),
            physical_scale: PhysicalScaleProfile::stage0_alpha("replay"),
            balance: BalanceProfile::stage0_alpha("replay"),
        };
        let contract = SimulationContract::from_profiles(&profiles).expect("valid profiles");
        Simulation::new(SimulationPackage::new(
            "replay",
            InitialWorld::Empty,
            StageFeatureSet::none(),
            contract,
            profiles,
        ))
        .expect("simulation starts")
    }

    fn empty_replay(simulation: &Simulation, final_next_tick: u64) -> Replay {
        Replay::new(
            simulation.replay_header(),
            Vec::new(),
            vec![
                HashCheckpoint {
                    next_tick: Tick(0),
                    state_hash: simulation.state_hash(),
                },
                HashCheckpoint {
                    next_tick: Tick(final_next_tick),
                    state_hash: simulation.state_hash(),
                },
            ],
        )
        .expect("Replay shape is valid")
    }

    fn empty_replay_json(simulation: &Simulation) -> serde_json::Value {
        let artifact = ReplayArtifact::new("scenario.json", empty_replay(simulation, 1))
            .expect("the test Replay has a valid locator");
        serde_json::from_slice(&encode_replay_artifact(&artifact).expect("the test Replay encodes"))
            .expect("the encoded Replay is JSON")
    }

    fn decode_json(value: &serde_json::Value) -> Result<ReplayArtifact, ReplayError> {
        decode_replay_artifact(&serde_json::to_vec(value).expect("test JSON serializes"))
    }

    fn alternate_profile_hash() -> ProfileHash {
        ProfileHash::from_hex(&"1".repeat(64)).expect("alternate Profile Hash is canonical")
    }

    fn alternate_state_hash() -> StateHash {
        StateHash::from_hex(&"2".repeat(64)).expect("alternate State Hash is canonical")
    }

    #[test]
    fn seed_hex_is_exact_lowercase_and_fixed_width() {
        assert_eq!(Seed::ZERO.to_string(), "0".repeat(64));
        assert_eq!(Seed::from_hex(&"0".repeat(64)), Ok(Seed::ZERO));
        assert_eq!(Seed::from_hex("0"), Err(SeedParseError::InvalidLength));
        let mut uppercase = "0".repeat(64);
        uppercase.replace_range(0..1, "A");
        assert_eq!(
            Seed::from_hex(&uppercase),
            Err(SeedParseError::InvalidCharacter { index: 0 })
        );
    }

    #[test]
    fn replay_json_round_trip_is_canonical_and_strict() {
        let simulation = simulation();
        let replay = empty_replay(&simulation, 1);
        let artifact = ReplayArtifact::new("../scenarios/empty.json", replay).unwrap();
        let encoded = encode_replay_artifact(&artifact).expect("Replay encodes");
        let decoded = decode_replay_artifact(&encoded).expect("Replay decodes");
        assert_eq!(decoded, artifact);
        assert_eq!(encode_replay_artifact(&decoded).unwrap(), encoded);
        assert!(encoded.starts_with(b"{\n  \"scenarioPath\""));
        assert!(encoded.ends_with(b"\n"));
        assert!(!encoded.ends_with(b"\n\n"));

        let mut unknown: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
        unknown["unknown"] = serde_json::json!(true);
        assert!(matches!(
            decode_replay_artifact(&serde_json::to_vec(&unknown).unwrap()),
            Err(ReplayError::InvalidJson { .. })
        ));
    }

    #[test]
    fn replay_json_rejects_numeric_width_and_float_inputs() {
        let simulation = simulation();
        let valid = empty_replay_json(&simulation);

        let mut format_overflow = valid.clone();
        format_overflow["header"]["formatVersion"] = serde_json::json!(u64::from(u32::MAX) + 1);
        let mut format_float = valid.clone();
        format_float["header"]["formatVersion"] = serde_json::json!(1.5);

        let compact = serde_json::to_string(&valid).expect("valid Replay serializes");
        let next_tick_overflow =
            compact.replacen("\"nextTick\":0", "\"nextTick\":18446744073709551616", 1);

        let mut command_value = valid;
        command_value["commands"] = serde_json::json!([{
            "targetTick": 0,
            "ordinal": 0,
            "command": {
                "type": "place-gate",
                "gateType": "not",
                "origin": { "x": 0, "y": 0 },
                "routingDomain": { "kind": "open-world" }
            }
        }]);
        let coordinate_overflow = serde_json::to_string(&command_value)
            .expect("command Replay serializes")
            .replacen("\"x\":0", "\"x\":9223372036854775808", 1);

        let cases = [
            (
                "u32 formatVersion overflow",
                serde_json::to_vec(&format_overflow).expect("test JSON serializes"),
            ),
            (
                "integer field containing a float",
                serde_json::to_vec(&format_float).expect("test JSON serializes"),
            ),
            ("u64 nextTick overflow", next_tick_overflow.into_bytes()),
            ("i64 coordinate overflow", coordinate_overflow.into_bytes()),
        ];

        for (name, bytes) in cases {
            assert!(
                matches!(
                    decode_replay_artifact(&bytes),
                    Err(ReplayError::InvalidJson { .. })
                ),
                "numeric rejection case `{name}` must fail as invalid JSON"
            );
        }
    }

    #[test]
    fn replay_json_rejects_unsupported_versions_hashes_seed_and_world_inputs() {
        let simulation = simulation();
        let valid = empty_replay_json(&simulation);
        let uppercase_hash = format!("A{}", "0".repeat(63));
        let uppercase_seed = format!("A{}", "0".repeat(63));
        let nonzero_seed = format!("{}1", "0".repeat(63));

        let header_cases = vec![
            (
                "format version",
                "formatVersion",
                serde_json::json!(2),
                ReplayError::UnsupportedFormatVersion {
                    expected: REPLAY_FORMAT_VERSION_V1,
                    actual: 2,
                },
            ),
            (
                "semantics version",
                "semanticsVersion",
                serde_json::json!("aon-semantics-unsupported"),
                ReplayError::UnsupportedSemanticsVersion {
                    actual: "aon-semantics-unsupported".to_owned(),
                },
            ),
            (
                "State Hash version",
                "stateHashVersion",
                serde_json::json!("aon-state-unsupported"),
                ReplayError::UnsupportedStateHashVersion {
                    expected: STATE_HASH_VERSION_V3,
                    actual: "aon-state-unsupported".to_owned(),
                },
            ),
            (
                "world generator version",
                "worldGeneratorVersion",
                serde_json::json!("aon-generator-unsupported"),
                ReplayError::UnsupportedWorldGeneratorVersion {
                    expected: WORLD_GENERATOR_VERSION_EMPTY_V1,
                    actual: "aon-generator-unsupported".to_owned(),
                },
            ),
            (
                "hash algorithm",
                "hashAlgorithmId",
                serde_json::json!("sha256"),
                ReplayError::UnsupportedHashAlgorithm {
                    actual: "sha256".to_owned(),
                },
            ),
            (
                "uppercase Profile Hash",
                "numericProfileHash",
                serde_json::json!(uppercase_hash),
                ReplayError::InvalidHash {
                    field: "numericProfileHash",
                    error: HashParseError::InvalidCharacter { index: 0 },
                },
            ),
            (
                "invalid initial State Hash width",
                "initialStateHash",
                serde_json::json!("0"),
                ReplayError::InvalidHash {
                    field: "initialStateHash",
                    error: HashParseError::InvalidLength,
                },
            ),
            (
                "uppercase Seed",
                "seed",
                serde_json::json!(uppercase_seed),
                ReplayError::InvalidSeed {
                    error: SeedParseError::InvalidCharacter { index: 0 },
                },
            ),
            (
                "invalid Seed width",
                "seed",
                serde_json::json!("0"),
                ReplayError::InvalidSeed {
                    error: SeedParseError::InvalidLength,
                },
            ),
            (
                "nonzero Empty-world Seed",
                "seed",
                serde_json::json!(nonzero_seed),
                ReplayError::NonzeroEmptyWorldSeed,
            ),
        ];

        for (name, field, replacement, expected) in header_cases {
            let mut candidate = valid.clone();
            candidate["header"][field] = replacement;
            assert_eq!(
                decode_json(&candidate),
                Err(expected),
                "typed decoder case `{name}` returned the wrong error"
            );
        }

        let mut world_inputs = valid;
        world_inputs["worldInputs"] = serde_json::json!([{}]);
        assert_eq!(
            decode_json(&world_inputs),
            Err(ReplayError::UnsupportedWorldInputs { count: 1 })
        );
    }

    #[test]
    fn command_json_round_trips_all_stage_zero_variants() {
        let simulation = simulation();
        let commands = vec![
            CommandEnvelope {
                target_tick: Tick(0),
                ordinal: 0,
                command: Command::PlaceGate(PlaceGateCommand {
                    gate_type: GateType::Not,
                    origin: FixedVec2::new(Fixed(i64::MIN), Fixed(i64::MAX)),
                    routing_domain: RoutingDomain::OpenWorld,
                }),
            },
            CommandEnvelope {
                target_tick: Tick(0),
                ordinal: 1,
                command: Command::PlaceWire(PlaceWireCommand {
                    routing_domain: RoutingDomain::FixedSubstrate(EntityId(1)),
                    points: vec![
                        FixedVec2::new(Fixed(i64::MIN), Fixed(0)),
                        FixedVec2::new(Fixed(i64::MAX), Fixed(0)),
                    ],
                    endpoint_a: EndpointTarget::Free,
                    endpoint_b: EndpointTarget::GatePort(GatePortRef {
                        gate: GateId(EntityId(u64::MAX)),
                        port: GatePort::InputB,
                    }),
                }),
            },
            CommandEnvelope {
                target_tick: Tick(0),
                ordinal: 2,
                command: Command::PlaceJunction(PlaceJunctionCommand {
                    routing_domain: RoutingDomain::MobileSubstrate(EntityId(u64::MAX)),
                    position: FixedVec2::new(Fixed(-1), Fixed(1)),
                }),
            },
            CommandEnvelope {
                target_tick: Tick(0),
                ordinal: 3,
                command: Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
                    origin: FixedVec2::new(Fixed(0), Fixed(0)),
                    routing_area: FixedAabb::new(
                        FixedVec2::new(Fixed(-2), Fixed(-2)),
                        FixedVec2::new(Fixed(2), Fixed(2)),
                    ),
                    footprint: FixedAabb::new(
                        FixedVec2::new(Fixed(-1), Fixed(-1)),
                        FixedVec2::new(Fixed(1), Fixed(1)),
                    ),
                }),
            },
            CommandEnvelope {
                target_tick: Tick(0),
                ordinal: 4,
                command: Command::PlaceMobileSubstrate(PlaceMobileSubstrateCommand {
                    origin: FixedVec2::new(Fixed(3), Fixed(4)),
                    routing_area: FixedAabb::new(
                        FixedVec2::new(Fixed(-2), Fixed(-2)),
                        FixedVec2::new(Fixed(2), Fixed(2)),
                    ),
                    footprint: FixedAabb::new(
                        FixedVec2::new(Fixed(-1), Fixed(-1)),
                        FixedVec2::new(Fixed(1), Fixed(1)),
                    ),
                }),
            },
            CommandEnvelope {
                target_tick: Tick(0),
                ordinal: 5,
                command: Command::RemoveEntity(RemoveEntityCommand {
                    target: EntityId(u64::MAX),
                }),
            },
            CommandEnvelope {
                target_tick: Tick(0),
                ordinal: 6,
                command: Command::BindPort(BindPortCommand {
                    wire: WireId(EntityId(u64::MAX)),
                    end: WireEnd::B,
                    target: EndpointTarget::Junction(crate::JunctionId(EntityId(u64::MAX))),
                }),
            },
            CommandEnvelope {
                target_tick: Tick(0),
                ordinal: 7,
                command: Command::SetExternalDriver(SetExternalDriverCommand {
                    driver: DriverId(EntityId(u64::MAX)),
                    level: LogicLevel::X,
                    strength: DriveStrength(u64::MAX),
                }),
            },
        ];
        let mut reversed = commands.clone();
        reversed.reverse();
        let replay = Replay::new(
            simulation.replay_header(),
            reversed,
            vec![
                HashCheckpoint {
                    next_tick: Tick(0),
                    state_hash: simulation.state_hash(),
                },
                HashCheckpoint {
                    next_tick: Tick(1),
                    state_hash: simulation.state_hash(),
                },
            ],
        )
        .unwrap();
        let artifact = ReplayArtifact::new("scenario.json", replay).unwrap();
        let decoded = decode_replay_artifact(&encode_replay_artifact(&artifact).unwrap()).unwrap();
        assert_eq!(decoded.replay().commands(), commands);
    }

    #[test]
    fn duplicate_ordinal_replay_normalization_is_input_json_and_execution_invariant() {
        let template = simulation();
        let header = template.replay_header();
        let commands = vec![
            CommandEnvelope {
                target_tick: Tick(0),
                ordinal: 5,
                command: Command::PlaceGate(PlaceGateCommand {
                    gate_type: GateType::Not,
                    origin: FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
                    routing_domain: RoutingDomain::OpenWorld,
                }),
            },
            CommandEnvelope {
                target_tick: Tick(0),
                ordinal: 5,
                command: Command::PlaceJunction(PlaceJunctionCommand {
                    routing_domain: RoutingDomain::OpenWorld,
                    position: FixedVec2::new(Fixed(65_536), Fixed::ZERO),
                }),
            },
            CommandEnvelope {
                target_tick: Tick(0),
                ordinal: 6,
                command: Command::PlaceJunction(PlaceJunctionCommand {
                    routing_domain: RoutingDomain::OpenWorld,
                    position: FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
                }),
            },
        ];
        let checkpoints = vec![
            HashCheckpoint {
                next_tick: Tick(0),
                state_hash: template.state_hash(),
            },
            HashCheckpoint {
                next_tick: Tick(1),
                state_hash: template.state_hash(),
            },
        ];
        let forward = ReplayArtifact::new(
            "scenario.json",
            Replay::new(header, commands.clone(), checkpoints.clone()).expect("Replay is valid"),
        )
        .expect("Replay locator is valid");
        let mut reversed_input = commands;
        reversed_input.reverse();
        let input_reversed = ReplayArtifact::new(
            "scenario.json",
            Replay::new(header, reversed_input, checkpoints).expect("reversed Replay is valid"),
        )
        .expect("Replay locator is valid");

        let canonical = encode_replay_artifact(&forward).expect("forward Replay encodes");
        assert_eq!(
            encode_replay_artifact(&input_reversed).expect("reversed-input Replay encodes"),
            canonical
        );

        let mut reversed_json: serde_json::Value =
            serde_json::from_slice(&canonical).expect("canonical Replay is JSON");
        reversed_json["commands"]
            .as_array_mut()
            .expect("commands is an array")
            .reverse();
        let json_reversed = decode_json(&reversed_json).expect("reversed JSON Replay decodes");
        assert_eq!(
            encode_replay_artifact(&json_reversed).expect("reversed JSON Replay encodes"),
            canonical
        );

        let forward_batch = forward
            .replay()
            .commands_for_tick(Tick(0))
            .cloned()
            .collect::<Vec<_>>();
        let reversed_batch = json_reversed
            .replay()
            .commands_for_tick(Tick(0))
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(forward_batch, reversed_batch);

        let mut forward_simulation = simulation();
        let mut reversed_simulation = simulation();
        let forward_report = forward_simulation
            .step(&forward_batch)
            .expect("duplicate ordinals are ordinary rejections");
        let reversed_report = reversed_simulation
            .step(&reversed_batch)
            .expect("normalized duplicate ordinals are ordinary rejections");
        assert_eq!(forward_report, reversed_report);
        assert_eq!(forward_report.command_acceptances.len(), 1);
        assert_eq!(forward_report.command_rejections.len(), 2);
        assert!(
            forward_report
                .command_rejections
                .iter()
                .all(|rejection| { rejection.reason == CommandRejectionReason::DuplicateOrdinal })
        );
        assert_eq!(
            forward_simulation.state_hash(),
            reversed_simulation.state_hash()
        );
    }

    #[test]
    fn replay_shape_rejects_seed_checkpoint_and_command_boundary_errors() {
        let simulation = simulation();
        let mut header = simulation.replay_header();
        header.seed = Seed::from_hex(&format!("{}1", "0".repeat(63))).unwrap();
        assert_eq!(
            Replay::new(
                header,
                Vec::new(),
                vec![HashCheckpoint {
                    next_tick: Tick(0),
                    state_hash: header.initial_state_hash,
                }],
            ),
            Err(ReplayError::NonzeroEmptyWorldSeed)
        );

        let header = simulation.replay_header();
        assert!(matches!(
            Replay::new(header, Vec::new(), Vec::new()),
            Err(ReplayError::MissingInitialCheckpoint)
        ));
        assert!(matches!(
            Replay::new(
                header,
                vec![CommandEnvelope {
                    target_tick: Tick(1),
                    ordinal: 0,
                    command: Command::RemoveEntity(RemoveEntityCommand {
                        target: EntityId(1),
                    }),
                }],
                vec![
                    HashCheckpoint {
                        next_tick: Tick(0),
                        state_hash: header.initial_state_hash,
                    },
                    HashCheckpoint {
                        next_tick: Tick(1),
                        state_hash: header.initial_state_hash,
                    },
                ],
            ),
            Err(ReplayError::CommandOutsideRunBoundary { .. })
        ));
    }

    #[test]
    fn checkpoint_shape_sparse_trace_and_first_divergence_follow_next_tick_rules() {
        let mut recorder = simulation();
        let header = recorder.replay_header();
        let mut trace = vec![recorder.state_hash()];
        for _ in 0..4 {
            trace.push(
                recorder
                    .step(&[])
                    .expect("empty Tick records a State Hash")
                    .state_hash,
            );
        }

        assert_eq!(
            Replay::new(
                header,
                Vec::new(),
                vec![HashCheckpoint {
                    next_tick: Tick(1),
                    state_hash: header.initial_state_hash,
                }],
            ),
            Err(ReplayError::InitialCheckpointTick { actual: Tick(1) })
        );
        assert_eq!(
            Replay::new(
                header,
                Vec::new(),
                vec![HashCheckpoint {
                    next_tick: Tick(0),
                    state_hash: alternate_state_hash(),
                }],
            ),
            Err(ReplayError::InitialCheckpointHashMismatch {
                header: header.initial_state_hash,
                checkpoint: alternate_state_hash(),
            })
        );
        assert_eq!(
            Replay::new(
                header,
                Vec::new(),
                vec![
                    HashCheckpoint {
                        next_tick: Tick(0),
                        state_hash: trace[0],
                    },
                    HashCheckpoint {
                        next_tick: Tick(u64::MAX),
                        state_hash: trace[0],
                    },
                ],
            ),
            Err(ReplayError::TraceLengthOverflow)
        );

        for (checkpoints, expected) in [
            (
                vec![
                    HashCheckpoint {
                        next_tick: Tick(0),
                        state_hash: trace[0],
                    },
                    HashCheckpoint {
                        next_tick: Tick(2),
                        state_hash: trace[2],
                    },
                    HashCheckpoint {
                        next_tick: Tick(2),
                        state_hash: trace[2],
                    },
                ],
                ReplayError::CheckpointOrder {
                    previous: Tick(2),
                    actual: Tick(2),
                },
            ),
            (
                vec![
                    HashCheckpoint {
                        next_tick: Tick(0),
                        state_hash: trace[0],
                    },
                    HashCheckpoint {
                        next_tick: Tick(3),
                        state_hash: trace[3],
                    },
                    HashCheckpoint {
                        next_tick: Tick(2),
                        state_hash: trace[2],
                    },
                ],
                ReplayError::CheckpointOrder {
                    previous: Tick(3),
                    actual: Tick(2),
                },
            ),
        ] {
            assert_eq!(Replay::new(header, Vec::new(), checkpoints), Err(expected));
        }

        let sparse = Replay::new(
            header,
            Vec::new(),
            vec![
                HashCheckpoint {
                    next_tick: Tick(0),
                    state_hash: trace[0],
                },
                HashCheckpoint {
                    next_tick: Tick(2),
                    state_hash: trace[2],
                },
                HashCheckpoint {
                    next_tick: Tick(4),
                    state_hash: trace[4],
                },
            ],
        )
        .expect("sparse checkpoints are valid");
        assert_eq!(sparse.verify_trace(&trace), Ok(()));

        let divergent = Replay::new(
            header,
            Vec::new(),
            vec![
                HashCheckpoint {
                    next_tick: Tick(0),
                    state_hash: trace[0],
                },
                HashCheckpoint {
                    next_tick: Tick(2),
                    state_hash: alternate_state_hash(),
                },
                HashCheckpoint {
                    next_tick: Tick(4),
                    state_hash: alternate_state_hash(),
                },
            ],
        )
        .expect("divergent golden values do not change Replay shape");
        assert_eq!(
            divergent.verify_trace(&trace),
            Err(ReplayError::CheckpointDivergence {
                next_tick: Tick(2),
                expected: alternate_state_hash(),
                actual: trace[2],
            })
        );
    }

    #[test]
    fn header_and_trace_validation_report_first_exact_mismatch() {
        let mut simulation = simulation();
        let replay = empty_replay(&simulation, 1);
        assert_eq!(replay.validate_against(&simulation), Ok(()));
        let before = simulation.state_hash();
        let first_header = simulation.replay_header();
        let second_header = simulation.replay_header();
        assert_eq!(first_header, second_header);
        assert_eq!(simulation.state_hash(), before);
        assert!(matches!(
            replay.verify_trace(&[simulation.state_hash()]),
            Err(ReplayError::TraceLengthMismatch { .. })
        ));

        simulation.step(&[]).expect("empty Tick succeeds");
        assert_eq!(simulation.replay_header(), first_header);
        assert_eq!(
            replay.validate_against(&simulation),
            Err(ReplayError::ReplayRequiresFreshSimulation { actual: Tick(1) })
        );
    }

    #[test]
    fn profile_and_initial_hash_mismatches_report_exact_fields_without_mutation() {
        let simulation = simulation();
        let before_tick = simulation.next_tick();
        let before_hash = simulation.state_hash();
        let before_header = simulation.replay_header();

        let cases: [(ReplayContractField, HeaderMutation); 4] = [
            (ReplayContractField::NumericProfileHash, |header| {
                header.numeric_profile_hash = alternate_profile_hash();
            }),
            (ReplayContractField::PhysicalScaleProfileHash, |header| {
                header.physical_scale_profile_hash = alternate_profile_hash();
            }),
            (ReplayContractField::BalanceProfileHash, |header| {
                header.balance_profile_hash = alternate_profile_hash();
            }),
            (ReplayContractField::InitialStateHash, |header| {
                header.initial_state_hash = alternate_state_hash();
            }),
        ];

        for (expected_field, mutate) in cases {
            let mut header = before_header;
            mutate(&mut header);
            let replay = Replay::new(
                header,
                Vec::new(),
                vec![
                    HashCheckpoint {
                        next_tick: Tick(0),
                        state_hash: header.initial_state_hash,
                    },
                    HashCheckpoint {
                        next_tick: Tick(1),
                        state_hash: header.initial_state_hash,
                    },
                ],
            )
            .expect("mismatched Header still forms a valid Replay");

            let error = replay
                .validate_against(&simulation)
                .expect_err("mismatched Header must fail before Tick 0");
            assert!(
                matches!(
                    error,
                    ReplayError::ContractMismatch { field, .. } if field == expected_field
                ),
                "expected mismatch field {expected_field:?}, got {error:?}"
            );
            assert_eq!(simulation.next_tick(), before_tick);
            assert_eq!(simulation.state_hash(), before_hash);
            assert_eq!(simulation.replay_header(), before_header);
        }
    }

    #[test]
    fn replay_report_and_public_observation_reads_do_not_mutate_simulation() {
        let mut simulation = simulation();
        let header = simulation.replay_header();
        let initial_hash = simulation.state_hash();
        let half_extent = 32 * crate::FIXED_ONE;
        let bounds = FixedAabb::new(
            FixedVec2::new(Fixed(-half_extent), Fixed(-half_extent)),
            FixedVec2::new(Fixed(half_extent), Fixed(half_extent)),
        );
        let substrate_command = CommandEnvelope {
            target_tick: Tick(0),
            ordinal: 0,
            command: Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
                origin: FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
                routing_area: bounds,
                footprint: bounds,
            }),
        };
        let substrate_report = simulation
            .step(std::slice::from_ref(&substrate_command))
            .expect("observation fixture Substrate is valid");
        assert_eq!(substrate_report.command_acceptances.len(), 1);

        let gate_command = CommandEnvelope {
            target_tick: Tick(1),
            ordinal: 0,
            command: Command::PlaceGate(PlaceGateCommand {
                gate_type: GateType::Not,
                origin: FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
                routing_domain: RoutingDomain::FixedSubstrate(EntityId(1)),
            }),
        };
        let report = simulation
            .step(std::slice::from_ref(&gate_command))
            .expect("observation fixture Gate is valid");
        let gate = GateId(
            report.command_acceptances[0]
                .created_entity
                .expect("Gate placement returns its EntityId"),
        );
        let artifact = ReplayArtifact::new(
            "scenario.json",
            Replay::new(
                header,
                vec![substrate_command, gate_command],
                vec![
                    HashCheckpoint {
                        next_tick: Tick(0),
                        state_hash: initial_hash,
                    },
                    HashCheckpoint {
                        next_tick: Tick(2),
                        state_hash: report.state_hash,
                    },
                ],
            )
            .expect("observation Replay is valid"),
        )
        .expect("observation Replay locator is valid");
        let before_tick = simulation.next_tick();
        let before_hash = simulation.state_hash();

        let replay = artifact.replay();
        let _scenario_path = artifact.scenario_path();
        let _header = replay.header();
        let _commands = replay.commands();
        let _commands_at_tick = replay.commands_for_tick(Tick(0)).collect::<Vec<_>>();
        let _world_inputs = replay.world_inputs();
        let _checkpoints = replay.checkpoints();
        let _final_next_tick = replay.final_next_tick();
        let _report_observation = (
            report.completed_tick,
            report.next_tick,
            report.state_hash,
            &report.command_acceptances,
            &report.command_rejections,
            report.topology_changed,
            &report.driver_changes,
            &report.signal_changes,
            report.signal_counters,
        );

        let ports = simulation
            .gate_signal_ports(gate)
            .expect("Gate signal ports are observable");
        let _scenario_id = simulation.scenario_id();
        let _contract = simulation.contract();
        let _profiles = simulation.profiles();
        let _topology_revision = simulation.topology_revision();
        let _replay_header = simulation.replay_header();
        let _gate_state = simulation.gate_signal_state(gate);
        let _output_sample = simulation.driver_sample(ports.output);
        let _external_sample = simulation.driver_sample(ports.input_a.external_driver);
        let _sink_level = simulation.sink_level(ports.input_a.sink);
        let _sink_driver_sample =
            simulation.sink_driver_sample(ports.input_a.sink, ports.input_a.external_driver);
        let _wrong_kind_wire = simulation.wire_signal_state(WireId(gate.entity_id()));
        let mut render = RenderSnapshot::default();
        simulation.write_render_snapshot(&mut render);
        let _render_observation = (
            render.scenario_id(),
            render.next_tick(),
            render.primitive_count(),
            render.state_hash(),
        );

        assert_eq!(simulation.next_tick(), before_tick);
        assert_eq!(simulation.state_hash(), before_hash);
    }

    #[test]
    fn scenario_locator_is_portable_relative_metadata() {
        let simulation = simulation();
        let replay = empty_replay(&simulation, 1);
        assert!(ReplayArtifact::new("../scenarios/empty.json", replay.clone()).is_ok());
        for invalid in ["", "/absolute.json", "C:/windows.json", "dir\\file.json"] {
            assert_eq!(
                ReplayArtifact::new(invalid, replay.clone()),
                Err(ReplayError::InvalidScenarioPath)
            );
        }
    }

    #[test]
    fn replay_state_hash_version_tracks_canonical_v3() {
        assert_eq!(crate::canonical::STATE_ENCODER_VERSION, 3);
        assert_eq!(StateHashVersion::current(), StateHashVersion::V3);
        assert_eq!(StateHashVersion::current().as_str(), STATE_HASH_VERSION_V3);
    }
}
