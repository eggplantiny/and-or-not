use aon_sim::{
    BalanceProfile, Command, CommandEnvelope, ConstructionError, ConstructionSite,
    ConstructionSiteId, ConstructionSiteStore, ConstructionTarget, ConstructionWorkContribution,
    DemandKind, EndpointTarget, Energy, EntityId, FIXED_ONE, Fixed, FixedAabb, FixedVec2, GateType,
    MobileId, PlaceConstructionSiteCommand, PlaceFixedSubstrateCommand, PlaceGateCommand,
    PlaceJunctionCommand, PlaceWireCommand, PowerNodeKey, PowerRatio, RoutingDomain, Tick, WireId,
    apply_construction_work, construction_nominal_demand, grant_construction_work,
    required_construction_work, scale_work,
};

fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

fn probe() -> aon_sim::ConstructionProbeProfile {
    BalanceProfile::construction_contact_damage_alpha("s1m4")
        .construction_probe
        .expect("reference v5 Construction probe")
}

fn junction_target(x: i64) -> ConstructionTarget {
    ConstructionTarget::Junction {
        routing_domain: RoutingDomain::OpenWorld,
        position: point(x, 0),
    }
}

#[test]
fn all_target_kinds_use_exact_one_final_ceiling_work_laws() {
    let probe = probe();
    for (gate_type, expected) in [(GateType::And, 8), (GateType::Or, 8), (GateType::Not, 6)] {
        assert_eq!(
            required_construction_work(
                &ConstructionTarget::Gate {
                    gate_type,
                    origin: point(0, 0),
                    routing_domain: RoutingDomain::OpenWorld,
                },
                &probe,
            ),
            Ok(Energy(expected))
        );
    }
    assert_eq!(
        required_construction_work(&junction_target(0), &probe),
        Ok(Energy(4))
    );

    let short = ConstructionTarget::Wire {
        routing_domain: RoutingDomain::OpenWorld,
        points: vec![point(0, 0), point(FIXED_ONE, 0)],
        endpoint_a: EndpointTarget::Free,
        endpoint_b: EndpointTarget::Free,
    };
    let direct = ConstructionTarget::Wire {
        routing_domain: RoutingDomain::OpenWorld,
        points: vec![point(0, 0), point(FIXED_ONE + 1, 0)],
        endpoint_a: EndpointTarget::Free,
        endpoint_b: EndpointTarget::Free,
    };
    let redundant = ConstructionTarget::Wire {
        routing_domain: RoutingDomain::OpenWorld,
        points: vec![
            point(0, 0),
            point(FIXED_ONE / 2, 0),
            point(FIXED_ONE + 1, 0),
        ],
        endpoint_a: EndpointTarget::Free,
        endpoint_b: EndpointTarget::Free,
    };
    assert_eq!(
        (
            required_construction_work(&short, &probe),
            required_construction_work(&direct, &probe),
            required_construction_work(&redundant, &probe),
        ),
        (Ok(Energy(3)), Ok(Energy(4)), Ok(Energy(4)))
    );

    let substrate = ConstructionTarget::FixedSubstrate {
        origin: point(10, 20),
        routing_area: FixedAabb::new(point(0, 0), point(FIXED_ONE, FIXED_ONE)),
        footprint: FixedAabb::new(point(0, 0), point(FIXED_ONE, 2 * FIXED_ONE + 1)),
    };
    assert_eq!(
        required_construction_work(&substrate, &probe),
        Ok(Energy(3))
    );
}

#[test]
fn malformed_or_zero_work_inputs_fail_with_typed_errors() {
    let mut probe = probe();
    let wire = ConstructionTarget::Wire {
        routing_domain: RoutingDomain::OpenWorld,
        points: vec![point(0, 0), point(0, 0)],
        endpoint_a: EndpointTarget::Free,
        endpoint_b: EndpointTarget::Free,
    };
    assert_eq!(
        required_construction_work(&wire, &probe),
        Err(ConstructionError::NegativeLength { raw: 0 })
    );

    let substrate = ConstructionTarget::FixedSubstrate {
        origin: point(0, 0),
        routing_area: FixedAabb::new(point(0, 0), point(1, 1)),
        footprint: FixedAabb::new(point(2, 0), point(2, 1)),
    };
    assert_eq!(
        required_construction_work(&substrate, &probe),
        Err(ConstructionError::NonPositiveExtent {
            axis: "footprint.width",
            raw: 0,
        })
    );

    let extreme_wire = ConstructionTarget::Wire {
        routing_domain: RoutingDomain::OpenWorld,
        points: vec![point(i64::MIN, 0), point(i64::MAX, 0)],
        endpoint_a: EndpointTarget::Free,
        endpoint_b: EndpointTarget::Free,
    };
    assert_eq!(
        required_construction_work(&extreme_wire, &probe),
        Err(ConstructionError::ArithmeticOverflow)
    );

    probe.wire_endpoint_work = u64::MAX;
    let one_wu_wire = ConstructionTarget::Wire {
        routing_domain: RoutingDomain::OpenWorld,
        points: vec![point(0, 0), point(FIXED_ONE, 0)],
        endpoint_a: EndpointTarget::Free,
        endpoint_b: EndpointTarget::Free,
    };
    assert_eq!(
        required_construction_work(&one_wu_wire, &probe),
        Err(ConstructionError::WorkOutOfRange {
            value: u128::from(u64::MAX) + 1,
        })
    );
}

