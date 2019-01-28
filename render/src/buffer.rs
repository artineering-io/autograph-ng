use crate::traits;
use crate::typedesc::PrimitiveType;
use crate::typedesc::TypeDesc;
use std::marker::PhantomData;
use std::mem;

pub struct BufferSlice<'a, R: RendererBackend> {
    pub buffer: &'a R::Buffer,
    pub offset: usize,
    pub size: usize,
}

//--------------------------------------------------------------------------------------------------

/// Marker trait for data that can be uploaded to a GPU buffer
pub trait BufferData: 'static {
    type Element;
    fn len(&self) -> usize;
}

impl<T: Copy + 'static> BufferData for T {
    type Element = T;
    fn len(&self) -> usize {
        1
    }
}

impl<U: BufferData> BufferData for [U] {
    type Element = U;
    fn len(&self) -> usize {
        (&self as &[U]).len()
    }
}
