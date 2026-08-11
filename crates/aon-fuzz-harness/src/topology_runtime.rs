use super::{
    CommandEncodingObservation, REFERENCE_BALANCE_PROFILE, REFERENCE_NUMERIC_PROFILE,
    REFERENCE_PHYSICAL_SCALE_PROFILE, REFERENCE_SCENARIO, exercise_command_encoding,
};
use aon_sim::{
    ArtifactBytes, BindPortCommand, Command, CommandEnvelope, DriveStrength, DriverSample,
    EndpointTarget, EntityId, Fixed, FixedAabb, FixedVec2, GateId, GatePort, GatePortRef,
    GateSignalPorts, GateSignalSnapshot, GateType, JunctionId, LogicLevel, PackageError,
    PlaceFixedSubstrateCommand, PlaceGateCommand, PlaceJunctionCommand, PlaceWireCommand,
    RemoveEntityCommand, RoutingDomain, SetExternalDriverCommand, Simulation, SimulationError,
    StateHash, StepReport, Tick, WireEnd, WireId, WireSignalSnapshot, decode_package,
};

const SUBSTRATE_ID: EntityId = EntityId(1);
const SOURCE_GATE: GateId = GateId(EntityId(2));
const TARGET_GATE: GateId = GateId(EntityId(3));
const FIRST_WIRE: WireId = WireId(EntityId(4));
const REBUILT_WIRE: WireId = WireId(EntityId(5));
const WORLD_PITCH: i64 = 65_536;
const CIRCUIT_PITCH: i64 = 16_384;
const PHYSICAL_ARRIVAL_TICK: Tick = Tick(5);

/// Maximum number of bytes, and therefore complete S0-M4 micro-scenarios, interpreted by the
/// topology-runtime target.
pub const MAX_TOPOLOGY_RUNTIME_INPUT_BYTES: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TopologyRuntimeScenario {
    AddAndRevisionRace,
    RemoveInFlight,
    RebindInFlight,
    BindAwayAndBack,
    RebuildIdenticalGeometry,
    UnrelatedTopologyEdit,
    RemoveAppliedRoute,
    CheckedMaxSample,
}

