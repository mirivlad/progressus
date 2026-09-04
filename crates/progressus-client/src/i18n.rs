use bevy::prelude::Resource;
use progressus_app::{
    Direction, ItemCategory, ItemKind, JobKind, JobState, MovementState, RecipeId, WorkstationKind,
};

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
    Door,
    Workbench,
    CancelJobs,
    Stockpile,
    Configure,
    AcceptedItems,
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
    InputPorts,
    RotateInputs,
    OutputPorts,
    RotateOutputs,
    Saves,
    SaveSlot,
    EmptySlot,
    InvalidSlot,
    SaveAction,
    LoadAction,
    SaveDirectory,
    SavedSlot,
    LoadedSlot,
    SaveError,
    LoadError,
    Character,
    Identifier,
    Cell,
    Movement,
    Work,
    Carrying,
    Satiety,
    NoneValue,
}

impl Locale {
    pub(crate) const fn tr(self, key: TextKey) -> &'static str {
        match (self.language, key) {
            (Language::Ru, TextKey::Select) => "Выбор",
            (Language::Ru, TextKey::StockpileAdd) => "Склад +",
            (Language::Ru, TextKey::StockpileRemove) => "Склад -",
            (Language::Ru, TextKey::Harvest) => "Добыча",
            (Language::Ru, TextKey::StoneWall) => "Каменная стена",
            (Language::Ru, TextKey::Door) => "Дверь",
            (Language::Ru, TextKey::Workbench) => "Верстак",
            (Language::Ru, TextKey::CancelJobs) => "Отмена задач",
            (Language::Ru, TextKey::Stockpile) => "Склад",
            (Language::Ru, TextKey::Configure) => "Настроить",
            (Language::Ru, TextKey::AcceptedItems) => "Разрешённые предметы",
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
            (Language::Ru, TextKey::InputPorts) => "Входы сырья",
            (Language::Ru, TextKey::RotateInputs) => "Повернуть входы",
            (Language::Ru, TextKey::OutputPorts) => "Выходы продукции",
            (Language::Ru, TextKey::RotateOutputs) => "Повернуть выходы",
            (Language::Ru, TextKey::Saves) => "Сохранения",
            (Language::Ru, TextKey::SaveSlot) => "Слот",
            (Language::Ru, TextKey::EmptySlot) => "Пусто",
            (Language::Ru, TextKey::InvalidSlot) => "Повреждено",
            (Language::Ru, TextKey::SaveAction) => "Сохранить",
            (Language::Ru, TextKey::LoadAction) => "Загрузить",
            (Language::Ru, TextKey::SaveDirectory) => "Каталог",
            (Language::Ru, TextKey::SavedSlot) => "Сохранён слот",
            (Language::Ru, TextKey::LoadedSlot) => "Загружен слот",
            (Language::Ru, TextKey::SaveError) => "Ошибка сохранения",
            (Language::Ru, TextKey::LoadError) => "Ошибка загрузки",
            (Language::Ru, TextKey::Character) => "Персонаж",
            (Language::Ru, TextKey::Identifier) => "ID",
            (Language::Ru, TextKey::Cell) => "Клетка",
            (Language::Ru, TextKey::Movement) => "Движение",
            (Language::Ru, TextKey::Work) => "Работа",
            (Language::Ru, TextKey::Carrying) => "Несёт",
            (Language::Ru, TextKey::Satiety) => "Сытость",
            (Language::Ru, TextKey::NoneValue) => "нет",
            (Language::En, TextKey::Select) => "Select",
            (Language::En, TextKey::StockpileAdd) => "Stockpile +",
            (Language::En, TextKey::StockpileRemove) => "Stockpile -",
            (Language::En, TextKey::Harvest) => "Harvest",
            (Language::En, TextKey::StoneWall) => "Stone wall",
            (Language::En, TextKey::Door) => "Door",
            (Language::En, TextKey::Workbench) => "Workbench",
            (Language::En, TextKey::CancelJobs) => "Cancel jobs",
            (Language::En, TextKey::Stockpile) => "Stockpile",
            (Language::En, TextKey::Configure) => "Configure",
            (Language::En, TextKey::AcceptedItems) => "Allowed items",
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
            (Language::En, TextKey::InputPorts) => "Material inputs",
            (Language::En, TextKey::RotateInputs) => "Rotate inputs",
            (Language::En, TextKey::OutputPorts) => "Product outputs",
            (Language::En, TextKey::RotateOutputs) => "Rotate outputs",
            (Language::En, TextKey::Saves) => "Saves",
            (Language::En, TextKey::SaveSlot) => "Slot",
            (Language::En, TextKey::EmptySlot) => "Empty",
            (Language::En, TextKey::InvalidSlot) => "Invalid",
            (Language::En, TextKey::SaveAction) => "Save",
            (Language::En, TextKey::LoadAction) => "Load",
            (Language::En, TextKey::SaveDirectory) => "Directory",
            (Language::En, TextKey::SavedSlot) => "Saved slot",
            (Language::En, TextKey::LoadedSlot) => "Loaded slot",
            (Language::En, TextKey::SaveError) => "Save error",
            (Language::En, TextKey::LoadError) => "Load error",
            (Language::En, TextKey::Character) => "Character",
            (Language::En, TextKey::Identifier) => "ID",
            (Language::En, TextKey::Cell) => "Cell",
            (Language::En, TextKey::Movement) => "Movement",
            (Language::En, TextKey::Work) => "Work",
            (Language::En, TextKey::Carrying) => "Carrying",
            (Language::En, TextKey::Satiety) => "Satiety",
            (Language::En, TextKey::NoneValue) => "none",
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

    pub(crate) const fn direction_name(self, direction: Direction) -> &'static str {
        match (self.language, direction) {
            (Language::Ru, Direction::North) => "север",
            (Language::Ru, Direction::East) => "восток",
            (Language::Ru, Direction::South) => "юг",
            (Language::Ru, Direction::West) => "запад",
            (Language::En, Direction::North) => "north",
            (Language::En, Direction::East) => "east",
            (Language::En, Direction::South) => "south",
            (Language::En, Direction::West) => "west",
        }
    }

