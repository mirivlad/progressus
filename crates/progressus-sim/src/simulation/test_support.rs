//! Shared fixtures for the authoritative simulation's module tests.

use progressus_content::{item, natural_resource, terrain, workstation};

use super::*;
pub(super) use crate::{Direction, HUNGRY_SATIETY, MAX_SATIETY, MovementSpeed, MovementState};

pub(super) fn cora() -> EntityId {
    EntityId::new(3).unwrap()
}

pub(super) fn set_satiety(simulation: &mut Simulation, character_id: EntityId, target: u8) {
    let character = simulation.characters.get_mut(&character_id).unwrap();
    while character.satiety() > target {
        character.decay_satiety();
    }
    if character.satiety() < target {
        character.restore_satiety(target - character.satiety());
    }
}

pub(super) fn total_berries(simulation: &Simulation) -> u32 {
    simulation
        .items()
        .filter(|item| item.kind() == item::BERRIES)
        .map(|item| item.quantity().get())
        .sum()
}

pub(super) fn clear_all_items(simulation: &mut Simulation) {
    let items = simulation
        .items()
        .map(|item| (item.id(), item.quantity().get()))
        .collect::<Vec<_>>();
    for (item_id, quantity) in items {
        simulation.item_world.consume(item_id, quantity).unwrap();
    }
}

pub(super) fn production_zone_cells(
    simulation: &Simulation,
    workstation_id: EntityId,
    kind: ProductionZoneKind,
) -> Vec<WorldCell> {
    simulation
        .production_logistics_world
        .get(workstation_id)
        .unwrap()
        .cells(kind)
        .collect()
}

pub(super) fn place_clear_workbench(simulation: &mut Simulation) -> EntityId {
    let cell = (-5..=5)
        .flat_map(|y| (-7..=7).map(move |x| WorldCell::new(x, y)))
        .find(|cell| {
            simulation.validate_workstation_cell(*cell).is_ok()
                && simulation
                    .construction_cell_has_removable_occupant(*cell)
                    .is_ok_and(|occupied| !occupied)
                && simulation.default_production_ports(*cell).is_ok()
        })
        .expect("seed 0 must expose a clear workbench fixture");
    let id = simulation
        .place_workstation(workstation::WORKBENCH, cell)
        .unwrap();
    assert_eq!(simulation.workstation_at(cell), Some(id));
    id
}

pub(super) fn insert_ground_stack(
    simulation: &mut Simulation,
    kind: ItemId,
    quantity: u32,
    cell: WorldCell,
) -> EntityId {
    let id = simulation.id_allocator.allocate().unwrap();
    simulation
        .item_world
        .insert_ground(ItemStack::new_ground(
            id,
            kind,
            ItemQuantity::new(quantity).unwrap(),
            WorldPosition::from_cell_center(cell).unwrap(),
        ))
        .unwrap();
    id
}

pub(super) fn seed_recipe_inputs(
    simulation: &mut Simulation,
    workstation_id: EntityId,
    wood: u32,
    stone: u32,
) -> (EntityId, EntityId) {
    let inputs = production_zone_cells(simulation, workstation_id, ProductionZoneKind::Input);
    assert!(inputs.len() >= 2, "workbench fixture needs two Input cells");
    let wood_id = insert_ground_stack(simulation, item::WOOD, wood, inputs[0]);
    let stone_id = insert_ground_stack(simulation, item::STONE, stone, inputs[1]);
    (wood_id, stone_id)
}

pub(super) fn total_item_quantity(simulation: &Simulation, kind: ItemId) -> u32 {
    simulation
        .items()
        .filter(|item| item.kind() == kind)
        .map(|item| item.quantity().get())
        .sum()
}

pub(super) fn distant_stockpile_cells(
    simulation: &Simulation,
    origin: WorldCell,
    count: usize,
) -> Vec<WorldCell> {
    let cells = (-5..=5)
        .flat_map(|y| (-7..=7).map(move |x| WorldCell::new(x, y)))
        .filter(|cell| cell_manhattan_distance(*cell, origin) >= 4)
        .filter(|cell| simulation.validate_stockpile_cell(*cell).is_ok())
        .take(count)
        .collect::<Vec<_>>();
    assert_eq!(
        cells.len(),
        count,
        "seed 0 must expose distant stockpile cells"
    );
    cells
}

