//use crate::api as gl;
use crate::api::types::*;
use crate::api::Gl;
use std::ptr;

//--------------------------------------------------------------------------------------------------

/// Copy + Clone to bypass a restriction of slotmap on stable rust.
#[derive(Copy, Clone, Debug)]
pub struct RawBuffer {
    pub obj: GLuint,
    pub size: usize,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct BufferDescription {
    pub size: usize,
}

//--------------------------------------------------------------------------------------------------
pub fn create_buffer(gl: &Gl, byte_size: usize, flags: GLenum, initial_data: Option<&[u8]>) -> GLuint {
    let mut obj: GLuint = 0;
    unsafe {
        gl.CreateBuffers(1, &mut obj);
        gl.NamedBufferStorage(
            obj,
            byte_size as isize,
            if let Some(data) = initial_data {
                data.as_ptr() as *const GLvoid
            } else {
                ptr::null()
            },
            flags,
        );
    }

    obj
}