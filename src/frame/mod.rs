use std::cell::{Cell, Ref, RefCell, RefMut};
use std::fs::File;
use std::io::{stdout, Write};
use std::mem;
use std::ptr;

use ash::vk;
use downcast_rs::Downcast;
use petgraph::{
    graph::{EdgeIndex, NodeIndex},
    visit::EdgeRef,
    Directed, Direction, Graph,
};
use sid_vec::{Id, IdVec};

use context::Context;
use resource::*;

mod alloc;
mod dependency;
mod dump;
mod graphviz;
mod resource;
mod sched;

pub mod compute;
pub mod graphics;
pub mod present;
pub mod transfer;

pub use self::sched::ScheduleOptimizationProfile;

use self::compute::{ComputeTask, ComputeTaskBuilder};
use self::dependency::*;
use self::graphics::{GraphicsTask, GraphicsTaskBuilder};
use self::present::{PresentTask, PresentTaskBuilder};
use self::resource::*;
use self::transfer::TransferTask;

//--------------------------------------------------------------------------------------------------
pub(crate) type TaskId = NodeIndex<u32>;
pub(crate) type DependencyId = EdgeIndex<u32>;
/// The frame graph type.
pub(crate) type FrameGraphInner = Graph<Task, Dependency, Directed, u32>;
pub(crate) struct FrameGraph(pub(crate) Graph<Task, Dependency, Directed, u32>);

/// Represents an operation in the frame graph.
#[derive(Debug)]
pub(crate) struct Task {
    /// Task name.
    pub(crate) name: String,
    /// On which queue this task is going to execute.
    /// If `None`, the task does not care.
    pub(crate) queue: u32,
    /// Type of workload
    pub(crate) details: TaskDetails,
}

#[derive(Debug)]
pub(crate) struct RayTracingTask {}

#[derive(Debug)]
pub(crate) enum TaskDetails {
    Graphics(GraphicsTask),
    Compute(ComputeTask),
    Transfer(TransferTask),
    Present(PresentTask),
    RayTracing(RayTracingTask),
    Other,
}

//--------------------------------------------------------------------------------------------------
/// Represents one output of a task.
/// This is used as part of the API to build dependencies between nodes.
pub struct TaskOutputRef<T> {
    /// ID of the resource in the frame resource table.
    pub id: T,
    /// Originating task.
    pub task: TaskId,
    /// What pipeline stage must have completed on the dependency.
    src_stage_mask: vk::PipelineStageFlags,
    /// Whether this resource has already been set as a read dependency.
    /// Prevents all writes.
    read: Cell<bool>,
    /// Whether this resource has already been set as a write dependency.
    /// Prevents all subsequent reads and writes.
    written: Cell<bool>,
    /// Estimated time for the resource to be ready (can vary between different usages).
    latency: u32,
}

impl<T> TaskOutputRef<T> {
    pub(crate) fn set_write(&self) -> Result<(), ()> {
        if self.read.get() {
            return Err(());
        }
        if self.written.get() {
            return Err(());
        }
        self.written.set(true);
        Ok(())
    }

    pub(crate) fn set_read(&self) -> Result<(), ()> {
        if self.written.get() {
            return Err(());
        }
        self.read.set(true);
        Ok(())
    }
}

pub type ImageRef = TaskOutputRef<ImageId>;
pub type BufferRef = TaskOutputRef<BufferId>;
pub type AttachmentRef = TaskOutputRef<AttachmentId>;

//--------------------------------------------------------------------------------------------------
pub(crate) struct RenderPass {
    attachments: IdVec<AttachmentIndex, ImageId>,
    attachments_desc: IdVec<AttachmentIndex, vk::AttachmentDescription>,
    tasks: Vec<TaskId>,
}

impl RenderPass {
    fn new() -> RenderPass {
        RenderPass {
            attachments: IdVec::new(),
            attachments_desc: IdVec::new(),
            tasks: Vec::new(),
        }
    }

    fn add_attachment(&mut self, img: ImageId, desc: vk::AttachmentDescription) -> AttachmentIndex {
        self.attachments.push(img);
        self.attachments_desc.push(desc)
    }
}

