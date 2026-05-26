use crate::app::IlluminatorApp;
use crate::tools::ToolKind;

pub fn show(app: &mut IlluminatorApp, ctx: &egui::Context) {
    egui::SidePanel::left("tools_palette")
        .resizable(false)
        .exact_width(44.0)
        .show(ctx, |ui| {
            ui.add_space(4.0);
            for tool in ToolKind::all() {
                let selected = *tool == app.active_tool;
                let label = egui::RichText::new(tool.icon()).size(18.0);
                let btn = egui::Button::new(label)
                    .min_size(egui::vec2(36.0, 36.0))
                    .selected(selected);
                let resp = ui.add(btn).on_hover_text(format!(
                    "{} ({})",
                    tool.label(),
                    key_name(tool.shortcut())
                ));
                if resp.clicked() {
                    app.active_tool = *tool;
                }
                ui.add_space(2.0);
            }
        });
}

fn key_name(k: egui::Key) -> &'static str {
    match k {
        egui::Key::A => "A",
        egui::Key::V => "V",
        egui::Key::M => "M",
        egui::Key::L => "L",
        egui::Key::H => "H",
        egui::Key::P => "P",
        egui::Key::T => "T",
        _ => "?",
    }
}
