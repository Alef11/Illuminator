use egui::epaint::PathStroke;
use egui::{Color32, Painter, PointerButton, Pos2, Rect, Sense, Shape, Stroke as EguiStroke};
use glam::DVec2;

use illuminator_core::doc::{Node, NodeId};
use illuminator_core::image::{ImageData, ImageNode};
use illuminator_core::path::{Path as IPath, PathNode, Anchor};
use illuminator_core::style::{Paint, Stroke as IStroke, Style};
use illuminator_render::{paint_canvas_backdrop, render_document, RenderCtx};

use crate::app::IlluminatorApp;
use crate::commands::{
    AddNodeCmd, EditHandleCmd, MoveAnchorsCmd, MoveNodesCmd, ResizeImageCmd, ResizePathCmd,
};
use crate::direct_select;
use crate::interaction::{Corner, HandlePart, Interaction, ShapeKind};
use crate::pen;
use crate::snap;
use crate::text;
use crate::tools::ToolKind;

const SELECTION_COLOR: Color32 = Color32::from_rgb(0x4d, 0xa0, 0xff);
const MARQUEE_FILL: Color32 = Color32::from_rgba_premultiplied(0x4d, 0xa0, 0xff, 32);

pub fn show(app: &mut IlluminatorApp, ctx: &egui::Context) {
    // If the user switched tools while a pen path is in progress, finish it
    // as an open path (Illustrator behaviour).
    if app.pen_state.is_some() && app.active_tool != ToolKind::Pen {
        pen::finish(app, false);
    }
    // Same for text editing: leaving the Text tool commits the current edit.
    if app.editing_text.is_some() && app.active_tool != ToolKind::Text {
        text::finish(app);
    }
    // Reset snap hint each frame; tools repopulate it during input.
    app.last_snap_hint = None;

    // Process text-edit keystrokes before the canvas widget swallows the click
    // (keystrokes don't belong to any widget — fine to consume here).
    if app.active_tool == ToolKind::Text {
        text::process_events(app, ctx);
    }

    egui::CentralPanel::default()
        .frame(egui::Frame::none().fill(Color32::from_gray(22)))
        .show(ctx, |ui| {
            let size = ui.available_size_before_wrap();
            let (response, painter) =
                ui.allocate_painter(size, Sense::click_and_drag());
            let rect = response.rect;
            let center = DVec2::new(rect.center().x as f64, rect.center().y as f64);

            paint_canvas_backdrop(&painter, rect);
            draw_origin_marker(&painter, &app.view, rect, center);

            handle_input(app, ctx, &response, rect, center);

            // Remember viewport state for menu-driven image placement.
            app.last_viewport_center = center;
            let cursor_screen = response.hover_pos();
            let cursor_world = cursor_screen.map(|p| {
                app.view
                    .screen_to_world(DVec2::new(p.x as f64, p.y as f64), center)
            });
            app.last_cursor_world = cursor_world;

            // Drop-import images. egui delivers dropped files via raw input.
            let dropped = ctx.input(|i| i.raw.dropped_files.clone());
            if !dropped.is_empty() {
                let drop_world = cursor_world.unwrap_or(app.view.pan);
                handle_dropped_files(app, dropped, drop_world);
            }

            let mut render_ctx = RenderCtx {
                painter: &painter,
                view: &app.view,
                viewport_center: center,
                egui_ctx: ctx,
                textures: &mut app.textures,
            };
            render_document(&app.document, &mut render_ctx);

            draw_overlays(app, &painter, rect, center);
            if app.active_tool == ToolKind::DirectSelect {
                direct_select::draw_overlay(app, &painter, cursor_screen, center);
            }
            if app.active_tool == ToolKind::Pen {
                pen::draw_overlay(app, &painter, cursor_world, center);
            }
            if app.active_tool == ToolKind::Text {
                text::draw_overlay(app, &painter, center);
            }

            update_cursor(ctx, app);
        });
}

