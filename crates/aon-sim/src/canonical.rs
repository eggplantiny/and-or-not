use crate::event::{
    DriverSample, DriverTransition, EventCalendar, EventKey, EventPayloadAllocator, SignalArrival,
};
use crate::path_certificate::{PathCertificateArena, PathElementStamp};
use crate::power_source::PowerSourceStore;
use crate::signal::{DriveVector, GateSignalRecord, SignalWorld, WireSignalSnapshot};
use crate::structural::StructuralWorld;
use crate::{
    EndpointTarget, EntityLocation, EntityRegistry, FixedAabb, FixedVec2, Heading, LogicLevel,
    MainCoreState, Revision, RoutingDomain, SimulationContract, StateHash, Tick, TrackPosition,
};

const STATE_DOMAIN: &[u8] = b"AON\0STATE\0V6\0";
pub(crate) const STATE_ENCODER_VERSION: u16 = 6;
const RESERVED_EMPTY_STORE_COUNT: usize = 3;

#[derive(Clone, Copy)]
pub(crate) struct StateView<'a> {
    pub contract: &'a SimulationContract,
    pub next_tick: Tick,
    pub topology_revision: Revision,
    pub main_core: Option<&'a MainCoreState>,
    pub power_sources: &'a PowerSourceStore,
    pub structural: &'a StructuralWorld,
    pub signal: &'a SignalWorld,
    pub event_payloads: &'a EventPayloadAllocator,
    pub driver_events: &'a EventCalendar<DriverTransition>,
    pub signal_events: &'a EventCalendar<SignalArrival>,
    pub path_certificates: &'a PathCertificateArena,
}

#[derive(Clone, Copy)]
struct StateComponents<'a> {
    contract: &'a SimulationContract,
    next_tick: Tick,
    topology_revision: Revision,
    entities: &'a EntityRegistry,
    main_core: Option<&'a MainCoreState>,
    power_sources: &'a PowerSourceStore,
    structural: Option<&'a StructuralWorld>,
    signal: &'a SignalWorld,
    event_payloads: &'a EventPayloadAllocator,
    driver_events: &'a EventCalendar<DriverTransition>,
    signal_events: &'a EventCalendar<SignalArrival>,
    path_certificates: &'a PathCertificateArena,
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
            main_core: state.main_core,
            power_sources: state.power_sources,
            structural: Some(state.structural),
            signal: state.signal,
            event_payloads: state.event_payloads,
            driver_events: state.driver_events,
            signal_events: state.signal_events,
            path_certificates: state.path_certificates,
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

    match state.main_core {
        Some(core) => {
            write_u8(1, write);
            write_u64(core.id().entity_id().0, write);
            encode_point(core.position(), write);
            match core.anchor_node() {
                crate::TopologyNodeId::MainCoreAnchor(id) => {
                    write_u8(0, write);
                    write_u64(id.entity_id().0, write);
                }
                crate::TopologyNodeId::PowerSourceAnchor(_) => {
                    unreachable!("a Main Core cannot expose a Power Source anchor")
                }
            }
            write_u64(core.capacity().0, write);
            write_u64(core.integrity().0, write);
            write_u64(core.heat_energy().0, write);
        }
        None => write_u8(0, write),
    }

    encode_power_sources(state.power_sources, write);

    if let Some(structural) = state.structural {
        encode_structural_stores(structural, write);
    } else {
        for _ in 0..5 {
            write_u64(0, write);
        }
    }

    encode_signal_stores(state.signal, write);
    write_u64(state.event_payloads.next_payload_order(), write);
    encode_driver_events(state.driver_events, write);
    encode_signal_events(state.signal_events, write);

    // Destruction, radiation, and relay stores remain reserved in V6.
    for _ in 0..RESERVED_EMPTY_STORE_COUNT {
        write_u64(0, write);
    }
    encode_path_certificates(state.path_certificates, write);
}

fn encode_power_sources(sources: &PowerSourceStore, write: &mut dyn FnMut(&[u8])) {
    let live_count = u32::try_from(sources.len())
        .expect("Power Source store live count must fit the canonical u32 boundary");
    write_u32(live_count, write);
    for source in sources.iter() {
        write_u64(source.id().entity_id().0, write);
        encode_point(source.position(), write);
        match source.power_attachment() {
            crate::TopologyNodeId::PowerSourceAnchor(id) => {
                write_u8(1, write);
                write_u64(id.entity_id().0, write);
            }
            crate::TopologyNodeId::MainCoreAnchor(_) => {
                unreachable!("a Power Source cannot expose a Main Core anchor")
            }
        }
        write_u64(source.generation_per_tick().0, write);
    }
}

fn encode_path_certificates(certificates: &PathCertificateArena, write: &mut dyn FnMut(&[u8])) {
    write_u64(certificates.frontier().0, write);
    write_u64(certificates.allocated_count(), write);
    for (id, certificate) in certificates.canonical_slots() {
        write_u64(id.0, write);
        match certificate {
            Some(_) => {
                write_u8(1, write);
                let elements = certificates
                    .elements(id)
                    .expect("canonical state requires a valid live Path Certificate range");
                let element_count = u32::try_from(elements.len())
                    .expect("Path Certificate arena guarantees u32 element ranges");
                write_u32(element_count, write);
                for &element in elements {
                    encode_path_element_stamp(element, write);
                }
            }
            None => write_u8(0, write),
        }
    }
}

fn encode_path_element_stamp(element: PathElementStamp, write: &mut dyn FnMut(&[u8])) {
    write_u8(element.canonical_tag(), write);
    write_u64(element.entity_id().0, write);
    write_u64(element.generation().0, write);
}

