use aon_app::cell_buffer::{CellPoint, CellTone, PresentationSource};
use aon_app::presenter::{PickTarget, PresenterError, ViewMode, Viewport, project_snapshot};
use aon_sim::{
    ArtifactBytes, Command, CommandEnvelope, EndpointTarget, EntityId, Fixed, FixedAabb, FixedVec2,
    GateId, GatePort, GatePortRef, GateType, JunctionId, PhysicalScaleProfile,
    PlaceFixedSubstrateCommand, PlaceGateCommand, PlaceJunctionCommand, PlaceWireCommand,
    RenderSnapshot, RoutingDomain, Simulation, SimulationPackage, Tick, WireEnd, WireId,
    decode_package,
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

fn envelope(tick: u64, ordinal: u64, command: Command) -> CommandEnvelope {
    CommandEnvelope {
        target_tick: Tick(tick),
        ordinal,
        command,
    }
}

fn step(simulation: &mut Simulation, commands: Vec<CommandEnvelope>) {
    let report = simulation.step(&commands).expect("fixture Tick succeeds");
    assert!(
        report.command_rejections.is_empty(),
        "fixture commands must be accepted: {:?}",
        report.command_rejections
    );
}

fn snapshot(simulation: &Simulation) -> RenderSnapshot {
    let mut snapshot = RenderSnapshot::default();
    simulation.write_render_snapshot(&mut snapshot);
    snapshot
}

fn crossing_fixture(reverse: bool) -> (PhysicalScaleProfile, RenderSnapshot) {
    let package = package();
    let physical = package.profiles().physical_scale().clone();
    let mut simulation = Simulation::new(package).expect("simulation starts");
    let oriented = |mut points: Vec<FixedVec2>| {
        if reverse {
            points.reverse();
        }
        points
    };
    step(
        &mut simulation,
        vec![
            envelope(
                0,
                0,
                Command::PlaceWire(PlaceWireCommand {
                    routing_domain: RoutingDomain::OpenWorld,
                    points: oriented(vec![point(-2 * WORLD_PITCH, 0), point(2 * WORLD_PITCH, 0)]),
                    endpoint_a: EndpointTarget::Free,
                    endpoint_b: EndpointTarget::Free,
                }),
            ),
            envelope(
                0,
                1,
                Command::PlaceWire(PlaceWireCommand {
                    routing_domain: RoutingDomain::OpenWorld,
                    points: oriented(vec![point(0, -2 * WORLD_PITCH), point(0, 2 * WORLD_PITCH)]),
                    endpoint_a: EndpointTarget::Free,
                    endpoint_b: EndpointTarget::Free,
                }),
            ),
        ],
    );
    (physical, snapshot(&simulation))
}

fn single_wire_fixture(points: Vec<FixedVec2>) -> (PhysicalScaleProfile, RenderSnapshot) {
    let package = package();
    let physical = package.profiles().physical_scale().clone();
    let mut simulation = Simulation::new(package).expect("simulation starts");
    step(
        &mut simulation,
        vec![envelope(
            0,
            0,
            Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::OpenWorld,
                points,
                endpoint_a: EndpointTarget::Free,
                endpoint_b: EndpointTarget::Free,
            }),
        )],
    );
    (physical, snapshot(&simulation))
}

#[test]
fn network_crossing_is_clipped_oriented_and_reversal_invariant() {
    let viewport = Viewport::new(CellPoint::new(-1, -1), 3, 3);
    let (physical, forward_snapshot) = crossing_fixture(false);
    let forward = project_snapshot(&forward_snapshot, &physical, ViewMode::Network, viewport)
        .expect("forward projection succeeds");
    let (_, reversed_snapshot) = crossing_fixture(true);
    let reversed = project_snapshot(&reversed_snapshot, &physical, ViewMode::Network, viewport)
        .expect("reversed projection succeeds");

    assert_eq!(forward.buffer().to_text(), "·│·\n─╳─\n·│·");
    assert_eq!(forward.buffer(), reversed.buffer());
    assert_eq!(forward.diagnostics(), &[]);
    assert_eq!(
        forward.pick_targets(CellPoint::new(0, 0)),
        &[
            PickTarget::Entity(EntityId(2)),
            PickTarget::Entity(EntityId(1)),
        ]
    );
    assert_eq!(
        forward.buffer().pick(CellPoint::new(0, 0)),
        Some(PresentationSource::Canonical(EntityId(2)))
    );
}

