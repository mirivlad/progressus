//! Physical item kinds.

use crate::capability::CapabilityId;
use crate::registry::content_handle;
use crate::slot::SlotId;

/// The largest quantity one physical stack may hold. This is storage
/// granularity on the ground, unrelated to what a person can lift.
pub const MAX_STACK_QUANTITY: u32 = 1024;

/// One pair of hands, in fixed-point load units.
///
/// A mixed load shares one pair of hands rather than filling independent
/// buckets, so carrying is checked as `sum(quantity / hand_load) <= 1`. That
/// fraction is evaluated in integers, because authoritative simulation may not
/// depend on floating point. The constant is the least common multiple of
/// `1..=16`, so every small hand load divides it exactly and the sum is exact;
/// a registry test holds that property. See ADR-0024.
pub const HAND_LOAD_UNITS: u64 = 720_720;

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
    /// How many of this one pair of hands holds. Authored in the item's own
    /// units, which is what a designer can reason about.
    pub hand_load: u32,
    /// Where this may be equipped, if anywhere.
    pub equip_slot: Option<SlotId>,
    /// What equipping this lets its bearer do.
    pub provides: &'static [CapabilityId],
}

static PRIMITIVE_TOOL_CAPABILITIES: &[CapabilityId] = &[crate::capability::MINE];

/// Append-only: registry order is part of deterministic simulation outcomes.
pub static ITEMS: &[ItemDefinition] = &[
    ItemDefinition {
        name: "wood",
        category: ItemCategory::Resources,
        nutrition: 0,
        hand_load: 10,
        equip_slot: None,
        provides: &[],
    },
    ItemDefinition {
        name: "stone",
        category: ItemCategory::Resources,
        nutrition: 0,
        hand_load: 5,
        equip_slot: None,
        provides: &[],
    },
    ItemDefinition {
        name: "primitive_tool",
        category: ItemCategory::Products,
        nutrition: 0,
        hand_load: 3,
        equip_slot: Some(crate::slot::TOOL),
        provides: PRIMITIVE_TOOL_CAPABILITIES,
    },
    ItemDefinition {
        name: "berries",
        category: ItemCategory::Food,
        nutrition: 50,
        hand_load: 20,
        equip_slot: None,
        provides: &[],
    },
    ItemDefinition {
        name: "copper_ore",
        category: ItemCategory::Resources,
        nutrition: 0,
        hand_load: 4,
        equip_slot: None,
        provides: &[],
    },
];

content_handle!(ItemId, ItemDefinition, ITEMS, item);

impl ItemId {
    /// Whether equipping this grants the capability.
    pub fn provides(self, capability: CapabilityId) -> bool {
        self.definition().provides.contains(&capability)
    }

    /// Food is a property, not an identity: anything nourishing can be eaten.
    pub const fn is_food(self) -> bool {
        self.definition().nutrition > 0
    }

    /// What carrying this quantity costs, as a share of one pair of hands.
    /// Rounds up, so a rounding error can only ever refuse a load, never
    /// admit one that does not fit.
    pub const fn load_cost(self, quantity: u32) -> u64 {
        (quantity as u64 * HAND_LOAD_UNITS).div_ceil(self.definition().hand_load as u64)
    }
}

pub const WOOD: ItemId = item("wood");
pub const STONE: ItemId = item("stone");
pub const PRIMITIVE_TOOL: ItemId = item("primitive_tool");
pub const BERRIES: ItemId = item("berries");
pub const COPPER_ORE: ItemId = item("copper_ore");

#[cfg(test)]
mod tests {
    use super::*;

    /// The fraction rule is evaluated in integers. If every hand load divides
    /// the unit constant, the sum is exact and no load is ever refused by
    /// rounding alone.
    #[test]
    fn every_hand_load_divides_the_unit_constant_exactly() {
        for id in ItemId::all() {
            let load = id.definition().hand_load;
            assert!(load > 0, "{} cannot be picked up at all", id.name());
            assert!(
                HAND_LOAD_UNITS.is_multiple_of(u64::from(load)),
                "{} has hand load {load}, which does not divide {HAND_LOAD_UNITS} \
                 and would make the carrying check round",
                id.name()
            );
            assert_eq!(id.load_cost(load), HAND_LOAD_UNITS, "{}", id.name());
            assert_eq!(id.load_cost(0), 0, "{}", id.name());
        }
    }

    #[test]
    fn load_cost_is_proportional_and_never_understates() {
        assert_eq!(WOOD.definition().hand_load, 10);
        assert_eq!(STONE.definition().hand_load, 5);
        // Half a pair of hands each, so five wood and two stone fit together
        // while six and three do not.
        assert_eq!(
            WOOD.load_cost(5) + STONE.load_cost(2),
            720_720 / 2 + 720_720 * 2 / 5
        );
        assert!(WOOD.load_cost(5) + STONE.load_cost(2) <= HAND_LOAD_UNITS);
        assert!(WOOD.load_cost(6) + STONE.load_cost(3) > HAND_LOAD_UNITS);
        // One more than a full load never fits.
        for id in ItemId::all() {
            assert!(id.load_cost(id.definition().hand_load + 1) > HAND_LOAD_UNITS);
        }
    }

    #[test]
    fn named_constants_address_their_own_definitions() {
        for (id, name) in [
            (WOOD, "wood"),
            (STONE, "stone"),
            (PRIMITIVE_TOOL, "primitive_tool"),
            (BERRIES, "berries"),
            (COPPER_ORE, "copper_ore"),
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
