use aon_sim::{
    CanonicalPowerRoute, CompiledPowerTopology, DemandId, DemandKind, DriveStrength, Energy,
    EntityId, FIXED_ONE, Fixed, FixedVec2, GateId, HeatEnergy, PowerBodyEdge, PowerDemand,
    PowerLoadAttachment, PowerLossCoefficient, PowerNodeKey, PowerPathToken, PowerRatio,
    PowerRegionId, PowerRouteKey, PowerRouteWire, PowerSourceAttachment, PowerSourceId,
    PowerSourceState, PowerTopologyInput, Tick, WireEnd, WireId, brownout_gate_delay,
    distribute_transmission_heat, scale_drive, scale_movement, scale_work, solve_power_region,
    transmission_loss,
};

const REGION: PowerRegionId = PowerRegionId(7);

const fn wire(id: u64) -> WireId {
    WireId(EntityId(id))
}

const fn source_id(id: u64) -> PowerSourceId {
    PowerSourceId(EntityId(id))
}

fn source(id: u64, generation: u64) -> PowerSourceState {
    PowerSourceState::new(
        source_id(id),
        FixedVec2::new(Fixed(id as i64), Fixed::ZERO),
        Energy(generation),
    )
}

fn route(source: u64, rows: &[(u64, i64, u32)]) -> CanonicalPowerRoute {
    let total_length = rows
        .iter()
        .try_fold(0_i64, |sum, (_, length, _)| sum.checked_add(*length))
        .expect("fixture route length fits");
    let segment_count = rows
        .iter()
        .try_fold(0_u32, |sum, (_, _, count)| sum.checked_add(*count))
        .expect("fixture segment count fits");
    let mut tokens = rows
        .iter()
        .map(|(id, _, _)| PowerPathToken::new(3, EntityId(*id), 0))
        .collect::<Vec<_>>();
    tokens.push(PowerPathToken::new(7, EntityId(source), 0));
    CanonicalPowerRoute::new(
        source_id(source),
        PowerRouteKey::new(Fixed(total_length), segment_count, tokens)
            .expect("fixture route key is valid"),
        rows.iter()
            .map(|(id, length, count)| {
                PowerRouteWire::new(wire(*id), Fixed(*length), *count)
                    .expect("fixture route row is valid")
            })
            .collect(),
    )
    .expect("fixture canonical route is valid")
}

fn connected_demand(
    owner: u64,
    kind: DemandKind,
    nominal: u64,
    source: u64,
    route_wire: u64,
) -> PowerDemand {
    PowerDemand::new(
        EntityId(owner),
        kind,
        REGION,
        Energy(nominal),
        Some(route(source, &[(route_wire, FIXED_ONE, 1)])),
    )
}

#[test]
fn common_ratio_has_exact_full_half_and_source_less_zero_boundaries() {
    let zero_loss = PowerLossCoefficient::new(0, 1).expect("coefficient is valid");
    let demands = [
        connected_demand(1, DemandKind::GateDrive, FIXED_ONE as u64, 10, 20),
        connected_demand(2, DemandKind::Movement, FIXED_ONE as u64, 10, 21),
    ];

    let full = solve_power_region(
        REGION,
        &[source(10, 2 * FIXED_ONE as u64)],
        &demands,
        zero_loss,
    )
    .expect("full-power region solves");
    assert_eq!(full.ratio(), PowerRatio::ONE);
    assert!(full.grants().iter().all(|grant| {
        grant.ratio() == PowerRatio::ONE && grant.granted() == Energy(FIXED_ONE as u64)
    }));

    let half = PowerRatio::new(Fixed(FIXED_ONE / 2)).expect("one half is representable");
    let brownout = solve_power_region(REGION, &[source(10, FIXED_ONE as u64)], &demands, zero_loss)
        .expect("half-power region solves");
    assert_eq!(brownout.ratio(), half);
    assert!(brownout.grants().iter().all(|grant| {
        grant.ratio() == half && grant.granted() == Energy((FIXED_ONE / 2) as u64)
    }));

    let source_less_demands = [
        PowerDemand::new(
            EntityId(1),
            DemandKind::GateDrive,
            REGION,
            Energy(FIXED_ONE as u64),
            None,
        ),
        PowerDemand::new(
            EntityId(2),
            DemandKind::Movement,
            REGION,
            Energy(FIXED_ONE as u64),
            None,
        ),
    ];
    let source_less = solve_power_region(REGION, &[], &source_less_demands, zero_loss)
        .expect("source-less region is a valid zero-ratio solve");
    assert_eq!(source_less.ratio(), PowerRatio::ZERO);
    assert!(
        source_less
            .grants()
            .iter()
            .all(|grant| { grant.ratio() == PowerRatio::ZERO && grant.granted() == Energy(0) })
    );
}

