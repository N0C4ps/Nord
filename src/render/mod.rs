use nord_app::{
    ApplicationBufferDesc, ApplicationBufferUsage, ApplicationClearColor,
    ApplicationColorAttachmentBinding, ApplicationCompiledShader, ApplicationDiagnosticError,
    ApplicationErrorContext, ApplicationGraphicsPipelineDesc, ApplicationPipelineDescriptorBinding,
    ApplicationPlatform, ApplicationRenderConfig, ApplicationRenderQueue,
    ApplicationRenderTargetDesc, ApplicationRenderer, ApplicationResourceUsage,
    ApplicationShaderCompileOptions, ApplicationShaderCompiler, ApplicationShaderStage,
    ApplicationTextureFormat, ApplicationVertexAttribute, ApplicationVertexBufferBinding,
    ApplicationVertexFormat, ApplicationViewport, ApplicationWindowHandle,
};
use std::path::PathBuf;

const CLEAR_COLOR: ApplicationClearColor = ApplicationClearColor::rgba(0.025, 0.035, 0.055, 1.0);
const TRIANGLE_VERTEX_COUNT: u32 = 3;
const TRIANGLE_VERTEX_STRIDE: u32 = 12;

/// Surface-sized game output managed by the game renderer.
#[derive(Clone, Copy)]
struct GameSurface {
    color: nord_app::ApplicationGpuResourceHandle,
    view: nord_app::ApplicationDescriptorHandle,
    width: u32,
    height: u32,
}

/// Game-owned policy that connects Nord's generic renderer to the app loop.
pub(super) struct GameRenderer {
    renderer: Option<ApplicationRenderer>,
    surface: GameSurface,
    vertex_buffer: nord_app::ApplicationGpuResourceHandle,
    pipeline: ApplicationGraphicsPipelineDesc,
    vertex_shader: ApplicationCompiledShader,
    pixel_shader: ApplicationCompiledShader,
}

impl GameRenderer {
    pub(super) fn initialize(
        platform: &ApplicationPlatform,
        window: ApplicationWindowHandle,
        width: u32,
        height: u32,
    ) -> nord_app::ApplicationResult<Self> {
        let mut renderer = ApplicationRenderer::initialize(
            platform,
            window,
            ApplicationRenderConfig::new(width, height),
        )
        .map_err(ApplicationDiagnosticError::msg)
        .context("initializing Nord's graphics infrastructure")?;

        let shader_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("assets")
            .join("shaders")
            .join("nord_triangle.hlsl");
        let compiler = ApplicationShaderCompiler::default();
        let vertex_shader = compiler
            .compile(&ApplicationShaderCompileOptions::new(
                &shader_path,
                ApplicationShaderStage::Vertex,
                "VSMain",
            ))
            .map_err(ApplicationDiagnosticError::msg)
            .context("compiling the game's triangle vertex shader")?;
        let pixel_shader = compiler
            .compile(&ApplicationShaderCompileOptions::new(
                &shader_path,
                ApplicationShaderStage::Pixel,
                "PSMain",
            ))
            .map_err(ApplicationDiagnosticError::msg)
            .context("compiling the game's triangle pixel shader")?;

        let pipeline = ApplicationGraphicsPipelineDesc::new(
            vertex_shader.shader_id,
            pixel_shader.shader_id,
            ApplicationTextureFormat::Rgba8Unorm,
        )
        .with_vertex_attribute(ApplicationVertexAttribute {
            semantic: "POSITION".into(),
            semantic_index: 0,
            format: ApplicationVertexFormat::Float3,
            input_slot: 0,
            byte_offset: 0,
            per_instance: false,
            instance_step_rate: 0,
        });
        renderer
            .create_graphics_pipeline(&pipeline, &vertex_shader, &pixel_shader, 1)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the game's triangle graphics pipeline")?;

        let vertices = [[0.0_f32, 0.72, 0.0], [-0.68, -0.55, 0.0], [0.68, -0.55, 0.0]];
        let vertex_bytes =
            vertices.iter().flatten().flat_map(|value| value.to_le_bytes()).collect::<Vec<_>>();
        let vertex_buffer = renderer
            .create_buffer(ApplicationBufferDesc::new(
                vertex_bytes.len() as u64,
                ApplicationBufferUsage::COPY_DESTINATION.union(ApplicationBufferUsage::VERTEX),
            )?)
            .map_err(ApplicationDiagnosticError::msg)
            .context("allocating the game's triangle vertex buffer")?;
        renderer
            .upload_buffer(vertex_buffer, 0, &vertex_bytes)
            .map_err(ApplicationDiagnosticError::msg)
            .context("uploading the game's triangle vertices")?;

        let surface = Self::create_surface(&mut renderer, width, height)?;
        let allocator_stats = renderer.gpu_allocator_stats();
        nord_app::debug!(
            target: "nord::render",
            reserved_gpu_bytes = allocator_stats.reserved_bytes,
            "Renderer resource bookkeeping services are ready"
        );
        nord_app::info!(
            name: "triangle_render_path_initialized",
            target: "nord::render",
            backend = ?renderer.backend(),
            "Game triangle pipeline and render graph are ready"
        );
        Ok(Self {
            renderer: Some(renderer),
            surface,
            vertex_buffer,
            pipeline,
            vertex_shader,
            pixel_shader,
        })
    }

