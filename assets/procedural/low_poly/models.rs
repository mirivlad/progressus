//! Deterministic, presentation-only low-poly models for the primary 3D client.

use bevy::{
    asset::RenderAssetUsages,
    mesh::PrimitiveTopology,
    prelude::{Mesh, Vec3},
};
use std::f32::consts::TAU;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ModelKind {
    Tree,
    StoneOutcrop,
    BerryBush,
    Character,
    Workbench,
    Wall,
    Door,
    OpenDoor,
    ConstructionWall,
    ConstructionDoor,
    Wood,
    Stone,
    PrimitiveTool,
    Berries,
    CopperVein,
    CopperOre,
    Cart,
    /// Stands in for content this build has no authored model for, so a new
    /// definition is visible in the world instead of invisible. See ADR-0021.
    Placeholder,
}

impl ModelKind {
    /// Every kind, so tests cover a new model without being edited. Nothing in
    /// the running client needs to enumerate kinds.
    #[cfg(test)]
    pub const ALL: [Self; 18] = [
        Self::Tree,
        Self::StoneOutcrop,
        Self::BerryBush,
        Self::Character,
        Self::Workbench,
        Self::Wall,
        Self::Door,
        Self::OpenDoor,
        Self::ConstructionWall,
        Self::ConstructionDoor,
        Self::Wood,
        Self::Stone,
        Self::PrimitiveTool,
        Self::Berries,
        Self::CopperVein,
        Self::CopperOre,
        Self::Placeholder,
        Self::Cart,
    ];

    /// How many distinct shapes this kind has. Bounded per ADR-0005: the mesh
    /// cache holds one entry per kind and variant, never one per world cell.
    /// Numerous, closely spaced things carry more shapes than rare ones.
    pub const fn variant_count(self) -> u8 {
        match self {
            Self::Tree => 6,
            Self::BerryBush | Self::StoneOutcrop | Self::CopperVein => 5,
            Self::Character => 4,
            Self::Wood | Self::Stone | Self::Berries | Self::CopperOre => 3,
            Self::PrimitiveTool | Self::Workbench | Self::Placeholder | Self::Cart => 2,
            // Structure variants are connectivity masks, not shapes.
            Self::Wall
            | Self::Door
            | Self::OpenDoor
            | Self::ConstructionWall
            | Self::ConstructionDoor => 16,
        }
    }

    /// Whether a placed instance may be turned and resized for variety.
    /// Built things may not: their orientation carries meaning.
    pub const fn accepts_pose_variety(self) -> bool {
        matches!(
            self,
            Self::Tree | Self::StoneOutcrop | Self::BerryBush | Self::CopperVein
        )
    }
}

pub fn model_mesh(kind: ModelKind, variant: u8) -> Mesh {
    let mut geometry = Geometry::default();
    let variant = variant % kind.variant_count();
    match kind {
        ModelKind::Tree => tree(&mut geometry, variant),
        ModelKind::StoneOutcrop => stone_outcrop(&mut geometry, variant),
        ModelKind::BerryBush => berry_bush(&mut geometry, variant),
        ModelKind::Character => character(&mut geometry, variant),
        ModelKind::Workbench => workbench(&mut geometry, variant),
        ModelKind::Wall => wall(&mut geometry, 10),
        ModelKind::Door => door(&mut geometry, false),
        ModelKind::OpenDoor => door(&mut geometry, true),
        ModelKind::ConstructionWall => construction_wall(&mut geometry, 10),
        ModelKind::ConstructionDoor => construction_door(&mut geometry),
        ModelKind::Wood => wood(&mut geometry, variant),
        ModelKind::Stone => loose_stone(&mut geometry, variant),
        ModelKind::PrimitiveTool => primitive_tool(&mut geometry, variant),
        ModelKind::Berries => berries(&mut geometry, variant),
        ModelKind::CopperVein => copper_vein(&mut geometry, variant),
        ModelKind::CopperOre => copper_ore(&mut geometry, variant),
        ModelKind::Cart => cart(&mut geometry, variant),
        ModelKind::Placeholder => placeholder(&mut geometry, variant),
    }
    geometry.mesh()
}

/// Connections: north (-Z), east (+X), south (+Z), west (-X).
pub fn structure_mesh(kind: ModelKind, connections: u8) -> Mesh {
    let connections = connections & 15;
    let mut geometry = Geometry::default();
    match kind {
        ModelKind::Wall => wall(&mut geometry, connections),
        ModelKind::ConstructionWall => construction_wall(&mut geometry, connections),
        ModelKind::Door | ModelKind::OpenDoor | ModelKind::ConstructionDoor => {
            if kind == ModelKind::ConstructionDoor {
                construction_door(&mut geometry);
            } else {
                door(&mut geometry, kind == ModelKind::OpenDoor);
            }
            // An isolated door faces along Z; ties keep this stable default.
            if (connections & 5).count_ones() > (connections & 10).count_ones() {
                for vector in geometry.positions.iter_mut().chain(&mut geometry.normals) {
                    *vector = [-vector[2], vector[1], vector[0]];
                }
            }
        }
        _ => return model_mesh(kind, 0),
    }
    geometry.mesh()
}

type Rgba = [f32; 4];

