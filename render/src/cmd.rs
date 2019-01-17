use crate::sync::*;
use crate::{
    interface::{PipelineInterface, PipelineInterfaceVisitor},
    BufferTypeless, DescriptorSet, Framebuffer, GraphicsPipeline, Image, IndexFormat,
    RendererBackend, ScissorRect, Swapchain, Viewport,
};
use derivative::Derivative;
use std::ops::Range;
use crate::interface::VertexBufferDescriptor;
use crate::interface::IndexBufferDescriptor;

/// Represents a command to be executed by the renderer backend.
/// Before being sent to the backend, all commands are collected into a single array, and then
/// sorted accorded to their `sortkey`. This sort is stable,
/// so if two commands in a command buffer have the same sortkey, the order of insertion is kept.
/// However, commands with the same sorting key from different command buffers
/// can end up interleaved.
pub struct Command<'a, R: RendererBackend> {
    pub sortkey: u64,
    pub cmd: CommandInner<'a, R>,
}

// Explicit clone impl because of #26925
impl<'a, R: RendererBackend> Clone for Command<'a, R> {
    fn clone(&self) -> Self {
        Command {
            cmd: self.cmd.clone(),
            sortkey: self.sortkey,
        }
    }
}

/*
pub struct CmdSetVertexBuffers<'a, R: RendererBackend> {
    count: usize,
    buffers: [&'a R::Buffer]
}*/

// command header(with sort key), followed by subcommands (state-change commands)

/// Parameters for non-indexed draw commands.
#[derive(Copy, Clone, Debug)]
pub struct DrawParams {
    pub vertex_count: u32,
    pub instance_count: u32,
    pub first_vertex: u32,
    pub first_instance: u32,
}

/// Parameters for indexed draw commands.
#[derive(Copy, Clone, Debug)]
pub struct DrawIndexedParams {
    pub index_count: u32,
    pub instance_count: u32,
    pub first_index: u32,
    pub vertex_offset: i32,
    pub first_instance: u32,
}

#[derive(Derivative)]
#[derivative(Clone(bound = ""))]
pub enum CommandInner<'a, R: RendererBackend> {
    // MAIN (LEAD-IN) COMMANDS ---------------------------------------------------------------------
    PipelineBarrier {},
    ClearImageFloat {
        image: Image<'a, R>,
        color: [f32; 4],
    },
    ClearDepthStencilImage {
        image: Image<'a, R>,
        depth: f32,
        stencil: Option<u8>,
    },
    Present {
        image: Image<'a, R>,
        swapchain: Swapchain<'a, R>,
    },
    DrawHeader {
        pipeline: GraphicsPipeline<'a, R>,
    },

    // STATE CHANGE COMMANDS -----------------------------------------------------------------------
    SetDescriptorSets {
        descriptor_sets: Vec<DescriptorSet<'a, R>>,
    },
    SetFramebuffer {
        framebuffer: Framebuffer<'a, R>,
    },
    SetVertexBuffers {
        vertex_buffers: Vec<BufferTypeless<'a, R>>,
    },
    SetIndexBuffer {
        index_buffer: BufferTypeless<'a, R>,
        offset: usize,
        ty: IndexFormat,
    },
    SetScissors {
        //first: u32,
        scissors: Vec<ScissorRect>,
    },
    SetViewports {
        //first: u32,
        viewports: Vec<Viewport>,
    },

    // DRAW (LEAD-OUT) COMMANDS --------------------------------------------------------------------
    Draw {
        vertex_count: u32,
        instance_count: u32,
        first_vertex: u32,
        first_instance: u32,
    },
    DrawIndexed {
        index_count: u32,
        instance_count: u32,
        first_index: u32,
        vertex_offset: i32,
        first_instance: u32,
    },
}
/*
// Explicit clone impl because of #26925
impl<'a, R: RendererBackend> Clone for CommandInner<'a, R> {
    fn clone(&self) -> Self {
        // The initial implementation was `unsafe { mem::transmute_copy(self) }`
        // and making sure that no variants have destructors.
        // I proptly forgot about this last point and put a Vec in a variant,
        // which led to a very hard to debug use-after-free.
        // So
        match *self {
            CommandInner::PipelineBarrier {} => CommandInner::PipelineBarrier {},
            CommandInner::ClearImageFloat { image, color } => {
                CommandInner::ClearImageFloat { image, color }
            }
            CommandInner::ClearDepthStencilImage {
                image,
                depth,
                stencil,
            } => CommandInner::ClearDepthStencilImage {
                image,
                depth,
                stencil,
            },
            CommandInner::Present { image, swapchain } => {
                CommandInner::Present { image, swapchain }
            }
            CommandInner::DrawHeader { pipeline } => CommandInner::DrawHeader { pipeline },

            CommandInner::SetDescriptorSets {
                ref descriptor_sets,
            } => CommandInner::SetDescriptorSets {
                descriptor_sets: descriptor_sets.clone(),
            },
            CommandInner::SetFramebuffer { framebuffer } => {
                CommandInner::SetFramebuffer { framebuffer }
            }
            CommandInner::SetVertexBuffers { ref vertex_buffers } => {
                CommandInner::SetVertexBuffers {
                    vertex_buffers: vertex_buffers.clone(),
                }
            }
            CommandInner::SetIndexBuffer {
                index_buffer,
                offset,
                ty,
            } => CommandInner::SetIndexBuffer {
                index_buffer,
                offset,
                ty,
            },
            CommandInner::SetScissors { ref scissors } => CommandInner::SetScissors {
                scissors: scissors.clone(),
            },
            //CommandInner::SetAllScissors { scissor } => CommandInner::SetAllScissors { scissor },
            CommandInner::SetViewports { ref viewports } => CommandInner::SetViewports {
                viewports: viewports.clone(),
            },
            // CommandInner::SetAllViewports { viewport } => {
            //    CommandInner::SetAllViewports { viewport }
            //}
            CommandInner::Draw {
                vertex_count,
                instance_count,
                first_vertex,
                first_instance,
            } => CommandInner::Draw {
                vertex_count,
                instance_count,
                first_vertex,
                first_instance,
            },

            CommandInner::DrawIndexed {
                index_count,
                instance_count,
                first_index,
                vertex_offset,
                first_instance,
            } => CommandInner::DrawIndexed {
                index_count,
                instance_count,
                first_index,
                vertex_offset,
                first_instance,
            },
        }
    }
}
*/

