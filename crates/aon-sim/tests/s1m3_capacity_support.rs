use aon_sim::{
    Capacity, CapacityProbeProfile, CapacitySupportError, CapacitySupportProbeProfile, Energy,
    EntityId, FIXED_ONE, Rational, WireId, calculate_capacity_support_demand, capacity_excess,
    distribute_capacity_support_demand,
};

fn ratio(numerator: i64, denominator: i64) -> Rational {
    Rational::new(numerator, denominator).expect("fixture coefficient is valid")
}

fn capacity_profile() -> CapacityProbeProfile {
    CapacityProbeProfile {
        main_core_capacity: 100,
        relay_capacity: 500,
        overcap_linear_k: ratio(1, 1),
        overcap_quadratic_k: ratio(2, 1),
        capacity_denominator_floor: 1,
        relay_offline_grace_ticks: 10,
        support_heat_fraction: ratio(1, 2),
    }
}

fn support_profile() -> CapacitySupportProbeProfile {
    CapacitySupportProbeProfile {
        support_power_per_ncu: ratio(1, 1),
    }
}

fn ncu(value: u64) -> Capacity {
    Capacity(
        value
            .checked_mul(FIXED_ONE as u64)
            .expect("small fixture Capacity fits"),
    )
}

fn wire(value: u64) -> WireId {
    WireId(EntityId(value))
}

#[test]
fn c22_exact_curve_and_sorted_proportional_distribution_match_independent_oracle() {
    let demand = calculate_capacity_support_demand(
        ncu(120),
        ncu(100),
        &capacity_profile(),
        &support_profile(),
    )
    .expect("C22 curve is representable");
    assert_eq!(capacity_excess(ncu(120), ncu(100)), ncu(20));
    assert_eq!(demand, Energy(28));

    let increased = calculate_capacity_support_demand(
        ncu(121),
        ncu(100),
        &capacity_profile(),
        &support_profile(),
    )
    .expect("the paired 121-NCU curve is representable");
    assert_eq!(increased, Energy(30));
    assert!(
        increased > demand,
        "increasing U across this exact valid pair strictly increases demand"
    );

    let shares = distribute_capacity_support_demand(
        ncu(120),
        demand,
        &[(wire(9), ncu(50)), (wire(3), ncu(70))],
    )
    .expect("C22 distribution is valid");
    assert_eq!(shares.len(), 2);
    assert_eq!(shares[0].wire(), wire(3));
    assert_eq!(shares[0].length(), ncu(70));
    assert_eq!(shares[0].demand(), Energy(17));
    assert_eq!(shares[1].wire(), wire(9));
    assert_eq!(shares[1].length(), ncu(50));
    assert_eq!(shares[1].demand(), Energy(11));
    assert_eq!(shares.iter().map(|share| share.demand().0).sum::<u64>(), 28);
}

#[test]
fn one_final_ceil_preserves_fractional_cross_term_and_excess_is_monotonic() {
    let capacity = CapacityProbeProfile {
        overcap_linear_k: ratio(1, 3),
        overcap_quadratic_k: ratio(2, 7),
        ..capacity_profile()
    };
    let support = CapacitySupportProbeProfile {
        support_power_per_ncu: ratio(5, 11),
    };

    let mut previous = Energy(0);
    for used_raw in (99 * FIXED_ONE as u64)..=(104 * FIXED_ONE as u64) {
        let used = Capacity(used_raw);
        let demand = calculate_capacity_support_demand(used, ncu(100), &capacity, &support)
            .expect("bounded exact oracle range is representable");
        let oracle = independent_demand_oracle(used, ncu(100), &capacity, &support);
        assert_eq!(demand, oracle, "oracle mismatch at raw used={used_raw}");
        assert!(
            demand >= previous,
            "demand regressed at raw used={used_raw}"
        );
        previous = demand;
    }

    // Both terms are fractional here. Rounding either before composition gives a different answer.
    let fractional =
        calculate_capacity_support_demand(Capacity(ncu(100).0 + 1), ncu(100), &capacity, &support)
            .expect("one-raw-unit excess is representable");
    assert_eq!(fractional, Energy(1));
}

#[test]
fn zero_excess_short_circuits_and_active_invalid_coefficients_fail_closed() {
    let mut invalid_capacity = capacity_profile();
    invalid_capacity.overcap_quadratic_k = ratio(0, 1);
    let invalid_support = CapacitySupportProbeProfile {
        support_power_per_ncu: ratio(-1, 1),
    };

    assert_eq!(
        calculate_capacity_support_demand(ncu(100), ncu(100), &invalid_capacity, &invalid_support),
        Ok(Energy(0))
    );
    assert_eq!(
        calculate_capacity_support_demand(
            ncu(101),
            ncu(100),
            &invalid_capacity,
            &support_profile()
        ),
        Err(CapacitySupportError::NonPositiveQuadraticCoefficient)
    );

    invalid_capacity = capacity_profile();
    invalid_capacity.overcap_linear_k = ratio(-1, 1);
    assert_eq!(
        calculate_capacity_support_demand(
            ncu(101),
            ncu(100),
            &invalid_capacity,
            &support_profile()
        ),
        Err(CapacitySupportError::NegativeLinearCoefficient)
    );
    assert_eq!(
        calculate_capacity_support_demand(
            ncu(101),
            ncu(100),
            &capacity_profile(),
            &invalid_support
        ),
        Err(CapacitySupportError::NonPositiveSupportPowerPerNcu)
    );

    let mut zero_floor = capacity_profile();
    zero_floor.capacity_denominator_floor = 0;
    assert_eq!(
        calculate_capacity_support_demand(ncu(101), ncu(100), &zero_floor, &support_profile()),
        Err(CapacitySupportError::ZeroCapacityDenominatorFloor)
    );
}