//--------------------------------------------------------------------------------------------------
pub struct RenderPassTag;
pub type RenderPassId = Id<RenderPassTag, u32>;
type RenderPasses = IdVec<RenderPassId, RenderPass>;

/*impl RenderPasses
{
    fn new() -> RenderPasses {
        RenderPasses {
            renderpasses: IdVec::new()
        }
    }

    /// Creates a renderpass.
    pub fn new_renderpass(&mut self) -> RenderPassId {
        self.renderpasses.push(RenderPass::new())
    }

    /// Adds an attachment to the specified renderpass.
    pub fn add_renderpass_attachment(
        &mut self,
        renderpass: RenderPassId,
        img: ImageId,
        desc: vk::AttachmentDescription,
    ) -> AttachmentIndex {
        self.renderpasses[renderpass].add_attachment(img, desc)
    }

    /// Gets the description of the specified renderpass.
    pub fn renderpass_attachment_desc(&self, renderpass: RenderPassId, attachment_index: AttachmentIndex) -> &vk::AttachmentDescription
    {
        &self.renderpasses[renderpass].attachments_desc[attachment_index]
    }

    /// Gets the description of the specified renderpass.
    pub fn renderpass_attachment_desc_mut(&mut self, renderpass: RenderPassId, attachment_index: AttachmentIndex) -> &mut vk::AttachmentDescription
    {
        &mut self.renderpasses[renderpass].attachments_desc[attachment_index]
    }
}*/

//--------------------------------------------------------------------------------------------------
impl FrameGraph {
    fn new() -> FrameGraph {
        FrameGraph(Graph::new())
    }

    /// Creates a new task that will execute on the specified queue.
    /// Returns the ID to the newly created task.
    fn create_task_on_queue(
        &mut self,
        name: impl Into<String>,
        queue: u32,
        details: TaskDetails,
    ) -> TaskId {
        self.0.add_node(Task {
            name: name.into(),
            queue,
            details,
        })
    }

    /// Adds or updates a dependency between two tasks in the graph.
    fn add_dependency(&mut self, src: TaskId, dst: TaskId, dependency: Dependency) -> DependencyId {
        // look for an already existing dependency
        if let Some(edge) = self.0.find_edge(src, dst) {
            let dep = self.0.edge_weight_mut(edge).unwrap();

            match (&mut dep.barrier, &dependency.barrier) {
                // buffer barrier
                (
                    &mut BarrierDetail::Buffer(ref mut barrier_a),
                    &BarrierDetail::Buffer(ref barrier_b),
                )
                    if barrier_a.id == barrier_b.id =>
                {
                    dep.src_stage_mask |= dependency.src_stage_mask;
                    dep.dst_stage_mask |= dependency.dst_stage_mask;
                    barrier_a.src_access_mask |= barrier_b.src_access_mask;
                    barrier_a.dst_access_mask |= barrier_b.dst_access_mask;
                    dep.latency = dep.latency.max(dependency.latency);
                    return edge;
                }
                // image barrier
                (
                    &mut BarrierDetail::Image(ref mut barrier_a),
                    &BarrierDetail::Image(ref barrier_b),
                )
                    if barrier_a.id == barrier_b.id =>
                {
                    dep.src_stage_mask |= dependency.src_stage_mask;
                    dep.dst_stage_mask |= dependency.dst_stage_mask;
                    barrier_a.src_access_mask |= barrier_b.src_access_mask;
                    barrier_a.dst_access_mask |= barrier_b.dst_access_mask;
                    // must be a compatible layout
                    dep.latency = dep.latency.max(dependency.latency);
                    return edge;
                }
                // FIXME subpass barrier on an attachment reference: merge with existing dependency?
                _ => {}
            }
        }

        // new dependency
        self.0.add_edge(src, dst, dependency)
    }

    /// Adds a sequencing constraint between two nodes.
    /// A sequencing constraint does not involve any resource.
    fn add_sequence_dependency(&mut self, src: TaskId, dst: TaskId) -> DependencyId {
        self.add_dependency(
            src,
            dst,
            Dependency {
                src_stage_mask: vk::PIPELINE_STAGE_TOP_OF_PIPE_BIT, // ignored
                dst_stage_mask: vk::PIPELINE_STAGE_BOTTOM_OF_PIPE_BIT,
                barrier: BarrierDetail::Sequence,
                latency: 0, // FIXME not sure...
            },
        )
    }

