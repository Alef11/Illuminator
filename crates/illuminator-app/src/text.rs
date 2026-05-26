//! Text tool — click to place, type to edit, Esc/Enter/click-out to commit.
//!
//! v1 uses egui's `Event::Text` / `Event::Key` stream for input. No cursor
//! rendering, no selection inside the text, no IME — that comes later.

use egui::{Color32, Painter, Pos2, Stroke as EguiStroke};
use glam::DVec2;

use illuminator_core::command::Command;
use illuminator_core::doc::{Node, NodeId, NodeKind};
use illuminator_core::text::{FontFamily, TextNode};

use crate::app::IlluminatorApp;
use crate::commands::AddNodeCmd;

const TEXT_PEN_COLOR: Color32 = Color32::from_rgb(0x4d, 0xa0, 0xff);

/// Handle a Text-tool click: focus the text under the cursor, or create a
/// new empty `TextNode` at the click position and immediately start editing it.
pub fn on_click(app: &mut IlluminatorApp, p: Pos2, center: DVec2) {
    let world = app
        .view
        .screen_to_world(DVec2::new(p.x as f64, p.y as f64), center);

    // Click on an existing TextNode → re-enter edit mode on it.
    if let Some(hit) = pick_text_at(app, world) {
        app.editing_text = Some(hit);
        app.selection.clear();
        app.selection.insert(hit);
        return;
    }

    // Otherwise create a new empty TextNode and start editing.
    let node = Node::text(
        "Text",
        TextNode {
            position: world,
            text: String::new(),
            font_size: 24.0,
            family: FontFamily::Proportional,
            color: app.default_stroke.unwrap_or(illuminator_core::style::Color::BLACK),
            style: Default::default(),
            width: None,
        },
    );
    let cmd: Box<dyn Command> = Box::new(AddNodeCmd::new(
        app.active_layer,
        node,
        "Add Text",
    ));
    app.commands.push(cmd, &mut app.document);
    if let Some(layer) = app.document.layer_arena.get(app.active_layer) {
        if let Some(new_id) = layer.nodes.last().copied() {
            app.selection.clear();
            app.selection.insert(new_id);
            app.editing_text = Some(new_id);
        }
    }
    app.dirty = true;
}

fn pick_text_at(app: &IlluminatorApp, world: DVec2) -> Option<NodeId> {
    for &layer_id in &app.document.layers {
        let Some(layer) = app.document.layer_arena.get(layer_id) else { continue };
        if !layer.visible || layer.locked {
            continue;
        }
        for &node_id in layer.nodes.iter().rev() {
            let Some(node) = app.document.node_arena.get(node_id) else { continue };
            if let NodeKind::Text(t) = &node.kind {
                let (min, max) = t.approx_bounds();
                if world.x >= min.x && world.x <= max.x && world.y >= min.y && world.y <= max.y {
                    return Some(node_id);
                }
            }
        }
    }
    None
}

/// Drain text input events from egui and apply them to the currently-edited
/// text node. Call once per frame from canvas.
pub fn process_events(app: &mut IlluminatorApp, ctx: &egui::Context) {
    let Some(text_id) = app.editing_text else { return };

    let events = ctx.input(|i| i.events.clone());
    let mut should_finish = false;
    let mut changed = false;
    for event in events {
        match event {
            egui::Event::Text(s) => {
                if let Some(node) = app.document.node_arena.get_mut(text_id) {
                    if let NodeKind::Text(t) = &mut node.kind {
                        t.text.push_str(&s);
                        changed = true;
                    }
                }
            }
            egui::Event::Key {
                key: egui::Key::Backspace,
                pressed: true,
                ..
            } => {
                if let Some(node) = app.document.node_arena.get_mut(text_id) {
                    if let NodeKind::Text(t) = &mut node.kind {
                        t.text.pop();
                        changed = true;
                    }
                }
            }
            egui::Event::Key {
                key: egui::Key::Enter,
                pressed: true,
                modifiers,
                ..
            } if !modifiers.shift => {
                should_finish = true;
            }
            egui::Event::Key {
                key: egui::Key::Enter,
                pressed: true,
                modifiers,
                ..
            } if modifiers.shift => {
                // Shift+Enter inserts a newline (egui won't have given us Text("\n")).
                if let Some(node) = app.document.node_arena.get_mut(text_id) {
                    if let NodeKind::Text(t) = &mut node.kind {
                        t.text.push('\n');
                        changed = true;
                    }
                }
            }
            egui::Event::Key {
                key: egui::Key::Escape,
                pressed: true,
                ..
            } => {
                should_finish = true;
            }
            _ => {}
        }
    }
    if changed {
        app.dirty = true;
    }
    if should_finish {
        finish(app);
    }
}

/// Stop editing the current text node. If it's empty, remove it (no point
/// in a zero-character text node).
pub fn finish(app: &mut IlluminatorApp) {
    let Some(id) = app.editing_text.take() else { return };
    let is_empty = matches!(
        app.document.node_arena.get(id),
        Some(node) if matches!(&node.kind, NodeKind::Text(t) if t.text.is_empty())
    );
    if is_empty {
        app.document.remove_node(id);
        app.selection.remove(&id);
    }
}

/// Draw the editing caret marker (just a frame around the text's bbox for v1).
pub fn draw_overlay(
    app: &IlluminatorApp,
    painter: &Painter,
    viewport_center: DVec2,
) {
    let Some(id) = app.editing_text else { return };
    let Some(node) = app.document.node_arena.get(id) else { return };
    let NodeKind::Text(t) = &node.kind else { return };
    let (mn, mx) = t.approx_bounds();
    let smin = app.view.world_to_screen(mn, viewport_center);
    let smax = app.view.world_to_screen(mx, viewport_center);
    let r = egui::Rect::from_two_pos(
        Pos2::new(smin.x as f32, smin.y as f32),
        Pos2::new(smax.x as f32, smax.y as f32),
    );
    painter.rect_stroke(r, 0.0, EguiStroke::new(1.0, TEXT_PEN_COLOR));
    // Blinking-cursor placeholder: vertical line at right edge.
    let cursor_x = r.right();
    let cursor_top = r.top();
    let cursor_bot = r.bottom();
    painter.line_segment(
        [Pos2::new(cursor_x, cursor_top), Pos2::new(cursor_x, cursor_bot)],
        EguiStroke::new(1.5, TEXT_PEN_COLOR),
    );
}
