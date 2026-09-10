//! Gatherable resources that worldgen places in the world.
//!
//! How much a particular deposit yields is decided per cell by worldgen and
//! travels on the placed resource. What harvesting it produces, and whether it
//! comes back, are properties of the kind and live here.

use crate::item::{self, ItemId};
use crate::registry::content_handle;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NaturalResourceDefinition {
    /// Stable identity used by saves and localization.
    pub name: &'static str,
    /// The physical item one harvest produces.
    pub yields: ItemId,
    /// Ticks until a harvested source is gatherable again.
    /// `None` means harvesting depletes it permanently.
    pub regrow_ticks: Option<u64>,
}

/// Append-only: registry order is part of deterministic simulation outcomes.
pub static NATURAL_RESOURCES: &[NaturalResourceDefinition] = &[
    NaturalResourceDefinition {
        name: "tree",
        yields: item::WOOD,
        regrow_ticks: None,
    },
    NaturalResourceDefinition {
        name: "stone_outcrop",
        yields: item::STONE,
        regrow_ticks: None,
    },
    NaturalResourceDefinition {
        name: "berry_bush",
        yields: item::BERRIES,
        regrow_ticks: Some(512),
    },
    NaturalResourceDefinition {
        name: "copper_vein",
        yields: item::COPPER_ORE,
        regrow_ticks: None,
    },
];

content_handle!(
    NaturalResourceId,
    NaturalResourceDefinition,
    NATURAL_RESOURCES,
    natural_resource
);

impl NaturalResourceId {
    /// Renewable sources regrow instead of depleting permanently.
    pub const fn is_renewable(self) -> bool {
        self.definition().regrow_ticks.is_some()
    }
}

pub const TREE: NaturalResourceId = natural_resource("tree");
pub const STONE_OUTCROP: NaturalResourceId = natural_resource("stone_outcrop");
pub const BERRY_BUSH: NaturalResourceId = natural_resource("berry_bush");
pub const COPPER_VEIN: NaturalResourceId = natural_resource("copper_vein");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_constants_address_their_own_definitions() {
        for (id, name) in [
            (TREE, "tree"),
            (STONE_OUTCROP, "stone_outcrop"),
            (BERRY_BUSH, "berry_bush"),
            (COPPER_VEIN, "copper_vein"),
        ] {
            assert_eq!(id.name(), name);
            assert_eq!(NaturalResourceId::from_name(name), Some(id));
        }
    }

    #[test]
    fn harvest_output_and_renewability_are_definition_fields() {
        assert_eq!(TREE.definition().yields, item::WOOD);
        assert_eq!(STONE_OUTCROP.definition().yields, item::STONE);
        assert_eq!(BERRY_BUSH.definition().yields, item::BERRIES);
        assert!(!TREE.is_renewable());
        assert!(!STONE_OUTCROP.is_renewable());
        assert_eq!(BERRY_BUSH.definition().regrow_ticks, Some(512));
        assert!(BERRY_BUSH.is_renewable());
        assert_eq!(COPPER_VEIN.definition().yields, item::COPPER_ORE);
        assert!(!COPPER_VEIN.is_renewable());
    }

    #[test]
    fn renewable_sources_declare_a_positive_regrowth_delay() {
        for id in NaturalResourceId::all() {
            match id.definition().regrow_ticks {
                Some(ticks) => assert!(ticks > 0, "{} regrows instantly", id.name()),
                None => assert!(!id.is_renewable()),
            }
        }
    }

    #[test]
    fn unknown_names_resolve_to_nothing_rather_than_a_substitute() {
        assert_eq!(NaturalResourceId::from_name("iron_vein"), None);
    }
}
