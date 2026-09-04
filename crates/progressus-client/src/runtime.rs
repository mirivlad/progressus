use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{self, Display, Formatter};

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use progressus_app::{
    Application, ApplicationError, ChunkCoord, ClientSnapshot, Command, EntityId, JobKind,
    NewGameOptions, SnapshotQuery, StructureKind, WorkstationKind, WorldCell, WorldPosition,
    WorldSeed,
};

use crate::i18n::Locale;
use crate::interaction::{TickScheduler, movement_command};
use crate::modal::{
    ModalPresentation, ModalState, modal_interaction, modal_keyboard, save_modal_interaction,
    sync_modal,
};
use crate::navigation::{SelectedCharacter, VisualMotion, quantize_local_click, select_nearest};
use crate::presentation::PresentationError;
use crate::procedural_assets::ProceduralAssetRegistry;
use crate::render::{
    NavigationDebug, PresentationCache, camera_controls, draw_job_designations,
    draw_selected_character, draw_selected_navigation, draw_stockpiles, draw_tool_drag,
    setup_camera, sync_presentation,
};
use crate::save_slots::SaveStore;
use crate::ui::{
    HudPaletteState, SelectedStockpile, StockpileClickState, ToolMode, ToolState, ZoneVisibility,
    configure_stockpile_interaction, hud_palette_interaction, language_toggle_interaction,
    pause_toggle_interaction, refresh_toolbar_localization, save_menu_interaction,
    setup_character_inspector, setup_stockpile_inspector, setup_toolbar, sync_character_inspector,
    sync_hud_tooltip, sync_stockpile_inspector, toolbar_interaction, update_ui_capture,
    zone_visibility_interaction,
};
use crate::ui_font::setup_ui_font;

impl Resource for TickScheduler {}
impl Resource for SaveStore {}

#[derive(Resource)]
pub(crate) struct AuthoritativeClient {
    application: Application,
    snapshot: ClientSnapshot,
    snapshot_dirty: bool,
}

impl AuthoritativeClient {
    #[cfg(test)]
    pub(crate) fn new() -> Result<Self, ClientError> {
        Self::new_with_seed(WorldSeed::new(0))
    }

    pub(crate) fn new_with_seed(seed: WorldSeed) -> Result<Self, ClientError> {
        let application = Application::new_game(NewGameOptions { seed })?;
        let snapshot = application.snapshot(SnapshotQuery::default())?;
        Ok(Self {
            application,
            snapshot,
            snapshot_dirty: true,
        })
    }

    pub(crate) fn snapshot(&self) -> &ClientSnapshot {
        &self.snapshot
    }

    pub(crate) fn application_mut(&mut self) -> &mut Application {
        &mut self.application
    }

    pub(crate) fn save_json(&self) -> Result<Vec<u8>, ClientError> {
        Ok(self.application.save_json()?)
    }

    pub(crate) fn load_json(&mut self, bytes: &[u8]) -> Result<(), ClientError> {
        let application = Application::from_save_json(bytes)?;
        let snapshot = application.snapshot(SnapshotQuery::default())?;
        self.application = application;
        self.snapshot = snapshot;
        self.snapshot_dirty = true;
        Ok(())
    }

    pub(crate) fn spatial_snapshot(
        &self,
        chunks: Vec<ChunkCoord>,
        include_terrain: bool,
        include_ground_items: bool,
        include_natural_resources: bool,
    ) -> Result<ClientSnapshot, ClientError> {
        Ok(self.application.snapshot(SnapshotQuery {
            chunks,
            include_terrain,
            include_ground_items,
            include_natural_resources,
            ..SnapshotQuery::default()
        })?)
    }

    #[cfg(test)]
    pub(crate) fn terrain_snapshot(
        &self,
        chunks: Vec<ChunkCoord>,
    ) -> Result<ClientSnapshot, ClientError> {
        self.spatial_snapshot(chunks, true, true, true)
    }

    pub(crate) fn take_snapshot_dirty(&mut self) -> bool {
        std::mem::take(&mut self.snapshot_dirty)
    }

    pub(crate) fn refresh_lightweight_snapshot(
        &mut self,
        navigation_for: Option<EntityId>,
    ) -> Result<(), ClientError> {
        self.snapshot = self.application.snapshot(SnapshotQuery {
            navigation_for,
            ..SnapshotQuery::default()
        })?;
        self.snapshot_dirty = true;
        Ok(())
    }
}

pub(crate) fn cora_id() -> EntityId {
    EntityId::new(3).expect("3 is a valid nonzero Progressus entity ID")
}

pub(crate) fn advance_authority(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut scheduler: ResMut<TickScheduler>,
    mut authoritative: ResMut<AuthoritativeClient>,
    selected: Res<SelectedCharacter>,
    modal: Res<ModalState>,
) {
    let mut command_attempted = false;
    if !modal.is_open()
        && let Some(command) = movement_command(&keys, cora_id())
    {
        command_attempted = true;
        if let Err(error) = authoritative.application.execute(command) {
            warn!("movement command rejected: {error}");
        }
    }
    let tick_due = scheduler.advance(time.delta());
    if tick_due
        && let Err(error) = authoritative
            .application
            .execute(Command::AdvanceTicks { count: 1 })
    {
        error!("authoritative tick failed: {error}");
        return;
    }
    if (command_attempted || tick_due)
        && let Err(error) = authoritative.refresh_lightweight_snapshot(selected.0)
    {
        error!("authoritative snapshot failed: {error}");
    }
}