    fn create_surface(
        renderer: &mut ApplicationRenderer,
        width: u32,
        height: u32,
    ) -> nord_app::ApplicationResult<GameSurface> {
        let target = renderer
            .create_render_target(ApplicationRenderTargetDesc::new(
                width,
                height,
                Some(ApplicationTextureFormat::Rgba8Unorm),
                None,
            )?)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the game's surface-sized render target")?;
        let color = target.color.expect("color target was requested");
        let view = renderer
            .create_render_target_view(color)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the game's render-target view")?;
        Ok(GameSurface { color, view, width, height })
    }

    pub(super) fn render_frame(&mut self) -> nord_app::ApplicationResult<()> {
        let renderer = self
            .renderer
            .as_mut()
            .ok_or_else(|| ApplicationDiagnosticError::msg("renderer has already shut down"))?;
        let surface = self.surface;
        let vertex_buffer = self.vertex_buffer;
        let pipeline = self.pipeline.clone();
        let vertex_shader = self.vertex_shader.clone();
        let pixel_shader = self.pixel_shader.clone();
        let vertex_size = TRIANGLE_VERTEX_COUNT * TRIANGLE_VERTEX_STRIDE;
        let viewport_width = exact_surface_extent(surface.width)?;
        let viewport_height = exact_surface_extent(surface.height)?;
        let mut queue = ApplicationRenderQueue::new();
        queue
            .enqueue(
                nord_app::ApplicationRenderPassDesc::new("Nord game triangle")
                    .use_resource(surface.color, ApplicationResourceUsage::ColorAttachmentWrite)
                    .use_resource(vertex_buffer, ApplicationResourceUsage::VertexBufferRead),
                move |encoder| {
                    encoder.begin_graphics_pass_with_attachments(
                        &[ApplicationColorAttachmentBinding {
                            resource: surface.color,
                            view: surface.view,
                            clear_color: CLEAR_COLOR,
                        }],
                        None,
                        1.0,
                        Some(ApplicationViewport::new(0.0, 0.0, viewport_width, viewport_height)),
                        None,
                    )?;
                    encoder.draw(
                        &pipeline,
                        &vertex_shader,
                        &pixel_shader,
                        &[] as &[ApplicationPipelineDescriptorBinding],
                        &[ApplicationVertexBufferBinding {
                            slot: 0,
                            buffer: vertex_buffer,
                            offset: 0,
                            size: vertex_size,
                            stride: TRIANGLE_VERTEX_STRIDE,
                        }],
                        None,
                        nord_app::ApplicationDrawCall::Vertices {
                            vertex_count: 3,
                            instance_count: 1,
                            first_vertex: 0,
                            first_instance: 0,
                        },
                    )?;
                    encoder.end_graphics_pass()?;
                    Ok(())
                },
            )
            .map_err(|error| ApplicationDiagnosticError::msg(format!("{error:?}")))
            .context("encoding the game's triangle render pass")?;
        renderer
            .submit_render_queue(&mut queue)
            .map_err(|error| ApplicationDiagnosticError::msg(format!("{error:?}")))
            .context("submitting the game's triangle render graph")?;
        renderer
            .present_texture(surface.color)
            .map_err(ApplicationDiagnosticError::msg)
            .context("presenting the game's rendered triangle")
    }

    pub(super) fn wait_for_frame(&self) -> nord_app::ApplicationResult<()> {
        self.renderer
            .as_ref()
            .ok_or_else(|| ApplicationDiagnosticError::msg("renderer has already shut down"))?
            .wait_for_frame()
            .map_err(ApplicationDiagnosticError::msg)
            .context("waiting for graphics frame readiness")
    }

    pub(super) fn resize(&mut self, width: u32, height: u32) -> nord_app::ApplicationResult<()> {
        let renderer = self
            .renderer
            .as_mut()
            .ok_or_else(|| ApplicationDiagnosticError::msg("renderer has already shut down"))?;
        if width == 0 || height == 0 {
            return renderer
                .resize(width, height)
                .map_err(ApplicationDiagnosticError::msg)
                .context("suspending the Nord render surface");
        }
        if width == self.surface.width && height == self.surface.height {
            return Ok(());
        }
        renderer
            .destroy_resource(self.surface.color)
            .map_err(ApplicationDiagnosticError::msg)
            .context("retiring the previous game render target")?;
        renderer
            .resize(width, height)
            .map_err(ApplicationDiagnosticError::msg)
            .context("resizing the Nord render surface")?;
        self.surface = Self::create_surface(renderer, width, height)?;
        Ok(())
    }

    pub(super) fn surface_suspended(&self) -> bool {
        self.renderer.as_ref().is_none_or(ApplicationRenderer::surface_suspended)
    }

    pub(super) fn shutdown(&mut self) -> nord_app::ApplicationResult<()> {
        let Some(mut renderer) = self.renderer.take() else {
            return Ok(());
        };
        renderer
            .wait_idle()
            .map_err(ApplicationDiagnosticError::msg)
            .context("waiting for graphics work before game renderer shutdown")?;
        renderer
            .destroy_resource(self.surface.color)
            .map_err(ApplicationDiagnosticError::msg)
            .context("destroying the game's render target")?;
        renderer
            .destroy_resource(self.vertex_buffer)
            .map_err(ApplicationDiagnosticError::msg)
            .context("destroying the game's triangle vertex buffer")?;
        Ok(())
    }
}

fn exact_surface_extent(extent: u32) -> nord_app::ApplicationResult<f32> {
    const MAX_EXACT_INTEGER_F32: u32 = 1 << 24;
    if extent > MAX_EXACT_INTEGER_F32 {
        return Err(ApplicationDiagnosticError::msg(
            "render surface extent exceeds exact floating-point viewport precision",
        ));
    }
    #[allow(clippy::cast_precision_loss)]
    Ok(extent as f32)
}
