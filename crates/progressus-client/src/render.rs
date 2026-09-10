//! Presentation of the authoritative 2D world in a disposable 3D scene.
pub(crate) use crate::low_poly::controls::camera_controls;
pub(crate) use crate::low_poly::scene::{
    SceneCache as PresentationCache, sync as sync_presentation,
};
pub(crate) use crate::low_poly::{animate as interpolate_character_visuals, setup as setup_camera};
use crate::{
    low_poly::{Pawn, View, space},
    navigation::SelectedCharacter,
    runtime::AuthoritativeClient,
    ui::{SelectedStockpile, ToolMode, ToolState, ZoneVisibility},
};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use progressus_app::{EntityId, JobKind, JobState, WorldCell, WorldPosition};

#[derive(Resource, Default)]
pub(crate) struct NavigationDebug(pub(crate) bool);
fn plane(p: Vec3) -> Isometry3d {
    Isometry3d::new(
        p + Vec3::Y * 0.045,
        Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
    )
}
fn cell_rect(gizmos: &mut Gizmos, cell: WorldCell, origin: WorldCell, color: Color) {
    gizmos.rect(
        plane(space::cell_local(cell, origin)),
        Vec2::splat(0.98),
        color,
    );
}

pub(crate) fn draw_selected_character(
    selected: Res<SelectedCharacter>,
    pawns: Query<(&Pawn, &Transform)>,
    mut gizmos: Gizmos,
) {
    if let Some((_, t)) = pawns.iter().find(|(p, _)| Some(p.0) == selected.0) {
        gizmos.circle(plane(t.translation), 0.45, Color::srgb(0.2, 1., 0.95));
    }
}
pub(crate) fn draw_selected_navigation(
    selected: Res<SelectedCharacter>,
    game: Res<AuthoritativeClient>,
    view: Res<View>,
    pawns: Query<(&Pawn, &Transform)>,
    mut gizmos: Gizmos,
) {
    let Some(nav) = &game.snapshot().navigation else {
        return;
    };
    if selected.0 != Some(nav.character_id) {
        return;
    }
    let Some((_, t)) = pawns.iter().find(|(p, _)| p.0 == nav.character_id) else {
        return;
    };
    let color = Color::srgb(0.2, 1., 0.95);
    let mut previous = t.translation + Vec3::Y * 0.055;
    for &point in &nav.remaining_waypoints {
        let next = space::local(point, view.origin) + Vec3::Y * 0.055;
        gizmos.line(previous, next, color);
        previous = next;
    }
    if let Some(destination) = nav.destination {
        gizmos.circle(plane(space::local(destination, view.origin)), 0.24, color);
    }
}

pub(crate) fn draw_tool_drag(
    tool: Res<ToolState>,
    view: Res<View>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut gizmos: Gizmos,
) {
    if tool.mode == ToolMode::Select || tool.pointer_over_ui {
        return;
    }
    let current = tool.drag_current.or_else(|| {
        let (window, (camera, transform)) = (windows.single().ok()?, cameras.single().ok()?);
        let point = space::ground(
            camera
                .viewport_to_world(transform, window.cursor_position()?)
                .ok()?,
        )?;
        Some(space::position(point, view.origin)?.containing_cell())
    });
    let Some(last) = current else {
        return;
    };
    let first = tool.drag_start.unwrap_or(last);
    let width = (i128::from(first.x()) - i128::from(last.x())).abs() + 1;
    let height = (i128::from(first.y()) - i128::from(last.y())).abs() + 1;
    if width * height > 4096 {
        return;
    }
    for y in first.y().min(last.y())..=first.y().max(last.y()) {
        for x in first.x().min(last.x())..=first.x().max(last.x()) {
            cell_rect(
                &mut gizmos,
                WorldCell::new(x, y),
                view.origin,
                Color::srgb(1., 0.78, 0.2),
            );
        }
    }
}

