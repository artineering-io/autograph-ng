Renderer API notes
==================================

# Old design

### Resource allocation
* separate for each queue
* do not alias memory for now, but re-use

### Graph
* Node = pass type (graphics or compute), callback function
callback function parameters:
* command buffer, within a subpass instance initialized with the correct attachments
* container object that holds all requested resources in their correct state (all images, sampled images, buffer, storage buffer, uniform buffers, etc.)
The callback just has to submit the draw command.
* Edge = dependency
    * toposort
    * check for concurrent read/write hazards (ok by API design)
    * infer usage of resources from graphs (mutate graph)
- schedule renderpasses
- reorder to minimize layout transitions
- allocate resources with aliasing
- group in render passes (draw commands with the same attachments; notably: chains without `sample` dependencies)
     - new dependency type: attachment input
     - at least one subpass for each different attachment?
     - minimize the number of attachments
     - a sample dependency always breaks the render pass
- heuristic for renderpasses:
     - schedule a pass (starting from the first one)
     - follow all output attachments
     - if no successor has a sample-dependency, and NO OTHER ATTACHMENTS, then merge the successors into the renderpass
     - schedule_renderpasses()
     - create dependencies between subpasses (e.g. output-to-input attachment)
     - user-provided hints?
- schedule renderpasses (dependencies between renderpasses: e.g. layout transitions)
- insert memory barriers
- insert layout transitions
Various graphs:
- initial graph
- graph with render passes
- graph with render passes and explicit layout transitions

All work on the GPU is done inside nodes in a frame.
- DEVICE: Submission queue: when allocating a transient resource:
     - find a block to allocate, try to sync on semaphore,
	- if not yet signalled, then allocate a new block
     - if failed (transient memory limit exceeded), then sync on a suitable block
- DEVICE: Submission queue: when importing a resource: semaphore sync (may be in use by another queue)
- UPLOADS: fence sync on frame N-max_frames_in_flight
When using a resource, what to sync on?
- Associate semaphore to resource
Sometimes, a group of resources (Buffers, Images, Memory blocks) can share the same semaphore:
- SyncResourceGroup
A resource can belong to a SyncGroup? Rc<SyncGroup>? SyncGroupId?
The SyncGroup is assigned when? on submission?
all resources used during the construction of a command buffer should be recorded
```
context.sync_group(frame, command_buffer, |sync_resources| {
	// if resource
	sync_resources.use(...)
})
```
Next step: insert queue barriers
- for each node
     - for each dependency
         - if cross-queue dependency
             - if already syncing on a resource of the same queue that is finished later: do nothing
             - otherwise, if finished earlier: remove old sync from task and put it on the later task (allocate semaphore if needed)
- tasks have list of semaphores to signal, and semaphores to wait
Next step:
- traverse each queue subgraph and regroup tasks into 'jobs' that do not have any dependency on other queues
- for each job, collect wait/signal semaphores
- don't forget to add semaphores for external dependencies
- this is handled by the import tasks
- import tasks: what do they do?
     - execute on the specified queue
     - synchronizes with the previous frame (semaphores in the resource): adds semaphore to the job
     - be careful not to synchronize with the semaphore from 2 or 3 frames before!
- should also have export/exit nodes?
     - exit nodes for external resources: signal resource ready
     - automatic insertion of exit tasks
- for each job: ring buffer of MAX_FRAMES_IN_FLIGHT semaphores?
     - no need, I think, can reuse the same semaphores
     - same for external resources, AS LONG AS the wait is on the same queue
         - if the wait changes queues, then all hope is lost...
             - e.g. if the queue is empty, then the GPU might execute it immediately, but frame N-2 might not have finished
                 and may signal the semaphore, which will launch the task on the new queue with old data
         - solution: have multiple semaphores for external resources, always wait on semaphore for frame N-1 exit