/// Command buffers contain a list of commands.
pub struct CommandBuffer<'a, R: RendererBackend> {
    commands: Vec<Command<'a, R>>,
}

/// API exposed by command buffers.
/// Can build multiple command buffers concurrently in different threads.
impl<'a, R: RendererBackend> CommandBuffer<'a, R> {
    pub(super) fn new() -> CommandBuffer<'a, R> {
        CommandBuffer {
            commands: Vec::new(),
        }
    }

    fn push_command(&mut self, sortkey: u64, cmd: CommandInner<'a, R>) {
        self.commands.push(Command { cmd, sortkey })
    }

    // fn self.push_header_command(sortkey)
    // fn self.push_trailing_command()

    pub fn iter(&self) -> impl Iterator<Item = &Command<'a, R>> {
        self.commands.iter()
    }

    //----------------------------------------------------------------------------------------------
    // Manual sync

    /// Inserts an explicit pipeline barrier.
    pub fn pipeline_barrier(
        &mut self,
        _sort_key: u64,
        _src: PipelineStageFlags,
        _dst: PipelineStageFlags,
        _memory_barriers: &[MemoryBarrier<R>],
    ) {
        unimplemented!()
    }

    //----------------------------------------------------------------------------------------------
    // Allocate

    //----------------------------------------------------------------------------------------------
    // Copy

    /// Copy data between buffers.
    pub fn copy_buffer(
        &mut self,
        _sort_key: u64,
        _src: BufferTypeless<'a, R>,
        _dst: BufferTypeless<'a, R>,
        _src_range: Range<u64>,
        _dst_range: Range<u64>,
    ) {
        unimplemented!()
    }

    //----------------------------------------------------------------------------------------------
    // Clear

    /// Clears an image.
    pub fn clear_image(&mut self, sortkey: u64, image: Image<'a, R>, color: &[f32; 4]) {
        self.push_command(
            sortkey,
            CommandInner::ClearImageFloat {
                image,
                color: *color,
            },
        )
    }

    /// Clears an image.
    pub fn clear_depth_stencil_image(
        &mut self,
        sortkey: u64,
        image: Image<'a, R>,
        depth: f32,
        stencil: Option<u8>,
    ) {
        self.push_command(
            sortkey,
            CommandInner::ClearDepthStencilImage {
                image,
                depth,
                stencil,
            },
        )
    }

    //----------------------------------------------------------------------------------------------
    // Draw

    fn set_descriptor_sets<I: IntoIterator<Item=DescriptorSet<'a,R>>>(&mut self, sortkey: u64, descriptor_sets: I) {
        self.push_command(
            sortkey,
            CommandInner::SetDescriptorSets {
                descriptor_sets: descriptor_sets.into_iter().collect(),
            },
        )
    }

    fn set_framebuffer(&mut self, sortkey: u64, framebuffer: Framebuffer<'a, R>) {
        self.push_command(sortkey, CommandInner::SetFramebuffer { framebuffer })
    }

    fn set_vertex_buffers<'tcx, I: IntoIterator<Item=VertexBufferDescriptor<'a,'tcx,R>>>(&mut self, sortkey: u64, vertex_buffers: I) {
        self.push_command(
            sortkey,
            CommandInner::SetVertexBuffers {
                vertex_buffers: vertex_buffers.into_iter().map(|d| d.buffer).collect(),
            },
        )
    }

    fn set_index_buffer(
        &mut self,
        sortkey: u64,
        index_buffer: BufferTypeless<'a, R>,
        offset: usize,
        ty: IndexFormat,
    ) {
        self.push_command(
            sortkey,
            CommandInner::SetIndexBuffer {
                index_buffer,
                offset,
                ty,
            },
        )
    }

    fn set_viewports<I: IntoIterator<Item=Viewport>>(&mut self, sortkey: u64, viewports: I) {
        self.push_command(
            sortkey,
            CommandInner::SetViewports {
                viewports: viewports.into_iter().collect(),
            },
        )
    }

    fn set_scissors<I: IntoIterator<Item=ScissorRect>>(&mut self, sortkey: u64, scissors: I) {
        self.push_command(
            sortkey,
            CommandInner::SetScissors {
                scissors: scissors.into_iter().collect(),
            },
        )
    }

    fn bind_pipeline_interface<PI: PipelineInterface<'a, R>>(
        &mut self,
        sortkey: u64,
        pipeline: GraphicsPipeline<'a, R>,
        interface: &PI,
    ) {
        self.push_command(sortkey, CommandInner::DrawHeader { pipeline });

        struct Visitor<'a, 'b, R: RendererBackend> {
            sortkey: u64,
            cmdbuf: &'b mut CommandBuffer<'a, R>,
        }

        impl<'a, 'b, R: RendererBackend> PipelineInterfaceVisitor<'a, R> for Visitor<'a, 'b, R> {
            fn visit_descriptor_sets<I: IntoIterator<Item=DescriptorSet<'a,R>>>(&mut self, descriptor_sets: I) {
                self.cmdbuf
                    .set_descriptor_sets(self.sortkey, descriptor_sets);
            }

            fn visit_vertex_buffers<'tcx, I: IntoIterator<Item=VertexBufferDescriptor<'a,'tcx,R>>>(&mut self, vertex_buffers: I)
            {
                self.cmdbuf.set_vertex_buffers(self.sortkey, vertex_buffers);
            }

            fn visit_index_buffer(&mut self, buffer: IndexBufferDescriptor<'a, R>)
            {
                self.cmdbuf
                    .set_index_buffer(self.sortkey, buffer.buffer, buffer.offset as usize, buffer.format);
            }

            fn visit_framebuffer(&mut self, framebuffer: Framebuffer<'a, R>) {
                self.cmdbuf.set_framebuffer(self.sortkey, framebuffer);
            }

            fn visit_dynamic_viewports<I: IntoIterator<Item=Viewport>>(&mut self, viewports: I) {
                self.cmdbuf.set_viewports(self.sortkey, viewports);
            }

            fn visit_dynamic_scissors<I: IntoIterator<Item=ScissorRect>>(&mut self, scissors: I) {
                self.cmdbuf.set_scissors(self.sortkey, scissors);
            }
        }

        let mut v = Visitor {
            sortkey,
            cmdbuf: self,
        };

        interface.do_visit(&mut v);
    }

    pub fn draw<PI: PipelineInterface<'a, R>>(
        &mut self,
        sortkey: u64,
        pipeline: GraphicsPipeline<'a, R>,
        interface: &PI,
        params: DrawParams,
    ) {
        self.bind_pipeline_interface(sortkey, pipeline, interface);
        self.push_command(
            sortkey,
            CommandInner::Draw {
                vertex_count: params.vertex_count,
                instance_count: params.instance_count,
                first_vertex: params.first_vertex,
                first_instance: params.first_instance,
            },
        );
    }

    pub fn draw_indexed<PI: PipelineInterface<'a, R>>(
        &mut self,
        sortkey: u64,
        pipeline: GraphicsPipeline<'a, R>,
        interface: &PI,
        params: DrawIndexedParams,
    ) {
        self.bind_pipeline_interface(sortkey, pipeline, interface);
        self.push_command(
            sortkey,
            CommandInner::DrawIndexed {
                index_count: params.index_count,
                instance_count: params.instance_count,
                first_index: params.first_index,
                vertex_offset: params.vertex_offset,
                first_instance: params.first_instance,
            },
        );
    }

    //----------------------------------------------------------------------------------------------
    // Present

    /// Presents the specified image to the swapchain.
    /// Might incur a copy / blit or format conversion if necessary.
    pub fn present(&mut self, sortkey: u64, image: Image<'a, R>, swapchain: Swapchain<'a, R>) {
        self.push_command(sortkey, CommandInner::Present { image, swapchain })
    }
}

/// TODO optimize (radix sort, dense command buffer layout, separate index map)
pub fn sort_command_buffers<'a, R: RendererBackend>(
    cmdbufs: Vec<CommandBuffer<'a, R>>,
) -> Vec<Command<'a, R>> {
    let mut fused = Vec::new();
    //let mut sortkeys = Vec::new();
    //let mut i: usize = 0;
    for cmdbuf in cmdbufs.iter() {
        for cmd in cmdbuf.commands.iter() {
            fused.push(cmd.clone());
            //sortkeys.push(cmd.sortkey);
        }
    }

    fused.sort_by(|cmd_a, cmd_b| cmd_a.sortkey.cmp(&cmd_b.sortkey));
    fused
}
