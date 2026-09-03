use std::collections::BTreeMap;

use crate::{EntityId, ItemKind, WorldCell};

pub const CONSTRUCT_WORK_TICKS: u32 = 8;
pub const STONE_WALL_COST: u32 = 2;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StructureKind {
    StoneWall,
}

impl StructureKind {
    pub const fn material_kind(self) -> ItemKind {
        match self {
            Self::StoneWall => ItemKind::Stone,
        }
    }

    pub const fn material_quantity(self) -> u32 {
        match self {
            Self::StoneWall => STONE_WALL_COST,
        }
    }

    pub const fn work_ticks(self) -> u32 {
        match self {
            Self::StoneWall => CONSTRUCT_WORK_TICKS,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ConstructionMaterialState {
    Reserved,
    Delivered,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConstructionSite {
    id: EntityId,
    kind: StructureKind,
    cell: WorldCell,
    material_item_id: Option<EntityId>,
    material_state: Option<ConstructionMaterialState>,
}

impl ConstructionSite {
    pub(crate) const fn new(id: EntityId, kind: StructureKind, cell: WorldCell) -> Self {
        Self {
            id,
            kind,
            cell,
            material_item_id: None,
            material_state: None,
        }
    }

    pub const fn id(&self) -> EntityId {
        self.id
    }

    pub const fn kind(&self) -> StructureKind {
        self.kind
    }

    pub const fn cell(&self) -> WorldCell {
        self.cell
    }
    pub const fn material_item_id(&self) -> Option<EntityId> {
        self.material_item_id
    }

    pub const fn material_state(&self) -> Option<ConstructionMaterialState> {
        self.material_state
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Structure {
    id: EntityId,
    kind: StructureKind,
    cell: WorldCell,
}

impl Structure {
    pub(crate) const fn new(id: EntityId, kind: StructureKind, cell: WorldCell) -> Self {
        Self { id, kind, cell }
    }

    pub const fn id(&self) -> EntityId {
        self.id
    }

    pub const fn kind(&self) -> StructureKind {
        self.kind
    }

    pub const fn cell(&self) -> WorldCell {
        self.cell
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ConstructionWorld {
    sites: BTreeMap<EntityId, ConstructionSite>,
    structures: BTreeMap<EntityId, Structure>,
    site_by_cell: BTreeMap<WorldCell, EntityId>,
    structure_by_cell: BTreeMap<WorldCell, EntityId>,
    site_by_material: BTreeMap<EntityId, EntityId>,
    revision: u64,
}
impl ConstructionWorld {
    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn sites(&self) -> impl ExactSizeIterator<Item = &ConstructionSite> {
        self.sites.values()
    }

    pub(crate) fn structures(&self) -> impl ExactSizeIterator<Item = &Structure> {
        self.structures.values()
    }

    pub(crate) fn site(&self, id: EntityId) -> Option<&ConstructionSite> {
        self.sites.get(&id)
    }

    pub(crate) fn site_at(&self, cell: WorldCell) -> Option<EntityId> {
        self.site_by_cell.get(&cell).copied()
    }

    pub(crate) fn structure_at(&self, cell: WorldCell) -> Option<EntityId> {
        self.structure_by_cell.get(&cell).copied()
    }

    pub(crate) fn site_for_material(&self, item_id: EntityId) -> Option<EntityId> {
        self.site_by_material.get(&item_id).copied()
    }

    pub(crate) fn insert_site(
        &mut self,
        site: ConstructionSite,
    ) -> Result<(), ConstructionWorldError> {
        let id = site.id();
        let cell = site.cell();
        if self.sites.contains_key(&id) || self.structures.contains_key(&id) {
            return Err(ConstructionWorldError::DuplicateConstructionId(id));
        }
        if self.site_by_cell.contains_key(&cell) || self.structure_by_cell.contains_key(&cell) {
            return Err(ConstructionWorldError::CellAlreadyOccupied(cell));
        }
        self.site_by_cell.insert(cell, id);
        self.sites.insert(id, site);
        self.bump_revision()
    }
    pub(crate) fn reserve_material(
        &mut self,
        site_id: EntityId,
        item_id: EntityId,
    ) -> Result<(), ConstructionWorldError> {
        let site = self
            .sites
            .get(&site_id)
            .ok_or(ConstructionWorldError::UnknownSite(site_id))?;
        if site.material_item_id.is_some() {
            return Err(ConstructionWorldError::SiteAlreadyHasMaterial(site_id));
        }
        if let Some(existing_site_id) = self.site_by_material.get(&item_id) {
            return Err(ConstructionWorldError::MaterialAlreadyReserved {
                item_id,
                site_id: *existing_site_id,
            });
        }
        let site = self
            .sites
            .get_mut(&site_id)
            .expect("site was checked above");
        site.material_item_id = Some(item_id);
        site.material_state = Some(ConstructionMaterialState::Reserved);
        self.site_by_material.insert(item_id, site_id);
        self.bump_revision()
    }

    pub(crate) fn mark_material_delivered(
        &mut self,
        site_id: EntityId,
        item_id: EntityId,
    ) -> Result<(), ConstructionWorldError> {
        let site = self
            .sites
            .get_mut(&site_id)
            .ok_or(ConstructionWorldError::UnknownSite(site_id))?;
        if site.material_item_id != Some(item_id)
            || self.site_by_material.get(&item_id) != Some(&site_id)
        {
            return Err(ConstructionWorldError::MaterialReservationMismatch { site_id, item_id });
        }
        site.material_state = Some(ConstructionMaterialState::Delivered);
        self.bump_revision()
    }
    pub(crate) fn mark_material_reserved(
        &mut self,
        site_id: EntityId,
        item_id: EntityId,
    ) -> Result<(), ConstructionWorldError> {
        let site = self
            .sites
            .get_mut(&site_id)
            .ok_or(ConstructionWorldError::UnknownSite(site_id))?;
        if site.material_item_id != Some(item_id)
            || self.site_by_material.get(&item_id) != Some(&site_id)
        {
            return Err(ConstructionWorldError::MaterialReservationMismatch { site_id, item_id });
        }
        site.material_state = Some(ConstructionMaterialState::Reserved);
        self.bump_revision()
    }

    pub(crate) fn release_material(
        &mut self,
        site_id: EntityId,
    ) -> Result<Option<EntityId>, ConstructionWorldError> {
        let site = self
            .sites
            .get_mut(&site_id)
            .ok_or(ConstructionWorldError::UnknownSite(site_id))?;
        let Some(item_id) = site.material_item_id.take() else {
            site.material_state = None;
            return Ok(None);
        };
        site.material_state = None;
        if self.site_by_material.remove(&item_id) != Some(site_id) {
            return Err(ConstructionWorldError::IndexCorruption);
        }
        self.bump_revision()?;
        Ok(Some(item_id))
    }

    pub(crate) fn remove_site(
        &mut self,
        site_id: EntityId,
    ) -> Result<ConstructionSite, ConstructionWorldError> {
        let site = self
            .sites
            .remove(&site_id)
            .ok_or(ConstructionWorldError::UnknownSite(site_id))?;
        if self.site_by_cell.remove(&site.cell()) != Some(site_id) {
            return Err(ConstructionWorldError::IndexCorruption);
        }
        if let Some(item_id) = site.material_item_id
            && self.site_by_material.remove(&item_id) != Some(site_id)
        {
            return Err(ConstructionWorldError::IndexCorruption);
        }
        self.bump_revision()?;
        Ok(site)
    }
    pub(crate) fn complete_site(
        &mut self,
        site_id: EntityId,
    ) -> Result<Structure, ConstructionWorldError> {
        let site = self
            .sites
            .remove(&site_id)
            .ok_or(ConstructionWorldError::UnknownSite(site_id))?;
        if self.site_by_cell.remove(&site.cell()) != Some(site_id) {
            return Err(ConstructionWorldError::IndexCorruption);
        }
        if let Some(item_id) = site.material_item_id
            && self.site_by_material.remove(&item_id) != Some(site_id)
        {
            return Err(ConstructionWorldError::IndexCorruption);
        }
        let structure = Structure::new(site.id(), site.kind(), site.cell());
        self.structure_by_cell
            .insert(structure.cell(), structure.id());
        self.structures.insert(structure.id(), structure.clone());
        self.bump_revision()?;
        Ok(structure)
    }

    fn bump_revision(&mut self) -> Result<(), ConstructionWorldError> {
        self.revision = self
            .revision
            .checked_add(1)
            .ok_or(ConstructionWorldError::RevisionOverflow)?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn indexes_are_consistent(&self) -> bool {
        self.sites.iter().all(|(id, site)| {
            self.site_by_cell.get(&site.cell()) == Some(id)
                && site
                    .material_item_id
                    .is_none_or(|item_id| self.site_by_material.get(&item_id) == Some(id))
        }) && self
            .structures
            .iter()
            .all(|(id, structure)| self.structure_by_cell.get(&structure.cell()) == Some(id))
            && self.site_by_material.iter().all(|(item_id, site_id)| {
                self.sites
                    .get(site_id)
                    .is_some_and(|site| site.material_item_id == Some(*item_id))
            })
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConstructionWorldError {
    DuplicateConstructionId(EntityId),
    UnknownSite(EntityId),
    CellAlreadyOccupied(WorldCell),
    SiteAlreadyHasMaterial(EntityId),
    MaterialAlreadyReserved {
        item_id: EntityId,
        site_id: EntityId,
    },
    MaterialReservationMismatch {
        site_id: EntityId,
        item_id: EntityId,
    },
    IndexCorruption,
    RevisionOverflow,
}

#[cfg(test)]
mod tests {
    use crate::{EntityId, WorldCell};

    use super::{ConstructionMaterialState, ConstructionSite, ConstructionWorld, StructureKind};

    fn id(value: u64) -> EntityId {
        EntityId::new(value).unwrap()
    }

    #[test]
    fn site_material_and_structure_transitions_keep_indexes_consistent() {
        let mut world = ConstructionWorld::default();
        let cell = WorldCell::new(4, -3);
        world
            .insert_site(ConstructionSite::new(
                id(20),
                StructureKind::StoneWall,
                cell,
            ))
            .unwrap();
        world.reserve_material(id(20), id(6)).unwrap();
        assert_eq!(world.site_for_material(id(6)), Some(id(20)));
        assert_eq!(
            world.site(id(20)).unwrap().material_state(),
            Some(ConstructionMaterialState::Reserved)
        );
        world.mark_material_delivered(id(20), id(6)).unwrap();
        assert!(world.indexes_are_consistent());

        let structure = world.complete_site(id(20)).unwrap();
        assert_eq!(structure.id(), id(20));
        assert_eq!(world.structure_at(cell), Some(id(20)));
        assert_eq!(world.site_for_material(id(6)), None);
        assert!(world.indexes_are_consistent());
    }
}
