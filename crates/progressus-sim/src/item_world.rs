use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;

use progressus_content::{ItemId, MAX_STACK_QUANTITY, SlotId};

use crate::{ChunkCoord, EntityId, WorldPosition};

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
    Ground {
        position: WorldPosition,
    },
    Carried {
        character_id: EntityId,
    },
    /// Worn or held in a named slot rather than in the hands, so it does not
    /// count against what a character can carry. See ADR-0024.
    Equipped {
        character_id: EntityId,
        slot: SlotId,
    },
    /// Inside a container, which has a location of its own. This is what lets
    /// a loaded cart be set down: the goods belong to the cart, not to whoever
    /// happens to be pushing it.
    Contained {
        container_id: EntityId,
    },
}

/// How deep containers may nest. A bucket may ride in a cart; a cart may not
/// ride in a cart. Provisional, and stated as a number rather than left to
/// emerge from whatever the code happens to allow.
pub const MAX_CONTAINER_DEPTH: usize = 2;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemStack {
    id: EntityId,
    kind: ItemId,
    quantity: ItemQuantity,
    location: ItemLocation,
}

impl ItemStack {
    pub(crate) const fn new_ground(
        id: EntityId,
        kind: ItemId,
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

    pub const fn kind(&self) -> ItemId {
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
            ItemLocation::Carried { .. }
            | ItemLocation::Equipped { .. }
            | ItemLocation::Contained { .. } => None,
        }
    }

    /// Who has this in their hands. Equipment is deliberately excluded: it is
    /// borne, not carried, and must not consume carrying capacity.
    pub const fn carrier(&self) -> Option<EntityId> {
        match self.location {
            ItemLocation::Ground { .. }
            | ItemLocation::Equipped { .. }
            | ItemLocation::Contained { .. } => None,
            ItemLocation::Carried { character_id } => Some(character_id),
        }
    }

    /// Which container holds this, if any.
    pub const fn container(&self) -> Option<EntityId> {
        match self.location {
            ItemLocation::Contained { container_id } => Some(container_id),
            ItemLocation::Ground { .. }
            | ItemLocation::Carried { .. }
            | ItemLocation::Equipped { .. } => None,
        }
    }

