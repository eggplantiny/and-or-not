use aon_sim::{
    ArtifactBytes, Capacity, Command, CommandEnvelope, DemandId, DemandKind, EndpointTarget,
    Energy, EntityId, FIXED_ONE, Fixed, FixedVec2, HeatEnergy, JunctionId, PlaceJunctionCommand,
    PlaceWireCommand, PowerHeatKind, PowerRatio, PowerSourceId, RemoveEntityCommand, RoutingDomain,
    Simulation, SimulationPackage, WireId, decode_balance_profile, decode_numeric_profile,
    decode_package, decode_physical_scale_profile,
};
use serde_json::json;

const NUMERIC: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/numeric/v1.json"
));
const PHYSICAL: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/physical-scale/stage0-alpha.json"
));
const BALANCE_V3: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/balance/s1-m2-power-probe-alpha.json"
));
const BALANCE_V4: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/balance/s1-m3-capacity-support-alpha.json"
));

const CORE_HEAT: HeatEnergy = HeatEnergy(9);
const SOURCE: PowerSourceId = PowerSourceId(EntityId(2));
const C22_JUNCTION: JunctionId = JunctionId(EntityId(3));
const C22_WIRE_70: WireId = WireId(EntityId(4));
const C22_WIRE_50: WireId = WireId(EntityId(5));

const fn wu(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x * FIXED_ONE), Fixed(y * FIXED_ONE))
}

fn package(balance_bytes: &[u8], generation: u64, scenario_id: &str) -> SimulationPackage {
    let numeric = decode_numeric_profile(NUMERIC).expect("Numeric Profile decodes");
    let physical = decode_physical_scale_profile(PHYSICAL).expect("Physical Profile decodes");
    let balance = decode_balance_profile(balance_bytes).expect("Balance Profile decodes");
    let scenario = serde_json::to_vec(&json!({
        "schemaVersion": 3,
        "scenarioId": scenario_id,
        "semanticsVersion": "aon-semantics-v1",
        "hashAlgorithm": "blake3-v1",
        "initialWorld": {
            "kind": "main-core-power-v1",
            "mainCore": {
                "position": { "x": -16 * FIXED_ONE, "y": -16 * FIXED_ONE },
                "integrity": 1_000,
                "heatEnergy": CORE_HEAT.0
            },
            "powerSources": [{
                "position": { "x": 0, "y": 0 },
                "generationPerTick": generation
            }]
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
                "path": "profiles/numeric/v1.json",
                "profileId": numeric.profile_id,
                "profileHash": numeric.canonical_hash().expect("Numeric hashes").to_string()
            },
            "physicalScale": {
                "path": "profiles/physical-scale/stage0-alpha.json",
                "profileId": physical.profile_id,
                "profileHash": physical.canonical_hash().expect("Physical hashes").to_string()
            },
            "balance": {
                "path": "profiles/balance/test.json",
                "profileId": balance.profile_id,
                "profileHash": balance.canonical_hash().expect("Balance hashes").to_string()
            }
        }
    }))
    .expect("Scenario serializes");
    decode_package(ArtifactBytes {
        scenario: &scenario,
        numeric_profile: NUMERIC,
        physical_scale_profile: PHYSICAL,
        balance_profile: balance_bytes,
    })
    .expect("focused package decodes")
}

fn step(simulation: &mut Simulation, commands: Vec<Command>) -> aon_sim::StepReport {
    let target_tick = simulation.next_tick();
    let envelopes = commands
        .into_iter()
        .enumerate()
        .map(|(ordinal, command)| CommandEnvelope {
            target_tick,
            ordinal: u64::try_from(ordinal).expect("ordinal fits"),
            command,
        })
        .collect::<Vec<_>>();
    let report = simulation.step(&envelopes).expect("focused Tick succeeds");
    assert!(
        report.command_rejections.is_empty(),
        "focused commands are accepted: {:?}",
        report.command_rejections
    );
    assert_eq!(report.command_acceptances.len(), envelopes.len());
    report
}