fn handle_input(
    app: &mut IlluminatorApp,
    ctx: &egui::Context,
    response: &egui::Response,
    rect: Rect,
    center: DVec2,
) {
    // --- Wheel zoom (anchored at cursor) ---
    if response.hovered() {
        let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
        if scroll.abs() > 0.0 {
            if let Some(pos) = response.hover_pos() {
                let screen = DVec2::new(pos.x as f64, pos.y as f64);
                let factor = 1.1_f64.powf(scroll as f64 * 0.05);
                app.view.zoom_at(screen, center, factor);
            }
        }
        // Ctrl+wheel: alternate (also zoom). Already covered above.
    }

    // --- Pan dispatch: middle-button OR space-held OR Hand tool ---
    let space_held = ctx.input(|i| i.key_down(egui::Key::Space));
    let middle_dragging = response.dragged_by(PointerButton::Middle);
    let primary_dragging = response.dragged();
    let pan_active = middle_dragging
        || (space_held && primary_dragging)
        || (app.active_tool == ToolKind::Hand && primary_dragging);

    if pan_active {
        let delta = ctx.input(|i| i.pointer.delta());
        if delta != egui::Vec2::ZERO {
            app.view
                .pan_by_screen(DVec2::new(delta.x as f64, delta.y as f64));
        }
        // While panning, abandon any other interaction.
        if !matches!(app.interaction, Interaction::None | Interaction::Panning { .. }) {
            app.interaction = Interaction::None;
        }
        return;
    }

    // --- Tool-specific drag/click dispatch (primary button) ---
    if response.drag_started() {
        if let Some(p) = response.interact_pointer_pos() {
            on_drag_start(app, ctx, p, center);
        }
    } else if response.dragged() {
        if let Some(p) = response.interact_pointer_pos() {
            on_drag_update(app, ctx, p, center);
        }
    } else if response.drag_stopped() {
        if app.active_tool == ToolKind::Pen {
            pen::on_drag_end(app);
        } else {
            on_drag_end(app, center);
        }
    } else if response.clicked() {
        if let Some(p) = response.interact_pointer_pos() {
            on_click(app, ctx, p, center);
        }
    }

    // Mark the canvas wanting focus so keyboard shortcuts (V/M/L/H) work without
    // first needing a click in some other widget to release focus.
    let _ = rect; // suppress unused
}

fn on_drag_start(app: &mut IlluminatorApp, ctx: &egui::Context, p: Pos2, center: DVec2) {
    let world = app.view.screen_to_world(DVec2::new(p.x as f64, p.y as f64), center);

    match app.active_tool {
        ToolKind::Hand => {
            // Handled by pan_active branch above; nothing to do here.
        }
        ToolKind::Pen => {
            pen::on_drag_start(app, p, center);
        }
        ToolKind::DirectSelect => {
            // 1. Handle endpoints — preferred when overlapping the anchor.
            if let Some((node_id, anchor_idx, which)) =
                direct_select::pick_handle_at(app, p, center)
            {
                let alt = ctx.input(|i| i.modifiers.alt);
                let (start_in, start_out, was_smooth) = app
                    .document
                    .node_arena
                    .get(node_id)
                    .and_then(|n| match &n.kind {
                        illuminator_core::doc::NodeKind::Path(p) => {
                            p.path.anchors.get(anchor_idx).map(|a| (a.in_handle, a.out_handle, a.smooth))
                        }
                        _ => None,
                    })
                    .unwrap_or((DVec2::ZERO, DVec2::ZERO, false));
                app.interaction = Interaction::MovingHandle {
                    node_id,
                    anchor_idx,
                    which,
                    start_in,
                    start_out,
                    was_smooth,
                    break_symmetry: alt,
                };
            // 2. Anchor pick.
            } else if let Some((node_id, anchor_idx)) = direct_select::pick_anchor_at(app, p, center) {
                app.interaction = Interaction::MovingAnchors {
                    items: vec![(node_id, anchor_idx)],
                    last_world: world,
                    accumulated: DVec2::ZERO,
                };
            } else {
                // 3. Fall through to node-level select.
                let shift = ctx.input(|i| i.modifiers.shift);
                let hit = pick_node_at(app, world);
                if let Some(id) = hit {
                    if !shift {
                        app.selection.clear();
                    }
                    app.selection.insert(id);
                } else if !shift {
                    app.selection.clear();
                }
            }
        }
        ToolKind::Rectangle | ToolKind::Ellipse => {
            let kind = if app.active_tool == ToolKind::Rectangle {
                ShapeKind::Rect
            } else {
                ShapeKind::Ellipse
            };
            let snapped = snap::snap_world(app, world, None);
            app.last_snap_hint = snapped.hint;
            app.interaction = Interaction::DrawingShape {
                kind,
                start_world: snapped.position,
                current_world: snapped.position,
            };
        }
        ToolKind::Artboard => {
            let snapped = snap::snap_world(app, world, None);
            app.last_snap_hint = snapped.hint;
            app.interaction = Interaction::DrawingArtboard {
                start_world: snapped.position,
                current_world: snapped.position,
            };
        }
        ToolKind::Select => {
            // 1. Corner-resize if the cursor is on a corner of a single selected shape/image.
            if let Some((node_id, corner)) = pick_selection_corner(app, p, center) {
                if let Some(node) = app.document.node_arena.get(node_id) {
                    let bounds = node.bounds();
                    if let Some((mn, mx)) = bounds {
                        let anchor_world = corner.opposite().world_pos(mn, mx);
                        let size = mx - mn;
                        match &node.kind {
                            illuminator_core::doc::NodeKind::Image(img) => {
                                let aspect = img.image.width as f64
                                    / img.image.height.max(1) as f64;
                                app.interaction = Interaction::ResizingImage {
                                    node_id,
                                    corner,
                                    anchor_world,
                                    original_position: img.position,
                                    original_size: img.size,
                                    aspect,
                                    original_anchors: None,
                                };
                                return;
                            }
                            illuminator_core::doc::NodeKind::Path(pn) => {
                                let aspect = size.x / size.y.max(1e-6);
                                app.interaction = Interaction::ResizingImage {
                                    node_id,
                                    corner,
                                    anchor_world,
                                    original_position: mn,
                                    original_size: size,
                                    aspect,
                                    original_anchors: Some(pn.path.anchors.clone()),
                                };
                                return;
                            }
                            _ => {}
                        }
                    }
                }
            }
            let shift = ctx.input(|i| i.modifiers.shift);
            let hit = pick_node_at(app, world);
            if let Some(hit_id) = hit {
                if !app.selection.contains(&hit_id) {
                    if !shift {
                        app.selection.clear();
                    }
                    app.selection.insert(hit_id);
                }
                app.interaction = Interaction::MovingSelection {
                    last_world: world,
                    accumulated: DVec2::ZERO,
                };
            } else {
                if !shift {
                    app.selection.clear();
                }
                app.interaction = Interaction::Marquee {
                    start_screen: p,
                    current_screen: p,
                };
            }
        }
        ToolKind::Text => {
            // Text doesn't drag — fall back to click semantics.
            text::on_click(app, p, center);
        }
    }
}

