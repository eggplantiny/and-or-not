use aon_app::cell_buffer::{CellPoint, CellTone, PresentationSource};
use aon_app::inspector::{
    InspectorHostState, InspectorInput, InspectorRate, InspectorSelection, InspectorTarget,
    inspector_panel,
};
use aon_app::presenter::{PickTarget, ViewMode, Viewport, project_snapshot};
use aon_app::probe::ProbeRack;
use aon_sim::{
    ArtifactBytes, Command, CommandEnvelope, EndpointTarget, EntityId, Fixed, FixedAabb, FixedVec2,
    Heading, MobileId, MobilePort, MobilePortRef, PlaceMobileSubstrateCommand, PlaceWireCommand,
    RenderSnapshot, RoutingDomain, Simulation, Tick, TrackPosition, WireId, decode_package,
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

const WORLD_PITCH: i64 = 65_536;
const CIRCUIT_PITCH: i64 = 16_384;

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

fn fixture_on_track(
    points: Vec<FixedVec2>,
    mobile_origin: FixedVec2,
    ticks_after_placement: usize,
) -> (aon_sim::PhysicalScaleProfile, Simulation, RenderSnapshot) {
    let package = decode_package(ArtifactBytes {
        scenario: SCENARIO,
        numeric_profile: NUMERIC,
        physical_scale_profile: PHYSICAL,
        balance_profile: BALANCE,
    })
    .expect("reference package");
    let physical = package.profiles().physical_scale().clone();
    let mut simulation = Simulation::new(package).expect("simulation");
    simulation
        .step(&[envelope(
            0,
            0,
            Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::OpenWorld,
                points,
                endpoint_a: EndpointTarget::Free,
                endpoint_b: EndpointTarget::Free,
            }),
        )])
        .expect("track");
    let bounds = FixedAabb::new(
        point(-4 * CIRCUIT_PITCH, -4 * CIRCUIT_PITCH),
        point(4 * CIRCUIT_PITCH, 4 * CIRCUIT_PITCH),
    );
    simulation
        .step(&[envelope(
            1,
            0,
            Command::PlaceMobileSubstrate(PlaceMobileSubstrateCommand {
                origin: mobile_origin,
                routing_area: bounds,
                footprint: bounds,
            }),
        )])
        .expect("mobile");
    for _ in 0..ticks_after_placement {
        simulation.step(&[]).expect("mobile advances on track");
    }
    let mut snapshot = RenderSnapshot::default();
    simulation.write_render_snapshot(&mut snapshot);
    (physical, simulation, snapshot)
}

fn fixture() -> (aon_sim::PhysicalScaleProfile, Simulation, RenderSnapshot) {
    fixture_on_track(
        vec![point(0, 0), point(4 * WORLD_PITCH, 0)],
        point(WORLD_PITCH, 0),
        0,
    )
}

fn network_glyph(
    physical: &aon_sim::PhysicalScaleProfile,
    snapshot: &RenderSnapshot,
    at: CellPoint,
    viewport: Viewport,
) -> char {
    project_snapshot(snapshot, physical, ViewMode::Network, viewport)
        .expect("network projection")
        .buffer()
        .visual(at)
        .expect("mobile cell")
        .glyph
}

#[test]
fn network_view_draws_and_picks_the_mobile_direction_without_mutating_snapshot() {
    let (physical, _, snapshot) = fixture();
    let before = snapshot.clone();
    let presentation = project_snapshot(
        &snapshot,
        &physical,
        ViewMode::Network,
        Viewport::new(CellPoint::new(-1, -1), 8, 3),
    )
    .expect("network projection");
    let at = CellPoint::new(2, 0);
    let visual = presentation.buffer().visual(at).expect("mobile cell");
    assert_eq!(visual.glyph, '>');
    assert_eq!(visual.tone, CellTone::Neutral);
    assert_eq!(
        visual.source,
        Some(PresentationSource::Canonical(EntityId(2)))
    );
    assert_eq!(
        presentation.primary_pick(at),
        Some(PickTarget::Entity(EntityId(2)))
    );
    assert_eq!(snapshot, before);
}

#[test]
fn non_axis_aligned_diagonal_uses_canonical_offset_for_forward_and_reverse_arrows() {
    let points = vec![point(0, 0), point(3 * WORLD_PITCH, 4 * WORLD_PITCH)];
    let viewport = Viewport::new(CellPoint::new(-1, -1), 6, 7);

    let (physical, _, forward) = fixture_on_track(points.clone(), point(0, 0), 0);
    assert_eq!(
        forward.mobiles()[0].track_position,
        TrackPosition::Edge {
            edge: WireId(EntityId(1)),
            offset: Fixed(WORLD_PITCH),
            heading: Heading::Forward,
        }
    );
    let rounded = forward.mobiles()[0].world_position;
    let dx = i128::from(points[1].x.0 - points[0].x.0);
    let dy = i128::from(points[1].y.0 - points[0].y.0);
    let px = i128::from(rounded.x.0 - points[0].x.0);
    let py = i128::from(rounded.y.0 - points[0].y.0);
    assert_ne!(dx * py, dy * px, "rounded projection is not collinear");
    assert_eq!(
        network_glyph(&physical, &forward, CellPoint::new(0, 0), viewport),
        '^'
    );

    let (physical, _, reverse) =
        fixture_on_track(points, point(3 * WORLD_PITCH, 4 * WORLD_PITCH), 0);
    assert_eq!(
        reverse.mobiles()[0].track_position,
        TrackPosition::Edge {
            edge: WireId(EntityId(1)),
            offset: Fixed(4 * WORLD_PITCH),
            heading: Heading::Reverse,
        }
    );
    assert_eq!(
        network_glyph(&physical, &reverse, CellPoint::new(2, 3), viewport),
        'v'
    );
}

