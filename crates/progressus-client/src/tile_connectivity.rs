use std::collections::BTreeSet;

use progressus_app::WorldCell;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct CardinalConnections(u8);

impl CardinalConnections {
    pub(crate) const NORTH: u8 = 1 << 0;
    pub(crate) const EAST: u8 = 1 << 1;
    pub(crate) const SOUTH: u8 = 1 << 2;
    pub(crate) const WEST: u8 = 1 << 3;

    pub(crate) fn from_cells(cell: WorldCell, cells: &BTreeSet<WorldCell>) -> Self {
        let mut bits = 0;
        if neighbour(cell, 0, 1).is_some_and(|next| cells.contains(&next)) {
            bits |= Self::NORTH;
        }
        if neighbour(cell, 1, 0).is_some_and(|next| cells.contains(&next)) {
            bits |= Self::EAST;
        }
        if neighbour(cell, 0, -1).is_some_and(|next| cells.contains(&next)) {
            bits |= Self::SOUTH;
        }
        if neighbour(cell, -1, 0).is_some_and(|next| cells.contains(&next)) {
            bits |= Self::WEST;
        }
        Self(bits)
    }

    pub(crate) const fn bits(self) -> u8 {
        self.0
    }
}

fn neighbour(cell: WorldCell, dx: i64, dy: i64) -> Option<WorldCell> {
    Some(WorldCell::new(
        cell.x().checked_add(dx)?,
        cell.y().checked_add(dy)?,
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use progressus_app::WorldCell;

    use super::CardinalConnections;

    #[test]
    fn cardinal_mask_covers_straights_corners_tees_and_crosses() {
        let center = WorldCell::new(0, 0);
        let cells = [
            center,
            WorldCell::new(0, 1),
            WorldCell::new(1, 0),
            WorldCell::new(0, -1),
            WorldCell::new(-1, 0),
        ]
        .into_iter()
        .collect::<BTreeSet<_>>();
        assert_eq!(
            CardinalConnections::from_cells(center, &cells).bits(),
            0b1111
        );

        let corner = [center, WorldCell::new(0, 1), WorldCell::new(1, 0)]
            .into_iter()
            .collect::<BTreeSet<_>>();
        assert_eq!(
            CardinalConnections::from_cells(center, &corner).bits(),
            0b0011
        );
    }
    #[test]
    fn horizontal_pair_is_reciprocal_and_preserves_west_bit() {
        let left = WorldCell::new(10, -4);
        let right = WorldCell::new(11, -4);
        let cells = [left, right].into_iter().collect::<BTreeSet<_>>();

        assert_eq!(
            CardinalConnections::from_cells(left, &cells).bits(),
            CardinalConnections::EAST
        );
        assert_eq!(
            CardinalConnections::from_cells(right, &cells).bits(),
            CardinalConnections::WEST
        );
        assert_eq!(CardinalConnections::WEST, 0b1000);
    }

    #[test]
    fn vertical_pair_uses_positive_world_y_as_north() {
        let south = WorldCell::new(3, 8);
        let north = WorldCell::new(3, 9);
        let cells = [south, north].into_iter().collect::<BTreeSet<_>>();

        assert_eq!(
            CardinalConnections::from_cells(south, &cells).bits(),
            CardinalConnections::NORTH
        );
        assert_eq!(
            CardinalConnections::from_cells(north, &cells).bits(),
            CardinalConnections::SOUTH
        );
    }
}
