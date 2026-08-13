use super::{
    CommandEncodingObservation, REFERENCE_BALANCE_PROFILE, REFERENCE_NUMERIC_PROFILE,
    REFERENCE_PHYSICAL_SCALE_PROFILE, REFERENCE_SCENARIO, exercise_command_encoding,
};
use aon_sim::{
    ArtifactBytes, BindPortCommand, Command, CommandEnvelope, CommandRejectionReason,
    EndpointTarget, EntityId, Fixed, FixedAabb, FixedVec2, GateId, GatePort, GatePortRef, GateType,
    Heading, JunctionDecisionKind, JunctionId, LogicLevel, MobileId, MobilePort, MobilePortRef,
    PackageError, PlaceGateCommand, PlaceJunctionCommand, PlaceMobileSubstrateCommand,
    PlaceWireCommand, RemoveEntityCommand, RenderSnapshot, RoutingDomain, Simulation,
    SimulationError, StateHash, StepReport, TrackPosition, WireEnd, WireId, decode_package,
};

const WORLD_PITCH: i64 = 65_536;
const CIRCUIT_PITCH: i64 = 16_384;
const TURN_EDGE_PITCHES: i64 = 24;
const TURN_START_PITCHES: i64 = 20;
const MAX_TURN_SEARCH_STEPS: usize = 32;
const MAX_QUANTIZED_COORDINATE: i64 = 9_223_372_036_854_710_272;

/// Maximum number of bytes, and therefore complete S0-M7 mobility micro-scenarios, interpreted
/// by the mobility-runtime target.
pub const MAX_MOBILITY_RUNTIME_INPUT_BYTES: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MobilityRuntimeScenario {
    Straight,
    Left,
    Right,
    Reverse,
    BindAndRemove,
    PlacementRollback,
    CheckedMaximumCoordinate,
    CheckedMinimumCoordinate,
}

