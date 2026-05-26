//! Concrete [`Command`] implementations.
//!
//! Tools never mutate the document directly — they construct one of these and
//! push it onto the [`CommandStack`].

use glam::DVec2;
use illuminator_core::command::Command;
use illuminator_core::doc::{Document, LayerId, Node, NodeId, NodeKind};
use illuminator_core::path::{Anchor, Path as IPath, PathNode};
use illuminator_core::style::{Color, Paint, Stroke};

/// Insert one or more fully-formed `Node`s onto a layer. Undo removes all of them.
pub struct AddNodeCmd {
    pub layer_id: LayerId,
    pending: Vec<Node>,
    inserted_ids: Vec<NodeId>,
    label: String,
}

impl AddNodeCmd {
    pub fn new(layer_id: LayerId, node: Node, label: impl Into<String>) -> Self {
        Self {
            layer_id,
            pending: vec![node],
            inserted_ids: Vec::new(),
            label: label.into(),
        }
    }

    pub fn with_mirrors(layer_id: LayerId, node: Node, mirrors: Vec<Node>, label: impl Into<String>) -> Self {
        let mut pending = vec![node];
        pending.extend(mirrors);
        Self {
            layer_id,
            pending,
            inserted_ids: Vec::new(),
            label: label.into(),
        }
    }

    #[allow(dead_code)]
    pub fn inserted_id(&self) -> Option<NodeId> {
        self.inserted_ids.first().copied()
    }
}

impl Command for AddNodeCmd {
    fn apply(&mut self, doc: &mut Document) {
        if !self.pending.is_empty() {
            let nodes = std::mem::take(&mut self.pending);
            for node in nodes {
                if let Some(id) = doc.add_node(self.layer_id, node) {
                    self.inserted_ids.push(id);
                }
            }
        }
    }

    fn undo(&mut self, doc: &mut Document) {
        let ids = std::mem::take(&mut self.inserted_ids);
        for id in ids {
            if let Some(node) = doc.remove_node(id) {
                self.pending.push(node);
            }
        }
        self.pending.reverse();
    }

    fn label(&self) -> &str {
        &self.label
    }
}

/// Translate selected nodes by a delta. Move tool commits this on mouseup.
pub struct MoveNodesCmd {
    pub node_ids: Vec<NodeId>,
    pub delta: DVec2,
}

impl MoveNodesCmd {
    pub fn new(node_ids: Vec<NodeId>, delta: DVec2) -> Self {
        Self { node_ids, delta }
    }

    fn translate(doc: &mut Document, ids: &[NodeId], delta: DVec2) {
        for id in ids {
            if let Some(node) = doc.node_arena.get_mut(*id) {
                node.translate(delta);
            }
        }
    }
}

impl Command for MoveNodesCmd {
    fn apply(&mut self, doc: &mut Document) {
        Self::translate(doc, &self.node_ids, self.delta);
    }

    fn undo(&mut self, doc: &mut Document) {
        Self::translate(doc, &self.node_ids, -self.delta);
    }

    fn label(&self) -> &str { "Move" }
}

/// Translate specific anchors within paths. Direct Select tool commits this
/// on mouseup after a live-preview drag.
pub struct MoveAnchorsCmd {
    pub items: Vec<(NodeId, usize)>,
    pub delta: DVec2,
}

impl MoveAnchorsCmd {
    pub fn new(items: Vec<(NodeId, usize)>, delta: DVec2) -> Self {
        Self { items, delta }
    }

    fn translate(doc: &mut Document, items: &[(NodeId, usize)], delta: DVec2) {
        for (node_id, idx) in items {
            let Some(node) = doc.node_arena.get_mut(*node_id) else { continue };
            if let NodeKind::Path(p) = &mut node.kind {
                if let Some(a) = p.path.anchors.get_mut(*idx) {
                    a.pos += delta;
                }
            }
        }
    }
}

impl Command for MoveAnchorsCmd {
    fn apply(&mut self, doc: &mut Document) {
        Self::translate(doc, &self.items, self.delta);
    }

    fn undo(&mut self, doc: &mut Document) {
        Self::translate(doc, &self.items, -self.delta);
    }

    fn label(&self) -> &str { "Move Anchor" }
}

