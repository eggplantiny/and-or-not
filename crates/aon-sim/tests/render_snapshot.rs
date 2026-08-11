use aon_sim::{
    ArtifactBytes, Command, CommandEnvelope, ConnectionGeneration, EndpointTarget, EntityId, Fixed,
    FixedAabb, FixedVec2, GateId, GatePort, GatePortRef, GateType, InitialWorld, JunctionId,
    LogicLevel, PlaceFixedSubstrateCommand, PlaceGateCommand, PlaceJunctionCommand,
    PlaceWireCommand, RemoveEntityCommand, RenderSnapshot, RoutingDomain, SignalProbeTarget,
    SignalProbeValue, Simulation, SimulationPackage, Tick, WireId, decode_package,
};

const SCENARIO: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/scenarios/empty.json"
));
const NUMERIC: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/numeric/v1.json"
));
const PHYSICAL: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/physical-scale/stage0-alpha.json"
));
const BALANCE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/balance/stage0-alpha.json"
));

const CIRCUIT_PITCH: i64 = 16_384;
const WORLD_PITCH: i64 = 65_536;
const SUBSTRATE_HALF_EXTENT: i64 = 4 * WORLD_PITCH;

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

fn package() -> SimulationPackage {
    decode_package(ArtifactBytes {
        scenario: SCENARIO,
        numeric_profile: NUMERIC,
        physical_scale_profile: PHYSICAL,
        balance_profile: BALANCE,
    })
    .expect("the reference package is valid")
}

fn envelope(target_tick: u64, ordinal: u64, command: Command) -> CommandEnvelope {
    CommandEnvelope {
        target_tick: Tick(target_tick),
        ordinal,
        command,
    }
}

fn snapshot_fixture() -> Simulation {
    let mut simulation = Simulation::new(package()).expect("the reference simulation starts");
    let bounds = FixedAabb::new(
        point(-SUBSTRATE_HALF_EXTENT, -SUBSTRATE_HALF_EXTENT),
        point(SUBSTRATE_HALF_EXTENT, SUBSTRATE_HALF_EXTENT),
    );
    simulation
        .step(&[envelope(
            0,
            0,
            Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
                origin: point(0, 0),
                routing_area: bounds,
                footprint: bounds,
            }),
        )])
        .expect("the fixed Substrate is valid");
    let domain = RoutingDomain::FixedSubstrate(EntityId(1));
    let placements = simulation
        .step(&[
            envelope(
                1,
                30,
                Command::PlaceJunction(PlaceJunctionCommand {
                    routing_domain: domain,
                    position: point(0, 4 * CIRCUIT_PITCH),
                }),
            ),
            envelope(
                1,
                20,
                Command::PlaceGate(PlaceGateCommand {
                    gate_type: GateType::Not,
                    origin: point(4 * CIRCUIT_PITCH, 0),
                    routing_domain: domain,
                }),
            ),
            envelope(
                1,
                10,
                Command::PlaceGate(PlaceGateCommand {
                    gate_type: GateType::Not,
                    origin: point(0, 0),
                    routing_domain: domain,
                }),
            ),
        ])
        .expect("the Gate and Junction placements are valid");
    assert_eq!(
        placements
            .command_acceptances
            .iter()
            .map(|acceptance| acceptance.created_entity)
            .collect::<Vec<_>>(),
        vec![Some(EntityId(2)), Some(EntityId(3)), Some(EntityId(4))]
    );

    simulation
        .step(&[envelope(
            2,
            0,
            Command::PlaceWire(PlaceWireCommand {
                routing_domain: domain,
                points: vec![point(CIRCUIT_PITCH, 0), point(3 * CIRCUIT_PITCH, 0)],
                endpoint_a: EndpointTarget::GatePort(GatePortRef {
                    gate: GateId(EntityId(2)),
                    port: GatePort::Output,
                }),
                endpoint_b: EndpointTarget::GatePort(GatePortRef {
                    gate: GateId(EntityId(3)),
                    port: GatePort::InputA,
                }),
            }),
        )])
        .expect("the bound Wire is valid");

    for tick in 3..10 {
        simulation
            .step(&[])
            .unwrap_or_else(|error| panic!("empty Tick {tick} succeeds: {error}"));
    }
    simulation
}

