use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, btree_map::Entry};

use crate::{
    ChunkCoord, Direction, EffectiveChunk, Simulation, SimulationError, Terrain, WorldCell,
};

pub(crate) const PATHFINDING_NODE_BUDGET: usize = 50_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PathfindingError {
    PathNotFound,
    SearchBudgetExceeded,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct OpenNode {
    f_score: u128,
    h_score: u128,
    insertion: u64,
    cell: WorldCell,
}

#[cfg(test)]
pub(crate) fn find_path(
    simulation: &Simulation,
    start: WorldCell,
    goal: WorldCell,
) -> Result<Result<Vec<WorldCell>, PathfindingError>, SimulationError> {
    find_path_with_budget(simulation, start, goal, PATHFINDING_NODE_BUDGET, false)
}

pub(crate) fn find_explored_path(
    simulation: &Simulation,
    start: WorldCell,
    goal: WorldCell,
) -> Result<Result<Vec<WorldCell>, PathfindingError>, SimulationError> {
    find_path_with_budget(simulation, start, goal, PATHFINDING_NODE_BUDGET, true)
}

pub(crate) fn find_closest_explored_path(
    simulation: &Simulation,
    start: WorldCell,
    goal: WorldCell,
) -> Result<Result<Vec<WorldCell>, PathfindingError>, SimulationError> {
    let mut chunks = BTreeMap::<ChunkCoord, EffectiveChunk>::new();
    let mut open = BinaryHeap::new();
    let mut costs = BTreeMap::from([(start, 0_usize)]);
    let mut predecessors = BTreeMap::<WorldCell, WorldCell>::new();
    let mut insertion = 0_u64;
    let h_score = manhattan(start, goal);
    let mut best = (h_score, 0_usize, start);
    open.push(Reverse(OpenNode {
        f_score: h_score,
        h_score,
        insertion,
        cell: start,
    }));
    let mut expanded = 0_usize;

    while let Some(Reverse(node)) = open.pop() {
        let Some(&cost) = costs.get(&node.cell) else {
            continue;
        };
        if node.f_score != u128::from(cost as u64) + manhattan(node.cell, goal) {
            continue;
        }
        if expanded == PATHFINDING_NODE_BUDGET {
            return Ok(Err(PathfindingError::SearchBudgetExceeded));
        }
        expanded += 1;
        best = best.min((manhattan(node.cell, goal), cost, node.cell));

        for direction in [
            Direction::East,
            Direction::North,
            Direction::South,
            Direction::West,
        ] {
            let Some(next) = direction.adjacent(node.cell) else {
                continue;
            };
            if next == start {
                continue;
            }
            let Some(step_cost) = traversal_cost(simulation, next, &mut chunks, true)? else {
                continue;
            };
            let next_cost = cost + step_cost;
            if costs.get(&next).is_some_and(|known| *known <= next_cost) {
                continue;
            }
            costs.insert(next, next_cost);
            predecessors.insert(next, node.cell);
            insertion = insertion
                .checked_add(1)
                .expect("A* insertion sequence overflow");
            let h_score = manhattan(next, goal);
            open.push(Reverse(OpenNode {
                f_score: u128::from(next_cost as u64) + h_score,
                h_score,
                insertion,
                cell: next,
            }));
        }
    }

    Ok(Ok(reconstruct_path(start, best.2, &predecessors)))
}

fn find_path_with_budget(
    simulation: &Simulation,
    start: WorldCell,
    goal: WorldCell,
    budget: usize,
    require_explored: bool,
) -> Result<Result<Vec<WorldCell>, PathfindingError>, SimulationError> {
    let mut chunks = BTreeMap::<ChunkCoord, EffectiveChunk>::new();
    if traversal_cost(simulation, goal, &mut chunks, require_explored)?.is_none() {
        return Ok(Err(PathfindingError::PathNotFound));
    }

    let mut open = BinaryHeap::new();
    let mut costs = BTreeMap::from([(start, 0_usize)]);
    let mut predecessors = BTreeMap::<WorldCell, WorldCell>::new();
    let mut insertion = 0_u64;
    let h_score = manhattan(start, goal);
    open.push(Reverse(OpenNode {
        f_score: h_score,
        h_score,
        insertion,
        cell: start,
    }));
    let mut expanded = 0_usize;

    while let Some(Reverse(node)) = open.pop() {
        let Some(&cost) = costs.get(&node.cell) else {
            continue;
        };
        if node.f_score != u128::from(cost as u64) + manhattan(node.cell, goal) {
            continue;
        }
        if expanded == budget {
            return Ok(Err(PathfindingError::SearchBudgetExceeded));
        }
        expanded += 1;
        if node.cell == goal {
            return Ok(Ok(reconstruct_path(start, goal, &predecessors)));
        }

        for direction in [
            Direction::East,
            Direction::North,
            Direction::South,
            Direction::West,
        ] {
            let Some(next) = direction.adjacent(node.cell) else {
                continue;
            };
            if next == start {
                continue;
            }
            let Some(step_cost) = traversal_cost(simulation, next, &mut chunks, require_explored)?
            else {
                continue;
            };
            let next_cost = cost + step_cost;
            if costs.get(&next).is_some_and(|known| *known <= next_cost) {
                continue;
            }
            costs.insert(next, next_cost);
            predecessors.insert(next, node.cell);
            insertion = insertion
                .checked_add(1)
                .expect("A* insertion sequence overflow");
            let h_score = manhattan(next, goal);
            open.push(Reverse(OpenNode {
                f_score: u128::from(next_cost as u64) + h_score,
                h_score,
                insertion,
                cell: next,
            }));
        }
    }

    Ok(Err(PathfindingError::PathNotFound))
}

fn traversal_cost(
    simulation: &Simulation,
    cell: WorldCell,
    chunks: &mut BTreeMap<ChunkCoord, EffectiveChunk>,
    require_explored: bool,
) -> Result<Option<usize>, SimulationError> {
    if require_explored && !simulation.is_explored(cell) {
        return Ok(None);
    }
    let structure_cost = match simulation.structure_kind_at(cell) {
        Some(kind) => match kind.navigation_cost() {
            Some(cost) => cost,
            None => return Ok(None),
        },
        None => 1,
    };
    let (coordinate, local) = cell.split();
    if let Entry::Vacant(entry) = chunks.entry(coordinate) {
        entry.insert(simulation.effective_chunk(coordinate)?);
    }
    Ok((chunks[&coordinate].terrain_at(local) == Some(Terrain::Grass)).then_some(structure_cost))
}

fn manhattan(first: WorldCell, second: WorldCell) -> u128 {
    u128::from(first.x().abs_diff(second.x())) + u128::from(first.y().abs_diff(second.y()))
}

fn reconstruct_path(
    start: WorldCell,
    goal: WorldCell,
    predecessors: &BTreeMap<WorldCell, WorldCell>,
) -> Vec<WorldCell> {
    let mut path = vec![goal];
    while *path.last().expect("path starts at goal") != start {
        path.push(predecessors[path.last().expect("path is nonempty")]);
    }
    path.reverse();
    path
}

#[cfg(test)]
mod tests {
    use crate::{Simulation, Terrain, WorldCell, WorldSeed};

    use super::{PathfindingError, find_path, find_path_with_budget};

    #[test]
    fn equal_cost_obstacle_path_has_a_stable_golden_cell_sequence() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        for cell in [
            WorldCell::new(0, 0),
            WorldCell::new(1, 0),
            WorldCell::new(2, 0),
            WorldCell::new(0, 1),
            WorldCell::new(1, 1),
            WorldCell::new(2, 1),
            WorldCell::new(0, -1),
            WorldCell::new(1, -1),
            WorldCell::new(2, -1),
        ] {
            simulation
                .set_terrain_override(cell, Terrain::Grass)
                .unwrap();
        }
        simulation
            .set_terrain_override(WorldCell::new(1, 0), Terrain::Rock)
            .unwrap();

        assert_eq!(
            find_path(&simulation, WorldCell::new(0, 0), WorldCell::new(2, 0)).unwrap(),
            Ok(vec![
                WorldCell::new(0, 0),
                WorldCell::new(0, 1),
                WorldCell::new(1, 1),
                WorldCell::new(2, 1),
                WorldCell::new(2, 0),
            ])
        );
    }

    #[test]
    fn blocked_start_can_exit_but_cannot_be_reentered() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        for cell in [WorldCell::new(1, 0), WorldCell::new(2, 0)] {
            simulation
                .set_terrain_override(cell, Terrain::Grass)
                .unwrap();
        }
        simulation
            .set_terrain_override(WorldCell::new(0, 0), Terrain::Rock)
            .unwrap();

        assert_eq!(
            find_path(&simulation, WorldCell::new(0, 0), WorldCell::new(2, 0)).unwrap(),
            Ok(vec![
                WorldCell::new(0, 0),
                WorldCell::new(1, 0),
                WorldCell::new(2, 0)
            ])
        );
    }

    #[test]
    fn exhausted_frontier_and_budget_are_distinct_errors() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        for cell in [
            WorldCell::new(0, 0),
            WorldCell::new(2, 0),
            WorldCell::new(-1, 0),
            WorldCell::new(0, 1),
            WorldCell::new(0, -1),
        ] {
            simulation
                .set_terrain_override(cell, Terrain::Grass)
                .unwrap();
        }
        for cell in [
            WorldCell::new(1, 0),
            WorldCell::new(-1, 0),
            WorldCell::new(0, 1),
            WorldCell::new(0, -1),
        ] {
            simulation
                .set_terrain_override(cell, Terrain::Rock)
                .unwrap();
        }
        assert_eq!(
            find_path_with_budget(
                &simulation,
                WorldCell::new(0, 0),
                WorldCell::new(2, 0),
                1,
                false,
            )
            .unwrap(),
            Err(PathfindingError::PathNotFound)
        );

        simulation
            .set_terrain_override(WorldCell::new(1, 0), Terrain::Grass)
            .unwrap();
        assert_eq!(
            find_path_with_budget(
                &simulation,
                WorldCell::new(0, 0),
                WorldCell::new(2, 0),
                1,
                false,
            )
            .unwrap(),
            Err(PathfindingError::SearchBudgetExceeded)
        );
    }

    #[test]
    fn effective_overrides_block_and_open_the_same_route() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        for cell in [
            WorldCell::new(0, 0),
            WorldCell::new(1, 0),
            WorldCell::new(2, 0),
        ] {
            simulation
                .set_terrain_override(cell, Terrain::Grass)
                .unwrap();
        }
        for cell in [
            WorldCell::new(-1, 0),
            WorldCell::new(0, 1),
            WorldCell::new(0, -1),
        ] {
            simulation
                .set_terrain_override(cell, Terrain::Rock)
                .unwrap();
        }
        simulation
            .set_terrain_override(WorldCell::new(1, 0), Terrain::Rock)
            .unwrap();
        assert_eq!(
            find_path(&simulation, WorldCell::new(0, 0), WorldCell::new(2, 0)).unwrap(),
            Err(PathfindingError::PathNotFound)
        );
        simulation
            .set_terrain_override(WorldCell::new(1, 0), Terrain::Grass)
            .unwrap();
        assert_eq!(
            find_path(&simulation, WorldCell::new(0, 0), WorldCell::new(2, 0)).unwrap(),
            Ok(vec![
                WorldCell::new(0, 0),
                WorldCell::new(1, 0),
                WorldCell::new(2, 0)
            ])
        );
    }

    #[test]
    fn pathfinding_crosses_chunk_boundary_without_global_grid() {
        let mut simulation = Simulation::new(WorldSeed::new(2)).unwrap();
        for cell in [WorldCell::new(31, 0), WorldCell::new(32, 0)] {
            simulation
                .set_terrain_override(cell, Terrain::Grass)
                .unwrap();
        }
        assert_eq!(
            find_path(&simulation, WorldCell::new(31, 0), WorldCell::new(32, 0)).unwrap(),
            Ok(vec![WorldCell::new(31, 0), WorldCell::new(32, 0)])
        );
    }
}
