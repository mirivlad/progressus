use bevy::prelude::*;
use progressus_app::{ChunkCoord, SUBUNITS_PER_CELL, WorldCell, WorldPosition};

// Local X points east, local -Z points north; Y never enters authority.
pub(crate) fn local(position: WorldPosition, origin: WorldCell) -> Vec3 {
    let origin = WorldPosition::from_cell_center(origin).expect("valid cell center");
    Vec3::new(
        (position.x_subunits() - origin.x_subunits()) as f32 / SUBUNITS_PER_CELL as f32,
        0.0,
        -(position.y_subunits() - origin.y_subunits()) as f32 / SUBUNITS_PER_CELL as f32,
    )
}

pub(crate) fn cell_local(cell: WorldCell, origin: WorldCell) -> Vec3 {
    Vec3::new(
        (i128::from(cell.x()) - i128::from(origin.x())) as f32,
        0.0,
        -(i128::from(cell.y()) - i128::from(origin.y())) as f32,
    )
}

pub(crate) fn position(point: Vec3, origin: WorldCell) -> Option<WorldPosition> {
    if !point.is_finite() {
        return None;
    }
    let origin = WorldPosition::from_cell_center(origin).ok()?;
    origin
        .checked_translate(
            (point.x * SUBUNITS_PER_CELL as f32).round() as i128,
            (-point.z * SUBUNITS_PER_CELL as f32).round() as i128,
        )
        .ok()
}

pub(crate) fn ground(ray: Ray3d) -> Option<Vec3> {
    ray.plane_intersection_point(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))
}

pub(crate) fn visible_chunks(points: &[Vec3], origin: WorldCell) -> Vec<ChunkCoord> {
    let cells: Vec<_> = points
        .iter()
        .filter_map(|&p| position(p, origin))
        .map(|p| p.containing_cell())
        .collect();
    let Some(first) = cells.first() else {
        return Vec::new();
    };
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (first.x(), first.x(), first.y(), first.y());
    for c in cells {
        min_x = min_x.min(c.x());
        max_x = max_x.max(c.x());
        min_y = min_y.min(c.y());
        max_y = max_y.max(c.y());
    }
    let min = WorldCell::new(min_x.saturating_sub(3), min_y.saturating_sub(3))
        .split()
        .0;
    let max = WorldCell::new(max_x.saturating_add(3), max_y.saturating_add(3))
        .split()
        .0;
    // Zoom is bounded; fail closed if a malformed projection requests a vast world.
    if i128::from(max.x()) - i128::from(min.x()) > 12
        || i128::from(max.y()) - i128::from(min.y()) > 12
    {
        return Vec::new();
    }
    (min.y()..=max.y())
        .flat_map(|y| (min.x()..=max.x()).map(move |x| ChunkCoord::new(x, y)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn far_signed_coordinates_roundtrip_before_float_conversion() {
        for base in [0, -33, 9_000_000_000_000_000, -9_000_000_000_000_000] {
            let origin = WorldCell::new(base, -base);
            let p = WorldPosition::from_cell_center(origin)
                .unwrap()
                .checked_translate(-1300, 777)
                .unwrap();
            assert_eq!(position(local(p, origin), origin), Some(p));
        }
        assert!(position(Vec3::NAN, WorldCell::new(0, 0)).is_none());
    }
    #[test]
    fn ground_ray_ignores_presentation_height_and_rejects_horizon() {
        assert_eq!(
            ground(Ray3d::new(Vec3::new(2., 8., -3.), Dir3::NEG_Y)),
            Some(Vec3::new(2., 0., -3.))
        );
        assert!(ground(Ray3d::new(Vec3::Y, Dir3::X)).is_none());
    }
    #[test]
    fn viewport_crosses_negative_chunk_boundary_without_discovery() {
        let chunks = visible_chunks(
            &[Vec3::new(-2., 0., 2.), Vec3::new(2., 0., -2.)],
            WorldCell::new(0, 0),
        );
        assert_eq!(chunks.len(), 4);
        assert!(chunks.contains(&ChunkCoord::new(-1, -1)));
    }
}