#[test]
fn construction_demand_is_mobile_owned_track_attached_and_grant_reuses_scale_work() {
    let probe = probe();
    let attachment = PowerNodeKey::WireOffset(WireId(EntityId(7)), Fixed(123));
    let demand = construction_nominal_demand(
        ConstructionSiteId(EntityId(40)),
        MobileId(EntityId(9)),
        attachment,
        &probe,
    )
    .expect("valid builder demand");
    assert_eq!(demand.owner(), EntityId(9));
    assert_eq!(demand.kind(), DemandKind::Construction);
    assert_eq!(demand.nominal(), Energy(8));
    assert_eq!(demand.node(), attachment);

    let half = PowerRatio::new(Fixed(FIXED_ONE / 2)).unwrap();
    assert_eq!(
        grant_construction_work(Energy(7), half),
        scale_work(Energy(7), half).map_err(ConstructionError::Power)
    );
    assert_eq!(grant_construction_work(Energy(7), half), Ok(Energy(4)));

    assert_eq!(
        construction_nominal_demand(
            ConstructionSiteId(EntityId(40)),
            MobileId(EntityId(9)),
            PowerNodeKey::WireBody(WireId(EntityId(7))),
            &probe,
        ),
        Err(ConstructionError::InvalidConstructionAttachment {
            builder: MobileId(EntityId(9)),
        })
    );
}

#[test]
fn site_store_sorts_tombstones_and_applies_multi_builder_work_atomically() {
    let mut sites = ConstructionSiteStore::new(vec![
        ConstructionSite {
            id: ConstructionSiteId(EntityId(9)),
            target: junction_target(9),
            required_work: Energy(10),
            completed_work: Energy(1),
            activation_ready: false,
        },
        ConstructionSite {
            id: ConstructionSiteId(EntityId(2)),
            target: junction_target(2),
            required_work: Energy(4),
            completed_work: Energy(0),
            activation_ready: false,
        },
    ])
    .unwrap();
    assert_eq!(
        sites.iter().map(|site| site.id).collect::<Vec<_>>(),
        vec![
            ConstructionSiteId(EntityId(2)),
            ConstructionSiteId(EntityId(9))
        ]
    );

    let results = apply_construction_work(
        &mut sites,
        &[
            ConstructionWorkContribution {
                site: ConstructionSiteId(EntityId(9)),
                builder: MobileId(EntityId(8)),
                granted_work: Energy(8),
            },
            ConstructionWorkContribution {
                site: ConstructionSiteId(EntityId(9)),
                builder: MobileId(EntityId(3)),
                granted_work: Energy(8),
            },
        ],
    )
    .unwrap();
    assert_eq!(
        results
            .iter()
            .map(|row| (row.builder, row.applied_work, row.completed_work))
            .collect::<Vec<_>>(),
        vec![
            (MobileId(EntityId(3)), Energy(8), Energy(9)),
            (MobileId(EntityId(8)), Energy(1), Energy(10)),
        ]
    );
    assert!(
        sites
            .get(ConstructionSiteId(EntityId(9)))
            .unwrap()
            .activation_ready
    );

    let before = sites.clone();
    assert_eq!(
        apply_construction_work(
            &mut sites,
            &[ConstructionWorkContribution {
                site: ConstructionSiteId(EntityId(99)),
                builder: MobileId(EntityId(1)),
                granted_work: Energy(1),
            }],
        ),
        Err(ConstructionError::UnknownSite {
            site: ConstructionSiteId(EntityId(99)),
        })
    );
    assert_eq!(sites, before);

    assert!(sites.remove(ConstructionSiteId(EntityId(2))).is_ok());
    assert_eq!(sites.get(ConstructionSiteId(EntityId(2))), None);
    assert_eq!(sites.len(), 1);
    assert_eq!(
        sites.insert(ConstructionSite {
            id: ConstructionSiteId(EntityId(2)),
            target: junction_target(12),
            required_work: Energy(4),
            completed_work: Energy(0),
            activation_ready: false,
        }),
        Err(ConstructionError::DuplicateSite {
            site: ConstructionSiteId(EntityId(2)),
        })
    );
}

