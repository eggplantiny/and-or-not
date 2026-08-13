use crate::geometry::{canonical_polyline_points, segment_length};
use crate::power_source::PowerSourceStore;
use crate::power_topology::{
    CompiledPowerTopology, PowerBodyEdge, PowerLoadAttachment, PowerNodeKey, PowerSourceAttachment,
    PowerTopologyError, PowerTopologyInput,
};
use crate::structural::StructuralWorld;
use crate::topology::{EndpointTarget, GatePort, WireEnd, WireRecord};
use crate::{EntityId, Fixed, FixedVec2, NumericError, RESERVED_ENTITY_ID, WireId};
use std::cmp::Ordering;
use thiserror::Error;

/// Builds the derived physical Power graph before per-Tick loads are attached.
///
/// The returned input contains every live Wire and every canonical Power Source, but no loads.
/// Callers may clone it and append a complete Phase-4 load set without mutating canonical world
/// state. Bodies and Sources are sorted by their stable typed Entity IDs.
pub(crate) fn build_power_topology_base(
    structural: &StructuralWorld,
    sources: &PowerSourceStore,
) -> Result<PowerTopologyInput, PowerAdapterError> {
    let mut bodies = structural
        .wires()
        .iter_alive()
        .map(|(_, wire)| adapt_wire(wire))
        .collect::<Result<Vec<_>, _>>()?;
    bodies.sort_unstable_by_key(|body| body.wire);

    let mut source_attachments = sources
        .iter()
        .map(|source| PowerSourceAttachment {
            source: source.id(),
            node: PowerNodeKey::SourceAnchor(source.id()),
        })
        .collect::<Vec<_>>();
    source_attachments.sort_unstable_by_key(|attachment| attachment.source);

    Ok(PowerTopologyInput {
        bodies,
        sources: source_attachments,
        loads: Vec::new(),
    })
}

/// Rebuilds and compiles the physical Power graph with one already-complete derived load set.
///
/// The topology compiler owns duplicate-load checks, component IDs, and canonical route choice.
/// This helper deliberately does not cache derived topology in canonical state.
pub(crate) fn compile_power_topology_with_loads(
    structural: &StructuralWorld,
    sources: &PowerSourceStore,
    loads: impl IntoIterator<Item = PowerLoadAttachment>,
) -> Result<CompiledPowerTopology, PowerAdapterError> {
    let mut input = build_power_topology_base(structural, sources)?;
    input.loads.extend(loads);
    CompiledPowerTopology::compile(&input).map_err(PowerAdapterError::from)
}

fn adapt_wire(wire: WireRecord<'_>) -> Result<PowerBodyEdge, PowerAdapterError> {
    let (&endpoint_a_position, &endpoint_b_position) = wire
        .points
        .first()
        .zip(wire.points.last())
        .ok_or(PowerAdapterError::InvalidCanonicalState)?;
    if wire.points.len() < 2 || endpoint_a_position == endpoint_b_position {
        return Err(PowerAdapterError::InvalidCanonicalState);
    }

    let canonical_points = canonical_polyline_points(wire.points);
    if canonical_points.len() < 2
        || canonical_points.first().copied() != Some(endpoint_a_position)
        || canonical_points.last().copied() != Some(endpoint_b_position)
    {
        return Err(PowerAdapterError::InvalidCanonicalState);
    }

    let segment_lengths = canonical_points
        .windows(2)
        .map(|segment| {
            let length = segment_length(segment[0], segment[1])?;
            if length.0 <= 0 {
                return Err(PowerAdapterError::InvalidCanonicalState);
            }
            Ok(length)
        })
        .collect::<Result<Vec<_>, PowerAdapterError>>()?;
    let length = segment_lengths.iter().copied().try_fold(
        Fixed::ZERO,
        |sum, segment| -> Result<Fixed, PowerAdapterError> { Ok(sum.checked_add(segment)?) },
    )?;
    if length.0 <= 0 {
        return Err(PowerAdapterError::InvalidCanonicalState);
    }

    let a_descriptor = endpoint_descriptor(wire.endpoint_a, endpoint_a_position);
    let b_descriptor = endpoint_descriptor(wire.endpoint_b, endpoint_b_position);
    let canonical_lower_end = match a_descriptor.cmp(&b_descriptor) {
        Ordering::Less => WireEnd::A,
        Ordering::Greater => WireEnd::B,
        Ordering::Equal => {
            return Err(PowerAdapterError::IndistinguishableWireEndpoints { wire: wire.id });
        }
    };

    Ok(PowerBodyEdge {
        wire: wire.id,
        a: endpoint_power_node(wire.id, WireEnd::A, wire.endpoint_a),
        b: endpoint_power_node(wire.id, WireEnd::B, wire.endpoint_b),
        length,
        segment_lengths,
        canonical_lower_end,
    })
}

