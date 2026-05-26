use glam::DVec2;
use illuminator_core::doc::NodeKind;
use illuminator_core::style::{Color, Paint, Stroke};

use crate::app::IlluminatorApp;
use crate::commands::AlignDistributeCmd;

pub fn show(app: &mut IlluminatorApp, ui: &mut egui::Ui) {
    ui.heading("Properties");
    ui.add_space(4.0);

    let selection_count = app.selection.len();
    if selection_count == 0 {
        ui.label(egui::RichText::new("No selection").weak());
        ui.separator();
        defaults_section(app, ui);
        return;
    }

    ui.label(format!("{selection_count} selected"));
    ui.add_space(4.0);

    align_distribute_section(app, ui);

    if selection_count == 1 {
        let id = *app.selection.iter().next().unwrap();
        if let Some(node) = app.document.node_arena.get_mut(id) {
            if let Some((mn, mx)) = node.bounds() {
                ui.group(|ui| {
                    ui.label(egui::RichText::new("Transform").strong());
                    ui.horizontal(|ui| {
                        ui.label("X:");
                        let mut x_val = mn.x;
                        if ui.add(egui::DragValue::new(&mut x_val).speed(1.0)).changed() {
                            let delta = glam::DVec2::new(x_val - mn.x, 0.0);
                            node.translate(delta);
                            app.dirty = true;
                        }
                        ui.label("Y:");
                        let mut y_val = mn.y;
                        if ui.add(egui::DragValue::new(&mut y_val).speed(1.0)).changed() {
                            let delta = glam::DVec2::new(0.0, y_val - mn.y);
                            node.translate(delta);
                            app.dirty = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        let current_w = mx.x - mn.x;
                        let current_h = mx.y - mn.y;
                        ui.label("W:");
                        let mut w_val = current_w;
                        if ui.add(egui::DragValue::new(&mut w_val).speed(1.0).range(1.0..=f64::MAX)).changed() {
                            let scale_x = w_val / current_w.max(1e-6);
                            scale_node_relative_to(node, mn, glam::DVec2::new(scale_x, 1.0));
                            app.dirty = true;
                        }
                        ui.label("H:");
                        let mut h_val = current_h;
                        if ui.add(egui::DragValue::new(&mut h_val).speed(1.0).range(1.0..=f64::MAX)).changed() {
                            let scale_y = h_val / current_h.max(1e-6);
                            scale_node_relative_to(node, mn, glam::DVec2::new(1.0, scale_y));
                            app.dirty = true;
                        }
                    });
                });
                ui.add_space(4.0);
            }
        }
    }

    // For Phase 1, edit only the first selected node's style. Batch edit later.
    let id = *app.selection.iter().next().unwrap();
    let Some(node) = app.document.node_arena.get_mut(id) else {
        return;
    };

    match &mut node.kind {
        NodeKind::Image(img) => {
            image_properties(img, ui, &mut app.dirty);
            return;
        }
        NodeKind::Text(t) => {
            text_properties(t, ui, &mut app.dirty);
            return;
        }
        NodeKind::Path(_) => {} // fall through to path editor
    }

    let path_node = match &mut node.kind {
        NodeKind::Path(p) => p,
        _ => unreachable!(),
    };
    let style = &mut path_node.style;

    let mut fill_type = match &style.fill {
        Some(Paint::Solid(_)) => 0,
        Some(Paint::LinearGradient { .. }) => 1,
        Some(Paint::RadialGradient { .. }) => 2,
        None => 3,
    };

    ui.horizontal(|ui| {
        ui.label("Fill Type:");
        let changed = ui.selectable_value(&mut fill_type, 3, "None").changed()
            || ui.selectable_value(&mut fill_type, 0, "Solid").changed()
            || ui.selectable_value(&mut fill_type, 1, "Linear").changed()
            || ui.selectable_value(&mut fill_type, 2, "Radial").changed();

        if changed {
            style.fill = match fill_type {
                0 => Some(Paint::Solid(Color::rgb(0.8, 0.8, 0.8))),
                1 => Some(Paint::LinearGradient {
                    start: DVec2::new(-100.0, 0.0),
                    end: DVec2::new(100.0, 0.0),
                    stops: vec![
                        illuminator_core::style::GradientStop { offset: 0.0, color: Color::BLACK },
                        illuminator_core::style::GradientStop { offset: 1.0, color: Color::WHITE },
                    ],
                }),
                2 => Some(Paint::RadialGradient {
                    center: DVec2::ZERO,
                    radius: 100.0,
                    stops: vec![
                        illuminator_core::style::GradientStop { offset: 0.0, color: Color::WHITE },
                        illuminator_core::style::GradientStop { offset: 1.0, color: Color::BLACK },
                    ],
                }),
                _ => None,
            };
            app.dirty = true;
        }
    });

    if let Some(fill_paint) = &mut style.fill {
        match fill_paint {
            Paint::Solid(c) => {
                ui.horizontal(|ui| {
                    ui.label("  Color:");
                    let mut rgba = c.to_array();
                    if ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed() {
                        *c = Color::from_array(rgba);
                        app.dirty = true;
                    }
                });
            }
            Paint::LinearGradient { start, end, stops } => {
                ui.horizontal(|ui| {
                    ui.label("  Start:");
                    let x_changed = ui.add(egui::DragValue::new(&mut start.x).speed(1.0).prefix("X ")).changed();
                    let y_changed = ui.add(egui::DragValue::new(&mut start.y).speed(1.0).prefix("Y ")).changed();
                    if x_changed || y_changed { app.dirty = true; }
                });
                ui.horizontal(|ui| {
                    ui.label("  End  :");
                    let x_changed = ui.add(egui::DragValue::new(&mut end.x).speed(1.0).prefix("X ")).changed();
                    let y_changed = ui.add(egui::DragValue::new(&mut end.y).speed(1.0).prefix("Y ")).changed();
                    if x_changed || y_changed { app.dirty = true; }
                });
                edit_gradient_stops(stops, ui, &mut app.dirty);
            }
            Paint::RadialGradient { center, radius, stops } => {
                ui.horizontal(|ui| {
                    ui.label("  Center:");
                    let x_changed = ui.add(egui::DragValue::new(&mut center.x).speed(1.0).prefix("X ")).changed();
                    let y_changed = ui.add(egui::DragValue::new(&mut center.y).speed(1.0).prefix("Y ")).changed();
                    if x_changed || y_changed { app.dirty = true; }
                });
                ui.horizontal(|ui| {
                    ui.label("  Radius:");
                    if ui.add(egui::DragValue::new(radius).speed(1.0).range(1.0..=10_000.0)).changed() {
                        app.dirty = true;
                    }
                });
                edit_gradient_stops(stops, ui, &mut app.dirty);
            }
        }
    }

    let mut stroke_enabled = style.stroke.is_some();
    let mut stroke_color = match &style.stroke {
        Some(s) => match &s.paint {
            Paint::Solid(c) => *c,
            Paint::LinearGradient { stops, .. } | Paint::RadialGradient { stops, .. } => {
                stops.first().map(|st| st.color).unwrap_or(Color::BLACK)
            }
        },
        None => Color::BLACK,
    };
    let mut stroke_width = style.stroke.as_ref().map(|s| s.width).unwrap_or(1.0);
    ui.horizontal(|ui| {
        let toggled = ui.checkbox(&mut stroke_enabled, "Stroke").changed();
        let mut rgba = stroke_color.to_array();
        let color_changed = ui
            .color_edit_button_rgba_unmultiplied(&mut rgba)
            .changed();
        if color_changed {
            stroke_color = Color::from_array(rgba);
        }
        let width_changed = ui
            .add(
                egui::DragValue::new(&mut stroke_width)
                    .range(0.0..=200.0)
                    .speed(0.1)
                    .suffix(" px"),
            )
            .changed();
        if toggled || color_changed || width_changed {
            style.stroke = if stroke_enabled {
                Some(Stroke {
                    paint: Paint::Solid(stroke_color),
                    width: stroke_width,
                    ..Default::default()
                })
            } else {
                None
            };
            app.dirty = true;
        }
    });

    let mut opacity = style.opacity;
    if ui
        .add(egui::Slider::new(&mut opacity, 0.0..=1.0).text("Opacity"))
        .changed()
    {
        style.opacity = opacity;
        app.dirty = true;
    }
}

fn text_properties(
    t: &mut illuminator_core::text::TextNode,
    ui: &mut egui::Ui,
    dirty: &mut bool,
) {
    ui.label(egui::RichText::new("Text").weak());

    ui.horizontal(|ui| {
        ui.label("Font");
        let mut is_mono = matches!(t.family, illuminator_core::text::FontFamily::Monospace);
        if ui.selectable_label(!is_mono, "Sans").clicked() {
            t.family = illuminator_core::text::FontFamily::Proportional;
            *dirty = true;
            is_mono = false;
        }
        if ui.selectable_label(is_mono, "Mono").clicked() {
            t.family = illuminator_core::text::FontFamily::Monospace;
            *dirty = true;
        }
    });

    let mut size = t.font_size;
    if ui
        .add(
            egui::DragValue::new(&mut size)
                .speed(0.5)
                .range(4.0..=2_000.0)
                .suffix(" px")
                .prefix("Size "),
        )
        .changed()
    {
        t.font_size = size;
        *dirty = true;
    }

    let mut rgba = t.color.to_array();
    if ui
        .horizontal(|ui| {
            ui.label("Color");
            ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed()
        })
        .inner
    {
        t.color = Color::from_array(rgba);
        *dirty = true;
    }

    let mut opacity = t.style.opacity;
    if ui
        .add(egui::Slider::new(&mut opacity, 0.0..=1.0).text("Opacity"))
        .changed()
    {
        t.style.opacity = opacity;
        *dirty = true;
    }

    ui.horizontal(|ui| {
        let mut has_wrap = t.width.is_some();
        if ui.checkbox(&mut has_wrap, "Wrap Width").changed() {
            if has_wrap {
                t.width = Some(300.0);
            } else {
                t.width = None;
            }
            *dirty = true;
        }
        if let Some(w) = &mut t.width {
            if ui
                .add(
                    egui::DragValue::new(w)
                        .speed(1.0)
                        .range(10.0..=10_000.0)
                        .suffix(" px"),
                )
                .changed()
            {
                *dirty = true;
            }
        }
    });

    ui.separator();
    ui.label(egui::RichText::new("Content").small().weak());
    if ui
        .add(
            egui::TextEdit::multiline(&mut t.text)
                .desired_rows(2)
                .desired_width(f32::INFINITY),
        )
        .changed()
    {
        *dirty = true;
    }
}

fn image_properties(
    img: &mut illuminator_core::image::ImageNode,
    ui: &mut egui::Ui,
    dirty: &mut bool,
) {
    ui.label(egui::RichText::new("Image").weak());
    ui.label(format!("{} × {} px", img.image.width, img.image.height));
    ui.separator();

    let mut opacity = img.style.opacity;
    if ui
        .add(egui::Slider::new(&mut opacity, 0.0..=1.0).text("Opacity"))
        .changed()
    {
        img.style.opacity = opacity;
        *dirty = true;
    }
    ui.add_space(4.0);

    let mut size_x = img.size.x;
    let mut size_y = img.size.y;
    let aspect = img.image.width as f64 / img.image.height.max(1) as f64;
    ui.horizontal(|ui| {
        ui.label("Size");
        let x_changed = ui
            .add(
                egui::DragValue::new(&mut size_x)
                    .speed(1.0)
                    .range(1.0..=200_000.0)
                    .prefix("W "),
            )
            .changed();
        let y_changed = ui
            .add(
                egui::DragValue::new(&mut size_y)
                    .speed(1.0)
                    .range(1.0..=200_000.0)
                    .prefix("H "),
            )
            .changed();
        if x_changed && !y_changed {
            size_y = size_x / aspect.max(1e-9);
        } else if y_changed && !x_changed {
            size_x = size_y * aspect;
        }
        if x_changed || y_changed {
            img.size = glam::DVec2::new(size_x, size_y);
            *dirty = true;
        }
    });

    if ui.button("Reset to original size").clicked() {
        img.size = glam::DVec2::new(img.image.width as f64, img.image.height as f64);
        *dirty = true;
    }
}

fn defaults_section(app: &mut IlluminatorApp, ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("Defaults for new shapes").small().weak());

    let mut fill_enabled = app.default_fill.is_some();
    let mut fill = app.default_fill.unwrap_or(Color::rgb(0.85, 0.85, 0.85));
    ui.horizontal(|ui| {
        let toggled = ui.checkbox(&mut fill_enabled, "Fill").changed();
        let mut rgba = fill.to_array();
        let changed = ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed();
        if changed {
            fill = Color::from_array(rgba);
        }
        if toggled || changed {
            app.default_fill = fill_enabled.then_some(fill);
        }
    });

    let mut stroke_enabled = app.default_stroke.is_some();
    let mut stroke = app.default_stroke.unwrap_or(Color::BLACK);
    let mut width = app.default_stroke_width;
    ui.horizontal(|ui| {
        let toggled = ui.checkbox(&mut stroke_enabled, "Stroke").changed();
        let mut rgba = stroke.to_array();
        let changed = ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed();
        if changed {
            stroke = Color::from_array(rgba);
        }
        let width_changed = ui
            .add(
                egui::DragValue::new(&mut width)
                    .range(0.0..=200.0)
                    .speed(0.1)
                    .suffix(" px"),
            )
            .changed();
        if toggled || changed || width_changed {
            app.default_stroke = stroke_enabled.then_some(stroke);
            app.default_stroke_width = width;
        }
    });

    ui.separator();
    ui.label(egui::RichText::new("Symmetry & Mirroring").small().weak());
    ui.horizontal(|ui| {
        ui.label("Mirror Mode:");
        let mut mode = app.symmetry;
        let changed = ui.selectable_value(&mut mode, crate::app::SymmetryMode::None, "None").changed()
            || ui.selectable_value(&mut mode, crate::app::SymmetryMode::Horizontal, "H-Axis").changed()
            || ui.selectable_value(&mut mode, crate::app::SymmetryMode::Vertical, "V-Axis").changed()
            || ui.selectable_value(&mut mode, crate::app::SymmetryMode::Dual, "Dual").changed();
        if changed {
            app.symmetry = mode;
        }
    });

    ui.separator();
    ui.label(egui::RichText::new("Artboards").small().weak());
    
    if app.document.artboards.is_empty() {
        ui.label(egui::RichText::new("No artboards. Use Artboard tool (◰ / O) to draw one!").small().weak());
    } else {
        let mut to_delete = None;
        for (idx, artboard) in app.document.artboards.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut artboard.name);
                if ui.button("🗑").clicked() {
                    to_delete = Some(idx);
                }
            });
        }
        if let Some(idx) = to_delete {
            app.document.artboards.remove(idx);
            app.dirty = true;
        }
    }
}

