//! Direct Select tool — per-anchor pick and drag.
//!
//! Minimal v1: hit-test anchors of nodes already selected at the node level;
//! drag translates just that anchor. Handle editing arrives in a follow-up.

use egui::epaint::{PathShape, PathStroke};
use egui::{Color32, Painter, Pos2, Shape, Stroke as EguiStroke};
use glam::DVec2;

use illuminator_core::doc::{NodeId, NodeKind};
use illuminator_core::transform::ViewTransform;

use crate::app::IlluminatorApp;
use crate::interaction::HandlePart;

const ANCHOR_HIT_RADIUS_PX: f64 = 8.0;
const HANDLE_HIT_RADIUS_PX: f64 = 7.0;
const ANCHOR_COLOR: Color32 = Color32::from_rgb(0x4d, 0xa0, 0xff);
const HANDLE_COLOR: Color32 = Color32::from_rgba_premultiplied(0x4d, 0xa0, 0xff, 200);

/// If the screen-space point `p` is within `ANCHOR_HIT_RADIUS_PX` of an
/// anchor belonging to a selected node, return its `(NodeId, anchor_idx)`.
/// Hit-test the in/out handle endpoints of selected paths. Handles are easier
/// to grab than anchors when overlapping, so we test them first.
pub fn pick_handle_at(
    app: &IlluminatorApp,
    p: Pos2,
    center: DVec2,
) -> Option<(NodeId, usize, HandlePart)> {
    let cursor = DVec2::new(p.x as f64, p.y as f64);
    for &id in &app.selection {
        let Some(node) = app.document.node_arena.get(id) else { continue };
        if let NodeKind::Path(p_node) = &node.kind {
            for (idx, a) in p_node.path.anchors.iter().enumerate() {
                for (which, vec) in [
                    (HandlePart::Out, a.out_handle),
                    (HandlePart::In, a.in_handle),
                ] {
                    if vec.length_squared() < 1e-12 {
                        continue;
                    }
                    let target = app.view.world_to_screen(a.pos + vec, center);
                    if (target - cursor).length() <= HANDLE_HIT_RADIUS_PX {
                        return Some((id, idx, which));
                    }
                }
            }
        }
    }
    None
}

pub fn pick_anchor_at(
    app: &IlluminatorApp,
    p: Pos2,
    center: DVec2,
) -> Option<(NodeId, usize)> {
    let cursor = DVec2::new(p.x as f64, p.y as f64);
    // Iterate selected nodes; later we could also pick anchors of nearby
    // unselected nodes to allow direct-edit without node-level select first.
    for &id in &app.selection {
        let Some(node) = app.document.node_arena.get(id) else { continue };
        if let NodeKind::Path(p_node) = &node.kind {
            for (idx, a) in p_node.path.anchors.iter().enumerate() {
                let s = app.view.world_to_screen(a.pos, center);
                if (s - cursor).length() <= ANCHOR_HIT_RADIUS_PX {
                    return Some((id, idx));
                }
            }
        }
    }
    None
}

/// Translate a set of `(node, anchor)` pairs by `delta` directly on the
/// document (used for live drag preview; undo command pushed on release).
pub fn translate_anchors(
    app: &mut IlluminatorApp,
    items: &[(NodeId, usize)],
    delta: DVec2,
) {
    for (node_id, idx) in items {
        let Some(node) = app.document.node_arena.get_mut(*node_id) else { continue };
        if let NodeKind::Path(p) = &mut node.kind {
            if let Some(a) = p.path.anchors.get_mut(*idx) {
                a.pos += delta;
            }
        }
    }
}

/// Draw small anchor markers on every anchor of every selected node, plus
/// highlight the one currently under the cursor.
pub fn draw_overlay(
    app: &IlluminatorApp,
    painter: &Painter,
    cursor: Option<Pos2>,
    center: DVec2,
) {
    let view = &app.view;
    let hover = cursor.and_then(|p| pick_anchor_at(app, p, center));

    for &id in &app.selection {
        let Some(node) = app.document.node_arena.get(id) else { continue };
        if let NodeKind::Path(p_node) = &node.kind {
            // First draw the path outline a bit dimmer to make anchors pop.
            draw_path_outline(painter, &p_node.path.anchors, p_node.path.closed, view, center);
            // Handles (lines + dot) drawn under anchors so anchor squares sit on top.
            for a in p_node.path.anchors.iter() {
                draw_handle(painter, view, a.pos, a.out_handle, center);
                draw_handle(painter, view, a.pos, a.in_handle, center);
            }
            for (idx, a) in p_node.path.anchors.iter().enumerate() {
                let s = view.world_to_screen(a.pos, center);
                let p = Pos2::new(s.x as f32, s.y as f32);
                let hovered = hover == Some((id, idx));
                let size = if hovered { 9.0 } else { 6.0 };
                let r = egui::Rect::from_center_size(p, egui::vec2(size, size));
                let fill = if hovered {
                    Color32::from_rgb(0xff, 0xcc, 0x4d)
                } else if a.smooth {
                    Color32::WHITE
                } else {
                    // Slightly different fill for corner anchors so users can
                    // tell them apart from smooth at a glance.
                    Color32::from_rgb(0xee, 0xee, 0xee)
                };
                painter.rect_filled(r, 0.0, fill);
                painter.rect_stroke(r, 0.0, EguiStroke::new(1.0, ANCHOR_COLOR));
            }
        }
    }
}

