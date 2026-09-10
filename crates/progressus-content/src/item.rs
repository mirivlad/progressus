//! Physical item kinds.

use crate::registry::content_handle;

/// The largest quantity one physical stack may hold.
pub const MAX_STACK_QUANTITY: u32 = 1024;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ItemCategory {
    Resources,
    Food,
    Products,
}

impl ItemCategory {
    pub const ALL: [Self; 3] = [Self::Resources, Self::Food, Self::Products];

    /// Every item in this category, in registry order.
    pub fn items(self) -> impl Iterator<Item = ItemId> {
        ItemId::all().filter(move |item| item.definition().category == self)
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Resources => "resources",
            Self::Food => "food",
            Self::Products => "products",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|category| category.name() == name)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ItemDefinition {
    /// Stable identity used by saves and localization.
    pub name: &'static str,
    pub category: ItemCategory,
    /// Satiety one unit restores when eaten. Zero means this is not food.
    pub nutrition: u8,
}

/// Append-only: registry order is part of deterministic simulation outcomes.
pub static ITEMS: &[ItemDefinition] = &[
    ItemDefinition {
        name: "wood",
        category: ItemCategory::Resources,
        nutrition: 0,
    },
    ItemDefinition {
        name: "stone",
        category: ItemCategory::Resources,
        nutrition: 0,
    },
    ItemDefinition {
        name: "primitive_tool",
        category: ItemCategory::Products,
        nutrition: 0,
    },
    ItemDefinition {
        name: "berries",
        category: ItemCategory::Food,
        nutrition: 50,
    },
];

content_handle!(ItemId, ItemDefinition, ITEMS, item);

impl ItemId {
    /// Food is a property, not an identity: anything nourishing can be eaten.
    pub const fn is_food(self) -> bool {
        self.definition().nutrition > 0
    }
}

pub const WOOD: ItemId = item("wood");
pub const STONE: ItemId = item("stone");
pub const PRIMITIVE_TOOL: ItemId = item("primitive_tool");
pub const BERRIES: ItemId = item("berries");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_constants_address_their_own_definitions() {
        for (id, name) in [
            (WOOD, "wood"),
            (STONE, "stone"),
            (PRIMITIVE_TOOL, "primitive_tool"),
            (BERRIES, "berries"),
        ] {
            assert_eq!(id.name(), name);
            assert_eq!(ItemId::from_name(name), Some(id));
        }
    }

    #[test]
    fn names_are_unique_and_lowercase() {
        let mut names: Vec<_> = ItemId::all().map(ItemId::name).collect();
        assert_eq!(names.len(), ItemId::count());
        names.sort_unstable();
        let unique = names.len();
        names.dedup();
        assert_eq!(names.len(), unique, "item names must be unique");
        for name in ItemId::all().map(ItemId::name) {
            assert!(
                name.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "{name} is not a stable lowercase name"
            );
        }
    }

    #[test]
    fn unknown_names_resolve_to_nothing_rather_than_a_substitute() {
        assert_eq!(ItemId::from_name("iron_ingot"), None);
        assert_eq!(ItemId::from_name("Wood"), None);
        assert_eq!(ItemId::from_name(""), None);
    }

    #[test]
    fn food_is_a_property_and_only_food_carries_nutrition() {
        assert!(BERRIES.is_food());
        assert_eq!(BERRIES.definition().nutrition, 50);
        for id in ItemId::all().filter(|id| !id.is_food()) {
            assert_eq!(id.definition().nutrition, 0);
            assert_ne!(id.definition().category, ItemCategory::Food);
        }
        for id in ItemCategory::Food.items() {
            assert!(
                id.is_food(),
                "{} is filed as food but feeds nobody",
                id.name()
            );
        }
    }

    #[test]
    fn categories_partition_the_registry() {
        let mut collected: Vec<_> = ItemCategory::ALL
            .into_iter()
            .flat_map(ItemCategory::items)
            .collect();
        collected.sort_unstable();
        assert_eq!(collected, ItemId::all().collect::<Vec<_>>());
    }
}