#[test]
fn lower_ratio_raises_delay_and_lowers_drive_movement_and_work_with_exact_rounding() {
    let half = PowerRatio::new(Fixed(FIXED_ONE / 2)).expect("one half is representable");
    let quarter = PowerRatio::new(Fixed(FIXED_ONE / 4)).expect("one quarter is representable");

    let delays = [PowerRatio::ONE, half, PowerRatio::ZERO]
        .map(|ratio| brownout_gate_delay(Tick(3), ratio, quarter).expect("delay scales"));
    assert_eq!(delays, [Tick(3), Tick(6), Tick(12)]);

    let drives = [PowerRatio::ONE, half, PowerRatio::ZERO]
        .map(|ratio| scale_drive(DriveStrength(7), ratio).expect("Drive scales"));
    let movement = [PowerRatio::ONE, half, PowerRatio::ZERO]
        .map(|ratio| scale_movement(Fixed(7), ratio).expect("movement scales"));
    let work = [PowerRatio::ONE, half, PowerRatio::ZERO]
        .map(|ratio| scale_work(Energy(7), ratio).expect("work scales"));
    assert_eq!(
        drives,
        [DriveStrength(7), DriveStrength(4), DriveStrength(0)]
    );
    assert_eq!(movement, [Fixed(7), Fixed(4), Fixed(0)]);
    assert_eq!(work, [Energy(7), Energy(4), Energy(0)]);

    // Exact RNE ties: 0.5 rounds to even 0, 1.5 to even 2, and 2.5 to even 2.
    assert_eq!(scale_drive(DriveStrength(1), half), Ok(DriveStrength(0)));
    assert_eq!(scale_drive(DriveStrength(3), half), Ok(DriveStrength(2)));
    assert_eq!(scale_movement(Fixed(5), half), Ok(Fixed(2)));
    assert_eq!(scale_work(Energy(5), half), Ok(Energy(2)));

    // Exact ceil boundary: ceil(2 / 0.75) = 3.
    let three_quarters =
        PowerRatio::new(Fixed(3 * FIXED_ONE / 4)).expect("three quarters is representable");
    assert_eq!(
        brownout_gate_delay(Tick(2), three_quarters, quarter),
        Ok(Tick(3))
    );
}

#[test]
fn region_solution_is_invariant_to_source_and_demand_permutation() {
    let zero_loss = PowerLossCoefficient::new(0, 1).expect("coefficient is valid");
    let first = connected_demand(20, DemandKind::Movement, FIXED_ONE as u64, 10, 30);
    let second = connected_demand(10, DemandKind::WireSensing, FIXED_ONE as u64, 10, 31);
    let sources = [
        source(11, (FIXED_ONE / 2) as u64),
        source(10, (FIXED_ONE / 2) as u64),
    ];

    let forward = solve_power_region(
        REGION,
        &sources,
        &[first.clone(), second.clone()],
        zero_loss,
    )
    .expect("forward permutation solves");
    let reverse = solve_power_region(
        REGION,
        &[sources[1], sources[0]],
        &[second, first],
        zero_loss,
    )
    .expect("reverse permutation solves");
    assert_eq!(forward, reverse);
    assert_eq!(
        forward
            .grants()
            .iter()
            .map(|grant| grant.demand_id())
            .collect::<Vec<_>>(),
        vec![
            DemandId::new(EntityId(10), DemandKind::WireSensing),
            DemandId::new(EntityId(20), DemandKind::Movement),
        ]
    );
}

