use aon_sim::{
    ArtifactBytes, HASH_ALGORITHM_ID_BLAKE3_V1, REPLAY_FORMAT_VERSION_V2, ReplayContractField,
    ReplayError, SEMANTICS_VERSION_V1, STATE_HASH_VERSION_V3, STATE_HASH_VERSION_V4,
    STATE_HASH_VERSION_V6, Seed, Simulation, StateHash, Tick, WORLD_GENERATOR_VERSION_EMPTY_V1,
    decode_package, decode_replay_artifact, encode_replay_artifact,
};

const FEEDBACK_RING: &[u8] = include_bytes!("../../../fixtures/replays/feedback-ring-v1.json");
const STAGE0_100K: &[u8] = include_bytes!("../../../fixtures/replays/stage0-100k-v1.json");
const S1_M1_CAPACITY: &[u8] =
    include_bytes!("../../../fixtures/replays/s1-m1-capacity-accounting-v1.json");
const RETAINED_V3_EMPTY: &[u8] = include_bytes!("fixtures/replay-v3-empty-v1.json");
const RETAINED_V4_EMPTY: &[u8] = include_bytes!("fixtures/replay-v4-empty-v1.json");
const SCENARIO: &[u8] = include_bytes!("../../../fixtures/scenarios/empty.json");
const NUMERIC: &[u8] = include_bytes!("../../../profiles/numeric/v1.json");
const PHYSICAL: &[u8] = include_bytes!("../../../profiles/physical-scale/stage0-alpha.json");
const BALANCE: &[u8] = include_bytes!("../../../profiles/balance/stage0-alpha.json");

const INITIAL_STATE_HASH: &str = "0010f831c5b32198d1f0f49f08a29629a5bfa9177504c2bf7271e6bc1a20fef1";
const NUMERIC_PROFILE_HASH: &str =
    "fe92f0c723660040a3200254890c8a34ec3ed9e65fc242de1c0951e4ecd00469";
const PHYSICAL_SCALE_PROFILE_HASH: &str =
    "0e0f7fe8c9ccbf0b159d44e4e53d05417cf558c37e796e5f8bccd8221aec6490";
const BALANCE_PROFILE_HASH: &str =
    "b1540d6ad19c616ce60e96523108264355311168c51a0b92de2fdf596e2646fd";

#[test]
fn retained_v3_empty_artifact_strictly_decodes_and_round_trips_exactly() {
    let artifact =
        decode_replay_artifact(RETAINED_V3_EMPTY).expect("retained V3 Replay strictly decodes");

    assert_eq!(
        artifact.replay().header().state_hash_version.as_str(),
        STATE_HASH_VERSION_V3
    );
    assert!(artifact.replay().commands().is_empty());
    assert_eq!(artifact.replay().checkpoints().len(), 1);
    assert_eq!(
        encode_replay_artifact(&artifact).expect("retained V3 Replay canonically encodes"),
        RETAINED_V3_EMPTY
    );

    let package = decode_package(ArtifactBytes {
        scenario: SCENARIO,
        numeric_profile: NUMERIC,
        physical_scale_profile: PHYSICAL,
        balance_profile: BALANCE,
    })
    .expect("reference package decodes");
    let simulation = Simulation::new(package).expect("fresh V6 simulation starts");
    assert_eq!(
        artifact.replay().validate_against(&simulation),
        Err(ReplayError::ContractMismatch {
            field: ReplayContractField::FormatVersion,
            expected: "1".to_owned(),
            actual: REPLAY_FORMAT_VERSION_V2.to_string(),
        })
    );
}

#[test]
fn retained_v4_empty_artifact_strictly_decodes_round_trips_and_is_execution_rejected() {
    let artifact =
        decode_replay_artifact(RETAINED_V4_EMPTY).expect("retained V4 Replay strictly decodes");

    assert_eq!(
        artifact.replay().header().state_hash_version.as_str(),
        STATE_HASH_VERSION_V4
    );
    assert!(artifact.replay().commands().is_empty());
    assert_eq!(artifact.replay().checkpoints().len(), 1);
    assert_eq!(
        encode_replay_artifact(&artifact).expect("retained V4 Replay canonically encodes"),
        RETAINED_V4_EMPTY
    );

    let package = decode_package(ArtifactBytes {
        scenario: SCENARIO,
        numeric_profile: NUMERIC,
        physical_scale_profile: PHYSICAL,
        balance_profile: BALANCE,
    })
    .expect("reference package decodes");
    let simulation = Simulation::new(package).expect("fresh V6 simulation starts");
    assert_eq!(
        artifact.replay().validate_against(&simulation),
        Err(ReplayError::ContractMismatch {
            field: ReplayContractField::FormatVersion,
            expected: "1".to_owned(),
            actual: REPLAY_FORMAT_VERSION_V2.to_string(),
        })
    );
}

