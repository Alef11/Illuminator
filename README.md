# Illuminator

A native Windows vector design app — an Illustrator alternative focused on logos and designs.
Built with **Rust + egui + wgpu**.

See [PLAN.md](PLAN.md) for full design and roadmap.

## Status — Phase 4 + polish complete

What works today:

- Infinite canvas with cursor-anchored wheel zoom (1% – 6,400,000%) and middle-button / space-drag pan
- **Snap to Grid + Smart Guides** (View menu) — smart-snap to other shapes' anchors and image corners during shape draw, move, anchor drag, and Pen placement; visual hint marker at the snap target
- **Pen tool (P)** — click for corner anchor, click-drag for smooth anchor with mirrored handles, alt-drag for asymmetric handles, click first anchor to close, Enter to finish open, Esc to cancel, Backspace to drop last anchor
- **Direct Select (A)** v2 — anchor markers, smooth/corner anchors distinguished visually, drag anchors *or* drag in/out handles to reshape curves; Alt-drag a handle to break smooth-symmetry
- **Text tool (T)** — click empty area to start a new text node, type to edit, Shift+Enter for newline, Enter / Esc / click-out to commit; click existing text to re-enter editing; Properties panel sets font family (Sans / Mono), size, color, content
- Rectangle (M) and Ellipse (L) tools (drag to draw, with snapping)
- Selection (V): click-pick, marquee, shift to extend, drag selected shapes to move; **drag image corners to resize** (Shift locks aspect ratio)
- **Image import** — drag-drop PNG/JPG/BMP/WEBP onto the canvas, or File → Place Image…; embedded in document, full alpha & layer blending; W/H drag values in Properties for precise sizing
- Undo / Redo (Ctrl+Z / Ctrl+Y) — every interaction commits a single, scoped undo step
- Layers panel: add, delete, rename, reorder, visibility / lock / opacity
- **Reference layers** (locked + multiply-blend by default — drop your inspiration JPG, switch to a normal layer, trace over with Pen)
- Properties panel: per-shape fill / stroke / width / opacity, per-image W/H + opacity, per-text font / size / color, plus defaults for new shapes
- File format `.ilm` — JSON, versioned, full open / save / save-as (images embedded as base64)

## Build and run

```
cargo run --bin illuminator
```

Release build:

```
cargo run --release --bin illuminator
```

Requires **rustc 1.88+**. Tested on rustc 1.95 (Windows 11, MSVC).

## Keyboard shortcuts

| Key            | Action                          |
|----------------|---------------------------------|
| V              | Select tool                     |
| A              | Direct Select (anchor / handle) |
| P              | Pen tool                        |
| T              | Text tool                       |
| M              | Rectangle tool                  |
| L              | Ellipse tool                    |
| H              | Hand (pan) tool                 |
| Space + drag   | Pan (any tool)                  |
| Middle drag    | Pan (any tool)                  |
| Mouse wheel    | Zoom at cursor                  |
| Alt + drag     | (Pen) asymmetric handles        |
| Enter          | (Pen) finish open path          |
| Esc            | (Pen) cancel in-progress path   |
| Backspace      | (Pen) drop last anchor / (else) delete selection |
| Ctrl + Z       | Undo                            |
| Ctrl + Y       | Redo                            |
| Ctrl + Shift+Z | Redo (alternate)                |
| Ctrl + S       | Save                            |
| Ctrl + Shift+S | Save As…                        |
| Ctrl + A       | Select all                      |
| Delete         | Delete selection                |
| Shift + click  | Add to / toggle selection       |

## Repo layout

```
crates/
├── illuminator-core/   pure data model: Document, Path, Style, ViewTransform, file format, commands
├── illuminator-render/ Scene → egui::Painter (Skia swap-in slot for Phase 2+)
└── illuminator-app/    eframe binary: tools, panels, menu, canvas widget
```

## What's next

- Phase 5 — SVG import / export, PDF export
- skia-safe swap-in (text shaping, proper non-convex tessellation, gradients)
- Boolean path ops (union / intersect / subtract)
- Smart guides v2 — snap to midpoints, edges, and equal-spacing
- Direct-Select v3 — add/delete anchor on a segment, double-click to toggle smooth/corner
- System font picker for the Text tool (currently uses egui's bundled fonts)

## License

MIT OR Apache-2.0