const BARK: Rgba = [0.28, 0.13, 0.055, 1.0];
const BARK_LIGHT: Rgba = [0.42, 0.22, 0.08, 1.0];
const LEAF: Rgba = [0.13, 0.47, 0.08, 1.0];
const LEAF_LIGHT: Rgba = [0.30, 0.65, 0.12, 1.0];
const STONE: Rgba = [0.38, 0.40, 0.38, 1.0];
const MORTAR: Rgba = [0.56, 0.53, 0.46, 1.0];
const COPPER: Rgba = [0.72, 0.39, 0.17, 1.0];
const VERDIGRIS: Rgba = [0.25, 0.60, 0.50, 1.0];
const BLUEPRINT: Rgba = [0.10, 0.66, 0.88, 0.72];

#[derive(Default)]
struct Geometry {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<Rgba>,
}

impl Geometry {
    fn triangle(&mut self, a: Vec3, b: Vec3, c: Vec3, color: Rgba) {
        let normal = (b - a).cross(c - a).normalize_or_zero();
        for point in [a, b, c] {
            self.positions.push(point.to_array());
            self.normals.push(normal.to_array());
            self.colors.push(color);
        }
    }

    fn quad(&mut self, a: Vec3, b: Vec3, c: Vec3, d: Vec3, color: Rgba) {
        self.triangle(a, b, c, color);
        self.triangle(a, c, d, color);
    }

    fn cuboid(&mut self, center: Vec3, size: Vec3, color: Rgba) {
        let h = size * 0.5;
        let p = |x, y, z| center + Vec3::new(x * h.x, y * h.y, z * h.z);
        self.quad(
            p(-1., -1., 1.),
            p(1., -1., 1.),
            p(1., 1., 1.),
            p(-1., 1., 1.),
            color,
        );
        self.quad(
            p(1., -1., -1.),
            p(-1., -1., -1.),
            p(-1., 1., -1.),
            p(1., 1., -1.),
            shade(color, 0.72),
        );
        self.quad(
            p(1., -1., 1.),
            p(1., -1., -1.),
            p(1., 1., -1.),
            p(1., 1., 1.),
            shade(color, 0.88),
        );
        self.quad(
            p(-1., -1., -1.),
            p(-1., -1., 1.),
            p(-1., 1., 1.),
            p(-1., 1., -1.),
            shade(color, 0.78),
        );
        self.quad(
            p(-1., 1., 1.),
            p(1., 1., 1.),
            p(1., 1., -1.),
            p(-1., 1., -1.),
            shade(color, 1.14),
        );
        self.quad(
            p(-1., -1., -1.),
            p(1., -1., -1.),
            p(1., -1., 1.),
            p(-1., -1., 1.),
            shade(color, 0.62),
        );
    }

    fn prism(&mut self, base: Vec3, height: f32, bottom: f32, top: f32, sides: usize, color: Rgba) {
        let top_center = base + Vec3::Y * height;
        for i in 0..sides {
            let a = TAU * i as f32 / sides as f32;
            let b = TAU * (i + 1) as f32 / sides as f32;
            let ba = base + Vec3::new(a.cos() * bottom, 0.0, a.sin() * bottom);
            let bb = base + Vec3::new(b.cos() * bottom, 0.0, b.sin() * bottom);
            let ta = top_center + Vec3::new(a.cos() * top, 0.0, a.sin() * top);
            let tb = top_center + Vec3::new(b.cos() * top, 0.0, b.sin() * top);
            self.quad(ba, ta, tb, bb, shade(color, 0.82 + 0.24 * a.cos()));
            self.triangle(top_center, tb, ta, shade(color, 1.12));
            self.triangle(base, ba, bb, shade(color, 0.65));
        }
    }

    fn gem(&mut self, center: Vec3, radius: Vec3, sides: usize, color: Rgba) {
        let top = center + Vec3::Y * radius.y;
        let bottom = center - Vec3::Y * radius.y;
        for i in 0..sides {
            let a = TAU * i as f32 / sides as f32;
            let b = TAU * (i + 1) as f32 / sides as f32;
            let pa = center + Vec3::new(a.cos() * radius.x, 0., a.sin() * radius.z);
            let pb = center + Vec3::new(b.cos() * radius.x, 0., b.sin() * radius.z);
            self.triangle(top, pb, pa, shade(color, 0.92 + a.cos() * 0.16));
            self.triangle(bottom, pa, pb, shade(color, 0.70 + a.sin() * 0.10));
        }
    }

    fn beam(&mut self, from: Vec3, to: Vec3, width: f32, color: Rgba) {
        let delta = to - from;
        let up = delta.normalize_or_zero();
        let side = up
            .cross(if up.y.abs() > 0.8 { Vec3::X } else { Vec3::Y })
            .normalize_or_zero()
            * width;
        let front = up.cross(side).normalize_or_zero() * width;
        let a = from;
        let b = to;
        self.quad(
            a - side - front,
            a + side - front,
            b + side - front,
            b - side - front,
            color,
        );
        self.quad(
            a + side + front,
            a - side + front,
            b - side + front,
            b + side + front,
            shade(color, 0.8),
        );
        self.quad(
            a + side - front,
            a + side + front,
            b + side + front,
            b + side - front,
            shade(color, 1.08),
        );
        self.quad(
            a - side + front,
            a - side - front,
            b - side - front,
            b - side + front,
            shade(color, 0.7),
        );
        self.quad(
            b - side - front,
            b + side - front,
            b + side + front,
            b - side + front,
            shade(color, 1.15),
        );
    }

    fn mesh(self) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.positions);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_COLOR,
            self.colors
                .into_iter()
                .map(super::linear_color)
                .collect::<Vec<_>>(),
        );
        mesh
    }
}

fn shade(mut color: Rgba, factor: f32) -> Rgba {
    for channel in &mut color[..3] {
        *channel = (*channel * factor).min(1.0);
    }
    color
}

