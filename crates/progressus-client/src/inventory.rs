use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use progressus_app::{
    ClientSnapshot, Command, EntityId, InventoryItemSnapshot, ItemId, ItemLocation, JobKind,
    NaturalResourceId, WorldCell,
};
use progressus_content::SlotId;

use crate::i18n::{Language, Locale};
use crate::modal::ModalState;
use crate::navigation::SelectedCharacter;
use crate::runtime::AuthoritativeClient;
use crate::ui::{ToolMode, ToolState, UiCapture};
use crate::ui_font::UiFont;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InspectedObject {
    Item(EntityId),
    Resource(WorldCell),
}

#[derive(Resource, Debug, Default)]
pub(crate) struct InspectionState {
    pub(crate) hovered: Option<InspectedObject>,
    pub(crate) pinned: Option<InspectedObject>,
    selected_item: Option<EntityId>,
    feedback: Option<ActionFeedback>,
    revision: u64,
}

impl InspectionState {
    pub(crate) fn pin(&mut self, object: InspectedObject) {
        if self.pinned != Some(object) {
            self.pinned = Some(object);
            self.selected_item = match object {
                InspectedObject::Item(id) => Some(id),
                InspectedObject::Resource(_) => None,
            };
            self.feedback = None;
            self.bump();
        }
    }

    pub(crate) fn clear(&mut self) {
        if self.pinned.take().is_some()
            || self.selected_item.take().is_some()
            || self.feedback.take().is_some()
        {
            self.bump();
        }
    }

    fn bump(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }
}

#[derive(Clone, Debug)]
struct ActionFeedback {
    text: String,
    rejected: bool,
}

#[derive(Resource, Default)]
pub(crate) struct InventoryPresentation {
    root: Option<Entity>,
    signature: Option<InventorySignature>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct InventorySignature {
    character: Option<EntityId>,
    pinned: Option<InspectedObject>,
    selected_item: Option<EntityId>,
    item_revision: u64,
    resource_revision: u64,
    job_revision: u64,
    locale: Language,
    state_revision: u64,
}

#[derive(Component)]
pub(crate) struct WorldHoverTooltip;

#[derive(Component)]
pub(crate) struct WorldHoverTooltipText;

#[derive(Component)]
pub(crate) struct InventoryItemButton(EntityId);

#[derive(Component)]
pub(crate) struct InventoryActionButton(InventoryAction);

const PANEL: Color = Color::srgba(0.045, 0.055, 0.06, 0.96);
const BORDER: Color = Color::srgb(0.28, 0.34, 0.36);
const TEXT: Color = Color::srgb(0.91, 0.93, 0.94);
const MUTED: Color = Color::srgb(0.67, 0.73, 0.75);
const BUTTON: Color = Color::srgb(0.12, 0.14, 0.15);
const SELECTED_BUTTON: Color = Color::srgb(0.10, 0.46, 0.58);

pub(crate) fn setup_world_hover_tooltip(mut commands: Commands, font: Res<UiFont>) {
    commands
        .spawn((
            WorldHoverTooltip,
            Visibility::Hidden,
            Node {
                position_type: PositionType::Absolute,
                width: px(300),
                padding: UiRect::all(px(8)),
                border: UiRect::all(px(1)),
                ..default()
            },
            BackgroundColor(PANEL),
            BorderColor::all(BORDER),
            GlobalZIndex(18),
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new(""),
                TextFont {
                    font: font.0.clone(),
                    font_size: 12.5,
                    ..default()
                },
                TextColor(TEXT),
                WorldHoverTooltipText,
            ));
        });
}

pub(crate) fn update_world_hover(
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    view: Res<crate::low_poly::View>,
    cache: Res<crate::low_poly::scene::SceneCache>,
    tool: Res<ToolState>,
    modal: Res<ModalState>,
    mut inspection: ResMut<InspectionState>,
) {
    inspection.hovered = None;
    if modal.is_open() || tool.pointer_over_ui {
        return;
    }
    let (Ok(window), Ok((camera, transform))) = (windows.single(), cameras.single()) else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Ok(ray) = camera.viewport_to_world(transform, cursor) else {
        return;
    };
    let Some(point) = crate::low_poly::space::ground(ray) else {
        return;
    };
    let Some(position) = crate::low_poly::space::position(point, view.origin) else {
        return;
    };
    inspection.hovered = inspectable_at(&cache, position.containing_cell());
}

