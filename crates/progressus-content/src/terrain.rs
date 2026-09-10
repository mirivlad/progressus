//! Base terrain kinds.

use crate::registry::content_handle;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerrainDefinition {
    /// Stable identity used by saves and localization.
    pub name: &'static str,
    /// Whether characters and pathfinding may enter this terrain.
    pub walkable: bool,
}

/// Append-only: registry order is part of deterministic simulation outcomes.
pub static TERRAINS: &[TerrainDefinition] = &[
    TerrainDefinition {
        name: "grass",
        walkable: true,
    },
    TerrainDefinition {
        name: "water",
        walkable: false,
    },
    TerrainDefinition {
        name: "rock",
        walkable: false,
    },
];

content_handle!(TerrainId, TerrainDefinition, TERRAINS, terrain);

impl TerrainId {
    pub const fn is_walkable(self) -> bool {
        self.definition().walkable
    }
}

pub const GRASS: TerrainId = terrain("grass");
pub const WATER: TerrainId = terrain("water");
pub const ROCK: TerrainId = terrain("rock");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_constants_address_their_own_definitions() {
        for (id, name) in [(GRASS, "grass"), (WATER, "water"), (ROCK, "rock")] {
            assert_eq!(id.name(), name);
            assert_eq!(TerrainId::from_name(name), Some(id));
        }
    }

    #[test]
    fn walkability_is_a_property_of_the_definition() {
        assert!(GRASS.is_walkable());
        assert!(!WATER.is_walkable());
        assert!(!ROCK.is_walkable());
    }

    #[test]
    fn unknown_names_resolve_to_nothing_rather_than_a_substitute() {
        assert_eq!(TerrainId::from_name("sand"), None);
        assert_eq!(TerrainId::from_name("Grass"), None);
    }
}
