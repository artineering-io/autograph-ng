use crate::{
    buffer::Buffer,
    descriptor::{Descriptor, ResourceBinding},
    format::Format,
    image::{DepthStencilView, RenderTargetView},
    vertex::{
        IndexBufferView, IndexData, IndexFormat, Semantic, VertexBufferView, VertexData,
        VertexInputRate, VertexLayout,
    },
    Arena, Backend, Api,
};
pub use autograph_api_macros::Arguments;
use autograph_spirv::{TypeDesc};
use bitflags::bitflags;
use ordered_float::NotNan;
use std::{fmt::Debug, marker::PhantomData, mem};

pub mod validate;

bitflags! {
    #[derive(Default)]
    pub struct ShaderStageFlags: u32 {
        const VERTEX = (1 << 0);
        const GEOMETRY = (1 << 1);
        const FRAGMENT = (1 << 2);
        const TESS_CONTROL = (1 << 3);
        const TESS_EVAL = (1 << 4);
        const COMPUTE = (1 << 5);
        const ALL_GRAPHICS = Self::VERTEX.bits | Self::GEOMETRY.bits | Self::FRAGMENT.bits | Self::TESS_CONTROL.bits | Self::TESS_EVAL.bits;
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum PrimitiveTopology {
    PointList,
    LineList,
    TriangleList,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum ShaderFormat {
    SpirV,
    BackendSpecific,
}

#[derive(Copy, Clone, Debug)]
pub struct GraphicsShaderStages<'a, 're, B: Backend> {
    //pub format: ShaderFormat,
    pub vertex: ShaderModule<'a, 're, B>,
    pub geometry: Option<ShaderModule<'a, 're, B>>,
    pub fragment: Option<ShaderModule<'a, 're, B>>,
    pub tess_eval: Option<ShaderModule<'a, 're, B>>,
    pub tess_control: Option<ShaderModule<'a, 're, B>>,
}

impl<'a, 're, B: Backend> GraphicsShaderStages<'a, 're, B> {
    pub fn new_vertex_fragment(
        vertex: ShaderModule<'a, 're, B>,
        fragment: ShaderModule<'a, 're, B>,
    ) -> GraphicsShaderStages<'a, 're, B> {
        GraphicsShaderStages {
            vertex,
            fragment: fragment.into(),
            geometry: None,
            tess_control: None,
            tess_eval: None,
        }
    }
}

bitflags! {
    #[derive(Default)]
    pub struct CullModeFlags: u32 {
        const NONE = 0;
        const FRONT = 1;
        const BACK = 2;
        const FRONT_AND_BACK = Self::FRONT.bits | Self::BACK.bits;
    }
}

bitflags! {
    #[derive(Default)]
    pub struct DynamicStateFlags: u32 {
        const VIEWPORT = (1 << 0);
        const SCISSOR = (1 << 1);
        const LINE_WIDTH = (1 << 2);
        const DEPTH_BIAS = (1 << 3);
        const BLEND_CONSTANTS = (1 << 4);
        const DEPTH_BOUNDS = (1 << 5);
        const STENCIL_COMPARE_MASK = (1 << 6);
        const STENCIL_WRITE_MASK = (1 << 7);
        const STENCIL_REFERENCE = (1 << 8);
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum PolygonMode {
    Line,
    Fill,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum FrontFace {
    Clockwise,
    CounterClockwise,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum DepthBias {
    Disabled,
    Enabled {
        constant_factor: NotNan<f32>,
        clamp: NotNan<f32>,
        slope_factor: NotNan<f32>,
    },
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct RasterisationState {
    pub depth_clamp_enable: bool,
    pub rasterizer_discard_enable: bool,
    pub polygon_mode: PolygonMode,
    pub cull_mode: CullModeFlags,
    pub depth_bias: DepthBias,
    pub front_face: FrontFace,
    pub line_width: NotNan<f32>,
}

impl RasterisationState {
    pub const DEFAULT: RasterisationState = RasterisationState {
        depth_clamp_enable: false,
        rasterizer_discard_enable: false,
        polygon_mode: PolygonMode::Fill,
        cull_mode: CullModeFlags::NONE,
        depth_bias: DepthBias::Disabled,
        front_face: FrontFace::Clockwise,
        line_width: unsafe { mem::transmute(1.0f32) },
    };
}

impl Default for RasterisationState {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
#[repr(C)]
pub struct Viewport {
    pub x: NotNan<f32>,
    pub y: NotNan<f32>,
    pub width: NotNan<f32>,
    pub height: NotNan<f32>,
    pub min_depth: NotNan<f32>,
    pub max_depth: NotNan<f32>,
}

impl From<(u32, u32)> for Viewport {
    fn from((w, h): (u32, u32)) -> Self {
        Viewport {
            x: 0.0.into(),
            y: 0.0.into(),
            width: (w as f32).into(),
            height: (h as f32).into(),
            min_depth: 0.0.into(),
            max_depth: 1.0.into(),
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
#[repr(C)]
pub struct ScissorRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Scissor {
    Enabled(ScissorRect),
    Disabled,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Viewports<'a> {
    Static(&'a [Viewport]),
    Dynamic,
}

impl<'a> From<Viewports<'a>> for ViewportsOwned {
    fn from(v: Viewports) -> Self {
        match v {
            Viewports::Static(v) => ViewportsOwned::Static(v.to_vec()),
            Viewports::Dynamic => ViewportsOwned::Dynamic,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum ViewportsOwned {
    Static(Vec<Viewport>),
    Dynamic,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum Scissors<'a> {
    Static(&'a [Scissor]),
    Dynamic,
}

impl<'a> From<Scissors<'a>> for ScissorsOwned {
    fn from(s: Scissors) -> Self {
        match s {
            Scissors::Static(s) => ScissorsOwned::Static(s.to_vec()),
            Scissors::Dynamic => ScissorsOwned::Dynamic,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum ScissorsOwned {
    Static(Vec<Scissor>),
    Dynamic,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ViewportState<'a> {
    pub viewports: Viewports<'a>,
    pub scissors: Scissors<'a>,
}

impl<'a> Default for ViewportState<'a> {
    fn default() -> Self {
        ViewportState {
            scissors: Scissors::Static(&[Scissor::Disabled]),
            viewports: Viewports::Dynamic,
        }
    }
}

impl<'a> ViewportState<'a> {
    pub const DYNAMIC_VIEWPORT_SCISSOR: ViewportState<'static> = ViewportState {
        viewports: Viewports::Dynamic,
        scissors: Scissors::Dynamic,
    };
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct InputAssemblyState {
    pub topology: PrimitiveTopology,
    pub primitive_restart_enable: bool,
}

impl Default for InputAssemblyState {
    fn default() -> Self {
        InputAssemblyState {
            topology: PrimitiveTopology::TriangleList,
            primitive_restart_enable: false,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum SampleShading {
    Disabled,
    Enabled { min_sample_shading: NotNan<f32> },
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct MultisampleState {
    pub rasterization_samples: u32,
    pub sample_shading: SampleShading,
    pub alpha_to_coverage_enable: bool,
    pub alpha_to_one_enable: bool,
}

impl Default for MultisampleState {
    fn default() -> Self {
        MultisampleState {
            rasterization_samples: 1,
            sample_shading: SampleShading::Disabled,
            alpha_to_coverage_enable: false,
            alpha_to_one_enable: false,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct AttachmentDescription {
    pub format: Format,
    pub samples: u32,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct AttachmentLayout<'a> {
    pub input_attachments: &'a [AttachmentDescription],
    pub depth_attachment: Option<AttachmentDescription>,
    pub color_attachments: &'a [AttachmentDescription],
    //pub resolve_attachments: &'a [AttachmentDescription]
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum CompareOp {
    Never = 0,
    Less = 1,
    Equal = 2,
    LessOrEqual = 3,
    Greater = 4,
    NotEqual = 5,
    GreaterOrEqual = 6,
    Always = 7,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum StencilOp {
    Keep = 0,
    Zero = 1,
    Replace = 2,
    IncrementAndClamp = 3,
    DecrementAndClamp = 4,
    Invert = 5,
    IncrementAndWrap = 6,
    DecrementAndWrap = 7,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct StencilOpState {
    pub fail_op: StencilOp,
    pub pass_op: StencilOp,
    pub depth_fail_op: StencilOp,
    pub compare_op: CompareOp,
    pub compare_mask: u32,
    pub write_mask: u32,
    pub reference: u32,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum DepthBoundTest {
    Disabled,
    Enabled {
        min_depth_bounds: NotNan<f32>,
        max_depth_bounds: NotNan<f32>,
    },
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum StencilTest {
    Disabled,
    Enabled {
        front: StencilOpState,
        back: StencilOpState,
    },
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct DepthStencilState {
    pub depth_test_enable: bool,
    pub depth_write_enable: bool,
    pub depth_compare_op: CompareOp,
    pub depth_bounds_test: DepthBoundTest,
    pub stencil_test: StencilTest,
}

impl Default for DepthStencilState {
    fn default() -> Self {
        DepthStencilState {
            depth_test_enable: false,
            depth_write_enable: false,
            depth_compare_op: CompareOp::Less,
            depth_bounds_test: DepthBoundTest::Disabled,
            stencil_test: StencilTest::Disabled,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum LogicOp {
    Clear = 0,
    And = 1,
    AndReverse = 2,
    Copy = 3,
    AndInverted = 4,
    NoOp = 5,
    Xor = 6,
    Or = 7,
    Nor = 8,
    Equivalent = 9,
    Invert = 10,
    OrReverse = 11,
    CopyInverted = 12,
    OrInverted = 13,
    Nand = 14,
    Set = 15,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum BlendFactor {
    Zero = 0,
    One = 1,
    SrcColor = 2,
    OneMinusSrcColor = 3,
    DstColor = 4,
    OneMinusDstColor = 5,
    SrcAlpha = 6,
    OneMinusSrcAlpha = 7,
    DstAlpha = 8,
    OneMinusDstAlpha = 9,
    ConstantColor = 10,
    OneMinusConstantColor = 11,
    ConstantAlpha = 12,
    OneMinusConstantAlpha = 13,
    SrcAlphaSaturate = 14,
    Src1Color = 15,
    OneMinusSrc1Color = 16,
    Src1Alpha = 17,
    OneMinusSrc1Alpha = 18,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum BlendOp {
    Add = 0,
    Subtract = 1,
    ReverseSubtract = 2,
    Min = 3,
    Max = 4,
}

bitflags! {
    pub struct ColorComponentFlags: u32 {
        const R = 0x0000_0001;
        const G = 0x0000_0002;
        const B = 0x0000_0004;
        const A = 0x0000_0008;
        const RGBA = Self::R.bits | Self::G.bits | Self::B.bits  | Self::A.bits;
        const ALL = Self::R.bits | Self::G.bits | Self::B.bits  | Self::A.bits;
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum ColorBlendAttachmentState {
    Disabled,
    Enabled {
        src_color_blend_factor: BlendFactor,
        dst_color_blend_factor: BlendFactor,
        color_blend_op: BlendOp,
        src_alpha_blend_factor: BlendFactor,
        dst_alpha_blend_factor: BlendFactor,
        alpha_blend_op: BlendOp,
        color_write_mask: ColorComponentFlags,
    },
}

impl ColorBlendAttachmentState {
    pub const DISABLED: ColorBlendAttachmentState = ColorBlendAttachmentState::Disabled;
    pub const ALPHA_BLENDING: ColorBlendAttachmentState = ColorBlendAttachmentState::Enabled {
        color_blend_op: BlendOp::Add,
        src_color_blend_factor: BlendFactor::SrcAlpha,
        dst_color_blend_factor: BlendFactor::OneMinusSrcAlpha,
        alpha_blend_op: BlendOp::Add,
        src_alpha_blend_factor: BlendFactor::One,
        dst_alpha_blend_factor: BlendFactor::Zero,
        color_write_mask: ColorComponentFlags::ALL,
    };
}

impl Default for ColorBlendAttachmentState {
    fn default() -> Self {
        ColorBlendAttachmentState::Disabled
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum ColorBlendAttachments<'a> {
    All(&'a ColorBlendAttachmentState),
    Separate(&'a [ColorBlendAttachmentState]),
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct ColorBlendState<'a> {
    pub logic_op: Option<LogicOp>,
    pub attachments: ColorBlendAttachments<'a>,
    pub blend_constants: [NotNan<f32>; 4],
}

impl<'a> ColorBlendState<'a> {
    pub const DISABLED: ColorBlendState<'static> = ColorBlendState {
        attachments: ColorBlendAttachments::All(&ColorBlendAttachmentState::Disabled),
        blend_constants: [unsafe { mem::transmute(0.0f32) }; 4],
        logic_op: None,
    };

    pub const ALPHA_BLENDING: ColorBlendState<'static> = ColorBlendState {
        attachments: ColorBlendAttachments::All(&ColorBlendAttachmentState::ALPHA_BLENDING),
        blend_constants: [unsafe { mem::transmute(0.0f32) }; 4],
        logic_op: None,
    };
}

#[derive(Copy, Clone)]
pub struct GraphicsPipelineCreateInfo<'a, 'b, B: Backend> {
    /// Shaders
    pub shader_stages: GraphicsShaderStages<'a, 'b, B>,
    pub viewport_state: ViewportState<'b>,
    pub rasterization_state: RasterisationState,
    pub multisample_state: MultisampleState,
    pub depth_stencil_state: DepthStencilState,
    pub input_assembly_state: InputAssemblyState,
    pub color_blend_state: ColorBlendState<'b>,
    //pub dynamic_state: DynamicStateFlags,
}

//--------------------------------------------------------------------------------------------------

/// Shader module.
///
/// We keep a reference to the SPIR-V bytecode for interface checking when building pipelines.
#[derive(derivative::Derivative)]
#[derivative(Copy(bound = ""), Clone(bound = ""), Debug(bound = ""))]
pub struct ShaderModule<'a, 're, B: Backend> {
    pub(crate) module: &'a B::ShaderModule,
    pub(crate) reflection: &'re ShaderStageReflection<'re>,
}

impl<'a, 're, B: Backend> ShaderModule<'a, 're, B> {
    pub fn inner(&self) -> &'a B::ShaderModule {
        self.module
    }

    pub fn reflection(&self) -> &'re ShaderStageReflection<'re> {
        self.reflection
    }
}

/*
impl<'a, B: Backend, T: PipelineInterface<'a, B>> GraphicsPipeline<'a,B,T> {
    pub fn root_signature(&self) -> PipelineSignatureTypeless<'a, B> {
        PipelineSignatureTypeless(self.0.signature)
    }
}*/

/*
// Type erasure for GraphicsPipelines
impl<'a, B: Backend, T: PipelineInterface<'a, B>> From<GraphicsPipeline<'a, B, T>>
    for GraphicsPipelineTypeless<'a, B>
{
    fn from(pipeline: GraphicsPipeline<'a, B, T>) -> Self {
        pipeline.0
    }
}*/

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct VertexInputBinding<'a> {
    pub layout: VertexLayout<'a>,
    pub rate: VertexInputRate,
    pub base_location: Option<u32>,
}

/// Describes the contents (all arguments) of an argument block.
///
/// This can be seen as the 'layout' or 'format' of an argument block.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct SignatureDescription<'a> {
    /// Signatures of inherited argument blocks.
    ///
    /// The length of this slice defines the number of _inherited argument blocks_.
    pub inherited: &'a [&'a SignatureDescription<'a>],

    /// Descriptors in the block.
    ///
    /// The length of this slice defines the number of _descriptors_ in a block.
    pub descriptors: &'a [ResourceBinding<'a>],

    /// Layouts of all vertex buffers in the block.
    ///
    /// The length of this slice defines the number of _vertex buffers_ in a block.
    pub vertex_inputs: &'a [VertexInputBinding<'a>],

    /// (Color) outputs of the fragment shader. The block contains one _render target_ image for
    /// each entry.
    ///
    /// The length of this slice defines the number of _render targets_ in a block.
    pub fragment_outputs: &'a [FragmentOutputDescription],

    /// Depth-stencil output of the fragment shader. If not `None` then the block contains a
    /// depth-stencil render target image.
    pub depth_stencil_fragment_output: Option<FragmentOutputDescription>,

    /// The format of the index buffer. If not `None`, then the block contains an index buffer.
    pub index_format: Option<IndexFormat>,

    /// The number of viewports defined in the block.
    ///
    /// At most one signature in a signature tree can have a non-zero number of viewports.
    /// Equivalently, you cannot split the definition of viewports across several argument blocks,
    /// and when an argument block defines viewports, it must define all of them at once.
    ///
    /// FIXME: actually check that
    pub num_viewports: usize,

    /// The number of scissors defined in the block.
    ///
    /// This follows the same rule as `num_viewports`.
    pub num_scissors: usize,

    /// Indicates that this block and its inherited blocks fully define the outputs of a fragment shader.
    ///
    /// An inheriting block must not define additional fragment outputs in the `fragment_outputs`
    /// and `depth_stencil_fragment_output` members.
    ///
    /// The purpose of this flag is to allow backends that need _framebuffer objects_ (e.g. OpenGL or Vulkan)
    /// to create them in advance and store them inside long-lived argument blocks
    /// instead of creating them on-the-fly.
    pub is_root_fragment_output_signature: bool,

    /// Indicates that this block and its inherited blocks fully define the inputs of a vertex shader.
    ///
    /// An inheriting block must not define additional vertex inputs in `vertex_layouts`.
    pub is_root_vertex_input_signature: bool,
}

impl<'a> SignatureDescription<'a> {
    pub const EMPTY: SignatureDescription<'static> = SignatureDescription {
        inherited: &[],
        descriptors: &[],
        vertex_inputs: &[],
        fragment_outputs: &[],
        depth_stencil_fragment_output: None,
        index_format: None,
        num_viewports: 0,
        num_scissors: 0,
        is_root_fragment_output_signature: false,
        is_root_vertex_input_signature: false,
    };

    pub const fn empty() -> SignatureDescription<'static> {
        Self::EMPTY
    }

    /// Count the total number of viewport entries.
    pub fn count_viewports(&self) -> usize {
        self.num_viewports
            + self
                .inherited
                .iter()
                .map(|&s| s.count_viewports())
                .sum::<usize>()
    }

    /// Count the total number of scissor entries.
    pub fn count_scissors(&self) -> usize {
        self.num_scissors
            + self
                .inherited
                .iter()
                .map(|&s| s.count_scissors())
                .sum::<usize>()
    }
}

pub trait Signature<'a, B: Backend>: Copy + Clone + Debug {
    fn inner(&self) -> &'a B::Signature;
    fn description(&self) -> &SignatureDescription;
}

#[derive(derivative::Derivative)]
#[derivative(Copy(bound = ""), Clone(bound = ""), Debug(bound = ""))]
pub struct TypedSignature<'a, B: Backend, T: Arguments<'a, B>>(
    pub(crate) &'a B::Signature,
    pub(crate) PhantomData<&'a T>,
);

impl<'a, B: Backend, T: Arguments<'a, B>> Signature<'a, B> for TypedSignature<'a, B, T> {
    fn inner(&self) -> &'a B::Signature {
        self.0
    }
    fn description(&self) -> &SignatureDescription {
        T::SIGNATURE
    }
}

/// Argument block.
///
/// An _argument block_ is a set of GPU states that are set before executing a command. They can be
/// seen as a block of arguments for draw or compute commands.
/// They are typically created from an object implementing the [Arguments] trait, but can be created
/// manually if necessary (e.g. when the interface to a shader is not known until runtime).
///
/// The contents of an argument block is described by a [Signature].
/// See also [SignatureDescription].
#[derive(derivative::Derivative)]
#[derivative(Copy(bound = ""), Clone(bound = ""), Debug(bound = ""))]
pub struct ArgumentBlock<'a, B: Backend, S: Signature<'a, B>> {
    pub(crate) arguments: &'a B::ArgumentBlock,
    pub(crate) signature: S,
}

/// Type alias for argument blocks with a statically known signature.
pub type TypedArgumentBlock<'a, B, T> = ArgumentBlock<'a, B, TypedSignature<'a, B, T>>;

/// Argument block without an associated signature.
#[derive(derivative::Derivative)]
#[derivative(Copy(bound = ""), Clone(bound = ""), Debug(bound = ""))]
pub struct BareArgumentBlock<'a, B: Backend>(pub &'a B::ArgumentBlock);

impl<'a, B: Backend, S: Signature<'a, B>> From<ArgumentBlock<'a, B, S>>
    for BareArgumentBlock<'a, B>
{
    fn from(b: ArgumentBlock<'a, B, S>) -> Self {
        BareArgumentBlock(b.arguments)
    }
}

/// Graphics pipeline.
#[derive(derivative::Derivative)]
#[derivative(Copy(bound = ""), Clone(bound = ""), Debug(bound = ""))]
pub struct GraphicsPipeline<'a, B: Backend, S: Signature<'a, B>> {
    pub(crate) inner: &'a B::GraphicsPipeline,
    pub(crate) signature: S,
}

/// Graphics pipeline without an associated signature.
#[derive(derivative::Derivative)]
#[derivative(Copy(bound = ""), Clone(bound = ""), Debug(bound = ""))]
pub struct GraphicsPipelineTypeless<'a, B: Backend>(pub(crate) &'a B::GraphicsPipeline);

/// Type alias for argument blocks with a statically known signature.
pub type TypedGraphicsPipeline<'a, B, T> = GraphicsPipeline<'a, B, TypedSignature<'a, B, T>>;

/// Trait for types that can be converted into an argument block.
pub trait IntoArgumentBlock<'a, B: Backend, S: Signature<'a, B>> {
    fn into_block(self, signature: S, arena: &'a Arena<B>) -> ArgumentBlock<'a, B, S>;
}

///
/// Describes pipeline states to set before issuing a draw or compute call.
///
/// Types implementing the [Arguments] trait contain the following pieces of information:
/// * descriptors to bind to the pipeline
///     * uniform buffers
///     * storage buffers
///     * sampled images
///     * storage images
///     * etc.
/// * vertex buffers
/// * index buffer
/// * render targets (color, depth and stencil)
/// * viewports
/// * scissor rectangles
/// * inherited argument blocks
///
/// They provide the [into_block] methods for turning them into a form optimized for GPU submission
/// (see [ArgumentBlock]).
///
/// Common sets of arguments can be reused and shared via _inherited argument blocks_.
/// It is advised to create different argument blocks for arguments that are shared by many commands,
/// and depending on the update frequency of the data they refer to.
/// For instance, one would typically create a separate argument block for render targets
/// and keep it across frames as render targets change infrequently.
///
/// #### Custom derive
/// It is possible to automatically derive the [Arguments] traits for structs. E.g:
///
///```
/// #[derive(Arguments)]
/// #[argument(backend="B")]
/// pub struct ExampleInterface<'a> {
///    #[argument(render_target)]
///    pub color_target: Image<'a>,
///    #[argument(uniform_buffer)]
///    pub per_frame: Buffer<'a, PerFrameParams>,
///    #[argument(uniform_buffer)]
///    pub per_object: Buffer<'a, PerObjectParams>,
///    #[argument(viewport)]
///    pub viewport: Viewport,
///    #[argument(vertex_buffer)]
///    pub vertex_buffer: Buffer<'a, [Vertex]>,
/// }
/// ```
///
/// In that case, if the render target is shared between different pipeline interfaces, it is better
/// to put it in a separate argument block:
///
/// ```
/// #[derive(Arguments)]
/// #[argument(backend="B")]
/// pub struct RenderTargets<'a> {
///    #[argument(render_target)]
///    pub color_target: Image<'a>,
/// }
///
/// #[derive(Arguments)]
/// #[argument(backend="B")]
/// pub struct ExampleA<'a> {
///    #[argument(inherit)]
///    pub render_targets: RenderTargets<'a>,
///    // ...
/// }
///
/// #[derive(Arguments)]
/// #[argument(backend="B")]
/// pub struct ExampleB<'a> {
///    #[argument(inherit)]
///    pub render_targets: RenderTargets<'a>,
///    // ...
/// }
/// ```
///
/// TODO document more
pub trait Arguments<'a, B: Backend>: Sized {
    const SIGNATURE: &'static SignatureDescription<'static>;

    /// A 'static marker type that uniquely identifies Self: this is for getting a TypeId.
    type UniqueType: 'static;
    type IntoInterface: Arguments<'a, B> + 'a;

    fn get_inherited_signatures(_renderer: &'a Api<B>) -> Vec<&'a B::Signature> {
        vec![]
    }

    fn into_block(
        self,
        signature: TypedSignature<'a, B, Self::IntoInterface>,
        arena: &'a Arena<B>,
    ) -> ArgumentBlock<'a, B, TypedSignature<'a, B, Self::IntoInterface>>;
}

impl<'a, B: Backend, A: Arguments<'a, B>>
    IntoArgumentBlock<'a, B, TypedSignature<'a, B, A::IntoInterface>> for A
{
    fn into_block(
        self,
        signature: TypedSignature<'a, B, A::IntoInterface>,
        arena: &'a Arena<B>,
    ) -> ArgumentBlock<'a, B, TypedSignature<'a, B, A::IntoInterface>> {
        A::into_block(self, signature, arena)
    }
}

impl<'a, B: Backend, P: Arguments<'a, B>> Arguments<'a, B>
    for ArgumentBlock<'a, B, TypedSignature<'a, B, P>>
{
    const SIGNATURE: &'static SignatureDescription<'static> = P::SIGNATURE;
    type UniqueType = P::UniqueType;
    type IntoInterface = P;

    fn get_inherited_signatures(renderer: &'a Api<B>) -> Vec<&'a B::Signature> {
        P::get_inherited_signatures(renderer)
    }

    fn into_block(
        self,
        _signature: TypedSignature<'a, B, P>,
        _arena: &'a Arena<B>,
    ) -> ArgumentBlock<'a, B, TypedSignature<'a, B, P>> {
        self.into()
    }
}

//--------------------------------------------------------------------------------------------------

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct VertexInputAttributeDescription<'tcx> {
    pub location: Option<u32>,
    pub ty: &'tcx TypeDesc<'tcx>,
    pub semantic: Option<Semantic<'tcx>>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct FragmentOutputDescription {
    // nothing yet, we just care about the count
}

/// Shader reflection information for one stage.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ShaderStageReflection<'a> {
    pub stage: ShaderStageFlags,
    pub descriptors: &'a [ResourceBinding<'a>],
    pub vertex_input_attributes: &'a [VertexInputAttributeDescription<'a>],
    pub fragment_outputs: &'a [FragmentOutputDescription],
}

/// Shader bytecode and reflection information.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ReflectedShader<'bc, 're> {
    pub bytecode: &'bc [u8],
    pub reflection: &'re ShaderStageReflection<'re>,
}

//--------------------------------------------------------------------------------------------------

// not good: this borrows the builder, cannot be stored in a struct
#[derive(derivative::Derivative)]
#[derivative(Copy(bound = ""), Clone(bound = ""), Debug(bound = ""))]
pub struct DynamicSignature<'a, B: Backend> {
    description: &'a SignatureDescription<'a>,
    raw: &'a B::Signature,
}

pub struct DynamicSignatureBuilder<'a, B: Backend> {
    inherited: Vec<&'a SignatureDescription<'a>>,
    inherited_signatures: Vec<&'a B::Signature>,
    descriptors: Vec<ResourceBinding<'a>>,
    vertex_inputs: Vec<VertexInputBinding<'a>>,
    fragment_outputs: Vec<FragmentOutputDescription>,
    depth_stencil_fragment_output: Option<FragmentOutputDescription>,
    index_format: Option<IndexFormat>,
    num_viewports: usize,
    num_scissors: usize,
    is_root_fragment_output_signature: bool,
    is_root_vertex_input_signature: bool,
}

impl<'a, B: Backend> DynamicSignatureBuilder<'a, B> {
    pub fn new() -> DynamicSignatureBuilder<'a, B> {
        DynamicSignatureBuilder {
            inherited: Vec::new(),
            inherited_signatures: Vec::new(),
            descriptors: Vec::new(),
            vertex_inputs: Vec::new(),
            fragment_outputs: Vec::new(),
            depth_stencil_fragment_output: None,
            index_format: None,
            num_viewports: 1,
            num_scissors: 0,
            is_root_fragment_output_signature: false,
            is_root_vertex_input_signature: false,
        }
    }
    pub fn inherited(&mut self, sig: &'a impl Signature<'a, B>) -> &mut Self {
        self.inherited.push(sig.description());
        self.inherited_signatures.push(sig.inner());
        self
    }
    pub fn descriptor(&mut self, d: ResourceBinding<'a>) -> &mut Self {
        self.descriptors.push(d);
        self
    }
    pub fn vertex_input(&mut self, vi: VertexInputBinding<'a>) -> &mut Self {
        self.is_root_vertex_input_signature = true;
        self.vertex_inputs.push(vi);
        self
    }
    pub fn viewport_count(&mut self, count: usize) -> &mut Self {
        self.num_viewports = count;
        self
    }
    pub fn scissor_count(&mut self, count: usize) -> &mut Self {
        self.num_scissors = count;
        self
    }
    pub fn index_format(&mut self, format: IndexFormat) -> &mut Self {
        self.is_root_vertex_input_signature = true;
        self.index_format = Some(format);
        self
    }
    pub fn fragment_output(&mut self, frag: FragmentOutputDescription) -> &mut Self {
        self.is_root_fragment_output_signature = true;
        self.fragment_outputs.push(frag);
        self
    }
    pub fn depth_stencil_fragment_output(&mut self, ds: FragmentOutputDescription) -> &mut Self {
        self.is_root_fragment_output_signature = true;
        self.depth_stencil_fragment_output = Some(ds);
        self
    }
    pub fn root_fragment_output_signature(&mut self, is: bool) -> &mut Self {
        self.is_root_fragment_output_signature = is;
        self
    }
    pub fn root_vertex_input_signature(&mut self, is: bool) -> &mut Self {
        self.is_root_vertex_input_signature = is;
        self
    }

    // not good: borrows builder, result cannot be stored in a struct
    // dynamicsignature should own stuff (box? Rc?)
    pub fn build(&self, arena: &'a Arena<B>) -> DynamicSignature<'a, B> {
        let inherited = arena.misc.alloc_extend(self.inherited.iter().cloned());
        let descriptors = arena.misc.alloc_extend(self.descriptors.iter().cloned());
        let vertex_inputs = arena.misc.alloc_extend(self.vertex_inputs.iter().cloned());
        let fragment_outputs = arena
            .misc
            .alloc_extend(self.fragment_outputs.iter().cloned());

        let description = arena.misc.alloc(SignatureDescription {
            inherited,
            descriptors,
            vertex_inputs,
            fragment_outputs,
            depth_stencil_fragment_output: self.depth_stencil_fragment_output,
            index_format: self.index_format,
            num_viewports: self.num_viewports,
            num_scissors: self.num_scissors,
            is_root_fragment_output_signature: self.is_root_fragment_output_signature,
            is_root_vertex_input_signature: self.is_root_vertex_input_signature,
        });
        let raw = arena.create_signature(&self.inherited_signatures, description);

        DynamicSignature { description, raw }
    }
}

impl<'a, B: Backend> Signature<'a, B> for DynamicSignature<'a, B> {
    fn inner(&self) -> &'a B::Signature {
        self.raw
    }

    fn description(&self) -> &SignatureDescription {
        self.description
    }
}

/// FIXME we are filling Vecs and option when we could be filling descriptors directly in the
/// allocated space by the backend.
/// This is because the current interface needs all params at the same time.
/// Maybe a slightly less safe approach would be better here.
/// (trait ArgBlock in backend: methods to set a parameter slot + finalize)
///
/// Should this implement IntoArgumentBlock?
pub struct DynamicArgumentBlockBuilder<'a, B: Backend> {
    signature: DynamicSignature<'a, B>,
    inherited: Vec<BareArgumentBlock<'a, B>>,
    descriptors: Vec<Descriptor<'a, B>>,
    vertex_buffers: Vec<VertexBufferView<'a, B>>,
    index_buffer: Option<IndexBufferView<'a, B>>,
    render_targets: Vec<RenderTargetView<'a, B>>,
    depth_stencil_target: Option<DepthStencilView<'a, B>>,
    viewports: Vec<Viewport>,
    scissors: Vec<Scissor>,
}

impl<'a, B: Backend> DynamicArgumentBlockBuilder<'a, B> {
    pub fn new(signature: DynamicSignature<'a, B>) -> DynamicArgumentBlockBuilder<'a, B> {
        DynamicArgumentBlockBuilder {
            signature,
            inherited: Vec::new(),
            descriptors: Vec::new(),
            vertex_buffers: Vec::new(),
            index_buffer: None,
            render_targets: Vec::new(),
            depth_stencil_target: None,
            viewports: Vec::new(),
            scissors: Vec::new(),
        }
    }

    pub fn inherited<S: Signature<'a, B>>(&mut self, args: ArgumentBlock<'a, B, S>) -> &mut Self {
        self.inherited.push(args.into());
        self
    }
    pub fn descriptor(&mut self, d: Descriptor<'a, B>) -> &mut Self {
        self.descriptors.push(d);
        self
    }
    pub fn vertex_buffer<V: VertexData>(&mut self, vb: Buffer<'a, B, [V]>) -> &mut Self {
        self.vertex_buffers.push(vb.into());
        self
    }
    pub fn viewport(&mut self, v: Viewport) -> &mut Self {
        self.viewports.push(v);
        self
    }
    pub fn scissor(&mut self, s: Scissor) -> &mut Self {
        self.scissors.push(s);
        self
    }
    pub fn index_buffer<I: IndexData>(&mut self, ib: Buffer<'a, B, [I]>) -> &mut Self {
        self.index_buffer = Some(ib.into());
        self
    }
    pub fn render_target(&mut self, rtv: RenderTargetView<'a, B>) -> &mut Self {
        self.render_targets.push(rtv);
        self
    }
    pub fn depth_stencil_target(&mut self, ds: DepthStencilView<'a, B>) -> &mut Self {
        self.depth_stencil_target = Some(ds);
        self
    }
}

impl<'a, 'b, B: Backend> IntoArgumentBlock<'a, B, DynamicSignature<'a, B>>
    for DynamicArgumentBlockBuilder<'a, B>
{
    fn into_block(
        self,
        signature: DynamicSignature<'a, B>,
        arena: &'a Arena<B>,
    ) -> ArgumentBlock<'a, B, DynamicSignature<'a, B>> {
        // comparing the signatures would also work, but this is faster
        assert_eq!(signature.raw as *const _, self.signature.raw as *const _);
        arena.create_argument_block(
            signature,
            self.inherited.into_iter(),
            self.descriptors.into_iter(),
            self.vertex_buffers.into_iter(),
            self.index_buffer,
            self.render_targets.into_iter(),
            self.depth_stencil_target,
            self.viewports.into_iter(),
            self.scissors.into_iter(),
        )
    }
}
