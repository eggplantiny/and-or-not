#![forbid(unsafe_code)]

mod mobility_runtime;
mod topology_runtime;

pub use mobility_runtime::{
    MAX_MOBILITY_RUNTIME_INPUT_BYTES, MobilityRuntimeCoverage, MobilityRuntimeExecutionObservation,
    MobilityRuntimeObservation, MobilityRuntimeScenario, exercise_mobility_runtime,
};
pub use topology_runtime::{
    MAX_TOPOLOGY_RUNTIME_INPUT_BYTES, TopologyRuntimeCoverage, TopologyRuntimeExecutionObservation,
    TopologyRuntimeObservation, TopologyRuntimeScenario, exercise_topology_runtime,
};

use aon_sim::{
    ArtifactBytes, BindPortCommand, Command, CommandEncodingError, CommandEnvelope, DriveStrength,
    DriverId, DriverSample, EndpointTarget, EntityId, Fixed, FixedAabb, FixedVec2, GateId,
    GatePort, GatePortRef, GateSignalPorts, GateSignalSnapshot, GateType, GeometryError,
    JunctionId, LogicLevel, ModuleError, NumericError, PackageError, PlaceFixedSubstrateCommand,
    PlaceGateCommand, PlaceJunctionCommand, PlaceMobileSubstrateCommand, PlaceWireCommand,
    RemoveEntityCommand, RoutingDomain, SetExternalDriverCommand, Simulation, SimulationError,
    StateHash, StepReport, Tick, WireEnd, WireId, WireSignalSnapshot,
    decode_experiment_plan_artifact, decode_module_artifact, decode_package,
    decode_replay_artifact, decode_scenario_manifest, polyline_length, validate_quantized,
};
use std::fmt;

const REFERENCE_SCENARIO: &[u8] = include_bytes!("../../../fixtures/scenarios/empty.json");
const REFERENCE_NUMERIC_PROFILE: &[u8] = include_bytes!("../../../profiles/numeric/v1.json");
const REFERENCE_PHYSICAL_SCALE_PROFILE: &[u8] =
    include_bytes!("../../../profiles/physical-scale/stage0-alpha.json");
const REFERENCE_BALANCE_PROFILE: &[u8] =
    include_bytes!("../../../profiles/balance/stage0-alpha.json");

const STATEFUL_WORLD_PITCH: i64 = 65_536;
const STATEFUL_CIRCUIT_PITCH: i64 = 16_384;
const STATEFUL_SUBSTRATE_ID: EntityId = EntityId(1);
const STATEFUL_GATE_ID: EntityId = EntityId(2);
const STATEFUL_JUNCTION_ID: EntityId = EntityId(3);
const STATEFUL_TOMBSTONE_ID: EntityId = EntityId(4);
const STATEFUL_WIRE_ID: EntityId = EntityId(5);
const STATEFUL_BATCH_TICK: Tick = Tick(3);

const SIGNAL_SUBSTRATE_ID: EntityId = EntityId(1);
const SIGNAL_SOURCE_GATE_ID: GateId = GateId(EntityId(2));
const SIGNAL_TARGET_GATE_ID: GateId = GateId(EntityId(3));
const SIGNAL_REMOVED_GATE_ID: GateId = GateId(EntityId(4));
const SIGNAL_WIRE_ID: WireId = WireId(EntityId(5));
const SIGNAL_SETTLE_TICKS: usize = 8;

/// Maximum number of bytes interpreted by one decoder invocation.
pub const MAX_DECODER_INPUT_BYTES: usize = 16 * 1024;

/// Maximum number of bytes interpreted by one Replay decoder invocation.
pub const MAX_REPLAY_INPUT_BYTES: usize = MAX_DECODER_INPUT_BYTES;

/// Maximum number of bytes interpreted by one Experiment Plan decoder invocation.
pub const MAX_EXPERIMENT_INPUT_BYTES: usize = MAX_DECODER_INPUT_BYTES;

/// Maximum number of bytes interpreted by one Module decoder invocation.
pub const MAX_MODULE_INPUT_BYTES: usize = MAX_DECODER_INPUT_BYTES;

/// Maximum number of bytes interpreted by one geometry invocation.
pub const MAX_GEOMETRY_INPUT_BYTES: usize = 4 * 1024;

/// Maximum number of points constructed by one geometry invocation.
pub const MAX_POLYLINE_POINTS: usize = 64;

/// Maximum number of bytes interpreted by one command-batch invocation.
pub const MAX_COMMAND_INPUT_BYTES: usize = MAX_GEOMETRY_INPUT_BYTES;

/// Maximum number of envelopes constructed by one command-batch invocation.
pub const MAX_COMMAND_ENVELOPES: usize = 16;

/// Maximum number of raw vertices constructed for one arbitrary Wire command.
pub const MAX_COMMAND_WIRE_POINTS: usize = 8;

