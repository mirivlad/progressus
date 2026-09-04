use std::collections::BTreeMap;

use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParam;
use bevy::image::ImageSampler;
use bevy::prelude::{Assets, Handle, Image, ResMut, Resource, Sprite, Vec2};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use progressus_app::{
    DoorState, EntityId, ItemKind, NaturalResourceKind, StructureKind, Terrain, WorkstationKind,
    WorldCell,
};

use crate::tile_connectivity::{CardinalConnections, TerrainConnections};

const ART_PIXELS: u32 = 16;
const VARIANT_COUNT: u8 = 8;
const QUANTITY_PIXEL_WORLD_SIZE: f32 = 0.5;

#[path = "../../../assets/procedural/mod.rs"]
mod asset_code;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum ProceduralAssetKind {
    Grass,
    Water,
    Rock,
    Human,
    WoodStack,
    StoneStack,
    PrimitiveTool,
    BerriesStack,
    Workbench,
    StoneWallBlueprint,
    StoneWall,
    DoorBlueprint,
    DoorClosed,
    DoorOpen,
    Tree,
    StoneOutcrop,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct ProceduralAssetKey {
    kind: ProceduralAssetKind,
    variant: u8,
    topology: u8,
}

impl ProceduralAssetKey {
    const fn new(kind: ProceduralAssetKind, variant: u8) -> Self {
        Self {
            kind,
            variant: variant % VARIANT_COUNT,
            topology: 0,
        }
    }

    const fn topology(kind: ProceduralAssetKind, connections: CardinalConnections) -> Self {
        Self {
            kind,
            variant: 0,
            topology: connections.bits() & 0x0f,
        }
    }

    const fn terrain(
        kind: ProceduralAssetKind,
        variant: u8,
        connections: TerrainConnections,
    ) -> Self {
        Self {
            kind,
            variant: variant % VARIANT_COUNT,
            topology: connections.bits(),
        }
    }
}

#[derive(Resource, Default)]
pub(crate) struct ProceduralAssetRegistry {
    images: BTreeMap<ProceduralAssetKey, Handle<Image>>,
    quantity_images: BTreeMap<u32, Handle<Image>>,
}

#[derive(SystemParam)]
pub(crate) struct ProceduralAssetParams<'w> {
    images: ResMut<'w, Assets<Image>>,
    registry: ResMut<'w, ProceduralAssetRegistry>,
}

impl ProceduralAssetParams<'_> {
    pub(crate) fn parts(&mut self) -> (&mut Assets<Image>, &mut ProceduralAssetRegistry) {
        (&mut self.images, &mut self.registry)
    }
}

impl ProceduralAssetRegistry {
    pub(crate) fn sprite(
        &mut self,
        images: &mut Assets<Image>,
        key: ProceduralAssetKey,
        size: Vec2,
    ) -> Sprite {
        let image = self.image(images, key);
        let mut sprite = Sprite::from_image(image);
        sprite.custom_size = Some(size);
        sprite
    }

    pub(crate) fn image_handle(
        &mut self,
        images: &mut Assets<Image>,
        key: ProceduralAssetKey,
    ) -> Handle<Image> {
        self.image(images, key)
    }

    pub(crate) fn quantity_sprite(&mut self, images: &mut Assets<Image>, quantity: u32) -> Sprite {
        let image = if let Some(handle) = self.quantity_images.get(&quantity) {
            handle.clone()
        } else {
            let handle = images.add(render_quantity_image(quantity));
            self.quantity_images.insert(quantity, handle.clone());
            handle
        };
        let (width, height) = asset_code::quantity_dimensions(quantity);
        let mut sprite = Sprite::from_image(image);
        sprite.custom_size = Some(Vec2::new(
            width as f32 * QUANTITY_PIXEL_WORLD_SIZE,
            height as f32 * QUANTITY_PIXEL_WORLD_SIZE,
        ));
        sprite
    }

    fn image(&mut self, images: &mut Assets<Image>, key: ProceduralAssetKey) -> Handle<Image> {
        if let Some(handle) = self.images.get(&key) {
            return handle.clone();
        }
        let handle = images.add(render_image(key));
        self.images.insert(key, handle.clone());
        handle
    }