/// Six individual trees rather than six heights. Trunk proportion, crown
/// shape, blob count, lean and leaf tone all move together, so two neighbours
/// read as different trees at play distance instead of as a repeated stamp.
fn tree(g: &mut Geometry, variant: u8) {
    struct Shape {
        height: f32,
        trunk: f32,
        crown_radius: f32,
        blobs: usize,
        spread: f32,
        rise: f32,
        lean: f32,
        tone: f32,
    }
    const SHAPES: [Shape; 6] = [
        // Tall and narrow, a crowded-forest tree reaching for light.
        Shape {
            height: 2.55,
            trunk: 0.095,
            crown_radius: 0.38,
            blobs: 3,
            spread: 0.09,
            rise: 0.30,
            lean: 0.00,
            tone: 0.86,
        },
        // Low and broad, grown in the open.
        Shape {
            height: 1.50,
            trunk: 0.155,
            crown_radius: 0.46,
            blobs: 4,
            spread: 0.17,
            rise: 0.12,
            lean: 0.05,
            tone: 1.12,
        },
        // Two clear tiers.
        Shape {
            height: 2.05,
            trunk: 0.115,
            crown_radius: 0.44,
            blobs: 2,
            spread: 0.16,
            rise: 0.42,
            lean: 0.02,
            tone: 0.98,
        },
        // Leaning, one-sided crown.
        Shape {
            height: 1.90,
            trunk: 0.120,
            crown_radius: 0.40,
            blobs: 3,
            spread: 0.24,
            rise: 0.16,
            lean: 0.17,
            tone: 1.05,
        },
        // Young and slight.
        Shape {
            height: 1.20,
            trunk: 0.075,
            crown_radius: 0.30,
            blobs: 2,
            spread: 0.11,
            rise: 0.14,
            lean: 0.08,
            tone: 1.20,
        },
        // Old and spreading.
        Shape {
            height: 2.20,
            trunk: 0.185,
            crown_radius: 0.42,
            blobs: 5,
            spread: 0.24,
            rise: 0.20,
            lean: 0.03,
            tone: 0.78,
        },
    ];
    let shape = &SHAPES[variant as usize % SHAPES.len()];
    let turn = variant as f32 * 1.17;
    let lean = Vec3::new(turn.cos() * shape.lean, 0., turn.sin() * shape.lean);

    g.prism(
        Vec3::ZERO,
        shape.height * 0.55,
        shape.trunk,
        shape.trunk * 0.62,
        6,
        BARK,
    );
    let fork = lean * 0.5 + Vec3::Y * shape.height * 0.55;
    for i in 0..shape.blobs.min(3) {
        let a = turn + TAU * i as f32 / shape.blobs.max(1) as f32;
        g.beam(
            fork,
            fork + Vec3::new(
                a.cos() * shape.crown_radius * 0.7,
                0.20,
                a.sin() * shape.crown_radius * 0.7,
            ),
            0.036,
            BARK_LIGHT,
        );
    }

    let leaf = shade(LEAF, shape.tone);
    let leaf_light = shade(LEAF_LIGHT, shape.tone);
    let crown = fork + lean;
    for i in 0..shape.blobs {
        let a = turn * 1.7 + TAU * i as f32 / shape.blobs as f32;
        let step = i as f32 / shape.blobs as f32;
        let center = crown
            + Vec3::new(
                a.cos() * shape.spread,
                shape.height * shape.rise * step,
                a.sin() * shape.spread,
            );
        let scale = 1.0 - step * 0.28;
        g.gem(
            center,
            Vec3::new(
                shape.crown_radius * scale,
                shape.crown_radius * scale * 0.88,
                shape.crown_radius * scale,
            ),
            if shape.blobs > 3 { 6 } else { 7 },
            if i % 2 == 0 { leaf } else { leaf_light },
        );
    }
}

/// Five outcrops that differ in boulder count and massing, so a rocky slope
/// does not repeat one silhouette.
fn stone_outcrop(g: &mut Geometry, variant: u8) {
    struct Shape {
        boulders: usize,
        radius: f32,
        height: f32,
        spread: f32,
    }
    const SHAPES: [Shape; 5] = [
        Shape {
            boulders: 3,
            radius: 0.40,
            height: 0.30,
            spread: 0.20,
        },
        Shape {
            boulders: 1,
            radius: 0.50,
            height: 0.44,
            spread: 0.00,
        },
        Shape {
            boulders: 4,
            radius: 0.27,
            height: 0.20,
            spread: 0.28,
        },
        Shape {
            boulders: 2,
            radius: 0.44,
            height: 0.36,
            spread: 0.16,
        },
        Shape {
            boulders: 5,
            radius: 0.23,
            height: 0.16,
            spread: 0.31,
        },
    ];
    let shape = &SHAPES[variant as usize % SHAPES.len()];
    let turn = variant as f32 * 1.31;
    for i in 0..shape.boulders {
        let a = turn + TAU * i as f32 / shape.boulders as f32;
        let step = i as f32 / shape.boulders as f32;
        let scale = 1.0 - step * 0.34;
        g.gem(
            Vec3::new(
                a.cos() * shape.spread,
                shape.height * scale * 0.62,
                a.sin() * shape.spread,
            ),
            Vec3::new(
                shape.radius * scale,
                shape.height * scale,
                shape.radius * scale * 0.86,
            ),
            if i % 2 == 0 { 7 } else { 6 },
            shade(STONE, 0.82 + step * 0.34),
        );
    }
}