pub(crate) fn inspectable_at(
    cache: &crate::low_poly::scene::SceneCache,
    cell: WorldCell,
) -> Option<InspectedObject> {
    cache
        .items
        .iter()
        .filter(|item| item.position.containing_cell() == cell)
        .min_by_key(|item| item.id)
        .map(|item| InspectedObject::Item(item.id))
        .or_else(|| {
            cache
                .resources
                .iter()
                .any(|resource| resource.cell == cell)
                .then_some(InspectedObject::Resource(cell))
        })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn sync_world_hover_tooltip(
    locale: Res<Locale>,
    tool: Res<ToolState>,
    inspection: Res<InspectionState>,
    cache: Res<crate::low_poly::scene::SceneCache>,
    authoritative: Res<AuthoritativeClient>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut panels: Query<(&mut Visibility, &mut Node), With<WorldHoverTooltip>>,
    mut texts: Query<&mut Text, With<WorldHoverTooltipText>>,
) {
    let (Ok((mut visibility, mut node)), Ok(mut text)) = (panels.single_mut(), texts.single_mut())
    else {
        return;
    };
    let Some(object) = inspection.hovered else {
        *visibility = Visibility::Hidden;
        return;
    };
    let hint = match (locale.language, tool.mode == ToolMode::Select) {
        (Language::Ru, true) => "Щёлкните, чтобы закрепить",
        (Language::Ru, false) => "Alt+щелчок: закрепить, не применяя инструмент",
        (Language::En, true) => "Click to pin",
        (Language::En, false) => "Alt+click: pin without applying the tool",
    };
    let body = match object {
        InspectedObject::Item(id) => inventory_rows(authoritative.snapshot(), &cache)
            .remove(&id)
            .map(|item| {
                let reservation = match (locale.language, item.reserved) {
                    (Language::Ru, true) => " · зарезервировано",
                    (Language::En, true) => " · reserved",
                    _ => "",
                };
                format!(
                    "{} x{}{}\n{}",
                    locale.item_name(item.kind),
                    item.quantity,
                    reservation,
                    hint
                )
            }),
        InspectedObject::Resource(cell) => cache
            .resources
            .iter()
            .find(|resource| resource.cell == cell)
            .map(|resource| {
                format!(
                    "{}\n{}: {} x{}\n{}",
                    locale.natural_resource_name(resource.kind),
                    match locale.language {
                        Language::Ru => "Добыча",
                        Language::En => "Output",
                    },
                    locale.item_name(resource.kind.definition().yields),
                    resource.yield_quantity,
                    hint
                )
            }),
    };
    let Some(body) = body else {
        *visibility = Visibility::Hidden;
        return;
    };
    **text = body;
    if let Ok(window) = windows.single()
        && let Some(cursor) = window.cursor_position()
    {
        node.left = px((cursor.x + 16.).min((window.width() - 316.).max(8.)));
        node.top = px((cursor.y + 18.).min((window.height() - 100.).max(8.)));
    }
    *visibility = Visibility::Visible;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InventoryAction {
    PickUp {
        character_id: EntityId,
        item_id: EntityId,
    },
    Drop {
        character_id: EntityId,
        item_id: EntityId,
    },
    Equip {
        character_id: EntityId,
        item_id: EntityId,
    },
    Unequip {
        character_id: EntityId,
        item_id: EntityId,
    },
    Load {
        character_id: EntityId,
        item_id: EntityId,
        container_id: EntityId,
    },
    Unload {
        character_id: EntityId,
        item_id: EntityId,
    },
}

pub(crate) const fn click_pins_inspection(tool_active: bool, alt_pressed: bool) -> bool {
    !tool_active || alt_pressed
}

pub(crate) fn actions_for_item(
    character_id: EntityId,
    item_id: EntityId,
    location: ItemLocation,
    container_ids: &[EntityId],
    equip_slot: Option<SlotId>,
) -> Vec<InventoryAction> {
    match location {
        ItemLocation::Ground { .. } => std::iter::once(InventoryAction::PickUp {
            character_id,
            item_id,
        })
        .chain(
            container_ids
                .iter()
                .copied()
                .filter(move |id| *id != item_id)
                .map(move |container_id| InventoryAction::Load {
                    character_id,
                    item_id,
                    container_id,
                }),
        )
        .collect(),
        ItemLocation::Carried {
            character_id: holder,
        } if holder == character_id => {
            let mut actions = Vec::with_capacity(2);
            if equip_slot.is_some() {
                actions.push(InventoryAction::Equip {
                    character_id,
                    item_id,
                });
            }
            actions.push(InventoryAction::Drop {
                character_id,
                item_id,
            });
            actions
        }
        ItemLocation::Equipped {
            character_id: bearer,
            ..
        } if bearer == character_id => vec![InventoryAction::Unequip {
            character_id,
            item_id,
        }],
        ItemLocation::Contained { .. } => vec![InventoryAction::Unload {
            character_id,
            item_id,
        }],
        ItemLocation::Carried { .. } | ItemLocation::Equipped { .. } => Vec::new(),
    }
}

/// Container capacity is declared in whole hand loads; every load beside it
/// is measured in the fractional units those divide into.
fn hand_loads_to_units(capacity: u32) -> u64 {
    u64::from(capacity) * progressus_content::HAND_LOAD_UNITS
}

fn load_share(load: u64, capacity: u64) -> String {
    if capacity == 0 {
        return "0%".to_owned();
    }
    let tenths = load.saturating_mul(1_000) / capacity;
    if tenths.is_multiple_of(10) {
        format!("{}%", tenths / 10)
    } else {
        format!("{}.{:01}%", tenths / 10, tenths % 10)
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn item_inspector_text(
    locale: Locale,
    kind: ItemId,
    id: EntityId,
    quantity: u32,
    location: ItemLocation,
    reserved: bool,
    load: u64,
    capacity: Option<u64>,
    contents: &[String],
) -> String {
    let location = match location {
        ItemLocation::Ground { position } => {
            let cell = position.containing_cell();
            match locale.language {
                Language::Ru => format!("На земле: ({}, {})", cell.x(), cell.y()),
                Language::En => format!("On ground: ({}, {})", cell.x(), cell.y()),
            }
        }
        ItemLocation::Carried { character_id } => match locale.language {
            Language::Ru => format!("В руках: персонаж #{}", character_id.value()),
            Language::En => format!("Carried: character #{}", character_id.value()),
        },
        ItemLocation::Equipped { character_id, slot } => match locale.language {
            Language::Ru => format!(
                "Экипировано: {} (персонаж #{})",
                locale.slot_name(slot.name()),
                character_id.value()
            ),
            Language::En => format!(
                "Equipped: {} (character #{})",
                locale.slot_name(slot.name()),
                character_id.value()
            ),
        },
        ItemLocation::Contained { container_id } => match locale.language {
            Language::Ru => format!("В контейнере #{}", container_id.value()),
            Language::En => format!("In container #{}", container_id.value()),
        },
    };
    let reservation = match (locale.language, reserved) {
        (Language::Ru, true) => "Резерв: да",
        (Language::Ru, false) => "Резерв: нет",
        (Language::En, true) => "Reserved: yes",
        (Language::En, false) => "Reserved: no",
    };
    let mut lines = vec![
        format!("{} #{} x{}", locale.item_name(kind), id.value(), quantity),
        location,
        reservation.to_owned(),
    ];
    if let Some(capacity) = capacity {
        lines.push(match locale.language {
            Language::Ru => format!("Загрузка: {}", load_share(load, capacity)),
            Language::En => format!("Load: {}", load_share(load, capacity)),
        });
        lines.push(match (locale.language, contents.is_empty()) {
            (Language::Ru, true) => "Содержимое: нет".to_owned(),
            (Language::En, true) => "Contents: none".to_owned(),
            (Language::Ru, false) => format!("Содержимое: {}", contents.join(", ")),
            (Language::En, false) => format!("Contents: {}", contents.join(", ")),
        });
    }
    lines.join("\n")
}

pub(crate) fn source_inspector_text(
    locale: Locale,
    kind: NaturalResourceId,
    cell: WorldCell,
    yield_quantity: u32,
    job_id: Option<EntityId>,
) -> String {
    let output = kind.definition().yields;
    let tool = match (locale.language, kind.needs_no_tool()) {
        (Language::Ru, true) => "Инструмент: можно голыми руками",
        (Language::Ru, false) => "Инструмент: требуется инструмент",
        (Language::En, true) => "Tool: bare hands",
        (Language::En, false) => "Tool: tool required",
    };
    let job = match (locale.language, job_id) {
        (Language::Ru, Some(id)) => format!("Назначенная работа: #{}", id.value()),
        (Language::Ru, None) => "Назначенной работы нет".to_owned(),
        (Language::En, Some(id)) => format!("Assigned job: #{}", id.value()),
        (Language::En, None) => "No assigned job".to_owned(),
    };
    format!(
        "{}\n{}: ({}, {})\n{}: {} x{}\n{}\n{}",
        locale.natural_resource_name(kind),
        match locale.language {
            Language::Ru => "Клетка",
            Language::En => "Cell",
        },
        cell.x(),
        cell.y(),
        match locale.language {
            Language::Ru => "Добыча",
            Language::En => "Output",
        },
        locale.item_name(output),
        yield_quantity,
        tool,
        job
    )
}

pub(crate) fn inventory_item_interaction(
    item_buttons: Query<(&Interaction, &InventoryItemButton), Changed<Interaction>>,
    action_buttons: Query<(&Interaction, &InventoryActionButton), Changed<Interaction>>,
    locale: Res<Locale>,
    mut inspection: ResMut<InspectionState>,
    mut authoritative: ResMut<AuthoritativeClient>,
    selected: Res<SelectedCharacter>,
) {
    if let Some((_, button)) = item_buttons
        .iter()
        .find(|(interaction, _)| **interaction == Interaction::Pressed)
    {
        inspection.selected_item = Some(button.0);
        inspection.feedback = None;
        inspection.bump();
        return;
    }
    let Some((_, button)) = action_buttons
        .iter()
        .find(|(interaction, _)| **interaction == Interaction::Pressed)
    else {
        return;
    };
    let result = authoritative.application_mut().execute(button.0.command());
    inspection.feedback = Some(match result {
        Ok(()) => {
            if let Err(error) = authoritative.refresh_lightweight_snapshot(selected.0) {
                ActionFeedback {
                    text: format!("{}: {error}", action_failed_label(*locale)),
                    rejected: true,
                }
            } else {
                ActionFeedback {
                    text: action_completed_label(*locale).to_owned(),
                    rejected: false,
                }
            }
        }
        Err(error) => ActionFeedback {
            text: format!("{}: {error}", action_rejected_label(*locale)),
            rejected: true,
        },
    });
    inspection.bump();
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn sync_inventory_panel(
    mut commands: Commands,
    locale: Res<Locale>,
    font: Res<UiFont>,
    selected: Res<SelectedCharacter>,
    authoritative: Res<AuthoritativeClient>,
    cache: Res<crate::low_poly::scene::SceneCache>,
    mut inspection: ResMut<InspectionState>,
    mut presentation: ResMut<InventoryPresentation>,
) {
    let snapshot = authoritative.snapshot();
    let signature = InventorySignature {
        character: selected.0,
        pinned: inspection.pinned,
        selected_item: inspection.selected_item,
        item_revision: snapshot.item_revision,
        resource_revision: snapshot.resource_revision,
        job_revision: snapshot.job_revision,
        locale: locale.language,
        state_revision: inspection.revision,
    };
    if presentation.signature.as_ref() == Some(&signature) {
        return;
    }
    if let Some(root) = presentation.root.take() {
        commands.entity(root).despawn();
    }

    let rows = inventory_rows(snapshot, &cache);
    let root = match inspection.pinned {
        Some(InspectedObject::Item(id)) => rows.get(&id).map(|item| {
            spawn_item_panel(
                &mut commands,
                *locale,
                &font,
                selected.0,
                item,
                &rows,
                inspection.feedback.as_ref(),
            )
        }),
        Some(InspectedObject::Resource(cell)) => cache
            .resources
            .iter()
            .find(|resource| resource.cell == cell)
            .map(|resource| {
                spawn_text_panel(
                    &mut commands,
                    source_inspector_text(
                        *locale,
                        resource.kind,
                        cell,
                        resource.yield_quantity,
                        harvest_job_at(snapshot, cell),
                    ),
                    &font,
                )
            }),
        None => selected.0.and_then(|character_id| {
            snapshot
                .characters
                .iter()
                .any(|character| character.id == character_id)
                .then(|| {
                    spawn_character_inventory_panel(
                        &mut commands,
                        *locale,
                        &font,
                        character_id,
                        &rows,
                        inspection.selected_item,
                        inspection.feedback.as_ref(),
                    )
                })
        }),
    };
    if inspection.pinned.is_some() && root.is_none() {
        inspection.clear();
    }
    presentation.root = root;
    presentation.signature = Some(signature);
}

fn inventory_rows(
    snapshot: &ClientSnapshot,
    cache: &crate::low_poly::scene::SceneCache,
) -> BTreeMap<EntityId, InventoryItemSnapshot> {
    snapshot
        .inventory_items
        .iter()
        .chain(cache.inventory_items.iter())
        .cloned()
        .map(|item| (item.id, item))
        .collect()
}

fn harvest_job_at(snapshot: &ClientSnapshot, cell: WorldCell) -> Option<EntityId> {
    snapshot.jobs.iter().find_map(|job| match job.kind {
        JobKind::Harvest { source } if source == cell => Some(job.id),
        _ => None,
    })
}

fn belongs_to_character(
    item: &InventoryItemSnapshot,
    character_id: EntityId,
    rows: &BTreeMap<EntityId, InventoryItemSnapshot>,
    visiting: &mut BTreeSet<EntityId>,
) -> bool {
    if !visiting.insert(item.id) {
        return false;
    }
    match item.location {
        ItemLocation::Carried {
            character_id: holder,
        }
        | ItemLocation::Equipped {
            character_id: holder,
            ..
        } => holder == character_id,
        ItemLocation::Contained { container_id } => rows
            .get(&container_id)
            .is_some_and(|parent| belongs_to_character(parent, character_id, rows, visiting)),
        ItemLocation::Ground { .. } => false,
    }
}

fn character_items(
    character_id: EntityId,
    rows: &BTreeMap<EntityId, InventoryItemSnapshot>,
) -> Vec<&InventoryItemSnapshot> {
    rows.values()
        .filter(|item| belongs_to_character(item, character_id, rows, &mut BTreeSet::new()))
        .collect()
}

fn container_contents(
    container_id: EntityId,
    rows: &BTreeMap<EntityId, InventoryItemSnapshot>,
    locale: Locale,
) -> Vec<String> {
    rows.values()
        .filter(|item| item.location == ItemLocation::Contained { container_id })
        .map(|item| format!("{} x{}", locale.item_name(item.kind), item.quantity))
        .collect()
}

fn spawn_panel<'a>(commands: &'a mut Commands<'_, '_>) -> EntityCommands<'a> {
    commands.spawn((
        UiCapture,
        Interaction::default(),
        Node {
            position_type: PositionType::Absolute,
            right: px(12),
            top: px(205),
            bottom: px(74),
            width: px(340),
            padding: UiRect::all(px(12)),
            row_gap: px(8),
            flex_direction: FlexDirection::Column,
            overflow: Overflow::scroll_y(),
            border: UiRect::all(px(1)),
            ..default()
        },
        BackgroundColor(PANEL),
        BorderColor::all(BORDER),
        GlobalZIndex(14),
    ))
}

fn text_bundle(text: impl Into<String>, font: &UiFont, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(text),
        TextFont {
            font: font.0.clone(),
            font_size: size,
            ..default()
        },
        TextColor(color),
    )
}

fn spawn_text_panel(commands: &mut Commands, text: String, font: &UiFont) -> Entity {
    spawn_panel(commands)
        .with_children(|panel| {
            panel.spawn(text_bundle(text, font, 14., TEXT));
        })
        .id()
}

fn spawn_item_panel(
    commands: &mut Commands,
    locale: Locale,
    font: &UiFont,
    character_id: Option<EntityId>,
    item: &InventoryItemSnapshot,
    rows: &BTreeMap<EntityId, InventoryItemSnapshot>,
    feedback: Option<&ActionFeedback>,
) -> Entity {
    let contents = container_contents(item.id, rows, locale);
    let description = item_inspector_text(
        locale,
        item.kind,
        item.id,
        item.quantity,
        item.location,
        item.reserved,
        item.contained_load.unwrap_or(item.load),
        item.capacity.map(hand_loads_to_units),
        &contents,
    );
    spawn_panel(commands)
        .with_children(|panel| {
            panel.spawn(text_bundle(description, font, 14., TEXT));
            if item.reserved {
                panel.spawn(text_bundle(reserved_hint(locale), font, 12.5, MUTED));
            } else if let Some(character_id) = character_id {
                let container_ids = rows
                    .values()
                    .filter(|candidate| candidate.capacity.is_some())
                    .map(|candidate| candidate.id)
                    .collect::<Vec<_>>();
                let actions = actions_for_item(
                    character_id,
                    item.id,
                    item.location,
                    &container_ids,
                    item.kind.definition().equip_slot,
                );
                spawn_actions(panel, locale, font, &actions, rows);
            } else {
                panel.spawn(text_bundle(actor_hint(locale), font, 12.5, MUTED));
            }
            spawn_feedback(panel, font, feedback);
        })
        .id()
}

fn spawn_character_inventory_panel(
    commands: &mut Commands,
    locale: Locale,
    font: &UiFont,
    character_id: EntityId,
    rows: &BTreeMap<EntityId, InventoryItemSnapshot>,
    selected_item: Option<EntityId>,
    feedback: Option<&ActionFeedback>,
) -> Entity {
    let items = character_items(character_id, rows);
    let hands_load: u64 = items
        .iter()
        .filter(|item| item.location == ItemLocation::Carried { character_id })
        .map(|item| item.load)
        .sum();
    spawn_panel(commands)
        .with_children(|panel| {
            panel.spawn(text_bundle(
                match locale.language {
                    Language::Ru => format!(
                        "Инвентарь\nРуки: {}",
                        load_share(hands_load, progressus_content::HAND_LOAD_UNITS)
                    ),
                    Language::En => format!(
                        "Inventory\nHands: {}",
                        load_share(hands_load, progressus_content::HAND_LOAD_UNITS)
                    ),
                },
                font,
                15.,
                TEXT,
            ));
            if items.is_empty() {
                panel.spawn(text_bundle(no_items_label(locale), font, 13., MUTED));
            }
            for item in &items {
                panel
                    .spawn((
                        Button,
                        InventoryItemButton(item.id),
                        UiCapture,
                        Node {
                            width: percent(100),
                            min_height: px(31),
                            padding: UiRect::axes(px(8), px(5)),
                            border: UiRect::all(px(1)),
                            ..default()
                        },
                        BackgroundColor(if selected_item == Some(item.id) {
                            SELECTED_BUTTON
                        } else {
                            BUTTON
                        }),
                        BorderColor::all(BORDER),
                    ))
                    .with_children(|button| {
                        button.spawn(text_bundle(
                            format!(
                                "{} x{} · {}",
                                locale.item_name(item.kind),
                                item.quantity,
                                location_marker(locale, item.location)
                            ),
                            font,
                            12.5,
                            TEXT,
                        ));
                    });
            }
            if let Some(item) = selected_item.and_then(|id| rows.get(&id))
                && items.iter().any(|candidate| candidate.id == item.id)
            {
                let contents = container_contents(item.id, rows, locale);
                panel.spawn(text_bundle(
                    item_inspector_text(
                        locale,
                        item.kind,
                        item.id,
                        item.quantity,
                        item.location,
                        item.reserved,
                        item.contained_load.unwrap_or(item.load),
                        item.capacity.map(hand_loads_to_units),
                        &contents,
                    ),
                    font,
                    12.5,
                    MUTED,
                ));
                if item.reserved {
                    panel.spawn(text_bundle(reserved_hint(locale), font, 12.5, MUTED));
                } else {
                    spawn_actions(
                        panel,
                        locale,
                        font,
                        &actions_for_item(
                            character_id,
                            item.id,
                            item.location,
                            &[],
                            item.kind.definition().equip_slot,
                        ),
                        rows,
                    );
                }
            }
            spawn_feedback(panel, font, feedback);
        })
        .id()
}

fn location_marker(locale: Locale, location: ItemLocation) -> String {
    match location {
        ItemLocation::Ground { .. } => match locale.language {
            Language::Ru => "земля".to_owned(),
            Language::En => "ground".to_owned(),
        },
        ItemLocation::Carried { .. } => match locale.language {
            Language::Ru => "руки".to_owned(),
            Language::En => "hands".to_owned(),
        },
        ItemLocation::Equipped { slot, .. } => match locale.language {
            Language::Ru => format!("экипировано: {}", locale.slot_name(slot.name())),
            Language::En => format!("equipped: {}", locale.slot_name(slot.name())),
        },
        ItemLocation::Contained { container_id } => match locale.language {
            Language::Ru => format!("в #{}", container_id.value()),
            Language::En => format!("in #{}", container_id.value()),
        },
    }
}

fn spawn_actions(
    panel: &mut ChildSpawnerCommands,
    locale: Locale,
    font: &UiFont,
    actions: &[InventoryAction],
    rows: &BTreeMap<EntityId, InventoryItemSnapshot>,
) {
    for action in actions {
        panel
            .spawn((
                Button,
                InventoryActionButton(*action),
                UiCapture,
                Node {
                    min_height: px(31),
                    padding: UiRect::horizontal(px(9)),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(px(1)),
                    ..default()
                },
                BackgroundColor(BUTTON),
                BorderColor::all(BORDER),
            ))
            .with_children(|button| {
                button.spawn(text_bundle(
                    action_label(locale, *action, rows),
                    font,
                    12.5,
                    TEXT,
                ));
            });
    }
}

fn spawn_feedback(
    panel: &mut ChildSpawnerCommands,
    font: &UiFont,
    feedback: Option<&ActionFeedback>,
) {
    if let Some(feedback) = feedback {
        panel.spawn(text_bundle(
            feedback.text.clone(),
            font,
            12.5,
            if feedback.rejected {
                Color::srgb(1., 0.45, 0.42)
            } else {
                Color::srgb(0.55, 0.92, 0.62)
            },
        ));
    }
}

fn actor_hint(locale: Locale) -> &'static str {
    match locale.language {
        Language::Ru => "Сначала выберите персонажа, затем закрепите предмет.",
        Language::En => "Select a character, then pin the item.",
    }
}

fn reserved_hint(locale: Locale) -> &'static str {
    match locale.language {
        Language::Ru => "Предмет зарезервирован работой; ручные действия недоступны.",
        Language::En => "A job reserved this item; manual actions are unavailable.",
    }
}