#[allow(clippy::type_complexity)]
pub(crate) fn pointer_navigation(
    input: (Res<ButtonInput<MouseButton>>, Res<ButtonInput<KeyCode>>),
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera2d>>,
    interaction_state: (
        ResMut<SelectedCharacter>,
        ResMut<SelectedStockpile>,
        ResMut<StockpileClickState>,
        ResMut<ToolState>,
        ResMut<ModalState>,
    ),
    time: Res<Time>,
    mut authoritative: ResMut<AuthoritativeClient>,
    cache: Res<PresentationCache>,
) {
    let (buttons, keys) = input;
    let (mut selected, mut selected_stockpile, mut stockpile_click, mut tool, mut modal) =
        interaction_state;
    if modal.is_open() {
        return;
    }
    if buttons.pressed(MouseButton::Middle) || buttons.just_released(MouseButton::Middle) {
        return;
    }
    let tool_active = tool.mode != ToolMode::Select;
    let area_tool = tool.mode.uses_area_drag();

    if tool_active && buttons.just_pressed(MouseButton::Right) {
        tool.mode = ToolMode::Select;
        tool.cancel_drag();
        return;
    }
    if tool.pointer_over_ui && tool.drag_start.is_none() {
        return;
    }
    let needs_pointer = buttons.just_pressed(MouseButton::Left)
        || buttons.just_pressed(MouseButton::Right)
        || (area_tool
            && (buttons.pressed(MouseButton::Left) || buttons.just_released(MouseButton::Left)));
    if !needs_pointer {
        return;
    }

    let (Ok(window), Ok((camera, camera_transform))) = (windows.single(), cameras.single()) else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Some(world) = camera.viewport_to_world_2d(camera_transform, cursor).ok() else {
        return;
    };
    let Some(origin_cell) = cache.render_origin else {
        return;
    };
    let Ok(origin) = progressus_app::WorldPosition::from_cell_center(origin_cell) else {
        return;
    };
    let Ok(target) = quantize_local_click(origin, world.x, world.y) else {
        warn!("pointer position cannot be represented as an authoritative world position");
        return;
    };

    let modified_left = keys.any_pressed([
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::ShiftLeft,
        KeyCode::ShiftRight,
    ]);
    if buttons.just_pressed(MouseButton::Left) && !modified_left {
        // Selectable physical objects have priority over the current tool. The
        // tool remains active so the player can inspect something and then
        // immediately continue the previous designation/build action. Zone
        // editing keeps direct access to stockpile cells, but characters and
        // workstations still win over the zone tool.
        let allow_stockpile_selection = !matches!(
            tool.mode,
            ToolMode::StockpileAdd | ToolMode::StockpileRemove
        );
        if handle_selectable_click(
            authoritative.snapshot(),
            target,
            time.elapsed_secs(),
            &mut selected,
            &mut selected_stockpile,
            &mut stockpile_click,
            &mut modal,
            allow_stockpile_selection,
        ) {
            tool.cancel_drag();
            if let Err(error) = authoritative.refresh_lightweight_snapshot(selected.0) {
                error!("authoritative snapshot failed after selection: {error}");
            }
            return;
        }
    }

    if tool_active {
        let cell = target.containing_cell();
        if !area_tool {
            if buttons.just_pressed(MouseButton::Left) {
                match apply_point_tool(&mut authoritative, tool.mode, cell) {
                    Ok(()) => {
                        if let Err(error) = authoritative.refresh_lightweight_snapshot(selected.0) {
                            error!("authoritative snapshot failed after tool action: {error}");
                        }
                    }
                    Err(error) => warn!("tool action rejected: {error}"),
                }
            }
            return;
        }
        if buttons.just_pressed(MouseButton::Left) {
            tool.drag_start = Some(cell);
            tool.drag_current = Some(cell);
            return;
        }
        if buttons.pressed(MouseButton::Left) && tool.drag_start.is_some() {
            tool.drag_current = Some(cell);
        }
        if buttons.just_released(MouseButton::Left) {
            let (Some(first), Some(last)) = (tool.drag_start, tool.drag_current) else {
                tool.cancel_drag();
                return;
            };
            let mode = tool.mode;
            if first == last
                && matches!(mode, ToolMode::StockpileAdd | ToolMode::StockpileRemove)
                && stockpile_at(authoritative.snapshot(), last).is_some()
            {
                tool.cancel_drag();
                if handle_selectable_click(
                    authoritative.snapshot(),
                    target,
                    time.elapsed_secs(),
                    &mut selected,
                    &mut selected_stockpile,
                    &mut stockpile_click,
                    &mut modal,
                    true,
                ) {
                    if let Err(error) = authoritative.refresh_lightweight_snapshot(selected.0) {
                        error!("authoritative snapshot failed after stockpile selection: {error}");
                    }
                    return;
                }
            }
            tool.cancel_drag();
            match apply_tool_area(&mut authoritative, mode, first, last) {
                Ok(()) => {
                    if let Err(error) = authoritative.refresh_lightweight_snapshot(selected.0) {
                        error!("authoritative snapshot failed after area designation: {error}");
                    }
                }
                Err(error) => warn!("area designation rejected: {error}"),
            }
        }
        return;
    }

    if buttons.just_pressed(MouseButton::Left)
        && keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight])
    {
        let cell = target.containing_cell();
        let command = if let Some(stockpile_id) = stockpile_at(authoritative.snapshot(), cell) {
            Command::SetStockpileCell {
                stockpile_id,
                cell,
                enabled: false,
            }
        } else {
            // The single-cell shortcut creates an independent stockpile on an
            // empty cell. It must never silently attach the cell to an
            // unrelated global stockpile with a different item policy.
            Command::CreateStockpile { cell }
        };
        match authoritative.application.execute(command) {
            Ok(()) => {
                if let Err(error) = authoritative.refresh_lightweight_snapshot(selected.0) {
                    error!("authoritative snapshot failed after stockpile edit: {error}");
                }
            }
            Err(error) => warn!("stockpile edit rejected: {error}"),
        }
        return;
    }

    if buttons.just_pressed(MouseButton::Left)
        && keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight])
    {
        let source = target.containing_cell();
        let command = harvest_job_at(authoritative.snapshot(), source)
            .map_or(Command::DesignateHarvest { source }, |job_id| {
                Command::CancelJob { job_id }
            });
        match authoritative.application.execute(command) {
            Ok(()) => {
                if let Err(error) = authoritative.refresh_lightweight_snapshot(selected.0) {
                    error!("authoritative snapshot failed after harvest designation: {error}");
                }
            }
            Err(error) => warn!("harvest designation rejected: {error}"),
        }
        return;
    }

    if buttons.just_pressed(MouseButton::Left) {
        selected.0 = None;
        selected_stockpile.0 = None;
        stockpile_click.last = None;
        if let Err(error) = authoritative.refresh_lightweight_snapshot(selected.0) {
            error!("authoritative snapshot failed after selection clear: {error}");
        }
        return;
    }

    let Some(character_id) = selected.0 else {
        return;
    };
    match authoritative.application.execute(Command::MoveTo {
        character_id,
        destination: target,
    }) {
        Ok(()) => {
            if let Err(error) = authoritative.refresh_lightweight_snapshot(Some(character_id)) {
                error!("authoritative snapshot failed after move command: {error}");
            }
        }
        Err(error) => warn!("move command rejected: {error}"),
    }
}

const MAX_DESIGNATION_CELLS: usize = 4096;

fn apply_point_tool(
    authoritative: &mut AuthoritativeClient,
    mode: ToolMode,
    cell: WorldCell,
) -> Result<(), ClientError> {
    match mode {
        ToolMode::Door => {
            authoritative
                .application
                .execute(Command::DesignateConstruction {
                    kind: StructureKind::Door,
                    cell,
                })?;
        }
        ToolMode::Workbench => {
            if workstation_at(authoritative.snapshot(), cell).is_none() {
                authoritative
                    .application
                    .execute(Command::PlaceWorkstation {
                        kind: WorkstationKind::Workbench,
                        cell,
                    })?;
            }
        }
        ToolMode::Select
        | ToolMode::StockpileAdd
        | ToolMode::StockpileRemove
        | ToolMode::Harvest
        | ToolMode::Wall
        | ToolMode::CancelJobs => {}
    }
    Ok(())
}

