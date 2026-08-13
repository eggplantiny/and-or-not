use aon_sim::{
    ArtifactBytes, BalanceProfile, Command, CommandEnvelope, DemandId, DemandKind, DriveStrength,
    EndpointTarget, Energy, EntityId, FIXED_ONE, Fixed, FixedVec2, GateId, HeatEnergy,
    InitialWorld, NumericProfile, PhysicalScaleProfile, PlaceWireCommand, PowerHeatKind,
    PowerRatio, PowerSourceId, ProfileBundle, Replay, RoutingDomain, Simulation,
    SimulationContract, SimulationPackage, StageFeatureSet, StepReport, Tick, WireEnd, WireId,
    decode_balance_profile, decode_package, decode_replay_artifact,
};
use serde_json::Value;

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
    "/../../profiles/balance/s1-m2-power-probe-alpha.json"
));
const C08_SCENARIO: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/scenarios/s1-m2-c08-brownout-full-v1.json"
));
const C08_REPLAY: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/replays/s1-m2-c08-brownout-full-v1.json"
));

const SOURCE: PowerSourceId = PowerSourceId(EntityId(2));
const LOSS_WIRE: WireId = WireId(EntityId(3));
const C08_MOBILE: aon_sim::MobileId = aon_sim::MobileId(EntityId(5));
const C08_GATE: GateId = GateId(EntityId(6));
const C08_SENSED_WIRE: WireId = WireId(EntityId(8));

fn package(scenario: &[u8], balance: &[u8]) -> SimulationPackage {
    decode_package(ArtifactBytes {
        scenario,
        numeric_profile: NUMERIC,
        physical_scale_profile: PHYSICAL,
        balance_profile: balance,
    })
    .expect("the S1-M2 package decodes")
}

fn nonzero_loss_package() -> SimulationPackage {
    let mut balance_json: Value =
        serde_json::from_slice(BALANCE).expect("the retained Balance Profile is JSON");
    balance_json["profileId"] = "balance-s1-m2-runtime-loss".into();
    balance_json["powerProbe"]["powerLossK"]["numerator"] = 1.into();
    let balance_bytes =
        serde_json::to_vec(&balance_json).expect("the custom Balance Profile encodes");
    let balance =
        decode_balance_profile(&balance_bytes).expect("the custom Balance Profile decodes");

    let mut scenario_json: Value =
        serde_json::from_slice(C08_SCENARIO).expect("the retained C08 Scenario is JSON");
    scenario_json["scenarioId"] = "s1-m2-runtime-loss".into();
    scenario_json["initialWorld"]["powerSources"][0]["generationPerTick"] = 12.into();
    scenario_json["profiles"]["balance"]["profileId"] = balance.profile_id.clone().into();
    scenario_json["profiles"]["balance"]["profileHash"] = balance
        .canonical_hash()
        .expect("the custom Balance Profile hashes")
        .to_string()
        .into();
    let scenario_bytes = serde_json::to_vec(&scenario_json).expect("the custom Scenario encodes");

    package(&scenario_bytes, &balance_bytes)
}

fn power(report: &StepReport) -> &aon_sim::PowerStepReport {
    report.power.as_ref().expect("Power is enabled")
}

fn assert_stable_order(report: &aon_sim::PowerStepReport) {
    assert!(report.regions.is_sorted_by_key(|row| row.region));
    assert!(report.loads.is_sorted_by_key(|row| row.demand));
    assert!(report.sense.is_sorted_by_key(|row| (row.wire, row.end)));
    assert!(report.gates.is_sorted_by_key(|row| row.gate));
    assert!(report.mobiles.is_sorted_by_key(|row| row.mobile));
    assert!(
        report
            .heat_contributions
            .is_sorted_by_key(|row| (row.owner, row.kind, row.demand))
    );
    for pair in report.sense.chunks_exact(2) {
        assert_eq!(pair[0].wire, pair[1].wire);
        assert_eq!([pair[0].end, pair[1].end], [WireEnd::A, WireEnd::B]);
    }
}

