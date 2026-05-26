use crate::app::IlluminatorApp;

pub fn show(app: &mut IlluminatorApp, ctx: &egui::Context) {
    egui::TopBottomPanel::bottom("status_bar")
        .exact_height(24.0)
        .show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(format!("{}", app.active_tool.label()));
                ui.separator();
                ui.label(format!("Zoom: {:.0}%", app.view.zoom * 100.0));
                ui.separator();
                if let Some(cursor) = app.last_cursor_world {
                    ui.label(format!("Cursor: ({:.1}, {:.1})", cursor.x, cursor.y));
                    ui.separator();
                }
                ui.label(format!(
                    "Pan: ({:.1}, {:.1})",
                    app.view.pan.x, app.view.pan.y
                ));
                ui.separator();
                ui.label(format!("Selection: {}", app.selection.len()));
                ui.separator();
                ui.label(format!(
                    "Layer: {}",
                    app.document
                        .layer_arena
                        .get(app.active_layer)
                        .map(|l| l.name.as_str())
                        .unwrap_or("—")
                ));
            });
        });
}
