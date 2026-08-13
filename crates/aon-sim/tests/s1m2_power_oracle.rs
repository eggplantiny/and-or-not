use aon_sim::{
    CanonicalPowerRoute, DemandKind, Energy, EntityId, FIXED_ONE, Fixed, FixedVec2,
    POWER_RATIO_SOLVER_COMPARISONS, PowerDemand, PowerError, PowerLossCoefficient, PowerPathToken,
    PowerRatio, PowerRegionId, PowerRouteKey, PowerRouteWire, PowerSourceId, PowerSourceState,
    WireId, solve_power_region,
};

const REGION: PowerRegionId = PowerRegionId(11);
const ONE_RAW: u64 = FIXED_ONE as u64;

#[derive(Clone, Copy)]
struct OracleDemand {
    nominal: u64,
    distance_raw: u64,
}

fn source(id: u64, generation: u64) -> PowerSourceState {
    PowerSourceState::new(
        PowerSourceId(EntityId(id)),
        FixedVec2::new(Fixed(id as i64), Fixed::ZERO),
        Energy(generation),
    )
}

fn route(source_id: u64, wire_id: u64, distance_raw: u64) -> CanonicalPowerRoute {
    let distance = Fixed(i64::try_from(distance_raw).expect("oracle distance fits Fixed"));
    CanonicalPowerRoute::new(
        PowerSourceId(EntityId(source_id)),
        PowerRouteKey::new(
            distance,
            1,
            vec![
                PowerPathToken::new(3, EntityId(wire_id), 0),
                PowerPathToken::new(7, EntityId(source_id), 0),
            ],
        )
        .expect("oracle route key is valid"),
        vec![
            PowerRouteWire::new(WireId(EntityId(wire_id)), distance, 1)
                .expect("oracle route Wire is valid"),
        ],
    )
    .expect("oracle route is canonical")
}

fn connected_demands(source_id: u64, demands: &[OracleDemand]) -> Vec<PowerDemand> {
    demands
        .iter()
        .enumerate()
        .map(|(index, demand)| {
            let ordinal = u64::try_from(index).expect("bounded oracle index fits u64");
            PowerDemand::new(
                EntityId(100 + ordinal),
                DemandKind::Movement,
                REGION,
                Energy(demand.nominal),
                Some(route(source_id, 200 + ordinal, demand.distance_raw)),
            )
        })
        .collect()
}

fn rne_scaled(nominal: u64, ratio_raw: u64) -> u64 {
    let numerator = u128::from(nominal) * u128::from(ratio_raw);
    let denominator = u128::from(ONE_RAW);
    let floor = numerator / denominator;
    let remainder = numerator % denominator;
    let rounded = match (remainder * 2).cmp(&denominator) {
        std::cmp::Ordering::Less => floor,
        std::cmp::Ordering::Greater => floor + 1,
        std::cmp::Ordering::Equal if floor.is_multiple_of(2) => floor,
        std::cmp::Ordering::Equal => floor + 1,
    };
    u64::try_from(rounded).expect("bounded oracle grant fits u64")
}

fn ceil_loss(coefficient: (u64, u64), distance_raw: u64, granted: u64) -> u64 {
    if coefficient.0 == 0 || distance_raw == 0 || granted == 0 {
        return 0;
    }
    let numerator = u128::from(coefficient.0)
        * u128::from(distance_raw)
        * u128::from(granted)
        * u128::from(granted);
    let denominator = u128::from(coefficient.1) * u128::from(ONE_RAW);
    let quotient = numerator / denominator;
    let rounded = quotient + u128::from(!numerator.is_multiple_of(denominator));
    u64::try_from(rounded).expect("bounded oracle loss fits u64")
}

fn exhaustive_max_ratio(generation: u64, demands: &[OracleDemand], coefficient: (u64, u64)) -> u64 {
    (0..=ONE_RAW)
        .filter(|&ratio_raw| oracle_cost(demands, coefficient, ratio_raw) <= generation.into())
        .max()
        .expect("ratio zero is feasible in every bounded case")
}

fn oracle_cost(demands: &[OracleDemand], coefficient: (u64, u64), ratio_raw: u64) -> u128 {
    demands
        .iter()
        .map(|demand| {
            let granted = rne_scaled(demand.nominal, ratio_raw);
            u128::from(granted) + u128::from(ceil_loss(coefficient, demand.distance_raw, granted))
        })
        .sum()
}