/// Five bushes that differ in lobe count, height and fruit load, so a berry
/// patch does not read as one shrub stamped repeatedly.
fn berry_bush(g: &mut Geometry, variant: u8) {
    const LOBES: [usize; 5] = [3, 4, 5, 6, 7];
    const HEIGHTS: [f32; 5] = [0.30, 0.46, 0.38, 0.52, 0.42];
    const TONES: [f32; 5] = [1.0, 1.14, 0.88, 1.06, 0.94];
    let index = variant as usize % LOBES.len();
    let (lobes, height, tone) = (LOBES[index], HEIGHTS[index], TONES[index]);
    let turn = variant as f32 * 0.83;
    let reach = 0.22 + height * 0.18;

    for i in 0..lobes {
        let a = turn + TAU * i as f32 / lobes as f32;
        // Stems part at the root rather than rising from one point, so the
        // bush is planted in the ground instead of balanced on a stick.
        g.beam(
            Vec3::new(a.cos() * reach * 0.28, 0., a.sin() * reach * 0.28),
            Vec3::new(a.cos() * reach * 1.15, height, a.sin() * reach * 1.15),
            0.018,
            BARK,
        );
        g.gem(
            Vec3::new(
                a.cos() * reach,
                height - 0.04 + (i % 2) as f32 * 0.08,
                a.sin() * reach,
            ),
            Vec3::new(0.27, 0.24, 0.27),
            6,
            if i % 2 == 0 {
                shade(LEAF, tone)
            } else {
                shade(LEAF_LIGHT, tone)
            },
        );
    }
    // Fruit sits on the crown surface rather than floating above it.
    let berries = lobes * 2;
    for i in 0..berries {
        let lobe = i % lobes;
        let a = turn + TAU * lobe as f32 / lobes as f32;
        let face = a + if i < lobes { -0.6 } else { 1.2 };
        let center = Vec3::new(
            a.cos() * reach,
            height - 0.04 + (lobe % 2) as f32 * 0.08,
            a.sin() * reach,
        );
        let position = center + Vec3::new(face.cos() * 0.14, 0.13, face.sin() * 0.14);
        g.gem(position, Vec3::splat(0.042), 5, [0.63, 0.05, 0.16, 1.]);
    }
}

fn character(g: &mut Geometry, variant: u8) {
    let cloth = [
        [0.16, 0.36, 0.58, 1.],
        [0.54, 0.25, 0.16, 1.],
        [0.24, 0.48, 0.28, 1.],
        [0.47, 0.28, 0.56, 1.],
    ][variant as usize];
    let skin = [
        0.69 + variant as f32 * 0.045,
        0.49 + variant as f32 * 0.035,
        0.33 + variant as f32 * 0.025,
        1.,
    ];
    g.beam(
        Vec3::new(-0.09, 0., 0.),
        Vec3::new(-0.07, 0.40, 0.),
        0.045,
        [0.19, 0.15, 0.12, 1.],
    );
    g.beam(
        Vec3::new(0.09, 0., 0.),
        Vec3::new(0.07, 0.40, 0.),
        0.045,
        [0.19, 0.15, 0.12, 1.],
    );
    g.prism(Vec3::new(0., 0.34, 0.), 0.38, 0.16, 0.12, 6, cloth);
    g.beam(
        Vec3::new(-0.12, 0.64, 0.),
        Vec3::new(-0.25, 0.39, 0.015),
        0.035,
        skin,
    );
    g.beam(
        Vec3::new(0.12, 0.64, 0.),
        Vec3::new(0.25, 0.39, -0.015),
        0.035,
        skin,
    );
    g.gem(
        Vec3::new(0., 0.82, 0.),
        Vec3::new(0.13, 0.145, 0.12),
        7,
        skin,
    );
    g.gem(
        Vec3::new(0., 0.91, -0.012),
        Vec3::new(0.14, 0.075, 0.13),
        7,
        [0.13 + variant as f32 * 0.035, 0.075, 0.035, 1.],
    );
}

fn workbench(g: &mut Geometry, variant: u8) {
    let wood = shade(BARK_LIGHT, 0.92 + variant as f32 * 0.035);
    g.cuboid(Vec3::new(0., 0.48, 0.), Vec3::new(0.82, 0.12, 0.48), wood);
    for x in [-0.31, 0.31] {
        for z in [-0.16, 0.16] {
            g.cuboid(Vec3::new(x, 0.24, z), Vec3::new(0.09, 0.48, 0.09), BARK);
        }
    }
    g.cuboid(
        Vec3::new(0., 0.31, -0.18),
        Vec3::new(0.65, 0.07, 0.07),
        BARK,
    );
    g.beam(
        Vec3::new(-0.22, 0.57, -0.05),
        Vec3::new(0.18, 0.60, 0.08),
        0.025,
        [0.18, 0.20, 0.19, 1.],
    );
    g.gem(
        Vec3::new(0.22, 0.61, 0.08),
        Vec3::new(0.11, 0.045, 0.07),
        5,
        [0.43, 0.46, 0.43, 1.],
    );
}