#[test]
fn nonzero_loss_is_ceiled_and_all_heat_is_conserved_by_wire_id() {
    let coefficient = PowerLossCoefficient::new(1, 3).expect("coefficient is valid");
    let canonical_route = route(10, &[(30, 2 * FIXED_ONE, 1), (10, FIXED_ONE, 1)]);

    // ceil((1/3) * 3WU * 2^2) = 4 raw Energy units.
    assert_eq!(
        transmission_loss(Fixed(3 * FIXED_ONE), Energy(2), coefficient),
        Ok(Energy(4))
    );
    let demand_id = DemandId::new(EntityId(5), DemandKind::GateDrive);
    let heat = distribute_transmission_heat(demand_id, Energy(4), &canonical_route)
        .expect("loss distributes");
    assert_eq!(heat.len(), 2);
    assert_eq!(heat[0].wire(), wire(10));
    assert_eq!(heat[0].heat_energy(), HeatEnergy(2));
    assert_eq!(heat[1].wire(), wire(30));
    assert_eq!(heat[1].heat_energy(), HeatEnergy(2));
    assert_eq!(heat.iter().map(|row| row.heat_energy().0).sum::<u64>(), 4);

    let demand = PowerDemand::new(
        EntityId(5),
        DemandKind::GateDrive,
        REGION,
        Energy(2),
        Some(canonical_route),
    );
    let solved = solve_power_region(REGION, &[source(10, 6)], &[demand], coefficient)
        .expect("generation covers the full grant plus nonzero loss");
    assert_eq!(solved.ratio(), PowerRatio::ONE);
    assert_eq!(solved.grants()[0].granted(), Energy(2));
    assert_eq!(solved.grants()[0].transmission_loss(), Energy(4));
    assert_eq!(solved.grants()[0].source_cost(), Energy(6));
    assert_eq!(solved.transmission_heat(), heat.as_slice());
}

#[test]
fn public_topology_compiler_coalesces_virtual_and_partial_wire_routes() {
    let source = source_id(50);
    let physical_wire = wire(10);
    let gate = GateId(EntityId(60));
    let intrinsic = DemandId::new(physical_wire.entity_id(), DemandKind::WireSensing);
    let partial = DemandId::new(EntityId(70), DemandKind::Movement);
    let whole = DemandId::new(gate.entity_id(), DemandKind::GateIdle);
    let compiled = CompiledPowerTopology::compile(&PowerTopologyInput {
        bodies: vec![PowerBodyEdge {
            wire: physical_wire,
            a: PowerNodeKey::SourceAnchor(source),
            b: PowerNodeKey::GatePower(gate),
            length: Fixed(9),
            segment_lengths: vec![Fixed(3), Fixed(4), Fixed(2)],
            canonical_lower_end: WireEnd::A,
        }],
        sources: vec![PowerSourceAttachment {
            source,
            node: PowerNodeKey::SourceAnchor(source),
        }],
        loads: vec![
            PowerLoadAttachment {
                demand: intrinsic,
                node: PowerNodeKey::WireBody(physical_wire),
            },
            PowerLoadAttachment {
                demand: partial,
                node: PowerNodeKey::WireOffset(physical_wire, Fixed(8)),
            },
            PowerLoadAttachment {
                demand: whole,
                node: PowerNodeKey::GatePower(gate),
            },
        ],
    })
    .expect("public abstract topology compiles");

    let intrinsic_route = compiled
        .load(intrinsic)
        .expect("intrinsic load exists")
        .source_route()
        .expect("intrinsic route exists");
    assert_eq!(intrinsic_route.key().total_length(), Fixed(4));
    assert_eq!(intrinsic_route.key().segment_count(), 2);
    assert_eq!(intrinsic_route.wires().len(), 1);
    assert_eq!(intrinsic_route.wires()[0].length(), Fixed(4));
    assert_eq!(intrinsic_route.wires()[0].segment_count(), 2);

    let partial_route = compiled
        .load(partial)
        .expect("partial load exists")
        .source_route()
        .expect("partial route exists");
    assert_eq!(partial_route.key().total_length(), Fixed(8));
    assert_eq!(partial_route.key().segment_count(), 3);
    assert_eq!(partial_route.wires().len(), 1);
    assert_eq!(partial_route.wires()[0].length(), Fixed(8));
    assert_eq!(partial_route.wires()[0].segment_count(), 3);

    let whole_route = compiled
        .load(whole)
        .expect("whole-Wire load exists")
        .source_route()
        .expect("whole-Wire route exists");
    assert_eq!(whole_route.key().total_length(), Fixed(9));
    assert_eq!(whole_route.key().segment_count(), 3);
    assert_eq!(whole_route.wires().len(), 1);
    assert_eq!(whole_route.wires()[0].length(), Fixed(9));
    assert_eq!(whole_route.wires()[0].segment_count(), 3);
}
