use crate::{
    audio::{AudioChannel, AudioSettings},
    score_audition::{SAMPLE_RATE, continuous_pcm_window},
};
use bevy::{
    audio::{AddAudioSource, AudioSink, AudioSinkPlayback, Decodable, Source, Volume},
    prelude::*,
};
use std::{
    sync::{
        Arc, Mutex,
        mpsc::{Receiver, SyncSender, TryRecvError, sync_channel},
    },
    time::Duration,
};

const CHANNELS: u16 = 2;
const CHUNK_SECONDS: usize = 12;
const BUFFERED_CHUNKS: usize = 2;
const OUTPUT_ATTACH_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Asset, TypePath)]
pub(crate) struct ContinuousMusic {
    receiver: Arc<Mutex<Option<Receiver<Vec<f32>>>>>,
    initial: Vec<f32>,
}

pub(crate) struct ContinuousMusicDecoder {
    receiver: Receiver<Vec<f32>>,
    samples: std::vec::IntoIter<f32>,
    skipped_samples: usize,
}

impl Iterator for ContinuousMusicDecoder {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(sample) = self.samples.next() {
                return Some(sample);
            }
            match self.receiver.try_recv() {
                Ok(mut samples) => {
                    let skip = self.skipped_samples.min(samples.len());
                    samples.drain(..skip);
                    self.skipped_samples -= skip;
                    self.samples = samples.into_iter();
                }
                Err(TryRecvError::Empty) => {
                    self.skipped_samples = self.skipped_samples.saturating_add(1);
                    return Some(0.0);
                }
                Err(TryRecvError::Disconnected) => {
                    return None;
                }
            }
        }
    }
}

