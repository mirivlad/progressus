use crate::{
    audio_observer::{MusicSchedule, WorkObserver, audible_cues},
    audio_synthesis::{EffectKind, effect_wav, music_wav},
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
            }) / 100.0,
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
struct MusicVoice {
    entity: Entity,
    asset: Handle<AudioSource>,
    requested: f64,
    started: bool,
}
#[derive(Resource, Default)]
pub(crate) struct SettlementAudio {
    observer: WorkObserver,
    schedule: MusicSchedule,
    task: Option<Task<Vec<u8>>>,
    next: Option<Handle<AudioSource>>,
    index: u64,
    voice: Option<MusicVoice>,
    effects: BTreeMap<(EffectKind, u64), Handle<AudioSource>>,
    disabled: bool,
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
    state.task = Some(AsyncComputeTaskPool::get().spawn(async { music_wav(0) }));
    state.index = 1;
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
    if let Some(voice) = state.voice.as_mut() {
        if let Ok(sink) = sinks.get(voice.entity) {
            voice.started = true;
            if sink.empty() {
                let voice = state.voice.take().expect("music voice exists");
                commands.entity(voice.entity).try_despawn();
                assets.remove(voice.asset.id());
                state.schedule.finished(now);
            }
        } else if !voice.started && now - voice.requested > 5.0 {
            warn!("Audio playback unavailable; continuing silently");
            let voice = state.voice.take().expect("music voice exists");
            commands.entity(voice.entity).try_despawn();
            assets.remove(voice.asset.id());
            if let Some(next) = state.next.take() {
                assets.remove(next.id());
            }
            state.task = None;
            state.disabled = true;
            return;
        }
    }
    if let Some(task) = state.task.as_mut()
        && let Some(bytes) = block_on(poll_once(task))
    {
        state.next = Some(assets.add(AudioSource {
            bytes: bytes.into(),
        }));
        state.task = None;
    }
    if state.voice.is_none()
        && state.schedule.ready(now)
        && let Some(asset) = state.next.take()
    {
        let entity = commands
            .spawn((
                AudioPlayer::new(asset.clone()),
                PlaybackSettings::ONCE.with_volume(settings.volume(AudioChannel::Music)),
                AudioChannel::Music,
            ))
            .id();
        state.voice = Some(MusicVoice {
            entity,
            asset,
            requested: now,
            started: false,
        });
        state.schedule.started();
    }
    if state.task.is_none() && state.next.is_none() {
        let index = state.index;
        state.index = state.index.wrapping_add(1);
        state.task = Some(AsyncComputeTaskPool::get().spawn(async move { music_wav(index) }));
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

pub(crate) fn apply_audio_levels(
    settings: Res<AudioSettings>,
    mut voices: Query<(
        &AudioChannel,
        Option<&mut AudioSink>,
        Option<&mut SpatialAudioSink>,
        &mut PlaybackSettings,
    )>,
) {
    if !settings.is_changed() {
        return;
    }
    for (channel, sink, spatial, mut playback) in &mut voices {
        let volume = settings.volume(*channel);
        playback.volume = volume;
        if let Some(mut sink) = sink {
            sink.set_volume(volume);
        }
        if let Some(mut sink) = spatial {
            sink.set_volume(volume);
        }
    }
}
