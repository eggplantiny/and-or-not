use aon_app::cell_buffer::{CellPoint, CellTone, PresentationSource};
use aon_app::inspector::{
    InspectorHostState, InspectorInput, InspectorRate, InspectorSelection, InspectorTarget,
    inspector_lines,
};
use aon_app::presenter::{PickTarget, ViewMode, Viewport, project_snapshot};
use aon_sim::{
    ArtifactBytes, Command, CommandEnvelope, EndpointTarget, EntityId, Fixed, FixedVec2,
    MainCoreId, PlaceWireCommand, RenderSnapshot, RoutingDomain, Simulation, Tick, decode_package,
};

const SCENARIO: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/scenarios/s1-m1-capacity-accounting-v1.json"
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
    "/../../profiles/balance/capacity-probe-alpha.json"
));

const WORLD_PITCH: i64 = 65_536;

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

#[test]
fn network_view_and_inspector_project_the_main_core_without_mutating_the_snapshot() {
    let package = decode_package(ArtifactBytes {
        scenario: SCENARIO,
        numeric_profile: NUMERIC,
        physical_scale_profile: PHYSICAL,
        balance_profile: BALANCE,
    })
    .expect("the S1-M1 package decodes");
    let physical = package.profiles().physical_scale().clone();
    let mut simulation = Simulation::new(package).expect("the Main Core world starts");
    let report = simulation
        .step(&[CommandEnvelope {
            target_tick: Tick(0),
            ordinal: 0,
            command: Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::OpenWorld,
                points: vec![point(0, 0), point(WORLD_PITCH, 0)],
                endpoint_a: EndpointTarget::MainCoreAnchor(MainCoreId(EntityId(1))),
                endpoint_b: EndpointTarget::Free,
            }),
        }])
        .expect("a Wire binds to the Main Core anchor");

    let mut snapshot = RenderSnapshot::default();
    simulation.write_render_snapshot(&mut snapshot);
    assert_eq!(snapshot.primitive_count(), 2);
    let before = snapshot.clone();

    let presentation = project_snapshot(
        &snapshot,
        &physical,
        ViewMode::Network,
        Viewport::new(CellPoint::new(-1, -1), 4, 3),
    )
    .expect("the Main Core projects");
    let visual = presentation
        .buffer()
        .visual(CellPoint::new(0, 0))
        .expect("the Main Core is visible");
    assert_eq!(visual.glyph, '@');
    assert_eq!(visual.tone, CellTone::Neutral);
    assert_eq!(
        visual.source,
        Some(PresentationSource::Canonical(EntityId(1)))
    );
    assert_eq!(
        presentation.primary_pick(CellPoint::new(0, 0)),
        Some(PickTarget::Entity(EntityId(1)))
    );

    let lines = inspector_lines(InspectorInput {
        snapshot: &snapshot,
        retained_reports: &[report],
        host_state: InspectorHostState::Paused,
        rate: InspectorRate::One,
        selection: Some(InspectorSelection {
            target: InspectorTarget::Entity(EntityId(1)),
            latest_command: None,
        }),
        selected_arrival: None,
    });
    for expected in [
        "main_core.id=1",
        "main_core.position=(0,0)",
        "main_core.capacity=65536000",
        "main_core.integrity=1000",
        "main_core.heat_energy=0",
    ] {
        assert!(
            lines.iter().any(|line| line == expected),
            "missing {expected}"
        );
    }

    let wire_lines = inspector_lines(InspectorInput {
        snapshot: &snapshot,
        retained_reports: &[],
        host_state: InspectorHostState::Paused,
        rate: InspectorRate::One,
        selection: Some(InspectorSelection {
            target: InspectorTarget::Entity(EntityId(2)),
            latest_command: None,
        }),
        selected_arrival: None,
    });
    assert!(
        wire_lines
            .iter()
            .any(|line| line == "wire.endpoint_a=main-core-anchor:1")
    );
    assert_eq!(snapshot, before);
}
