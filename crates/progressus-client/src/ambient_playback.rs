//! Playback timing only. Neither simulation time nor audio device handles belong here.

pub(crate) const CROSSFADE_SECONDS: f64 = 6.0;

pub(crate) struct PlaybackTiming {
    requested: f64,
    started: Option<f64>,
}

impl PlaybackTiming {
    pub fn new(requested: f64) -> Self {
        Self {
            requested,
            started: None,
        }
    }
    pub fn observe_started(&mut self, now: f64) {
        self.started.get_or_insert(now);
    }
    pub fn gain(&self, now: f64) -> f32 {
        self.started.map_or(0.0, |start| {
            ((now - start) / CROSSFADE_SECONDS).clamp(0.0, 1.0) as f32
        })
    }
    pub fn replacement_due(
        &self,
        now: f64,
        duration: f64,
        looping: bool,
        next_ready: bool,
        outgoing: bool,
    ) -> bool {
        looping
            && next_ready
            && !outgoing
            && self
                .started
                .is_some_and(|start| now - start >= duration - CROSSFADE_SECONDS)
    }
    pub fn unavailable(&self, now: f64) -> bool {
        self.started.is_none() && now - self.requested > 5.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crossfade_waits_for_actual_sink_then_preserves_combined_gain() {
        let mut next = PlaybackTiming::new(10.0);
        assert_eq!(next.gain(14.0), 0.0);
        next.observe_started(14.0);
        assert_eq!(next.gain(14.0), 0.0);
        assert_eq!(next.gain(17.0), 0.5);
        next.observe_started(18.0);
        assert_eq!(
            next.gain(20.0),
            1.0,
            "repeated observation must not reset fade"
        );
        for step in 0..100 {
            let incoming = next.gain(14.0 + f64::from(step) / 10.0);
            let outgoing = 1.0 - incoming;
            assert!((0.0..=1.0).contains(&incoming));
            assert_eq!(incoming + outgoing, 1.0);
        }
    }

    #[test]
    fn late_generation_keeps_old_bed_and_never_replays_melody() {
        let mut current = PlaybackTiming::new(0.0);
        current.observe_started(1.0);
        assert!(!current.replacement_due(58.99, 64.0, true, true, false));
        assert!(current.replacement_due(59.0, 64.0, true, true, false));
        assert!(!current.replacement_due(1000.0, 64.0, true, false, false));
        assert!(!current.replacement_due(1000.0, 64.0, true, true, true));
        assert!(!current.replacement_due(1000.0, 64.0, false, true, false));
        assert_eq!(current.gain(1000.0), 1.0);
    }

    #[test]
    fn missing_device_times_out_but_started_sink_is_not_a_startup_failure() {
        let mut pending = PlaybackTiming::new(50.0);
        assert!(!pending.unavailable(54.9));
        assert!(pending.unavailable(55.1));
        pending.observe_started(55.2);
        assert!(!pending.unavailable(10000.0));
    }
}
