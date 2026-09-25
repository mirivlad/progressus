//! Disposable render hierarchy for a character. Simulation identity stays on the root.

use super::{
    MotionTarget, Palette, Pawn,
    models::{CartPart, CharacterPart, ModelKind},
    space,
};
use crate::{navigation::VisualMotion, runtime::AuthoritativeClient};
use bevy::prelude::*;
use progressus_app::{
    EntityId, InventoryItemSnapshot, ItemId, ItemLocation, JobKind, JobSnapshot, JobState,
    WorldCell, WorldPosition, item, slot,
};

#[derive(Component)]
pub(crate) struct CharacterRig {
    pub(crate) torso: Entity,
    pub(crate) head: Entity,
    pub(crate) left_arm: Entity,
    pub(crate) right_arm: Entity,
    pub(crate) left_leg: Entity,
    pub(crate) right_leg: Entity,
}

#[derive(Component)]
pub(crate) struct EquippedVisual {
    pub(crate) bearer_id: EntityId,
    wheels: Option<[Entity; 2]>,
    last_position: Option<WorldPosition>,
    wheel_angle: f32,
}

pub(crate) fn spawn_equipped(
    commands: &mut Commands,
    palette: &mut Palette,
    meshes: &mut Assets<Mesh>,
    bearer_root: Entity,
    rig: &CharacterRig,
    bearer_id: EntityId,
    item: (EntityId, ItemId),
) -> Entity {
    let (item_id, kind) = item;
    let variant = item_id.value() as u8 % 2;
    let material = palette.material.clone();
    if kind == item::PRIMITIVE_TOOL {
        let mesh = palette.get(ModelKind::PrimitiveTool, variant, meshes);
        let mut visual = None;
        commands.entity(rig.right_arm).with_children(|parent| {
            visual = Some(
                parent
                    .spawn((
                        Mesh3d(mesh),
                        MeshMaterial3d(material),
                        Transform {
                            translation: Vec3::new(0., -0.30, 0.),
                            rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
                            scale: Vec3::splat(1.2),
                        },
                        EquippedVisual {
                            bearer_id,
                            wheels: None,
                            last_position: None,
                            wheel_angle: 0.,
                        },
                    ))
                    .id(),
            );
        });
        return visual.expect("tool visual child was spawned");
    }
    let body = palette.cart_part(CartPart::Body, variant, meshes);
    let wheel = palette.cart_part(CartPart::Wheel, variant, meshes);
    let mut visual = None;
    commands.entity(bearer_root).with_children(|parent| {
        visual = Some(
            parent
                .spawn((
                    Mesh3d(body),
                    MeshMaterial3d(material.clone()),
                    Transform {
                        translation: Vec3::new(0., 0., -0.55),
                        rotation: Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2),
                        ..default()
                    },
                ))
                .id(),
        );
    });
    let visual = visual.expect("cart visual child was spawned");
    let mut wheels = Vec::with_capacity(2);
    commands.entity(visual).with_children(|parent| {
        for z in [-0.19, 0.19] {
            wheels.push(
                parent
                    .spawn((
                        Mesh3d(wheel.clone()),
                        MeshMaterial3d(material.clone()),
                        Transform::from_xyz(-0.06, 0.16, z),
                    ))
                    .id(),
            );
        }
    });
    commands.entity(visual).insert(EquippedVisual {
        bearer_id,
        wheels: Some([wheels[0], wheels[1]]),
        last_position: None,
        wheel_angle: 0.,
    });
    visual
}

