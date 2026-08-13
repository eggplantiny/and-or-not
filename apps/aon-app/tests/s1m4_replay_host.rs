use aon_app::run_replay_host_harness;
use aon_headless::{load_package, load_replay, run_replay_file};
use aon_sim::{
    Capacity, DemandId, DemandKind, DestructionKind, DestructionReport, Energy, EntityId,
    FIXED_ONE, HeatEnergy, InteractionHeatKind, RunEndCause, RunStatus, SignalArrivalKind,
    StateHash, Tick, WireId,
};
use std::path::PathBuf;

const INITIAL_HASH: &str = "51ace8554724d927c81c68716d15cc58a4115959076d031688ef85915e960111";
const CASES: [(&str, u64, &str); 5] = [
    (
        "construction-partial-multibuilder-v1.json",
        5,
        "31a1089b727c09776cd66796f116b1e0397286604bf2ef261ac38c0bec68efe1",
    ),
    (
        "construction-four-targets-v1.json",
        21,
        "0670210744f55ef99e67d4170546bcd88f8a90a9dbe506d54163601c93fee3de",
    ),
    (
        "c10-contact-v1.json",
        9,
        "5ba4d9a856a765cd59a98592b82b9de256a389ece2aecfe6cd34ef0e26c4b420",
    ),
    (
        "c09-wire-break-v1.json",
        52,
        "7452a1b72aa6622f8d894cd64866707ce6c7fdb3c2faf8efdcc4c6ee0c7a0bd4",
    ),
    (
        "terminal-v1.json",
        56,
        "fe1000209769a38c50440dd1bbcfe70d19d2cb09529343125590b06e4e129777",
    ),
];

fn replay(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/replays/s1-m4")
        .join(name)
}

fn scenario() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/scenarios/s1-m4-construction-contact-damage-v1.json")
}

fn hash(value: &str) -> StateHash {
    StateHash::from_hex(value).expect("retained State hashes are canonical lowercase hex")
}

#[test]
fn retained_s1m4_complete_reports_and_v7_hashes_match_headless_and_bevy() {
    let expected_scenario = scenario()
        .canonicalize()
        .expect("the retained Scenario exists");
    for (name, completed_ticks, final_hash) in CASES {
        let path = replay(name);
        let artifact = load_replay(&path).expect("the retained Replay decodes");
        let resolved_scenario = path
            .parent()
            .expect("Replay has a parent")
            .join(artifact.scenario_path());
        assert!(resolved_scenario.is_file());
        assert_eq!(
            resolved_scenario
                .canonicalize()
                .expect("the referenced Scenario canonicalizes"),
            expected_scenario
        );
        let package = load_package(resolved_scenario).expect("the retained Scenario package loads");
        let headless = run_replay_file(&path).expect("the retained Replay runs headlessly");
        assert_eq!(headless.completed_ticks(), completed_ticks);
        assert_eq!(headless.checkpoints()[0], hash(INITIAL_HASH));
        assert_eq!(headless.final_hash(), hash(final_hash));
        for (updates, presenter) in [(0, false), (5, true)] {
            let bevy = run_replay_host_harness(
                package.clone(),
                artifact.replay().clone(),
                updates,
                presenter,
            )
            .expect("the retained Replay runs through Bevy FixedUpdate");
            assert_eq!(bevy.checkpoints(), headless.checkpoints());
            assert_eq!(bevy.reports(), headless.reports());
        }

        if name == "construction-four-targets-v1.json" {
            assert_eq!(
                headless.reports()[13]
                    .network_accounting
                    .expect("Wire completion reports Capacity")
                    .used(),
                Capacity(u64::try_from(FIXED_ONE).expect("WU is positive"))
            );
            assert_eq!(
                headless.reports()[14]
                    .network_accounting
                    .expect("Wire activation reports Capacity")
                    .used(),
                Capacity(2 * u64::try_from(FIXED_ONE).expect("WU is positive"))
            );
        }
        if name == "c10-contact-v1.json" {
            let report = &headless.reports()[8];
            let demand = DemandId::new(EntityId(11), DemandKind::LiveWire);
            let live = report
                .power
                .as_ref()
                .expect("C-10 reports Power")
                .load(demand)
                .expect("C-10 reports its Live demand");
            assert_eq!((live.nominal, live.granted), (Energy(20), Energy(20)));
            assert_eq!(
                report
                    .contacts
                    .iter()
                    .map(|row| (row.wire, row.target.entity_id(), row.weight, row.absorbed))
                    .collect::<Vec<_>>(),
                [
                    (WireId(EntityId(11)), EntityId(5), 1, Energy(5)),
                    (WireId(EntityId(11)), EntityId(6), 1, Energy(5)),
                ]
            );
            assert!(report.interaction_heat.iter().any(|row| {
                row.owner == EntityId(11)
                    && row.kind == InteractionHeatKind::LiveWireRemainder
                    && row.demand == Some(demand)
                    && row.energy == HeatEnergy(10)
            }));
        }
        if name == "c09-wire-break-v1.json" {
            assert_eq!(
                headless.reports()[45]
                    .damage
                    .iter()
                    .find(|row| row.target == EntityId(11))
                    .map(|row| {
                        (
                            row.electrical_exposure,
                            row.integrity_after.0,
                            row.pending_destruction,
                        )
                    }),
                Some((Energy(10), 0, true))
            );
            assert_eq!(
                headless.reports()[46].destructions,
                [DestructionReport {
                    target: EntityId(11),
                    kind: DestructionKind::Damage,
                }]
            );
            assert_eq!(
                headless.reports()[45]
                    .network_accounting
                    .expect("pending Tick reports Capacity")
                    .used()
                    .0
                    - headless.reports()[46]
                        .network_accounting
                        .expect("removal Tick reports Capacity")
                        .used()
                        .0,
                u64::try_from(20 * FIXED_ONE).expect("20 WU is positive")
            );
            assert!(
                headless.reports()[46]
                    .power
                    .as_ref()
                    .expect("removal Tick reports Power")
                    .loads
                    .iter()
                    .all(|row| row.demand.owner() != EntityId(11))
            );
            let stale = &headless.reports()[51];
            assert_eq!(stale.signal_counters.invalid_path_arrivals, 1);
            assert_eq!(stale.signal_counters.signal_arrivals_applied, 0);
            assert!(stale.signal_arrivals.iter().any(|arrival| {
                arrival.due_tick == Tick(51) && arrival.kind == SignalArrivalKind::Propagation
            }));
        }
        if name == "terminal-v1.json" {
            assert_eq!(
                headless.reports()[55].run_status,
                RunStatus::Ended {
                    completed_tick: Tick(55),
                    cause: RunEndCause::MainCoreDestroyed,
                }
            );
        }
    }
}
