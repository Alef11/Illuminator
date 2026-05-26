//! Text node.
//!
//! v1 uses egui's built-in font rendering (no skia-safe yet). Bounds are
//! *approximate* — fine for hit-testing UX; exact metrics come back when we
//! swap in skia-safe (Phase 5+).

use glam::DVec2;
use serde::{Deserialize, Serialize};

use crate::style::{Color, Style};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FontFamily {
    Proportional,
    Monospace,
}

impl Default for FontFamily {
    fn default() -> Self { Self::Proportional }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextNode {
    /// World-space top-left of the text's bounding box.
    pub position: DVec2,
    pub text: String,
    /// World units (= screen pixels at 100% zoom).
    pub font_size: f64,
    #[serde(default)]
    pub family: FontFamily,
    pub color: Color,
    #[serde(default)]
    pub style: Style,
    #[serde(default)]
    pub width: Option<f64>,
}

impl Default for TextNode {
    fn default() -> Self {
        Self {
            position: DVec2::ZERO,
            text: String::new(),
            font_size: 24.0,
            family: FontFamily::Proportional,
            color: Color::BLACK,
            style: Style::default(),
            width: None,
        }
    }
}

impl TextNode {
    /// Rough AABB based on character count × an em-width estimate. Used for
    /// selection/marquee hit-testing; renderer uses real glyph metrics.
    pub fn approx_bounds(&self) -> (DVec2, DVec2) {
        if let Some(w) = self.width {
            let height = self.font_size * 1.2 * (self.text.split('\n').count() as f64).max(1.0);
            let min = self.position;
            let max = min + DVec2::new(w, height);
            return (min, max);
        }
        let chars = self.text.chars().count().max(1) as f64;
        // 0.55 ≈ average em-width-to-em-height ratio for Latin proportional text.
        let em_ratio = match self.family {
            FontFamily::Proportional => 0.55,
            FontFamily::Monospace => 0.62,
        };
        let width = chars * self.font_size * em_ratio;
        let height = self.font_size * 1.2;
        let min = self.position;
        let max = min + DVec2::new(width, height);
        (min, max)
    }
}
