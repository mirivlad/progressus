use bevy::prelude::*;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum ToolMode {
    #[default]
    Select,
    StockpileAdd,
    StockpileRemove,
    Harvest,
    Wall,
    Workbench,
    Craft,
    CancelJobs,
}

impl ToolMode {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Select => "Select",
            Self::StockpileAdd => "Stockpile +",
            Self::StockpileRemove => "Stockpile -",
            Self::Harvest => "Harvest",
            Self::Wall => "Stone wall",
            Self::Workbench => "Workbench",
            Self::Craft => "Craft tool",
            Self::CancelJobs => "Cancel jobs",
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
pub(crate) struct ToolButton(ToolMode);

#[derive(Component)]
pub(crate) struct ToolStatus;

#[derive(Component)]
pub(crate) struct UiCapture;

const NORMAL_BUTTON: Color = Color::srgb(0.12, 0.14, 0.15);
const HOVERED_BUTTON: Color = Color::srgb(0.20, 0.24, 0.26);
const ACTIVE_BUTTON: Color = Color::srgb(0.10, 0.46, 0.58);

pub(crate) fn setup_toolbar(mut commands: Commands) {
    commands.spawn((
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
        children![
            tool_button(ToolMode::Select),
            tool_button(ToolMode::StockpileAdd),
            tool_button(ToolMode::StockpileRemove),
            tool_button(ToolMode::Harvest),
            tool_button(ToolMode::Wall),
            tool_button(ToolMode::Workbench),
            tool_button(ToolMode::Craft),
            tool_button(ToolMode::CancelJobs),
            (
                Text::new("Mode: Select"),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(Color::srgb(0.82, 0.88, 0.90)),
                Node {
                    margin: UiRect::left(px(8)),
                    ..default()
                },
                ToolStatus,
            ),
        ],
    ));
}

fn tool_button(mode: ToolMode) -> impl Bundle {
    (
        Button,
        ToolButton(mode),
        UiCapture,
        Node {
            min_width: px(86),
            height: px(36),
            padding: UiRect::horizontal(px(10)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            border: UiRect::all(px(1)),
            ..default()
        },
        BackgroundColor(NORMAL_BUTTON),
        BorderColor::all(Color::srgb(0.30, 0.34, 0.36)),
        children![(
            Text::new(mode.label()),
            TextFont {
                font_size: 13.0,
                ..default()
            },
            TextColor(Color::srgb(0.91, 0.93, 0.94)),
        )],
    )
}

pub(crate) fn toolbar_interaction(
    keys: Res<ButtonInput<KeyCode>>,
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
        **text = format!("Mode: {}", state.mode.label());
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
