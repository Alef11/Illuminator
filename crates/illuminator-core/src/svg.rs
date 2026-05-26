//! SVG import / export.
//!
//! **Export** – walks the [`Document`] and emits SVG 1.1 XML via `xmlwriter`.
//! Reference layers are excluded by default.
//!
//! **Import** – parses an SVG file with `usvg`, converts the resolved tree of
//! cubic paths back into our [`PathNode`] / [`ImageNode`] model. Text in the
//! source SVG is flattened to outlines by `usvg` (matches Illustrator's
//! "Create Outlines" behaviour).

use std::path::Path as FsPath;

use glam::DVec2;

use crate::doc::{Document, NodeKind, Layer, NodeId};
use crate::image::{ImageData, ImageNode};
use crate::io::IoError;
use crate::path::{Anchor, Path as IPath, PathNode};
use crate::style::{Color, LineCap, LineJoin, Paint, Stroke as IStroke, Style};
use crate::text::{FontFamily, TextNode};

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

/// Options controlling SVG export behaviour.
#[derive(Clone, Debug)]
pub struct ExportOptions {
    /// When `false` (default), reference layers are omitted from the SVG.
    pub include_reference_layers: bool,
    /// Pretty-print with indentation.
    pub indent: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            include_reference_layers: false,
            indent: true,
        }
    }
}

/// Write a selection of nodes to an SVG 1.1 file.
pub fn export_svg_selection(
    doc: &Document,
    selection: &std::collections::HashSet<NodeId>,
    path: &FsPath,
    opts: &ExportOptions,
) -> Result<(), IoError> {
    if selection.is_empty() {
        return export_svg(doc, path, opts);
    }

    let mut temp_doc = Document {
        version: doc.version,
        title: format!("{} Selection", doc.title),
        layers: Vec::new(),
        layer_arena: slotmap::SlotMap::with_key(),
        node_arena: slotmap::SlotMap::with_key(),
        artboards: doc.artboards.clone(),
    };

    for &layer_id in &doc.layers {
        let Some(layer) = doc.layer_arena.get(layer_id) else { continue };
        let selected_nodes_in_layer: Vec<NodeId> = layer.nodes.iter()
            .copied()
            .filter(|nid| selection.contains(nid))
            .collect();

        if !selected_nodes_in_layer.is_empty() {
            let mut temp_layer = Layer::new(&layer.name);
            temp_layer.visible = layer.visible;
            temp_layer.locked = layer.locked;
            temp_layer.opacity = layer.opacity;
            temp_layer.blend = layer.blend;
            temp_layer.kind = layer.kind.clone();

            for &nid in &selected_nodes_in_layer {
                if let Some(node) = doc.node_arena.get(nid) {
                    let temp_nid = temp_doc.node_arena.insert(node.clone());
                    temp_layer.nodes.push(temp_nid);
                }
            }

            let temp_layer_id = temp_doc.layer_arena.insert(temp_layer);
            temp_doc.layers.push(temp_layer_id);
        }
    }

    export_svg(&temp_doc, path, opts)
}