    pub(crate) const fn movement_name(self, movement: MovementState) -> &'static str {
        match (self.language, movement) {
            (Language::Ru, MovementState::Idle) => "стоит",
            (Language::Ru, MovementState::ManualDirectional { .. }) => "идёт",
            (Language::Ru, MovementState::Navigating { .. }) => "идёт к цели",
            (Language::Ru, MovementState::Wandering { .. }) => "гуляет",
            (Language::En, MovementState::Idle) => "idle",
            (Language::En, MovementState::ManualDirectional { .. }) => "moving",
            (Language::En, MovementState::Navigating { .. }) => "navigating",
            (Language::En, MovementState::Wandering { .. }) => "wandering",
        }
    }

    pub(crate) const fn item_name(self, kind: ItemKind) -> &'static str {
        match (self.language, kind) {
            (Language::Ru, ItemKind::Wood) => "Дерево",
            (Language::Ru, ItemKind::Stone) => "Камень",
            (Language::Ru, ItemKind::PrimitiveTool) => "Примитивный инструмент",
            (Language::Ru, ItemKind::Berries) => "Ягоды",
            (Language::En, ItemKind::Wood) => "Wood",
            (Language::En, ItemKind::Stone) => "Stone",
            (Language::En, ItemKind::PrimitiveTool) => "Primitive tool",
            (Language::En, ItemKind::Berries) => "Berries",
        }
    }

    pub(crate) const fn item_category_name(self, category: ItemCategory) -> &'static str {
        match (self.language, category) {
            (Language::Ru, ItemCategory::Resources) => "Ресурсы",
            (Language::Ru, ItemCategory::Food) => "Еда",
            (Language::Ru, ItemCategory::Products) => "Изделия",
            (Language::En, ItemCategory::Resources) => "Resources",
            (Language::En, ItemCategory::Food) => "Food",
            (Language::En, ItemCategory::Products) => "Products",
        }
    }

    pub(crate) const fn job_kind_name(self, kind: JobKind) -> &'static str {
        match (self.language, kind) {
            (Language::Ru, JobKind::Harvest { .. }) => "добыча",
            (Language::Ru, JobKind::Eat { .. }) => "еда",
            (Language::Ru, JobKind::Haul { .. }) => "перенос на склад",
            (Language::Ru, JobKind::Craft { .. }) => "производство",
            (Language::Ru, JobKind::SupplyProduction { .. }) => "подача сырья",
            (Language::Ru, JobKind::DeliverConstruction { .. }) => "доставка на стройку",
            (Language::Ru, JobKind::Construct { .. }) => "строительство",
            (Language::En, JobKind::Harvest { .. }) => "harvest",
            (Language::En, JobKind::Eat { .. }) => "eat",
            (Language::En, JobKind::Haul { .. }) => "haul",
            (Language::En, JobKind::Craft { .. }) => "craft",
            (Language::En, JobKind::SupplyProduction { .. }) => "production supply",
            (Language::En, JobKind::DeliverConstruction { .. }) => "construction delivery",
            (Language::En, JobKind::Construct { .. }) => "construction",
        }
    }

    pub(crate) const fn job_state_name(self, state: JobState) -> &'static str {
        match (self.language, state) {
            (Language::Ru, JobState::Available) => "ожидает",
            (Language::Ru, JobState::Reserved { .. }) => "назначено",
            (Language::Ru, JobState::Transporting { .. }) => "несёт",
            (Language::Ru, JobState::Working { .. }) => "работает",
            (Language::En, JobState::Available) => "available",
            (Language::En, JobState::Reserved { .. }) => "reserved",
            (Language::En, JobState::Transporting { .. }) => "transporting",
            (Language::En, JobState::Working { .. }) => "working",
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
