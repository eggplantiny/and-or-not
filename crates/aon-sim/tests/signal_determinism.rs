use aon_sim::{
    BalanceProfile, Command, CommandAcceptance, CommandEnvelope, CommandRejection,
    CommandRejectionReason, DriveStrength, DriverId, EntityId, FIXED_ONE, Fixed, FixedAabb,
    FixedVec2, GateId, GateSignalPorts, GateType, InitialWorld, LogicLevel, NumericProfile,
    PhysicalScaleProfile, PlaceFixedSubstrateCommand, PlaceGateCommand, PlaceJunctionCommand,
    ProfileBundle, RemoveEntityCommand, RoutingDomain, SetExternalDriverCommand, Simulation,
    SimulationContract, SimulationPackage, StageFeatureSet, Tick,
};

const CIRCUIT_PITCH: i64 = 16_384;
const SUBSTRATE_HALF_EXTENT: i64 = 16 * FIXED_ONE;

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

fn simulation() -> Simulation {
    let mut balance = BalanceProfile::stage0_alpha("signal-determinism");
    balance.fanout_free_load = 1_000;
    let profiles = ProfileBundle {
        numeric: NumericProfile::reference_v1("signal-determinism"),
        physical_scale: PhysicalScaleProfile::stage0_alpha("signal-determinism"),
        balance,
    };
    let contract = SimulationContract::from_profiles(&profiles).expect("profiles are valid");
    let mut required_features = StageFeatureSet::none();
    required_features.signal = true;
    Simulation::new(SimulationPackage::new(
        "signal-determinism",
        InitialWorld::Empty,
        required_features,
        contract,
        profiles,
    ))
    .expect("the signal determinism simulation starts")
}

fn envelope(simulation: &Simulation, ordinal: u64, command: Command) -> CommandEnvelope {
    CommandEnvelope {
        target_tick: simulation.next_tick(),
        ordinal,
        command,
    }
}

fn substrate_command() -> Command {
    let bounds = FixedAabb::new(
        point(-SUBSTRATE_HALF_EXTENT, -SUBSTRATE_HALF_EXTENT),
        point(SUBSTRATE_HALF_EXTENT, SUBSTRATE_HALF_EXTENT),
    );
    Command::PlaceFixedSubstrate(PlaceFixedSubstrateCommand {
        origin: point(0, 0),
        routing_area: bounds,
        footprint: bounds,
    })
}

fn gate_command(routing_domain: RoutingDomain, gate_type: GateType, origin: FixedVec2) -> Command {
    Command::PlaceGate(PlaceGateCommand {
        gate_type,
        origin,
        routing_domain,
    })
}

fn junction_command(routing_domain: RoutingDomain, position: FixedVec2) -> Command {
    Command::PlaceJunction(PlaceJunctionCommand {
        routing_domain,
        position,
    })
}

fn set_external(driver: DriverId, level: LogicLevel, strength: u64) -> Command {
    Command::SetExternalDriver(SetExternalDriverCommand {
        driver,
        level,
        strength: DriveStrength(strength),
    })
}

fn expect_created(simulation: &mut Simulation, command: Command) -> EntityId {
    let target_tick = simulation.next_tick();
    let report = simulation
        .step(&[CommandEnvelope {
            target_tick,
            ordinal: 0,
            command,
        }])
        .expect("the fixture placement is valid");
    assert!(report.command_rejections.is_empty());
    report.command_acceptances[0]
        .created_entity
        .expect("the placement creates an entity")
}

fn expect_rejected(simulation: &mut Simulation, driver: DriverId, reason: CommandRejectionReason) {
    let target_tick = simulation.next_tick();
    let report = simulation
        .step(&[CommandEnvelope {
            target_tick,
            ordinal: 0,
            command: set_external(driver, LogicLevel::High, 100),
        }])
        .expect("an invalid Driver is an ordinary command rejection");
    assert!(report.command_acceptances.is_empty());
    assert_eq!(
        report.command_rejections,
        vec![CommandRejection {
            target_tick,
            ordinal: 0,
            reason,
        }]
    );
}