fn align_distribute_section(app: &mut IlluminatorApp, ui: &mut egui::Ui) {
    let selection_count = app.selection.len();
    if selection_count < 2 {
        return;
    }

    ui.separator();
    ui.label(egui::RichText::new("Align & Distribute").small().weak());
    ui.add_space(2.0);

    // Compute bounding boxes of all selected nodes that have valid bounds.
    let mut bounds_list = Vec::new();
    for id in &app.selection {
        if let Some(node) = app.document.node_arena.get(*id) {
            if let Some((min, max)) = node.bounds() {
                bounds_list.push((*id, min, max));
            }
        }
    }

    if bounds_list.is_empty() {
        return;
    }

    // Compute union bounds
    let mut union_min = bounds_list[0].1;
    let mut union_max = bounds_list[0].2;
    for &(_, min, max) in &bounds_list[1..] {
        union_min = union_min.min(min);
        union_max = union_max.max(max);
    }

    ui.horizontal(|ui| {
        // Alignment row
        if ui.button("⇦ Left").on_hover_text("Align Left").clicked() {
            let mut translations = Vec::new();
            for &(id, min, _) in &bounds_list {
                translations.push((id, DVec2::new(union_min.x - min.x, 0.0)));
            }
            let cmd = AlignDistributeCmd::new(translations, "Align Left");
            app.commands.push(Box::new(cmd), &mut app.document);
            app.dirty = true;
        }
        if ui.button("⬈ Center H").on_hover_text("Align Horizontal Center").clicked() {
            let union_center_x = (union_min.x + union_max.x) / 2.0;
            let mut translations = Vec::new();
            for &(id, min, max) in &bounds_list {
                let center_x = (min.x + max.x) / 2.0;
                translations.push((id, DVec2::new(union_center_x - center_x, 0.0)));
            }
            let cmd = AlignDistributeCmd::new(translations, "Align Center X");
            app.commands.push(Box::new(cmd), &mut app.document);
            app.dirty = true;
        }
        if ui.button("⇨ Right").on_hover_text("Align Right").clicked() {
            let mut translations = Vec::new();
            for &(id, _, max) in &bounds_list {
                translations.push((id, DVec2::new(union_max.x - max.x, 0.0)));
            }
            let cmd = AlignDistributeCmd::new(translations, "Align Right");
            app.commands.push(Box::new(cmd), &mut app.document);
            app.dirty = true;
        }
    });

    ui.add_space(2.0);

    ui.horizontal(|ui| {
        if ui.button("⇧ Top").on_hover_text("Align Top").clicked() {
            let mut translations = Vec::new();
            for &(id, min, _) in &bounds_list {
                translations.push((id, DVec2::new(0.0, union_min.y - min.y)));
            }
            let cmd = AlignDistributeCmd::new(translations, "Align Top");
            app.commands.push(Box::new(cmd), &mut app.document);
            app.dirty = true;
        }
        if ui.button("⬌ Center V").on_hover_text("Align Vertical Center").clicked() {
            let union_center_y = (union_min.y + union_max.y) / 2.0;
            let mut translations = Vec::new();
            for &(id, min, max) in &bounds_list {
                let center_y = (min.y + max.y) / 2.0;
                translations.push((id, DVec2::new(0.0, union_center_y - center_y)));
            }
            let cmd = AlignDistributeCmd::new(translations, "Align Center Y");
            app.commands.push(Box::new(cmd), &mut app.document);
            app.dirty = true;
        }
        if ui.button("⇩ Bottom").on_hover_text("Align Bottom").clicked() {
            let mut translations = Vec::new();
            for &(id, _, max) in &bounds_list {
                translations.push((id, DVec2::new(0.0, union_max.y - max.y)));
            }
            let cmd = AlignDistributeCmd::new(translations, "Align Bottom");
            app.commands.push(Box::new(cmd), &mut app.document);
            app.dirty = true;
        }
    });

    // Distribution (needs at least 3 elements)
    if selection_count >= 3 {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button("⬌ Distribute H").on_hover_text("Distribute Horizontally").clicked() {
                let mut sorted = bounds_list.clone();
                sorted.sort_by(|a, b| {
                    let cx_a = (a.1.x + a.2.x) / 2.0;
                    let cx_b = (b.1.x + b.2.x) / 2.0;
                    cx_a.partial_cmp(&cx_b).unwrap_or(std::cmp::Ordering::Equal)
                });

                let n = sorted.len();
                let c_first = (sorted[0].1.x + sorted[0].2.x) / 2.0;
                let c_last = (sorted[n - 1].1.x + sorted[n - 1].2.x) / 2.0;

                let mut translations = Vec::new();
                for (i, &(id, min, max)) in sorted.iter().enumerate() {
                    let cx = (min.x + max.x) / 2.0;
                    let target_cx = if n > 1 {
                        c_first + i as f64 * (c_last - c_first) / (n - 1) as f64
                    } else {
                        cx
                    };
                    translations.push((id, DVec2::new(target_cx - cx, 0.0)));
                }

                let cmd = AlignDistributeCmd::new(translations, "Distribute Horizontally");
                app.commands.push(Box::new(cmd), &mut app.document);
                app.dirty = true;
            }

            if ui.button("⬍ Distribute V").on_hover_text("Distribute Vertically").clicked() {
                let mut sorted = bounds_list.clone();
                sorted.sort_by(|a, b| {
                    let cy_a = (a.1.y + a.2.y) / 2.0;
                    let cy_b = (b.1.y + b.2.y) / 2.0;
                    cy_a.partial_cmp(&cy_b).unwrap_or(std::cmp::Ordering::Equal)
                });

                let n = sorted.len();
                let c_first = (sorted[0].1.y + sorted[0].2.y) / 2.0;
                let c_last = (sorted[n - 1].1.y + sorted[n - 1].2.y) / 2.0;

                let mut translations = Vec::new();
                for (i, &(id, min, max)) in sorted.iter().enumerate() {
                    let cy = (min.y + max.y) / 2.0;
                    let target_cy = if n > 1 {
                        c_first + i as f64 * (c_last - c_first) / (n - 1) as f64
                    } else {
                        cy
                    };
                    translations.push((id, DVec2::new(0.0, target_cy - cy)));
                }

                let cmd = AlignDistributeCmd::new(translations, "Distribute Vertically");
                app.commands.push(Box::new(cmd), &mut app.document);
                app.dirty = true;
            }
        });
    }

    ui.separator();
}

