//! Immutable decoded straight-alpha RGBA8 images shared between scene nodes.
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
pub(crate) fn next_identity() -> Result<u64, &'static str> {
    NEXT_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .map_err(|_| "image identity exhausted")
}
#[derive(Debug)]
pub struct ImageData {
    id: u64,
    width: u32,
    height: u32,
    pixels: Arc<std::sync::OnceLock<Arc<[u8]>>>,
    procedural: Option<Arc<ProceduralImage>>,
    transform: crate::affine::Affine,
}
/// A compute shader producing premultiplied RGBA8, with a lazy straight-alpha
/// fallback for software consumers. Binding 0 is a read-only f32 storage buffer;
/// binding 1 is an rgba8unorm storage texture. One invocation covers one cell.
pub struct ProceduralImage {
    pub shader: &'static str,
    pub parameters: Vec<f32>,
    pub dispatch: [u32; 2],
    fallback: Box<dyn Fn() -> Arc<[u8]> + Send + Sync>,
}
impl std::fmt::Debug for ProceduralImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProceduralImage")
    }
}
impl ImageData {
    pub fn new(
        width: u32,
        height: u32,
        pixels: impl Into<Arc<[u8]>>,
    ) -> Result<Self, &'static str> {
        let pixels = pixels.into();
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or("image size overflow")?;
        if width == 0 || height == 0 || pixels.len() != expected {
            return Err("image requires nonzero dimensions and width * height * 4 RGBA bytes");
        }
        let id = next_identity()?;
        Ok(Self {
            id,
            width,
            height,
            pixels: Arc::new(std::sync::OnceLock::from(pixels)),
            procedural: None,
            transform: crate::affine::Affine::IDENTITY,
        })
    }
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn pixels(&self) -> &[u8] {
        self.pixels
            .get_or_init(|| (self.procedural.as_ref().unwrap().fallback)())
    }
    pub fn procedural(&self) -> Option<&ProceduralImage> {
        self.procedural.as_deref()
    }
    pub fn compute(
        width: u32,
        height: u32,
        shader: &'static str,
        parameters: Vec<f32>,
        dispatch: [u32; 2],
        fallback: impl Fn() -> Arc<[u8]> + Send + Sync + 'static,
    ) -> Result<Self, &'static str> {
        if width == 0 || height == 0 || dispatch.contains(&0) {
            return Err("nonzero image dimensions required");
        }
        Ok(Self {
            id: next_identity()?,
            width,
            height,
            pixels: Arc::new(std::sync::OnceLock::new()),
            procedural: Some(Arc::new(ProceduralImage {
                shader,
                parameters,
                dispatch,
                fallback: Box::new(fallback),
            })),
            transform: crate::affine::Affine::IDENTITY,
        })
    }
    /// Share decoded pixels and GPU upload identity with a different paint
    /// transformation. The matrix acts around the allocated image's center;
    /// it does not change intrinsic size or surrounding layout. Non-finite
    /// matrices reset to identity. Singular matrices paint and hit-test empty.
    pub fn transformed(&self, transform: crate::affine::Affine) -> Self {
        Self {
            id: self.id,
            width: self.width,
            height: self.height,
            pixels: self.pixels.clone(),
            procedural: self.procedural.clone(),
            transform: if transform.is_finite() {
                transform
            } else {
                crate::affine::Affine::IDENTITY
            },
        }
    }
    pub fn transform(&self) -> crate::affine::Affine {
        self.transform
    }
    pub fn paint_transform(&self, bounds: crate::scene::Rect) -> crate::affine::Affine {
        if self.transform == crate::affine::Affine::IDENTITY {
            return self.transform;
        }
        self.transform.around(
            bounds.x + bounds.width * 0.5,
            bounds.y + bounds.height * 0.5,
        )
    }
    pub fn paint_bounds(&self, bounds: crate::scene::Rect) -> crate::scene::Rect {
        if self.transform == crate::affine::Affine::IDENTITY {
            return bounds;
        }
        if self.transform.inverse().is_none() {
            return crate::scene::Rect::default();
        }
        self.paint_transform(bounds).bounds(bounds)
    }
}
impl PartialEq for ImageData {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.transform == other.transform
    }
}
impl Eq for ImageData {}
