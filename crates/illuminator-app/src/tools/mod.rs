#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolKind {
    Select,
    DirectSelect,
    Pen,
    Text,
    Rectangle,
    Ellipse,
    Hand,
    Artboard,
}

impl ToolKind {
    pub fn label(self) -> &'static str {
        match self {
            ToolKind::Select => "Select",
            ToolKind::DirectSelect => "Direct Select",
            ToolKind::Pen => "Pen",
            ToolKind::Text => "Text",
            ToolKind::Rectangle => "Rectangle",
            ToolKind::Ellipse => "Ellipse",
            ToolKind::Hand => "Hand",
            ToolKind::Artboard => "Artboard",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            ToolKind::Select => "▶",
            ToolKind::DirectSelect => "▷",
            ToolKind::Pen => "✒",
            ToolKind::Text => "T",
            ToolKind::Rectangle => "▭",
            ToolKind::Ellipse => "◯",
            ToolKind::Hand => "✋",
            ToolKind::Artboard => "◰",
        }
    }

    pub fn shortcut(self) -> egui::Key {
        match self {
            ToolKind::Select => egui::Key::V,
            ToolKind::DirectSelect => egui::Key::A,
            ToolKind::Pen => egui::Key::P,
            ToolKind::Text => egui::Key::T,
            ToolKind::Rectangle => egui::Key::M,
            ToolKind::Ellipse => egui::Key::L,
            ToolKind::Hand => egui::Key::H,
            ToolKind::Artboard => egui::Key::O,
        }
    }

    pub fn all() -> &'static [ToolKind] {
        &[
            ToolKind::Select,
            ToolKind::DirectSelect,
            ToolKind::Pen,
            ToolKind::Text,
            ToolKind::Rectangle,
            ToolKind::Ellipse,
            ToolKind::Hand,
            ToolKind::Artboard,
        ]
    }
}
