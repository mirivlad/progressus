use std::collections::BTreeMap;

use bevy::{asset::RenderAssetUsages, mesh::PrimitiveTopology, prelude::*};
use progressus_app::{ChunkSnapshot, KnownTerrain, LocalCell, TerrainId, WorldCell, terrain};

pub(super) fn terrain_mesh(chunk: &ChunkSnapshot, known: &BTreeMap<WorldCell, TerrainId>) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut colors = Vec::new();
    let side = usize::from(chunk.side);
    let mut triangle = |a: Vec3, b: Vec3, c: Vec3, color: [f32; 4]| {
        if (b - a).cross(c - a).length_squared() < 0.000001 {
            return;
        }
        let normal = (b - a).cross(c - a).normalize().to_array();
        positions.extend([a.to_array(), b.to_array(), c.to_array()]);
        normals.extend([normal; 3]);
        colors.extend([color; 3]);
    };
    for (i, terrain) in chunk.cells.iter().enumerate() {
        let KnownTerrain::Known(kind) = terrain else {
            continue;
        };
        let Some(cell) = chunk
            .coordinate
            .world_cell(LocalCell::new((i % side) as u16, (i / side) as u16))
        else {
            continue;
        };
        let neighbour = |dx: i64, dy: i64| {
            cell.x()
                .checked_add(dx)
                .zip(cell.y().checked_add(dy))
                .and_then(|(x, y)| known.get(&WorldCell::new(x, y)))
                .copied()
        };
        let x = (i % side) as f32;
        let z = -((i / side) as f32);
        let hash = (cell.x() as u64)
            .wrapping_mul(731)
            .wrapping_add((cell.y() as u64).wrapping_mul(157));
        let variation = (hash % 11) as f32 * 0.006;
        let color = match kind.name() {
            "grass" => [0.24 + variation, 0.43 + variation, 0.12 + variation, 1.],
            "water" => [0.035, 0.34 + variation, 0.46 + variation, 1.],
            "rock" => [0.39 + variation, 0.41 + variation, 0.38 + variation, 1.],
            _ => [0.5 + variation, 0.5 + variation, 0.5 + variation, 1.],
        };
        let sand = [0.68, 0.56, 0.32, 1.];
        let base = if *kind == terrain::WATER { -0.14 } else { 0. };
        let corners = [
            Vec3::new(-0.5, 0., -0.5),
            Vec3::new(-0.5, 0., 0.5),
            Vec3::new(0.5, 0., 0.5),
            Vec3::new(0.5, 0., -0.5),
        ];
        // This clockwise ring has upward winding in world X/Z coordinates.
        let offsets = [(-1, 0), (0, -1), (1, 0), (0, 1)];
        let same: [bool; 4] =
            std::array::from_fn(|j| neighbour(offsets[j].0, offsets[j].1) == Some(*kind));
        let origin = Vec3::new(x, 0., z);
        let mut ring = Vec::with_capacity(8);
        for j in 0..4 {
            let previous = (j + 3) % 4;
            let corner = corners[j];
            let cut = *kind != terrain::GRASS && !same[previous] && !same[j];
            let height = if *kind == terrain::ROCK
                && same[previous]
                && same[j]
                && neighbour(
                    offsets[previous].0 + offsets[j].0,
                    offsets[previous].1 + offsets[j].1,
                ) == Some(terrain::ROCK)
            {
                2.65
            } else {
                base
            };
            if cut {
                let a = corner.lerp(corners[previous], 0.23) + origin;
                let b = corner.lerp(corners[(j + 1) % 4], 0.23) + origin;
                triangle(
                    corner + origin,
                    b,
                    a,
                    if *kind == terrain::WATER {
                        sand
                    } else {
                        [0.30, 0.40, 0.17, 1.]
                    },
                );
                ring.push((a + Vec3::Y * base, true));
                ring.push((b + Vec3::Y * base, !same[j]));
            } else {
                ring.push((corner + origin + Vec3::Y * height, !same[j]));
            }
            // Edge midpoint is shared across chunks and connects mountain ridges.
            if *kind == terrain::ROCK {
                let h = if same[j] { 2.65 } else { 0. };
                ring.push((
                    corner.lerp(corners[(j + 1) % 4], 0.5) + origin + Vec3::Y * h,
                    !same[j],
                ));
            }
        }
        let center = origin
            + Vec3::Y
                * if *kind == terrain::ROCK {
                    2.8 + (hash % 7) as f32 * 0.05
                } else {
                    base
                };
        for j in 0..ring.len() {
            let (a, exposed) = ring[j];
            let b = ring[(j + 1) % ring.len()].0;
            if *kind == terrain::WATER && exposed {
                let inner_a = a.lerp(center, 0.18);
                let inner_b = b.lerp(center, 0.18);
                let bank_a = Vec3::new(a.x, 0., a.z);
                let bank_b = Vec3::new(b.x, 0., b.z);
                triangle(bank_a, bank_b, inner_b, sand);
                triangle(bank_a, inner_b, inner_a, sand);
                triangle(inner_a, inner_b, center, color);
                triangle(a, bank_a, inner_a, sand);
                triangle(bank_b, b, inner_b, sand);
            } else {
                triangle(a, b, center, color);
            }
            // Skirts close exposed biome/discovery edges; same-terrain edges join directly.
            if exposed && *kind != terrain::WATER {
                let bottom_a = Vec3::new(a.x, -0.28, a.z);
                let bottom_b = Vec3::new(b.x, -0.28, b.z);
                let edge = [color[0] * 0.7, color[1] * 0.7, color[2] * 0.65, 1.];
                triangle(a, bottom_a, b, edge);
                triangle(b, bottom_a, bottom_b, edge);
            }
        }
        // The sandy bank reaches the original square footprint, including the
        // filled bevel corners. Close that outer edge, not the inset water ring.
        if *kind == terrain::WATER {
            for j in 0..4 {
                if same[j] {
                    continue;
                }
                let a = corners[j] + origin;
                let b = corners[(j + 1) % 4] + origin;
                let bottom_a = a - Vec3::Y * 0.28;
                let bottom_b = b - Vec3::Y * 0.28;
                triangle(a, bottom_a, b, sand);
                triangle(b, bottom_a, bottom_b, sand);
            }
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_COLOR,
        colors
            .into_iter()
            .map(super::linear_color)
            .collect::<Vec<_>>(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use progressus_app::ChunkCoord;

    fn chunk(kind: KnownTerrain) -> ChunkSnapshot {
        ChunkSnapshot {
            coordinate: ChunkCoord::new(0, 0),
            side: 1,
            cells: vec![kind],
        }
    }

    fn positions(mesh: &Mesh) -> &Vec<[f32; 3]> {
        let bevy::mesh::VertexAttributeValues::Float32x3(positions) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
        else {
            panic!("positions");
        };
        positions
    }

    #[test]
    fn mountains_rise_well_above_resource_rocks() {
        let mesh = terrain_mesh(&chunk(KnownTerrain::Known(terrain::ROCK)), &BTreeMap::new());
        assert!(positions(&mesh).iter().any(|p| p[1] >= 2.5));
    }

    #[test]
    fn hidden_cells_produce_no_geometry() {
        let known = BTreeMap::from([(WorldCell::new(0, 0), terrain::ROCK)]);
        assert_eq!(
            terrain_mesh(&chunk(KnownTerrain::Unknown), &known).count_vertices(),
            0
        );
    }

    #[test]
    fn isolated_water_has_beveled_shore_with_sand_above_water() {
        let mesh = terrain_mesh(
            &chunk(KnownTerrain::Known(terrain::WATER)),
            &BTreeMap::new(),
        );
        let points = positions(&mesh);
        assert!(points.iter().any(|p| p[1] == -0.14));
        assert!(
            points
                .iter()
                .any(|p| p[1] == 0. && (p[0].abs() - 0.27).abs() < 0.001)
        );
        let bevy::mesh::VertexAttributeValues::Float32x4(colors) =
            mesh.attribute(Mesh::ATTRIBUTE_COLOR).unwrap()
        else {
            panic!("colors");
        };
        assert!(colors.contains(&super::super::linear_color([0.68, 0.56, 0.32, 1.])));
    }

    #[test]
    fn known_rock_edges_share_the_same_ridge_height() {
        let mut left = chunk(KnownTerrain::Known(terrain::ROCK));
        left.side = 32;
        left.cells = vec![KnownTerrain::Unknown; 32 * 32];
        left.cells[31] = KnownTerrain::Known(terrain::ROCK);
        let mut right = chunk(KnownTerrain::Known(terrain::ROCK));
        right.coordinate = ChunkCoord::new(1, 0);
        let known = BTreeMap::from([
            (WorldCell::new(31, 0), terrain::ROCK),
            (WorldCell::new(32, 0), terrain::ROCK),
        ]);
        let a = terrain_mesh(&left, &known);
        let b = terrain_mesh(&right, &known);
        assert!(positions(&a).contains(&[31.5, 2.65, 0.]));
        assert!(positions(&b).contains(&[-0.5, 2.65, 0.]));
    }
    #[test]
    fn shore_skirts_close_the_actual_square_footprint_at_ground_height() {
        let mesh = terrain_mesh(
            &chunk(KnownTerrain::Known(terrain::WATER)),
            &BTreeMap::new(),
        );
        for corner in [
            [-0.5, 0., -0.5],
            [-0.5, 0., 0.5],
            [0.5, 0., 0.5],
            [0.5, 0., -0.5],
        ] {
            let bottom = [corner[0], -0.28, corner[2]];
            assert!(
                positions(&mesh)
                    .chunks_exact(3)
                    .any(|face| face.contains(&corner) && face.contains(&bottom)),
                "shore skirt must reach the bank corner {corner:?}"
            );
        }
    }

    #[test]
    fn every_neighborhood_covers_the_cell_with_upward_top_faces() {
        let offsets = [
            (-1, -1),
            (0, -1),
            (1, -1),
            (-1, 0),
            (1, 0),
            (-1, 1),
            (0, 1),
            (1, 1),
        ];
        for kind in [terrain::GRASS, terrain::ROCK, terrain::WATER] {
            for mask in 0..256 {
                let known = offsets
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| mask & (1 << i) != 0)
                    .map(|(_, &(x, y))| (WorldCell::new(x, y), kind))
                    .collect();
                let mesh = terrain_mesh(&chunk(KnownTerrain::Known(kind)), &known);
                let mut area = 0.;
                for face in positions(&mesh).chunks_exact(3) {
                    let [a, b, c] = [
                        Vec3::from_array(face[0]),
                        Vec3::from_array(face[1]),
                        Vec3::from_array(face[2]),
                    ];
                    let projected = (b - a).cross(c - a).y;
                    assert!(
                        projected >= -0.000001,
                        "downward face: {kind:?}, mask {mask}, {face:?}"
                    );
                    area += projected * 0.5;
                }
                assert!(
                    (area - 1.).abs() < 0.00001,
                    "cell coverage {area}: {kind:?}, mask {mask}"
                );
            }
        }
    }
    #[test]
    fn four_chunk_corner_height_agrees_for_every_partial_discovery_mask() {
        let cells = [(31, 31), (32, 31), (31, 32), (32, 32)];
        for mask in 1..16 {
            let known = cells
                .iter()
                .enumerate()
                .filter(|(i, _)| mask & (1 << i) != 0)
                .map(|(_, &(x, y))| (WorldCell::new(x, y), terrain::ROCK))
                .collect();
            let expected = if mask == 15 { 2.65 } else { 0. };
            for (i, &(x, y)) in cells.iter().enumerate() {
                if mask & (1 << i) == 0 {
                    continue;
                }
                let (coordinate, local) = WorldCell::new(x, y).split();
                let mut snapshot = ChunkSnapshot {
                    coordinate,
                    side: 32,
                    cells: vec![KnownTerrain::Unknown; 1024],
                };
                snapshot.cells[usize::from(local.y()) * 32 + usize::from(local.x())] =
                    KnownTerrain::Known(terrain::ROCK);
                let mesh = terrain_mesh(&snapshot, &known);
                let cx = 31.5 - coordinate.x() as f32 * 32.;
                let cz = -31.5 + coordinate.y() as f32 * 32.;
                let heights: Vec<_> = positions(&mesh)
                    .iter()
                    .filter(|p| p[0] == cx && p[2] == cz && p[1] >= 0.)
                    .map(|p| p[1])
                    .collect();
                assert!(!heights.is_empty());
                assert!(
                    heights.iter().all(|&h| h == expected),
                    "corner mismatch: mask {mask}, {coordinate:?}, {heights:?}"
                );
            }
        }
    }
}
