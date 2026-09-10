use super::*;
use bevy::{
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll},
    window::PrimaryWindow,
};
#[allow(clippy::too_many_arguments)]
pub(crate) fn camera_controls(
    keys: Res<ButtonInput<KeyCode>>,
    modal: Res<crate::modal::ModalState>,
    tool: Res<crate::ui::ToolState>,
    selected: Res<crate::navigation::SelectedCharacter>,
    authoritative: Res<crate::runtime::AuthoritativeClient>,
    buttons: Res<ButtonInput<MouseButton>>,
    mouse: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    time: Res<Time>,
    mut view: ResMut<View>,
    mut cameras: Query<(&mut Transform, &mut Projection), With<Camera3d>>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    if modal.is_open() {
        return;
    }
    if keys.just_pressed(KeyCode::KeyF)
        && let Some(id) = selected.0
        && let Some(character) = authoritative
            .snapshot()
            .characters
            .iter()
            .find(|c| c.id == id)
    {
        view.reset(character.containing_cell);
        view.focus = space::local(character.position, view.origin);
    }
    let Ok(window) = windows.single() else {
        return;
    };
    if !tool.pointer_over_ui {
        view.height = (view.height * (1. - scroll.delta.y * 0.08).clamp(0.5, 2.)).clamp(6., 72.);
    }
    if keys.pressed(KeyCode::KeyQ) {
        view.yaw += time.delta_secs();
    }
    if keys.pressed(KeyCode::KeyE) {
        view.yaw -= time.delta_secs();
    }
    let right = Vec3::new(view.yaw.cos(), 0., -view.yaw.sin());
    let up = Vec3::new(-view.yaw.sin(), 0., -view.yaw.cos());
    let horizontal =
        f32::from(keys.pressed(KeyCode::KeyD)) - f32::from(keys.pressed(KeyCode::KeyA));
    let vertical = f32::from(keys.pressed(KeyCode::KeyW)) - f32::from(keys.pressed(KeyCode::KeyS));
    let speed = view.height * 0.65 * time.delta_secs();
    view.focus += (right * horizontal + up * vertical) * speed;
    if buttons.pressed(MouseButton::Middle) && !tool.pointer_over_ui {
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
            view.rebase(WorldCell::new(x, y));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        let mut app = App::new();
        app.init_resource::<View>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<AccumulatedMouseMotion>()
            .init_resource::<AccumulatedMouseScroll>()
            .init_resource::<Time>()
            .init_resource::<crate::modal::ModalState>()
            .init_resource::<crate::ui::ToolState>()
            .init_resource::<crate::navigation::SelectedCharacter>()
            .insert_resource(crate::runtime::AuthoritativeClient::new().unwrap())
            .add_systems(Update, camera_controls);
        app.world_mut().spawn((Window::default(), PrimaryWindow));
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_millis(100));
        app
    }

    #[test]
    fn arrows_remain_authoritative_movement_and_wasd_pans_only_the_view() {
        let mut app = app();
        let before = app
            .world()
            .resource::<crate::runtime::AuthoritativeClient>()
            .save_json()
            .unwrap();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ArrowRight);
        app.update();
        assert_eq!(app.world().resource::<View>().focus, Vec3::ZERO);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyD);
        app.update();
        assert_ne!(app.world().resource::<View>().focus, Vec3::ZERO);
        assert_eq!(
            before,
            app.world()
                .resource::<crate::runtime::AuthoritativeClient>()
                .save_json()
                .unwrap()
        );
    }

    #[test]
    fn modal_blocks_camera_and_ui_capture_blocks_pointer_pan_and_zoom() {
        let mut app = app();
        app.world_mut()
            .resource_mut::<crate::modal::ModalState>()
            .open_saves();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyD);
        app.update();
        assert_eq!(app.world().resource::<View>().focus, Vec3::ZERO);
        app.world_mut()
            .resource_mut::<crate::modal::ModalState>()
            .close();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::KeyD);
        app.world_mut()
            .resource_mut::<crate::ui::ToolState>()
            .pointer_over_ui = true;
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Middle);
        app.world_mut()
            .resource_mut::<AccumulatedMouseMotion>()
            .delta = Vec2::splat(40.);
        app.world_mut()
            .resource_mut::<AccumulatedMouseScroll>()
            .delta = Vec2::Y;
        let height = app.world().resource::<View>().height;
        app.update();
        assert_eq!(app.world().resource::<View>().height, height);
        assert_eq!(app.world().resource::<View>().focus, Vec3::ZERO);
    }

    #[test]
    fn rebase_preserves_absolute_focus() {
        let mut app = app();
        app.world_mut().resource_mut::<View>().focus = Vec3::new(100.25, 0., -90.5);
        let before = space::position(
            app.world().resource::<View>().focus,
            app.world().resource::<View>().origin,
        );
        app.update();
        let view = app.world().resource::<View>();
        assert_eq!(space::position(view.focus, view.origin), before);
        assert!(view.focus.x.abs() < 1. && view.focus.z.abs() < 1.);
        assert!(view.rebased);
    }
}
