use crate::structural::StructuralWorld;
use crate::{
    EndpointTarget, EntityLocation, EntityRegistry, FixedAabb, FixedVec2, Revision, RoutingDomain,
    SimulationContract, StateHash, Tick,
};

const STATE_DOMAIN: &[u8] = b"AON\0STATE\0V1\0";
const STATE_ENCODER_VERSION: u16 = 1;
const FUTURE_EMPTY_STORE_COUNT: usize = 4;

pub(crate) fn state_hash(
    contract: &SimulationContract,
    next_tick: Tick,
    topology_revision: Revision,
    structural: &StructuralWorld,
) -> StateHash {
    let mut hasher = blake3::Hasher::new();
    encode_state(
        contract,
        next_tick,
        topology_revision,
        structural,
        &mut |bytes| {
            hasher.update(bytes);
        },
    );
    StateHash::from_bytes(*hasher.finalize().as_bytes())
}

fn encode_state(
    contract: &SimulationContract,
    next_tick: Tick,
    topology_revision: Revision,
    structural: &StructuralWorld,
    write: &mut dyn FnMut(&[u8]),
) {
    encode_state_components(
        contract,
        next_tick,
        topology_revision,
        structural.entities(),
        Some(structural),
        write,
    );
}

fn encode_state_components(
    contract: &SimulationContract,
    next_tick: Tick,
    topology_revision: Revision,
    entities: &EntityRegistry,
    structural: Option<&StructuralWorld>,
    write: &mut dyn FnMut(&[u8]),
) {
    write(STATE_DOMAIN);
    write_u16(STATE_ENCODER_VERSION, write);
    write_u8(contract.semantics_version.canonical_tag(), write);
    write(contract.numeric_profile_hash.as_bytes());
    write(contract.physical_scale_profile_hash.as_bytes());
    write(contract.balance_profile_hash.as_bytes());
    write_u64(next_tick.0, write);
    write_u64(topology_revision.0, write);
    write_u64(entities.next_id().0, write);
    write_u64(entities.allocated_count(), write);
    for (entity_id, location) in entities.canonical_slots() {
        write_u64(entity_id.0, write);
        match location {
            Some(location) => {
                write_u8(1, write);
                write_u8(entity_kind_tag(location), write);
            }
            None => write_u8(0, write),
        }
    }

    if let Some(structural) = structural {
        encode_structural_stores(structural, write);
    } else {
        for _ in 0..4 {
            write_u64(0, write);
        }
    }

    // Mobile substrate, scheduled event, pending destruction, and path-certificate stores are
    // introduced by later milestones.
    for _ in 0..FUTURE_EMPTY_STORE_COUNT {
        write_u64(0, write);
    }
}

fn encode_structural_stores(structural: &StructuralWorld, write: &mut dyn FnMut(&[u8])) {
    let mut gates: Vec<_> = structural
        .gates()
        .iter_alive()
        .map(|(_, record)| record)
        .collect();
    gates.sort_unstable_by_key(|record| record.id.entity_id());
    write_u64(gates.len() as u64, write);
    for record in gates {
        write_u64(record.id.entity_id().0, write);
        write_u8(record.gate_type.canonical_tag(), write);
        encode_point(record.origin, write);
        encode_routing_domain(record.routing_domain, write);
    }

    let mut wires: Vec<_> = structural
        .wires()
        .iter_alive()
        .map(|(_, record)| record)
        .collect();
    wires.sort_unstable_by_key(|record| record.id.entity_id());
    write_u64(wires.len() as u64, write);
    for record in wires {
        write_u64(record.id.entity_id().0, write);
        encode_routing_domain(record.routing_domain, write);
        write_u64(record.connection_generation.0, write);
        write_u32(record.points.len() as u32, write);
        for &point in record.points {
            encode_point(point, write);
        }
        encode_endpoint(record.endpoint_a, write);
        encode_endpoint(record.endpoint_b, write);
    }

    let mut junctions: Vec<_> = structural
        .junctions()
        .iter_alive()
        .map(|(_, record)| record)
        .collect();
    junctions.sort_unstable_by_key(|record| record.id.entity_id());
    write_u64(junctions.len() as u64, write);
    for record in junctions {
        write_u64(record.id.entity_id().0, write);
        encode_routing_domain(record.routing_domain, write);
        encode_point(record.position, write);
        write_u64(record.connection_generation.0, write);
    }

    let mut substrates: Vec<_> = structural
        .fixed_substrates()
        .iter_alive()
        .map(|(_, record)| record)
        .collect();
    substrates.sort_unstable_by_key(|record| record.id);
    write_u64(substrates.len() as u64, write);
    for record in substrates {
        write_u64(record.id.0, write);
        encode_point(record.origin, write);
        encode_aabb(record.routing_area, write);
        encode_aabb(record.footprint, write);
    }
}

