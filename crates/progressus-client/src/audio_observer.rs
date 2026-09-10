//! Disposable observation of physical work; never issues simulation commands.
use crate::audio_synthesis::EffectKind;
use progressus_app::{
    ChunkCoord, ClientSnapshot, EntityId, GroundItemSnapshot, JobKind, JobState, SimulationTick,
    WorldCell,
};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) struct Cue {
    pub kind: EffectKind,
    pub cell: WorldCell,
    pub variant: u64,
}
#[derive(Default)]
pub(crate) struct WorkObserver {
    tick: Option<SimulationTick>,
    jobs: BTreeMap<EntityId, (JobKind, JobState)>,
    carried: BTreeMap<EntityId, EntityId>,
}
impl WorkObserver {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn is_new_tick(&self, snapshot: &ClientSnapshot) -> bool {
        self.tick != Some(snapshot.tick)
    }
    pub fn drop_chunks(&self, snapshot: &ClientSnapshot) -> Vec<ChunkCoord> {
        self.carried
            .iter()
            .filter(|(item, _)| !snapshot.carried_items.iter().any(|i| i.id == **item))
            .filter_map(|(_, worker)| {
                snapshot
                    .characters
                    .iter()
                    .find(|c| c.id == *worker)
                    .map(|c| c.containing_cell.split().0)
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    pub fn observe(
        &mut self,
        snapshot: &ClientSnapshot,
        ground: &[GroundItemSnapshot],
    ) -> Vec<Cue> {
        if !self.is_new_tick(snapshot) {
            return Vec::new();
        }
        let baseline = self.tick.is_none_or(|tick| snapshot.tick < tick);
        let mut cues = Vec::new();
        if !baseline {
            for job in &snapshot.jobs {
                if self.jobs.get(&job.id) == Some(&(job.kind, job.state)) {
                    continue;
                }
                let JobState::Working { worker_id, .. } = job.state else {
                    continue;
                };
                let kind = match job.kind {
                    JobKind::Harvest { .. } => EffectKind::Harvest,
                    JobKind::Construct { .. } => EffectKind::Construct,
                    JobKind::Craft { .. } => EffectKind::Craft,
                    _ => continue,
                };
                if let Some(worker) = snapshot.characters.iter().find(|c| c.id == worker_id) {
                    cues.push(Cue {
                        kind,
                        cell: worker.containing_cell,
                        variant: job.id.value().wrapping_add(snapshot.tick.value()),
                    });
                }
            }
            for item in &snapshot.carried_items {
                if self.carried.get(&item.id) == Some(&item.character_id) {
                    continue;
                }
                if let Some(worker) = snapshot
                    .characters
                    .iter()
                    .find(|c| c.id == item.character_id)
                {
                    cues.push(Cue {
                        kind: EffectKind::Pickup,
                        cell: worker.containing_cell,
                        variant: item.id.value(),
                    });
                }
            }
            for item in ground {
                if self.carried.contains_key(&item.id)
                    && !snapshot.carried_items.iter().any(|c| c.id == item.id)
                {
                    cues.push(Cue {
                        kind: EffectKind::Drop,
                        cell: item.position.containing_cell(),
                        variant: item.id.value(),
                    });
                }
            }
        }
        self.tick = Some(snapshot.tick);
        self.jobs = snapshot
            .jobs
            .iter()
            .map(|j| (j.id, (j.kind, j.state)))
            .collect();
        self.carried = snapshot
            .carried_items
            .iter()
            .map(|i| (i.id, i.character_id))
            .collect();
        cues
    }
}

// Keep the admission rule independently testable without an audio device.
pub(crate) fn audible_cues(
    cues: Vec<Cue>,
    project: impl Fn(WorldCell) -> Option<[f32; 2]>,
    active: usize,
) -> Vec<(Cue, f32)> {
    let budget = 8_usize.saturating_sub(active).min(4);
    cues.into_iter()
        .filter_map(|cue| {
            let [x, y] = project(cue.cell)?;
            if !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
                return None;
            }
            Some((cue, ((x - 0.5) * 1.6).clamp(-0.8, 0.8)))
        })
        .take(budget)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use progressus_app::{
        Application, JobSnapshot, NewGameOptions, SimulationTick, SnapshotQuery, WorldSeed,
    };

    #[test]
    fn pickup_and_drop_require_observed_physical_transitions() {
        use progressus_app::{CarriedItemSnapshot, ItemKind, WorldPosition};
        let app = Application::new_game(NewGameOptions {
            seed: WorldSeed::new(0),
        })
        .unwrap();
        let mut snapshot = app.snapshot(SnapshotQuery::default()).unwrap();
        let mut observer = WorkObserver::default();
        observer.observe(&snapshot, &[]);
        snapshot.tick = SimulationTick::new(1);
        let id = EntityId::new(500).unwrap();
        snapshot.carried_items.push(CarriedItemSnapshot {
            id,
            kind: ItemKind::Wood,
            quantity: 1,
            character_id: snapshot.characters[0].id,
        });
        assert_eq!(observer.observe(&snapshot, &[])[0].kind, EffectKind::Pickup);
        snapshot.tick = SimulationTick::new(2);
        snapshot.carried_items.clear();
        let ground = GroundItemSnapshot {
            id,
            kind: ItemKind::Wood,
            quantity: 1,
            position: WorldPosition::from_cell_center(WorldCell::new(0, 0)).unwrap(),
        };
        assert_eq!(
            observer.observe(&snapshot, &[ground])[0].kind,
            EffectKind::Drop
        );
    }

    #[test]
    fn audio_observation_and_synthesis_preserve_a_real_command_sequence() {
        use crate::{
            ambient_synthesis::{AmbientLayer, layer_wav},
            audio_synthesis::effect_wav,
        };
        use progressus_app::Command;
        let new_game = || {
            Application::new_game(NewGameOptions {
                seed: WorldSeed::new(0),
            })
            .unwrap()
        };
        let mut audible = new_game();
        let mut silent = new_game();
        let mut observer = WorkObserver::default();
        observer.observe(&audible.snapshot(SnapshotQuery::default()).unwrap(), &[]);
        for layer in AmbientLayer::ALL {
            assert!(layer_wav(layer, 1).len() > 44);
        }
        for command in [
            Command::CreateStockpile {
                cell: WorldCell::new(2, 1),
            },
            Command::DesignateHarvest {
                source: WorldCell::new(3, 3),
            },
        ] {
            audible.execute(command).unwrap();
            silent.execute(command).unwrap();
        }
        let mut observed = 0;
        for tick in 0_u32..1024 {
            audible.execute(Command::AdvanceTicks { count: 1 }).unwrap();
            silent.execute(Command::AdvanceTicks { count: 1 }).unwrap();
            let snapshot = audible.snapshot(SnapshotQuery::default()).unwrap();
            for cue in observer.observe(&snapshot, &[]) {
                assert!(effect_wav(cue.kind, cue.variant).len() > 44);
                observed += 1;
            }
            if tick.is_multiple_of(64) {
                assert_eq!(audible.save_json().unwrap(), silent.save_json().unwrap());
            }
        }
        assert!(
            observed > 0,
            "exercise real work audio, not an untouched world"
        );
        assert_eq!(audible.save_json().unwrap(), silent.save_json().unwrap());
    }

    #[test]
    fn offscreen_cues_do_not_take_voice_slots_and_admission_is_bounded() {
        let cues = || {
            (0..20)
                .map(|i| Cue {
                    kind: EffectKind::Craft,
                    cell: WorldCell::new(if i < 10 { 500 } else { 1 }, 0),
                    variant: i,
                })
                .collect()
        };
        let project = |cell: WorldCell| Some([cell.x() as f32 / 100., 0.5]);
        assert_eq!(audible_cues(cues(), project, 0).len(), 4);
        assert_eq!(audible_cues(cues(), project, 7).len(), 1);
        assert!(audible_cues(cues(), project, 8).is_empty());
    }

    #[test]
    fn observation_is_tick_scoped_and_load_resets_even_at_same_tick() {
        let app = Application::new_game(NewGameOptions {
            seed: WorldSeed::new(0),
        })
        .unwrap();
        let mut snapshot = app.snapshot(SnapshotQuery::default()).unwrap();
        let mut observer = WorkObserver::default();
        let id = EntityId::new(300).unwrap();
        let worker = snapshot.characters[0].id;
        snapshot.jobs.push(JobSnapshot {
            id,
            kind: JobKind::Harvest {
                source: WorldCell::new(3, 3),
            },
            state: JobState::Working {
                worker_id: worker,
                remaining_ticks: 4,
            },
        });
        assert!(observer.observe(&snapshot, &[]).is_empty());
        snapshot.tick = SimulationTick::new(1);
        snapshot.jobs[0].state = JobState::Working {
            worker_id: worker,
            remaining_ticks: 3,
        };
        assert_eq!(observer.observe(&snapshot, &[]).len(), 1);
        assert!(observer.observe(&snapshot, &[]).is_empty());
        observer.reset();
        assert!(observer.observe(&snapshot, &[]).is_empty());
        snapshot.tick = SimulationTick::new(2);
        snapshot.jobs.clear();
        assert!(
            observer.observe(&snapshot, &[]).is_empty(),
            "cancellation is not a completion cue"
        );
    }
}
