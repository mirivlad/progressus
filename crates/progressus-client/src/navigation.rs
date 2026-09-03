use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::Resource;
use progressus_app::{EntityId, SUBUNITS_PER_CELL, SimulationTick, WorldPosition};

pub(crate) const CELL_SIZE: f32 = 12.0;

#[derive(Resource, Default)]
pub(crate) struct SelectedCharacter(pub(crate) Option<EntityId>);

#[derive(Clone, Debug)]
pub(crate) struct CharacterVisualMotion {
    pub(crate) source_tick: SimulationTick,
    pub(crate) trace: Vec<WorldPosition>,
    pub(crate) elapsed_seconds: f32,
}

#[derive(Resource, Default)]
pub(crate) struct VisualMotion {
    pub(crate) characters: BTreeMap<EntityId, CharacterVisualMotion>,
}

impl VisualMotion {
    pub(crate) fn replace(
        &mut self,
        character_id: EntityId,
        source_tick: SimulationTick,
        trace: Vec<WorldPosition>,
    ) {
        if self
            .characters
            .get(&character_id)
            .is_some_and(|motion| motion.source_tick == source_tick)
        {
            return;
        }
        self.characters.insert(
            character_id,
            CharacterVisualMotion {
                source_tick,
                trace,
                elapsed_seconds: 0.0,
            },
        );
    }

    pub(crate) fn retain(&mut self, character_ids: impl IntoIterator<Item = EntityId>) {
        let retained = character_ids.into_iter().collect::<BTreeSet<_>>();
        self.characters.retain(|id, _| retained.contains(id));
    }

    pub(crate) fn clear(&mut self) {
        self.characters.clear();
    }
}

pub(crate) fn select_nearest(
    candidates: impl IntoIterator<Item = (EntityId, WorldPosition)>,
    target: WorldPosition,
    hit_radius_subunits: i128,
) -> Option<EntityId> {
    candidates
        .into_iter()
        .filter_map(|(id, position)| {
            let dx = position.x_subunits() - target.x_subunits();
            let dy = position.y_subunits() - target.y_subunits();
            let distance_squared = dx * dx + dy * dy;
            (distance_squared <= hit_radius_subunits * hit_radius_subunits)
                .then_some((distance_squared, id))
        })
        .min_by_key(|(distance_squared, id)| (*distance_squared, *id))
        .map(|(_, id)| id)
}

pub(crate) fn quantize_local_click(
    origin: WorldPosition,
    local_x: f32,
    local_y: f32,
) -> Result<WorldPosition, ()> {
    let delta_x = (local_x / CELL_SIZE * SUBUNITS_PER_CELL as f32).round() as i128;
    let delta_y = (local_y / CELL_SIZE * SUBUNITS_PER_CELL as f32).round() as i128;
    WorldPosition::from_subunits(
        origin.x_subunits().checked_add(delta_x).ok_or(())?,
        origin.y_subunits().checked_add(delta_y).ok_or(())?,
    )
    .map_err(|_| ())
}

pub(crate) fn interpolate_trace(trace: &[WorldPosition], fraction: f32) -> WorldPosition {
    let fraction = fraction.clamp(0.0, 1.0);
    let lengths = trace
        .windows(2)
        .map(|pair| {
            (pair[1].x_subunits() - pair[0].x_subunits()).abs()
                + (pair[1].y_subunits() - pair[0].y_subunits()).abs()
        })
        .collect::<Vec<_>>();
    let total = lengths.iter().sum::<i128>();
    if trace.len() <= 1 || total == 0 {
        return *trace.last().expect("trace is nonempty");
    }
    let mut remaining = (total as f32 * fraction).round() as i128;
    for (pair, length) in trace.windows(2).zip(lengths) {
        if remaining <= length {
            let dx = pair[1].x_subunits() - pair[0].x_subunits();
            let dy = pair[1].y_subunits() - pair[0].y_subunits();
            return pair[0]
                .checked_translate(dx.signum() * remaining, dy.signum() * remaining)
                .expect("trace endpoints are valid");
        }
        remaining -= length;
    }
    *trace.last().expect("trace is nonempty")
}

#[cfg(test)]
mod tests {
    use progressus_app::{EntityId, SimulationTick, WorldCell, WorldPosition};

    use super::{VisualMotion, interpolate_trace, quantize_local_click, select_nearest};

    #[test]
    fn nearest_selection_breaks_equal_distance_by_entity_id() {
        let origin = WorldPosition::from_cell_center(WorldCell::new(0, 0)).unwrap();
        assert_eq!(
            select_nearest(
                [
                    (
                        EntityId::new(4).unwrap(),
                        origin.checked_translate(10, 0).unwrap()
                    ),
                    (
                        EntityId::new(3).unwrap(),
                        origin.checked_translate(-10, 0).unwrap()
                    ),
                ],
                origin,
                32,
            ),
            Some(EntityId::new(3).unwrap())
        );
    }

    #[test]
    fn local_click_quantizes_center_negative_and_boundary() {
        let origin = WorldPosition::from_cell_center(WorldCell::new(-1, 0)).unwrap();
        assert_eq!(quantize_local_click(origin, 0.0, 0.0).unwrap(), origin);
        assert_eq!(
            quantize_local_click(origin, 3.0, 0.0).unwrap().x_subunits(),
            -256
        );
        assert_eq!(
            quantize_local_click(origin, 6.0, 0.0)
                .unwrap()
                .containing_cell(),
            WorldCell::new(0, 0)
        );
    }

    #[test]
    fn trace_interpolation_visits_corner_instead_of_chord() {
        let trace = [
            WorldPosition::from_subunits(0, 0).unwrap(),
            WorldPosition::from_subunits(10, 0).unwrap(),
            WorldPosition::from_subunits(10, 10).unwrap(),
        ];
        assert_eq!(interpolate_trace(&trace, 0.5), trace[1]);
    }

    #[test]
    fn identical_authoritative_tick_does_not_restart_visual_motion() {
        let id = EntityId::new(3).unwrap();
        let trace = vec![
            WorldPosition::from_subunits(0, 0).unwrap(),
            WorldPosition::from_subunits(100, 0).unwrap(),
        ];
        let mut motion = VisualMotion::default();
        motion.replace(id, SimulationTick::new(8), trace.clone());
        motion.characters.get_mut(&id).unwrap().elapsed_seconds = 0.125;

        motion.replace(id, SimulationTick::new(8), trace);

        let character = &motion.characters[&id];
        assert_eq!(character.elapsed_seconds, 0.125);
        assert_eq!(character.source_tick, SimulationTick::new(8));
    }

    #[test]
    fn visual_motion_tracks_multiple_characters_independently() {
        let first = EntityId::new(1).unwrap();
        let second = EntityId::new(2).unwrap();
        let mut motion = VisualMotion::default();
        motion.replace(
            first,
            SimulationTick::new(9),
            vec![WorldPosition::from_subunits(0, 0).unwrap()],
        );
        motion.replace(
            second,
            SimulationTick::new(9),
            vec![WorldPosition::from_subunits(100, 0).unwrap()],
        );

        assert_eq!(motion.characters.len(), 2);
        motion.characters.get_mut(&first).unwrap().elapsed_seconds = 0.1;
        assert_eq!(motion.characters[&first].elapsed_seconds, 0.1);
        assert_eq!(motion.characters[&second].elapsed_seconds, 0.0);

        motion.retain([second]);
        assert!(!motion.characters.contains_key(&first));
        assert!(motion.characters.contains_key(&second));
    }
}