pub(crate) fn equipped_visual(
    row: &InventoryItemSnapshot,
    bearer: EntityId,
) -> Option<(EntityId, ItemId)> {
    match row.location {
        ItemLocation::Equipped {
            character_id,
            slot: worn_slot,
        } if character_id == bearer
            && worn_slot == slot::TOOL
            && matches!(row.kind, item::PRIMITIVE_TOOL | item::CART) =>
        {
            Some((row.id, row.kind))
        }
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PoseKind {
    Idle,
    Walk,
    Work,
    Sleep,
}

pub(crate) struct CharacterPose {
    pub(crate) left_leg: f32,
    pub(crate) right_leg: f32,
    pub(crate) left_arm: f32,
    pub(crate) right_arm: f32,
    pub(crate) torso_bob: f32,
}

pub(crate) fn pose_kind(
    id: EntityId,
    trace: &[WorldPosition],
    elapsed_seconds: f32,
    jobs: &[JobSnapshot],
) -> PoseKind {
    if elapsed_seconds < 0.25 && trace.first() != trace.last() {
        return PoseKind::Walk;
    }
    if jobs.iter().any(|job| {
        matches!(job.state, JobState::Working { worker_id, .. } if worker_id == id)
            && matches!(job.kind, JobKind::Sleep { .. })
    }) {
        return PoseKind::Sleep;
    }
    if jobs.iter().any(|job| {
        matches!(job.state, JobState::Working { worker_id, .. } if worker_id == id)
            && matches!(
                job.kind,
                JobKind::Harvest { .. }
                    | JobKind::ExcavateRock { .. }
                    | JobKind::Craft { .. }
                    | JobKind::Construct { .. }
                    | JobKind::PrepareConstruction { .. }
            )
    }) {
        PoseKind::Work
    } else {
        PoseKind::Idle
    }
}

pub(crate) fn pose(kind: PoseKind, phase_seconds: f32) -> CharacterPose {
    // Five seconds contains exactly eight cycles at 1.6 Hz, so wrapping is seamless.
    let phase = phase_seconds.rem_euclid(5.0);
    let wave = (phase * std::f32::consts::TAU * 1.6).sin();
    match kind {
        PoseKind::Idle => CharacterPose {
            left_leg: 0.,
            right_leg: 0.,
            left_arm: 0.,
            right_arm: 0.,
            torso_bob: wave * 0.008,
        },
        PoseKind::Walk => CharacterPose {
            left_leg: wave * 0.38,
            right_leg: -wave * 0.38,
            left_arm: -wave * 0.25,
            right_arm: wave * 0.25,
            torso_bob: wave.abs() * 0.025,
        },
        PoseKind::Work => CharacterPose {
            left_leg: 0.,
            right_leg: 0.,
            left_arm: -0.25 + wave * 0.3,
            right_arm: -0.25 + wave * 0.3,
            torso_bob: wave * 0.012,
        },
        PoseKind::Sleep => CharacterPose {
            left_leg: -0.95,
            right_leg: -0.95,
            left_arm: -0.55,
            right_arm: -0.55,
            torso_bob: wave * 0.004,
        },
    }
}

pub(crate) fn animate_rigs(
    game: Res<AuthoritativeClient>,
    motion: Res<VisualMotion>,
    mut rigs: Query<(&Pawn, &CharacterRig, &mut Transform)>,
    mut equipped: Query<&mut EquippedVisual>,
    mut parts: Query<&mut Transform, Without<Pawn>>,
) {
    for (pawn, rig, mut root) in &mut rigs {
        let Some(m) = motion.characters.get(&pawn.0) else {
            continue;
        };
        let kind = pose_kind(pawn.0, &m.trace, m.elapsed_seconds, &game.snapshot().jobs);
        let pose = pose(kind, m.phase_seconds);
        let sleeping = kind == PoseKind::Sleep;
        let bed_sleep = game.snapshot().jobs.iter().any(|job| {
            matches!(job.state, JobState::Working { worker_id, .. } if worker_id == pawn.0)
                && matches!(
                    job.kind,
                    JobKind::Sleep {
                        bed_id: Some(_),
                        ..
                    }
                )
        });
        let sleep_height = if bed_sleep { 0.39 } else { 0.13 };
        if sleeping {
            // The bed runs along world X. Keep the presentation rig aligned
            // with it while the authoritative character stays at cell center.
            root.rotation = Quat::IDENTITY;
        }
        if let Ok(mut torso) = parts.get_mut(rig.torso) {
            torso.translation = if sleeping {
                Vec3::new(0., sleep_height + pose.torso_bob, 0.)
            } else {
                Vec3::new(0., 0.34 + pose.torso_bob, 0.)
            };
            torso.rotation = Quat::from_rotation_z(if sleeping {
                -std::f32::consts::FRAC_PI_2
            } else {
                0.
            });
        }
        if let Ok(mut head) = parts.get_mut(rig.head) {
            head.translation = if sleeping {
                Vec3::new(0.16, sleep_height + pose.torso_bob, 0.)
            } else {
                Vec3::new(0., 0.72 + pose.torso_bob, 0.)
            };
            head.rotation = Quat::from_rotation_z(if sleeping {
                -std::f32::consts::FRAC_PI_2
            } else {
                0.
            });
            head.scale = Vec3::splat(if sleeping { 0.75 } else { 1. });
        }
        for (entity, angle, standing, lying) in [
            (
                rig.left_arm,
                pose.left_arm,
                Vec3::new(-0.15, 0.65, 0.),
                Vec3::new(0.02, 0.38, -0.13),
            ),
            (
                rig.right_arm,
                pose.right_arm,
                Vec3::new(0.15, 0.65, 0.),
                Vec3::new(0.02, 0.38, 0.13),
            ),
            (
                rig.left_leg,
                pose.left_leg,
                Vec3::new(-0.085, 0.37, 0.),
                Vec3::new(-0.13, 0.38, -0.06),
            ),
            (
                rig.right_leg,
                pose.right_leg,
                Vec3::new(0.085, 0.37, 0.),
                Vec3::new(-0.13, 0.38, 0.06),
            ),
        ] {
            if let Ok(mut part) = parts.get_mut(entity) {
                part.rotation = if sleeping {
                    Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2)
                } else {
                    Quat::from_rotation_x(angle)
                };
                part.translation = if sleeping {
                    Vec3::new(lying.x, sleep_height, lying.z)
                } else {
                    standing
                };
                part.scale = Vec3::splat(if sleeping { 0.75 } else { 1. });
            }
        }
    }
    for mut visual in &mut equipped {
        let Some(wheels) = visual.wheels else {
            continue;
        };
        let Some(motion) = motion.characters.get(&visual.bearer_id) else {
            continue;
        };
        let point =
            crate::navigation::interpolate_trace(&motion.trace, motion.elapsed_seconds / 0.25);
        if let Some(previous) = visual.last_position {
            let dx = (point.x_subunits() - previous.x_subunits()) as f64;
            let dy = (point.y_subunits() - previous.y_subunits()) as f64;
            let distance = dx.hypot(dy) / progressus_app::SUBUNITS_PER_CELL as f64;
            visual.wheel_angle =
                (visual.wheel_angle + (distance / 0.16) as f32).rem_euclid(std::f32::consts::TAU);
        }
        visual.last_position = Some(point);
        for wheel in wheels {
            if let Ok(mut transform) = parts.get_mut(wheel) {
                transform.rotation = Quat::from_rotation_z(visual.wheel_angle);
            }
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use progressus_app::{
        InventoryItemSnapshot, ItemLocation, JobKind, JobSnapshot, JobState, item, slot,
    };

    #[test]
    fn equipped_visual_uses_physical_tool_slot_and_bearer() {
        let bearer = EntityId::new(3).unwrap();
        let other = EntityId::new(4).unwrap();
        let point = WorldPosition::from_subunits(0, 0).unwrap();
        let row = |id, kind, location| InventoryItemSnapshot {
            id,
            kind,
            quantity: 1,
            location,
            reserved: false,
            load: 1,
            contained_load: None,
            capacity: None,
        };
        let tool = EntityId::new(10).unwrap();
        let cart = EntityId::new(11).unwrap();
        let items = [
            row(
                EntityId::new(5).unwrap(),
                item::PRIMITIVE_TOOL,
                ItemLocation::Ground { position: point },
            ),
            row(
                EntityId::new(6).unwrap(),
                item::PRIMITIVE_TOOL,
                ItemLocation::Carried {
                    character_id: bearer,
                },
            ),
            row(
                EntityId::new(7).unwrap(),
                item::PRIMITIVE_TOOL,
                ItemLocation::Contained { container_id: cart },
            ),
            row(
                EntityId::new(8).unwrap(),
                item::PRIMITIVE_TOOL,
                ItemLocation::Equipped {
                    character_id: other,
                    slot: slot::TOOL,
                },
            ),
            row(
                tool,
                item::PRIMITIVE_TOOL,
                ItemLocation::Equipped {
                    character_id: bearer,
                    slot: slot::TOOL,
                },
            ),
        ];
        assert_eq!(
            items.iter().find_map(|row| equipped_visual(row, bearer)),
            Some((tool, item::PRIMITIVE_TOOL))
        );
        assert_eq!(
            items[..4]
                .iter()
                .find_map(|row| equipped_visual(row, bearer)),
            None
        );
        let cart_row = [row(
            cart,
            item::CART,
            ItemLocation::Equipped {
                character_id: bearer,
                slot: slot::TOOL,
            },
        )];
        assert_eq!(
            cart_row.iter().find_map(|row| equipped_visual(row, bearer)),
            Some((cart, item::CART))
        );
    }

    #[test]
    fn pose_kind_prioritizes_movement_then_active_work() {
        let id = EntityId::new(3).unwrap();
        let first = WorldPosition::from_subunits(0, 0).unwrap();
        let last = WorldPosition::from_subunits(100, 0).unwrap();
        let mut work = [JobSnapshot {
            id: EntityId::new(9).unwrap(),
            kind: JobKind::Harvest {
                source: WorldCell::new(0, 0),
            },
            state: JobState::Working {
                worker_id: id,
                remaining_ticks: 2,
            },
        }];
        assert_eq!(pose_kind(id, &[first, last], 0.1, &work), PoseKind::Walk);
        assert_eq!(pose_kind(id, &[first, last], 0.3, &work), PoseKind::Work);
        assert_eq!(pose_kind(id, &[last], 0., &work), PoseKind::Work);
        work[0].kind = JobKind::Construct {
            site_id: EntityId::new(12).unwrap(),
        };
        assert_eq!(pose_kind(id, &[last], 0., &work), PoseKind::Work);
        work[0].kind = JobKind::EquipTool {
            item_id: EntityId::new(15).unwrap(),
            requested_worker_id: Some(id),
        };
        assert_eq!(pose_kind(id, &[last], 0., &work), PoseKind::Idle);
        work[0].kind = JobKind::Sleep {
            character_id: id,
            bed_id: None,
        };
        assert_eq!(pose_kind(id, &[last], 0., &work), PoseKind::Sleep);
        work[0].kind = JobKind::Harvest {
            source: WorldCell::new(0, 0),
        };
        work[0].state = JobState::Available;
        assert_eq!(pose_kind(id, &[last], 0., &work), PoseKind::Idle);
        assert_eq!(pose_kind(id, &[last], 0., &[]), PoseKind::Idle);
    }

    #[test]
    fn character_pose_moves_opposing_limbs_with_bounded_angles() {
        let walk = pose(PoseKind::Walk, 0.15625);
        assert!(walk.left_leg * walk.right_leg < 0.);
        assert!(walk.left_arm * walk.right_arm < 0.);
        assert!(walk.left_leg.abs() <= 0.5);
        let work = pose(PoseKind::Work, 0.15625);
        assert_eq!(work.left_leg, 0.);
        assert_eq!(work.right_leg, 0.);
        assert_ne!(work.right_arm, 0.);
        let idle = pose(PoseKind::Idle, 0.15625);
        assert_eq!(idle.left_leg, 0.);
        assert_eq!(idle.right_leg, 0.);
        assert!((pose(PoseKind::Walk, 5.15625).left_leg - walk.left_leg).abs() < 0.0001);
        for value in [
            walk.left_leg,
            walk.right_leg,
            walk.left_arm,
            walk.right_arm,
            work.left_arm,
            work.right_arm,
            idle.torso_bob,
        ] {
            assert!(value.is_finite());
            assert!(value.abs() <= 0.5);
        }
    }
}
