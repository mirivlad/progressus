//! Disposable render hierarchy for a character. Simulation identity stays on the root.

use super::{MotionTarget, Palette, Pawn, models::CharacterPart, space};
use bevy::prelude::*;
use progressus_app::{EntityId, WorldCell, WorldPosition};

#[derive(Component)]
pub(crate) struct CharacterRig {
    pub(crate) torso: Entity,
    pub(crate) head: Entity,
    pub(crate) left_arm: Entity,
    pub(crate) right_arm: Entity,
    pub(crate) left_leg: Entity,
    pub(crate) right_leg: Entity,
}

pub(crate) fn spawn(
    commands: &mut Commands,
    palette: &mut Palette,
    meshes: &mut Assets<Mesh>,
    id: EntityId,
    position: WorldPosition,
    origin: WorldCell,
) -> Entity {
    let variant = id.value() as u8 % 4;
    let material = palette.material.clone();
    let torso = palette.character_part(CharacterPart::Torso, variant, meshes);
    let head = palette.character_part(CharacterPart::Head, variant, meshes);
    let arm = palette.character_part(CharacterPart::Arm, variant, meshes);
    let leg = palette.character_part(CharacterPart::Leg, variant, meshes);
    let root = commands
        .spawn((
            Pawn(id),
            MotionTarget(id, Vec3::ZERO),
            Transform::from_translation(space::local(position, origin)),
            Visibility::Inherited,
        ))
        .id();
    let mut parts = Vec::with_capacity(6);
    commands.entity(root).with_children(|parent| {
        for (mesh, translation) in [
            (torso, Vec3::new(0., 0.34, 0.)),
            (head, Vec3::new(0., 0.72, 0.)),
            (arm.clone(), Vec3::new(-0.15, 0.65, 0.)),
            (arm, Vec3::new(0.15, 0.65, 0.)),
            (leg.clone(), Vec3::new(-0.085, 0.37, 0.)),
            (leg, Vec3::new(0.085, 0.37, 0.)),
        ] {
            parts.push(
                parent
                    .spawn((
                        Mesh3d(mesh),
                        MeshMaterial3d(material.clone()),
                        Transform::from_translation(translation),
                    ))
                    .id(),
            );
        }
    });
    commands.entity(root).insert(CharacterRig {
        torso: parts[0],
        head: parts[1],
        left_arm: parts[2],
        right_arm: parts[3],
        left_leg: parts[4],
        right_leg: parts[5],
    });
    root
}
