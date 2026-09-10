//! Buildable structures: what they cost, how long they take, and how they
//! behave once finished.

use crate::item::{self, ItemId};
use crate::registry::content_handle;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StructureDefinition {
    /// Stable identity used by saves and localization.
    pub name: &'static str,
    /// The physical item a site must receive before work may begin.
    pub material: ItemId,
    pub material_quantity: u32,
    pub work_ticks: u32,
    /// Pathfinding cost once finished. `None` blocks movement entirely.
    pub navigation_cost: Option<usize>,
    /// Whether the structure joins the cardinal wall network of ADR-0011,
    /// which also decides whether it encloses a space.
    pub connects_to_wall_network: bool,
    /// Whether the structure opens and closes, like a door, instead of being
    /// permanently solid.
    pub has_open_state: bool,
}

/// Append-only: registry order is part of deterministic simulation outcomes.
pub static STRUCTURES: &[StructureDefinition] = &[
    StructureDefinition {
        name: "stone_wall",
        material: item::STONE,
        material_quantity: 2,
        work_ticks: 8,
        navigation_cost: None,
        connects_to_wall_network: true,
        has_open_state: false,
    },
    StructureDefinition {
        name: "door",
        material: item::WOOD,
        material_quantity: 2,
        work_ticks: 6,
        navigation_cost: Some(2),
        connects_to_wall_network: true,
        has_open_state: true,
    },
];

content_handle!(StructureId, StructureDefinition, STRUCTURES, structure);

impl StructureId {
    /// A structure that does not block movement can be walked over.
    pub const fn is_passable(self) -> bool {
        self.definition().navigation_cost.is_some()
    }
}

pub const STONE_WALL: StructureId = structure("stone_wall");
pub const DOOR: StructureId = structure("door");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_constants_address_their_own_definitions() {
        for (id, name) in [(STONE_WALL, "stone_wall"), (DOOR, "door")] {
            assert_eq!(id.name(), name);
            assert_eq!(StructureId::from_name(name), Some(id));
        }
    }

    #[test]
    fn costs_and_passability_are_definition_fields() {
        assert_eq!(STONE_WALL.definition().material, item::STONE);
        assert_eq!(STONE_WALL.definition().material_quantity, 2);
        assert!(!STONE_WALL.is_passable());
        assert_eq!(DOOR.definition().material, item::WOOD);
        assert_eq!(DOOR.definition().navigation_cost, Some(2));
        assert!(DOOR.is_passable());
    }

    #[test]
    fn every_structure_costs_real_material_and_real_work() {
        for id in StructureId::all() {
            let definition = id.definition();
            assert!(
                definition.material_quantity > 0,
                "{} builds itself from nothing",
                id.name()
            );
            assert!(
                definition.work_ticks > 0,
                "{} finishes without work",
                id.name()
            );
            assert!(
                definition.material_quantity <= item::MAX_STACK_QUANTITY,
                "{} cannot be supplied by one stack",
                id.name()
            );
        }
    }

    #[test]
    fn only_passable_structures_can_open_and_close() {
        assert!(DOOR.definition().has_open_state);
        assert!(!STONE_WALL.definition().has_open_state);
        for id in StructureId::all().filter(|id| id.definition().has_open_state) {
            assert!(
                id.is_passable(),
                "{} opens and closes but never lets anyone through",
                id.name()
            );
        }
    }

    #[test]
    fn unknown_names_resolve_to_nothing_rather_than_a_substitute() {
        assert_eq!(StructureId::from_name("bed"), None);
    }
}