#[test]
fn polyline_turn_and_exact_corner_supercover_have_stable_glyphs() {
    let (physical, turn_snapshot) = single_wire_fixture(vec![
        point(-WORLD_PITCH, 0),
        point(0, 0),
        point(0, WORLD_PITCH),
    ]);
    let turn = project_snapshot(
        &turn_snapshot,
        &physical,
        ViewMode::Network,
        Viewport::new(CellPoint::new(-1, 0), 2, 2),
    )
    .expect("turn projects");
    assert_eq!(turn.buffer().to_text(), "·│\n─┘");

    let diagonal_points = vec![
        point(-2 * WORLD_PITCH, -2 * WORLD_PITCH),
        point(2 * WORLD_PITCH, 2 * WORLD_PITCH),
    ];
    let (_, diagonal_snapshot) = single_wire_fixture(diagonal_points.clone());
    let diagonal = project_snapshot(
        &diagonal_snapshot,
        &physical,
        ViewMode::Network,
        Viewport::new(CellPoint::new(-1, -1), 3, 3),
    )
    .expect("diagonal projects");
    let (_, reverse_snapshot) = single_wire_fixture(diagonal_points.into_iter().rev().collect());
    let reverse = project_snapshot(
        &reverse_snapshot,
        &physical,
        ViewMode::Network,
        Viewport::new(CellPoint::new(-1, -1), 3, 3),
    )
    .expect("reverse diagonal projects");
    assert_eq!(diagonal.buffer(), reverse.buffer());
    assert_eq!(diagonal.buffer().to_text(), "·┌┼\n┌┼┘\n┼┘·");
}

#[test]
fn huge_offscreen_span_is_clipped_without_dropping_visible_cells() {
    let package = package();
    let physical = package.profiles().physical_scale().clone();
    let mut simulation = Simulation::new(package).expect("simulation starts");
    let extent = 2_000_000 * WORLD_PITCH;
    step(
        &mut simulation,
        vec![envelope(
            0,
            0,
            Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::OpenWorld,
                points: vec![point(-extent, 0), point(extent, 0)],
                endpoint_a: EndpointTarget::Free,
                endpoint_b: EndpointTarget::Free,
            }),
        )],
    );

    let presented = project_snapshot(
        &snapshot(&simulation),
        &physical,
        ViewMode::Network,
        Viewport::new(CellPoint::new(-1, -1), 3, 3),
    )
    .expect("checked clipping succeeds");

    assert_eq!(presented.buffer().to_text(), "···\n───\n···");
    assert_eq!(presented.diagnostics(), &[]);
    assert_eq!(
        presented.primary_pick(CellPoint::new(0, 0)),
        Some(PickTarget::Entity(EntityId(1)))
    );
}

#[test]
fn negative_sub_pitch_endpoint_uses_mathematical_floor_division() {
    let package = package();
    let physical = package.profiles().physical_scale().clone();
    let quantum = physical.wire_geometry_quantum.0;
    let mut simulation = Simulation::new(package).expect("simulation starts");
    step(
        &mut simulation,
        vec![envelope(
            0,
            0,
            Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::OpenWorld,
                points: vec![point(-quantum, 0), point(quantum, 0)],
                endpoint_a: EndpointTarget::Free,
                endpoint_b: EndpointTarget::Free,
            }),
        )],
    );

    let presented = project_snapshot(
        &snapshot(&simulation),
        &physical,
        ViewMode::Network,
        Viewport::new(CellPoint::new(-1, 0), 2, 1),
    )
    .expect("negative coordinate projection succeeds");

    assert_eq!(presented.buffer().to_text(), "──");
    assert_eq!(
        presented.pick_targets(CellPoint::new(-1, 0)),
        &[
            PickTarget::WireEnd {
                wire: WireId(EntityId(1)),
                end: WireEnd::A,
            },
            PickTarget::Entity(EntityId(1)),
        ]
    );
}