pub(super) fn shared_workbench_fixture_cells(
    simulation: &Simulation,
) -> (WorldCell, WorldCell, WorldCell, WorldCell, WorldCell) {
    for y in -4..=4 {
        for x in -5..=5 {
            let shared = WorldCell::new(x, y);
            let first_bench = WorldCell::new(x - 1, y);
            let second_bench = WorldCell::new(x + 1, y);
            let first_stone = WorldCell::new(x - 1, y + 1);
            let second_stone = WorldCell::new(x + 1, y + 1);
            if simulation.validate_stockpile_cell(shared).is_ok()
                && simulation.validate_stockpile_cell(first_stone).is_ok()
                && simulation.validate_stockpile_cell(second_stone).is_ok()
                && simulation.validate_workstation_cell(first_bench).is_ok()
                && simulation.validate_workstation_cell(second_bench).is_ok()
                && simulation
                    .construction_cell_has_removable_occupant(first_bench)
                    .is_ok_and(|occupied| !occupied)
                && simulation
                    .construction_cell_has_removable_occupant(second_bench)
                    .is_ok_and(|occupied| !occupied)
                && simulation.default_production_ports(first_bench).is_ok()
                && simulation.default_production_ports(second_bench).is_ok()
            {
                return (shared, first_bench, second_bench, first_stone, second_stone);
            }
        }
    }
    panic!("seed 0 must expose a shared-input two-workbench fixture");
}

pub(super) fn empty_stockpile_cells(simulation: &Simulation, count: usize) -> Vec<WorldCell> {
    let occupied = simulation
        .items()
        .filter_map(ItemStack::ground_position)
        .map(WorldPosition::containing_cell)
        .collect::<BTreeSet<_>>();
    let cells = (-5..=5)
        .flat_map(|y| (-7..=7).map(move |x| WorldCell::new(x, y)))
        .filter(|cell| simulation.is_explored(*cell))
        .filter(|cell| simulation.effective_terrain_at(*cell).unwrap() == terrain::GRASS)
        .filter(|cell| simulation.natural_resource_at(*cell).unwrap().is_none())
        .filter(|cell| !occupied.contains(cell))
        .filter(|cell| {
            simulation
                .characters()
                .all(|character| character.position().containing_cell() != *cell)
        })
        .take(count)
        .collect::<Vec<_>>();
    assert_eq!(
        cells.len(),
        count,
        "seed 0 must expose enough empty stockpile cells"
    );
    cells
}

pub(super) fn transporting_haul_fixture(
    simulation: &mut Simulation,
) -> (EntityId, EntityId, EntityId) {
    for _ in 0..64 {
        simulation.advance_ticks(1).unwrap();
        if let Some(job) = simulation.jobs().find(|job| {
            matches!(job.state(), JobState::Transporting { .. })
                && matches!(job.kind(), JobKind::Haul { .. })
        }) {
            let JobKind::Haul { item_id, .. } = job.kind() else {
                unreachable!();
            };
            return (job.id(), item_id, job.state().worker().unwrap());
        }
    }
    panic!("expected a haul job to enter Transporting state");
}

pub(super) fn harvest_fixture(simulation: &Simulation) -> (WorldCell, NaturalResource) {
    for y in -5..=5 {
        for x in -7..=7 {
            let cell = WorldCell::new(x, y);
            if !simulation.is_explored(cell) {
                continue;
            }
            let Some(resource) = simulation.natural_resource_at(cell).unwrap() else {
                continue;
            };
            if resource.kind() == natural_resource::BERRY_BUSH {
                continue;
            }
            let destination = WorldPosition::from_cell_center(cell).unwrap();
            if simulation.characters().any(|character| {
                simulation
                    .plan_navigation_route(character.id(), destination)
                    .is_ok()
            }) {
                return (cell, resource);
            }
        }
    }
    panic!("seed 0 must expose at least one reachable natural-resource source");
}

pub(super) fn character(simulation: &Simulation, id: EntityId) -> &Character {
    simulation.characters.get(&id).unwrap()
}

pub(super) fn character_mut(simulation: &mut Simulation, id: EntityId) -> &mut Character {
    simulation.characters.get_mut(&id).unwrap()
}

pub(super) fn position_at_cell(cell: WorldCell) -> WorldPosition {
    WorldPosition::from_cell_center(cell).unwrap()
}

pub(super) fn blocked_stop_position(source: WorldCell, direction: Direction) -> WorldPosition {
    let origin = WorldPosition::from_cell_origin(source).unwrap();
    match direction {
        Direction::East => origin.checked_translate(1023, 512).unwrap(),
        Direction::West => origin.checked_translate(0, 512).unwrap(),
        Direction::North => origin.checked_translate(512, 1023).unwrap(),
        Direction::South => origin.checked_translate(512, 0).unwrap(),
    }
}