    /// Who has this equipped, and where.
    pub const fn bearer(&self) -> Option<(EntityId, SlotId)> {
        match self.location {
            ItemLocation::Equipped { character_id, slot } => Some((character_id, slot)),
            ItemLocation::Ground { .. }
            | ItemLocation::Carried { .. }
            | ItemLocation::Contained { .. } => None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ItemWorld {
    items: BTreeMap<EntityId, ItemStack>,
    ground_by_chunk: BTreeMap<ChunkCoord, BTreeSet<EntityId>>,
    carried_by_character: BTreeMap<EntityId, BTreeSet<EntityId>>,
    /// One item per character per slot, so equipment cannot silently stack.
    equipped_by_character: BTreeMap<EntityId, BTreeMap<SlotId, EntityId>>,
    contents_by_container: BTreeMap<EntityId, BTreeSet<EntityId>>,
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
            ItemLocation::Equipped { character_id, slot } => {
                self.remove_equipped_index(item_id, character_id, slot)
            }
            ItemLocation::Contained { container_id } => {
                self.remove_contained_index(item_id, container_id)
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
        let location = self
            .items
            .get(&item_id)
            .ok_or(ItemWorldError::UnknownItem(item_id))?
            .location();
        let holder = self
            .holder_of(item_id)
            .ok_or(ItemWorldError::ExpectedCarriedItem(item_id))?;
        if holder != expected_carrier {
            return Err(ItemWorldError::WrongCarrier {
                item_id,
                expected: expected_carrier,
                actual: holder,
            });
        }
        match location {
            ItemLocation::Carried { character_id } => {
                self.remove_carried_index(item_id, character_id);
            }
            ItemLocation::Contained { container_id } => {
                self.remove_contained_index(item_id, container_id);
            }
            ItemLocation::Equipped { character_id, slot } => {
                self.remove_equipped_index(item_id, character_id, slot);
            }
            ItemLocation::Ground { .. } => {
                return Err(ItemWorldError::ExpectedCarriedItem(item_id));
            }
        }
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

    pub(crate) fn move_contained_to_ground(
        &mut self,
        item_id: EntityId,
        expected_container: EntityId,
        position: WorldPosition,
    ) -> Result<(), ItemWorldError> {
        let container_id = self
            .items
            .get(&item_id)
            .ok_or(ItemWorldError::UnknownItem(item_id))?
            .container()
            .ok_or(ItemWorldError::ExpectedCarriedItem(item_id))?;
        if container_id != expected_container {
            return Err(ItemWorldError::IndexCorruption);
        }
        self.remove_contained_index(item_id, container_id);
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

    /// Moves a stack from a character's hands into one of their slots. The
    /// slot must be free: equipment does not stack.
    pub(crate) fn equip_carried(
        &mut self,
        item_id: EntityId,
        character_id: EntityId,
        slot: SlotId,
    ) -> Result<(), ItemWorldError> {
        let carrier = self
            .items
            .get(&item_id)
            .ok_or(ItemWorldError::UnknownItem(item_id))?
            .carrier()
            .ok_or(ItemWorldError::ExpectedCarriedItem(item_id))?;
        if carrier != character_id {
            return Err(ItemWorldError::ExpectedCarriedItem(item_id));
        }
        if self
            .equipped_by_character
            .get(&character_id)
            .is_some_and(|slots| slots.contains_key(&slot))
        {
            return Err(ItemWorldError::SlotAlreadyOccupied {
                character_id,
                item_id,
            });
        }
        self.remove_carried_index(item_id, character_id);
        self.items
            .get_mut(&item_id)
            .expect("item was checked above")
            .location = ItemLocation::Equipped { character_id, slot };
        self.equipped_by_character
            .entry(character_id)
            .or_default()
            .insert(slot, item_id);
        self.bump_revision();
        Ok(())
    }

    /// Moves equipment back into the bearer's hands, where the ordinary
    /// carrying rules apply to it again.
    pub(crate) fn unequip_to_carried(&mut self, item_id: EntityId) -> Result<(), ItemWorldError> {
        let (character_id, slot) = self
            .items
            .get(&item_id)
            .ok_or(ItemWorldError::UnknownItem(item_id))?
            .bearer()
            .ok_or(ItemWorldError::ExpectedEquippedItem(item_id))?;
        self.remove_equipped_index(item_id, character_id, slot);
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

    /// What this character has equipped, by slot.
    pub(crate) fn equipped_by(
        &self,
        character_id: EntityId,
    ) -> impl Iterator<Item = (SlotId, &ItemStack)> {
        self.equipped_by_character
            .get(&character_id)
            .into_iter()
            .flat_map(|slots| slots.iter())
            .map(|(slot, id)| {
                (
                    *slot,
                    self.items
                        .get(id)
                        .expect("equipment index only contains live item IDs"),
                )
            })
    }

    /// Puts a stack into a container, from the ground or from someone's hands.
    ///
    /// This enforces structure only — the container exists, the move creates no
    /// cycle, and it nests no deeper than [`MAX_CONTAINER_DEPTH`]. Whether a
    /// character may reach that container is the simulation's business, not the
    /// item world's.
    pub(crate) fn move_to_container(
        &mut self,
        item_id: EntityId,
        container_id: EntityId,
    ) -> Result<(), ItemWorldError> {
        if item_id == container_id {
            return Err(ItemWorldError::ContainerCycle {
                item_id,
                container_id,
            });
        }
        let location = self
            .items
            .get(&item_id)
            .ok_or(ItemWorldError::UnknownItem(item_id))?
            .location();
        if !self.items.contains_key(&container_id) {
            return Err(ItemWorldError::UnknownItem(container_id));
        }
        let chain = self.container_chain(container_id)?;
        if chain.contains(&item_id) {
            return Err(ItemWorldError::ContainerCycle {
                item_id,
                container_id,
            });
        }
        let subtree_depth = self.contained_subtree_depth(item_id, &mut BTreeSet::new())?;
        if chain.len() + subtree_depth >= MAX_CONTAINER_DEPTH {
            return Err(ItemWorldError::ContainerTooDeep {
                item_id,
                container_id,
            });
        }

        match location {
            ItemLocation::Ground { position } => self.remove_ground_index(item_id, position),
            ItemLocation::Carried { character_id } => {
                self.remove_carried_index(item_id, character_id);
            }
            ItemLocation::Equipped { .. } | ItemLocation::Contained { .. } => {
                return Err(ItemWorldError::ExpectedGroundItem(item_id));
            }
        }
        self.items
            .get_mut(&item_id)
            .expect("item was checked above")
            .location = ItemLocation::Contained { container_id };
        self.contents_by_container
            .entry(container_id)
            .or_default()
            .insert(item_id);
        self.bump_revision();
        Ok(())
    }

    /// What is inside this container.
    pub(crate) fn contents_of(&self, container_id: EntityId) -> impl Iterator<Item = &ItemStack> {
        self.contents_by_container
            .get(&container_id)
            .into_iter()
            .flat_map(|ids| ids.iter())
            .map(|id| {
                self.items
                    .get(id)
                    .expect("container index only contains live item IDs")
            })
    }

    /// The character who ultimately has hold of this stack, following it out
    /// through any containers. `None` means it rests on the ground.
    pub(crate) fn holder_of(&self, item_id: EntityId) -> Option<EntityId> {
        let mut current = item_id;
        for _ in 0..=MAX_CONTAINER_DEPTH {
            let item = self.items.get(&current)?;
            match item.location() {
                ItemLocation::Ground { .. } => return None,
                ItemLocation::Carried { character_id }
                | ItemLocation::Equipped { character_id, .. } => return Some(character_id),
                ItemLocation::Contained { container_id } => current = container_id,
            }
        }
        None
    }

    /// The containers this stack sits inside, outermost last. Bounded by the
    /// depth limit, which is what makes the cycle check cheap.
    fn container_chain(&self, item_id: EntityId) -> Result<Vec<EntityId>, ItemWorldError> {
        let mut chain = Vec::new();
        let mut current = item_id;
        loop {
            chain.push(current);
            let Some(item) = self.items.get(&current) else {
                return Err(ItemWorldError::UnknownItem(current));
            };
            let Some(next) = item.container() else {
                return Ok(chain);
            };
            if chain.contains(&next) || chain.len() > MAX_CONTAINER_DEPTH + 1 {
                return Err(ItemWorldError::IndexCorruption);
            }
            current = next;
        }
    }

    /// The stack this one ultimately sits in — itself, unless containers hold
    /// it. Where that stack rests is where the whole nest rests.
    pub(crate) fn root_of(&self, item_id: EntityId) -> Option<&ItemStack> {
        let chain = self.container_chain(item_id).ok()?;
        self.items.get(chain.last()?)
    }

    fn contained_subtree_depth(
        &self,
        item_id: EntityId,
        visited: &mut BTreeSet<EntityId>,
    ) -> Result<usize, ItemWorldError> {
        if !visited.insert(item_id) {
            return Err(ItemWorldError::IndexCorruption);
        }
        let mut depth = 0;
        if let Some(contents) = self.contents_by_container.get(&item_id) {
            for child_id in contents {
                if !self.items.contains_key(child_id) {
                    return Err(ItemWorldError::IndexCorruption);
                }
                depth = depth.max(1 + self.contained_subtree_depth(*child_id, visited)?);
            }
        }
        visited.remove(&item_id);
        Ok(depth)
    }

    fn remove_contained_index(&mut self, item_id: EntityId, container_id: EntityId) {
        let contents = self
            .contents_by_container
            .get_mut(&container_id)
            .expect("contained item has a matching container index");
        assert!(
            contents.remove(&item_id),
            "contained item is present in its container index"
        );
        if contents.is_empty() {
            self.contents_by_container.remove(&container_id);
        }
    }

    fn remove_equipped_index(&mut self, item_id: EntityId, character_id: EntityId, slot: SlotId) {
        let slots = self
            .equipped_by_character
            .get_mut(&character_id)
            .expect("equipped item has a matching bearer index");
        debug_assert_eq!(
            slots.get(&slot),
            Some(&item_id),
            "equipped item is present in its bearer index"
        );
        slots.remove(&slot);
        if slots.is_empty() {
            self.equipped_by_character.remove(&character_id);
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
                    if self
                        .equipped_by_character
                        .values()
                        .any(|slots| slots.values().any(|id| *id == item.id()))
                    {
                        return false;
                    }
                }
                ItemLocation::Equipped { character_id, slot } => {
                    if self
                        .equipped_by_character
                        .get(&character_id)
                        .and_then(|slots| slots.get(&slot))
                        != Some(&item.id())
                    {
                        return false;
                    }
                    if self
                        .ground_by_chunk
                        .values()
                        .any(|ids| ids.contains(&item.id()))
                        || self
                            .carried_by_character
                            .values()
                            .any(|ids| ids.contains(&item.id()))
                    {
                        return false;
                    }
                }
                ItemLocation::Contained { container_id } => {
                    if !self
                        .contents_by_container
                        .get(&container_id)
                        .is_some_and(|ids| ids.contains(&item.id()))
                    {
                        return false;
                    }
                    // A stack must sit in exactly one place, and its container
                    // must actually exist and not enclose it.
                    if self
                        .ground_by_chunk
                        .values()
                        .any(|ids| ids.contains(&item.id()))
                        || self
                            .carried_by_character
                            .values()
                            .any(|ids| ids.contains(&item.id()))
                    {
                        return false;
                    }
                    match self.container_chain(item.id()) {
                        Ok(chain) if chain.len() <= MAX_CONTAINER_DEPTH + 1 => {}
                        _ => return false,
                    }
                }
            }
        }
        let indexed = self
            .ground_by_chunk
            .values()
            .chain(self.carried_by_character.values())
            .chain(self.contents_by_container.values())
            .map(BTreeSet::len)
            .sum::<usize>()
            + self
                .equipped_by_character
                .values()
                .map(BTreeMap::len)
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
    ExpectedEquippedItem(EntityId),
    IndexCorruption,
    ContainerCycle {
        item_id: EntityId,
        container_id: EntityId,
    },
    ContainerTooDeep {
        item_id: EntityId,
        container_id: EntityId,
    },
    SlotAlreadyOccupied {
        character_id: EntityId,
        item_id: EntityId,
    },
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

    /// Containment turns item location from a flat set into a tree, which is
    /// where physical accounting can quietly break. These pin the rules
    /// ADR-0024 settled before any of this was written.
    fn container_fixture() -> (ItemWorld, EntityId, EntityId, EntityId) {
        let mut world = ItemWorld::default();
        let position = WorldPosition::from_cell_center(WorldCell::new(0, 0)).unwrap();
        let cart = id(1);
        let bucket = id(2);
        let goods = id(3);
        for (item, kind) in [
            (cart, item::CART),
            (bucket, item::CART),
            (goods, item::WOOD),
        ] {
            world
                .insert_ground(ItemStack::new_ground(
                    item,
                    kind,
                    ItemQuantity::new(1).unwrap(),
                    position,
                ))
                .unwrap();
        }
        (world, cart, bucket, goods)
    }

    #[test]
    fn nothing_may_be_placed_inside_itself_directly_or_through_a_chain() {
        let (mut world, cart, bucket, _) = container_fixture();
        assert!(matches!(
            world.move_to_container(cart, cart),
            Err(ItemWorldError::ContainerCycle { .. })
        ));
        world.move_to_container(bucket, cart).unwrap();
        assert!(
            matches!(
                world.move_to_container(cart, bucket),
                Err(ItemWorldError::ContainerCycle { .. })
            ),
            "a container swallowed the container it sits inside"
        );
        assert!(world.indexes_are_consistent());
    }

    #[test]
    fn containers_nest_no_deeper_than_the_declared_limit() {
        let (mut world, cart, bucket, goods) = container_fixture();
        world.move_to_container(bucket, cart).unwrap();
        // cart -> bucket is depth two, which is the limit, so nothing may go
        // inside the bucket.
        assert!(matches!(
            world.move_to_container(goods, bucket),
            Err(ItemWorldError::ContainerTooDeep { .. })
        ));
        world.move_to_container(goods, cart).unwrap();
        assert!(world.indexes_are_consistent());
    }

    /// The limit is easy to respect one item at a time and easy to break by
    /// moving a container that already holds something: its contents ride a
    /// level deeper without ever being touched themselves.
    #[test]
    fn moving_a_loaded_container_cannot_smuggle_its_contents_too_deep() {
        let (mut world, cart, bucket, goods) = container_fixture();
        let position = WorldPosition::from_cell_center(WorldCell::new(0, 0)).unwrap();
        world.move_to_container(goods, bucket).unwrap();

        assert!(matches!(
            world.move_to_container(bucket, cart),
            Err(ItemWorldError::ContainerTooDeep { .. })
        ));
        assert_eq!(world.get(bucket).unwrap().ground_position(), Some(position));
        assert_eq!(world.get(goods).unwrap().container(), Some(bucket));
        assert!(world.indexes_are_consistent());
    }

    #[test]
    fn setting_a_container_down_does_not_spill_it() {
        let (mut world, cart, _, goods) = container_fixture();
        let bearer = id(10);
        world.move_to_container(goods, cart).unwrap();
        world.move_to_carried(cart, bearer).unwrap();
        world.equip_carried(cart, bearer, slot::TOOL).unwrap();

        assert_eq!(world.holder_of(goods), Some(bearer));
        assert_eq!(world.contents_of(cart).count(), 1);

        let ground = WorldPosition::from_cell_center(WorldCell::new(5, 5)).unwrap();
        world.unequip_to_carried(cart).unwrap();
        world.move_to_ground(cart, bearer, ground).unwrap();

        assert_eq!(
            world.get(goods).unwrap().container(),
            Some(cart),
            "the goods left the cart when it was set down"
        );
        assert_eq!(world.holder_of(goods), None);
        assert!(world.get(goods).unwrap().ground_position().is_none());
        assert!(world.indexes_are_consistent());
    }

    /// Contents belong to the container, not to whoever is pushing it, and a
    /// stack is never in two places at once.
    #[test]
    fn contents_follow_their_container_and_sit_in_exactly_one_place() {
        let (mut world, cart, _, goods) = container_fixture();
        let first = id(10);
        let second = id(11);
        world.move_to_container(goods, cart).unwrap();
        world.move_to_carried(cart, first).unwrap();
        assert_eq!(world.holder_of(goods), Some(first));
        assert_eq!(world.carried_items_by(first).count(), 1, "only the cart");

        let ground = WorldPosition::from_cell_center(WorldCell::new(2, 0)).unwrap();
        world.move_to_ground(cart, first, ground).unwrap();
        world.move_to_carried(cart, second).unwrap();
        assert_eq!(world.holder_of(goods), Some(second));
        assert_eq!(world.iter().count(), 3);
        assert!(world.indexes_are_consistent());
    }
    use crate::{EntityId, WorldCell, WorldPosition};
    use progressus_content::{item, slot};

    use super::{ItemQuantity, ItemStack, ItemWorld, ItemWorldError, MAX_STACK_QUANTITY};

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
                item::WOOD,
                ItemQuantity::new(1000).unwrap(),
                position,
            ))
            .unwrap();
        world
            .insert_ground(ItemStack::new_ground(
                id(21),
                item::WOOD,
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
                item::WOOD,
                ItemQuantity::new(115).unwrap(),
                position,
            ))
            .unwrap();

        world.split_ground_stack(id(30), id(31), 2).unwrap();

        assert_eq!(world.get(id(30)).unwrap().quantity().get(), 113);
        let split = world.get(id(31)).unwrap();
        assert_eq!(split.kind(), item::WOOD);
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
                item::WOOD,
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
        let item =
            ItemStack::new_ground(id(8), item::WOOD, ItemQuantity::new(7).unwrap(), position);
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
        assert_eq!(carried.kind(), item::WOOD);
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
