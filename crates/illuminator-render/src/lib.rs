//! Renders an Illuminator [`Document`] into an egui [`Painter`].
//!
//! v1 uses egui's `epaint` for fills/strokes. When we need real text shaping,
//! complex paints, or proper non-convex tessellation, we'll swap to skia-safe
//! behind this same `render_document` entry point.

use std::collections::HashMap;

use egui::epaint::{PathShape, PathStroke};
use egui::{Color32, ColorImage, Painter, Pos2, Rect, Shape, Stroke as EguiStroke, TextureHandle};
use glam::DVec2;

use illuminator_core::doc::{Document, LayerKind, NodeKind};
use illuminator_core::image::ImageNode;
use illuminator_core::path::Path as IPath;
use illuminator_core::style::{BlendMode, Color as IColor, Paint, Style};
use illuminator_core::text::{FontFamily, TextNode};
use illuminator_core::transform::ViewTransform;

/// Cache of decoded image textures, keyed by [`illuminator_core::ImageData::hash`].
pub type TextureCache = HashMap<u64, TextureHandle>;

pub struct RenderCtx<'a> {
    pub painter: &'a Painter,
    pub view: &'a ViewTransform,
    pub viewport_center: DVec2,
    pub egui_ctx: &'a egui::Context,
    pub textures: &'a mut TextureCache,
}

impl<'a> RenderCtx<'a> {
    #[inline]
    pub fn world_to_pos2(&self, world: DVec2) -> Pos2 {
        let s = self.view.world_to_screen(world, self.viewport_center);
        Pos2::new(s.x as f32, s.y as f32)
    }
}

pub fn render_document(doc: &Document, ctx: &mut RenderCtx<'_>) {
    draw_artboards(doc, ctx);

    // layers[0] is topmost — paint back-to-front.
    for &layer_id in doc.layers.iter().rev() {
        let Some(layer) = doc.layer_arena.get(layer_id) else { continue };
        if !layer.visible {
            continue;
        }
        let dimmed = matches!(layer.kind, LayerKind::Reference { dimmed: true });
        let layer_alpha = layer.opacity.clamp(0.0, 1.0) * if dimmed { 0.85 } else { 1.0 };
        for &node_id in &layer.nodes {
            let Some(node) = doc.node_arena.get(node_id) else { continue };
            match &node.kind {
                NodeKind::Path(p) => draw_path(&p.path, &p.style, layer_alpha, layer.blend, ctx),
                NodeKind::Image(img) => draw_image(img, layer_alpha, ctx),
                NodeKind::Text(t) => draw_text(t, layer_alpha, ctx),
            }
        }
    }
}

fn draw_artboards(doc: &Document, ctx: &RenderCtx<'_>) {
    for artboard in &doc.artboards {
        let s_min = ctx.view.world_to_screen(artboard.min, ctx.viewport_center);
        let s_max = ctx.view.world_to_screen(artboard.max, ctx.viewport_center);
        let rect = Rect::from_two_pos(
            Pos2::new(s_min.x as f32, s_min.y as f32),
            Pos2::new(s_max.x as f32, s_max.y as f32),
        );

        // Render card drop shadow and clean border
        ctx.painter.rect_filled(rect.translate(egui::vec2(2.0, 2.0)), 2.0, Color32::from_black_alpha(40));
        ctx.painter.rect_filled(rect, 2.0, Color32::WHITE);
        ctx.painter.rect_stroke(rect, 2.0, EguiStroke::new(1.0, Color32::from_gray(120)));

        // Render artboard name above top-left
        let label_pos = Pos2::new(rect.min.x, rect.min.y - 14.0);
        ctx.painter.text(
            label_pos,
            egui::Align2::LEFT_TOP,
            &artboard.name,
            egui::FontId::proportional(11.0),
            Color32::from_gray(160),
        );
    }
}

// --- Path drawing (unchanged from Phase 1) ---

