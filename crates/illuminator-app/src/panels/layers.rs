use illuminator_core::doc::{Layer, LayerId};

use crate::app::IlluminatorApp;

pub fn show(app: &mut IlluminatorApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.heading("Layers");
        ui.add_space(8.0);
        if ui.small_button("+").on_hover_text("New layer").clicked() {
            let n = app.document.layer_arena.len() + 1;
            let id = app.document.add_layer(Layer::new(format!("Layer {n}")));
            app.active_layer = id;
            app.dirty = true;
        }
        if ui
            .small_button("⊕")
            .on_hover_text("New reference layer")
            .clicked()
        {
            let id = app.document.add_layer(Layer::reference("Reference"));
            app.active_layer = id;
            app.dirty = true;
        }
        if ui
            .small_button("✕")
            .on_hover_text("Delete active layer")
            .clicked()
            && app.document.layers.len() > 1
        {
            let target = app.active_layer;
            app.document.remove_layer(target);
            if let Some(first) = app.document.layers.first().copied() {
                app.active_layer = first;
            }
            app.selection
                .retain(|id| app.document.node_arena.contains_key(*id));
            app.dirty = true;
        }
    });
    ui.add_space(4.0);

    let layer_ids: Vec<LayerId> = app.document.layers.clone();
    for layer_id in layer_ids {
        layer_row(app, ui, layer_id);
    }
}

fn layer_row(app: &mut IlluminatorApp, ui: &mut egui::Ui, layer_id: LayerId) {
    // Snapshot once so we don't hold a borrow across the mutating closure body.
    let snapshot = match app.document.layer_arena.get(layer_id) {
        Some(l) => LayerSnapshot {
            name: l.name.clone(),
            visible: l.visible,
            locked: l.locked,
            opacity: l.opacity,
            is_reference: l.kind.is_reference(),
        },
        None => return,
    };
    let is_active = layer_id == app.active_layer;

    let bg = if is_active {
        ui.visuals().selection.bg_fill.linear_multiply(0.35)
    } else {
        egui::Color32::TRANSPARENT
    };
    egui::Frame::default()
        .fill(bg)
        .inner_margin(egui::Margin::symmetric(4.0, 3.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let mut visible = snapshot.visible;
                if ui.add(egui::Checkbox::without_text(&mut visible)).changed() {
                    if let Some(l) = app.document.layer_arena.get_mut(layer_id) {
                        l.visible = visible;
                        app.dirty = true;
                    }
                }
                let lock_glyph = if snapshot.locked { "🔒" } else { "🔓" };
                if ui.small_button(lock_glyph).on_hover_text("Lock").clicked() {
                    if let Some(l) = app.document.layer_arena.get_mut(layer_id) {
                        l.locked = !l.locked;
                        app.dirty = true;
                    }
                }
                if snapshot.is_reference {
                    ui.label(egui::RichText::new("REF").small().weak());
                }
                let mut name = snapshot.name.clone();
                let resp =
                    ui.add(egui::TextEdit::singleline(&mut name).desired_width(140.0));
                if resp.changed() {
                    if let Some(l) = app.document.layer_arena.get_mut(layer_id) {
                        l.name = name;
                        app.dirty = true;
                    }
                }
                if resp.clicked() {
                    app.active_layer = layer_id;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Opacity");
                let mut opacity = snapshot.opacity;
                if ui
                    .add(egui::Slider::new(&mut opacity, 0.0..=1.0).show_value(false))
                    .changed()
                {
                    if let Some(l) = app.document.layer_arena.get_mut(layer_id) {
                        l.opacity = opacity;
                        app.dirty = true;
                    }
                }
                ui.label(format!("{:.0}%", snapshot.opacity * 100.0));
            });
        });
    ui.add_space(2.0);
}

struct LayerSnapshot {
    name: String,
    visible: bool,
    locked: bool,
    opacity: f32,
    is_reference: bool,
}
