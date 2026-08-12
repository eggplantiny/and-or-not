use aon_sim::{
    AbsoluteModuleGeometry, Fixed, FixedAabb, FixedVec2, GateBlueprint, GatePort, GateType,
    HashAlgorithmId, JunctionBlueprint, MODULE_FORMAT_VERSION_V1, ModuleBlueprint, ModuleContract,
    ModuleEndpoint, ModuleError, ModuleFormatVersion, ModuleIoBinding, ModuleLocalId,
    ModuleProvenance, ModuleRoutingDomain, NumericError, PhysicalScaleProfile, ProfileHash,
    SemanticsVersion, SimulationContract, SubstrateBlueprint, WireBlueprint,
    decode_module_artifact, encode_module_artifact, validate_module_against,
};

const QUANTUM: i64 = 1_024;
const CIRCUIT_PITCH: i64 = 16_384;
const WORLD_PITCH: i64 = 65_536;
const MODULE_HASH_GOLDEN: &str = "e7130605cbaebd753f8f338be7a633d8006bc6f85b14bbd5c74e44ecd0a06172";

fn id(value: u32) -> ModuleLocalId {
    ModuleLocalId::new(value).unwrap()
}

fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

fn aabb(min_x: i64, min_y: i64, max_x: i64, max_y: i64) -> FixedAabb {
    FixedAabb::new(point(min_x, min_y), point(max_x, max_y))
}

fn hash(byte: u8) -> ProfileHash {
    ProfileHash::from_hex(&format!("{byte:02x}").repeat(32)).unwrap()
}

fn fixture() -> (ModuleBlueprint, SimulationContract, PhysicalScaleProfile) {
    let physical = PhysicalScaleProfile::stage0_alpha("module-test-physical");
    let physical_hash = physical.canonical_hash().unwrap();
    let numeric_hash = hash(0x11);
    let balance_hash = hash(0x22);
    let contract = SimulationContract {
        semantics_version: SemanticsVersion::AonV1,
        numeric_profile_hash: numeric_hash,
        physical_scale_profile_hash: physical_hash,
        balance_profile_hash: balance_hash,
    };
    let module = ModuleBlueprint {
        format_version: ModuleFormatVersion::V1,
        hash_algorithm_id: HashAlgorithmId::Blake3V1,
        name: "retained-and".to_owned(),
        contract: ModuleContract {
            semantics_version: SemanticsVersion::AonV1,
            numeric_profile_hash: numeric_hash,
            physical_scale_profile_hash: physical_hash,
        },
        balance_profile_hash: Some(balance_hash),
        geometry: AbsoluteModuleGeometry {
            substrates: vec![SubstrateBlueprint {
                id: id(1),
                origin: point(0, 0),
                routing_area: aabb(-65_536, -65_536, 65_536, 65_536),
                footprint: aabb(-131_072, -131_072, 131_072, 131_072),
            }],
            gates: vec![GateBlueprint {
                id: id(2),
                substrate: id(1),
                gate_type: GateType::And,
                origin: point(0, 0),
            }],
            junctions: vec![JunctionBlueprint {
                id: id(3),
                routing_domain: ModuleRoutingDomain::Substrate(id(1)),
                position: point(65_536, 0),
            }],
            wires: vec![WireBlueprint {
                id: id(4),
                routing_domain: ModuleRoutingDomain::Substrate(id(1)),
                points: vec![point(16_384, 0), point(65_536, 0)],
                endpoint_a: ModuleEndpoint::GatePort {
                    gate: id(2),
                    port: GatePort::Output,
                },
                endpoint_b: ModuleEndpoint::Junction(id(3)),
            }],
        },
        io_bindings: vec![
            ModuleIoBinding {
                name: "out".to_owned(),
                endpoint: ModuleEndpoint::Junction(id(3)),
            },
            ModuleIoBinding {
                name: "in".to_owned(),
                endpoint: ModuleEndpoint::GatePort {
                    gate: id(2),
                    port: GatePort::InputA,
                },
            },
        ],
        provenance: ModuleProvenance {
            source: Some("tests/fixture-v1".to_owned()),
        },
    };
    (module, contract, physical)
}

fn geometry_only_fixture() -> (ModuleBlueprint, SimulationContract, PhysicalScaleProfile) {
    let (mut module, contract, physical) = fixture();
    module.geometry.gates.clear();
    module.geometry.junctions.clear();
    module.geometry.wires.clear();
    module.io_bindings.clear();
    (module, contract, physical)
}

fn free_wire(
    local_id: u32,
    routing_domain: ModuleRoutingDomain,
    points: Vec<FixedVec2>,
) -> WireBlueprint {
    WireBlueprint {
        id: id(local_id),
        routing_domain,
        points,
        endpoint_a: ModuleEndpoint::Free,
        endpoint_b: ModuleEndpoint::Free,
    }
}

