use anyhow::{Context, Result};
use arboard::Clipboard;
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};
use sha2::{Digest, Sha256};
use std::sync::{Arc, RwLock};

#[derive(Clone)]
pub struct CachedImage {
    pub hash: [u8; 32],
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub type ClipboardCache = Arc<RwLock<Option<CachedImage>>>;

pub fn new_cache() -> ClipboardCache {
    Arc::new(RwLock::new(None))
}

pub fn read_clipboard(cache: &ClipboardCache) -> Result<Option<CachedImage>> {
    let mut clipboard = Clipboard::new().context("failed to open clipboard")?;

    let img = match clipboard.get_image() {
        Ok(img) => img,
        Err(arboard::Error::ContentNotAvailable) => return Ok(None),
        Err(e) => return Err(anyhow::anyhow!("clipboard error: {e}")),
    };

    let rgba_bytes = img.bytes.as_ref();
    let width = img.width as u32;
    let height = img.height as u32;

    let hash: [u8; 32] = Sha256::digest(rgba_bytes).into();

    // Check cache
    {
        let cached = cache.read().unwrap();
        if let Some(ref c) = *cached {
            if c.hash == hash {
                return Ok(Some(c.clone()));
            }
        }
    }

    // Encode to PNG
    let mut png_buf = Vec::new();
    let encoder = PngEncoder::new(&mut png_buf);
    encoder
        .write_image(rgba_bytes, width, height, ExtendedColorType::Rgba8)
        .context("failed to encode PNG")?;

    let cached_image = CachedImage {
        hash,
        png: png_buf,
        width,
        height,
    };

    // Update cache
    {
        let mut cached = cache.write().unwrap();
        *cached = Some(cached_image.clone());
    }

    Ok(Some(cached_image))
}
