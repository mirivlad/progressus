use bevy::prelude::*;
use progressus_app::{JobState, MovementState};

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

impl ToolState {
    pub(crate) fn cancel_drag(&mut self) {
        self.drag_start = None;
        self.drag_current = None;
    }
}

#[derive(Component)]
pub(crate) struct ToolButton(pub(crate) ToolMode);
#[derive(Component)]
pub(crate) struct ToolButtonLabel(ToolMode);
#[derive(Component)]
pub(crate) struct ToolStatus;
#[derive(Component)]
pub(crate) struct PauseToggle;
#[derive(Component)]
pub(crate) struct PauseToggleLabel;
#[derive(Component)]
pub(crate) struct SaveMenuButton;
#[derive(Component)]
pub(crate) struct SaveMenuButtonLabel;
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

const NORMAL_BUTTON: Color = Color::srgb(0.12, 0.14, 0.15);
const HOVERED_BUTTON: Color = Color::srgb(0.20, 0.24, 0.26);
const ACTIVE_BUTTON: Color = Color::srgb(0.10, 0.46, 0.58);

pub(crate) fn setup_toolbar(
    mut commands: Commands,
    locale: Res<Locale>,
    scheduler: Res<TickScheduler>,
    font: Res<UiFont>,
) {
    let modes = [
        ToolMode::Select,
        ToolMode::StockpileAdd,
        ToolMode::StockpileRemove,
        ToolMode::Harvest,
        ToolMode::Wall,
        ToolMode::Workbench,
        ToolMode::CancelJobs,
    ];
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(12),
                bottom: px(12),
                height: px(54),
                padding: UiRect::all(px(7)),
                column_gap: px(6),
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
            for mode in modes {
                toolbar
                    .spawn((
                        Button,
                        ToolButton(mode),
                        UiCapture,
                        tool_button_node(),
                        BackgroundColor(NORMAL_BUTTON),
                        BorderColor::all(Color::srgb(0.30, 0.34, 0.36)),
                    ))
                    .with_children(|button| {
                        button.spawn((
                            Text::new(locale.tr(mode.text_key())),
                            TextFont {
                                font: font.0.clone(),
                                font_size: 13.0,
                                ..default()
                            },
                            TextColor(Color::srgb(0.91, 0.93, 0.94)),
                            ToolButtonLabel(mode),
                        ));
                    });
            }

            toolbar.spawn((
                Text::new(format!(
                    "{}: {}",
                    locale.tr(TextKey::Mode),
                    locale.tr(ToolMode::Select.text_key())
                )),
                TextFont {
                    font: font.0.clone(),
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(0.82, 0.88, 0.90)),
                Node {
                    margin: UiRect::left(px(8)),
                    ..default()
                },
                ToolStatus,
            ));

            toolbar
                .spawn((
                    Button,
                    PauseToggle,
                    UiCapture,
                    Node {
                        min_width: px(96),
                        height: px(36),
                        margin: UiRect::left(px(8)),
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
                        Text::new(pause_label(*locale, scheduler.is_paused())),
                        TextFont {
                            font: font.0.clone(),
                            font_size: 13.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.91, 0.93, 0.94)),
                        PauseToggleLabel,
                    ));
                });

            toolbar
                .spawn((
                    Button,
                    SaveMenuButton,
                    UiCapture,
                    Node {
                        min_width: px(96),
                        height: px(36),
                        margin: UiRect::left(px(8)),
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
                        Text::new(locale.tr(TextKey::Saves)),
                        TextFont {
                            font: font.0.clone(),
                            font_size: 13.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.91, 0.93, 0.94)),
                        SaveMenuButtonLabel,
                    ));
                });

            toolbar
                .spawn((
                    Button,
                    LanguageToggle,
                    UiCapture,
                    Node {
                        width: px(44),
                        height: px(36),
                        margin: UiRect::left(px(8)),
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
                        Text::new(language_target(locale.language)),
                        TextFont {
                            font: font.0.clone(),
                            font_size: 13.0,
                            ..default()
                        },
                        TextColor(Color::srgb(0.91, 0.93, 0.94)),
                        LanguageToggleLabel,
                    ));
                });
        });
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
        "{}: {}\n{}: {}\n{}: ({}, {})\n{}: {}\n{}: {}\n{}: {}",
        locale.tr(TextKey::Character),
        character.name,
        locale.tr(TextKey::Identifier),
        character.id.value(),
        locale.tr(TextKey::Cell),
        character.containing_cell.x(),
        character.containing_cell.y(),
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

fn tool_button_node() -> Node {
    Node {
        min_width: px(86),
        height: px(36),
        padding: UiRect::horizontal(px(10)),
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

fn pause_label(locale: Locale, paused: bool) -> &'static str {
    locale.tr(if paused {
        TextKey::Resume
    } else {
        TextKey::Pause
    })
}

pub(crate) fn toolbar_interaction(
    keys: Res<ButtonInput<KeyCode>>,
    locale: Res<Locale>,
    mut state: ResMut<ToolState>,
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
    }

    for (interaction, button, mut background, mut border) in &mut buttons {
        if *interaction == Interaction::Pressed {
            state.mode = button.0;
            state.cancel_drag();
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

pub(crate) fn pause_toggle_interaction(
    keys: Res<ButtonInput<KeyCode>>,
    locale: Res<Locale>,
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
        **text = pause_label(*locale, paused).to_owned();
    }
}

pub(crate) fn save_menu_interaction(
    buttons: Query<&Interaction, (Changed<Interaction>, With<SaveMenuButton>)>,
    mut modal: ResMut<ModalState>,
    mut tool: ResMut<ToolState>,
    mut save_store: ResMut<SaveStore>,
) {
    if buttons
        .iter()
        .any(|interaction| *interaction == Interaction::Pressed)
    {
        tool.mode = ToolMode::Select;
        tool.cancel_drag();
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

type PauseLabelFilter = (With<PauseToggleLabel>, Without<ToolButtonLabel>);
type SaveMenuLabelFilter = (
    With<SaveMenuButtonLabel>,
    Without<ToolButtonLabel>,
    Without<PauseToggleLabel>,
);
type LanguageLabelFilter = (
    With<LanguageToggleLabel>,
    Without<ToolButtonLabel>,
    Without<PauseToggleLabel>,
    Without<SaveMenuButtonLabel>,
);

pub(crate) fn refresh_toolbar_localization(
    locale: Res<Locale>,
    scheduler: Res<TickScheduler>,
    mut tool_labels: Query<(&ToolButtonLabel, &mut Text)>,
    mut pause_labels: Query<&mut Text, PauseLabelFilter>,
    mut save_labels: Query<&mut Text, SaveMenuLabelFilter>,
    mut language_labels: Query<&mut Text, LanguageLabelFilter>,
) {
    if !locale.is_changed() {
        return;
    }
    for (label, mut text) in &mut tool_labels {
        **text = locale.tr(label.0.text_key()).to_owned();
    }
    if let Ok(mut text) = pause_labels.single_mut() {
        **text = pause_label(*locale, scheduler.is_paused()).to_owned();
    }
    if let Ok(mut text) = save_labels.single_mut() {
        **text = locale.tr(TextKey::Saves).to_owned();
    }
    if let Ok(mut text) = language_labels.single_mut() {
        **text = language_target(locale.language).to_owned();
    }
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
