use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const BLACK: Self = Self { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };
    pub const WHITE: Self = Self { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };
    pub const TRANSPARENT: Self = Self { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub fn to_array(self) -> [f32; 4] { [self.r, self.g, self.b, self.a] }
    pub fn from_array(a: [f32; 4]) -> Self { Self { r: a[0], g: a[1], b: a[2], a: a[3] } }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GradientStop {
    pub offset: f32, // 0.0 ..= 1.0
    pub color: Color,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Paint {
    Solid(Color),
    LinearGradient {
        start: glam::DVec2,
        end: glam::DVec2,
        stops: Vec<GradientStop>,
    },
    RadialGradient {
        center: glam::DVec2,
        radius: f64,
        stops: Vec<GradientStop>,
    },
}

impl Default for Paint {
    fn default() -> Self { Paint::Solid(Color::BLACK) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineCap { Butt, Round, Square }
impl Default for LineCap { fn default() -> Self { Self::Butt } }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineJoin { Miter, Round, Bevel }
impl Default for LineJoin { fn default() -> Self { Self::Miter } }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Stroke {
    pub paint: Paint,
    pub width: f64,
    pub cap: LineCap,
    pub join: LineJoin,
    #[serde(default)]
    pub dash: Option<Vec<f32>>,
}

impl Default for Stroke {
    fn default() -> Self {
        Self {
            paint: Paint::Solid(Color::BLACK),
            width: 1.0,
            cap: LineCap::default(),
            join: LineJoin::default(),
            dash: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Style {
    pub fill: Option<Paint>,
    pub stroke: Option<Stroke>,
    pub opacity: f32,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            fill: Some(Paint::Solid(Color::rgb(0.8, 0.8, 0.8))),
            stroke: Some(Stroke::default()),
            opacity: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
}

impl Default for BlendMode { fn default() -> Self { Self::Normal } }
