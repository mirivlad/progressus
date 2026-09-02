use progressus_sim::{
    EntityId, ItemKind, ItemLocation, Terrain, WorldCell, WorldPosition, WorldSeed,
};
use progressus_sim::{Simulation, SimulationTick};

#[test]
fn new_game_has_five_stable_characters_on_walkable_cells() {
    let simulation = Simulation::new(WorldSeed::new(42)).unwrap();

    assert_eq!(simulation.tick(), SimulationTick::new(0));
    let characters = simulation.characters().collect::<Vec<_>>();
    assert_eq!(characters.len(), 5);

    let expected = [
        (1, "Ada", -2),
        (2, "Borin", -1),
        (3, "Cora", 0),
        (4, "Dain", 1),
        (5, "Elin", 2),
    ];

    for (character, (id, name, x)) in characters.iter().zip(expected) {
        assert_eq!(character.id(), EntityId::new(id).unwrap());
        assert_eq!(character.name(), name);
        assert_eq!(
            character.position(),
            WorldPosition::from_cell_center(WorldCell::new(x, 0)).unwrap()
        );

        let (chunk_coordinate, local) = character.position().containing_cell().split();
        let chunk = simulation.generated_chunk(chunk_coordinate).unwrap();
        assert_eq!(chunk.terrain_at(local), Some(Terrain::Grass));
    }
}

#[test]
fn identical_commands_produce_identical_public_state() {
    let mut first = Simulation::new(WorldSeed::new(73)).unwrap();
    let mut second = Simulation::new(WorldSeed::new(73)).unwrap();

    first.advance_ticks(100_000).unwrap();
    second.advance_ticks(100_000).unwrap();

    assert_eq!(first.tick(), second.tick());
    assert_eq!(
        first.characters().cloned().collect::<Vec<_>>(),
        second.characters().cloned().collect::<Vec<_>>()
    );
    assert_eq!(
        first.items().cloned().collect::<Vec<_>>(),
        second.items().cloned().collect::<Vec<_>>()
    );
}

#[test]
fn new_game_has_physical_starting_supplies_with_global_stable_ids() {
    let simulation = Simulation::new(WorldSeed::new(42)).unwrap();
    let items = simulation.items().collect::<Vec<_>>();

    assert_eq!(items.len(), 4);
    assert_eq!(items[0].id(), EntityId::new(6).unwrap());
    assert_eq!(items[0].kind(), ItemKind::Wood);
    assert!(matches!(items[0].location(), ItemLocation::Ground { .. }));
    assert_eq!(items[3].id(), EntityId::new(9).unwrap());
    assert_eq!(items[3].kind(), ItemKind::Stone);
}

#[test]
fn entity_id_zero_is_invalid() {
    assert_eq!(EntityId::new(0), None);
}

#[test]
fn next_stable_entity_id_continues_after_bootstrap_characters() {
    let simulation = Simulation::new(WorldSeed::new(42)).unwrap();

    assert_eq!(simulation.next_entity_id(), EntityId::new(10));
}