/// Change an anchor's in/out handle offsets (Direct Select handle drag).
///
/// Tracks the delta from drag-start so undo restores exactly. Also records
/// the smooth flag so we can flip it back if the user broke symmetry.
pub struct EditHandleCmd {
    pub node_id: NodeId,
    pub anchor_idx: usize,
    pub in_delta: DVec2,
    pub out_delta: DVec2,
    /// `(was, became)` if the smooth flag changed during the drag.
    pub smooth_change: Option<(bool, bool)>,
}

impl EditHandleCmd {
    pub fn new(node_id: NodeId, anchor_idx: usize, in_delta: DVec2, out_delta: DVec2) -> Self {
        Self {
            node_id,
            anchor_idx,
            in_delta,
            out_delta,
            smooth_change: None,
        }
    }

    pub fn with_smooth_change(mut self, was: bool, became: bool) -> Self {
        if was != became {
            self.smooth_change = Some((was, became));
        }
        self
    }

    fn apply_offsets(&self, doc: &mut Document, in_d: DVec2, out_d: DVec2, smooth: Option<bool>) {
        let Some(node) = doc.node_arena.get_mut(self.node_id) else { return };
        let NodeKind::Path(p) = &mut node.kind else { return };
        let Some(a) = p.path.anchors.get_mut(self.anchor_idx) else { return };
        a.in_handle += in_d;
        a.out_handle += out_d;
        if let Some(s) = smooth {
            a.smooth = s;
        }
    }
}

impl Command for EditHandleCmd {
    fn apply(&mut self, doc: &mut Document) {
        let smooth = self.smooth_change.map(|(_, became)| became);
        self.apply_offsets(doc, self.in_delta, self.out_delta, smooth);
    }
    fn undo(&mut self, doc: &mut Document) {
        let smooth = self.smooth_change.map(|(was, _)| was);
        self.apply_offsets(doc, -self.in_delta, -self.out_delta, smooth);
    }
    fn label(&self) -> &str { "Edit Handle" }
}

/// Resize an image (position + size). Live-previewed; recorded on release.
pub struct ResizeImageCmd {
    pub node_id: NodeId,
    pub from_position: DVec2,
    pub from_size: DVec2,
    pub to_position: DVec2,
    pub to_size: DVec2,
}

impl ResizeImageCmd {
    pub fn new(
        node_id: NodeId,
        from_position: DVec2,
        from_size: DVec2,
        to_position: DVec2,
        to_size: DVec2,
    ) -> Self {
        Self {
            node_id,
            from_position,
            from_size,
            to_position,
            to_size,
        }
    }
}

impl Command for ResizeImageCmd {
    fn apply(&mut self, doc: &mut Document) {
        let Some(node) = doc.node_arena.get_mut(self.node_id) else { return };
        let NodeKind::Image(img) = &mut node.kind else { return };
        img.position = self.to_position;
        img.size = self.to_size;
    }
    fn undo(&mut self, doc: &mut Document) {
        let Some(node) = doc.node_arena.get_mut(self.node_id) else { return };
        let NodeKind::Image(img) = &mut node.kind else { return };
        img.position = self.from_position;
        img.size = self.from_size;
    }
    fn label(&self) -> &str { "Resize Image" }
}

/// Delete nodes. Stores removed nodes so undo can restore them (note: under
/// a new id; selection that referenced the old id must be cleared on undo).
pub struct DeleteNodesCmd {
    pub node_ids: Vec<NodeId>,
    removed: Vec<(LayerId, Node)>,
    label: String,
}

impl DeleteNodesCmd {
    pub fn new(node_ids: Vec<NodeId>) -> Self {
        Self {
            node_ids,
            removed: Vec::new(),
            label: String::from("Delete"),
        }
    }
}

impl Command for DeleteNodesCmd {
    fn apply(&mut self, doc: &mut Document) {
        self.removed.clear();
        for id in &self.node_ids {
            let Some(layer_id) = doc.layer_of(*id) else { continue };
            if let Some(node) = doc.remove_node(*id) {
                self.removed.push((layer_id, node));
            }
        }
    }

    fn undo(&mut self, doc: &mut Document) {
        for (layer_id, node) in self.removed.drain(..) {
            doc.add_node(layer_id, node);
        }
    }

    fn label(&self) -> &str { &self.label }
}

/// A command that translates multiple selected nodes to align or distribute them.
pub struct AlignDistributeCmd {
    pub translations: Vec<(NodeId, DVec2)>,
    pub label: String,
}