fn encode_point(point: FixedVec2, write: &mut dyn FnMut(&[u8])) {
    write_i64(point.x.0, write);
    write_i64(point.y.0, write);
}

fn encode_aabb(aabb: FixedAabb, write: &mut dyn FnMut(&[u8])) {
    encode_point(aabb.min, write);
    encode_point(aabb.max, write);
}

fn encode_routing_domain(domain: RoutingDomain, write: &mut dyn FnMut(&[u8])) {
    write_u8(domain.canonical_tag(), write);
    match domain {
        RoutingDomain::OpenWorld => {}
        RoutingDomain::FixedSubstrate(id) | RoutingDomain::MobileSubstrate(id) => {
            write_u64(id.0, write);
        }
    }
}

fn encode_endpoint(endpoint: EndpointTarget, write: &mut dyn FnMut(&[u8])) {
    write_u8(endpoint.canonical_tag(), write);
    match endpoint {
        EndpointTarget::Free => {}
        EndpointTarget::Junction(id) => write_u64(id.entity_id().0, write),
        EndpointTarget::GatePort(reference) => {
            write_u64(reference.gate.entity_id().0, write);
            write_u8(reference.port.canonical_tag(), write);
        }
    }
}

const fn entity_kind_tag(location: EntityLocation) -> u8 {
    match location {
        EntityLocation::MainCore => 0,
        EntityLocation::RelaySite(_) => 1,
        EntityLocation::Gate(_) => 2,
        EntityLocation::Wire(_) => 3,
        EntityLocation::Junction(_) => 4,
        EntityLocation::FixedSubstrate(_) => 5,
        EntityLocation::MobileSubstrate(_) => 6,
        EntityLocation::PowerSource(_) => 7,
        EntityLocation::Quartz(_) => 8,
        EntityLocation::Deposit(_) => 9,
        EntityLocation::Enemy(_) => 10,
        EntityLocation::ConstructionSite(_) => 11,
    }
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
    use crate::{
        Command, CommandEnvelope, EndpointTarget, EntityId, EntityLocation, Fixed, FixedAabb,
        FixedSubstrateIndex, GateId, GateIndex, GatePort, GatePortRef, GateType, JunctionId,
        JunctionIndex, PhysicalScaleProfile, PlaceFixedSubstrateCommand, PlaceGateCommand,
        PlaceJunctionCommand, PlaceWireCommand, ProfileHash, RoutingDomain, WireIndex,
    };

    fn contract() -> SimulationContract {
        SimulationContract {
            semantics_version: crate::SemanticsVersion::AonV1,
            numeric_profile_hash: ProfileHash::from_bytes([0x11; 32]),
            physical_scale_profile_hash: ProfileHash::from_bytes([0x22; 32]),
            balance_profile_hash: ProfileHash::from_bytes([0x33; 32]),
        }
    }

    fn identity_state_hash(entities: &EntityRegistry) -> StateHash {
        let mut hasher = blake3::Hasher::new();
        encode_state_components(
            &contract(),
            Tick(0),
            Revision(0),
            entities,
            None,
            &mut |bytes| {
                hasher.update(bytes);
            },
        );
        StateHash::from_bytes(*hasher.finalize().as_bytes())
    }

    const fn point(x: i64, y: i64) -> FixedVec2 {
        FixedVec2::new(Fixed(x), Fixed(y))
    }

    fn envelope(tick: u64, ordinal: u64, command: Command) -> CommandEnvelope {
        CommandEnvelope {
            target_tick: Tick(tick),
            ordinal,
            command,
        }
    }

    #[test]
    fn state_encoding_has_exact_contract_tick_revision_and_identity_order() {
        let mut entities = EntityRegistry::new();
        entities
            .allocate(EntityLocation::Gate(GateIndex(7)))
            .expect("gate allocation succeeds");
        let removed = entities
            .allocate(EntityLocation::Wire(WireIndex(9)))
            .expect("wire allocation succeeds");
        entities.remove(removed).expect("wire removal succeeds");

        let mut actual = Vec::new();
        encode_state_components(
            &contract(),
            Tick(5),
            Revision(3),
            &entities,
            None,
            &mut |bytes| actual.extend_from_slice(bytes),
        );

        let mut expected = Vec::new();
        expected.extend_from_slice(STATE_DOMAIN);
        expected.extend_from_slice(&STATE_ENCODER_VERSION.to_le_bytes());
        expected.push(0);
        expected.extend_from_slice(&[0x11; 32]);
        expected.extend_from_slice(&[0x22; 32]);
        expected.extend_from_slice(&[0x33; 32]);
        expected.extend_from_slice(&5_u64.to_le_bytes());
        expected.extend_from_slice(&3_u64.to_le_bytes());
        expected.extend_from_slice(&3_u64.to_le_bytes());
        expected.extend_from_slice(&2_u64.to_le_bytes());
        expected.extend_from_slice(&1_u64.to_le_bytes());
        expected.push(1);
        expected.push(2);
        expected.extend_from_slice(&2_u64.to_le_bytes());
        expected.push(0);
        expected.extend_from_slice(&[0_u8; (4 + FUTURE_EMPTY_STORE_COUNT) * 8]);

        assert_eq!(actual, expected);
    }

    #[test]
    fn tombstones_and_allocation_frontier_change_state_hash() {
        let empty = EntityRegistry::new();
        let mut tombstone = EntityRegistry::new();
        let id = tombstone
            .allocate(EntityLocation::MainCore)
            .expect("allocation succeeds");
        tombstone.remove(id).expect("removal succeeds");

        assert_ne!(identity_state_hash(&empty), identity_state_hash(&tombstone));
    }

    #[test]
    fn dense_storage_compaction_does_not_change_state_hash() {
        let mut first = EntityRegistry::new();
        let id = first
            .allocate(EntityLocation::Gate(GateIndex(7)))
            .expect("allocation succeeds");
        let mut compacted = first.clone();
        compacted
            .update_location(id, EntityLocation::Gate(GateIndex(0)))
            .expect("live entity location updates");

        assert_eq!(identity_state_hash(&first), identity_state_hash(&compacted));
    }

    #[test]
    fn structural_soa_slots_dense_indices_and_capacity_do_not_change_state_hash() {
        const WORLD_PITCH: i64 = 65_536;
        let physical = PhysicalScaleProfile::stage0_alpha("canonical-layout-test");
        let bounds = FixedAabb::new(
            point(-2 * WORLD_PITCH, -2 * WORLD_PITCH),
            point(2 * WORLD_PITCH, 2 * WORLD_PITCH),
        );
        let mut world = StructuralWorld::new();
        let substrate_report = world
            .apply_phase0(
                Tick(0),
                &[
                    envelope(
                        0,
                        0,
                        Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
                            origin: point(-8 * WORLD_PITCH, 0),
                            routing_area: bounds,
                            footprint: bounds,
                        }),
                    ),
                    envelope(
                        0,
                        1,
                        Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
                            origin: point(8 * WORLD_PITCH, 0),
                            routing_area: bounds,
                            footprint: bounds,
                        }),
                    ),
                ],
                &physical,
            )
            .expect("substrate fixture succeeds");
        assert!(substrate_report.rejections.is_empty());

        let domain = RoutingDomain::FixedSubstrate(EntityId(1));
        let structural_report = world
            .apply_phase0(
                Tick(1),
                &[
                    envelope(
                        1,
                        0,
                        Command::PlaceGate(PlaceGateCommand {
                            gate_type: GateType::Not,
                            origin: point(-8 * WORLD_PITCH, 0),
                            routing_domain: domain,
                        }),
                    ),
                    envelope(
                        1,
                        1,
                        Command::PlaceGate(PlaceGateCommand {
                            gate_type: GateType::Not,
                            origin: point(-7 * WORLD_PITCH, 0),
                            routing_domain: domain,
                        }),
                    ),
                    envelope(
                        1,
                        2,
                        Command::PlaceWire(PlaceWireCommand {
                            routing_domain: RoutingDomain::OpenWorld,
                            points: vec![
                                point(0, 10 * WORLD_PITCH),
                                point(WORLD_PITCH, 10 * WORLD_PITCH),
                            ],
                            endpoint_a: EndpointTarget::Free,
                            endpoint_b: EndpointTarget::Free,
                        }),
                    ),
                    envelope(
                        1,
                        3,
                        Command::PlaceWire(PlaceWireCommand {
                            routing_domain: RoutingDomain::OpenWorld,
                            points: vec![
                                point(0, 12 * WORLD_PITCH),
                                point(WORLD_PITCH, 12 * WORLD_PITCH),
                            ],
                            endpoint_a: EndpointTarget::Free,
                            endpoint_b: EndpointTarget::Free,
                        }),
                    ),
                    envelope(
                        1,
                        4,
                        Command::PlaceJunction(PlaceJunctionCommand {
                            routing_domain: RoutingDomain::OpenWorld,
                            position: point(0, 14 * WORLD_PITCH),
                        }),
                    ),
                    envelope(
                        1,
                        5,
                        Command::PlaceJunction(PlaceJunctionCommand {
                            routing_domain: RoutingDomain::OpenWorld,
                            position: point(WORLD_PITCH, 14 * WORLD_PITCH),
                        }),
                    ),
                ],
                &physical,
            )
            .expect("two-record structural fixture succeeds");
        assert!(structural_report.rejections.is_empty());

        let baseline = state_hash(&contract(), Tick(2), Revision(1), &world);
        let mut reordered = world.clone();
        reordered.reserve_layout_capacity_for_test(128);
        reordered
            .swap_gate_slots_for_test(GateIndex(0), GateIndex(1))
            .expect("Gate slots can be rearranged for the layout property");
        reordered
            .swap_wire_slots_for_test(WireIndex(0), WireIndex(1))
            .expect("Wire slots can be rearranged for the layout property");
        reordered
            .swap_junction_slots_for_test(JunctionIndex(0), JunctionIndex(1))
            .expect("Junction slots can be rearranged for the layout property");
        reordered
            .swap_fixed_substrate_slots_for_test(FixedSubstrateIndex(0), FixedSubstrateIndex(1))
            .expect("Substrate slots can be rearranged for the layout property");

        assert_ne!(world, reordered, "the physical SoA layout must differ");
        assert_eq!(
            baseline,
            state_hash(&contract(), Tick(2), Revision(1), &reordered)
        );
    }

    #[test]
    fn structural_state_encoding_has_exact_entity_order_records_and_raw_vertices() {
        const WORLD_PITCH: i64 = 65_536;
        let physical = PhysicalScaleProfile::stage0_alpha("canonical-structural-test");
        let mut world = StructuralWorld::new();
        let substrate_bounds = FixedAabb::new(
            point(-4 * WORLD_PITCH, -4 * WORLD_PITCH),
            point(4 * WORLD_PITCH, 4 * WORLD_PITCH),
        );
        world
            .apply_phase0(
                Tick(0),
                &[envelope(
                    0,
                    0,
                    Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
                        origin: point(0, 0),
                        routing_area: substrate_bounds,
                        footprint: substrate_bounds,
                    }),
                )],
                &physical,
            )
            .expect("substrate placement succeeds");
        let domain = RoutingDomain::FixedSubstrate(EntityId(1));
        world
            .apply_phase0(
                Tick(1),
                &[
                    envelope(
                        1,
                        0,
                        Command::PlaceGate(PlaceGateCommand {
                            gate_type: GateType::And,
                            origin: point(0, 0),
                            routing_domain: domain,
                        }),
                    ),
                    envelope(
                        1,
                        1,
                        Command::PlaceJunction(PlaceJunctionCommand {
                            routing_domain: domain,
                            position: point(2 * WORLD_PITCH, 0),
                        }),
                    ),
                ],
                &physical,
            )
            .expect("gate and junction placement succeeds");
        world
            .apply_phase0(
                Tick(2),
                &[envelope(
                    2,
                    0,
                    Command::PlaceWire(PlaceWireCommand {
                        routing_domain: domain,
                        points: vec![point(16_384, 0), point(2 * WORLD_PITCH, 0)],
                        endpoint_a: EndpointTarget::GatePort(GatePortRef {
                            gate: GateId(EntityId(2)),
                            port: GatePort::Output,
                        }),
                        endpoint_b: EndpointTarget::Junction(JunctionId(EntityId(3))),
                    }),
                )],
                &physical,
            )
            .expect("bound wire placement succeeds");

        let mut actual = Vec::new();
        encode_state(&contract(), Tick(3), Revision(2), &world, &mut |bytes| {
            actual.extend_from_slice(bytes)
        });

        let mut expected = Vec::new();
        expected.extend_from_slice(STATE_DOMAIN);
        expected.extend_from_slice(&STATE_ENCODER_VERSION.to_le_bytes());
        expected.push(0);
        expected.extend_from_slice(&[0x11; 32]);
        expected.extend_from_slice(&[0x22; 32]);
        expected.extend_from_slice(&[0x33; 32]);
        for value in [3_u64, 2, 5, 4] {
            expected.extend_from_slice(&value.to_le_bytes());
        }
        for (id, kind) in [(1_u64, 5_u8), (2, 2), (3, 4), (4, 3)] {
            expected.extend_from_slice(&id.to_le_bytes());
            expected.extend_from_slice(&[1, kind]);
        }

        expected.extend_from_slice(&1_u64.to_le_bytes());
        expected.extend_from_slice(&2_u64.to_le_bytes());
        expected.push(0);
        append_point(&mut expected, point(0, 0));
        expected.push(1);
        expected.extend_from_slice(&1_u64.to_le_bytes());

        expected.extend_from_slice(&1_u64.to_le_bytes());
        expected.extend_from_slice(&4_u64.to_le_bytes());
        expected.push(1);
        expected.extend_from_slice(&1_u64.to_le_bytes());
        expected.extend_from_slice(&0_u64.to_le_bytes());
        expected.extend_from_slice(&2_u32.to_le_bytes());
        append_point(&mut expected, point(16_384, 0));
        append_point(&mut expected, point(2 * WORLD_PITCH, 0));
        expected.push(2);
        expected.extend_from_slice(&2_u64.to_le_bytes());
        expected.push(2);
        expected.push(1);
        expected.extend_from_slice(&3_u64.to_le_bytes());

        expected.extend_from_slice(&1_u64.to_le_bytes());
        expected.extend_from_slice(&3_u64.to_le_bytes());
        expected.push(1);
        expected.extend_from_slice(&1_u64.to_le_bytes());
        append_point(&mut expected, point(2 * WORLD_PITCH, 0));
        expected.extend_from_slice(&1_u64.to_le_bytes());

        expected.extend_from_slice(&1_u64.to_le_bytes());
        expected.extend_from_slice(&1_u64.to_le_bytes());
        append_point(&mut expected, point(0, 0));
        append_aabb(&mut expected, substrate_bounds);
        append_aabb(&mut expected, substrate_bounds);
        expected.extend_from_slice(&[0_u8; FUTURE_EMPTY_STORE_COUNT * 8]);

        assert_eq!(actual, expected);
        assert_eq!(
            state_hash(&contract(), Tick(3), Revision(2), &world).to_string(),
            "e580cf66bcbf780fa58765194e9c4c8073731e34d2b30e102e313094a2e73a4b"
        );
    }

    fn append_point(output: &mut Vec<u8>, point: FixedVec2) {
        output.extend_from_slice(&point.x.0.to_le_bytes());
        output.extend_from_slice(&point.y.0.to_le_bytes());
    }

    fn append_aabb(output: &mut Vec<u8>, aabb: FixedAabb) {
        append_point(output, aabb.min);
        append_point(output, aabb.max);
    }
}