#[test]
fn vertex_boundary_uses_next_segment_forward_and_previous_segment_reverse() {
    let points = vec![
        point(0, 0),
        point(2 * WORLD_PITCH, 0),
        point(2 * WORLD_PITCH, 2 * WORLD_PITCH),
    ];
    let viewport = Viewport::new(CellPoint::new(-1, -1), 5, 5);
    let vertex = TrackPosition::Edge {
        edge: WireId(EntityId(1)),
        offset: Fixed(2 * WORLD_PITCH),
        heading: Heading::Forward,
    };

    let (physical, _, forward) = fixture_on_track(points.clone(), point(0, 0), 1);
    assert_eq!(forward.mobiles()[0].track_position, vertex);
    assert_eq!(
        network_glyph(&physical, &forward, CellPoint::new(2, 0), viewport),
        '^'
    );

    let (physical, _, reverse) =
        fixture_on_track(points, point(2 * WORLD_PITCH, 2 * WORLD_PITCH), 1);
    assert_eq!(
        reverse.mobiles()[0].track_position,
        TrackPosition::Edge {
            edge: WireId(EntityId(1)),
            offset: Fixed(2 * WORLD_PITCH),
            heading: Heading::Reverse,
        }
    );
    assert_eq!(
        network_glyph(&physical, &reverse, CellPoint::new(2, 0), viewport),
        '<'
    );
}

#[test]
fn mobile_circuit_view_exposes_three_bindable_intrinsic_ports() {
    let (physical, _, snapshot) = fixture();
    assert_eq!(snapshot.mobiles()[0].build, None);
    assert_eq!(snapshot.mobiles()[0].ports.build, None);
    assert_eq!(snapshot.mobiles()[0].damage_state, None);
    let presentation = project_snapshot(
        &snapshot,
        &physical,
        ViewMode::Circuit {
            substrate: EntityId(2),
        },
        Viewport::new(CellPoint::new(-5, -5), 11, 11),
    )
    .expect("mobile circuit projection");
    let mobile = MobileId(EntityId(2));
    for (point, port, glyph) in [
        (CellPoint::new(-4, -4), MobilePort::Stop, 'S'),
        (CellPoint::new(4, -4), MobilePort::Left, 'L'),
        (CellPoint::new(4, 4), MobilePort::Right, 'R'),
    ] {
        assert_eq!(
            presentation.buffer().visual(point).map(|cell| cell.glyph),
            Some(glyph)
        );
        assert_eq!(
            presentation.primary_pick(point),
            Some(PickTarget::MobilePort(MobilePortRef { mobile, port }))
        );
    }
}

#[test]
fn mobile_port_probe_and_inspector_are_read_only_snapshot_projections() {
    let (_, simulation, snapshot) = fixture();
    let before_hash = simulation.state_hash();
    let before_tick = simulation.next_tick();
    let before_snapshot = snapshot.clone();
    let mobile = snapshot.mobiles()[0];
    let stop = MobilePortRef {
        mobile: mobile.id,
        port: MobilePort::Stop,
    };

    let mut probes = ProbeRack::default();
    let probe = probes
        .add_validated(
            &simulation,
            aon_sim::SignalProbeTarget::Sink(mobile.ports.stop),
        )
        .expect("intrinsic STOP sink is probeable");
    assert_eq!(
        probes.trace(probe).expect("probe trace").target(),
        aon_sim::SignalProbeTarget::Sink(mobile.ports.stop)
    );

    let inspector = inspector_panel(InspectorInput {
        snapshot: &snapshot,
        retained_reports: &[],
        host_state: InspectorHostState::Paused,
        rate: InspectorRate::One,
        selection: Some(InspectorSelection {
            target: InspectorTarget::MobilePort(stop),
            latest_command: None,
        }),
        selected_arrival: None,
    })
    .to_text();
    assert!(inspector.contains("selection=mobile-port:2:Stop"));
    assert!(inspector.contains("mobile.id=2"));
    assert!(inspector.contains("mobile.stop=LOW"));
    assert!(inspector.contains("mobile.left=LOW"));
    assert!(inspector.contains("mobile.right=LOW"));

    assert_eq!(simulation.state_hash(), before_hash);
    assert_eq!(simulation.next_tick(), before_tick);
    assert_eq!(snapshot, before_snapshot);
}
