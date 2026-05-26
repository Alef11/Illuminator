# Illuminator — Plan

A native Windows vector design app for logos and illustrations. Illustrator-style infinite canvas, bezier path tool first, then text/images/layers.

Stack: **Rust + egui (eframe) + Skia (skia-safe) + wgpu**.

---

## 1. Vision and scope

**Audience.** Logo and graphic designers who want a fast, native, no-subscription Illustrator alternative on Windows.

**v1.0 promise.** "Open the app, draw a clean bezier logo with shapes and text on a reference photo, export SVG/PNG."

**Out of scope for v1.** Print/CMYK workflows, complex effects (mesh gradients, 3D, brushes), plugin system, collaboration, mobile.

**Non-goals.** Pixel painting (Photoshop territory), page layout (InDesign), animation.

---

## 2. Tech stack and crates

### Core
| Concern | Choice | Why |
|---|---|---|
| Language | Rust (stable, 2024 edition) | Memory safety, performance, strong types for a complex data model |
| App shell | `eframe` (egui) | Immediate-mode UI is excellent for tool palettes, properties, layers panels |
| UI backend | wgpu (via eframe) | GPU-accelerated, future-proof, cross-vendor |
| 2D renderer | `skia-safe` | Industry-grade vector rasterizer; same engine as Chrome/Flutter. Handles paths, text, gradients, blend modes |
| Path math | `kurbo` | Bezier math (offsets, intersections, flattening) when Skia's API isn't enough |
| ID/arena | `slotmap` | Stable IDs for nodes; cheap to clone/serialize; avoids Rc<RefCell<>> graph hell |
| Serialization | `serde`, `serde_json`, `rmp-serde` (later) | JSON for `.ilm` files first; MessagePack later for size |
| SVG | `usvg` + `resvg` | Best-in-class SVG parsing and rasterization in Rust |
| Images | `image` | PNG/JPEG/WEBP decode |
| Fonts | Skia's `Typeface` + `font-kit` for system enumeration | System font picker + Skia for shaping/rendering |
| Math | `glam` | SIMD-friendly Vec2/Mat3 |
| Undo | `im` (immutable collections) for snapshot diffs, or hand-rolled command stack | Decided in Phase 2 |
| Logging | `tracing` + `tracing-subscriber` | Standard |
| Errors | `thiserror` (library), `anyhow` (app boundaries) | Standard |

### Why egui + Skia (not lyon-only, not iced)

- **egui** wins on iteration speed for tool-heavy UIs. Panels, dockable layout, immediate re-render on input. iced is more structured but requires more boilerplate for a freeform design app.
- **Skia** handles text shaping, gradients, blend modes, and high-quality path rasterization out of the box. A pure-Rust `lyon + wgpu` path is doable but you'd reimplement a year of features.
- We render the **UI chrome with egui** and the **document canvas with Skia**, compositing Skia's output as a texture into the egui frame each tick.