impl TopologyRuntimeScenario {
    const fn from_selector(selector: u8) -> Self {
        match selector & 0b111 {
            0 => Self::AddAndRevisionRace,
            1 => Self::RemoveInFlight,
            2 => Self::RebindInFlight,
            3 => Self::BindAwayAndBack,
            4 => Self::RebuildIdenticalGeometry,
            5 => Self::UnrelatedTopologyEdit,
            6 => Self::RemoveAppliedRoute,
            _ => Self::CheckedMaxSample,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TopologyRuntimeCoverage {
    pub completed_scenarios: u64,
    pub permuted_command_batches: u64,
    pub routes_added: u64,
    pub routes_removed: u64,
    pub routes_retained: u64,
    pub routes_replaced: u64,
    pub topology_sync_arrivals_staged: u64,
    pub stale_revision_arrivals: u64,
    pub invalid_path_arrivals: u64,
    pub idempotent_signal_arrivals: u64,
    pub add_revision_race_outcomes: u64,
    pub remove_in_flight_outcomes: u64,
    pub rebind_in_flight_outcomes: u64,
    pub bind_away_back_outcomes: u64,
    pub rebuild_outcomes: u64,
    pub unrelated_edit_outcomes: u64,
    pub removed_slot_outcomes: u64,
    pub checked_max_sample_outcomes: u64,
    pub slot_revision_observations: u64,
}

impl TopologyRuntimeCoverage {
    fn checked_merge(&mut self, other: Self) -> Result<(), ()> {
        macro_rules! merge {
            ($field:ident) => {
                self.$field = self.$field.checked_add(other.$field).ok_or(())?;
            };
        }
        merge!(completed_scenarios);
        merge!(permuted_command_batches);
        merge!(routes_added);
        merge!(routes_removed);
        merge!(routes_retained);
        merge!(routes_replaced);
        merge!(topology_sync_arrivals_staged);
        merge!(stale_revision_arrivals);
        merge!(invalid_path_arrivals);
        merge!(idempotent_signal_arrivals);
        merge!(add_revision_race_outcomes);
        merge!(remove_in_flight_outcomes);
        merge!(rebind_in_flight_outcomes);
        merge!(bind_away_back_outcomes);
        merge!(rebuild_outcomes);
        merge!(unrelated_edit_outcomes);
        merge!(removed_slot_outcomes);
        merge!(checked_max_sample_outcomes);
        merge!(slot_revision_observations);
        Ok(())
    }

    fn checked_record_report(&mut self, report: &StepReport) -> Result<(), ()> {
        let counters = report.signal_counters;
        self.routes_added = self
            .routes_added
            .checked_add(counters.routes_added)
            .ok_or(())?;
        self.routes_removed = self
            .routes_removed
            .checked_add(counters.routes_removed)
            .ok_or(())?;
        self.routes_retained = self
            .routes_retained
            .checked_add(counters.routes_retained)
            .ok_or(())?;
        self.routes_replaced = self
            .routes_replaced
            .checked_add(counters.routes_replaced)
            .ok_or(())?;
        self.topology_sync_arrivals_staged = self
            .topology_sync_arrivals_staged
            .checked_add(counters.topology_sync_arrivals_staged)
            .ok_or(())?;
        self.stale_revision_arrivals = self
            .stale_revision_arrivals
            .checked_add(counters.stale_revision_arrivals)
            .ok_or(())?;
        self.invalid_path_arrivals = self
            .invalid_path_arrivals
            .checked_add(counters.invalid_path_arrivals)
            .ok_or(())?;
        self.idempotent_signal_arrivals = self
            .idempotent_signal_arrivals
            .checked_add(counters.idempotent_signal_arrivals)
            .ok_or(())?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TopologyRuntimeExecutionObservation {
    PackageRejected(PackageError),
    SimulationRejected {
        case_index: usize,
        replica: u8,
        error: SimulationError,
    },
    RunError {
        case_index: usize,
        step_index: usize,
        replica: u8,
        error: SimulationError,
    },
    EncoderMismatch {
        case_index: usize,
        step_index: usize,
    },
    DeterminismMismatch {
        case_index: usize,
        step_index: usize,
    },
    ExpectationMismatch {
        case_index: usize,
        step_index: usize,
    },
    Completed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopologyRuntimeObservation {
    pub consumed_len: usize,
    pub generated_scenarios: usize,
    pub generated_steps: usize,
    pub scenarios: Vec<TopologyRuntimeScenario>,
    pub step_reports: Vec<StepReport>,
    pub state_hashes: Vec<StateHash>,
    pub encodings: Vec<CommandEncodingObservation>,
    pub coverage: TopologyRuntimeCoverage,
    pub execution: TopologyRuntimeExecutionObservation,
}

impl TopologyRuntimeObservation {
    /// Returns a harness failure. Every known command must encode, every Simulation error is
    /// fatal, and both insertion-order replicas must agree after every Tick.
    pub fn invariant_failure(&self) -> Option<&'static str> {
        if self.encodings.iter().any(|encoding| {
            encoding.allocated_result.is_err()
                || encoding.streamed_result.is_err()
                || !encoding.bytes_match
        }) {
            return Some("a topology-runtime command failed canonical encoding");
        }
        match self.execution {
            TopologyRuntimeExecutionObservation::PackageRejected(_) => {
                Some("the embedded reference package was rejected")
            }
            TopologyRuntimeExecutionObservation::SimulationRejected { .. } => {
                Some("the embedded reference topology simulation was rejected")
            }
            TopologyRuntimeExecutionObservation::RunError { .. } => {
                Some("a bounded topology-runtime scenario produced a SimulationError")
            }
            TopologyRuntimeExecutionObservation::EncoderMismatch { .. } => {
                Some("topology-runtime command encoders disagreed")
            }
            TopologyRuntimeExecutionObservation::DeterminismMismatch { .. } => {
                Some("permuted topology-runtime replicas disagreed")
            }
            TopologyRuntimeExecutionObservation::ExpectationMismatch { .. } => {
                Some("an S0-M4 counter, slot, revision, or lifecycle outcome was unexpected")
            }
            TopologyRuntimeExecutionObservation::Completed => None,
        }
    }
}

/// Runs bounded, stateful S0-M4 topology scenarios against two insertion-order permutations.
///
/// Every selected scenario creates a physical delay-three route while a Gate-output transition is
/// already due. This stages a TopologySync Revision N and Propagation Revision N+1 in the same
/// Tick. Subsequent operations either let the pair arrive, invalidate it while in flight, replace
/// its generation, rebuild the geometry under a new ID, or edit unrelated topology. Assertions are
/// based on `StepReport` counters, applied Slot samples, immutable snapshots, and per-Tick hashes.
pub fn exercise_topology_runtime(input: &[u8]) -> TopologyRuntimeObservation {
    let bounded = &input[..input.len().min(MAX_TOPOLOGY_RUNTIME_INPUT_BYTES)];
    let mut observation = TopologyRuntimeObservation {
        consumed_len: bounded.len(),
        generated_scenarios: 0,
        generated_steps: 0,
        scenarios: Vec::with_capacity(bounded.len()),
        step_reports: Vec::new(),
        state_hashes: Vec::new(),
        encodings: Vec::new(),
        coverage: TopologyRuntimeCoverage::default(),
        execution: TopologyRuntimeExecutionObservation::Completed,
    };

    let package = match decode_package(ArtifactBytes {
        scenario: REFERENCE_SCENARIO,
        numeric_profile: REFERENCE_NUMERIC_PROFILE,
        physical_scale_profile: REFERENCE_PHYSICAL_SCALE_PROFILE,
        balance_profile: REFERENCE_BALANCE_PROFILE,
    }) {
        Ok(package) => package,
        Err(error) => {
            observation.execution = TopologyRuntimeExecutionObservation::PackageRejected(error);
            return observation;
        }
    };

    for (case_index, &selector) in bounded.iter().enumerate() {
        let scenario = TopologyRuntimeScenario::from_selector(selector);
        observation.scenarios.push(scenario);
        let trace = match run_scenario(package.clone(), case_index, scenario) {
            Ok(trace) => trace,
            Err(failure) => {
                observation.execution = failure.into_observation();
                return observation;
            }
        };
        observation.generated_scenarios += 1;
        observation.generated_steps += trace.reports.len();
        observation.step_reports.extend(trace.reports);
        observation.state_hashes.extend(trace.hashes);
        observation.encodings.extend(trace.encodings);
        if observation.coverage.checked_merge(trace.coverage).is_err() {
            observation.execution = TopologyRuntimeExecutionObservation::ExpectationMismatch {
                case_index,
                step_index: observation.generated_steps,
            };
            return observation;
        }
    }

    observation
}

struct ScenarioTrace {
    reports: Vec<StepReport>,
    hashes: Vec<StateHash>,
    encodings: Vec<CommandEncodingObservation>,
    coverage: TopologyRuntimeCoverage,
}

enum ScenarioFailure {
    SimulationRejected {
        case_index: usize,
        replica: u8,
        error: SimulationError,
    },
    Run {
        case_index: usize,
        step_index: usize,
        replica: u8,
        error: SimulationError,
    },
    Encoder {
        case_index: usize,
        step_index: usize,
    },
    Determinism {
        case_index: usize,
        step_index: usize,
    },
    Expectation {
        case_index: usize,
        step_index: usize,
    },
}

impl ScenarioFailure {
    fn into_observation(self) -> TopologyRuntimeExecutionObservation {
        match self {
            Self::SimulationRejected {
                case_index,
                replica,
                error,
            } => TopologyRuntimeExecutionObservation::SimulationRejected {
                case_index,
                replica,
                error,
            },
            Self::Run {
                case_index,
                step_index,
                replica,
                error,
            } => TopologyRuntimeExecutionObservation::RunError {
                case_index,
                step_index,
                replica,
                error,
            },
            Self::Encoder {
                case_index,
                step_index,
            } => TopologyRuntimeExecutionObservation::EncoderMismatch {
                case_index,
                step_index,
            },
            Self::Determinism {
                case_index,
                step_index,
            } => TopologyRuntimeExecutionObservation::DeterminismMismatch {
                case_index,
                step_index,
            },
            Self::Expectation {
                case_index,
                step_index,
            } => TopologyRuntimeExecutionObservation::ExpectationMismatch {
                case_index,
                step_index,
            },
        }
    }
}

#[derive(Clone, Copy)]
struct FixtureIds {
    source: GateSignalPorts,
    target: GateSignalPorts,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PublicSnapshot {
    next_tick: Tick,
    topology_revision: aon_sim::Revision,
    source_gate: GateSignalSnapshot,
    target_gate: GateSignalSnapshot,
    source_external: DriverSample,
    source_output: DriverSample,
    target_external: DriverSample,
    target_output: DriverSample,
    source_input: LogicLevel,
    target_input: LogicLevel,
    target_slot_from_source: Option<DriverSample>,
    wire_states: Vec<(WireId, Option<WireSignalSnapshot>)>,
}

struct PairRunner {
    case_index: usize,
    left: Simulation,
    right: Simulation,
    ids: Option<FixtureIds>,
    known_wires: Vec<WireId>,
    reports: Vec<StepReport>,
    hashes: Vec<StateHash>,
    encodings: Vec<CommandEncodingObservation>,
    coverage: TopologyRuntimeCoverage,
}

impl PairRunner {
    fn new(
        package: aon_sim::SimulationPackage,
        case_index: usize,
    ) -> Result<Self, ScenarioFailure> {
        let left = Simulation::new(package.clone()).map_err(|error| {
            ScenarioFailure::SimulationRejected {
                case_index,
                replica: 0,
                error,
            }
        })?;
        let right =
            Simulation::new(package).map_err(|error| ScenarioFailure::SimulationRejected {
                case_index,
                replica: 1,
                error,
            })?;
        Ok(Self {
            case_index,
            left,
            right,
            ids: None,
            known_wires: Vec::new(),
            reports: Vec::new(),
            hashes: Vec::new(),
            encodings: Vec::new(),
            coverage: TopologyRuntimeCoverage::default(),
        })
    }

    fn step(&mut self, commands: Vec<Command>) -> Result<StepReport, ScenarioFailure> {
        let step_index = self.reports.len();
        let tick = self.left.next_tick();
        if self.right.next_tick() != tick {
            return Err(self.determinism_failure(step_index));
        }
        let envelopes: Vec<_> = commands
            .into_iter()
            .enumerate()
            .map(|(ordinal, command)| CommandEnvelope {
                target_tick: tick,
                ordinal: ordinal as u64,
                command,
            })
            .collect();
        let encodings: Vec<_> = envelopes.iter().map(exercise_command_encoding).collect();
        if encodings.iter().any(|encoding| {
            encoding.allocated_result.is_err()
                || encoding.streamed_result.is_err()
                || !encoding.bytes_match
        }) {
            self.encodings.extend(encodings);
            return Err(ScenarioFailure::Encoder {
                case_index: self.case_index,
                step_index,
            });
        }
        self.encodings.extend(encodings);

        let left_report = self
            .left
            .step(&envelopes)
            .map_err(|error| ScenarioFailure::Run {
                case_index: self.case_index,
                step_index,
                replica: 0,
                error,
            })?;
        let mut permuted = envelopes;
        if permuted.len() > 1 {
            permuted.reverse();
            self.coverage.permuted_command_batches = self
                .coverage
                .permuted_command_batches
                .checked_add(1)
                .ok_or_else(|| self.expectation_failure(step_index))?;
        }
        let right_report = self
            .right
            .step(&permuted)
            .map_err(|error| ScenarioFailure::Run {
                case_index: self.case_index,
                step_index,
                replica: 1,
                error,
            })?;
        if left_report != right_report
            || left_report.state_hash != self.left.state_hash()
            || right_report.state_hash != self.right.state_hash()
            || self.left.state_hash() != self.right.state_hash()
        {
            return Err(self.determinism_failure(step_index));
        }
        if let Some(ids) = self.ids
            && public_snapshot(&self.left, ids, &self.known_wires)
                != public_snapshot(&self.right, ids, &self.known_wires)
        {
            return Err(self.determinism_failure(step_index));
        }
        self.coverage
            .checked_record_report(&left_report)
            .map_err(|()| self.expectation_failure(step_index))?;
        self.hashes.push(left_report.state_hash);
        self.reports.push(left_report.clone());
        Ok(left_report)
    }

    fn install_fixture_ids(&mut self, ids: FixtureIds) -> Result<(), ScenarioFailure> {
        self.ids = Some(ids);
        self.ensure_public_match()
    }

    fn register_wire(&mut self, wire: WireId) -> Result<(), ScenarioFailure> {
        self.known_wires.push(wire);
        self.known_wires.sort_unstable();
        self.known_wires.dedup();
        self.ensure_public_match()
    }

    fn ensure_public_match(&self) -> Result<(), ScenarioFailure> {
        let step_index = self.reports.len().saturating_sub(1);
        let ids = self
            .ids
            .ok_or_else(|| self.expectation_failure(step_index))?;
        if public_snapshot(&self.left, ids, &self.known_wires)
            != public_snapshot(&self.right, ids, &self.known_wires)
        {
            return Err(self.determinism_failure(step_index));
        }
        Ok(())
    }

    fn expect(&self, condition: bool) -> Result<(), ScenarioFailure> {
        if condition {
            Ok(())
        } else {
            Err(self.expectation_failure(self.reports.len().saturating_sub(1)))
        }
    }

    fn ids(&self) -> Result<FixtureIds, ScenarioFailure> {
        self.ids
            .ok_or_else(|| self.expectation_failure(self.reports.len().saturating_sub(1)))
    }

    fn expectation_failure(&self, step_index: usize) -> ScenarioFailure {
        ScenarioFailure::Expectation {
            case_index: self.case_index,
            step_index,
        }
    }

    fn determinism_failure(&self, step_index: usize) -> ScenarioFailure {
        ScenarioFailure::Determinism {
            case_index: self.case_index,
            step_index,
        }
    }

    fn finish(mut self) -> Result<ScenarioTrace, ScenarioFailure> {
        self.coverage.completed_scenarios = 1;
        Ok(ScenarioTrace {
            reports: self.reports,
            hashes: self.hashes,
            encodings: self.encodings,
            coverage: self.coverage,
        })
    }
}

fn public_snapshot(
    simulation: &Simulation,
    ids: FixtureIds,
    known_wires: &[WireId],
) -> Option<PublicSnapshot> {
    Some(PublicSnapshot {
        next_tick: simulation.next_tick(),
        topology_revision: simulation.topology_revision(),
        source_gate: simulation.gate_signal_state(SOURCE_GATE)?,
        target_gate: simulation.gate_signal_state(TARGET_GATE)?,
        source_external: simulation.driver_sample(ids.source.input_a.external_driver)?,
        source_output: simulation.driver_sample(ids.source.output)?,
        target_external: simulation.driver_sample(ids.target.input_a.external_driver)?,
        target_output: simulation.driver_sample(ids.target.output)?,
        source_input: simulation.sink_level(ids.source.input_a.sink)?,
        target_input: simulation.sink_level(ids.target.input_a.sink)?,
        target_slot_from_source: simulation
            .sink_driver_sample(ids.target.input_a.sink, ids.source.output),
        wire_states: known_wires
            .iter()
            .copied()
            .map(|wire| (wire, simulation.wire_signal_state(wire)))
            .collect(),
    })
}

fn run_scenario(
    package: aon_sim::SimulationPackage,
    case_index: usize,
    scenario: TopologyRuntimeScenario,
) -> Result<ScenarioTrace, ScenarioFailure> {
    let mut runner = PairRunner::new(package, case_index)?;
    build_fixture(&mut runner)?;
    match scenario {
        TopologyRuntimeScenario::AddAndRevisionRace => run_add_revision_race(&mut runner)?,
        TopologyRuntimeScenario::RemoveInFlight => run_remove_in_flight(&mut runner)?,
        TopologyRuntimeScenario::RebindInFlight => run_rebind_in_flight(&mut runner)?,
        TopologyRuntimeScenario::BindAwayAndBack => run_bind_away_back(&mut runner)?,
        TopologyRuntimeScenario::RebuildIdenticalGeometry => run_rebuild(&mut runner)?,
        TopologyRuntimeScenario::UnrelatedTopologyEdit => run_unrelated_edit(&mut runner)?,
        TopologyRuntimeScenario::RemoveAppliedRoute => run_remove_applied_route(&mut runner)?,
        TopologyRuntimeScenario::CheckedMaxSample => run_checked_max_sample(&mut runner)?,
    }
    runner.finish()
}

fn build_fixture(runner: &mut PairRunner) -> Result<(), ScenarioFailure> {
    let bounds = FixedAabb::new(
        point(-32 * WORLD_PITCH, -32 * WORLD_PITCH),
        point(32 * WORLD_PITCH, 32 * WORLD_PITCH),
    );
    let substrate = runner.step(vec![Command::PlaceFixedSubstrate(
        PlaceFixedSubstrateCommand {
            origin: point(0, 0),
            routing_area: bounds,
            footprint: bounds,
        },
    )])?;
    runner.expect(all_accepted(&substrate, 1))?;
    runner.expect(created_entities(&substrate) == vec![SUBSTRATE_ID])?;

    let gates = runner.step(vec![
        Command::PlaceGate(PlaceGateCommand {
            gate_type: GateType::Not,
            origin: point(0, 0),
            routing_domain: domain(),
        }),
        Command::PlaceGate(PlaceGateCommand {
            gate_type: GateType::Not,
            origin: point(34 * CIRCUIT_PITCH, 0),
            routing_domain: domain(),
        }),
    ])?;
    runner.expect(all_accepted(&gates, 2))?;
    runner.expect(
        created_entities(&gates) == vec![SOURCE_GATE.entity_id(), TARGET_GATE.entity_id()],
    )?;
    runner.expect(gates.signal_counters.routes_added == 2)?;
    runner.expect(gates.signal_counters.topology_sync_arrivals_staged == 2)?;

    let left_source = runner
        .left
        .gate_signal_ports(SOURCE_GATE)
        .ok_or_else(|| runner.expectation_failure(1))?;
    let left_target = runner
        .left
        .gate_signal_ports(TARGET_GATE)
        .ok_or_else(|| runner.expectation_failure(1))?;
    let right_source = runner
        .right
        .gate_signal_ports(SOURCE_GATE)
        .ok_or_else(|| runner.expectation_failure(1))?;
    let right_target = runner
        .right
        .gate_signal_ports(TARGET_GATE)
        .ok_or_else(|| runner.expectation_failure(1))?;
    runner.expect(left_source == right_source && left_target == right_target)?;
    runner.install_fixture_ids(FixtureIds {
        source: left_source,
        target: left_target,
    })?;
    Ok(())
}

fn connect_while_output_transition_is_due(
    runner: &mut PairRunner,
) -> Result<StepReport, ScenarioFailure> {
    let ids = runner.ids()?;
    let report = runner.step(vec![
        Command::PlaceWire(wire_command()),
        Command::SetExternalDriver(SetExternalDriverCommand {
            driver: ids.source.input_a.external_driver,
            level: LogicLevel::Low,
            strength: DriveStrength(0),
        }),
    ])?;
    runner.expect(all_accepted(&report, 2))?;
    runner.expect(created_entities(&report) == vec![FIRST_WIRE.entity_id()])?;
    runner.expect(report.completed_tick == Tick(2))?;
    runner.expect(report.signal_counters.routes_added == 1)?;
    runner.expect(report.signal_counters.routes_removed == 0)?;
    runner.expect(report.signal_counters.routes_retained == 2)?;
    runner.expect(report.signal_counters.routes_replaced == 0)?;
    runner.expect(report.signal_counters.topology_sync_arrivals_staged == 1)?;
    let source_change = report
        .driver_changes
        .iter()
        .find(|change| change.driver == ids.source.output);
    runner.expect(source_change.is_some_and(|change| {
        change.previous.revision.0 == 0
            && change.current.revision.0 == 1
            && change.current.level == LogicLevel::High
            && change.current.strength == DriveStrength(400)
            && change.current.emitted_at == Tick(2)
    }))?;
    runner.register_wire(FIRST_WIRE)?;
    runner.expect(
        runner
            .left
            .sink_driver_sample(ids.target.input_a.sink, ids.source.output)
            .is_none(),
    )?;
    Ok(report)
}

fn run_add_revision_race(runner: &mut PairRunner) -> Result<(), ScenarioFailure> {
    connect_while_output_transition_is_due(runner)?;
    advance_to_physical_arrival(runner)?;
    let report = runner
        .reports
        .last()
        .cloned()
        .ok_or_else(|| runner.expectation_failure(0))?;
    expect_revision_race_arrival(runner, &report)?;
    runner.coverage.add_revision_race_outcomes = 1;
    runner.coverage.slot_revision_observations = 1;
    Ok(())
}

fn run_remove_in_flight(runner: &mut PairRunner) -> Result<(), ScenarioFailure> {
    connect_while_output_transition_is_due(runner)?;
    let removal = runner.step(vec![Command::RemoveEntity(RemoveEntityCommand {
        target: FIRST_WIRE.entity_id(),
    })])?;
    runner.expect(all_accepted(&removal, 1))?;
    runner.expect(removal.signal_counters.routes_removed == 1)?;
    runner.expect(removal.signal_counters.topology_sync_arrivals_staged == 0)?;
    advance_to_physical_arrival(runner)?;
    let due = runner
        .reports
        .last()
        .ok_or_else(|| runner.expectation_failure(0))?;
    let ids = runner.ids()?;
    runner.expect(due.signal_counters.invalid_path_arrivals == 2)?;
    runner.expect(due.signal_counters.stale_revision_arrivals == 0)?;
    runner.expect(
        runner
            .left
            .sink_driver_sample(ids.target.input_a.sink, ids.source.output)
            .is_none(),
    )?;
    runner.expect(runner.left.wire_signal_state(FIRST_WIRE).is_none())?;
    runner.coverage.remove_in_flight_outcomes = 1;
    Ok(())
}

fn run_rebind_in_flight(runner: &mut PairRunner) -> Result<(), ScenarioFailure> {
    connect_while_output_transition_is_due(runner)?;
    let rebind = runner.step(vec![Command::BindPort(BindPortCommand {
        wire: FIRST_WIRE,
        end: WireEnd::B,
        target: EndpointTarget::Free,
    })])?;
    runner.expect(all_accepted(&rebind, 1))?;
    runner.expect(rebind.signal_counters.routes_removed == 1)?;
    advance_to_physical_arrival(runner)?;
    let due = runner
        .reports
        .last()
        .ok_or_else(|| runner.expectation_failure(0))?;
    let ids = runner.ids()?;
    runner.expect(due.signal_counters.invalid_path_arrivals == 2)?;
    runner.expect(runner.left.wire_signal_state(FIRST_WIRE).is_some())?;
    runner.expect(
        runner
            .left
            .sink_driver_sample(ids.target.input_a.sink, ids.source.output)
            .is_none(),
    )?;
    runner.coverage.rebind_in_flight_outcomes = 1;
    Ok(())
}

fn run_bind_away_back(runner: &mut PairRunner) -> Result<(), ScenarioFailure> {
    connect_while_output_transition_is_due(runner)?;
    let replacement = runner.step(vec![
        Command::BindPort(BindPortCommand {
            wire: FIRST_WIRE,
            end: WireEnd::B,
            target: EndpointTarget::Free,
        }),
        Command::BindPort(BindPortCommand {
            wire: FIRST_WIRE,
            end: WireEnd::B,
            target: target_input_endpoint(),
        }),
    ])?;
    runner.expect(all_accepted(&replacement, 2))?;
    runner.expect(replacement.signal_counters.routes_replaced == 1)?;
    runner.expect(replacement.signal_counters.routes_retained == 2)?;
    runner.expect(replacement.signal_counters.topology_sync_arrivals_staged == 1)?;
    advance_to_physical_arrival(runner)?;
    let old_due = runner
        .reports
        .last()
        .ok_or_else(|| runner.expectation_failure(0))?;
    runner.expect(old_due.signal_counters.invalid_path_arrivals == 2)?;
    let new_due = runner.step(Vec::new())?;
    let ids = runner.ids()?;
    let slot = runner
        .left
        .sink_driver_sample(ids.target.input_a.sink, ids.source.output);
    runner.expect(new_due.completed_tick == Tick(6))?;
    runner.expect(new_due.signal_counters.invalid_path_arrivals == 0)?;
    runner.expect(new_due.signal_counters.signal_arrivals_applied == 1)?;
    runner.expect(slot.is_some_and(|sample| sample.revision.0 == 1))?;
    runner.coverage.bind_away_back_outcomes = 1;
    runner.coverage.slot_revision_observations = 1;
    Ok(())
}

fn run_rebuild(runner: &mut PairRunner) -> Result<(), ScenarioFailure> {
    connect_while_output_transition_is_due(runner)?;
    let replacement = runner.step(vec![
        Command::RemoveEntity(RemoveEntityCommand {
            target: FIRST_WIRE.entity_id(),
        }),
        Command::PlaceWire(wire_command()),
    ])?;
    runner.expect(all_accepted(&replacement, 2))?;
    runner.expect(created_entities(&replacement) == vec![REBUILT_WIRE.entity_id()])?;
    runner.expect(replacement.signal_counters.routes_replaced == 1)?;
    runner.expect(replacement.signal_counters.topology_sync_arrivals_staged == 1)?;
    runner.register_wire(REBUILT_WIRE)?;
    advance_to_physical_arrival(runner)?;
    let old_due = runner
        .reports
        .last()
        .ok_or_else(|| runner.expectation_failure(0))?;
    runner.expect(old_due.signal_counters.invalid_path_arrivals == 2)?;
    let new_due = runner.step(Vec::new())?;
    let ids = runner.ids()?;
    let slot = runner
        .left
        .sink_driver_sample(ids.target.input_a.sink, ids.source.output);
    runner.expect(new_due.completed_tick == Tick(6))?;
    runner.expect(new_due.signal_counters.signal_arrivals_applied == 1)?;
    runner.expect(slot.is_some_and(|sample| sample.revision.0 == 1))?;
    runner.expect(runner.left.wire_signal_state(FIRST_WIRE).is_none())?;
    runner.expect(runner.left.wire_signal_state(REBUILT_WIRE).is_some())?;
    runner.coverage.rebuild_outcomes = 1;
    runner.coverage.slot_revision_observations = 1;
    Ok(())
}

fn run_unrelated_edit(runner: &mut PairRunner) -> Result<(), ScenarioFailure> {
    connect_while_output_transition_is_due(runner)?;
    let edit = runner.step(vec![Command::PlaceJunction(PlaceJunctionCommand {
        routing_domain: domain(),
        position: point(0, 8 * WORLD_PITCH),
    })])?;
    runner.expect(all_accepted(&edit, 1))?;
    runner.expect(created_entities(&edit) == vec![JunctionId(EntityId(5)).entity_id()])?;
    runner.expect(edit.signal_counters.routes_added == 0)?;
    runner.expect(edit.signal_counters.routes_removed == 0)?;
    runner.expect(edit.signal_counters.routes_replaced == 0)?;
    runner.expect(edit.signal_counters.routes_retained == 3)?;
    runner.expect(edit.signal_counters.topology_sync_arrivals_staged == 0)?;
    advance_to_physical_arrival(runner)?;
    let due = runner
        .reports
        .last()
        .cloned()
        .ok_or_else(|| runner.expectation_failure(0))?;
    expect_revision_race_arrival(runner, &due)?;
    runner.expect(due.signal_counters.invalid_path_arrivals == 0)?;
    runner.coverage.unrelated_edit_outcomes = 1;
    runner.coverage.slot_revision_observations = 1;
    Ok(())
}

fn run_remove_applied_route(runner: &mut PairRunner) -> Result<(), ScenarioFailure> {
    connect_while_output_transition_is_due(runner)?;
    advance_to_physical_arrival(runner)?;
    let due = runner
        .reports
        .last()
        .cloned()
        .ok_or_else(|| runner.expectation_failure(0))?;
    expect_revision_race_arrival(runner, &due)?;
    let ids = runner.ids()?;
    runner.expect(runner.left.sink_level(ids.target.input_a.sink) == Some(LogicLevel::High))?;
    let removal = runner.step(vec![Command::RemoveEntity(RemoveEntityCommand {
        target: FIRST_WIRE.entity_id(),
    })])?;
    runner.expect(all_accepted(&removal, 1))?;
    runner.expect(removal.completed_tick == Tick(6))?;
    runner.expect(removal.signal_counters.routes_removed == 1)?;
    runner.expect(removal.signal_counters.sinks_resolved >= 1)?;
    runner.expect(
        runner
            .left
            .sink_driver_sample(ids.target.input_a.sink, ids.source.output)
            .is_none(),
    )?;
    runner.expect(runner.left.sink_level(ids.target.input_a.sink) == Some(LogicLevel::Low))?;
    runner.coverage.removed_slot_outcomes = 1;
    runner.coverage.slot_revision_observations = 1;
    Ok(())
}

fn run_checked_max_sample(runner: &mut PairRunner) -> Result<(), ScenarioFailure> {
    let ids = runner.ids()?;
    let first = runner.step(vec![Command::SetExternalDriver(SetExternalDriverCommand {
        driver: ids.target.input_a.external_driver,
        level: LogicLevel::High,
        strength: DriveStrength(u64::MAX),
    })])?;
    runner.expect(all_accepted(&first, 1))?;
    let first_sample = runner
        .left
        .driver_sample(ids.target.input_a.external_driver)
        .ok_or_else(|| runner.expectation_failure(2))?;
    let first_slot = runner
        .left
        .sink_driver_sample(ids.target.input_a.sink, ids.target.input_a.external_driver)
        .ok_or_else(|| runner.expectation_failure(2))?;
    runner.expect(
        first_sample.revision.0 == 1
            && first_sample.level == LogicLevel::High
            && first_sample.strength == DriveStrength(u64::MAX)
            && first_sample.emitted_at == Tick(2)
            && first_slot == first_sample,
    )?;

    let no_op = runner.step(vec![Command::SetExternalDriver(SetExternalDriverCommand {
        driver: ids.target.input_a.external_driver,
        level: LogicLevel::High,
        strength: DriveStrength(u64::MAX),
    })])?;
    runner.expect(all_accepted(&no_op, 1))?;
    runner.expect(
        !no_op
            .driver_changes
            .iter()
            .any(|change| change.driver == ids.target.input_a.external_driver),
    )?;
    runner.expect(
        runner
            .left
            .driver_sample(ids.target.input_a.external_driver)
            == Some(first_sample),
    )?;
    runner.expect(
        runner
            .left
            .sink_driver_sample(ids.target.input_a.sink, ids.target.input_a.external_driver)
            == Some(first_slot),
    )?;

    let second = runner.step(vec![Command::SetExternalDriver(SetExternalDriverCommand {
        driver: ids.target.input_a.external_driver,
        level: LogicLevel::High,
        strength: DriveStrength(u64::MAX - 1),
    })])?;
    runner.expect(all_accepted(&second, 1))?;
    let second_sample = runner
        .left
        .driver_sample(ids.target.input_a.external_driver)
        .ok_or_else(|| runner.expectation_failure(4))?;
    runner.expect(
        second_sample.revision.0 == 2
            && second_sample.strength == DriveStrength(u64::MAX - 1)
            && second_sample.emitted_at == Tick(4),
    )?;
    runner.expect(
        runner
            .left
            .sink_driver_sample(ids.target.input_a.sink, ids.target.input_a.external_driver)
            == Some(second_sample),
    )?;
    runner.coverage.checked_max_sample_outcomes = 1;
    runner.coverage.slot_revision_observations = 3;
    Ok(())
}

fn advance_to_physical_arrival(runner: &mut PairRunner) -> Result<(), ScenarioFailure> {
    while runner.left.next_tick() <= PHYSICAL_ARRIVAL_TICK {
        let report = runner.step(Vec::new())?;
        if report.completed_tick < PHYSICAL_ARRIVAL_TICK {
            let ids = runner.ids()?;
            runner.expect(
                runner
                    .left
                    .sink_driver_sample(ids.target.input_a.sink, ids.source.output)
                    .is_none(),
            )?;
        }
    }
    Ok(())
}

fn expect_revision_race_arrival(
    runner: &PairRunner,
    report: &StepReport,
) -> Result<(), ScenarioFailure> {
    let ids = runner.ids()?;
    let slot = runner
        .left
        .sink_driver_sample(ids.target.input_a.sink, ids.source.output);
    runner.expect(report.completed_tick == PHYSICAL_ARRIVAL_TICK)?;
    runner.expect(report.signal_counters.stale_revision_arrivals == 1)?;
    runner.expect(report.signal_counters.invalid_path_arrivals == 0)?;
    runner.expect(report.signal_counters.signal_arrivals_applied == 1)?;
    runner.expect(slot.is_some_and(|sample| {
        sample.driver_id == ids.source.output
            && sample.revision.0 == 1
            && sample.level == LogicLevel::High
            && sample.strength == DriveStrength(400)
            && sample.emitted_at == Tick(2)
    }))?;
    Ok(())
}

fn all_accepted(report: &StepReport, command_count: usize) -> bool {
    report.command_rejections.is_empty() && report.command_acceptances.len() == command_count
}

fn created_entities(report: &StepReport) -> Vec<EntityId> {
    report
        .command_acceptances
        .iter()
        .filter_map(|acceptance| acceptance.created_entity)
        .collect()
}

fn wire_command() -> PlaceWireCommand {
    PlaceWireCommand {
        routing_domain: domain(),
        points: vec![point(CIRCUIT_PITCH, 0), point(33 * CIRCUIT_PITCH, 0)],
        endpoint_a: EndpointTarget::GatePort(GatePortRef {
            gate: SOURCE_GATE,
            port: GatePort::Output,
        }),
        endpoint_b: target_input_endpoint(),
    }
}

const fn target_input_endpoint() -> EndpointTarget {
    EndpointTarget::GatePort(GatePortRef {
        gate: TARGET_GATE,
        port: GatePort::InputA,
    })
}

const fn domain() -> RoutingDomain {
    RoutingDomain::FixedSubstrate(SUBSTRATE_ID)
}

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_TOPOLOGY_RUNTIME_INPUT_BYTES, TopologyRuntimeExecutionObservation,
        exercise_topology_runtime,
    };
    use std::panic::{AssertUnwindSafe, catch_unwind};

    #[test]
    fn all_scenarios_reach_verified_s0_m4_outcomes() {
        let observation = exercise_topology_runtime(b"01234567");
        assert_eq!(
            observation.execution,
            TopologyRuntimeExecutionObservation::Completed
        );
        assert_eq!(observation.invariant_failure(), None);
        assert_eq!(observation.generated_scenarios, 8);
        assert_eq!(observation.state_hashes.len(), observation.generated_steps);
        let coverage = observation.coverage;
        assert_eq!(coverage.completed_scenarios, 8);
        assert!(coverage.permuted_command_batches >= 8);
        assert!(coverage.routes_added > 0);
        assert!(coverage.routes_removed > 0);
        assert!(coverage.routes_retained > 0);
        assert!(coverage.routes_replaced > 0);
        assert!(coverage.topology_sync_arrivals_staged > 0);
        assert!(coverage.stale_revision_arrivals > 0);
        assert!(coverage.invalid_path_arrivals > 0);
        assert_eq!(coverage.add_revision_race_outcomes, 1);
        assert_eq!(coverage.remove_in_flight_outcomes, 1);
        assert_eq!(coverage.rebind_in_flight_outcomes, 1);
        assert_eq!(coverage.bind_away_back_outcomes, 1);
        assert_eq!(coverage.rebuild_outcomes, 1);
        assert_eq!(coverage.unrelated_edit_outcomes, 1);
        assert_eq!(coverage.removed_slot_outcomes, 1);
        assert_eq!(coverage.checked_max_sample_outcomes, 1);
        assert!(coverage.slot_revision_observations >= 8);
    }

    #[test]
    fn arbitrary_input_is_bounded_and_never_panics() {
        let mut input = vec![0xa5; MAX_TOPOLOGY_RUNTIME_INPUT_BYTES];
        let expected = exercise_topology_runtime(&input);
        input.extend_from_slice(b"ignored topology-runtime suffix");
        let replay = catch_unwind(AssertUnwindSafe(|| exercise_topology_runtime(&input)));
        let Ok(replay) = replay else {
            panic!("bounded topology-runtime input panicked");
        };
        assert_eq!(replay, expected);
        assert_eq!(replay.invariant_failure(), None);
    }
}
