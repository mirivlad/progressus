use bevy::prelude::*;
use progressus_app::{JobState, MAX_SATIETY, MovementState};

use crate::i18n::{Language, Locale, TextKey};
use crate::interaction::TickScheduler;
use crate::modal::ModalState;
use crate::navigation::SelectedCharacter;
use crate::runtime::AuthoritativeClient;
use crate::save_slots::SaveStore;
use crate::ui_font::UiFont;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum ToolMode {
    #[default]
    Select,
    StockpileAdd,
    StockpileRemove,
    Harvest,
    Wall,
    Door,
    Workbench,
    CancelJobs,
}

impl ToolMode {
    pub(crate) const fn text_key(self) -> TextKey {
        match self {
            Self::Select => TextKey::Select,
            Self::StockpileAdd => TextKey::StockpileAdd,
            Self::StockpileRemove => TextKey::StockpileRemove,
            Self::Harvest => TextKey::Harvest,
            Self::Wall => TextKey::StoneWall,
            Self::Door => TextKey::Door,
            Self::Workbench => TextKey::Workbench,
            Self::CancelJobs => TextKey::CancelJobs,
        }
    }

    pub(crate) const fn uses_area_drag(self) -> bool {
        matches!(
            self,
            Self::StockpileAdd
                | Self::StockpileRemove
                | Self::Harvest
                | Self::Wall
                | Self::CancelJobs
        )
    }
}

