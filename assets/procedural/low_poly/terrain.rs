use bevy::{asset::RenderAssetUsages, mesh::PrimitiveTopology, prelude::*};
use progressus_app::{ChunkSnapshot, KnownTerrain, Terrain};

pub(super) fn terrain_mesh(chunk: &ChunkSnapshot) -> Mesh {
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut colors = Vec::new();
    let side = usize::from(chunk.side);
    let mut triangle = |a: Vec3, b: Vec3, c: Vec3, color: [f32; 4]| {
        let normal = (b - a).cross(c - a).normalize_or_zero().to_array();
        positions.extend([a.to_array(), b.to_array(), c.to_array()]);
        normals.extend([normal; 3]);
        colors.extend([color; 3]);
    };
    for (i, known) in chunk.cells.iter().enumerate() {
        let KnownTerrain::Known(kind) = known else {
            continue;
        };
        let x = (i % side) as f32;
        let z = -((i / side) as f32);
        let hash = (chunk.coordinate.x() as u64)
            .wrapping_mul(731)
            .wrapping_add(chunk.coordinate.y() as u64)
            .wrapping_mul(157)
            .wrapping_add(i as u64 * 71);
        let variation = (hash % 7) as f32 * 0.005;
        let (h, color) = match kind {
            Terrain::Grass => (
                0.,
                [0.25 + variation, 0.39 + variation, 0.16 + variation, 1.],
            ),
            Terrain::Water => (
                -0.16,
                [0.09 + variation, 0.31 + variation, 0.39 + variation, 1.],
            ),
            Terrain::Rock => (
                0.03,
                [0.39 + variation, 0.40 + variation, 0.34 + variation, 1.],
            ),
        };
        let corners = [
            Vec3::new(x - 0.5, h, z - 0.5),
            Vec3::new(x - 0.5, h, z + 0.5),
            Vec3::new(x + 0.5, h, z + 0.5),
            Vec3::new(x + 0.5, h, z - 0.5),
        ];
        if *kind == Terrain::Rock {
            let center = Vec3::new(x + 0.1, h + 0.18 + (hash % 4) as f32 * 0.06, z - 0.06);
            for j in 0..4 {
                triangle(corners[j], corners[(j + 1) % 4], center, color);
            }
        } else {
            triangle(corners[0], corners[1], corners[2], color);
            triangle(corners[0], corners[2], corners[3], color);
        }
        // Shallow banks and an earth edge hide gaps at water/discovery boundaries.
        // These skirts do not query hidden neighbouring terrain.
        let edge = [color[0] * 0.7, color[1] * 0.7, color[2] * 0.65, 1.];
        for j in 0..4 {
            let column = i % side;
            let row = i / side;
            let neighbour = match j {
                0 if column > 0 => Some(i - 1),
                1 if row > 0 => Some(i - side),
                2 if column + 1 < side => Some(i + 1),
                3 if row + 1 < side => Some(i + side),
                _ => None,
            };
            if neighbour.is_some_and(|n| chunk.cells[n] == *known) {
                continue;
            }
            let a = corners[j];
            let b = corners[(j + 1) % 4];
            let bottom_a = Vec3::new(a.x, -0.28, a.z);
            let bottom_b = Vec3::new(b.x, -0.28, b.z);
            triangle(a, bottom_a, b, edge);
            triangle(b, bottom_a, bottom_b, edge);
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use progressus_app::ChunkCoord;
    #[test]
    fn hidden_cells_produce_no_geometry() {
        let mut chunk = ChunkSnapshot {
            coordinate: ChunkCoord::new(-1, 0),
            side: 2,
            cells: vec![KnownTerrain::Unknown; 4],
        };
        assert_eq!(terrain_mesh(&chunk).count_vertices(), 0);
        chunk.cells[0] = KnownTerrain::Known(Terrain::Grass);
        let grass = terrain_mesh(&chunk).count_vertices();
        assert_eq!(grass, 30);
        chunk.cells[1] = KnownTerrain::Known(Terrain::Rock);
        assert_eq!(terrain_mesh(&chunk).count_vertices(), grass + 36);
    }
}