pub(crate) fn draw_stockpiles(
    game: Res<AuthoritativeClient>,
    view: Res<View>,
    cache: Res<PresentationCache>,
    zones: Res<ZoneVisibility>,
    selected: Res<SelectedStockpile>,
    mut gizmos: Gizmos,
) {
    if !zones.visible {
        return;
    }
    for stockpile in &game.snapshot().stockpiles {
        let color = if selected.0 == Some(stockpile.id) {
            Color::srgb(0.4, 1., 0.5)
        } else {
            Color::srgb(0.15, 0.65, 0.8)
        };
        for &cell in &stockpile.cells {
            if cache.terrain.contains_key(&cell.split().0) {
                cell_rect(&mut gizmos, cell, view.origin, color);
            }
        }
        if selected.0 == Some(stockpile.id) {
            let cells: std::collections::BTreeSet<_> = stockpile.cells.iter().copied().collect();
            for &cell in &stockpile.cells {
                if !cache.terrain.contains_key(&cell.split().0) {
                    continue;
                }
                let p = space::cell_local(cell, view.origin) + Vec3::Y * 0.065;
                for (dx, dy, a, b) in [
                    (-1, 0, Vec3::new(-0.5, 0., -0.5), Vec3::new(-0.5, 0., 0.5)),
                    (1, 0, Vec3::new(0.5, 0., -0.5), Vec3::new(0.5, 0., 0.5)),
                    (0, 1, Vec3::new(-0.5, 0., -0.5), Vec3::new(0.5, 0., -0.5)),
                    (0, -1, Vec3::new(-0.5, 0., 0.5), Vec3::new(0.5, 0., 0.5)),
                ] {
                    let neighbor = cell
                        .x()
                        .checked_add(dx)
                        .zip(cell.y().checked_add(dy))
                        .map(|(x, y)| WorldCell::new(x, y));
                    if neighbor.is_none_or(|c| !cells.contains(&c)) {
                        gizmos.line(p + a, p + b, Color::WHITE);
                    }
                }
            }
        }
    }
    for logistics in &game.snapshot().production_logistics {
        for (&cell, color) in logistics
            .input_cells
            .iter()
            .map(|c| (c, Color::srgb(1., 0.25, 0.12)))
            .chain(
                logistics
                    .output_cells
                    .iter()
                    .map(|c| (c, Color::srgb(1., 0.85, 0.15))),
            )
        {
            if cache.terrain.contains_key(&cell.split().0) {
                cell_rect(&mut gizmos, cell, view.origin, color);
            }
        }
    }
}

pub(crate) fn draw_job_designations(
    game: Res<AuthoritativeClient>,
    view: Res<View>,
    cache: Res<PresentationCache>,
    mut gizmos: Gizmos,
) {
    let snapshot = game.snapshot();
    for job in &snapshot.jobs {
        let cell = match job.kind {
            JobKind::Harvest { source } => Some(source),
            JobKind::Eat { item_id, .. } => cache
                .items
                .iter()
                .find(|i| i.id == item_id)
                .map(|i| i.position.containing_cell()),
            JobKind::Craft { workstation_id, .. } => snapshot
                .workstations
                .iter()
                .find(|w| w.id == workstation_id)
                .map(|w| w.cell),
            JobKind::DeliverConstruction { site_id, .. } | JobKind::Construct { site_id } => {
                snapshot
                    .construction_sites
                    .iter()
                    .find(|s| s.id == site_id)
                    .map(|s| s.cell)
            }
            JobKind::Haul { .. } | JobKind::SupplyProduction { .. } => None,
        };
        let Some(cell) = cell else {
            continue;
        };
        if !cache.terrain.contains_key(&cell.split().0) {
            continue;
        }
        let color = match job.state {
            JobState::Available => Color::srgb(1., 0.75, 0.2),
            JobState::Working { .. } => Color::srgb(1., 0.35, 0.1),
            _ => Color::srgb(0.3, 0.85, 1.),
        };
        gizmos.circle(plane(space::cell_local(cell, view.origin)), 0.4, color);
    }
}

