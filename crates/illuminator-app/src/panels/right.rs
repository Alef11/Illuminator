use crate::app::IlluminatorApp;

pub fn show(app: &mut IlluminatorApp, ctx: &egui::Context) {
    egui::SidePanel::right("right_panels")
        .resizable(true)
        .default_width(280.0)
        .min_width(240.0)
        .max_width(480.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                super::layers::show(app, ui);
                ui.separator();
                super::properties::show(app, ui);
            });
        });
}