#[derive(Resource, Debug, Default)]
pub(crate) struct ToolState {
    pub(crate) mode: ToolMode,
    pub(crate) drag_start: Option<progressus_app::WorldCell>,
    pub(crate) drag_current: Option<progressus_app::WorldCell>,
    pub(crate) pointer_over_ui: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HudPalette {
    Orders,
    Zones,
    Build,
}

#[derive(Resource, Debug, Default)]
pub(crate) struct HudPaletteState {
    pub(crate) open: Option<HudPalette>,
}

#[derive(Resource, Debug)]
pub(crate) struct ZoneVisibility {
    pub(crate) visible: bool,
}

impl Default for ZoneVisibility {
    fn default() -> Self {
        Self { visible: true }
    }
}

#[derive(Resource, Debug, Default)]
pub(crate) struct SelectedStockpile(pub(crate) Option<progressus_app::EntityId>);

#[derive(Resource, Debug, Default)]
pub(crate) struct StockpileClickState {
    pub(crate) last: Option<(progressus_app::EntityId, f32)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HudTooltipKind {
    Tool(ToolMode),
    Palette(HudPalette),
    Pause,
    Saves,
    Language,
    ZoneVisibility,
}

impl ToolState {
    pub(crate) fn cancel_drag(&mut self) {
        self.drag_start = None;
        self.drag_current = None;
    }
}

#[derive(Component)]
pub(crate) struct ToolButton(pub(crate) ToolMode);
#[derive(Component)]
pub(crate) struct PaletteToggle(pub(crate) HudPalette);
#[derive(Component)]
pub(crate) struct PalettePanel(pub(crate) HudPalette);
#[derive(Component)]
pub(crate) struct ZoneVisibilityToggle;
#[derive(Component)]
pub(crate) struct HudTooltipSource(pub(crate) HudTooltipKind);
#[derive(Component)]
pub(crate) struct HudTooltipPanel;
#[derive(Component)]
pub(crate) struct HudTooltipText;
#[derive(Component)]
pub(crate) struct ToolStatus;
#[derive(Component)]
pub(crate) struct PauseToggle;
#[derive(Component)]
pub(crate) struct PauseToggleLabel;
#[derive(Component)]
pub(crate) struct SaveMenuButton;
#[derive(Component)]
pub(crate) struct LanguageToggle;
#[derive(Component)]
pub(crate) struct LanguageToggleLabel;
#[derive(Component)]
pub(crate) struct UiCapture;
#[derive(Component)]
pub(crate) struct CharacterInspector;
#[derive(Component)]
pub(crate) struct CharacterInspectorText;
#[derive(Component)]
pub(crate) struct StockpileInspector;
#[derive(Component)]
pub(crate) struct StockpileInspectorText;
#[derive(Component)]
pub(crate) struct ConfigureStockpileButton;
#[derive(Component)]
pub(crate) struct ConfigureStockpileButtonLabel;

const NORMAL_BUTTON: Color = Color::srgb(0.12, 0.14, 0.15);
const HOVERED_BUTTON: Color = Color::srgb(0.20, 0.24, 0.26);
const ACTIVE_BUTTON: Color = Color::srgb(0.10, 0.46, 0.58);

pub(crate) fn setup_toolbar(
    mut commands: Commands,
    locale: Res<Locale>,
    scheduler: Res<TickScheduler>,
    font: Res<UiFont>,
) {
    commands
        .spawn((
            HudTooltipPanel,
            Visibility::Hidden,
            Node {
                position_type: PositionType::Absolute,
                left: px(12),
                bottom: px(76),
                width: px(360),
                padding: UiRect::all(px(9)),
                border: UiRect::all(px(1)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.045, 0.055, 0.06, 0.97)),
            BorderColor::all(Color::srgb(0.28, 0.34, 0.36)),
            GlobalZIndex(20),
        ))
        .with_children(|panel| {
            panel.spawn((
                text_bundle("", &font, 12.5, Color::srgb(0.91, 0.93, 0.94)),
                HudTooltipText,
            ));
        });

    spawn_palette_panel(
        &mut commands,
        HudPalette::Orders,
        54.0,
        &[(ToolMode::Harvest, "H"), (ToolMode::CancelJobs, "×")],
        &font,
    );
    spawn_palette_panel(
        &mut commands,
        HudPalette::Zones,
        102.0,
        &[
            (ToolMode::StockpileAdd, "+"),
            (ToolMode::StockpileRemove, "−"),
        ],
        &font,
    );
    spawn_palette_panel(
        &mut commands,
        HudPalette::Build,
        150.0,
        &[
            (ToolMode::Wall, "W"),
            (ToolMode::Door, "D"),
            (ToolMode::Workbench, "T"),
        ],
        &font,
    );

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(12),
                bottom: px(12),
                height: px(54),
                padding: UiRect::all(px(7)),
                column_gap: px(5),
                align_items: AlignItems::Center,
                border: UiRect::all(px(1)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.045, 0.055, 0.06, 0.94)),
            BorderColor::all(Color::srgb(0.28, 0.34, 0.36)),
            Interaction::default(),
            UiCapture,
        ))
        .with_children(|toolbar| {
            spawn_hud_tool_button(toolbar, ToolMode::Select, "↖", &font);
            spawn_palette_toggle(toolbar, HudPalette::Orders, "O", &font);
            spawn_palette_toggle(toolbar, HudPalette::Zones, "▦", &font);
            spawn_palette_toggle(toolbar, HudPalette::Build, "B", &font);

            toolbar
                .spawn(Node {
                    width: px(1),
                    height: px(30),
                    margin: UiRect::horizontal(px(4)),
                    ..default()
                })
                .insert(BackgroundColor(Color::srgb(0.28, 0.34, 0.36)));

            toolbar
                .spawn((
                    Button,
                    ZoneVisibilityToggle,
                    HudTooltipSource(HudTooltipKind::ZoneVisibility),
                    UiCapture,
                    hud_button_node(),
                    BackgroundColor(ACTIVE_BUTTON),
                    BorderColor::all(Color::srgb(0.30, 0.34, 0.36)),
                ))
                .with_children(|button| {
                    button.spawn(text_bundle("◫", &font, 18.0, Color::srgb(0.91, 0.93, 0.94)));
                });

            toolbar
                .spawn((
                    Button,
                    PauseToggle,
                    HudTooltipSource(HudTooltipKind::Pause),
                    UiCapture,
                    hud_button_node(),
                    BackgroundColor(NORMAL_BUTTON),
                    BorderColor::all(Color::srgb(0.30, 0.34, 0.36)),
                ))
                .with_children(|button| {
                    button.spawn((
                        text_bundle(
                            pause_icon(scheduler.is_paused()),
                            &font,
                            16.0,
                            Color::srgb(0.91, 0.93, 0.94),
                        ),
                        PauseToggleLabel,
                    ));
                });

            toolbar
                .spawn((
                    Button,
                    SaveMenuButton,
                    HudTooltipSource(HudTooltipKind::Saves),
                    UiCapture,
                    hud_button_node(),
                    BackgroundColor(NORMAL_BUTTON),
                    BorderColor::all(Color::srgb(0.30, 0.34, 0.36)),
                ))
                .with_children(|button| {
                    button.spawn(text_bundle("S", &font, 15.0, Color::srgb(0.91, 0.93, 0.94)));
                });

            toolbar
                .spawn((
                    Button,
                    LanguageToggle,
                    HudTooltipSource(HudTooltipKind::Language),
                    UiCapture,
                    hud_button_node(),
                    BackgroundColor(NORMAL_BUTTON),
                    BorderColor::all(Color::srgb(0.30, 0.34, 0.36)),
                ))
                .with_children(|button| {
                    button.spawn((
                        text_bundle(
                            language_target(locale.language),
                            &font,
                            12.0,
                            Color::srgb(0.91, 0.93, 0.94),
                        ),
                        LanguageToggleLabel,
                    ));
                });

            toolbar.spawn((
                Text::new(format!(
                    "{}: {}",
                    locale.tr(TextKey::Mode),
                    locale.tr(ToolMode::Select.text_key())
                )),
                TextFont {
                    font: font.0.clone(),
                    font_size: 12.5,
                    ..default()
                },
                TextColor(Color::srgb(0.82, 0.88, 0.90)),
                Node {
                    margin: UiRect::left(px(7)),
                    ..default()
                },
                ToolStatus,
            ));
        });
}

