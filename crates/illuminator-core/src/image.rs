//! Raster image node.
//!
//! `ImageData` owns the *original* encoded bytes (PNG / JPEG / WEBP / BMP) — we
//! never re-encode lossily. The renderer decodes on first use and caches a
//! texture handle keyed by `hash`.

use std::hash::{Hash, Hasher};

use glam::DVec2;
use serde::{Deserialize, Serialize};

use crate::style::Style;

#[derive(Debug, thiserror::Error)]
pub enum ImageDecodeError {
    #[error("could not decode image: {0}")]
    Decode(#[from] image::ImageError),
    #[error("unrecognised image format")]
    UnknownFormat,
}

/// Raw, serializable image payload. Bytes round-trip through base64 in JSON.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImageData {
    #[serde(with = "base64_bytes")]
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// Stable across save/load (recomputed in [`crate::io::load_document`]).
    /// Used as texture-cache key by the renderer.
    #[serde(skip)]
    pub hash: u64,
}

impl ImageData {
    /// Decode just enough to validate format and read dimensions; stores the
    /// original bytes for both later renders and saving back.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, ImageDecodeError> {
        let format = image::guess_format(&bytes).map_err(ImageDecodeError::Decode)?;
        let _ = format; // accepted formats are bounded by the `image` features we enable
        let img = image::load_from_memory(&bytes)?;
        let width = img.width();
        let height = img.height();
        let hash = hash_bytes(&bytes);
        Ok(Self { bytes, width, height, hash })
    }

    pub fn recompute_hash(&mut self) {
        self.hash = hash_bytes(&self.bytes);
    }
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

/// An axis-aligned raster image placement on the canvas.
///
/// `position` is the top-left in world space; `size` is the placed size, also
/// in world units. Defaults are 1:1 mapping from image pixel dimensions.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImageNode {
    pub position: DVec2,
    pub size: DVec2,
    #[serde(default)]
    pub style: Style,
    pub image: ImageData,
}

impl ImageNode {
    pub fn bounds(&self) -> (DVec2, DVec2) {
        let min = self.position;
        let max = self.position + self.size;
        (min.min(max), min.max(max))
    }
}

mod base64_bytes {
    use base64::{engine::general_purpose, Engine as _};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(bytes: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        let b64 = general_purpose::STANDARD.encode(bytes);
        b64.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        general_purpose::STANDARD
            .decode(&s)
            .map_err(serde::de::Error::custom)
    }
}

