use crate::event::{
    DriverSample, DriverTransition, EventCalendar, EventKey, EventPayloadAllocator, SignalArrival,
};
use crate::signal::{DriveVector, SignalWorld};
use crate::structural::StructuralWorld;
use crate::{
    EndpointTarget, EntityLocation, EntityRegistry, FixedAabb, FixedVec2, LogicLevel, Revision,
    RoutingDomain, SimulationContract, StateHash, Tick,
};

const STATE_DOMAIN: &[u8] = b"AON\0STATE\0V2\0";
const STATE_ENCODER_VERSION: u16 = 2;
const FUTURE_EMPTY_STORE_COUNT: usize = 5;

#[derive(Clone, Copy)]
pub(crate) struct StateView<'a> {
    pub contract: &'a SimulationContract,
    pub next_tick: Tick,
    pub topology_revision: Revision,
    pub structural: &'a StructuralWorld,
    pub signal: &'a SignalWorld,
    pub event_payloads: &'a EventPayloadAllocator,
    pub driver_events: &'a EventCalendar<DriverTransition>,
    pub signal_events: &'a EventCalendar<SignalArrival>,
}

#[derive(Clone, Copy)]
struct StateComponents<'a> {
    contract: &'a SimulationContract,
    next_tick: Tick,
    topology_revision: Revision,
    entities: &'a EntityRegistry,
    structural: Option<&'a StructuralWorld>,
    signal: &'a SignalWorld,
    event_payloads: &'a EventPayloadAllocator,
    driver_events: &'a EventCalendar<DriverTransition>,
    signal_events: &'a EventCalendar<SignalArrival>,
}

pub(crate) fn state_hash(state: StateView<'_>) -> StateHash {
    let mut hasher = blake3::Hasher::new();
    encode_state(state, &mut |bytes| {
        hasher.update(bytes);
    });
    StateHash::from_bytes(*hasher.finalize().as_bytes())
}

fn encode_state(state: StateView<'_>, write: &mut dyn FnMut(&[u8])) {
    encode_state_components(
        StateComponents {
            contract: state.contract,
            next_tick: state.next_tick,
            topology_revision: state.topology_revision,
            entities: state.structural.entities(),
            structural: Some(state.structural),
            signal: state.signal,
            event_payloads: state.event_payloads,
            driver_events: state.driver_events,
            signal_events: state.signal_events,
        },
        write,
    );
}

fn encode_state_components(state: StateComponents<'_>, write: &mut dyn FnMut(&[u8])) {
    write(STATE_DOMAIN);
    write_u16(STATE_ENCODER_VERSION, write);
    write_u8(state.contract.semantics_version.canonical_tag(), write);
    write(state.contract.numeric_profile_hash.as_bytes());
    write(state.contract.physical_scale_profile_hash.as_bytes());
    write(state.contract.balance_profile_hash.as_bytes());
    write_u64(state.next_tick.0, write);
    write_u64(state.topology_revision.0, write);
    write_u64(state.entities.next_id().0, write);
    write_u64(state.entities.allocated_count(), write);
    for (entity_id, location) in state.entities.canonical_slots() {
        write_u64(entity_id.0, write);
        match location {
            Some(location) => {
                write_u8(1, write);
                write_u8(entity_kind_tag(location), write);
            }
            None => write_u8(0, write),
        }
    }

    if let Some(structural) = state.structural {
        encode_structural_stores(structural, write);
    } else {
        for _ in 0..4 {
            write_u64(0, write);
        }
    }

    encode_signal_stores(state.signal, write);
    write_u64(state.event_payloads.next_payload_order(), write);
    encode_driver_events(state.driver_events, write);
    encode_signal_events(state.signal_events, write);

    // Mobile substrate, destruction, radiation, relay, and path-certificate sections are
    // introduced by later milestones. Their fixed empty section markers keep the V2 layout
    // unambiguous without making their derived or scratch representations canonical early.
    for _ in 0..FUTURE_EMPTY_STORE_COUNT {
        write_u64(0, write);
    }
}

