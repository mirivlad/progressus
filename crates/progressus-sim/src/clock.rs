use crate::SimulationError;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SimulationTick(u64);

impl SimulationTick {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SimulationClock {
    tick: SimulationTick,
}

impl SimulationClock {
    pub(crate) const fn new(tick: u64) -> Self {
        Self {
            tick: SimulationTick::new(tick),
        }
    }

    pub(crate) const fn tick(self) -> SimulationTick {
        self.tick
    }

    pub(crate) fn advance(&mut self, count: u64) -> Result<(), SimulationError> {
        let next = self
            .tick
            .value()
            .checked_add(count)
            .ok_or(SimulationError::TickOverflow)?;
        self.tick = SimulationTick::new(next);
        Ok(())
    }

    #[cfg(test)]
    const fn value(self) -> u64 {
        self.tick.value()
    }
}

#[cfg(test)]
mod tests {
    use super::SimulationClock;
    use crate::SimulationError;

    #[test]
    fn tick_overflow_fails_explicitly() {
        let mut clock = SimulationClock::new(u64::MAX - 1);

        assert_eq!(clock.advance(1), Ok(()));
        assert_eq!(clock.value(), u64::MAX);
        assert_eq!(clock.advance(1), Err(SimulationError::TickOverflow));
    }
}