fn apply_tool_area(
    authoritative: &mut AuthoritativeClient,
    mode: ToolMode,
    first: WorldCell,
    last: WorldCell,
) -> Result<(), ClientError> {
    let cells = rectangle_cells(first, last).ok_or(ClientError::Presentation(
        PresentationError::DesignationAreaTooLarge,
    ))?;
    let chunks = cells
        .iter()
        .map(|cell| cell.split().0)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let area_snapshot = authoritative.application.snapshot(SnapshotQuery {
        chunks,
        ..SnapshotQuery::default()
    })?;

    match mode {
        ToolMode::Select => {}
        ToolMode::StockpileAdd => {
            let resource_cells = area_snapshot
                .natural_resources
                .iter()
                .map(|resource| resource.cell)
                .collect::<BTreeSet<_>>();
            let production_zone_cells = area_snapshot
                .production_logistics
                .iter()
                .flat_map(|logistics| {
                    logistics
                        .input_cells
                        .iter()
                        .chain(logistics.output_cells.iter())
                        .copied()
                })
                .collect::<BTreeSet<_>>();
            let owner_by_cell = area_snapshot
                .stockpiles
                .iter()
                .flat_map(|stockpile| {
                    stockpile
                        .cells
                        .iter()
                        .copied()
                        .map(move |cell| (cell, stockpile.id))
                })
                .collect::<BTreeMap<_, _>>();
            let overlapping = cells
                .iter()
                .filter_map(|cell| owner_by_cell.get(cell).copied())
                .collect::<BTreeSet<_>>();
            // Extend only one stockpile that the painted rectangle actually
            // overlaps. A fresh region gets a new ID, and painting across
            // multiple existing stockpiles never merges their policies.
            let mut stockpile_id = (overlapping.len() == 1)
                .then(|| *overlapping.first().expect("one overlap has one id"));
            for cell in cells {
                if known_terrain_at(&area_snapshot, cell) != Some(progressus_app::Terrain::Grass)
                    || resource_cells.contains(&cell)
                    || production_zone_cells.contains(&cell)
                {
                    continue;
                }
                if owner_by_cell.contains_key(&cell) {
                    continue;
                }
                if let Some(id) = stockpile_id {
                    authoritative
                        .application
                        .execute(Command::SetStockpileCell {
                            stockpile_id: id,
                            cell,
                            enabled: true,
                        })?;
                } else {
                    authoritative
                        .application
                        .execute(Command::CreateStockpile { cell })?;
                    let refreshed = authoritative
                        .application
                        .snapshot(SnapshotQuery::default())?;
                    stockpile_id = stockpile_at(&refreshed, cell);
                }
            }
        }
        ToolMode::StockpileRemove => {
            let owners = area_snapshot
                .stockpiles
                .iter()
                .flat_map(|stockpile| {
                    stockpile
                        .cells
                        .iter()
                        .copied()
                        .map(move |cell| (cell, stockpile.id))
                })
                .collect::<BTreeMap<_, _>>();
            for cell in cells {
                if let Some(stockpile_id) = owners.get(&cell).copied() {
                    authoritative
                        .application
                        .execute(Command::SetStockpileCell {
                            stockpile_id,
                            cell,
                            enabled: false,
                        })?;
                }
            }
        }
        ToolMode::Harvest => {
            let existing = area_snapshot
                .jobs
                .iter()
                .filter_map(|job| match job.kind {
                    JobKind::Harvest { source } => Some(source),
                    JobKind::Eat { .. }
                    | JobKind::Haul { .. }
                    | JobKind::SupplyProduction { .. }
                    | JobKind::Craft { .. }
                    | JobKind::DeliverConstruction { .. }
                    | JobKind::Construct { .. } => None,
                })
                .collect::<BTreeSet<_>>();
            let selected = cells.into_iter().collect::<BTreeSet<_>>();
            for resource in area_snapshot.natural_resources {
                if selected.contains(&resource.cell) && !existing.contains(&resource.cell) {
                    authoritative
                        .application
                        .execute(Command::DesignateHarvest {
                            source: resource.cell,
                        })?;
                }
            }
        }
        ToolMode::Wall => {
            let resource_cells = area_snapshot
                .natural_resources
                .iter()
                .map(|resource| resource.cell)
                .collect::<BTreeSet<_>>();
            let stockpile_cells = area_snapshot
                .stockpiles
                .iter()
                .flat_map(|stockpile| stockpile.cells.iter().copied())
                .collect::<BTreeSet<_>>();
            let workstation_cells = area_snapshot
                .workstations
                .iter()
                .map(|workstation| workstation.cell)
                .collect::<BTreeSet<_>>();
            let item_cells = area_snapshot
                .ground_items
                .iter()
                .map(|item| item.position.containing_cell())
                .collect::<BTreeSet<_>>();
            let production_zone_cells = area_snapshot
                .production_logistics
                .iter()
                .flat_map(|logistics| {
                    logistics
                        .input_cells
                        .iter()
                        .chain(logistics.output_cells.iter())
                        .copied()
                })
                .collect::<BTreeSet<_>>();
            let construction_cells = area_snapshot
                .construction_sites
                .iter()
                .map(|site| site.cell)
                .chain(
                    area_snapshot
                        .structures
                        .iter()
                        .map(|structure| structure.cell),
                )
                .collect::<BTreeSet<_>>();
            let character_cells = area_snapshot
                .characters
                .iter()
                .map(|character| character.containing_cell)
                .collect::<BTreeSet<_>>();
            for cell in cells {
                if known_terrain_at(&area_snapshot, cell) != Some(progressus_app::Terrain::Grass)
                    || resource_cells.contains(&cell)
                    || stockpile_cells.contains(&cell)
                    || workstation_cells.contains(&cell)
                    || item_cells.contains(&cell)
                    || production_zone_cells.contains(&cell)
                    || construction_cells.contains(&cell)
                    || character_cells.contains(&cell)
                {
                    continue;
                }
                authoritative
                    .application
                    .execute(Command::DesignateConstruction {
                        kind: StructureKind::StoneWall,
                        cell,
                    })?;
            }
        }
        ToolMode::Door | ToolMode::Workbench => {}
        ToolMode::CancelJobs => {
            let selected = cells.into_iter().collect::<BTreeSet<_>>();
            let jobs = area_snapshot
                .jobs
                .iter()
                .filter_map(|job| match job.kind {
                    JobKind::Harvest { source } if selected.contains(&source) => Some(job.id),
                    JobKind::Harvest { .. }
                    | JobKind::Eat { .. }
                    | JobKind::Haul { .. }
                    | JobKind::SupplyProduction { .. }
                    | JobKind::Craft { .. }
                    | JobKind::DeliverConstruction { .. }
                    | JobKind::Construct { .. } => None,
                })
                .collect::<Vec<_>>();
            for job_id in jobs {
                authoritative
                    .application
                    .execute(Command::CancelJob { job_id })?;
            }
            let sites = area_snapshot
                .construction_sites
                .iter()
                .filter(|site| selected.contains(&site.cell))
                .map(|site| site.id)
                .collect::<Vec<_>>();
            for site_id in sites {
                authoritative
                    .application
                    .execute(Command::CancelConstruction { site_id })?;
            }
        }
    }
    Ok(())
}