    #[cfg(test)]
    fn cached_count(&self) -> usize {
        self.images.len() + self.quantity_images.len()
    }
}

pub(crate) fn terrain_asset(
    terrain: Terrain,
    cell: WorldCell,
    connections: TerrainConnections,
) -> ProceduralAssetKey {
    let kind = match terrain {
        Terrain::Grass => ProceduralAssetKind::Grass,
        Terrain::Water => ProceduralAssetKind::Water,
        Terrain::Rock => ProceduralAssetKind::Rock,
    };
    match terrain {
        Terrain::Grass => ProceduralAssetKey::new(kind, variant_for_cell(cell)),
        Terrain::Water | Terrain::Rock => {
            ProceduralAssetKey::terrain(kind, variant_for_cell(cell), connections)
        }
    }
}

pub(crate) fn character_asset(id: EntityId) -> ProceduralAssetKey {
    ProceduralAssetKey::new(ProceduralAssetKind::Human, variant_for_entity(id))
}

pub(crate) fn item_asset(kind: ItemKind, id: EntityId) -> ProceduralAssetKey {
    let kind = match kind {
        ItemKind::Wood => ProceduralAssetKind::WoodStack,
        ItemKind::Stone => ProceduralAssetKind::StoneStack,
        ItemKind::PrimitiveTool => ProceduralAssetKind::PrimitiveTool,
        ItemKind::Berries => ProceduralAssetKind::BerriesStack,
    };
    ProceduralAssetKey::new(kind, variant_for_entity(id))
}

pub(crate) fn workstation_asset(kind: WorkstationKind, id: EntityId) -> ProceduralAssetKey {
    let kind = match kind {
        WorkstationKind::Workbench => ProceduralAssetKind::Workbench,
    };
    ProceduralAssetKey::new(kind, variant_for_entity(id))
}

pub(crate) fn construction_site_asset(
    kind: StructureKind,
    connections: CardinalConnections,
) -> ProceduralAssetKey {
    let kind = match kind {
        StructureKind::StoneWall => ProceduralAssetKind::StoneWallBlueprint,
        StructureKind::Door => ProceduralAssetKind::DoorBlueprint,
    };
    ProceduralAssetKey::topology(kind, connections)
}

pub(crate) fn structure_asset(
    kind: StructureKind,
    connections: CardinalConnections,
    door_state: Option<DoorState>,
) -> ProceduralAssetKey {
    let kind = match kind {
        StructureKind::StoneWall => ProceduralAssetKind::StoneWall,
        StructureKind::Door => match door_state.unwrap_or(DoorState::Closed) {
            DoorState::Closed => ProceduralAssetKind::DoorClosed,
            DoorState::Open => ProceduralAssetKind::DoorOpen,
        },
    };
    ProceduralAssetKey::topology(kind, connections)
}

pub(crate) fn resource_asset(kind: NaturalResourceKind, cell: WorldCell) -> ProceduralAssetKey {
    let kind = match kind {
        NaturalResourceKind::Tree => ProceduralAssetKind::Tree,
        NaturalResourceKind::StoneOutcrop => ProceduralAssetKind::StoneOutcrop,
    };
    ProceduralAssetKey::new(kind, variant_for_cell(cell))
}

fn variant_for_entity(id: EntityId) -> u8 {
    mix64(id.value()) as u8 % VARIANT_COUNT
}