pub(super) fn near_cell_edge(cell: WorldCell, direction: Direction) -> WorldPosition {
    let origin = WorldPosition::from_cell_origin(cell).unwrap();
    match direction {
        Direction::East => origin.checked_translate(1022, 512).unwrap(),
        Direction::West => origin.checked_translate(1, 512).unwrap(),
        Direction::North => origin.checked_translate(512, 1022).unwrap(),
        Direction::South => origin.checked_translate(512, 1).unwrap(),
    }
}

pub(super) fn place_on_grass(simulation: &mut Simulation, id: EntityId, position: WorldCell) {
    assert_eq!(
        simulation.effective_terrain_at(position).unwrap(),
        terrain::GRASS
    );
    simulation
        .characters
        .get_mut(&id)
        .unwrap()
        .set_position(position_at_cell(position));
}

pub(super) fn persisted_movement_stops_on(terrain: TerrainId) {
    let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
    let cora = cora();
    let (position, direction) = find_adjacent_terrain_fixture(&simulation, terrain);
    place_on_grass(&mut simulation, cora, position);
    simulation
        .characters
        .get_mut(&cora)
        .unwrap()
        .set_movement(MovementState::ManualDirectional { direction });

    simulation.advance_ticks(3).unwrap();

    assert_eq!(
        character(&simulation, cora).position(),
        blocked_stop_position(position, direction)
    );
    assert_eq!(character(&simulation, cora).movement(), MovementState::Idle);
}

pub(super) fn find_replacement_fixture(
    simulation: &Simulation,
) -> (WorldCell, Direction, Direction) {
    for y in -64..=64 {
        for x in -64..=64 {
            let position = WorldCell::new(x, y);
            if raw_terrain_at(simulation, position) != terrain::GRASS {
                continue;
            }
            for accepted in [
                Direction::East,
                Direction::West,
                Direction::North,
                Direction::South,
            ] {
                if raw_terrain_at(simulation, accepted.adjacent(position).unwrap())
                    != terrain::GRASS
                {
                    continue;
                }
                for blocked in [
                    Direction::East,
                    Direction::West,
                    Direction::North,
                    Direction::South,
                ] {
                    if raw_terrain_at(simulation, blocked.adjacent(position).unwrap())
                        != terrain::GRASS
                    {
                        return (position, accepted, blocked);
                    }
                }
            }
        }
    }
    panic!("expected an adjacent grass and blocked terrain fixture");
}

pub(super) fn find_turn_fixture(simulation: &Simulation) -> WorldCell {
    for y in -64..=64 {
        for x in -64..=64 {
            let position = WorldCell::new(x, y);
            let east = Direction::East.adjacent(position).unwrap();
            let north_from_east = Direction::North.adjacent(east).unwrap();
            if [position, east, north_from_east]
                .into_iter()
                .all(|cell| raw_terrain_at(simulation, cell) == terrain::GRASS)
            {
                return position;
            }
        }
    }
    panic!("expected an east then north grass fixture");
}

pub(super) fn find_adjacent_terrain_fixture(
    simulation: &Simulation,
    target_terrain: TerrainId,
) -> (WorldCell, Direction) {
    for y in -64..=64 {
        for x in -64..=64 {
            let position = WorldCell::new(x, y);
            if raw_terrain_at(simulation, position) != terrain::GRASS {
                continue;
            }
            for direction in [
                Direction::East,
                Direction::West,
                Direction::North,
                Direction::South,
            ] {
                if raw_terrain_at(simulation, direction.adjacent(position).unwrap())
                    == target_terrain
                {
                    return (position, direction);
                }
            }
        }
    }
    panic!("expected adjacent {target_terrain:?} terrain fixture");
}

pub(super) fn raw_terrain_at(simulation: &Simulation, position: WorldCell) -> TerrainId {
    let (coordinate, local) = position.split();
    simulation
        .generated_chunk(coordinate)
        .unwrap()
        .terrain_at(local)
        .unwrap()
}

pub(super) fn find_raw_grass_with_neighbor(
    simulation: &Simulation,
    neighbor: TerrainId,
) -> (WorldCell, Direction) {
    for y in -64..=64 {
        for x in -64..=64 {
            let start = WorldCell::new(x, y);
            if raw_terrain_at(simulation, start) != terrain::GRASS {
                continue;
            }
            for direction in [
                Direction::East,
                Direction::West,
                Direction::North,
                Direction::South,
            ] {
                let target = direction.adjacent(start).unwrap();
                if raw_terrain_at(simulation, target) == neighbor {
                    return (start, direction);
                }
            }
        }
    }
    panic!("expected raw grass next to {neighbor:?}");
}
