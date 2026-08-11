use crate::geometry::FixedVec2;
use crate::identity::{DriverId, WireId};
use crate::numeric::{DriveStrength, EntityId, Tick};
use crate::topology::{EndpointTarget, FixedAabb, GatePort, GateType, RoutingDomain, WireEnd};
use thiserror::Error;

const COMMAND_DOMAIN: &[u8] = b"AON\0COMMAND\0V1\0";
const COMMAND_ENCODER_VERSION: u16 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandEnvelope {
    pub target_tick: Tick,
    pub ordinal: u64,
    pub command: Command,
}

impl CommandEnvelope {
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, CommandEncodingError> {
        let mut output = Vec::new();
        self.write_canonical(&mut |bytes| output.extend_from_slice(bytes))?;
        Ok(output)
    }

    pub fn write_canonical(
        &self,
        write: &mut dyn FnMut(&[u8]),
    ) -> Result<(), CommandEncodingError> {
        write(COMMAND_DOMAIN);
        write_u16(COMMAND_ENCODER_VERSION, write);
        write_u64(self.target_tick.0, write);
        write_u64(self.ordinal, write);
        self.command.encode_canonical(write)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    PlaceGate(PlaceGateCommand),
    PlaceWire(PlaceWireCommand),
    PlaceJunction(PlaceJunctionCommand),
    PlaceFixedSubstrate(PlaceFixedSubstrateCommand),
    PlaceMobileSubstrate(PlaceMobileSubstrateCommand),
    RemoveEntity(RemoveEntityCommand),
    BindPort(BindPortCommand),
    SetExternalDriver(SetExternalDriverCommand),
}

impl Command {
    fn encode_canonical(&self, write: &mut dyn FnMut(&[u8])) -> Result<(), CommandEncodingError> {
        write_u8(self.canonical_tag(), write);
        match self {
            Self::PlaceGate(command) => {
                encode_gate_type(command.gate_type, write);
                encode_point(command.origin, write);
                encode_routing_domain(command.routing_domain, write);
            }
            Self::PlaceWire(command) => {
                encode_routing_domain(command.routing_domain, write);
                encode_points(&command.points, write)?;
                encode_endpoint_target(command.endpoint_a, write);
                encode_endpoint_target(command.endpoint_b, write);
            }
            Self::PlaceJunction(command) => {
                encode_routing_domain(command.routing_domain, write);
                encode_point(command.position, write);
            }
            Self::PlaceFixedSubstrate(command) => {
                encode_point(command.origin, write);
                encode_aabb(command.routing_area, write);
                encode_aabb(command.footprint, write);
            }
            Self::PlaceMobileSubstrate(command) => {
                encode_point(command.origin, write);
                encode_aabb(command.routing_area, write);
                encode_aabb(command.footprint, write);
            }
            Self::RemoveEntity(command) => encode_entity_id(command.target, write),
            Self::BindPort(command) => {
                encode_entity_id(command.wire.entity_id(), write);
                encode_wire_end(command.end, write);
                encode_endpoint_target(command.target, write);
            }
            Self::SetExternalDriver(command) => {
                encode_entity_id(command.driver.entity_id(), write);
                encode_logic_level(command.level, write);
                write_u64(command.strength.0, write);
            }
        }
        Ok(())
    }