#[test]
fn distribution_is_permutation_stable_conservative_and_fail_closed() {
    let canonical = [
        (wire(2), Capacity(1)),
        (wire(4), Capacity(2)),
        (wire(8), Capacity(4)),
    ];
    let permuted = [canonical[2], canonical[0], canonical[1]];
    let expected = distribute_capacity_support_demand(Capacity(7), Energy(11), &canonical)
        .expect("canonical distribution succeeds");
    let actual = distribute_capacity_support_demand(Capacity(7), Energy(11), &permuted)
        .expect("permuted distribution succeeds");
    assert_eq!(actual, expected);
    assert_eq!(actual.iter().map(|share| share.demand().0).sum::<u64>(), 11);
    assert_eq!(
        actual
            .iter()
            .map(|share| (share.wire(), share.demand()))
            .collect::<Vec<_>>(),
        vec![
            (wire(2), Energy(2)),
            (wire(4), Energy(3)),
            (wire(8), Energy(6))
        ]
    );

    assert_eq!(
        distribute_capacity_support_demand(Capacity(0), Energy(0), &[]),
        Ok(Vec::new())
    );
    assert_eq!(
        distribute_capacity_support_demand(Capacity(1), Energy(1), &[]),
        Err(CapacitySupportError::EmptyWireSet)
    );
    assert_eq!(
        distribute_capacity_support_demand(
            Capacity(2),
            Energy(1),
            &[(wire(2), Capacity(1)), (wire(2), Capacity(1))]
        ),
        Err(CapacitySupportError::DuplicateWire { wire: wire(2) })
    );
    assert_eq!(
        distribute_capacity_support_demand(Capacity(1), Energy(1), &[(wire(2), Capacity(0))]),
        Err(CapacitySupportError::ZeroWireLength { wire: wire(2) })
    );
    assert_eq!(
        distribute_capacity_support_demand(Capacity(3), Energy(1), &[(wire(2), Capacity(2))]),
        Err(CapacitySupportError::UsedCapacityMismatch {
            declared: Capacity(3),
            actual: Capacity(2)
        })
    );
}

#[test]
fn curve_and_distribution_report_typed_overflow_without_saturation() {
    let mut capacity = capacity_profile();
    capacity.overcap_linear_k = ratio(i64::MAX, 1);
    capacity.overcap_quadratic_k = ratio(i64::MAX, 1);
    let support = CapacitySupportProbeProfile {
        support_power_per_ncu: ratio(i64::MAX, 1),
    };
    assert_eq!(
        calculate_capacity_support_demand(Capacity(u64::MAX), Capacity(0), &capacity, &support),
        Err(CapacitySupportError::ArithmeticOverflow)
    );

    let mut floor_overflow = capacity_profile();
    floor_overflow.capacity_denominator_floor = u64::MAX;
    assert_eq!(
        calculate_capacity_support_demand(
            Capacity(FIXED_ONE as u64 + 1),
            Capacity(0),
            &floor_overflow,
            &support_profile()
        ),
        Err(CapacitySupportError::CapacityDenominatorFloorOverflow)
    );

    let mut out_of_range = capacity_profile();
    out_of_range.overcap_linear_k = ratio(0, 1);
    out_of_range.overcap_quadratic_k = ratio(1, 1);
    assert_eq!(
        calculate_capacity_support_demand(
            Capacity(u64::MAX),
            Capacity(0),
            &out_of_range,
            &support_profile()
        ),
        Err(CapacitySupportError::DemandOutOfRange)
    );

    assert_eq!(
        distribute_capacity_support_demand(
            Capacity(u64::MAX),
            Energy(1),
            &[(wire(2), Capacity(u64::MAX)), (wire(3), Capacity(1))]
        ),
        Err(CapacitySupportError::ArithmeticOverflow)
    );
}

fn independent_demand_oracle(
    used: Capacity,
    supported: Capacity,
    capacity: &CapacityProbeProfile,
    support: &CapacitySupportProbeProfile,
) -> Energy {
    let excess = u128::from(used.0.saturating_sub(supported.0));
    if excess == 0 {
        return Energy(0);
    }
    let floor = u128::from(capacity.capacity_denominator_floor) * FIXED_ONE as u128;
    let denominator_capacity = u128::from(supported.0).max(floor);
    let linear_n = capacity.overcap_linear_k.numerator() as u128;
    let linear_d = capacity.overcap_linear_k.denominator() as u128;
    let quadratic_n = capacity.overcap_quadratic_k.numerator() as u128;
    let quadratic_d = capacity.overcap_quadratic_k.denominator() as u128;
    let support_n = support.support_power_per_ncu.numerator() as u128;
    let support_d = support.support_power_per_ncu.denominator() as u128;

    let numerator = support_n
        * (linear_n * excess * quadratic_d * denominator_capacity
            + quadratic_n * excess * excess * linear_d);
    let denominator = support_d * linear_d * quadratic_d * denominator_capacity * FIXED_ONE as u128;
    Energy(numerator.div_ceil(denominator) as u64)
}