fn no_items_label(locale: Locale) -> &'static str {
    match locale.language {
        Language::Ru => "Предметов нет",
        Language::En => "No items",
    }
}

fn action_completed_label(locale: Locale) -> &'static str {
    match locale.language {
        Language::Ru => "Действие выполнено",
        Language::En => "Action completed",
    }
}

fn action_rejected_label(locale: Locale) -> &'static str {
    match locale.language {
        Language::Ru => "Действие отклонено",
        Language::En => "Action rejected",
    }
}

fn action_failed_label(locale: Locale) -> &'static str {
    match locale.language {
        Language::Ru => "Не удалось обновить состояние",
        Language::En => "Could not refresh state",
    }
}

fn action_label(
    locale: Locale,
    action: InventoryAction,
    rows: &BTreeMap<EntityId, InventoryItemSnapshot>,
) -> String {
    match action {
        InventoryAction::PickUp { .. } => match locale.language {
            Language::Ru => "Поднять".to_owned(),
            Language::En => "Pick up".to_owned(),
        },
        InventoryAction::Drop { .. } => match locale.language {
            Language::Ru => "Положить на землю".to_owned(),
            Language::En => "Drop / park".to_owned(),
        },
        InventoryAction::Equip { .. } => match locale.language {
            Language::Ru => "Экипировать".to_owned(),
            Language::En => "Equip".to_owned(),
        },
        InventoryAction::Unequip { .. } => match locale.language {
            Language::Ru => "Снять в руки".to_owned(),
            Language::En => "Unequip to hands".to_owned(),
        },
        InventoryAction::Load { container_id, .. } => {
            let container = rows
                .get(&container_id)
                .map(|item| locale.item_name(item.kind))
                .unwrap_or("container");
            match locale.language {
                Language::Ru => format!("Загрузить в {container} #{}", container_id.value()),
                Language::En => format!("Load into {container} #{}", container_id.value()),
            }
        }
        InventoryAction::Unload { .. } => match locale.language {
            Language::Ru => "Выгрузить на землю".to_owned(),
            Language::En => "Unload to ground".to_owned(),
        },
    }
}

impl InventoryAction {
    fn command(self) -> Command {
        match self {
            Self::PickUp {
                character_id,
                item_id,
            } => Command::PickUpItem {
                character_id,
                item_id,
            },
            Self::Drop {
                character_id,
                item_id,
            } => Command::DropItem {
                character_id,
                item_id,
            },
            Self::Equip {
                character_id,
                item_id,
            } => Command::EquipItem {
                character_id,
                item_id,
            },
            Self::Unequip {
                character_id,
                item_id,
            } => Command::UnequipItem {
                character_id,
                item_id,
            },
            Self::Load {
                character_id,
                item_id,
                container_id,
            } => Command::LoadContainer {
                character_id,
                item_id,
                container_id,
            },
            Self::Unload {
                character_id,
                item_id,
            } => Command::UnloadContainer {
                character_id,
                item_id,
            },
        }
    }
}

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