fn on_drag_update(app: &mut IlluminatorApp, ctx: &egui::Context, p: Pos2, center: DVec2) {
    if app.active_tool == ToolKind::Pen {
        pen::on_drag_update(app, ctx, p, center);
        return;
    }
    let world = app.view.screen_to_world(DVec2::new(p.x as f64, p.y as f64), center);

    // First derive what to do from the current interaction state (immutable
    // read), then apply it (snap helper needs &IlluminatorApp; updating the
    // doc and interaction needs &mut). This two-step avoids overlapping borrows.
    enum Action {
        Nothing,
        DrawShape,
        DrawArtboard,
        Marquee,
        MoveSelection { last: DVec2 },
        MoveAnchors { items: Vec<(NodeId, usize)>, last: DVec2 },
        MoveHandle {
            node_id: NodeId,
            anchor_idx: usize,
            which: HandlePart,
            break_symmetry: bool,
            was_smooth: bool,
        },
        ResizeImage {
            node_id: NodeId,
            corner: Corner,
            anchor_world: DVec2,
            aspect: f64,
            original_position: DVec2,
            original_size: DVec2,
            original_anchors: Option<Vec<Anchor>>,
        },
    }
    let action = match &app.interaction {
        Interaction::DrawingShape { .. } => Action::DrawShape,
        Interaction::DrawingArtboard { .. } => Action::DrawArtboard,
        Interaction::Marquee { .. } => Action::Marquee,
        Interaction::MovingSelection { last_world, .. } => Action::MoveSelection { last: *last_world },
        Interaction::MovingAnchors { items, last_world, .. } => {
            Action::MoveAnchors { items: items.clone(), last: *last_world }
        }
        Interaction::MovingHandle {
            node_id,
            anchor_idx,
            which,
            break_symmetry,
            was_smooth,
            ..
        } => Action::MoveHandle {
            node_id: *node_id,
            anchor_idx: *anchor_idx,
            which: *which,
            break_symmetry: *break_symmetry,
            was_smooth: *was_smooth,
        },
        Interaction::ResizingImage {
            node_id,
            corner,
            anchor_world,
            original_position,
            original_size,
            aspect,
            original_anchors,
        } => Action::ResizeImage {
            node_id: *node_id,
            corner: *corner,
            anchor_world: *anchor_world,
            aspect: *aspect,
            original_position: *original_position,
            original_size: *original_size,
            original_anchors: original_anchors.clone(),
        },
        _ => Action::Nothing,
    };

    match action {
        Action::DrawShape => {
            let snapped = snap::snap_world(app, world, None);
            app.last_snap_hint = snapped.hint;
            if let Interaction::DrawingShape { current_world, .. } = &mut app.interaction {
                *current_world = snapped.position;
            }
        }
        Action::DrawArtboard => {
            let snapped = snap::snap_world(app, world, None);
            app.last_snap_hint = snapped.hint;
            if let Interaction::DrawingArtboard { current_world, .. } = &mut app.interaction {
                *current_world = snapped.position;
            }
        }
        Action::Marquee => {
            if let Interaction::Marquee { current_screen, .. } = &mut app.interaction {
                *current_screen = p;
            }
        }
        Action::MoveSelection { last } => {
            let exclude = app.selection.iter().next().copied();
            let snapped = snap::snap_world(app, world, exclude);
            app.last_snap_hint = snapped.hint;
            let delta = snapped.position - last;
            if let Interaction::MovingSelection { last_world, accumulated } = &mut app.interaction {
                *last_world = snapped.position;
                *accumulated += delta;
            }
            let ids: Vec<NodeId> = app.selection.iter().copied().collect();
            translate_nodes(app, &ids, delta);
        }
        Action::MoveAnchors { items, last } => {
            let exclude = items.first().map(|(id, _)| *id);
            let snapped = snap::snap_world(app, world, exclude);
            app.last_snap_hint = snapped.hint;
            let delta = snapped.position - last;
            if let Interaction::MovingAnchors { last_world, accumulated, .. } = &mut app.interaction {
                *last_world = snapped.position;
                *accumulated += delta;
            }
            direct_select::translate_anchors(app, &items, delta);
        }
        Action::MoveHandle {
            node_id,
            anchor_idx,
            which,
            break_symmetry,
            was_smooth,
        } => {
            let Some(node) = app.document.node_arena.get_mut(node_id) else { return };
            let illuminator_core::doc::NodeKind::Path(p) = &mut node.kind else { return };
            let Some(a) = p.path.anchors.get_mut(anchor_idx) else { return };
            let new_vec = world - a.pos;
            match which {
                HandlePart::Out => {
                    a.out_handle = new_vec;
                    if was_smooth && !break_symmetry {
                        a.in_handle = -new_vec;
                    } else if break_symmetry {
                        a.smooth = false;
                    }
                }
                HandlePart::In => {
                    a.in_handle = new_vec;
                    if was_smooth && !break_symmetry {
                        a.out_handle = -new_vec;
                    } else if break_symmetry {
                        a.smooth = false;
                    }
                }
            }
            app.dirty = true;
        }
        Action::ResizeImage {
            node_id,
            corner,
            anchor_world,
            aspect,
            original_position,
            original_size,
            original_anchors,
        } => {
            let shift = ctx.input(|i| i.modifiers.shift);
            // Live: corner moves to cursor; opposite corner = anchor_world fixed.
            let mut new_corner = world;
            if shift && aspect > 0.0 {
                // Lock aspect — snap the cursor onto the line through anchor_world
                // at the original aspect, in the direction of the drag.
                let dx = (new_corner.x - anchor_world.x).abs();
                let dy = (new_corner.y - anchor_world.y).abs();
                let target_dy = dx / aspect;
                let target_dx = dy * aspect;
                let use_x = target_dy > dy;
                let new_dx = if use_x { dx } else { target_dx };
                let new_dy = if use_x { target_dy } else { dy };
                new_corner = DVec2::new(
                    anchor_world.x + (new_corner.x - anchor_world.x).signum() * new_dx,
                    anchor_world.y + (new_corner.y - anchor_world.y).signum() * new_dy,
                );
            }
            let mn = new_corner.min(anchor_world);
            let mx = new_corner.max(anchor_world);
            let size = (mx - mn).max(DVec2::new(1.0, 1.0));
            let position = mn;
            if let Some(node) = app.document.node_arena.get_mut(node_id) {
                match &mut node.kind {
                    illuminator_core::doc::NodeKind::Image(img) => {
                        img.position = position;
                        img.size = size;
                        app.dirty = true;
                    }
                    illuminator_core::doc::NodeKind::Path(pn) => {
                        if let Some(orig_anchors) = &original_anchors {
                            let orig_min = original_position;
                            let orig_max = original_position + original_size;
                            let original_corner_pos = corner.world_pos(orig_min, orig_max);
                            let v_orig = original_corner_pos - anchor_world;
                            let v_new = new_corner - anchor_world;
                            let scale_x = if v_orig.x.abs() > 1e-6 { v_new.x / v_orig.x } else { 1.0 };
                            let scale_y = if v_orig.y.abs() > 1e-6 { v_new.y / v_orig.y } else { 1.0 };

                            let mut new_anchors = orig_anchors.clone();
                            for a in &mut new_anchors {
                                a.pos.x = anchor_world.x + (a.pos.x - anchor_world.x) * scale_x;
                                a.pos.y = anchor_world.y + (a.pos.y - anchor_world.y) * scale_y;
                                a.in_handle.x *= scale_x;
                                a.in_handle.y *= scale_y;
                                a.out_handle.x *= scale_x;
                                a.out_handle.y *= scale_y;
                            }
                            pn.path.anchors = new_anchors;
                            app.dirty = true;
                        }
                    }
                    _ => {}
                }
            }
            let _ = corner;
        }
        Action::Nothing => {}
    }
}

