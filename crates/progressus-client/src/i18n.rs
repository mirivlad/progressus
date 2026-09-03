use bevy::prelude::Resource;
use progressus_app::{RecipeId, WorkstationKind};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Resource)]
pub(crate) struct Locale {
    pub(crate) language: Language,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Language {
    #[default]
    Ru,
    En,
}

impl Language {
    pub(crate) const fn toggled(self) -> Self {
        match self {
            Self::Ru => Self::En,
            Self::En => Self::Ru,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TextKey {
    Select,
    StockpileAdd,
    StockpileRemove,
    Harvest,
    StoneWall,
    Workbench,
    CancelJobs,
    Mode,
    Orders,
    Recipes,
    Remaining,
    AddOrder,
    Delete,
    Close,
    PrimitiveTool,
    NoOrders,
    RemoveWorkstation,
    Logistics,
    InputZone,
    OutputZone,
    AddCells,
    RemoveCells,
    InputZoneAdd,
    InputZoneRemove,
    OutputZoneAdd,
    OutputZoneRemove,
}

impl Locale {
    pub(crate) const fn tr(self, key: TextKey) -> &'static str {
        match (self.language, key) {
            (Language::Ru, TextKey::Select) => "Выбор",
            (Language::Ru, TextKey::StockpileAdd) => "Склад +",
            (Language::Ru, TextKey::StockpileRemove) => "Склад -",
            (Language::Ru, TextKey::Harvest) => "Добыча",
            (Language::Ru, TextKey::StoneWall) => "Каменная стена",
            (Language::Ru, TextKey::Workbench) => "Верстак",
            (Language::Ru, TextKey::CancelJobs) => "Отмена задач",
            (Language::Ru, TextKey::Mode) => "Режим",
            (Language::Ru, TextKey::Orders) => "Задания",
            (Language::Ru, TextKey::Recipes) => "Рецепты",
            (Language::Ru, TextKey::Remaining) => "Осталось",
            (Language::Ru, TextKey::AddOrder) => "Добавить",
            (Language::Ru, TextKey::Delete) => "Удалить",
            (Language::Ru, TextKey::Close) => "Закрыть",
            (Language::Ru, TextKey::PrimitiveTool) => "Примитивный инструмент",
            (Language::Ru, TextKey::NoOrders) => "Заданий пока нет",
            (Language::Ru, TextKey::RemoveWorkstation) => "Убрать верстак",
            (Language::Ru, TextKey::Logistics) => "Логистика",
            (Language::Ru, TextKey::InputZone) => "Вход",
            (Language::Ru, TextKey::OutputZone) => "Выход",
            (Language::Ru, TextKey::AddCells) => "Добавить клетки",
            (Language::Ru, TextKey::RemoveCells) => "Убрать клетки",
            (Language::Ru, TextKey::InputZoneAdd) => "Вход +",
            (Language::Ru, TextKey::InputZoneRemove) => "Вход -",
            (Language::Ru, TextKey::OutputZoneAdd) => "Выход +",
            (Language::Ru, TextKey::OutputZoneRemove) => "Выход -",
            (Language::En, TextKey::Select) => "Select",
            (Language::En, TextKey::StockpileAdd) => "Stockpile +",
            (Language::En, TextKey::StockpileRemove) => "Stockpile -",
            (Language::En, TextKey::Harvest) => "Harvest",
            (Language::En, TextKey::StoneWall) => "Stone wall",
            (Language::En, TextKey::Workbench) => "Workbench",
            (Language::En, TextKey::CancelJobs) => "Cancel jobs",
            (Language::En, TextKey::Mode) => "Mode",
            (Language::En, TextKey::Orders) => "Orders",
            (Language::En, TextKey::Recipes) => "Recipes",
            (Language::En, TextKey::Remaining) => "Remaining",
            (Language::En, TextKey::AddOrder) => "Add",
            (Language::En, TextKey::Delete) => "Delete",
            (Language::En, TextKey::Close) => "Close",
            (Language::En, TextKey::PrimitiveTool) => "Primitive tool",
            (Language::En, TextKey::NoOrders) => "No orders yet",
            (Language::En, TextKey::RemoveWorkstation) => "Remove workbench",
            (Language::En, TextKey::Logistics) => "Logistics",
            (Language::En, TextKey::InputZone) => "Input",
            (Language::En, TextKey::OutputZone) => "Output",
            (Language::En, TextKey::AddCells) => "Add cells",
            (Language::En, TextKey::RemoveCells) => "Remove cells",
            (Language::En, TextKey::InputZoneAdd) => "Input +",
            (Language::En, TextKey::InputZoneRemove) => "Input -",
            (Language::En, TextKey::OutputZoneAdd) => "Output +",
            (Language::En, TextKey::OutputZoneRemove) => "Output -",
        }
    }

    pub(crate) const fn recipe_name(self, recipe_id: RecipeId) -> &'static str {
        match recipe_id {
            RecipeId::PrimitiveTool => self.tr(TextKey::PrimitiveTool),
        }
    }

    pub(crate) const fn workstation_name(self, kind: WorkstationKind) -> &'static str {
        match kind {
            WorkstationKind::Workbench => self.tr(TextKey::Workbench),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Language, Locale, TextKey};

    #[test]
    fn russian_is_the_default_and_english_is_complete_for_shared_ui_keys() {
        let mut locale = Locale::default();
        assert_eq!(locale.language, Language::Ru);
        assert_eq!(locale.tr(TextKey::Workbench), "Верстак");
        locale.language = locale.language.toggled();
        assert_eq!(locale.language, Language::En);
        assert_eq!(locale.tr(TextKey::Workbench), "Workbench");
        assert_eq!(locale.tr(TextKey::Remaining), "Remaining");
    }
}