/// Write the document to an SVG 1.1 file.
pub fn export_svg(doc: &Document, path: &FsPath, opts: &ExportOptions) -> Result<(), IoError> {
    let xml_opts = xmlwriter::Options {
        use_single_quote: false,
        indent: if opts.indent {
            xmlwriter::Indent::Spaces(2)
        } else {
            xmlwriter::Indent::None
        },
        attributes_indent: xmlwriter::Indent::None,
    };
    let mut w = xmlwriter::XmlWriter::new(xml_opts);

    // Compute a viewBox from the union of all visible node bounds.
    let (vb_min, vb_max) = document_bounds(doc, opts.include_reference_layers);
    let vb_w = (vb_max.x - vb_min.x).max(1.0);
    let vb_h = (vb_max.y - vb_min.y).max(1.0);

    w.start_element("svg");
    w.write_attribute("xmlns", "http://www.w3.org/2000/svg");
    w.write_attribute("xmlns:xlink", "http://www.w3.org/1999/xlink");
    w.write_attribute("version", "1.1");
    w.write_attribute(
        "viewBox",
        &format!("{} {} {} {}", vb_min.x, vb_min.y, vb_w, vb_h),
    );
    w.write_attribute("width", &format!("{vb_w}"));
    w.write_attribute("height", &format!("{vb_h}"));

    // layers[0] is topmost. SVG paints in document order (first = bottom), so
    // we iterate in reverse so topmost layer paints last.
    for &layer_id in doc.layers.iter().rev() {
        let Some(layer) = doc.layer_arena.get(layer_id) else {
            continue;
        };
        if !layer.visible {
            continue;
        }
        if !opts.include_reference_layers && layer.kind.is_reference() {
            continue;
        }

        w.start_element("g");
        w.write_attribute("id", &sanitise_id(&layer.name));
        if layer.opacity < 1.0 {
            w.write_attribute("opacity", &format!("{:.3}", layer.opacity));
        }

        for &node_id in &layer.nodes {
            let Some(node) = doc.node_arena.get(node_id) else {
                continue;
            };
            match &node.kind {
                NodeKind::Path(p) => write_path(&mut w, p),
                NodeKind::Text(t) => write_text(&mut w, t),
                NodeKind::Image(img) => write_image(&mut w, img),
            }
        }

        w.end_element(); // </g>
    }

    w.end_element(); // </svg>

    let xml = w.end_document();
    let header = r#"<?xml version="1.0" encoding="UTF-8"?>"#;
    let full = format!("{header}\n{xml}");
    std::fs::write(path, full)?;
    tracing::info!(?path, "exported SVG");
    Ok(())
}

fn write_path(w: &mut xmlwriter::XmlWriter, pn: &PathNode) {
    let path = &pn.path;
    if path.anchors.is_empty() {
        return;
    }
    w.start_element("path");
    w.write_attribute("d", &path_to_d(path));
    write_style_attrs(w, &pn.style);
    w.end_element();
}

fn write_text(w: &mut xmlwriter::XmlWriter, t: &TextNode) {
    w.start_element("text");
    w.write_attribute("x", &format!("{:.4}", t.position.x));
    w.write_attribute("y", &format!("{:.4}", t.position.y + t.font_size));
    w.write_attribute("font-size", &format!("{:.2}", t.font_size));
    let family = match t.family {
        FontFamily::Proportional => "sans-serif",
        FontFamily::Monospace => "monospace",
    };
    w.write_attribute("font-family", family);
    w.write_attribute("fill", &color_to_hex(t.color));
    if t.color.a < 1.0 {
        w.write_attribute("fill-opacity", &format!("{:.3}", t.color.a));
    }
    // Multi-line text: split into <tspan> elements with dy offsets.
    let lines: Vec<&str> = t.text.split('\n').collect();
    if lines.len() <= 1 {
        w.write_text(&t.text);
    } else {
        for (i, line) in lines.iter().enumerate() {
            w.start_element("tspan");
            w.write_attribute("x", &format!("{:.4}", t.position.x));
            if i > 0 {
                w.write_attribute("dy", &format!("{:.2}", t.font_size * 1.2));
            }
            w.write_text(line);
            w.end_element();
        }
    }
    w.end_element();
}

fn write_image(w: &mut xmlwriter::XmlWriter, img: &ImageNode) {
    use base64::{engine::general_purpose, Engine as _};
    w.start_element("image");
    w.write_attribute("x", &format!("{:.4}", img.position.x));
    w.write_attribute("y", &format!("{:.4}", img.position.y));
    w.write_attribute("width", &format!("{:.4}", img.size.x));
    w.write_attribute("height", &format!("{:.4}", img.size.y));
    // Guess mime type from first bytes.
    let mime = if img.image.bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "image/png"
    } else if img.image.bytes.starts_with(&[0xFF, 0xD8]) {
        "image/jpeg"
    } else {
        "image/png" // fallback
    };
    let b64 = general_purpose::STANDARD.encode(&img.image.bytes);
    let href = format!("data:{mime};base64,{b64}");
    w.write_attribute("xlink:href", &href);
    if img.style.opacity < 1.0 {
        w.write_attribute("opacity", &format!("{:.3}", img.style.opacity));
    }
    w.end_element();
}