impl MobilityRuntimeScenario {
    const fn from_selector(selector: u8) -> Self {
        match selector & 0b111 {
            0 => Self::Straight,
            1 => Self::Left,
            2 => Self::Right,
            3 => Self::Reverse,
            4 => Self::BindAndRemove,
            5 => Self::PlacementRollback,
            6 => Self::CheckedMaximumCoordinate,
            _ => Self::CheckedMinimumCoordinate,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MobilityRuntimeCoverage {
    pub completed_scenarios: u64,
    pub permuted_command_batches: u64,
    pub mobile_placements: u64,
    pub placement_rejections: u64,
    pub mobile_port_bindings: u64,
    pub explicit_track_bindings: u64,
    pub occupied_track_rejections: u64,
    pub mobile_removals: u64,
    pub track_removals: u64,
    pub straight_turns: u64,
    pub left_turns: u64,
    pub right_turns: u64,
    pub reverse_turns: u64,
    pub movement_observations: u64,
    pub checked_maximum_coordinate_paths: u64,
    pub checked_minimum_coordinate_paths: u64,
}

impl MobilityRuntimeCoverage {
    fn checked_merge(&mut self, other: Self) -> Result<(), ()> {
        macro_rules! merge {
            ($field:ident) => {
                self.$field = self.$field.checked_add(other.$field).ok_or(())?;
            };
        }
        merge!(completed_scenarios);
        merge!(permuted_command_batches);
        merge!(mobile_placements);
        merge!(placement_rejections);
        merge!(mobile_port_bindings);
        merge!(explicit_track_bindings);
        merge!(occupied_track_rejections);
        merge!(mobile_removals);
        merge!(track_removals);
        merge!(straight_turns);
        merge!(left_turns);
        merge!(right_turns);
        merge!(reverse_turns);
        merge!(movement_observations);
        merge!(checked_maximum_coordinate_paths);
        merge!(checked_minimum_coordinate_paths);
        Ok(())
    }

    fn checked_record_report(&mut self, report: &StepReport) -> Result<(), ()> {
        let movements = u64::try_from(report.mobile_movements.len()).map_err(|_| ())?;
        self.movement_observations = self
            .movement_observations
            .checked_add(movements)
            .ok_or(())?;
        for movement in &report.mobile_movements {
            for decision in &movement.junction_decisions {
                let counter = match decision.kind {
                    JunctionDecisionKind::Straight => &mut self.straight_turns,
                    JunctionDecisionKind::Left => &mut self.left_turns,
                    JunctionDecisionKind::Right => &mut self.right_turns,
                    JunctionDecisionKind::Reverse => &mut self.reverse_turns,
                    JunctionDecisionKind::MissingRequestedSide => continue,
                };
                *counter = counter.checked_add(1).ok_or(())?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MobilityRuntimeExecutionObservation {
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
pub struct MobilityRuntimeObservation {
    pub consumed_len: usize,
    pub generated_scenarios: usize,
    pub generated_steps: usize,
    pub scenarios: Vec<MobilityRuntimeScenario>,
    pub step_reports: Vec<StepReport>,
    pub state_hashes: Vec<StateHash>,
    pub encodings: Vec<CommandEncodingObservation>,
    pub coverage: MobilityRuntimeCoverage,
    pub execution: MobilityRuntimeExecutionObservation,
}

impl MobilityRuntimeObservation {
    /// Returns a harness failure. Every known command must encode, every Simulation error is
    /// fatal, and both insertion-order replicas must agree after every Tick.
    pub fn invariant_failure(&self) -> Option<&'static str> {
        if self.encodings.iter().any(|encoding| {
            encoding.allocated_result.is_err()
                || encoding.streamed_result.is_err()
                || !encoding.bytes_match
        }) {
            return Some("a mobility-runtime command failed canonical encoding");
        }
        match self.execution {
            MobilityRuntimeExecutionObservation::PackageRejected(_) => {
                Some("the embedded reference package was rejected")
            }
            MobilityRuntimeExecutionObservation::SimulationRejected { .. } => {
                Some("the embedded reference mobility simulation was rejected")
            }
            MobilityRuntimeExecutionObservation::RunError { .. } => {
                Some("a bounded mobility-runtime scenario produced a SimulationError")
            }
            MobilityRuntimeExecutionObservation::EncoderMismatch { .. } => {
                Some("mobility-runtime command encoders disagreed")
            }
            MobilityRuntimeExecutionObservation::DeterminismMismatch { .. } => {
                Some("permuted mobility-runtime replicas disagreed")
            }
            MobilityRuntimeExecutionObservation::ExpectationMismatch { .. } => Some(
                "an S0-M7 placement, binding, movement, removal, or arithmetic outcome was unexpected",
            ),
            MobilityRuntimeExecutionObservation::Completed => None,
        }
    }
}

/// Runs bounded, stateful S0-M7 mobility scenarios against two insertion-order permutations.
///
/// Each byte selects one complete scenario. Turn scenarios construct an explicit four-way Track,
/// place a Mobile, route settled Mobile-local NOT outputs into LEFT/RIGHT, and verify the C14
/// decision. Lifecycle scenarios exercise explicit junction binding, occupied-track rejection,
/// Mobile removal, rebinding, and removal. Boundary scenarios move and bounce on Tracks adjacent
/// to the largest and smallest quantized `i64` coordinates. Reports, state hashes, and immutable
/// render snapshots must agree after every Tick.
pub fn exercise_mobility_runtime(input: &[u8]) -> MobilityRuntimeObservation {
    let bounded = &input[..input.len().min(MAX_MOBILITY_RUNTIME_INPUT_BYTES)];
    let mut observation = MobilityRuntimeObservation {
        consumed_len: bounded.len(),
        generated_scenarios: 0,
        generated_steps: 0,
        scenarios: Vec::with_capacity(bounded.len()),
        step_reports: Vec::new(),
        state_hashes: Vec::new(),
        encodings: Vec::new(),
        coverage: MobilityRuntimeCoverage::default(),
        execution: MobilityRuntimeExecutionObservation::Completed,
    };

    let package = match decode_package(ArtifactBytes {
        scenario: REFERENCE_SCENARIO,
        numeric_profile: REFERENCE_NUMERIC_PROFILE,
        physical_scale_profile: REFERENCE_PHYSICAL_SCALE_PROFILE,
        balance_profile: REFERENCE_BALANCE_PROFILE,
    }) {
        Ok(package) => package,
        Err(error) => {
            observation.execution = MobilityRuntimeExecutionObservation::PackageRejected(error);
            return observation;
        }
    };

    for (case_index, &selector) in bounded.iter().enumerate() {
        let scenario = MobilityRuntimeScenario::from_selector(selector);
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
            observation.execution = MobilityRuntimeExecutionObservation::ExpectationMismatch {
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
    coverage: MobilityRuntimeCoverage,
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
    fn into_observation(self) -> MobilityRuntimeExecutionObservation {
        match self {
            Self::SimulationRejected {
                case_index,
                replica,
                error,
            } => MobilityRuntimeExecutionObservation::SimulationRejected {
                case_index,
                replica,
                error,
            },
            Self::Run {
                case_index,
                step_index,
                replica,
                error,
            } => MobilityRuntimeExecutionObservation::RunError {
                case_index,
                step_index,
                replica,
                error,
            },
            Self::Encoder {
                case_index,
                step_index,
            } => MobilityRuntimeExecutionObservation::EncoderMismatch {
                case_index,
                step_index,
            },
            Self::Determinism {
                case_index,
                step_index,
            } => MobilityRuntimeExecutionObservation::DeterminismMismatch {
                case_index,
                step_index,
            },
            Self::Expectation {
                case_index,
                step_index,
            } => MobilityRuntimeExecutionObservation::ExpectationMismatch {
                case_index,
                step_index,
            },
        }
    }
}

struct PairRunner {
    case_index: usize,
    left: Simulation,
    right: Simulation,
    reports: Vec<StepReport>,
    hashes: Vec<StateHash>,
    encodings: Vec<CommandEncodingObservation>,
    coverage: MobilityRuntimeCoverage,
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
            reports: Vec::new(),
            hashes: Vec::new(),
            encodings: Vec::new(),
            coverage: MobilityRuntimeCoverage::default(),
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

        let mut left_snapshot = RenderSnapshot::default();
        let mut right_snapshot = RenderSnapshot::default();
        self.left.write_render_snapshot(&mut left_snapshot);
        self.right.write_render_snapshot(&mut right_snapshot);
        if left_report != right_report
            || left_report.state_hash != self.left.state_hash()
            || right_report.state_hash != self.right.state_hash()
            || self.left.state_hash() != self.right.state_hash()
            || left_snapshot != right_snapshot
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

    fn snapshot(&self) -> RenderSnapshot {
        let mut snapshot = RenderSnapshot::default();
        self.left.write_render_snapshot(&mut snapshot);
        snapshot
    }

    fn expect(&self, condition: bool) -> Result<(), ScenarioFailure> {
        if condition {
            Ok(())
        } else {
            Err(self.expectation_failure(self.reports.len().saturating_sub(1)))
        }
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

fn run_scenario(
    package: aon_sim::SimulationPackage,
    case_index: usize,
    scenario: MobilityRuntimeScenario,
) -> Result<ScenarioTrace, ScenarioFailure> {
    let mut runner = PairRunner::new(package, case_index)?;
    match scenario {
        MobilityRuntimeScenario::Straight => {
            run_turn(&mut runner, LogicLevel::Low, LogicLevel::Low)?
        }
        MobilityRuntimeScenario::Left => run_turn(&mut runner, LogicLevel::High, LogicLevel::Low)?,
        MobilityRuntimeScenario::Right => run_turn(&mut runner, LogicLevel::Low, LogicLevel::High)?,
        MobilityRuntimeScenario::Reverse => {
            run_turn(&mut runner, LogicLevel::High, LogicLevel::High)?
        }
        MobilityRuntimeScenario::BindAndRemove => run_bind_and_remove(&mut runner)?,
        MobilityRuntimeScenario::PlacementRollback => run_placement_rollback(&mut runner)?,
        MobilityRuntimeScenario::CheckedMaximumCoordinate => {
            run_checked_coordinate(&mut runner, true)?
        }
        MobilityRuntimeScenario::CheckedMinimumCoordinate => {
            run_checked_coordinate(&mut runner, false)?
        }
    }
    runner.finish()
}

fn run_turn(
    runner: &mut PairRunner,
    left: LogicLevel,
    right: LogicLevel,
) -> Result<(), ScenarioFailure> {
    let junction = JunctionId(EntityId(1));
    let incoming = WireId(EntityId(2));
    let straight = WireId(EntityId(3));
    let left_edge = WireId(EntityId(4));
    let right_edge = WireId(EntityId(5));
    let mobile = MobileId(EntityId(6));

    let junction_report = runner.step(vec![Command::PlaceJunction(PlaceJunctionCommand {
        routing_domain: RoutingDomain::OpenWorld,
        position: point(0, 0),
    })])?;
    runner.expect(all_accepted(&junction_report, 1))?;
    runner.expect(created_entities(&junction_report) == vec![junction.entity_id()])?;

    let track_report = runner.step(vec![
        track(
            point(-TURN_EDGE_PITCHES * WORLD_PITCH, 0),
            point(0, 0),
            EndpointTarget::Free,
            EndpointTarget::Junction(junction),
        ),
        track(
            point(0, 0),
            point(TURN_EDGE_PITCHES * WORLD_PITCH, 0),
            EndpointTarget::Junction(junction),
            EndpointTarget::Free,
        ),
        track(
            point(0, 0),
            point(0, TURN_EDGE_PITCHES * WORLD_PITCH),
            EndpointTarget::Junction(junction),
            EndpointTarget::Free,
        ),
        track(
            point(0, 0),
            point(0, -TURN_EDGE_PITCHES * WORLD_PITCH),
            EndpointTarget::Junction(junction),
            EndpointTarget::Free,
        ),
    ])?;
    runner.expect(all_accepted(&track_report, 4))?;
    runner.expect(
        created_entities(&track_report)
            == vec![
                incoming.entity_id(),
                straight.entity_id(),
                left_edge.entity_id(),
                right_edge.entity_id(),
            ],
    )?;

    let placement = runner.step(vec![mobile_command(point(
        -TURN_START_PITCHES * WORLD_PITCH,
        0,
    ))])?;
    runner.expect(all_accepted(&placement, 1))?;
    runner.expect(created_entities(&placement) == vec![mobile.entity_id()])?;
    runner.coverage.mobile_placements = 1;

    let high_ports: Vec<_> = [(MobilePort::Left, left), (MobilePort::Right, right)]
        .into_iter()
        .filter_map(|(port, level)| (level == LogicLevel::High).then_some(port))
        .collect();
    if !high_ports.is_empty() {
        let gates = runner.step(
            high_ports
                .iter()
                .map(|port| {
                    Command::PlaceGate(PlaceGateCommand {
                        gate_type: GateType::Not,
                        origin: point(0, port_y(*port)),
                        routing_domain: RoutingDomain::MobileSubstrate(mobile.entity_id()),
                    })
                })
                .collect(),
        )?;
        runner.expect(all_accepted(&gates, high_ports.len()))?;
        let gate_ids: Vec<_> = created_entities(&gates).into_iter().map(GateId).collect();
        let bindings = runner.step(
            gate_ids
                .into_iter()
                .zip(high_ports.iter().copied())
                .map(|(gate, port)| {
                    let y = port_y(port);
                    Command::PlaceWire(PlaceWireCommand {
                        routing_domain: RoutingDomain::MobileSubstrate(mobile.entity_id()),
                        points: vec![point(CIRCUIT_PITCH, y), point(3 * CIRCUIT_PITCH, y)],
                        endpoint_a: EndpointTarget::GatePort(GatePortRef {
                            gate,
                            port: GatePort::Output,
                        }),
                        endpoint_b: EndpointTarget::MobilePort(MobilePortRef { mobile, port }),
                    })
                })
                .collect(),
        )?;
        runner.expect(all_accepted(&bindings, high_ports.len()))?;
        runner.coverage.mobile_port_bindings =
            u64::try_from(high_ports.len()).map_err(|_| runner.expectation_failure(4))?;
    }

    let expected_kind = match (left, right) {
        (LogicLevel::Low, LogicLevel::Low) => JunctionDecisionKind::Straight,
        (LogicLevel::High, LogicLevel::Low) => JunctionDecisionKind::Left,
        (LogicLevel::Low, LogicLevel::High) => JunctionDecisionKind::Right,
        (LogicLevel::High, LogicLevel::High) => JunctionDecisionKind::Reverse,
        _ => return Err(runner.expectation_failure(runner.reports.len())),
    };
    let expected_edge = match expected_kind {
        JunctionDecisionKind::Straight => straight,
        JunctionDecisionKind::Left => left_edge,
        JunctionDecisionKind::Right => right_edge,
        JunctionDecisionKind::Reverse => incoming,
        JunctionDecisionKind::MissingRequestedSide => {
            return Err(runner.expectation_failure(runner.reports.len()));
        }
    };

    for _ in 0..MAX_TURN_SEARCH_STEPS {
        let report = runner.step(Vec::new())?;
        if let Some(decision) = report
            .mobile_movements
            .iter()
            .flat_map(|movement| &movement.junction_decisions)
            .next()
        {
            runner.expect(decision.kind == expected_kind)?;
            runner.expect(decision.selected_edge == Some(expected_edge))?;
            let movement = &report.mobile_movements[0];
            runner.expect(movement.controls.stop == LogicLevel::Low)?;
            runner.expect(movement.controls.left == left)?;
            runner.expect(movement.controls.right == right)?;
            runner.expect(movement.consumed_budget == Fixed(WORLD_PITCH))?;
            runner.expect(match expected_kind {
                JunctionDecisionKind::Reverse => {
                    movement.end
                        == TrackPosition::Edge {
                            edge: incoming,
                            offset: Fixed((TURN_EDGE_PITCHES - 1) * WORLD_PITCH),
                            heading: Heading::Reverse,
                        }
                }
                _ => {
                    movement.end
                        == TrackPosition::Edge {
                            edge: expected_edge,
                            offset: Fixed(WORLD_PITCH),
                            heading: Heading::Forward,
                        }
                }
            })?;
            return Ok(());
        }
    }
    Err(runner.expectation_failure(runner.reports.len().saturating_sub(1)))
}

fn run_bind_and_remove(runner: &mut PairRunner) -> Result<(), ScenarioFailure> {
    let junction = JunctionId(EntityId(1));
    let wire = WireId(EntityId(2));
    let mobile = MobileId(EntityId(3));
    let junction_report = runner.step(vec![Command::PlaceJunction(PlaceJunctionCommand {
        routing_domain: RoutingDomain::OpenWorld,
        position: point(4 * WORLD_PITCH, 0),
    })])?;
    runner.expect(all_accepted(&junction_report, 1))?;
    let wire_report = runner.step(vec![track(
        point(0, 0),
        point(4 * WORLD_PITCH, 0),
        EndpointTarget::Free,
        EndpointTarget::Free,
    )])?;
    runner.expect(all_accepted(&wire_report, 1))?;

    let binding = runner.step(vec![Command::BindPort(BindPortCommand {
        wire,
        end: WireEnd::B,
        target: EndpointTarget::Junction(junction),
    })])?;
    runner.expect(all_accepted(&binding, 1))?;
    runner.coverage.explicit_track_bindings = 1;

    let placement = runner.step(vec![mobile_command(point(WORLD_PITCH, 0))])?;
    runner.expect(all_accepted(&placement, 1))?;
    runner.expect(created_entities(&placement) == vec![mobile.entity_id()])?;
    runner.coverage.mobile_placements = 1;

    let occupied_binding = runner.step(vec![Command::BindPort(BindPortCommand {
        wire,
        end: WireEnd::B,
        target: EndpointTarget::Free,
    })])?;
    runner.expect(rejected_with(
        &occupied_binding,
        CommandRejectionReason::TrackOccupied,
    ))?;
    let occupied_removal = runner.step(vec![Command::RemoveEntity(RemoveEntityCommand {
        target: wire.entity_id(),
    })])?;
    runner.expect(rejected_with(
        &occupied_removal,
        CommandRejectionReason::TrackOccupied,
    ))?;
    runner.coverage.occupied_track_rejections = 2;

    let mobile_removal = runner.step(vec![Command::RemoveEntity(RemoveEntityCommand {
        target: mobile.entity_id(),
    })])?;
    runner.expect(all_accepted(&mobile_removal, 1))?;
    runner.expect(mobile_removal.mobile_movements.is_empty())?;
    runner.expect(runner.snapshot().mobiles().is_empty())?;
    runner.coverage.mobile_removals = 1;

    let unbind = runner.step(vec![Command::BindPort(BindPortCommand {
        wire,
        end: WireEnd::B,
        target: EndpointTarget::Free,
    })])?;
    runner.expect(all_accepted(&unbind, 1))?;
    runner.coverage.explicit_track_bindings = 2;
    let removals = runner.step(vec![
        Command::RemoveEntity(RemoveEntityCommand {
            target: wire.entity_id(),
        }),
        Command::RemoveEntity(RemoveEntityCommand {
            target: junction.entity_id(),
        }),
    ])?;
    runner.expect(all_accepted(&removals, 2))?;
    runner.coverage.track_removals = 2;
    Ok(())
}

fn run_placement_rollback(runner: &mut PairRunner) -> Result<(), ScenarioFailure> {
    let wire = WireId(EntityId(1));
    let track_report = runner.step(vec![track(
        point(0, 0),
        point(8 * WORLD_PITCH, 0),
        EndpointTarget::Free,
        EndpointTarget::Free,
    )])?;
    runner.expect(all_accepted(&track_report, 1))?;
    runner.expect(created_entities(&track_report) == vec![wire.entity_id()])?;

    let rejected = runner.step(vec![mobile_command(point(WORLD_PITCH, WORLD_PITCH))])?;
    runner.expect(rejected_with(
        &rejected,
        CommandRejectionReason::UnsupportedPlacement,
    ))?;
    runner.expect(runner.snapshot().mobiles().is_empty())?;
    runner.coverage.placement_rejections = 1;

    let accepted = runner.step(vec![mobile_command(point(2 * WORLD_PITCH, 0))])?;
    runner.expect(all_accepted(&accepted, 1))?;
    runner.expect(created_entities(&accepted) == vec![EntityId(2)])?;
    runner.expect(runner.snapshot().mobiles()[0].id.entity_id() == EntityId(2))?;
    runner.coverage.mobile_placements = 1;
    Ok(())
}

fn run_checked_coordinate(runner: &mut PairRunner, maximum: bool) -> Result<(), ScenarioFailure> {
    let (start, end, origin, expected_endpoint) = if maximum {
        (
            MAX_QUANTIZED_COORDINATE - 4 * WORLD_PITCH,
            MAX_QUANTIZED_COORDINATE,
            MAX_QUANTIZED_COORDINATE - WORLD_PITCH,
            MAX_QUANTIZED_COORDINATE,
        )
    } else {
        (
            i64::MIN + 4 * WORLD_PITCH,
            i64::MIN,
            i64::MIN + WORLD_PITCH,
            i64::MIN,
        )
    };
    let wire = WireId(EntityId(1));
    let track_report = runner.step(vec![track(
        point(start, 0),
        point(end, 0),
        EndpointTarget::Free,
        EndpointTarget::Free,
    )])?;
    runner.expect(all_accepted(&track_report, 1))?;
    let placement = runner.step(vec![mobile_command(point(origin, 0))])?;
    runner.expect(all_accepted(&placement, 1))?;
    runner.expect(placement.mobile_movements.len() == 1)?;
    let snapshot = runner.snapshot();
    runner.expect(snapshot.mobiles()[0].world_position == point(expected_endpoint, 0))?;
    runner.expect(
        snapshot.mobiles()[0].track_position
            == TrackPosition::Edge {
                edge: wire,
                offset: Fixed(4 * WORLD_PITCH),
                heading: Heading::Forward,
            },
    )?;

    let bounce = runner.step(Vec::new())?;
    runner.expect(bounce.mobile_movements.len() == 1)?;
    runner.expect(
        bounce.mobile_movements[0].end
            == TrackPosition::Edge {
                edge: wire,
                offset: Fixed(3 * WORLD_PITCH),
                heading: Heading::Reverse,
            },
    )?;
    runner.expect(bounce.mobile_movements[0].consumed_budget == Fixed(WORLD_PITCH))?;
    runner.coverage.mobile_placements = 1;
    if maximum {
        runner.coverage.checked_maximum_coordinate_paths = 1;
    } else {
        runner.coverage.checked_minimum_coordinate_paths = 1;
    }
    Ok(())
}

fn track(
    start: FixedVec2,
    end: FixedVec2,
    endpoint_a: EndpointTarget,
    endpoint_b: EndpointTarget,
) -> Command {
    Command::PlaceWire(PlaceWireCommand {
        routing_domain: RoutingDomain::OpenWorld,
        points: vec![start, end],
        endpoint_a,
        endpoint_b,
    })
}

fn mobile_command(origin: FixedVec2) -> Command {
    let bounds = FixedAabb::new(
        point(-4 * CIRCUIT_PITCH, -4 * CIRCUIT_PITCH),
        point(4 * CIRCUIT_PITCH, 4 * CIRCUIT_PITCH),
    );
    Command::PlaceMobileSubstrate(PlaceMobileSubstrateCommand {
        origin,
        routing_area: bounds,
        footprint: bounds,
    })
}

fn port_y(port: MobilePort) -> i64 {
    match port {
        MobilePort::Stop => 0,
        MobilePort::Left => -2 * CIRCUIT_PITCH,
        MobilePort::Right => 2 * CIRCUIT_PITCH,
        MobilePort::Build => unreachable!("BUILD has no fixed Stage-0 control-port anchor"),
    }
}

fn all_accepted(report: &StepReport, command_count: usize) -> bool {
    report.command_acceptances.len() == command_count && report.command_rejections.is_empty()
}

fn created_entities(report: &StepReport) -> Vec<EntityId> {
    report
        .command_acceptances
        .iter()
        .filter_map(|acceptance| acceptance.created_entity)
        .collect()
}

fn rejected_with(report: &StepReport, reason: CommandRejectionReason) -> bool {
    report.command_acceptances.is_empty()
        && report.command_rejections.len() == 1
        && report.command_rejections[0].reason == reason
}

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_MOBILITY_RUNTIME_INPUT_BYTES, MobilityRuntimeExecutionObservation,
        exercise_mobility_runtime,
    };
    use std::panic::{AssertUnwindSafe, catch_unwind};

    #[test]
    fn all_scenarios_reach_verified_s0_m7_outcomes() {
        let observation = exercise_mobility_runtime(&[0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(
            observation.execution,
            MobilityRuntimeExecutionObservation::Completed
        );
        assert_eq!(observation.invariant_failure(), None);
        assert_eq!(observation.generated_scenarios, 8);
        assert_eq!(observation.state_hashes.len(), observation.generated_steps);
        let coverage = observation.coverage;
        assert_eq!(coverage.completed_scenarios, 8);
        assert!(coverage.permuted_command_batches >= 4);
        assert_eq!(coverage.mobile_placements, 8);
        assert_eq!(coverage.placement_rejections, 1);
        assert_eq!(coverage.mobile_port_bindings, 4);
        assert_eq!(coverage.explicit_track_bindings, 2);
        assert_eq!(coverage.occupied_track_rejections, 2);
        assert_eq!(coverage.mobile_removals, 1);
        assert_eq!(coverage.track_removals, 2);
        assert_eq!(coverage.straight_turns, 1);
        assert_eq!(coverage.left_turns, 1);
        assert_eq!(coverage.right_turns, 1);
        assert_eq!(coverage.reverse_turns, 1);
        assert!(coverage.movement_observations > 0);
        assert_eq!(coverage.checked_maximum_coordinate_paths, 1);
        assert_eq!(coverage.checked_minimum_coordinate_paths, 1);
    }

    #[test]
    fn arbitrary_input_is_bounded_deterministic_and_never_panics() {
        let mut input = vec![0xa5; MAX_MOBILITY_RUNTIME_INPUT_BYTES];
        let expected = exercise_mobility_runtime(&input);
        input.extend_from_slice(b"ignored mobility-runtime suffix");
        let replay = catch_unwind(AssertUnwindSafe(|| exercise_mobility_runtime(&input)));
        let Ok(replay) = replay else {
            panic!("bounded mobility-runtime input panicked");
        };
        assert_eq!(replay, expected);
        assert_eq!(replay.invariant_failure(), None);
    }

    #[test]
    fn deterministic_generated_streams_complete_without_panics() {
        let mut state = 0x43cb_95e1_76d4_2a09_u64;
        for case_index in 0..24 {
            let len = case_index % (MAX_MOBILITY_RUNTIME_INPUT_BYTES + 1);
            let mut bytes = Vec::with_capacity(len);
            for _ in 0..len {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                bytes.push(state as u8);
            }
            let result = catch_unwind(AssertUnwindSafe(|| exercise_mobility_runtime(&bytes)));
            let Ok(observation) = result else {
                panic!("mobility runtime panicked for generated case {case_index}");
            };
            assert_eq!(
                observation.execution,
                MobilityRuntimeExecutionObservation::Completed,
                "generated case {case_index} did not complete"
            );
            assert_eq!(observation.invariant_failure(), None);
        }
    }
}
