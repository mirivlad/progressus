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
    /// Stands in for content this build has no authored model for, so a new
    /// definition is visible in the world instead of invisible. See ADR-0021.
    Placeholder,
}

pub fn model_mesh(kind: ModelKind, variant: u8) -> Mesh {
    let mut geometry = Geometry::default();
    let variant = variant % 4;
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

fn tree(g: &mut Geometry, variant: u8) {
    let height = 1.85 + variant as f32 * 0.12;
    g.prism(Vec3::new(0., 0., 0.), height * 0.58, 0.13, 0.085, 6, BARK);
    for (angle, rise) in [(0.3, 0.67), (2.4, 0.58), (4.5, 0.72)] {
        let angle = angle + variant as f32 * 0.37;
        let from = Vec3::new(0., height * rise, 0.);
        let to = from + Vec3::new(angle.cos() * 0.32, 0.22, angle.sin() * 0.32);
        g.beam(from, to, 0.038, BARK_LIGHT);
    }
    let crown = height * 0.72;
    g.gem(
        Vec3::new(0., crown, 0.),
        Vec3::new(0.53, 0.48, 0.50),
        7,
        LEAF,
    );
    g.gem(
        Vec3::new(-0.26, crown + 0.22, 0.08),
        Vec3::new(0.36, 0.39, 0.35),
        6,
        LEAF_LIGHT,
    );
    g.gem(
        Vec3::new(0.27, crown + 0.17, -0.06),
        Vec3::new(0.39, 0.35, 0.37),
        7,
        shade(LEAF, 0.9),
    );
}

fn stone_outcrop(g: &mut Geometry, variant: u8) {
    let offset = variant as f32 * 0.025;
    g.gem(
        Vec3::new(-0.12, 0.28, 0.02),
        Vec3::new(0.42, 0.28 + offset, 0.34),
        7,
        STONE,
    );
    g.gem(
        Vec3::new(0.22, 0.18, -0.10),
        Vec3::new(0.29, 0.19, 0.27),
        6,
        shade(STONE, 1.12),
    );
    g.gem(
        Vec3::new(0.03, 0.12, 0.27),
        Vec3::new(0.22, 0.13, 0.18),
        5,
        shade(STONE, 0.78),
    );
}

fn berry_bush(g: &mut Geometry, variant: u8) {
    let turn = variant as f32 * 0.41;
    for i in 0..5 {
        let a = turn + TAU * i as f32 / 5.;
        g.beam(
            Vec3::new(0., 0.03, 0.),
            Vec3::new(a.cos() * 0.3, 0.46, a.sin() * 0.3),
            0.018,
            BARK,
        );
        g.gem(
            Vec3::new(a.cos() * 0.26, 0.42 + (i % 2) as f32 * 0.08, a.sin() * 0.26),
            Vec3::new(0.28, 0.25, 0.28),
            6,
            if i % 2 == 0 { LEAF } else { LEAF_LIGHT },
        );
    }
    for i in 0..10 {
        let lobe = i % 5;
        let a = turn + TAU * lobe as f32 / 5.;
        let face = a + if i < 5 { -0.6 } else { 1.2 };
        let center = Vec3::new(
            a.cos() * 0.26,
            0.42 + (lobe % 2) as f32 * 0.08,
            a.sin() * 0.26,
        );
        // At this height the crown's facet radius is about 0.13 cells.
        // Embed fruit slightly into that surface instead of floating above it.
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
        Vec3::new(-0.09, 0.03, 0.),
        Vec3::new(-0.07, 0.40, 0.),
        0.045,
        [0.19, 0.15, 0.12, 1.],
    );
    g.beam(
        Vec3::new(0.09, 0.03, 0.),
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
            0.10 + offset.abs() * 0.2,
            -angle.sin() * 0.16 + offset,
        );
        let to = Vec3::new(
            angle.cos() * 0.16,
            0.10 + offset.abs() * 0.2,
            angle.sin() * 0.16 + offset,
        );
        g.beam(from, to, 0.05, BARK_LIGHT);
    }
}

fn loose_stone(g: &mut Geometry, variant: u8) {
    let s = 0.10 + variant as f32 * 0.008;
    g.gem(Vec3::new(0., s, 0.), Vec3::new(0.16, s, 0.13), 6, STONE);
}

fn primitive_tool(g: &mut Geometry, variant: u8) {
    let turn = variant as f32 * 0.28;
    let direction = Vec3::new(turn.cos(), 0., turn.sin());
    g.beam(
        -direction * 0.15 + Vec3::Y * 0.08,
        direction * 0.14 + Vec3::Y * 0.12,
        0.025,
        BARK_LIGHT,
    );
    g.gem(
        direction * 0.18 + Vec3::Y * 0.14,
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

    const KINDS: [ModelKind; 14] = [
        ModelKind::Tree,
        ModelKind::StoneOutcrop,
        ModelKind::BerryBush,
        ModelKind::Character,
        ModelKind::Workbench,
        ModelKind::Wall,
        ModelKind::Door,
        ModelKind::OpenDoor,
        ModelKind::ConstructionWall,
        ModelKind::ConstructionDoor,
        ModelKind::Wood,
        ModelKind::Stone,
        ModelKind::PrimitiveTool,
        ModelKind::Berries,
    ];

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
        for kind in KINDS {
            assert_eq!(
                float3(&model_mesh(kind, 1), Mesh::ATTRIBUTE_POSITION),
                float3(&model_mesh(kind, 5), Mesh::ATTRIBUTE_POSITION)
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
