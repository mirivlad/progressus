use crate::{SimulationError, WorldCell};

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
    position: WorldCell,
}

impl Character {
    pub(crate) fn new(id: EntityId, name: &str, position: WorldCell) -> Self {
        Self {
            id,
            name: name.to_owned(),
            position,
        }
    }

    pub const fn id(&self) -> EntityId {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn position(&self) -> WorldCell {
        self.position
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
}
