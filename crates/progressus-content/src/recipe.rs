//! Production recipes: exact physical inputs converted to exact outputs.

use crate::item::{self, ItemId};
use crate::registry::content_handle;
use crate::workstation::{self, WorkstationId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecipeInput {
    pub item: ItemId,
    pub quantity: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecipeDefinition {
    /// Stable identity used by saves and localization.
    pub name: &'static str,
    pub inputs: &'static [RecipeInput],
    pub output: ItemId,
    pub output_quantity: u32,
    pub workstation: WorkstationId,
    pub work_ticks: u32,
}

static PRIMITIVE_TOOL_INPUTS: &[RecipeInput] = &[
    RecipeInput {
        item: item::WOOD,
        quantity: 2,
    },
    RecipeInput {
        item: item::STONE,
        quantity: 1,
    },
];

static CART_INPUTS: &[RecipeInput] = &[
    RecipeInput {
        item: item::WOOD,
        quantity: 8,
    },
    RecipeInput {
        item: item::PRIMITIVE_TOOL,
        quantity: 1,
    },
];

/// Append-only: registry order is part of deterministic simulation outcomes.
pub static RECIPES: &[RecipeDefinition] = &[
    RecipeDefinition {
        name: "primitive_tool",
        inputs: PRIMITIVE_TOOL_INPUTS,
        output: item::PRIMITIVE_TOOL,
        output_quantity: 1,
        workstation: workstation::WORKBENCH,
        work_ticks: 6,
    },
    RecipeDefinition {
        name: "cart",
        inputs: CART_INPUTS,
        output: item::CART,
        output_quantity: 1,
        workstation: workstation::WORKBENCH,
        work_ticks: 16,
    },
];

content_handle!(RecipeId, RecipeDefinition, RECIPES, recipe);

impl RecipeId {
    /// Every recipe this workstation can execute, in registry order.
    pub fn for_workstation(workstation: WorkstationId) -> impl Iterator<Item = Self> {
        Self::all().filter(move |recipe| recipe.definition().workstation == workstation)
    }
}

pub const PRIMITIVE_TOOL: RecipeId = recipe("primitive_tool");
pub const CART: RecipeId = recipe("cart");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_constants_address_their_own_definitions() {
        assert_eq!(PRIMITIVE_TOOL.name(), "primitive_tool");
        assert_eq!(RecipeId::from_name("primitive_tool"), Some(PRIMITIVE_TOOL));
    }

    #[test]
    fn the_primitive_tool_recipe_keeps_its_physical_quantities() {
        let definition = PRIMITIVE_TOOL.definition();
        assert_eq!(definition.workstation, workstation::WORKBENCH);
        assert_eq!(definition.output, item::PRIMITIVE_TOOL);
        assert_eq!(definition.output_quantity, 1);
        assert_eq!(
            definition.inputs,
            [
                RecipeInput {
                    item: item::WOOD,
                    quantity: 2
                },
                RecipeInput {
                    item: item::STONE,
                    quantity: 1
                },
            ]
        );
    }

    #[test]
    fn every_recipe_consumes_and_produces_positive_quantities() {
        for id in RecipeId::all() {
            let definition = id.definition();
            assert!(
                !definition.inputs.is_empty(),
                "{} creates matter from nothing",
                id.name()
            );
            assert!(
                definition.output_quantity > 0,
                "{} produces nothing",
                id.name()
            );
            assert!(definition.work_ticks > 0, "{} is instantaneous", id.name());
            let mut items: Vec<_> = definition.inputs.iter().map(|input| input.item).collect();
            items.sort_unstable();
            let distinct = items.len();
            items.dedup();
            assert_eq!(items.len(), distinct, "{} lists an input twice", id.name());
            for input in definition.inputs {
                assert!(
                    input.quantity > 0,
                    "{} consumes zero of an input",
                    id.name()
                );
            }
        }
    }

    #[test]
    fn recipes_are_reachable_through_their_workstation() {
        assert_eq!(
            RecipeId::for_workstation(workstation::WORKBENCH).collect::<Vec<_>>(),
            vec![PRIMITIVE_TOOL, CART]
        );
        for id in RecipeId::all() {
            assert!(
                RecipeId::for_workstation(id.definition().workstation).any(|other| other == id)
            );
        }
    }

    #[test]
    fn unknown_names_resolve_to_nothing_rather_than_a_substitute() {
        assert_eq!(RecipeId::from_name("copper_ingot"), None);
    }
}