impl AlignDistributeCmd {
    pub fn new(translations: Vec<(NodeId, DVec2)>, label: impl Into<String>) -> Self {
        Self {
            translations,
            label: label.into(),
        }
    }
}

impl Command for AlignDistributeCmd {
    fn apply(&mut self, doc: &mut Document) {
        for (id, delta) in &self.translations {
            if let Some(node) = doc.node_arena.get_mut(*id) {
                node.translate(*delta);
            }
        }
    }

    fn undo(&mut self, doc: &mut Document) {
        for (id, delta) in &self.translations {
            if let Some(node) = doc.node_arena.get_mut(*id) {
                node.translate(-*delta);
            }
        }
    }

    fn label(&self) -> &str {
        &self.label
    }
}

/// A command that outlines a path's stroke, converting it into a closed filled path.
pub struct OutlineStrokeCmd {
    pub node_id: NodeId,
    pub original_kind: Option<NodeKind>,
    pub outlined_kind: Option<NodeKind>,
}

impl OutlineStrokeCmd {
    pub fn new(node_id: NodeId) -> Self {
        Self {
            node_id,
            original_kind: None,
            outlined_kind: None,
        }
    }
}

impl Command for OutlineStrokeCmd {
    fn apply(&mut self, doc: &mut Document) {
        if self.outlined_kind.is_some() {
            if let Some(node) = doc.node_arena.get_mut(self.node_id) {
                node.kind = self.outlined_kind.clone().unwrap();
            }
            return;
        }

        let Some(node) = doc.node_arena.get(self.node_id) else { return };
        let NodeKind::Path(pn) = &node.kind else { return };
        let stroke_width = pn.style.stroke.as_ref().map(|s| s.width).unwrap_or(2.0);
        let stroke_color = match pn.style.stroke.as_ref().map(|s| &s.paint) {
            Some(Paint::Solid(c)) => *c,
            _ => Color::BLACK,
        };

        // Cache the original
        self.original_kind = Some(node.kind.clone());

        // Perform Outline logic
        let anchors = &pn.path.anchors;
        let n = anchors.len();
        if n < 2 {
            return;
        }

        let mut out_anchors = Vec::new();
        let mut in_anchors = Vec::new();
        let half_w = stroke_width / 2.0;

        for i in 0..n {
            let pos = anchors[i].pos;

            // Compute normal at i
            let norm = if pn.path.closed {
                let prev = anchors[(i + n - 1) % n].pos;
                let next = anchors[(i + 1) % n].pos;
                let v1 = (pos - prev).normalize_or_zero();
                let v2 = (next - pos).normalize_or_zero();
                let mut n_dir = DVec2::new(-v1.y - v2.y, v1.x + v2.x);
                if n_dir.length_squared() < 1e-9 {
                    n_dir = DVec2::new(-v1.y, v1.x);
                }
                n_dir.normalize_or_zero()
            } else {
                if i == 0 {
                    let next = anchors[1].pos;
                    let v = (next - pos).normalize_or_zero();
                    DVec2::new(-v.y, v.x).normalize_or_zero()
                } else if i == n - 1 {
                    let prev = anchors[n - 2].pos;
                    let v = (pos - prev).normalize_or_zero();
                    DVec2::new(-v.y, v.x).normalize_or_zero()
                } else {
                    let prev = anchors[i - 1].pos;
                    let next = anchors[i + 1].pos;
                    let v1 = (pos - prev).normalize_or_zero();
                    let v2 = (next - pos).normalize_or_zero();
                    let mut n_dir = DVec2::new(-v1.y - v2.y, v1.x + v2.x);
                    if n_dir.length_squared() < 1e-9 {
                        n_dir = DVec2::new(-v1.y, v1.x);
                    }
                    n_dir.normalize_or_zero()
                }
            };

            out_anchors.push(Anchor::corner(pos + norm * half_w));
            in_anchors.push(Anchor::corner(pos - norm * half_w));
        }

        let mut final_anchors = out_anchors;
        in_anchors.reverse();
        final_anchors.extend(in_anchors);

        let outlined_path = IPath {
            anchors: final_anchors,
            closed: true,
        };

        let mut style = pn.style.clone();
        style.fill = Some(Paint::Solid(stroke_color));
        style.stroke = None;

        let new_pn = PathNode {
            transform: pn.transform,
            style,
            path: outlined_path,
        };

        let new_kind = NodeKind::Path(new_pn);
        self.outlined_kind = Some(new_kind.clone());

        if let Some(node) = doc.node_arena.get_mut(self.node_id) {
            node.kind = new_kind;
        }
    }