#[test]
fn endpoint_ids_are_independent_monotonic_tombstones_with_stable_rejections() {
    let mut simulation = simulation();
    let substrate = expect_created(&mut simulation, substrate_command());
    let domain = RoutingDomain::FixedSubstrate(substrate);

    // Structural allocations do not advance either endpoint namespace.
    assert_eq!(
        expect_created(
            &mut simulation,
            junction_command(domain, point(-4 * CIRCUIT_PITCH, 8 * CIRCUIT_PITCH)),
        ),
        EntityId(2)
    );
    assert_eq!(
        expect_created(
            &mut simulation,
            junction_command(domain, point(4 * CIRCUIT_PITCH, 8 * CIRCUIT_PITCH)),
        ),
        EntityId(3)
    );
    let first_gate = GateId(expect_created(
        &mut simulation,
        gate_command(domain, GateType::And, point(-4 * CIRCUIT_PITCH, 0)),
    ));
    assert_eq!(first_gate.entity_id(), EntityId(4));

    let first = simulation
        .gate_signal_ports(first_gate)
        .expect("the first Gate exposes its endpoints");
    let first_b = first.input_b.expect("AND has Input B");
    assert_eq!(first.input_a.external_driver, DriverId(EntityId(1)));
    assert_eq!(first_b.external_driver, DriverId(EntityId(2)));
    assert_eq!(first.output, DriverId(EntityId(3)));
    assert_eq!(first.input_a.sink.entity_id(), EntityId(1));
    assert_eq!(first_b.sink.entity_id(), EntityId(2));

    expect_rejected(
        &mut simulation,
        first.output,
        CommandRejectionReason::InvalidDriverKind,
    );
    expect_rejected(
        &mut simulation,
        DriverId(EntityId(4)),
        CommandRejectionReason::UnknownDriver,
    );

    let removal_tick = simulation.next_tick();
    let removed = simulation
        .step(&[CommandEnvelope {
            target_tick: removal_tick,
            ordinal: 0,
            command: Command::RemoveEntity(RemoveEntityCommand {
                target: first_gate.entity_id(),
            }),
        }])
        .expect("the first Gate can be removed");
    assert_eq!(
        removed.command_acceptances,
        vec![CommandAcceptance {
            target_tick: removal_tick,
            ordinal: 0,
            created_entity: None,
        }]
    );
    assert_eq!(simulation.gate_signal_ports(first_gate), None);
    assert_eq!(
        simulation.driver_sample(first.input_a.external_driver),
        None
    );
    assert_eq!(simulation.sink_level(first.input_a.sink), None);

    let second_gate = GateId(expect_created(
        &mut simulation,
        gate_command(domain, GateType::Not, point(4 * CIRCUIT_PITCH, 0)),
    ));
    let second = simulation
        .gate_signal_ports(second_gate)
        .expect("the replacement Gate exposes fresh endpoints");
    assert_eq!(second.input_a.external_driver, DriverId(EntityId(4)));
    assert_eq!(second.output, DriverId(EntityId(5)));
    assert_eq!(second.input_a.sink.entity_id(), EntityId(3));
    assert!(second.input_b.is_none());

    expect_rejected(
        &mut simulation,
        first.input_a.external_driver,
        CommandRejectionReason::RemovedDriver,
    );
}

