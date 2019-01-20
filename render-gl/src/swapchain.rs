use autograph_render::traits;
use std::fmt;

/// Trait implemented by objects that can act as a swapchain.
///
/// OpenGL does not have the concept of "swapchains": this is typically handled by the
/// underlying window system. This type wraps around window handles and provides an interface
/// for getting the size of the swapchain (default framebuffer) and present an image to the screen
/// (swap buffers).
pub trait SwapchainInner: Send + Sync {
    fn size(&self) -> (u32, u32);
    fn present(&self);
}

/// Represents an OpenGL "swapchain".
pub(crate) struct GlSwapchain {
    pub(crate) inner: Box<dyn SwapchainInner>,
}

impl fmt::Debug for GlSwapchain {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(f, "Swapchain {{..}}")
    }
}

impl traits::Swapchain for GlSwapchain {
    fn size(&self) -> (u32, u32) {
        self.inner.size()
    }
}