    /// Updates the "destination access mask" field of an image dependency.
    /// Panics if `dependency` is not an image dependency.
    fn add_image_barrier_access_flags(&mut self, dependency: DependencyId, flags: vk::AccessFlags) {
        self.0
            .edge_weight_mut(dependency)
            .unwrap()
            .as_image_barrier_mut()
            .unwrap()
            .dst_access_mask |= flags;
    }
    /// Collects all tasks using this resource but that do not produce another version of it.
    fn collect_last_uses_of_image(&self, img: ImageId) -> Vec<TaskId> {
        let uses = self
            .0
            .node_indices()
            .filter(|n| {
                // is the resource used in an incoming dependency?
                let incoming = self
                    .0
                    .edges_directed(*n, Direction::Incoming)
                    .any(|e| e.weight().get_image_id() == Some(img));
                // does not appear in any outgoing dependency
                let outgoing = self
                    .0
                    .edges_directed(*n, Direction::Outgoing)
                    .any(|e| e.weight().get_image_id() == Some(img));

                incoming && !outgoing
            }).collect::<Vec<_>>();

        uses
    }

    /// Collects all tasks using this resource but that do not produce another version of it.
    fn collect_last_uses_of_buffer(&self, buf: BufferId) -> Vec<TaskId> {
        let uses = self
            .0
            .node_indices()
            .filter(|n| {
                // is the resource used in an incoming dependency?
                let incoming = self
                    .0
                    .edges_directed(*n, Direction::Incoming)
                    .any(|e| e.weight().get_buffer_id() == Some(buf));
                // does not appear in any outgoing dependency
                let outgoing = self
                    .0
                    .edges_directed(*n, Direction::Outgoing)
                    .any(|e| e.weight().get_buffer_id() == Some(buf));

                incoming && !outgoing
            }).collect::<Vec<_>>();

        uses
    }
}

//--------------------------------------------------------------------------------------------------

/// A frame: manages transient resources within a frame.
pub struct Frame<'ctx> {
    pub(crate) context: &'ctx mut Context,
    /// The DAG of tasks.
    pub(crate) graph: FrameGraph,
    /// Resource table.
    pub(crate) resources: Resources<'ctx>,
    /// Table of renderpasses.
    pub(crate) renderpasses: RenderPasses,
}

//--------------------------------------------------------------------------------------------------
// Frame implementation