#[test]
fn render_snapshot_is_a_sorted_exact_owned_projection() {
    let simulation = snapshot_fixture();
    let before = simulation.state_hash();
    let next_tick_before = simulation.next_tick();
    let mut snapshot = RenderSnapshot::default();

    simulation.write_render_snapshot(&mut snapshot);

    assert_eq!(simulation.state_hash(), before);
    assert_eq!(simulation.next_tick(), next_tick_before);
    assert_eq!(snapshot.state_hash(), before);
    assert_eq!(snapshot.scenario_id(), simulation.scenario_id());
    assert_eq!(snapshot.next_tick(), simulation.next_tick());
    assert_eq!(snapshot.topology_revision(), simulation.topology_revision());
    assert_eq!(snapshot.contract(), simulation.contract());
    assert_eq!(snapshot.primitive_count(), 5);
    assert_eq!(snapshot.fixed_substrates().len(), 1);
    assert_eq!(snapshot.gates().len(), 2);
    assert_eq!(snapshot.wires().len(), 1);
    assert_eq!(snapshot.junctions().len(), 1);
    assert_eq!(
        snapshot
            .fixed_substrates()
            .iter()
            .map(|record| record.id)
            .collect::<Vec<_>>(),
        vec![EntityId(1)]
    );
    assert_eq!(
        snapshot
            .gates()
            .iter()
            .map(|record| record.id.entity_id())
            .collect::<Vec<_>>(),
        vec![EntityId(2), EntityId(3)]
    );
    assert_eq!(
        snapshot
            .wires()
            .iter()
            .map(|record| record.id.entity_id())
            .collect::<Vec<_>>(),
        vec![EntityId(5)]
    );
    assert_eq!(
        snapshot
            .junctions()
            .iter()
            .map(|record| record.id.entity_id())
            .collect::<Vec<_>>(),
        vec![EntityId(4)]
    );
    let substrate = snapshot.fixed_substrates()[0];
    assert_eq!(substrate.id, EntityId(1));
    assert_eq!(substrate.origin, point(0, 0));
    assert_eq!(substrate.routing_area, substrate.footprint);

    let first_gate = snapshot.gates()[0];
    assert_eq!(first_gate.id, GateId(EntityId(2)));
    assert_eq!(first_gate.gate_type, GateType::Not);
    assert_eq!(first_gate.origin, point(0, 0));
    assert_eq!(
        first_gate.input_a_level,
        simulation
            .sink_level(first_gate.ports.input_a.sink)
            .expect("the rendered input Sink is live")
    );
    assert_eq!(first_gate.input_b_level, None);
    assert_eq!(
        first_gate.input_a_external_sample,
        simulation
            .driver_sample(first_gate.ports.input_a.external_driver)
            .expect("the rendered input A external Driver is live")
    );
    assert_eq!(first_gate.input_b_external_sample, None);
    assert_eq!(first_gate.input_a_external_sample.level, LogicLevel::Low);
    assert_eq!(first_gate.input_a_external_sample.strength.0, 0);
    assert_eq!(
        first_gate.output_sample,
        simulation
            .driver_sample(first_gate.ports.output)
            .expect("the rendered output Driver is live")
    );
    let first_signal = simulation
        .gate_signal_state(first_gate.id)
        .expect("the rendered Gate signal state is live");
    assert_eq!(first_gate.current_output, first_signal.current_output);
    assert_eq!(first_gate.desired_output, first_signal.desired_output);
    assert_eq!(
        first_gate.pending_generation,
        first_signal.pending_generation
    );
    assert_eq!(first_gate.pending_due_tick, first_signal.pending_due_tick);
    assert_eq!(first_gate.pending_level, first_signal.pending_level);
    assert_eq!(
        first_gate.pending_switch_energy,
        first_signal.pending_switch_energy
    );
    assert_eq!(
        first_gate.cancelled_switching_heat,
        first_signal.cancelled_switching_heat
    );
    assert_eq!(first_gate.current_output, LogicLevel::High);
    assert_eq!(first_gate.desired_output, LogicLevel::High);
    assert_eq!(first_gate.output_sample.level, LogicLevel::High);
    assert_eq!(first_gate.output_sample.strength.0, 400);
    assert_eq!(first_gate.pending_due_tick, None);
    assert_eq!(first_gate.pending_level, None);

    let second_gate = snapshot.gates()[1];
    assert_eq!(second_gate.input_a_level, LogicLevel::High);
    assert_eq!(second_gate.current_output, LogicLevel::Low);
    assert_eq!(second_gate.desired_output, LogicLevel::Low);

    let wire = &snapshot.wires()[0];
    assert_eq!(wire.id, WireId(EntityId(5)));
    assert_eq!(
        wire.points,
        vec![point(CIRCUIT_PITCH, 0), point(3 * CIRCUIT_PITCH, 0)]
    );
    assert_eq!(
        wire.endpoint_a,
        EndpointTarget::GatePort(GatePortRef {
            gate: GateId(EntityId(2)),
            port: GatePort::Output,
        })
    );
    assert_eq!(
        wire.endpoint_b,
        EndpointTarget::GatePort(GatePortRef {
            gate: GateId(EntityId(3)),
            port: GatePort::InputA,
        })
    );
    assert_eq!(wire.connection_generation, ConnectionGeneration::INITIAL);
    assert_eq!(wire.active_level, LogicLevel::High);
    assert_eq!(wire.active_drive.high, 400);
    assert_eq!(wire.active_drive.low, 0);
    assert_eq!(wire.active_drive.unknown, 0);
    assert_eq!(wire.previous_drive, wire.active_drive);
    assert_eq!(wire.previous_level, LogicLevel::High);

    let junction = snapshot.junctions()[0];
    assert_eq!(junction.id, JunctionId(EntityId(4)));
    assert_eq!(junction.position, point(0, 4 * CIRCUIT_PITCH));
    assert_eq!(
        junction.connection_generation,
        ConnectionGeneration::INITIAL
    );
}