fn write_style_attrs(w: &mut xmlwriter::XmlWriter, style: &Style) {
    match &style.fill {
        Some(Paint::Solid(c)) => {
            w.write_attribute("fill", &color_to_hex(*c));
            if c.a < 1.0 {
                w.write_attribute("fill-opacity", &format!("{:.3}", c.a));
            }
        }
        Some(Paint::LinearGradient { stops, .. }) | Some(Paint::RadialGradient { stops, .. }) => {
            let c = stops.first().map(|s| s.color).unwrap_or(Color::BLACK);
            w.write_attribute("fill", &color_to_hex(c));
            if c.a < 1.0 {
                w.write_attribute("fill-opacity", &format!("{:.3}", c.a));
            }
        }
        None => {
            w.write_attribute("fill", "none");
        }
    }
    match &style.stroke {
        Some(s) => {
            let c = match &s.paint {
                Paint::Solid(c) => *c,
                Paint::LinearGradient { stops, .. } | Paint::RadialGradient { stops, .. } => {
                    stops.first().map(|st| st.color).unwrap_or(Color::BLACK)
                }
            };
            w.write_attribute("stroke", &color_to_hex(c));
            if c.a < 1.0 {
                w.write_attribute("stroke-opacity", &format!("{:.3}", c.a));
            }
            w.write_attribute("stroke-width", &format!("{:.4}", s.width));
            match s.cap {
                LineCap::Round => { w.write_attribute("stroke-linecap", "round"); }
                LineCap::Square => { w.write_attribute("stroke-linecap", "square"); }
                LineCap::Butt => {} // SVG default
            }
            match s.join {
                LineJoin::Round => { w.write_attribute("stroke-linejoin", "round"); }
                LineJoin::Bevel => { w.write_attribute("stroke-linejoin", "bevel"); }
                LineJoin::Miter => {} // SVG default
            }
            if let Some(dash) = &s.dash {
                let vals: Vec<String> = dash.iter().map(|v| format!("{v}")).collect();
                w.write_attribute("stroke-dasharray", &vals.join(","));
            }
        }
        None => {
            w.write_attribute("stroke", "none");
        }
    }
    if style.opacity < 1.0 {
        w.write_attribute("opacity", &format!("{:.3}", style.opacity));
    }
}

/// Build an SVG path data string from our anchor model.
fn path_to_d(path: &IPath) -> String {
    let mut d = String::with_capacity(path.anchors.len() * 60);
    let n = path.anchors.len();
    if n == 0 {
        return d;
    }
    let segments = if path.closed { n } else { n.saturating_sub(1) };

    // MoveTo the first anchor.
    let first = &path.anchors[0];
    d.push_str(&format!("M{:.4},{:.4}", first.pos.x, first.pos.y));

    for i in 0..segments {
        let a = &path.anchors[i];
        let b = &path.anchors[(i + 1) % n];
        let line_only =
            a.out_handle.length_squared() < 1e-12 && b.in_handle.length_squared() < 1e-12;
        if line_only {
            d.push_str(&format!(" L{:.4},{:.4}", b.pos.x, b.pos.y));
        } else {
            let c1 = a.pos + a.out_handle;
            let c2 = b.pos + b.in_handle;
            d.push_str(&format!(
                " C{:.4},{:.4} {:.4},{:.4} {:.4},{:.4}",
                c1.x, c1.y, c2.x, c2.y, b.pos.x, b.pos.y
            ));
        }
    }
    if path.closed {
        d.push('Z');
    }
    d
}

fn color_to_hex(c: Color) -> String {
    let r = (c.r.clamp(0.0, 1.0) * 255.0) as u8;
    let g = (c.g.clamp(0.0, 1.0) * 255.0) as u8;
    let b = (c.b.clamp(0.0, 1.0) * 255.0) as u8;
    format!("#{r:02x}{g:02x}{b:02x}")
}

fn sanitise_id(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect()
}

fn document_bounds(doc: &Document, include_ref: bool) -> (DVec2, DVec2) {
    let mut min = DVec2::new(f64::MAX, f64::MAX);
    let mut max = DVec2::new(f64::MIN, f64::MIN);
    let mut any = false;
    for &layer_id in &doc.layers {
        let Some(layer) = doc.layer_arena.get(layer_id) else { continue };
        if !layer.visible { continue; }
        if !include_ref && layer.kind.is_reference() { continue; }
        for &nid in &layer.nodes {
            let Some(node) = doc.node_arena.get(nid) else { continue };
            if let Some((mn, mx)) = node.bounds() {
                min = min.min(mn);
                max = max.max(mx);
                any = true;
            }
        }
    }
    if !any {
        return (DVec2::ZERO, DVec2::new(100.0, 100.0));
    }
    (min, max)
}

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