#[test]
fn duplicate_contributions_are_rejected_independent_of_input_order() {
    let mut sites = ConstructionSiteStore::new(vec![ConstructionSite {
        id: ConstructionSiteId(EntityId(1)),
        target: junction_target(0),
        required_work: Energy(4),
        completed_work: Energy(0),
        activation_ready: false,
    }])
    .unwrap();
    let duplicate = ConstructionWorkContribution {
        site: ConstructionSiteId(EntityId(1)),
        builder: MobileId(EntityId(2)),
        granted_work: Energy(1),
    };
    assert_eq!(
        apply_construction_work(&mut sites, &[duplicate, duplicate]),
        Err(ConstructionError::DuplicateContribution {
            site: duplicate.site,
            builder: duplicate.builder,
        })
    );
    assert_eq!(sites.get(duplicate.site).unwrap().completed_work, Energy(0));
}

#[test]
fn canonical_store_rejects_incoherent_progress_and_duplicate_site_ids() {
    let id = ConstructionSiteId(EntityId(5));
    let invalid = ConstructionSite {
        id,
        target: junction_target(0),
        required_work: Energy(4),
        completed_work: Energy(4),
        activation_ready: false,
    };
    assert_eq!(
        ConstructionSiteStore::new(vec![invalid]),
        Err(ConstructionError::WorkOutOfRange { value: 4 })
    );

    let site = ConstructionSite {
        id,
        target: junction_target(0),
        required_work: Energy(4),
        completed_work: Energy(0),
        activation_ready: false,
    };
    assert_eq!(
        ConstructionSiteStore::new(vec![site.clone(), site]),
        Err(ConstructionError::DuplicateSite { site: id })
    );
}

#[test]
fn tag8_payload_reuses_each_direct_target_encoding_after_kind_tag() {
    let aabb = FixedAabb::new(point(-1, -2), point(3, 4));
    let pairs = vec![
        (
            ConstructionTarget::Gate {
                gate_type: GateType::Or,
                origin: point(5, 6),
                routing_domain: RoutingDomain::OpenWorld,
            },
            Command::PlaceGate(PlaceGateCommand {
                gate_type: GateType::Or,
                origin: point(5, 6),
                routing_domain: RoutingDomain::OpenWorld,
            }),
            0_u8,
        ),
        (
            ConstructionTarget::Wire {
                routing_domain: RoutingDomain::OpenWorld,
                points: vec![point(0, 0), point(1, 2)],
                endpoint_a: EndpointTarget::Free,
                endpoint_b: EndpointTarget::Free,
            },
            Command::PlaceWire(PlaceWireCommand {
                routing_domain: RoutingDomain::OpenWorld,
                points: vec![point(0, 0), point(1, 2)],
                endpoint_a: EndpointTarget::Free,
                endpoint_b: EndpointTarget::Free,
            }),
            1,
        ),
        (
            junction_target(7),
            Command::PlaceJunction(PlaceJunctionCommand {
                routing_domain: RoutingDomain::OpenWorld,
                position: point(7, 0),
            }),
            2,
        ),
        (
            ConstructionTarget::FixedSubstrate {
                origin: point(5, 6),
                routing_area: aabb,
                footprint: aabb,
            },
            Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
                origin: point(5, 6),
                routing_area: aabb,
                footprint: aabb,
            }),
            3,
        ),
    ];
    let body_offset = b"AON\0COMMAND\0V1\0".len() + 2 + 8 + 8;
    for (target, direct, target_kind) in pairs {
        let site_bytes = CommandEnvelope {
            target_tick: Tick(3),
            ordinal: 4,
            command: Command::PlaceConstructionSite(PlaceConstructionSiteCommand { target }),
        }
        .canonical_bytes()
        .unwrap();
        let direct_bytes = CommandEnvelope {
            target_tick: Tick(3),
            ordinal: 4,
            command: direct,
        }
        .canonical_bytes()
        .unwrap();
        assert_eq!(site_bytes[body_offset], 8);
        assert_eq!(site_bytes[body_offset + 1], target_kind);
        assert_eq!(
            &site_bytes[body_offset + 2..],
            &direct_bytes[body_offset + 1..]
        );
    }
}
