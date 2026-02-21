#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rectangle<T = f32> {
    /// X coordinate of the top-left corner.
    pub x: T,

    /// Y coordinate of the top-left corner.
    pub y: T,

    /// Width of the rectangle.
    pub width: T,

    /// Height of the rectangle.
    pub height: T,
}

/// An amount of space in 2 dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Size<T = f32> {
    /// The width.
    pub width: T,
    /// The height.
    pub height: T,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Default)]
pub struct Hasher(twox_hash::XxHash64);

#[cfg(not(target_arch = "wasm32"))]
impl core::hash::Hasher for Hasher {
    fn write(&mut self, bytes: &[u8]) {
        self.0.write(bytes)
    }

    fn finish(&self) -> u64 {
        self.0.finish()
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Debug, Default)]
pub struct Hasher(std::collections::hash_map::DefaultHasher);

#[cfg(target_arch = "wasm32")]
impl core::hash::Hasher for Hasher {
    fn write(&mut self, bytes: &[u8]) {
        core::hash::Hasher::write(&mut self.0, bytes)
    }

    fn finish(&self) -> u64 {
        core::hash::Hasher::finish(&self.0)
    }
}
