use eframe::Frame;
use illuminator_core::doc::{Document, Layer, Node, NodeKind};
use illuminator_core::image::{ImageData, ImageNode};
use illuminator_core::io;
use illuminator_core::svg;

use crate::app::IlluminatorApp;
use crate::commands::{AddNodeCmd, DeleteNodesCmd, OutlineStrokeCmd, BooleanOpCmd, BooleanOp};
use crate::pen;
use crate::tools::ToolKind;

pub fn show(app: &mut IlluminatorApp, ctx: &egui::Context, _frame: &mut Frame) {
    egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
        egui::menu::bar(ui, |ui| {
            file_menu(app, ui, ctx);
            edit_menu(app, ui);
            view_menu(app, ui);
            symmetry_menu(app, ui);
            layer_menu(app, ui);
            help_menu(ui);
        });
    });

    handle_shortcuts(app, ctx);
}

fn file_menu(app: &mut IlluminatorApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.menu_button("File", |ui| {
        if ui.button("New").clicked() {
            app.replace_document(Document::default(), None);
            ui.close_menu();
        }
        if ui.button("Open…").clicked() {
            ui.close_menu();
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Illuminator", &["ilm"])
                .pick_file()
            {
                match io::load_document(&path) {
                    Ok(doc) => app.replace_document(doc, Some(path)),
                    Err(e) => tracing::error!("open failed: {e}"),
                }
            }
        }
        ui.separator();
        let save_enabled = app.current_path.is_some();
        if ui.add_enabled(save_enabled, egui::Button::new("Save")).clicked() {
            ui.close_menu();
            save(app, false);
        }
        if ui.button("Save As…").clicked() {
            ui.close_menu();
            save(app, true);
        }
        ui.separator();
        if ui.button("Place Image…").clicked() {
            ui.close_menu();
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Image", &["png", "jpg", "jpeg", "bmp", "webp"])
                .pick_file()
            {
                match std::fs::read(&path) {
                    Ok(bytes) => match ImageData::from_bytes(bytes) {
                        Ok(image) => place_image_at_view_center(app, image),
                        Err(e) => tracing::error!("decode failed: {e}"),
                    },
                    Err(e) => tracing::error!("read failed: {e}"),
                }
            }
        }
        ui.separator();
        if ui.button("Import SVG…").clicked() {
            ui.close_menu();
            import_svg_file(app);
        }
        if ui.button("Export SVG…").clicked() {
            ui.close_menu();
            export_svg_file(app);
        }
        if ui.add_enabled(!app.selection.is_empty(), egui::Button::new("Export Selection as SVG…")).clicked() {
            ui.close_menu();
            export_svg_selection_file(app);
        }
        ui.separator();
        if ui.button("Quit").clicked() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    });
}

fn place_image_at_view_center(app: &mut IlluminatorApp, image: ImageData) {
    let size = glam::DVec2::new(image.width as f64, image.height as f64);
    // Center on the current cursor world position if available, else view pan.
    let center = app.last_cursor_world.unwrap_or(app.view.pan);
    let position = center - size * 0.5;
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
}

