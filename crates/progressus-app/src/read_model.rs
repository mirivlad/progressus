use progressus_sim::{Character, GeneratedChunk};

use crate::{
    CHUNK_SIDE, ChunkCoord, EntityId, LocalCell, SimulationTick, Terrain, WorldCell,
    WorldgenVersion,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientSnapshot {
    pub tick: SimulationTick,
    pub worldgen_version: WorldgenVersion,
    pub chunks: Vec<ChunkSnapshot>,
    pub characters: Vec<CharacterSnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChunkSnapshot {
    pub coordinate: ChunkCoord,
    pub side: u16,
    /// Terrain in row-major order: `index = local_y * side + local_x`.
    pub cells: Vec<Terrain>,
}

impl ChunkSnapshot {
    pub fn terrain_at(&self, local: LocalCell) -> Option<Terrain> {
        if local.x() >= self.side || local.y() >= self.side {
            return None;
        }

        let index = usize::from(local.y()) * usize::from(self.side) + usize::from(local.x());
        self.cells.get(index).copied()
    }
}

impl From<GeneratedChunk> for ChunkSnapshot {
    fn from(chunk: GeneratedChunk) -> Self {
        Self {
            coordinate: chunk.coordinate(),
            side: CHUNK_SIDE,
            cells: chunk.cells().to_vec(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CharacterSnapshot {
    pub id: EntityId,
    pub name: String,
    pub position: WorldCell,
}

impl From<&Character> for CharacterSnapshot {
    fn from(character: &Character) -> Self {
        Self {
            id: character.id(),
            name: character.name().to_owned(),
            position: character.position(),
        }
    }
}