#[test]
fn invalid_pitch_and_unrepresentable_viewport_fail_before_projection() {
    let mut physical = package().profiles().physical_scale().clone();
    physical.world_routing_pitch = Fixed::ZERO;
    assert_eq!(
        project_snapshot(
            &RenderSnapshot::default(),
            &physical,
            ViewMode::Network,
            Viewport::new(CellPoint::new(0, 0), 1, 1),
        ),
        Err(PresenterError::NonPositivePitch { pitch: Fixed::ZERO })
    );

    let physical = package().profiles().physical_scale().clone();
    assert_eq!(
        project_snapshot(
            &RenderSnapshot::default(),
            &physical,
            ViewMode::Network,
            Viewport::new(CellPoint::new(i32::MAX, 0), 2, 1),
        ),
        Err(PresenterError::ViewportCoordinateOverflow)
    );
}

fn fixed_circuit_fixture() -> (PhysicalScaleProfile, RenderSnapshot, EntityId) {
    let package = package();
    let physical = package.profiles().physical_scale().clone();
    let mut simulation = Simulation::new(package).expect("simulation starts");
    let substrate_origin = point(4 * WORLD_PITCH, -2 * WORLD_PITCH);
    let local_bounds = FixedAabb::new(
        point(-8 * CIRCUIT_PITCH, -4 * CIRCUIT_PITCH),
        point(8 * CIRCUIT_PITCH, 4 * CIRCUIT_PITCH),
    );
    step(
        &mut simulation,
        vec![envelope(
            0,
            0,
            Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
                origin: substrate_origin,
                routing_area: local_bounds,
                footprint: local_bounds,
            }),
        )],
    );
    let substrate = EntityId(1);
    let domain = RoutingDomain::FixedSubstrate(substrate);
    step(
        &mut simulation,
        vec![
            envelope(
                1,
                0,
                Command::PlaceGate(PlaceGateCommand {
                    gate_type: GateType::Not,
                    origin: substrate_origin,
                    routing_domain: domain,
                }),
            ),
            envelope(
                1,
                1,
                Command::PlaceGate(PlaceGateCommand {
                    gate_type: GateType::Not,
                    origin: point(
                        substrate_origin.x.0 + 4 * CIRCUIT_PITCH,
                        substrate_origin.y.0,
                    ),
                    routing_domain: domain,
                }),
            ),
        ],
    );
    step(
        &mut simulation,
        vec![envelope(
            2,
            0,
            Command::PlaceWire(PlaceWireCommand {
                routing_domain: domain,
                points: vec![
                    point(substrate_origin.x.0 + CIRCUIT_PITCH, substrate_origin.y.0),
                    point(
                        substrate_origin.x.0 + 3 * CIRCUIT_PITCH,
                        substrate_origin.y.0,
                    ),
                ],
                endpoint_a: EndpointTarget::GatePort(GatePortRef {
                    gate: GateId(EntityId(2)),
                    port: GatePort::Output,
                }),
                endpoint_b: EndpointTarget::GatePort(GatePortRef {
                    gate: GateId(EntityId(3)),
                    port: GatePort::InputA,
                }),
            }),
        )],
    );
    (physical, snapshot(&simulation), substrate)
}