fn variant_for_cell(cell: WorldCell) -> u8 {
    let seed =
        (cell.x() as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ (cell.y() as u64).rotate_left(29);
    mix64(seed) as u8 % VARIANT_COUNT
}

fn mix64(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn render_quantity_image(quantity: u32) -> Image {
    let (width, height) = asset_code::quantity_dimensions(quantity);
    let mut canvas = Canvas::new(width, height);
    asset_code::quantity_label(&mut canvas, quantity);
    canvas.into_image()
}

fn render_image(key: ProceduralAssetKey) -> Image {
    let mut canvas = Canvas::new(ART_PIXELS, ART_PIXELS);
    match key.kind {
        ProceduralAssetKind::Grass => asset_code::grass(&mut canvas, key.variant),
        ProceduralAssetKind::Water => asset_code::water(&mut canvas, key.variant, key.topology),
        ProceduralAssetKind::Rock => asset_code::rock(&mut canvas, key.variant, key.topology),
        ProceduralAssetKind::Human => asset_code::human(&mut canvas, key.variant),
        ProceduralAssetKind::WoodStack => asset_code::wood_stack(&mut canvas, key.variant),
        ProceduralAssetKind::StoneStack => asset_code::stone_stack(&mut canvas, key.variant),
        ProceduralAssetKind::PrimitiveTool => asset_code::primitive_tool(&mut canvas, key.variant),
        ProceduralAssetKind::BerriesStack => asset_code::berries_stack(&mut canvas, key.variant),
        ProceduralAssetKind::Workbench => asset_code::workbench(&mut canvas, key.variant),
        ProceduralAssetKind::StoneWallBlueprint => {
            asset_code::stone_wall_blueprint(&mut canvas, key.topology)
        }
        ProceduralAssetKind::StoneWall => asset_code::stone_wall(&mut canvas, key.topology),
        ProceduralAssetKind::DoorBlueprint => asset_code::door_blueprint(&mut canvas, key.topology),
        ProceduralAssetKind::DoorClosed => asset_code::door(&mut canvas, key.topology, false),
        ProceduralAssetKind::DoorOpen => asset_code::door(&mut canvas, key.topology, true),
        ProceduralAssetKind::Tree => asset_code::tree(&mut canvas, key.variant),
        ProceduralAssetKind::StoneOutcrop => asset_code::stone_outcrop(&mut canvas, key.variant),
    }
    canvas.into_image()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Rgba8([u8; 4]);

impl Rgba8 {
    pub(super) const fn rgb(red: u8, green: u8, blue: u8) -> Self {
        Self([red, green, blue, 255])
    }

    pub(super) const fn rgba(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self([red, green, blue, alpha])
    }
}

pub(super) struct Canvas {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Canvas {
    fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width as usize * height as usize * 4],
        }
    }

    fn into_image(self) -> Image {
        let mut image = Image::new(
            Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            self.pixels,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        image.sampler = ImageSampler::nearest();
        image
    }

    pub(super) fn fill(&mut self, color: Rgba8) {
        for pixel in self.pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&color.0);
        }
    }

    pub(super) fn pixel(&mut self, x: i32, y: i32, color: Rgba8) {
        let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
            return;
        };
        if x >= self.width || y >= self.height {
            return;
        }
        let offset = ((y * self.width + x) * 4) as usize;
        self.pixels[offset..offset + 4].copy_from_slice(&color.0);
    }

    pub(super) fn rect(&mut self, x: i32, y: i32, width: i32, height: i32, color: Rgba8) {
        for py in y..y.saturating_add(height) {
            for px in x..x.saturating_add(width) {
                self.pixel(px, py, color);
            }
        }
    }

    pub(super) fn circle(&mut self, center_x: i32, center_y: i32, radius: i32, color: Rgba8) {
        self.ellipse(center_x, center_y, radius, radius, color);
    }

    pub(super) fn ellipse(
        &mut self,
        center_x: i32,
        center_y: i32,
        radius_x: i32,
        radius_y: i32,
        color: Rgba8,
    ) {
        if radius_x <= 0 || radius_y <= 0 {
            return;
        }
        let rx2 = i64::from(radius_x) * i64::from(radius_x);
        let ry2 = i64::from(radius_y) * i64::from(radius_y);
        let limit = rx2 * ry2;
        for y in -radius_y..=radius_y {
            for x in -radius_x..=radius_x {
                let value = i64::from(x * x) * ry2 + i64::from(y * y) * rx2;
                if value <= limit {
                    self.pixel(center_x + x, center_y + y, color);
                }
            }
        }
    }

    pub(super) fn line(&mut self, mut x0: i32, mut y0: i32, x1: i32, y1: i32, color: Rgba8) {
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut error = dx + dy;
        loop {
            self.pixel(x0, y0, color);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let doubled = error * 2;
            if doubled >= dy {
                error += dy;
                x0 += sx;
            }
            if doubled <= dx {
                error += dx;
                y0 += sy;
            }
        }
    }

    pub(super) fn scatter(&mut self, mut seed: u64, count: u32, area: [i32; 4], colors: &[Rgba8]) {
        let [x, y, width, height] = area;
        if width <= 0 || height <= 0 || colors.is_empty() {
            return;
        }
        for _ in 0..count {
            seed = mix64(seed.wrapping_add(0x9e37_79b9_7f4a_7c15));
            let px = x + (seed % width as u64) as i32;
            seed = mix64(seed.rotate_left(17));
            let py = y + (seed % height as u64) as i32;
            let color = colors[(seed as usize >> 8) % colors.len()];
            self.pixel(px, py, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::{Assets, Vec2};
    use std::collections::BTreeSet;

    use progressus_app::{DoorState, EntityId, StructureKind, Terrain, WorldCell};

    use crate::tile_connectivity::{CardinalConnections, TerrainConnections};

    use super::{ProceduralAssetRegistry, render_image, structure_asset, terrain_asset};

    fn image_hash(image: &bevy::prelude::Image) -> u64 {
        image
            .data
            .as_deref()
            .unwrap()
            .iter()
            .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
            })
    }

    #[test]
    fn procedural_rasterization_is_deterministic_and_variant_bounded() {
        let key = terrain_asset(
            Terrain::Grass,
            WorldCell::new(-17, 29),
            TerrainConnections::default(),
        );
        assert_eq!(
            image_hash(&render_image(key)),
            image_hash(&render_image(key))
        );
        for x in -100..100 {
            let key = terrain_asset(
                Terrain::Water,
                WorldCell::new(x, x * 3),
                TerrainConnections::default(),
            );
            assert!(key.variant < 8);
        }
    }

    #[test]
    fn registry_reuses_generated_images_for_same_recipe_variant() {
        let mut registry = ProceduralAssetRegistry::default();
        let mut images = Assets::default();
        let key = terrain_asset(
            Terrain::Rock,
            WorldCell::new(4, 7),
            TerrainConnections::default(),
        );
        let first = registry.sprite(&mut images, key, Vec2::splat(12.0));
        let second = registry.sprite(&mut images, key, Vec2::splat(12.0));
        assert_eq!(first.image.id(), second.image.id());
        assert_eq!(registry.cached_count(), 1);
    }

    #[test]
    fn entity_variants_are_stable() {
        let id = EntityId::new(42).unwrap();
        assert_eq!(super::character_asset(id), super::character_asset(id));
    }
    fn alpha_at(image: &bevy::prelude::Image, x: usize, y: usize) -> u8 {
        image.data.as_deref().unwrap()[(y * 16 + x) * 4 + 3]
    }

    #[test]
    fn wall_raster_reaches_only_the_requested_cardinal_edges() {
        let center = WorldCell::new(0, 0);
        let north_cells = [center, WorldCell::new(0, 1)]
            .into_iter()
            .collect::<BTreeSet<_>>();
        let north = render_image(structure_asset(
            StructureKind::StoneWall,
            CardinalConnections::from_cells(center, &north_cells),
            None,
        ));
        assert_ne!(alpha_at(&north, 8, 0), 0);
        assert_eq!(alpha_at(&north, 8, 15), 0);

        let west_cells = [center, WorldCell::new(-1, 0)]
            .into_iter()
            .collect::<BTreeSet<_>>();
        let west = render_image(structure_asset(
            StructureKind::StoneWall,
            CardinalConnections::from_cells(center, &west_cells),
            None,
        ));
        assert_ne!(alpha_at(&west, 0, 8), 0);
        assert_eq!(alpha_at(&west, 15, 8), 0);
    }

    #[test]
    fn door_open_and_closed_assets_are_distinct_and_share_wall_topology() {
        let center = WorldCell::new(0, 0);
        let cells = [center, WorldCell::new(-1, 0), WorldCell::new(1, 0)]
            .into_iter()
            .collect::<BTreeSet<_>>();
        let connections = CardinalConnections::from_cells(center, &cells);
        let closed = render_image(structure_asset(
            StructureKind::Door,
            connections,
            Some(DoorState::Closed),
        ));
        let open = render_image(structure_asset(
            StructureKind::Door,
            connections,
            Some(DoorState::Open),
        ));
        assert_ne!(image_hash(&closed), image_hash(&open));
    }
}
