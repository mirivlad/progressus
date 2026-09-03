use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;

use crate::{ChunkCoord, EntityId, WorldPosition};

pub const MAX_STACK_QUANTITY: u32 = 1024;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ItemKind {
    Wood,
    Stone,
    PrimitiveTool,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ItemQuantity(NonZeroU32);

impl ItemQuantity {
    pub const fn new(value: u32) -> Option<Self> {
        match NonZeroU32::new(value) {
            Some(value) if value.get() <= MAX_STACK_QUANTITY => Some(Self(value)),
            _ => None,
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
        if item.quantity().get() > MAX_STACK_QUANTITY {
            return Err(ItemWorldError::StackQuantityExceedsMaximum {
                item_id: item.id(),
                quantity: item.quantity().get(),
            });
        }
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

    pub(crate) fn split_ground_stack(
        &mut self,
        source_id: EntityId,
        split_id: EntityId,
        amount: u32,
    ) -> Result<(), ItemWorldError> {
        if amount == 0 {
            return Err(ItemWorldError::ZeroSplit);
        }
        if self.items.contains_key(&split_id) {
            return Err(ItemWorldError::DuplicateItem(split_id));
        }
        let source = self
            .items
            .get(&source_id)
            .ok_or(ItemWorldError::UnknownItem(source_id))?;
        let position = source
            .ground_position()
            .ok_or(ItemWorldError::ExpectedGroundItem(source_id))?;
        let available = source.quantity().get();
        if amount >= available {
            return Err(ItemWorldError::SplitMustLeaveRemainder {
                item_id: source_id,
                requested: amount,
                available,
            });
        }
        let kind = source.kind();
        self.items
            .get_mut(&source_id)
            .expect("source stack was checked above")
            .quantity = ItemQuantity::new(available - amount)
            .expect("a valid split leaves a positive source quantity");
        self.items.insert(
            split_id,
            ItemStack::new_ground(
                split_id,
                kind,
                ItemQuantity::new(amount)
                    .expect("split quantity is positive and within stack capacity"),
                position,
            ),
        );
        self.ground_by_chunk
            .entry(position.containing_cell().split().0)
            .or_default()
            .insert(split_id);
        self.bump_revision();
        Ok(())
    }

    pub(crate) fn consume(&mut self, item_id: EntityId, amount: u32) -> Result<(), ItemWorldError> {
        if amount == 0 {
            return Err(ItemWorldError::ZeroConsumption);
        }
        let item = self
            .items
            .get(&item_id)
            .ok_or(ItemWorldError::UnknownItem(item_id))?;
        let available = item.quantity().get();
        if amount > available {
            return Err(ItemWorldError::InsufficientQuantity {
                item_id,
                requested: amount,
                available,
            });
        }
        if amount < available {
            self.items
                .get_mut(&item_id)
                .expect("item was checked above")
                .quantity = ItemQuantity::new(available - amount)
                .expect("partial consumption leaves a positive quantity");
            self.bump_revision();
            return Ok(());
        }

        let location = item.location();
        match location {
            ItemLocation::Ground { position } => self.remove_ground_index(item_id, position),
            ItemLocation::Carried { character_id } => {
                self.remove_carried_index(item_id, character_id)
            }
        }
        self.items
            .remove(&item_id)
            .expect("item was checked above and its location index was removed");
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

    pub(crate) fn merge_ground_stacks(
        &mut self,
        target_id: EntityId,
        source_id: EntityId,
    ) -> Result<(), ItemWorldError> {
        if target_id == source_id {
            return Err(ItemWorldError::CannotMergeStackWithItself(target_id));
        }
        let target = self
            .items
            .get(&target_id)
            .ok_or(ItemWorldError::UnknownItem(target_id))?;
        let source = self
            .items
            .get(&source_id)
            .ok_or(ItemWorldError::UnknownItem(source_id))?;
        let target_position = target
            .ground_position()
            .ok_or(ItemWorldError::ExpectedGroundItem(target_id))?;
        let source_position = source
            .ground_position()
            .ok_or(ItemWorldError::ExpectedGroundItem(source_id))?;
        if target_position.containing_cell() != source_position.containing_cell() {
            return Err(ItemWorldError::MergeDifferentCells {
                target_id,
                source_id,
            });
        }
        if target.kind() != source.kind() {
            return Err(ItemWorldError::MergeDifferentKinds {
                target_id,
                source_id,
            });
        }
        let target_quantity = target.quantity().get();
        let source_quantity = source.quantity().get();
        let combined = target_quantity
            .checked_add(source_quantity)
            .ok_or(ItemWorldError::StackQuantityOverflow)?;
        if combined > MAX_STACK_QUANTITY {
            return Err(ItemWorldError::StackCapacityExceeded {
                target_id,
                source_id,
                combined,
            });
        }

        self.remove_ground_index(source_id, source_position);
        self.items.remove(&source_id);
        self.items
            .get_mut(&target_id)
            .expect("target stack was checked above")
            .quantity = ItemQuantity::new(combined).expect("combined stack quantity is positive");
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
    ZeroConsumption,
    ZeroSplit,
    SplitMustLeaveRemainder {
        item_id: EntityId,
        requested: u32,
        available: u32,
    },
    InsufficientQuantity {
        item_id: EntityId,
        requested: u32,
        available: u32,
    },
    ExpectedGroundItem(EntityId),
    ExpectedCarriedItem(EntityId),
    WrongCarrier {
        item_id: EntityId,
        expected: EntityId,
        actual: EntityId,
    },
    StackQuantityExceedsMaximum {
        item_id: EntityId,
        quantity: u32,
    },
    CannotMergeStackWithItself(EntityId),
    MergeDifferentCells {
        target_id: EntityId,
        source_id: EntityId,
    },
    MergeDifferentKinds {
        target_id: EntityId,
        source_id: EntityId,
    },
    StackCapacityExceeded {
        target_id: EntityId,
        source_id: EntityId,
        combined: u32,
    },
    StackQuantityOverflow,
}

#[cfg(test)]
mod tests {
    use crate::{EntityId, WorldCell, WorldPosition};

    use super::{ItemKind, ItemQuantity, ItemStack, ItemWorld, ItemWorldError, MAX_STACK_QUANTITY};

    fn id(value: u64) -> EntityId {
        EntityId::new(value).unwrap()
    }

    #[test]
    fn zero_quantity_is_not_a_valid_stack() {
        assert_eq!(ItemQuantity::new(0), None);
        assert_eq!(ItemQuantity::new(1).unwrap().get(), 1);
        assert_eq!(
            ItemQuantity::new(MAX_STACK_QUANTITY).unwrap().get(),
            MAX_STACK_QUANTITY
        );
        assert_eq!(ItemQuantity::new(MAX_STACK_QUANTITY + 1), None);
    }

    #[test]
    fn ground_stacks_merge_up_to_the_physical_stack_limit() {
        let position = WorldPosition::from_cell_center(WorldCell::new(3, 4)).unwrap();
        let mut world = ItemWorld::default();
        world
            .insert_ground(ItemStack::new_ground(
                id(20),
                ItemKind::Wood,
                ItemQuantity::new(1000).unwrap(),
                position,
            ))
            .unwrap();
        world
            .insert_ground(ItemStack::new_ground(
                id(21),
                ItemKind::Wood,
                ItemQuantity::new(24).unwrap(),
                position.checked_translate(100, 100).unwrap(),
            ))
            .unwrap();
        world.merge_ground_stacks(id(20), id(21)).unwrap();
        assert_eq!(
            world.get(id(20)).unwrap().quantity().get(),
            MAX_STACK_QUANTITY
        );
        assert!(world.get(id(21)).is_none());
        assert!(world.indexes_are_consistent());
    }

    #[test]
    fn splitting_ground_stack_preserves_kind_position_and_total_quantity() {
        let position = WorldPosition::from_cell_center(WorldCell::new(4, -2)).unwrap();
        let mut world = ItemWorld::default();
        world
            .insert_ground(ItemStack::new_ground(
                id(30),
                ItemKind::Wood,
                ItemQuantity::new(115).unwrap(),
                position,
            ))
            .unwrap();

        world.split_ground_stack(id(30), id(31), 2).unwrap();

        assert_eq!(world.get(id(30)).unwrap().quantity().get(), 113);
        let split = world.get(id(31)).unwrap();
        assert_eq!(split.kind(), ItemKind::Wood);
        assert_eq!(split.quantity().get(), 2);
        assert_eq!(split.ground_position(), Some(position));
        assert_eq!(
            world.get(id(30)).unwrap().quantity().get() + split.quantity().get(),
            115
        );
        assert!(world.indexes_are_consistent());
    }

    #[test]
    fn consumption_preserves_remainder_and_removes_fully_consumed_stack_from_indexes() {
        let position = WorldPosition::from_cell_center(WorldCell::new(1, 2)).unwrap();
        let mut world = ItemWorld::default();
        world
            .insert_ground(ItemStack::new_ground(
                id(11),
                ItemKind::Wood,
                ItemQuantity::new(5).unwrap(),
                position,
            ))
            .unwrap();
        let revision = world.revision();

        world.consume(id(11), 2).unwrap();
        assert_eq!(world.get(id(11)).unwrap().quantity().get(), 3);
        assert_eq!(world.revision(), revision + 1);
        assert!(world.indexes_are_consistent());

        assert_eq!(
            world.consume(id(11), 4),
            Err(ItemWorldError::InsufficientQuantity {
                item_id: id(11),
                requested: 4,
                available: 3,
            })
        );
        assert_eq!(
            world.consume(id(11), 0),
            Err(ItemWorldError::ZeroConsumption)
        );
        assert_eq!(world.get(id(11)).unwrap().quantity().get(), 3);

        world.consume(id(11), 3).unwrap();
        assert!(world.get(id(11)).is_none());
        assert_eq!(
            world
                .ground_items_in_chunk(position.containing_cell().split().0)
                .count(),
            0
        );
        assert!(world.indexes_are_consistent());
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