/// Result of importing an SVG file: a collection of path and image nodes ready
/// to be inserted into a layer.
#[derive(Default)]
pub struct SvgImportResult {
    pub paths: Vec<PathNode>,
    pub images: Vec<ImageNode>,
}

/// Parse an SVG file via `usvg` and convert to our native node types.
pub fn import_svg(path: &FsPath) -> Result<SvgImportResult, IoError> {
    let data = std::fs::read(path)?;
    import_svg_from_bytes(&data)
}

/// Parse SVG bytes (useful for tests and drag-drop).
pub fn import_svg_from_bytes(data: &[u8]) -> Result<SvgImportResult, IoError> {
    let opts = usvg::Options::default();
    let tree = usvg::Tree::from_data(data, &opts).map_err(|e| IoError::Svg(e.to_string()))?;

    let mut result = SvgImportResult::default();
    collect_nodes(tree.root(), &mut result);
    tracing::info!(
        paths = result.paths.len(),
        images = result.images.len(),
        "imported SVG"
    );
    Ok(result)
}

fn collect_nodes(group: &usvg::Group, result: &mut SvgImportResult) {
    for child in group.children() {
        match child {
            usvg::Node::Group(ref g) => {
                collect_nodes(g, result);
            }
            usvg::Node::Path(ref p) => {
                if let Some(pn) = convert_path(p) {
                    result.paths.push(pn);
                }
            }
            usvg::Node::Image(ref img) => {
                if let Some(node) = convert_image(img) {
                    result.images.push(node);
                }
            }
            usvg::Node::Text(ref t) => {
                // usvg flattens text to outlined paths. The flattened()
                // method always returns a &Group (may be empty).
                let group = t.flattened();
                collect_nodes(group, result);
            }
        }
    }
}

fn convert_path(p: &usvg::Path) -> Option<PathNode> {
    let data = p.data();
    let mut anchors: Vec<Anchor> = Vec::new();
    let mut closed = false;

    // Track whether we're at the very start of a subpath (after MoveTo).
    // usvg guarantees absolute coords and only MoveTo/LineTo/QuadTo/CubicTo/Close.
    use usvg::tiny_skia_path::PathSegment;
    for seg in data.segments() {
        match seg {
            PathSegment::MoveTo(pt) => {
                anchors.push(Anchor::corner(DVec2::new(pt.x as f64, pt.y as f64)));
            }
            PathSegment::LineTo(pt) => {
                anchors.push(Anchor::corner(DVec2::new(pt.x as f64, pt.y as f64)));
            }
            PathSegment::QuadTo(c, end) => {
                // Elevate quadratic to cubic: cp1 = P0 + 2/3*(C-P0),
                // cp2 = End + 2/3*(C-End).
                let prev_pos = anchors.last().map(|a| a.pos).unwrap_or(DVec2::ZERO);
                let cv = DVec2::new(c.x as f64, c.y as f64);
                let ev = DVec2::new(end.x as f64, end.y as f64);
                let cp1 = prev_pos + (cv - prev_pos) * (2.0 / 3.0);
                let cp2 = ev + (cv - ev) * (2.0 / 3.0);
                // Set out_handle on previous anchor.
                if let Some(prev) = anchors.last_mut() {
                    prev.out_handle = cp1 - prev.pos;
                }
                let in_handle = cp2 - ev;
                anchors.push(Anchor {
                    pos: ev,
                    in_handle,
                    out_handle: DVec2::ZERO,
                    smooth: false,
                });
            }
            PathSegment::CubicTo(c1, c2, end) => {
                let c1v = DVec2::new(c1.x as f64, c1.y as f64);
                let c2v = DVec2::new(c2.x as f64, c2.y as f64);
                let ev = DVec2::new(end.x as f64, end.y as f64);
                // Set out_handle on previous anchor.
                if let Some(prev) = anchors.last_mut() {
                    prev.out_handle = c1v - prev.pos;
                }
                let in_handle = c2v - ev;
                // Heuristic: anchor is smooth if in and out handles are
                // roughly collinear and on opposite sides.
                let smooth = false; // conservative — can be refined later
                anchors.push(Anchor {
                    pos: ev,
                    in_handle,
                    out_handle: DVec2::ZERO,
                    smooth,
                });
            }
            PathSegment::Close => {
                closed = true;
                // If the last anchor is very close to the first (because the
                // SVG path closed with an explicit LineTo back to the start
                // before `Z`), collapse them.
                if anchors.len() >= 2 {
                    let first_pos = anchors[0].pos;
                    if let Some(last) = anchors.last() {
                        if (last.pos - first_pos).length() < 1e-6 {
                            // Transfer last anchor's in_handle to the first.
                            let in_h = last.in_handle;
                            anchors.pop();
                            if let Some(f) = anchors.first_mut() {
                                f.in_handle = in_h;
                            }
                        }
                    }
                }
            }
        }
    }

    if anchors.is_empty() {
        return None;
    }

    let ipath = IPath { anchors, closed };
    let style = convert_style(p.fill(), p.stroke());

    Some(PathNode {
        transform: Default::default(),
        style,
        path: ipath,
    })
}

