use crate::{
    ambient_playback::PlaybackTiming,
    ambient_synthesis::{AmbientLayer, WORK_VOICE_GAIN, layer_wav},
    audio_observer::{WorkObserver, audible_cues},
    audio_synthesis::{EffectKind, effect_wav},
    render::PresentationCache,
    runtime::AuthoritativeClient,
};
use bevy::{
    audio::Volume,
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, block_on, poll_once},
};
use std::collections::BTreeMap;

#[derive(Resource)]
pub(crate) struct AudioSettings {
    pub music: u8,
    pub effects: u8,
}
impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            music: 25,
            effects: 50,
        }
    }
}
#[derive(Component, Clone, Copy)]
pub(crate) enum AudioChannel {
    Music,
    Effects,
}
impl AudioSettings {
    fn volume(&self, channel: AudioChannel) -> Volume {
        Volume::Linear(
            f32::from(match channel {
                AudioChannel::Music => self.music,
                AudioChannel::Effects => self.effects,
            }) / 100.0
                * match channel {
                    AudioChannel::Music => 1.0,
                    AudioChannel::Effects => WORK_VOICE_GAIN,
                },
        )
    }
    pub fn adjust(&mut self, channel: AudioChannel, delta: i16) {
        let v = match channel {
            AudioChannel::Music => &mut self.music,
            AudioChannel::Effects => &mut self.effects,
        };
        *v = (i16::from(*v) + delta).clamp(0, 100) as u8;
    }
}
#[derive(Component)]
pub(crate) struct SoundMenuButton;
#[derive(Component)]
pub(crate) struct EffectVoice {
    expires: f64,
}
#[derive(Component)]
pub(crate) struct AmbientGain(f32);

struct AmbientVoice {
    entity: Entity,
    asset: Handle<AudioSource>,
    timing: PlaybackTiming,
}
struct LayerPlayer {
    layer: AmbientLayer,
    task: Option<Task<Vec<u8>>>,
    next: Option<Handle<AudioSource>>,
    index: u64,
    current: Option<AmbientVoice>,
    outgoing: Option<AmbientVoice>,
}
impl LayerPlayer {
    fn new(layer: AmbientLayer) -> Self {
        Self {
            layer,
            task: None,
            next: None,
            index: 0,
            current: None,
            outgoing: None,
        }
    }

    fn clear(&mut self, commands: &mut Commands, assets: &mut Assets<AudioSource>) {
        for voice in [self.current.take(), self.outgoing.take()]
            .into_iter()
            .flatten()
        {
            retire_voice(voice, commands, assets);
        }
        if let Some(asset) = self.next.take() {
            assets.remove(asset.id());
        }
        self.task = None;
    }

    fn update(
        &mut self,
        now: f64,
        commands: &mut Commands,
        assets: &mut Assets<AudioSource>,
        sinks: &Query<&AudioSink>,
    ) -> bool {
        if let Some(voice) = &mut self.current {
            if let Ok(sink) = sinks.get(voice.entity) {
                voice.timing.observe_started(now);
                if sink.empty() {
                    retire_voice(
                        self.current.take().expect("current exists"),
                        commands,
                        assets,
                    );
                    self.current = self.outgoing.take();
                }
            } else if voice.timing.unavailable(now) {
                return false;
            }
        }
        if let Some(voice) = &self.current {
            let gain = voice.timing.gain(now);
            commands.entity(voice.entity).insert(AmbientGain(gain));
            if let Some(outgoing) = &self.outgoing {
                commands
                    .entity(outgoing.entity)
                    .insert(AmbientGain(1.0 - gain));
            }
            if gain >= 1.0
                && let Some(outgoing) = self.outgoing.take()
            {
                retire_voice(outgoing, commands, assets);
            }
        }
        if let Some(task) = &mut self.task
            && let Some(bytes) = block_on(poll_once(task))
        {
            self.next = Some(assets.add(AudioSource {
                bytes: bytes.into(),
            }));
            self.task = None;
        }
        let replace = self.current.as_ref().is_some_and(|voice| {
            voice.timing.replacement_due(
                now,
                self.layer.seconds(),
                self.layer.looping(),
                self.next.is_some(),
                self.outgoing.is_some(),
            )
        });
        if (self.current.is_none() || replace)
            && let Some(asset) = self.next.take()
        {
            self.outgoing = self.current.take();
            let playback = if self.layer.looping() {
                PlaybackSettings::LOOP
            } else {
                PlaybackSettings::ONCE
            };
            let entity = commands
                .spawn((
                    AudioPlayer::new(asset.clone()),
                    playback.with_volume(Volume::SILENT),
                    AudioChannel::Music,
                    AmbientGain(0.0),
                ))
                .id();
            self.current = Some(AmbientVoice {
                entity,
                asset,
                timing: PlaybackTiming::new(now),
            });
            debug!(?self.layer, index = self.index.saturating_sub(1), "Starting ambient layer");
        }
        if self.task.is_none() && self.next.is_none() {
            let layer = self.layer;
            let index = self.index;
            self.index = self.index.wrapping_add(1);
            self.task = Some(AsyncComputeTaskPool::get().spawn(async move {
                let started = std::time::Instant::now();
                let bytes = layer_wav(layer, index);
                debug!(
                    ?layer,
                    index,
                    generation_ms = started.elapsed().as_secs_f64() * 1000.0,
                    bytes = bytes.len(),
                    "Generated ambient layer"
                );
                bytes
            }));
        }
        true
    }
}