fn draw_path(
    path: &IPath,
    style: &Style,
    layer_alpha: f32,
    _blend: BlendMode,
    ctx: &RenderCtx<'_>,
) {
    let mut polyline: Vec<Pos2> = Vec::with_capacity(path.anchors.len() * 4);
    flatten_path(path, ctx, &mut polyline);
    if polyline.len() < 2 {
        return;
    }

    if let Some(fill_paint) = &style.fill {
        match fill_paint {
            Paint::Solid(c) => {
                let fill_color = icolor_to_color32(*c, style.opacity * layer_alpha);
                ctx.painter.add(Shape::Path(PathShape {
                    points: polyline.clone(),
                    closed: path.closed,
                    fill: fill_color,
                    stroke: PathStroke::NONE,
                }));
            }
            Paint::LinearGradient { .. } | Paint::RadialGradient { .. } => {
                draw_gradient_fill(&polyline, fill_paint, style.opacity * layer_alpha, ctx);
            }
        }
    }

    if let Some(s) = &style.stroke {
        let width = (s.width * ctx.view.zoom).max(0.5) as f32;
        let color = paint_to_color32(&s.paint, style.opacity * layer_alpha);
        ctx.painter.add(Shape::Path(PathShape {
            points: polyline,
            closed: path.closed,
            fill: Color32::TRANSPARENT,
            stroke: PathStroke::new(width, color),
        }));
    }
}

fn flatten_path(path: &IPath, ctx: &RenderCtx<'_>, out: &mut Vec<Pos2>) {
    out.clear();
    if path.anchors.is_empty() {
        return;
    }
    let n = path.anchors.len();
    let segments = if path.closed { n } else { n.saturating_sub(1) };
    out.push(ctx.world_to_pos2(path.anchors[0].pos));
    for i in 0..segments {
        let a = &path.anchors[i];
        let b = &path.anchors[(i + 1) % n];
        let p0 = a.pos;
        let p1 = a.pos + a.out_handle;
        let p2 = b.pos + b.in_handle;
        let p3 = b.pos;
        let line_only =
            a.out_handle.length_squared() < 1e-12 && b.in_handle.length_squared() < 1e-12;
        if !line_only {
            flatten_cubic(p0, p1, p2, p3, ctx, out, 0);
        }
        out.push(ctx.world_to_pos2(p3));
    }
}

fn flatten_cubic(
    p0: DVec2,
    p1: DVec2,
    p2: DVec2,
    p3: DVec2,
    ctx: &RenderCtx<'_>,
    out: &mut Vec<Pos2>,
    depth: u32,
) {
    if depth > 10 {
        return;
    }
    let chord = p3 - p0;
    let chord_len = chord.length();
    let tol_world = 0.5 / ctx.view.zoom;
    let dev = if chord_len < 1e-12 {
        (p1 - p0).length().max((p2 - p0).length())
    } else {
        let nrm = DVec2::new(-chord.y, chord.x) / chord_len;
        ((p1 - p0).dot(nrm)).abs().max(((p2 - p0).dot(nrm)).abs())
    };
    if dev <= tol_world {
        return;
    }
    let q0 = 0.5 * (p0 + p1);
    let q1 = 0.5 * (p1 + p2);
    let q2 = 0.5 * (p2 + p3);
    let r0 = 0.5 * (q0 + q1);
    let r1 = 0.5 * (q1 + q2);
    let s = 0.5 * (r0 + r1);
    flatten_cubic(p0, q0, r0, s, ctx, out, depth + 1);
    out.push(ctx.world_to_pos2(s));
    flatten_cubic(s, r1, q2, p3, ctx, out, depth + 1);
}

// --- Image drawing ---

