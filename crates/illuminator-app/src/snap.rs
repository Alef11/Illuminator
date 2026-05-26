//! Snapping — grid + smart-snap to nearby anchor points.
//!
//! Tools call [`snap_world`] with a candidate world position; it returns the
//! adjusted position plus an optional visual hint that the canvas draws as a
//! small marker during the drag.

use glam::DVec2;
use illuminator_core::doc::{NodeId, NodeKind};

use crate::app::IlluminatorApp;

#[derive(Clone, Debug)]
pub struct SnapSettings {
    pub grid_enabled: bool,
    pub grid_size: f64,
    pub smart_enabled: bool,
    /// Snap radius in *screen* pixels — interpreted per zoom level.
    pub snap_distance_px: f64,
}

impl Default for SnapSettings {
    fn default() -> Self {
        Self {
            grid_enabled: false,
            grid_size: 10.0,
            smart_enabled: true,
            snap_distance_px: 8.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SnapResult {
    pub position: DVec2,
    /// World-space point to render a hint marker at, when smart-snap engaged.
    pub hint: Option<DVec2>,
}

/// Snap `pos` to the strongest candidate. Smart-snap wins over grid. Pass
/// `exclude` to skip a node's own anchors (e.g. when moving a node, you don't
/// want to snap to its own corners).
pub fn snap_world(
    app: &IlluminatorApp,
    pos: DVec2,
    exclude: Option<NodeId>,
) -> SnapResult {
    if app.snap.smart_enabled {
        if let Some(target) = find_smart_target(app, pos, exclude) {
            return SnapResult { position: target, hint: Some(target) };
        }
    }
    if app.snap.grid_enabled {
        let g = app.snap.grid_size.max(1e-6);
        let sx = (pos.x / g).round() * g;
        let sy = (pos.y / g).round() * g;
        return SnapResult { position: DVec2::new(sx, sy), hint: None };
    }
    SnapResult { position: pos, hint: None }
}

fn find_smart_target(
    app: &IlluminatorApp,
    pos: DVec2,
    exclude: Option<NodeId>,
) -> Option<DVec2> {
    let tol_world = app.snap.snap_distance_px / app.view.zoom;
    let tol_sq = tol_world * tol_world;
    let mut best: Option<(DVec2, f64)> = None;
    for (id, node) in app.document.node_arena.iter() {
        if Some(id) == exclude {
            continue;
        }
        for p in node_snap_points(node) {
            let d2 = (p - pos).length_squared();
            if d2 <= tol_sq {
                if best.map_or(true, |(_, bd)| d2 < bd) {
                    best = Some((p, d2));
                }
            }
        }
    }
    best.map(|(p, _)| p)
}

fn node_snap_points(node: &illuminator_core::doc::Node) -> Vec<DVec2> {
    match &node.kind {
        NodeKind::Path(p) => {
            let mut pts: Vec<DVec2> = p.path.anchors.iter().map(|a| a.pos).collect();
            let n = p.path.anchors.len();
            if n >= 2 {
                for i in 0..n - 1 {
                    let m = (p.path.anchors[i].pos + p.path.anchors[i + 1].pos) * 0.5;
                    pts.push(m);
                }
                if p.path.closed {
                    let m = (p.path.anchors[n - 1].pos + p.path.anchors[0].pos) * 0.5;
                    pts.push(m);
                }
            }
            pts
        }
        NodeKind::Image(i) => {
            let (min, max) = i.bounds();
            let mid = (min + max) * 0.5;
            vec![
                DVec2::new(min.x, min.y),
                DVec2::new(max.x, min.y),
                DVec2::new(max.x, max.y),
                DVec2::new(min.x, max.y),
                mid,
            ]
        }
        NodeKind::Text(t) => {
            let (min, max) = t.approx_bounds();
            vec![DVec2::new(min.x, min.y), DVec2::new(max.x, max.y)]
        }
    }
}