fn edit_menu(app: &mut IlluminatorApp, ui: &mut egui::Ui) {
    ui.menu_button("Edit", |ui| {
        let undo_label = app
            .commands
            .undo_label()
            .map(|s| format!("Undo {s}"))
            .unwrap_or_else(|| "Undo".to_string());
        if ui
            .add_enabled(app.commands.can_undo(), egui::Button::new(undo_label))
            .clicked()
        {
            app.commands.undo(&mut app.document);
            app.dirty = true;
            ui.close_menu();
        }
        let redo_label = app
            .commands
            .redo_label()
            .map(|s| format!("Redo {s}"))
            .unwrap_or_else(|| "Redo".to_string());
        if ui
            .add_enabled(app.commands.can_redo(), egui::Button::new(redo_label))
            .clicked()
        {
            app.commands.redo(&mut app.document);
            app.dirty = true;
            ui.close_menu();
        }
        ui.separator();
        if ui
            .add_enabled(!app.selection.is_empty(), egui::Button::new("Delete"))
            .clicked()
        {
            let ids: Vec<_> = app.selection.iter().copied().collect();
            app.commands
                .push(Box::new(DeleteNodesCmd::new(ids)), &mut app.document);
            app.selection.clear();
            app.dirty = true;
            ui.close_menu();
        }
        if ui.button("Select All").clicked() {
            select_all(app);
            ui.close_menu();
        }
        if ui.button("Deselect").clicked() {
            app.selection.clear();
            ui.close_menu();
        }

        ui.separator();
        
        // Boolean Operations Menu
        ui.menu_button("Boolean Operations", |ui| {
            let bool_enabled = app.selection.len() >= 2;
            if ui.add_enabled(bool_enabled, egui::Button::new("Union")).clicked() {
                let ids: Vec<_> = app.selection.iter().copied().collect();
                let cmd = BooleanOpCmd::new(ids, BooleanOp::Union);
                app.commands.push(Box::new(cmd), &mut app.document);
                app.selection.clear();
                app.dirty = true;
                ui.close_menu();
            }
            if ui.add_enabled(bool_enabled, egui::Button::new("Intersect")).clicked() {
                let ids: Vec<_> = app.selection.iter().copied().collect();
                let cmd = BooleanOpCmd::new(ids, BooleanOp::Intersect);
                app.commands.push(Box::new(cmd), &mut app.document);
                app.selection.clear();
                app.dirty = true;
                ui.close_menu();
            }
            if ui.add_enabled(bool_enabled, egui::Button::new("Subtract")).clicked() {
                let ids: Vec<_> = app.selection.iter().copied().collect();
                let cmd = BooleanOpCmd::new(ids, BooleanOp::Subtract);
                app.commands.push(Box::new(cmd), &mut app.document);
                app.selection.clear();
                app.dirty = true;
                ui.close_menu();
            }
        });

        // Outline Stroke Menu
        let outline_enabled = if app.selection.len() == 1 {
            let id = *app.selection.iter().next().unwrap();
            if let Some(node) = app.document.node_arena.get(id) {
                matches!(node.kind, NodeKind::Path(_))
            } else {
                false
            }
        } else {
            false
        };
        if ui.add_enabled(outline_enabled, egui::Button::new("Outline Stroke")).clicked() {
            let id = *app.selection.iter().next().unwrap();
            let cmd = OutlineStrokeCmd::new(id);
            app.commands.push(Box::new(cmd), &mut app.document);
            app.dirty = true;
            ui.close_menu();
        }
    });
}

fn view_menu(app: &mut IlluminatorApp, ui: &mut egui::Ui) {
    ui.menu_button("View", |ui| {
        if ui.button("Zoom In").clicked() {
            app.view.zoom = (app.view.zoom * 1.25)
                .clamp(illuminator_core::transform::ViewTransform::MIN_ZOOM,
                       illuminator_core::transform::ViewTransform::MAX_ZOOM);
            ui.close_menu();
        }
        if ui.button("Zoom Out").clicked() {
            app.view.zoom = (app.view.zoom / 1.25)
                .clamp(illuminator_core::transform::ViewTransform::MIN_ZOOM,
                       illuminator_core::transform::ViewTransform::MAX_ZOOM);
            ui.close_menu();
        }
        if ui.button("Actual Size (100%)").clicked() {
            app.view.zoom = 1.0;
            ui.close_menu();
        }
        if ui.button("Reset View").clicked() {
            app.view = illuminator_core::transform::ViewTransform::default();
            ui.close_menu();
        }
        ui.separator();
        let mut grid = app.snap.grid_enabled;
        if ui.checkbox(&mut grid, "Snap to Grid").changed() {
            app.snap.grid_enabled = grid;
        }
        ui.horizontal(|ui| {
            ui.label("  Size");
            ui.add(
                egui::DragValue::new(&mut app.snap.grid_size)
                    .speed(0.5)
                    .range(1.0..=10_000.0)
                    .suffix(" px"),
            );
        });
        let mut smart = app.snap.smart_enabled;
        if ui.checkbox(&mut smart, "Smart Guides (anchor snap)").changed() {
            app.snap.smart_enabled = smart;
        }
    });
}

fn layer_menu(app: &mut IlluminatorApp, ui: &mut egui::Ui) {
    ui.menu_button("Layer", |ui| {
        if ui.button("New Layer").clicked() {
            let n = app.document.layer_arena.len() + 1;
            let id = app
                .document
                .add_layer(Layer::new(format!("Layer {n}")));
            app.active_layer = id;
            app.dirty = true;
            ui.close_menu();
        }
        if ui.button("New Reference Layer").clicked() {
            let id = app
                .document
                .add_layer(Layer::reference("Reference"));
            app.active_layer = id;
            app.dirty = true;
            ui.close_menu();
        }
        ui.separator();
        if ui.button("New Artboard").clicked() {
            let n = app.document.artboards.len() + 1;
            let offset = (n - 1) as f64 * 850.0;
            app.document.artboards.push(illuminator_core::doc::Artboard {
                name: format!("Artboard {n}"),
                min: glam::DVec2::new(-400.0 + offset, -300.0),
                max: glam::DVec2::new(400.0 + offset, 300.0),
            });
            app.dirty = true;
            ui.close_menu();
        }
    });
}