#[test]
fn main_core_v1_replay_rejects_a_nonzero_seed_before_execution() {
    let mut document: serde_json::Value =
        serde_json::from_slice(S1_M1_CAPACITY).expect("retained Main Core Replay JSON parses");
    document["header"]["seed"] = serde_json::Value::String(format!("1{}", "0".repeat(63)));
    let bytes = serde_json::to_vec(&document).expect("mutated Replay JSON encodes");

    assert_eq!(
        decode_replay_artifact(&bytes),
        Err(ReplayError::NonzeroMainCoreWorldSeed)
    );
}

#[test]
fn retained_feedback_ring_is_the_exact_canonical_replay_encoding() {
    let artifact = decode_replay_artifact(FEEDBACK_RING).expect("feedback Replay strictly decodes");
    let canonical = encode_replay_artifact(&artifact).expect("feedback Replay encodes");

    assert_eq!(canonical, FEEDBACK_RING);
    assert_eq!(
        decode_replay_artifact(&canonical).expect("canonical feedback Replay decodes"),
        artifact
    );
    assert_eq!(artifact.scenario_path(), "../scenarios/empty.json");
    assert_eq!(artifact.replay().commands().len(), 3);
    assert_eq!(artifact.replay().checkpoints().len(), 22);
    assert_eq!(artifact.replay().final_next_tick(), Tick(21));
    assert_eq!(
        artifact
            .replay()
            .checkpoints()
            .last()
            .expect("feedback Replay has a final checkpoint")
            .state_hash,
        StateHash::from_hex("11699b3a0decbd6d72f227060fe737b637be9503aa2a6e726b3a8776b4ae6188")
            .expect("feedback final golden is canonical")
    );
}

#[test]
fn retained_100k_replay_round_trips_semantically_and_freezes_its_contract_golden() {
    let artifact = decode_replay_artifact(STAGE0_100K).expect("100k Replay strictly decodes");
    let canonical = encode_replay_artifact(&artifact).expect("100k Replay canonically encodes");
    let canonical_artifact =
        decode_replay_artifact(&canonical).expect("canonical 100k Replay strictly decodes");

    assert_eq!(canonical_artifact, artifact);
    assert_eq!(artifact.scenario_path(), "../scenarios/empty.json");

    let replay = artifact.replay();
    let header = replay.header();
    assert_eq!(header.format_version.as_u32(), REPLAY_FORMAT_VERSION_V2);
    assert_eq!(header.semantics_version.as_str(), SEMANTICS_VERSION_V1);
    assert_eq!(
        header.numeric_profile_hash.to_string(),
        NUMERIC_PROFILE_HASH
    );
    assert_eq!(
        header.physical_scale_profile_hash.to_string(),
        PHYSICAL_SCALE_PROFILE_HASH
    );
    assert_eq!(
        header.balance_profile_hash.to_string(),
        BALANCE_PROFILE_HASH
    );
    assert_eq!(header.state_hash_version.as_str(), STATE_HASH_VERSION_V6);
    assert_eq!(
        header.world_generator_version.as_str(),
        WORLD_GENERATOR_VERSION_EMPTY_V1
    );
    assert_eq!(header.seed, Seed::ZERO);
    assert_eq!(header.initial_state_hash.to_string(), INITIAL_STATE_HASH);
    assert_eq!(
        header.hash_algorithm_id.as_str(),
        HASH_ALGORITHM_ID_BLAKE3_V1
    );

    assert_eq!(replay.commands().len(), 13);
    assert_eq!(replay.checkpoints().len(), 6);
    assert_eq!(replay.checkpoints()[0].next_tick, Tick(0));
    assert_eq!(
        replay.checkpoints()[0].state_hash,
        header.initial_state_hash
    );
    let final_checkpoint = replay
        .checkpoints()
        .last()
        .expect("100k Replay has a final checkpoint");
    assert_eq!(final_checkpoint.next_tick, Tick(100_000));
    assert_eq!(
        final_checkpoint.state_hash,
        StateHash::from_hex("957a8644c2c9246f37912701883b7eb4da097dea2b515b91ddfb1b25bfcd5a83")
            .expect("100k final golden is canonical")
    );
}