fn spawn_hud_tool_button(
    parent: &mut ChildSpawnerCommands,
    mode: ToolMode,
    icon: &'static str,
    font: &UiFont,
) {
    parent
        .spawn((
            Button,
            ToolButton(mode),
            HudTooltipSource(HudTooltipKind::Tool(mode)),
            UiCapture,
            hud_button_node(),
            BackgroundColor(NORMAL_BUTTON),
            BorderColor::all(Color::srgb(0.30, 0.34, 0.36)),
        ))
        .with_children(|button| {
            button.spawn(text_bundle(icon, font, 17.0, Color::srgb(0.91, 0.93, 0.94)));
        });
}

fn spawn_palette_toggle(
    parent: &mut ChildSpawnerCommands,
    palette: HudPalette,
    icon: &'static str,
    font: &UiFont,
) {
    parent
        .spawn((
            Button,
            PaletteToggle(palette),
            HudTooltipSource(HudTooltipKind::Palette(palette)),
            UiCapture,
            hud_button_node(),
            BackgroundColor(NORMAL_BUTTON),
            BorderColor::all(Color::srgb(0.30, 0.34, 0.36)),
        ))
        .with_children(|button| {
            button.spawn(text_bundle(icon, font, 16.0, Color::srgb(0.91, 0.93, 0.94)));
        });
}