fn encode_signal_stores(signal: &SignalWorld, write: &mut dyn FnMut(&[u8])) {
    write_u64(signal.driver_frontier().entity_id().0, write);
    write_u64(signal.allocated_driver_count(), write);
    for (driver_id, record) in signal.canonical_driver_slots() {
        write_u64(driver_id.entity_id().0, write);
        match record {
            Some(record) => {
                write_u8(1, write);
                write_u64(record.owner.entity_id().0, write);
                write_u8(record.role.canonical_tag(), write);
                encode_driver_sample(record.sample, write);
            }
            None => write_u8(0, write),
        }
    }

    write_u64(signal.sink_frontier().entity_id().0, write);
    write_u64(signal.allocated_sink_count(), write);
    for (sink_id, record) in signal.canonical_sink_slots() {
        write_u64(sink_id.entity_id().0, write);
        match record {
            Some(record) => {
                write_u8(1, write);
                write_u64(record.owner.entity_id().0, write);
                write_u8(record.role.canonical_tag(), write);
                write_u8(logic_level_tag(record.resolved_level), write);
                write_u8(u8::from(record.dirty), write);
            }
            None => write_u8(0, write),
        }
    }

    let gates: Vec<_> = signal.iter_gates().collect();
    write_u64(gates.len() as u64, write);
    for gate in gates {
        write_u64(gate.gate.entity_id().0, write);
        write_u64(gate.ports.input_a.sink.entity_id().0, write);
        write_u64(gate.ports.input_a.external_driver.entity_id().0, write);
        match gate.ports.input_b {
            Some(input_b) => {
                write_u8(1, write);
                write_u64(input_b.sink.entity_id().0, write);
                write_u64(input_b.external_driver.entity_id().0, write);
            }
            None => write_u8(0, write),
        }
        write_u64(gate.ports.output.entity_id().0, write);
        write_u8(logic_level_tag(gate.current_output), write);
        write_u8(logic_level_tag(gate.desired_output), write);
        write_u32(gate.pending_generation, write);
        encode_optional_tick(gate.pending_due_tick, write);
        encode_optional_level(gate.pending_level, write);
        encode_optional_u64(gate.pending_switch_energy.map(|energy| energy.0), write);
        write_u64(gate.cancelled_switching_heat.0, write);
    }

    let wires: Vec<_> = signal.iter_wires().collect();
    write_u64(wires.len() as u64, write);
    for (wire_id, state) in wires {
        write_u64(wire_id.entity_id().0, write);
        encode_drive_vector(state.active, write);
        encode_drive_vector(state.previous, write);
    }

    let slots: Vec<_> = signal.iter_slots().collect();
    write_u64(slots.len() as u64, write);
    for slot in slots {
        write_u64(slot.sink.entity_id().0, write);
        write_u64(slot.driver.entity_id().0, write);
        write_u8(logic_level_tag(slot.level), write);
        write_u64(slot.strength.0, write);
        write_u64(slot.revision.0, write);
        write_u64(slot.emitted_at.0, write);
    }
}

fn encode_driver_events(events: &EventCalendar<DriverTransition>, write: &mut dyn FnMut(&[u8])) {
    write_u64(events.len() as u64, write);
    for event in events.canonical_view() {
        encode_event_key(event.key, write);
        write_u64(event.driver_id.entity_id().0, write);
        write_u8(logic_level_tag(event.level), write);
        write_u64(event.strength.0, write);
        write_u32(event.pending_generation, write);
        write_u8(event.cause.canonical_tag(), write);
    }
}

fn encode_signal_events(events: &EventCalendar<SignalArrival>, write: &mut dyn FnMut(&[u8])) {
    write_u64(events.len() as u64, write);
    for event in events.canonical_view() {
        encode_event_key(event.key, write);
        write_u64(event.source_driver.entity_id().0, write);
        write_u64(event.sink.entity_id().0, write);
        encode_driver_sample(event.sample, write);
        encode_optional_u64(
            event.path_certificate.map(|certificate| certificate.0),
            write,
        );
        write_u8(event.kind.canonical_tag(), write);
    }
}

fn encode_event_key(key: EventKey, write: &mut dyn FnMut(&[u8])) {
    write_u64(key.due_tick.0, write);
    write_u8(key.kind_order, write);
    write_u64(key.target_id, write);
    write_u64(key.source_id, write);
    write_u64(key.revision.0, write);
    write_u32(key.generation, write);
    write_u64(key.payload_order, write);
}

fn encode_driver_sample(sample: DriverSample, write: &mut dyn FnMut(&[u8])) {
    write_u8(logic_level_tag(sample.level), write);
    write_u64(sample.strength.0, write);
    write_u64(sample.revision.0, write);
    write_u64(sample.emitted_at.0, write);
    write_u64(sample.driver_id.entity_id().0, write);
}