/// Maps only Power-bearing endpoint surfaces into shared graph nodes.
///
/// Free, Main Core, Signal Gate, Mobile-control, and Sense surfaces terminate the Power
/// projection at the physical Wire end. Geometry crossings never reach this mapping and therefore
/// never connect.
fn endpoint_power_node(wire: WireId, end: WireEnd, target: EndpointTarget) -> PowerNodeKey {
    match target {
        EndpointTarget::Junction(junction) => PowerNodeKey::Junction(junction),
        EndpointTarget::GatePort(reference) if reference.port == GatePort::Power => {
            PowerNodeKey::GatePower(reference.gate)
        }
        EndpointTarget::PowerSourceAnchor(source) => PowerNodeKey::SourceAnchor(source),
        EndpointTarget::Free
        | EndpointTarget::GatePort(_)
        | EndpointTarget::MobilePort(_)
        | EndpointTarget::MainCoreAnchor(_)
        | EndpointTarget::WireSensePort(_) => PowerNodeKey::WireEnd(wire, end),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct EndpointSemanticDescriptor {
    target_tag: u8,
    referenced_entity: EntityId,
    local_subtag: u8,
    position_x: i64,
    position_y: i64,
}

/// Implements the frozen orientation descriptor exactly, independent of Rust enum layout.
fn endpoint_descriptor(target: EndpointTarget, position: FixedVec2) -> EndpointSemanticDescriptor {
    let (referenced_entity, local_subtag) = match target {
        EndpointTarget::Free => (RESERVED_ENTITY_ID, 0),
        EndpointTarget::Junction(junction) => (junction.entity_id(), 0),
        EndpointTarget::GatePort(reference) => {
            (reference.gate.entity_id(), reference.port.canonical_tag())
        }
        EndpointTarget::MobilePort(reference) => {
            (reference.mobile.entity_id(), reference.port.canonical_tag())
        }
        EndpointTarget::MainCoreAnchor(core) => (core.entity_id(), 0),
        EndpointTarget::PowerSourceAnchor(source) => (source.entity_id(), 0),
        EndpointTarget::WireSensePort(reference) => {
            (reference.wire.entity_id(), reference.end.canonical_tag())
        }
    };
    EndpointSemanticDescriptor {
        target_tag: target.canonical_tag(),
        referenced_entity,
        local_subtag,
        position_x: position.x.0,
        position_y: position.y.0,
    }
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub(crate) enum PowerAdapterError {
    #[error("Power adapter numeric overflow")]
    NumericOverflow,

    #[error("Power Wire {wire:?} has indistinguishable canonical endpoint descriptors")]
    IndistinguishableWireEndpoints { wire: WireId },

    #[error("Power adapter observed invalid canonical structural state")]
    InvalidCanonicalState,

    #[error(transparent)]
    Topology(#[from] PowerTopologyError),
}

impl From<NumericError> for PowerAdapterError {
    fn from(_: NumericError) -> Self {
        Self::NumericOverflow
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::power::PowerSourceState;
    use crate::topology::{GatePortRef, RoutingDomain};
    use crate::{Energy, GateId, JunctionId, PowerSourceId};

    const fn point(x: i64, y: i64) -> FixedVec2 {
        FixedVec2::new(Fixed(x), Fixed(y))
    }

    const fn wire(id: u64) -> WireId {
        WireId(EntityId(id))
    }

    fn record<'a>(
        points: &'a [FixedVec2],
        endpoint_a: EndpointTarget,
        endpoint_b: EndpointTarget,
    ) -> WireRecord<'a> {
        WireRecord {
            id: wire(10),
            routing_domain: RoutingDomain::OpenWorld,
            points,
            endpoint_a,
            endpoint_b,
            connection_generation: crate::ConnectionGeneration::INITIAL,
            damage_state: None,
        }
    }

    #[test]
    fn adapter_maps_only_power_surfaces_and_retains_canonical_segment_lengths() {
        let points = [point(0, 0), point(2, 0), point(5, 0), point(5, 4)];
        let source = PowerSourceId(EntityId(50));
        let body = adapt_wire(record(
            &points,
            EndpointTarget::PowerSourceAnchor(source),
            EndpointTarget::GatePort(GatePortRef {
                gate: GateId(EntityId(60)),
                port: GatePort::Power,
            }),
        ))
        .expect("valid Wire adapts");

        assert_eq!(body.a, PowerNodeKey::SourceAnchor(source));
        assert_eq!(body.b, PowerNodeKey::GatePower(GateId(EntityId(60))));
        assert_eq!(body.segment_lengths, vec![Fixed(5), Fixed(4)]);
        assert_eq!(body.length, Fixed(9));
        assert_eq!(body.canonical_lower_end, WireEnd::B);

        assert_eq!(
            endpoint_power_node(
                wire(10),
                WireEnd::B,
                EndpointTarget::GatePort(GatePortRef {
                    gate: GateId(EntityId(60)),
                    port: GatePort::Output,
                })
            ),
            PowerNodeKey::WireEnd(wire(10), WireEnd::B)
        );
    }

    #[test]
    fn complete_descriptor_uses_port_then_position_and_reverses_strictly() {
        let gate = GateId(EntityId(60));
        let output = EndpointTarget::GatePort(GatePortRef {
            gate,
            port: GatePort::Output,
        });
        let power = EndpointTarget::GatePort(GatePortRef {
            gate,
            port: GatePort::Power,
        });
        assert!(
            endpoint_descriptor(output, point(99, 99)) < endpoint_descriptor(power, point(0, 0))
        );
        assert!(endpoint_descriptor(power, point(0, 0)) < endpoint_descriptor(power, point(1, 0)));

        let points = [point(9, 0), point(0, 0)];
        let body = adapt_wire(record(&points, power, output)).expect("reverse descriptor adapts");
        assert_eq!(body.canonical_lower_end, WireEnd::B);
    }

    #[test]
    fn source_only_base_compiles_as_one_isolated_region() {
        let source = PowerSourceState::new(PowerSourceId(EntityId(50)), point(0, 0), Energy(10));
        let sources = PowerSourceStore::new(vec![source]).expect("Source store is valid");
        let structural = StructuralWorld::new();
        let base = build_power_topology_base(&structural, &sources).expect("base adapts");
        assert!(base.bodies.is_empty());
        assert!(base.loads.is_empty());
        assert_eq!(base.sources.len(), 1);

        let compiled = compile_power_topology_with_loads(&structural, &sources, [])
            .expect("isolated Source graph compiles");
        assert_eq!(compiled.regions().len(), 1);
        assert_eq!(
            compiled.region_for_source(source.id()),
            Some(compiled.regions()[0].id())
        );
    }

    #[test]
    fn indistinguishable_descriptors_fail_closed() {
        let points = [point(0, 0), point(1, 0), point(0, 0)];
        assert_eq!(
            adapt_wire(record(
                &points,
                EndpointTarget::Junction(JunctionId(EntityId(5))),
                EndpointTarget::Junction(JunctionId(EntityId(5))),
            )),
            Err(PowerAdapterError::InvalidCanonicalState)
        );
    }
}