fn convert_style(fill: Option<&usvg::Fill>, stroke: Option<&usvg::Stroke>) -> Style {
    let fill_paint = fill.and_then(|f| {
        match f.paint() {
            usvg::Paint::Color(c) => {
                let alpha = f.opacity().get();
                Some(Paint::Solid(Color::rgba(
                    c.red as f32 / 255.0,
                    c.green as f32 / 255.0,
                    c.blue as f32 / 255.0,
                    alpha,
                )))
            }
            _ => None, // gradients/patterns → fall back to none
        }
    });

    let stroke_style = stroke.and_then(|s| {
        let paint = match s.paint() {
            usvg::Paint::Color(c) => {
                let alpha = s.opacity().get();
                Paint::Solid(Color::rgba(
                    c.red as f32 / 255.0,
                    c.green as f32 / 255.0,
                    c.blue as f32 / 255.0,
                    alpha,
                ))
            }
            _ => return None,
        };
        let cap = match s.linecap() {
            usvg::LineCap::Butt => LineCap::Butt,
            usvg::LineCap::Round => LineCap::Round,
            usvg::LineCap::Square => LineCap::Square,
        };
        let join = match s.linejoin() {
            usvg::LineJoin::Miter | usvg::LineJoin::MiterClip => LineJoin::Miter,
            usvg::LineJoin::Round => LineJoin::Round,
            usvg::LineJoin::Bevel => LineJoin::Bevel,
        };
        let width = s.width().get() as f64;
        let dash = if let Some(d) = s.dasharray() {
            Some(d.iter().map(|v| *v as f32).collect())
        } else {
            None
        };
        Some(IStroke { paint, width, cap, join, dash })
    });

    Style {
        fill: fill_paint,
        stroke: stroke_style,
        opacity: 1.0,
    }
}

