use aon_app::cell_buffer::{CellPoint, CellTone, PresentationSource};
use aon_app::inspector::{
    InspectorHostState, InspectorInput, InspectorRate, InspectorSelection, InspectorTarget,
    inspector_lines,
};
use aon_app::presenter::{PickTarget, ViewMode, Viewport, project_snapshot};
use aon_sim::{
    BalanceProfile, Command, CommandEnvelope, ConstructionTarget, EndpointTarget,
    EnemyInitialState, Fixed, FixedAabb, FixedVec2, HeatEnergy, InitialWorld, Integrity,
    LogicLevel, MobilePort, MobilePortRef, NumericProfile, PhysicalScaleProfile,
    PlaceConstructionSiteCommand, PlaceMobileSubstrateCommand, PlaceWireCommand, ProfileBundle,
    RenderSnapshot, RoutingDomain, RunStatus, Simulation, SimulationContract, SimulationPackage,
    StageFeatureSet, Tick,
};

const WORLD_PITCH: i64 = 65_536;
const CIRCUIT_PITCH: i64 = 16_384;

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

fn package() -> SimulationPackage {
    let profiles = ProfileBundle {
        numeric: NumericProfile::reference_v1("numeric-s1m4-presenter"),
        physical_scale: PhysicalScaleProfile::stage0_alpha("physical-s1m4-presenter"),
        balance: BalanceProfile::construction_contact_damage_alpha("balance-s1m4-presenter"),
    };
    let contract = SimulationContract::from_profiles(&profiles).expect("S1-M4 profiles validate");
    SimulationPackage::new(
        "s1m4-presenter",
        InitialWorld::MainCorePowerEnemyV1 {
            main_core_position: point(0, 0),
            main_core_integrity: Integrity(100),
            main_core_heat_energy: HeatEnergy(0),
            power_sources: vec![],
            enemies: vec![EnemyInitialState::new(
                point(-4 * WORLD_PITCH, 0),
                point(0, 0),
                Fixed(WORLD_PITCH),
                Integrity(10),
                HeatEnergy(3),
            )],
        },
        StageFeatureSet {
            signal: true,
            mobility: true,
            capacity: true,
            sensing: true,
            power: true,
            construction: true,
            contact: true,
            damage: true,
            ..StageFeatureSet::none()
        },
        contract,
        profiles,
    )
}

fn envelope(target_tick: u64, command: Command) -> CommandEnvelope {
    CommandEnvelope {
        target_tick: Tick(target_tick),
        ordinal: 0,
        command,
    }
}

fn fixture() -> (PhysicalScaleProfile, RenderSnapshot) {
    let package = package();
    let physical = package.profiles().physical_scale().clone();
    let mut simulation = Simulation::new(package).expect("S1-M4 simulation starts");
    simulation
        .step(&[envelope(
            0,
            Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::OpenWorld,
                points: vec![point(10 * WORLD_PITCH, 0), point(14 * WORLD_PITCH, 0)],
                endpoint_a: EndpointTarget::Free,
                endpoint_b: EndpointTarget::Free,
            }),
        )])
        .expect("track Wire is valid");
    let bounds = FixedAabb::new(
        point(-4 * CIRCUIT_PITCH, -4 * CIRCUIT_PITCH),
        point(4 * CIRCUIT_PITCH, 4 * CIRCUIT_PITCH),
    );
    simulation
        .step(&[envelope(
            1,
            Command::PlaceMobileSubstrate(PlaceMobileSubstrateCommand {
                origin: point(11 * WORLD_PITCH, 0),
                routing_area: bounds,
                footprint: bounds,
            }),
        )])
        .expect("Mobile is valid");
    let report = simulation
        .step(&[envelope(
            2,
            Command::PlaceConstructionSite(PlaceConstructionSiteCommand {
                target: ConstructionTarget::Junction {
                    routing_domain: RoutingDomain::OpenWorld,
                    position: point(6 * WORLD_PITCH, 0),
                },
            }),
        )])
        .expect("Construction Site is valid");
    assert!(report.command_rejections.is_empty());
    let mut snapshot = RenderSnapshot::default();
    simulation.write_render_snapshot(&mut snapshot);
    (physical, snapshot)
}