fn encode_drive_vector(vector: DriveVector, write: &mut dyn FnMut(&[u8])) {
    write_u128(vector.high, write);
    write_u128(vector.low, write);
    write_u128(vector.unknown, write);
}

fn encode_optional_tick(value: Option<Tick>, write: &mut dyn FnMut(&[u8])) {
    encode_optional_u64(value.map(|tick| tick.0), write);
}

fn encode_optional_level(value: Option<LogicLevel>, write: &mut dyn FnMut(&[u8])) {
    match value {
        Some(level) => {
            write_u8(1, write);
            write_u8(logic_level_tag(level), write);
        }
        None => write_u8(0, write),
    }
}

fn encode_optional_u64(value: Option<u64>, write: &mut dyn FnMut(&[u8])) {
    match value {
        Some(value) => {
            write_u8(1, write);
            write_u64(value, write);
        }
        None => write_u8(0, write),
    }
}

const fn logic_level_tag(level: LogicLevel) -> u8 {
    match level {
        LogicLevel::Low => 0,
        LogicLevel::High => 1,
        LogicLevel::X => 2,
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

fn write_u128(value: u128, write: &mut dyn FnMut(&[u8])) {
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

    struct TestRuntime {
        signal: SignalWorld,
        payloads: EventPayloadAllocator,
        driver_events: EventCalendar<DriverTransition>,
        signal_events: EventCalendar<SignalArrival>,
    }

    impl TestRuntime {
        fn new() -> Self {
            Self {
                signal: SignalWorld::new(),
                payloads: EventPayloadAllocator::new(),
                driver_events: EventCalendar::new(),
                signal_events: EventCalendar::new(),
            }
        }

        fn view<'a>(
            &'a self,
            contract: &'a SimulationContract,
            next_tick: Tick,
            topology_revision: Revision,
            structural: &'a StructuralWorld,
        ) -> StateView<'a> {
            StateView {
                contract,
                next_tick,
                topology_revision,
                structural,
                signal: &self.signal,
                event_payloads: &self.payloads,
                driver_events: &self.driver_events,
                signal_events: &self.signal_events,
            }
        }
    }

    fn identity_state_hash(entities: &EntityRegistry) -> StateHash {
        let runtime = TestRuntime::new();
        let mut hasher = blake3::Hasher::new();
        encode_state_components(
            StateComponents {
                contract: &contract(),
                next_tick: Tick(0),
                topology_revision: Revision(0),
                entities,
                structural: None,
                signal: &runtime.signal,
                event_payloads: &runtime.payloads,
                driver_events: &runtime.driver_events,
                signal_events: &runtime.signal_events,
            },
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
        let runtime = TestRuntime::new();
        encode_state_components(
            StateComponents {
                contract: &contract(),
                next_tick: Tick(5),
                topology_revision: Revision(3),
                entities: &entities,
                structural: None,
                signal: &runtime.signal,
                event_payloads: &runtime.payloads,
                driver_events: &runtime.driver_events,
                signal_events: &runtime.signal_events,
            },
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
        append_empty_runtime(&mut expected);

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

        let runtime = TestRuntime::new();
        let contract = contract();
        let baseline = state_hash(runtime.view(&contract, Tick(2), Revision(1), &world));
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
            state_hash(runtime.view(&contract, Tick(2), Revision(1), &reordered))
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
        let runtime = TestRuntime::new();
        let contract = contract();
        encode_state(
            runtime.view(&contract, Tick(3), Revision(2), &world),
            &mut |bytes| actual.extend_from_slice(bytes),
        );

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
        append_empty_signal_and_events(&mut expected);

        assert_eq!(actual, expected);
        assert_eq!(
            state_hash(runtime.view(&contract, Tick(3), Revision(2), &world)).to_string(),
            "5bcdc86b023dcefa76ba84fdc9125332e002b9962ea447651e61d43711587867"
        );
    }

    #[test]
    fn event_sections_encode_complete_keys_payloads_and_shared_frontier() {
        let driver = crate::DriverId(crate::EntityId(7));
        let source = crate::DriverId(crate::EntityId(8));
        let sink = crate::SinkId(crate::EntityId(9));
        let mut payloads = EventPayloadAllocator::new();
        let mut driver_events = EventCalendar::new();
        let mut signal_events = EventCalendar::new();
        driver_events
            .stage(
                &mut payloads,
                [DriverTransition::s0m3(
                    Tick(11),
                    driver,
                    LogicLevel::X,
                    crate::DriveStrength(13),
                    17,
                    crate::DriverTransitionCause::GateOutput,
                )],
            )
            .expect("Driver event stages");
        signal_events
            .stage(
                &mut payloads,
                [SignalArrival::s0m3_propagation(
                    Tick(19),
                    source,
                    sink,
                    DriverSample {
                        level: LogicLevel::High,
                        strength: crate::DriveStrength(23),
                        revision: Revision(0),
                        emitted_at: Tick(5),
                        driver_id: source,
                    },
                )],
            )
            .expect("Signal event stages");

        let mut actual = Vec::new();
        write_u64(payloads.next_payload_order(), &mut |bytes| {
            actual.extend_from_slice(bytes)
        });
        encode_driver_events(&driver_events, &mut |bytes| actual.extend_from_slice(bytes));
        encode_signal_events(&signal_events, &mut |bytes| actual.extend_from_slice(bytes));

        let mut expected = Vec::new();
        expected.extend_from_slice(&3_u64.to_le_bytes());
        expected.extend_from_slice(&1_u64.to_le_bytes());
        append_event_key(&mut expected, (11, 0, 7, 7, 0, 17, 1));
        expected.extend_from_slice(&7_u64.to_le_bytes());
        expected.push(2);
        expected.extend_from_slice(&13_u64.to_le_bytes());
        expected.extend_from_slice(&17_u32.to_le_bytes());
        expected.push(1);

        expected.extend_from_slice(&1_u64.to_le_bytes());
        append_event_key(&mut expected, (19, 1, 9, 8, 0, 0, 2));
        expected.extend_from_slice(&8_u64.to_le_bytes());
        expected.extend_from_slice(&9_u64.to_le_bytes());
        expected.push(1);
        expected.extend_from_slice(&23_u64.to_le_bytes());
        expected.extend_from_slice(&0_u64.to_le_bytes());
        expected.extend_from_slice(&5_u64.to_le_bytes());
        expected.extend_from_slice(&8_u64.to_le_bytes());
        expected.push(0);
        expected.push(0);

        assert_eq!(actual, expected);
    }

    #[test]
    fn event_candidate_permutations_and_drained_payload_holes_are_hashed_canonically() {
        let structural = StructuralWorld::new();
        let signal = SignalWorld::new();
        let contract = contract();
        let first = DriverTransition::s0m3(
            Tick(4),
            crate::DriverId(crate::EntityId(1)),
            LogicLevel::Low,
            crate::DriveStrength(3),
            0,
            crate::DriverTransitionCause::ExternalDriver,
        );
        let second = DriverTransition::s0m3(
            Tick(4),
            crate::DriverId(crate::EntityId(2)),
            LogicLevel::High,
            crate::DriveStrength(5),
            0,
            crate::DriverTransitionCause::ExternalDriver,
        );

        let mut left_payloads = EventPayloadAllocator::new();
        let mut left_events = EventCalendar::new();
        left_events
            .stage(&mut left_payloads, [second, first])
            .expect("left events stage");
        let mut right_payloads = EventPayloadAllocator::new();
        let mut right_events = EventCalendar::new();
        right_events
            .stage(&mut right_payloads, [first, second])
            .expect("right events stage");
        let signal_events = EventCalendar::new();

        let left = state_hash(StateView {
            contract: &contract,
            next_tick: Tick(3),
            topology_revision: Revision(0),
            structural: &structural,
            signal: &signal,
            event_payloads: &left_payloads,
            driver_events: &left_events,
            signal_events: &signal_events,
        });
        let right = state_hash(StateView {
            event_payloads: &right_payloads,
            driver_events: &right_events,
            ..StateView {
                contract: &contract,
                next_tick: Tick(3),
                topology_revision: Revision(0),
                structural: &structural,
                signal: &signal,
                event_payloads: &left_payloads,
                driver_events: &left_events,
                signal_events: &signal_events,
            }
        });
        assert_eq!(left, right);

        left_events
            .drain_due(Tick(4))
            .expect("events drain and leave payload tombstones");
        let drained = state_hash(StateView {
            next_tick: Tick(5),
            event_payloads: &left_payloads,
            driver_events: &left_events,
            ..StateView {
                contract: &contract,
                next_tick: Tick(3),
                topology_revision: Revision(0),
                structural: &structural,
                signal: &signal,
                event_payloads: &left_payloads,
                driver_events: &left_events,
                signal_events: &signal_events,
            }
        });
        let fresh_payloads = EventPayloadAllocator::new();
        let fresh_events = EventCalendar::new();
        let fresh = state_hash(StateView {
            contract: &contract,
            next_tick: Tick(5),
            topology_revision: Revision(0),
            structural: &structural,
            signal: &signal,
            event_payloads: &fresh_payloads,
            driver_events: &fresh_events,
            signal_events: &signal_events,
        });
        assert_ne!(drained, fresh, "the payload frontier preserves drained IDs");
    }

    #[test]
    fn every_signal_store_section_is_hash_sensitive() {
        let gate = GateId(EntityId(1));
        let wire = crate::WireId(EntityId(2));
        let mut base = SignalWorld::new();
        let ports = base
            .activate_gate(gate, GateType::Not, Tick(0))
            .expect("Gate signal state activates");
        base.activate_wire(wire)
            .expect("Wire signal state activates");
        let base_bytes = signal_bytes(&base);

        let mut driver_changed = base.clone();
        driver_changed
            .apply_driver_sample(
                ports.input_a.external_driver,
                LogicLevel::High,
                crate::DriveStrength(101),
                Tick(1),
            )
            .expect("Driver sample changes");
        assert_ne!(signal_bytes(&driver_changed), base_bytes);

        let mut sink_slot_changed = base.clone();
        sink_slot_changed
            .apply_slot_sample(
                ports.input_a.sink,
                DriverSample::s0m3(
                    ports.input_a.external_driver,
                    LogicLevel::High,
                    crate::DriveStrength(101),
                    Tick(0),
                ),
            )
            .expect("Sink Driver slot changes");
        assert_ne!(signal_bytes(&sink_slot_changed), base_bytes);

        let mut gate_changed = base.clone();
        gate_changed
            .advance_pending_generation(gate)
            .expect("pending generation advances");
        gate_changed
            .set_pending(gate, Tick(3), LogicLevel::High, crate::Energy(5))
            .expect("pending Gate transition records");
        gate_changed
            .add_cancelled_heat(gate, crate::Energy(7))
            .expect("cancelled heat changes");
        assert_ne!(signal_bytes(&gate_changed), base_bytes);

        let mut wire_changed = base.clone();
        wire_changed
            .set_wire_excitations(&std::collections::BTreeMap::from([(
                wire,
                DriveVector {
                    high: 11,
                    low: 13,
                    unknown: 17,
                },
            )]))
            .expect("Wire excitation changes");
        assert_ne!(signal_bytes(&wire_changed), base_bytes);

        let mut tombstoned = base;
        tombstoned.remove_gate(gate).expect("Gate endpoints remove");
        assert_ne!(signal_bytes(&tombstoned), base_bytes);
    }

    fn append_point(output: &mut Vec<u8>, point: FixedVec2) {
        output.extend_from_slice(&point.x.0.to_le_bytes());
        output.extend_from_slice(&point.y.0.to_le_bytes());
    }

    fn append_aabb(output: &mut Vec<u8>, aabb: FixedAabb) {
        append_point(output, aabb.min);
        append_point(output, aabb.max);
    }

    fn append_event_key(output: &mut Vec<u8>, key: (u64, u8, u64, u64, u64, u32, u64)) {
        let (due_tick, kind, target, source, revision, generation, payload) = key;
        output.extend_from_slice(&due_tick.to_le_bytes());
        output.push(kind);
        output.extend_from_slice(&target.to_le_bytes());
        output.extend_from_slice(&source.to_le_bytes());
        output.extend_from_slice(&revision.to_le_bytes());
        output.extend_from_slice(&generation.to_le_bytes());
        output.extend_from_slice(&payload.to_le_bytes());
    }

    fn append_empty_runtime(output: &mut Vec<u8>) {
        for _ in 0..4 {
            output.extend_from_slice(&0_u64.to_le_bytes());
        }
        append_empty_signal_and_events(output);
    }

    fn signal_bytes(signal: &SignalWorld) -> Vec<u8> {
        let mut bytes = Vec::new();
        encode_signal_stores(signal, &mut |part| bytes.extend_from_slice(part));
        bytes
    }

    fn append_empty_signal_and_events(output: &mut Vec<u8>) {
        for value in [1_u64, 0, 1, 0, 0, 0, 0, 1, 0, 0] {
            output.extend_from_slice(&value.to_le_bytes());
        }
        for _ in 0..FUTURE_EMPTY_STORE_COUNT {
            output.extend_from_slice(&0_u64.to_le_bytes());
        }
    }
}