    fn undo(&mut self, doc: &mut Document) {
        if let Some(orig) = &self.original_kind {
            if let Some(node) = doc.node_arena.get_mut(self.node_id) {
                node.kind = orig.clone();
            }
        }
    }

    fn label(&self) -> &str {
        "Outline Stroke"
    }
}

/// Operations for computing path boolean logic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BooleanOp {
    Union,
    Intersect,
    Subtract,
}

/// A command that performs boolean logic (Union, Intersect, Subtract) on selected path bounds.
pub struct BooleanOpCmd {
    pub node_ids: Vec<NodeId>,
    pub op: BooleanOp,
    pub original_nodes: Vec<(NodeId, Node, LayerId)>,
    pub created_node_id: Option<NodeId>,
    pub created_node: Option<Node>,
    pub target_layer_id: Option<LayerId>,
}

impl BooleanOpCmd {
    pub fn new(node_ids: Vec<NodeId>, op: BooleanOp) -> Self {
        Self {
            node_ids,
            op,
            original_nodes: Vec::new(),
            created_node_id: None,
            created_node: None,
            target_layer_id: None,
        }
    }
}

impl Command for BooleanOpCmd {
    fn apply(&mut self, doc: &mut Document) {
        if self.created_node_id.is_some() {
            for (id, _, _) in &self.original_nodes {
                doc.remove_node(*id);
            }
            if let (Some(layer_id), Some(node)) = (self.target_layer_id, self.created_node.take()) {
                self.created_node_id = doc.add_node(layer_id, node);
            }
            return;
        }

        let mut rects = Vec::new();
        let mut target_layer = None;

        for id in &self.node_ids {
            if let Some(node) = doc.node_arena.get(*id) {
                if let Some((min, max)) = node.bounds() {
                    rects.push((min, max));
                    if target_layer.is_none() {
                        target_layer = doc.layer_of(*id);
                    }
                }
            }
        }

        if rects.len() < 2 || target_layer.is_none() {
            return;
        }

        let layer_id = target_layer.unwrap();
        self.target_layer_id = Some(layer_id);

        self.original_nodes.clear();
        for id in &self.node_ids {
            if let Some(node) = doc.node_arena.get(*id) {
                if let Some(lid) = doc.layer_of(*id) {
                    self.original_nodes.push((*id, node.clone(), lid));
                }
            }
        }

        let mut final_min = rects[0].0;
        let mut final_max = rects[0].1;

        match self.op {
            BooleanOp::Union => {
                for &(min, max) in &rects[1..] {
                    final_min = final_min.min(min);
                    final_max = final_max.max(max);
                }
            }
            BooleanOp::Intersect => {
                for &(min, max) in &rects[1..] {
                    final_min = final_min.max(min);
                    final_max = final_max.min(max);
                }
                if final_min.x >= final_max.x || final_min.y >= final_max.y {
                    return;
                }
            }
            BooleanOp::Subtract => {
                let r0_min = rects[0].0;
                let r0_max = rects[0].1;
                let r1_min = rects[1].0;
                let r1_max = rects[1].1;

                if r1_min.x >= r0_max.x || r1_max.x <= r0_min.x || r1_min.y >= r0_max.y || r1_max.y <= r0_min.y {
                    final_min = r0_min;
                    final_max = r0_max;
                } else {
                    if r1_min.x > r0_min.x {
                        final_min = r0_min;
                        final_max = DVec2::new(r1_min.x, r0_max.y);
                    } else if r1_max.x < r0_max.x {
                        final_min = DVec2::new(r1_max.x, r0_min.y);
                        final_max = r0_max;
                    } else if r1_min.y > r0_min.y {
                        final_min = r0_min;
                        final_max = DVec2::new(r0_max.x, r1_min.y);
                    } else if r1_max.y < r0_max.y {
                        final_min = DVec2::new(r0_min.x, r1_max.y);
                        final_max = r0_max;
                    } else {
                        for (id, _, _) in &self.original_nodes {
                            doc.remove_node(*id);
                        }
                        return;
                    }
                }
            }
        }

        let path = IPath::rectangle(final_min, final_max);
        let path_node = PathNode {
            transform: Default::default(),
            style: Default::default(),
            path,
        };
        let new_node = Node::path("Combined Shape", path_node);
        self.created_node = Some(new_node.clone());

        for (id, _, _) in &self.original_nodes {
            doc.remove_node(*id);
        }

        self.created_node_id = doc.add_node(layer_id, new_node);
    }