#[test]
fn snapshot_and_signal_probe_reads_preserve_hash_and_snapshot_storage_is_reusable() {
    let mut simulation = snapshot_fixture();
    let mut snapshot = RenderSnapshot::default();
    let before = simulation.state_hash();
    let next_tick_before = simulation.next_tick();
    simulation.write_render_snapshot(&mut snapshot);
    let retained_snapshot = snapshot.clone();
    let gate = snapshot.gates()[0];
    let wire = snapshot.wires()[0].clone();

    let input = simulation
        .signal_probe(SignalProbeTarget::GateInputA(gate.id))
        .expect("a live Gate input can be probed");
    assert_eq!(input.next_tick, simulation.next_tick());
    assert_eq!(input.target, SignalProbeTarget::GateInputA(gate.id));
    assert_eq!(
        input.value,
        SignalProbeValue::Sink {
            sink: gate.ports.input_a.sink,
            level: gate.input_a_level,
        }
    );
    assert_eq!(
        simulation
            .signal_probe(SignalProbeTarget::Sink(gate.ports.input_a.sink))
            .expect("a raw live Sink can be probed")
            .value,
        input.value
    );

    let output = simulation
        .signal_probe(SignalProbeTarget::GateOutput(gate.id))
        .expect("a live Gate output can be probed");
    assert_eq!(output.value, SignalProbeValue::Driver(gate.output_sample));
    assert_eq!(
        simulation.signal_probe(SignalProbeTarget::Driver(gate.ports.output)),
        Some(aon_sim::SignalProbeSample {
            target: SignalProbeTarget::Driver(gate.ports.output),
            next_tick: simulation.next_tick(),
            value: SignalProbeValue::Driver(gate.output_sample),
        })
    );
    assert_eq!(
        simulation
            .signal_probe(SignalProbeTarget::Wire(wire.id))
            .expect("a live Wire can be probed")
            .value,
        SignalProbeValue::Wire {
            active_drive: wire.active_drive,
            previous_drive: wire.previous_drive,
            active_level: wire.active_level,
            previous_level: wire.previous_level,
        }
    );
    assert_eq!(
        simulation.signal_probe(SignalProbeTarget::GateInputB(gate.id)),
        None
    );
    assert_eq!(
        simulation.signal_probe(SignalProbeTarget::Wire(WireId(EntityId(999)))),
        None
    );
    assert_eq!(simulation.state_hash(), before);
    assert_eq!(simulation.next_tick(), next_tick_before);

    simulation
        .step(&[envelope(
            simulation.next_tick().0,
            0,
            Command::RemoveEntity(RemoveEntityCommand {
                target: wire.id.entity_id(),
            }),
        )])
        .expect("the observed Wire can be removed");
    let after_remove = simulation.state_hash();
    simulation.write_render_snapshot(&mut snapshot);

    assert_eq!(simulation.state_hash(), after_remove);
    assert_eq!(snapshot.state_hash(), after_remove);
    assert!(snapshot.wires().is_empty());
    assert_eq!(snapshot.primitive_count(), 4);
    assert_eq!(retained_snapshot.wires().len(), 1);
    assert_eq!(retained_snapshot.wires()[0].points, wire.points);
}

#[test]
fn empty_snapshot_contract_remains_compatible() {
    let package = package();
    let simulation = Simulation::new(SimulationPackage::new(
        "standalone-empty",
        InitialWorld::Empty,
        package.required_features(),
        *package.contract(),
        package.profiles().clone(),
    ))
    .expect("an empty package starts");
    let before = simulation.state_hash();
    let next_tick_before = simulation.next_tick();
    let mut snapshot = RenderSnapshot::default();

    simulation.write_render_snapshot(&mut snapshot);

    assert_eq!(snapshot.scenario_id(), "standalone-empty");
    assert_eq!(snapshot.next_tick(), Tick(0));
    assert_eq!(snapshot.primitive_count(), 0);
    assert_eq!(snapshot.state_hash(), before);
    assert_eq!(snapshot.contract(), simulation.contract());
    assert!(snapshot.fixed_substrates().is_empty());
    assert!(snapshot.gates().is_empty());
    assert!(snapshot.wires().is_empty());
    assert!(snapshot.junctions().is_empty());
    assert_eq!(simulation.state_hash(), before);
    assert_eq!(simulation.next_tick(), next_tick_before);
}
