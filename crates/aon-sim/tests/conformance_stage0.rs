use aon_sim::{FIXED_ONE, Fixed, FixedVec2, cell_coordinate, segment_length};

#[test]
fn c17_numeric_geometry_uses_integer_euclidean_length_and_floor_cells() {
    let origin = FixedVec2::new(Fixed::ZERO, Fixed::ZERO);
    let three_four = FixedVec2::new(Fixed(3 * FIXED_ONE), Fixed(4 * FIXED_ONE));

    assert_eq!(segment_length(origin, three_four), Ok(Fixed(5 * FIXED_ONE)));
    assert_eq!(cell_coordinate(Fixed(-1), Fixed(FIXED_ONE)), Ok(-1));
}
