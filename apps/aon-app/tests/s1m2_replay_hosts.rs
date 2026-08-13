use aon_app::run_replay_host_harness;
use aon_headless::{load_package, load_replay, run_replay, run_replay_file};
use aon_sim::{
    BalanceProfile, Command, CommandEnvelope, EndpointTarget, Fixed, FixedVec2, HashCheckpoint,
    HostileCollider, LogicLevel, NumericProfile, PhysicalScaleProfile, PlaceWireCommand,
    ProfileBundle, Replay, ReplayFormatVersion, RoutingDomain, Simulation, SimulationContract,
    SimulationPackage, StateHash, StateHashVersion, StepReport, Tick, WorldInputEvent,
    decode_scenario_manifest,
};

const SCENARIO_V3: &[u8] = br#"{
  "schemaVersion": 3,
  "scenarioId": "s1-m2-replay-hosts",
  "semanticsVersion": "aon-semantics-v1",
  "hashAlgorithm": "blake3-v1",
  "initialWorld": {
    "kind": "main-core-power-v1",
    "mainCore": {
      "position": { "x": 0, "y": 0 },
      "integrity": 1000,
      "heatEnergy": 0
    },
    "powerSources": [
      {
        "position": { "x": 0, "y": 0 },
        "generationPerTick": 100
      }
    ]
  },
  "requiredFeatures": {
    "signal": true,
    "mobility": false,
    "capacity": true,
    "sensing": true,
    "power": true,
    "relay": false,
    "payload": false,
    "radiation": false
  },
  "profiles": {
    "numeric": {
      "path": "numeric.json",
      "profileId": "numeric-s1m2-host-test",
      "profileHash": "0000000000000000000000000000000000000000000000000000000000000000"
    },
    "physicalScale": {
      "path": "physical.json",
      "profileId": "physical-s1m2-host-test",
      "profileHash": "0000000000000000000000000000000000000000000000000000000000000000"
    },
    "balance": {
      "path": "balance.json",
      "profileId": "balance-s1m2-host-test",
      "profileHash": "0000000000000000000000000000000000000000000000000000000000000000"
    }
  }
}"#;

const FINAL_NEXT_TICK: u64 = 5;

fn package() -> SimulationPackage {
    let scenario =
        decode_scenario_manifest(SCENARIO_V3).expect("in-memory Scenario v3 is strictly valid");
    let profiles = ProfileBundle {
        numeric: NumericProfile::reference_v1("numeric-s1m2-host-test"),
        physical_scale: PhysicalScaleProfile::stage0_alpha("physical-s1m2-host-test"),
        balance: BalanceProfile::power_probe_alpha("balance-s1m2-host-test"),
    };
    let contract =
        SimulationContract::from_profiles(&profiles).expect("in-memory Profiles are valid");
    SimulationPackage::new(
        scenario.scenario_id(),
        scenario.initial_world().clone(),
        scenario.required_features(),
        contract,
        profiles,
    )
}

fn wire_command() -> CommandEnvelope {
    CommandEnvelope {
        target_tick: Tick(0),
        ordinal: 0,
        command: Command::PlaceWire(PlaceWireCommand {
            routing_domain: RoutingDomain::OpenWorld,
            points: vec![
                FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
                FixedVec2::new(Fixed(65_536), Fixed::ZERO),
            ],
            endpoint_a: EndpointTarget::Free,
            endpoint_b: EndpointTarget::Free,
        }),
    }
}

fn hostile_frame() -> WorldInputEvent {
    WorldInputEvent::HostileFrame {
        target_tick: Tick(1),
        hostiles: vec![HostileCollider {
            id: 7,
            center: FixedVec2::new(Fixed(32_768), Fixed::ZERO),
            radius: Fixed::ZERO,
        }],
    }
}

fn empty_frame() -> WorldInputEvent {
    WorldInputEvent::HostileFrame {
        target_tick: Tick(0),
        hostiles: Vec::new(),
    }
}

