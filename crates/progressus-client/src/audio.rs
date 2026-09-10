use crate::{
    ambient_synthesis::WORK_VOICE_GAIN,
    audio_observer::{WorkObserver, audible_cues},
    audio_synthesis::{EffectKind, effect_wav},
    low_poly::{View, space},
    runtime::AuthoritativeClient,
};
use bevy::{audio::Volume, prelude::*};
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
    pub(crate) fn volume(&self, channel: AudioChannel) -> Volume {
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
#[derive(Resource, Default)]
pub(crate) struct SettlementAudio {
    observer: WorkObserver,
    effects: BTreeMap<(EffectKind, u64), Handle<AudioSource>>,
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
    settings: Res<AudioSettings>,
    authoritative: Res<AuthoritativeClient>,
    view: Res<View>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    effects: Query<(Entity, &EffectVoice)>,
) {
    let now = time.elapsed_secs_f64();
    for (entity, voice) in &effects {
        if now >= voice.expires {
            commands.entity(entity).try_despawn();
        }
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
    let (Ok((camera, transform)), Ok(window)) = (cameras.single(), windows.single()) else { return; };
    let admitted = audible_cues(cues, |cell| {
        let p = camera.world_to_viewport(transform, space::cell_local(cell, view.origin)).ok()?;
        Some([p.x / window.width().max(1.), p.y / window.height().max(1.)])
    }, effects.iter().count());
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
        Option<&mut AudioSink>,
        Option<&mut SpatialAudioSink>,
        &mut PlaybackSettings,
    )>,
) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_level_remains_independent_from_music() {
        let mut app = App::new();
        app.init_resource::<AudioSettings>()
            .add_systems(Update, apply_audio_levels);
        let music = app
            .world_mut()
            .spawn((AudioChannel::Music, PlaybackSettings::ONCE))
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
        assert_eq!(volume(&app, music), 0.25);
        assert_eq!(
            volume(&app, effect),
            0.04,
            "work voices reserve mix headroom for eight simultaneous sounds"
        );
        app.world_mut().resource_mut::<AudioSettings>().music = 0;
        app.update();
        assert_eq!(volume(&app, music), 0.0);
        assert_eq!(volume(&app, effect), 0.04);
        app.world_mut().resource_mut::<AudioSettings>().music = 100;
        app.world_mut().resource_mut::<AudioSettings>().effects = 0;
        app.update();
        assert_eq!(volume(&app, music), 1.0);
        assert_eq!(volume(&app, effect), 0.0);
    }
}
