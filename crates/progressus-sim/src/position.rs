use crate::WorldCell;

pub const SUBUNITS_PER_CELL: i128 = 1024;
const CELL_CENTER_OFFSET: i128 = SUBUNITS_PER_CELL / 2;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorldPosition {
    x_subunits: i128,
    y_subunits: i128,
}

impl WorldPosition {
    pub fn from_subunits(x_subunits: i128, y_subunits: i128) -> Result<Self, WorldPositionError> {
        if !axis_is_representable(x_subunits) || !axis_is_representable(y_subunits) {
            return Err(WorldPositionError::OutsideWorldCellRange);
        }

        Ok(Self {
            x_subunits,
            y_subunits,
        })
    }

    pub fn from_cell_origin(cell: WorldCell) -> Result<Self, WorldPositionError> {
        Self::from_cell_offset(cell, 0)
    }

    pub fn from_cell_center(cell: WorldCell) -> Result<Self, WorldPositionError> {
        Self::from_cell_offset(cell, CELL_CENTER_OFFSET)
    }

    pub const fn x_subunits(self) -> i128 {
        self.x_subunits
    }

    pub const fn y_subunits(self) -> i128 {
        self.y_subunits
    }

    pub fn containing_cell(self) -> WorldCell {
        WorldCell::new(
            i64::try_from(self.x_subunits.div_euclid(SUBUNITS_PER_CELL))
                .expect("valid world positions have representable x cells"),
            i64::try_from(self.y_subunits.div_euclid(SUBUNITS_PER_CELL))
                .expect("valid world positions have representable y cells"),
        )
    }

    pub fn checked_translate(
        self,
        delta_x_subunits: i128,
        delta_y_subunits: i128,
    ) -> Result<Self, WorldPositionError> {
        let x_subunits = self
            .x_subunits
            .checked_add(delta_x_subunits)
            .ok_or(WorldPositionError::OutsideWorldCellRange)?;
        let y_subunits = self
            .y_subunits
            .checked_add(delta_y_subunits)
            .ok_or(WorldPositionError::OutsideWorldCellRange)?;
        Self::from_subunits(x_subunits, y_subunits)
    }

    fn from_cell_offset(cell: WorldCell, offset: i128) -> Result<Self, WorldPositionError> {
        let x_subunits = i128::from(cell.x())
            .checked_mul(SUBUNITS_PER_CELL)
            .and_then(|value| value.checked_add(offset))
            .ok_or(WorldPositionError::OutsideWorldCellRange)?;
        let y_subunits = i128::from(cell.y())
            .checked_mul(SUBUNITS_PER_CELL)
            .and_then(|value| value.checked_add(offset))
            .ok_or(WorldPositionError::OutsideWorldCellRange)?;
        Self::from_subunits(x_subunits, y_subunits)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldPositionError {
    OutsideWorldCellRange,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InteractionRadius(u32);

impl InteractionRadius {
    pub const fn zero() -> Self {
        Self(0)
    }

    pub const fn new(subunits: u32) -> Self {
        Self(subunits)
    }

    pub const fn subunits(self) -> u32 {
        self.0
    }
}

pub fn within_interaction_range(
    first: WorldPosition,
    first_radius: InteractionRadius,
    second: WorldPosition,
    second_radius: InteractionRadius,
) -> bool {
    let reach =
        i128::from(u64::from(first_radius.subunits()) + u64::from(second_radius.subunits()));
    let delta_x = first.x_subunits() - second.x_subunits();
    let delta_y = first.y_subunits() - second.y_subunits();
    let absolute_x = delta_x.abs();
    let absolute_y = delta_y.abs();

    if absolute_x > reach || absolute_y > reach {
        return false;
    }

    absolute_x * absolute_x + absolute_y * absolute_y <= reach * reach
}

fn axis_is_representable(subunits: i128) -> bool {
    i64::try_from(subunits.div_euclid(SUBUNITS_PER_CELL)).is_ok()
}
