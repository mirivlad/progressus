//! Cached workstation icons for the production-order modal.

use std::collections::BTreeMap;

use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParam;
use bevy::image::ImageSampler;
use bevy::prelude::{Assets, Handle, Image, ResMut, Resource};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use progressus_app::{EntityId, WorkstationId};

const ART_PIXELS: u32 = 16;
const VARIANT_COUNT: u8 = 8;

#[path = "../../../assets/procedural/workstations.rs"]
mod workstation_icons;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct ProceduralAssetKey {
    variant: u8,
}

#[derive(Resource, Default)]
pub(crate) struct ProceduralAssetRegistry {
    images: BTreeMap<ProceduralAssetKey, Handle<Image>>,
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
    pub(crate) fn image_handle(
        &mut self,
        images: &mut Assets<Image>,
        key: ProceduralAssetKey,
    ) -> Handle<Image> {
        if let Some(handle) = self.images.get(&key) {
            return handle.clone();
        }
        let handle = images.add(render_image(key));
        self.images.insert(key, handle.clone());
        handle
    }
}

pub(crate) fn workstation_asset(_kind: WorkstationId, id: EntityId) -> ProceduralAssetKey {
    ProceduralAssetKey {
        variant: mix64(id.value()) as u8 % VARIANT_COUNT,
    }
}

fn mix64(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn render_image(key: ProceduralAssetKey) -> Image {
    let mut canvas = Canvas::new(ART_PIXELS, ART_PIXELS);
    workstation_icons::workbench(&mut canvas, key.variant);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use progressus_app::workstation;

    #[test]
    fn workstation_icon_preserves_original_pixels() {
        // Captured from the original renderer before removing world sprites.
        for variant in 0..VARIANT_COUNT {
            let image = render_image(ProceduralAssetKey { variant });
            let hash = image
                .data
                .as_deref()
                .unwrap()
                .iter()
                .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
                    (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
                });
            assert_eq!(
                hash,
                if variant % 2 == 0 {
                    16_298_374_082_481_907_991
                } else {
                    8_966_466_947_505_188_951
                }
            );
        }
    }

    #[test]
    fn registry_reuses_icons_and_remains_bounded_across_workstations() {
        let mut registry = ProceduralAssetRegistry::default();
        let mut images = Assets::default();
        for id in 1..1000 {
            let key = workstation_asset(workstation::WORKBENCH, EntityId::new(id).unwrap());
            assert!(key.variant < VARIANT_COUNT);
            let first = registry.image_handle(&mut images, key);
            let second = registry.image_handle(&mut images, key);
            assert_eq!(first.id(), second.id());
        }
        assert!(registry.images.len() <= usize::from(VARIANT_COUNT));
        assert_eq!(registry.images.len(), images.len());
    }
}