fn wire(
    from: FixedVec2,
    to: FixedVec2,
    endpoint_a: EndpointTarget,
    endpoint_b: EndpointTarget,
) -> Command {
    Command::PlaceWire(PlaceWireCommand {
        routing_domain: RoutingDomain::OpenWorld,
        points: vec![from, to],
        endpoint_a,
        endpoint_b,
    })
}

fn build_c22(simulation: &mut Simulation) -> aon_sim::StepReport {
    let junction = step(
        simulation,
        vec![Command::PlaceJunction(PlaceJunctionCommand {
            routing_domain: RoutingDomain::OpenWorld,
            position: wu(70, 0),
        })],
    );
    assert_eq!(
        junction.command_acceptances[0].created_entity,
        Some(C22_JUNCTION.entity_id())
    );
    step(
        simulation,
        vec![
            wire(
                wu(0, 0),
                wu(70, 0),
                EndpointTarget::PowerSourceAnchor(SOURCE),
                EndpointTarget::Junction(C22_JUNCTION),
            ),
            wire(
                wu(70, 0),
                wu(120, 0),
                EndpointTarget::Junction(C22_JUNCTION),
                EndpointTarget::Free,
            ),
        ],
    )
}

#[test]
fn c22_flows_through_phase4_power_and_phase8_with_exact_values() {
    let mut simulation =
        Simulation::new(package(BALANCE_V4, 268, "s1-m3-c22-runtime")).expect("C22 starts");
    let report = build_c22(&mut simulation);

    let accounting = report
        .network_accounting
        .expect("capacity accounting is active");
    assert_eq!(accounting.used(), Capacity(120 * FIXED_ONE as u64));
    assert_eq!(accounting.supported(), Capacity(100 * FIXED_ONE as u64));
    assert_eq!(accounting.excess(), Some(Capacity(20 * FIXED_ONE as u64)));
    assert_eq!(accounting.total_support_demand(), Some(Energy(28)));

    let power = report.power.as_ref().expect("Power report exists");
    assert_eq!(power.regions.len(), 1);
    assert_eq!(power.regions[0].generation, Energy(268));
    assert_eq!(power.regions[0].total_nominal_demand, Energy(268));
    assert_eq!(power.regions[0].ratio, PowerRatio::ONE);
    for (wire, length, support) in [(C22_WIRE_70, 70, 17), (C22_WIRE_50, 50, 11)] {
        for kind in [DemandKind::WireLeakage, DemandKind::WireSensing] {
            let load = power
                .load(DemandId::new(wire.entity_id(), kind))
                .expect("intrinsic Wire load exists");
            assert_eq!(load.nominal, Energy(length));
            assert_eq!(load.granted, Energy(length));
        }
        let load = power
            .load(DemandId::new(
                wire.entity_id(),
                DemandKind::OvercapacitySupport,
            ))
            .expect("capacity-support load exists");
        assert_eq!(load.nominal, Energy(support));
        assert_eq!(load.granted, Energy(support));
    }
    let support_heat = power
        .heat_contributions
        .iter()
        .filter(|heat| heat.kind == PowerHeatKind::OvercapacitySupport)
        .map(|heat| (heat.owner, heat.energy))
        .collect::<Vec<_>>();
    assert_eq!(
        support_heat,
        vec![(C22_WIRE_70, HeatEnergy(4)), (C22_WIRE_50, HeatEnergy(3))]
    );
    assert_eq!(
        simulation
            .main_core_state()
            .expect("core remains")
            .heat_energy(),
        CORE_HEAT,
        "Phase-8 support heat is report-only in S1-M3"
    );

    let hash_before = simulation.state_hash();
    let network = simulation
        .network_analyzer_snapshot()
        .expect("network analyzer succeeds")
        .expect("network analyzer is active");
    assert_eq!(network.accounting(), accounting);
    assert_eq!(
        network
            .wires()
            .iter()
            .map(|wire| (wire.wire(), wire.support_demand()))
            .collect::<Vec<_>>(),
        vec![
            (C22_WIRE_70, Some(Energy(17))),
            (C22_WIRE_50, Some(Energy(11)))
        ]
    );
    let analyzed_power = simulation
        .power_sense_analyzer_snapshot()
        .expect("Power analyzer succeeds")
        .expect("Power analyzer is active");
    assert_eq!(analyzed_power.regions, power.regions);
    assert_eq!(analyzed_power.loads, power.loads);
    assert_eq!(
        simulation.state_hash(),
        hash_before,
        "analyzers are read-only"
    );
}

