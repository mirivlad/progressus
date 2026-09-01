use progressus_sim::{
    InteractionRadius, SUBUNITS_PER_CELL, WorldCell, WorldPosition, within_interaction_range,
};

#[test]
fn centers_and_boundaries_use_euclidean_containment() {
    assert_eq!(SUBUNITS_PER_CELL, 1024);
    assert_eq!(
        WorldPosition::from_cell_center(WorldCell::new(-1, 0))
            .unwrap()
            .x_subunits(),
        -512,
    );
    assert_eq!(
        WorldPosition::from_subunits(-1, 0)
            .unwrap()
            .containing_cell(),
        WorldCell::new(-1, 0),
    );
    assert_eq!(
        WorldPosition::from_subunits(0, 0)
            .unwrap()
            .containing_cell(),
        WorldCell::new(0, 0),
    );

    for cell in [
        WorldCell::new(-32, -1),
        WorldCell::new(0, 0),
        WorldCell::new(32, 7),
    ] {
        assert_eq!(
            WorldPosition::from_cell_center(cell)
                .unwrap()
                .containing_cell(),
            cell,
        );
    }
}

#[test]
fn checked_translation_and_construction_do_not_wrap() {
    let maximum = WorldPosition::from_cell_origin(WorldCell::new(i64::MAX, 0)).unwrap();
    assert!(maximum.checked_translate(SUBUNITS_PER_CELL, 0).is_err());
    assert!(WorldPosition::from_subunits(i128::MAX, 0).is_err());
}

#[test]
fn integer_interaction_radius_is_exact_and_safe_for_distant_positions() {
    let actor = WorldPosition::from_subunits(100, 100).unwrap();
    let inside = WorldPosition::from_subunits(103, 104).unwrap();
    let outside = WorldPosition::from_subunits(104, 104).unwrap();

    assert!(within_interaction_range(
        actor,
        InteractionRadius::new(5),
        inside,
        InteractionRadius::zero(),
    ));
    assert!(!within_interaction_range(
        actor,
        InteractionRadius::new(5),
        outside,
        InteractionRadius::zero(),
    ));

    let far_west = WorldPosition::from_cell_center(WorldCell::new(i64::MIN, 0)).unwrap();
    let far_east = WorldPosition::from_cell_center(WorldCell::new(i64::MAX, 0)).unwrap();
    assert!(!within_interaction_range(
        far_west,
        InteractionRadius::zero(),
        far_east,
        InteractionRadius::zero(),
    ));
}
