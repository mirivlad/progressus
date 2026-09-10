//! Opt-in presentation experiment. No authoritative height or renderer migration.
mod controls;
#[path = "../../../../assets/procedural/low_poly/models.rs"]
mod models;
mod scene;
mod space;
#[path = "../../../../assets/procedural/low_poly/terrain.rs"]
mod terrain;

use crate::{
    interaction::TickScheduler,
    navigation::{VisualMotion, interpolate_trace},
};
use bevy::{camera::ScalingMode, prelude::*};
use models::ModelKind;
use progressus_app::{
    Application, ClientSnapshot, Command, EntityId, NewGameOptions, SnapshotQuery, WorldCell,
    WorldSeed,
};
use std::{collections::BTreeMap, error::Error};

#[derive(Resource)]
struct Game {
    app: Application,
    snapshot: ClientSnapshot,
    selected: Option<EntityId>,
    dirty: bool,
    message: String,
}
impl Game {
    fn refresh(&mut self) {
        match self.app.snapshot(SnapshotQuery {
            navigation_for: self.selected,
            ..default()
        }) {
            Ok(snapshot) => {
                self.snapshot = snapshot;
                self.dirty = true;
            }
            Err(error) => self.message = error.to_string(),
        }
    }
    fn execute(&mut self, command: Command) {
        match self.app.execute(command) {
            Ok(()) => {
                self.message = "Order accepted".into();
                self.refresh();
            }
            Err(error) => self.message = error.to_string(),
        }
    }
}
#[derive(Resource)]
struct View {
    origin: WorldCell,
    focus: Vec3,
    height: f32,
    yaw: f32,
    rebased: bool,
}
impl View {
    fn camera_transform(&self) -> Transform {
        let offset = Vec3::new(self.yaw.sin(), 1.1, self.yaw.cos()) * 40.;
        Transform::from_translation(self.focus + offset).looking_at(self.focus, Vec3::Y)
    }
}
#[derive(Component)]
struct Pawn(EntityId);
#[derive(Component)]
struct MotionTarget(EntityId, Vec3);
#[derive(Component)]
struct Hud;
#[derive(Resource)]
struct Palette {
    models: BTreeMap<(ModelKind, u8), Handle<Mesh>>,
    material: Handle<StandardMaterial>,
}
impl Palette {
    fn get(&mut self, kind: ModelKind, variant: u8, meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
        self.models
            .entry((kind, variant % 4))
            .or_insert_with(|| meshes.add(models::model_mesh(kind, variant % 4)))
            .clone()
    }
}

pub fn run(arguments: impl IntoIterator<Item = String>) -> Result<(), Box<dyn Error>> {
    let mut seed = 0;
    let mut save = None;
    let mut diagnostics = false;
    let mut args = arguments.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--seed" => seed=args.next().ok_or("--seed requires a number")?.parse()?,
            "--load" => save=Some(args.next().ok_or("--load requires a JSON path")?),
            "--diagnostics" => diagnostics=true,
            _ => return Err(format!("unknown argument {arg}; usage: progressus-low-poly [--seed N] [--load PATH] [--diagnostics]").into()),
        }
    }
    let application = if let Some(path) = save {
        Application::from_save_json(&std::fs::read(path)?)?
    } else {
        Application::new_game(NewGameOptions {
            seed: WorldSeed::new(seed),
        })?
    };
    let snapshot = application.snapshot(SnapshotQuery::default())?;
    let origin = snapshot
        .characters
        .first()
        .map_or(WorldCell::new(0, 0), |c| c.containing_cell);
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Progressus — Low-poly experiment".into(),
            resolution: (1440, 900).into(),
            ..default()
        }),
        ..default()
    }))
    .insert_resource(ClearColor(Color::srgb(0.075, 0.105, 0.12)))
    .insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.78, 0.85, 1.),
        brightness: 350.,
        ..default()
    })
    .insert_resource(Game {
        app: application,
        snapshot,
        selected: None,
        dirty: true,
        message: "Select a settler, then right-click to move".into(),
    })
    .insert_resource(View {
        origin,
        focus: Vec3::ZERO,
        height: 18.,
        yaw: std::f32::consts::FRAC_PI_4,
        rebased: true,
    })
    .init_resource::<crate::audio::AudioSettings>()
    .add_plugins(crate::continuous_music::ContinuousMusicPlugin)
    .init_resource::<TickScheduler>()
    .init_resource::<VisualMotion>()
    .init_resource::<scene::SceneCache>()
    .init_resource::<controls::Tool>()
    .add_systems(Startup, (crate::ui_font::setup_ui_font, setup).chain())
    .add_systems(
        Update,
        (
            controls::camera_controls,
            controls::buttons,
            controls::input,
            advance,
            scene::sync,
            animate,
            controls::overlay,
        )
            .chain(),
    );
    if diagnostics {
        app.add_plugins(bevy::diagnostic::FrameTimeDiagnosticsPlugin::default())
            .add_plugins(bevy::diagnostic::LogDiagnosticsPlugin::default())
            .add_plugins(bevy::diagnostic::EntityCountDiagnosticsPlugin::default());
    }
    app.run();
    Ok(())
}

fn setup(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    view: Res<View>,
    font: Res<crate::ui_font::UiFont>,
) {
    commands.spawn((
        Camera3d::default(),
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: view.height,
            },
            near: 0.1,
            far: 250.,
            ..OrthographicProjection::default_3d()
        }),
        view.camera_transform(),
        Msaa::Sample4,
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 9000.,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(-20., 40., 20.).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.insert_resource(Palette {
        models: BTreeMap::new(),
        material: materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.95,
            ..default()
        }),
    });
    controls::setup_toolbar(&mut commands, &font.0);
    commands.spawn((
        Text::new(""),
        TextFont {
            font_size: 15.,
            font: font.0.clone(),
            ..default()
        },
        TextColor(Color::srgb(0.9, 0.92, 0.86)),
        Node {
            position_type: PositionType::Absolute,
            left: px(20),
            top: px(18),
            padding: UiRect::all(px(14)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.035, 0.05, 0.06, 0.92)),
        Hud,
    ));
}
fn advance(time: Res<Time>, mut clock: ResMut<TickScheduler>, mut game: ResMut<Game>) {
    if clock.advance(time.delta()) {
        if let Err(error) = game.app.execute(Command::AdvanceTicks { count: 1 }) {
            game.message = error.to_string();
            clock.set_paused(true);
        }
        game.refresh();
    }
}
fn animate(
    time: Res<Time>,
    view: Res<View>,
    mut motion: ResMut<VisualMotion>,
    mut pawns: Query<(&MotionTarget, &mut Transform)>,
) {
    for m in motion.characters.values_mut() {
        m.elapsed_seconds += time.delta_secs();
    }
    for (pawn, mut transform) in &mut pawns {
        let Some(m) = motion.characters.get(&pawn.0) else {
            continue;
        };
        let point = interpolate_trace(&m.trace, m.elapsed_seconds / 0.25);
        transform.translation = space::local(point, view.origin) + pawn.1;
        if let (Some(first), Some(last)) = (m.trace.first(), m.trace.last()) {
            let delta = space::local(*last, view.origin) - space::local(*first, view.origin);
            if delta.length_squared() > 0.00001 {
                transform.rotation = Quat::from_rotation_y(delta.x.atan2(delta.z));
            }
        }
    }
}
