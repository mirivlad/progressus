use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;

use crate::{ChunkCoord, EntityId, WorldPosition};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ItemKind {
    Wood,
    Stone,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ItemQuantity(NonZeroU32);

impl ItemQuantity {
    pub const fn new(value: u32) -> Option<Self> {
        match NonZeroU32::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ItemLocation {
    Ground { position: WorldPosition },
    Carried { character_id: EntityId },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemStack {
    id: EntityId,
    kind: ItemKind,
    quantity: ItemQuantity,
    location: ItemLocation,
}

impl ItemStack {
    pub(crate) const fn new_ground(
        id: EntityId,
        kind: ItemKind,
        quantity: ItemQuantity,
        position: WorldPosition,
    ) -> Self {
        Self {
            id,
            kind,
            quantity,
            location: ItemLocation::Ground { position },
        }
    }

    pub const fn id(&self) -> EntityId {
        self.id
    }

    pub const fn kind(&self) -> ItemKind {
        self.kind
    }

    pub const fn quantity(&self) -> ItemQuantity {
        self.quantity
    }

    pub const fn location(&self) -> ItemLocation {
        self.location
    }

    pub const fn ground_position(&self) -> Option<WorldPosition> {
        match self.location {
            ItemLocation::Ground { position } => Some(position),
            ItemLocation::Carried { .. } => None,
        }
    }

    pub const fn carrier(&self) -> Option<EntityId> {
        match self.location {
            ItemLocation::Ground { .. } => None,
            ItemLocation::Carried { character_id } => Some(character_id),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ItemWorld {
    items: BTreeMap<EntityId, ItemStack>,
    ground_by_chunk: BTreeMap<ChunkCoord, BTreeSet<EntityId>>,
    carried_by_character: BTreeMap<EntityId, BTreeSet<EntityId>>,
    revision: u64,
}

impl ItemWorld {
    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn get(&self, id: EntityId) -> Option<&ItemStack> {
        self.items.get(&id)
    }

    pub(crate) fn iter(&self) -> impl ExactSizeIterator<Item = &ItemStack> {
        self.items.values()
    }

    pub(crate) fn ground_items_in_chunk(
        &self,
        chunk: ChunkCoord,
    ) -> impl Iterator<Item = &ItemStack> {
        self.ground_by_chunk
            .get(&chunk)
            .into_iter()
            .flat_map(|ids| ids.iter())
            .map(|id| {
                self.items
                    .get(id)
                    .expect("ground item index only contains live item IDs")
            })
    }

    #[cfg(test)]
    pub(crate) fn carried_items_by(
        &self,
        character_id: EntityId,
    ) -> impl Iterator<Item = &ItemStack> {
        self.carried_by_character
            .get(&character_id)
            .into_iter()
            .flat_map(|ids| ids.iter())
            .map(|id| {
                self.items
                    .get(id)
                    .expect("carried item index only contains live item IDs")
            })
    }

    pub(crate) fn insert_ground(&mut self, item: ItemStack) -> Result<(), ItemWorldError> {
        let id = item.id();
        let position = item
            .ground_position()
            .ok_or(ItemWorldError::ExpectedGroundItem(id))?;
        if self.items.contains_key(&id) {
            return Err(ItemWorldError::DuplicateItem(id));
        }

        self.items.insert(id, item);
        self.ground_by_chunk
            .entry(position.containing_cell().split().0)
            .or_default()
            .insert(id);
        self.bump_revision();
        Ok(())
    }

    pub(crate) fn move_to_carried(
        &mut self,
        item_id: EntityId,
        character_id: EntityId,
    ) -> Result<(), ItemWorldError> {
        let position = self
            .items
            .get(&item_id)
            .ok_or(ItemWorldError::UnknownItem(item_id))?
            .ground_position()
            .ok_or(ItemWorldError::ExpectedGroundItem(item_id))?;
        self.remove_ground_index(item_id, position);
        self.items
            .get_mut(&item_id)
            .expect("item was checked above")
            .location = ItemLocation::Carried { character_id };
        self.carried_by_character
            .entry(character_id)
            .or_default()
            .insert(item_id);
        self.bump_revision();
        Ok(())
    }

    pub(crate) fn move_to_ground(
        &mut self,
        item_id: EntityId,
        expected_carrier: EntityId,
        position: WorldPosition,
    ) -> Result<(), ItemWorldError> {
        let carrier = self
            .items
            .get(&item_id)
            .ok_or(ItemWorldError::UnknownItem(item_id))?
            .carrier()
            .ok_or(ItemWorldError::ExpectedCarriedItem(item_id))?;
        if carrier != expected_carrier {
            return Err(ItemWorldError::WrongCarrier {
                item_id,
                expected: expected_carrier,
                actual: carrier,
            });
        }

        self.remove_carried_index(item_id, carrier);
        self.items
            .get_mut(&item_id)
            .expect("item was checked above")
            .location = ItemLocation::Ground { position };
        self.ground_by_chunk
            .entry(position.containing_cell().split().0)
            .or_default()
            .insert(item_id);
        self.bump_revision();
        Ok(())
    }

    fn remove_ground_index(&mut self, item_id: EntityId, position: WorldPosition) {
        let chunk = position.containing_cell().split().0;
        let ids = self
            .ground_by_chunk
            .get_mut(&chunk)
            .expect("ground item has a matching chunk index");
        assert!(
            ids.remove(&item_id),
            "ground item is present in its chunk index"
        );
        if ids.is_empty() {
            self.ground_by_chunk.remove(&chunk);
        }
    }

    fn remove_carried_index(&mut self, item_id: EntityId, character_id: EntityId) {
        let ids = self
            .carried_by_character
            .get_mut(&character_id)
            .expect("carried item has a matching carrier index");
        assert!(
            ids.remove(&item_id),
            "carried item is present in its carrier index"
        );
        if ids.is_empty() {
            self.carried_by_character.remove(&character_id);
        }
    }

    fn bump_revision(&mut self) {
        self.revision = self
            .revision
            .checked_add(1)
            .expect("item revision overflow");
    }

    #[cfg(test)]
    pub(crate) fn indexes_are_consistent(&self) -> bool {
        for item in self.items.values() {
            match item.location() {
                ItemLocation::Ground { position } => {
                    let chunk = position.containing_cell().split().0;
                    if !self
                        .ground_by_chunk
                        .get(&chunk)
                        .is_some_and(|ids| ids.contains(&item.id()))
                    {
                        return false;
                    }
                    if self
                        .carried_by_character
                        .values()
                        .any(|ids| ids.contains(&item.id()))
                    {
                        return false;
                    }
                }
                ItemLocation::Carried { character_id } => {
                    if !self
                        .carried_by_character
                        .get(&character_id)
                        .is_some_and(|ids| ids.contains(&item.id()))
                    {
                        return false;
                    }
                    if self
                        .ground_by_chunk
                        .values()
                        .any(|ids| ids.contains(&item.id()))
                    {
                        return false;
                    }
                }
            }
        }
        let indexed = self
            .ground_by_chunk
            .values()
            .chain(self.carried_by_character.values())
            .map(BTreeSet::len)
            .sum::<usize>();
        indexed == self.items.len()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ItemWorldError {
    DuplicateItem(EntityId),
    UnknownItem(EntityId),
    ExpectedGroundItem(EntityId),
    ExpectedCarriedItem(EntityId),
    WrongCarrier {
        item_id: EntityId,
        expected: EntityId,
        actual: EntityId,
    },
}

#[cfg(test)]
mod tests {
    use crate::{EntityId, WorldCell, WorldPosition};

    use super::{ItemKind, ItemQuantity, ItemStack, ItemWorld};

    fn id(value: u64) -> EntityId {
        EntityId::new(value).unwrap()
    }

    #[test]
    fn zero_quantity_is_not_a_valid_stack() {
        assert_eq!(ItemQuantity::new(0), None);
        assert_eq!(ItemQuantity::new(1).unwrap().get(), 1);
    }

    #[test]
    fn transfer_updates_exactly_one_location_index_and_preserves_identity() {
        let position = WorldPosition::from_cell_center(WorldCell::new(2, -3)).unwrap();
        let dropped = position.checked_translate(200, -100).unwrap();
        let item = ItemStack::new_ground(
            id(8),
            ItemKind::Wood,
            ItemQuantity::new(7).unwrap(),
            position,
        );
        let mut world = ItemWorld::default();
        world.insert_ground(item).unwrap();
        assert!(world.indexes_are_consistent());
        assert_eq!(
            world
                .ground_items_in_chunk(position.containing_cell().split().0)
                .count(),
            1
        );

        world.move_to_carried(id(8), id(3)).unwrap();
        assert!(world.indexes_are_consistent());
        assert_eq!(
            world
                .ground_items_in_chunk(position.containing_cell().split().0)
                .count(),
            0
        );
        let carried = world.carried_items_by(id(3)).next().unwrap();
        assert_eq!(carried.id(), id(8));
        assert_eq!(carried.kind(), ItemKind::Wood);
        assert_eq!(carried.quantity().get(), 7);

        world.move_to_ground(id(8), id(3), dropped).unwrap();
        assert!(world.indexes_are_consistent());
        let ground = world
            .ground_items_in_chunk(dropped.containing_cell().split().0)
            .next()
            .unwrap();
        assert_eq!(ground.id(), id(8));
        assert_eq!(ground.ground_position(), Some(dropped));
        assert_eq!(world.carried_items_by(id(3)).count(), 0);
    }
}
