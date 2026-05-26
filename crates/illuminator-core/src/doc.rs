use glam::DVec2;
use serde::{Deserialize, Serialize};
use slotmap::{new_key_type, SlotMap};

use crate::image::ImageNode;
use crate::path::PathNode;
use crate::style::BlendMode;
use crate::text::TextNode;

new_key_type! {
    pub struct LayerId;
    pub struct NodeId;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum LayerKind {
    Normal,
    /// Locked-by-default tracing layer, multiplied at reduced opacity so the
    /// underlying art shows through cleanly.
    Reference {
        #[serde(default = "default_true")]
        dimmed: bool,
    },
}

fn default_true() -> bool { true }

impl Default for LayerKind {
    fn default() -> Self { Self::Normal }
}

impl LayerKind {
    pub fn is_reference(&self) -> bool {
        matches!(self, LayerKind::Reference { .. })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Layer {
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub opacity: f32,
    #[serde(default)]
    pub blend: BlendMode,
    #[serde(default)]
    pub kind: LayerKind,
    pub nodes: Vec<NodeId>,
}

impl Layer {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            visible: true,
            locked: false,
            opacity: 1.0,
            blend: BlendMode::Normal,
            kind: LayerKind::Normal,
            nodes: Vec::new(),
        }
    }

    pub fn reference(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            visible: true,
            locked: true,
            opacity: 0.5,
            blend: BlendMode::Multiply,
            kind: LayerKind::Reference { dimmed: true },
            nodes: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum NodeKind {
    Path(PathNode),
    Image(ImageNode),
    Text(TextNode),
    // Group(GroupNode) — post-v1
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Node {
    pub name: String,
    pub kind: NodeKind,
}

impl Node {
    pub fn path(name: impl Into<String>, path: PathNode) -> Self {
        Self { name: name.into(), kind: NodeKind::Path(path) }
    }

    pub fn image(name: impl Into<String>, image: ImageNode) -> Self {
        Self { name: name.into(), kind: NodeKind::Image(image) }
    }

    pub fn text(name: impl Into<String>, text: TextNode) -> Self {
        Self { name: name.into(), kind: NodeKind::Text(text) }
    }

    /// Conservative AABB in world coordinates. None for empty paths.
    pub fn bounds(&self) -> Option<(DVec2, DVec2)> {
        match &self.kind {
            NodeKind::Path(p) => p.path.bounds(),
            NodeKind::Image(i) => Some(i.bounds()),
            NodeKind::Text(t) => Some(t.approx_bounds()),
        }
    }

    /// Translate all anchor positions / image origin / text origin by `delta`.
    pub fn translate(&mut self, delta: DVec2) {
        match &mut self.kind {
            NodeKind::Path(p) => {
                for a in &mut p.path.anchors {
                    a.pos += delta;
                }
            }
            NodeKind::Image(i) => {
                i.position += delta;
            }
            NodeKind::Text(t) => {
                t.position += delta;
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Artboard {
    pub name: String,
    pub min: DVec2,
    pub max: DVec2,
}

/// The whole document. Single source of truth; renderers and tools observe
/// it, mutations happen only through commands.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Document {
    #[serde(default = "current_version")]
    pub version: u32,
    pub title: String,
    /// Top-of-list = topmost layer (Illustrator convention).
    pub layers: Vec<LayerId>,
    pub layer_arena: SlotMap<LayerId, Layer>,
    pub node_arena: SlotMap<NodeId, Node>,
    #[serde(default)]
    pub artboards: Vec<Artboard>,
}

pub const CURRENT_VERSION: u32 = 1;
fn current_version() -> u32 { CURRENT_VERSION }

impl Default for Document {
    fn default() -> Self {
        let mut layer_arena: SlotMap<LayerId, Layer> = SlotMap::with_key();
        let id = layer_arena.insert(Layer::new("Layer 1"));
        let artboards = Vec::new();
        Self {
            version: CURRENT_VERSION,
            title: String::from("Untitled"),
            layers: vec![id],
            layer_arena,
            node_arena: SlotMap::with_key(),
            artboards,
        }
    }
}

impl Document {
    pub fn add_layer(&mut self, layer: Layer) -> LayerId {
        let id = self.layer_arena.insert(layer);
        self.layers.insert(0, id);
        id
    }

    pub fn remove_layer(&mut self, id: LayerId) -> Option<Layer> {
        let layer = self.layer_arena.remove(id)?;
        for node_id in &layer.nodes {
            self.node_arena.remove(*node_id);
        }
        self.layers.retain(|l| *l != id);
        Some(layer)
    }

    pub fn add_node(&mut self, layer_id: LayerId, node: Node) -> Option<NodeId> {
        let node_id = self.node_arena.insert(node);
        let layer = self.layer_arena.get_mut(layer_id)?;
        layer.nodes.push(node_id);
        Some(node_id)
    }

    pub fn remove_node(&mut self, id: NodeId) -> Option<Node> {
        let node = self.node_arena.remove(id)?;
        for layer in self.layer_arena.values_mut() {
            layer.nodes.retain(|n| *n != id);
        }
        Some(node)
    }

    /// Layer that owns a node, if any.
    pub fn layer_of(&self, node_id: NodeId) -> Option<LayerId> {
        for (lid, layer) in self.layer_arena.iter() {
            if layer.nodes.contains(&node_id) {
                return Some(lid);
            }
        }
        None
    }
}