fn wall(g: &mut Geometry, connections: u8) {
    let connections = if connections == 0 { 10 } else { connections };
    // Center pier and arms meet without gaps at both junctions and cell edges.
    for row in 0..4 {
        let y = 0.1225 + row as f32 * 0.245;
        let color = shade(STONE, if row % 2 == 0 { 1.0 } else { 1.12 });
        g.cuboid(Vec3::new(0., y, 0.), Vec3::new(0.26, 0.245, 0.26), color);
        for (bit, direction) in [(1, -Vec3::Z), (2, Vec3::X), (4, Vec3::Z), (8, -Vec3::X)] {
            if connections & bit != 0 {
                let size = if bit & 5 != 0 {
                    Vec3::new(0.26, 0.245, 0.37)
                } else {
                    Vec3::new(0.37, 0.245, 0.26)
                };
                g.cuboid(direction * 0.315 + Vec3::Y * y, size, color);
            }
        }
    }
    g.cuboid(Vec3::new(0., 1.0, 0.), Vec3::new(0.28, 0.08, 0.28), MORTAR);
    for (bit, direction) in [(1, -Vec3::Z), (2, Vec3::X), (4, Vec3::Z), (8, -Vec3::X)] {
        if connections & bit != 0 {
            let size = if bit & 5 != 0 {
                Vec3::new(0.28, 0.08, 0.36)
            } else {
                Vec3::new(0.36, 0.08, 0.28)
            };
            g.cuboid(direction * 0.32 + Vec3::Y, size, MORTAR);
        }
    }
}

fn door(g: &mut Geometry, open: bool) {
    g.cuboid(
        Vec3::new(-0.43, 0.51, 0.),
        Vec3::new(0.14, 1.02, 0.22),
        STONE,
    );
    g.cuboid(
        Vec3::new(0.43, 0.51, 0.),
        Vec3::new(0.14, 1.02, 0.22),
        STONE,
    );
    g.cuboid(Vec3::new(0., 0.96, 0.), Vec3::new(0.86, 0.12, 0.22), MORTAR);
    let center = if open {
        Vec3::new(0.36, 0.48, -0.35)
    } else {
        Vec3::new(0., 0.48, 0.)
    };
    let size = if open {
        Vec3::new(0.12, 0.84, 0.68)
    } else {
        Vec3::new(0.68, 0.84, 0.10)
    };
    g.cuboid(center, size, BARK_LIGHT);
    for y in [0.27, 0.50, 0.73] {
        g.cuboid(
            center + Vec3::new(0., y - 0.48, if open { -0.01 } else { 0.065 }),
            Vec3::new(
                size.x * 1.04,
                0.045,
                if open { size.z * 1.03 } else { 0.04 },
            ),
            BARK,
        );
    }
}

fn construction_wall(g: &mut Geometry, connections: u8) {
    let connections = if connections == 0 { 10 } else { connections };
    g.cuboid(Vec3::Y * 0.5, Vec3::new(0.06, 1.0, 0.06), BLUEPRINT);
    for (bit, direction) in [(1, -Vec3::Z), (2, Vec3::X), (4, Vec3::Z), (8, -Vec3::X)] {
        if connections & bit == 0 {
            continue;
        }
        g.cuboid(
            direction * 0.47 + Vec3::Y * 0.5,
            Vec3::new(0.06, 1.0, 0.06),
            BLUEPRINT,
        );
        for y in [0.10, 0.95] {
            let size = if bit & 5 != 0 {
                Vec3::new(0.06, 0.06, 0.5)
            } else {
                Vec3::new(0.5, 0.06, 0.06)
            };
            g.cuboid(direction * 0.25 + Vec3::Y * y, size, BLUEPRINT);
        }
        g.beam(
            direction * 0.04 + Vec3::Y * 0.14,
            direction * 0.44 + Vec3::Y * 0.90,
            0.02,
            shade(BLUEPRINT, 1.12),
        );
    }
}

fn construction_door(g: &mut Geometry) {
    // Paired jamb uprights and lintel outline the future doorway without
    // a diagonal brace obscuring the walkable opening.
    for x in [-0.47, -0.35, 0.35, 0.47] {
        g.cuboid(Vec3::new(x, 0.5, 0.), Vec3::new(0.06, 1.0, 0.06), BLUEPRINT);
    }
    for y in [0.92, 1.01] {
        g.cuboid(Vec3::new(0., y, 0.), Vec3::new(1.0, 0.06, 0.06), BLUEPRINT);
    }
}

/// A neutral marker that reads as "this exists but has no art yet".
fn placeholder(g: &mut Geometry, variant: u8) {
    let tint = 0.42 + variant as f32 * 0.04;
    g.gem(
        Vec3::new(0., 0.18, 0.),
        Vec3::new(0.14, 0.18, 0.14),
        4,
        [tint, tint * 0.55, tint, 1.],
    );
}

fn wood(g: &mut Geometry, variant: u8) {
    let angle = variant as f32 * 0.31;
    for offset in [-0.065_f32, 0.065] {
        let from = Vec3::new(
            -angle.cos() * 0.16,
            0.026 + offset.abs() * 0.2,
            -angle.sin() * 0.16 + offset,
        );
        let to = Vec3::new(
            angle.cos() * 0.16,
            0.026 + offset.abs() * 0.2,
            angle.sin() * 0.16 + offset,
        );
        g.beam(from, to, 0.05, BARK_LIGHT);
    }
}

