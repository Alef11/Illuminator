use glam::DVec2;
use serde::{Deserialize, Serialize};

use crate::style::Style;
use crate::transform::Affine;

/// A path anchor with cubic-bezier handles.
///
/// Handles are stored as offsets relative to `pos` (matching SVG/PostScript
/// convention). A zero handle means "corner" — no curve on that side.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Anchor {
    pub pos: DVec2,
    #[serde(default)]
    pub in_handle: DVec2,
    #[serde(default)]
    pub out_handle: DVec2,
    #[serde(default)]
    pub smooth: bool,
}

impl Anchor {
    pub fn corner(pos: DVec2) -> Self {
        Self { pos, in_handle: DVec2::ZERO, out_handle: DVec2::ZERO, smooth: false }
    }

    pub fn smooth_with(pos: DVec2, out_handle: DVec2) -> Self {
        Self { pos, in_handle: -out_handle, out_handle, smooth: true }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Path {
    pub anchors: Vec<Anchor>,
    #[serde(default)]
    pub closed: bool,
}

impl Path {
    pub fn rectangle(min: DVec2, max: DVec2) -> Self {
        Self {
            anchors: vec![
                Anchor::corner(DVec2::new(min.x, min.y)),
                Anchor::corner(DVec2::new(max.x, min.y)),
                Anchor::corner(DVec2::new(max.x, max.y)),
                Anchor::corner(DVec2::new(min.x, max.y)),
            ],
            closed: true,
        }
    }

    /// Cubic-bezier ellipse with the standard `kappa` control-point factor.
    /// Visually indistinguishable from a true ellipse.
    pub fn ellipse(center: DVec2, radius: DVec2) -> Self {
        const K: f64 = 0.552_284_749_830_793_4;
        let (rx, ry) = (radius.x, radius.y);
        let hx = rx * K;
        let hy = ry * K;
        // Order: right, bottom, left, top (y-down).
        let anchors = vec![
            Anchor {
                pos: DVec2::new(center.x + rx, center.y),
                in_handle: DVec2::new(0.0, -hy),
                out_handle: DVec2::new(0.0, hy),
                smooth: true,
            },
            Anchor {
                pos: DVec2::new(center.x, center.y + ry),
                in_handle: DVec2::new(hx, 0.0),
                out_handle: DVec2::new(-hx, 0.0),
                smooth: true,
            },
            Anchor {
                pos: DVec2::new(center.x - rx, center.y),
                in_handle: DVec2::new(0.0, hy),
                out_handle: DVec2::new(0.0, -hy),
                smooth: true,
            },
            Anchor {
                pos: DVec2::new(center.x, center.y - ry),
                in_handle: DVec2::new(-hx, 0.0),
                out_handle: DVec2::new(hx, 0.0),
                smooth: true,
            },
        ];
        Self { anchors, closed: true }
    }

    /// Conservative AABB including handle endpoints. Good enough for spatial
    /// culling and selection hit-tests; real bounds would solve cubic derivative.
    pub fn bounds(&self) -> Option<(DVec2, DVec2)> {
        let first = self.anchors.first()?;
        let mut min = first.pos;
        let mut max = first.pos;
        for a in &self.anchors {
            min = min.min(a.pos);
            max = max.max(a.pos);
            let i = a.pos + a.in_handle;
            let o = a.pos + a.out_handle;
            min = min.min(i).min(o);
            max = max.max(i).max(o);
        }
        Some((min, max))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PathNode {
    #[serde(default = "identity_affine")]
    pub transform: Affine,
    #[serde(default)]
    pub style: Style,
    pub path: Path,
}

impl Default for PathNode {
    fn default() -> Self {
        Self { transform: Affine::IDENTITY, style: Style::default(), path: Path::default() }
    }
}

fn identity_affine() -> Affine { Affine::IDENTITY }
