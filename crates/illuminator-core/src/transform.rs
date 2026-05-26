use glam::{DAffine2, DVec2};
use serde::{Deserialize, Serialize};

pub type Affine = DAffine2;

/// World-space view transform. The canvas is conceptually infinite; this
/// describes which slice of world space is on screen and at what scale.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ViewTransform {
    /// World-space point that sits at the viewport center.
    pub pan: DVec2,
    /// Screen pixels per world unit.
    pub zoom: f64,
}

impl Default for ViewTransform {
    fn default() -> Self {
        Self { pan: DVec2::ZERO, zoom: 1.0 }
    }
}

impl ViewTransform {
    pub const MIN_ZOOM: f64 = 0.01;
    pub const MAX_ZOOM: f64 = 64_000.0;

    #[inline]
    pub fn world_to_screen(&self, world: DVec2, viewport_center: DVec2) -> DVec2 {
        (world - self.pan) * self.zoom + viewport_center
    }

    #[inline]
    pub fn screen_to_world(&self, screen: DVec2, viewport_center: DVec2) -> DVec2 {
        (screen - viewport_center) / self.zoom + self.pan
    }

    /// Zoom around a fixed screen point — the world point under that pixel
    /// stays put. This is what wheel-zoom should do.
    pub fn zoom_at(&mut self, screen_pos: DVec2, viewport_center: DVec2, factor: f64) {
        let world_before = self.screen_to_world(screen_pos, viewport_center);
        self.zoom = (self.zoom * factor).clamp(Self::MIN_ZOOM, Self::MAX_ZOOM);
        let world_after = self.screen_to_world(screen_pos, viewport_center);
        self.pan += world_before - world_after;
    }

    /// Pan by a screen-space delta (positive x moves view right).
    #[inline]
    pub fn pan_by_screen(&mut self, screen_delta: DVec2) {
        self.pan -= screen_delta / self.zoom;
    }
}