#[test]
fn v4_undercapacity_reports_explicit_zero_without_materializing_a_load() {
    let mut simulation =
        Simulation::new(package(BALANCE_V4, 100, "s1-m3-under-capacity")).expect("world starts");
    let report = step(
        &mut simulation,
        vec![wire(
            wu(0, 0),
            wu(50, 0),
            EndpointTarget::PowerSourceAnchor(SOURCE),
            EndpointTarget::Free,
        )],
    );
    let accounting = report.network_accounting.expect("accounting exists");
    assert_eq!(accounting.excess(), Some(Capacity(0)));
    assert_eq!(accounting.total_support_demand(), Some(Energy(0)));
    assert!(
        report
            .power
            .as_ref()
            .expect("Power exists")
            .loads
            .iter()
            .all(|load| load.demand.kind() != DemandKind::OvercapacitySupport)
    );
    let analyzer = simulation
        .network_analyzer_snapshot()
        .expect("Analyzer succeeds")
        .expect("Analyzer exists");
    assert_eq!(analyzer.wires()[0].support_demand(), Some(Energy(0)));
}

#[test]
fn source_less_overcapacity_loads_persist_but_receive_no_grant_or_heat() {
    let mut simulation =
        Simulation::new(package(BALANCE_V4, 268, "s1-m3-source-less")).expect("world starts");
    let report = step(
        &mut simulation,
        vec![wire(
            wu(0, 10),
            wu(120, 10),
            EndpointTarget::Free,
            EndpointTarget::Free,
        )],
    );
    assert_eq!(
        report
            .network_accounting
            .expect("accounting exists")
            .total_support_demand(),
        Some(Energy(28))
    );
    let power = report.power.expect("Power exists");
    let support = power
        .load(DemandId::new(EntityId(3), DemandKind::OvercapacitySupport))
        .expect("source-less support remains a nominal load");
    assert_eq!(support.nominal, Energy(28));
    assert_eq!(support.granted, Energy(0));
    assert_eq!(support.ratio, PowerRatio::ZERO);
    assert!(
        power
            .heat_contributions
            .iter()
            .all(|heat| heat.kind != PowerHeatKind::OvercapacitySupport)
    );
}

