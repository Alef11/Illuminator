//! Pen tool — the headline interaction.
//!
//! Authoring model (matches Illustrator):
//! - Click → corner anchor at cursor.
//! - Click-drag → smooth anchor; drag vector sets the out-handle and the
//!   in-handle is mirrored (`smooth = true`).
//! - Alt held during drag → asymmetric (in-handle stays zero, smooth = false).
//! - Click the first anchor → close the path and commit.
//! - Esc → cancel (discard in-progress).
//! - Enter / Return → finish as an open path.
//! - Backspace → drop the last anchor.

use egui::epaint::{PathShape, PathStroke};
use egui::{Color32, Painter, Pos2, Shape, Stroke as EguiStroke};
use glam::DVec2;

use illuminator_core::command::Command;
use illuminator_core::doc::Node;
use illuminator_core::path::{Anchor, Path as IPath, PathNode};
use illuminator_core::style::{Paint, Stroke as IStroke, Style};
use illuminator_core::transform::ViewTransform;

use crate::app::IlluminatorApp;
use crate::commands::AddNodeCmd;
use crate::snap;

/// Screen-space radius (px) within which a click on the first anchor closes the path.
const CLOSE_HIT_RADIUS_PX: f64 = 9.0;

const PEN_COLOR: Color32 = Color32::from_rgb(0x4d, 0xa0, 0xff);
const PEN_COLOR_DIM: Color32 = Color32::from_rgba_premultiplied(0x4d, 0xa0, 0xff, 140);
const HANDLE_LINE: Color32 = Color32::from_rgba_premultiplied(0x4d, 0xa0, 0xff, 180);

#[derive(Default, Debug, Clone)]
pub struct PenState {
    pub anchors: Vec<Anchor>,
    /// True while the mouse is held down placing the current anchor; the
    /// last anchor's handles are being live-edited by the drag.
    pub dragging: bool,
}

impl PenState {
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool { self.anchors.is_empty() }
}

// --- Input handlers — called from canvas.rs when the active tool is Pen. ---

pub fn on_click(app: &mut IlluminatorApp, p: Pos2, center: DVec2) {
    if try_close(app, p, center) {
        return;
    }
    let world = world(app, p, center);
    let snapped = snap::snap_world(app, world, None);
    app.last_snap_hint = snapped.hint;
    let pen = app.pen_state.get_or_insert_with(PenState::default);
    pen.anchors.push(Anchor::corner(snapped.position));
    pen.dragging = false;
}

pub fn on_drag_start(app: &mut IlluminatorApp, p: Pos2, center: DVec2) {
    if try_close(app, p, center) {
        return;
    }
    let world = world(app, p, center);
    let snapped = snap::snap_world(app, world, None);
    app.last_snap_hint = snapped.hint;
    let pen = app.pen_state.get_or_insert_with(PenState::default);
    pen.anchors.push(Anchor::corner(snapped.position));
    pen.dragging = true;
}

pub fn on_drag_update(app: &mut IlluminatorApp, ctx: &egui::Context, p: Pos2, center: DVec2) {
    let world_pos = world(app, p, center);
    let alt = ctx.input(|i| i.modifiers.alt);
    let Some(pen) = &mut app.pen_state else { return };
    if !pen.dragging {
        return;
    }
    let Some(last) = pen.anchors.last_mut() else { return };
    let out = world_pos - last.pos;
    last.out_handle = out;
    if alt {
        // Asymmetric — leave in_handle at its previous value (zero for a fresh
        // anchor). Useful for cusps and corners with one curved side.
        last.smooth = false;
    } else {
        last.in_handle = -out;
        last.smooth = true;
    }
}

pub fn on_drag_end(app: &mut IlluminatorApp) {
    if let Some(pen) = &mut app.pen_state {
        pen.dragging = false;
    }
}

/// Finish the in-progress path, committing it to the document via an
/// `AddNodeCmd`. `closed` controls whether it's a closed shape or open path.
pub fn finish(app: &mut IlluminatorApp, closed: bool) {
    let Some(pen) = app.pen_state.take() else { return };
    if pen.anchors.len() < 2 {
        return;
    }
    let path = IPath {
        anchors: pen.anchors,
        closed,
    };
    let style = Style {
        fill: if closed { app.default_fill.map(Paint::Solid) } else { None },
        stroke: app.default_stroke.map(|c| IStroke {
            paint: Paint::Solid(c),
            width: app.default_stroke_width,
            ..Default::default()
        }),
        opacity: 1.0,
    };
    let name = if closed { "Shape" } else { "Path" };
    let node = Node::path(name, PathNode { path, style, ..Default::default() });
    let label = if closed { "Add Closed Path" } else { "Add Path" };
    let copies = crate::app::mirror_node(&node, app.symmetry);
    let cmd: Box<dyn Command> = Box::new(AddNodeCmd::with_mirrors(app.active_layer, node, copies, label));
    app.commands.push(cmd, &mut app.document);
    if let Some(layer) = app.document.layer_arena.get(app.active_layer) {
        if let Some(new_id) = layer.nodes.last().copied() {
            app.selection.clear();
            app.selection.insert(new_id);
        }
    }
    app.dirty = true;
}

pub fn cancel(app: &mut IlluminatorApp) {
    app.pen_state = None;
}

pub fn remove_last_anchor(app: &mut IlluminatorApp) {
    let Some(pen) = &mut app.pen_state else { return };
    pen.anchors.pop();
    pen.dragging = false;
    if pen.anchors.is_empty() {
        app.pen_state = None;
    }
}