fn spawn_palette_panel(
    commands: &mut Commands,
    palette: HudPalette,
    left: f32,
    tools: &[(ToolMode, &'static str)],
    font: &UiFont,
) {
    commands
        .spawn((
            PalettePanel(palette),
            Visibility::Hidden,
            Node {
                position_type: PositionType::Absolute,
                left: px(left),
                bottom: px(72),
                height: px(48),
                padding: UiRect::all(px(5)),
                column_gap: px(5),
                align_items: AlignItems::Center,
                border: UiRect::all(px(1)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.045, 0.055, 0.06, 0.97)),
            BorderColor::all(Color::srgb(0.28, 0.34, 0.36)),
            Interaction::default(),
            UiCapture,
            GlobalZIndex(15),
        ))
        .with_children(|panel| {
            for (mode, icon) in tools {
                spawn_hud_tool_button(panel, *mode, icon, font);
            }
        });
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

pub(crate) fn setup_character_inspector(mut commands: Commands, font: Res<UiFont>) {
    commands
        .spawn((
            CharacterInspector,
            UiCapture,
            Interaction::default(),
            Visibility::Hidden,
            Node {
                position_type: PositionType::Absolute,
                right: px(12),
                top: px(12),
                width: px(286),
                padding: UiRect::all(px(12)),
                border: UiRect::all(px(1)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.045, 0.055, 0.06, 0.94)),
            BorderColor::all(Color::srgb(0.28, 0.34, 0.36)),
        ))
        .with_children(|panel| {
            panel.spawn((
                Text::new(""),
                TextFont {
                    font: font.0.clone(),
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::srgb(0.91, 0.93, 0.94)),
                CharacterInspectorText,
            ));
        });
}

pub(crate) fn setup_stockpile_inspector(
    mut commands: Commands,
    locale: Res<Locale>,
    font: Res<UiFont>,
) {
    commands
        .spawn((
            StockpileInspector,
            UiCapture,
            Interaction::default(),
            Visibility::Hidden,
            Node {
                position_type: PositionType::Absolute,
                right: px(12),
                top: px(12),
                width: px(286),
                padding: UiRect::all(px(12)),
                row_gap: px(10),
                flex_direction: FlexDirection::Column,
                border: UiRect::all(px(1)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.045, 0.055, 0.06, 0.94)),
            BorderColor::all(Color::srgb(0.28, 0.34, 0.36)),
        ))
        .with_children(|panel| {
            panel.spawn((
                text_bundle("", &font, 14.0, Color::srgb(0.91, 0.93, 0.94)),
                StockpileInspectorText,
            ));
            panel
                .spawn((
                    Button,
                    ConfigureStockpileButton,
                    UiCapture,
                    Node {
                        height: px(32),
                        padding: UiRect::horizontal(px(10)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(px(1)),
                        ..default()
                    },
                    BackgroundColor(NORMAL_BUTTON),
                    BorderColor::all(Color::srgb(0.30, 0.34, 0.36)),
                ))
                .with_children(|button| {
                    button.spawn((
                        text_bundle(
                            locale.tr(TextKey::Configure),
                            &font,
                            13.0,
                            Color::srgb(0.91, 0.93, 0.94),
                        ),
                        ConfigureStockpileButtonLabel,
                    ));
                });
        });
}

pub(crate) fn sync_stockpile_inspector(
    locale: Res<Locale>,
    mut selected: ResMut<SelectedStockpile>,
    authoritative: Res<AuthoritativeClient>,
    mut panels: Query<&mut Visibility, With<StockpileInspector>>,
    mut texts: Query<&mut Text, With<StockpileInspectorText>>,
) {
    let Some(stockpile_id) = selected.0 else {
        if let Ok(mut visibility) = panels.single_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    };
    let Some(stockpile) = authoritative
        .snapshot()
        .stockpiles
        .iter()
        .find(|stockpile| stockpile.id == stockpile_id)
    else {
        selected.0 = None;
        if let Ok(mut visibility) = panels.single_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    };
    let allowed = progressus_app::ItemKind::ALL
        .into_iter()
        .filter(|kind| !stockpile.disallowed_items.contains(kind))
        .map(|kind| locale.item_name(kind))
        .collect::<Vec<_>>();
    let allowed = if allowed.len() == progressus_app::ItemKind::ALL.len() {
        match locale.language {
            Language::Ru => "все".to_owned(),
            Language::En => "all".to_owned(),
        }
    } else if allowed.is_empty() {
        locale.tr(TextKey::NoneValue).to_owned()
    } else {
        allowed.join(", ")
    };
    if let Ok(mut text) = texts.single_mut() {
        **text = format!(
            "{} #{}\n{}: {}\n{}: {}",
            locale.tr(TextKey::Stockpile),
            stockpile.id.value(),
            locale.tr(TextKey::Cell),
            stockpile.cells.len(),
            locale.tr(TextKey::AcceptedItems),
            allowed
        );
    }
    if let Ok(mut visibility) = panels.single_mut() {
        *visibility = Visibility::Visible;
    }
}

pub(crate) fn configure_stockpile_interaction(
    buttons: Query<&Interaction, (Changed<Interaction>, With<ConfigureStockpileButton>)>,
    selected: Res<SelectedStockpile>,
    mut modal: ResMut<ModalState>,
) {
    if buttons
        .iter()
        .any(|interaction| *interaction == Interaction::Pressed)
        && let Some(stockpile_id) = selected.0
    {
        modal.open_stockpile(stockpile_id);
    }
}

pub(crate) fn sync_character_inspector(
    locale: Res<Locale>,
    selected: Res<SelectedCharacter>,
    authoritative: Res<AuthoritativeClient>,
    mut panels: Query<&mut Visibility, With<CharacterInspector>>,
    mut texts: Query<&mut Text, With<CharacterInspectorText>>,
) {
    let Some(character_id) = selected.0 else {
        if let Ok(mut visibility) = panels.single_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    };
    let snapshot = authoritative.snapshot();
    let Some(character) = snapshot
        .characters
        .iter()
        .find(|character| character.id == character_id)
    else {
        if let Ok(mut visibility) = panels.single_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    };

    let movement = match character.movement {
        MovementState::Idle => locale.movement_name(character.movement).to_owned(),
        MovementState::ManualDirectional { direction } => format!(
            "{} ({})",
            locale.movement_name(character.movement),
            locale.direction_name(direction)
        ),
        MovementState::Navigating { destination } => {
            let cell = destination.containing_cell();
            format!(
                "{} -> ({}, {})",
                locale.movement_name(character.movement),
                cell.x(),
                cell.y()
            )
        }
    };

    let work = snapshot
        .jobs
        .iter()
        .find(|job| job.state.worker() == Some(character_id))
        .map(|job| {
            let suffix = match job.state {
                JobState::Working {
                    remaining_ticks, ..
                } => match locale.language {
                    Language::Ru => format!(" ({remaining_ticks} т.)"),
                    Language::En => format!(" ({remaining_ticks} ticks)"),
                },
                _ => String::new(),
            };
            format!(
                "{} - {}{}",
                locale.job_kind_name(job.kind),
                locale.job_state_name(job.state),
                suffix
            )
        })
        .unwrap_or_else(|| locale.tr(TextKey::NoneValue).to_owned());

    let carrying = snapshot
        .carried_items
        .iter()
        .filter(|item| item.character_id == character_id)
        .map(|item| format!("{} x{}", locale.item_name(item.kind), item.quantity))
        .collect::<Vec<_>>();
    let carrying = if carrying.is_empty() {
        locale.tr(TextKey::NoneValue).to_owned()
    } else {
        carrying.join(", ")
    };

    let text = format!(
        "{}: {}\n{}: {}\n{}: ({}, {})\n{}: {}/{}\n{}: {}\n{}: {}\n{}: {}",
        locale.tr(TextKey::Character),
        character.name,
        locale.tr(TextKey::Identifier),
        character.id.value(),
        locale.tr(TextKey::Cell),
        character.containing_cell.x(),
        character.containing_cell.y(),
        locale.tr(TextKey::Satiety),
        character.satiety,
        MAX_SATIETY,
        locale.tr(TextKey::Movement),
        movement,
        locale.tr(TextKey::Work),
        work,
        locale.tr(TextKey::Carrying),
        carrying
    );
    if let Ok(mut output) = texts.single_mut() {
        **output = text;
    }
    if let Ok(mut visibility) = panels.single_mut() {
        *visibility = Visibility::Visible;
    }
}

fn hud_button_node() -> Node {
    Node {
        width: px(38),
        height: px(36),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        border: UiRect::all(px(1)),
        ..default()
    }
}

const fn language_target(language: Language) -> &'static str {
    match language {
        Language::Ru => "EN",
        Language::En => "RU",
    }
}

const fn pause_icon(paused: bool) -> &'static str {
    if paused { "▶" } else { "Ⅱ" }
}

pub(crate) fn toolbar_interaction(
    keys: Res<ButtonInput<KeyCode>>,
    locale: Res<Locale>,
    mut state: ResMut<ToolState>,
    mut palettes: ResMut<HudPaletteState>,
    mut buttons: Query<
        (
            &Interaction,
            &ToolButton,
            &mut BackgroundColor,
            &mut BorderColor,
        ),
        With<Button>,
    >,
    mut status: Query<&mut Text, With<ToolStatus>>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        state.mode = ToolMode::Select;
        state.cancel_drag();
        palettes.open = None;
    }

    for (interaction, button, mut background, mut border) in &mut buttons {
        if *interaction == Interaction::Pressed {
            state.mode = button.0;
            state.cancel_drag();
            palettes.open = None;
        }
        *background = BackgroundColor(if state.mode == button.0 {
            ACTIVE_BUTTON
        } else if *interaction == Interaction::Hovered {
            HOVERED_BUTTON
        } else {
            NORMAL_BUTTON
        });
        *border = BorderColor::all(if state.mode == button.0 {
            Color::srgb(0.32, 0.92, 1.0)
        } else {
            Color::srgb(0.30, 0.34, 0.36)
        });
    }

    if let Ok(mut text) = status.single_mut() {
        **text = format!(
            "{}: {}",
            locale.tr(TextKey::Mode),
            locale.tr(state.mode.text_key())
        );
    }
}

pub(crate) fn hud_palette_interaction(
    mut state: ResMut<HudPaletteState>,
    pressed: Query<(&Interaction, &PaletteToggle), (Changed<Interaction>, With<Button>)>,
    mut buttons: Query<(&Interaction, &PaletteToggle, &mut BackgroundColor), With<Button>>,
    mut panels: Query<(&PalettePanel, &mut Visibility)>,
) {
    for (interaction, button) in &pressed {
        if *interaction == Interaction::Pressed {
            state.open = if state.open == Some(button.0) {
                None
            } else {
                Some(button.0)
            };
        }
    }
    for (interaction, button, mut background) in &mut buttons {
        *background = BackgroundColor(if state.open == Some(button.0) {
            ACTIVE_BUTTON
        } else if *interaction == Interaction::Hovered {
            HOVERED_BUTTON
        } else {
            NORMAL_BUTTON
        });
    }
    for (panel, mut visibility) in &mut panels {
        *visibility = if state.open == Some(panel.0) {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

pub(crate) fn zone_visibility_interaction(
    mut zones: ResMut<ZoneVisibility>,
    mut buttons: Query<
        (&Interaction, &mut BackgroundColor),
        (Changed<Interaction>, With<ZoneVisibilityToggle>),
    >,
) {
    for (interaction, mut background) in &mut buttons {
        if *interaction == Interaction::Pressed {
            zones.visible = !zones.visible;
        }
        *background = BackgroundColor(if zones.visible {
            ACTIVE_BUTTON
        } else if *interaction == Interaction::Hovered {
            HOVERED_BUTTON
        } else {
            NORMAL_BUTTON
        });
    }
}

pub(crate) fn pause_toggle_interaction(
    keys: Res<ButtonInput<KeyCode>>,
    mut scheduler: ResMut<TickScheduler>,
    buttons: Query<&Interaction, (Changed<Interaction>, With<PauseToggle>)>,
    mut labels: Query<&mut Text, With<PauseToggleLabel>>,
) {
    let pressed = buttons
        .iter()
        .any(|interaction| *interaction == Interaction::Pressed);
    if !pressed && !keys.just_pressed(KeyCode::KeyP) {
        return;
    }
    let paused = scheduler.toggle_paused();
    if let Ok(mut text) = labels.single_mut() {
        **text = pause_icon(paused).to_owned();
    }
}

pub(crate) fn save_menu_interaction(
    buttons: Query<&Interaction, (Changed<Interaction>, With<SaveMenuButton>)>,
    mut modal: ResMut<ModalState>,
    mut tool: ResMut<ToolState>,
    mut palettes: ResMut<HudPaletteState>,
    mut save_store: ResMut<SaveStore>,
) {
    if buttons
        .iter()
        .any(|interaction| *interaction == Interaction::Pressed)
    {
        tool.mode = ToolMode::Select;
        tool.cancel_drag();
        palettes.open = None;
        save_store.refresh();
        modal.open_saves();
    }
}

pub(crate) fn language_toggle_interaction(
    mut locale: ResMut<Locale>,
    buttons: Query<&Interaction, (Changed<Interaction>, With<LanguageToggle>)>,
) {
    if buttons
        .iter()
        .any(|interaction| *interaction == Interaction::Pressed)
    {
        locale.language = locale.language.toggled();
    }
}

pub(crate) fn refresh_toolbar_localization(
    locale: Res<Locale>,
    mut language_labels: Query<&mut Text, With<LanguageToggleLabel>>,
    mut configure_labels: Query<
        &mut Text,
        (
            With<ConfigureStockpileButtonLabel>,
            Without<LanguageToggleLabel>,
        ),
    >,
) {
    if !locale.is_changed() {
        return;
    }
    if let Ok(mut text) = language_labels.single_mut() {
        **text = language_target(locale.language).to_owned();
    }
    if let Ok(mut text) = configure_labels.single_mut() {
        **text = locale.tr(TextKey::Configure).to_owned();
    }
}

pub(crate) fn sync_hud_tooltip(
    locale: Res<Locale>,
    scheduler: Res<TickScheduler>,
    zones: Res<ZoneVisibility>,
    sources: Query<(&Interaction, &HudTooltipSource)>,
    mut panels: Query<&mut Visibility, With<HudTooltipPanel>>,
    mut texts: Query<&mut Text, With<HudTooltipText>>,
) {
    let hovered = sources
        .iter()
        .find(|(interaction, _)| **interaction == Interaction::Hovered)
        .map(|(_, source)| source.0);
    let Ok(mut visibility) = panels.single_mut() else {
        return;
    };
    let Some(kind) = hovered else {
        *visibility = Visibility::Hidden;
        return;
    };
    if let Ok(mut text) = texts.single_mut() {
        **text = hud_tooltip_text(*locale, kind, scheduler.is_paused(), zones.visible);
    }
    *visibility = Visibility::Visible;
}

fn hud_tooltip_text(
    locale: Locale,
    kind: HudTooltipKind,
    paused: bool,
    zones_visible: bool,
) -> String {
    let (name, description, hotkey) = match (locale.language, kind) {
        (Language::Ru, HudTooltipKind::Tool(ToolMode::Select)) => {
            ("Выбор", "Выбрать персонажа, склад или объект.", Some("Esc"))
        }
        (Language::Ru, HudTooltipKind::Tool(ToolMode::Harvest)) => (
            "Добыча",
            "Назначить добычу ресурсов в выделенной области.",
            None,
        ),
        (Language::Ru, HudTooltipKind::Tool(ToolMode::CancelJobs)) => (
            "Отмена задач",
            "Отменить задания в выделенной области.",
            None,
        ),
        (Language::Ru, HudTooltipKind::Tool(ToolMode::StockpileAdd)) => (
            "Добавить склад",
            "Создать или расширить складскую зону.",
            None,
        ),
        (Language::Ru, HudTooltipKind::Tool(ToolMode::StockpileRemove)) => {
            ("Убрать склад", "Убрать клетки из складской зоны.", None)
        }
        (Language::Ru, HudTooltipKind::Tool(ToolMode::Wall)) => (
            "Каменная стена",
            "Запланировать строительство каменной стены.",
            None,
        ),
        (Language::Ru, HudTooltipKind::Tool(ToolMode::Door)) => (
            "Дверь",
            "Поставить автоматически открывающуюся дверь в проходе стены.",
            None,
        ),
        (Language::Ru, HudTooltipKind::Tool(ToolMode::Workbench)) => {
            ("Верстак", "Поставить производственный верстак.", None)
        }
        (Language::Ru, HudTooltipKind::Palette(HudPalette::Orders)) => (
            "Задания",
            "Открыть инструменты приказов и отмены работ.",
            None,
        ),
        (Language::Ru, HudTooltipKind::Palette(HudPalette::Zones)) => {
            ("Зоны", "Открыть инструменты складских зон.", None)
        }
        (Language::Ru, HudTooltipKind::Palette(HudPalette::Build)) => {
            ("Строительство", "Открыть палитру строительства.", None)
        }
        (Language::Ru, HudTooltipKind::Pause) => (
            if paused {
                "Продолжить"
            } else {
                "Пауза"
            },
            "Остановить или продолжить ход симуляции.",
            Some("P"),
        ),
        (Language::Ru, HudTooltipKind::Saves) => {
            ("Сохранения", "Открыть меню сохранения и загрузки.", None)
        }
        (Language::Ru, HudTooltipKind::Language) => ("Язык", "Переключить язык интерфейса.", None),
        (Language::Ru, HudTooltipKind::ZoneVisibility) => (
            if zones_visible {
                "Скрыть зоны"
            } else {
                "Показать зоны"
            },
            "Переключить отображение складских зон на карте.",
            None,
        ),
        (Language::En, HudTooltipKind::Tool(ToolMode::Select)) => (
            "Select",
            "Select a character, stockpile, or object.",
            Some("Esc"),
        ),
        (Language::En, HudTooltipKind::Tool(ToolMode::Harvest)) => (
            "Harvest",
            "Designate resources for harvesting in an area.",
            None,
        ),
        (Language::En, HudTooltipKind::Tool(ToolMode::CancelJobs)) => {
            ("Cancel jobs", "Cancel jobs in the selected area.", None)
        }
        (Language::En, HudTooltipKind::Tool(ToolMode::StockpileAdd)) => {
            ("Add stockpile", "Create or extend a stockpile zone.", None)
        }
        (Language::En, HudTooltipKind::Tool(ToolMode::StockpileRemove)) => (
            "Remove stockpile",
            "Remove cells from a stockpile zone.",
            None,
        ),
        (Language::En, HudTooltipKind::Tool(ToolMode::Wall)) => {
            ("Stone wall", "Designate stone wall construction.", None)
        }
        (Language::En, HudTooltipKind::Tool(ToolMode::Door)) => (
            "Door",
            "Place an automatically opening door in a wall passage.",
            None,
        ),
        (Language::En, HudTooltipKind::Tool(ToolMode::Workbench)) => {
            ("Workbench", "Place a production workbench.", None)
        }
        (Language::En, HudTooltipKind::Palette(HudPalette::Orders)) => {
            ("Orders", "Open work-order and cancellation tools.", None)
        }
        (Language::En, HudTooltipKind::Palette(HudPalette::Zones)) => {
            ("Zones", "Open stockpile zone tools.", None)
        }
        (Language::En, HudTooltipKind::Palette(HudPalette::Build)) => {
            ("Build", "Open the construction palette.", None)
        }
        (Language::En, HudTooltipKind::Pause) => (
            if paused { "Resume" } else { "Pause" },
            "Pause or resume simulation time.",
            Some("P"),
        ),
        (Language::En, HudTooltipKind::Saves) => ("Saves", "Open save and load controls.", None),
        (Language::En, HudTooltipKind::Language) => {
            ("Language", "Switch the interface language.", None)
        }
        (Language::En, HudTooltipKind::ZoneVisibility) => (
            if zones_visible {
                "Hide zones"
            } else {
                "Show zones"
            },
            "Toggle stockpile zone overlays on the map.",
            None,
        ),
    };
    hotkey.map_or_else(
        || format!("{name}\n{description}"),
        |key| format!("{name}\n{description}  [{key}]"),
    )
}

pub(crate) fn update_ui_capture(
    interactions: Query<&Interaction, With<UiCapture>>,
    mut state: ResMut<ToolState>,
) {
    state.pointer_over_ui = interactions
        .iter()
        .any(|interaction| *interaction != Interaction::None);
}

#[cfg(test)]
mod tests {
    use super::ToolMode;
    use crate::i18n::{Locale, TextKey};

    #[test]
    fn toolbar_modes_are_localized_instead_of_owning_display_strings() {
        let locale = Locale::default();
        assert_eq!(ToolMode::Workbench.text_key(), TextKey::Workbench);
        assert_eq!(locale.tr(ToolMode::Workbench.text_key()), "Верстак");
        assert!(ToolMode::Wall.uses_area_drag());
        assert!(!ToolMode::Workbench.uses_area_drag());
    }
}
