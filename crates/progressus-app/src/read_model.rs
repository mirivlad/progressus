use progressus_sim::{Character, GeneratedChunk};

use crate::{
    CHUNK_SIDE, ChunkCoord, EntityId, SimulationTick, Terrain, WorldCell, WorldgenVersion,
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
    pub cells: Vec<Terrain>,
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