fn try_close(app: &mut IlluminatorApp, p: Pos2, center: DVec2) -> bool {
    let Some(pen) = &app.pen_state else { return false };
    if pen.anchors.len() < 2 {
        return false;
    }
    let first_pos = pen.anchors[0].pos;
    let first_screen = app.view.world_to_screen(first_pos, center);
    let cursor = DVec2::new(p.x as f64, p.y as f64);
    if (first_screen - cursor).length() <= CLOSE_HIT_RADIUS_PX {
        finish(app, true);
        true
    } else {
        false
    }
}

#[inline]
fn world(app: &IlluminatorApp, p: Pos2, center: DVec2) -> DVec2 {
    app.view
        .screen_to_world(DVec2::new(p.x as f64, p.y as f64), center)
}

// --- Overlay rendering — called once per frame from canvas.rs. ---

pub fn draw_overlay(
    app: &IlluminatorApp,
    painter: &Painter,
    cursor_world: Option<DVec2>,
    center: DVec2,
) {
    let Some(pen) = &app.pen_state else { return };
    if pen.anchors.is_empty() {
        return;
    }
    let view = &app.view;

    // 1. Committed segments
    let mut points: Vec<Pos2> = Vec::new();
    flatten_anchors(&pen.anchors, view, center, &mut points);
    if points.len() >= 2 {
        painter.add(Shape::Path(PathShape {
            points,
            closed: false,
            fill: Color32::TRANSPARENT,
            stroke: PathStroke::new(1.5, PEN_COLOR),
        }));
    }

    // 2. Rubber-band preview from last anchor's out-handle to cursor
    if let Some(cursor_w) = cursor_world {
        if let Some(last) = pen.anchors.last() {
            // While dragging, the live segment is implicit in the committed
            // path (we're modifying the last anchor's handles in real time).
            // Show rubber-band only when not currently dragging.
            if !pen.dragging {
                let p0 = last.pos;
                let p1 = last.pos + last.out_handle;
                let p2 = cursor_w;
                let p3 = cursor_w;
                let mut pts = vec![world_to_pos(view, p0, center)];
                flatten_cubic(p0, p1, p2, p3, view, center, &mut pts, 0);
                pts.push(world_to_pos(view, p3, center));
                painter.add(Shape::Path(PathShape {
                    points: pts,
                    closed: false,
                    fill: Color32::TRANSPARENT,
                    stroke: PathStroke::new(1.0, PEN_COLOR_DIM),
                }));
            }
        }
    }

    // 3. Handle lines (only for the last anchor while dragging)
    if pen.dragging {
        if let Some(last) = pen.anchors.last() {
            draw_handle(painter, view, last, true, center);
            draw_handle(painter, view, last, false, center);
        }
    }

    // 4. Anchor markers
    let close_candidate =
        pen.anchors.len() >= 2 && cursor_is_near_first(app, cursor_world, center);
    for (i, a) in pen.anchors.iter().enumerate() {
        let is_first_and_closable = i == 0 && close_candidate;
        let is_last = i == pen.anchors.len() - 1;
        draw_anchor(painter, view, a, center, is_first_and_closable, is_last);
    }
}

fn cursor_is_near_first(
    app: &IlluminatorApp,
    cursor_world: Option<DVec2>,
    center: DVec2,
) -> bool {
    let Some(pen) = &app.pen_state else { return false };
    let Some(first) = pen.anchors.first() else { return false };
    let Some(cw) = cursor_world else { return false };
    let cur_screen = app.view.world_to_screen(cw, center);
    let first_screen = app.view.world_to_screen(first.pos, center);
    (first_screen - cur_screen).length() <= CLOSE_HIT_RADIUS_PX
}

fn draw_anchor(
    painter: &Painter,
    view: &ViewTransform,
    a: &Anchor,
    center: DVec2,
    highlight_close: bool,
    is_last: bool,
) {
    let p = world_to_pos(view, a.pos, center);
    let size = if highlight_close { 8.0 } else { 6.0 };
    let half = size * 0.5;
    let rect = egui::Rect::from_center_size(p, egui::vec2(size, size));
    let fill = if highlight_close {
        Color32::from_rgb(0xff, 0xcc, 0x4d)
    } else if is_last {
        PEN_COLOR
    } else {
        Color32::WHITE
    };
    painter.rect_filled(rect, 0.0, fill);
    painter.rect_stroke(rect, 0.0, EguiStroke::new(1.0, PEN_COLOR));
    let _ = half;
}

fn draw_handle(
    painter: &Painter,
    view: &ViewTransform,
    a: &Anchor,
    is_out: bool,
    center: DVec2,
) {
    let handle = if is_out { a.out_handle } else { a.in_handle };
    if handle.length_squared() < 1e-12 {
        return;
    }
    let p0 = world_to_pos(view, a.pos, center);
    let p1 = world_to_pos(view, a.pos + handle, center);
    painter.line_segment([p0, p1], EguiStroke::new(1.0, HANDLE_LINE));
    let r = egui::Rect::from_center_size(p1, egui::vec2(5.0, 5.0));
    painter.add(Shape::circle_filled(p1, 3.0, PEN_COLOR));
    let _ = r;
}

fn flatten_anchors(
    anchors: &[Anchor],
    view: &ViewTransform,
    center: DVec2,
    out: &mut Vec<Pos2>,
) {
    if anchors.is_empty() {
        return;
    }
    out.push(world_to_pos(view, anchors[0].pos, center));
    for window in anchors.windows(2) {
        let a = window[0];
        let b = window[1];
        let p0 = a.pos;
        let p1 = a.pos + a.out_handle;
        let p2 = b.pos + b.in_handle;
        let p3 = b.pos;
        let line_only =
            a.out_handle.length_squared() < 1e-12 && b.in_handle.length_squared() < 1e-12;
        if !line_only {
            flatten_cubic(p0, p1, p2, p3, view, center, out, 0);
        }
        out.push(world_to_pos(view, p3, center));
    }
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
