use std::time::Duration;

use bevy::prelude::{ButtonInput, KeyCode};
use progressus_app::{Command, Direction, EntityId};

pub const TICK_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Default)]
pub struct TickScheduler {
    elapsed: Duration,
    paused: bool,
}

impl TickScheduler {
    pub const fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        self.reset_timing();
    }

    pub fn toggle_paused(&mut self) -> bool {
        let paused = !self.paused;
        self.set_paused(paused);
        paused
    }

    pub fn reset_timing(&mut self) {
        self.elapsed = Duration::ZERO;
    }

    pub fn advance(&mut self, frame_delta: Duration) -> bool {
        if self.paused {
            return false;
        }
        self.elapsed = self.elapsed.saturating_add(frame_delta);
        if self.elapsed < TICK_INTERVAL {
            return false;
        }
        self.elapsed = Duration::ZERO;
        true
    }
}

pub fn movement_command(keys: &ButtonInput<KeyCode>, character_id: EntityId) -> Option<Command> {
    if keys.just_pressed(KeyCode::Space) {
        return Some(Command::StopMovement { character_id });
    }
    let direction = if keys.just_pressed(KeyCode::ArrowRight) {
        Some(Direction::East)
    } else if keys.just_pressed(KeyCode::ArrowUp) {
        Some(Direction::North)
    } else if keys.just_pressed(KeyCode::ArrowDown) {
        Some(Direction::South)
    } else if keys.just_pressed(KeyCode::ArrowLeft) {
        Some(Direction::West)
    } else {
        None
    }?;
    Some(Command::SetMovementDirection {
        character_id,
        direction,
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bevy::prelude::{ButtonInput, KeyCode};
    use progressus_app::{Command, Direction, EntityId};

    use super::{TickScheduler, movement_command};

    #[test]
    fn scheduler_emits_one_tick_and_discards_long_frame_backlog() {
        let mut scheduler = TickScheduler::default();
        assert!(!scheduler.advance(Duration::from_millis(249)));
        assert!(scheduler.advance(Duration::from_millis(1)));
        assert!(!scheduler.advance(Duration::ZERO));
        assert!(scheduler.advance(Duration::from_secs(3)));
        assert!(!scheduler.advance(Duration::ZERO));
    }

    #[test]
    fn pause_discards_elapsed_time_and_does_not_build_backlog() {
        let mut scheduler = TickScheduler::default();
        assert!(!scheduler.advance(Duration::from_millis(200)));
        scheduler.set_paused(true);
        assert!(scheduler.is_paused());
        assert!(!scheduler.advance(Duration::from_secs(10)));

        scheduler.set_paused(false);
        assert!(!scheduler.is_paused());
        assert!(!scheduler.advance(Duration::from_millis(249)));
        assert!(scheduler.advance(Duration::from_millis(1)));
    }

    #[test]
    fn direction_is_sent_on_press_edge_not_on_held_frame() {
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::ArrowRight);
        assert_eq!(
            movement_command(&keys, EntityId::new(3).unwrap()),
            Some(Command::SetMovementDirection {
                character_id: EntityId::new(3).unwrap(),
                direction: Direction::East,
            }),
        );
        keys.clear_just_pressed(KeyCode::ArrowRight);
        assert_eq!(movement_command(&keys, EntityId::new(3).unwrap()), None);
    }

    #[test]
    fn stop_event_has_priority_over_direction_event() {
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::Space);
        keys.press(KeyCode::ArrowUp);
        assert_eq!(
            movement_command(&keys, EntityId::new(3).unwrap()),
            Some(Command::StopMovement {
                character_id: EntityId::new(3).unwrap()
            }),
        );
    }
}