fn draw_handle(
    painter: &Painter,
    view: &ViewTransform,
    anchor_pos: DVec2,
    handle: DVec2,
    center: DVec2,
) {
    if handle.length_squared() < 1e-12 {
        return;
    }
    let p0 = view.world_to_screen(anchor_pos, center);
    let p1 = view.world_to_screen(anchor_pos + handle, center);
    let s0 = Pos2::new(p0.x as f32, p0.y as f32);
    let s1 = Pos2::new(p1.x as f32, p1.y as f32);
    painter.line_segment([s0, s1], EguiStroke::new(1.0, HANDLE_COLOR));
    painter.add(egui::Shape::circle_filled(s1, 3.5, HANDLE_COLOR));
    painter.add(egui::Shape::circle_stroke(
        s1,
        3.5,
        EguiStroke::new(1.0, ANCHOR_COLOR),
    ));
}

fn draw_path_outline(
    painter: &Painter,
    anchors: &[illuminator_core::path::Anchor],
    closed: bool,
    view: &ViewTransform,
    center: DVec2,
) {
    if anchors.is_empty() {
        return;
    }
    let mut pts: Vec<Pos2> = Vec::new();
    pts.push(world_to_pos(view, anchors[0].pos, center));
    let n = anchors.len();
    let segments = if closed { n } else { n - 1 };
    for i in 0..segments {
        let a = anchors[i];
        let b = anchors[(i + 1) % n];
        let p0 = a.pos;
        let p1 = a.pos + a.out_handle;
        let p2 = b.pos + b.in_handle;
        let p3 = b.pos;
        let line_only =
            a.out_handle.length_squared() < 1e-12 && b.in_handle.length_squared() < 1e-12;
        if !line_only {
            flatten_cubic(p0, p1, p2, p3, view, center, &mut pts, 0);
        }
        pts.push(world_to_pos(view, p3, center));
    }
    painter.add(Shape::Path(PathShape {
        points: pts,
        closed,
        fill: Color32::TRANSPARENT,
        stroke: PathStroke::new(1.0, Color32::from_rgba_premultiplied(0x4d, 0xa0, 0xff, 140)),
    }));
}

fn flatten_cubic(
    p0: DVec2,
    p1: DVec2,
    p2: DVec2,
    p3: DVec2,
    view: &ViewTransform,
    center: DVec2,
    out: &mut Vec<Pos2>,
    depth: u32,
) {
    if depth > 10 {
        return;
    }
    let chord = p3 - p0;
    let chord_len = chord.length();
    let tol = 0.5 / view.zoom;
    let dev = if chord_len < 1e-12 {
        (p1 - p0).length().max((p2 - p0).length())
    } else {
        let n = DVec2::new(-chord.y, chord.x) / chord_len;
        ((p1 - p0).dot(n)).abs().max(((p2 - p0).dot(n)).abs())
    };
    if dev <= tol {
        return;
    }
    let q0 = 0.5 * (p0 + p1);
    let q1 = 0.5 * (p1 + p2);
    let q2 = 0.5 * (p2 + p3);
    let r0 = 0.5 * (q0 + q1);
    let r1 = 0.5 * (q1 + q2);
    let s = 0.5 * (r0 + r1);
    flatten_cubic(p0, q0, r0, s, view, center, out, depth + 1);
    out.push(world_to_pos(view, s, center));
    flatten_cubic(s, r1, q2, p3, view, center, out, depth + 1);
}

#[inline]
fn world_to_pos(view: &ViewTransform, w: DVec2, center: DVec2) -> Pos2 {
    let s = view.world_to_screen(w, center);
    Pos2::new(s.x as f32, s.y as f32)
}
