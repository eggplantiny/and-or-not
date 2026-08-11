#![forbid(unsafe_code)]

use aon_sim::{
    ArtifactBytes, BindPortCommand, Command, CommandEncodingError, CommandEnvelope, DriveStrength,
    DriverId, EndpointTarget, EntityId, Fixed, FixedAabb, FixedVec2, GateId, GatePort, GatePortRef,
    GateType, GeometryError, JunctionId, LogicLevel, NumericError, PackageError,
    PlaceFixedSubstrateCommand, PlaceGateCommand, PlaceJunctionCommand,
    PlaceMobileSubstrateCommand, PlaceWireCommand, RemoveEntityCommand, RoutingDomain,
    SetExternalDriverCommand, Simulation, SimulationError, StepReport, Tick, WireEnd, WireId,
    decode_package, decode_scenario_manifest, polyline_length, validate_quantized,
};

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

/// Maximum number of bytes interpreted by one decoder invocation.
pub const MAX_DECODER_INPUT_BYTES: usize = 16 * 1024;

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
        MAX_DECODER_INPUT_BYTES, MAX_GEOMETRY_INPUT_BYTES, REFERENCE_BALANCE_PROFILE,
        REFERENCE_NUMERIC_PROFILE, REFERENCE_PHYSICAL_SCALE_PROFILE, REFERENCE_SCENARIO,
        STATEFUL_BATCH_TICK, STATEFUL_GATE_ID, STATEFUL_JUNCTION_ID, STATEFUL_TOMBSTONE_ID,
        STATEFUL_WIRE_ID, StatefulCommandExecutionObservation, exercise_commands, exercise_decoder,
        exercise_geometry, exercise_stateful_commands, stateful_envelope, stateful_prefix_batches,
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
                (4, CommandRejectionReason::UnsupportedPlacement),
                (5, CommandRejectionReason::UnknownEntity),
                (6, CommandRejectionReason::UnknownEntity),
                (7, CommandRejectionReason::UnsupportedCommand),
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