fn help_menu(ui: &mut egui::Ui) {
    ui.menu_button("Help", |ui| {
        ui.label("Illuminator v0.1");
        ui.label("Native vector design for Windows.");
    });
}

fn handle_shortcuts(app: &mut IlluminatorApp, ctx: &egui::Context) {
    let any_focused = ctx.memory(|m| m.focused()).is_some();
    // Text editing is essentially a focus state for our purposes: skip global
    // shortcuts so the user can type V/M/L/etc. as text content.
    let editing_text = app.editing_text.is_some();
    ctx.input_mut(|i| {
        // Ctrl+Z / Ctrl+Y
        if i.consume_key(egui::Modifiers::CTRL, egui::Key::Z) {
            app.commands.undo(&mut app.document);
            app.dirty = true;
        }
        if i.consume_key(egui::Modifiers::CTRL, egui::Key::Y)
            || i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::Z)
        {
            app.commands.redo(&mut app.document);
            app.dirty = true;
        }
        // Ctrl+S / Ctrl+Shift+S
        if i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::S) {
            save(app, true);
        } else if i.consume_key(egui::Modifiers::CTRL, egui::Key::S) {
            save(app, false);
        }
        // Ctrl+Shift+E — Export SVG
        if i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::E) {
            export_svg_file(app);
        }
        // Zoom/View shortcuts
        if i.consume_key(egui::Modifiers::CTRL, egui::Key::Plus)
            || i.consume_key(egui::Modifiers::CTRL, egui::Key::Equals)
        {
            app.view.zoom = (app.view.zoom * 1.25)
                .clamp(illuminator_core::transform::ViewTransform::MIN_ZOOM,
                       illuminator_core::transform::ViewTransform::MAX_ZOOM);
        }
        if i.consume_key(egui::Modifiers::CTRL, egui::Key::Minus) {
            app.view.zoom = (app.view.zoom / 1.25)
                .clamp(illuminator_core::transform::ViewTransform::MIN_ZOOM,
                       illuminator_core::transform::ViewTransform::MAX_ZOOM);
        }
        if i.consume_key(egui::Modifiers::CTRL, egui::Key::Num0) {
            app.view = illuminator_core::transform::ViewTransform::default();
        }
        if i.consume_key(egui::Modifiers::CTRL, egui::Key::Num1) {
            app.view.zoom = 1.0;
        }
        // Ctrl+A
        if i.consume_key(egui::Modifiers::CTRL, egui::Key::A) {
            select_all(app);
        }
        // Pen-tool-specific keys (Esc cancel, Enter finish, Backspace remove last anchor)
        let pen_active = app.active_tool == ToolKind::Pen && app.pen_state.is_some();
        if pen_active {
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                pen::cancel(app);
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Enter) {
                pen::finish(app, false);
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace) {
                pen::remove_last_anchor(app);
            }
        } else if editing_text {
            // Text-edit owns Backspace/Delete/Enter/Escape — handled in
            // text::process_events. Do nothing here so we don't double-process.
        } else if (i.consume_key(egui::Modifiers::NONE, egui::Key::Delete)
            || i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace))
            && !app.selection.is_empty()
        {
            // Delete / Backspace removes selection (only when not pen-drawing)
            let ids: Vec<_> = app.selection.iter().copied().collect();
            app.commands
                .push(Box::new(DeleteNodesCmd::new(ids)), &mut app.document);
            app.selection.clear();
            app.dirty = true;
        }
        // Tool shortcuts (V/M/L/H/P/A/T) — only if no widget has focus and no
        // text-edit session is active (otherwise typing 't' would yank you out
        // of the text and into the Text tool, which… is funny but wrong).
        if !any_focused && !editing_text {
            for tool in crate::tools::ToolKind::all() {
                if i.consume_key(egui::Modifiers::NONE, tool.shortcut()) {
                    app.active_tool = *tool;
                }
            }
        }
    });
}