fn draw_image(node: &ImageNode, layer_alpha: f32, ctx: &mut RenderCtx<'_>) {
    let texture = match ctx.textures.get(&node.image.hash) {
        Some(t) => t.clone(),
        None => match upload_texture(node, ctx.egui_ctx) {
            Some(t) => {
                ctx.textures.insert(node.image.hash, t.clone());
                t
            }
            None => return,
        },
    };

    let (mn, mx) = node.bounds();
    let s_min = ctx.view.world_to_screen(mn, ctx.viewport_center);
    let s_max = ctx.view.world_to_screen(mx, ctx.viewport_center);
    let rect = Rect::from_two_pos(
        Pos2::new(s_min.x as f32, s_min.y as f32),
        Pos2::new(s_max.x as f32, s_max.y as f32),
    );
    let alpha = (node.style.opacity * layer_alpha).clamp(0.0, 1.0);
    let tint = Color32::from_white_alpha((alpha * 255.0) as u8);
    let uv = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0));
    ctx.painter.image(texture.id(), rect, uv, tint);
}

// --- Text drawing ---

fn draw_text(node: &TextNode, layer_alpha: f32, ctx: &RenderCtx<'_>) {
    if node.text.is_empty() {
        return;
    }
    let pos = ctx.view.world_to_screen(node.position, ctx.viewport_center);
    let size_px = (node.font_size * ctx.view.zoom).max(1.0) as f32;
    let font_id = match node.family {
        FontFamily::Proportional => egui::FontId::proportional(size_px),
        FontFamily::Monospace => egui::FontId::monospace(size_px),
    };
    let color = icolor_to_color32(node.color, node.style.opacity * layer_alpha);

    if let Some(w) = node.width {
        let mut job = egui::text::LayoutJob::simple(
            node.text.clone(),
            font_id,
            color,
            (w * ctx.view.zoom) as f32,
        );
        job.wrap.break_anywhere = false;
        let galley = ctx.egui_ctx.fonts(|f| f.layout_job(job));
        ctx.painter.galley(
            Pos2::new(pos.x as f32, pos.y as f32),
            galley,
            color,
        );
    } else {
        ctx.painter.text(
            Pos2::new(pos.x as f32, pos.y as f32),
            egui::Align2::LEFT_TOP,
            &node.text,
            font_id,
            color,
        );
    }
}

fn upload_texture(node: &ImageNode, egui_ctx: &egui::Context) -> Option<TextureHandle> {
    let decoded = match image::load_from_memory(&node.image.bytes) {
        Ok(img) => img.to_rgba8(),
        Err(e) => {
            tracing::warn!("image decode failed: {e}");
            return None;
        }
    };
    let (w, h) = (decoded.width() as usize, decoded.height() as usize);
    let color = ColorImage::from_rgba_unmultiplied([w, h], decoded.as_raw());
    let name = format!("ilm_img_{:016x}", node.image.hash);
    let options = egui::TextureOptions {
        magnification: egui::TextureFilter::Linear,
        minification: egui::TextureFilter::Linear,
        wrap_mode: egui::TextureWrapMode::ClampToEdge,
        mipmap_mode: Some(egui::TextureFilter::Linear),
    };
    Some(egui_ctx.load_texture(name, color, options))
}

// --- Color helpers ---

fn paint_to_color32(paint: &Paint, alpha: f32) -> Color32 {
    let c = match paint {
        Paint::Solid(c) => *c,
        Paint::LinearGradient { stops, .. } | Paint::RadialGradient { stops, .. } => {
            stops.first().map(|s| s.color).unwrap_or(IColor::rgba(0.0, 0.0, 0.0, 0.0))
        }
    };
    icolor_to_color32(c, alpha)
}

