pub const CHUNK_SIDE: u16 = 32;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorldCell {
    x: i64,
    y: i64,
}

impl WorldCell {
    pub const fn new(x: i64, y: i64) -> Self {
        Self { x, y }
    }

    pub const fn x(self) -> i64 {
        self.x
    }

    pub const fn y(self) -> i64 {
        self.y
    }

    pub fn split(self) -> (ChunkCoord, LocalCell) {
        let side = i64::from(CHUNK_SIDE);
        (
            ChunkCoord::new(self.x.div_euclid(side), self.y.div_euclid(side)),
            LocalCell::new(
                self.x.rem_euclid(side) as u16,
                self.y.rem_euclid(side) as u16,
            ),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ChunkCoord {
    x: i64,
    y: i64,
}

impl ChunkCoord {
    pub const fn new(x: i64, y: i64) -> Self {
        Self { x, y }
    }

    pub const fn x(self) -> i64 {
        self.x
    }

    pub const fn y(self) -> i64 {
        self.y
    }

    pub fn world_cell(self, local: LocalCell) -> Option<WorldCell> {
        if local.x() >= CHUNK_SIDE || local.y() >= CHUNK_SIDE {
            return None;
        }

        let side = i64::from(CHUNK_SIDE);
        Some(WorldCell::new(
            self.x
                .checked_mul(side)?
                .checked_add(i64::from(local.x()))?,
            self.y
                .checked_mul(side)?
                .checked_add(i64::from(local.y()))?,
        ))
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LocalCell {
    x: u16,
    y: u16,
}

impl LocalCell {
    pub const fn new(x: u16, y: u16) -> Self {
        Self { x, y }
    }

    pub const fn x(self) -> u16 {
        self.x
    }

    pub const fn y(self) -> u16 {
        self.y
    }
}
