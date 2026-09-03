use std::collections::{BTreeMap, BTreeSet};

use crate::{EntityId, RecipeId};

pub const MAX_PRODUCTION_ORDER_RUNS: u32 = 999_999;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ProductionTarget {
    Finite { remaining_runs: u32 },
    Infinite,
}

impl ProductionTarget {
    pub const fn finite(remaining_runs: u32) -> Self {
        Self::Finite { remaining_runs }
    }

    pub const fn remaining_runs(self) -> Option<u32> {
        match self {
            Self::Finite { remaining_runs } => Some(remaining_runs),
            Self::Infinite => None,
        }
    }

    pub const fn is_pending(self) -> bool {
        match self {
            Self::Finite { remaining_runs } => remaining_runs != 0,
            Self::Infinite => true,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProductionOrder {
    id: EntityId,
    workstation_id: EntityId,
    recipe_id: RecipeId,
    target: ProductionTarget,
}

impl ProductionOrder {
    pub(crate) const fn new(
        id: EntityId,
        workstation_id: EntityId,
        recipe_id: RecipeId,
        target: ProductionTarget,
    ) -> Self {
        Self {
            id,
            workstation_id,
            recipe_id,
            target,
        }
    }

    pub const fn id(&self) -> EntityId {
        self.id
    }
    pub const fn workstation_id(&self) -> EntityId {
        self.workstation_id
    }
    pub const fn recipe_id(&self) -> RecipeId {
        self.recipe_id
    }
    pub const fn target(&self) -> ProductionTarget {
        self.target
    }
    pub const fn remaining_runs(&self) -> Option<u32> {
        self.target.remaining_runs()
    }
    pub const fn is_pending(&self) -> bool {
        self.target.is_pending()
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ProductionWorld {
    orders: BTreeMap<EntityId, ProductionOrder>,
    by_workstation: BTreeMap<EntityId, BTreeSet<EntityId>>,
    revision: u64,
}

impl ProductionWorld {
    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn iter(&self) -> impl ExactSizeIterator<Item = &ProductionOrder> {
        self.orders.values()
    }

    pub(crate) fn get(&self, id: EntityId) -> Option<&ProductionOrder> {
        self.orders.get(&id)
    }

    pub(crate) fn orders_for_workstation(
        &self,
        workstation_id: EntityId,
    ) -> impl Iterator<Item = &ProductionOrder> {
        self.by_workstation
            .get(&workstation_id)
            .into_iter()
            .flatten()
            .filter_map(|id| self.orders.get(id))
    }

    pub(crate) fn first_pending_for_workstation(
        &self,
        workstation_id: EntityId,
    ) -> Option<&ProductionOrder> {
        self.orders_for_workstation(workstation_id)
            .find(|order| order.is_pending())
    }

    pub(crate) fn insert(&mut self, order: ProductionOrder) -> Result<(), ProductionWorldError> {
        validate_target(order.target)?;
        let id = order.id;
        if self.orders.contains_key(&id) {
            return Err(ProductionWorldError::DuplicateOrder(id));
        }
        self.by_workstation
            .entry(order.workstation_id)
            .or_default()
            .insert(id);
        self.orders.insert(id, order);
        self.bump_revision()
    }

    pub(crate) fn set_target(
        &mut self,
        order_id: EntityId,
        target: ProductionTarget,
    ) -> Result<(), ProductionWorldError> {
        validate_target(target)?;
        let order = self
            .orders
            .get_mut(&order_id)
            .ok_or(ProductionWorldError::UnknownOrder(order_id))?;
        if order.target == target {
            return Ok(());
        }
        order.target = target;
        self.bump_revision()
    }

    pub(crate) fn complete_one(&mut self, order_id: EntityId) -> Result<(), ProductionWorldError> {
        let order = self
            .orders
            .get_mut(&order_id)
            .ok_or(ProductionWorldError::UnknownOrder(order_id))?;
        match order.target {
            ProductionTarget::Finite { remaining_runs: 0 } => {
                Err(ProductionWorldError::OrderAlreadyComplete(order_id))
            }
            ProductionTarget::Finite { remaining_runs } => {
                order.target = ProductionTarget::Finite {
                    remaining_runs: remaining_runs - 1,
                };
                self.bump_revision()
            }
            ProductionTarget::Infinite => Ok(()),
        }
    }

    pub(crate) fn remove(
        &mut self,
        order_id: EntityId,
    ) -> Result<ProductionOrder, ProductionWorldError> {
        let order = self
            .orders
            .remove(&order_id)
            .ok_or(ProductionWorldError::UnknownOrder(order_id))?;
        let ids = self
            .by_workstation
            .get_mut(&order.workstation_id)
            .ok_or(ProductionWorldError::IndexCorruption)?;
        if !ids.remove(&order_id) {
            return Err(ProductionWorldError::IndexCorruption);
        }
        if ids.is_empty() {
            self.by_workstation.remove(&order.workstation_id);
        }
        self.bump_revision()?;
        Ok(order)
    }

    pub(crate) fn remove_for_workstation(
        &mut self,
        workstation_id: EntityId,
    ) -> Result<Vec<ProductionOrder>, ProductionWorldError> {
        let ids = self
            .by_workstation
            .remove(&workstation_id)
            .unwrap_or_default();
        let mut removed = Vec::with_capacity(ids.len());
        for id in ids {
            removed.push(
                self.orders
                    .remove(&id)
                    .ok_or(ProductionWorldError::IndexCorruption)?,
            );
        }
        if !removed.is_empty() {
            self.bump_revision()?;
        }
        Ok(removed)
    }

    fn bump_revision(&mut self) -> Result<(), ProductionWorldError> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or(ProductionWorldError::RevisionOverflow)?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn indexes_are_consistent(&self) -> bool {
        self.orders.iter().all(|(id, order)| {
            self.by_workstation
                .get(&order.workstation_id)
                .is_some_and(|ids| ids.contains(id))
        }) && self.by_workstation.iter().all(|(workstation_id, ids)| {
            !ids.is_empty()
                && ids.iter().all(|id| {
                    self.orders
                        .get(id)
                        .is_some_and(|order| order.workstation_id == *workstation_id)
                })
        })
    }
}

fn validate_target(target: ProductionTarget) -> Result<(), ProductionWorldError> {
    if let ProductionTarget::Finite { remaining_runs } = target
        && remaining_runs > MAX_PRODUCTION_ORDER_RUNS
    {
        return Err(ProductionWorldError::QuantityTooLarge(remaining_runs));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProductionWorldError {
    DuplicateOrder(EntityId),
    UnknownOrder(EntityId),
    OrderAlreadyComplete(EntityId),
    QuantityTooLarge(u32),
    IndexCorruption,
    RevisionOverflow,
}

#[cfg(test)]
mod tests {
    use super::{ProductionOrder, ProductionTarget, ProductionWorld};
    use crate::{EntityId, RecipeId};

    fn id(value: u64) -> EntityId {
        EntityId::new(value).unwrap()
    }

    #[test]
    fn workstation_orders_keep_creation_order_and_completed_orders_stay_editable() {
        let mut world = ProductionWorld::default();
        world
            .insert(ProductionOrder::new(
                id(10),
                id(5),
                RecipeId::PrimitiveTool,
                ProductionTarget::finite(2),
            ))
            .unwrap();
        world
            .insert(ProductionOrder::new(
                id(11),
                id(5),
                RecipeId::PrimitiveTool,
                ProductionTarget::finite(3),
            ))
            .unwrap();
        assert_eq!(
            world.first_pending_for_workstation(id(5)).unwrap().id(),
            id(10)
        );
        world.complete_one(id(10)).unwrap();
        world.complete_one(id(10)).unwrap();
        assert_eq!(world.get(id(10)).unwrap().remaining_runs(), Some(0));
        assert_eq!(
            world.first_pending_for_workstation(id(5)).unwrap().id(),
            id(11)
        );
        world
            .set_target(id(10), ProductionTarget::finite(4))
            .unwrap();
        assert_eq!(
            world.first_pending_for_workstation(id(5)).unwrap().id(),
            id(10)
        );
        assert!(world.indexes_are_consistent());
    }

    #[test]
    fn infinite_order_never_counts_down_and_stays_pending() {
        let mut world = ProductionWorld::default();
        world
            .insert(ProductionOrder::new(
                id(10),
                id(5),
                RecipeId::PrimitiveTool,
                ProductionTarget::Infinite,
            ))
            .unwrap();
        let revision = world.revision();
        for _ in 0..4 {
            world.complete_one(id(10)).unwrap();
        }
        assert_eq!(
            world.get(id(10)).unwrap().target(),
            ProductionTarget::Infinite
        );
        assert_eq!(world.revision(), revision);
        assert_eq!(
            world.first_pending_for_workstation(id(5)).unwrap().id(),
            id(10)
        );
    }
}
