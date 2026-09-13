//! Bounded, on-demand enclosure test for physical shelter.

use super::*;

const ENCLOSURE_CELL_BUDGET: usize = 256;

impl Simulation {
    pub(super) fn is_enclosed(&self, start: WorldCell) -> Result<bool, SimulationError> {
        if !self.shelter_fill_passable(start)? {
            return Ok(false);
        }
        let mut visited = BTreeSet::from([start]);
        let mut frontier = VecDeque::from([start]);
        while let Some(cell) = frontier.pop_front() {
            for direction in [
                Direction::East,
                Direction::West,
                Direction::North,
                Direction::South,
            ] {
                let Some(neighbor) = direction.adjacent(cell) else {
                    continue;
                };
                if visited.contains(&neighbor) || !self.shelter_fill_passable(neighbor)? {
                    continue;
                }
                visited.insert(neighbor);
                if visited.len() > ENCLOSURE_CELL_BUDGET {
                    return Ok(false);
                }
                frontier.push_back(neighbor);
            }
        }
        Ok(true)
    }

    fn shelter_fill_passable(&self, cell: WorldCell) -> Result<bool, SimulationError> {
        if self.effective_terrain_at(cell)? != terrain::GRASS {
            return Ok(false);
        }
        Ok(!self
            .structure_kind_at(cell)
            .is_some_and(|kind| kind.definition().connects_to_wall_network))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use progressus_content::structure;

    fn grass_square(simulation: &mut Simulation, radius: i64) {
        for y in -radius..=radius {
            for x in -radius..=radius {
                simulation
                    .set_terrain_override(WorldCell::new(x, y), terrain::GRASS)
                    .unwrap();
            }
        }
    }

    fn complete_structure(simulation: &mut Simulation, kind: StructureId, cell: WorldCell) {
        let id = simulation.id_allocator.allocate().unwrap();
        simulation
            .construction_world
            .insert_site(ConstructionSite::new(id, kind, cell))
            .unwrap();
        simulation.construction_world.complete_site(id).unwrap();
    }

    #[test]
    fn enclosure_closed_wall_ring_is_shelter_but_one_gap_opens_it() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        grass_square(&mut simulation, 20);
        for y in -1_i64..=1 {
            for x in -1_i64..=1 {
                if x.abs() == 1 || y.abs() == 1 {
                    complete_structure(
                        &mut simulation,
                        structure::STONE_WALL,
                        WorldCell::new(x, y),
                    );
                }
            }
        }
        assert!(simulation.is_enclosed(WorldCell::new(0, 0)).unwrap());
        let gap = WorldCell::new(0, 1);
        let id = simulation.structure_at(gap).unwrap();
        simulation.construction_world.remove_structure(id).unwrap();
        assert!(!simulation.is_enclosed(WorldCell::new(0, 0)).unwrap());

        simulation
            .construction_world
            .insert_site(ConstructionSite::new(id, structure::STONE_WALL, gap))
            .unwrap();
        assert!(!simulation.is_enclosed(WorldCell::new(0, 0)).unwrap());
        simulation.construction_world.remove_site(id).unwrap();

        complete_structure(&mut simulation, structure::DOOR, gap);
        assert!(simulation.is_enclosed(WorldCell::new(0, 0)).unwrap());
        let door = simulation.structure_at(gap).unwrap();
        simulation
            .construction_world
            .set_door_open_until(door, Some(SimulationTick::new(10)))
            .unwrap();
        assert!(simulation.is_enclosed(WorldCell::new(0, 0)).unwrap());
    }

    #[test]
    fn enclosure_rock_pocket_shelters_but_bed_and_large_open_area_do_not() {
        let mut simulation = Simulation::new(WorldSeed::new(0)).unwrap();
        grass_square(&mut simulation, 20);
        let start = WorldCell::new(0, 0);
        let exploration_before = simulation.exploration_revision();
        let residency_before = simulation.residency_revision();
        let chunks_before = simulation.resident_chunks().collect::<Vec<_>>();
        complete_structure(&mut simulation, structure::BED, start);
        assert!(!simulation.is_enclosed(start).unwrap());
        for direction in [
            Direction::East,
            Direction::West,
            Direction::North,
            Direction::South,
        ] {
            simulation
                .set_terrain_override(direction.adjacent(start).unwrap(), terrain::ROCK)
                .unwrap();
        }
        assert!(simulation.is_enclosed(start).unwrap());
        assert_eq!(simulation.exploration_revision(), exploration_before);
        assert_eq!(simulation.residency_revision(), residency_before);
        assert_eq!(
            simulation.resident_chunks().collect::<Vec<_>>(),
            chunks_before
        );
    }
}
