use crate::{ItemKind, WorkstationKind};

pub const CRAFT_WORK_TICKS: u32 = 6;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RecipeId {
    PrimitiveTool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecipeInput {
    pub kind: ItemKind,
    pub quantity: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecipeDefinition {
    pub id: RecipeId,
    pub inputs: &'static [RecipeInput],
    pub output_kind: ItemKind,
    pub output_quantity: u32,
    pub workstation: WorkstationKind,
    pub work_ticks: u32,
}

const PRIMITIVE_TOOL_INPUTS: [RecipeInput; 2] = [
    RecipeInput {
        kind: ItemKind::Wood,
        quantity: 2,
    },
    RecipeInput {
        kind: ItemKind::Stone,
        quantity: 1,
    },
];

pub const fn recipe_definition(id: RecipeId) -> RecipeDefinition {
    match id {
        RecipeId::PrimitiveTool => RecipeDefinition {
            id,
            inputs: &PRIMITIVE_TOOL_INPUTS,
            output_kind: ItemKind::PrimitiveTool,
            output_quantity: 1,
            workstation: WorkstationKind::Workbench,
            work_ticks: CRAFT_WORK_TICKS,
        },
    }
}

#[cfg(test)]
mod tests {
    use crate::{ItemKind, WorkstationKind};

    use super::{RecipeId, recipe_definition};

    #[test]
    fn primitive_tool_recipe_is_a_small_typed_physical_recipe() {
        let recipe = recipe_definition(RecipeId::PrimitiveTool);
        assert_eq!(recipe.workstation, WorkstationKind::Workbench);
        assert_eq!(recipe.output_kind, ItemKind::PrimitiveTool);
        assert_eq!(recipe.output_quantity, 1);
        assert_eq!(recipe.inputs.len(), 2);
        assert_eq!(recipe.inputs[0].kind, ItemKind::Wood);
        assert_eq!(recipe.inputs[0].quantity, 2);
        assert_eq!(recipe.inputs[1].kind, ItemKind::Stone);
        assert_eq!(recipe.inputs[1].quantity, 1);
    }
}