fn record_replay(
    package: &SimulationPackage,
    world_inputs: Vec<WorldInputEvent>,
) -> (Replay, Vec<StateHash>, Vec<StepReport>) {
    let mut simulation = Simulation::new(package.clone()).expect("S1-M2 Simulation bootstraps");
    let header = simulation.replay_header();
    let commands = vec![wire_command()];
    let mut hashes = vec![simulation.state_hash()];
    let mut reports = Vec::new();

    for raw_tick in 0..FINAL_NEXT_TICK {
        let tick = Tick(raw_tick);
        let command_batch = commands
            .iter()
            .filter(|command| command.target_tick == tick)
            .cloned()
            .collect::<Vec<_>>();
        let input_batch = world_inputs
            .iter()
            .filter(|input| input.target_tick() == tick)
            .cloned()
            .collect::<Vec<_>>();
        let report = simulation
            .step_with_world_inputs(&command_batch, &input_batch)
            .unwrap_or_else(|error| panic!("direct S1-M2 recording Tick {raw_tick}: {error:?}"));
        assert!(report.command_rejections.is_empty());
        hashes.push(report.state_hash);
        reports.push(report);
    }

    let checkpoints = hashes
        .iter()
        .copied()
        .enumerate()
        .map(|(next_tick, state_hash)| HashCheckpoint {
            next_tick: Tick(u64::try_from(next_tick).expect("fixture Tick fits u64")),
            state_hash,
        })
        .collect();
    let replay = Replay::new_v2(header, commands, world_inputs, checkpoints)
        .expect("recorded Replay v2 is valid");
    (replay, hashes, reports)
}

fn assert_trace(expected: &[StateHash], actual: &[StateHash]) {
    assert_eq!(actual.len(), expected.len());
    if let Some((next_tick, (expected, actual))) = expected
        .iter()
        .zip(actual)
        .enumerate()
        .find(|(_, (expected, actual))| expected != actual)
    {
        panic!("first V6 divergence at nextTick {next_tick}: {expected} != {actual}");
    }
}

#[test]
fn replay_v2_hostile_frames_match_direct_headless_and_bevy_and_empty_is_omittable() {
    let package = package();
    let (explicit, expected_hashes, expected_reports) =
        record_replay(&package, vec![hostile_frame(), empty_frame()]);
    assert_eq!(explicit.header().format_version, ReplayFormatVersion::V2);
    assert_eq!(explicit.header().state_hash_version, StateHashVersion::V6);
    assert_eq!(explicit.world_inputs().len(), 2);
    assert!(explicit.world_inputs()[0].hostiles().is_empty());

    let headless = run_replay(package.clone(), &explicit)
        .expect("Replay v2 executes through the Headless host");
    let bevy = run_replay_host_harness(package.clone(), explicit.clone(), 3, true)
        .expect("Replay v2 executes through the Bevy host");
    assert_trace(&expected_hashes, headless.checkpoints());
    assert_eq!(headless.reports(), expected_reports);
    assert_trace(&expected_hashes, bevy.checkpoints());
    assert_eq!(bevy.reports(), expected_reports);

    assert!(expected_reports.iter().any(|report| {
        report
            .driver_changes
            .iter()
            .any(|change| change.current.level == LogicLevel::High)
    }));
    assert!(expected_reports.iter().any(|report| {
        report
            .driver_changes
            .iter()
            .any(|change| change.current.level == LogicLevel::Low)
    }));

    let omitted = Replay::new_v2(
        *explicit.header(),
        explicit.commands().to_vec(),
        vec![hostile_frame()],
        explicit.checkpoints().to_vec(),
    )
    .expect("omitting an empty frame preserves Replay validity");
    let omitted_headless = run_replay(package.clone(), &omitted)
        .expect("omitted-empty Replay executes through Headless");
    let omitted_bevy = run_replay_host_harness(package, omitted, 0, false)
        .expect("omitted-empty Replay executes through Bevy");
    assert_trace(&expected_hashes, omitted_headless.checkpoints());
    assert_eq!(omitted_headless.reports(), expected_reports);
    assert_trace(&expected_hashes, omitted_bevy.checkpoints());
    assert_eq!(omitted_bevy.reports(), expected_reports);
}

fn retained_fixture_path(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/replays")
        .join(name)
}

#[test]
fn retained_c07_and_c08_reports_and_v6_hashes_match_headless_and_bevy() {
    for name in [
        "s1-m2-c07-sensing-v1.json",
        "s1-m2-c08-brownout-full-v1.json",
        "s1-m2-c08-brownout-half-v1.json",
    ] {
        let replay_path = retained_fixture_path(name);
        let artifact = load_replay(&replay_path).expect("retained Replay decodes");
        let scenario_path = replay_path
            .parent()
            .expect("Replay fixture has a parent")
            .join(artifact.scenario_path());
        let package = load_package(&scenario_path).expect("retained Scenario package loads");
        let headless = run_replay_file(&replay_path).expect("retained Replay runs headlessly");
        let bevy = run_replay_host_harness(package, artifact.replay().clone(), 3, true)
            .expect("retained Replay runs through Bevy FixedUpdate");

        assert_trace(headless.checkpoints(), bevy.checkpoints());
        assert_eq!(
            headless.reports(),
            bevy.reports(),
            "Headless and Bevy reports diverged for {name}"
        );
    }
}