/// Maximum number of bytes, and therefore post-prefix Ticks, interpreted by the signal-runtime
/// target.
pub const MAX_SIGNAL_RUNTIME_INPUT_BYTES: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecoderTarget {
    Scenario,
    NumericProfile,
    PhysicalScaleProfile,
    BalanceProfile,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecoderObservation {
    pub target: DecoderTarget,
    pub payload_len: usize,
    pub result: Result<(), PackageError>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayDecoderObservation {
    pub payload_len: usize,
    pub result: Result<(), aon_sim::ReplayError>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExperimentDecoderObservation {
    pub payload_len: usize,
    pub result: Result<(), aon_sim::ExperimentArtifactError>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleDecoderObservation {
    pub payload_len: usize,
    pub result: Result<(), ModuleDecoderError>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModuleDecoderError {
    InvalidUtf8,
    Artifact(ModuleError),
}

impl fmt::Display for ModuleDecoderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUtf8 => formatter.write_str("Module artifact is not valid UTF-8"),
            Self::Artifact(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ModuleDecoderError {}

/// Supplies a bounded arbitrary byte stream to the strict versioned Replay decoder.
pub fn exercise_replay_decoder(input: &[u8]) -> ReplayDecoderObservation {
    let bounded = &input[..input.len().min(MAX_REPLAY_INPUT_BYTES)];
    ReplayDecoderObservation {
        payload_len: bounded.len(),
        result: decode_replay_artifact(bounded).map(|_| ()),
    }
}

/// Supplies a bounded arbitrary byte stream to the strict Experiment Plan v1 decoder.
pub fn exercise_experiment_decoder(input: &[u8]) -> ExperimentDecoderObservation {
    let bounded = &input[..input.len().min(MAX_EXPERIMENT_INPUT_BYTES)];
    ExperimentDecoderObservation {
        payload_len: bounded.len(),
        result: decode_experiment_plan_artifact(bounded).map(|_| ()),
    }
}

/// Supplies a bounded arbitrary byte stream to the strict Module v1 decoder.
///
/// The public Module decoder accepts text, so invalid UTF-8 is preserved as a stable boundary
/// rejection instead of being replaced before the decoder is reached.
pub fn exercise_module_decoder(input: &[u8]) -> ModuleDecoderObservation {
    let bounded = &input[..input.len().min(MAX_MODULE_INPUT_BYTES)];
    let result = std::str::from_utf8(bounded)
        .map_err(|_| ModuleDecoderError::InvalidUtf8)
        .and_then(|source| {
            decode_module_artifact(source)
                .map(|_| ())
                .map_err(ModuleDecoderError::Artifact)
        });
    ModuleDecoderObservation {
        payload_len: bounded.len(),
        result,
    }
}

/// Interprets an arbitrary byte stream as one of the four public artifact decode paths.
///
/// The low two bits of the first byte select the target. The remaining, bounded bytes are
/// supplied to that decoder. Profile targets use the checked-in reference artifacts for the
/// other package members so the selected profile decoder is always reached.
pub fn exercise_decoder(input: &[u8]) -> DecoderObservation {
    let bounded = &input[..input.len().min(MAX_DECODER_INPUT_BYTES)];
    let selector = bounded.first().copied().unwrap_or(0);
    let payload = bounded.get(1..).unwrap_or_default();
    let target = match selector & 0b11 {
        0 => DecoderTarget::Scenario,
        1 => DecoderTarget::NumericProfile,
        2 => DecoderTarget::PhysicalScaleProfile,
        _ => DecoderTarget::BalanceProfile,
    };

    let result = match target {
        DecoderTarget::Scenario => decode_scenario_manifest(payload).map(|_| ()),
        DecoderTarget::NumericProfile => decode_package(ArtifactBytes {
            scenario: REFERENCE_SCENARIO,
            numeric_profile: payload,
            physical_scale_profile: REFERENCE_PHYSICAL_SCALE_PROFILE,
            balance_profile: REFERENCE_BALANCE_PROFILE,
        })
        .map(|_| ()),
        DecoderTarget::PhysicalScaleProfile => decode_package(ArtifactBytes {
            scenario: REFERENCE_SCENARIO,
            numeric_profile: REFERENCE_NUMERIC_PROFILE,
            physical_scale_profile: payload,
            balance_profile: REFERENCE_BALANCE_PROFILE,
        })
        .map(|_| ()),
        DecoderTarget::BalanceProfile => decode_package(ArtifactBytes {
            scenario: REFERENCE_SCENARIO,
            numeric_profile: REFERENCE_NUMERIC_PROFILE,
            physical_scale_profile: REFERENCE_PHYSICAL_SCALE_PROFILE,
            balance_profile: payload,
        })
        .map(|_| ()),
    };

    DecoderObservation {
        target,
        payload_len: payload.len(),
        result,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeometryObservation {
    pub consumed_len: usize,
    pub quantum: Fixed,
    pub point_count: usize,
    pub validation_results: Vec<Result<(), GeometryError>>,
    pub length: Result<Fixed, NumericError>,
}

/// Maps an arbitrary byte stream to a bounded, quantized polyline and executes its validation
/// and canonical length paths.
///
/// Bytes are consumed cyclically, which makes even short inputs useful while keeping the mapping
/// deterministic. The quantum is a positive power of two up to `2^31`; coordinates are signed
/// 32-bit quantum multiples, so construction itself cannot overflow `i64`.
pub fn exercise_geometry(input: &[u8]) -> GeometryObservation {
    let bounded = &input[..input.len().min(MAX_GEOMETRY_INPUT_BYTES)];
    let mut cursor = CyclicBytes::new(bounded);
    let point_count = usize::from(cursor.next_u8()) % (MAX_POLYLINE_POINTS + 1);
    let quantum = Fixed(1_i64 << (cursor.next_u8() % 32));
    let mut points = Vec::with_capacity(point_count);

    for _ in 0..point_count {
        let x = i64::from(cursor.next_i32()) * quantum.0;
        let y = i64::from(cursor.next_i32()) * quantum.0;
        points.push(FixedVec2::new(Fixed(x), Fixed(y)));
    }

    let validation_results = points
        .iter()
        .copied()
        .map(|point| validate_quantized(point, quantum))
        .collect();
    let length = polyline_length(&points);

    GeometryObservation {
        consumed_len: bounded.len(),
        quantum,
        point_count,
        validation_results,
        length,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandEncodingObservation {
    pub allocated_result: Result<usize, CommandEncodingError>,
    pub streamed_result: Result<usize, CommandEncodingError>,
    pub bytes_match: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum CommandExecutionObservation {
    PackageRejected(PackageError),
    SimulationRejected(SimulationError),
    Stepped(Result<StepReport, SimulationError>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandObservation {
    pub consumed_len: usize,
    pub envelopes: Vec<CommandEnvelope>,
    pub variant_mask: u16,
    pub encodings: Vec<CommandEncodingObservation>,
    pub execution: CommandExecutionObservation,
}

impl CommandObservation {
    /// Returns a harness failure that must never be produced by bounded arbitrary input.
    pub fn invariant_failure(&self) -> Option<&'static str> {
        if self.encodings.iter().any(|encoding| !encoding.bytes_match) {
            return Some("allocated and streaming command encoders disagreed");
        }
        match &self.execution {
            CommandExecutionObservation::PackageRejected(_) => {
                Some("the embedded reference package was rejected")
            }
            CommandExecutionObservation::SimulationRejected(_) => {
                Some("the embedded reference simulation was rejected")
            }
            CommandExecutionObservation::Stepped(Err(SimulationError::NumericOverflow))
            | CommandExecutionObservation::Stepped(Ok(_)) => None,
            CommandExecutionObservation::Stepped(Err(_)) => {
                Some("arbitrary commands produced a non-numeric run error")
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StatefulCommandExecutionObservation {
    PackageRejected(PackageError),
    SimulationRejected(SimulationError),
    PrefixRunError {
        step_index: usize,
        error: SimulationError,
    },
    PrefixCommandRejected {
        step_index: usize,
        report: StepReport,
    },
    Stepped(Result<StepReport, SimulationError>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatefulCommandObservation {
    pub consumed_len: usize,
    pub envelopes: Vec<CommandEnvelope>,
    pub variant_mask: u16,
    pub encodings: Vec<CommandEncodingObservation>,
    pub prefix_reports: Vec<StepReport>,
    pub execution: StatefulCommandExecutionObservation,
}

impl StatefulCommandObservation {
    /// Returns a harness failure that must never be produced by bounded arbitrary input.
    pub fn invariant_failure(&self) -> Option<&'static str> {
        if self.encodings.iter().any(|encoding| !encoding.bytes_match) {
            return Some("allocated and streaming command encoders disagreed");
        }
        match &self.execution {
            StatefulCommandExecutionObservation::PackageRejected(_) => {
                Some("the embedded reference package was rejected")
            }
            StatefulCommandExecutionObservation::SimulationRejected(_) => {
                Some("the embedded reference simulation was rejected")
            }
            StatefulCommandExecutionObservation::PrefixRunError { .. } => {
                Some("the deterministic stateful prefix produced a run error")
            }
            StatefulCommandExecutionObservation::PrefixCommandRejected { .. } => {
                Some("the deterministic stateful prefix rejected a command")
            }
            StatefulCommandExecutionObservation::Stepped(Err(SimulationError::NumericOverflow))
            | StatefulCommandExecutionObservation::Stepped(Ok(_)) => None,
            StatefulCommandExecutionObservation::Stepped(Err(_)) => {
                Some("arbitrary stateful commands produced a non-numeric run error")
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SignalRuntimeCoverage {
    pub valid_external_updates: u64,
    pub removed_driver_attempts: u64,
    pub wrong_kind_driver_attempts: u64,
    pub predicted_driver_attempts: u64,
    pub simultaneous_update_batches: u64,
    pub simultaneous_driver_event_batches: u64,
    pub coalesced_update_batches: u64,
    pub permuted_insertion_batches: u64,
    pub max_strength_updates: u64,
    pub driver_transitions_applied: u64,
    pub signal_arrivals_applied: u64,
    pub sinks_resolved: u64,
    pub driver_changes: u64,
    pub signal_changes: u64,
    pub gate_output_changes: u64,
    pub wire_excitation_changes: u64,
    pub pending_gate_observations: u64,
    pub nonzero_wire_observations: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum SignalRuntimeExecutionObservation {
    PackageRejected(PackageError),
    SimulationRejected {
        replica: u8,
        error: SimulationError,
    },
    PrefixRunError {
        replica: u8,
        step_index: usize,
        error: SimulationError,
    },
    PrefixCommandRejected {
        replica: u8,
        step_index: usize,
        report: StepReport,
    },
    PrefixInvariantViolation {
        replica: u8,
        step_index: usize,
    },
    DeterminismMismatch {
        step_index: Option<usize>,
    },
    RunError {
        replica: u8,
        step_index: usize,
        error: SimulationError,
    },
    Completed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignalRuntimeObservation {
    pub consumed_len: usize,
    pub generated_steps: usize,
    pub prefix_reports: Vec<StepReport>,
    pub step_reports: Vec<StepReport>,
    pub state_hashes: Vec<StateHash>,
    pub encodings: Vec<CommandEncodingObservation>,
    pub expectation_failures: u64,
    pub coverage: SignalRuntimeCoverage,
    pub execution: SignalRuntimeExecutionObservation,
}

impl SignalRuntimeObservation {
    /// Returns a harness failure. The signal target has no intentionally fatal input: bounded
    /// Driver samples must not produce `NumericOverflow`, `InvalidCanonicalState`, or any other
    /// run error.
    pub fn invariant_failure(&self) -> Option<&'static str> {
        if self.encodings.iter().any(|encoding| !encoding.bytes_match) {
            return Some("allocated and streaming signal command encoders disagreed");
        }
        if self.expectation_failures != 0 {
            return Some("signal command outcome or public observation disagreed with its model");
        }
        match self.execution {
            SignalRuntimeExecutionObservation::PackageRejected(_) => {
                Some("the embedded reference package was rejected")
            }
            SignalRuntimeExecutionObservation::SimulationRejected { .. } => {
                Some("the embedded reference signal simulation was rejected")
            }
            SignalRuntimeExecutionObservation::PrefixRunError { .. } => {
                Some("the deterministic signal prefix produced a run error")
            }
            SignalRuntimeExecutionObservation::PrefixCommandRejected { .. } => {
                Some("the deterministic signal prefix rejected a command")
            }
            SignalRuntimeExecutionObservation::PrefixInvariantViolation { .. } => {
                Some("the deterministic signal prefix violated its public contract")
            }
            SignalRuntimeExecutionObservation::DeterminismMismatch { .. } => {
                Some("equivalent signal command streams produced different observations")
            }
            SignalRuntimeExecutionObservation::RunError { .. } => {
                Some("bounded signal commands produced an unexpected run error")
            }
            SignalRuntimeExecutionObservation::Completed => None,
        }
    }
}

/// Runs a bounded stateful S0-M3 signal stream against two independently built simulations.
///
/// Each byte selects one Tick containing a valid external update, a removed/wrong-kind/predicted
/// Driver attempt, a simultaneous pair, an ordinal-last coalescing pair, or no commands. The
/// second simulation receives multi-command batches in reverse insertion order. Canonical command
/// ordering requires both replicas to produce identical reports, public observations, and hashes.
pub fn exercise_signal_runtime(input: &[u8]) -> SignalRuntimeObservation {
    let bounded = &input[..input.len().min(MAX_SIGNAL_RUNTIME_INPUT_BYTES)];
    let mut observation = SignalRuntimeObservation {
        consumed_len: bounded.len(),
        generated_steps: 0,
        prefix_reports: Vec::new(),
        step_reports: Vec::with_capacity(bounded.len()),
        state_hashes: Vec::with_capacity(bounded.len()),
        encodings: Vec::new(),
        expectation_failures: 0,
        coverage: SignalRuntimeCoverage::default(),
        execution: SignalRuntimeExecutionObservation::Completed,
    };

    let package = match decode_package(ArtifactBytes {
        scenario: REFERENCE_SCENARIO,
        numeric_profile: REFERENCE_NUMERIC_PROFILE,
        physical_scale_profile: REFERENCE_PHYSICAL_SCALE_PROFILE,
        balance_profile: REFERENCE_BALANCE_PROFILE,
    }) {
        Ok(package) => package,
        Err(error) => {
            observation.execution = SignalRuntimeExecutionObservation::PackageRejected(error);
            return observation;
        }
    };

    let left = match Simulation::new(package.clone()) {
        Ok(simulation) => simulation,
        Err(error) => {
            observation.execution =
                SignalRuntimeExecutionObservation::SimulationRejected { replica: 0, error };
            return observation;
        }
    };
    let right = match Simulation::new(package) {
        Ok(simulation) => simulation,
        Err(error) => {
            observation.execution =
                SignalRuntimeExecutionObservation::SimulationRejected { replica: 1, error };
            return observation;
        }
    };

    let mut left = match build_signal_fixture(left) {
        Ok(fixture) => fixture,
        Err(failure) => {
            observation.execution = failure.into_observation(0);
            return observation;
        }
    };
    let mut right = match build_signal_fixture(right) {
        Ok(fixture) => fixture,
        Err(failure) => {
            observation.execution = failure.into_observation(1);
            return observation;
        }
    };
    observation.prefix_reports.clone_from(&left.prefix_reports);

    if left.ids != right.ids
        || left.prefix_reports != right.prefix_reports
        || left.simulation.state_hash() != right.simulation.state_hash()
        || signal_public_snapshot(&left.simulation, left.ids)
            != signal_public_snapshot(&right.simulation, right.ids)
    {
        observation.execution =
            SignalRuntimeExecutionObservation::DeterminismMismatch { step_index: None };
        return observation;
    }

    let Some(mut previous_snapshot) = signal_public_snapshot(&left.simulation, left.ids) else {
        observation.execution = SignalRuntimeExecutionObservation::PrefixInvariantViolation {
            replica: 0,
            step_index: left.prefix_reports.len(),
        };
        return observation;
    };

    for (step_index, &selector) in bounded.iter().enumerate() {
        let batch = signal_runtime_batch(selector, left.simulation.next_tick(), left.ids);
        observation.generated_steps += 1;
        observation
            .encodings
            .extend(batch.envelopes.iter().map(exercise_command_encoding));
        record_signal_batch_intent(&mut observation.coverage, &batch);

        let left_report = match left.simulation.step(&batch.envelopes) {
            Ok(report) => report,
            Err(error) => {
                observation.execution = SignalRuntimeExecutionObservation::RunError {
                    replica: 0,
                    step_index,
                    error,
                };
                return observation;
            }
        };
        let mut permuted = batch.envelopes.clone();
        if permuted.len() > 1 {
            permuted.reverse();
            observation.coverage.permuted_insertion_batches += 1;
        }
        let right_report = match right.simulation.step(&permuted) {
            Ok(report) => report,
            Err(error) => {
                observation.execution = SignalRuntimeExecutionObservation::RunError {
                    replica: 1,
                    step_index,
                    error,
                };
                return observation;
            }
        };

        let left_snapshot = signal_public_snapshot(&left.simulation, left.ids);
        let right_snapshot = signal_public_snapshot(&right.simulation, right.ids);
        if left_report != right_report
            || left_report.state_hash != left.simulation.state_hash()
            || right_report.state_hash != right.simulation.state_hash()
            || left_snapshot != right_snapshot
        {
            observation.execution = SignalRuntimeExecutionObservation::DeterminismMismatch {
                step_index: Some(step_index),
            };
            return observation;
        }

        let Some(current_snapshot) = left_snapshot else {
            observation.expectation_failures += 1;
            observation.execution = SignalRuntimeExecutionObservation::PrefixInvariantViolation {
                replica: 0,
                step_index: left.prefix_reports.len() + step_index,
            };
            return observation;
        };
        if !signal_report_matches(&left_report, &batch)
            || !signal_samples_match(&current_snapshot, &batch)
        {
            observation.expectation_failures += 1;
        }
        record_signal_step_coverage(
            &mut observation.coverage,
            &batch,
            &left_report,
            &previous_snapshot,
            &current_snapshot,
        );
        previous_snapshot = current_snapshot;
        observation.state_hashes.push(left_report.state_hash);
        observation.step_reports.push(left_report);
    }

    observation
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SignalFixtureIds {
    source: GateSignalPorts,
    target: GateSignalPorts,
    removed: GateSignalPorts,
    predicted_driver: DriverId,
    wire: WireId,
}

struct SignalFixture {
    simulation: Simulation,
    ids: SignalFixtureIds,
    prefix_reports: Vec<StepReport>,
}

enum SignalPrefixFailure {
    Run {
        step_index: usize,
        error: SimulationError,
    },
    CommandRejected {
        step_index: usize,
        report: Box<StepReport>,
    },
    Invariant {
        step_index: usize,
    },
}

impl SignalPrefixFailure {
    fn into_observation(self, replica: u8) -> SignalRuntimeExecutionObservation {
        match self {
            Self::Run { step_index, error } => SignalRuntimeExecutionObservation::PrefixRunError {
                replica,
                step_index,
                error,
            },
            Self::CommandRejected { step_index, report } => {
                SignalRuntimeExecutionObservation::PrefixCommandRejected {
                    replica,
                    step_index,
                    report: *report,
                }
            }
            Self::Invariant { step_index } => {
                SignalRuntimeExecutionObservation::PrefixInvariantViolation {
                    replica,
                    step_index,
                }
            }
        }
    }
}

fn build_signal_fixture(mut simulation: Simulation) -> Result<SignalFixture, SignalPrefixFailure> {
    let mut prefix_reports = Vec::with_capacity(4 + SIGNAL_SETTLE_TICKS);
    let substrate_bounds = FixedAabb::new(
        signal_point(-32 * STATEFUL_WORLD_PITCH, -32 * STATEFUL_WORLD_PITCH),
        signal_point(32 * STATEFUL_WORLD_PITCH, 32 * STATEFUL_WORLD_PITCH),
    );
    let domain = RoutingDomain::FixedSubstrate(SIGNAL_SUBSTRATE_ID);

    run_signal_prefix_step(
        &mut simulation,
        vec![Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
            origin: signal_point(0, 0),
            routing_area: substrate_bounds,
            footprint: substrate_bounds,
        })],
        &mut prefix_reports,
    )?;
    if prefix_created_entities(&prefix_reports[0]) != vec![SIGNAL_SUBSTRATE_ID] {
        return Err(SignalPrefixFailure::Invariant { step_index: 0 });
    }

    run_signal_prefix_step(
        &mut simulation,
        vec![
            Command::PlaceGate(PlaceGateCommand {
                gate_type: GateType::Not,
                origin: signal_point(0, 0),
                routing_domain: domain,
            }),
            Command::PlaceGate(PlaceGateCommand {
                gate_type: GateType::Not,
                origin: signal_point(34 * STATEFUL_CIRCUIT_PITCH, 0),
                routing_domain: domain,
            }),
            Command::PlaceGate(PlaceGateCommand {
                gate_type: GateType::Not,
                origin: signal_point(0, 16 * STATEFUL_WORLD_PITCH),
                routing_domain: domain,
            }),
        ],
        &mut prefix_reports,
    )?;
    if prefix_created_entities(&prefix_reports[1])
        != vec![
            SIGNAL_SOURCE_GATE_ID.entity_id(),
            SIGNAL_TARGET_GATE_ID.entity_id(),
            SIGNAL_REMOVED_GATE_ID.entity_id(),
        ]
    {
        return Err(SignalPrefixFailure::Invariant { step_index: 1 });
    }
    let source = simulation
        .gate_signal_ports(SIGNAL_SOURCE_GATE_ID)
        .ok_or(SignalPrefixFailure::Invariant { step_index: 1 })?;
    let target = simulation
        .gate_signal_ports(SIGNAL_TARGET_GATE_ID)
        .ok_or(SignalPrefixFailure::Invariant { step_index: 1 })?;
    let removed = simulation
        .gate_signal_ports(SIGNAL_REMOVED_GATE_ID)
        .ok_or(SignalPrefixFailure::Invariant { step_index: 1 })?;
    let predicted_driver = DriverId(EntityId(
        removed
            .output
            .entity_id()
            .0
            .checked_add(1)
            .ok_or(SignalPrefixFailure::Invariant { step_index: 1 })?,
    ));

    run_signal_prefix_step(
        &mut simulation,
        vec![Command::PlaceWire(PlaceWireCommand {
            routing_domain: domain,
            points: vec![
                signal_point(STATEFUL_CIRCUIT_PITCH, 0),
                signal_point(33 * STATEFUL_CIRCUIT_PITCH, 0),
            ],
            endpoint_a: EndpointTarget::GatePort(GatePortRef {
                gate: SIGNAL_SOURCE_GATE_ID,
                port: GatePort::Output,
            }),
            endpoint_b: EndpointTarget::GatePort(GatePortRef {
                gate: SIGNAL_TARGET_GATE_ID,
                port: GatePort::InputA,
            }),
        })],
        &mut prefix_reports,
    )?;
    if prefix_created_entities(&prefix_reports[2]) != vec![SIGNAL_WIRE_ID.entity_id()]
        || simulation.wire_signal_state(SIGNAL_WIRE_ID).is_none()
    {
        return Err(SignalPrefixFailure::Invariant { step_index: 2 });
    }

    run_signal_prefix_step(
        &mut simulation,
        vec![Command::RemoveEntity(RemoveEntityCommand {
            target: SIGNAL_REMOVED_GATE_ID.entity_id(),
        })],
        &mut prefix_reports,
    )?;
    if simulation
        .gate_signal_ports(SIGNAL_REMOVED_GATE_ID)
        .is_some()
        || simulation
            .driver_sample(removed.input_a.external_driver)
            .is_some()
        || simulation.driver_sample(removed.output).is_some()
    {
        return Err(SignalPrefixFailure::Invariant { step_index: 3 });
    }

    for _ in 0..SIGNAL_SETTLE_TICKS {
        run_signal_prefix_step(&mut simulation, Vec::new(), &mut prefix_reports)?;
    }

    let ids = SignalFixtureIds {
        source,
        target,
        removed,
        predicted_driver,
        wire: SIGNAL_WIRE_ID,
    };
    if signal_public_snapshot(&simulation, ids).is_none() {
        return Err(SignalPrefixFailure::Invariant {
            step_index: prefix_reports.len(),
        });
    }
    Ok(SignalFixture {
        simulation,
        ids,
        prefix_reports,
    })
}

fn run_signal_prefix_step(
    simulation: &mut Simulation,
    commands: Vec<Command>,
    prefix_reports: &mut Vec<StepReport>,
) -> Result<(), SignalPrefixFailure> {
    let step_index = prefix_reports.len();
    let tick = simulation.next_tick();
    let envelopes: Vec<_> = commands
        .into_iter()
        .enumerate()
        .map(|(ordinal, command)| CommandEnvelope {
            target_tick: tick,
            ordinal: ordinal as u64,
            command,
        })
        .collect();
    let report = simulation
        .step(&envelopes)
        .map_err(|error| SignalPrefixFailure::Run { step_index, error })?;
    if !report.command_rejections.is_empty() || report.command_acceptances.len() != envelopes.len()
    {
        return Err(SignalPrefixFailure::CommandRejected {
            step_index,
            report: Box::new(report),
        });
    }
    prefix_reports.push(report);
    Ok(())
}

fn prefix_created_entities(report: &StepReport) -> Vec<EntityId> {
    report
        .command_acceptances
        .iter()
        .filter_map(|acceptance| acceptance.created_entity)
        .collect()
}

const fn signal_point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SignalPublicSnapshot {
    source_gate: GateSignalSnapshot,
    target_gate: GateSignalSnapshot,
    wire: WireSignalSnapshot,
    source_external: DriverSample,
    target_external: DriverSample,
    source_output: DriverSample,
    target_output: DriverSample,
    source_input: LogicLevel,
    target_input: LogicLevel,
}

fn signal_public_snapshot(
    simulation: &Simulation,
    ids: SignalFixtureIds,
) -> Option<SignalPublicSnapshot> {
    Some(SignalPublicSnapshot {
        source_gate: simulation.gate_signal_state(SIGNAL_SOURCE_GATE_ID)?,
        target_gate: simulation.gate_signal_state(SIGNAL_TARGET_GATE_ID)?,
        wire: simulation.wire_signal_state(ids.wire)?,
        source_external: simulation.driver_sample(ids.source.input_a.external_driver)?,
        target_external: simulation.driver_sample(ids.target.input_a.external_driver)?,
        source_output: simulation.driver_sample(ids.source.output)?,
        target_output: simulation.driver_sample(ids.target.output)?,
        source_input: simulation.sink_level(ids.source.input_a.sink)?,
        target_input: simulation.sink_level(ids.target.input_a.sink)?,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExpectedSignalCommand {
    Accepted,
    Rejected(aon_sim::CommandRejectionReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SignalBatchKind {
    Valid,
    Removed,
    WrongKind,
    Predicted,
    Simultaneous,
    Coalesced,
    Empty,
}

struct SignalRuntimeBatch {
    envelopes: Vec<CommandEnvelope>,
    expected: Vec<(u64, ExpectedSignalCommand)>,
    expected_live_samples: Vec<(DriverId, LogicLevel, DriveStrength)>,
    kind: SignalBatchKind,
}

fn signal_runtime_batch(
    selector: u8,
    target_tick: Tick,
    ids: SignalFixtureIds,
) -> SignalRuntimeBatch {
    let kind = selector & 0b111;
    let level = signal_level(selector);
    let next_level = rotate_signal_level(level);
    let strength = signal_strength(selector);
    let mut envelopes = Vec::with_capacity(2);
    let mut expected = Vec::with_capacity(2);
    let mut expected_live_samples = Vec::with_capacity(2);

    let mut push = |ordinal: u64,
                    driver: DriverId,
                    level: LogicLevel,
                    strength: DriveStrength,
                    outcome: ExpectedSignalCommand| {
        envelopes.push(CommandEnvelope {
            target_tick,
            ordinal,
            command: Command::SetExternalDriver(SetExternalDriverCommand {
                driver,
                level,
                strength,
            }),
        });
        expected.push((ordinal, outcome));
    };

    let batch_kind = match kind {
        0 => {
            push(
                0,
                ids.source.input_a.external_driver,
                level,
                strength,
                ExpectedSignalCommand::Accepted,
            );
            expected_live_samples.push((ids.source.input_a.external_driver, level, strength));
            SignalBatchKind::Valid
        }
        1 => {
            push(
                0,
                ids.target.input_a.external_driver,
                level,
                strength,
                ExpectedSignalCommand::Accepted,
            );
            expected_live_samples.push((ids.target.input_a.external_driver, level, strength));
            SignalBatchKind::Valid
        }
        2 => {
            push(
                0,
                ids.removed.input_a.external_driver,
                level,
                strength,
                ExpectedSignalCommand::Rejected(aon_sim::CommandRejectionReason::RemovedDriver),
            );
            SignalBatchKind::Removed
        }
        3 => {
            push(
                0,
                ids.source.output,
                level,
                strength,
                ExpectedSignalCommand::Rejected(aon_sim::CommandRejectionReason::InvalidDriverKind),
            );
            SignalBatchKind::WrongKind
        }
        4 => {
            push(
                0,
                ids.predicted_driver,
                level,
                strength,
                ExpectedSignalCommand::Rejected(aon_sim::CommandRejectionReason::UnknownDriver),
            );
            SignalBatchKind::Predicted
        }
        5 => {
            push(
                1,
                ids.source.input_a.external_driver,
                level,
                strength,
                ExpectedSignalCommand::Accepted,
            );
            push(
                0,
                ids.target.input_a.external_driver,
                next_level,
                strength,
                ExpectedSignalCommand::Accepted,
            );
            expected_live_samples.push((ids.source.input_a.external_driver, level, strength));
            expected_live_samples.push((ids.target.input_a.external_driver, next_level, strength));
            SignalBatchKind::Simultaneous
        }
        6 => {
            push(
                0,
                ids.source.input_a.external_driver,
                level,
                strength,
                ExpectedSignalCommand::Accepted,
            );
            push(
                1,
                ids.source.input_a.external_driver,
                next_level,
                strength,
                ExpectedSignalCommand::Accepted,
            );
            expected_live_samples.push((ids.source.input_a.external_driver, next_level, strength));
            SignalBatchKind::Coalesced
        }
        _ => SignalBatchKind::Empty,
    };

    SignalRuntimeBatch {
        envelopes,
        expected,
        expected_live_samples,
        kind: batch_kind,
    }
}

const fn signal_level(selector: u8) -> LogicLevel {
    match (selector >> 3) % 3 {
        0 => LogicLevel::Low,
        1 => LogicLevel::High,
        _ => LogicLevel::X,
    }
}

const fn rotate_signal_level(level: LogicLevel) -> LogicLevel {
    match level {
        LogicLevel::Low => LogicLevel::High,
        LogicLevel::High => LogicLevel::X,
        LogicLevel::X => LogicLevel::Low,
    }
}

const fn signal_strength(selector: u8) -> DriveStrength {
    DriveStrength(match selector >> 5 {
        0 => 0,
        1 => 99,
        2 => 100,
        _ => u64::MAX,
    })
}

fn signal_report_matches(report: &StepReport, batch: &SignalRuntimeBatch) -> bool {
    if report.command_acceptances.len() + report.command_rejections.len() != batch.expected.len() {
        return false;
    }
    batch
        .expected
        .iter()
        .all(|&(ordinal, outcome)| match outcome {
            ExpectedSignalCommand::Accepted => {
                report.command_acceptances.iter().any(|acceptance| {
                    acceptance.target_tick == report.completed_tick
                        && acceptance.ordinal == ordinal
                        && acceptance.created_entity.is_none()
                })
            }
            ExpectedSignalCommand::Rejected(reason) => {
                report.command_rejections.iter().any(|rejection| {
                    rejection.target_tick == report.completed_tick
                        && rejection.ordinal == ordinal
                        && rejection.reason == reason
                })
            }
        })
}

fn signal_samples_match(snapshot: &SignalPublicSnapshot, batch: &SignalRuntimeBatch) -> bool {
    batch
        .expected_live_samples
        .iter()
        .all(|&(driver, level, strength)| {
            let sample = if driver == snapshot.source_external.driver_id {
                snapshot.source_external
            } else if driver == snapshot.target_external.driver_id {
                snapshot.target_external
            } else {
                return false;
            };
            sample.level == level && sample.strength == strength
        })
}

fn record_signal_batch_intent(coverage: &mut SignalRuntimeCoverage, batch: &SignalRuntimeBatch) {
    match batch.kind {
        SignalBatchKind::Valid => coverage.valid_external_updates += 1,
        SignalBatchKind::Removed => coverage.removed_driver_attempts += 1,
        SignalBatchKind::WrongKind => coverage.wrong_kind_driver_attempts += 1,
        SignalBatchKind::Predicted => coverage.predicted_driver_attempts += 1,
        SignalBatchKind::Simultaneous => {
            coverage.valid_external_updates += 2;
            coverage.simultaneous_update_batches += 1;
        }
        SignalBatchKind::Coalesced => {
            coverage.valid_external_updates += 2;
            coverage.coalesced_update_batches += 1;
        }
        SignalBatchKind::Empty => {}
    }
    coverage.max_strength_updates += batch
        .expected_live_samples
        .iter()
        .filter(|(_, _, strength)| strength.0 == u64::MAX)
        .count() as u64;
}

fn record_signal_step_coverage(
    coverage: &mut SignalRuntimeCoverage,
    batch: &SignalRuntimeBatch,
    report: &StepReport,
    previous: &SignalPublicSnapshot,
    current: &SignalPublicSnapshot,
) {
    coverage.driver_transitions_applied += report.signal_counters.driver_transitions_applied;
    coverage.signal_arrivals_applied += report.signal_counters.signal_arrivals_applied;
    coverage.sinks_resolved += report.signal_counters.sinks_resolved;
    coverage.driver_changes += report.driver_changes.len() as u64;
    coverage.signal_changes += report.signal_changes.len() as u64;
    if batch.kind == SignalBatchKind::Simultaneous && report.driver_changes.len() >= 2 {
        coverage.simultaneous_driver_event_batches += 1;
    }
    if previous.source_gate.current_output != current.source_gate.current_output {
        coverage.gate_output_changes += 1;
    }
    if previous.target_gate.current_output != current.target_gate.current_output {
        coverage.gate_output_changes += 1;
    }
    if previous.wire != current.wire {
        coverage.wire_excitation_changes += 1;
    }
    if current.source_gate.pending_due_tick.is_some()
        || current.target_gate.pending_due_tick.is_some()
    {
        coverage.pending_gate_observations += 1;
    }
    if current.wire.active.high != 0
        || current.wire.active.low != 0
        || current.wire.active.unknown != 0
    {
        coverage.nonzero_wire_observations += 1;
    }
}

#[derive(Clone, Copy)]
struct CommandFuzzContext {
    valid_tick: Tick,
    seeded_entities: bool,
}

struct ArbitraryCommandBatch {
    consumed_len: usize,
    envelopes: Vec<CommandEnvelope>,
    variant_mask: u16,
    encodings: Vec<CommandEncodingObservation>,
}

/// Maps arbitrary bytes to a bounded deterministic S0-M2 command batch, canonically encodes every
/// envelope through both public encoding paths, and submits the batch to a fresh Simulation.
///
/// The first bounded byte selects the envelope count. Remaining bytes are consumed cyclically.
/// Each command index rotates the byte-selected tag so short inputs can still cover all eight
/// command variants. Tick, ordinal, EntityId, coordinate, domain, endpoint, and point-count fields
/// deliberately include valid, reserved, extreme, duplicate, and malformed representations.
pub fn exercise_commands(input: &[u8]) -> CommandObservation {
    let batch = arbitrary_command_batch(
        input,
        CommandFuzzContext {
            valid_tick: Tick(0),
            seeded_entities: false,
        },
    );
    let execution = match decode_package(ArtifactBytes {
        scenario: REFERENCE_SCENARIO,
        numeric_profile: REFERENCE_NUMERIC_PROFILE,
        physical_scale_profile: REFERENCE_PHYSICAL_SCALE_PROFILE,
        balance_profile: REFERENCE_BALANCE_PROFILE,
    }) {
        Ok(package) => match Simulation::new(package) {
            Ok(mut simulation) => {
                CommandExecutionObservation::Stepped(simulation.step(&batch.envelopes))
            }
            Err(error) => CommandExecutionObservation::SimulationRejected(error),
        },
        Err(error) => CommandExecutionObservation::PackageRejected(error),
    };

    CommandObservation {
        consumed_len: batch.consumed_len,
        envelopes: batch.envelopes,
        variant_mask: batch.variant_mask,
        encodings: batch.encodings,
        execution,
    }
}

/// Executes the arbitrary command batch after constructing a deterministic three-Tick structural
/// prefix entirely through the public command API.
///
/// The prefix leaves a live Fixed Substrate, Gate, Junction, and explicitly bound Wire, plus one
/// tombstoned EntityId. Stateful EntityId selection can therefore reach successful and rejected
/// Bind/Remove operations, wrong-kind references, Fixed-domain validation, and explicit endpoint
/// validation instead of stopping every reference at an empty world's allocation frontier.
pub fn exercise_stateful_commands(input: &[u8]) -> StatefulCommandObservation {
    let batch = arbitrary_command_batch(
        input,
        CommandFuzzContext {
            valid_tick: STATEFUL_BATCH_TICK,
            seeded_entities: true,
        },
    );
    let mut prefix_reports = Vec::new();
    let execution = match decode_package(ArtifactBytes {
        scenario: REFERENCE_SCENARIO,
        numeric_profile: REFERENCE_NUMERIC_PROFILE,
        physical_scale_profile: REFERENCE_PHYSICAL_SCALE_PROFILE,
        balance_profile: REFERENCE_BALANCE_PROFILE,
    }) {
        Ok(package) => match Simulation::new(package) {
            Ok(mut simulation) => {
                let mut prefix_failure = None;
                for (step_index, commands) in stateful_prefix_batches().into_iter().enumerate() {
                    match simulation.step(&commands) {
                        Ok(report) if report.command_rejections.is_empty() => {
                            prefix_reports.push(report);
                        }
                        Ok(report) => {
                            prefix_failure =
                                Some(StatefulCommandExecutionObservation::PrefixCommandRejected {
                                    step_index,
                                    report,
                                });
                            break;
                        }
                        Err(error) => {
                            prefix_failure =
                                Some(StatefulCommandExecutionObservation::PrefixRunError {
                                    step_index,
                                    error,
                                });
                            break;
                        }
                    }
                }
                prefix_failure.unwrap_or_else(|| {
                    StatefulCommandExecutionObservation::Stepped(simulation.step(&batch.envelopes))
                })
            }
            Err(error) => StatefulCommandExecutionObservation::SimulationRejected(error),
        },
        Err(error) => StatefulCommandExecutionObservation::PackageRejected(error),
    };

    StatefulCommandObservation {
        consumed_len: batch.consumed_len,
        envelopes: batch.envelopes,
        variant_mask: batch.variant_mask,
        encodings: batch.encodings,
        prefix_reports,
        execution,
    }
}

fn arbitrary_command_batch(input: &[u8], context: CommandFuzzContext) -> ArbitraryCommandBatch {
    let bounded = &input[..input.len().min(MAX_COMMAND_INPUT_BYTES)];
    let command_count =
        bounded.first().copied().map(usize::from).unwrap_or(0) % (MAX_COMMAND_ENVELOPES + 1);
    let mut cursor = CyclicBytes::new(bounded.get(1..).unwrap_or_default());
    let mut envelopes = Vec::with_capacity(command_count);
    let mut variant_mask = 0_u16;

    for index in 0..command_count {
        let target_tick = arbitrary_tick(context.valid_tick, &mut cursor);
        let ordinal = arbitrary_ordinal(index, &mut cursor);
        let (tag, command) = arbitrary_command(index, &mut cursor, context);
        variant_mask |= 1_u16 << tag;
        envelopes.push(CommandEnvelope {
            target_tick,
            ordinal,
            command,
        });
    }

    let encodings = envelopes.iter().map(exercise_command_encoding).collect();
    ArbitraryCommandBatch {
        consumed_len: bounded.len(),
        envelopes,
        variant_mask,
        encodings,
    }
}

fn stateful_prefix_batches() -> [Vec<CommandEnvelope>; 3] {
    let substrate_bounds = FixedAabb::new(
        stateful_point(-4 * STATEFUL_WORLD_PITCH, -4 * STATEFUL_WORLD_PITCH),
        stateful_point(4 * STATEFUL_WORLD_PITCH, 4 * STATEFUL_WORLD_PITCH),
    );
    let domain = RoutingDomain::FixedSubstrate(STATEFUL_SUBSTRATE_ID);

    [
        vec![stateful_envelope(
            0,
            0,
            Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
                origin: stateful_point(0, 0),
                routing_area: substrate_bounds,
                footprint: substrate_bounds,
            }),
        )],
        vec![
            stateful_envelope(
                1,
                0,
                Command::PlaceGate(PlaceGateCommand {
                    gate_type: GateType::And,
                    origin: stateful_point(0, 0),
                    routing_domain: domain,
                }),
            ),
            stateful_envelope(
                1,
                1,
                Command::PlaceJunction(PlaceJunctionCommand {
                    routing_domain: domain,
                    position: stateful_point(2 * STATEFUL_WORLD_PITCH, 0),
                }),
            ),
            stateful_envelope(
                1,
                2,
                Command::PlaceJunction(PlaceJunctionCommand {
                    routing_domain: domain,
                    position: stateful_point(-2 * STATEFUL_WORLD_PITCH, 2 * STATEFUL_WORLD_PITCH),
                }),
            ),
        ],
        vec![
            stateful_envelope(
                2,
                0,
                Command::RemoveEntity(RemoveEntityCommand {
                    target: STATEFUL_TOMBSTONE_ID,
                }),
            ),
            stateful_envelope(
                2,
                1,
                Command::PlaceWire(PlaceWireCommand {
                    routing_domain: domain,
                    points: vec![
                        stateful_point(STATEFUL_CIRCUIT_PITCH, 0),
                        stateful_point(2 * STATEFUL_WORLD_PITCH, 0),
                    ],
                    endpoint_a: EndpointTarget::GatePort(GatePortRef {
                        gate: GateId(STATEFUL_GATE_ID),
                        port: GatePort::Output,
                    }),
                    endpoint_b: EndpointTarget::Junction(JunctionId(STATEFUL_JUNCTION_ID)),
                }),
            ),
        ],
    ]
}

fn stateful_envelope(tick: u64, ordinal: u64, command: Command) -> CommandEnvelope {
    CommandEnvelope {
        target_tick: Tick(tick),
        ordinal,
        command,
    }
}

const fn stateful_point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

fn exercise_command_encoding(envelope: &CommandEnvelope) -> CommandEncodingObservation {
    let allocated = envelope.canonical_bytes();
    let mut streamed = Vec::new();
    let streamed_result = envelope
        .write_canonical(&mut |bytes| streamed.extend_from_slice(bytes))
        .map(|()| streamed.len());
    let bytes_match = match (&allocated, &streamed_result) {
        (Ok(bytes), Ok(_)) => bytes == &streamed,
        (Err(allocated_error), Err(streamed_error)) => allocated_error == streamed_error,
        _ => false,
    };

    CommandEncodingObservation {
        allocated_result: allocated.map(|bytes| bytes.len()),
        streamed_result,
        bytes_match,
    }
}

fn arbitrary_command(
    index: usize,
    cursor: &mut CyclicBytes<'_>,
    context: CommandFuzzContext,
) -> (u8, Command) {
    let tag = cursor.next_u8().wrapping_add(index as u8) & 0b111;
    let command = match tag {
        0 => Command::PlaceGate(PlaceGateCommand {
            gate_type: arbitrary_gate_type(cursor),
            origin: arbitrary_point(cursor),
            routing_domain: arbitrary_routing_domain(cursor, context),
        }),
        1 => {
            let routing_domain = arbitrary_routing_domain(cursor, context);
            let point_count =
                usize::from(cursor.next_u8()) % (MAX_COMMAND_WIRE_POINTS.saturating_add(1));
            let points = (0..point_count).map(|_| arbitrary_point(cursor)).collect();
            Command::PlaceWire(PlaceWireCommand {
                routing_domain,
                points,
                endpoint_a: arbitrary_endpoint(cursor, context),
                endpoint_b: arbitrary_endpoint(cursor, context),
            })
        }
        2 => Command::PlaceJunction(PlaceJunctionCommand {
            routing_domain: arbitrary_routing_domain(cursor, context),
            position: arbitrary_point(cursor),
        }),
        3 => Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
            origin: arbitrary_point(cursor),
            routing_area: arbitrary_aabb(cursor),
            footprint: arbitrary_aabb(cursor),
        }),
        4 => Command::PlaceMobileSubstrate(PlaceMobileSubstrateCommand {
            origin: arbitrary_point(cursor),
            routing_area: arbitrary_aabb(cursor),
            footprint: arbitrary_aabb(cursor),
        }),
        5 => Command::RemoveEntity(RemoveEntityCommand {
            target: arbitrary_entity_id(cursor, context),
        }),
        6 => Command::BindPort(BindPortCommand {
            wire: WireId(arbitrary_entity_id(cursor, context)),
            end: match cursor.next_u8() & 1 {
                0 => WireEnd::A,
                _ => WireEnd::B,
            },
            target: arbitrary_endpoint(cursor, context),
        }),
        _ => Command::SetExternalDriver(SetExternalDriverCommand {
            driver: DriverId(arbitrary_entity_id(cursor, context)),
            level: match cursor.next_u8() % 3 {
                0 => LogicLevel::Low,
                1 => LogicLevel::High,
                _ => LogicLevel::X,
            },
            strength: DriveStrength(cursor.next_u64()),
        }),
    };
    (tag, command)
}

fn arbitrary_tick(valid_tick: Tick, cursor: &mut CyclicBytes<'_>) -> Tick {
    let mode = cursor.next_u8() & 0b11;
    let raw = cursor.next_u64();
    Tick(match mode {
        0 => valid_tick.0,
        1 => valid_tick.0.saturating_add(1),
        2 => u64::MAX,
        _ => raw,
    })
}

fn arbitrary_ordinal(index: usize, cursor: &mut CyclicBytes<'_>) -> u64 {
    let mode = cursor.next_u8() & 0b11;
    let raw = cursor.next_u64();
    match mode {
        0 => index as u64,
        1 => 0,
        2 => u64::MAX,
        _ => raw,
    }
}

fn arbitrary_gate_type(cursor: &mut CyclicBytes<'_>) -> GateType {
    match cursor.next_u8() % 3 {
        0 => GateType::And,
        1 => GateType::Or,
        _ => GateType::Not,
    }
}

fn arbitrary_routing_domain(
    cursor: &mut CyclicBytes<'_>,
    context: CommandFuzzContext,
) -> RoutingDomain {
    match cursor.next_u8() % 3 {
        0 => RoutingDomain::OpenWorld,
        1 => RoutingDomain::FixedSubstrate(arbitrary_entity_id(cursor, context)),
        _ => RoutingDomain::MobileSubstrate(arbitrary_entity_id(cursor, context)),
    }
}

fn arbitrary_endpoint(cursor: &mut CyclicBytes<'_>, context: CommandFuzzContext) -> EndpointTarget {
    match cursor.next_u8() % 3 {
        0 => EndpointTarget::Free,
        1 => EndpointTarget::Junction(JunctionId(arbitrary_entity_id(cursor, context))),
        _ => EndpointTarget::GatePort(GatePortRef {
            gate: GateId(arbitrary_entity_id(cursor, context)),
            port: match cursor.next_u8() & 0b11 {
                0 => GatePort::InputA,
                1 => GatePort::InputB,
                2 => GatePort::Output,
                _ => GatePort::Power,
            },
        }),
    }
}

fn arbitrary_entity_id(cursor: &mut CyclicBytes<'_>, context: CommandFuzzContext) -> EntityId {
    let mode = cursor.next_u8();
    let raw = cursor.next_u64();
    if context.seeded_entities {
        EntityId(match mode & 0b1111 {
            0 => 0,
            1 => STATEFUL_SUBSTRATE_ID.0,
            2 => STATEFUL_GATE_ID.0,
            3 => STATEFUL_JUNCTION_ID.0,
            4 => STATEFUL_TOMBSTONE_ID.0,
            5 => STATEFUL_WIRE_ID.0,
            6 => STATEFUL_WIRE_ID.0 + 1,
            7 => u64::MAX,
            _ => raw,
        })
    } else {
        EntityId(match mode & 0b11 {
            0 => 0,
            1 => 1,
            2 => u64::MAX,
            _ => raw,
        })
    }
}

fn arbitrary_aabb(cursor: &mut CyclicBytes<'_>) -> FixedAabb {
    FixedAabb::new(arbitrary_point(cursor), arbitrary_point(cursor))
}

fn arbitrary_point(cursor: &mut CyclicBytes<'_>) -> FixedVec2 {
    FixedVec2::new(arbitrary_fixed(cursor), arbitrary_fixed(cursor))
}

fn arbitrary_fixed(cursor: &mut CyclicBytes<'_>) -> Fixed {
    let mode = cursor.next_u8() & 0b111;
    let raw = cursor.next_i64();
    Fixed(match mode {
        0 => 0,
        1 => 1_024,
        2 => 16_384,
        3 => 65_536,
        4 => -65_536,
        5 => i64::MAX,
        6 => i64::MIN,
        _ => raw,
    })
}

struct CyclicBytes<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> CyclicBytes<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn next_u8(&mut self) -> u8 {
        if self.bytes.is_empty() {
            return 0;
        }
        let value = self.bytes[self.position % self.bytes.len()];
        self.position += 1;
        value
    }

    fn next_i32(&mut self) -> i32 {
        i32::from_le_bytes([
            self.next_u8(),
            self.next_u8(),
            self.next_u8(),
            self.next_u8(),
        ])
    }

    fn next_u64(&mut self) -> u64 {
        u64::from_le_bytes([
            self.next_u8(),
            self.next_u8(),
            self.next_u8(),
            self.next_u8(),
            self.next_u8(),
            self.next_u8(),
            self.next_u8(),
            self.next_u8(),
        ])
    }

    fn next_i64(&mut self) -> i64 {
        i64::from_le_bytes([
            self.next_u8(),
            self.next_u8(),
            self.next_u8(),
            self.next_u8(),
            self.next_u8(),
            self.next_u8(),
            self.next_u8(),
            self.next_u8(),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CommandExecutionObservation, DecoderTarget, MAX_COMMAND_INPUT_BYTES,
        MAX_DECODER_INPUT_BYTES, MAX_EXPERIMENT_INPUT_BYTES, MAX_GEOMETRY_INPUT_BYTES,
        MAX_MODULE_INPUT_BYTES, MAX_REPLAY_INPUT_BYTES, MAX_SIGNAL_RUNTIME_INPUT_BYTES,
        REFERENCE_BALANCE_PROFILE, REFERENCE_NUMERIC_PROFILE, REFERENCE_PHYSICAL_SCALE_PROFILE,
        REFERENCE_SCENARIO, STATEFUL_BATCH_TICK, STATEFUL_GATE_ID, STATEFUL_JUNCTION_ID,
        STATEFUL_TOMBSTONE_ID, STATEFUL_WIRE_ID, SignalRuntimeExecutionObservation,
        StatefulCommandExecutionObservation, TopologyRuntimeExecutionObservation,
        exercise_commands, exercise_decoder, exercise_experiment_decoder, exercise_geometry,
        exercise_module_decoder, exercise_replay_decoder, exercise_signal_runtime,
        exercise_stateful_commands, exercise_topology_runtime, stateful_envelope,
        stateful_prefix_batches,
    };
    use aon_sim::{
        ArtifactBytes, BindPortCommand, Command, CommandAcceptance, CommandRejection,
        CommandRejectionReason, EndpointTarget, EntityId, Simulation, Tick, WireEnd, WireId,
        decode_package,
    };
    use std::panic::{AssertUnwindSafe, catch_unwind};

    #[test]
    fn decoder_selector_reaches_every_typed_path() {
        assert_eq!(exercise_decoder(&[0]).target, DecoderTarget::Scenario);
        assert_eq!(exercise_decoder(&[1]).target, DecoderTarget::NumericProfile);
        assert_eq!(
            exercise_decoder(&[2]).target,
            DecoderTarget::PhysicalScaleProfile
        );
        assert_eq!(exercise_decoder(&[3]).target, DecoderTarget::BalanceProfile);
    }

    #[test]
    fn each_selected_reference_artifact_is_accepted() {
        for (selector, artifact) in [
            (0_u8, REFERENCE_SCENARIO),
            (1, REFERENCE_NUMERIC_PROFILE),
            (2, REFERENCE_PHYSICAL_SCALE_PROFILE),
            (3, REFERENCE_BALANCE_PROFILE),
        ] {
            let mut input = Vec::with_capacity(artifact.len() + 1);
            input.push(selector);
            input.extend_from_slice(artifact);
            assert!(
                exercise_decoder(&input).result.is_ok(),
                "reference artifact selected by {selector} was rejected"
            );
        }

        assert!(
            exercise_experiment_decoder(include_bytes!(
                "../../../fixtures/experiments/s1-m0-physical-scale-v1.json"
            ))
            .result
            .is_ok()
        );
        assert!(
            exercise_module_decoder(include_bytes!("../corpus/module/valid-empty.case"))
                .result
                .is_ok()
        );
    }

    #[test]
    fn arbitrary_geometry_is_quantized_by_construction() {
        let observation = exercise_geometry(b"deterministic quantized polyline");
        assert!(observation.validation_results.iter().all(Result::is_ok));
    }

    #[test]
    fn short_command_input_covers_all_variants_and_both_encoders() {
        let observation = exercise_commands(&[8, 0]);

        assert_eq!(observation.envelopes.len(), 8);
        assert_eq!(observation.variant_mask, 0xff);
        assert!(observation.encodings.iter().all(|encoding| {
            encoding.allocated_result.is_ok()
                && encoding.streamed_result.is_ok()
                && encoding.bytes_match
        }));
        let CommandExecutionObservation::Stepped(Ok(report)) = &observation.execution else {
            panic!("the all-variant command batch must complete with ordinary results");
        };
        assert_eq!(report.command_acceptances.len(), 1);
        assert_eq!(report.command_acceptances[0].target_tick, Tick(0));
        assert_eq!(report.command_acceptances[0].ordinal, 2);
        assert_eq!(
            report.command_acceptances[0].created_entity,
            Some(EntityId(1))
        );
        assert_eq!(
            report
                .command_rejections
                .iter()
                .map(|rejection| (rejection.ordinal, rejection.reason))
                .collect::<Vec<_>>(),
            vec![
                (0, CommandRejectionReason::UnsupportedPlacement),
                (1, CommandRejectionReason::InvalidGeometryShape),
                (3, CommandRejectionReason::InvalidGeometryShape),
                (4, CommandRejectionReason::InvalidGeometryShape),
                (5, CommandRejectionReason::UnknownEntity),
                (6, CommandRejectionReason::UnknownEntity),
                (7, CommandRejectionReason::UnknownDriver),
            ]
        );
    }

    #[test]
    fn arbitrary_command_mapping_and_execution_are_deterministic() {
        let input = b"ticks ordinals ids malformed geometry and every command tag";
        assert_eq!(exercise_commands(input), exercise_commands(input));
        assert_eq!(
            exercise_stateful_commands(input),
            exercise_stateful_commands(input)
        );
    }

    #[test]
    fn stateful_target_builds_the_minimal_fixed_domain_prefix() {
        let observation = exercise_stateful_commands(&[8, 0]);

        assert_eq!(observation.prefix_reports.len(), 3);
        assert_eq!(
            observation.prefix_reports[0]
                .command_acceptances
                .iter()
                .map(|acceptance| acceptance.created_entity)
                .collect::<Vec<_>>(),
            vec![Some(EntityId(1))]
        );
        assert_eq!(
            observation.prefix_reports[1]
                .command_acceptances
                .iter()
                .map(|acceptance| acceptance.created_entity)
                .collect::<Vec<_>>(),
            vec![
                Some(STATEFUL_GATE_ID),
                Some(STATEFUL_JUNCTION_ID),
                Some(STATEFUL_TOMBSTONE_ID)
            ]
        );
        assert_eq!(
            observation.prefix_reports[2].command_acceptances,
            vec![
                CommandAcceptance {
                    target_tick: Tick(2),
                    ordinal: 0,
                    created_entity: None,
                },
                CommandAcceptance {
                    target_tick: Tick(2),
                    ordinal: 1,
                    created_entity: Some(STATEFUL_WIRE_ID),
                },
            ]
        );
        assert!(
            observation
                .encodings
                .iter()
                .all(|encoding| encoding.bytes_match)
        );
        let StatefulCommandExecutionObservation::Stepped(Ok(report)) = &observation.execution
        else {
            panic!("the stateful arbitrary batch must run after a valid prefix");
        };
        assert_eq!(report.completed_tick, STATEFUL_BATCH_TICK);
    }

    #[test]
    fn stateful_seed_reaches_effective_bind_remove_tombstone_and_wrong_kind_paths() {
        let mut simulation = seeded_simulation();
        let report = simulation
            .step(&[
                stateful_envelope(
                    3,
                    0,
                    Command::BindPort(BindPortCommand {
                        wire: WireId(STATEFUL_WIRE_ID),
                        end: WireEnd::B,
                        target: EndpointTarget::Free,
                    }),
                ),
                stateful_envelope(
                    3,
                    1,
                    Command::BindPort(BindPortCommand {
                        wire: WireId(STATEFUL_GATE_ID),
                        end: WireEnd::A,
                        target: EndpointTarget::Free,
                    }),
                ),
                stateful_envelope(
                    3,
                    2,
                    Command::RemoveEntity(aon_sim::RemoveEntityCommand {
                        target: STATEFUL_TOMBSTONE_ID,
                    }),
                ),
                stateful_envelope(
                    3,
                    3,
                    Command::RemoveEntity(aon_sim::RemoveEntityCommand {
                        target: STATEFUL_WIRE_ID,
                    }),
                ),
            ])
            .expect("the stateful reachability batch has only ordinary results");

        assert_eq!(
            report.command_acceptances,
            vec![
                CommandAcceptance {
                    target_tick: STATEFUL_BATCH_TICK,
                    ordinal: 0,
                    created_entity: None,
                },
                CommandAcceptance {
                    target_tick: STATEFUL_BATCH_TICK,
                    ordinal: 3,
                    created_entity: None,
                },
            ]
        );
        assert_eq!(
            report.command_rejections,
            vec![
                CommandRejection {
                    target_tick: STATEFUL_BATCH_TICK,
                    ordinal: 1,
                    reason: CommandRejectionReason::InvalidPortBinding,
                },
                CommandRejection {
                    target_tick: STATEFUL_BATCH_TICK,
                    ordinal: 2,
                    reason: CommandRejectionReason::RemovedEntity,
                },
            ]
        );
        assert!(report.topology_changed);
    }

    #[test]
    fn oversized_inputs_are_ignored_after_the_documented_limits() {
        let mut decoder = vec![0x5a; MAX_DECODER_INPUT_BYTES];
        let expected_decoder = exercise_decoder(&decoder);
        decoder.extend_from_slice(b"ignored decoder suffix");
        assert_eq!(exercise_decoder(&decoder), expected_decoder);

        let mut replay = vec![b'{'; MAX_REPLAY_INPUT_BYTES];
        let expected_replay = exercise_replay_decoder(&replay);
        replay.extend_from_slice(b"ignored Replay suffix");
        assert_eq!(exercise_replay_decoder(&replay), expected_replay);

        let mut experiment = vec![b'{'; MAX_EXPERIMENT_INPUT_BYTES];
        let expected_experiment = exercise_experiment_decoder(&experiment);
        experiment.extend_from_slice(b"ignored Experiment suffix");
        assert_eq!(
            exercise_experiment_decoder(&experiment),
            expected_experiment
        );

        let mut module = vec![b'{'; MAX_MODULE_INPUT_BYTES];
        let expected_module = exercise_module_decoder(&module);
        module.extend_from_slice(b"ignored Module suffix");
        assert_eq!(exercise_module_decoder(&module), expected_module);

        let mut geometry = vec![0xa5; MAX_GEOMETRY_INPUT_BYTES];
        let expected_geometry = exercise_geometry(&geometry);
        geometry.extend_from_slice(b"ignored geometry suffix");
        assert_eq!(exercise_geometry(&geometry), expected_geometry);

        let mut commands = vec![0x3c; MAX_COMMAND_INPUT_BYTES];
        let expected_commands = exercise_commands(&commands);
        let expected_stateful_commands = exercise_stateful_commands(&commands);
        commands.extend_from_slice(b"ignored command suffix");
        assert_eq!(exercise_commands(&commands), expected_commands);
        assert_eq!(
            exercise_stateful_commands(&commands),
            expected_stateful_commands
        );

        let mut signal = vec![0x6d; MAX_SIGNAL_RUNTIME_INPUT_BYTES];
        let expected_signal = exercise_signal_runtime(&signal);
        signal.extend_from_slice(b"ignored signal-runtime suffix");
        assert_eq!(exercise_signal_runtime(&signal), expected_signal);
    }

    #[test]
    fn deterministic_random_streams_do_not_panic() {
        let mut state = 0xd1b5_4a32_d192_ed03_u64;
        for case_index in 0..2_048_usize {
            let length = case_index % 257;
            let mut bytes = vec![0_u8; length];
            for byte in &mut bytes {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                *byte = state.to_le_bytes()[0];
            }

            let decoder = catch_unwind(AssertUnwindSafe(|| exercise_decoder(&bytes)));
            assert!(
                decoder.is_ok(),
                "decoder panicked for generated case {case_index}"
            );

            let replay = catch_unwind(AssertUnwindSafe(|| exercise_replay_decoder(&bytes)));
            assert!(
                replay.is_ok(),
                "Replay decoder panicked for generated case {case_index}"
            );

            let experiment = catch_unwind(AssertUnwindSafe(|| exercise_experiment_decoder(&bytes)));
            assert!(
                experiment.is_ok(),
                "Experiment decoder panicked for generated case {case_index}"
            );

            let module = catch_unwind(AssertUnwindSafe(|| exercise_module_decoder(&bytes)));
            assert!(
                module.is_ok(),
                "Module decoder panicked for generated case {case_index}"
            );

            let geometry = catch_unwind(AssertUnwindSafe(|| exercise_geometry(&bytes)));
            assert!(
                geometry.is_ok(),
                "geometry panicked for generated case {case_index}"
            );

            let commands = catch_unwind(AssertUnwindSafe(|| exercise_commands(&bytes)));
            let Ok(commands) = commands else {
                panic!("commands panicked for generated case {case_index}");
            };
            assert_eq!(
                commands.invariant_failure(),
                None,
                "commands violated a harness invariant for generated case {case_index}"
            );

            let stateful_commands =
                catch_unwind(AssertUnwindSafe(|| exercise_stateful_commands(&bytes)));
            let Ok(stateful_commands) = stateful_commands else {
                panic!("stateful commands panicked for generated case {case_index}");
            };
            assert_eq!(
                stateful_commands.invariant_failure(),
                None,
                "stateful commands violated a harness invariant for generated case {case_index}"
            );

            if case_index < 256 {
                let signal_runtime =
                    catch_unwind(AssertUnwindSafe(|| exercise_signal_runtime(&bytes)));
                let Ok(signal_runtime) = signal_runtime else {
                    panic!("signal runtime panicked for generated case {case_index}");
                };
                assert_eq!(
                    signal_runtime.execution,
                    SignalRuntimeExecutionObservation::Completed,
                    "signal runtime did not complete for generated case {case_index}"
                );
                assert_eq!(
                    signal_runtime.invariant_failure(),
                    None,
                    "signal runtime violated a harness invariant for generated case {case_index}"
                );
            }

            if case_index < 64 {
                let topology_runtime =
                    catch_unwind(AssertUnwindSafe(|| exercise_topology_runtime(&bytes)));
                let Ok(topology_runtime) = topology_runtime else {
                    panic!("topology runtime panicked for generated case {case_index}");
                };
                assert_eq!(
                    topology_runtime.execution,
                    TopologyRuntimeExecutionObservation::Completed,
                    "topology runtime did not complete for generated case {case_index}"
                );
                assert_eq!(
                    topology_runtime.invariant_failure(),
                    None,
                    "topology runtime violated a harness invariant for generated case {case_index}"
                );
            }
        }
    }

    fn seeded_simulation() -> Simulation {
        let package = decode_package(ArtifactBytes {
            scenario: REFERENCE_SCENARIO,
            numeric_profile: REFERENCE_NUMERIC_PROFILE,
            physical_scale_profile: REFERENCE_PHYSICAL_SCALE_PROFILE,
            balance_profile: REFERENCE_BALANCE_PROFILE,
        })
        .expect("the checked-in reference package decodes");
        let mut simulation = Simulation::new(package).expect("the reference simulation starts");
        for commands in stateful_prefix_batches() {
            let report = simulation
                .step(&commands)
                .expect("the deterministic stateful prefix has no run error");
            assert!(report.command_rejections.is_empty());
        }
        simulation
    }
}