/// Rock with copper showing through, so a vein reads as stone at a distance
/// and as ore up close.
/// Rock with copper showing through, so a vein reads as stone at a distance
/// and as ore up close. Five arrangements of where the metal surfaces.
fn copper_vein(g: &mut Geometry, variant: u8) {
    const HOST: [(f32, f32, usize); 5] = [
        (0.38, 0.26, 7),
        (0.30, 0.34, 6),
        (0.44, 0.20, 7),
        (0.34, 0.30, 5),
        (0.26, 0.24, 6),
    ];
    let index = variant as usize % HOST.len();
    let (radius, height, sides) = HOST[index];
    let turn = variant as f32 * 0.97;

    g.gem(
        Vec3::new(turn.cos() * 0.05, height * 0.62, turn.sin() * 0.05),
        Vec3::new(radius, height, radius * 0.9),
        sides,
        shade(STONE, 0.88 + index as f32 * 0.05),
    );
    for i in 0..=(index % 3) {
        let a = turn * 1.6 + TAU * i as f32 / 3.;
        g.gem(
            Vec3::new(
                a.cos() * radius * 0.55,
                height * 1.15,
                a.sin() * radius * 0.55,
            ),
            Vec3::splat(0.10 + (i % 2) as f32 * 0.045),
            5,
            if i % 2 == 0 { COPPER } else { VERDIGRIS },
        );
    }
}

/// A two-wheeled hand cart: a shallow box on an axle with a drawbar.
fn cart(g: &mut Geometry, variant: u8) {
    let tilt = variant as f32 * 0.05;
    g.cuboid(
        Vec3::new(0., 0.26 + tilt, 0.),
        Vec3::new(0.46, 0.16, 0.34),
        BARK_LIGHT,
    );
    for z in [-0.19_f32, 0.19] {
        g.gem(
            Vec3::new(-0.06, 0.16, z),
            Vec3::new(0.17, 0.16, 0.05),
            7,
            shade(BARK, 0.85),
        );
    }
    g.beam(
        Vec3::new(0.20, 0.30 + tilt, 0.),
        Vec3::new(0.52, 0.20 + tilt, 0.),
        0.032,
        BARK,
    );
}

fn copper_ore(g: &mut Geometry, variant: u8) {
    let s = 0.09 + variant as f32 * 0.007;
    g.gem(
        Vec3::new(0., s * 0.6, 0.),
        Vec3::new(0.15, s, 0.12),
        6,
        shade(STONE, 0.9),
    );
    g.gem(
        Vec3::new(0.03, s * 1.1, -0.02),
        Vec3::new(0.07, s * 0.6, 0.06),
        5,
        COPPER,
    );
}

fn loose_stone(g: &mut Geometry, variant: u8) {
    let s = 0.10 + variant as f32 * 0.008;
    g.gem(
        Vec3::new(0., s * 0.6, 0.),
        Vec3::new(0.16, s, 0.13),
        6,
        STONE,
    );
}

fn primitive_tool(g: &mut Geometry, variant: u8) {
    let turn = variant as f32 * 0.28;
    let direction = Vec3::new(turn.cos(), 0., turn.sin());
    // A dropped tool lies on the ground rather than hovering over it.
    g.beam(
        -direction * 0.15 + Vec3::Y * 0.026,
        direction * 0.14 + Vec3::Y * 0.055,
        0.025,
        BARK_LIGHT,
    );
    g.gem(
        direction * 0.18 + Vec3::Y * 0.055,
        Vec3::new(0.10, 0.055, 0.075),
        5,
        shade(STONE, 0.82),
    );
}