fn retire_voice(voice: AmbientVoice, commands: &mut Commands, assets: &mut Assets<AudioSource>) {
    commands.entity(voice.entity).try_despawn();
    assets.remove(voice.asset.id());
}

#[derive(Resource)]
pub(crate) struct SettlementAudio {
    observer: WorkObserver,
    layers: [LayerPlayer; 3],
    effects: BTreeMap<(EffectKind, u64), Handle<AudioSource>>,
    disabled: bool,
    last_diagnostic: f64,
}
impl Default for SettlementAudio {
    fn default() -> Self {
        Self {
            observer: WorkObserver::default(),
            layers: AmbientLayer::ALL.map(LayerPlayer::new),
            effects: BTreeMap::new(),
            disabled: false,
            last_diagnostic: 0.0,
        }
    }
}
impl SettlementAudio {
    pub fn reset_observer(&mut self) {
        self.observer.reset();
    }
}

pub(crate) fn setup_audio(
    mut commands: Commands,
    mut state: ResMut<SettlementAudio>,
    mut assets: ResMut<Assets<AudioSource>>,
) {
    commands.spawn((SpatialListener::new(2.0), Transform::default()));
    for kind in EffectKind::ALL {
        debug!("Preparing {} sound variants", kind.name());
        for variant in 0..3 {
            state.effects.insert(
                (kind, variant),
                assets.add(AudioSource {
                    bytes: effect_wav(kind, variant).into(),
                }),
            );
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(crate) fn update_audio(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut state: ResMut<SettlementAudio>,
    mut assets: ResMut<Assets<AudioSource>>,
    settings: Res<AudioSettings>,
    authoritative: Res<AuthoritativeClient>,
    cache: Res<PresentationCache>,
    cameras: Query<(&Transform, &Projection), With<Camera2d>>,
    sinks: Query<&AudioSink>,
    effects: Query<(Entity, &EffectVoice)>,
) {
    let now = time.elapsed_secs_f64();
    for (entity, voice) in &effects {
        if now >= voice.expires {
            commands.entity(entity).try_despawn();
        }
    }
    if state.disabled {
        return;
    }
    for layer in &mut state.layers {
        if !layer.update(now, &mut commands, &mut assets, &sinks) {
            warn!("Audio playback unavailable; continuing silently");
            for layer in &mut state.layers {
                layer.clear(&mut commands, &mut assets);
            }
            for (entity, _) in &effects {
                commands.entity(entity).try_despawn();
            }
            state.disabled = true;
            return;
        }
    }
    if now - state.last_diagnostic >= 15.0 {
        state.last_diagnostic = now;
        let ambient_voices: usize = state
            .layers
            .iter()
            .map(|l| usize::from(l.current.is_some()) + usize::from(l.outgoing.is_some()))
            .sum();
        let pending_tasks = state.layers.iter().filter(|l| l.task.is_some()).count();
        debug!(
            ambient_voices,
            pending_tasks,
            audio_assets = assets.len(),
            effect_voices = effects.iter().count(),
            "Ambient resource counts"
        );
    }
    let snapshot = authoritative.snapshot();
    if !state.observer.is_new_tick(snapshot) {
        return;
    }
    let chunks = state.observer.drop_chunks(snapshot);
    let ground = if chunks.is_empty() {
        Vec::new()
    } else {
        match authoritative.spatial_snapshot(chunks, false, true, false) {
            Ok(snapshot) => snapshot.ground_items,
            Err(error) => {
                warn!("audio drop observation unavailable: {error}");
                Vec::new()
            }
        }
    };
    let cues = state.observer.observe(snapshot, &ground);
    if settings.effects == 0 {
        return;
    }
    let (Some(origin), Ok((camera, Projection::Orthographic(projection)))) =
        (cache.render_origin, cameras.single())
    else {
        return;
    };
    let admitted = audible_cues(
        cues,
        origin,
        [camera.translation.x, camera.translation.y],
        [
            projection.area.min.x,
            projection.area.min.y,
            projection.area.max.x,
            projection.area.max.y,
        ],
        effects.iter().count(),
    );
    for (cue, pan) in admitted {
        let Some(asset) = state.effects.get(&(cue.kind, cue.variant % 3)) else {
            continue;
        };
        commands.spawn((
            AudioPlayer::new(asset.clone()),
            PlaybackSettings::DESPAWN
                .with_volume(settings.volume(AudioChannel::Effects))
                .with_spatial(true),
            Transform::from_xyz(pan, 0.0, 0.0),
            AudioChannel::Effects,
            EffectVoice { expires: now + 2.0 },
        ));
    }
}

#[allow(clippy::type_complexity)]
pub(crate) fn apply_audio_levels(
    settings: Res<AudioSettings>,
    mut voices: Query<(
        &AudioChannel,
        Option<&AmbientGain>,
        Option<&mut AudioSink>,
        Option<&mut SpatialAudioSink>,
        &mut PlaybackSettings,
    )>,
) {
    for (channel, gain, sink, spatial, mut playback) in &mut voices {
        let volume =
            Volume::Linear(settings.volume(*channel).to_linear() * gain.map_or(1.0, |g| g.0));
        playback.volume = volume;
        if let Some(mut sink) = sink {
            sink.set_volume(volume);
        }
        if let Some(mut sink) = spatial {
            sink.set_volume(volume);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn repeated_transitions_keep_resources_bounded_when_worker_and_sink_are_late() {
        AsyncComputeTaskPool::get_or_init(bevy::tasks::TaskPool::new);
        let mut state = SettlementAudio::default();
        // An intentionally stalled worker exercises the real update system's
        // not-ready path; completed PCM is supplied explicitly below.
        for layer in &mut state.layers {
            layer.task = Some(AsyncComputeTaskPool::get().spawn(std::future::pending()));
        }
        let mut app = App::new();
        app.insert_resource(state)
            .init_resource::<Time<Real>>()
            .init_resource::<Assets<AudioSource>>()
            .init_resource::<AudioSettings>()
            .init_resource::<PresentationCache>()
            .insert_resource(AuthoritativeClient::new().unwrap())
            .add_systems(Update, (update_audio, apply_audio_levels).chain());
        let supply = |app: &mut App| {
            for index in [0, 2] {
                let asset =
                    app.world_mut()
                        .resource_mut::<Assets<AudioSource>>()
                        .add(AudioSource {
                            bytes: vec![0; 44].into(),
                        });
                app.world_mut().resource_mut::<SettlementAudio>().layers[index].next = Some(asset);
            }
        };
        let step = |app: &mut App, seconds: f64| {
            app.world_mut()
                .resource_mut::<Time<Real>>()
                .advance_by(Duration::from_secs_f64(seconds));
            app.update();
        };
        let ready = |app: &mut App| {
            let now = app.world().resource::<Time<Real>>().elapsed_secs_f64();
            for layer in &mut app.world_mut().resource_mut::<SettlementAudio>().layers {
                if let Some(current) = &mut layer.current {
                    // Device attachment is external; record its observed start
                    // while retaining real entities, assets, tasks and systems.
                    current.timing.observe_started(now);
                }
            }
        };
        supply(&mut app);
        app.update();
        ready(&mut app);
        step(&mut app, 6.0);
        for _ in 0..40 {
            let previous = [0, 2].map(|index| {
                app.world().resource::<SettlementAudio>().layers[index]
                    .current
                    .as_ref()
                    .unwrap()
                    .entity
            });
            step(&mut app, 100.0);
            for (index, entity) in [0, 2].into_iter().zip(previous) {
                let layer = &app.world().resource::<SettlementAudio>().layers[index];
                assert_eq!(layer.current.as_ref().unwrap().entity, entity);
                assert!(layer.outgoing.is_none());
                assert_eq!(app.world().get::<AmbientGain>(entity).unwrap().0, 1.0);
            }
            supply(&mut app);
            app.update();
            step(&mut app, 1.0);
            for entity in previous {
                assert_eq!(
                    app.world().get::<AmbientGain>(entity).unwrap().0,
                    1.0,
                    "old layer must remain audible before new sink starts"
                );
            }
            assert_eq!(app.world().resource::<Assets<AudioSource>>().len(), 4);
            ready(&mut app);
            step(&mut app, 3.0);
            for entity in previous {
                assert_eq!(app.world().get::<AmbientGain>(entity).unwrap().0, 0.5);
            }
            step(&mut app, 3.0);
            assert!(
                previous
                    .iter()
                    .all(|id| app.world().get_entity(*id).is_err())
            );
            assert_eq!(app.world().resource::<Assets<AudioSource>>().len(), 2);
            let world = app.world_mut();
            assert_eq!(world.query::<&AudioPlayer>().iter(world).count(), 2);
            assert!(!world.resource::<SettlementAudio>().disabled);
        }
    }

    #[test]
    fn independent_levels_preserve_crossfade_and_apply_without_settings_change() {
        let mut app = App::new();
        app.init_resource::<AudioSettings>()
            .add_systems(Update, apply_audio_levels);
        let music = app
            .world_mut()
            .spawn((
                AudioChannel::Music,
                AmbientGain(0.5),
                PlaybackSettings::LOOP,
            ))
            .id();
        let effect = app
            .world_mut()
            .spawn((AudioChannel::Effects, PlaybackSettings::ONCE))
            .id();
        app.update();
        let volume = |app: &App, entity| {
            app.world()
                .get::<PlaybackSettings>(entity)
                .unwrap()
                .volume
                .to_linear()
        };
        assert_eq!(volume(&app, music), 0.125);
        assert_eq!(
            volume(&app, effect),
            0.04,
            "work voices reserve mix headroom for eight simultaneous sounds"
        );
        app.world_mut().get_mut::<AmbientGain>(music).unwrap().0 = 0.75;
        app.update();
        assert_eq!(volume(&app, music), 0.1875);
        app.world_mut().resource_mut::<AudioSettings>().music = 0;
        app.update();
        assert_eq!(volume(&app, music), 0.0);
        assert_eq!(volume(&app, effect), 0.04);
        app.world_mut().resource_mut::<AudioSettings>().music = 100;
        app.world_mut().resource_mut::<AudioSettings>().effects = 0;
        app.update();
        assert_eq!(volume(&app, music), 0.75);
        assert_eq!(volume(&app, effect), 0.0);
    }

    #[test]
    fn unavailable_output_releases_all_pending_and_overlapping_ambient_assets() {
        let mut app = App::new();
        let mut voice_entities = Vec::new();
        let mut assets = Assets::<AudioSource>::default();
        let mut state = SettlementAudio::default();
        for layer in &mut state.layers {
            let mut voice = || AmbientVoice {
                entity: app.world_mut().spawn_empty().id(),
                asset: assets.add(AudioSource {
                    bytes: vec![0; 44].into(),
                }),
                timing: PlaybackTiming::new(0.0),
            };
            layer.current = Some(voice());
            layer.outgoing = Some(voice());
            voice_entities.push(layer.current.as_ref().unwrap().entity);
            voice_entities.push(layer.outgoing.as_ref().unwrap().entity);
            layer.next = Some(assets.add(AudioSource {
                bytes: vec![0; 44].into(),
            }));
        }
        assert_eq!(assets.len(), 9);
        let effect = app.world_mut().spawn(EffectVoice { expires: 20.0 }).id();
        let mut clock = Time::<Real>::default();
        clock.advance_by(Duration::from_secs(6));
        let authoritative = AuthoritativeClient::new().unwrap();
        let save = authoritative.save_json().unwrap();
        app.insert_resource(clock)
            .insert_resource(state)
            .insert_resource(assets)
            .insert_resource(authoritative)
            .init_resource::<AudioSettings>()
            .init_resource::<PresentationCache>()
            .add_systems(Update, update_audio);
        app.update();
        let state = app.world().resource::<SettlementAudio>();
        assert!(state.disabled);
        assert!(state.layers.iter().all(|layer| layer.current.is_none()
            && layer.outgoing.is_none()
            && layer.next.is_none()
            && layer.task.is_none()));
        assert!(app.world().resource::<Assets<AudioSource>>().is_empty());
        assert!(app.world().get_entity(effect).is_err());
        assert!(
            voice_entities
                .iter()
                .all(|id| app.world().get_entity(*id).is_err())
        );
        app.update();
        assert!(
            voice_entities
                .iter()
                .all(|id| app.world().get_entity(*id).is_err())
        );
        assert_eq!(
            app.world()
                .resource::<AuthoritativeClient>()
                .save_json()
                .unwrap(),
            save
        );
    }
}