pub(crate) fn draw_navigation_debug(
    keys: Res<ButtonInput<KeyCode>>,
    mut debug: ResMut<NavigationDebug>,
    game: Res<AuthoritativeClient>,
    view: Res<View>,
    selected: Res<SelectedCharacter>,
    mut gizmos: Gizmos,
) {
    if keys.just_pressed(KeyCode::F3) {
        debug.0 = !debug.0;
    }
    if !debug.0 {
        return;
    }
    for coord in &game.snapshot().resident_chunks {
        if let Some(cell) = coord.world_cell(progressus_app::LocalCell::new(0, 0)) {
            let center = space::cell_local(cell, view.origin) + Vec3::new(15.5, 0., -15.5);
            gizmos.rect(plane(center), Vec2::splat(32.), Color::srgb(0.4, 0.8, 1.));
        }
    }
    if let Some(c) = game
        .snapshot()
        .characters
        .iter()
        .find(|c| Some(c.id) == selected.0)
    {
        let p = space::local(c.position, view.origin) + Vec3::Y * 0.06;
        gizmos.line(p - Vec3::X * 0.3, p + Vec3::X * 0.3, Color::WHITE);
        gizmos.line(p - Vec3::Z * 0.3, p + Vec3::Z * 0.3, Color::WHITE);
    }
}

#[derive(Component)]
pub(crate) struct StackLabel(EntityId);
/// Labels are projected into screen space to remain legible as the camera turns.
#[allow(clippy::too_many_arguments)]
pub(crate) fn sync_stack_labels(
    mut commands: Commands,
    cache: Res<PresentationCache>,
    game: Res<AuthoritativeClient>,
    view: Res<View>,
    font: Res<crate::ui_font::UiFont>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut labels: Query<(Entity, &StackLabel, &mut Text, &mut Node, &mut Visibility)>,
) {
    let (Ok((camera, transform)), Ok(window)) = (cameras.single(), windows.single()) else {
        return;
    };
    let values: std::collections::BTreeMap<_, _> = cache
        .items
        .iter()
        .map(|i| (i.id, (i.quantity, i.position, 0.38)))
        .chain(game.snapshot().carried_items.iter().filter_map(|i| {
            game.snapshot()
                .characters
                .iter()
                .find(|c| c.id == i.character_id)
                .map(|c| (i.id, (i.quantity, c.position, 0.9)))
        }))
        .collect();
    let mut existing = std::collections::BTreeSet::new();
    for (entity, label, mut text, mut node, mut visibility) in &mut labels {
        let Some(&(quantity, position, height)) = values.get(&label.0) else {
            commands.entity(entity).despawn();
            continue;
        };
        existing.insert(label.0);
        text.0 = quantity.to_string();
        project_label(
            &mut node,
            &mut visibility,
            camera,
            transform,
            window,
            position,
            height,
            view.origin,
        );
    }
    for (id, (quantity, position, height)) in values {
        if existing.contains(&id) {
            continue;
        }
        let mut node = Node {
            position_type: PositionType::Absolute,
            padding: UiRect::axes(px(3), px(1)),
            ..default()
        };
        let mut visibility = Visibility::Hidden;
        project_label(
            &mut node,
            &mut visibility,
            camera,
            transform,
            window,
            position,
            height,
            view.origin,
        );
        commands.spawn((
            StackLabel(id),
            GlobalZIndex(-1),
            Text::new(quantity.to_string()),
            TextFont {
                font: font.0.clone(),
                font_size: 14.,
                ..default()
            },
            TextColor(Color::WHITE),
            BackgroundColor(Color::srgba(0.025, 0.035, 0.04, 0.8)),
            node,
            visibility,
        ));
    }
}
#[allow(clippy::too_many_arguments)]
fn project_label(
    node: &mut Node,
    visibility: &mut Visibility,
    camera: &Camera,
    transform: &GlobalTransform,
    window: &Window,
    position: WorldPosition,
    height: f32,
    origin: WorldCell,
) {
    if let Ok(p) =
        camera.world_to_viewport(transform, space::local(position, origin) + Vec3::Y * height)
        && p.x >= 0.
        && p.x < window.width()
        && p.y >= 0.
        && p.y < window.height()
    {
        node.left = px(p.x + 6.);
        node.top = px(p.y - 8.);
        *visibility = Visibility::Inherited;
    } else {
        *visibility = Visibility::Hidden;
    }
}