#[test]
fn predicted_driver_rejects_then_observed_driver_accepts_noop_and_ordinal_last() {
    let mut simulation = simulation();
    let substrate = expect_created(&mut simulation, substrate_command());
    let domain = RoutingDomain::FixedSubstrate(substrate);
    let target_tick = simulation.next_tick();
    let report = simulation
        .step(&[
            CommandEnvelope {
                target_tick,
                ordinal: 10,
                command: gate_command(domain, GateType::And, point(0, 0)),
            },
            CommandEnvelope {
                target_tick,
                ordinal: 20,
                command: set_external(DriverId(EntityId(1)), LogicLevel::High, 100),
            },
        ])
        .expect("a predicted same-batch Driver is an ordinary rejection");
    assert_eq!(
        report.command_acceptances,
        vec![CommandAcceptance {
            target_tick,
            ordinal: 10,
            created_entity: Some(EntityId(2)),
        }]
    );
    assert_eq!(
        report.command_rejections,
        vec![CommandRejection {
            target_tick,
            ordinal: 20,
            reason: CommandRejectionReason::UnknownDriver,
        }]
    );

    let gate = GateId(EntityId(2));
    let ports = simulation
        .gate_signal_ports(gate)
        .expect("accepted placement publishes the real Driver IDs");
    assert_eq!(ports.input_a.external_driver, DriverId(EntityId(1)));

    let next_tick = simulation.next_tick();
    let accepted = simulation
        .step(&[CommandEnvelope {
            target_tick: next_tick,
            ordinal: 0,
            command: set_external(ports.input_a.external_driver, LogicLevel::High, 100),
        }])
        .expect("the observed Driver is accepted on the next Tick");
    assert_eq!(accepted.command_acceptances.len(), 1);
    assert!(accepted.command_rejections.is_empty());
    assert_eq!(
        simulation
            .driver_sample(ports.input_a.external_driver)
            .expect("the observed Driver remains live")
            .level,
        LogicLevel::High
    );

    let no_op = simulation
        .step(&[envelope(
            &simulation,
            0,
            set_external(ports.input_a.external_driver, LogicLevel::High, 100),
        )])
        .expect("setting the active sample again is accepted");
    assert_eq!(no_op.command_acceptances.len(), 1);
    assert!(no_op.command_rejections.is_empty());
    assert!(no_op.driver_changes.is_empty());
    assert_eq!(no_op.signal_counters.driver_transitions_applied, 0);
    assert_eq!(no_op.signal_counters.signal_arrivals_applied, 0);
    assert_eq!(no_op.signal_counters.sinks_resolved, 0);

    let coalesce_tick = simulation.next_tick();
    let coalesced = simulation
        .step(&[
            CommandEnvelope {
                target_tick: coalesce_tick,
                ordinal: 9,
                command: set_external(ports.input_a.external_driver, LogicLevel::Low, 250),
            },
            CommandEnvelope {
                target_tick: coalesce_tick,
                ordinal: 3,
                command: set_external(ports.input_a.external_driver, LogicLevel::X, 75),
            },
            CommandEnvelope {
                target_tick: coalesce_tick,
                ordinal: 5,
                command: set_external(ports.input_a.external_driver, LogicLevel::High, 150),
            },
        ])
        .expect("same-Driver requests coalesce by ordinal");
    assert_eq!(coalesced.command_acceptances.len(), 3);
    assert!(coalesced.command_rejections.is_empty());
    assert_eq!(coalesced.driver_changes.len(), 1);
    let sample = simulation
        .driver_sample(ports.input_a.external_driver)
        .expect("the external Driver remains live");
    assert_eq!(sample.level, LogicLevel::Low);
    assert_eq!(sample.strength, DriveStrength(250));
}

fn assert_replica_observations(left: &Simulation, right: &Simulation, gates: &[GateId]) {
    assert_eq!(left.next_tick(), right.next_tick());
    assert_eq!(left.topology_revision(), right.topology_revision());
    assert_eq!(left.state_hash(), right.state_hash());
    for &gate in gates {
        let left_state = left.gate_signal_state(gate);
        let right_state = right.gate_signal_state(gate);
        assert_eq!(left_state, right_state);
        if let Some(GateSignalPorts {
            input_a,
            input_b,
            output,
        }) = left.gate_signal_ports(gate)
        {
            assert_eq!(
                left.driver_sample(input_a.external_driver),
                right.driver_sample(input_a.external_driver)
            );
            assert_eq!(
                left.sink_level(input_a.sink),
                right.sink_level(input_a.sink)
            );
            assert_eq!(left.driver_sample(output), right.driver_sample(output));
            if let Some(input_b) = input_b {
                assert_eq!(
                    left.driver_sample(input_b.external_driver),
                    right.driver_sample(input_b.external_driver)
                );
                assert_eq!(
                    left.sink_level(input_b.sink),
                    right.sink_level(input_b.sink)
                );
            }
        }
    }
}

