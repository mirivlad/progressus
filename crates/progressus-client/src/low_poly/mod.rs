//! Primary low-poly presentation; authoritative gameplay remains in progressus-app.
pub(crate) mod controls;
#[path = "../../../../assets/procedural/low_poly/models.rs"]
mod models;
pub(crate) mod scene;
pub(crate) mod space;
#[path = "../../../../assets/procedural/low_poly/terrain.rs"]
mod terrain;
use crate::navigation::{VisualMotion, interpolate_trace};
use bevy::{camera::ScalingMode, prelude::*};
use models::ModelKind;
use progressus_app::{EntityId, WorldCell};
use std::collections::BTreeMap;
#[derive(Resource)]
pub(crate) struct View {
    pub(crate) origin: WorldCell,
    pub(crate) previous_origin: WorldCell,
    pub(crate) focus: Vec3,
    pub(crate) height: f32,
    pub(crate) yaw: f32,
    pub(crate) rebased: bool,
}
impl View {
    pub(crate) fn camera_transform(&self) -> Transform {
        let offset = Vec3::new(self.yaw.sin(), 1.1, self.yaw.cos()) * 40.;
        Transform::from_translation(self.focus + offset).looking_at(self.focus, Vec3::Y)
    }
}
#[derive(Component)]
pub(crate) struct Pawn(pub(crate) EntityId);
#[derive(Component)]
pub(crate) struct MotionTarget(pub(crate) EntityId, pub(crate) Vec3);
#[derive(Resource)]
pub(crate) struct Palette {
    pub(crate) models: BTreeMap<(ModelKind, u8), Handle<Mesh>>,
    pub(crate) material: Handle<StandardMaterial>,
}
impl Palette {
    pub(crate) fn get(
        &mut self,
        kind: ModelKind,
        variant: u8,
        meshes: &mut Assets<Mesh>,
    ) -> Handle<Mesh> {
        let structure = matches!(
            kind,
            ModelKind::Wall
                | ModelKind::Door
                | ModelKind::OpenDoor
                | ModelKind::ConstructionWall
                | ModelKind::ConstructionDoor
        );
        let variant = variant % kind.variant_count();
        self.models
            .entry((kind, variant))
            .or_insert_with(|| {
                meshes.add(if structure {
                    models::structure_mesh(kind, variant)
                } else {
                    models::model_mesh(kind, variant)
                })
            })
            .clone()
    }
}

impl Default for View {
    fn default() -> Self {
        Self {
            origin: WorldCell::new(0, 0),
            previous_origin: WorldCell::new(0, 0),
            focus: Vec3::ZERO,
            height: 18.,
            yaw: std::f32::consts::FRAC_PI_4,
            rebased: true,
        }
    }
}
impl View {
    pub(crate) fn rebase(&mut self, origin: WorldCell) {
        if !self.rebased {
            self.previous_origin = self.origin;
        }
        self.origin = origin;
        self.rebased = true;
    }
    pub(crate) fn reset(&mut self, origin: WorldCell) {
        self.rebase(origin);
        self.focus = Vec3::ZERO;
        self.rebased = true;
    }
}
pub(crate) fn setup(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    view: Res<View>,
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
}
pub(crate) fn animate(
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

/// Mesh vertex colors are linear; authored palette values are screen sRGB.
fn linear_color(rgba: [f32; 4]) -> [f32; 4] {
    let c = Color::srgba(rgba[0], rgba[1], rgba[2], rgba[3]).to_linear();
    [c.red, c.green, c.blue, c.alpha]
}