fn edit_gradient_stops(stops: &mut Vec<illuminator_core::style::GradientStop>, ui: &mut egui::Ui, dirty: &mut bool) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("  Stops").weak());
        if ui.button("+ Add").clicked() {
            let last_offset = stops.last().map(|s| s.offset).unwrap_or(0.0);
            let new_offset = (last_offset + 0.1).clamp(0.0, 1.0);
            stops.push(illuminator_core::style::GradientStop {
                offset: new_offset,
                color: Color::WHITE,
            });
            *dirty = true;
        }
    });
    
    let mut to_remove = None;
    stops.sort_by(|a, b| a.offset.partial_cmp(&b.offset).unwrap_or(std::cmp::Ordering::Equal));

    for (idx, stop) in stops.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            // Color picker
            let mut rgba = stop.color.to_array();
            if ui.color_edit_button_rgba_unmultiplied(&mut rgba).changed() {
                stop.color = Color::from_array(rgba);
                *dirty = true;
            }
            // Offset slider
            ui.add(egui::Slider::new(&mut stop.offset, 0.0..=1.0));
            if ui.button("🗑").clicked() {
                to_remove = Some(idx);
            }
        });
    }

    if let Some(idx) = to_remove {
        if stops.len() > 1 {
            stops.remove(idx);
            *dirty = true;
        }
    }
}

fn scale_node_relative_to(node: &mut illuminator_core::doc::Node, anchor: glam::DVec2, scale: glam::DVec2) {
    match &mut node.kind {
        illuminator_core::doc::NodeKind::Path(p) => {
            for a in &mut p.path.anchors {
                a.pos.x = anchor.x + (a.pos.x - anchor.x) * scale.x;
                a.pos.y = anchor.y + (a.pos.y - anchor.y) * scale.y;
                a.in_handle.x *= scale.x;
                a.in_handle.y *= scale.y;
                a.out_handle.x *= scale.x;
                a.out_handle.y *= scale.y;
            }
        }
        illuminator_core::doc::NodeKind::Image(i) => {
            i.position.x = anchor.x + (i.position.x - anchor.x) * scale.x;
            i.position.y = anchor.y + (i.position.y - anchor.y) * scale.y;
            i.size.x *= scale.x;
            i.size.y *= scale.y;
        }
        illuminator_core::doc::NodeKind::Text(t) => {
            t.position.x = anchor.x + (t.position.x - anchor.x) * scale.x;
            t.position.y = anchor.y + (t.position.y - anchor.y) * scale.y;
            if let Some(w) = &mut t.width {
                *w *= scale.x;
            }
        }
    }
}