fn save(app: &mut IlluminatorApp, force_dialog: bool) {
    let path = if force_dialog || app.current_path.is_none() {
        rfd::FileDialog::new()
            .add_filter("Illuminator", &["ilm"])
            .set_file_name(&format!("{}.ilm", app.document.title))
            .save_file()
    } else {
        app.current_path.clone()
    };
    if let Some(path) = path {
        match io::save_document(&app.document, &path) {
            Ok(()) => {
                app.current_path = Some(path);
                app.dirty = false;
            }
            Err(e) => tracing::error!("save failed: {e}"),
        }
    }
}

fn select_all(app: &mut IlluminatorApp) {
    app.selection.clear();
    for layer_id in &app.document.layers {
        let Some(layer) = app.document.layer_arena.get(*layer_id) else { continue };
        if !layer.visible || layer.locked {
            continue;
        }
        for node_id in &layer.nodes {
            app.selection.insert(*node_id);
        }
    }
}

fn import_svg_file(app: &mut IlluminatorApp) {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("SVG", &["svg", "svgz"])
        .pick_file()
    else {
        return;
    };
    let file_stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("SVG Import")
        .to_string();
    match svg::import_svg(&path) {
        Ok(result) => {
            let layer_id = app.document.add_layer(Layer::new(&file_stem));
            app.active_layer = layer_id;
            app.selection.clear();
            for pn in result.paths {
                let node = Node::path("Path", pn);
                if let Some(nid) = app.document.add_node(layer_id, node) {
                    app.selection.insert(nid);
                }
            }
            for img in result.images {
                let node = Node::image("Image", img);
                if let Some(nid) = app.document.add_node(layer_id, node) {
                    app.selection.insert(nid);
                }
            }
            app.dirty = true;
            tracing::info!(layer = %file_stem, "SVG imported");
        }
        Err(e) => tracing::error!("SVG import failed: {e}"),
    }
}

fn export_svg_file(app: &mut IlluminatorApp) {
    let default_name = app
        .current_path
        .as_ref()
        .and_then(|p| p.file_stem())
        .and_then(|s| s.to_str())
        .unwrap_or(&app.document.title);
    let Some(path) = rfd::FileDialog::new()
        .add_filter("SVG", &["svg"])
        .set_file_name(&format!("{default_name}.svg"))
        .save_file()
    else {
        return;
    };
    let opts = svg::ExportOptions::default();
    match svg::export_svg(&app.document, &path, &opts) {
        Ok(()) => tracing::info!(?path, "SVG exported"),
        Err(e) => tracing::error!("SVG export failed: {e}"),
    }
}

fn export_svg_selection_file(app: &mut IlluminatorApp) {
    if app.selection.is_empty() {
        return;
    }
    let default_name = app
        .current_path
        .as_ref()
        .and_then(|p| p.file_stem())
        .and_then(|s| s.to_str())
        .map(|s| format!("{}_selection", s))
        .unwrap_or_else(|| format!("{}_selection", app.document.title));
    let Some(path) = rfd::FileDialog::new()
        .add_filter("SVG", &["svg"])
        .set_file_name(&format!("{default_name}.svg"))
        .save_file()
    else {
        return;
    };
    let opts = svg::ExportOptions::default();
    match svg::export_svg_selection(&app.document, &app.selection, &path, &opts) {
        Ok(()) => tracing::info!(?path, "SVG selection exported"),
        Err(e) => tracing::error!("SVG selection export failed: {e}"),
    }
}

fn symmetry_menu(app: &mut IlluminatorApp, ui: &mut egui::Ui) {
    ui.menu_button("Symmetry", |ui| {
        ui.label(egui::RichText::new("Drawing Mirror Mode").weak());
        ui.separator();
        if ui.selectable_value(&mut app.symmetry, crate::app::SymmetryMode::None, "None").clicked() {
            ui.close_menu();
        }
        if ui.selectable_value(&mut app.symmetry, crate::app::SymmetryMode::Horizontal, "Horizontal (X-Axis)").clicked() {
            ui.close_menu();
        }
        if ui.selectable_value(&mut app.symmetry, crate::app::SymmetryMode::Vertical, "Vertical (Y-Axis)").clicked() {
            ui.close_menu();
        }
        if ui.selectable_value(&mut app.symmetry, crate::app::SymmetryMode::Dual, "Dual Axis (Both)").clicked() {
            ui.close_menu();
        }
    });
}

