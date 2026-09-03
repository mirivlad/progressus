use std::collections::VecDeque;

use crate::{InteractionRadius, SimulationError, WorldCell, WorldPosition};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    East,
    West,
    North,
    South,
}

impl Direction {
    pub fn adjacent(self, cell: WorldCell) -> Option<WorldCell> {
        match self {
            Self::East => cell.x().checked_add(1).map(|x| WorldCell::new(x, cell.y())),
            Self::West => cell.x().checked_sub(1).map(|x| WorldCell::new(x, cell.y())),
            Self::North => cell.y().checked_add(1).map(|y| WorldCell::new(cell.x(), y)),
            Self::South => cell.y().checked_sub(1).map(|y| WorldCell::new(cell.x(), y)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MovementState {
    Idle,
    ManualDirectional { direction: Direction },
    Navigating { destination: WorldPosition },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NavigationRoute {
    pub(crate) destination: WorldPosition,
    pub(crate) waypoints: VecDeque<WorldPosition>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MovementSpeed(u32);

impl MovementSpeed {
    pub const fn new(subunits_per_tick: u32) -> Option<Self> {
        if subunits_per_tick == 0 {
            None
        } else {
            Some(Self(subunits_per_tick))
        }
    }

    pub const fn subunits_per_tick(self) -> u32 {
        self.0
    }
}

pub const DEFAULT_CHARACTER_SPEED: MovementSpeed = MovementSpeed(256);
pub const DEFAULT_CHARACTER_INTERACTION_RADIUS: InteractionRadius = InteractionRadius::new(768);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EntityId(u64);

impl EntityId {
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Character {
    id: EntityId,
    name: String,
    position: WorldPosition,
    speed: MovementSpeed,
    interaction_radius: InteractionRadius,
    movement: MovementState,
    route: Option<NavigationRoute>,
    last_tick_motion_trace: Vec<WorldPosition>,
}

impl Character {
    pub(crate) fn new(id: EntityId, name: &str, position: WorldPosition) -> Self {
        Self {
            id,
            name: name.to_owned(),
            position,
            speed: DEFAULT_CHARACTER_SPEED,
            interaction_radius: DEFAULT_CHARACTER_INTERACTION_RADIUS,
            movement: MovementState::Idle,
            route: None,
            last_tick_motion_trace: vec![position],
        }
    }

    pub(crate) fn restore(
        id: EntityId,
        name: String,
        position: WorldPosition,
        speed: MovementSpeed,
        interaction_radius: InteractionRadius,
        movement: MovementState,
        route: Option<NavigationRoute>,
    ) -> Self {
        Self {
            id,
            name,
            position,
            speed,
            interaction_radius,
            movement,
            route,
            last_tick_motion_trace: vec![position],
        }
    }

    pub const fn id(&self) -> EntityId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn position(&self) -> WorldPosition {
        self.position
    }

    pub const fn speed(&self) -> MovementSpeed {
        self.speed
    }

    pub const fn interaction_radius(&self) -> InteractionRadius {
        self.interaction_radius
    }

    pub const fn movement(&self) -> MovementState {
        self.movement
    }

    pub(crate) fn navigation_route(&self) -> Option<&NavigationRoute> {
        self.route.as_ref()
    }

    pub fn last_tick_motion_trace(&self) -> &[WorldPosition] {
        &self.last_tick_motion_trace
    }

    pub fn navigation_destination(&self) -> Option<WorldPosition> {
        self.route.as_ref().map(|route| route.destination)
    }

    pub fn navigation_waypoints(&self) -> impl Iterator<Item = WorldPosition> + '_ {
        self.route
            .iter()
            .flat_map(|route| route.waypoints.iter().copied())
    }

    pub(crate) fn set_position(&mut self, position: WorldPosition) {
        self.position = position;
    }

    #[cfg(test)]
    pub(crate) fn set_speed(&mut self, speed: MovementSpeed) {
        self.speed = speed;
    }

    pub(crate) fn set_movement(&mut self, movement: MovementState) {
        self.movement = movement;
        if !matches!(movement, MovementState::Navigating { .. }) {
            self.route = None;
        }
    }

    pub(crate) fn set_navigation_route(&mut self, route: NavigationRoute) {
        self.movement = MovementState::Navigating {
            destination: route.destination,
        };
        self.route = Some(route);
    }

    pub(crate) fn set_last_tick_motion_trace(&mut self, trace: Vec<WorldPosition>) {
        self.last_tick_motion_trace = trace;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct EntityIdAllocator {
    next: Option<u64>,
}

impl EntityIdAllocator {
    pub(crate) const fn new() -> Self {
        Self { next: Some(1) }
    }

    pub(crate) fn allocate(&mut self) -> Result<EntityId, SimulationError> {
        let value = self.next.ok_or(SimulationError::EntityIdExhausted)?;
        let id = EntityId::new(value).ok_or(SimulationError::EntityIdExhausted)?;
        self.next = value.checked_add(1);
        Ok(id)
    }

    pub(crate) const fn restore_next(next: Option<u64>) -> Self {
        Self { next }
    }

    pub(crate) fn peek(self) -> Option<EntityId> {
        self.next.and_then(EntityId::new)
    }
}