#[test]
fn abandoned_wire_raises_global_support_while_powered_region_uses_one_partial_ratio() {
    let mut simulation = Simulation::new(package(BALANCE_V4, 67, "s1-m3-mixed-partial-power"))
        .expect("world starts");
    let report = step(
        &mut simulation,
        vec![
            wire(
                wu(0, 0),
                wu(70, 0),
                EndpointTarget::PowerSourceAnchor(SOURCE),
                EndpointTarget::Free,
            ),
            wire(
                wu(0, 10),
                wu(50, 10),
                EndpointTarget::Free,
                EndpointTarget::Free,
            ),
        ],
    );
    assert_eq!(
        report.command_acceptances.len(),
        2,
        "overcapacity does not reject either Wire"
    );
    let accounting = report.network_accounting.expect("accounting exists");
    assert_eq!(accounting.used(), Capacity(120 * FIXED_ONE as u64));
    assert_eq!(accounting.excess(), Some(Capacity(20 * FIXED_ONE as u64)));
    assert_eq!(accounting.total_support_demand(), Some(Energy(28)));

    let power = report.power.expect("Power exists");
    let powered_support = power
        .load(DemandId::new(EntityId(3), DemandKind::OvercapacitySupport))
        .expect("powered Wire support exists");
    let powered_leakage = power
        .load(DemandId::new(EntityId(3), DemandKind::WireLeakage))
        .expect("powered Wire leakage exists");
    let powered_sensing = power
        .load(DemandId::new(EntityId(3), DemandKind::WireSensing))
        .expect("powered Wire sensing exists");
    let powered_region = power
        .region(powered_support.region)
        .expect("powered region exists");
    assert_eq!(powered_region.generation, Energy(67));
    assert_eq!(powered_region.total_nominal_demand, Energy(157));
    assert_eq!(powered_region.ratio.raw(), 28_554);
    assert_eq!(powered_support.ratio, powered_region.ratio);
    assert_eq!(powered_leakage.ratio, powered_region.ratio);
    assert_eq!(powered_sensing.ratio, powered_region.ratio);
    assert_eq!(powered_support.nominal, Energy(17));
    assert_eq!(powered_support.granted, Energy(7));
    assert_eq!(powered_leakage.granted, Energy(30));
    assert_eq!(powered_sensing.granted, Energy(30));

    let abandoned_support = power
        .load(DemandId::new(EntityId(4), DemandKind::OvercapacitySupport))
        .expect("abandoned Wire support remains a load");
    assert_eq!(abandoned_support.nominal, Energy(11));
    assert_eq!(abandoned_support.granted, Energy(0));
    assert_eq!(abandoned_support.ratio, PowerRatio::ZERO);

    let support_heat = power
        .heat_contributions
        .iter()
        .filter(|heat| heat.kind == PowerHeatKind::OvercapacitySupport)
        .collect::<Vec<_>>();
    assert_eq!(
        support_heat.len(),
        1,
        "unmet abandoned support creates no Heat"
    );
    assert_eq!(support_heat[0].owner, WireId(EntityId(3)));
    assert_eq!(
        support_heat[0].energy,
        HeatEnergy(2),
        "nearest-even rounds the actual 7-Energy grant by 1/4, not nominal support"
    );
    assert_eq!(
        simulation
            .main_core_state()
            .expect("core remains")
            .heat_energy(),
        CORE_HEAT,
        "support Heat remains report-only"
    );
}

#[test]
fn removal_recomputes_support_and_v3_remains_opted_out() {
    let mut v4 =
        Simulation::new(package(BALANCE_V4, 268, "s1-m3-removal")).expect("v4 world starts");
    let _ = build_c22(&mut v4);
    let removed = step(
        &mut v4,
        vec![Command::RemoveEntity(RemoveEntityCommand {
            target: C22_WIRE_70.entity_id(),
        })],
    );
    let accounting = removed.network_accounting.expect("accounting exists");
    assert_eq!(accounting.used(), Capacity(50 * FIXED_ONE as u64));
    assert_eq!(accounting.excess(), Some(Capacity(0)));
    assert_eq!(accounting.total_support_demand(), Some(Energy(0)));
    assert!(
        removed
            .power
            .expect("Power exists")
            .loads
            .iter()
            .all(|load| load.demand.kind() != DemandKind::OvercapacitySupport)
    );

    let mut v3 =
        Simulation::new(package(BALANCE_V3, 100, "s1-m3-v3-opt-out")).expect("v3 world starts");
    let v3_report = step(
        &mut v3,
        vec![wire(
            wu(0, 0),
            wu(1_200, 0),
            EndpointTarget::PowerSourceAnchor(SOURCE),
            EndpointTarget::Free,
        )],
    );
    let v3_accounting = v3_report.network_accounting.expect("v3 accounting exists");
    assert_eq!(
        v3_accounting.used(),
        Capacity(1_200 * FIXED_ONE as u64),
        "the retained v3 case really exceeds its 1,000-NCU Main Core capacity"
    );
    assert_eq!(v3_accounting.excess(), None);
    assert_eq!(v3_accounting.total_support_demand(), None);
    assert_eq!(
        v3.network_analyzer_snapshot()
            .expect("v3 Analyzer succeeds")
            .expect("v3 Analyzer exists")
            .wires()[0]
            .support_demand(),
        None
    );
    assert!(
        v3_report
            .power
            .expect("v3 Power exists")
            .loads
            .iter()
            .all(|load| load.demand.kind() != DemandKind::OvercapacitySupport)
    );
}
