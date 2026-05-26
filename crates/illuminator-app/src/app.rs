use std::collections::HashSet;
use std::path::PathBuf;

use eframe::CreationContext;
use illuminator_core::command::CommandStack;
use illuminator_core::doc::{Document, LayerId, NodeId};
use illuminator_core::style::Color;
use illuminator_core::transform::ViewTransform;
use illuminator_render::TextureCache;

use crate::interaction::Interaction;
use crate::pen::PenState;
use crate::snap::SnapSettings;
use crate::tools::ToolKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymmetryMode {
    None,
    Horizontal,
    Vertical,
    Dual,
}

pub struct IlluminatorApp {
    pub document: Document,
    pub view: ViewTransform,
    pub commands: CommandStack,
    pub selection: HashSet<NodeId>,
    pub active_layer: LayerId,
    pub active_tool: ToolKind,
    pub current_path: Option<PathBuf>,
    pub interaction: Interaction,
    /// In-progress Pen-tool path. `Some` from the first anchor placement until
    /// the user closes, finishes, or cancels.
    pub pen_state: Option<PenState>,
    pub default_fill: Option<Color>,
    pub default_stroke: Option<Color>,
    pub default_stroke_width: f64,
    pub dirty: bool,
    /// Decoded image textures, keyed by image-content hash. Lives outside the
    /// document because `egui::TextureHandle` is not serializable.
    pub textures: TextureCache,
    /// Last canvas viewport center in screen pixels. Stored so File→Place and
    /// drag-drop can place the image at the cursor / current view.
    pub last_viewport_center: glam::DVec2,
    /// Last cursor world-position over the canvas, for image drop placement.
    pub last_cursor_world: Option<glam::DVec2>,
    pub snap: SnapSettings,
    /// World position of the active snap target this frame — drawn as a hint
    /// marker by the canvas overlay during a drag.
    pub last_snap_hint: Option<glam::DVec2>,
    /// NodeId of the TextNode currently being edited by the Text tool, if any.
    pub editing_text: Option<NodeId>,
    pub last_autosave: Option<std::time::Instant>,
    pub symmetry: SymmetryMode,
    pub has_recovery_file: bool,
}

impl IlluminatorApp {
    pub fn new(cc: &CreationContext<'_>) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());

        let document = Document::default();
        let active_layer = *document
            .layers
            .first()
            .expect("default document always has one layer");

        Self {
            document,
            view: ViewTransform::default(),
            commands: CommandStack::default(),
            selection: HashSet::new(),
            active_layer,
            active_tool: ToolKind::Select,
            current_path: None,
            interaction: Interaction::None,
            pen_state: None,
            default_fill: Some(Color::rgb(0.85, 0.85, 0.85)),
            default_stroke: Some(Color::BLACK),
            default_stroke_width: 1.0,
            dirty: false,
            textures: TextureCache::default(),
            last_viewport_center: glam::DVec2::ZERO,
            last_cursor_world: None,
            snap: SnapSettings::default(),
            last_snap_hint: None,
            editing_text: None,
            last_autosave: None,
            has_recovery_file: std::env::temp_dir().join("illuminator_autosave.ilm").exists(),
            symmetry: SymmetryMode::None,
        }
    }

    pub fn window_title(&self) -> String {
        let name = self
            .current_path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or(&self.document.title);
        let star = if self.dirty { "*" } else { "" };
        format!("{star}{name} — Illuminator")
    }

    /// Replace the document (after open) and reset transient state.
    pub fn replace_document(&mut self, doc: Document, path: Option<PathBuf>) {
        self.document = doc;
        self.active_layer = self
            .document
            .layers
            .first()
            .copied()
            .unwrap_or_else(|| {
                let l = illuminator_core::doc::Layer::new("Layer 1");
                self.document.add_layer(l)
            });
        self.selection.clear();
        self.commands.clear();
        self.interaction = Interaction::None;
        self.pen_state = None;
        self.editing_text = None;
        self.textures.clear();
        self.view = ViewTransform::default();
        self.current_path = path;
        self.dirty = false;
    }
}

