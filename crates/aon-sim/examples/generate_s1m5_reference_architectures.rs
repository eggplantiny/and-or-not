//! Deterministic retained S1-M5 Brute/Computed reference-architecture generator.
//!
//! The complete artifact publication/verification pipeline lives here rather than in canonical
//! simulation code. `--write` is the only mode that may publish fixtures; default mode rebuilds
//! and byte-compares every retained artifact.

mod support;

use aon_sim::{
    ArtifactBytes, Fixed, FixedAabb, FixedVec2, GatePort, GateType, ReferenceArchitectureArtifact,
    ReferenceArchitectureBindingEndpoint, ReferenceArchitectureEndpoint,
    ReferenceArchitectureFormatVersion, ReferenceArchitectureLocalId,
    ReferenceArchitectureMaterializationSchedule, ReferenceArchitectureObservationBinding,
    ReferenceArchitectureOperation, ReferenceArchitectureRole, ReferenceArchitectureRoleBinding,
    ReferenceArchitectureRoutingDomain, ReferenceArchitectureScenarioResolution,
    ReferenceArchitectureSemanticTarget, ReferenceFixedSubstrate, ReferenceGate, ReferenceJunction,
    ReferenceWire, Simulation, Tick, WireEnd, decode_balance_profile, decode_numeric_profile,
    decode_package, decode_physical_scale_profile, decode_scenario_manifest,
    encode_reference_architecture_artifact, materialize_reference_architecture_pair,
};
use serde_json::json;
use std::path::Path;
use support::s1m5_pipeline::{
    S1M5RetainedSearchInput, build_first_complete_s1m5_retained_artifacts,
};
use support::s1m5_publication::{S1M5RetainedBytes, publish_or_verify_s1m5};
use support::s1m5_readback::{S1M5ReadbackProfileBytes, verify_checked_s1m5_bundle_with_profiles};

const NUMERIC: &[u8] = include_bytes!("../../../profiles/numeric/v1.json");
const PHYSICAL: &[u8] = include_bytes!("../../../profiles/physical-scale/stage0-alpha.json");
const BALANCE: &[u8] =
    include_bytes!("../../../profiles/balance/s1-m4-construction-contact-damage-alpha.json");

const SCENARIO_ID: &str = "s1-m5-reference-architectures-v1";

const WU: i64 = aon_sim::FIXED_ONE;
const Q: i64 = WU / 64;
const CIRCUIT_PITCH: i64 = 16_384;
const WORLD_PITCH: i64 = 65_536;