fn on_drag_end(app: &mut IlluminatorApp, center: DVec2) {
    // Take the interaction out so we can borrow app freely.
    let interaction = std::mem::replace(&mut app.interaction, Interaction::None);
    match interaction {
        Interaction::DrawingArtboard { start_world, current_world } => {
            let min = start_world.min(current_world);
            let max = start_world.max(current_world);
            let size = max - min;
            if size.x.abs() < 10.0 || size.y.abs() < 10.0 {
                return; // ignore zero-size or tiny artboards
            }
            let n = app.document.artboards.len() + 1;
            app.document.artboards.push(illuminator_core::doc::Artboard {
                name: format!("Artboard {n}"),
                min,
                max,
            });
            app.dirty = true;
        }
        Interaction::DrawingShape { kind, start_world, current_world } => {
            let min = start_world.min(current_world);
            let max = start_world.max(current_world);
            let size = max - min;
            if size.x.abs() < 0.5 || size.y.abs() < 0.5 {
                return; // ignore zero-size shapes
            }
            let (name, path) = match kind {
                ShapeKind::Rect => ("Rectangle", IPath::rectangle(min, max)),
                ShapeKind::Ellipse => (
                    "Ellipse",
                    IPath::ellipse((min + max) * 0.5, (max - min) * 0.5),
                ),
            };
            let style = Style {
                fill: app.default_fill.map(Paint::Solid),
                stroke: app.default_stroke.map(|c| IStroke {
                    paint: Paint::Solid(c),
                    width: app.default_stroke_width,
                    ..Default::default()
                }),
                opacity: 1.0,
            };
            let node = Node::path(name, PathNode { path, style, ..Default::default() });
            let copies = crate::app::mirror_node(&node, app.symmetry);
            let cmd = AddNodeCmd::with_mirrors(
                app.active_layer,
                node,
                copies,
                format!("Add {name}"),
            );
            let boxed: Box<dyn illuminator_core::command::Command> = Box::new(cmd);
            // Push first; then re-find the freshly inserted node so we can
            // select it (the inserted_id from AddNodeCmd isn't reachable through
            // the trait-object boundary).
            app.commands.push(boxed, &mut app.document);
            if let Some(layer) = app.document.layer_arena.get(app.active_layer) {
                if let Some(new_id) = layer.nodes.last().copied() {
                    app.selection.clear();
                    app.selection.insert(new_id);
                }
            }
            app.dirty = true;
        }
        Interaction::Marquee { start_screen, current_screen } => {
            let s_min = Pos2::new(
                start_screen.x.min(current_screen.x),
                start_screen.y.min(current_screen.y),
            );
            let s_max = Pos2::new(
                start_screen.x.max(current_screen.x),
                start_screen.y.max(current_screen.y),
            );
            let w_min =
                app.view.screen_to_world(DVec2::new(s_min.x as f64, s_min.y as f64), center);
            let w_max =
                app.view.screen_to_world(DVec2::new(s_max.x as f64, s_max.y as f64), center);
            let bbox = (w_min.min(w_max), w_min.max(w_max));
            let mut hits: Vec<NodeId> = Vec::new();
            for layer_id in &app.document.layers {
                let Some(layer) = app.document.layer_arena.get(*layer_id) else { continue };
                if !layer.visible || layer.locked {
                    continue;
                }
                for node_id in &layer.nodes {
                    if let Some(node) = app.document.node_arena.get(*node_id) {
                        if let Some((mn, mx)) = node.bounds() {
                            if mn.x >= bbox.0.x
                                && mn.y >= bbox.0.y
                                && mx.x <= bbox.1.x
                                && mx.y <= bbox.1.y
                            {
                                hits.push(*node_id);
                            }
                        }
                    }
                }
            }
            for id in hits {
                app.selection.insert(id);
            }
        }
        Interaction::MovingSelection { accumulated, .. } => {
            if accumulated.length_squared() > 1e-12 {
                let ids: Vec<NodeId> = app.selection.iter().copied().collect();
                // Doc already moved during preview; record the command without re-applying.
                let cmd = MoveNodesCmd::new(ids, accumulated);
                app.commands.record_applied(Box::new(cmd));
                app.dirty = true;
            }
        }
        Interaction::MovingAnchors { items, accumulated, .. } => {
            if accumulated.length_squared() > 1e-12 {
                let cmd = MoveAnchorsCmd::new(items, accumulated);
                app.commands.record_applied(Box::new(cmd));
                app.dirty = true;
            }
        }
        Interaction::ResizingImage {
            node_id,
            original_position,
            original_size,
            original_anchors,
            ..
        } => {
            if let Some(node) = app.document.node_arena.get(node_id) {
                match &node.kind {
                    illuminator_core::doc::NodeKind::Image(img) => {
                        if img.position != original_position || img.size != original_size {
                            let cmd = ResizeImageCmd::new(
                                node_id,
                                original_position,
                                original_size,
                                img.position,
                                img.size,
                            );
                            app.commands.record_applied(Box::new(cmd));
                            app.dirty = true;
                        }
                    }
                    illuminator_core::doc::NodeKind::Path(pn) => {
                        if let Some(orig_anchors) = original_anchors {
                            let changed = orig_anchors.iter().zip(&pn.path.anchors).any(|(old, new)| {
                                (old.pos - new.pos).length_squared() > 1e-12
                                    || (old.in_handle - new.in_handle).length_squared() > 1e-12
                                    || (old.out_handle - new.out_handle).length_squared() > 1e-12
                            });
                            if changed {
                                let cmd = ResizePathCmd::new(
                                    node_id,
                                    orig_anchors.clone(),
                                    pn.path.anchors.clone(),
                                );
                                app.commands.record_applied(Box::new(cmd));
                                app.dirty = true;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Interaction::MovingHandle {
            node_id,
            anchor_idx,
            start_in,
            start_out,
            was_smooth,
            ..
        } => {
            // Look at the current handle state to compute deltas vs. drag start.
            if let Some(node) = app.document.node_arena.get(node_id) {
                if let illuminator_core::doc::NodeKind::Path(p) = &node.kind {
                    if let Some(a) = p.path.anchors.get(anchor_idx) {
                        let in_delta = a.in_handle - start_in;
                        let out_delta = a.out_handle - start_out;
                        if in_delta.length_squared() > 1e-12
                            || out_delta.length_squared() > 1e-12
                            || a.smooth != was_smooth
                        {
                            let cmd = EditHandleCmd::new(node_id, anchor_idx, in_delta, out_delta)
                                .with_smooth_change(was_smooth, a.smooth);
                            app.commands.record_applied(Box::new(cmd));
                            app.dirty = true;
                        }
                    }
                }
            }
        }
        Interaction::None | Interaction::Panning { .. } => {}
    }
}

fn on_click(app: &mut IlluminatorApp, ctx: &egui::Context, p: Pos2, center: DVec2) {
    match app.active_tool {
        ToolKind::Pen => {
            pen::on_click(app, p, center);
        }
        ToolKind::Text => {
            text::on_click(app, p, center);
        }
        ToolKind::Select | ToolKind::DirectSelect => {
            let world = app
                .view
                .screen_to_world(DVec2::new(p.x as f64, p.y as f64), center);
            let shift = ctx.input(|i| i.modifiers.shift);
            let hit = pick_node_at(app, world);
            if let Some(id) = hit {
                if shift {
                    if app.selection.contains(&id) {
                        app.selection.remove(&id);
                    } else {
                        app.selection.insert(id);
                    }
                } else {
                    app.selection.clear();
                    app.selection.insert(id);
                }
            } else if !shift {
                app.selection.clear();
            }
        }
        _ => {}
    }
}

/// When the Select tool is active and exactly one resizable node (Image or Path) is
/// selected, return the corner of that node whose screen-space handle is under `p`, if any.
fn pick_selection_corner(
    app: &IlluminatorApp,
    p: Pos2,
    center: DVec2,
) -> Option<(NodeId, Corner)> {
    if app.selection.len() != 1 {
        return None;
    }
    let id = *app.selection.iter().next()?;
    let node = app.document.node_arena.get(id)?;
    match &node.kind {
        illuminator_core::doc::NodeKind::Image(_) | illuminator_core::doc::NodeKind::Path(_) => {}
        _ => return None,
    }
    let (mn, mx) = node.bounds()?;
    let cursor = DVec2::new(p.x as f64, p.y as f64);
    for corner in [Corner::NW, Corner::NE, Corner::SE, Corner::SW] {
        let world = corner.world_pos(mn, mx);
        let s = app.view.world_to_screen(world, center);
        if (s - cursor).length() <= 7.0 {
            return Some((id, corner));
        }
    }
    None
}

fn pick_node_at(app: &IlluminatorApp, world: DVec2) -> Option<NodeId> {
    // layers[0] is topmost; within a layer, last node is topmost.
    for &layer_id in &app.document.layers {
        let Some(layer) = app.document.layer_arena.get(layer_id) else { continue };
        if !layer.visible || layer.locked {
            continue;
        }
        for &node_id in layer.nodes.iter().rev() {
            let Some(node) = app.document.node_arena.get(node_id) else { continue };
            if let Some((min, max)) = node.bounds() {
                if world.x >= min.x && world.x <= max.x && world.y >= min.y && world.y <= max.y {
                    return Some(node_id);
                }
            }
        }
    }
    None
}

fn translate_nodes(app: &mut IlluminatorApp, ids: &[NodeId], delta: DVec2) {
    for id in ids {
        let Some(node) = app.document.node_arena.get_mut(*id) else { continue };
        node.translate(delta);
    }
}

fn handle_dropped_files(
    app: &mut IlluminatorApp,
    files: Vec<egui::DroppedFile>,
    drop_world: DVec2,
) {
    let mut placed_at = drop_world;
    for file in files {
        let bytes = if let Some(b) = file.bytes {
            b.to_vec()
        } else if let Some(path) = file.path {
            // Could also be an .ilm dropped onto the canvas — try image first.
            match std::fs::read(&path) {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!("read {:?} failed: {e}", path);
                    continue;
                }
            }
        } else {
            continue;
        };
        match ImageData::from_bytes(bytes) {
            Ok(image) => {
                let size = DVec2::new(image.width as f64, image.height as f64);
                // Center on the drop point.
                let position = placed_at - size * 0.5;
                let node = Node::image(
                    "Image",
                    ImageNode {
                        position,
                        size,
                        style: Default::default(),
                        image,
                    },
                );
                let cmd: Box<dyn illuminator_core::command::Command> =
                    Box::new(AddNodeCmd::new(app.active_layer, node, "Place Image"));
                app.commands.push(cmd, &mut app.document);
                if let Some(layer) = app.document.layer_arena.get(app.active_layer) {
                    if let Some(new_id) = layer.nodes.last().copied() {
                        app.selection.clear();
                        app.selection.insert(new_id);
                    }
                }
                app.dirty = true;
                // Offset subsequent drops so they don't stack exactly.
                placed_at += DVec2::new(20.0, 20.0);
            }
            Err(e) => tracing::warn!("image decode failed: {e}"),
        }
    }
}

fn draw_overlays(app: &IlluminatorApp, painter: &Painter, _rect: Rect, center: DVec2) {
    // Selection bounding boxes
    for id in &app.selection {
        let Some(node) = app.document.node_arena.get(*id) else { continue };
        let bounds = node.bounds();
        if let Some((wmin, wmax)) = bounds {
            let smin = app.view.world_to_screen(wmin, center);
            let smax = app.view.world_to_screen(wmax, center);
            let r = Rect::from_two_pos(
                Pos2::new(smin.x as f32, smin.y as f32),
                Pos2::new(smax.x as f32, smax.y as f32),
            );
            painter.rect_stroke(r, 0.0, EguiStroke::new(1.0, SELECTION_COLOR));
            // Corner handles
            for c in [r.left_top(), r.right_top(), r.left_bottom(), r.right_bottom()] {
                let hs = 3.0;
                let handle = Rect::from_center_size(c, egui::vec2(hs * 2.0, hs * 2.0));
                painter.rect_filled(handle, 0.0, Color32::WHITE);
                painter.rect_stroke(handle, 0.0, EguiStroke::new(1.0, SELECTION_COLOR));
            }
        }
    }

    // In-progress shape preview
    if let Interaction::DrawingShape { kind, start_world, current_world } = &app.interaction {
        let smin = app.view.world_to_screen(start_world.min(*current_world), center);
        let smax = app.view.world_to_screen(start_world.max(*current_world), center);
        let r = Rect::from_two_pos(
            Pos2::new(smin.x as f32, smin.y as f32),
            Pos2::new(smax.x as f32, smax.y as f32),
        );
        let stroke = EguiStroke::new(1.0, SELECTION_COLOR);
        match kind {
            ShapeKind::Rect => {
                painter.rect_stroke(r, 0.0, stroke);
            }
            ShapeKind::Ellipse => {
                painter.add(Shape::Path(egui::epaint::PathShape {
                    points: sample_ellipse(r, 64),
                    closed: true,
                    fill: Color32::TRANSPARENT,
                    stroke: PathStroke::new(stroke.width, stroke.color),
                }));
            }
        }

        // Live dimensions tooltip
        let w_dist = (current_world.x - start_world.x).abs();
        let h_dist = (current_world.y - start_world.y).abs();
        let label = format!("{:.1} px × {:.1} px", w_dist, h_dist);
        let label_pos = r.max + egui::vec2(10.0, 10.0);
        let font_id = egui::FontId::proportional(11.0);
        let galley = painter.layout_no_wrap(label, font_id, Color32::WHITE);
        let tooltip_rect = Rect::from_min_size(label_pos, galley.size()).expand(4.0);
        painter.rect_filled(tooltip_rect, 3.0, Color32::from_black_alpha(180));
        painter.galley(label_pos, galley, Color32::WHITE);
    }

    // In-progress artboard preview
    if let Interaction::DrawingArtboard { start_world, current_world } = &app.interaction {
        let smin = app.view.world_to_screen(start_world.min(*current_world), center);
        let smax = app.view.world_to_screen(start_world.max(*current_world), center);
        let r = Rect::from_two_pos(
            Pos2::new(smin.x as f32, smin.y as f32),
            Pos2::new(smax.x as f32, smax.y as f32),
        );
        let stroke = EguiStroke::new(1.5, Color32::from_rgb(0x4d, 0xa0, 0xff));
        painter.rect_stroke(r, 0.0, stroke);
        
        // Label inside
        painter.text(
            r.min + egui::vec2(6.0, 6.0),
            egui::Align2::LEFT_TOP,
            "New Artboard",
            egui::FontId::proportional(11.0),
            Color32::from_rgb(0x4d, 0xa0, 0xff),
        );
    }

    // Marquee
    if let Interaction::Marquee { start_screen, current_screen } = &app.interaction {
        let r = Rect::from_two_pos(*start_screen, *current_screen);
        painter.rect_filled(r, 0.0, MARQUEE_FILL);
        painter.rect_stroke(r, 0.0, EguiStroke::new(1.0, SELECTION_COLOR));
    }

    // Snap-target hint
    if let Some(target) = app.last_snap_hint {
        let s = app.view.world_to_screen(target, center);
        let p = Pos2::new(s.x as f32, s.y as f32);
        
        // Detect if snapped to a path midpoint
        let mut is_midpoint = false;
        for (_id, node) in app.document.node_arena.iter() {
            if let illuminator_core::doc::NodeKind::Path(p) = &node.kind {
                let n = p.path.anchors.len();
                if n >= 2 {
                    for i in 0..n - 1 {
                        let m = (p.path.anchors[i].pos + p.path.anchors[i + 1].pos) * 0.5;
                        if (m - target).length_squared() < 1e-4 {
                            is_midpoint = true;
                            break;
                        }
                    }
                    if p.path.closed {
                        let m = (p.path.anchors[n - 1].pos + p.path.anchors[0].pos) * 0.5;
                        if (m - target).length_squared() < 1e-4 {
                            is_midpoint = true;
                        }
                    }
                }
            }
        }

        let snap_color = if is_midpoint {
            Color32::from_rgb(0x2e, 0xcc, 0x71) // Green for midpoint guides
        } else {
            Color32::from_rgb(0xff, 0xcc, 0x4d) // Yellow for normal smart-snap
        };

        painter.line_segment(
            [Pos2::new(p.x - 6.0, p.y), Pos2::new(p.x + 6.0, p.y)],
            EguiStroke::new(1.5, snap_color),
        );
        painter.line_segment(
            [Pos2::new(p.x, p.y - 6.0), Pos2::new(p.x, p.y + 6.0)],
            EguiStroke::new(1.5, snap_color),
        );
        painter.add(Shape::circle_stroke(p, 4.0, EguiStroke::new(1.0, snap_color)));

        if is_midpoint {
            let font_id = egui::FontId::proportional(10.0);
            let galley = painter.layout_no_wrap("Midpoint".to_string(), font_id, Color32::WHITE);
            let text_rect = Rect::from_min_size(p + egui::vec2(8.0, -14.0), galley.size()).expand(2.0);
            painter.rect_filled(text_rect, 2.0, Color32::from_black_alpha(180));
            painter.galley(p + egui::vec2(8.0, -14.0), galley, Color32::WHITE);
        }
    }
}

fn sample_ellipse(r: Rect, segments: usize) -> Vec<Pos2> {
    let c = r.center();
    let rx = r.width() * 0.5;
    let ry = r.height() * 0.5;
    (0..segments)
        .map(|i| {
            let t = i as f32 / segments as f32 * std::f32::consts::TAU;
            Pos2::new(c.x + rx * t.cos(), c.y + ry * t.sin())
        })
        .collect()
}

fn draw_origin_marker(
    painter: &Painter,
    view: &illuminator_core::transform::ViewTransform,
    rect: Rect,
    center: DVec2,
) {
    let origin_screen = view.world_to_screen(DVec2::ZERO, center);
    let p = Pos2::new(origin_screen.x as f32, origin_screen.y as f32);
    if rect.contains(p) {
        let dim = Color32::from_rgba_unmultiplied(255, 255, 255, 40);
        painter.line_segment(
            [Pos2::new(rect.left(), p.y), Pos2::new(rect.right(), p.y)],
            EguiStroke::new(1.0, dim),
        );
        painter.line_segment(
            [Pos2::new(p.x, rect.top()), Pos2::new(p.x, rect.bottom())],
            EguiStroke::new(1.0, dim),
        );
    }
}

fn update_cursor(ctx: &egui::Context, app: &IlluminatorApp) {
    let cursor = match app.active_tool {
        ToolKind::Select => egui::CursorIcon::Default,
        ToolKind::DirectSelect => egui::CursorIcon::PointingHand,
        ToolKind::Pen => egui::CursorIcon::Crosshair,
        ToolKind::Text => egui::CursorIcon::Text,
        ToolKind::Rectangle | ToolKind::Ellipse => egui::CursorIcon::Crosshair,
        ToolKind::Hand => egui::CursorIcon::Grab,
        ToolKind::Artboard => egui::CursorIcon::Crosshair,
    };
    ctx.set_cursor_icon(cursor);
}
