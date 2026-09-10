use super::*;
use bevy::{
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll},
    window::PrimaryWindow,
};
use progressus_app::{RecipeId, StructureKind, WorkstationKind};

#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Tool {
    #[default]
    Select,
    Harvest,
    Stockpile,
    Wall,
    Door,
    Workbench,
    Craft,
    Cancel,
}
impl Tool {
    fn label(self) -> &'static str {
        match self {
            Self::Select => "Select",
            Self::Harvest => "Harvest",
            Self::Stockpile => "Stockpile",
            Self::Wall => "Stone wall",
            Self::Door => "Door",
            Self::Workbench => "Workbench",
            Self::Craft => "Craft tool",
            Self::Cancel => "Cancel construction",
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn camera_controls(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    mouse: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    time: Res<Time>,
    mut view: ResMut<View>,
    mut cameras: Query<(&mut Transform, &mut Projection), With<Camera3d>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    view.height = (view.height * (1. - scroll.delta.y * 0.08).clamp(0.5, 2.)).clamp(6., 72.);
    if keys.pressed(KeyCode::KeyQ) {
        view.yaw += time.delta_secs();
    }
    if keys.pressed(KeyCode::KeyE) {
        view.yaw -= time.delta_secs();
    }
    let right = Vec3::new(view.yaw.cos(), 0., -view.yaw.sin());
    let up = Vec3::new(-view.yaw.sin(), 0., -view.yaw.cos());
    let horizontal = f32::from(keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight))
        - f32::from(keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft));
    let vertical = f32::from(keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp))
        - f32::from(keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown));
    let speed = view.height * 0.65 * time.delta_secs();
    view.focus += (right * horizontal + up * vertical) * speed;
    if buttons.pressed(MouseButton::Middle) {
        let scale = view.height / window.height().max(1.);
        view.focus += -right * mouse.delta.x * scale + up * mouse.delta.y * scale * 1.35;
    }
    if view.focus.x.abs() > 64. || view.focus.z.abs() > 64. {
        let dx = view.focus.x.round() as i64;
        let dy = (-view.focus.z).round() as i64;
        if let (Some(x), Some(y)) = (
            view.origin.x().checked_add(dx),
            view.origin.y().checked_add(dy),
        ) {
            view.origin = WorldCell::new(x, y);
            view.focus -= Vec3::new(dx as f32, 0., -dy as f32);
            view.rebased = true;
        }
    }
    for (mut transform, mut projection) in &mut cameras {
        *transform = view.camera_transform();
        if let Projection::Orthographic(p) = &mut *projection {
            p.scaling_mode = ScalingMode::FixedVertical {
                viewport_height: view.height,
            };
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn input(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    pawns: Query<(&Pawn, &Transform)>,
    mut view: ResMut<View>,
    mut game: ResMut<Game>,
    mut tool: ResMut<Tool>,
    mut clock: ResMut<TickScheduler>,
    hud_nodes: Query<(&ComputedNode, &UiGlobalTransform), Or<(With<Hud>, With<Toolbar>)>>,
) {
    for (key, mode) in [
        (KeyCode::Digit1, Tool::Select),
        (KeyCode::Digit2, Tool::Harvest),
        (KeyCode::Digit3, Tool::Stockpile),
        (KeyCode::Digit4, Tool::Wall),
        (KeyCode::Digit5, Tool::Door),
        (KeyCode::Digit6, Tool::Workbench),
        (KeyCode::Digit7, Tool::Craft),
        (KeyCode::Digit8, Tool::Cancel),
    ] {
        if keys.just_pressed(key) {
            *tool = mode;
        }
    }
    if keys.just_pressed(KeyCode::Escape) {
        *tool = Tool::Select;
    }
    if keys.just_pressed(KeyCode::KeyP) {
        clock.toggle_paused();
    }
    if keys.just_pressed(KeyCode::KeyF)
        && let Some(id) = game.selected
        && let Some(c) = game.snapshot.characters.iter().find(|c| c.id == id)
    {
        view.origin = c.containing_cell;
        view.focus = space::local(c.position, view.origin);
        view.rebased = true;
    }
    if buttons.pressed(MouseButton::Middle) {
        return;
    }
    if !(buttons.just_pressed(MouseButton::Left) || buttons.just_pressed(MouseButton::Right)) {
        return;
    }
    if buttons.just_pressed(MouseButton::Right) && *tool != Tool::Select {
        *tool = Tool::Select;
        return;
    }
    let (Ok(window), Ok((camera, _))) = (windows.single(), cameras.single()) else {
        return;
    };
    let transform = &GlobalTransform::from(view.camera_transform());
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    // The compact HUD owns this rectangle; clicks must not pass through it.
    if hud_nodes
        .iter()
        .any(|(node, transform)| node.contains_point(*transform, cursor * window.scale_factor()))
    {
        return;
    }
    let Ok(ray) = camera.viewport_to_world(transform, cursor) else {
        return;
    };
    let Some(point) = space::ground(ray) else {
        return;
    };
    let Some(target) = space::position(point, view.origin) else {
        return;
    };
    if buttons.just_pressed(MouseButton::Left) {
        let nearest = pawns
            .iter()
            .filter_map(|(pawn, t)| {
                let feet = camera.world_to_viewport(transform, t.translation).ok()?;
                let head = camera
                    .world_to_viewport(transform, t.translation + Vec3::Y * 0.9)
                    .ok()?;
                let segment = head - feet;
                let fraction =
                    ((cursor - feet).dot(segment) / segment.length_squared().max(1.)).clamp(0., 1.);
                let distance = cursor.distance(feet + fraction * segment);
                (distance < 14.).then_some((distance, pawn.0))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        if let Some((_, id)) = nearest {
            game.selected = Some(id);
            game.refresh();
            return;
        }
        let cell = target.containing_cell();
        let command = match *tool {
            Tool::Select => {
                game.selected = None;
                game.refresh();
                None
            }
            Tool::Harvest => Some(Command::DesignateHarvest { source: cell }),
            Tool::Stockpile => Some(Command::CreateStockpile { cell }),
            Tool::Wall => Some(Command::DesignateConstruction {
                kind: StructureKind::StoneWall,
                cell,
            }),
            Tool::Door => Some(Command::DesignateConstruction {
                kind: StructureKind::Door,
                cell,
            }),
            Tool::Workbench => Some(Command::PlaceWorkstation {
                kind: WorkstationKind::Workbench,
                cell,
            }),
            Tool::Craft => game
                .snapshot
                .workstations
                .iter()
                .find(|w| w.cell == cell)
                .map(|w| Command::DesignateCraft {
                    workstation_id: w.id,
                    recipe_id: RecipeId::PrimitiveTool,
                }),
            Tool::Cancel => game
                .snapshot
                .construction_sites
                .iter()
                .find(|s| s.cell == cell)
                .map(|s| Command::CancelConstruction { site_id: s.id }),
        };
        if let Some(command) = command {
            game.execute(command);
        }
    } else if let Some(character_id) = game.selected {
        game.execute(Command::MoveTo {
            character_id,
            destination: target,
        });
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn overlay(
    mut gizmos: Gizmos,
    game: Res<Game>,
    view: Res<View>,
    tool: Res<Tool>,
    clock: Res<TickScheduler>,
    pawns: Query<(&Pawn, &Transform)>,
    mut hud: Query<&mut Text, With<Hud>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
) {
    let gold = Color::srgb(1., 0.78, 0.29);
    let circle = |p: Vec3| {
        Isometry3d::new(
            p + Vec3::Y * 0.035,
            Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
        )
    };
    if let Some(id) = game.selected
        && let Some((_, t)) = pawns.iter().find(|(p, _)| p.0 == id)
    {
        gizmos.circle(circle(t.translation), 0.42, gold);
    }
    if let Some(nav) = &game.snapshot.navigation {
        let mut previous = pawns
            .iter()
            .find(|(p, _)| p.0 == nav.character_id)
            .map(|(_, t)| t.translation + Vec3::Y * 0.045);
        for waypoint in &nav.remaining_waypoints {
            let p = space::local(*waypoint, view.origin) + Vec3::Y * 0.045;
            if let Some(a) = previous {
                gizmos.line(a, p, gold);
            }
            previous = Some(p);
        }
        if let Some(destination) = nav.destination {
            gizmos.circle(circle(space::local(destination, view.origin)), 0.25, gold);
        }
    }
    for stockpile in &game.snapshot.stockpiles {
        for &cell in &stockpile.cells {
            let p = space::cell_local(cell, view.origin) + Vec3::Y * 0.035;
            gizmos.rect(
                Isometry3d::new(p, Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                Vec2::splat(0.94),
                Color::srgb(0.40, 0.80, 0.84),
            );
        }
    }
    if *tool != Tool::Select
        && let (Ok(window), Ok((camera, transform))) = (windows.single(), cameras.single())
        && let Some(cursor) = window.cursor_position()
        && let Ok(ray) = camera.viewport_to_world(transform, cursor)
        && let Some(p) = space::ground(ray)
        && let Some(pos) = space::position(p, view.origin)
    {
        gizmos.rect(
            Isometry3d::new(
                space::cell_local(pos.containing_cell(), view.origin) + Vec3::Y * 0.04,
                Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
            ),
            Vec2::splat(1.),
            gold,
        );
    }
    let selected = game
        .selected
        .and_then(|id| game.snapshot.characters.iter().find(|c| c.id == id))
        .map_or("No settler selected".into(), |c| {
            format!(
                "{} | food {} | ({}, {})",
                c.name,
                c.satiety,
                c.containing_cell.x(),
                c.containing_cell.y()
            )
        });
    for mut text in &mut hud {
        text.0 = format!(
            "PROGRESSUS / LOW-POLY EXPERIMENT\n{} | tick {} | {}\n{}\nWASD / middle drag: pan | Wheel: zoom | Q/E: rotate\nLeft: select / tool | Right: move | P: pause | F: focus\n{}",
            tool.label(),
            game.snapshot.tick.value(),
            if clock.is_paused() {
                "PAUSED"
            } else {
                "RUNNING"
            },
            selected,
            game.message
        );
    }
}

#[derive(Component, Clone, Copy)]
pub(super) enum Action {
    Tool(Tool),
    Pause,
    Music,
}

#[derive(Component)]
pub(super) struct Toolbar;

pub(super) fn setup_toolbar(commands: &mut Commands, font: &Handle<Font>) {
    commands
        .spawn((
            Toolbar,
            Node {
                position_type: PositionType::Absolute,
                bottom: px(16),
                left: px(20),
                column_gap: px(6),
                padding: UiRect::all(px(8)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.035, 0.05, 0.06, 0.94)),
        ))
        .with_children(|row| {
            for (label, action) in [
                ("1 Select", Action::Tool(Tool::Select)),
                ("2 Harvest", Action::Tool(Tool::Harvest)),
                ("3 Stockpile", Action::Tool(Tool::Stockpile)),
                ("4 Wall", Action::Tool(Tool::Wall)),
                ("5 Door", Action::Tool(Tool::Door)),
                ("6 Bench", Action::Tool(Tool::Workbench)),
                ("7 Craft", Action::Tool(Tool::Craft)),
                ("8 Cancel", Action::Tool(Tool::Cancel)),
                ("Pause", Action::Pause),
                ("Music", Action::Music),
            ] {
                row.spawn((
                    Button,
                    action,
                    Node {
                        padding: UiRect::axes(px(10), px(9)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.16, 0.21, 0.22)),
                ))
                .with_children(|button| {
                    button.spawn((
                        Text::new(label),
                        TextFont {
                            font_size: 15.,
                            font: font.clone(),
                            ..default()
                        },
                        TextColor(Color::srgb(0.93, 0.92, 0.84)),
                    ));
                });
            }
        });
}

pub(super) fn buttons(
    mut buttons: Query<(&Interaction, &Action, &mut BackgroundColor), With<Button>>,
    mut tool: ResMut<Tool>,
    mut clock: ResMut<TickScheduler>,
    mut audio: ResMut<crate::audio::AudioSettings>,
    mut previous: Local<bool>,
) {
    let mut pressed = false;
    for (interaction, action, mut background) in &mut buttons {
        let active = match action {
            Action::Tool(mode) => *mode == *tool,
            Action::Pause => clock.is_paused(),
            Action::Music => audio.music > 0,
        };
        background.0 = if *interaction == Interaction::Hovered || active {
            Color::srgb(0.30, 0.37, 0.32)
        } else {
            Color::srgb(0.16, 0.21, 0.22)
        };
        if *interaction == Interaction::Pressed {
            pressed = true;
            if !*previous {
                match action {
                    Action::Tool(mode) => *tool = *mode,
                    Action::Pause => {
                        clock.toggle_paused();
                    }
                    Action::Music => audio.music = if audio.music == 0 { 25 } else { 0 },
                }
            }
        }
    }
    *previous = pressed;
}
