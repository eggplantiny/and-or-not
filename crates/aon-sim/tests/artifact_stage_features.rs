use aon_sim::{
    ArtifactBytes, Simulation, SimulationError, StageFeatureSet, decode_package,
    decode_scenario_manifest,
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

fn scenario_requiring(feature: &str) -> Vec<u8> {
    let mut scenario: serde_json::Value =
        serde_json::from_slice(SCENARIO).expect("reference scenario JSON is valid");
    scenario["requiredFeatures"][feature] = true.into();
    serde_json::to_vec(&scenario).expect("test scenario serializes")
}

fn decode_test_package(scenario: &[u8]) -> aon_sim::SimulationPackage {
    decode_package(ArtifactBytes {
        scenario,
        numeric_profile: NUMERIC,
        physical_scale_profile: PHYSICAL,
        balance_profile: BALANCE,
    })
    .expect("test package decodes")
}

#[test]
fn signal_feature_is_decoded_and_supported_at_simulation_start() {
    let scenario = scenario_requiring("signal");
    let manifest = decode_scenario_manifest(&scenario).expect("signal scenario decodes");
    assert_eq!(
        manifest.required_features(),
        StageFeatureSet {
            signal: true,
            ..StageFeatureSet::none()
        }
    );

    Simulation::new(decode_test_package(&scenario))
        .expect("S0-M3 implements the signal feature boundary");
}

#[test]
fn mobility_feature_is_decoded_and_supported_at_simulation_start() {
    let scenario = scenario_requiring("mobility");
    let manifest = decode_scenario_manifest(&scenario).expect("mobility scenario decodes");
    assert_eq!(
        manifest.required_features(),
        StageFeatureSet {
            mobility: true,
            ..StageFeatureSet::none()
        }
    );

    Simulation::new(decode_test_package(&scenario))
        .expect("S0-M7 implements the mobility feature boundary");
}

#[test]
fn later_stage_features_remain_unsupported() {
    for feature in ["sensing", "power", "relay", "payload", "radiation"] {
        let scenario = scenario_requiring(feature);
        assert_eq!(
            Simulation::new(decode_test_package(&scenario)).err(),
            Some(SimulationError::UnsupportedStageFeature { feature })
        );
    }
}

#[test]
fn capacity_is_supported_only_by_a_main_core_initial_world() {
    let scenario = scenario_requiring("capacity");
    assert_eq!(
        Simulation::new(decode_test_package(&scenario)).err(),
        Some(SimulationError::CapacityRequiresMainCore)
    );
}

#[test]
fn stage_zero_features_do_not_mask_an_unsupported_feature() {
    let mut scenario: serde_json::Value =
        serde_json::from_slice(SCENARIO).expect("reference scenario JSON is valid");
    scenario["requiredFeatures"]["signal"] = true.into();
    scenario["requiredFeatures"]["mobility"] = true.into();
    scenario["requiredFeatures"]["sensing"] = true.into();
    let scenario = serde_json::to_vec(&scenario).expect("test scenario serializes");

    assert_eq!(
        Simulation::new(decode_test_package(&scenario)).err(),
        Some(SimulationError::UnsupportedStageFeature { feature: "sensing" })
    );
}