#[test]
fn network_view_draws_and_picks_enemy_and_site_from_canonical_positions() {
    let (physical, snapshot) = fixture();
    let before = snapshot.clone();
    let presentation = project_snapshot(
        &snapshot,
        &physical,
        ViewMode::Network,
        Viewport::new(CellPoint::new(-6, -1), 22, 3),
    )
    .expect("S1-M4 Network projection succeeds");

    let enemy = &snapshot.enemies()[0];
    let enemy_at = CellPoint::new(-4, 0);
    assert_eq!(
        presentation
            .buffer()
            .visual(enemy_at)
            .map(|cell| cell.glyph),
        Some('E')
    );
    assert_eq!(
        presentation
            .buffer()
            .visual(enemy_at)
            .map(|cell| cell.source),
        Some(Some(PresentationSource::Canonical(enemy.id.entity_id())))
    );
    assert_eq!(
        presentation.primary_pick(enemy_at),
        Some(PickTarget::Entity(enemy.id.entity_id()))
    );

    let site = &snapshot.construction_sites()[0];
    let site_at = CellPoint::new(6, 0);
    assert_eq!(
        presentation
            .buffer()
            .visual(site_at)
            .map(|cell| (cell.glyph, cell.tone)),
        Some(('j', CellTone::Ghost))
    );
    assert_eq!(
        presentation.primary_pick(site_at),
        Some(PickTarget::Entity(site.id.entity_id()))
    );
    assert_eq!(snapshot, before, "presentation is read-only");
}

#[test]
fn v5_mobile_build_uses_the_unused_corner_as_a_host_only_visual_anchor() {
    let (physical, snapshot) = fixture();
    let mobile = snapshot.mobiles()[0];
    assert_eq!(mobile.build, Some(LogicLevel::Low));
    let presentation = project_snapshot(
        &snapshot,
        &physical,
        ViewMode::Circuit {
            substrate: mobile.id.entity_id(),
        },
        Viewport::new(CellPoint::new(-5, -5), 11, 11),
    )
    .expect("Mobile Circuit projection succeeds");
    let build_at = CellPoint::new(-4, 4);
    assert_eq!(
        presentation
            .buffer()
            .visual(build_at)
            .map(|cell| cell.glyph),
        Some('B')
    );
    assert_eq!(
        presentation.primary_pick(build_at),
        Some(PickTarget::MobilePort(MobilePortRef {
            mobile: mobile.id,
            port: MobilePort::Build,
        }))
    );
}

#[test]
fn inspector_exposes_run_enemy_site_damage_and_build_state() {
    let (_, snapshot) = fixture();
    assert_eq!(snapshot.run_status(), RunStatus::Running);
    let enemy = snapshot.enemies()[0];
    let site = &snapshot.construction_sites()[0];
    let mobile = snapshot.mobiles()[0];

    let inspect = |target| {
        inspector_lines(InspectorInput {
            snapshot: &snapshot,
            retained_reports: &[],
            host_state: InspectorHostState::Paused,
            rate: InspectorRate::One,
            selection: Some(InspectorSelection {
                target,
                latest_command: None,
            }),
            selected_arrival: None,
        })
        .join("\n")
    };

    let enemy_text = inspect(InspectorTarget::Entity(enemy.id.entity_id()));
    assert!(enemy_text.contains("session.run_status=RUNNING"));
    assert!(enemy_text.contains("enemy.position=(-262144,0)"));
    assert!(enemy_text.contains("enemy.integrity=10"));
    assert!(enemy_text.contains("enemy.heat_energy=3"));

    let site_text = inspect(InspectorTarget::Entity(site.id.entity_id()));
    assert!(site_text.contains("construction_site.target=junction"));
    assert!(site_text.contains("construction_site.required_work=4"));
    assert!(site_text.contains("construction_site.completed_work=0"));
    assert!(site_text.contains("construction_site.activation_ready=false"));

    let mobile_text = inspect(InspectorTarget::MobilePort(MobilePortRef {
        mobile: mobile.id,
        port: MobilePort::Build,
    }));
    assert!(mobile_text.contains("mobile.build=LOW"));
    assert!(!mobile_text.contains("mobile.build_sink=-"));
    assert!(mobile_text.contains("mobile.integrity=20"));
    assert!(mobile_text.contains("mobile.heat_energy=0"));
}
