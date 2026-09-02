use progressus_sim::{
    Character, ItemKind, ItemStack, Job, JobKind, JobState, MovementState, NaturalResource,
    NaturalResourceKind, WorldPosition,
};

use crate::{ChunkCoord, EntityId, LocalCell, SimulationTick, Terrain, WorldCell, WorldgenVersion};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientSnapshot {
    pub tick: SimulationTick,
    pub worldgen_version: WorldgenVersion,
    pub exploration_revision: u64,
    pub item_revision: u64,
    pub resource_revision: u64,
    pub job_revision: u64,
    pub chunks: Vec<ChunkSnapshot>,
    pub ground_items: Vec<GroundItemSnapshot>,
    pub natural_resources: Vec<NaturalResourceSnapshot>,
    pub jobs: Vec<JobSnapshot>,
    pub characters: Vec<CharacterSnapshot>,
    pub navigation: Option<NavigationSnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroundItemSnapshot {
    pub id: EntityId,
    pub kind: ItemKind,
    pub quantity: u32,
    pub position: WorldPosition,
}

impl GroundItemSnapshot {
    pub(crate) fn from_ground_item(item: &ItemStack) -> Self {
        Self {
            id: item.id(),
            kind: item.kind(),
            quantity: item.quantity().get(),
            position: item
                .ground_position()
                .expect("ground item snapshots are built only from ground items"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NaturalResourceSnapshot {
    pub cell: WorldCell,
    pub kind: NaturalResourceKind,
    pub yield_quantity: u32,
}

impl NaturalResourceSnapshot {
    pub(crate) fn new(cell: WorldCell, resource: NaturalResource) -> Self {
        Self {
            cell,
            kind: resource.kind(),
            yield_quantity: resource.yield_quantity(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobSnapshot {
    pub id: EntityId,
    pub kind: JobKind,
    pub state: JobState,
}

impl From<&Job> for JobSnapshot {
    fn from(job: &Job) -> Self {
        Self {
            id: job.id(),
            kind: job.kind(),
            state: job.state(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NavigationSnapshot {
    pub character_id: EntityId,
    pub destination: Option<WorldPosition>,
    pub remaining_waypoints: Vec<WorldPosition>,
    pub last_tick_motion_trace: Vec<WorldPosition>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KnownTerrain {
    Unknown,
    Known(Terrain),
}

impl From<&Character> for NavigationSnapshot {
    fn from(character: &Character) -> Self {
        Self {
            character_id: character.id(),
            destination: character.navigation_destination(),
            remaining_waypoints: character.navigation_waypoints().collect(),
            last_tick_motion_trace: character.last_tick_motion_trace().to_vec(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChunkSnapshot {
    pub coordinate: ChunkCoord,
    pub side: u16,
    /// Terrain in row-major order: `index = local_y * side + local_x`.
    pub cells: Vec<KnownTerrain>,
}

impl ChunkSnapshot {
    pub fn terrain_at(&self, local: LocalCell) -> Option<KnownTerrain> {
        if local.x() >= self.side || local.y() >= self.side {
            return None;
        }

        let index = usize::from(local.y()) * usize::from(self.side) + usize::from(local.x());
        self.cells.get(index).copied()
    }

    pub fn known_terrain_at(&self, local: LocalCell) -> Option<Terrain> {
        match self.terrain_at(local)? {
            KnownTerrain::Unknown => None,
            KnownTerrain::Known(terrain) => Some(terrain),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacterSnapshot {
    pub id: EntityId,
    pub name: String,
    pub position: WorldPosition,
    pub containing_cell: WorldCell,
    pub movement: MovementState,
}

impl From<&Character> for CharacterSnapshot {
    fn from(character: &Character) -> Self {
        Self {
            id: character.id(),
            name: character.name().to_owned(),
            position: character.position(),
            containing_cell: character.position().containing_cell(),
            movement: character.movement(),
        }
    }
}