fn rectangle_cells(first: WorldCell, last: WorldCell) -> Option<Vec<WorldCell>> {
    let min_x = first.x().min(last.x());
    let max_x = first.x().max(last.x());
    let min_y = first.y().min(last.y());
    let max_y = first.y().max(last.y());
    let width = i128::from(max_x) - i128::from(min_x) + 1;
    let height = i128::from(max_y) - i128::from(min_y) + 1;
    let count = width.checked_mul(height)?;
    if count > MAX_DESIGNATION_CELLS as i128 {
        return None;
    }
    let mut cells = Vec::with_capacity(count as usize);
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            cells.push(WorldCell::new(x, y));
        }
    }
    Some(cells)
}

fn known_terrain_at(snapshot: &ClientSnapshot, cell: WorldCell) -> Option<progressus_app::Terrain> {
    let (coordinate, local) = cell.split();
    snapshot
        .chunks
        .iter()
        .find(|chunk| chunk.coordinate == coordinate)?
        .known_terrain_at(local)
}

fn harvest_job_at(snapshot: &ClientSnapshot, source: WorldCell) -> Option<EntityId> {
    snapshot.jobs.iter().find_map(|job| match job.kind {
        JobKind::Harvest { source: job_source } if job_source == source => Some(job.id),
        _ => None,
    })
}

#[allow(clippy::too_many_arguments)]
fn handle_selectable_click(
    snapshot: &ClientSnapshot,
    target: WorldPosition,
    now: f32,
    selected: &mut SelectedCharacter,
    selected_stockpile: &mut SelectedStockpile,
    stockpile_click: &mut StockpileClickState,
    modal: &mut ModalState,
    allow_stockpile_selection: bool,
) -> bool {
    if let Some(workstation_id) = workstation_at(snapshot, target.containing_cell()) {
        selected.0 = None;
        selected_stockpile.0 = None;
        stockpile_click.last = None;
        modal.open_workstation(workstation_id);
        return true;
    }
    if let Some(character_id) = select_nearest(
        snapshot
            .characters
            .iter()
            .map(|character| (character.id, character.position)),
        target,
        progressus_app::SUBUNITS_PER_CELL / 2,
    ) {
        selected.0 = Some(character_id);
        selected_stockpile.0 = None;
        stockpile_click.last = None;
        return true;
    }
    if allow_stockpile_selection
        && let Some(stockpile_id) = stockpile_at(snapshot, target.containing_cell())
    {
        selected.0 = None;
        selected_stockpile.0 = Some(stockpile_id);
        let double_click = stockpile_click
            .last
            .is_some_and(|(id, at)| id == stockpile_id && now - at <= 0.35);
        stockpile_click.last = Some((stockpile_id, now));
        if double_click {
            modal.open_stockpile(stockpile_id);
        }
        return true;
    }
    false
}

fn workstation_at(snapshot: &ClientSnapshot, cell: WorldCell) -> Option<EntityId> {
    snapshot
        .workstations
        .iter()
        .find(|workstation| workstation.cell == cell)
        .map(|workstation| workstation.id)
}

fn stockpile_at(snapshot: &ClientSnapshot, cell: WorldCell) -> Option<EntityId> {
    snapshot
        .stockpiles
        .iter()
        .find(|stockpile| stockpile.cells.binary_search(&cell).is_ok())
        .map(|stockpile| stockpile.id)
}

#[derive(Debug)]
pub enum ClientError {
    Application(ApplicationError),
    Presentation(PresentationError),
}

impl Display for ClientError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Application(error) => Display::fmt(error, formatter),
            Self::Presentation(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for ClientError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Application(error) => Some(error),
            Self::Presentation(error) => Some(error),
        }
    }
}

impl From<ApplicationError> for ClientError {
    fn from(error: ApplicationError) -> Self {
        Self::Application(error)
    }
}

impl From<PresentationError> for ClientError {
    fn from(error: PresentationError) -> Self {
        Self::Presentation(error)
    }
}

impl Display for PresentationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::DesignationAreaTooLarge => {
                formatter.write_str("designation area exceeds 4096 cells")
            }
            Self::VisibleWindowOutOfRange { center } => write!(
                formatter,
                "a complete visible chunk window cannot be formed around ({}, {})",
                center.x(),
                center.y()
            ),
        }
    }
}

impl Error for PresentationError {}

pub fn run() -> Result<(), ClientError> {
    run_with_seed(0)
}