const fn point_raw(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

const fn wu(x: i64, y: i64) -> FixedVec2 {
    point_raw(x * WU, y * WU)
}

const fn cp(x: i64, y: i64) -> FixedVec2 {
    point_raw(x * CIRCUIT_PITCH, y * CIRCUIT_PITCH)
}

const fn wp(x: i64, y: i64) -> FixedVec2 {
    point_raw(x * WORLD_PITCH, y * WORLD_PITCH)
}

fn id(raw: u32) -> ReferenceArchitectureLocalId {
    ReferenceArchitectureLocalId::new(raw).expect("fixture local IDs are nonzero")
}

fn aabb_wu(radius: i64) -> FixedAabb {
    FixedAabb::new(wu(-radius, -radius), wu(radius, radius))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArchitectureKind {
    Brute,
    Computed,
}

fn scenario_bytes() -> Vec<u8> {
    let numeric = decode_numeric_profile(NUMERIC).expect("Numeric Profile decodes");
    let physical = decode_physical_scale_profile(PHYSICAL).expect("Physical Profile decodes");
    let balance = decode_balance_profile(BALANCE).expect("Balance Profile decodes");

    // One high-generation bridge at the center of every independent sector substrate. The
    // sources are semantically sorted by exact `(x,y,generation)` before world generation.
    let sources = [wp(-64, 0), wp(0, -64), wp(0, 64), wp(64, 0)];
    // Four identical canonical trajectories, one per cardinal sector. In sector-local q units
    // (q = 1/64 WU), every Enemy starts at (34,-35) and advances (-1,+1) per Tick. This keeps the
    // complete four-stage build free of interaction, then crosses both selected defense paths in
    // the retained response window. Scenario-v4 supplies the semantic Enemy ordering.
    let velocity = point_raw(-Q, Q);
    let enemies = sources.map(|source| {
        (
            point_raw(source.x.0 + 34 * Q, source.y.0 - 35 * Q),
            velocity,
        )
    });
    let mut bytes = serde_json::to_vec_pretty(&json!({
        "schemaVersion": 4,
        "scenarioId": SCENARIO_ID,
        "semanticsVersion": "aon-semantics-v1",
        "hashAlgorithm": "blake3-v1",
        "initialWorld": {
            "kind": "main-core-power-enemy-v1",
            "mainCore": {
                "position": { "x": wp(-256, -256).x.0, "y": wp(-256, -256).y.0 },
                "integrity": 100,
                "heatEnergy": 0
            },
            "powerSources": sources.into_iter().map(|position| json!({
                "position": { "x": position.x.0, "y": position.y.0 },
                "generationPerTick": 100_000
            })).collect::<Vec<_>>(),
            "enemies": enemies.into_iter().map(|(position, velocity)| json!({
                "position": { "x": position.x.0, "y": position.y.0 },
                "velocityPerTick": { "x": velocity.x.0, "y": velocity.y.0 },
                "radius": WU / 16,
                "integrity": 10,
                "heatEnergy": 0
            })).collect::<Vec<_>>()
        },
        "requiredFeatures": {
            "signal": true,
            "mobility": true,
            "capacity": true,
            "sensing": true,
            "power": true,
            "relay": false,
            "payload": false,
            "radiation": false,
            "construction": true,
            "contact": true,
            "damage": true
        },
        "profiles": {
            "numeric": {
                "path": "../../profiles/numeric/v1.json",
                "profileId": numeric.profile_id,
                "profileHash": numeric.canonical_hash().expect("Numeric hashes").to_string()
            },
            "physicalScale": {
                "path": "../../profiles/physical-scale/stage0-alpha.json",
                "profileId": physical.profile_id,
                "profileHash": physical.canonical_hash().expect("Physical hashes").to_string()
            },
            "balance": {
                "path": "../../profiles/balance/s1-m4-construction-contact-damage-alpha.json",
                "profileId": balance.profile_id,
                "profileHash": balance.canonical_hash().expect("Balance hashes").to_string()
            }
        }
    }))
    .expect("Scenario JSON encodes");
    bytes.push(b'\n');
    bytes
}

fn package(scenario: &[u8]) -> aon_sim::SimulationPackage {
    decode_package(ArtifactBytes {
        scenario,
        numeric_profile: NUMERIC,
        physical_scale_profile: PHYSICAL,
        balance_profile: BALANCE,
    })
    .expect("generated S1-M5 package decodes")
}

fn scenario_resolution(simulation: &Simulation) -> ReferenceArchitectureScenarioResolution {
    ReferenceArchitectureScenarioResolution {
        main_core: simulation.main_core_state().expect("Main Core exists").id(),
        power_sources: simulation
            .power_sources()
            .map(|source| source.id())
            .collect(),
        enemies: simulation
            .enemies()
            .iter()
            .map(|enemy| enemy.id())
            .collect(),
    }
}

fn fixed_substrate(local_id: u32, origin: FixedVec2) -> ReferenceArchitectureOperation {
    ReferenceArchitectureOperation::PlaceFixedSubstrate(ReferenceFixedSubstrate {
        id: id(local_id),
        origin,
        routing_area: aabb_wu(28),
        footprint: aabb_wu(28),
    })
}

fn gate(
    local_id: u32,
    substrate: u32,
    gate_type: GateType,
    origin: FixedVec2,
) -> ReferenceArchitectureOperation {
    ReferenceArchitectureOperation::PlaceGate(ReferenceGate {
        id: id(local_id),
        routing_domain: ReferenceArchitectureRoutingDomain::FixedSubstrate(id(substrate)),
        gate_type,
        origin,
    })
}

fn junction(local_id: u32, substrate: u32, position: FixedVec2) -> ReferenceArchitectureOperation {
    ReferenceArchitectureOperation::PlaceJunction(ReferenceJunction {
        id: id(local_id),
        routing_domain: ReferenceArchitectureRoutingDomain::FixedSubstrate(id(substrate)),
        position,
    })
}

fn wire(
    local_id: u32,
    substrate: u32,
    points: Vec<FixedVec2>,
    endpoint_a: ReferenceArchitectureEndpoint,
    endpoint_b: ReferenceArchitectureEndpoint,
) -> ReferenceArchitectureOperation {
    ReferenceArchitectureOperation::PlaceWire(ReferenceWire {
        id: id(local_id),
        routing_domain: ReferenceArchitectureRoutingDomain::FixedSubstrate(id(substrate)),
        points,
        endpoint_a,
        endpoint_b,
    })
}

fn artifact(
    kind: ArchitectureKind,
    display_name: &str,
    simulation: &Simulation,
    operations: Vec<ReferenceArchitectureOperation>,
    role_bindings: Vec<ReferenceArchitectureRoleBinding>,
    observation_bindings: Vec<ReferenceArchitectureObservationBinding>,
) -> ReferenceArchitectureArtifact {
    let mut binding_batches = std::array::from_fn::<_, 4, _>(|_| Vec::new());
    for operation in &operations {
        let ReferenceArchitectureOperation::PlaceWire(wire) = operation else {
            continue;
        };
        for (end, target) in [(WireEnd::A, wire.endpoint_a), (WireEnd::B, wire.endpoint_b)] {
            if target == ReferenceArchitectureEndpoint::Free {
                continue;
            }
            let stage = if !matches!(target, ReferenceArchitectureEndpoint::PowerSource { .. }) {
                0
            } else {
                let raw = wire.id.get();
                match kind {
                    // Brute has no state to bootstrap, so every Source end activates in the
                    // shared final stage.
                    ArchitectureKind::Brute => match raw % 10 {
                        0 | 3 => 3,
                        other => panic!("unclassified Brute Source endpoint {raw}/{other}"),
                    },
                    // This is the proven four-stage merge of the original seven semantic
                    // groups: S0+S1 | S2+S3 | S4 | S5+S6.
                    ArchitectureKind::Computed if (2000..2400).contains(&raw) => 0,
                    ArchitectureKind::Computed => match raw % 100 {
                        30 => 0,
                        31 | 32 => 1,
                        35 => 2,
                        21 | 33 | 34 | 36 => 3,
                        other => panic!("unclassified Computed Source endpoint {raw}/{other}"),
                    },
                }
            };
            binding_batches[stage]
                .push(ReferenceArchitectureBindingEndpoint { wire: wire.id, end });
        }
    }
    for batch in &mut binding_batches {
        batch.sort();
    }
    let counts = binding_batches.each_ref().map(Vec::len);
    assert_eq!(
        counts,
        match kind {
            ArchitectureKind::Brute => [48, 0, 0, 32],
            ArchitectureKind::Computed => [156, 8, 4, 16],
        },
        "the retained four-stage endpoint partition is frozen",
    );
    ReferenceArchitectureArtifact {
        format_version: ReferenceArchitectureFormatVersion::V2,
        hash_algorithm_id: aon_sim::HashAlgorithmId::Blake3V1,
        display_name: display_name.to_owned(),
        contract: *simulation.contract(),
        operations,
        role_bindings,
        observation_bindings,
        materialization_schedule: Some(ReferenceArchitectureMaterializationSchedule {
            binding_batches: binding_batches.into_iter().collect(),
        }),
    }
}

fn role(name: impl Into<String>, local_id: u32) -> ReferenceArchitectureRoleBinding {
    ReferenceArchitectureRoleBinding {
        name: name.into(),
        target: ReferenceArchitectureSemanticTarget::LocalEntity(id(local_id)),
    }
}

fn sensor_observation(
    name: impl Into<String>,
    wire_local_id: u32,
) -> ReferenceArchitectureObservationBinding {
    ReferenceArchitectureObservationBinding {
        name: name.into(),
        target: ReferenceArchitectureSemanticTarget::WireSensePort {
            wire: id(wire_local_id),
            end: WireEnd::A,
        },
    }
}

fn gate_port(local_id: u32, port: GatePort) -> ReferenceArchitectureEndpoint {
    ReferenceArchitectureEndpoint::GatePort {
        gate: id(local_id),
        port,
    }
}

fn sensor_port(local_id: u32) -> ReferenceArchitectureEndpoint {
    ReferenceArchitectureEndpoint::WireSensePort {
        wire: id(local_id),
        end: WireEnd::A,
    }
}

/// Initial feasibility fixture for all sixteen exact Brute channels. Each sector occupies an
/// isolated fixed substrate centered on its Source/Enemy anchor. Channel geometry is deliberately
/// short so materialization and startup prove the alpha thermal window rather than hiding it.
fn brute_artifact(simulation: &Simulation) -> ReferenceArchitectureArtifact {
    let mut operations = Vec::new();
    let mut roles = Vec::new();
    let mut observations = Vec::new();
    for sector in 0..4_u32 {
        let substrate = 1 + sector;
        let origin = match sector {
            0 => wp(-64, 0),
            1 => wp(0, -64),
            2 => wp(0, 64),
            _ => wp(64, 0),
        };
        operations.push(fixed_substrate(substrate, origin));
        let junction_offsets = [wu(2, 2), wu(-2, 2), wu(-2, -2), wu(2, -2)];
        let defense_bends = [cp(6, 8), wu(0, 2), wu(-2, 0), wu(0, -2)];
        // Short, non-collinear sensing bodies share the immutable Source anchor without sharing
        // any positive-length segment with a trunk or defense rib.
        let sensor_offsets = [
            point_raw(WU, WU / 2),
            cp(-1, -3),
            point_raw(-WU, -WU / 2),
            cp(-2, 1),
        ];
        let trunk_first_offsets = [cp(1, 1), cp(-1, 1), cp(-1, -1), cp(1, -1)];
        for channel in 0..4_u32 {
            let base = 100 + sector * 100 + channel * 10;
            let sensor = base;
            let trunk = base + 1;
            let branch = base + 2;
            let defense = base + 3;
            let add = |left: FixedVec2, right: FixedVec2| {
                point_raw(left.x.0 + right.x.0, left.y.0 + right.y.0)
            };
            let sensor_a = origin;
            let sensor_b = add(origin, sensor_offsets[channel as usize]);
            let trunk_first = add(origin, trunk_first_offsets[channel as usize]);
            let trunk_b = add(origin, junction_offsets[channel as usize]);
            let defense_bend = add(origin, defense_bends[channel as usize]);
            let defense_points = if channel == 0 {
                vec![trunk_b, add(origin, cp(5, -3)), origin]
            } else if channel == 3 {
                vec![trunk_b, add(origin, cp(-2, -8)), origin]
            } else {
                vec![trunk_b, defense_bend, origin]
            };
            operations.push(wire(
                sensor,
                substrate,
                vec![sensor_a, sensor_b],
                ReferenceArchitectureEndpoint::PowerSource { ordinal: sector },
                ReferenceArchitectureEndpoint::Free,
            ));
            operations.push(junction(branch, substrate, trunk_b));
            operations.push(wire(
                trunk,
                substrate,
                if channel == 3 {
                    vec![sensor_a, add(origin, cp(-3, -10)), trunk_b]
                } else {
                    vec![sensor_a, trunk_first, trunk_b]
                },
                sensor_port(sensor),
                ReferenceArchitectureEndpoint::Junction(id(branch)),
            ));
            operations.push(wire(
                defense,
                substrate,
                defense_points,
                ReferenceArchitectureEndpoint::Junction(id(branch)),
                ReferenceArchitectureEndpoint::PowerSource { ordinal: sector },
            ));
            let sector_name = ["west", "south", "north", "east"][sector as usize];
            roles.push(role(format!("sensor.{sector_name}.{channel}"), sensor));
            roles.push(role(format!("trunk.{sector_name}.{channel}"), trunk));
            roles.push(role(format!("defense.{sector_name}.{channel}"), defense));
            observations.push(sensor_observation(
                format!("sensor.{sector_name}.{channel}"),
                sensor,
            ));
        }
    }
    artifact(
        ArchitectureKind::Brute,
        "S1-M5 Brute reference architecture",
        simulation,
        operations,
        roles,
        observations,
    )
}

/// First complete Computed structural oracle. The topology is intentionally assembled in this
/// generator rather than encoded as opaque fixture JSON: 12 reduction OR + four state cells of
/// two OR/two NOT, four shared trunks, and four defenses.
fn computed_artifact(simulation: &Simulation) -> ReferenceArchitectureArtifact {
    let mut operations = Vec::new();
    let mut roles = Vec::new();
    let mut observations = Vec::new();
    for sector in 0..4_u32 {
        let substrate = 1 + sector;
        let origin = match sector {
            0 => wp(-64, 0),
            1 => wp(0, -64),
            2 => wp(0, 64),
            _ => wp(64, 0),
        };
        operations.push(fixed_substrate(substrate, origin));
        let gate_base = 1000 + sector * 100;
        let sensor_base = 2000 + sector * 100;
        let junction_base = 3000 + sector * 100;
        let wire_base = 4000 + sector * 100;
        let at = |x: i64, y: i64| {
            point_raw(
                origin.x.0 + x * CIRCUIT_PITCH,
                origin.y.0 + y * CIRCUIT_PITCH,
            )
        };
        let binary_port = |x: i64, y: i64, port: GatePort| match port {
            GatePort::InputA => point_raw(
                origin.x.0 + x * CIRCUIT_PITCH - CIRCUIT_PITCH,
                origin.y.0 + y * CIRCUIT_PITCH - CIRCUIT_PITCH / 2,
            ),
            GatePort::InputB => point_raw(
                origin.x.0 + x * CIRCUIT_PITCH - CIRCUIT_PITCH,
                origin.y.0 + y * CIRCUIT_PITCH + CIRCUIT_PITCH / 2,
            ),
            GatePort::Output => at(x + 1, y),
            GatePort::Power => at(x, y - 1),
        };
        let not_port = |x: i64, y: i64, port: GatePort| match port {
            GatePort::InputA => at(x - 1, y),
            GatePort::Output => at(x + 1, y),
            GatePort::Power => at(x, y - 1),
            GatePort::InputB => unreachable!("NOT has no InputB"),
        };

        // P01, P23, and Set reduce four sampled presences. The two OR/NOT pairs implement
        // Q=NOT(Reset OR Qbar), Qbar=NOT(Set OR Q), with Reset retained LOW in v1.
        for (offset, gate_type, x, y) in [
            (0, GateType::Or, 2, 3),
            (1, GateType::Or, 2, -3),
            (2, GateType::Or, 5, 3),
            (3, GateType::Or, 8, -3),
            (4, GateType::Not, 11, -3),
            (5, GateType::Or, 8, 3),
            (6, GateType::Not, 11, 3),
        ] {
            operations.push(gate(gate_base + offset, substrate, gate_type, at(x, y)));
        }
        operations.push(junction(junction_base, substrate, at(6, 0)));
        operations.push(junction(junction_base + 1, substrate, at(4, -1)));

        let sensor_offsets = [
            point_raw(-1_024, 2_048),
            point_raw(-16_384, -49_152),
            point_raw(-3_072, 8_192),
            point_raw(-5_120, 13_312),
        ];
        for channel in 0..4_u32 {
            let sensor = sensor_base + channel;
            let sensor_b = point_raw(
                origin.x.0 + sensor_offsets[channel as usize].x.0,
                origin.y.0 + sensor_offsets[channel as usize].y.0,
            );
            operations.push(wire(
                sensor,
                substrate,
                vec![origin, sensor_b],
                ReferenceArchitectureEndpoint::PowerSource { ordinal: sector },
                ReferenceArchitectureEndpoint::Free,
            ));
            let sector_name = ["west", "south", "north", "east"][sector as usize];
            roles.push(role(format!("sensor.{sector_name}.{channel}"), sensor));
            observations.push(sensor_observation(
                format!("sensor.{sector_name}.{channel}"),
                sensor,
            ));
        }

        for (offset, sensor, target_gate, target_port, points) in [
            (
                0,
                sensor_base,
                gate_base,
                GatePort::InputA,
                vec![origin, binary_port(2, 3, GatePort::InputA)],
            ),
            (
                1,
                sensor_base + 1,
                gate_base,
                GatePort::InputB,
                vec![origin, binary_port(2, 3, GatePort::InputB)],
            ),
            (
                2,
                sensor_base + 2,
                gate_base + 1,
                GatePort::InputA,
                vec![origin, binary_port(2, -3, GatePort::InputA)],
            ),
            (
                3,
                sensor_base + 3,
                gate_base + 1,
                GatePort::InputB,
                vec![origin, binary_port(2, -3, GatePort::InputB)],
            ),
        ] {
            operations.push(wire(
                wire_base + offset,
                substrate,
                points,
                sensor_port(sensor),
                gate_port(target_gate, target_port),
            ));
        }
        operations.push(wire(
            wire_base + 4,
            substrate,
            vec![
                binary_port(2, 3, GatePort::Output),
                binary_port(5, 3, GatePort::InputA),
            ],
            gate_port(gate_base, GatePort::Output),
            gate_port(gate_base + 2, GatePort::InputA),
        ));
        operations.push(wire(
            wire_base + 5,
            substrate,
            vec![
                binary_port(2, -3, GatePort::Output),
                binary_port(5, 3, GatePort::InputB),
            ],
            gate_port(gate_base + 1, GatePort::Output),
            gate_port(gate_base + 2, GatePort::InputB),
        ));
        operations.push(wire(
            wire_base + 6,
            substrate,
            vec![
                binary_port(5, 3, GatePort::Output),
                binary_port(8, 3, GatePort::InputB),
            ],
            gate_port(gate_base + 2, GatePort::Output),
            gate_port(gate_base + 5, GatePort::InputB),
        ));

        operations.push(wire(
            wire_base + 10,
            substrate,
            vec![
                binary_port(8, -3, GatePort::Output),
                not_port(11, -3, GatePort::InputA),
            ],
            gate_port(gate_base + 3, GatePort::Output),
            gate_port(gate_base + 4, GatePort::InputA),
        ));
        operations.push(wire(
            wire_base + 11,
            substrate,
            vec![
                binary_port(8, 3, GatePort::Output),
                not_port(11, 3, GatePort::InputA),
            ],
            gate_port(gate_base + 5, GatePort::Output),
            gate_port(gate_base + 6, GatePort::InputA),
        ));
        operations.push(wire(
            wire_base + 12,
            substrate,
            vec![
                not_port(11, -3, GatePort::Output),
                at(13, -3),
                at(13, 0),
                at(6, 0),
            ],
            gate_port(gate_base + 4, GatePort::Output),
            ReferenceArchitectureEndpoint::Junction(id(junction_base)),
        ));
        operations.push(wire(
            wire_base + 13,
            substrate,
            vec![at(6, 0), binary_port(8, 3, GatePort::InputA)],
            ReferenceArchitectureEndpoint::Junction(id(junction_base)),
            gate_port(gate_base + 5, GatePort::InputA),
        ));
        operations.push(wire(
            wire_base + 14,
            substrate,
            vec![
                not_port(11, 3, GatePort::Output),
                at(14, 3),
                at(14, 1),
                at(6, -2),
                binary_port(8, -3, GatePort::InputB),
            ],
            gate_port(gate_base + 6, GatePort::Output),
            gate_port(gate_base + 3, GatePort::InputB),
        ));
        operations.push(wire(
            wire_base + 20,
            substrate,
            vec![at(6, 0), at(4, -1)],
            ReferenceArchitectureEndpoint::Junction(id(junction_base)),
            ReferenceArchitectureEndpoint::Junction(id(junction_base + 1)),
        ));
        operations.push(wire(
            wire_base + 21,
            substrate,
            vec![at(4, -1), at(5, -3), origin],
            ReferenceArchitectureEndpoint::Junction(id(junction_base + 1)),
            ReferenceArchitectureEndpoint::PowerSource { ordinal: sector },
        ));

        let power_targets = [
            binary_port(2, 3, GatePort::Power),
            binary_port(2, -3, GatePort::Power),
            binary_port(5, 3, GatePort::Power),
            binary_port(8, -3, GatePort::Power),
            not_port(11, -3, GatePort::Power),
            binary_port(8, 3, GatePort::Power),
            not_port(11, 3, GatePort::Power),
        ];
        let power_routes = [
            vec![origin, power_targets[0]],
            vec![origin, at(0, -5), power_targets[1]],
            vec![origin, power_targets[2]],
            vec![origin, at(-2, -5), power_targets[3]],
            vec![origin, at(1, -5), at(6, -5), power_targets[4]],
            vec![origin, power_targets[5]],
            vec![origin, power_targets[6]],
        ];
        for (offset, points) in power_routes.into_iter().enumerate() {
            operations.push(wire(
                wire_base + 30 + u32::try_from(offset).expect("seven power wires"),
                substrate,
                points,
                ReferenceArchitectureEndpoint::PowerSource { ordinal: sector },
                gate_port(
                    gate_base + u32::try_from(offset).expect("seven gates"),
                    GatePort::Power,
                ),
            ));
        }

        let sector_name = ["west", "south", "north", "east"][sector as usize];
        roles.push(role(format!("reduction.{sector_name}.pair0"), gate_base));
        roles.push(role(
            format!("reduction.{sector_name}.pair1"),
            gate_base + 1,
        ));
        roles.push(role(
            format!("reduction.{sector_name}.presence"),
            gate_base + 2,
        ));
        roles.push(role(format!("shared.{sector_name}"), wire_base + 20));
        roles.push(role(format!("defense.{sector_name}.0"), wire_base + 21));
        roles.push(role(
            format!(
                "state.{}.q",
                ["west", "south", "north", "east"][sector as usize]
            ),
            gate_base + 4,
        ));
        roles.push(role(
            format!(
                "state.{}.qbar",
                ["west", "south", "north", "east"][sector as usize]
            ),
            gate_base + 6,
        ));
    }
    artifact(
        ArchitectureKind::Computed,
        "S1-M5 Computed reference architecture",
        simulation,
        operations,
        roles,
        observations,
    )
}

#[derive(Clone)]
struct GeneratedBundle {
    scenario: Vec<u8>,
    brute_design: Vec<u8>,
    computed_design: Vec<u8>,
    pair: Vec<u8>,
    plan: Vec<u8>,
    metric_set: Vec<u8>,
    brute_replay: Vec<u8>,
    computed_replay: Vec<u8>,
    brute_metric: Vec<u8>,
    computed_metric: Vec<u8>,
    brute_run_id: String,
    computed_run_id: String,
    build_end_tick: Tick,
    measurement_start_tick: Tick,
    max_ticks: Tick,
}

impl GeneratedBundle {
    fn retained_bytes(&self) -> S1M5RetainedBytes<'_> {
        S1M5RetainedBytes {
            scenario: &self.scenario,
            brute_design: &self.brute_design,
            computed_design: &self.computed_design,
            pair: &self.pair,
            plan: &self.plan,
            metric_set: &self.metric_set,
            brute_replay: &self.brute_replay,
            computed_replay: &self.computed_replay,
            brute_metric: &self.brute_metric,
            computed_metric: &self.computed_metric,
        }
    }
}

fn build_complete_bundle() -> GeneratedBundle {
    let scenario_bytes = scenario_bytes();
    let scenario =
        decode_scenario_manifest(&scenario_bytes).expect("generated S1-M5 Scenario decodes");
    assert_eq!(scenario.scenario_id(), SCENARIO_ID);

    let package = package(&scenario_bytes);
    let pristine = Simulation::new(package.clone()).expect("S1-M5 Simulation starts");
    let resolution = scenario_resolution(&pristine);
    let brute = brute_artifact(&pristine);
    let computed = computed_artifact(&pristine);
    let brute_design = encode_reference_architecture_artifact(&brute)
        .expect("Brute design encodes")
        .into_bytes();
    let computed_design = encode_reference_architecture_artifact(&computed)
        .expect("Computed design encodes")
        .into_bytes();

    let ((_, brute_materialization), (_, computed_materialization)) =
        materialize_reference_architecture_pair(
            (
                Simulation::new(package.clone()).expect("private Brute candidate starts"),
                &brute,
                &resolution,
            ),
            (
                Simulation::new(package.clone()).expect("private Computed candidate starts"),
                &computed,
                &resolution,
            ),
        )
        .expect("Brute and Computed materialize on one common four-stage schedule");

    let retained = build_first_complete_s1m5_retained_artifacts(S1M5RetainedSearchInput {
        scenario: &scenario,
        package: &package,
        scenario_resolution: &resolution,
        brute: &brute,
        brute_materialization: &brute_materialization,
        computed: &computed,
        computed_materialization: &computed_materialization,
    })
    .expect("the first complete retained response window builds");

    let build_end_tick = retained.pair.pair.build_end_tick();
    let measurement_start_tick = retained.pair.pair.measurement_start_tick();
    let max_ticks = retained.pair.pair.max_ticks();
    assert_eq!(
        build_end_tick,
        Tick(18),
        "the four-stage build boundary is frozen"
    );
    assert_eq!(
        measurement_start_tick, build_end_tick,
        "measurement begins at the final common quiescent boundary",
    );
    assert_eq!(
        max_ticks,
        Tick(20),
        "the first complete response horizon is frozen"
    );

    let run_id = |role| {
        retained
            .pair
            .runs
            .iter()
            .find(|run| run.design.role == role)
            .map(|run| run.run_id.to_string())
            .expect("both retained role Runs exist")
    };
    let brute_run_id = run_id(ReferenceArchitectureRole::Brute);
    let computed_run_id = run_id(ReferenceArchitectureRole::Computed);

    GeneratedBundle {
        scenario: scenario_bytes,
        brute_design,
        computed_design,
        pair: retained.pair.pair_bytes,
        plan: retained.pair.plan_bytes,
        metric_set: retained.pair.metric_set_bytes,
        brute_replay: retained.brute.replay_bytes,
        computed_replay: retained.computed.replay_bytes,
        brute_metric: retained.brute.metric_bytes,
        computed_metric: retained.computed.metric_bytes,
        brute_run_id,
        computed_run_id,
        build_end_tick,
        measurement_start_tick,
        max_ticks,
    }
}

fn assert_complete_double_build(first: &GeneratedBundle, second: &GeneratedBundle) {
    for (name, left, right) in [
        (
            "Scenario",
            first.scenario.as_slice(),
            second.scenario.as_slice(),
        ),
        (
            "Brute design",
            first.brute_design.as_slice(),
            second.brute_design.as_slice(),
        ),
        (
            "Computed design",
            first.computed_design.as_slice(),
            second.computed_design.as_slice(),
        ),
        ("Pair", first.pair.as_slice(), second.pair.as_slice()),
        ("Plan", first.plan.as_slice(), second.plan.as_slice()),
        (
            "Metric Set",
            first.metric_set.as_slice(),
            second.metric_set.as_slice(),
        ),
        (
            "Brute Replay",
            first.brute_replay.as_slice(),
            second.brute_replay.as_slice(),
        ),
        (
            "Computed Replay",
            first.computed_replay.as_slice(),
            second.computed_replay.as_slice(),
        ),
        (
            "Brute metric",
            first.brute_metric.as_slice(),
            second.brute_metric.as_slice(),
        ),
        (
            "Computed metric",
            first.computed_metric.as_slice(),
            second.computed_metric.as_slice(),
        ),
    ] {
        assert_eq!(left, right, "{name} is byte-stable across complete builds");
    }
    assert_eq!(first.brute_run_id, second.brute_run_id);
    assert_eq!(first.computed_run_id, second.computed_run_id);
    assert_eq!(first.build_end_tick, second.build_end_tick);
    assert_eq!(first.measurement_start_tick, second.measurement_start_tick,);
    assert_eq!(first.max_ticks, second.max_ticks);
}

fn main() {
    let mut write = false;
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--write" if !write => write = true,
            _ => panic!("usage: generate_s1m5_reference_architectures [--write]"),
        }
    }

    let generated = build_complete_bundle();
    let repeated = build_complete_bundle();
    assert_complete_double_build(&generated, &repeated);

    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let publication = publish_or_verify_s1m5(&workspace_root, generated.retained_bytes(), write)
        .expect("the complete S1-M5 bundle publishes or byte-verifies transactionally");
    let readback = verify_checked_s1m5_bundle_with_profiles(
        &workspace_root,
        S1M5ReadbackProfileBytes {
            numeric: NUMERIC,
            physical_scale: PHYSICAL,
            balance: BALANCE,
        },
    )
    .expect("the checked-in S1-M5 bundle passes executable readback");
    assert_eq!(readback.publication, publication);
    assert_eq!(readback.build_end_tick, generated.build_end_tick);
    assert_eq!(
        readback.measurement_start_tick,
        generated.measurement_start_tick,
    );
    assert_eq!(readback.max_ticks, generated.max_ticks);

    println!("s1m5ReferenceStatus=verified");
    println!("bruteArtifactHash={}", publication.brute_design_hash);
    println!("computedArtifactHash={}", publication.computed_design_hash);
    println!("pairArtifactHash={}", publication.pair_hash);
    println!("bruteRunId={}", generated.brute_run_id);
    println!("computedRunId={}", generated.computed_run_id);
}
