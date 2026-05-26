use egui::Pos2;
use glam::DVec2;
use illuminator_core::doc::NodeId;
use illuminator_core::path::Anchor;

/// What the user is currently doing on the canvas. Transient — never serialized.
#[derive(Clone, Debug)]
pub enum Interaction {
    None,
    /// Middle-mouse or space-drag panning. Not currently entered into — pan
    /// input is short-circuited in [`crate::canvas`] for simplicity — kept here
    /// for when pan needs persistent state (e.g. inertia).
    #[allow(dead_code)]
    Panning { last_screen: Pos2 },
    /// Drawing a rectangle/ellipse by dragging.
    DrawingShape {
        kind: ShapeKind,
        start_world: DVec2,
        current_world: DVec2,
    },
    /// Drawing an artboard by dragging.
    DrawingArtboard {
        start_world: DVec2,
        current_world: DVec2,
    },
    /// Selection rubber-band.
    Marquee {
        start_screen: Pos2,
        current_screen: Pos2,
    },
    /// Dragging whole selected nodes (Select tool).
    MovingSelection {
        last_world: DVec2,
        accumulated: DVec2,
    },
    /// Dragging individual anchors of a path (Direct Select tool).
    MovingAnchors {
        items: Vec<(NodeId, usize)>,
        last_world: DVec2,
        accumulated: DVec2,
    },
    /// Resizing a single image by dragging one of its corners. The opposite
    /// corner stays pinned in world space.
    ResizingImage {
        node_id: NodeId,
        corner: Corner,
        /// World-position of the *opposite* corner (the pinned one).
        anchor_world: DVec2,
        original_position: DVec2,
        original_size: DVec2,
        /// Image pixel aspect (width / height) — used for Shift-locking.
        aspect: f64,
        original_anchors: Option<Vec<Anchor>>,
    },
    /// Dragging a single handle endpoint (in or out) of an anchor.
    MovingHandle {
        node_id: NodeId,
        anchor_idx: usize,
        which: HandlePart,
        /// Original handle vectors before drag — used to detect cumulative delta.
        start_in: DVec2,
        start_out: DVec2,
        /// Whether the anchor was `smooth` at drag start (we restore on undo).
        was_smooth: bool,
        /// If true, the two handles move independently (Alt held at drag start).
        break_symmetry: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandlePart {
    In,
    Out,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corner {
    NW,
    NE,
    SE,
    SW,
}

impl Corner {
    /// World position of *this* corner of an AABB given its (min, max).
    pub fn world_pos(self, min: DVec2, max: DVec2) -> DVec2 {
        match self {
            Corner::NW => DVec2::new(min.x, min.y),
            Corner::NE => DVec2::new(max.x, min.y),
            Corner::SE => DVec2::new(max.x, max.y),
            Corner::SW => DVec2::new(min.x, max.y),
        }
    }

    pub fn opposite(self) -> Self {
        match self {
            Corner::NW => Corner::SE,
            Corner::NE => Corner::SW,
            Corner::SE => Corner::NW,
            Corner::SW => Corner::NE,
        }
    }
}

impl Default for Interaction {
    fn default() -> Self { Self::None }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeKind {
    Rect,
    Ellipse,
}