fn independent_module_hash(module: &ModuleBlueprint) -> String {
    fn u8_bytes(bytes: &mut Vec<u8>, value: u8) {
        bytes.push(value);
    }

    fn u16_bytes(bytes: &mut Vec<u8>, value: u16) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn u32_bytes(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn i64_bytes(bytes: &mut Vec<u8>, value: i64) {
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn text(bytes: &mut Vec<u8>, value: &str) {
        u32_bytes(bytes, u32::try_from(value.len()).unwrap());
        bytes.extend_from_slice(value.as_bytes());
    }

    fn count(bytes: &mut Vec<u8>, value: usize) {
        u32_bytes(bytes, u32::try_from(value).unwrap());
    }

    fn local_id(bytes: &mut Vec<u8>, value: ModuleLocalId) {
        u32_bytes(bytes, value.get());
    }

    fn fixed_point(bytes: &mut Vec<u8>, value: FixedVec2) {
        i64_bytes(bytes, value.x.0);
        i64_bytes(bytes, value.y.0);
    }

    fn fixed_aabb(bytes: &mut Vec<u8>, value: FixedAabb) {
        fixed_point(bytes, value.min);
        fixed_point(bytes, value.max);
    }

    fn domain(bytes: &mut Vec<u8>, value: ModuleRoutingDomain) {
        match value {
            ModuleRoutingDomain::OpenWorld => u8_bytes(bytes, 0),
            ModuleRoutingDomain::Substrate(substrate) => {
                u8_bytes(bytes, 1);
                local_id(bytes, substrate);
            }
        }
    }

    fn port_tag(port: GatePort) -> u8 {
        match port {
            GatePort::InputA => 0,
            GatePort::InputB => 1,
            GatePort::Output => 2,
            GatePort::Power => 3,
        }
    }

    fn endpoint(bytes: &mut Vec<u8>, value: ModuleEndpoint) {
        match value {
            ModuleEndpoint::Free => u8_bytes(bytes, 0),
            ModuleEndpoint::Junction(junction) => {
                u8_bytes(bytes, 1);
                local_id(bytes, junction);
            }
            ModuleEndpoint::GatePort { gate, port } => {
                u8_bytes(bytes, 2);
                local_id(bytes, gate);
                u8_bytes(bytes, port_tag(port));
            }
        }
    }

    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"AON\0MODULE\0V1\0");
    u16_bytes(&mut bytes, 1);
    u32_bytes(&mut bytes, module.format_version.as_u32());
    text(&mut bytes, module.hash_algorithm_id.as_str());
    text(&mut bytes, module.contract.semantics_version.as_str());
    bytes.extend_from_slice(module.contract.numeric_profile_hash.as_bytes());
    bytes.extend_from_slice(module.contract.physical_scale_profile_hash.as_bytes());

    let mut substrates: Vec<_> = module.geometry.substrates.iter().collect();
    substrates.sort_by_key(|substrate| substrate.id);
    count(&mut bytes, substrates.len());
    for substrate in substrates {
        local_id(&mut bytes, substrate.id);
        fixed_point(&mut bytes, substrate.origin);
        fixed_aabb(&mut bytes, substrate.routing_area);
        fixed_aabb(&mut bytes, substrate.footprint);
    }

    let mut gates: Vec<_> = module.geometry.gates.iter().collect();
    gates.sort_by_key(|gate| gate.id);
    count(&mut bytes, gates.len());
    for gate in gates {
        local_id(&mut bytes, gate.id);
        local_id(&mut bytes, gate.substrate);
        u8_bytes(
            &mut bytes,
            match gate.gate_type {
                GateType::And => 0,
                GateType::Or => 1,
                GateType::Not => 2,
            },
        );
        fixed_point(&mut bytes, gate.origin);
    }

    let mut junctions: Vec<_> = module.geometry.junctions.iter().collect();
    junctions.sort_by_key(|junction| junction.id);
    count(&mut bytes, junctions.len());
    for junction in junctions {
        local_id(&mut bytes, junction.id);
        domain(&mut bytes, junction.routing_domain);
        fixed_point(&mut bytes, junction.position);
    }

    let mut wires: Vec<_> = module.geometry.wires.iter().collect();
    wires.sort_by_key(|wire| wire.id);
    count(&mut bytes, wires.len());
    for wire in wires {
        local_id(&mut bytes, wire.id);
        domain(&mut bytes, wire.routing_domain);
        count(&mut bytes, wire.points.len());
        for &point in &wire.points {
            fixed_point(&mut bytes, point);
        }
        endpoint(&mut bytes, wire.endpoint_a);
        endpoint(&mut bytes, wire.endpoint_b);
    }

    let mut io_bindings: Vec<_> = module.io_bindings.iter().collect();
    let independent_endpoint_sort_key = |endpoint: ModuleEndpoint| match endpoint {
        ModuleEndpoint::Free => (0, 0, 0),
        ModuleEndpoint::Junction(junction) => (1, junction.get(), 0),
        ModuleEndpoint::GatePort { gate, port } => (2, gate.get(), port_tag(port)),
    };
    io_bindings.sort_by(|left, right| {
        left.name
            .as_bytes()
            .cmp(right.name.as_bytes())
            .then_with(|| {
                independent_endpoint_sort_key(left.endpoint)
                    .cmp(&independent_endpoint_sort_key(right.endpoint))
            })
    });
    count(&mut bytes, io_bindings.len());
    for binding in io_bindings {
        text(&mut bytes, &binding.name);
        endpoint(&mut bytes, binding.endpoint);
    }

    blake3::hash(&bytes).to_hex().to_string()
}

#[test]
fn exact_contract_validation_preserves_every_absolute_coordinate() {
    let (module, contract, mut physical) = fixture();
    let before = module.clone();
    let before_hash = module.semantic_hash().unwrap();
    let before_bytes = encode_module_artifact(&module).unwrap();

    validate_module_against(&module, &contract, &physical).unwrap();

    physical.profile_id = "metadata-only-physical-id".to_owned();
    assert_eq!(
        physical.canonical_hash().unwrap(),
        contract.physical_scale_profile_hash
    );
    validate_module_against(&module, &contract, &physical).unwrap();

    assert_eq!(module, before);
    assert_eq!(module.semantic_hash().unwrap(), before_hash);
    assert_eq!(encode_module_artifact(&module).unwrap(), before_bytes);
    assert_eq!(module.geometry.substrates[0].origin, point(0, 0));
    assert_eq!(module.geometry.gates[0].origin, point(0, 0));
    assert_eq!(
        module.geometry.wires[0].points,
        vec![point(16_384, 0), point(65_536, 0)]
    );
}

#[test]
fn compatibility_mismatches_precede_invalid_geometry_and_do_not_mutate() {
    let (mut module, contract, physical) = fixture();
    module.geometry.wires[0].points[0] = point(1, 0);

    module.contract.numeric_profile_hash = hash(0x33);
    let before = module.clone();
    let before_hash = module.semantic_hash().unwrap();
    let before_bytes = encode_module_artifact(&module).unwrap();
    assert!(matches!(
        validate_module_against(&module, &contract, &physical),
        Err(ModuleError::NumericProfileMismatch { .. })
    ));
    assert_eq!(module, before);
    assert_eq!(module.semantic_hash().unwrap(), before_hash);
    assert_eq!(encode_module_artifact(&module).unwrap(), before_bytes);

    module.contract.numeric_profile_hash = contract.numeric_profile_hash;
    module.contract.physical_scale_profile_hash = hash(0x44);
    let before = module.clone();
    let before_hash = module.semantic_hash().unwrap();
    let before_bytes = encode_module_artifact(&module).unwrap();
    assert!(matches!(
        validate_module_against(&module, &contract, &physical),
        Err(ModuleError::PhysicalScaleProfileMismatch { .. })
    ));
    assert_eq!(module, before);
    assert_eq!(module.semantic_hash().unwrap(), before_hash);
    assert_eq!(encode_module_artifact(&module).unwrap(), before_bytes);
}

#[test]
fn valid_geometry_rejects_numeric_and_physical_contract_mismatches() {
    let (module, mut target, physical) = fixture();
    let before_hash = module.semantic_hash().unwrap();
    let before_bytes = encode_module_artifact(&module).unwrap();

    target.numeric_profile_hash = hash(0x33);
    assert!(matches!(
        validate_module_against(&module, &target, &physical),
        Err(ModuleError::NumericProfileMismatch { .. })
    ));

    target.numeric_profile_hash = module.contract.numeric_profile_hash;
    target.physical_scale_profile_hash = hash(0x44);
    assert!(matches!(
        validate_module_against(&module, &target, &physical),
        Err(ModuleError::PhysicalScaleProfileMismatch { .. })
    ));

    assert_eq!(module.semantic_hash().unwrap(), before_hash);
    assert_eq!(encode_module_artifact(&module).unwrap(), before_bytes);
}

#[test]
fn supplied_physical_profile_must_match_the_already_matched_contract() {
    let (module, contract, _) = fixture();
    let mut different = PhysicalScaleProfile::stage0_alpha("different-geometry");
    different.wire_body_radius = Fixed(different.wire_body_radius.0 * 2);

    assert!(matches!(
        validate_module_against(&module, &contract, &different),
        Err(ModuleError::TargetPhysicalProfileMismatch { .. })
    ));
}

#[test]
fn semantic_hash_is_canonical_sensitive_and_excludes_analysis_metadata() {
    let (module, _, _) = fixture();
    let expected = module.semantic_hash().unwrap();
    assert_eq!(expected.to_string(), MODULE_HASH_GOLDEN);

    let mut metadata = module.clone();
    metadata.name = "renamed-display-only".to_owned();
    metadata.provenance.source = Some("a/different/path".to_owned());
    metadata.balance_profile_hash = Some(hash(0x99));
    metadata.io_bindings.reverse();
    assert_eq!(metadata.semantic_hash().unwrap(), expected);

    let mut geometry = module.clone();
    geometry.geometry.wires[0].points[1].x = Fixed(49_152);
    geometry.geometry.junctions[0].position.x = Fixed(49_152);
    assert_ne!(geometry.semantic_hash().unwrap(), expected);

    let mut contract = module;
    contract.contract.numeric_profile_hash = hash(0x55);
    assert_ne!(contract.semantic_hash().unwrap(), expected);
}

#[test]
fn strict_json_round_trip_sorts_records_and_retains_raw_fixed_values() {
    let (module, _, _) = fixture();
    let encoded = encode_module_artifact(&module).unwrap();
    let decoded = decode_module_artifact(&encoded).unwrap();

    assert_eq!(
        decoded.semantic_hash().unwrap(),
        module.semantic_hash().unwrap()
    );
    assert_eq!(encode_module_artifact(&decoded).unwrap(), encoded);
    assert_eq!(decoded.io_bindings[0].name, "in");
    assert_eq!(decoded.io_bindings[1].name, "out");
    assert_eq!(decoded.geometry.wires[0].points[0].x.0, 16_384);
    assert_eq!(decoded.geometry.wires[0].points[1].x.0, 65_536);
    assert!(encoded.ends_with('\n'));
    assert!(encoded.contains(&format!("\"formatVersion\": {MODULE_FORMAT_VERSION_V1}")));
    assert!(encoded.find("\"name\": \"in\"").unwrap() < encoded.find("\"name\": \"out\"").unwrap());
}

#[test]
fn retained_v1_fixture_exactly_reencodes_and_matches_the_hash_golden() {
    let source = include_str!("../../../fixtures/modules/s1m0-absolute-geometry-v1.json");
    let module = decode_module_artifact(source).unwrap();

    assert_eq!(encode_module_artifact(&module).unwrap(), source);
    assert_eq!(
        module.semantic_hash().unwrap().to_string(),
        MODULE_HASH_GOLDEN
    );
}

#[test]
fn independent_module_encoder_matches_retained_literal_golden() {
    let source = include_str!("../../../fixtures/modules/s1m0-absolute-geometry-v1.json");
    let module = decode_module_artifact(source).unwrap();

    let independently_encoded_hash = independent_module_hash(&module);

    assert_eq!(independently_encoded_hash, MODULE_HASH_GOLDEN);
    assert_eq!(
        module.semantic_hash().unwrap().to_string(),
        MODULE_HASH_GOLDEN
    );
}

#[test]
fn primitive_input_record_permutations_have_one_hash_and_canonical_json() {
    let (mut ordered, _, _) = fixture();
    ordered.geometry.substrates.push(SubstrateBlueprint {
        id: id(5),
        origin: point(262_144, 0),
        routing_area: aabb(-65_536, -65_536, 65_536, 65_536),
        footprint: aabb(-131_072, -131_072, 131_072, 131_072),
    });
    ordered.geometry.gates.push(GateBlueprint {
        id: id(6),
        substrate: id(5),
        gate_type: GateType::Not,
        origin: point(262_144, 0),
    });
    ordered.geometry.junctions.push(JunctionBlueprint {
        id: id(7),
        routing_domain: ModuleRoutingDomain::OpenWorld,
        position: point(0, 131_072),
    });
    ordered.geometry.wires.push(WireBlueprint {
        id: id(8),
        routing_domain: ModuleRoutingDomain::OpenWorld,
        points: vec![point(0, 131_072), point(65_536, 131_072)],
        endpoint_a: ModuleEndpoint::Junction(id(7)),
        endpoint_b: ModuleEndpoint::Free,
    });

    let mut permuted = ordered.clone();
    permuted.geometry.substrates.reverse();
    permuted.geometry.gates.reverse();
    permuted.geometry.junctions.reverse();
    permuted.geometry.wires.reverse();
    permuted.io_bindings.reverse();

    assert_eq!(
        permuted.semantic_hash().unwrap(),
        ordered.semantic_hash().unwrap()
    );
    assert_eq!(
        encode_module_artifact(&permuted).unwrap(),
        encode_module_artifact(&ordered).unwrap()
    );
}

#[test]
fn every_primitive_record_component_is_hash_sensitive() {
    let (base, _, _) = fixture();
    let expected = base.semantic_hash().unwrap();
    let mutations: &[fn(&mut ModuleBlueprint)] = &[
        |module| module.geometry.substrates[0].origin.x = Fixed(1),
        |module| module.geometry.substrates[0].routing_area.min.x = Fixed(-49_152),
        |module| module.geometry.substrates[0].footprint.max.y = Fixed(114_688),
        |module| module.geometry.gates[0].gate_type = GateType::Or,
        |module| module.geometry.gates[0].origin.y = Fixed(16_384),
        |module| module.geometry.junctions[0].routing_domain = ModuleRoutingDomain::OpenWorld,
        |module| module.geometry.junctions[0].position.y = Fixed(16_384),
        |module| module.geometry.wires[0].routing_domain = ModuleRoutingDomain::OpenWorld,
        |module| module.geometry.wires[0].points[0].y = Fixed(1_024),
        |module| module.geometry.wires[0].points.reverse(),
        |module| module.geometry.wires[0].points.insert(1, point(32_768, 0)),
        |module| module.geometry.wires[0].endpoint_a = ModuleEndpoint::Free,
        |module| {
            module.geometry.wires[0].endpoint_a = ModuleEndpoint::GatePort {
                gate: id(2),
                port: GatePort::InputA,
            };
        },
        |module| module.geometry.wires[0].endpoint_b = ModuleEndpoint::Free,
        |module| module.io_bindings[0].name.push('2'),
        |module| {
            module.io_bindings[0].endpoint = ModuleEndpoint::GatePort {
                gate: id(2),
                port: GatePort::InputA,
            };
        },
        |module| {
            module.geometry.substrates[0].id = id(9);
            module.geometry.gates[0].substrate = id(9);
            module.geometry.junctions[0].routing_domain = ModuleRoutingDomain::Substrate(id(9));
            module.geometry.wires[0].routing_domain = ModuleRoutingDomain::Substrate(id(9));
        },
        |module| {
            module.geometry.gates[0].id = id(9);
            module.geometry.wires[0].endpoint_a = ModuleEndpoint::GatePort {
                gate: id(9),
                port: GatePort::Output,
            };
            module.io_bindings[1].endpoint = ModuleEndpoint::GatePort {
                gate: id(9),
                port: GatePort::InputA,
            };
        },
        |module| {
            module.geometry.junctions[0].id = id(9);
            module.geometry.wires[0].endpoint_b = ModuleEndpoint::Junction(id(9));
            module.io_bindings[0].endpoint = ModuleEndpoint::Junction(id(9));
        },
        |module| module.geometry.wires[0].id = id(9),
    ];

    for (index, mutate) in mutations.iter().enumerate() {
        let mut changed = base.clone();
        mutate(&mut changed);
        assert_ne!(
            changed.semantic_hash().unwrap(),
            expected,
            "primitive mutation {index} did not change the semantic hash"
        );
    }

    let mut second_substrate = base.clone();
    second_substrate
        .geometry
        .substrates
        .push(SubstrateBlueprint {
            id: id(5),
            origin: point(262_144, 0),
            routing_area: aabb(-65_536, -65_536, 65_536, 65_536),
            footprint: aabb(-131_072, -131_072, 131_072, 131_072),
        });
    let second_substrate_hash = second_substrate.semantic_hash().unwrap();
    second_substrate.geometry.gates[0].substrate = id(5);
    assert_ne!(
        second_substrate.semantic_hash().unwrap(),
        second_substrate_hash,
        "gate substrate reference did not change the semantic hash"
    );
}

#[test]
fn strict_json_rejects_unknown_duplicate_trailing_and_floating_geometry() {
    let (module, _, _) = fixture();
    let encoded = encode_module_artifact(&module).unwrap();

    let unknown = encoded.replacen(
        &format!("\"formatVersion\": {MODULE_FORMAT_VERSION_V1}"),
        &format!("\"formatVersion\": {MODULE_FORMAT_VERSION_V1},\n  \"unknown\": true"),
        1,
    );
    assert!(matches!(
        decode_module_artifact(&unknown),
        Err(ModuleError::InvalidJson { .. })
    ));

    let duplicate = encoded.replacen(
        &format!("\"formatVersion\": {MODULE_FORMAT_VERSION_V1}"),
        &format!(
            "\"formatVersion\": {MODULE_FORMAT_VERSION_V1},\n  \"formatVersion\": {MODULE_FORMAT_VERSION_V1}"
        ),
        1,
    );
    assert!(matches!(
        decode_module_artifact(&duplicate),
        Err(ModuleError::InvalidJson { .. })
    ));

    assert!(matches!(
        decode_module_artifact(&(encoded.clone() + "{}")),
        Err(ModuleError::InvalidJson { .. })
    ));

    let floating = encoded.replacen("\"x\": 16384", "\"x\": 16384.0", 1);
    assert!(matches!(
        decode_module_artifact(&floating),
        Err(ModuleError::InvalidJson { .. })
    ));
}

#[test]
fn strict_json_rejects_unsupported_contract_and_malformed_hash_text() {
    let (module, _, _) = fixture();
    let encoded = encode_module_artifact(&module).unwrap();

    let unsupported_format = encoded.replacen(
        &format!("\"formatVersion\": {MODULE_FORMAT_VERSION_V1}"),
        "\"formatVersion\": 2",
        1,
    );
    assert_eq!(
        decode_module_artifact(&unsupported_format),
        Err(ModuleError::UnsupportedFormatVersion {
            expected: MODULE_FORMAT_VERSION_V1,
            actual: 2,
        })
    );

    let unsupported_algorithm = encoded.replacen(
        "\"hashAlgorithmId\": \"blake3-v1\"",
        "\"hashAlgorithmId\": \"sha256-v1\"",
        1,
    );
    assert!(matches!(
        decode_module_artifact(&unsupported_algorithm),
        Err(ModuleError::UnsupportedHashAlgorithm { actual }) if actual == "sha256-v1"
    ));

    let unsupported_semantics = encoded.replacen(
        "\"semanticsVersion\": \"aon-semantics-v1\"",
        "\"semanticsVersion\": \"aon-semantics-v2\"",
        1,
    );
    assert!(matches!(
        decode_module_artifact(&unsupported_semantics),
        Err(ModuleError::UnsupportedSemanticsVersion { actual }) if actual == "aon-semantics-v2"
    ));

    let malformed_hash = encoded.replacen(
        &format!("\"numericProfileHash\": \"{}\"", hash(0x11)),
        "\"numericProfileHash\": \"ABC\"",
        1,
    );
    assert!(matches!(
        decode_module_artifact(&malformed_hash),
        Err(ModuleError::InvalidHash {
            field: "contract.numericProfileHash",
            ..
        })
    ));
}

#[test]
fn duplicate_and_wrong_kind_references_are_rejected() {
    let (mut duplicate, _, _) = fixture();
    duplicate.geometry.junctions[0].id = id(2);
    assert!(matches!(
        duplicate.semantic_hash(),
        Err(ModuleError::DuplicateLocalId { id: duplicate }) if duplicate == id(2)
    ));

    let (mut wrong_kind, _, _) = fixture();
    wrong_kind.geometry.wires[0].endpoint_b = ModuleEndpoint::Junction(id(2));
    assert!(matches!(
        wrong_kind.semantic_hash(),
        Err(ModuleError::WrongKindReference {
            id: wrong,
            expected: "junction"
        }) if wrong == id(2)
    ));
}

#[test]
fn geometry_validation_covers_quantization_bounds_and_polyline_shape() {
    let (mut module, contract, physical) = fixture();
    module.geometry.gates[0].origin.x = Fixed(1);
    assert!(matches!(
        validate_module_against(&module, &contract, &physical),
        Err(ModuleError::NotQuantized { id: bad }) if bad == id(2)
    ));

    let (mut module, contract, physical) = fixture();
    module.geometry.substrates[0].routing_area.max.x = Fixed(262_144);
    assert!(matches!(
        validate_module_against(&module, &contract, &physical),
        Err(ModuleError::InvalidSubstrateBounds { id: bad }) if bad == id(1)
    ));

    let (mut module, contract, physical) = fixture();
    module.geometry.wires[0].points.truncate(1);
    assert!(matches!(
        validate_module_against(&module, &contract, &physical),
        Err(ModuleError::InvalidPolyline { id: bad }) if bad == id(4)
    ));
}

#[test]
fn wire_endpoints_need_quantum_and_bounds_but_not_routing_pitch() {
    let (mut module, contract, physical) = fixture();
    module.geometry.wires.push(WireBlueprint {
        id: id(5),
        routing_domain: ModuleRoutingDomain::OpenWorld,
        points: vec![point(1_024, 131_072), point(3_072, 131_072)],
        endpoint_a: ModuleEndpoint::Free,
        endpoint_b: ModuleEndpoint::Free,
    });

    validate_module_against(&module, &contract, &physical).unwrap();
}

#[test]
fn module_rejects_self_and_inter_wire_overlap_and_spacing() {
    let (mut self_overlap, contract, physical) = geometry_only_fixture();
    self_overlap.geometry.wires.push(free_wire(
        4,
        ModuleRoutingDomain::OpenWorld,
        vec![
            point(0, 4 * WORLD_PITCH),
            point(2 * WORLD_PITCH, 4 * WORLD_PITCH),
            point(WORLD_PITCH, 4 * WORLD_PITCH),
        ],
    ));
    let before = self_overlap.clone();
    assert!(matches!(
        validate_module_against(&self_overlap, &contract, &physical),
        Err(ModuleError::GeometryOverlap { first, second })
            if first == id(4) && second == id(4)
    ));
    assert_eq!(self_overlap, before);

    let (mut inter_overlap, contract, physical) = geometry_only_fixture();
    inter_overlap.geometry.wires.extend([
        free_wire(
            4,
            ModuleRoutingDomain::OpenWorld,
            vec![
                point(0, 4 * WORLD_PITCH),
                point(2 * WORLD_PITCH, 4 * WORLD_PITCH),
            ],
        ),
        free_wire(
            5,
            ModuleRoutingDomain::OpenWorld,
            vec![
                point(WORLD_PITCH, 4 * WORLD_PITCH),
                point(3 * WORLD_PITCH, 4 * WORLD_PITCH),
            ],
        ),
    ]);
    assert!(matches!(
        validate_module_against(&inter_overlap, &contract, &physical),
        Err(ModuleError::GeometryOverlap { first, second })
            if first == id(4) && second == id(5)
    ));

    let (mut inter_spacing, contract, physical) = geometry_only_fixture();
    inter_spacing.geometry.wires.extend([
        free_wire(
            4,
            ModuleRoutingDomain::OpenWorld,
            vec![
                point(0, 4 * WORLD_PITCH),
                point(2 * WORLD_PITCH, 4 * WORLD_PITCH),
            ],
        ),
        free_wire(
            5,
            ModuleRoutingDomain::OpenWorld,
            vec![
                point(0, 4 * WORLD_PITCH + QUANTUM),
                point(2 * WORLD_PITCH, 4 * WORLD_PITCH + QUANTUM),
            ],
        ),
    ]);
    assert!(matches!(
        validate_module_against(&inter_spacing, &contract, &physical),
        Err(ModuleError::InsufficientWireSpacing { first, second })
            if first == id(4) && second == id(5)
    ));

    let (mut self_spacing, contract, physical) = geometry_only_fixture();
    self_spacing.geometry.wires.push(free_wire(
        4,
        ModuleRoutingDomain::OpenWorld,
        vec![
            point(0, QUANTUM),
            point(WORLD_PITCH, 0),
            point(2 * WORLD_PITCH, 0),
            point(WORLD_PITCH, QUANTUM),
        ],
    ));
    assert!(matches!(
        validate_module_against(&self_spacing, &contract, &physical),
        Err(ModuleError::InsufficientWireSpacing { first, second })
            if first == id(4) && second == id(4)
    ));
}

#[test]
fn module_rejects_junction_in_wire_interior_and_invalid_gate_contacts() {
    let (mut junction_interior, contract, physical) = geometry_only_fixture();
    junction_interior
        .geometry
        .junctions
        .push(JunctionBlueprint {
            id: id(3),
            routing_domain: ModuleRoutingDomain::OpenWorld,
            position: point(0, 4 * WORLD_PITCH),
        });
    junction_interior.geometry.wires.push(free_wire(
        4,
        ModuleRoutingDomain::OpenWorld,
        vec![
            point(-WORLD_PITCH, 4 * WORLD_PITCH),
            point(WORLD_PITCH, 4 * WORLD_PITCH),
        ],
    ));
    assert!(matches!(
        validate_module_against(&junction_interior, &contract, &physical),
        Err(ModuleError::JunctionOnWireInterior { junction, wire })
            if junction == id(3) && wire == id(4)
    ));

    let (mut gate_interior, contract, physical) = fixture();
    gate_interior.geometry.junctions.clear();
    gate_interior.io_bindings.clear();
    gate_interior.geometry.wires = vec![free_wire(
        4,
        ModuleRoutingDomain::Substrate(id(1)),
        vec![point(-2 * CIRCUIT_PITCH, 0), point(2 * CIRCUIT_PITCH, 0)],
    )];
    let before = gate_interior.clone();
    assert!(matches!(
        validate_module_against(&gate_interior, &contract, &physical),
        Err(ModuleError::GeometryOverlap { first, second })
            if first == id(4) && second == id(2)
    ));
    assert_eq!(gate_interior, before);

    let (mut invalid_boundary, contract, physical) = fixture();
    invalid_boundary.geometry.junctions.clear();
    invalid_boundary.io_bindings.clear();
    invalid_boundary.geometry.wires = vec![free_wire(
        4,
        ModuleRoutingDomain::Substrate(id(1)),
        vec![
            point(-2 * CIRCUIT_PITCH, CIRCUIT_PITCH),
            point(-CIRCUIT_PITCH, CIRCUIT_PITCH),
        ],
    )];
    assert!(matches!(
        validate_module_against(&invalid_boundary, &contract, &physical),
        Err(ModuleError::InvalidGateContact { wire, gate })
            if wire == id(4) && gate == id(2)
    ));

    let (mut same_junction, contract, physical) = geometry_only_fixture();
    same_junction.geometry.junctions.push(JunctionBlueprint {
        id: id(3),
        routing_domain: ModuleRoutingDomain::OpenWorld,
        position: point(0, 4 * WORLD_PITCH),
    });
    same_junction.geometry.wires.push(WireBlueprint {
        id: id(4),
        routing_domain: ModuleRoutingDomain::OpenWorld,
        points: vec![
            point(0, 4 * WORLD_PITCH),
            point(WORLD_PITCH, 5 * WORLD_PITCH),
            point(-WORLD_PITCH, 5 * WORLD_PITCH),
            point(0, 4 * WORLD_PITCH),
        ],
        endpoint_a: ModuleEndpoint::Junction(id(3)),
        endpoint_b: ModuleEndpoint::Junction(id(3)),
    });
    assert!(matches!(
        validate_module_against(&same_junction, &contract, &physical),
        Err(ModuleError::SameJunctionEndpoints { wire, junction })
            if wire == id(4) && junction == id(3)
    ));
}

#[test]
fn module_allows_crossings_shared_physical_endpoints_and_profile_anchors() {
    let (profile_anchor, contract, physical) = fixture();
    let before = profile_anchor.clone();
    validate_module_against(&profile_anchor, &contract, &physical).unwrap();
    assert_eq!(profile_anchor, before);

    let (mut crossing, contract, physical) = geometry_only_fixture();
    crossing.geometry.wires.extend([
        free_wire(
            4,
            ModuleRoutingDomain::OpenWorld,
            vec![
                point(-WORLD_PITCH, 4 * WORLD_PITCH),
                point(WORLD_PITCH, 4 * WORLD_PITCH),
            ],
        ),
        free_wire(
            5,
            ModuleRoutingDomain::OpenWorld,
            vec![point(0, 3 * WORLD_PITCH), point(0, 5 * WORLD_PITCH)],
        ),
    ]);
    validate_module_against(&crossing, &contract, &physical).unwrap();

    let (mut shared_endpoint, contract, physical) = geometry_only_fixture();
    shared_endpoint.geometry.wires.extend([
        free_wire(
            4,
            ModuleRoutingDomain::OpenWorld,
            vec![
                point(0, 4 * WORLD_PITCH),
                point(WORLD_PITCH, 5 * WORLD_PITCH),
            ],
        ),
        free_wire(
            5,
            ModuleRoutingDomain::OpenWorld,
            vec![
                point(0, 4 * WORLD_PITCH),
                point(-WORLD_PITCH, 3 * WORLD_PITCH),
            ],
        ),
    ]);
    validate_module_against(&shared_endpoint, &contract, &physical).unwrap();
}

#[test]
fn module_geometry_error_precedence_is_overflow_then_quantum_then_pitch() {
    let (mut overflow, contract, physical) = geometry_only_fixture();
    overflow.geometry.wires.push(free_wire(
        4,
        ModuleRoutingDomain::OpenWorld,
        vec![point(i64::MIN, 0), point(i64::MAX, 0)],
    ));
    let before = overflow.clone();
    assert!(matches!(
        validate_module_against(&overflow, &contract, &physical),
        Err(ModuleError::Numeric(NumericError::Overflow))
    ));
    assert_eq!(overflow, before);

    let (mut quantum, contract, physical) = geometry_only_fixture();
    quantum.geometry.wires.push(free_wire(
        4,
        ModuleRoutingDomain::OpenWorld,
        vec![
            point(1, 4 * WORLD_PITCH),
            point(WORLD_PITCH, 4 * WORLD_PITCH),
        ],
    ));
    assert!(matches!(
        validate_module_against(&quantum, &contract, &physical),
        Err(ModuleError::NotQuantized { id: bad }) if bad == id(4)
    ));

    let (mut pitch, contract, physical) = geometry_only_fixture();
    pitch.geometry.wires.push(free_wire(
        4,
        ModuleRoutingDomain::Substrate(id(1)),
        vec![
            point(0, 0),
            point(5 * CIRCUIT_PITCH + QUANTUM, 0),
            point(CIRCUIT_PITCH, 0),
        ],
    ));
    assert!(matches!(
        validate_module_against(&pitch, &contract, &physical),
        Err(ModuleError::InvalidRoutingPitch { id: bad }) if bad == id(4)
    ));
}

#[test]
fn module_shape_and_reference_errors_precede_contract_mismatches() {
    let (mut duplicate, mut contract, physical) = fixture();
    duplicate.geometry.junctions[0].id = id(2);
    contract.numeric_profile_hash = hash(0x99);
    assert!(matches!(
        validate_module_against(&duplicate, &contract, &physical),
        Err(ModuleError::DuplicateLocalId { id: duplicate }) if duplicate == id(2)
    ));

    let (mut dangling, mut contract, physical) = fixture();
    dangling.geometry.wires[0].endpoint_b = ModuleEndpoint::Junction(id(99));
    contract.numeric_profile_hash = hash(0x99);
    assert!(matches!(
        validate_module_against(&dangling, &contract, &physical),
        Err(ModuleError::DanglingReference { id: dangling }) if dangling == id(99)
    ));

    let (mut short_wire, mut contract, physical) = fixture();
    short_wire.geometry.wires[0].points.truncate(1);
    contract.numeric_profile_hash = hash(0x99);
    assert!(matches!(
        validate_module_against(&short_wire, &contract, &physical),
        Err(ModuleError::InvalidPolyline { id: wire }) if wire == id(4)
    ));
}

#[test]
fn module_arithmetic_overflow_precedes_empty_aabb_bounds() {
    let (mut module, contract, physical) = geometry_only_fixture();
    module.geometry.substrates[0].routing_area = aabb(0, 0, QUANTUM, 0);
    module.geometry.substrates[0].footprint = aabb(0, 0, QUANTUM, 0);
    module.geometry.substrates[0].origin = point(i64::MAX, 0);

    assert!(matches!(
        validate_module_against(&module, &contract, &physical),
        Err(ModuleError::Numeric(NumericError::Overflow))
    ));
}