impl eframe::App for IlluminatorApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Keep the window title in sync with the current document.
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));

        // Recovery Dialog Modal
        if self.has_recovery_file {
            let mut open = true;
            egui::Window::new("Recovery Available")
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label(egui::RichText::new("⚠️ Unsaved Document Found").heading().strong());
                        ui.add_space(8.0);
                        ui.label("Illuminator closed unexpectedly during your last session. An autosaved recovery file is available.");
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            if ui.button(egui::RichText::new("Restore Session").strong()).clicked() {
                                let recovery_path = std::env::temp_dir().join("illuminator_autosave.ilm");
                                if let Ok(doc) = illuminator_core::io::load_document(&recovery_path) {
                                    self.replace_document(doc, None);
                                    self.dirty = true;
                                    tracing::info!("Recovered document from autosave");
                                } else {
                                    tracing::error!("Failed to load autosave file");
                                }
                                let _ = std::fs::remove_file(&recovery_path);
                                self.has_recovery_file = false;
                            }
                            if ui.button("Discard").clicked() {
                                let recovery_path = std::env::temp_dir().join("illuminator_autosave.ilm");
                                let _ = std::fs::remove_file(&recovery_path);
                                self.has_recovery_file = false;
                            }
                        });
                    });
                });
        }

        // Background Autosave
        if self.dirty {
            let now = std::time::Instant::now();
            let elapsed = self.last_autosave.map(|t| now.duration_since(t).as_secs()).unwrap_or(999);
            if elapsed >= 10 {
                let recovery_path = std::env::temp_dir().join("illuminator_autosave.ilm");
                if let Ok(()) = illuminator_core::io::save_document(&self.document, &recovery_path) {
                    tracing::debug!("Autosaved session to temporary recovery file");
                }
                self.last_autosave = Some(now);
            }
        } else {
            let recovery_path = std::env::temp_dir().join("illuminator_autosave.ilm");
            if recovery_path.exists() {
                let _ = std::fs::remove_file(&recovery_path);
            }
            self.last_autosave = None;
        }

        crate::menu::show(self, ctx, frame);
        crate::panels::tools_palette::show(self, ctx);
        crate::panels::right::show(self, ctx);
        crate::panels::status::show(self, ctx);
        crate::canvas::show(self, ctx);
    }
}

pub fn mirror_node(node: &illuminator_core::doc::Node, mode: SymmetryMode) -> Vec<illuminator_core::doc::Node> {
    let mut copies = Vec::new();
    match mode {
        SymmetryMode::None => {}
        SymmetryMode::Horizontal => {
            let mut copy = node.clone();
            reflect_node(&mut copy, true, false);
            copy.name = format!("{} (H-Mirrored)", node.name);
            copies.push(copy);
        }
        SymmetryMode::Vertical => {
            let mut copy = node.clone();
            reflect_node(&mut copy, false, true);
            copy.name = format!("{} (V-Mirrored)", node.name);
            copies.push(copy);
        }
        SymmetryMode::Dual => {
            // Horizontal
            let mut copy_h = node.clone();
            reflect_node(&mut copy_h, true, false);
            copy_h.name = format!("{} (H-Mirrored)", node.name);
            copies.push(copy_h);

            // Vertical
            let mut copy_v = node.clone();
            reflect_node(&mut copy_v, false, true);
            copy_v.name = format!("{} (V-Mirrored)", node.name);
            copies.push(copy_v);

            // Both
            let mut copy_hv = node.clone();
            reflect_node(&mut copy_hv, true, true);
            copy_hv.name = format!("{} (HV-Mirrored)", node.name);
            copies.push(copy_hv);
        }
    }
    copies
}

fn reflect_node(node: &mut illuminator_core::doc::Node, x: bool, y: bool) {
    match &mut node.kind {
        illuminator_core::doc::NodeKind::Path(p) => {
            for a in &mut p.path.anchors {
                if x {
                    a.pos.x = -a.pos.x;
                    a.in_handle.x = -a.in_handle.x;
                    a.out_handle.x = -a.out_handle.x;
                }
                if y {
                    a.pos.y = -a.pos.y;
                    a.in_handle.y = -a.in_handle.y;
                    a.out_handle.y = -a.out_handle.y;
                }
            }
        }
        illuminator_core::doc::NodeKind::Image(i) => {
            if x {
                i.position.x = -i.position.x - i.size.x;
            }
            if y {
                i.position.y = -i.position.y - i.size.y;
            }
        }
        illuminator_core::doc::NodeKind::Text(t) => {
            if x {
                t.position.x = -t.position.x;
            }
            if y {
                t.position.y = -t.position.y;
            }
        }
    }
}