fn assert_reads_do_not_mutate(simulation: &Simulation, report: &StepReport) {
    let hash_before = simulation.state_hash();
    let power = power(report);
    assert_stable_order(power);
    for sense in &power.sense {
        assert_eq!(
            simulation.driver_sample(sense.current_driver.driver_id),
            Some(sense.current_driver)
        );
        assert!(simulation.wire_sense_state(sense.wire).is_some());
    }
    for gate in &power.gates {
        assert_eq!(
            simulation
                .gate_signal_state(gate.gate)
                .expect("reported Gate remains live")
                .unpowered_ticks,
            gate.unpowered_ticks
        );
    }
    let _ = simulation
        .network_analyzer_snapshot()
        .expect("the derived Network analyzer succeeds");
    let first_power_analyzer = simulation
        .power_sense_analyzer_snapshot()
        .expect("the derived Power/Sense analyzer succeeds")
        .expect("Power is enabled");
    let second_power_analyzer = simulation
        .power_sense_analyzer_snapshot()
        .expect("the repeated Power/Sense analyzer read succeeds")
        .expect("Power remains enabled");
    assert_eq!(first_power_analyzer, second_power_analyzer);
    assert_eq!(first_power_analyzer.next_tick, simulation.next_tick());
    if power.mobiles.is_empty() && power.gates.is_empty() {
        assert_eq!(first_power_analyzer.regions, power.regions);
        assert_eq!(first_power_analyzer.loads, power.loads);
        assert_eq!(first_power_analyzer.senses, power.sense);
        assert_eq!(first_power_analyzer.gates, power.gates);
    }
    assert_eq!(simulation.state_hash(), hash_before);
}

#[test]
fn actual_simulation_hands_nonzero_loss_and_leakage_to_phase8_without_thermal_state() {
    let mut simulation =
        Simulation::new(nonzero_loss_package()).expect("the nonzero-loss Simulation starts");
    let initial_heat = simulation
        .main_core_state()
        .expect("the Power world has a Main Core")
        .heat_energy();
    let command = CommandEnvelope {
        target_tick: Tick(0),
        ordinal: 0,
        command: Command::PlaceWire(PlaceWireCommand {
            routing_domain: RoutingDomain::OpenWorld,
            points: vec![
                FixedVec2::new(Fixed::ZERO, Fixed::ZERO),
                FixedVec2::new(Fixed(2 * FIXED_ONE), Fixed::ZERO),
            ],
            endpoint_a: EndpointTarget::PowerSourceAnchor(SOURCE),
            endpoint_b: EndpointTarget::Free,
        }),
    };
    let report = simulation
        .step(&[command])
        .expect("the powered Wire Tick succeeds");
    assert!(report.command_rejections.is_empty());
    assert_eq!(
        report.command_acceptances[0].created_entity,
        Some(EntityId(3))
    );

    let power = power(&report);
    assert_stable_order(power);
    assert_eq!(power.regions.len(), 1);
    assert_eq!(power.regions[0].generation, Energy(12));
    assert_eq!(power.regions[0].ratio, PowerRatio::ONE);
    assert_eq!(power.loads.len(), 2);
    for kind in [DemandKind::WireLeakage, DemandKind::WireSensing] {
        let load = power
            .load(DemandId::new(LOSS_WIRE.entity_id(), kind))
            .expect("both intrinsic Wire loads are reported");
        assert_eq!(load.nominal, Energy(2));
        assert_eq!(load.granted, Energy(2));
        assert_eq!(load.transmission_loss, Energy(4));
        assert_eq!(load.source_cost, Energy(6));
    }
    assert_eq!(power.sense.len(), 2);
    assert!(power.gates.is_empty());
    assert!(power.mobiles.is_empty());
    assert_eq!(
        power.heat_contributions,
        vec![
            aon_sim::PowerHeatReport {
                owner: LOSS_WIRE,
                kind: PowerHeatKind::LeakageDissipation,
                demand: DemandId::new(LOSS_WIRE.entity_id(), DemandKind::WireLeakage),
                energy: HeatEnergy(2),
            },
            aon_sim::PowerHeatReport {
                owner: LOSS_WIRE,
                kind: PowerHeatKind::TransmissionLoss,
                demand: DemandId::new(LOSS_WIRE.entity_id(), DemandKind::WireLeakage),
                energy: HeatEnergy(4),
            },
            aon_sim::PowerHeatReport {
                owner: LOSS_WIRE,
                kind: PowerHeatKind::TransmissionLoss,
                demand: DemandId::new(LOSS_WIRE.entity_id(), DemandKind::WireSensing),
                energy: HeatEnergy(4),
            },
        ]
    );
    assert_eq!(
        simulation
            .main_core_state()
            .expect("the Main Core remains live")
            .heat_energy(),
        initial_heat,
        "Phase-8 heat is derived report data, not accumulated thermal state"
    );
    assert_reads_do_not_mutate(&simulation, &report);
}

