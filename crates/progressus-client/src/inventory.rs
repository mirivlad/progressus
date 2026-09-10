#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Language, Locale};
    use progressus_app::{
        EntityId, ItemLocation, WorldCell, WorldPosition, item, natural_resource,
    };
    use progressus_content::slot;

    fn id(value: u64) -> EntityId {
        EntityId::new(value).unwrap()
    }

    #[test]
    fn active_tools_require_alt_click_to_pin_world_inspection() {
        assert!(click_pins_inspection(false, false));
        assert!(!click_pins_inspection(true, false));
        assert!(click_pins_inspection(true, true));
    }

    #[test]
    fn item_actions_follow_physical_location() {
        let character = id(1);
        let item_id = id(20);
        let container_id = id(30);
        let ground = WorldPosition::from_cell_center(WorldCell::new(2, 3)).unwrap();

        assert_eq!(
            actions_for_item(
                character,
                item_id,
                ItemLocation::Ground { position: ground },
                &[container_id],
                Some(slot::TOOL),
            ),
            vec![
                InventoryAction::PickUp {
                    character_id: character,
                    item_id
                },
                InventoryAction::Load {
                    character_id: character,
                    item_id,
                    container_id,
                },
            ]
        );
        assert_eq!(
            actions_for_item(
                character,
                item_id,
                ItemLocation::Carried {
                    character_id: character
                },
                &[container_id],
                Some(slot::TOOL),
            ),
            vec![
                InventoryAction::Equip {
                    character_id: character,
                    item_id
                },
                InventoryAction::Drop {
                    character_id: character,
                    item_id
                },
            ]
        );
        assert_eq!(
            actions_for_item(
                character,
                item_id,
                ItemLocation::Equipped {
                    character_id: character,
                    slot: slot::TOOL,
                },
                &[],
                Some(slot::TOOL),
            ),
            vec![InventoryAction::Unequip {
                character_id: character,
                item_id
            }]
        );
        assert_eq!(
            actions_for_item(
                character,
                item_id,
                ItemLocation::Contained { container_id },
                &[],
                None,
            ),
            vec![InventoryAction::Unload {
                character_id: character,
                item_id
            }]
        );
    }

    #[test]
    fn localized_source_text_explains_output_tool_and_job() {
        let ru = source_inspector_text(
            Locale {
                language: Language::Ru,
            },
            natural_resource::COPPER_VEIN,
            WorldCell::new(4, -2),
            7,
            Some(id(44)),
        );
        assert!(ru.contains("Медная жила"));
        assert!(ru.contains("Медная руда x7"));
        assert!(ru.contains("требуется инструмент"));
        assert!(ru.contains("#44"));

        let en = source_inspector_text(
            Locale {
                language: Language::En,
            },
            natural_resource::TREE,
            WorldCell::new(1, 2),
            8,
            None,
        );
        assert!(en.contains("Tree"));
        assert!(en.contains("Wood x8"));
        assert!(en.contains("bare hands"));
        assert!(en.contains("No assigned job"));
    }

    #[test]
    fn localized_inventory_text_shows_equipment_and_container_load() {
        let text = item_inspector_text(
            Locale {
                language: Language::En,
            },
            item::CART,
            id(9),
            1,
            ItemLocation::Equipped {
                character_id: id(1),
                slot: slot::TOOL,
            },
            false,
            360_360,
            Some(2_882_880),
            &["Stone x2".to_owned(), "Wood x3".to_owned()],
        );
        assert!(text.contains("Cart #9 x1"));
        assert!(text.contains("Equipped: tool"));
        assert!(text.contains("Load: 12.5%"));
        assert!(text.contains("Contents: Stone x2, Wood x3"));
    }
}
