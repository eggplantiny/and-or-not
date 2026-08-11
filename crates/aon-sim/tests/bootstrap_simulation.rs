use aon_sim::{ArtifactBytes, RenderSnapshot, Simulation, StateHash, decode_package};

const SCENARIO: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/scenarios/empty.json"
));
const NUMERIC: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/numeric/bootstrap-empty-v1.json"
));
const PHYSICAL: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/physical-scale/bootstrap-empty-v1.json"
));
const BALANCE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../profiles/balance/bootstrap-empty-v1.json"
));

fn package() -> aon_sim::SimulationPackage {
    decode_package(ArtifactBytes {
        scenario: SCENARIO,
        numeric_profile: NUMERIC,
        physical_scale_profile: PHYSICAL,
        balance_profile: BALANCE,
    })
    .expect("bootstrap fixtures are valid")
}

fn trace(ticks: u64, snapshots_per_tick: u32) -> Vec<StateHash> {
    let mut simulation = Simulation::new(package()).expect("simulation is valid");
    let mut trace = vec![simulation.state_hash()];
    let mut snapshot = RenderSnapshot::default();

    for _ in 0..snapshots_per_tick {
        simulation.write_render_snapshot(&mut snapshot);
    }
    for _ in 0..ticks {
        let report = simulation.step(&[]).expect("empty step succeeds");
        trace.push(report.state_hash);
        for _ in 0..snapshots_per_tick {
            simulation.write_render_snapshot(&mut snapshot);
        }
    }
    trace
}

#[test]
fn step_advances_exactly_one_tick_and_reports_post_step_hash() {
    let mut simulation = Simulation::new(package()).expect("simulation is valid");

    let report = simulation.step(&[]).expect("empty step succeeds");

    assert_eq!(report.completed_tick, 0);
    assert_eq!(report.next_tick, 1);
    assert_eq!(simulation.next_tick(), 1);
    assert_eq!(report.state_hash, simulation.state_hash());
}

#[test]
fn every_step_reports_contiguous_tick_ids() {
    let mut simulation = Simulation::new(package()).expect("simulation is valid");

    for completed_tick in 0..100 {
        let report = simulation.step(&[]).expect("empty step succeeds");

        assert_eq!(report.completed_tick, completed_tick);
        assert_eq!(report.next_tick, completed_tick + 1);
        assert_eq!(simulation.next_tick(), completed_tick + 1);
        assert_eq!(report.state_hash, simulation.state_hash());
    }

    assert_eq!(simulation.next_tick(), 100);
}

#[test]
fn independent_instances_produce_the_same_trace() {
    for ticks in [0, 1, 100] {
        assert_eq!(trace(ticks, 0), trace(ticks, 0));
        assert_eq!(trace(ticks, 0).len(), ticks as usize + 1);
    }
}

#[test]
fn snapshot_frequency_does_not_change_canonical_state() {
    let baseline = trace(100, 0);

    assert_eq!(baseline, trace(100, 1));
    assert_eq!(baseline, trace(100, 7));
}

#[test]
fn snapshot_is_an_empty_read_only_projection() {
    let simulation = Simulation::new(package()).expect("simulation is valid");
    let before = simulation.state_hash();
    let mut snapshot = RenderSnapshot::default();

    simulation.write_render_snapshot(&mut snapshot);

    assert_eq!(snapshot.scenario_id(), "empty");
    assert_eq!(snapshot.next_tick(), 0);
    assert_eq!(snapshot.primitive_count(), 0);
    assert_eq!(snapshot.state_hash(), before);
    assert_eq!(simulation.state_hash(), before);
}

#[test]
fn bootstrap_hash_has_a_golden_value() {
    let simulation = Simulation::new(package()).expect("simulation is valid");

    // Updated only when the explicitly versioned bootstrap encoder changes.
    assert_eq!(
        simulation.state_hash().to_string(),
        "cac904f8b2b462ba4ba76dc3a2db4293e39d626a9cd43b118f302919bdff2144"
    );
}
