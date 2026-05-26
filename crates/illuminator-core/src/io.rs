use std::path::Path;

use crate::doc::{Document, NodeKind, CURRENT_VERSION};

#[derive(Debug, thiserror::Error)]
pub enum IoError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported document version: {0} (this build supports up to {CURRENT_VERSION})")]
    UnsupportedVersion(u32),
    #[error("SVG error: {0}")]
    Svg(String),
}

/// Write a document to disk as pretty-printed JSON. Format: `.ilm`.
pub fn save_document(doc: &Document, path: &Path) -> Result<(), IoError> {
    let json = serde_json::to_string_pretty(doc)?;
    std::fs::write(path, json)?;
    tracing::info!(?path, "saved document");
    Ok(())
}

pub fn load_document(path: &Path) -> Result<Document, IoError> {
    let bytes = std::fs::read(path)?;
    let mut doc: Document = serde_json::from_slice(&bytes)?;
    if doc.version > CURRENT_VERSION {
        return Err(IoError::UnsupportedVersion(doc.version));
    }
    rehash_images(&mut doc);
    tracing::info!(?path, version = doc.version, "loaded document");
    Ok(doc)
}

/// Image hashes are `#[serde(skip)]`'d (we don't trust persisted values to be
/// consistent across hasher versions). Recompute from bytes after load so the
/// renderer's texture cache works.
fn rehash_images(doc: &mut Document) {
    for node in doc.node_arena.values_mut() {
        if let NodeKind::Image(img) = &mut node.kind {
            img.image.recompute_hash();
        }
    }
}
