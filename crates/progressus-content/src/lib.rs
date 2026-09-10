//! Progressus content definitions.
//!
//! This crate holds what exists in the game — items, terrain, natural
//! resources, structures, workstations and recipes — as data. It owns no
//! authoritative state, no coordinates and no simulation logic, so every layer
//! above it can share one vocabulary.
//!
//! Each definition carries a stable lowercase name. That name is the identity
//! saves store and localization keys off; the runtime handle is a registry
//! index and is never persisted. Named constants such as [`item::WOOD`] are
//! resolved from the name at compile time, so a misspelling fails the build.
//!
//! Behavior belongs on definitions rather than on identity: food is an item
//! with nutrition, a renewable resource is one with a regrowth delay, and a
//! passable structure is one with a navigation cost. Adding content should not
//! require teaching any system a new name.
//!
//! Registries are append-only. Their order determines handle order, which
//! determines map iteration order, which is part of deterministic simulation
//! outcomes. See [`ADR-0021`](../../../docs/adr/0021-content-registry.md).

mod registry;

pub mod item;
pub mod natural_resource;
pub mod recipe;
pub mod structure;
pub mod terrain;
pub mod workstation;

pub use item::{HAND_LOAD_UNITS, ItemCategory, ItemDefinition, ItemId, MAX_STACK_QUANTITY};
pub use natural_resource::{NaturalResourceDefinition, NaturalResourceId};
pub use recipe::{RecipeDefinition, RecipeId, RecipeInput};
pub use structure::{StructureDefinition, StructureId};
pub use terrain::{TerrainDefinition, TerrainId};
pub use workstation::{WorkstationDefinition, WorkstationId};

#[cfg(test)]
mod tests {
    use super::*;

    /// Names are the persistence and localization contract, so a collision
    /// inside one registry is a data corruption waiting for a save format.
    #[test]
    fn every_registry_has_unique_names() {
        fn unique(kind: &str, names: Vec<&'static str>) {
            let mut sorted = names.clone();
            sorted.sort_unstable();
            let before = sorted.len();
            sorted.dedup();
            assert_eq!(sorted.len(), before, "{kind} names collide: {names:?}");
        }
        unique("item", ItemId::all().map(ItemId::name).collect());
        unique("terrain", TerrainId::all().map(TerrainId::name).collect());
        unique(
            "natural resource",
            NaturalResourceId::all()
                .map(NaturalResourceId::name)
                .collect(),
        );
        unique(
            "structure",
            StructureId::all().map(StructureId::name).collect(),
        );
        unique(
            "workstation",
            WorkstationId::all().map(WorkstationId::name).collect(),
        );
        unique("recipe", RecipeId::all().map(RecipeId::name).collect());
    }

    /// Handles are registry indexes, so ordering is stable and total. Anything
    /// keyed by a handle iterates in this order.
    #[test]
    fn handles_are_ordered_by_registry_position() {
        let items: Vec<_> = ItemId::all().collect();
        assert_eq!(items.len(), ItemId::count());
        let mut sorted = items.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, items);
        assert_eq!(
            items.iter().map(|id| id.name()).collect::<Vec<_>>(),
            ["wood", "stone", "primitive_tool", "berries", "copper_ore"]
        );
    }

    /// A definition may only reference content that exists, or a recipe could
    /// name an item no build can produce.
    #[test]
    fn cross_registry_references_resolve() {
        for id in NaturalResourceId::all() {
            let yields = id.definition().yields;
            assert_eq!(ItemId::from_name(yields.name()), Some(yields));
        }
        for id in StructureId::all() {
            let material = id.definition().material;
            assert_eq!(ItemId::from_name(material.name()), Some(material));
        }
        for id in RecipeId::all() {
            let definition = id.definition();
            assert_eq!(
                ItemId::from_name(definition.output.name()),
                Some(definition.output)
            );
            assert_eq!(
                WorkstationId::from_name(definition.workstation.name()),
                Some(definition.workstation)
            );
            for input in definition.inputs {
                assert_eq!(ItemId::from_name(input.item.name()), Some(input.item));
            }
        }
    }
}