pub fn run_with_seed(seed: u64) -> Result<(), ClientError> {
    App::new()
        .insert_resource(AuthoritativeClient::new_with_seed(WorldSeed::new(seed))?)
        .insert_resource(TickScheduler::default())
        .insert_resource(PresentationCache::default())
        .insert_resource(ProceduralAssetRegistry::default())
        .insert_resource(NavigationDebug::default())
        .insert_resource(SelectedCharacter::default())
        .insert_resource(SelectedStockpile::default())
        .insert_resource(StockpileClickState::default())
        .insert_resource(VisualMotion::default())
        .insert_resource(ToolState::default())
        .insert_resource(HudPaletteState::default())
        .insert_resource(ZoneVisibility::default())
        .insert_resource(Locale::default())
        .insert_resource(ModalState::default())
        .insert_resource(ModalPresentation::default())
        .insert_resource(SaveStore::default())
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: format!("Progressus — Prototype 01 — seed {seed}"),
                ..default()
            }),
            ..default()
        }))
        .add_systems(
            Startup,
            (
                setup_ui_font,
                setup_camera,
                setup_toolbar,
                setup_character_inspector,
                setup_stockpile_inspector,
            )
                .chain(),
        )
        .add_systems(
            Update,
            (
                (
                    language_toggle_interaction,
                    pause_toggle_interaction,
                    zone_visibility_interaction,
                    modal_keyboard,
                    save_menu_interaction,
                    configure_stockpile_interaction,
                    save_modal_interaction,
                    modal_interaction,
                    toolbar_interaction,
                    hud_palette_interaction,
                    refresh_toolbar_localization,
                    sync_hud_tooltip,
                    sync_modal,
                    update_ui_capture,
                    pointer_navigation,
                )
                    .chain(),
                (
                    advance_authority,
                    sync_presentation,
                    sync_character_inspector,
                    sync_stockpile_inspector,
                    crate::render::interpolate_character_visuals,
                    draw_selected_character,
                    draw_selected_navigation,
                    crate::render::draw_navigation_debug,
                    draw_tool_drag,
                    draw_job_designations,
                    draw_stockpiles,
                    camera_controls,
                )
                    .chain(),
            )
                .chain(),
        )
        .run();
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, VecDeque, btree_map::Entry};
    use std::time::Duration;

    use bevy::prelude::{
        App, Assets, ButtonInput, Camera2d, Children, Entity, Image, IntoScheduleConfigs, KeyCode,
        Time, Transform, Update, Vec3,
    };
    use progressus_app::{
        Application, CHUNK_SIDE, CharacterSnapshot, ChunkCoord, ClientSnapshot, Command, Direction,
        EntityId, MovementState, SUBUNITS_PER_CELL, SnapshotQuery, Terrain, WorldCell,
        WorldPosition,
    };

    use super::{AuthoritativeClient, advance_authority, rectangle_cells};
    use crate::interaction::TickScheduler;
    use crate::navigation::{SelectedCharacter, VisualMotion};
    use crate::procedural_assets::ProceduralAssetRegistry;
    use crate::render::{
        CharacterVisual, GroundItemVisual, NaturalResourceVisual, PresentationCache, TerrainRoot,
        sync_presentation,
    };

    const CROSSING_WALK_STEP_LIMIT: u64 = 1_024;
    const WALKER_DIRECTIONS: [Direction; 4] = [
        Direction::East,
        Direction::North,
        Direction::South,
        Direction::West,
    ];

    fn presentation_app(authoritative: AuthoritativeClient) -> App {
        let mut app = App::new();
        app.insert_resource(authoritative)
            .insert_resource(PresentationCache::default())
            .insert_resource(SelectedCharacter::default())
            .insert_resource(VisualMotion::default())
            .init_resource::<Assets<Image>>()
            .insert_resource(ProceduralAssetRegistry::default())
            .add_systems(Update, sync_presentation);
        app
    }

    fn mark_snapshot_dirty(app: &mut App) {
        app.world_mut()
            .resource_mut::<AuthoritativeClient>()
            .snapshot_dirty = true;
    }

    fn character(app: &App, id: EntityId) -> CharacterSnapshot {
        app.world()
            .resource::<AuthoritativeClient>()
            .snapshot()
            .characters
            .iter()
            .find(|character| character.id == id)
            .unwrap()
            .clone()
    }

    fn character_entity(app: &App, id: EntityId) -> Entity {
        app.world().resource::<PresentationCache>().characters[&id]
    }

    fn character_visual_count(app: &mut App, id: EntityId) -> usize {
        let world = app.world_mut();
        let mut visuals = world.query::<&CharacterVisual>();
        visuals.iter(world).filter(|visual| visual.id == id).count()
    }

    fn terrain_children(app: &App, root: Entity) -> Vec<Entity> {
        app.world()
            .entity(root)
            .get::<Children>()
            .unwrap()
            .iter()
            .copied()
            .collect()
    }

    #[test]
    fn paused_scheduler_stops_authoritative_ticks_and_resume_has_no_backlog() {
        let mut app = App::new();
        app.insert_resource(AuthoritativeClient::new().unwrap())
            .insert_resource(TickScheduler::default())
            .insert_resource(SelectedCharacter::default())
            .insert_resource(crate::modal::ModalState::default())
            .insert_resource(ButtonInput::<KeyCode>::default())
            .insert_resource(Time::<()>::default())
            .add_systems(Update, advance_authority);

        let initial_tick = app
            .world()
            .resource::<AuthoritativeClient>()
            .snapshot()
            .tick;
        app.world_mut()
            .resource_mut::<TickScheduler>()
            .set_paused(true);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs(5));
        app.update();
        assert_eq!(
            app.world()
                .resource::<AuthoritativeClient>()
                .snapshot()
                .tick,
            initial_tick
        );

        app.world_mut()
            .resource_mut::<TickScheduler>()
            .set_paused(false);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(249));
        app.update();
        assert_eq!(
            app.world()
                .resource::<AuthoritativeClient>()
                .snapshot()
                .tick,
            initial_tick
        );

        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_millis(1));
        app.update();
        assert_eq!(
            app.world()
                .resource::<AuthoritativeClient>()
                .snapshot()
                .tick
                .value(),
            initial_tick.value() + 1
        );
    }

    #[test]
    fn new_game_exposes_cora_in_lightweight_snapshot() {
        let client = AuthoritativeClient::new().unwrap();
        assert!(
            client
                .snapshot()
                .characters
                .iter()
                .any(|character| character.id == EntityId::new(3).unwrap())
        );
        assert!(client.snapshot().chunks.is_empty());
    }

    #[test]
    fn character_reconciliation_precedes_the_missing_cora_gate() {
        let mut authoritative = AuthoritativeClient::new().unwrap();
        authoritative
            .snapshot
            .characters
            .retain(|character| character.id != super::cora_id());

        let ada_id = EntityId::new(1).unwrap();
        let mut app = App::new();
        let ada_visual = app
            .world_mut()
            .spawn((CharacterVisual { id: ada_id }, Transform::default()))
            .id();
        let cora_visual = app
            .world_mut()
            .spawn((
                CharacterVisual {
                    id: super::cora_id(),
                },
                Transform::default(),
            ))
            .id();
        let mut cache = PresentationCache {
            central_chunk: Some(ChunkCoord::new(0, 0)),
            ..Default::default()
        };
        cache.characters.insert(ada_id, ada_visual);
        cache.characters.insert(super::cora_id(), cora_visual);
        app.insert_resource(authoritative)
            .insert_resource(cache)
            .insert_resource(SelectedCharacter(Some(super::cora_id())))
            .insert_resource(VisualMotion::default())
            .init_resource::<Assets<Image>>()
            .insert_resource(ProceduralAssetRegistry::default())
            .add_systems(Update, sync_presentation);

        app.update();

        let cache = app.world().resource::<PresentationCache>();
        assert_eq!(cache.characters.get(&ada_id), Some(&ada_visual));
        assert!(!cache.characters.contains_key(&super::cora_id()));
        assert_eq!(
            app.world()
                .entity(ada_visual)
                .get::<Transform>()
                .unwrap()
                .translation,
            Vec3::new(-24.0, 0.0, 10.0)
        );
        assert!(app.world().get_entity(cora_visual).is_err());
        assert_eq!(app.world().resource::<SelectedCharacter>().0, None);
    }

    #[test]
    fn dirty_sync_updates_chunk_terrain_without_replacing_the_root() {
        let mut app = presentation_app(AuthoritativeClient::new().unwrap());

        app.update();

        let (initial_root, initial_chunk_count, initial_window_count) = {
            let cache = app.world().resource::<PresentationCache>();
            (
                cache.terrain_root.unwrap(),
                cache.terrain_chunks.len(),
                cache.visible_window.as_ref().unwrap().coordinates().len(),
            )
        };
        let initial_children = terrain_children(&app, initial_root);
        assert_eq!(
            app.world().resource::<PresentationCache>().central_chunk,
            Some(ChunkCoord::new(0, 0))
        );
        assert!(!initial_children.is_empty());
        assert_eq!(initial_children.len(), initial_chunk_count);
        assert!(initial_chunk_count <= initial_window_count);

        let crossing_from = {
            let mut authoritative = app.world_mut().resource_mut::<AuthoritativeClient>();
            let crossing_from = walk_to_selected_positive_x_crossing(
                &mut authoritative.application,
                super::cora_id(),
            );
            authoritative.refresh_lightweight_snapshot(None).unwrap();
            crossing_from
        };
        app.update();

        assert_eq!(
            character(&app, super::cora_id()).containing_cell,
            crossing_from
        );
        assert_eq!(crossing_from.x(), 31);
        let crossing_center = crossing_from.split().0;
        assert_eq!(crossing_center.x(), 0);
        let (root_before, center_before, characters_before) = {
            let cache = app.world().resource::<PresentationCache>();
            (
                cache.terrain_root.unwrap(),
                cache.central_chunk,
                cache.characters.clone(),
            )
        };
        let children_before = terrain_children(&app, root_before);
        assert_eq!(center_before, Some(ChunkCoord::new(0, 0)));
        assert!(!children_before.is_empty());

        mark_snapshot_dirty(&mut app);
        app.update();

        let cache = app.world().resource::<PresentationCache>();
        assert_eq!(cache.terrain_root, Some(root_before));
        assert_eq!(cache.central_chunk, center_before);
        assert_eq!(cache.characters, characters_before);
        assert_eq!(terrain_children(&app, root_before), children_before);
        assert_eq!(
            app.world()
                .entity(character_entity(&app, super::cora_id()))
                .get::<Transform>()
                .unwrap()
                .translation,
            Vec3::new(31.0 * 12.0, crossing_from.y() as f32 * 12.0, 10.0,)
        );

        {
            let mut authoritative = app.world_mut().resource_mut::<AuthoritativeClient>();
            authoritative
                .application
                .execute(Command::SetMovementDirection {
                    character_id: super::cora_id(),
                    direction: Direction::East,
                })
                .unwrap();
            authoritative
                .application
                .execute(Command::AdvanceTicks { count: 4 })
                .unwrap();
            authoritative.refresh_lightweight_snapshot(None).unwrap();
        }
        app.update();

        let crossing_to = character(&app, super::cora_id()).containing_cell;
        assert_eq!(crossing_to, WorldCell::new(32, crossing_from.y()));
        let new_center = crossing_to.split().0;
        assert_eq!(new_center, ChunkCoord::new(1, crossing_center.y()));
        let (new_root, characters, new_chunk_count, new_window_count) = {
            let cache = app.world().resource::<PresentationCache>();
            assert_eq!(cache.central_chunk, Some(ChunkCoord::new(0, 0)));
            (
                cache.terrain_root.unwrap(),
                cache.characters.clone(),
                cache.terrain_chunks.len(),
                cache.visible_window.as_ref().unwrap().coordinates().len(),
            )
        };
        assert_eq!(new_root, root_before);
        assert!(app.world().get_entity(root_before).is_ok());
        let new_children = terrain_children(&app, new_root);
        assert!(!new_children.is_empty());
        assert_eq!(new_children.len(), new_chunk_count);
        assert!(new_chunk_count <= new_window_count);
        let terrain_root_count = {
            let world = app.world_mut();
            let mut terrain_roots = world.query::<&TerrainRoot>();
            terrain_roots.iter(world).count()
        };
        assert_eq!(terrain_root_count, 1);

        let render_origin = WorldPosition::from_cell_center(WorldCell::new(0, 0)).unwrap();
        for authoritative in &app
            .world()
            .resource::<AuthoritativeClient>()
            .snapshot()
            .characters
        {
            let expected = Vec3::new(
                (authoritative.position.x_subunits() - render_origin.x_subunits()) as f32
                    / SUBUNITS_PER_CELL as f32
                    * 12.0,
                (authoritative.position.y_subunits() - render_origin.y_subunits()) as f32
                    / SUBUNITS_PER_CELL as f32
                    * 12.0,
                10.0,
            );
            assert_eq!(
                app.world()
                    .entity(characters[&authoritative.id])
                    .get::<Transform>()
                    .unwrap()
                    .translation,
                expected
            );
        }
    }

    #[test]
    fn terrain_window_follows_camera_while_reusing_the_chunk_root() {
        let mut app = presentation_app(AuthoritativeClient::new().unwrap());
        let camera = app.world_mut().spawn((Camera2d, Transform::default())).id();

        app.update();
        let first_root = app
            .world()
            .resource::<PresentationCache>()
            .terrain_root
            .unwrap();
        let cora_before = character(&app, super::cora_id());
        let exploration_before = app
            .world()
            .resource::<AuthoritativeClient>()
            .snapshot()
            .exploration_revision;

        app.world_mut()
            .entity_mut(camera)
            .get_mut::<Transform>()
            .unwrap()
            .translation
            .x = 32.0 * 12.0;
        app.update();

        let cache = app.world().resource::<PresentationCache>();
        assert_eq!(cache.central_chunk, Some(ChunkCoord::new(1, 0)));
        assert_eq!(cache.terrain_root, Some(first_root));
        assert_eq!(character(&app, super::cora_id()), cora_before);
        assert_eq!(app.world().resource::<SelectedCharacter>().0, None);
        assert_eq!(
            app.world()
                .resource::<AuthoritativeClient>()
                .snapshot()
                .exploration_revision,
            exploration_before
        );
    }

    #[test]
    fn item_and_resource_revisions_do_not_rebuild_static_terrain() {
        let mut app = presentation_app(AuthoritativeClient::new().unwrap());
        app.world_mut().spawn((Camera2d, Transform::default()));
        app.update();

        let root_before = app
            .world()
            .resource::<PresentationCache>()
            .terrain_root
            .unwrap();
        let children_before = terrain_children(&app, root_before);

        {
            let mut authoritative = app.world_mut().resource_mut::<AuthoritativeClient>();
            authoritative.snapshot.item_revision =
                authoritative.snapshot.item_revision.wrapping_add(1);
            authoritative.snapshot.resource_revision =
                authoritative.snapshot.resource_revision.wrapping_add(1);
            authoritative.snapshot_dirty = true;
        }
        app.update();

        let cache = app.world().resource::<PresentationCache>();
        assert_eq!(cache.terrain_root, Some(root_before));
        assert_eq!(terrain_children(&app, root_before), children_before);
    }

    #[test]
    fn static_camera_keeps_explored_terrain_and_character_presentation_alive() {
        let mut app = presentation_app(AuthoritativeClient::new().unwrap());
        app.world_mut().spawn((Camera2d, Transform::default()));

        app.update();
        let root = app
            .world()
            .resource::<PresentationCache>()
            .terrain_root
            .unwrap();
        let terrain = terrain_children(&app, root);
        assert!(!terrain.is_empty());

        for _ in 0..8 {
            app.update();
        }

        let cache = app.world().resource::<PresentationCache>();
        assert_eq!(cache.terrain_root, Some(root));
        assert_eq!(terrain_children(&app, root), terrain);
        assert_eq!(cache.characters.len(), 5);
        assert_eq!(cache.ground_items.len(), 4);
        assert!(!cache.natural_resources.is_empty());
        for entity in cache.characters.values() {
            assert!(app.world().get_entity(*entity).is_ok());
        }
        for (id, entity) in &cache.ground_items {
            assert!(app.world().get_entity(*entity).is_ok());
            assert!(app.world().entity(*entity).contains::<GroundItemVisual>());
            assert_eq!(cache.ground_items.get(id), Some(entity));
        }
        for (cell, entity) in &cache.natural_resources {
            assert!(app.world().get_entity(*entity).is_ok());
            assert!(
                app.world()
                    .entity(*entity)
                    .contains::<NaturalResourceVisual>()
            );
            assert_eq!(cache.natural_resources.get(cell), Some(entity));
        }
    }

    fn walk_to_selected_positive_x_crossing(
        application: &mut Application,
        character_id: EntityId,
    ) -> WorldCell {
        let mut snapshot = application.snapshot(SnapshotQuery::default()).unwrap();
        let mut position = snapshot_character_position(&snapshot, character_id);
        let mut visit_counts = BTreeMap::from([(position, 1_u64)]);

        for steps in 0..CROSSING_WALK_STEP_LIMIT {
            let direction = select_least_visited_grass_direction(
                application,
                position,
                &visit_counts,
            )
            .unwrap_or_else(|| {
                panic!(
                    "least-visited walker has no adjacent grass cell before the x=0 -> x=1 crossing; character_id={} position=({}, {}) steps={steps} limit={CROSSING_WALK_STEP_LIMIT}",
                    character_id.value(),
                    position.x(),
                    position.y(),
                )
            });
            let target = direction.adjacent(position).unwrap_or_else(|| {
                panic!(
                    "least-visited walker selected an overflowing step; character_id={} position=({}, {}) direction={direction:?} steps={steps} limit={CROSSING_WALK_STEP_LIMIT}",
                    character_id.value(),
                    position.x(),
                    position.y(),
                )
            });

            if position.x() == 31 && target.x() == 32 && target.y() == position.y() {
                return position;
            }

            application
                .execute(Command::SetMovementDirection {
                    character_id,
                    direction,
                })
                .unwrap();
            application
                .execute(Command::AdvanceTicks { count: 4 })
                .unwrap();
            snapshot = application.snapshot(SnapshotQuery::default()).unwrap();
            let reached = snapshot_character_position(&snapshot, character_id);
            assert_eq!(
                reached,
                target,
                "authoritative walker step did not reach the selected grass cell; character_id={} from=({}, {}) direction={direction:?} steps={} limit={CROSSING_WALK_STEP_LIMIT}",
                character_id.value(),
                position.x(),
                position.y(),
                steps + 1,
            );
            position = reached;
            *visit_counts.entry(reached).or_insert(0) += 1;
        }

        panic!(
            "least-visited walker did not select a reachable x=31 -> x=32 grass crossing; character_id={} position=({}, {}) steps={CROSSING_WALK_STEP_LIMIT} limit={CROSSING_WALK_STEP_LIMIT}",
            character_id.value(),
            position.x(),
            position.y(),
        );
    }

    fn select_least_visited_grass_direction(
        application: &Application,
        position: WorldCell,
        visit_counts: &BTreeMap<WorldCell, u64>,
    ) -> Option<Direction> {
        let candidates = WALKER_DIRECTIONS
            .into_iter()
            .enumerate()
            .filter_map(|(order, direction)| {
                direction
                    .adjacent(position)
                    .map(|cell| (order, direction, cell))
            })
            .collect::<Vec<_>>();
        let mut chunks = candidates
            .iter()
            .map(|(_, _, cell)| cell.split().0)
            .collect::<Vec<_>>();
        chunks.sort_unstable();
        chunks.dedup();
        let terrain = application
            .snapshot(SnapshotQuery {
                chunks,
                ..SnapshotQuery::default()
            })
            .unwrap();

        candidates
            .into_iter()
            .filter(|(_, _, cell)| snapshot_terrain_at(&terrain, *cell) == Some(Terrain::Grass))
            .min_by_key(|(order, _, cell)| (visit_counts.get(cell).copied().unwrap_or(0), *order))
            .map(|(_, direction, _)| direction)
    }

    fn snapshot_character_position(snapshot: &ClientSnapshot, id: EntityId) -> WorldCell {
        snapshot
            .characters
            .iter()
            .find(|character| character.id == id)
            .unwrap()
            .containing_cell
    }

    fn snapshot_terrain_at(snapshot: &ClientSnapshot, position: WorldCell) -> Option<Terrain> {
        let (chunk, local) = position.split();
        snapshot
            .chunks
            .iter()
            .find(|candidate| candidate.coordinate == chunk)
            .and_then(|candidate| candidate.known_terrain_at(local))
    }

    #[test]
    fn character_reappearance_replaces_bevy_entity_once() {
        let mut app = presentation_app(AuthoritativeClient::new().unwrap());
        app.update();

        let ada_id = EntityId::new(1).unwrap();
        let ada = character(&app, ada_id);
        let old_visual = character_entity(&app, ada_id);
        {
            let mut authoritative = app.world_mut().resource_mut::<AuthoritativeClient>();
            authoritative
                .snapshot
                .characters
                .retain(|character| character.id != ada_id);
            authoritative.snapshot_dirty = true;
        }
        app.update();

        assert!(
            !app.world()
                .resource::<PresentationCache>()
                .characters
                .contains_key(&ada_id)
        );
        assert!(app.world().get_entity(old_visual).is_err());

        {
            let mut authoritative = app.world_mut().resource_mut::<AuthoritativeClient>();
            authoritative.snapshot.characters.push(ada);
            authoritative.snapshot_dirty = true;
        }
        app.update();

        let new_visual = character_entity(&app, ada_id);
        assert_ne!(new_visual, old_visual);
        assert_eq!(
            app.world()
                .entity(new_visual)
                .get::<CharacterVisual>()
                .unwrap()
                .id,
            ada_id
        );
        let new_transform = *app.world().entity(new_visual).get::<Transform>().unwrap();
        assert_eq!(character_visual_count(&mut app, ada_id), 1);

        mark_snapshot_dirty(&mut app);
        app.update();

        assert_eq!(character_entity(&app, ada_id), new_visual);
        assert_eq!(character_visual_count(&mut app, ada_id), 1);
        assert_eq!(
            *app.world().entity(new_visual).get::<Transform>().unwrap(),
            new_transform
        );
    }

    #[test]
    fn blocked_edge_intent_resyncs_from_authority_without_manual_presentation_changes() {
        let mut authoritative = AuthoritativeClient::new().unwrap();
        let (path, blocked_direction, blocked_key) =
            blocked_step_from_public_snapshots(&authoritative);
        for direction in path {
            authoritative
                .application
                .execute(Command::SetMovementDirection {
                    character_id: super::cora_id(),
                    direction,
                })
                .unwrap();
            authoritative
                .application
                .execute(Command::AdvanceTicks { count: 4 })
                .unwrap();
        }
        authoritative
            .application
            .execute(Command::StopMovement {
                character_id: super::cora_id(),
            })
            .unwrap();
        authoritative.refresh_lightweight_snapshot(None).unwrap();
        let blocked_from = authoritative
            .snapshot()
            .characters
            .iter()
            .find(|character| character.id == super::cora_id())
            .unwrap()
            .containing_cell;
        let blocked_target = blocked_direction.adjacent(blocked_from).unwrap();
        assert_ne!(terrain_at(&authoritative, blocked_target), Terrain::Grass);

        let mut app = App::new();
        app.insert_resource(authoritative)
            .insert_resource(TickScheduler::default())
            .insert_resource(PresentationCache::default())
            .insert_resource(SelectedCharacter::default())
            .insert_resource(VisualMotion::default())
            .insert_resource(ButtonInput::<KeyCode>::default())
            .insert_resource(Time::<()>::default())
            .init_resource::<Assets<Image>>()
            .insert_resource(ProceduralAssetRegistry::default())
            .add_systems(Update, (advance_authority, sync_presentation).chain());
        app.update();

        let authoritative_before = app
            .world()
            .resource::<AuthoritativeClient>()
            .snapshot()
            .clone();
        assert_eq!(
            authoritative_before
                .characters
                .iter()
                .find(|character| character.id == super::cora_id())
                .unwrap()
                .movement,
            MovementState::Idle
        );
        let (root_before, center_before, handles_before, transforms_before) = {
            let cache = app.world().resource::<PresentationCache>();
            let transforms = cache
                .characters
                .iter()
                .map(|(id, entity)| {
                    (
                        *id,
                        *app.world().entity(*entity).get::<Transform>().unwrap(),
                    )
                })
                .collect::<BTreeMap<_, _>>();
            (
                cache.terrain_root,
                cache.central_chunk,
                cache.characters.clone(),
                transforms,
            )
        };

        app.world_mut()
            .resource_mut::<AuthoritativeClient>()
            .snapshot
            .characters
            .iter_mut()
            .find(|character| character.id == super::cora_id())
            .unwrap()
            .position = WorldPosition::from_cell_center(WorldCell::new(
            blocked_from.x() + 7,
            blocked_from.y() + 11,
        ))
        .unwrap();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(blocked_key);

        app.update();

        let authoritative = app.world().resource::<AuthoritativeClient>();
        assert_eq!(
            authoritative
                .snapshot()
                .characters
                .iter()
                .find(|character| character.id == super::cora_id())
                .unwrap()
                .position,
            authoritative_before
                .characters
                .iter()
                .find(|character| character.id == super::cora_id())
                .unwrap()
                .position
        );
        assert_eq!(
            authoritative
                .snapshot()
                .characters
                .iter()
                .find(|character| character.id == super::cora_id())
                .unwrap()
                .movement,
            MovementState::ManualDirectional {
                direction: blocked_direction
            }
        );
        assert!(!authoritative.snapshot_dirty);
        let cache = app.world().resource::<PresentationCache>();
        assert_eq!(cache.terrain_root, root_before);
        assert_eq!(cache.central_chunk, center_before);
        assert_eq!(cache.characters, handles_before);
        for (id, entity) in &cache.characters {
            assert_eq!(
                app.world().entity(*entity).get::<Transform>().unwrap(),
                &transforms_before[id]
            );
        }
    }

    #[test]
    fn designation_rectangle_is_inclusive_ordered_and_bounded() {
        assert_eq!(
            rectangle_cells(WorldCell::new(2, 1), WorldCell::new(0, 0)).unwrap(),
            vec![
                WorldCell::new(0, 0),
                WorldCell::new(1, 0),
                WorldCell::new(2, 0),
                WorldCell::new(0, 1),
                WorldCell::new(1, 1),
                WorldCell::new(2, 1),
            ]
        );
        assert!(rectangle_cells(WorldCell::new(0, 0), WorldCell::new(63, 63)).is_some());
        assert!(rectangle_cells(WorldCell::new(0, 0), WorldCell::new(64, 63)).is_none());
    }

    fn blocked_step_from_public_snapshots(
        authoritative: &AuthoritativeClient,
    ) -> (Vec<Direction>, Direction, KeyCode) {
        let start = authoritative
            .snapshot()
            .characters
            .iter()
            .find(|character| character.id == super::cora_id())
            .unwrap()
            .containing_cell;
        let center = start.split().0;
        let chunks = (-1..=1)
            .flat_map(|y| (-1..=1).map(move |x| ChunkCoord::new(center.x() + x, center.y() + y)))
            .collect();
        let terrain = authoritative.terrain_snapshot(chunks).unwrap();
        let terrain_by_cell = terrain
            .chunks
            .iter()
            .flat_map(|chunk| {
                (0..CHUNK_SIDE).flat_map(move |local_y| {
                    (0..CHUNK_SIDE).map(move |local_x| {
                        let local = progressus_app::LocalCell::new(local_x, local_y);
                        (
                            chunk.coordinate.world_cell(local).unwrap(),
                            chunk.known_terrain_at(local),
                        )
                    })
                })
            })
            .collect::<BTreeMap<_, _>>();
        let directions = [
            (Direction::East, KeyCode::ArrowRight),
            (Direction::North, KeyCode::ArrowUp),
            (Direction::South, KeyCode::ArrowDown),
            (Direction::West, KeyCode::ArrowLeft),
        ];
        let mut frontier = VecDeque::from([start]);
        let mut predecessors = BTreeMap::<WorldCell, (WorldCell, Direction)>::new();
        predecessors.insert(start, (start, Direction::East));

        while let Some(position) = frontier.pop_front() {
            for (direction, key) in directions {
                let neighbor = direction.adjacent(position).unwrap();
                let Some(Some(terrain)) = terrain_by_cell.get(&neighbor) else {
                    continue;
                };
                if *terrain != Terrain::Grass {
                    let mut reversed = Vec::new();
                    let mut cursor = position;
                    while cursor != start {
                        let (previous, step) = predecessors[&cursor];
                        reversed.push(step);
                        cursor = previous;
                    }
                    reversed.reverse();
                    return (reversed, direction, key);
                }
                if let Entry::Vacant(entry) = predecessors.entry(neighbor) {
                    entry.insert((position, direction));
                    frontier.push_back(neighbor);
                }
            }
        }

        panic!("seed-0 public terrain snapshots contain no reachable blocked cardinal step")
    }

    fn terrain_at(authoritative: &AuthoritativeClient, position: WorldCell) -> Terrain {
        let (chunk, local) = position.split();
        authoritative.terrain_snapshot(vec![chunk]).unwrap().chunks[0]
            .known_terrain_at(local)
            .unwrap()
    }
}
