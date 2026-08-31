use progressus_sim::{EntityId, Terrain, WorldCell, WorldSeed};
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
        assert_eq!(character.position(), WorldCell::new(x, 0));

        let (chunk_coordinate, local) = character.position().split();
        let chunk = simulation.generate_chunk(chunk_coordinate).unwrap();
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
}

#[test]
fn entity_id_zero_is_invalid() {
    assert_eq!(EntityId::new(0), None);
}
