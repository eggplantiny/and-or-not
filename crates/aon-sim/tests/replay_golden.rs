use aon_sim::{
    ArtifactBytes, HASH_ALGORITHM_ID_BLAKE3_V1, REPLAY_FORMAT_VERSION_V1, ReplayError,
    SEMANTICS_VERSION_V1, STATE_HASH_VERSION_V3, STATE_HASH_VERSION_V4, Seed, Simulation,
    StateHash, Tick, WORLD_GENERATOR_VERSION_EMPTY_V1, decode_package, decode_replay_artifact,
    encode_replay_artifact,
};

const FEEDBACK_RING: &[u8] = include_bytes!("../../../fixtures/replays/feedback-ring-v1.json");
const STAGE0_100K: &[u8] = include_bytes!("../../../fixtures/replays/stage0-100k-v1.json");
const RETAINED_V3_EMPTY: &[u8] = include_bytes!("fixtures/replay-v3-empty-v1.json");
const SCENARIO: &[u8] = include_bytes!("../../../fixtures/scenarios/empty.json");
const NUMERIC: &[u8] = include_bytes!("../../../profiles/numeric/v1.json");
const PHYSICAL: &[u8] = include_bytes!("../../../profiles/physical-scale/stage0-alpha.json");
const BALANCE: &[u8] = include_bytes!("../../../profiles/balance/stage0-alpha.json");

const INITIAL_STATE_HASH: &str = "d38728eecf3689c031b8a57d69961c3f2b820915b6f174e1f6c7837d59b4c1f3";
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
    let simulation = Simulation::new(package).expect("fresh V4 simulation starts");
    assert_eq!(
        artifact.replay().validate_against(&simulation),
        Err(ReplayError::UnsupportedStateHashVersion {
            expected: STATE_HASH_VERSION_V4,
            actual: STATE_HASH_VERSION_V3.to_owned(),
        })
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
        StateHash::from_hex("db7b87e385d33bf3b9e771717420b0a8ab439fbbf5ca24d1210cbbc19dfed866")
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
    assert_eq!(header.format_version.as_u32(), REPLAY_FORMAT_VERSION_V1);
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
    assert_eq!(header.state_hash_version.as_str(), STATE_HASH_VERSION_V4);
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
        StateHash::from_hex("eda68c2223e47399ac7b0196034f94409b71f43ae260c8945af6d1111b469519")
            .expect("100k final golden is canonical")
    );
}