    fn undo(&mut self, doc: &mut Document) {
        if let Some(id) = self.created_node_id {
            self.created_node = doc.remove_node(id);
            self.created_node_id = None;
        }

        for (_id, node, layer_id) in &self.original_nodes {
            doc.add_node(*layer_id, node.clone());
        }
    }

    fn label(&self) -> &str {
        match self.op {
            BooleanOp::Union => "Union Shapes",
            BooleanOp::Intersect => "Intersect Shapes",
            BooleanOp::Subtract => "Subtract Shapes",
        }
    }
}

/// A command that resizes/scales a path node's anchors.
pub struct ResizePathCmd {
    pub node_id: NodeId,
    pub from_anchors: Vec<Anchor>,
    pub to_anchors: Vec<Anchor>,
}

impl ResizePathCmd {
    pub fn new(node_id: NodeId, from_anchors: Vec<Anchor>, to_anchors: Vec<Anchor>) -> Self {
        Self { node_id, from_anchors, to_anchors }
    }
}

impl Command for ResizePathCmd {
    fn apply(&mut self, doc: &mut Document) {
        if let Some(node) = doc.node_arena.get_mut(self.node_id) {
            if let NodeKind::Path(pn) = &mut node.kind {
                pn.path.anchors = self.to_anchors.clone();
            }
        }
    }

    fn undo(&mut self, doc: &mut Document) {
        if let Some(node) = doc.node_arena.get_mut(self.node_id) {
            if let NodeKind::Path(pn) = &mut node.kind {
                pn.path.anchors = self.from_anchors.clone();
            }
        }
    }