impl Source for ContinuousMusicDecoder {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        CHANNELS
    }
    fn sample_rate(&self) -> u32 {
        SAMPLE_RATE
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

impl Decodable for ContinuousMusic {
    type DecoderItem = f32;
    type Decoder = ContinuousMusicDecoder;

    fn decoder(&self) -> Self::Decoder {
        let receiver = self
            .receiver
            .lock()
            .expect("music receiver mutex poisoned")
            .take()
            .expect("continuous music source may only be decoded once");
        ContinuousMusicDecoder {
            receiver,
            samples: self.initial.clone().into_iter(),
            skipped_samples: 0,
        }
    }
}

#[derive(Resource, Default)]
struct ContinuousMusicState {
    entity: Option<Entity>,
    asset: Option<Handle<ContinuousMusic>>,
    elapsed_without_sink: Duration,
    disabled: bool,
    pending: Option<Mutex<Receiver<Vec<f32>>>>,
}

#[derive(Default)]
pub(crate) struct ContinuousMusicPlugin;

impl Plugin for ContinuousMusicPlugin {
    fn build(&self, app: &mut App) {
        app.add_audio_source::<ContinuousMusic>()
            .init_resource::<ContinuousMusicState>()
            .add_systems(Startup, start_continuous_music)
            .add_systems(
                Update,
                (
                    attach_prebuffered_music,
                    monitor_continuous_music,
                    apply_music_volume,
                )
                    .chain(),
            );
    }
}

fn start_continuous_music(mut state: ResMut<ContinuousMusicState>) {
    let (sender, receiver) = sync_channel(BUFFERED_CHUNKS);
    std::thread::Builder::new()
        .name("progressus-music".into())
        .spawn(move || render_chunks(sender, 0x5052_4f47_5245_5353))
        .expect("failed to start continuous music worker");
    state.pending = Some(Mutex::new(receiver));
}

fn attach_prebuffered_music(
    mut commands: Commands,
    mut sources: ResMut<Assets<ContinuousMusic>>,
    mut state: ResMut<ContinuousMusicState>,
) {
    let Some(receiver) = state.pending.take() else {
        return;
    };
    let receiver = receiver
        .into_inner()
        .expect("pending receiver mutex poisoned");
    let initial = match receiver.try_recv() {
        Ok(initial) => initial,
        Err(TryRecvError::Empty) => {
            state.pending = Some(Mutex::new(receiver));
            return;
        }
        Err(TryRecvError::Disconnected) => {
            state.disabled = true;
            warn!("Continuous music worker stopped before prebuffering");
            return;
        }
    };
    let asset = sources.add(ContinuousMusic {
        receiver: Arc::new(Mutex::new(Some(receiver))),
        initial,
    });
    let entity = commands
        .spawn((
            AudioPlayer(asset.clone()),
            PlaybackSettings::ONCE.with_volume(Volume::SILENT),
            AudioChannel::Music,
        ))
        .id();
    state.entity = Some(entity);
    state.asset = Some(asset);
}

fn render_chunks(sender: SyncSender<Vec<f32>>, seed: u64) {
    for chunk in 0_u64.. {
        let start = chunk as usize * CHUNK_SECONDS;
        let samples = continuous_pcm_window(seed, start, CHUNK_SECONDS);
        if sender.send(samples).is_err() {
            break;
        }
    }
}

fn monitor_continuous_music(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut sources: ResMut<Assets<ContinuousMusic>>,
    mut state: ResMut<ContinuousMusicState>,
    sinks: Query<&AudioSink>,
) {
    if state.disabled {
        return;
    }
    let Some(entity) = state.entity else {
        return;
    };
    if sinks.get(entity).is_ok() {
        state.elapsed_without_sink = Duration::ZERO;
        return;
    }
    state.elapsed_without_sink += time.delta();
    if state.elapsed_without_sink >= OUTPUT_ATTACH_TIMEOUT {
        warn!("Audio playback unavailable; continuing silently");
        commands.entity(entity).try_despawn();
        if let Some(asset) = state.asset.take() {
            sources.remove(asset.id());
        }
        state.entity = None;
        state.disabled = true;
    }
}

#[allow(clippy::type_complexity)]
fn apply_music_volume(
    settings: Res<AudioSettings>,
    mut voices: Query<
        (&AudioChannel, Option<&mut AudioSink>, &mut PlaybackSettings),
        With<AudioPlayer<ContinuousMusic>>,
    >,
) {
    for (channel, sink, mut playback) in &mut voices {
        let volume = settings.volume(*channel);
        playback.volume = volume;
        if let Some(mut sink) = sink {
            sink.set_volume(volume);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoder_preserves_chunk_boundaries_without_inserting_silence() {
        let (sender, receiver) = sync_channel(BUFFERED_CHUNKS);
        sender.send(vec![0.25, -0.5]).unwrap();
        sender.send(vec![0.75, -1.0]).unwrap();
        drop(sender);
        let decoder = ContinuousMusicDecoder {
            receiver,
            samples: Vec::new().into_iter(),
            skipped_samples: 0,
        };
        assert_eq!(decoder.collect::<Vec<_>>(), vec![0.25, -0.5, 0.75, -1.0]);
    }

    #[test]
    fn channel_capacity_bounds_rendered_lookahead() {
        let (sender, receiver) = sync_channel(BUFFERED_CHUNKS);
        for index in 0..BUFFERED_CHUNKS {
            sender.try_send(vec![index as f32]).unwrap();
        }
        assert!(sender.try_send(vec![99.0]).is_err());
        drop(receiver);
    }

    #[test]
    fn music_volume_is_independent_and_zero_mutes() {
        let mut app = App::new();
        app.init_resource::<AudioSettings>()
            .add_systems(Update, apply_music_volume);
        let entity = app
            .world_mut()
            .spawn((
                AudioPlayer::<ContinuousMusic>(Handle::default()),
                PlaybackSettings::ONCE,
                AudioChannel::Music,
            ))
            .id();
        app.update();
        assert_eq!(
            app.world()
                .get::<PlaybackSettings>(entity)
                .unwrap()
                .volume
                .to_linear(),
            0.25
        );
        app.world_mut().resource_mut::<AudioSettings>().music = 0;
        app.update();
        assert_eq!(
            app.world()
                .get::<PlaybackSettings>(entity)
                .unwrap()
                .volume
                .to_linear(),
            0.0
        );
    }

    #[test]
    fn missing_output_disables_music_and_releases_the_asset() {
        let mut app = App::new();
        let mut sources = Assets::<ContinuousMusic>::default();
        let (_sender, receiver) = sync_channel(BUFFERED_CHUNKS);
        let asset = sources.add(ContinuousMusic {
            receiver: Arc::new(Mutex::new(Some(receiver))),
            initial: Vec::new(),
        });
        let entity = app.world_mut().spawn_empty().id();
        app.insert_resource(sources)
            .insert_resource(ContinuousMusicState {
                entity: Some(entity),
                asset: Some(asset),
                ..default()
            })
            .init_resource::<Time<Real>>()
            .add_systems(Update, monitor_continuous_music);
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .advance_by(OUTPUT_ATTACH_TIMEOUT);
        app.update();
        let state = app.world().resource::<ContinuousMusicState>();
        assert!(state.disabled);
        assert!(state.entity.is_none());
        assert!(state.asset.is_none());
        assert!(app.world().resource::<Assets<ContinuousMusic>>().is_empty());
        assert!(app.world().get_entity(entity).is_err());
    }

    #[test]
    fn stalled_worker_returns_silence_without_blocking() {
        let (_sender, receiver) = sync_channel(BUFFERED_CHUNKS);
        let mut decoder = ContinuousMusicDecoder {
            receiver,
            samples: Vec::new().into_iter(),
            skipped_samples: 0,
        };
        let started = std::time::Instant::now();
        assert_eq!(decoder.next(), Some(0.0));
        assert!(started.elapsed() < Duration::from_millis(20));
        assert_eq!(decoder.skipped_samples, 1);
    }

    #[test]
    fn late_chunk_skips_elapsed_timeline_samples() {
        let (sender, receiver) = sync_channel(BUFFERED_CHUNKS);
        let mut decoder = ContinuousMusicDecoder {
            receiver,
            samples: Vec::new().into_iter(),
            skipped_samples: 0,
        };
        assert_eq!(decoder.next(), Some(0.0));
        sender.send(vec![1.0, 2.0, 3.0]).unwrap();
        assert_eq!(decoder.next(), Some(2.0));
        assert_eq!(decoder.next(), Some(3.0));
    }
}
