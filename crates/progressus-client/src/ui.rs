use bevy::prelude::*;
use progressus_app::{EntityId, ProductionZoneKind};

use crate::i18n::{Language, Locale, TextKey};
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
    ProductionZoneAdd {
        workstation_id: EntityId,
        kind: ProductionZoneKind,
    },
    ProductionZoneRemove {
        workstation_id: EntityId,
        kind: ProductionZoneKind,
    },
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
            Self::ProductionZoneAdd {
                kind: ProductionZoneKind::Input,
                ..
            } => TextKey::InputZoneAdd,
            Self::ProductionZoneRemove {
                kind: ProductionZoneKind::Input,
                ..
            } => TextKey::InputZoneRemove,
            Self::ProductionZoneAdd {
                kind: ProductionZoneKind::Output,
                ..
            } => TextKey::OutputZoneAdd,
            Self::ProductionZoneRemove {
                kind: ProductionZoneKind::Output,
                ..
            } => TextKey::OutputZoneRemove,
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
                | Self::ProductionZoneAdd { .. }
                | Self::ProductionZoneRemove { .. }
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
pub(crate) struct LanguageToggle;
#[derive(Component)]
pub(crate) struct LanguageToggleLabel;
#[derive(Component)]
pub(crate) struct UiCapture;

const NORMAL_BUTTON: Color = Color::srgb(0.12, 0.14, 0.15);
const HOVERED_BUTTON: Color = Color::srgb(0.20, 0.24, 0.26);
const ACTIVE_BUTTON: Color = Color::srgb(0.10, 0.46, 0.58);

pub(crate) fn setup_toolbar(mut commands: Commands, locale: Res<Locale>, font: Res<UiFont>) {
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
    mut tool_labels: Query<(&ToolButtonLabel, &mut Text)>,
    mut language_labels: Query<&mut Text, (With<LanguageToggleLabel>, Without<ToolButtonLabel>)>,
) {
    if !locale.is_changed() {
        return;
    }
    for (label, mut text) in &mut tool_labels {
        **text = locale.tr(label.0.text_key()).to_owned();
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