Synchronization of external resources:
- issue: can change queues (don't assume that they are all used on the same queue)
- can read on a queue, read on the other
- exit tasks: put a semaphore in the command stream, to be waited on by the entry (import) task
Limitation:
- cannot sequence (multiple) reads followed by a write!
- maybe it's possible: return another versioned handle from a read
- or modify graph: R(0) -> T1 -> W(0) -> T4 will add an implicit dependency on T4 to T2,T3
                   R(0) -> T2
                   R(0) -> T3
  -> this assumes breadth-first execution...
   t1.read(r0);     // pending access = [t1] (into r0 ref)
   t2.read(r0);     // pending access = [t1,t2]
   t3.read(r0);     // pending access = [t1,t2,t3]
   // now r0 has three readers: reclaim exclusive access
   let r0 = frame.sync(r0);      // r0 is a fresh reference without any R/W count, contains an implicit dependency on t1, t2, t3
   -> insert a virtual task that depends on the three nodes (i.e. not a resource dependency)
   // o1 means write after o1, but what about o2 and o3? => must detect R/W hazard
   t4.write(o1);             // will sync on t1, but it's not enough
   -> OPTION: could force sequencing of reads, in addition to writes
   -> to write a resource, must sync on all pending reads
   -> SOLUTION: add special "sequence" dependencies
Next step: build command buffers
- for each job, create command buffer, traverse graph
Put everything in the graph, including present operations
- some nodes should only execute on a given queue (e.g. the present queue)
Transfer queue:
- upload data immediately to upload buffer
- on schedule: to transfer queue: copy to resource
DONE Do away with dummy nodes for resource creation:
- clutters the graph with useless nodes, confuses scheduling.
- initialize to the correct state on first use.
DONE Decouple dependency edges and usage of the resource within the task.
- A resource can have multiple usages within the same task.
     - e.g. color attachment and input attachment
- Dependency = only pipeline barrier info
Implicit dependencies between tasks with ordering
- user submitted ordering is important
- write after read is not an error, but will insert a pipeline barrier automatically
- same for read after write
-> ordering is defined implicitly by the submission order.
-> benefits: less cluttered API

### Images


Creating persistent images
High-level uses:
- Immutable + sampled (texture)
- Attachment + sampled (postproc)
- Attachment only
- CPU upload
- CPU readback

Low-level:
- usage flags
- queues


Memory types:

    VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT = 0x00000001
    VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT = 0x00000002
    VK_MEMORY_PROPERTY_HOST_COHERENT_BIT = 0x00000004
    VK_MEMORY_PROPERTY_HOST_CACHED_BIT = 0x00000008
    VK_MEMORY_PROPERTY_LAZILY_ALLOCATED_BIT = 0x00000010
    VK_MEMORY_PROPERTY_PROTECTED_BIT = 0x00000020
    
Queue flags:

    VK_QUEUE_GRAPHICS_BIT
    VK_QUEUE_COMPUTE_BIT
    VK_QUEUE_TRANSFER_BIT
    VK_QUEUE_SPARSE_BINDING_BIT
    VK_QUEUE_PROTECTED_BIT
    
Immutable => VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT
CPU upload => VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT(R) + VK_MEMORY_PROPERTY_HOST_COHERENT_BIT(P)

Q: exposed level of detail to the user
```
Dimensions::Dim2d { width: 1280, height: 720 },
MipmapsCount::One
HostAccess::NoAccess | Mappable
```

External API:
* `Context::create_image()` VS `Image::new(&context)`
    * Second one was preferred previously
    * Deallocation is a bit weird: `Image::destroy(&context)`
    * Benefits: less code in `Context`, more idiomatic, no need for different functions when creating specialized image types

Internal API:
* Image: no desc structure, but impl ImageDescription
* Image::new_uninitialized(): queue families
* Image::bind_memory()
* Q: should pools be exposed in the external API?
    * User might need them, and it forces a cleaner design for the internal API
* Q: How does allocating an image in a pool works?
    * Image becomes implicitly bound to the pool
    * Releasing the pool is unsafe
    * Options:
        * Pool strong-refs the image
        * Image strong-refs the pool
        * Count number of allocations and deallocations, panic if count is not zero at exit
        * Do nothing, deallocating the pool while images are still live is undefined behavior and unsafe
            * Cannot expose this API to the user
            * OR: dropping the pool does not release the memory
                * memory is released when the last image is deleted
* Q: How does allocating anything works?
    * vulkan spec says that all objects created with a device must be destroyed before destroying the device 
    * but the current API does not ensure that a resource will be destroyed before the device
        * Option 1: track number of allocated objects, panic if count not zero at drop time (**)
            * Gives no information about the leak...
            * Lightweight option
        * Option 2: extend lifetime of device with Arc<Device>

```
Image::new(..., Some(pool));
```
* Q: Does the image owns its allocation?

Note: the external API is quite high-level
* Still, make sure that the internal API is not too unsafe
* API issue: pooled resources
    * e.g. free all images at once
* Internal API issue: leaking owned handles
* The overhead of adding an Arc<Context> is negligible
    * still, don't add it if not absolutely necessary (prefer passing VkDevice or Context)
* The overhead for safety appears in other ways:
    * need to put something into the created object to ensure that it won't be deleted on the wrong parent object by mistake
        * marker indicating that it comes from some parent object
* Conclusion: putting a refcounted backpointer to the parent object is the easiest solution
    * must allocate context in an Arc
    * might as well rename context to device, for good measure
* To support polymorphism and strongly-typed resources, images should be Arc<Image>, and have an Image trait

Lifetime of memory allocations:
* Before deleting a pool, must be sure that all associated resources are destroyed, and not in use inside the pipeline, 
  and that no internal handles remain.
  
Aliasing of memory allocations:
* Can't alias a memory allocation if passed by value to the object
    * Optional reference to allocation
    
Basically, just copy vulkano (...) except that:
* all GPU commands are managed by a frame graph
    * notably, all resource access (except for initialization) must happen within the frame graph
* ???

Bikeshedding API
* Parameters vs structs
    * Use parameters as they support generics with less noise
* e.g. swapchains
* structs: indirection when using generic parameters

Images
* Base unsafe type, unbound memory
* Tagged image types
    * TransferSourceCapability
    * TransferDestinationCapability
    * SampleCapability
    * StorageCapability
    * ColorAttachmentCapability
    * DepthAttachmentCapability
    * AttachmentCapability
    * TransientAttachmentCapability
    * Format
    * SpecificFormat<T>
    * UnknownFormat
* Define tagged image types by combining capabilities
* impl on Image or on TagType?
    * on type tag, not instantiable
* image tags: 

 
```
image_tag!{
    pub type Transfer = TransferSourceCapability + TransferDestinationCapability;
}

fn new() -> Swapchain<ImageType> {
}
```

Sharing between queues:
* know in advance
* encode in type
* be conservative
* rule: do not expose sharing to the user
    * queue creation and scheduling is handled by the framework, with hints from the graph
* make choices:
    * presentation images are always EXCLUSIVE
    * persistent images are always CONCURRENT across all queue families (by default)

Image usage should be abstracted away:
* delayed allocation
* images implicitly bound to a frame graph?
* issue: delayed uploads

```
with_frame(|frame| {
    accum = Image(...); 
    
})

frame {
    image XXX { ... }
    image YYY { ... }
    
    pass A { attachment <- XXX }
}

```

Issue: what is the lifetime of a frame?
- recreated on every frame
    - can be costly (allocation, scheduling)
    - good fit for dynamic workloads (e.g stuff that is run once every two frames?)
- OR create once, execute many
    - less costly, reuse stuff
    - inflexible (borrows persistent resources forever)
    - need "input ports" for dynamic data (cannot borrow input data forever, avoid shared references)
    

Swapchain images **owned** by the swapchain
- acquire_swapchain_image() returns what?
    - Arc?
    - Borrow?
    - Value?

Question: how to store a reference to the image when used within a frame?
    - API: expects Arc, store Arc in frame, guard with fence
    - API: expects Image, but clone internal Arc
    - Might be a reference to an image, but also indirectly to a swapchain
    - Do not store a reference, just check for GPU use on drop
A: take a copy of a generic image, or just the raw handle
    
Q: expose Images through Arc<> or through naked objects?
    - naked objects are possible, with "frame borrows"
        - frame requests that image lives as long as the current frame index
        - when image is dropped
            - check that frame is finished through device backref
            - if not, move object into deferred deletion list in device
        - need non-owning images

### Q: vkAcquireNextImageKHR should be called as late as possible. 

This raises an issue with the frame graph, which needs to call it back when generating the buffers.
- Borrow the swapchain image
    - ergonomics loss
- Turn the swapchain image into a generic "image reference"/"image proxy" that can be acquired at any moment in the future
    - impl IntoImageProxy for SwapchainImage
    - impl IntoImageProxy for GenericImage
    - Must decouple borrow (the resources must live for the current frame) from GPU lock 
        (wait for this semaphore before using the resource, signal this one when finished)
    - ImageProxies are just another name for a borrow...
    - Issue: cannot set some state into the borrowed resource during the frame
        - Notably, cannot remember the layout that the image is into when exiting a frame
        - Cannot remember anything across frames
            - Layout is one thing, but then again the initial layout has no reason to change across frames
            - anything else? except the data inside the image, can't think of anything
        - Other solution: remove image and imageproxies, just use single trait image, with impl FrameSynchronizedObject for Arc<Image>, borrow with Arcs

- Special-case swapchain images in ImageResource
    - `fn swapchain(&self) -> Option<...>`
    - calls underlying swapchainimageproxy
    - remove ImageProxy trait (just query the image directly for non-swapchains)
    
- (extreme) Build the command buffers on the fly
    - a.k.a do not pursue the frame graph approach
        
### Q: FrameGraphs vs on-the-fly command buffer generation?
FrameGraphs: full knowledge of the structure and dependencies inside the frame. Can reorder and schedule.

On-the-fly: 
- No reordering possible. 
- Must schedule explicitly or schedule with incomplete information.
- Aliasing of resources is still possible. 
- May be faster (no scheduling, no graph allocation, commands directly put into buffers)
- Just-in-time synchronization
- This is (mostly) an internal aspect, and should not change the API much: keep FrameGraph approach for now.
    
        
### Q: Scheduling

- Scheduling now happens per-task: each task is responsible for scheduling itself
- A task may output a command into a command buffer, or a queue operation directly (e.g. vkQueuePresentKHR), or both
    (e.g. layout transition + queue operation)
- all passes that belong to the same renderpass must be scheduled in the same command buffer
- guarantees when calling task::schedule
    - all resources are properly synchronized
- tasks should signal the context that they expect
    - renderpass(index)
    - command buffer
    - queue
    - then tasks can get the context they want: queue(), command_buffer(), wait_semaphores() ...
- operations:
    - TaskOperation::SubpassCommand()
    - TaskOperation::Command
    - TaskOperation::QueueSubmit(command buffer)
    - TaskOperation::QueuePresentKHR(...)
- TaskContext:
    - CommandBuffer(...)
    - RenderPass(...)
    - Queue(...)
- Expose a 'virtual queue' that makes no distinction between renderpass, command buffer, or queue ops
    - issue: cannot perform *any* synchronization within a task, even manual ones
        - is this OK?
        - no: provide raw access to queues
            
### Q: texture uploads
- should happen outside frames
- problem: lifetime of staging buffer?
    - staging buffer should be frame-bound
    - but upload could happen outside a frame
- problem: uploading very large amounts of texture data in one go:
    - upload blocks on frame finish, but the frame has not even started yet
    - can still upload in a frame, one time
- solution: create "temporary" frame for upload
    - frames do not need to correspond one-to-one with frames on the screen
    - is that true?
        - what about frames in flight?
        - distinguish between visual frames & non-visual frames?
- submit command buffer for initial upload to transfer queue, then set initial semaphore
    
Q: redesign image refs
    - more ergonomic: reference to image resource entry, with current state in the graph
        - issue: borrows the whole frame, must refcell everything
        - partial borrows would be nice

### Target API
- simple
    - drop the need to store resource versions: use ordering of commands
    - Read-after-write scenarios
        - a task may call another task that modifies an input resource, and the calling task reads the new resource
        as if it was not modified
            - prevented by handle rename
            - can be prevented by read-only handles, or &mut ref
- straightforward
- familiar
- concise
- prevents wrong usage
- use as few as possible rust-specific features
- importantly: does not interfere with data-driven scenarios
    - e.g. create graph from a file
- should be relatively low-level
    - higher level wrappers should be possible
    
### Internal API for dependencies
- should be able to specify one side of a dependency
    - semaphores to wait for
    - pipeline stage to wait for 
    
### Q: Expose render passes or not?
- should not, probably
- must have a grouping pass:
    - separate pass on the graph, or during scheduling?
        - schedule pass
        - if same renderpass tag
            - schedule as subpass
        - if not: terminate renderpass, start new one
        - next one: next tasks in topological order
            - evaluate renderpass merge candidates (does not use any of the previous attachments as sampled or storage images)
            - set renderpass index
            - try to schedule from given score
            

### Schedule state: 
- schedule stack (which ones to try next)

### API for graphics:
- Variant A:
    ```
    fn set_color_attachment(index, image, load, store) -> ImageRef
    fn set_depth_attachment(image, load, store) -> ImageRef
    fn set_input_attachment(index, image)
    ```
    - Issue (set_color_attachment validation): color attachment is valid only if not read, or read by input attachment of the same task
        However, no way of knowing that the read is from the same task

- Variant B:
    ```
    fn set_color_attachment(index, image, load, store) -> ImageRef
    fn set_color_input_attachment(index, image, input_index, store) -> ImageRef
    fn set_depth_attachment(image, load, store) -> ImageRef
    fn set_input_attachment(index, image)
    ```
    - set combined color+input attachment at the same time
    - advantage vs Variant A: no need to modify ImageRef
  
- Variant C (index-less):
    ```
    fn add_color_attachment(image, load, store) -> ImageRef
    fn set_depth_attachment(image, load, store) -> ImageRef
    fn add_input_attachment(image)
    ```
    - does not work with combined color+input attachment
    
- Variant D:
    ```
    fn set_color_attachments(index, [{image, load, store}]) -> ???
    fn set_depth_attachment(image, load, store) -> ImageRef
    fn set_input_attachments(index, [image])
    ```
    - Issue: how to return new versions of color attachments? 
    - Issue: see option A, color attachment validation
    
- Variant B' (cosmetic):
    ```
    fn color_attachment(index, image, load, store) -> ImageRef
    fn color_input_attachment(index, image, input_index, store) -> ImageRef
    fn depth_attachment(image, load, store) -> ImageRef
    fn input_attachment(index, image)
    ```
    
- Variant E (two-phase):
    ```
    fn attachment(image, load, store) -> (AttachmentId, ImageRef)
    fn color_attachment(index, attachment_id)
    fn depth_attachment(attachment_id)
    fn input_attachment(attachment_id)
    ```
    - Potential API misuse: store AttachmentId outside subpass
    
- Variant E' (two-phase across subpasses: "AttachmentRef"):
    ```
    fn load_attachment(image, load, store) -> AttachmentRef
    fn color_attachment(index, att_ref) -> AttachmentRef
    fn depth_attachment(att_ref) -> AttachmentRef
    fn input_attachment(att_ref)
    ```
    
- Variant E'+B (combined color+input, two-phase across subpasses: "AttachmentRef"):
    ```
    fn load_attachment(image, load, store) -> AttachmentRef
    fn color_attachment(index, att_ref) -> AttachmentRef
    fn depth_attachment(att_ref) -> AttachmentRef
    fn input_attachment(att_ref)
    ```
    - does not work very well with data-driven scenarios?
   
- Variant Z: No API
    - use frame graph only for synchronization
    
    
    
    

    
# Redesign
    
### Global draw call sorting and ordering
- Order by ID
- ID takes into account queues, dependencies

### Redesign #3:
- three layers of functionality
    - synchronization (-> frame graph)
    - memory allocation (-> frame graph)
    - submission (-> submit)
- new modules
    - renderer (creation and deletion of resources)
        - vk (vulkan backend)
            - instance
        - pass (API-agnostic description of passes)
        - sync (frame graph, frame sync: API-agnostic)
        - submit (command buffer, state caching)
- Lightweight object handles

### Redesign #4: highly flexible pipeline
- Goals: allow complex appearances that locally modify scheduling / need allocation of resources
- a.k.a. efficient post-process materials
- a.k.a. scatter rendering commands everywhere / gather at the end
- add geometry dynamically based on GPU query results

- Scenario A (local post-proc):
    - See mesh with a particular material / object group that has not been culled
    - Create (or get) temp image for this material / object group
    - Render stuff into image
    - At the end (when all objects of this object group have been rendered), compose into current canvas
         - In some predefined order
         - Release temporary image
    - Challenges:
        - Dynamically schedule operations on another queue depending on query results
            - acquire temporary image: imgMesh00
            - draw (no constraint) into imgMesh00
            - if not scheduled yet, schedule submodule
                - get imgMesh00 AFTER mesh-group-id
                - async blur imgMesh00
                - get imgMesh00 AFTER blur
                - get color AFTER mesh-group-id
                - compose imgMesh00 into color                
        - Schedule operation when all objects of this object group have been rendered: 
            - depends on resources
            - which revision of the resource?
                - need the correct sorting key / constraint ("AFTER mesh-group-id")
                - IMAGE image-id AFTER mesh-group-id
                - resource given as input, but revision determined dynamically
                - should have no need to signal explicitly the end of a mesh-group
                    - implicit ordering
            - which barriers?
            
- Scenario A' (dynamically added post-procs):
    - See mesh with custom post-procs after culling
    - Schedule post-proc 

- Scenario B:
    - Get final image
    - Do post-processing on it

- Scenario C:
    - See a stroke mesh
    - Render strokes into acceleration grid
    - When all stroke meshes are finished
    
### Q: what does the graph looks like? how to order and synchronize operations correctly?
- Revision of resources determined by order and constraints
- constraints on async: one-way data flow only
    - resources can only be written (produced) by ONE queue 
    
    
### Q: window system integration
- just pass a target window to the renderer constructor
    - winit::Window
    - OR glwindow
- configuration done through config file
- the renderer is a unique system (only one for the whole program)
    - can render to multiple windows
    
### Q: Shaders & graphics pipeline configuration
- type-safe, proc derive from struct
- still need an interface to bind parameters from the generated derive
    - `ShaderInterface::visit(binder)`: need a standardized procedural interface for the binder.
    - ShaderInterface and descriptor set layout?
        - can derive an unique descriptor set for a given layout, but how to specify locations?
    - BindingGroup: equivalent to a descriptor set, binds a group of resources at a standard location
        - shader must match (each matching binding must have the correct type)
        - warn/error (?) when some variables in the shader are not set
### Q: (issue) multithreaded command submission in backend / command sorting
- must track resource usage across the command stream, cannot do that in parallel
- possible solution: recover the dependency graph from the stream
    - costly...
- resource domains?
    - alloc/frees only valid for those domains
    - resource domains -> passes
        - special index in sort-key
        - has a dependency graph
        - register resources for passes
            - mask + value (cover both write and use)
        - transient resources allocated for whole passes
        - can submit passes in parallel 
        
### Q: Shader interface checking
- Typed pipelines: GraphicsPipeline1<State,I0>, GraphicsPipelineN<State,I0,I1,...>
- State = dynamic state & push constants & vertex inputs
- I0..In = descriptor sets
- Internal API: 
    - DescriptorSetLayout
    - PipelineLayout (descriptor set layouts + push constants)
    
    
### Q: Framebuffers and render targets
- A graphics pipeline expects a particular number of render targets with particular formats
- VK: a graphics pipeline expects even more: render targets + compatibility with render subpasses
- should we expose framebuffers (as a collection of attachments)
    - if yes, expose what?
        - formats?
        - formats and usage?
        - subpasses?
- Pipeline: collection of color attachment descriptions + Option<depth attachment description>
    - lookup framebuffer in cache
    - unused for GL
    - unused for vulkan (renderpasses)
- for vulkan:
    - same pipeline => same renderpass
    - renderpasses (and pipelines) created on-the-fly from attachment descriptions
    - OR 
        - create renderpass at the same time as pipeline
        - subpasses are only useful with input attachments and tiled rendering
    - hints in scopes:
        - r.renderpass(scope, subpasses)
        
### Q: Pipeline creation
 - Option A:
    Source -> ShaderModule
    ShaderModule + PipelineParameters -> Pipeline
 - Option B: no shader modules
    Source + PipelineParameters -> Pipeline
    Recompile shader modules every time
    Can be costly
 - Builder
    In renderer VS backend-specific
    Too much stuff repeated if only in backend
    Clone+Edit workflow
    Can load parts from files
    Issue: contains dynamically allocated vecs
    
    
### Q: Lack of statically-checked lifetimes is unfortunate: arena-based resources management
 - all queries must go through the backend
 - use-after-free is possible, albeit caught by the slotmap mechanism
 - Proposal: Arena based management
     - Long lived resources can use the arena of the renderer with lifetime `'rcx`
        - Allocated once, never released
     - Use frame objects to limit the use of a resource to a frame `'fcx`
     - Custom arenas: level, file, session...
     - pointers instead of handles
     - no exclusive borrows (already OK)
     - can prevent deletion of objects before frame is finished
     - RenderResources::create_resource(&'a self) -> &'a R::Resource;
        - trait RenderResources
        - impl RenderResources for Renderer
        - impl RenderResources for Frame
     - in backend, the lifetime of resources is extended 
     - slotmaps replaced by arena allocators
     - beware of associated types that need a lifetime (associated type in RendererBackend that borrows another?)
     - lifetimes will pollute a lot of things
        - but it's only a few (renderer, session, and frame)
     - Issue: 
        - arena + objects allocated from the arena in the same struct
            - not possible: arena management cannot be 'wrapped around'
            - EXCEPT if the arena does not borrow the main renderer
                - i.e. arenas containing objects can 'leak' if not deleted manually
                - this is OK!
        - caching
        
### Q: Command buffers?

### Q: Scoped/Persistent dichotomy
- wrong language: the difference is actually aliasable (within a frame, and thus transient) VS non-aliasable (and persistent)
    - you can have a non-aliasable resource that is still fully transient (assign different image from frame to frame)
    - persistent : memory contents are preserved across frames
    - non-aliasable : memory cannot be aliased (but does not imply persistent)
    - aliasable : share memory inside a frame, contents become undefined outside the noalias scope
    - transient : lifetime is for the frame only
- single API for both? or separate
- internals: all resources go through caches? or keep uncached resources?
    - first option results in simpler code
    - also enables swapping backing storage of persistent resources
- Issue: some buffers will be slices of a bigger buffer allocated in the arena
    - cannot swap between arenas
- Limitations of swaps:
    - cannot be query-dependent
    - limitations on transients? 
    - limitations on aliasables?
    - imposing no limitations may increase the complexity of the backend (indirections)
 
 
### Issue: cannot ensure that an arena will not live beyond the current frame
This prevents frame-based synchronization (e.g. multibuffers)

Alternative: arena-based GPU synchronization
- one sync per arena: GPU fence signal when arena is dropped 
    - one sync for each queue that uses the resources
- recycle arenas periodically

### Refactor: put resource management in a separate module
- GlResources
    - upload buffers
    - available images
    - images
    - CPU synchronized objects (upload buffers)
    - buffers

### Issue: resource swapping is somewhat problematic to implement (need an indirection)
 - can we do without?
    - probably not, this is an useful pattern
 - the indirection is not needed if there is no aliasing
    - it is the combination of swapping and aliasing that needs an indirection
    - yet, it's an useful pattern for self-contained render passes
        - notably: zero-copy, self-sorted post-proc ping-pong without manual bookkeeping of the ping-pong state    
 - resource swapping interacts badly with descriptor sets
    - ideally, we want to create descriptor sets in advance, during command generation
    - but cannot, because descriptor changes when swapping resources
        - it is possible, however, to update descriptor sets as a part of a command buffer
        - consider descriptor sets as a resource
 - actually, it interacts badly with every 'derived' resources (resources that reference other resources)
    - descriptor sets
    - framebuffers
    - can't create native objects for those in advance 
        - must create them on-the-fly 
 - interacts badly with conditional rendering
    - cannot swap during conditional rendering 
 - alternatives:
    - remove swapping
        - investigate whether it's really useful or not, can keep the infrastructure for now (doesn't change much)
    - in vk: swap underlying memory block (won't work)
    - defer update of descriptor sets during command submission 
        - can still allocate and pre-fill in advance (for immutable resources)
        - but additional memory needed to keep 'virtual/unresolved' descriptors
 - conclusion: drawbacks outweigh advantages
    - **remove swaps**
    - for the post-proc use case:
        - let the application handle it 
        - for instance: 
            - request a post-proc chain ID
            - if it's even, use the main buffer; if it's odd, use the other

### Q: draw states: what to put in blocks, what to bind separately?
- already a duplication between our command lists and native command buffers
    - necessary for command reordering
    - can't be avoided
    - or could it? 'sequential' draw calls can be put into a native command buffer directly
        ```
        cmd.secondary(sort_key, interface, |cmd| {
            cmd.command(...) // encoded on-the-fly in a native command buffer
        })       
        ```     
        - then sort with 'command buffer' granularity
        - related to secondary command buffers for drawcall-heavy workloads
            - then, indirect execution (possibly multiple times)
    - make our command lists useful
        - draw **calls** (plural): never a single draw call, always more than one         
- frontend is stateless (non-negotiable)
    - need a way to specify an array of draw calls without temporary allocation
    - for pipeline in pipelines
        - bind pipeline
        - for vbos in vertex_buffers 
            - bind vbo
            - for submesh in submeshs
                - bind submesh
    - the state cache will eliminate redundant state changes, but this still consumes a lot of memory
        - 'array' draw calls
            - `draw_multi(interfaces, &[drawcalls])`   
            - draw call: vertex buffer set index + draw call params
        - partial pipeline interfaces
            ```
            cmd.with_interface(interface, |cmd| {
                cmd.with_interface(interface, |cmd| {
                    cmd.draw(...);
                })
            })       
            ```              
- stakes: memory usage, data duplication
- use cases: same pipeline, different vertex buffers & draw commands
- render targets: block (framebuffer) or separate?
- individual dynamic states: Viewport, Scissors, Stencil, etc.
    - StateBlock?
    - SetXXX commands?
- vertex buffers:
    - StateBlock?
    - SetVertexBuffers?
- STATE BLOCK: render targets (framebuffers)
- others: state change commands
- command buffers should be more packed
    - variable-size commands
    - embrace indirect buffers
        

### Q: Framebuffers
 - useful as a group of attachments
 - if framebuffers are not exposed:
    - pass array of attachments every time
    - GL: must create a framebuffer 
        - probably from a cache
        - issue: lifetime of the created object 
            - framebuffers become invalid as soon as the textures are deleted => must mark for deletion
            - automatically delete a framebuffer that has not been used for some time
            - Scenario:
                - create framebuffer
                - delete texture
                - re-create another texture, which is given the same name
                - use framebuffer
                    -> what happens?
    - pushing framebuffers to the user can solve this (can ensure that the textures live long enough)
    - however it's kind of a useless feature from the user point of view 
        - no additional control provided
        - the same could be said for descriptor sets
    - in vulkan, framebuffers need a render pass to be created
        - a.k.a "framebuffer layout", includes subpasses
        - this will be exceedingly complicated to detect automatically on vulkan
        - can be compatible between different renderpasses, but the rules are somewhat complicated
        - benefit from exposing that to the user?
    - Q: are we going for the lowest common denominator?
        
 - creation can be costly, but it's a bother when changing framebuffers constantly
 - can use arenas to create a framebuffer inline
 - tile-based rendering?

### P: Alternative backend design
 - Slotmaps and handles
    - handles are hashable
 - Arenas are not part of the renderer and just wrap handles in safe lifetimes
    - Con: adds a level of indirection
    - prefer generic associated types (GAT) when they are finally available
    
    
### Issue: window resize
 - must re-create textures and framebuffers
    - granularity: frame
    - must cache framebuffers!
 - OR: exit scope somehow when resizing
 - final: use special scope (and arena) for swapchain-dependent resources
 
### Split crates (renderer + backend + extras)
 
### Observations from the current design
- works
- too early to make performance measurements
- creating pipelines and shaders is **tedious**
    - must have a GUI, or a full-fledged format
- updating pipeline interfaces is **tedious**
    - e.g. adding a uniform in a buffer
        - modify uniform in struct
        - change buffer interface (optional)
    - e.g. adding a shader resource
        - modify descriptor set
        - check shader so that the descriptor maps to the correct binding
        - modify descriptor set interface
    - first improvement: easier creation of descriptor sets
        - typed descriptor sets (implementing DescriptorSetInterface)
        - no need to specify the layout: put in cache
- investigate how to create reusable shaders / image passes
    - e.g. now, need to re-create a pipeline if the output format changes
        - shader templates?

### Authoring graphics pipelines
- shaders + pipeline states 
- a.k.a. effect files
- goals
    - composability
    - less boilerplate
- more like "pipeline templates": some states could/should be overridden by the application
    - self-contained "bits of state" that can be included/referenced by other pipelines
    - e.g. include template for vertex inputs, G-buffer outputs, blend modes, etc.
    - templates can require parameters
        - e.g. some bits of code, shader snippets, etc.
- shader composability
    - e.g. add another interpolated vertex value without having to modify the vertex shader every time
    - not for now, better put that into the shader language itself
- GUI editor?
    - javafx
- Combining/merging/joining files could be done at the JSON/TOML level 
    - GUI only for the data model
    - Only when resolving
- structure of a graphics pipeline template (.gt/.toml) file
    - toml or json? toml
    - documents (files can contain multiple documents)
        - document name
        - parameters 
            - name
            - type
        - imports
            - file
            - document name
        - derives
            - file
            - document name
        - state blocks
            - vertex_input 
            - input_assembly 
            - rasterizer 
            - ...
            - vertex_shader
        
### Authoring graphics pipelines: alternate approaches
- a specialized file format is heavy in maintenance
    - need proper parsing, validation, error reporting
    - templates may need to grow into full-featured expressions
    - in the end, raw code is more attractive
- Lots of low-level things that will be rarely changed: why make a GUI for that?
    - except for learning purposes, useless
- Code-driven approach
- idea: leverage the existing programming language (Rust) and author graphics pipelines in Rust
    - leverage type checking, functions, syntax for specifying data
- main drawback: must recompile when pipeline changes, and restart the application
- idea: dynamic modules
    - hot-reloadable, minimal dependencies
    - dynamic modules contain type-checked resources
    - can be generated by a build script if necessary
    - types are not hot-reloadable
- issue: dynamic shader modules with generics
    - renderer interface is not object-safe
    - can the renderer (and the arena) be object-safe?
        - backend-specific data to create a shader
##### Option A: make the backend object-safe
- Option A1: backend returns `&'a dyn Resource`
    - pointers are fatter
    - backend must downcast and check type before each use
        - overhead?
    - `Arena<R>` becomes `Box<dyn Arena>`
    - the rest is otherwise unchanged
- Option A2: backend returns handles
    - Issue: handles are not self-contained pointers to resources
        - Need a reference to the arena to resolve
        - But the lifetime of the arena is controlled by the user
        - Alternative: one single table of resources in backend, arena is just for lifetimes
            - increased overhead and syntactic noise
            - arena is just a vec of handles, no optimization of allocation patterns
##### Option B: do not make the backend object-safe
- hot-reload modules must be parameterized with a single backend type, or not at all
- entails recompilation of the main executable if something in a generic function changes
    - a lot of things can be generic: anything with backend types
        - GraphicsPipelineCreateInfo is generic
- just use the same backend type in each crate (can keep in sync with a shared crate containing a specialization of the renderer)
##### Option C: have both
- Object-safe wrapper around `Renderer`
- `dyn Renderer<Backend>`
- wrap around Arenas, Renderer, and Pointers
- impl<T> DynRenderer for Renderer<T>
- like Option A, but on top of the backend instead of implemented by the backend
- actually, can still use renderer, and have just `DynBackend` sitting on top of the backend
- trait BackendAny
- impl Backend for dyn BackendAny
- impl BackendAny for T where T: Backend
- wrapper methods that type-check the passed data
- there are still issues with descriptor remapping
    - translate vulkan SPIR-V to GL SPIR-V if using the SPIR-V path
    - otherwise, use spirv_cross (thank you very much)
- Issues:
    - CommandBuffers: are templated by the backend type
    - must go through all entries and downcast references
        - issue: may be slow
    - faster to do it on-the-fly? But needs a type-erased command buffer impl
        - this puts the type-erasure layer on top of Renderer<Backend>, not on top of the Backend itself
        - requires replacement of all references to Renderer<T>
        - can possibly allow optimizations (native command buffers generated on-the-fly)
            - e.g. can put commands with the same sortkey in the same native command buffer
        - sorting logic must also be in the backend... duplication of logic
            - although the sorting logic is not that complicated
        
### Crate refactor
- Split into multiple crates
    - renderer: the renderer
    - spirv(renderer): SPIR-V processing tools (Module, Ast)
    - backend_gl(renderer, spirv): OpenGL backend
- After refactor:
    - gfx2 is the renderer
    - gfx2-backend-gl is the OpenGL backend
    - gfx2-spirv contains SPIR-V manip tools
    - gfx2-derive are proc derives
- Move the `pipeline_file` module into a separate crate (or as a module in examples?)
    - there is nothing backend-specific in there anymore
- Rename gfx2 to something else?
    - avoid confusion with gfx-rs
    - autograph
- gfx2/autograph is the main crate
    - autograph_renderer would be the renderer (or autograph_gfx?)
    
### Ideas
- Contrary to popular trend, prefer code-driven approaches to data-driven approaches
    - or perhaps, consider code as data (nobody knows what data-driven means anyway)
- usually, data-driven means that there is a _schema_ for the data
    - the data is stored in files, and need to parse them according to the schema to load the data
    - as more flexibility is needed, the schema becomes more and more complicated
- usually, data-driven implies that the data files are hot-reloadable, which is nice
- approach: prefer hot-reloadable **code**
    - closer to the application
    - no schema, no parsing logic
- furthermore: prefer hot-reloadable **application** code, not scripts
    - application and modules written in the same, single language
    - no marshalling/interop layer
    - relies on the compiler
- Benefits:
    - (much) less parsing and validation logic 
    - type safety
    - composability
    - flexibility
- Drawbacks:
    - compilation speed 
    - possibly more verbose than specialized languages
        - consider EDSLs (with macros?)
    - also more noise/boilerplate in modules
        - alleviate with macros?
- Applications:
    - type-safe CSS (see kotlin)
        - need a good approach for EDSLs in rust
    - GUI widgets 
    - Graphics pipelines
    
#### Shader modules
- compile GLSL to SPIR-V on compilation, with a macro annotating a const &'str:
    - `#[vertex_shader]`, `#[fragment_shader]`, ..., `#[combined_shader]` 
    - generate shader info: ` gfx2_shader_info_for_xxx`, `gfx2_combined_shader_info_for_xxx`
    - also generate typeinfo data, because why not
    - and statically check against existing interfaces, 'cause why not
        - basically, typecheck shader interfaces during compilation
        - lift typedesc 
    - use rust types in GLSL?
        - GLSL included as proc macro, but no way to make rust types visible (must be later in compilation)
        - however, can do the reverse:
            - create structs from GLSL 
            - entirely possible with SPIR-V parser
            - write the interface once, in the shader
            - automatically generate interfaces (descriptorsetinterface, pipelineinterface) depending on shader        
    - issue: specialization may change the interface
    - issue: poor error messages (no const_assert! yet)
        - so, not a priority currently
    - possibly, generate interface types 
        - but not a priority
    - setup: 
        - one crate containing the shared interface types
        - one crate containing the shaders and config (hot-reloadable)
        - and one crate containing the main application
    - could possibly merge the interface and hot-reload crates into a single one
        - if we can ensure that the interface types do not change
- in main application
    ```
    let c = ExtensionCrate::load(&arena);
    let pipeline = c.load_graphics_pipeline::<Backend>(&arena, "<name or selector>"); // -> GraphicsPipeline
    ```
- shader modules should be backend-agnostic 
- bikeshedding: 
    - there is already something called `ShaderModule` (shader objects, vulkan shader modules)
    - crates = 'extension crates'
    - GraphicsPipelineDefinition
        - all states + SPIR-V bytecodes, no shader modules
        - interface type between extension crate and main application
- duplication: shader compiler crate and shader_compiler_derive
    - gfx2-shader-compiler crate (internal wrapper around GLSL preprocessor and shaderc)
        - Compiler
    - gfx2-shader-macros (gfx2-shader-compiler) (proc macro for compile-time GLSL checking)
        - static_shader!
    - gfx2-shader (gfx2-shader-derive) (prelude for hot-reloadable pipeline definitions)
        - trait Shader
            - fn spirv(&self) -> &[u32]
        - static_shader!
        - 
        

#### Extension crates
- hot-reloadable
- can contain:
    - shader code
    - backend-specific (backend-bound) rendering commands
    - backend-agnostic (unbound) rendering commands
        - type-erased resources
        - specialized format?

#### Issue: Arenas
- currently only usable by the backend
- but in theory could also be used for other things
    
#### Hot-reloadable modules
- Must have C interface (cdylib)
- Issue:
    - backend code is duplicated across plugins
    - cannot use a dylib since they simply don't work (linker error)
    - opengl function pointers are not loaded
        - (easy) solution: use gleam
        - should be better than global variables
        - closer to what vulkan does
        
#### Renderer utils
- blitting
- format conversion
- should be in a reusable library
- backend-agnostic
- implement in renderer?
    - no: impl RenderUtils<'rcx>
- not a priority

#### The Great Renaming of 2019
- gfx2 -> autograph
- renderer -> autograph-render
- derive -> autograph-render-derive (re-exported by render)
- backend_gl -> autograph-render-gl
- extension_runtime -> autograph-ext + autograph-ext-macros (or autograph-plugin)
- shader_macros -> autograph-shaders + autograph-shaders-macros

#### Geometry and Meshes
- priority: connection with Maya / Houdini
    - import only baked animations
- alembic?
    - documentation is complete shit, as expected
    - but most promising
    - bake sim to alembic, load scene, sync camera
    - alembic-rs ...
- custom plugins?
    - hook at the last moment before rendering, expose geometry
    - send geometry to connected process
    
#### Images
- priority: image pipelines
- nuke API?
    - Custom renderer: library, or out-of-process?
    - library first
    - need a GL context
        - issue: GL version?
        - drivers on client probably not up-to-date: cannot assume GL 4.6
            - GL_ARB_gl_spirv
            - GL_ARB_spirv_extensions
    - scanlines?
        - request full image before entering node
        - copy image to GPU
        - partial copy? tiled ops?
        - see request, read tile, upload tile to GPU
    - worker threads
        - engine() called on worker threads
        - first call triggers the GPU operation
- OpenFX API

#### Texture upload
- async, with PBOs
- let the driver do the optimization, for now

#### Create renderer from existing context
- grab info about the current context
- swap buffers?
    - custom callback in swapchain
    - not needed if rendering into a texture
    - swapchain size?
        - custom callback
    
#### Robustness
- handle object creation failures
    - images, buffers, framebuffers, etc.
    - return Result instead of panic!()
    
#### Architecture bikeshed no1
- A: private structs in top-level module, impl in submodules
- B: private struct in submodules, but public fields
    - prefer B
- module simplification:
    - merge buffer, image, descriptor, framebuffer, upload, resources in same module
        - arena
    - image::upload in cmd
- rational module simplification
    - identify cross-dependencies
    - put in same module
        - shaders and programs
    - window creation
        - no deps
    - backend impl <-> arena
    - arena <-> all resources
    - cmd execution is separate (deals with references only, no memory management)
    - sync is self contained
- modules:
    - top: backend impl + objects
    - mod resources: object creation & caching & aliasing
        - resources, arena, alias pool
        - impl backend for creation
    - mod cmd: command execution
        - cmd + state cache
        - need access inside objects
    - mod window
    - mod sync_arena
    - mod sync
    - mod format: public 
    - mod shader
        - shader compilation & graphics pipeline creation
    - mod api
    
#### Playground app
- playground
- playground-host
- init, update, render?
- use cases
    - have a render loop
    - add a line to load an image from a file, break render loop and reload
        - but save state
    - struct with state?
    - several structs
        - long-lived stuff
        - short-lived stuff
        - serialize state somehow
    - several entry points
        - `init() -> Outer`
        - `swapchain_init(&Outer) -> Swapchain`
        - `frame(&Outer, &Swapchain) -> Frame`
        - change frame
        - change swapchain, invalidate frame
        - change outer, invalidate all
    - one entry point, load state from hashmaps
    
```

fn main() {
    'outer: loop {
        plugin.init(...);
        
        'swapchain: loop {
            plugin.swapchain_init(...);
            
            'frame: loop {
                plugin.frame(...);
            }
        }
    }
}

struct App<'a> {
    
}

impl Playgrounds<'outer, 'frame> for App<'a> {
    
    pub fn init(&self, arena: &'outer Arena) {
        let x = aaa;
    }
    
    pub fn swapchain_resize(&self) {
        
    }
    
    pub fn render(arena: &'inner Arena) {
    }
}
```

#### TypeDescs in interface
- e.g. `Buffer<T>, Image`
- some types can have an associated TypeDesc, which can be matched against a TypeDesc expected by a shader
- this is distinct from formats (even if they overlap sometimes)
- BufferTypeless has no type info about the contents, Buffer<T> has one

#### Strongly-typed pipelines
- Pipeline <PipelineInterface, VertexTypes
- Remove descriptor set layouts?
    - descriptor set layouts derived in backend when creating pipeline
        - or maybe not in backend? cache in renderer, where typeids are known?
    - cached via hashing / typeid
    
#### Interface check
- In create_graphics_pipeline_typeless
    - submodule pipeline::validate
    - issue: no access to DescriptorSetLayoutBindings
        - do away with DescriptorSetLayout bindings in backend?
        - manage in backend, cached by typeid
        - rename DescriptorSetLayoutDescription -> DescriptorSetLayout
        - no explicit creation
        - gain in clarity
- GraphicsPipelineCreateInfoTypeless contains everything needed
    - vertex input
    - descriptor set layouts
    - fragment outputs
- double down on SPIR-V shader input
    - SPIR-V becomes mandatory for interface checking
- issue: SPIR-V binary is lost when we reach pipeline creation
    - have only opaque ShaderModules
    - Option A: ShaderModules keep a copy of the SPIR-V binary
        - duplication?
        - could also just keep the interface
        - in vulkano: everything encoded in the type
    - Option B: generate one type per shader => type reflection data + uniform types
        - good (this is what vulkano does, basically)
        - but can't hot-reload types
        - can associate a dynamic library to a type 
            - when querying the type, reload dynamic library
            - issue: costly to check on each access
            - issue: statics are not really static
            ```
            #[derive(Shader)]
            #[shader(file="...")]   
            struct BlitVertexShader;    // this is NOT hot-reloadable... but convenient
            ```
    - Option C: ShaderModules keep a ref to the SPIR-V source
        - actually, keep a ref to a ShaderModuleInterface, from which it's possible to get the interface (or just the SPIR-V bytecode)
        - ShaderModuleInterface + StaticShaderModule
        - DescriptorSetInterface + StaticDescriptorSet
        - PipelineInterface + StaticPipeline
    
    - Remark: hot-reload cannot be specific to shaders
        - reload whole crates or nothing
    - do not force hot-reload
- A: a shader is basically a function
- B: a shader is basically a type
    - with members for SPIR-V bytecode
    
        
#### Goals & issues of hot-reload
- not losing application state (that results from user input, or that is costly to regenerate)
- issue: if types are changed, becomes unsafe (must compare types for equality somehow?)
- way forward:
    - granularity of hot reload must be bigger, only loosely connected modules
    - e.g. 
        - module containing the application state
        - module containing the renderer implementation
    - don't reload individual shaders: too tightly coupled with the rest of the application
    - BUT, typical scenario:
        - user loads a 'style module' from a file
        - file contains shaders
        - and possibly custom interfaces (custom parameters, etc.)
            - cannot require SPIR-V to be available statically
            - cannot require Descriptor Set Layouts (!) to be available statically
        - alt resolution: the module does the whole rendering (calls functions, etc.)
            - just a function fn(renderer, arena, images, pipelinetypeless)
            - and a function fn(renderer) -> pipelinetypeless
            - but: need type erasure
    
#### Shall we talk about render passes again?
- RenderPasses = list of passes that share framebuffer attachments 
    - can keep fbo data in fast framebuffer memory
- in Vulkan: renderpass referenced by pipeline

#### Issue: data streaming
- allocate / free resources in an unscoped manner
- deletion of a resource must correspond to a scope exit in code
    - impractical: streaming of resources when moving into a level?
    - in renderer (or in backend): swap resources in/out depending on usage?
    - 'unloaded' resources: ref still valid, but not in main memory
        - automatically reload on usage
        - something like a cache, or an 'unloader'
            - memory hierarchies?
            - reloaders?
                - callback when resource needs to be reloaded
        - resources are not unloaded within a command buffer
            - done before execution of command buffers
            - how does this affect resources used in command buffers?
                - silently invalidated?
                - undefined behavior?
                - panic?
                - error?
                - replaced with another resource?
                
#### Final form of graphics pipelines
- type = pipeline (also associate shaders)
- no need to create the pipeline: cached into the type (or via typeid)
    - can cache as an associated static (there is only one renderer per application)
- just draw(pipeline)
- still needs to get static pipeline states somewhere...
    - custom attributes on struct
- specialization as generic parameters
- Cleanup autograph-shader-macros
    - remove everything except include_shader
    - remove preprocessor, replace with shaderc mechanism for includes
- TODO: do not pay for DescriptorSets verbosity when not needed
    - specify descriptor sets+binding in pipeline interface

#### Shader effect system
- make shader code as maintainable and modular as rust code
- ideas:
    - build system integration
    - module system integration
    - autogen shader interfaces
    - input/output interfaces as parameters instead of global variables
        - more than one shader entry point in a file
        - use HLSL?
            - typed entry points
    - rust+glsl shader modules
        - can reference shader modules from other crates
            - important for modular shaders, plugins, etc.
            - how?
                - somehow access data within a crate
                - but a crate is not only source code
                    - can be an rlib 
            - at first glance, this seems impossible in a proc-macro
            - the issue is that proc-macros operate before module resolution, and we have no access to contents of 
                other crates
            - one possible thing is to defer the compilation of shaders
                - generate code that combines shaders from different crates, then compile that into SPIR-V
                - but we lose shader validation at compile-time
        - ideally: reference/import types from rust
        - `#use autograph_shaders::Uniforms`
    - crazy idea: turn rust ASTs into a SPIR-V generator (_AST lifting_)
        - purely a syntactical transformation
        - validation at runtime (this is only rust code)
        - only describe descriptor sets once, as shader interface (with dummy types)
            - then pass descriptor sets + stage input as parameters
            - however, must 'lift' (again) the types to the host interface 
                - <Params as ShaderInterface>::HostInterface: DescriptorSetInterface
                - generate the host interface from the function
                    - details TBD
        - many potential issues
            - must annotate all used functions
                - but: could provide a set of 'safe' functions (standard library)
            - members?
                - e.g. have `e.function(stuff)`
                - how to resolve `function`?
                - solution: lift types and functions
                    - custom attribute `#[spirv(lift)]`
                    - will generate a type SPV_#typename with all members lifted
                    - or more precisely, impl Lift<Generator> for Typename with Target = Methods
            - let bindings:
                - `let e : type = init;` => `let e = gen.declare::<type>().declare(gen, |gen| <lifted-init-expr>)`
                - `let e = init;` => `let e = gen.declare::<???>(|gen| <lifted-init-expr>)`
                    - `gen.declare<T: Lifted>(init: impl FnOnce(gen) -> Value<T>)`
                - `let e = 3` => 
                    `let e = gen.declare(|gen| gen.lit_num(3))`
                - add binding node, of type
                - `trait Generator`
                - better than parsing AST, since functions can be generic (provided that the type is liftable)
        - other approach: RLSL
    - is it even desirable to use rust for shaders?
        - shaders are not meant to be 'system' code
        - see shaders as artistic inputs
        - does not have the same goals as application code
        - think about authoring techniques 
            - node graphs
            - others?
    - conclusion: wait until RLSL is testable, lots of good design choices here, probably the way to go
    - conclusion 2: until then, think about shader authoring, and how to make working with GLSL/SPIR-V less of a hassle
            
#### Automatic generation of host side interfaces from shader code
- challenge: describe the mapping precisely
    - shader function arguments -> stage inputs, descriptor sets, descriptors, stage outputs, and structured buffer layouts
    
#### Shader authoring
- think about higher-level shader authoring
    - node graphs
    - json description
- descriptor sets, descriptors are implicit
- we want people to be able to write code
    - but without the overhead of full-blown vulkan-flavored GLSL
    - match parameters by name
    - no descriptor sets
    - no layouts
    - inputs/outputs from context
- we want people to be able to write code *against an interface*
- what is a shader?
    - some code that defines the behavior of a stage in the graphics pipeline
    - code + parameters
    - can be represented with a function + some attributes
- how to turn the function parameters into something that the graphics API understands?
    - i.e. assign (set,binding) to parameters
    - if type of function param has a descriptor set attribute, then bind to descriptor set
    - otherwise, assign sequentially
```
fn fragment_shader(
    common_params: CommonParameters,   // (set=0)
    edge_tex: SampledImage,             // (set=1,binding=0)
    substrate_color: vec3,              // (set=1,binding=1,structured buffer)
    edge_intensity: f32,                // (set=1,binding=1,structured buffer)
    );
```
- maybe go higher-level still, and focus on the user-facing application
    - how to create a style!
    - don't care about shaders, etc. 
    - just care about modifying the geometry, generating and placing draw primitives, image processing, rendering some objects in a particular way...
    - provide high-level "nodes"
        - or something other than nodes, anyway
        - user does not code the node, but compose them
        - inside a node: full low level control over the graphics pipeline
        - still, nodes can be programmed with "bits of code" (tm) 
            - node = combine bits of code to make a full graphics pipeline
    - what's the language for the "bits of code"?
        - something that can be easily turned into SPIR-V
        - guess that GLSL is good enough...
- interface:
    - list of 'auxiliary' buffers
    - create an 'auxiliary' buffer for stylization
    - data between nodes:
        - geometry (data + metadata)
            - triangles, lines, points, curves?
        - images
    - use nodes only for sequential operations 
    - some nodes require special auxiliary buffers
        - e.g. edge map
        - automatically insert node if not present
        - "blackboard"-style    
        - but: what if want a node result only for a subset of the input data
            - e.g. want edge map, but only for a subset of the objects in the scene
                - graph constructors 
                    - function with inputs + parameters -> graph -> output
                    - output has a name which can be queried or pattern matched
                        - `edgeMap(scene, meshfilter)`
    - common structure:
        - take scene
        - filter by geometry, object id, material id
        - do some rendering and post-proc passes
        - structure: GeomFilter + Render -> output 
            - output is a piece of data derived from the scene with an unique signature 
    - parameters:
        - share parameters across nodes
    - important: 
        - nodes must be flexible in the data that they can take
            - image formats
            - vertex formats
        -> flexible shader interfaces / specializable SPIR-V

#### Issue: interface checking valid only for specialized SPIR-V
- must implement specialization in autograph-spirv


#### Descriptor set creation & update, and Vulkan's dynamic (uniform|storage) buffers
- currently, we allocate new descriptor sets every frame
    - but only one thing inside the descriptor changes: the offset into the upload buffer
    - vulkan has dynamic uniform buffers for that
    - other API's?
- currently: no way for the backend to know that very similar descriptor sets are going to be created from one frame to the other
- do away with descriptor sets?
    - they don't exactly map to other APIs anyway
- possibility: 
    - no descriptor sets
    - let the backend decide (from layout and annotations) the descriptor update strategy
    - instead of explicitly allocating buffers, pass "wrapper" to interfaces
        - contains a ref to the data on the CPU, and possibly a cached pointer to data on a GPU buffer
        - backend decides to upload the data to a buffer of its choice, which can be already referenced in descriptor sets
        - wrapper is arena-bound
    - given a draw call, know the descriptor set layout
        - question: can we reuse a descriptor set from before?
            - needs hashing
        - somehow, have an object that represents a "partial" set of bindings
            - and complete at draw call time
            - if completed with same buffer, but different offset, nothing to do
        -> partially filled descriptor sets
    - at pipeline creation time:
        - know root pipeline, and possibly other "inherited" pipelines 
            - inherited pipelines may live longer
            - if have an inherited pipeline interface, and the root interface is only a bit more data, then can make assumptions 
                - when creating a partial pipeline, don't do anything yet (store it somewhere)
                - when creating a full pipeline
                    - if the partial pipeline interface has already a descriptor set attached
    - Partial pipeline interfaces 
        - bikeshedding: state group?
        - StateGroup<T> where T: PipelineInterface
        - pipelineinterfacevisitor: 
            - visit_state_group()
            - visit_shader_data()
            
- Plan
    - Descriptor set -> state group
    
#### Pipeline signatures
- example:
```
struct P0 {
    color_target: Image,
}
struct P1 {
    common: P0,
    data: HostRef<T>
}
-> in vulkan, when allocating P1, can reuse same buffer every time
```
- expose pipeline signatures VS caching in backend
    - pipeline signatures -> descriptor set layouts in vulkan
    - ideally: update arguments in-place (after having allocated the descriptor set)
        - unsafe interface? (pipeline arguments are left uninitialized)
            - OK for leaving stuff uninitialized (it's what vulkan does)
            - it's not an interface made to be used directly anyway
    - issue: derived resources (framebuffers, etc)
        - when to allocate?
    - pipeline arguments must be lightweight!
        - many of them in-flight during the frame
        - store only needed state blocks
        - can pre-allocate pipeline argument blocks
        - pipeline arguments:
            - pointer to signature
            - list of blocks (dense command list, stored in arena)
                - sub-blocks
                - ubo
                - ssbo 
                - vb
                - ib
                - tex
                - img
                - viewport
    - signatures
        - describe everything about arguments
        - also sub-arguments
    - issue signatures must be allocated somewhere
        - preferably a long-lived arena
        - either passed explicitly (but where to store it?)
        - or cache it, query by typeid
            - caching in backend
    -> pipeline signature not exposed in backend
        - SignatureDescription -> signature
    -> issue/bottleneck: querying the cache of pipeline signatures
        - is it costly?
    -> issue: signatures without typeids
        - must hash
    - pipelines know the layouts -> pass pipeline?
    - dynamic arguments?
    
#### Pipeline argument blocks
- small bits of pipeline arguments

#### Pipeline signatures and root pipeline signatures
- Signatures are opaque objects for the backend
- Must do validation on SignatureDescriptions
- dynamic pipelines?
    - SignatureDescription as parameter

- assume that creating a PipelineSignature is a costly operation

- PipelineSignatures can be shared between multiple graphics pipelines, so don't let the backend create it from a description
    - in vk => descriptor set layouts, can be shared between pipelines
    - actually, let the backend create it from a description (it can handle de-duplication via typeids)
        - but: dynamically generated layouts?
- Yet, at pipeline creation time, must have the description of the signature so that it can be validated
    - keep a backref to the description used to create it
    - same as SPIR-V shaders
- alternative
    - create on-the-fly?
    - no
    
- PipelineArguments are always passed into blocks
    - to create a block, must have a ref to the signature
        - and to verify the interface, must have a ref to the description
            - not really necessary (the backend can check)
    - query by typeid from cache OR get from pipeline
        - get from pipeline => must have a backref to signature, which requires a lifetime, which breaks assoc types
            - store root signature alongside pipeline? (same lifetime anyway)
            - cannot get away with transmute to static because it's not dropless (ref to signature might not be valid anymore when we drop)
            - just use a *const
            
- Problem 1: Signature description must be available at pipeline creation time, but pipelines are not created at the same time as signatures
    - because can reuse signatures for multiple pipelines
- Solution A:
    - ref to signature description, like SPIR-V shaders
    - noisy
- Solution B: create signatures at the same time
    - issue: what about non-static signature descriptions (generated dynamically)
        - can't reuse
    - also: unnecessary cache lookups?
- Solution C: trait for backend signatures to get the original description
    - requires owned copy of description
    - puts burden of maintenance on the backend
    
- Discrepancy between create info of pipeline signatures (provide sub-signature descriptions) VS graphics pipelines (provide baked signatures)
    - can't reuse sub-signatures either 
    - Solution B avoids this discrepancy 
    
- In most cases, description available statically via type
    - only in dynamic cases the description is unavailable via type

- modify PipelineInterface
    - static: not directly create_info
    - create via backend, specify type
        - if cached in backend, return cached, otherwise call create_signature on pipelineinterface
        - visitor calls 
        
- inheriting from a dynamically generated signature should be possible

```
let d = SignatureBuilder::new();
d.sub_signature(&PipelineSignature);
d.uniform_buffer(...);
```

- PipelineSignature is a trait
    - inner()
    - get_desc() -> &'b PipelineSignatureDesc
- handle both static and owned case
 
- creating a pipeline, statically-known case:
    - enter create_graphics_pipeline<T>
    - get signature tree from T
    
- Pipeline signatures bundled with description (like ShaderModules)


- Problem 2:
    - must have signature to create a block
    - pipeline knows signature
    - bundle root signature with pipelines
        - pipeline = 3 pointers 
            - pipeline object (1)
            - root signature bundle
                - root signature (2)
                - root description (3)
            - if dynamic, need root description alive
                - alloc root description somewhere?
                - signature builder must be in arena
                - root signature bundle holds backrefs to all signatures

- PipelineInterface
    - create_signature() -> PipelineSignatureTypeless
    - into_arguments()
    
- cache pipeline signatures in frontend
    - why in backend?
        - lifetime issues?
        - need a location for storing a &'a PipelineSignature
        - certainly not in the Arena itself
        
- underlying issues: 
    1. creation data for signatures and shaders is needed for validation, but is not available *to the frontend* at validation time
        - actually it is available in many cases (static info)
    2. creation of signatures and shaders cannot happen simultaneously with validation because they are costly operations, the result of which (signatures and shaders)
        may be shared across multiple pipelines.
    3. it is possible to cache shaders and signatures to create during validation with sharing, but it's impractical 
    4. Validation is done in the frontend and shared across all backends
    5. Creation data for signatures is a tree (include info from sub-signatures)
    
- possible solution: move validation in backend
    - backend knows about the signatures
    - also, backend may have additional restrictions on what it considers to be a "valid pipeline"
    
- Solutions
    1. Move validation in backend
        - no need to keep refs to create_info, but backend must copy them
    2. Keep refs to create_infos
        - borrows data for a long time: avoid?
        - a long time = since the pipeline arguments need the signature, until all pipeline arguments are created
            - and since the graphics pipeline has a signature => for the whole lifetime of the graphics pipeline
    3. Have backend objects copy create_infos, but use them in frontend
    4. Have the *frontend* copy the create_infos in a privately owned arena
        - basically 2 but without borrows
      
- between solutions 1,3,4:
    - 4: the frontend may copy the create_infos, but the backend may also do the same
        - if we're always going to copy the create_infos, at least do it in one place only
    - 1: too much code duplication?
        - can put fine-grained validation code in render, but will need the create_info anyways

#### Targeting DX12
- pipeline signatures -> DX12 root signatures + other stuff
- validation?


#### Pipeline signatures & friends: going higher-level?
- validation in backend
- what if source is not always SPIR-V?
    - translate to GLSL? HLSL?
    - if no translation is possible, then validation must be done through API-specific reflection mechanisms
-> for better extensibility, and compatibility with existing systems, and performance (avoiding round-trips), 
    provide a way to use native shader code
-> put validation in backend?
    - no refs to create_infos
    - must deep copy descriptors, and also typedescs
        - if it's going to be done in every backend anyway, do it in the frontend
-> extract arena code into autograph-common lib

-> Copying typedescs for no reason (when descs are static) is inefficient
   
-> Final plan: 
    - Pass pointer to description to backend with arena lifetime
    - caller is responsible for allocating stuff in the correct place
    - expose API for dropless arena allocation
    
-> Q: Does creating a signature borrows sub-signatures?
    - Vulkan: signature => descriptor set layouts (and pools)
        - keep alive

-> Unfortunately, keeping ref in backend is impossible (too unsafe)

-> Final decision is to either:
    - copy in backend
    - copy in frontend
    - reference in frontend
    
-> Reference in frontend, with wrapper types or traits
    - PipelineSignature
    - trait DescribedPipelineSignature
        - full desc tree available 
    - trait DescribedGraphicsPipeline
        - signature description available
        - signature available (or queryable by typeid)
        
FINAL DESIGN: 
        
Signature trait
    - raw object 
    - signature description tree
    impl for TypedSignature<T>
    impl for DynamicSignature<T>
    
Argument trait (optional)
    - raw obj
    - signature
        
GraphicsPipeline trait
    - raw object
    - root signature description
    - root signature
    impl for TypedGraphicsPipeline<T>
    impl for DynamicGraphicsPipeline

#### Rename PipelineSignature -> Signature, PipelineArguments -> Arguments
- no need for too much verbosity

        
#### Issue: GraphicsPipeline must hold a ref to their signature, so that there is no need to look it up every time
- but no ATCs, so leads to very unsafe code in backend
- cheating lack of ATCs with 'static refs is dangerous
    - can misuse in backend
    
#### Note: in GlArena, storing a *const Signature in GlGraphicsPipeline is unsafe
- GlGraphicsPipeline is not dropless, so no guarantee that Signature still alive on drop


#### Reducing redundancy in the backend
- must pass the root signature description twice: when creating the signature, and when creating the graphics pipeline
- do not assume that the backend has to store the signature description
    -> self-contained
- create root signature and graphics pipeline at the same time
    - don't share root signatures
    - but can share inherited signatures
    
#### Synchronization may be costly
- mutex lock on every arena access
- sync should be opt-in
    - arenas not sync by default
    - objects still sync, but no mutable access to them anyway
    - let the user handle it
- should backend be accessed with &mut?
    - issue: alloc = mutable borrow
    
#### Alternative API for argument blocks
- just copy the pipeline interface in an arena, and have backend either
    - just put the interface INSIDE the command buffer! (through a dyn pointer)
        - simple memcpy
        - also, some "compiled" arg block if backend needs it (no-op otherwise)
    - store a dyn pointer an impl of PipelineInterface<B>
        
#### Viewports and scissors in argblocks
- the number of scissors must be equal to the number of specified viewports
- the number of viewports should be specified in the create info
    - but: redundancy (viewports in create info VS num viewports in signature)
- issue: num viewports is specified either:
    - in create_info
    - in signature
- imposed limitation: viewports can't be split across multiple argblocks
- the number of viewports in a signature MUST be known
    - no arrays or slices
    
#### Should the backend own the window?
- no, take a ref to the window
- this breaks the current boilerplate 
- actually impossible: Instance is an ATC
    - Rc<Window> or Arc<Window>
    
#### Arguments: the draw_quad issue
- create an argblock without vertices, but pipeline interface contains vertices
- QuadShader<P> where P is an Argument block without the vertices
   
    
#### OpenImageIO: consider reading only contiguous ranges of channels
- query channels by name, and fail with a new error (NonContiguousChannels) if they are not contiguous, or not in the 
 correct order
- on the other hand, it's useful for channels that are not in the correct order

#### Typed pixel uploads
- pixel uploads to images should be typed
    - upload type = type of a single channel
    - number of channels (components) taken from the format
        - except for packed types
- infer format from pixel type?
    - e.g. `[f32;4] -> R32G32B32A32_SFLOAT`
    - what about SNORM/SINT, UNORM/UINT and SRGB?
- common pixel trait?
    - Pixel
        - number of channels
        - channel interpretations (red, green, blue, depth, packed...)
            - layouts (R,G,B,RG,GR,RGB,RGBA,BGRA,ARGB,...)
        - bit width of components
        -> basically, associate types to each vulkan format
    - UnpackedPixel: Pixel
        - Subpixel (i8,i16,i32,u8,u16,u32,f16,f32)
        - create from slice of Subpixels
        
- should it support pixels with lots of channels?
    - this is about types, so no
    
#### Remove/archive dead crates:
- `common_shaders` is useless
- `plugin` has serious shortcomings and should not be used
- render examples don't work anymore: remove them
- openimageio should be a submodule
    
#### Testing infrastructure
- one line to create:
    - a window
    - a renderer
    - an arena
    - an event loop
- crates to test:
    - render
    - render-extra
    - render-gl
    - macros
- what to test:
    - validation of argument blocks
    - quad rendering
 
#### Strongly-typed render targets
- Image types:
    - RenderTarget: image usable as a color attachment
        - can be converted into image
    - DepthRenderTarget: image usable as a depth attachment
        - can be converted into image
    - Image: generic image type
        - pointer to image
        - flags?
        - can be downcasted to other types via TryFrom/TryInto
    - Texture: can be sampled
- Q: where to put the flags?
    - alongside image ref?
        - so that the backend doesn't need to worry about it
        - but wasted space if it's statically known
        - same as format, basically
- Image
    - ImageWithFormat
        - TypedImage

- RenderTarget(Image): guaranteed to be an image suitable for a color attachment
- DepthRenderTarget(Image): guaranteed to be an image suitable for a depth attachment
    - impl 
- Texture(Image): suitable for sampling as a texture
- Both?
    Texture + RenderTarget?
    Texture + RenderTarget + StaticFormat + DepthRenderTarget
- if need both RenderTarget & Texture (for instance), then use two wrapper types
- issue: copying image handles to argument structs incurs unnecessary copy of properties
    - except if it's a one-way conversion only
    
- trait Image
    - inner()
    - flags()
    - is_render_target()
    - is_texture()
    - format()
- trait TypedImage<I: Pixel>: ImageWithFormat<...>
    - blahblah
- RenderTarget<I: Image>
- Texture<I: Image>
- GenericImage
    - inner ref
    - flags
    - dimensions
    
- Distinction Image / ImageView?
    - ImageView in arguments
    - Image everywhere else
    
#### Investigate a D3D12 Backend as a proof of concept
- why not vulkan? 
    - D3D12 has nice tools 
- issues:
    - current lib needs SPIR-V for validation
    - but: no path from SPIR-V to DXIL yet
- proposal: redesign of shader compilation
    - autograph-render no longer expects SPIR-V as input
        - instead, expects backend-specific bytecode...
        - ... with reflection info in a **standardized format**
            - so use spirv crate to reflect spirv, and something else to reflect DXIL
            - extract TypeDesc and other outside of autograph-spirv (not spirv specific anymore)
            - generate reflection at **compile time**
        - issue: descriptor sets? root signatures? descriptor tables? 
            - the reflection format must be aware of all these little differences
- a single shader compiler crate that contains macro to
    - compile shader code to SPIR-V
    - compile shader code to DXIL
    - compile (or just copy) shader code to GLSL
    - backend crates re-export the macros that correspond to the targeted API 
- proposal: wrap/use DXC (DirectX shader compiler), and use HLSL
    - why? consider targeting d3d12
        - HLSL -> DXIL provided by DXC
        - HLSL -> SPIR-V **also** provided by DXC (spiregg)
        - for GL: HLSL -> SPIR-V -> GLSL -> driver (spirv-cross)
    - vs GLSL:
        - GLSL -> SPIR-V provided by glslang
        - but GLSL -> DXIL not provided by anything!
            - need GLSL -> SPIR-V -> HLSL -> DXIL
            - or GLSL -> HLSL -> DXIL
- also: slang (extension to HLSL) has the concept of ParameterBlocks, which map quite well to our ArgumentBlocks
    - also also, can already produce a compiler to DXBC, DXIL and SPIR-V
    
#### Autograph-render-extra: "batteries included" rendering engine
- frame graph library
- image loading utilities 
    - reexport OIIO
    - load texture directly from file
    - save texture to file
- general render passes
    - blitting
    - downsampling
    - adv. blending
- mesh rendering utilities
- 2D rendering utilities
- reexport UI

#### sort out descriptors / textures / render targets 
- storage_image: ImageView
- sampled_image: ImageView OR TextureImageView 
    - issue: ImageView can be created even if image cannot be sampled
    - the type in the member does not encode the requirement
        - the sampler can be statically specified
    - check validity as soon as possible
- render_target: RenderTargetView
- StorageImageView, TextureView, 
- Buffers:
    - UniformBufferView
    - StorageBufferView
- Bikeshedding: D3D vs Vulkan parlance
    - Shader Resource View/ConstantBuffer/StructuredBuffer vs Uniform Buffer
    - Unordered Access View / RWStructuredBuffer VS Storage Buffer
    - Vulkan parlance is more concise
    
- Sampled Image -> TextureView/TextureSamplerView
    - Texture{1D,2D,3D,Cube}View / TextureSamplerView
- Storage Image -> StorageImage{1D,2D,3D,Cube}View
- Uniform Buffer -> UniformBufferView / ConstantBufferView
- Storage Buffer -> StorageBufferView / RWBufferView
- Index Buffer -> IndexBufferView
- Vertex Buffer -> VertexBufferView
- Render target / framebuffer attachment -> RenderTargetView
- Depth stencil render target -> DepthStencilView

- View types:
    - Images that are sampled
        TextureView, TextureSamplerView
    - Images that are read by texel but not sampled
        ImageView (ReadOnlyImageView)
    - Images that are read OR written by texel but not sampled
        RwImageView
    - Constant buffers (buffer containing read-only, non-texel data)
        ConstantBufferView
    - Buffers containing texel data, can be viewed as an array of texels (?)
        TexelBufferView
    - Read/write buffers containing texel data
        RwTexelBufferView
        
    - Read only
        - Structured
            - ConstantBufferView
        - Sampled
            - TextureView/TextureSamplerView
        - Texel
            - ImageView (image-backed)
            - TexelBufferView (buffer-backed)
    - Read/write
        - Structured
            - RwBufferView
    
    - Storage VS UnorderedAccess VS Rw VS Mut?
        - which is clearer?
        - Rw conveys Read/Write, Storage conveys ... storage, and unordered access conveys something too technical
    - SampledImage VS Texture
        - texture is clearer, and shorter; use image for texel access
    - Uniform VS Constant VS Nothing
    

- no need for attributes anymore!
    - all descriptors on the same level    
    - just a list of pipeline::Parameter, backed by concrete pipeline::Argument

#### Shaders v2
- generate reflection data at compile time so that render does not need to parse SPIR-V
- issue: not using attributes for fields produce stuff in the wrong order
    - not optimal
    - revert to old design
    
#### Vertex input
- two concepts:
    - vertex buffers, bound to vertex input slots (or input-assembler slots)
        - vertex buffers have a layout that describe all the elements in the buffer, and to which attribute they correspond
    - vertex attributes, which are inputs to the vertex shader
        - don't care about format, input slots, or offsets
- issue: generating vb layouts at compile time, when the layout also contains attribute indices (which depend on the shader)
    - Solution A: hard-code attrib index in VertexData (using `#[vertex_attribute(<index>)]`)
        - But: can't reuse when not the same attrib index in vertex input
        - locations auto-generated in sequence depending on vertex layout + base location
    - Solution B: use semantics, like D3D12
        - assign semantics to VertexData
        - map to corresponding semantic in shader
        - if same semantic, then add index discriminant (e.g. POS(1), POS(2))
            - index discriminant specified in Signature, not VertexData
        - mapping to vulkan?
             - vulkan has no semantics, only location
             - use input names?
    - Solution C: use both
        - semantic + semantic index and base location 
            - for gl, vulkan, ignore semantic
            
#### Remove aliasing of resources in backend?
- aliasing must be known in advance 
- replace aliasing scopes with arenas
    - and make aliasing scopes real scopes
- issue: command generation must now happen sequentially
    - yet, can happen in parallel **within** a scope
- add commands for multiple passes 
    - if resources can be reused between passes => must keep alias scopes
    
#### Tempted to put a general-purpose dropless arena in `Arena`


#### Rethink `Copy+Clone` bound on signatures 
- graphics pipelines, argblocks, take a signature object by value
    - signatures are intended to be a wrapper over a pointer to the backend object
    - but dynamic signatures also store their description
        - big struct
- also it would get rid of these annoying `#[derivative(...)]` annotations
    - why hasn't this been fixed in rust yet?
- passing and storing by reference would change many things
    - additional indirection for static case
- alt. solution: impl Signature for &DynamicSignature
   

#### Issue: DynamicSignatures description
- must return `&SignatureDescription` as part of Signature trait
- cannot own contents of SignatureDescription (self-referential otherwise)
- must borrow description from SignatureBuilder
    - but then: prevents signatures from being stored in a struct (must have builder & signature in the same struct, self-referential)
- issue: no stand-alone DynamicSignature objects
- solution: allocate signature description data in arena
    - same lifetime as backend object
    - put a dropless arena in Arena?
        - private first?
        
#### Graphics pipeline file format
- List of operations, and meta-operations
- if it's a programming language, let it be a programming language
    - lua?
- give me fast hot reload
    - must compile shaders on the fly
    
- drop-in shaders that automatically link to the blackboard context
    - e.g. shader says "I want normals" or "I want blurred depth", and the system will automatically provide the correct resources
        -> need "resource generators"
    - shader says "I want texture from file XXX" and the system will load the texture from the file

- slang shaders with attributes?

- custom DSL?
    - if custom DSL, then need an IDE
    - MPS-based DSL?
```
// an item in the blackboard can be fully bound or partially bound
// if fully bound, then the outputs are defined
// otherwise, outputs are undefined
//
// nodes are referenced by strings
// strings can be something like `blur(param=const(0.0))` in which case the
// system will instantiate the blur node and a const(0.0) node bound to the blur node
// (shorthand instantiation)
// assigned name is blur(param=const(0.0)), base constructor is `blur`
//
// import nodes:
// - annotated slang files
// -
//
// builtin nodes:
// - load_image(filename[,format]) -- loads an image from a file
// - const(value[,type]) -- constant value (scalars, vectors, samplers, etc.)
// - display
// - interact
//
// shader files:
// - modified GLSL
```
- nodes are opaque to the user
    - just an operation that returns some data, living in some memory, with a certain type
- shader node type
    - apply a shader on data
    - define pipeline in code
    - define arguments in code
    - annotated slang files OR MPS-based projectional editor
    - imports and default values (auto-instantiate on import)
- MPS-based language/editor
    - edit in IDE, export as binary, XML or TOML structured text
        - import serialized file in engine 
        - use textgen
    - can go far
        - code reads like a report
        - implement GLSL in MPS
        - translate to raw GLSL
    - free autocompletion
    - custom editors for:
        - pipeline states
        - input layouts
        - blend modes
            - show formula!
        - samplers
        - input images (browse from filesystem!)
        - tess control / eval
        - CS workgroup size
        - matrices / vectors
        - mathematical formula (!)
        - units of measure (!)
        - a bunch of syntaxes for colors, etc.
        - subgraphs
    - type inference?
        - there is a typesystem component
    - no limit to complexity since no syntax to learn 
        - everything is discoverable through the IDE
    - need includes / imports
        - import modules containing code
    - hot-reloadable?
        - why not, this is the responsibility of the native application
- Also good for graphs!
    - reimport whole graph via RPC
- Also good for many other things
    - specialized shader types 
        - image processing (e.g. apply convolution kernel)
        - optimized global operations
            - min/max 
            - histogram
        - scene manipulation
            - select objects by query
            - transform/scale/rotate
            - add new objects
        - geometry manipulation 
- Sharing?
    - MPS models are XML files, no issue here
- MPS is also good for scripting!
    - state machines, event-based programming, GUI description
    - create the language, textgen or bingen to native repr, load into native application
        - all in a single IDE
        - yet, the native repr is decoupled from the IDE, so can edit manually if needed, or use another editor
    - zero actual code for lang since the lang workbench is also a projective editor
- Limitations?
    - domain-specific
        - which is good

```
import "normals"
import "depth"
optional import "color"

edgemap {
    vertex = "..."
    fragment = "..."
    texture[0] = normals
    texture[1] = depth
    
    param "name" = slider { type=float, min=0.0, max=1.0 }
}

```

### Second renaming
- "renderer" is misleading: autograph-render is more like an API
    - rename to autograph-api, or just autograph
    - Instance -> BackendInstance
    - Renderer -> Api, or Gfx (an instance of the graphics API)

- Image networks (Compositor network)
    - Edges are images
    - Transparent to evaluation (can use a tiled evaluation strategy if deemed necessary)

- Dynamic geometry generation?

- Graphics pipeline networks
    - Dependencies between render passes

- Pixel networks
    - Fragment shaders, basically, in graph form
    - Basically: a generic computation
- Vertex networks


What's more than houdini:
- Guanranteed GPU execution and best-effort real-time
    - Suitable for a video game
    
### Proposal for a third refactor
- leverage an existing cross-platform API, but put our own type-checking, validation, proc-macro and arena allocation goodness on it
    - wgpu
    - gfx-rs
    - maybe even remove the renderer trait entirely
        - use descriptor structs from wgpu or gfx-rs
        - (ray tracing?)
- issue: dependent on others to add support for some exotic features
    - like ray tracing, task & mesh shaders, etc.
    - OR: fork wgpu/gfx-rs and add own features
- conclusion: not enough evidence / examples to support going one way or another
    - just stick with what's already there and migrate later
    
### Reconsider command list sorting
- is that really necessary?
    - nothing to gain (yet)
    