fn step_replicas(
    left: &mut Simulation,
    right: &mut Simulation,
    commands: Vec<CommandEnvelope>,
    gates: &[GateId],
) {
    let mut reversed = commands.clone();
    reversed.reverse();
    let left_report = left
        .step(&commands)
        .expect("the forward replica Tick succeeds");
    let right_report = right
        .step(&reversed)
        .expect("the reversed replica Tick succeeds");
    assert_eq!(left_report, right_report);
    assert_eq!(left_report.state_hash, left.state_hash());
    assert_eq!(right_report.state_hash, right.state_hash());
    assert_replica_observations(left, right, gates);
}

#[test]
fn reversed_multi_command_batches_have_identical_reports_hashes_and_public_samples() {
    let mut left = simulation();
    let mut right = simulation();

    step_replicas(
        &mut left,
        &mut right,
        vec![CommandEnvelope {
            target_tick: Tick(0),
            ordinal: 0,
            command: substrate_command(),
        }],
        &[],
    );
    let domain = RoutingDomain::FixedSubstrate(EntityId(1));
    let target_tick = left.next_tick();
    let placement_commands = vec![
        CommandEnvelope {
            target_tick,
            ordinal: 10,
            command: gate_command(domain, GateType::Not, point(-4 * CIRCUIT_PITCH, 0)),
        },
        CommandEnvelope {
            target_tick,
            ordinal: 20,
            command: gate_command(domain, GateType::And, point(4 * CIRCUIT_PITCH, 0)),
        },
        CommandEnvelope {
            target_tick,
            ordinal: 30,
            command: junction_command(domain, point(0, 8 * CIRCUIT_PITCH)),
        },
    ];
    step_replicas(
        &mut left,
        &mut right,
        placement_commands,
        &[GateId(EntityId(2)), GateId(EntityId(3))],
    );

    let first = left.gate_signal_ports(GateId(EntityId(2))).unwrap();
    let second = left.gate_signal_ports(GateId(EntityId(3))).unwrap();
    let target_tick = left.next_tick();
    let initial_driver_commands = vec![
        CommandEnvelope {
            target_tick,
            ordinal: 20,
            command: set_external(first.input_a.external_driver, LogicLevel::High, 100),
        },
        CommandEnvelope {
            target_tick,
            ordinal: 10,
            command: set_external(second.input_a.external_driver, LogicLevel::X, 200),
        },
        CommandEnvelope {
            target_tick,
            ordinal: 30,
            command: set_external(
                second.input_b.unwrap().external_driver,
                LogicLevel::High,
                125,
            ),
        },
    ];
    step_replicas(
        &mut left,
        &mut right,
        initial_driver_commands,
        &[GateId(EntityId(2)), GateId(EntityId(3))],
    );
    let target_tick = left.next_tick();
    let coalesced_driver_commands = vec![
        CommandEnvelope {
            target_tick,
            ordinal: 9,
            command: set_external(first.input_a.external_driver, LogicLevel::Low, 250),
        },
        CommandEnvelope {
            target_tick,
            ordinal: 3,
            command: set_external(first.input_a.external_driver, LogicLevel::X, 75),
        },
        CommandEnvelope {
            target_tick,
            ordinal: 5,
            command: set_external(first.input_a.external_driver, LogicLevel::High, 150),
        },
        CommandEnvelope {
            target_tick,
            ordinal: 1,
            command: set_external(second.input_a.external_driver, LogicLevel::X, 200),
        },
    ];
    step_replicas(
        &mut left,
        &mut right,
        coalesced_driver_commands,
        &[GateId(EntityId(2)), GateId(EntityId(3))],
    );

    let first_sample = left
        .driver_sample(first.input_a.external_driver)
        .expect("the first public Driver remains live");
    assert_eq!(first_sample.level, LogicLevel::Low);
    assert_eq!(first_sample.strength, DriveStrength(250));
    assert_eq!(
        left.driver_sample(second.input_a.external_driver),
        right.driver_sample(second.input_a.external_driver)
    );
}