fn replay_tick(
    simulation: &mut Simulation,
    replay: &Replay,
) -> Result<StepReport, aon_sim::SimulationError> {
    let tick = simulation.next_tick();
    let commands = replay.commands_for_tick(tick).cloned().collect::<Vec<_>>();
    let world_inputs = replay
        .world_inputs_for_tick(tick)
        .cloned()
        .collect::<Vec<_>>();
    simulation.step_with_world_inputs(&commands, &world_inputs)
}

#[test]
fn c08_reports_are_deterministic_sorted_and_cover_sense_gate_and_mobile_rows() {
    let artifact = decode_replay_artifact(C08_REPLAY).expect("the retained C08 Replay decodes");
    let replay = artifact.replay();
    let mut first = Simulation::new(package(C08_SCENARIO, BALANCE)).expect("C08 starts");
    let mut second = Simulation::new(package(C08_SCENARIO, BALANCE)).expect("C08 starts twice");
    replay
        .validate_against(&first)
        .expect("the retained Replay matches C08");

    while first.next_tick() < replay.final_next_tick() {
        let completed_tick = first.next_tick();
        let first_report = replay_tick(&mut first, replay).expect("the first C08 Tick succeeds");
        let second_report = replay_tick(&mut second, replay).expect("the second C08 Tick succeeds");
        assert_eq!(first_report, second_report);
        assert_reads_do_not_mutate(&first, &first_report);

        if completed_tick == Tick(2) {
            let power = power(&first_report);
            assert_eq!(power.gates.len(), 1);
            assert_eq!(power.mobiles.len(), 1);

            let gate = power.gate(C08_GATE).expect("the C08 Gate is reported");
            assert_eq!(gate.ratio, PowerRatio::ONE);
            assert_eq!(gate.effective_delay, Tick(1));
            assert_eq!(gate.effective_drive, DriveStrength(400));
            assert_eq!(gate.unpowered_ticks, 0);

            let mobile = power
                .mobile(C08_MOBILE)
                .expect("the C08 Mobile is reported");
            assert_eq!(mobile.nominal_budget, Fixed(FIXED_ONE));
            assert_eq!(mobile.granted_budget, Fixed(FIXED_ONE));
            assert_eq!(mobile.ratio, Some(PowerRatio::ONE));
            assert_eq!(
                first_report.mobile_movements[0].granted_budget,
                mobile.granted_budget
            );

            for end in [WireEnd::A, WireEnd::B] {
                let sense = power
                    .sense(C08_SENSED_WIRE, end)
                    .expect("both C08 Sense ends are reported");
                assert!(!sense.sampled_presence);
                assert_eq!(sense.intended_level, aon_sim::LogicLevel::Low);
                assert_eq!(sense.intended_strength, DriveStrength(400));
            }
        }
    }
    assert_eq!(first.state_hash(), second.state_hash());
}

#[test]
fn reference_profile_inputs_used_by_the_fixture_are_canonical() {
    let profiles = ProfileBundle {
        numeric: NumericProfile::reference_v1("numeric-v1"),
        physical_scale: PhysicalScaleProfile::stage0_alpha("physical-scale-stage0-alpha"),
        balance: decode_balance_profile(BALANCE).expect("the retained Balance Profile decodes"),
    };
    profiles
        .validate()
        .expect("the reference profiles validate");
}

#[test]
fn feature_off_power_sense_analyzer_is_none_and_noninterfering() {
    let profiles = ProfileBundle {
        numeric: NumericProfile::reference_v1("runtime-report-off-numeric"),
        physical_scale: PhysicalScaleProfile::stage0_alpha("runtime-report-off-physical"),
        balance: BalanceProfile::stage0_alpha("runtime-report-off-balance"),
    };
    let contract = SimulationContract::from_profiles(&profiles).expect("profiles are valid");
    let simulation = Simulation::new(SimulationPackage::new(
        "runtime-report-off",
        InitialWorld::Empty,
        StageFeatureSet::none(),
        contract,
        profiles,
    ))
    .expect("the feature-off Simulation starts");
    let hash_before = simulation.state_hash();

    assert_eq!(simulation.power_sense_analyzer_snapshot(), Ok(None));
    assert_eq!(simulation.power_sense_analyzer_snapshot(), Ok(None));
    assert_eq!(simulation.state_hash(), hash_before);
}
