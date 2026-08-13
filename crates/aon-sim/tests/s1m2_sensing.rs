use aon_sim::{
    EntityId, Fixed, FixedVec2, HostileCollider, SparseOrderedChunkGrid, WireId, WireSensingInput,
    WireSensingOutput, circle_intersects_polyline_capsule, sample_wire_sensing,
    sample_wire_sensing_with_grid,
};

const fn point(x: i64, y: i64) -> FixedVec2 {
    FixedVec2::new(Fixed(x), Fixed(y))
}

const fn hostile(id: u64, x: i64, y: i64, radius: i64) -> HostileCollider {
    HostileCollider {
        id,
        center: point(x, y),
        radius: Fixed(radius),
    }
}

const fn wire<'a>(id: u64, points: &'a [FixedVec2]) -> WireSensingInput<'a> {
    WireSensingInput {
        id: WireId(EntityId(id)),
        points,
    }
}

#[test]
fn complete_frames_sample_zero_then_three_then_zero_without_persistence() {
    let points = [point(0, 0), point(10, 0)];
    let wires = [wire(1, &points)];
    let three = [
        hostile(30, 2, 0, 0),
        hostile(10, 5, 1, 0),
        hostile(20, 8, -1, 0),
    ];

    let empty_before =
        sample_wire_sensing(&wires, &[], Fixed(1), Fixed(4)).expect("empty frame samples");
    let occupied = sample_wire_sensing(&wires, &three, Fixed(1), Fixed(4))
        .expect("three-collider frame samples");
    let empty_after = sample_wire_sensing(&wires, &[], Fixed(1), Fixed(4))
        .expect("following empty frame samples");

    assert_eq!(
        empty_before,
        vec![WireSensingOutput {
            id: WireId(EntityId(1)),
            occupied: false,
        }]
    );
    assert_eq!(
        occupied,
        vec![WireSensingOutput {
            id: WireId(EntityId(1)),
            occupied: true,
        }]
    );
    assert_eq!(empty_after, empty_before);
}

#[test]
fn closed_capsules_include_exact_tangency_at_interiors_endcaps_and_bends() {
    let straight = [point(0, 0), point(10, 0)];
    let sense_radius = Fixed(2);

    // Interior segment: combined radius is exactly three raw units.
    assert!(
        circle_intersects_polyline_capsule(hostile(1, 5, 3, 1), &straight, sense_radius,)
            .expect("interior tangent is valid")
    );
    assert!(
        !circle_intersects_polyline_capsule(hostile(2, 5, 4, 1), &straight, sense_radius,)
            .expect("interior outside point is valid")
    );

    // Endpoint cap: projection lies before A, so the exact circular endcap decides the result.
    assert!(
        circle_intersects_polyline_capsule(hostile(3, -3, 0, 1), &straight, sense_radius,)
            .expect("endcap tangent is valid")
    );
    assert!(
        !circle_intersects_polyline_capsule(hostile(4, -4, 0, 1), &straight, sense_radius,)
            .expect("endcap outside point is valid")
    );

    // A bend is the union of its exact segment capsules; the shared corner is not a gap.
    let bend = [point(0, 0), point(10, 0), point(10, 10)];
    assert!(
        circle_intersects_polyline_capsule(hostile(5, 8, 2, 0), &bend, sense_radius)
            .expect("bend tangent is valid")
    );
    assert!(
        !circle_intersects_polyline_capsule(hostile(6, 7, 3, 0), &bend, sense_radius)
            .expect("bend outside point is valid")
    );
}

#[test]
fn wire_and_hostile_permutations_are_invariant_and_multiplicity_is_one_bit() {
    let near = [point(0, 0), point(10, 0)];
    let far = [point(0, 20), point(10, 20)];
    let wires_forward = [wire(9, &far), wire(3, &near)];
    let wires_reverse = [wire(3, &near), wire(9, &far)];
    let hostiles_forward = [
        hostile(30, 7, 0, 0),
        hostile(10, 3, 0, 0),
        hostile(20, 5, 1, 0),
    ];
    let hostiles_reverse = [
        hostiles_forward[2],
        hostiles_forward[1],
        hostiles_forward[0],
    ];
    let expected = vec![
        WireSensingOutput {
            id: WireId(EntityId(3)),
            occupied: true,
        },
        WireSensingOutput {
            id: WireId(EntityId(9)),
            occupied: false,
        },
    ];

    assert_eq!(
        sample_wire_sensing(&wires_forward, &hostiles_forward, Fixed(1), Fixed(4)),
        Ok(expected.clone())
    );
    assert_eq!(
        sample_wire_sensing(&wires_reverse, &hostiles_reverse, Fixed(1), Fixed(4)),
        Ok(expected)
    );

    let one = sample_wire_sensing(
        &[wire(3, &near)],
        &[hostiles_forward[0]],
        Fixed(1),
        Fixed(4),
    )
    .expect("single collider samples");
    let three = sample_wire_sensing(&[wire(3, &near)], &hostiles_forward, Fixed(1), Fixed(4))
        .expect("three colliders sample");
    assert_eq!(one, three);
    assert!(one[0].occupied);
}

#[test]
fn sparse_grid_matches_the_direct_exact_oracle_and_never_omits_a_hit() {
    let mut hostiles = Vec::new();
    let mut id = 1_u64;
    for x in -6_i64..=6 {
        for y in -5_i64..=5 {
            if (x + 2 * y).rem_euclid(3) != 0 {
                continue;
            }
            hostiles.push(hostile(id, x, y, id as i64 % 3));
            id += 1;
        }
    }
    hostiles.reverse();

    let first = [point(-7, -4), point(-1, 2), point(7, 2)];
    let second = [point(-8, 5), point(-3, 5), point(4, -2)];
    let third = [point(6, -6), point(6, 6)];
    let wires = [wire(9, &third), wire(2, &first), wire(5, &second)];

    for chunk_size in [Fixed(1), Fixed(3), Fixed(7)] {
        let grid = SparseOrderedChunkGrid::new(chunk_size, &hostiles)
            .expect("valid sparse grid constructs");
        assert_eq!(grid.hostile_count(), hostiles.len());
        for sense_radius in [Fixed(0), Fixed(1), Fixed(2)] {
            let sampled = sample_wire_sensing_with_grid(&wires, &grid, sense_radius)
                .expect("grid sample succeeds");
            for wire in &wires {
                let expected = hostiles.iter().copied().any(|collider| {
                    circle_intersects_polyline_capsule(collider, wire.points, sense_radius)
                        .expect("direct oracle succeeds")
                });
                let actual = sampled
                    .iter()
                    .find(|row| row.id == wire.id)
                    .expect("every Wire has one result")
                    .occupied;
                assert_eq!(
                    actual, expected,
                    "Wire {:?}, chunk {}, radius {}",
                    wire.id, chunk_size.0, sense_radius.0
                );

                let candidates = grid
                    .candidate_ids_for_polyline(wire.points, sense_radius)
                    .expect("candidate query succeeds");
                assert!(candidates.windows(2).all(|pair| pair[0] < pair[1]));
                for collider in hostiles.iter().copied().filter(|collider| {
                    circle_intersects_polyline_capsule(*collider, wire.points, sense_radius)
                        .expect("direct oracle succeeds")
                }) {
                    assert!(
                        candidates.binary_search(&collider.id).is_ok(),
                        "broad phase omitted exact hit {} for Wire {:?}",
                        collider.id,
                        wire.id
                    );
                }
            }
        }
    }
}