fn berries(g: &mut Geometry, variant: u8) {
    for i in 0..5 {
        let a = TAU * (i as f32 + variant as f32 * 0.17) / 5.;
        g.gem(
            Vec3::new(
                a.cos() * 0.085,
                0.06 + (i % 2) as f32 * 0.045,
                a.sin() * 0.085,
            ),
            Vec3::splat(0.052),
            5,
            if i % 2 == 0 {
                [0.62, 0.04, 0.14, 1.]
            } else {
                [0.82, 0.08, 0.18, 1.]
            },
        );
    }
    g.beam(
        Vec3::new(0., 0.10, 0.),
        Vec3::new(0.06, 0.19, 0.),
        0.009,
        LEAF,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

    const KINDS: [ModelKind; ModelKind::ALL.len()] = ModelKind::ALL;

    fn float3(mesh: &Mesh, attribute: bevy::mesh::MeshVertexAttribute) -> &Vec<[f32; 3]> {
        match mesh.attribute(attribute).expect("attribute") {
            VertexAttributeValues::Float32x3(values) => values,
            other => panic!("unexpected attribute {other:?}"),
        }
    }

    fn mesh_bounds(mesh: &Mesh) -> (Vec3, Vec3) {
        float3(mesh, Mesh::ATTRIBUTE_POSITION).iter().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(min, max), p| (min.min(Vec3::from(*p)), max.max(Vec3::from(*p))),
        )
    }

    /// The variant space must stay bounded: the mesh cache holds one entry per
    /// kind and variant, and an unbounded count would mean one mesh per world
    /// cell. See ADR-0005.
    #[test]
    fn every_kind_has_a_bounded_positive_variant_count() {
        let mut total = 0_usize;
        for kind in ModelKind::ALL {
            let count = kind.variant_count();
            assert!(count > 0, "{kind:?} has no variants");
            assert!(count <= 16, "{kind:?} has an unbounded variant space");
            total += count as usize;
        }
        assert!(total <= 128, "the whole mesh cache is {total} meshes");
    }

    /// Variants must differ in silhouette, not by a few percent of height. The
    /// bug this pins: four tree "variants" that differed only by 6% height and
    /// a small branch rotation, which read as one repeated tree in play.
    #[test]
    fn natural_variants_differ_in_shape_not_only_in_scale() {
        for kind in ModelKind::ALL
            .into_iter()
            .filter(|k| k.accepts_pose_variety())
        {
            let shapes: Vec<_> = (0..kind.variant_count())
                .map(|variant| {
                    let mesh = model_mesh(kind, variant);
                    let (min, max) = mesh_bounds(&mesh);
                    let triangles = float3(&mesh, Mesh::ATTRIBUTE_POSITION).len() / 3;
                    (max - min, triangles)
                })
                .collect();
            for (i, (extent, triangles)) in shapes.iter().enumerate() {
                for (j, (other_extent, other_triangles)) in shapes.iter().enumerate().skip(i + 1) {
                    // Either the outline or the construction must differ, and
                    // a difference in outline must be worth seeing.
                    let proportion = (extent.y / extent.x) - (other_extent.y / other_extent.x);
                    assert!(
                        triangles != other_triangles || proportion.abs() > 0.08,
                        "{kind:?} variants {i} and {j} are visually the same shape"
                    );
                }
            }
        }
    }

    /// A pose turns and resizes an instance; it must not stretch it into
    /// something that no longer reads as the same object.
    #[test]
    fn every_natural_model_fits_its_cell_after_the_widest_pose() {
        for kind in ModelKind::ALL
            .into_iter()
            .filter(|k| k.accepts_pose_variety())
        {
            for variant in 0..kind.variant_count() {
                let (min, max) = mesh_bounds(&model_mesh(kind, variant));
                let radius = min.x.abs().max(max.x).max(min.z.abs()).max(max.z);
                assert!(
                    radius <= 0.75,
                    "{kind:?} variant {variant} reaches {radius} from its cell centre"
                );
                assert!(
                    min.y >= -0.2,
                    "{kind:?} variant {variant} sinks below ground"
                );
            }
        }
    }

    /// A `gem` is a bipyramid: centring one at exactly its own vertical radius
    /// leaves the lower apex touching the ground at a single point, and from a
    /// low camera the object reads as hovering over its own shadow. Anything
    /// that stands on the ground must meet it with a footprint, not a point,
    /// and must not float above it either.
    #[test]
    fn ground_resting_models_meet_the_ground_with_a_footprint() {
        let standing = |kind: ModelKind| {
            kind.accepts_pose_variety()
                || matches!(
                    kind,
                    ModelKind::Wood
                        | ModelKind::Stone
                        | ModelKind::Berries
                        | ModelKind::CopperOre
                        | ModelKind::PrimitiveTool
                        | ModelKind::Workbench
                        | ModelKind::Character
                )
        };
        for kind in ModelKind::ALL.into_iter().filter(|k| standing(*k)) {
            for variant in 0..kind.variant_count() {
                let mesh = model_mesh(kind, variant);
                let points = float3(&mesh, Mesh::ATTRIBUTE_POSITION);
                // A footprint is either vertices sitting on the ground or the
                // cross-section of geometry that passes through it. A gem has
                // no vertices near the ground at all, only its two apexes, so
                // measuring vertices alone would miss a buried one entirely.
                let mut contact: Vec<Vec3> = points
                    .iter()
                    .map(|p| Vec3::from(*p))
                    .filter(|p| p.y.abs() <= 0.02)
                    .collect();
                for triangle in points.chunks_exact(3) {
                    for i in 0..3 {
                        let (a, b) = (Vec3::from(triangle[i]), Vec3::from(triangle[(i + 1) % 3]));
                        if (a.y < 0.) != (b.y < 0.) && (b.y - a.y).abs() > 1e-6 {
                            contact.push(a + (b - a) * (-a.y / (b.y - a.y)));
                        }
                    }
                }
                assert!(
                    !contact.is_empty(),
                    "{kind:?} variant {variant} floats above the ground"
                );
                let spread = contact.iter().fold(0.0_f32, |widest, a| {
                    contact.iter().fold(widest, |widest, b| {
                        widest.max((Vec3::new(a.x, 0., a.z) - Vec3::new(b.x, 0., b.z)).length())
                    })
                });
                assert!(
                    spread >= 0.08,
                    "{kind:?} variant {variant} meets the ground on a {spread:.3} wide point"
                );
            }
        }
    }

    #[test]
    fn connected_walls_and_blueprints_reach_only_requested_cell_edges() {
        for kind in [ModelKind::Wall, ModelKind::ConstructionWall] {
            for mask in 1..16 {
                let (min, max) = mesh_bounds(&structure_mesh(kind, mask));
                for (bit, extent) in [(1, -min.z), (2, max.x), (4, max.z), (8, -min.x)] {
                    assert!(extent <= 0.50001, "{kind:?} mask {mask}: exceeds cell");
                    if mask & bit != 0 {
                        assert!((extent - 0.5).abs() < 0.00001, "{kind:?} mask {mask}: gap");
                    } else {
                        assert!(extent < 0.2, "{kind:?} mask {mask}: unwanted arm");
                    }
                }
            }
        }
    }

    #[test]
    fn doorway_axis_follows_neighbors_and_opening_stays_clear() {
        for kind in [
            ModelKind::Door,
            ModelKind::OpenDoor,
            ModelKind::ConstructionDoor,
        ] {
            let (min, max) = mesh_bounds(&structure_mesh(kind, 5));
            assert!((min.z + 0.5).abs() < 0.00001);
            assert!((max.z - 0.5).abs() < 0.00001);
            let ew = structure_mesh(kind, 10);
            let ns = structure_mesh(kind, 5);
            assert_ne!(
                float3(&ew, Mesh::ATTRIBUTE_POSITION),
                float3(&ns, Mesh::ATTRIBUTE_POSITION)
            );
        }
        let blocks_opening = |kind| {
            let mesh = structure_mesh(kind, 10);
            // Project triangles onto XY to cast through the center of the door.
            float3(&mesh, Mesh::ATTRIBUTE_POSITION)
                .chunks_exact(3)
                .any(|t| {
                    let a = Vec3::from(t[0]);
                    let b = Vec3::from(t[1]) - a;
                    let c = Vec3::from(t[2]) - a;
                    let p = Vec3::new(0., 0.5, 0.) - a;
                    let determinant = b.x * c.y - b.y * c.x;
                    if determinant.abs() < 0.00001 {
                        return false;
                    }
                    let u = (p.x * c.y - p.y * c.x) / determinant;
                    let v = (b.x * p.y - b.y * p.x) / determinant;
                    u >= 0. && v >= 0. && u + v <= 1.
                })
        };
        assert!(blocks_opening(ModelKind::Door));
        assert!(!blocks_opening(ModelKind::OpenDoor));
        assert!(!blocks_opening(ModelKind::ConstructionDoor));
    }

    #[test]
    fn every_model_has_finite_flat_geometry() {
        for kind in KINDS {
            let mesh = model_mesh(kind, 0);
            let positions = float3(&mesh, Mesh::ATTRIBUTE_POSITION);
            let normals = float3(&mesh, Mesh::ATTRIBUTE_NORMAL);
            assert!(!positions.is_empty(), "{kind:?}");
            assert_eq!(positions.len(), normals.len(), "{kind:?}");
            assert_eq!(positions.len() % 3, 0, "{kind:?}");
            assert!(
                positions.iter().flatten().all(|value| value.is_finite()),
                "{kind:?}"
            );
            assert!(
                normals.iter().flatten().all(|value| value.is_finite()),
                "{kind:?}"
            );
            assert!(
                normals
                    .iter()
                    .all(|normal| Vec3::from_array(*normal).length_squared() > 0.99),
                "{kind:?}"
            );
            assert!(mesh.attribute(Mesh::ATTRIBUTE_COLOR).is_some(), "{kind:?}");
        }
    }

    #[test]
    fn model_scales_and_footprints_are_distinct() {
        let bounds = |kind| {
            let mesh = model_mesh(kind, 0);
            let points = float3(&mesh, Mesh::ATTRIBUTE_POSITION);
            points.iter().fold(
                (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                |(min, max), point| {
                    (
                        min.min(Vec3::from_array(*point)),
                        max.max(Vec3::from_array(*point)),
                    )
                },
            )
        };
        let (tree_min, tree_max) = bounds(ModelKind::Tree);
        let (person_min, person_max) = bounds(ModelKind::Character);
        let (item_min, item_max) = bounds(ModelKind::Berries);
        let (wall_min, wall_max) = bounds(ModelKind::Wall);
        assert!(tree_max.y - tree_min.y > 1.7);
        assert!((0.90..=1.02).contains(&(person_max.y - person_min.y)));
        assert!(item_max.y - item_min.y < 0.25);
        assert!(wall_max.x - wall_min.x > 0.9);
        assert_ne!(bounds(ModelKind::Door), bounds(ModelKind::OpenDoor));
    }

    #[test]
    fn variants_are_bounded_and_change_geometry() {
        // Variant space wraps at each kind's own count, so an out-of-range
        // variety value can never ask for an unbounded number of meshes.
        for kind in KINDS {
            let count = kind.variant_count();
            assert_eq!(
                float3(&model_mesh(kind, 0), Mesh::ATTRIBUTE_POSITION),
                float3(&model_mesh(kind, count), Mesh::ATTRIBUTE_POSITION),
                "{kind:?} does not wrap at {count}"
            );
            assert_eq!(
                float3(&model_mesh(kind, 1), Mesh::ATTRIBUTE_POSITION),
                float3(&model_mesh(kind, count + 1), Mesh::ATTRIBUTE_POSITION),
                "{kind:?} does not wrap at {count}"
            );
        }
        assert_ne!(
            float3(&model_mesh(ModelKind::Tree, 0), Mesh::ATTRIBUTE_POSITION),
            float3(&model_mesh(ModelKind::Tree, 1), Mesh::ATTRIBUTE_POSITION)
        );
    }
    #[test]
    fn convex_primitives_have_outward_winding_and_normals() {
        let center = Vec3::new(2., 3., -1.);
        let mut cube = Geometry::default();
        cube.cuboid(center, Vec3::splat(2.), STONE);
        let mut prism = Geometry::default();
        prism.prism(center - Vec3::Y, 2., 1., 0.7, 7, STONE);
        let mut gem = Geometry::default();
        gem.gem(center, Vec3::new(1., 2., 0.8), 7, STONE);
        let mut beam = Geometry::default();
        beam.beam(center - Vec3::Y, center + Vec3::Y, 0.2, STONE);
        for (name, geometry) in [
            ("cube", cube),
            ("prism", prism),
            ("gem", gem),
            ("beam", beam),
        ] {
            for (vertices, normals) in geometry
                .positions
                .chunks_exact(3)
                .zip(geometry.normals.chunks_exact(3))
            {
                let [a, b, c] = [
                    Vec3::from(vertices[0]),
                    Vec3::from(vertices[1]),
                    Vec3::from(vertices[2]),
                ];
                let outward = (a + b + c) / 3. - center;
                assert!(
                    (b - a).cross(c - a).dot(outward) > 0.,
                    "{name}: inward triangle"
                );
                assert!(
                    Vec3::from(normals[0]).dot(outward) > 0.,
                    "{name}: inward normal"
                );
            }
        }
    }
}