#[test]
fn solver_matches_an_independent_exhaustive_oracle_over_all_ratio_values() {
    struct Case {
        generation_by_source: &'static [u64],
        demands: &'static [OracleDemand],
        coefficient: (u64, u64),
    }

    let cases = [
        Case {
            generation_by_source: &[3],
            demands: &[
                OracleDemand {
                    nominal: 1,
                    distance_raw: ONE_RAW,
                },
                OracleDemand {
                    nominal: 3,
                    distance_raw: ONE_RAW,
                },
                OracleDemand {
                    nominal: 5,
                    distance_raw: ONE_RAW,
                },
            ],
            coefficient: (0, 1),
        },
        Case {
            generation_by_source: &[4, 2],
            demands: &[
                OracleDemand {
                    nominal: 8,
                    distance_raw: ONE_RAW,
                },
                OracleDemand {
                    nominal: 9,
                    distance_raw: 2 * ONE_RAW,
                },
            ],
            coefficient: (0, 1),
        },
        Case {
            generation_by_source: &[15],
            demands: &[
                OracleDemand {
                    nominal: 5,
                    distance_raw: ONE_RAW,
                },
                OracleDemand {
                    nominal: 7,
                    distance_raw: 2 * ONE_RAW,
                },
            ],
            coefficient: (1, 5),
        },
        Case {
            generation_by_source: &[100],
            demands: &[
                OracleDemand {
                    nominal: 1,
                    distance_raw: ONE_RAW,
                },
                OracleDemand {
                    nominal: 3,
                    distance_raw: ONE_RAW,
                },
            ],
            coefficient: (1, 10),
        },
    ];

    assert_eq!(POWER_RATIO_SOLVER_COMPARISONS, 17);
    for case in cases {
        let sources = case
            .generation_by_source
            .iter()
            .copied()
            .enumerate()
            .map(|(index, generation)| {
                source(
                    10 + u64::try_from(index).expect("bounded source index fits"),
                    generation,
                )
            })
            .collect::<Vec<_>>();
        let demands = connected_demands(sources[0].id().entity_id().0, case.demands);
        let coefficient = PowerLossCoefficient::new(case.coefficient.0, case.coefficient.1)
            .expect("oracle coefficient is valid");
        let solved = solve_power_region(REGION, &sources, &demands, coefficient)
            .expect("bounded production case solves");
        let generation = case.generation_by_source.iter().copied().sum();
        let expected = exhaustive_max_ratio(generation, case.demands, case.coefficient);
        assert_eq!(
            solved.ratio().raw(),
            i64::try_from(expected).expect("ratio raw fits i64")
        );
        if expected < ONE_RAW {
            assert!(
                oracle_cost(case.demands, case.coefficient, expected + 1) > u128::from(generation),
                "the production result must be the greatest feasible raw ratio"
            );
        }
    }

    let source_less_demands = [PowerDemand::new(
        EntityId(100),
        DemandKind::Movement,
        REGION,
        Energy(5),
        None,
    )];
    let source_less = solve_power_region(
        REGION,
        &[],
        &source_less_demands,
        PowerLossCoefficient::new(0, 1).expect("zero-loss coefficient is valid"),
    )
    .expect("source-less G=0 case solves");
    assert_eq!(source_less.ratio(), PowerRatio::ZERO);
}

#[test]
fn power_solver_overflow_is_typed_and_never_saturates() {
    assert_eq!(
        solve_power_region(
            REGION,
            &[source(10, u64::MAX), source(11, 1)],
            &[],
            PowerLossCoefficient::new(0, 1).expect("zero-loss coefficient is valid"),
        ),
        Err(PowerError::NumericOverflow)
    );

    let huge = [PowerDemand::new(
        EntityId(100),
        DemandKind::Movement,
        REGION,
        Energy(u64::MAX),
        Some(route(10, 200, i64::MAX as u64)),
    )];
    assert_eq!(
        solve_power_region(
            REGION,
            &[source(10, u64::MAX)],
            &huge,
            PowerLossCoefficient::new(u64::MAX, 1).expect("large coefficient is valid"),
        ),
        Err(PowerError::NumericOverflow)
    );
}
