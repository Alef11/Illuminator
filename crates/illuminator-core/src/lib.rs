//! Illuminator core: pure data model, geometry, file format. No UI, no rendering.

pub mod command;
pub mod doc;
pub mod image;
pub mod io;
pub mod path;
pub mod style;
pub mod svg;
pub mod text;
pub mod transform;

pub use doc::{Document, Layer, LayerId, LayerKind, Node, NodeId, NodeKind};
pub use image::{ImageData, ImageDecodeError, ImageNode};
pub use path::{Anchor, Path, PathNode};
pub use style::{BlendMode, Color, LineCap, LineJoin, Paint, Stroke, Style};
pub use svg::{ExportOptions, SvgImportResult};
pub use text::{FontFamily, TextNode};
pub use transform::ViewTransform;