impl<'ctx> Frame<'ctx> {
    /// Creates a new frame.
    fn new(context: &'ctx mut Context) -> Frame<'ctx> {
        let mut f = Frame {
            graph: FrameGraph::new(),
            context,
            resources: Resources::new(),
            renderpasses: IdVec::new(),
        };
        f
    }

    /// Creates a present task.
    /// The input must be an image of the swapchain.
    pub fn present(&mut self, img: &ImageRef) -> TaskId {
        let queue = self.context.present_queue;
        let mut builder = PresentTaskBuilder::new("present", &mut self.graph, &mut self.resources);
        builder.present(img);
        builder.finish()
    }

    /// Creates a new task.
    /// Returns the ID to the newly created task.
    pub fn graphics_subpass<S, R, F>(
        &mut self,
        name: S,
        renderpass: RenderPassId,
        setup: F,
    ) -> (TaskId, R)
    where
        S: Into<String>,
        F: FnOnce(&mut GraphicsTaskBuilder) -> R,
    {
        let mut builder = GraphicsTaskBuilder::new(
            name,
            renderpass,
            &mut self.graph,
            &mut self.resources,
            &mut self.renderpasses,
        );
        let r = setup(&mut builder);
        let t = builder.finish();
        (t, r)
    }

    /// Creates a new task.
    /// Returns the ID to the newly created task.
    pub fn compute_task<S, R, F>(&mut self, name: S, setup: F) -> (TaskId, R)
    where
        S: Into<String>,
        F: FnOnce(&mut ComputeTaskBuilder) -> R,
    {
        let mut builder = ComputeTaskBuilder::new(name, &mut self.graph, &mut self.resources);
        let r = setup(&mut builder);
        let t = builder.finish();
        (t, r)
    }

    /// Updates the data contained in a texture. This creates a task in the graph.
    /// This does not synchronize: the data to be modified is first uploaded into a
    /// staging buffer first.
    fn update_image(&mut self, img: &ImageRef, data: ()) -> ImageRef {
        unimplemented!()
    }

    /// Creates a new renderpass.
    pub fn new_renderpass(&mut self) -> RenderPassId {
        self.renderpasses.push(RenderPass::new())
    }

    /*/// Gets the dimensions of the image (width, height, depth).
    pub fn get_image_dimensions(&self, img: ImageId) -> (u32, u32, u32) {
        self.images[img.0 as usize].dimensions()
    }

    /// Gets the dimensions of the image.
    pub fn get_image_format(&self, img: ImageId) -> vk::Format {
        self.images[img.0 as usize].format()
    }*/

    /*/// Gets the dimensions of the image.
    pub fn get_image_desc(&self, )*/

    /// Creates a task that has a dependency on all the specified tasks.
    fn make_sequence_task(&mut self, name: impl Into<String>, tasks: &[TaskId]) -> TaskId {
        // create the sync task
        let dst_task = self.graph.create_task_on_queue(name, 0, TaskDetails::Other);
        for &src_task in tasks.iter() {
            self.graph.add_sequence_dependency(src_task, dst_task);
        }
        dst_task
    }

    pub fn create_image(
        &mut self,
        name: impl Into<String>,
        create_info: vk::ImageCreateInfo,
    ) -> ImageId {
        self.resources.create_image(name, create_info)
    }

    /// Creates a transient 2D image.
    /// The initial layout of the image is inferred from its usage in depending tasks.
    fn create_image_2d(
        &mut self,
        name: impl Into<String>,
        (width, height): (u32, u32),
        format: vk::Format,
    ) -> ImageId {
        let desc = vk::ImageCreateInfo {
            s_type: vk::StructureType::ImageCreateInfo,
            p_next: ptr::null(),
            flags: vk::ImageCreateFlags::default(),
            image_type: vk::ImageType::Type2d,
            format,
            extent: vk::Extent3D {
                width,
                height,
                depth: 1,
            },
            mip_levels: 1,                         // FIXME
            array_layers: 1,                       // FIXME
            samples: vk::SAMPLE_COUNT_1_BIT,       // FIXME
            tiling: vk::ImageTiling::Optimal,      // FIXME
            usage: vk::ImageUsageFlags::default(), // inferred from the graph
            sharing_mode: vk::SharingMode::Concurrent,
            queue_family_index_count: 0,
            p_queue_family_indices: ptr::null(),
            initial_layout: vk::ImageLayout::Undefined,
        };
        self.create_image(name, desc)
    }

    /// Imports a persistent image for use in the frame graph.
    pub fn import_image(&mut self, img: &'ctx Image) -> ImageRef {
        let task = self
            .graph
            .create_task_on_queue("import", 0, TaskDetails::Other);
        let id = self.resources.add_imported_image(img);
        ImageRef {
            id,
            read: Cell::new(false),
            written: Cell::new(false),
            task,
            src_stage_mask: vk::PIPELINE_STAGE_BOTTOM_OF_PIPE_BIT, // FIXME too conservative?
            latency: 0,
        }
    }

    pub fn submit(mut self) {
        // TODO
        self.dump(&mut stdout());
        let ordering = self.schedule(ScheduleOptimizationProfile::MaximizeAliasing);
        let mut dot = File::create("graph.dot").unwrap();
        self.dump_graphviz(&mut dot, Some(&ordering), false);
    }
}

//--------------------------------------------------------------------------------------------------
// Context

impl Context {
    /// Creates a frame.
    /// The frame handles the scheduling of all GPU operations, synchronization between
    /// command queues, and synchronization with the CPU.
    pub fn new_frame(&mut self) -> Frame {
        Frame::new(self)
    }
}