    const fn canonical_tag(&self) -> u8 {
        match self {
            Self::PlaceGate(_) => 0,
            Self::PlaceWire(_) => 1,
            Self::PlaceJunction(_) => 2,
            Self::PlaceFixedSubstrate(_) => 3,
            Self::PlaceMobileSubstrate(_) => 4,
            Self::RemoveEntity(_) => 5,
            Self::BindPort(_) => 6,
            Self::SetExternalDriver(_) => 7,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlaceGateCommand {
    pub gate_type: GateType,
    pub origin: FixedVec2,
    pub routing_domain: RoutingDomain,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaceWireCommand {
    pub routing_domain: RoutingDomain,
    pub points: Vec<FixedVec2>,
    pub endpoint_a: EndpointTarget,
    pub endpoint_b: EndpointTarget,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlaceJunctionCommand {
    pub routing_domain: RoutingDomain,
    pub position: FixedVec2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlaceFixedSubstrateCommand {
    pub origin: FixedVec2,
    pub routing_area: FixedAabb,
    pub footprint: FixedAabb,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlaceMobileSubstrateCommand {
    pub origin: FixedVec2,
    pub routing_area: FixedAabb,
    pub footprint: FixedAabb,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RemoveEntityCommand {
    pub target: EntityId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BindPortCommand {
    pub wire: WireId,
    pub end: WireEnd,
    pub target: EndpointTarget,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetExternalDriverCommand {
    pub driver: DriverId,
    pub level: LogicLevel,
    pub strength: DriveStrength,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogicLevel {
    Low,
    High,
    X,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandAcceptance {
    pub target_tick: Tick,
    pub ordinal: u64,
    pub created_entity: Option<EntityId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandRejection {
    pub target_tick: Tick,
    pub ordinal: u64,
    pub reason: CommandRejectionReason,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandRejectionReason {
    DuplicateOrdinal,
    WrongTick,
    UnknownEntity,
    RemovedEntity,
    InvalidGeometryQuantum,
    InvalidRoutingPitch,
    InvalidGeometryShape,
    ZeroLengthSegment,
    GeometryOverlap,
    InsufficientSpacing,
    UnsupportedPlacement,
    UnsupportedCommand,
    InvalidRoutingDomain,
    InvalidEndpoint,
    InvalidPort,
    InvalidPortBinding,
    SubstrateBoundsViolation,
    SubstrateInUse,
}

impl CommandRejectionReason {
    pub const fn canonical_tag(self) -> u8 {
        match self {
            Self::DuplicateOrdinal => 0,
            Self::WrongTick => 1,
            Self::UnknownEntity => 2,
            Self::RemovedEntity => 3,
            Self::InvalidGeometryQuantum => 4,
            Self::InvalidRoutingPitch => 5,
            Self::InvalidGeometryShape => 6,
            Self::ZeroLengthSegment => 7,
            Self::GeometryOverlap => 8,
            Self::InsufficientSpacing => 9,
            Self::UnsupportedPlacement => 10,
            Self::UnsupportedCommand => 11,
            Self::InvalidRoutingDomain => 12,
            Self::InvalidEndpoint => 13,
            Self::InvalidPort => 14,
            Self::InvalidPortBinding => 15,
            Self::SubstrateBoundsViolation => 16,
            Self::SubstrateInUse => 17,
        }
    }
}

#[derive(Clone, Copy, Debug, Error, PartialEq, Eq)]
pub enum CommandEncodingError {
    #[error("command point count {count} exceeds the canonical u32 length limit")]
    PointCountExceedsU32 { count: usize },
}

fn encode_points(
    points: &[FixedVec2],
    write: &mut dyn FnMut(&[u8]),
) -> Result<(), CommandEncodingError> {
    let count = canonical_point_count(points.len())?;
    write_u32(count, write);
    for &point in points {
        encode_point(point, write);
    }
    Ok(())
}

fn canonical_point_count(count: usize) -> Result<u32, CommandEncodingError> {
    u32::try_from(count).map_err(|_| CommandEncodingError::PointCountExceedsU32 { count })
}

fn encode_point(point: FixedVec2, write: &mut dyn FnMut(&[u8])) {
    write_i64(point.x.0, write);
    write_i64(point.y.0, write);
}

fn encode_aabb(aabb: FixedAabb, write: &mut dyn FnMut(&[u8])) {
    encode_point(aabb.min, write);
    encode_point(aabb.max, write);
}

fn encode_gate_type(gate_type: GateType, write: &mut dyn FnMut(&[u8])) {
    write_u8(gate_type.canonical_tag(), write);
}

fn encode_routing_domain(domain: RoutingDomain, write: &mut dyn FnMut(&[u8])) {
    write_u8(domain.canonical_tag(), write);
    match domain {
        RoutingDomain::OpenWorld => {}
        RoutingDomain::FixedSubstrate(entity) => {
            encode_entity_id(entity, write);
        }
        RoutingDomain::MobileSubstrate(entity) => {
            encode_entity_id(entity, write);
        }
    }
}

fn encode_wire_end(end: WireEnd, write: &mut dyn FnMut(&[u8])) {
    write_u8(end.canonical_tag(), write);
}

fn encode_endpoint_target(target: EndpointTarget, write: &mut dyn FnMut(&[u8])) {
    write_u8(target.canonical_tag(), write);
    match target {
        EndpointTarget::Free => {}
        EndpointTarget::Junction(junction) => {
            encode_entity_id(junction.entity_id(), write);
        }
        EndpointTarget::GatePort(reference) => {
            encode_entity_id(reference.gate.entity_id(), write);
            encode_gate_port(reference.port, write);
        }
    }
}

fn encode_gate_port(port: GatePort, write: &mut dyn FnMut(&[u8])) {
    write_u8(port.canonical_tag(), write);
}

fn encode_logic_level(level: LogicLevel, write: &mut dyn FnMut(&[u8])) {
    write_u8(
        match level {
            LogicLevel::Low => 0,
            LogicLevel::High => 1,
            LogicLevel::X => 2,
        },
        write,
    );
}

fn encode_entity_id(entity: EntityId, write: &mut dyn FnMut(&[u8])) {
    write_u64(entity.0, write);
}

fn write_u8(value: u8, write: &mut dyn FnMut(&[u8])) {
    write(&[value]);
}

fn write_u16(value: u16, write: &mut dyn FnMut(&[u8])) {
    write(&value.to_le_bytes());
}

fn write_u32(value: u32, write: &mut dyn FnMut(&[u8])) {
    write(&value.to_le_bytes());
}

fn write_u64(value: u64, write: &mut dyn FnMut(&[u8])) {
    write(&value.to_le_bytes());
}

fn write_i64(value: i64, write: &mut dyn FnMut(&[u8])) {
    write(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{GateId, JunctionId};
    use crate::numeric::Fixed;
    use crate::topology::GatePortRef;

    fn point(x: i64, y: i64) -> FixedVec2 {
        FixedVec2::new(Fixed(x), Fixed(y))
    }

    #[test]
    fn place_gate_envelope_has_exact_v1_canonical_bytes() {
        let envelope = CommandEnvelope {
            target_tick: Tick(0x0102_0304_0506_0708),
            ordinal: 0x1112_1314_1516_1718,
            command: Command::PlaceGate(PlaceGateCommand {
                gate_type: GateType::Or,
                origin: point(-2, 3),
                routing_domain: RoutingDomain::FixedSubstrate(EntityId(4)),
            }),
        };

        let mut expected = Vec::new();
        expected.extend_from_slice(COMMAND_DOMAIN);
        expected.extend_from_slice(&COMMAND_ENCODER_VERSION.to_le_bytes());
        expected.extend_from_slice(&envelope.target_tick.0.to_le_bytes());
        expected.extend_from_slice(&envelope.ordinal.to_le_bytes());
        expected.push(0);
        expected.push(1);
        expected.extend_from_slice(&(-2_i64).to_le_bytes());
        expected.extend_from_slice(&3_i64.to_le_bytes());
        expected.push(1);
        expected.extend_from_slice(&4_u64.to_le_bytes());

        assert_eq!(envelope.canonical_bytes(), Ok(expected));
    }

    #[test]
    fn streaming_and_collected_command_encodings_are_identical() {
        let envelope = CommandEnvelope {
            target_tick: Tick(4),
            ordinal: 9,
            command: Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: RoutingDomain::OpenWorld,
                position: point(-8, 16),
            }),
        };
        let mut streamed = Vec::new();

        envelope
            .write_canonical(&mut |bytes| streamed.extend_from_slice(bytes))
            .expect("streaming command encoding succeeds");

        assert_eq!(streamed, envelope.canonical_bytes().unwrap());
    }

    #[test]
    fn wire_and_binding_encoding_uses_explicit_lengths_and_topology_tags() {
        let wire = CommandEnvelope {
            target_tick: Tick(9),
            ordinal: 2,
            command: Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::MobileSubstrate(EntityId(7)),
                points: vec![point(-1, 2), point(3, -4)],
                endpoint_a: EndpointTarget::Free,
                endpoint_b: EndpointTarget::GatePort(GatePortRef {
                    gate: GateId(EntityId(11)),
                    port: GatePort::Power,
                }),
            }),
        }
        .canonical_bytes()
        .expect("wire command encodes");

        let payload_offset = COMMAND_DOMAIN.len() + 2 + 8 + 8;
        assert_eq!(wire[payload_offset], 1);
        assert_eq!(wire[payload_offset + 1], 2);
        assert_eq!(
            &wire[payload_offset + 10..payload_offset + 14],
            &2_u32.to_le_bytes()
        );
        assert!(wire.ends_with(&[2, 11, 0, 0, 0, 0, 0, 0, 0, 3]));

        let binding = CommandEnvelope {
            target_tick: Tick(9),
            ordinal: 3,
            command: Command::BindPort(BindPortCommand {
                wire: WireId(EntityId(5)),
                end: WireEnd::B,
                target: EndpointTarget::Junction(JunctionId(EntityId(12))),
            }),
        }
        .canonical_bytes()
        .expect("binding command encodes");
        assert!(binding.ends_with(&[5, 0, 0, 0, 0, 0, 0, 0, 1, 1, 12, 0, 0, 0, 0, 0, 0, 0]));
    }

    #[test]
    fn every_stage0_command_has_a_unique_stable_tag() {
        let aabb = FixedAabb {
            min: point(-1, -1),
            max: point(1, 1),
        };
        let commands = [
            Command::PlaceGate(PlaceGateCommand {
                gate_type: GateType::And,
                origin: point(0, 0),
                routing_domain: RoutingDomain::OpenWorld,
            }),
            Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::OpenWorld,
                points: vec![point(0, 0), point(1, 0)],
                endpoint_a: EndpointTarget::Free,
                endpoint_b: EndpointTarget::Free,
            }),
            Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: RoutingDomain::OpenWorld,
                position: point(0, 0),
            }),
            Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
                origin: point(0, 0),
                routing_area: aabb,
                footprint: aabb,
            }),
            Command::PlaceMobileSubstrate(PlaceMobileSubstrateCommand {
                origin: point(0, 0),
                routing_area: aabb,
                footprint: aabb,
            }),
            Command::RemoveEntity(RemoveEntityCommand {
                target: EntityId(1),
            }),
            Command::BindPort(BindPortCommand {
                wire: WireId(EntityId(1)),
                end: WireEnd::A,
                target: EndpointTarget::Free,
            }),
            Command::SetExternalDriver(SetExternalDriverCommand {
                driver: DriverId(EntityId(1)),
                level: LogicLevel::X,
                strength: DriveStrength(2),
            }),
        ];

        let tags: Vec<_> = commands.iter().map(Command::canonical_tag).collect();
        assert_eq!(tags, (0_u8..=7).collect::<Vec<_>>());
    }

    #[test]
    fn rejection_reason_tags_cover_the_frozen_surface_in_order() {
        let reasons = [
            CommandRejectionReason::DuplicateOrdinal,
            CommandRejectionReason::WrongTick,
            CommandRejectionReason::UnknownEntity,
            CommandRejectionReason::RemovedEntity,
            CommandRejectionReason::InvalidGeometryQuantum,
            CommandRejectionReason::InvalidRoutingPitch,
            CommandRejectionReason::InvalidGeometryShape,
            CommandRejectionReason::ZeroLengthSegment,
            CommandRejectionReason::GeometryOverlap,
            CommandRejectionReason::InsufficientSpacing,
            CommandRejectionReason::UnsupportedPlacement,
            CommandRejectionReason::UnsupportedCommand,
            CommandRejectionReason::InvalidRoutingDomain,
            CommandRejectionReason::InvalidEndpoint,
            CommandRejectionReason::InvalidPort,
            CommandRejectionReason::InvalidPortBinding,
            CommandRejectionReason::SubstrateBoundsViolation,
            CommandRejectionReason::SubstrateInUse,
        ];

        let tags: Vec<_> = reasons
            .into_iter()
            .map(CommandRejectionReason::canonical_tag)
            .collect();
        assert_eq!(tags, (0_u8..=17).collect::<Vec<_>>());
    }

    #[test]
    fn envelope_identity_does_not_depend_on_surrounding_batch_permutation() {
        let make = |ordinal, target| CommandEnvelope {
            target_tick: Tick(17),
            ordinal,
            command: Command::RemoveEntity(RemoveEntityCommand {
                target: EntityId(target),
            }),
        };
        let first = [make(3, 30), make(1, 10), make(2, 20)];
        let second = [make(2, 20), make(3, 30), make(1, 10)];

        let canonical_set = |batch: &[CommandEnvelope]| {
            let mut encoded: Vec<_> = batch
                .iter()
                .map(|envelope| {
                    (
                        envelope.target_tick,
                        envelope.ordinal,
                        envelope.canonical_bytes().expect("command encodes"),
                    )
                })
                .collect();
            encoded.sort_unstable_by_key(|(tick, ordinal, _)| (*tick, *ordinal));
            encoded
        };

        assert_eq!(canonical_set(&first), canonical_set(&second));
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn point_count_over_u32_is_a_typed_encoding_error() {
        assert_eq!(
            canonical_point_count(usize::MAX),
            Err(CommandEncodingError::PointCountExceedsU32 { count: usize::MAX })
        );
    }
}