#[test]
fn network_collapses_fixed_domain_while_circuit_exposes_local_pitch_and_ports() {
    let (physical, snapshot, substrate) = fixed_circuit_fixture();
    let network = project_snapshot(
        &snapshot,
        &physical,
        ViewMode::Network,
        Viewport::new(CellPoint::new(3, -3), 3, 3),
    )
    .expect("Network View projects");
    assert_eq!(network.buffer().to_text(), "···\n·■·\n···");
    assert!(!network.buffer().to_text().contains('!'));
    assert_eq!(
        network.primary_pick(CellPoint::new(4, -2)),
        Some(PickTarget::Entity(substrate))
    );

    let circuit = project_snapshot(
        &snapshot,
        &physical,
        ViewMode::Circuit { substrate },
        Viewport::new(CellPoint::new(-2, -1), 9, 3),
    )
    .expect("Circuit View projects");
    assert_eq!(
        circuit.buffer().to_text(),
        "·········\n·○!◉─◉!○·\n·········"
    );
    let output_port = GatePortRef {
        gate: GateId(EntityId(2)),
        port: GatePort::Output,
    };
    assert_eq!(
        circuit.pick_targets(CellPoint::new(1, 0)),
        &[
            PickTarget::GatePort(output_port),
            PickTarget::WireEnd {
                wire: WireId(EntityId(4)),
                end: WireEnd::A,
            },
            PickTarget::Entity(EntityId(4)),
        ]
    );
    assert_eq!(
        circuit.buffer().visual(CellPoint::new(1, 0)).unwrap().tone,
        CellTone::High
    );
}

#[test]
fn a_live_junction_overlays_crossing_wires_and_wins_picking() {
    let package = package();
    let physical = package.profiles().physical_scale().clone();
    let mut simulation = Simulation::new(package).expect("simulation starts");
    step(
        &mut simulation,
        vec![envelope(
            0,
            0,
            Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: RoutingDomain::OpenWorld,
                position: point(0, 0),
            }),
        )],
    );
    let junction = JunctionId(EntityId(1));
    let wire = |points: Vec<FixedVec2>, endpoint_a, endpoint_b| {
        Command::PlaceWire(PlaceWireCommand {
            routing_domain: RoutingDomain::OpenWorld,
            points,
            endpoint_a,
            endpoint_b,
        })
    };
    step(
        &mut simulation,
        vec![
            envelope(
                1,
                0,
                wire(
                    vec![point(-2 * WORLD_PITCH, 0), point(0, 0)],
                    EndpointTarget::Free,
                    EndpointTarget::Junction(junction),
                ),
            ),
            envelope(
                1,
                1,
                wire(
                    vec![point(0, 0), point(2 * WORLD_PITCH, 0)],
                    EndpointTarget::Junction(junction),
                    EndpointTarget::Free,
                ),
            ),
            envelope(
                1,
                2,
                wire(
                    vec![point(0, -2 * WORLD_PITCH), point(0, 0)],
                    EndpointTarget::Free,
                    EndpointTarget::Junction(junction),
                ),
            ),
            envelope(
                1,
                3,
                wire(
                    vec![point(0, 0), point(0, 2 * WORLD_PITCH)],
                    EndpointTarget::Junction(junction),
                    EndpointTarget::Free,
                ),
            ),
        ],
    );

    let presented = project_snapshot(
        &snapshot(&simulation),
        &physical,
        ViewMode::Network,
        Viewport::new(CellPoint::new(-1, -1), 3, 3),
    )
    .expect("junction projection succeeds");
    assert_eq!(presented.buffer().to_text(), "·│·\n─●─\n·│·");
    assert_eq!(
        presented.primary_pick(CellPoint::new(0, 0)),
        Some(PickTarget::Entity(EntityId(1)))
    );
    assert_eq!(
        &presented.pick_targets(CellPoint::new(0, 0))[..3],
        &[
            PickTarget::Entity(EntityId(1)),
            PickTarget::WireEnd {
                wire: WireId(EntityId(5)),
                end: WireEnd::A,
            },
            PickTarget::Entity(EntityId(5)),
        ]
    );
}