fn convert_image(img: &usvg::Image) -> Option<ImageNode> {
    let bytes = match img.kind() {
        usvg::ImageKind::PNG(data)
        | usvg::ImageKind::JPEG(data)
        | usvg::ImageKind::GIF(data)
        | usvg::ImageKind::WEBP(data) => data.to_vec(),
        usvg::ImageKind::SVG(_) => return None, // nested SVG — skip
    };
    let image_data = ImageData::from_bytes(bytes).ok()?;
    let rect = img.size();
    let position = DVec2::new(0.0, 0.0); // usvg places images via transform
    let size = DVec2::new(rect.width() as f64, rect.height() as f64);
    Some(ImageNode {
        position,
        size,
        style: Style::default(),
        image: image_data,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{Color, Paint, Stroke};
    use crate::path::{Path as IPath, Anchor};

    #[test]
    fn test_svg_roundtrip_path() {
        let mut doc = Document::default();
        let layer_id = doc.layers[0];

        // Create a simple path node: a closed triangle
        let path = IPath {
            anchors: vec![
                Anchor::corner(DVec2::new(10.0, 10.0)),
                Anchor::corner(DVec2::new(50.0, 10.0)),
                Anchor::corner(DVec2::new(30.0, 40.0)),
            ],
            closed: true,
        };
        let mut style = Style::default();
        style.fill = Some(Paint::Solid(Color::rgba(1.0, 0.0, 0.0, 1.0))); // red fill
        style.stroke = Some(Stroke {
            paint: Paint::Solid(Color::rgba(0.0, 0.0, 1.0, 0.5)), // semi-transparent blue stroke
            width: 3.0,
            cap: LineCap::Round,
            join: LineJoin::Round,
            dash: None,
        });

        let path_node = PathNode {
            transform: Default::default(),
            style,
            path,
        };

        doc.add_node(layer_id, crate::doc::Node::path("Triangle", path_node));

        // Create a temp file path
        let mut temp_path = std::env::temp_dir();
        temp_path.push("illuminator_test_roundtrip.svg");

        // Export SVG
        let opts = ExportOptions::default();
        export_svg(&doc, &temp_path, &opts).unwrap();

        // Import SVG back
        let result = import_svg(&temp_path).unwrap();

        // Clean up temp file
        let _ = std::fs::remove_file(&temp_path);

        assert_eq!(result.paths.len(), 1);
        let imported_node = &result.paths[0];

        // Assert path points
        assert_eq!(imported_node.path.anchors.len(), 3);
        assert!((imported_node.path.anchors[0].pos - DVec2::new(10.0, 10.0)).length() < 1e-4);
        assert!((imported_node.path.anchors[1].pos - DVec2::new(50.0, 10.0)).length() < 1e-4);
        assert!((imported_node.path.anchors[2].pos - DVec2::new(30.0, 40.0)).length() < 1e-4);
        assert!(imported_node.path.closed);

        // Assert style
        if let Some(Paint::Solid(c)) = &imported_node.style.fill {
            assert!((c.r - 1.0).abs() < 1e-4);
            assert!((c.g - 0.0).abs() < 1e-4);
            assert!((c.b - 0.0).abs() < 1e-4);
            assert!((c.a - 1.0).abs() < 1e-4);
        } else {
            panic!("Expected solid red fill");
        }

        if let Some(s) = &imported_node.style.stroke {
            let Paint::Solid(c) = &s.paint else { panic!("expected solid paint") };
            assert!((c.r - 0.0).abs() < 1e-4);
            assert!((c.g - 0.0).abs() < 1e-4);
            assert!((c.b - 1.0).abs() < 1e-4);
            assert!((c.a - 0.5).abs() < 1e-4);
            assert!((s.width - 3.0).abs() < 1e-4);
            assert_eq!(s.cap, LineCap::Round);
            assert_eq!(s.join, LineJoin::Round);
        } else {
            panic!("Expected stroke");
        }
    }

    #[test]
    fn test_svg_export_selection() {
        let mut doc = Document::default();
        let layer_id = doc.layers[0];

        // Add two path nodes
        let p1 = PathNode {
            transform: Default::default(),
            style: Style::default(),
            path: IPath {
                anchors: vec![Anchor::corner(DVec2::new(0.0, 0.0)), Anchor::corner(DVec2::new(5.0, 5.0))],
                closed: false,
            },
        };
        let _id1 = doc.add_node(layer_id, crate::doc::Node::path("Node 1", p1)).unwrap();

        let p2 = PathNode {
            transform: Default::default(),
            style: Style::default(),
            path: IPath {
                anchors: vec![Anchor::corner(DVec2::new(10.0, 10.0)), Anchor::corner(DVec2::new(15.0, 15.0))],
                closed: false,
            },
        };
        let id2 = doc.add_node(layer_id, crate::doc::Node::path("Node 2", p2)).unwrap();

        let mut selection = std::collections::HashSet::new();
        selection.insert(id2); // select only node 2

        let mut temp_path = std::env::temp_dir();
        temp_path.push("illuminator_test_selection.svg");

        let opts = ExportOptions::default();
        export_svg_selection(&doc, &selection, &temp_path, &opts).unwrap();

        let result = import_svg(&temp_path).unwrap();
        let _ = std::fs::remove_file(&temp_path);

        // Verify only node 2 was exported
        assert_eq!(result.paths.len(), 1);
        assert!((result.paths[0].path.anchors[0].pos - DVec2::new(10.0, 10.0)).length() < 1e-4);
    }
}