    fn label(&self) -> &str {
        "Scale Shape"
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use illuminator_core::path::{Path as IPath, Anchor, PathNode};
    use illuminator_core::style::Style;

    #[test]
    fn test_align_distribute_command() {
        let mut doc = Document::default();
        let layer_id = doc.layers[0];

        // Create a simple path node
        let path = IPath {
            anchors: vec![Anchor::corner(DVec2::new(10.0, 20.0))],
            closed: false,
        };
        let path_node = PathNode {
            transform: Default::default(),
            style: Style::default(),
            path,
        };
        let node_id = doc.add_node(layer_id, Node::path("Test Node", path_node)).unwrap();

        // Check initial pos
        {
            let node = doc.node_arena.get(node_id).unwrap();
            if let NodeKind::Path(pn) = &node.kind {
                assert_eq!(pn.path.anchors[0].pos, DVec2::new(10.0, 20.0));
            } else {
                panic!();
            }
        }

        // Apply translation cmd
        let mut cmd = AlignDistributeCmd::new(vec![(node_id, DVec2::new(5.0, -2.0))], "Align Test");
        cmd.apply(&mut doc);

        // Verify applied pos
        {
            let node = doc.node_arena.get(node_id).unwrap();
            if let NodeKind::Path(pn) = &node.kind {
                assert_eq!(pn.path.anchors[0].pos, DVec2::new(15.0, 18.0));
            } else {
                panic!();
            }
        }

        // Undo translation cmd
        cmd.undo(&mut doc);

        // Verify original pos
        {
            let node = doc.node_arena.get(node_id).unwrap();
            if let NodeKind::Path(pn) = &node.kind {
                assert_eq!(pn.path.anchors[0].pos, DVec2::new(10.0, 20.0));
            } else {
                panic!();
            }
        }
    }

    #[test]
    fn test_outline_stroke_command() {
        let mut doc = Document::default();
        let layer_id = doc.layers[0];

        let path = IPath {
            anchors: vec![
                Anchor::corner(DVec2::new(0.0, 0.0)),
                Anchor::corner(DVec2::new(10.0, 0.0)),
            ],
            closed: false,
        };
        let mut style = Style::default();
        style.stroke = Some(Stroke {
            paint: Paint::Solid(Color::rgb(1.0, 0.0, 0.0)),
            width: 4.0,
            ..Default::default()
        });
        style.fill = None;

        let path_node = PathNode {
            transform: Default::default(),
            style,
            path,
        };
        let node_id = doc.add_node(layer_id, Node::path("Line", path_node)).unwrap();

        let mut cmd = OutlineStrokeCmd::new(node_id);
        cmd.apply(&mut doc);

        let node = doc.node_arena.get(node_id).unwrap();
        if let NodeKind::Path(pn) = &node.kind {
            assert!(pn.style.fill.is_some());
            assert!(pn.style.stroke.is_none());
            assert_eq!(pn.path.anchors.len(), 4);
            assert!(pn.path.closed);
        } else {
            panic!();
        }

        cmd.undo(&mut doc);

        let node = doc.node_arena.get(node_id).unwrap();
        if let NodeKind::Path(pn) = &node.kind {
            assert!(pn.style.stroke.is_some());
            assert!(pn.style.fill.is_none());
            assert_eq!(pn.path.anchors.len(), 2);
            assert!(!pn.path.closed);
        } else {
            panic!();
        }
    }

    #[test]
    fn test_boolean_op_command() {
        let mut doc = Document::default();
        let layer_id = doc.layers[0];

        let r0 = PathNode {
            transform: Default::default(),
            style: Style::default(),
            path: IPath::rectangle(DVec2::new(0.0, 0.0), DVec2::new(10.0, 10.0)),
        };
        let id0 = doc.add_node(layer_id, Node::path("Rect 0", r0)).unwrap();

        let r1 = PathNode {
            transform: Default::default(),
            style: Style::default(),
            path: IPath::rectangle(DVec2::new(5.0, 5.0), DVec2::new(15.0, 15.0)),
        };
        let id1 = doc.add_node(layer_id, Node::path("Rect 1", r1)).unwrap();

        let mut cmd = BooleanOpCmd::new(vec![id0, id1], BooleanOp::Intersect);
        cmd.apply(&mut doc);

        assert!(doc.node_arena.get(id0).is_none());
        assert!(doc.node_arena.get(id1).is_none());
        
        let comb_id = cmd.created_node_id.unwrap();
        let node = doc.node_arena.get(comb_id).unwrap();
        if let NodeKind::Path(pn) = &node.kind {
            let (min, max) = pn.path.bounds().unwrap();
            assert!((min - DVec2::new(5.0, 5.0)).length() < 1e-4);
            assert!((max - DVec2::new(10.0, 10.0)).length() < 1e-4);
        } else {
            panic!();
        }

        cmd.undo(&mut doc);

        assert!(doc.node_arena.get(comb_id).is_none());
        assert_eq!(doc.node_arena.len(), 2);
    }

    #[test]
    fn test_resize_path_command() {
        let mut doc = Document::default();
        let layer_id = doc.layers[0];

        let path = IPath {
            anchors: vec![
                Anchor::corner(DVec2::new(10.0, 10.0)),
                Anchor::corner(DVec2::new(20.0, 20.0)),
            ],
            closed: false,
        };
        let path_node = PathNode {
            transform: Default::default(),
            style: Style::default(),
            path,
        };
        let node_id = doc.add_node(layer_id, Node::path("Line", path_node)).unwrap();

        let from_anchors = vec![
            Anchor::corner(DVec2::new(10.0, 10.0)),
            Anchor::corner(DVec2::new(20.0, 20.0)),
        ];
        let to_anchors = vec![
            Anchor::corner(DVec2::new(20.0, 20.0)),
            Anchor::corner(DVec2::new(40.0, 40.0)),
        ];

        let mut cmd = ResizePathCmd::new(node_id, from_anchors, to_anchors);
        cmd.apply(&mut doc);

        let node = doc.node_arena.get(node_id).unwrap();
        if let NodeKind::Path(pn) = &node.kind {
            assert_eq!(pn.path.anchors[0].pos, DVec2::new(20.0, 20.0));
            assert_eq!(pn.path.anchors[1].pos, DVec2::new(40.0, 40.0));
        } else {
            panic!();
        }

        cmd.undo(&mut doc);

        let node = doc.node_arena.get(node_id).unwrap();
        if let NodeKind::Path(pn) = &node.kind {
            assert_eq!(pn.path.anchors[0].pos, DVec2::new(10.0, 10.0));
            assert_eq!(pn.path.anchors[1].pos, DVec2::new(20.0, 20.0));
        } else {
            panic!();
        }
    }
}