### Build/tooling
- Windows: install `cargo`, MSVC build tools, Skia prebuilt binaries (skia-safe's build script handles it).
- Crate layout: cargo workspace (`illuminator-core`, `illuminator-render`, `illuminator-app`).
- Lint: `clippy -W clippy::pedantic` selectively; `rustfmt`.
- Tests: `cargo test` for core/math; visual regression via snapshot PNGs for renderer.

---

## 3. High-level architecture

```
┌────────────────────────────────────────────────────────────────┐
│ illuminator-app  (eframe binary)                               │
│  ┌──────────────────────────────────────────────────────────┐ │
│  │ UI (egui): menus, toolbar, panels, status bar            │ │
│  └──────────────────────────────────────────────────────────┘ │
│  ┌──────────────────────────────────────────────────────────┐ │
│  │ Canvas widget                                            │ │
│  │  - owns ViewTransform (pan, zoom)                        │ │
│  │  - dispatches input to active Tool                       │ │
│  │  - draws Skia surface to a wgpu texture each frame       │ │
│  └──────────────────────────────────────────────────────────┘ │
│  ┌──────────────────────────────────────────────────────────┐ │
│  │ Tool system: Select, DirectSelect, Pen, Rect, Ellipse,   │ │
│  │   Polygon, Text, Image, Hand, Zoom                       │ │
│  └──────────────────────────────────────────────────────────┘ │
│  ┌──────────────────────────────────────────────────────────┐ │
│  │ Command stack (undo/redo)                                │ │
│  └──────────────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────────────┐
│ illuminator-core                                               │
│  Document model: Document → Layer → Node (Path, Text, Image,   │
│  Group). SlotMap-backed arena, stable NodeIds.                 │
│  Geometry primitives, transforms, spatial index (AABB tree).   │
│  File format (serde).                                          │
└────────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌────────────────────────────────────────────────────────────────┐
│ illuminator-render                                             │
│  Scene → Skia draw calls. View transform. Visibility culling   │
│  via spatial index. Renders to SKSurface, hands texture to egui│
└────────────────────────────────────────────────────────────────┘
```

**Key principle.** The document model is the single source of truth and is pure data (Serde-friendly, no rendering or UI types in it). Renderers and UI read from it; tools mutate it only through `Command`s on the undo stack.

---

## 4. Document data model

```rust
// illuminator-core/src/doc.rs

pub struct Document {
    pub title: String,
    pub canvas_hint: Option<Rect>,   // optional "artboard" hint; canvas itself is infinite
    pub layers: Vec<LayerId>,        // top-to-bottom display order
    pub layer_arena: SlotMap<LayerId, Layer>,
    pub node_arena: SlotMap<NodeId, Node>,
}

pub struct Layer {
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub opacity: f32,                // 0..1
    pub blend: BlendMode,
    pub kind: LayerKind,             // Normal | Reference { dimmed: bool }
    pub nodes: Vec<NodeId>,          // back-to-front
}

pub enum Node {
    Path(PathNode),
    Text(TextNode),
    Image(ImageNode),
    Group(GroupNode),
}

pub struct PathNode {
    pub transform: Affine,           // local→world
    pub style: Style,
    pub path: Path,
}

pub struct Path {
    pub anchors: Vec<Anchor>,
    pub closed: bool,
}

pub struct Anchor {
    pub pos: DVec2,                  // world units, f64 for infinite canvas precision
    pub in_handle: DVec2,            // relative to pos; (0,0) = no handle (corner)
    pub out_handle: DVec2,           // relative to pos
    pub smooth: bool,                // mirror handles when editing
}

pub struct Style {
    pub fill: Option<Paint>,         // None = no fill
    pub stroke: Option<Stroke>,
    pub opacity: f32,
}

pub enum Paint {
    Solid(Color),
    LinearGradient { ... },          // post-MVP
    RadialGradient { ... },          // post-MVP
}

pub struct Stroke {
    pub paint: Paint,
    pub width: f64,
    pub cap: LineCap,
    pub join: LineJoin,
    pub dash: Option<Vec<f32>>,
}
```

**Why these shapes.**

- `DVec2` (f64) world coords avoid precision loss when zoomed in 1000× on a path far from origin — the "infinite canvas" only feels infinite if you don't see jitter.
- SlotMap gives stable `NodeId`s that survive deletes and serialization. Lets the undo stack reference nodes safely.
- Anchor matches SVG/PostScript exactly (in/out handles relative to point) — clean import/export and matches what designers expect.
- Layers are top-level, not nested-in-document-as-group, so the Layers panel has an obvious mapping.

---

## 5. Infinite canvas and view transform

```rust
pub struct ViewTransform {
    pub pan: DVec2,    // world coords at screen center
    pub zoom: f64,     // pixels per world unit
}
```

- World→screen: `screen = (world - pan) * zoom + viewport_center`.
- All hit-testing converts the mouse to world coords first.
- Zoom range: 0.01× to 64,000× (Illustrator-like). Limit programmatically to keep f64 precise.
- Pan/zoom controls: space-drag, mousewheel zoom (anchored to cursor), Ctrl+0 fit, Ctrl+1 actual size.
- Use a spatial index (rstar crate's R-tree, or a flat AABB list for v1) to cull off-screen nodes per frame.

**Snapping** (v1 lite, post-MVP full):
- Grid snap (toggle).
- Smart guides: snap to other nodes' anchors, midpoints, intersections.
- Pixel-grid snap when zoomed in.

---

## 6. Rendering pipeline

Each frame:

1. eframe gives us an `egui::Painter` and a wgpu device.
2. Compute visible AABB in world coords from the canvas widget's screen rect and the view transform.
3. Query spatial index for visible nodes per layer (back-to-front, by layer order, by node order).
4. Acquire/recreate an `SkSurface` sized to the canvas widget's pixel rect.
5. Apply view transform to the `SkCanvas`.
6. Walk layers: skip hidden; apply layer opacity & blend via `SkPaint`; draw each node.
   - `PathNode`: build `SkPath` from anchors (cubic bezier `cubicTo` between adjacent points using out/in handles), fill + stroke.
   - `TextNode`: shape with Skia's typeface; draw text blob.
   - `ImageNode`: blit `SkImage`.
7. Read the surface as a texture, hand to egui via `egui::TextureHandle::set` / register native texture, draw as `Image` in the canvas rect.
8. Overlay (in egui directly, not Skia): selection box, anchor handles, tool previews, snapping guides — these don't go through Skia so they stay crisp regardless of zoom.

**Performance budget.** 60 fps at 4K with 10k visible path nodes. Profile early; cache static layers to off-screen surfaces if needed (especially reference layers, which never change between edits).

---

## 7. Tool system

```rust
pub trait Tool {
    fn on_pointer_down(&mut self, ctx: &mut ToolCtx, ev: PointerEvent);
    fn on_pointer_move(&mut self, ctx: &mut ToolCtx, ev: PointerEvent);
    fn on_pointer_up(&mut self, ctx: &mut ToolCtx, ev: PointerEvent);
    fn on_key(&mut self, ctx: &mut ToolCtx, key: KeyEvent);
    fn draw_overlay(&self, ctx: &ToolCtx, painter: &egui::Painter); // selection/preview chrome
    fn cursor(&self) -> CursorKind;
}

pub struct ToolCtx<'a> {
    pub doc: &'a mut Document,
    pub view: &'a ViewTransform,
    pub commands: &'a mut CommandStack,
    pub selection: &'a mut Selection,
    pub active_layer: LayerId,
    pub modifiers: Modifiers,
}
```

**Tools shipped in v1:**
- **Select (V)** — marquee, click-pick, move, transform (scale/rotate via handles).
- **Direct Select (A)** — click anchors and handles; box-select anchors; drag to edit a single anchor or handle.
- **Pen (P)** — the main event. Click adds corner anchor; click-drag adds smooth anchor (drag distance sets out-handle, in-handle mirrored). Alt-click toggles smooth/corner. Click first anchor to close path. Enter/Esc to finish.
- **Add/Delete Anchor (+/-)** — context-sensitive on Pen.
- **Convert Anchor (Shift+C)** — toggle smooth/corner on existing anchor.
- **Rectangle (M), Ellipse (L), Polygon, Star, Line** — drag to create; shift constrains aspect.
- **Text (T)** — click to place caret; type. Properties panel sets font, size, alignment, tracking.
- **Image (I)** — drag-drop or menu import; click to place.
- **Hand (H / hold space)** — pan.
- **Zoom (Z)** — click zoom in, alt-click zoom out, drag-zoom to area.

Every mutating tool operation produces a `Command` pushed to the undo stack — never mutate the doc directly from a tool.

---

## 8. Bezier paths in depth (the #1 feature)

This is where Illustrator excels and where we must too.

**Authoring (Pen tool).**
- Click → add corner anchor (handles = (0,0)).
- Click-drag → add smooth anchor; out-handle = drag vector, in-handle = -drag vector.
- Alt during drag → break symmetry (independent handles).
- Hovering existing anchor → preview close-path indicator.
- Backspace mid-path → remove last anchor.
- Visual feedback: rubber-band preview cubic from last anchor to cursor while moving.

**Editing (Direct Select).**
- Drag anchor → move anchor; handles travel with it.
- Drag handle → rotate/scale that side; if anchor is `smooth`, opposite handle mirrors; alt breaks mirror.
- Double-click anchor → toggle smooth/corner.
- Selecting multiple anchors → move/scale/rotate as a group (transform widget appears).

**Numerical operations (post-MVP).**
- Add anchor at parametric position on segment (use de Casteljau).
- Path simplification (Ramer–Douglas–Peucker + smooth-fit).
- Boolean ops (union/intersect/subtract) via `kurbo` or Skia's `Op`.
- Path offset (stroke→fill conversion) via `kurbo`.

**Hit testing.**
- Anchor: small AABB around `pos`, in screen space (so hit area is consistent at all zooms).
- Handle: same, around `pos + handle`.
- Segment: flatten cubic to lines (Skia's `SkPath::AsLines` or kurbo's flatten), test distance to mouse in screen units.

---

## 9. Text, fonts, images

**Text.**
- Two modes: **point text** (Illustrator's standard click-to-type) for v1; **area text** (bounded box, wrapping) for v1.1.
- Font picker: enumerate system fonts via `font-kit`; render previews using each typeface.
- Style: family, weight, italic, size, fill/stroke, alignment, leading, tracking.
- Convert-to-outlines command (Type → Create Outlines): use Skia's `getTextPath` to bake into a `PathNode`. Critical for logo export.

**Images.**
- Supported: PNG, JPEG, WEBP for raster. SVG goes through `usvg` and lands as a Group of `PathNode`s + `TextNode`s.
- Drag-and-drop onto canvas → placed at drop point, centered, original pixel size mapped to world units 1:1.
- Lock aspect by default when scaling.

---

## 10. Layers (including reference layers)

UI: standard Illustrator-like Layers panel — reorder via drag, eyeball toggle, padlock toggle, opacity slider per layer.

**Reference layer kind.**
- `LayerKind::Reference { dimmed: bool }`.
- Defaults: locked, opacity 0.5, blend `Multiply` (so dark inspiration art shows through cleanly).
- Visually marked in panel (different icon, italicized name).
- Never exported by default — Export dialog has a "include reference layers" checkbox, off by default.
- Use case: drop an inspiration JPEG on a reference layer, design over it without touching it.

**Layer operations:**
- New, duplicate, delete, merge-down, rename, reorder.
- Move-selection-to-layer.
- Isolation mode (double-click a layer; hides others, dims them, restricts selection).

---

## 11. File format

**Native: `.ilm` (Illuminator).**
- v1: pretty-printed JSON via serde. Easy to debug, easy to diff, version-tagged.
- v2: optional binary (MessagePack) for size, same schema.
- Embedded images: base64-inline for small, or store as sibling files in a zip-bundle `.ilmz` (later).

```json
{
  "version": 1,
  "title": "Logo draft",
  "view": { "pan": [0, 0], "zoom": 1.0 },
  "layers": [
    { "name": "Reference", "kind": {"Reference": {"dimmed": true}}, "opacity": 0.5, "nodes": [...] },
    { "name": "Logo", "kind": "Normal", "nodes": [...] }
  ]
}
```

**Import.**
- SVG (priority, via `usvg`).
- PNG/JPEG/WEBP (raster, dropped into image node).

**Export.**
- SVG (priority — round-trippable for logos).
- PNG (rasterize via Skia at chosen DPI).
- PDF (later — Skia can produce PDF; PostScript fonts a separate problem).

**Versioning.** Every file has `"version": N`. Loader has a migration chain `migrate_v1_to_v2(...)`.

---

## 12. Project structure

```
Illuminator/
├── Cargo.toml                  # workspace
├── PLAN.md
├── README.md
├── crates/
│   ├── illuminator-core/       # document model, math, file format, undo
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── doc.rs
│   │   │   ├── path.rs
│   │   │   ├── transform.rs
│   │   │   ├── style.rs
│   │   │   ├── command.rs
│   │   │   ├── spatial.rs
│   │   │   └── io/{json.rs, svg.rs}
│   │   └── tests/
│   ├── illuminator-render/     # Skia scene rendering
│   │   └── src/lib.rs
│   └── illuminator-app/        # eframe binary
│       └── src/
│           ├── main.rs
│           ├── app.rs
│           ├── canvas.rs
│           ├── view.rs         # ViewTransform
│           ├── tools/
│           │   ├── mod.rs
│           │   ├── select.rs
│           │   ├── direct_select.rs
│           │   ├── pen.rs
│           │   ├── shapes.rs
│           │   ├── text.rs
│           │   ├── image.rs
│           │   └── hand.rs
│           └── panels/
│               ├── layers.rs
│               ├── properties.rs
│               ├── colors.rs
│               └── tools_palette.rs
└── assets/
    ├── icons/
    └── fonts/                  # bundled default UI fonts
```

---

## 13. Phased roadmap

Each phase is "shippable" — could be released to a friend for feedback.

### Phase 0 — Skeleton (1 week)
- Cargo workspace, eframe app boots, empty window with menu + toolbar + 3 panels (Layers / Properties / Tools).
- Skia surface compositing into the canvas region (just clear to gray).
- Logging, error type, file dialogs wired (open/save no-ops).
- **Done when:** the window looks like a design app even though nothing draws.

### Phase 1 — Infinite canvas + first shape (1–2 weeks)
- `ViewTransform`, pan with space-drag, zoom with wheel (cursor-anchored), Ctrl+0 fit, Ctrl+1 100%.
- Spatial index (start with a flat Vec of AABBs; swap to rstar when it bites).
- Rectangle and Ellipse tools.
- Selection tool: click-pick, marquee, move.
- `Document` save/load to `.ilm` JSON.
- **Done when:** you can draw 50 rectangles, save, reopen, pan around at 1000% zoom without jitter.

### Phase 2 — Pen tool and path editing (2–3 weeks) ⭐ headline feature
- Pen tool (click corner / click-drag smooth / alt break / close on first / esc finish).
- Direct-select: drag anchors and handles, smooth/corner toggle.
- Add/delete anchor.
- Undo/redo (command stack with `Box<dyn Command>` and `do/undo`).
- Properties panel: stroke color, stroke width, fill color, opacity.
- **Done when:** you can pen-trace a logo with curves, edit it, and undo every step.

### Phase 3 — Layers + reference art (1 week)
- Layers panel: list, reorder (drag), visible/lock/opacity, rename.
- Reference layer kind: locked, dimmed, Multiply blend by default; toggle in layer kebab menu.
- Move-selection-to-layer.
- **Done when:** drop a photo on a Reference layer, trace it with the Pen tool, hide the reference, see clean output.

### Phase 4 — Text (1–2 weeks)
- Font enumeration via font-kit, font picker UI with previews.
- Point text: click, type, edit. Family/weight/size/alignment in Properties.
- Convert-to-outlines command.
- **Done when:** a typeset wordmark logo renders identically before and after convert-to-outlines.

### Phase 5 — Images + SVG I/O (1 week)
- Image import (drag-drop + menu), placed as `ImageNode`.
- SVG import via `usvg` → native path nodes.
- SVG export of all native primitives.
- **Done when:** import an SVG, edit it, re-export, diff is minimal and visually identical.

### Phase 6 — Polish (2 weeks)
- Snapping (grid + smart guides).
- Align/distribute commands.
- Keyboard shortcuts review pass (match Illustrator where reasonable).
- Color picker UI.
- Status bar (zoom, cursor coords, selection count).
- Crash recovery (autosave every N seconds to temp).
- Installer (MSIX or WiX-based MSI).
- **Done when:** you'd actually use it to ship a real logo.

### Post-v1 (in priority order)
1. Boolean path ops (union/intersect/subtract/exclude).
2. Linear and radial gradients.
3. Area text + basic wrapping.
4. Path offset / outline stroke.
5. Artboards (multiple, per-document).
6. PDF export.
7. Symbols (reusable components).
8. Plugin/script API (WASM or Rhai).

**Rough total to v1:** 9–13 weeks of focused work.

---

## 14. Risks and open questions

| Risk | Mitigation |
|---|---|
| Skia-safe build pain on Windows | Verify in Phase 0. If brutal, fall back to `tiny-skia` (no GPU, OK for v1 prototype) or `vello` (wgpu-native vector renderer, less mature but Rust-idiomatic) |
| egui custom widgets feel non-native for pro tools | Accept it; Illustrator's UI density is high enough that egui's flat aesthetic is fine. Add custom theming pass in Phase 6 |
| Text shaping (complex scripts, kerning) | Skia handles Latin/CJK well. Punt RTL/Arabic ligatures and OpenType features to post-v1; document the gap |
| Infinite-canvas f64 precision | Already accounted for in `DVec2`; clamp zoom range; verify visually in Phase 1 |
| Spatial index complexity | Don't build it until profiling demands it. Flat AABB scan is fine for <1000 nodes |
| Undo memory growth | Snapshot-with-structural-sharing via `im` if command-stack memory becomes a problem |
| File format breakage between versions | Tag every file with `version`; never break old files — always migrate forward |

**Open questions to resolve before Phase 1:**
1. Color model: sRGB everywhere for v1, or wide-gamut from the start? (Recommend: sRGB for v1, design `Color` enum to allow Display P3 later.)
2. Coordinate system: y-down (screen-native, matches Skia) or y-up (math-native, matches Illustrator's ruler)? (Recommend: y-down internally; display y-up in the rulers/status bar to match designer expectations.)
3. Units in UI: pixels, points, millimeters? (Recommend: configurable per document; default pixels for screen-first logo work.)

---

## 15. Recommended next steps

1. Initialize the workspace (`cargo new --lib crates/illuminator-core`, etc.) and confirm `skia-safe` builds clean on this machine — this is the single biggest tooling risk.
2. Build the Phase 0 shell so the visual target is real.
3. Implement `ViewTransform` + Skia-to-egui texture compositing in isolation — it's the foundation everything else stands on.
4. Then commit to Phase 1 (rectangle, ellipse, save/load) before touching the Pen tool. Resist starting with the headliner — having a working pipeline first makes Phase 2 way easier.

When you're ready, say the word and we'll start scaffolding Phase 0.