fn encode_signal_stores(signal: &SignalWorld, write: &mut dyn FnMut(&[u8])) {
    write_u64(signal.driver_frontier().entity_id().0, write);
    write_u64(signal.allocated_driver_count(), write);
    for (driver_id, record) in signal.canonical_driver_slots() {
        write_u64(driver_id.entity_id().0, write);
        match record {
            Some(record) => {
                write_u8(1, write);
                write_u64(record.owner.0, write);
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
                write_u64(record.owner.0, write);
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
        encode_gate_signal_record(gate, write);
    }

    let mobiles: Vec<_> = signal.iter_mobile_entries().collect();
    write_u64(mobiles.len() as u64, write);
    for (mobile, ports) in mobiles {
        write_u64(mobile.entity_id().0, write);
        write_u64(ports.stop.entity_id().0, write);
        write_u64(ports.left.entity_id().0, write);
        write_u64(ports.right.entity_id().0, write);
    }

    let wires: Vec<_> = signal.iter_wires().collect();
    write_u64(wires.len() as u64, write);
    for (wire_id, state) in wires {
        encode_wire_signal_record(signal, wire_id, state, write);
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

fn encode_gate_signal_record(gate: GateSignalRecord, write: &mut dyn FnMut(&[u8])) {
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
    write_u64(gate.unpowered_ticks, write);
}

fn encode_wire_signal_record(
    signal: &SignalWorld,
    wire_id: crate::WireId,
    state: WireSignalSnapshot,
    write: &mut dyn FnMut(&[u8]),
) {
    write_u64(wire_id.entity_id().0, write);
    encode_drive_vector(state.active, write);
    encode_drive_vector(state.previous, write);
    match signal.wire_sense_snapshot(wire_id) {
        Some(sense) => {
            write_u8(1, write);
            write_u64(sense.ports.a.entity_id().0, write);
            write_u64(sense.ports.b.entity_id().0, write);
            write_u8(u8::from(sense.sampled_presence), write);
            write_u8(logic_level_tag(sense.intended_level), write);
            write_u64(sense.intended_strength.0, write);
        }
        None => write_u8(0, write),
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

    let mut mobiles: Vec<_> = structural
        .mobile_substrates()
        .iter_alive()
        .map(|(_, record)| record)
        .collect();
    mobiles.sort_unstable_by_key(|record| record.id.entity_id());
    write_u64(mobiles.len() as u64, write);
    for record in mobiles {
        write_u64(record.id.entity_id().0, write);
        encode_track_position(record.track_position, write);
        encode_aabb(record.routing_area, write);
        encode_aabb(record.footprint, write);
    }
}

fn encode_track_position(position: TrackPosition, write: &mut dyn FnMut(&[u8])) {
    match position {
        TrackPosition::Edge {
            edge,
            offset,
            heading,
        } => {
            write_u8(0, write);
            write_u64(edge.entity_id().0, write);
            write_i64(offset.0, write);
            write_u8(
                match heading {
                    Heading::Forward => 0,
                    Heading::Reverse => 1,
                },
                write,
            );
        }
        TrackPosition::Junction {
            junction,
            incoming_edge,
        } => {
            write_u8(1, write);
            write_u64(junction.entity_id().0, write);
            write_u64(incoming_edge.entity_id().0, write);
        }
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
        EndpointTarget::MobilePort(reference) => {
            write_u64(reference.mobile.entity_id().0, write);
            write_u8(reference.port.canonical_tag(), write);
        }
        EndpointTarget::MainCoreAnchor(core) => write_u64(core.entity_id().0, write),
        EndpointTarget::PowerSourceAnchor(source) => write_u64(source.entity_id().0, write),
        EndpointTarget::WireSensePort(reference) => {
            write_u64(reference.wire.entity_id().0, write);
            write_u8(reference.end.canonical_tag(), write);
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
    use crate::event::{UncertifiedSignalArrival, stage_signal_arrivals};
    use crate::{
        Command, CommandEnvelope, ConnectionGeneration, EndpointTarget, EntityId, EntityLocation,
        Fixed, FixedAabb, FixedSubstrateIndex, GateId, GateIndex, GatePort, GatePortRef, GateType,
        JunctionId, JunctionIndex, MobileId, MobilePort, MobilePortRef, MobileSubstrateIndex,
        PhysicalScaleProfile, PlaceFixedSubstrateCommand, PlaceGateCommand, PlaceJunctionCommand,
        PlaceMobileSubstrateCommand, PlaceWireCommand, ProfileHash, RoutingDomain, SinkId, WireId,
        WireIndex,
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
        power_sources: PowerSourceStore,
        signal: SignalWorld,
        payloads: EventPayloadAllocator,
        driver_events: EventCalendar<DriverTransition>,
        signal_events: EventCalendar<SignalArrival>,
        path_certificates: PathCertificateArena,
    }

    impl TestRuntime {
        fn new() -> Self {
            Self {
                power_sources: PowerSourceStore::default(),
                signal: SignalWorld::new(),
                payloads: EventPayloadAllocator::new(),
                driver_events: EventCalendar::new(),
                signal_events: EventCalendar::new(),
                path_certificates: PathCertificateArena::new(),
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
                main_core: None,
                power_sources: &self.power_sources,
                structural,
                signal: &self.signal,
                event_payloads: &self.payloads,
                driver_events: &self.driver_events,
                signal_events: &self.signal_events,
                path_certificates: &self.path_certificates,
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
                main_core: None,
                power_sources: &runtime.power_sources,
                structural: None,
                signal: &runtime.signal,
                event_payloads: &runtime.payloads,
                driver_events: &runtime.driver_events,
                signal_events: &runtime.signal_events,
                path_certificates: &runtime.path_certificates,
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

    const fn wire_stamp(id: u64, generation: u64) -> PathElementStamp {
        PathElementStamp::Wire {
            id: WireId(EntityId(id)),
            generation: ConnectionGeneration(generation),
        }
    }

    const fn junction_stamp(id: u64, generation: u64) -> PathElementStamp {
        PathElementStamp::Junction {
            id: JunctionId(EntityId(id)),
            generation: ConnectionGeneration(generation),
        }
    }

    #[test]
    fn state_encoding_v6_has_exact_contract_tick_revision_and_identity_order() {
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
                main_core: None,
                power_sources: &runtime.power_sources,
                structural: None,
                signal: &runtime.signal,
                event_payloads: &runtime.payloads,
                driver_events: &runtime.driver_events,
                signal_events: &runtime.signal_events,
                path_certificates: &runtime.path_certificates,
            },
            &mut |bytes| actual.extend_from_slice(bytes),
        );

        let mut expected = Vec::new();
        expected.extend_from_slice(b"AON\0STATE\0V6\0");
        expected.extend_from_slice(&6_u16.to_le_bytes());
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
        expected.push(0);
        append_empty_runtime(&mut expected);

        assert_eq!(actual, expected);
    }

    #[test]
    fn main_core_v6_section_has_exact_anchor_order_and_is_hash_sensitive() {
        let mut entities = EntityRegistry::new();
        let id = crate::MainCoreId(
            entities
                .allocate(EntityLocation::MainCore)
                .expect("Main Core allocation succeeds"),
        );
        let core = MainCoreState::new(
            id,
            point(-65_536, 131_072),
            crate::Capacity(65_536_000),
            crate::Integrity(77),
            crate::HeatEnergy(9),
        );
        let runtime = TestRuntime::new();
        let encode = |core: MainCoreState| {
            let mut bytes = Vec::new();
            encode_state_components(
                StateComponents {
                    contract: &contract(),
                    next_tick: Tick(0),
                    topology_revision: Revision(0),
                    entities: &entities,
                    main_core: Some(&core),
                    power_sources: &runtime.power_sources,
                    structural: None,
                    signal: &runtime.signal,
                    event_payloads: &runtime.payloads,
                    driver_events: &runtime.driver_events,
                    signal_events: &runtime.signal_events,
                    path_certificates: &runtime.path_certificates,
                },
                &mut |part| bytes.extend_from_slice(part),
            );
            bytes
        };

        let baseline = encode(core);
        let header_len = STATE_DOMAIN.len() + 2 + 1 + 32 * 3 + 8 * 4 + 10;
        let mut expected = vec![1];
        expected.extend_from_slice(&1_u64.to_le_bytes());
        append_point(&mut expected, point(-65_536, 131_072));
        expected.push(0);
        expected.extend_from_slice(&1_u64.to_le_bytes());
        expected.extend_from_slice(&65_536_000_u64.to_le_bytes());
        expected.extend_from_slice(&77_u64.to_le_bytes());
        expected.extend_from_slice(&9_u64.to_le_bytes());
        assert_eq!(&baseline[header_len..header_len + expected.len()], expected);

        let mut full_expected = Vec::new();
        full_expected.extend_from_slice(b"AON\0STATE\0V6\0");
        full_expected.extend_from_slice(&6_u16.to_le_bytes());
        full_expected.push(0);
        full_expected.extend_from_slice(&[0x11; 32]);
        full_expected.extend_from_slice(&[0x22; 32]);
        full_expected.extend_from_slice(&[0x33; 32]);
        full_expected.extend_from_slice(&0_u64.to_le_bytes());
        full_expected.extend_from_slice(&0_u64.to_le_bytes());
        full_expected.extend_from_slice(&2_u64.to_le_bytes());
        full_expected.extend_from_slice(&1_u64.to_le_bytes());
        full_expected.extend_from_slice(&1_u64.to_le_bytes());
        full_expected.push(1);
        full_expected.push(0);
        full_expected.extend_from_slice(&expected);
        append_empty_runtime(&mut full_expected);
        assert_eq!(baseline, full_expected);

        assert_ne!(
            baseline,
            encode(core.with_id_for_test(crate::MainCoreId(EntityId(2))))
        );

        for changed in [
            MainCoreState::new(
                id,
                point(-64_512, 131_072),
                crate::Capacity(65_536_000),
                crate::Integrity(77),
                crate::HeatEnergy(9),
            ),
            MainCoreState::new(
                id,
                point(-65_536, 132_096),
                crate::Capacity(65_536_000),
                crate::Integrity(77),
                crate::HeatEnergy(9),
            ),
            MainCoreState::new(
                id,
                point(-65_536, 131_072),
                crate::Capacity(65_536_001),
                crate::Integrity(77),
                crate::HeatEnergy(9),
            ),
            MainCoreState::new(
                id,
                point(-65_536, 131_072),
                crate::Capacity(65_536_000),
                crate::Integrity(78),
                crate::HeatEnergy(9),
            ),
            MainCoreState::new(
                id,
                point(-65_536, 131_072),
                crate::Capacity(65_536_000),
                crate::Integrity(77),
                crate::HeatEnergy(10),
            ),
        ] {
            assert_ne!(baseline, encode(changed));
        }

        let hash = StateHash::from_bytes(*blake3::hash(&baseline).as_bytes());
        assert_eq!(
            hash.to_string(),
            "b53415b52909b20aa0b89623689961f14bb51386e0313edbf3ae84aead89c341"
        );
    }

    #[test]
    fn power_source_v6_section_has_exact_sorted_records_and_field_sensitive_hash() {
        let source = |id, x, y, generation| {
            crate::PowerSourceState::new(
                crate::PowerSourceId(EntityId(id)),
                point(x, y),
                crate::Energy(generation),
            )
        };
        let encode = |states| {
            let store = PowerSourceStore::new(states).expect("Power Source fixture is valid");
            let mut bytes = Vec::new();
            encode_power_sources(&store, &mut |part| bytes.extend_from_slice(part));
            bytes
        };

        let actual = encode(vec![source(7, 131_072, -65_536, 19), source(2, -1, 3, 11)]);
        let mut expected = Vec::new();
        expected.extend_from_slice(&2_u32.to_le_bytes());
        for (id, position, generation) in [
            (2_u64, point(-1, 3), 11_u64),
            (7, point(131_072, -65_536), 19),
        ] {
            expected.extend_from_slice(&id.to_le_bytes());
            append_point(&mut expected, position);
            expected.push(1); // TopologyNodeId::PowerSourceAnchor.
            expected.extend_from_slice(&id.to_le_bytes());
            expected.extend_from_slice(&generation.to_le_bytes());
        }
        assert_eq!(actual, expected);

        let baseline_hash = blake3::hash(&actual);
        assert_eq!(
            baseline_hash.to_hex().as_str(),
            "07c4372486f3f926f550153d8ec7cdbe927108aaa63f465b1a07bcdb23f88592"
        );
        for changed in [
            encode(vec![source(8, 131_072, -65_536, 19), source(2, -1, 3, 11)]),
            encode(vec![source(7, 131_073, -65_536, 19), source(2, -1, 3, 11)]),
            encode(vec![source(7, 131_072, -65_535, 19), source(2, -1, 3, 11)]),
            encode(vec![source(7, 131_072, -65_536, 20), source(2, -1, 3, 11)]),
            encode(vec![source(7, 131_072, -65_536, 19)]),
        ] {
            assert_ne!(blake3::hash(&changed), baseline_hash);
        }
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
    fn structural_state_encoding_has_exact_v6_entity_order_records_and_raw_vertices() {
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
        expected.extend_from_slice(b"AON\0STATE\0V6\0");
        expected.extend_from_slice(&6_u16.to_le_bytes());
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

        expected.push(0);

        expected.extend_from_slice(&0_u32.to_le_bytes());

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
        expected.extend_from_slice(&0_u64.to_le_bytes());
        append_empty_signal_and_events(&mut expected);

        assert_eq!(actual, expected);
        assert_eq!(
            state_hash(runtime.view(&contract, Tick(3), Revision(2), &world)).to_string(),
            "617bb627b25d3da52a28a3295246f086e8cb639ed08edea5ca8401cfb000844e"
        );
    }

    #[test]
    fn mobile_track_positions_have_exact_v6_bytes_and_field_boundaries() {
        const EDGE_LENGTH: i64 = 65_536;
        let edge = |edge, offset, heading| TrackPosition::Edge {
            edge: WireId(EntityId(edge)),
            offset: Fixed(offset),
            heading,
        };
        let junction = |junction, incoming_edge| TrackPosition::Junction {
            junction: JunctionId(EntityId(junction)),
            incoming_edge: WireId(EntityId(incoming_edge)),
        };

        let baseline = track_position_bytes(edge(7, 0, Heading::Forward));
        let mut expected = vec![0];
        expected.extend_from_slice(&7_u64.to_le_bytes());
        expected.extend_from_slice(&0_i64.to_le_bytes());
        expected.push(0);
        assert_eq!(baseline, expected, "Edge/offset-zero/Forward bytes");

        let edge_changed = track_position_bytes(edge(8, 0, Heading::Forward));
        let mut expected = vec![0];
        expected.extend_from_slice(&8_u64.to_le_bytes());
        expected.extend_from_slice(&0_i64.to_le_bytes());
        expected.push(0);
        assert_eq!(edge_changed, expected, "Edge identity bytes");

        let edge_end = track_position_bytes(edge(7, EDGE_LENGTH, Heading::Forward));
        let mut expected = vec![0];
        expected.extend_from_slice(&7_u64.to_le_bytes());
        expected.extend_from_slice(&EDGE_LENGTH.to_le_bytes());
        expected.push(0);
        assert_eq!(edge_end, expected, "inclusive edge-length boundary bytes");

        let reversed = track_position_bytes(edge(7, 0, Heading::Reverse));
        let mut expected = vec![0];
        expected.extend_from_slice(&7_u64.to_le_bytes());
        expected.extend_from_slice(&0_i64.to_le_bytes());
        expected.push(1);
        assert_eq!(reversed, expected, "Reverse heading bytes");

        let at_junction = track_position_bytes(junction(11, 7));
        let mut expected = vec![1];
        expected.extend_from_slice(&11_u64.to_le_bytes());
        expected.extend_from_slice(&7_u64.to_le_bytes());
        assert_eq!(at_junction, expected, "Junction discriminant and IDs");

        let junction_changed = track_position_bytes(junction(12, 7));
        let incoming_changed = track_position_bytes(junction(11, 8));
        assert_ne!(baseline, edge_changed, "Edge ID is canonical");
        assert_ne!(baseline, edge_end, "Edge offset is canonical");
        assert_ne!(baseline, reversed, "Heading is canonical");
        assert_ne!(
            baseline, at_junction,
            "Edge/Junction discriminant is canonical"
        );
        assert_ne!(at_junction, junction_changed, "Junction ID is canonical");
        assert_ne!(
            at_junction, incoming_changed,
            "incoming Edge ID is canonical"
        );
    }

    #[test]
    fn populated_mobile_state_hash_is_sensitive_to_every_track_position_field() {
        const WORLD_PITCH: i64 = 65_536;
        let (world, runtime, mobile_index) = populated_mobile_fixture();
        let contract = contract();
        let mobile = MobileId(EntityId(5));
        let edge_a = WireId(EntityId(3));
        let edge_b = WireId(EntityId(4));
        let junction_a = JunctionId(EntityId(1));
        let junction_b = JunctionId(EntityId(2));
        let hash = |position| {
            let mut changed = world.clone();
            changed
                .commit_mobile_positions(&[(mobile_index, mobile, position)])
                .expect("test Mobile position updates");
            state_hash(runtime.view(&contract, Tick(3), Revision(3), &changed))
        };

        let edge_zero = TrackPosition::Edge {
            edge: edge_a,
            offset: Fixed(0),
            heading: Heading::Forward,
        };
        let baseline = hash(edge_zero);
        assert_ne!(
            baseline,
            hash(TrackPosition::Edge {
                edge: edge_b,
                offset: Fixed(0),
                heading: Heading::Forward,
            }),
            "Edge identity reaches the full V6 state hash"
        );
        assert_ne!(
            baseline,
            hash(TrackPosition::Edge {
                edge: edge_a,
                offset: Fixed(WORLD_PITCH),
                heading: Heading::Forward,
            }),
            "the exact edge-length offset boundary reaches the full V6 state hash"
        );
        assert_ne!(
            baseline,
            hash(TrackPosition::Edge {
                edge: edge_a,
                offset: Fixed(0),
                heading: Heading::Reverse,
            }),
            "Heading reaches the full V6 state hash"
        );

        let at_junction = TrackPosition::Junction {
            junction: junction_a,
            incoming_edge: edge_a,
        };
        let junction_hash = hash(at_junction);
        assert_ne!(
            baseline, junction_hash,
            "Edge/Junction discriminant reaches the full V6 state hash"
        );
        assert_ne!(
            junction_hash,
            hash(TrackPosition::Junction {
                junction: junction_b,
                incoming_edge: edge_a,
            }),
            "Junction identity reaches the full V6 state hash"
        );
        assert_ne!(
            junction_hash,
            hash(TrackPosition::Junction {
                junction: junction_a,
                incoming_edge: edge_b,
            }),
            "incoming Edge identity reaches the full V6 state hash"
        );
    }

    #[test]
    fn populated_mobile_control_sink_map_has_exact_v6_signal_bytes() {
        let mobile = MobileId(EntityId(17));
        let mut signal = SignalWorld::new();
        let ports = signal
            .activate_mobile(mobile)
            .expect("Mobile control sinks activate");
        assert_eq!(
            ports,
            crate::MobileControlPorts {
                stop: SinkId(EntityId(1)),
                left: SinkId(EntityId(2)),
                right: SinkId(EntityId(3)),
            },
            "STOP/LEFT/RIGHT retain distinct canonical Sink identities"
        );

        let mut expected = Vec::new();
        expected.extend_from_slice(&1_u64.to_le_bytes()); // Driver frontier.
        expected.extend_from_slice(&0_u64.to_le_bytes()); // Driver count.
        expected.extend_from_slice(&4_u64.to_le_bytes()); // Sink frontier.
        expected.extend_from_slice(&3_u64.to_le_bytes()); // Sink count.
        for (sink, role) in [(1_u64, 2_u8), (2, 3), (3, 4)] {
            expected.extend_from_slice(&sink.to_le_bytes());
            expected.push(1); // Live Sink slot.
            expected.extend_from_slice(&17_u64.to_le_bytes());
            expected.push(role);
            expected.push(0); // LogicLevel::Low.
            expected.push(0); // Clean.
        }
        expected.extend_from_slice(&0_u64.to_le_bytes()); // Gate count.
        expected.extend_from_slice(&1_u64.to_le_bytes()); // Mobile map count.
        for value in [17_u64, 1, 2, 3] {
            expected.extend_from_slice(&value.to_le_bytes());
        }
        expected.extend_from_slice(&0_u64.to_le_bytes()); // Wire count.
        expected.extend_from_slice(&0_u64.to_le_bytes()); // Sink/Driver slot count.

        assert_eq!(signal_bytes(&signal), expected);
    }

    #[test]
    fn mobile_port_endpoint_has_exact_v6_bytes_for_all_control_sinks() {
        let endpoint = |mobile, port| {
            endpoint_bytes(EndpointTarget::MobilePort(MobilePortRef {
                mobile: MobileId(EntityId(mobile)),
                port,
            }))
        };
        for (port, tag) in [
            (MobilePort::Stop, 0_u8),
            (MobilePort::Left, 1),
            (MobilePort::Right, 2),
        ] {
            let mut expected = vec![3]; // EndpointTarget::MobilePort.
            expected.extend_from_slice(&29_u64.to_le_bytes());
            expected.push(tag);
            assert_eq!(endpoint(29, port), expected);
        }
        assert_ne!(
            endpoint(29, MobilePort::Stop),
            endpoint(30, MobilePort::Stop),
            "Mobile endpoint owner identity is canonical"
        );
    }

    #[test]
    fn main_core_anchor_endpoint_has_exact_v6_bytes() {
        let endpoint = |id| {
            endpoint_bytes(EndpointTarget::MainCoreAnchor(crate::MainCoreId(EntityId(
                id,
            ))))
        };
        let mut expected = vec![4];
        expected.extend_from_slice(&29_u64.to_le_bytes());
        assert_eq!(endpoint(29), expected);
        assert_ne!(endpoint(29), endpoint(30));
    }

    #[test]
    fn populated_path_certificate_section_has_exact_v3_bytes() {
        let mut certificates = PathCertificateArena::new();
        let consumed_path = [wire_stamp(7, 11), junction_stamp(8, 13)];
        let allocated = certificates
            .allocate_batch(&[consumed_path.as_slice(), &[]])
            .expect("initial Path Certificates allocate");
        certificates
            .consume(allocated[0])
            .expect("first Path Certificate consumes");
        let retained_path = [junction_stamp(9, 17), wire_stamp(10, 19)];
        certificates
            .allocate_batch(&[retained_path.as_slice()])
            .expect("retained Path Certificate allocates");

        let actual = certificate_bytes(&certificates);
        let mut expected = Vec::new();
        expected.extend_from_slice(&4_u64.to_le_bytes());
        expected.extend_from_slice(&3_u64.to_le_bytes());
        expected.extend_from_slice(&1_u64.to_le_bytes());
        expected.push(0);
        expected.extend_from_slice(&2_u64.to_le_bytes());
        expected.push(1);
        expected.extend_from_slice(&0_u32.to_le_bytes());
        expected.extend_from_slice(&3_u64.to_le_bytes());
        expected.push(1);
        expected.extend_from_slice(&2_u32.to_le_bytes());
        expected.push(1);
        expected.extend_from_slice(&9_u64.to_le_bytes());
        expected.extend_from_slice(&17_u64.to_le_bytes());
        expected.push(0);
        expected.extend_from_slice(&10_u64.to_le_bytes());
        expected.extend_from_slice(&19_u64.to_le_bytes());

        assert_eq!(actual, expected);
    }

    #[test]
    fn path_certificate_fields_frontier_and_tombstones_are_hash_sensitive() {
        let arena = |elements: &[PathElementStamp]| {
            let mut certificates = PathCertificateArena::new();
            certificates
                .allocate_batch(&[elements])
                .expect("Path Certificate allocates");
            certificates
        };
        let baseline = arena(&[wire_stamp(7, 11), junction_stamp(8, 13)]);
        let baseline_hash = path_certificate_state_hash(&baseline);

        for changed in [
            arena(&[wire_stamp(7, 12), junction_stamp(8, 13)]),
            arena(&[wire_stamp(17, 11), junction_stamp(8, 13)]),
            arena(&[junction_stamp(7, 11), junction_stamp(8, 13)]),
            arena(&[junction_stamp(8, 13), wire_stamp(7, 11)]),
            arena(&[wire_stamp(7, 11)]),
        ] {
            assert_ne!(path_certificate_state_hash(&changed), baseline_hash);
        }

        let mut tombstoned = baseline.clone();
        tombstoned
            .consume(crate::PathCertificateId(1))
            .expect("Path Certificate consumes");
        assert_ne!(path_certificate_state_hash(&tombstoned), baseline_hash);

        let empty = PathCertificateArena::new();
        let mut allocated_tombstone = PathCertificateArena::new();
        let id = allocated_tombstone
            .allocate_batch(&[&[]])
            .expect("empty Path Certificate allocates")[0];
        allocated_tombstone
            .consume(id)
            .expect("empty Path Certificate consumes");
        assert_ne!(
            path_certificate_state_hash(&allocated_tombstone),
            path_certificate_state_hash(&empty),
            "the monotonic frontier and tombstone slot are canonical"
        );
    }

    #[test]
    fn path_certificate_raw_ranges_orphan_bytes_and_capacity_are_not_canonical() {
        let retained_path = [wire_stamp(31, 37), junction_stamp(41, 43)];
        let discarded_prefix = (0_u64..128)
            .map(|offset| wire_stamp(100 + offset, 200 + offset))
            .collect::<Vec<_>>();

        let mut larger_raw_arena = PathCertificateArena::new();
        let larger_ids = larger_raw_arena
            .allocate_batch(&[discarded_prefix.as_slice(), retained_path.as_slice()])
            .expect("larger raw arena allocates");
        larger_raw_arena
            .consume(larger_ids[0])
            .expect("larger orphan prefix consumes");

        let mut smaller_raw_arena = PathCertificateArena::new();
        let smaller_ids = smaller_raw_arena
            .allocate_batch(&[&[], retained_path.as_slice()])
            .expect("smaller raw arena allocates");
        smaller_raw_arena
            .consume(smaller_ids[0])
            .expect("empty orphan prefix consumes");

        assert_ne!(
            larger_raw_arena, smaller_raw_arena,
            "raw offsets, orphan elements, and backing allocations differ"
        );
        assert_eq!(
            certificate_bytes(&larger_raw_arena),
            certificate_bytes(&smaller_raw_arena)
        );
        assert_eq!(
            path_certificate_state_hash(&larger_raw_arena),
            path_certificate_state_hash(&smaller_raw_arena)
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
        let mut path_certificates = PathCertificateArena::new();
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
        stage_signal_arrivals(
            &mut signal_events,
            &mut payloads,
            &mut path_certificates,
            [UncertifiedSignalArrival::propagation(
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
                Vec::new(),
            )],
        )
        .expect("certified Signal event stages");

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
        expected.push(1);
        expected.extend_from_slice(&1_u64.to_le_bytes());
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
        let path_certificates = PathCertificateArena::new();
        let power_sources = PowerSourceStore::default();

        let left = state_hash(StateView {
            contract: &contract,
            next_tick: Tick(3),
            topology_revision: Revision(0),
            main_core: None,
            power_sources: &power_sources,
            structural: &structural,
            signal: &signal,
            event_payloads: &left_payloads,
            driver_events: &left_events,
            signal_events: &signal_events,
            path_certificates: &path_certificates,
        });
        let right = state_hash(StateView {
            event_payloads: &right_payloads,
            driver_events: &right_events,
            ..StateView {
                contract: &contract,
                next_tick: Tick(3),
                topology_revision: Revision(0),
                main_core: None,
                power_sources: &power_sources,
                structural: &structural,
                signal: &signal,
                event_payloads: &left_payloads,
                driver_events: &left_events,
                signal_events: &signal_events,
                path_certificates: &path_certificates,
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
                main_core: None,
                power_sources: &power_sources,
                structural: &structural,
                signal: &signal,
                event_payloads: &left_payloads,
                driver_events: &left_events,
                signal_events: &signal_events,
                path_certificates: &path_certificates,
            }
        });
        let fresh_payloads = EventPayloadAllocator::new();
        let fresh_events = EventCalendar::new();
        let fresh = state_hash(StateView {
            contract: &contract,
            next_tick: Tick(5),
            topology_revision: Revision(0),
            main_core: None,
            power_sources: &power_sources,
            structural: &structural,
            signal: &signal,
            event_payloads: &fresh_payloads,
            driver_events: &fresh_events,
            signal_events: &signal_events,
            path_certificates: &path_certificates,
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

        let mut unpowered_changed = base.clone();
        unpowered_changed
            .set_gate_unpowered_ticks(gate, 3)
            .expect("unpowered Tick counter changes");
        assert_ne!(signal_bytes(&unpowered_changed), base_bytes);

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

    #[test]
    fn gate_v6_row_appends_exact_unpowered_tick_counter() {
        let gate = GateId(EntityId(41));
        let mut signal = SignalWorld::new();
        signal
            .activate_gate(gate, GateType::Not, Tick(0))
            .expect("Gate signal state activates");

        let encode = |signal: &SignalWorld| {
            let mut bytes = Vec::new();
            encode_gate_signal_record(
                *signal.gate_record(gate).expect("Gate record is live"),
                &mut |part| bytes.extend_from_slice(part),
            );
            bytes
        };
        let baseline = encode(&signal);
        let mut expected = Vec::new();
        for value in [41_u64, 1, 1] {
            expected.extend_from_slice(&value.to_le_bytes());
        }
        expected.push(0); // No input B.
        expected.extend_from_slice(&2_u64.to_le_bytes());
        expected.extend_from_slice(&[0, 0]); // Current and desired LOW.
        expected.extend_from_slice(&0_u32.to_le_bytes());
        expected.extend_from_slice(&[0, 0, 0]); // No pending tuple.
        expected.extend_from_slice(&0_u64.to_le_bytes()); // Cancelled switching heat.
        expected.extend_from_slice(&0_u64.to_le_bytes()); // Unpowered Tick count.
        assert_eq!(baseline, expected);

        signal
            .set_gate_unpowered_ticks(gate, 9)
            .expect("unpowered Tick counter changes");
        let changed = encode(&signal);
        assert_eq!(
            &changed[..changed.len() - 8],
            &baseline[..baseline.len() - 8]
        );
        assert_eq!(&changed[changed.len() - 8..], &9_u64.to_le_bytes());
        assert_ne!(blake3::hash(&changed), blake3::hash(&baseline));
    }

    #[test]
    fn wire_v6_row_has_exact_optional_sense_state_and_field_sensitive_hash() {
        let wire = WireId(EntityId(41));
        let mut signal = SignalWorld::new();
        signal.activate_wire(wire).expect("Wire activates");

        let encode = |signal: &SignalWorld| {
            let mut bytes = Vec::new();
            encode_wire_signal_record(
                signal,
                wire,
                signal.wire_snapshot(wire).expect("Wire record is live"),
                &mut |part| bytes.extend_from_slice(part),
            );
            bytes
        };
        let without_sense = encode(&signal);
        let mut expected_without_sense = Vec::new();
        expected_without_sense.extend_from_slice(&41_u64.to_le_bytes());
        expected_without_sense.extend_from_slice(&[0; 16 * 6]);
        expected_without_sense.push(0);
        assert_eq!(without_sense, expected_without_sense);

        let ports = signal
            .activate_wire_sensing(wire, Tick(0))
            .expect("Wire sensing activates");
        signal
            .set_wire_sense_intent(wire, true, LogicLevel::High, crate::DriveStrength(333))
            .expect("Wire Sense intent changes");
        let with_sense = encode(&signal);
        let mut expected_with_sense = expected_without_sense;
        expected_with_sense.pop();
        expected_with_sense.push(1);
        expected_with_sense.extend_from_slice(&ports.a.entity_id().0.to_le_bytes());
        expected_with_sense.extend_from_slice(&ports.b.entity_id().0.to_le_bytes());
        expected_with_sense.push(1); // Sampled presence.
        expected_with_sense.push(1); // Intended HIGH.
        expected_with_sense.extend_from_slice(&333_u64.to_le_bytes());
        assert_eq!(with_sense, expected_with_sense);

        let baseline_hash = blake3::hash(&with_sense);
        for (sampled, level, strength) in [
            (false, LogicLevel::High, 333_u64),
            (true, LogicLevel::Low, 333),
            (true, LogicLevel::High, 334),
        ] {
            let mut changed = signal.clone();
            changed
                .set_wire_sense_intent(wire, sampled, level, crate::DriveStrength(strength))
                .expect("single Wire Sense field changes");
            assert_ne!(blake3::hash(&encode(&changed)), baseline_hash);
        }
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
        output.extend_from_slice(&0_u32.to_le_bytes());
        for _ in 0..5 {
            output.extend_from_slice(&0_u64.to_le_bytes());
        }
        append_empty_signal_and_events(output);
    }

    fn signal_bytes(signal: &SignalWorld) -> Vec<u8> {
        let mut bytes = Vec::new();
        encode_signal_stores(signal, &mut |part| bytes.extend_from_slice(part));
        bytes
    }

    fn track_position_bytes(position: TrackPosition) -> Vec<u8> {
        let mut bytes = Vec::new();
        encode_track_position(position, &mut |part| bytes.extend_from_slice(part));
        bytes
    }

    fn endpoint_bytes(endpoint: EndpointTarget) -> Vec<u8> {
        let mut bytes = Vec::new();
        encode_endpoint(endpoint, &mut |part| bytes.extend_from_slice(part));
        bytes
    }

    fn populated_mobile_fixture() -> (StructuralWorld, TestRuntime, MobileSubstrateIndex) {
        const WORLD_PITCH: i64 = 65_536;
        const CIRCUIT_PITCH: i64 = 16_384;
        let physical = PhysicalScaleProfile::stage0_alpha("canonical-mobile-test");
        let mut world = StructuralWorld::new();
        let mut runtime = TestRuntime::new();

        let junctions = world
            .apply_phase0_with_signal(
                &mut runtime.signal,
                Tick(0),
                &[
                    envelope(
                        0,
                        0,
                        Command::PlaceJunction(PlaceJunctionCommand {
                            routing_domain: RoutingDomain::OpenWorld,
                            position: point(0, 0),
                        }),
                    ),
                    envelope(
                        0,
                        1,
                        Command::PlaceJunction(PlaceJunctionCommand {
                            routing_domain: RoutingDomain::OpenWorld,
                            position: point(WORLD_PITCH, 0),
                        }),
                    ),
                ],
                &physical,
            )
            .expect("Mobile fixture Junctions place");
        assert!(junctions.rejections.is_empty());

        let wires = world
            .apply_phase0_with_signal(
                &mut runtime.signal,
                Tick(1),
                &[
                    envelope(
                        1,
                        0,
                        Command::PlaceWire(PlaceWireCommand {
                            routing_domain: RoutingDomain::OpenWorld,
                            points: vec![point(0, 0), point(WORLD_PITCH, 0)],
                            endpoint_a: EndpointTarget::Junction(JunctionId(EntityId(1))),
                            endpoint_b: EndpointTarget::Junction(JunctionId(EntityId(2))),
                        }),
                    ),
                    envelope(
                        1,
                        1,
                        Command::PlaceWire(PlaceWireCommand {
                            routing_domain: RoutingDomain::OpenWorld,
                            points: vec![point(0, 0), point(0, WORLD_PITCH)],
                            endpoint_a: EndpointTarget::Junction(JunctionId(EntityId(1))),
                            endpoint_b: EndpointTarget::Free,
                        }),
                    ),
                ],
                &physical,
            )
            .expect("Mobile fixture Edges place");
        assert!(wires.rejections.is_empty());

        let bounds = FixedAabb::new(
            point(-4 * CIRCUIT_PITCH, -4 * CIRCUIT_PITCH),
            point(4 * CIRCUIT_PITCH, 4 * CIRCUIT_PITCH),
        );
        let mobile = world
            .apply_phase0_with_signal(
                &mut runtime.signal,
                Tick(2),
                &[envelope(
                    2,
                    0,
                    Command::PlaceMobileSubstrate(PlaceMobileSubstrateCommand {
                        origin: point(WORLD_PITCH / 2, 0),
                        routing_area: bounds,
                        footprint: bounds,
                    }),
                )],
                &physical,
            )
            .expect("Mobile fixture substrate places");
        assert!(mobile.rejections.is_empty());
        assert_eq!(mobile.acceptances[0].created_entity, Some(EntityId(5)));
        let EntityLocation::MobileSubstrate(index) = world
            .entities()
            .location(EntityId(5))
            .copied()
            .expect("Mobile fixture entity is live")
        else {
            panic!("Mobile fixture identity maps to its substrate slot");
        };
        assert_eq!(
            runtime.signal.mobile_ports(MobileId(EntityId(5))),
            Some(crate::MobileControlPorts {
                stop: SinkId(EntityId(1)),
                left: SinkId(EntityId(2)),
                right: SinkId(EntityId(3)),
            })
        );
        (world, runtime, index)
    }

    fn certificate_bytes(certificates: &PathCertificateArena) -> Vec<u8> {
        let mut bytes = Vec::new();
        encode_path_certificates(certificates, &mut |part| bytes.extend_from_slice(part));
        bytes
    }

    fn path_certificate_state_hash(certificates: &PathCertificateArena) -> StateHash {
        let contract = contract();
        let structural = StructuralWorld::new();
        let signal = SignalWorld::new();
        let payloads = EventPayloadAllocator::new();
        let driver_events = EventCalendar::new();
        let signal_events = EventCalendar::new();
        let power_sources = PowerSourceStore::default();
        state_hash(StateView {
            contract: &contract,
            next_tick: Tick(0),
            topology_revision: Revision(0),
            main_core: None,
            power_sources: &power_sources,
            structural: &structural,
            signal: &signal,
            event_payloads: &payloads,
            driver_events: &driver_events,
            signal_events: &signal_events,
            path_certificates: certificates,
        })
    }

    fn append_empty_signal_and_events(output: &mut Vec<u8>) {
        for value in [1_u64, 0, 1, 0, 0, 0, 0, 0, 1, 0, 0] {
            output.extend_from_slice(&value.to_le_bytes());
        }
        for _ in 0..RESERVED_EMPTY_STORE_COUNT {
            output.extend_from_slice(&0_u64.to_le_bytes());
        }
        output.extend_from_slice(&1_u64.to_le_bytes());
        output.extend_from_slice(&0_u64.to_le_bytes());
    }
}