fn draw_gradient_fill(
    polyline: &[Pos2],
    paint: &Paint,
    opacity: f32,
    ctx: &RenderCtx<'_>,
) {
    let n = polyline.len();
    if n < 3 {
        return;
    }

    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    for pt in polyline {
        sum_x += pt.x;
        sum_y += pt.y;
    }
    let center = Pos2::new(sum_x / n as f32, sum_y / n as f32);

    let mut mesh = egui::epaint::Mesh::default();

    let screen_to_world = |pt: Pos2| -> DVec2 {
        let sc = DVec2::new(pt.x as f64, pt.y as f64);
        (sc - ctx.viewport_center) / ctx.view.zoom + ctx.view.pan
    };

    let get_color = |pt: Pos2| -> Color32 {
        let w_pos = screen_to_world(pt);
        match paint {
            Paint::Solid(c) => icolor_to_color32(*c, opacity),
            Paint::LinearGradient { start, end, stops } => {
                let v = *end - *start;
                let len_sq = v.length_squared();
                let t = if len_sq < 1e-9 {
                    0.0
                } else {
                    (w_pos - *start).dot(v) / len_sq
                };
                sample_gradient(stops, t as f32, opacity)
            }
            Paint::RadialGradient { center: g_center, radius, stops } => {
                let d = (w_pos - *g_center).length();
                let t = if *radius < 1e-9 {
                    0.0
                } else {
                    d / *radius
                };
                sample_gradient(stops, t as f32, opacity)
            }
        }
    };

    let center_idx = mesh.vertices.len() as u32;
    mesh.vertices.push(egui::epaint::Vertex {
        pos: center,
        uv: egui::epaint::WHITE_UV,
        color: get_color(center),
    });

    let perimeter_start = mesh.vertices.len() as u32;
    for pt in polyline {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: *pt,
            uv: egui::epaint::WHITE_UV,
            color: get_color(*pt),
        });
    }

    for i in 0..n {
        let idx0 = center_idx;
        let idx1 = perimeter_start + i as u32;
        let idx2 = perimeter_start + ((i + 1) % n) as u32;
        mesh.indices.push(idx0);
        mesh.indices.push(idx1);
        mesh.indices.push(idx2);
    }

    ctx.painter.add(Shape::mesh(mesh));
}

fn sample_gradient(stops: &[illuminator_core::style::GradientStop], mut t: f32, opacity: f32) -> Color32 {
    t = t.clamp(0.0, 1.0);
    if stops.is_empty() {
        return Color32::TRANSPARENT;
    }
    if stops.len() == 1 {
        return icolor_to_color32(stops[0].color, opacity);
    }
    let mut stops = stops.to_vec();
    stops.sort_by(|a, b| a.offset.partial_cmp(&b.offset).unwrap());

    if t <= stops[0].offset {
        return icolor_to_color32(stops[0].color, opacity);
    }
    if t >= stops.last().unwrap().offset {
        return icolor_to_color32(stops.last().unwrap().color, opacity);
    }

    for window in stops.windows(2) {
        let s0 = &window[0];
        let s1 = &window[1];
        if t >= s0.offset && t <= s1.offset {
            let lerp = (t - s0.offset) / (s1.offset - s0.offset);
            let r = s0.color.r + lerp * (s1.color.r - s0.color.r);
            let g = s0.color.g + lerp * (s1.color.g - s0.color.g);
            let b = s0.color.b + lerp * (s1.color.b - s0.color.b);
            let a = s0.color.a + lerp * (s1.color.a - s0.color.a);
            return icolor_to_color32(IColor::rgba(r, g, b, a), opacity);
        }
    }
    Color32::TRANSPARENT
}

fn icolor_to_color32(c: IColor, alpha: f32) -> Color32 {
    let a = (c.a * alpha).clamp(0.0, 1.0);
    Color32::from_rgba_unmultiplied(
        (c.r.clamp(0.0, 1.0) * 255.0) as u8,
        (c.g.clamp(0.0, 1.0) * 255.0) as u8,
        (c.b.clamp(0.0, 1.0) * 255.0) as u8,
        (a * 255.0) as u8,
    )
}

/// Draw a checkerboard "canvas" backdrop so the user can tell when they're
/// over empty world vs. inside the viewport.
pub fn paint_canvas_backdrop(painter: &Painter, rect: egui::Rect) {
    painter.rect_filled(rect, 0.0, Color32::from_gray(34));
}

/// Convenience for tool overlays.
pub fn pos2(v: DVec2) -> Pos2 { Pos2::new(v.x as f32, v.y as f32) }

#[allow(dead_code)]
fn _unused_egui_stroke() -> EguiStroke { EguiStroke::NONE }
