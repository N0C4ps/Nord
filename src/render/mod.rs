use crate::camera::GameCameraController;
use nord_app::{
    ApplicationBufferDesc, ApplicationBufferMemory, ApplicationBufferUsage, ApplicationClearColor,
    ApplicationColorAttachmentBinding, ApplicationCompiledShader, ApplicationDescriptorHandle,
    ApplicationDiagnosticError, ApplicationErrorContext, ApplicationGraphicsPipelineDesc,
    ApplicationPipelineDescriptorBinding, ApplicationPlatform, ApplicationRenderConfig,
    ApplicationRenderQueue, ApplicationRenderTargetDesc, ApplicationRenderer,
    ApplicationResourceUsage, ApplicationShaderBindingKind, ApplicationShaderCompileOptions,
    ApplicationShaderCompiler, ApplicationShaderStage, ApplicationTextureFormat,
    ApplicationVertexAttribute, ApplicationVertexBufferBinding, ApplicationVertexFormat,
    ApplicationViewport, ApplicationWindowHandle,
};
use nord_camera::PerspectiveCamera;
use nord_math::{Mat4, Quat, Vec3};
use nord_mesh::{IndexedMesh, MeshVertex};
use std::path::PathBuf;

const CLEAR_COLOR: ApplicationClearColor = ApplicationClearColor::rgba(0.025, 0.035, 0.055, 1.0);
const VERTEX_STRIDE: u32 = 24;
const CONSTANT_BUFFER_BYTES: usize = 256;

/// Surface-sized game output and its depth buffer.
#[derive(Clone, Copy)]
struct GameSurface {
    color: nord_app::ApplicationGpuResourceHandle,
    color_view: ApplicationDescriptorHandle,
    depth: nord_app::ApplicationGpuResourceHandle,
    depth_view: ApplicationDescriptorHandle,
    width: u32,
    height: u32,
}

/// Game-owned policy that connects Nord's generic renderer to the app loop.
pub(super) struct GameRenderer {
    renderer: Option<ApplicationRenderer>,
    surface: GameSurface,
    vertex_buffer: nord_app::ApplicationGpuResourceHandle,
    vertex_buffer_size: u32,
    index_buffer: nord_app::ApplicationGpuResourceHandle,
    index_buffer_size: u32,
    index_count: u32,
    camera_buffer: nord_app::ApplicationGpuResourceHandle,
    camera_binding: ApplicationPipelineDescriptorBinding,
    camera: GameCameraController,
    model: Mat4,
    pipeline: ApplicationGraphicsPipelineDesc,
    vertex_shader: ApplicationCompiledShader,
    pixel_shader: ApplicationCompiledShader,
}

struct MeshGpuResources {
    vertex_buffer: nord_app::ApplicationGpuResourceHandle,
    vertex_buffer_size: u32,
    index_buffer: nord_app::ApplicationGpuResourceHandle,
    index_buffer_size: u32,
    index_count: u32,
    vertex_count: usize,
}

impl GameRenderer {
    pub(super) fn initialize(
        platform: &ApplicationPlatform,
        window: ApplicationWindowHandle,
        width: u32,
        height: u32,
    ) -> nord_app::ApplicationResult<Self> {
        let aspect_ratio = surface_aspect(width, height)?;
        let camera = GameCameraController::new(create_game_camera(aspect_ratio)?, window);
        let mut renderer = ApplicationRenderer::initialize(
            platform,
            window,
            ApplicationRenderConfig::new(width, height),
        )
        .map_err(ApplicationDiagnosticError::msg)
        .context("initializing Nord's graphics infrastructure")?;
        let (vertex_shader, pixel_shader) = Self::compile_mesh_shaders()?;
        let pipeline = Self::create_mesh_pipeline(&mut renderer, &vertex_shader, &pixel_shader)?;
        let mesh = make_test_cube()?;
        let mesh_gpu = Self::upload_test_mesh(&mut renderer, &mesh)?;
        let (camera_buffer, camera_binding) = Self::create_camera_buffer(&mut renderer)?;
        let surface = Self::create_surface(&mut renderer, width, height)?;
        let model = test_cube_transform()?;
        let allocator_stats = renderer.gpu_allocator_stats();
        nord_app::debug!(
            target: "nord::render",
            reserved_gpu_bytes = allocator_stats.reserved_bytes,
            "Renderer resource bookkeeping services are ready"
        );
        nord_app::info!(
            name: "mesh_preview_render_path_initialized",
            target: "nord::render",
            backend = ?renderer.backend(),
            vertices = mesh_gpu.vertex_count,
            indices = mesh_gpu.index_count,
            "Game perspective camera, indexed mesh, and depth-tested pass are ready"
        );
        Ok(Self {
            renderer: Some(renderer),
            surface,
            vertex_buffer: mesh_gpu.vertex_buffer,
            vertex_buffer_size: mesh_gpu.vertex_buffer_size,
            index_buffer: mesh_gpu.index_buffer,
            index_buffer_size: mesh_gpu.index_buffer_size,
            index_count: mesh_gpu.index_count,
            camera_buffer,
            camera_binding,
            camera,
            model,
            pipeline,
            vertex_shader,
            pixel_shader,
        })
    }

    fn compile_mesh_shaders()
    -> nord_app::ApplicationResult<(ApplicationCompiledShader, ApplicationCompiledShader)> {
        let shader_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("assets")
            .join("shaders")
            .join("nord_mesh_preview.hlsl");
        let compiler = ApplicationShaderCompiler::default();
        let vertex = compiler
            .compile(&ApplicationShaderCompileOptions::new(
                &shader_path,
                ApplicationShaderStage::Vertex,
                "VSMain",
            ))
            .map_err(ApplicationDiagnosticError::msg)
            .context("compiling the game's 3D mesh vertex shader")?;
        let pixel = compiler
            .compile(&ApplicationShaderCompileOptions::new(
                &shader_path,
                ApplicationShaderStage::Pixel,
                "PSMain",
            ))
            .map_err(ApplicationDiagnosticError::msg)
            .context("compiling the game's 3D mesh pixel shader")?;
        Ok((vertex, pixel))
    }

    fn create_mesh_pipeline(
        renderer: &mut ApplicationRenderer,
        vertex: &ApplicationCompiledShader,
        pixel: &ApplicationCompiledShader,
    ) -> nord_app::ApplicationResult<ApplicationGraphicsPipelineDesc> {
        let mut pipeline = ApplicationGraphicsPipelineDesc::new(
            vertex.shader_id,
            pixel.shader_id,
            ApplicationTextureFormat::Rgba8Unorm,
        );
        pipeline.depth_format = Some(ApplicationTextureFormat::D32Float);
        pipeline.depth_test = true;
        pipeline.depth_write = true;
        pipeline = pipeline
            .with_vertex_attribute(ApplicationVertexAttribute {
                semantic: "POSITION".into(),
                semantic_index: 0,
                format: ApplicationVertexFormat::Float3,
                input_slot: 0,
                byte_offset: 0,
                per_instance: false,
                instance_step_rate: 0,
            })
            .with_vertex_attribute(ApplicationVertexAttribute {
                semantic: "NORMAL".into(),
                semantic_index: 0,
                format: ApplicationVertexFormat::Float3,
                input_slot: 0,
                byte_offset: 12,
                per_instance: false,
                instance_step_rate: 0,
            });
        renderer
            .create_graphics_pipeline(&pipeline, vertex, pixel, 1)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the game's depth-tested mesh pipeline")?;
        Ok(pipeline)
    }

    fn upload_test_mesh(
        renderer: &mut ApplicationRenderer,
        mesh: &IndexedMesh,
    ) -> nord_app::ApplicationResult<MeshGpuResources> {
        let (vertex_bytes, index_bytes, index_count) = encode_mesh(mesh)?;
        let vertex_buffer_size = u32::try_from(vertex_bytes.len())
            .map_err(ApplicationDiagnosticError::msg)
            .context("checking the game's mesh vertex-buffer size")?;
        let index_buffer_size = u32::try_from(index_bytes.len())
            .map_err(ApplicationDiagnosticError::msg)
            .context("checking the game's mesh index-buffer size")?;
        let vertex_buffer = renderer
            .create_buffer(ApplicationBufferDesc::new(
                u64::from(vertex_buffer_size),
                ApplicationBufferUsage::COPY_DESTINATION.union(ApplicationBufferUsage::VERTEX),
            )?)
            .map_err(ApplicationDiagnosticError::msg)
            .context("allocating the game's indexed cube vertex buffer")?;
        renderer
            .upload_buffer(vertex_buffer, 0, &vertex_bytes)
            .map_err(ApplicationDiagnosticError::msg)
            .context("uploading the game's cube vertices")?;
        let index_buffer = renderer
            .create_buffer(ApplicationBufferDesc::new(
                u64::from(index_buffer_size),
                ApplicationBufferUsage::COPY_DESTINATION.union(ApplicationBufferUsage::INDEX),
            )?)
            .map_err(ApplicationDiagnosticError::msg)
            .context("allocating the game's indexed cube index buffer")?;
        renderer
            .upload_buffer(index_buffer, 0, &index_bytes)
            .map_err(ApplicationDiagnosticError::msg)
            .context("uploading the game's cube indices")?;
        Ok(MeshGpuResources {
            vertex_buffer,
            vertex_buffer_size,
            index_buffer,
            index_buffer_size,
            index_count,
            vertex_count: mesh.vertices().len(),
        })
    }

    fn create_camera_buffer(
        renderer: &mut ApplicationRenderer,
    ) -> nord_app::ApplicationResult<(
        nord_app::ApplicationGpuResourceHandle,
        ApplicationPipelineDescriptorBinding,
    )> {
        let buffer = renderer
            .create_buffer(
                ApplicationBufferDesc::new(
                    CONSTANT_BUFFER_BYTES as u64,
                    ApplicationBufferUsage::CONSTANT,
                )?
                .with_memory(ApplicationBufferMemory::Upload),
            )
            .map_err(ApplicationDiagnosticError::msg)
            .context("allocating the game's per-frame camera constants")?;
        let binding = ApplicationPipelineDescriptorBinding {
            kind: ApplicationShaderBindingKind::ConstantBuffer,
            register: 0,
            space: 0,
            descriptor: renderer
                .create_constant_buffer_view(buffer)
                .map_err(ApplicationDiagnosticError::msg)
                .context("creating the game's camera constant-buffer binding")?,
        };
        Ok((buffer, binding))
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
                Some(ApplicationTextureFormat::D32Float),
            )?)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the game's color and depth render targets")?;
        let color = target.color.expect("color target was requested");
        let depth = target.depth.expect("depth target was requested");
        let color_view = renderer
            .create_render_target_view(color)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the game's color attachment view")?;
        let depth_view = renderer
            .create_depth_stencil_view(depth)
            .map_err(ApplicationDiagnosticError::msg)
            .context("creating the game's depth attachment view")?;
        Ok(GameSurface { color, color_view, depth, depth_view, width, height })
    }

    pub(super) fn render_frame(&mut self) -> nord_app::ApplicationResult<()> {
        let renderer = self
            .renderer
            .as_mut()
            .ok_or_else(|| ApplicationDiagnosticError::msg("renderer has already shut down"))?;
        let surface = self.surface;
        let vertex_buffer = self.vertex_buffer;
        let vertex_buffer_size = self.vertex_buffer_size;
        let index_buffer = self.index_buffer;
        let index_buffer_size = self.index_buffer_size;
        let camera_buffer = self.camera_buffer;
        let camera_binding = self.camera_binding;
        let index_count = self.index_count;
        let pipeline = self.pipeline.clone();
        let vertex_shader = self.vertex_shader.clone();
        let pixel_shader = self.pixel_shader.clone();
        let view_projection = self
            .camera
            .camera()
            .view_projection_matrix()
            .map_err(ApplicationDiagnosticError::msg)
            .context("building the game's view-projection matrix")?;
        let constants = encode_camera_constants(self.model, view_projection);
        renderer
            .write_buffer(camera_buffer, 0, &constants)
            .map_err(ApplicationDiagnosticError::msg)
            .context("updating the game's per-frame camera and model matrices")?;

        let viewport_width = exact_surface_extent(surface.width)?;
        let viewport_height = exact_surface_extent(surface.height)?;
        let mut queue = ApplicationRenderQueue::new();
        queue
            .enqueue(
                nord_app::ApplicationRenderPassDesc::new("Nord indexed 3D mesh")
                    .use_resource(surface.color, ApplicationResourceUsage::ColorAttachmentWrite)
                    .use_resource(surface.depth, ApplicationResourceUsage::DepthAttachmentWrite)
                    .use_resource(vertex_buffer, ApplicationResourceUsage::VertexBufferRead)
                    .use_resource(index_buffer, ApplicationResourceUsage::IndexBufferRead)
                    .use_resource(camera_buffer, ApplicationResourceUsage::ConstantBufferRead),
                move |encoder| {
                    encoder.begin_graphics_pass_with_attachments(
                        &[ApplicationColorAttachmentBinding {
                            resource: surface.color,
                            view: surface.color_view,
                            clear_color: CLEAR_COLOR,
                        }],
                        Some((surface.depth, surface.depth_view)),
                        1.0,
                        Some(ApplicationViewport::new(0.0, 0.0, viewport_width, viewport_height)),
                        None,
                    )?;
                    encoder.draw(
                        &pipeline,
                        &vertex_shader,
                        &pixel_shader,
                        &[camera_binding],
                        &[ApplicationVertexBufferBinding {
                            slot: 0,
                            buffer: vertex_buffer,
                            offset: 0,
                            size: vertex_buffer_size,
                            stride: VERTEX_STRIDE,
                        }],
                        Some(nord_app::ApplicationIndexBufferBinding {
                            buffer: index_buffer,
                            offset: 0,
                            size: index_buffer_size,
                            format: nord_app::ApplicationIndexFormat::Uint16,
                        }),
                        nord_app::ApplicationDrawCall::Indexed {
                            index_count,
                            instance_count: 1,
                            first_index: 0,
                            base_vertex: 0,
                            first_instance: 0,
                        },
                    )?;
                    encoder.end_graphics_pass()?;
                    Ok(())
                },
            )
            .map_err(|error| ApplicationDiagnosticError::msg(format!("{error:?}")))
            .context("encoding the game's indexed 3D mesh pass")?;
        renderer
            .submit_render_queue(&mut queue)
            .map_err(|error| ApplicationDiagnosticError::msg(format!("{error:?}")))
            .context("submitting the game's 3D mesh render graph")?;
        renderer
            .present_texture(surface.color)
            .map_err(ApplicationDiagnosticError::msg)
            .context("presenting the game's 3D mesh")
    }

    pub(super) fn wait_for_frame(&self) -> nord_app::ApplicationResult<()> {
        self.renderer
            .as_ref()
            .ok_or_else(|| ApplicationDiagnosticError::msg("renderer has already shut down"))?
            .wait_for_frame()
            .map_err(ApplicationDiagnosticError::msg)
            .context("waiting for graphics frame readiness")
    }

    pub(super) fn handle_input(
        &mut self,
        event: &nord_app::ApplicationInputEvent,
    ) -> nord_app::ApplicationResult<()> {
        self.camera.handle_input(event)
    }

    pub(super) fn reset_pointer_tracking(&mut self) {
        self.camera.reset_pointer_tracking();
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
        let aspect_ratio = surface_aspect(width, height)?;
        renderer
            .destroy_resource(self.surface.color)
            .map_err(ApplicationDiagnosticError::msg)
            .context("retiring the previous game color target")?;
        renderer
            .destroy_resource(self.surface.depth)
            .map_err(ApplicationDiagnosticError::msg)
            .context("retiring the previous game depth target")?;
        renderer
            .resize(width, height)
            .map_err(ApplicationDiagnosticError::msg)
            .context("resizing the Nord render surface")?;
        self.surface = Self::create_surface(renderer, width, height)?;
        self.camera
            .camera_mut()
            .set_aspect_ratio(aspect_ratio)
            .map_err(ApplicationDiagnosticError::msg)
            .context("updating the game's camera projection after resize")
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
        for resource in [
            self.surface.color,
            self.surface.depth,
            self.vertex_buffer,
            self.index_buffer,
            self.camera_buffer,
        ] {
            renderer
                .destroy_resource(resource)
                .map_err(ApplicationDiagnosticError::msg)
                .context("destroying a game-owned 3D render resource")?;
        }
        Ok(())
    }
}

fn create_game_camera(aspect_ratio: f32) -> nord_app::ApplicationResult<PerspectiveCamera> {
    let mut camera = PerspectiveCamera::new(60.0_f32.to_radians(), aspect_ratio, 0.1, 100.0)
        .map_err(ApplicationDiagnosticError::msg)
        .context("creating the game's perspective camera")?;
    camera
        .look_at(Vec3::new(3.2, 2.4, -5.5), Vec3::ZERO, Vec3::Y)
        .map_err(ApplicationDiagnosticError::msg)
        .context("orienting the game's perspective camera")?;
    Ok(camera)
}

fn test_cube_transform() -> nord_app::ApplicationResult<Mat4> {
    let rotation = Quat::from_axis_angle(Vec3::Y, 0.65)
        .zip(Quat::from_axis_angle(Vec3::X, -0.35))
        .map(|(yaw, pitch)| yaw * pitch)
        .ok_or_else(|| ApplicationDiagnosticError::msg("test cube rotation is invalid"))?;
    Mat4::from_translation_rotation_scale(Vec3::ZERO, rotation, Vec3::splat(0.9))
        .ok_or_else(|| ApplicationDiagnosticError::msg("test cube transform is invalid"))
}

fn make_test_cube() -> nord_app::ApplicationResult<IndexedMesh> {
    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    let faces = [
        (
            Vec3::Z,
            [
                Vec3::new(-1.0, -1.0, 1.0),
                Vec3::new(1.0, -1.0, 1.0),
                Vec3::new(1.0, 1.0, 1.0),
                Vec3::new(-1.0, 1.0, 1.0),
            ],
        ),
        (
            -Vec3::Z,
            [
                Vec3::new(-1.0, 1.0, -1.0),
                Vec3::new(1.0, 1.0, -1.0),
                Vec3::new(1.0, -1.0, -1.0),
                Vec3::new(-1.0, -1.0, -1.0),
            ],
        ),
        (
            Vec3::X,
            [
                Vec3::new(1.0, -1.0, -1.0),
                Vec3::new(1.0, 1.0, -1.0),
                Vec3::new(1.0, 1.0, 1.0),
                Vec3::new(1.0, -1.0, 1.0),
            ],
        ),
        (
            -Vec3::X,
            [
                Vec3::new(-1.0, -1.0, 1.0),
                Vec3::new(-1.0, 1.0, 1.0),
                Vec3::new(-1.0, 1.0, -1.0),
                Vec3::new(-1.0, -1.0, -1.0),
            ],
        ),
        (
            Vec3::Y,
            [
                Vec3::new(-1.0, 1.0, 1.0),
                Vec3::new(1.0, 1.0, 1.0),
                Vec3::new(1.0, 1.0, -1.0),
                Vec3::new(-1.0, 1.0, -1.0),
            ],
        ),
        (
            -Vec3::Y,
            [
                Vec3::new(-1.0, -1.0, -1.0),
                Vec3::new(1.0, -1.0, -1.0),
                Vec3::new(1.0, -1.0, 1.0),
                Vec3::new(-1.0, -1.0, 1.0),
            ],
        ),
    ];
    for (normal, corners) in faces {
        let first = u32::try_from(vertices.len())
            .map_err(|_| ApplicationDiagnosticError::msg("cube vertex index overflowed"))?;
        for position in corners {
            vertices.push(
                MeshVertex::new(position, normal)
                    .map_err(ApplicationDiagnosticError::msg)
                    .context("validating the game's cube vertex")?,
            );
        }
        indices.extend_from_slice(&[first, first + 1, first + 2, first, first + 2, first + 3]);
    }
    IndexedMesh::new(vertices, indices)
        .map_err(ApplicationDiagnosticError::msg)
        .context("validating the game's indexed cube mesh")
}

fn encode_mesh(mesh: &IndexedMesh) -> nord_app::ApplicationResult<(Vec<u8>, Vec<u8>, u32)> {
    let mut vertices = Vec::with_capacity(mesh.vertices().len() * VERTEX_STRIDE as usize);
    for vertex in mesh.vertices() {
        for value in vertex.position().to_array().into_iter().chain(vertex.normal().to_array()) {
            vertices.extend_from_slice(&value.to_le_bytes());
        }
    }
    let mut indices = Vec::with_capacity(mesh.indices().len() * 2);
    for &index in mesh.indices() {
        let index = u16::try_from(index)
            .map_err(|_| ApplicationDiagnosticError::msg("test mesh exceeds 16-bit indices"))?;
        indices.extend_from_slice(&index.to_le_bytes());
    }
    let index_count = u32::try_from(mesh.indices().len())
        .map_err(|_| ApplicationDiagnosticError::msg("test mesh has too many indices"))?;
    Ok((vertices, indices, index_count))
}

fn encode_camera_constants(model: Mat4, view_projection: Mat4) -> [u8; CONSTANT_BUFFER_BYTES] {
    let mut bytes = [0_u8; CONSTANT_BUFFER_BYTES];
    let mut offset = 0;
    for matrix in [model, view_projection] {
        for column in matrix.to_cols() {
            for value in column.to_array() {
                bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
                offset += 4;
            }
        }
    }
    bytes
}

fn surface_aspect(width: u32, height: u32) -> nord_app::ApplicationResult<f32> {
    if width == 0 || height == 0 {
        return Err(ApplicationDiagnosticError::msg(
            "perspective camera requires a non-zero render surface",
        ));
    }
    let width = exact_surface_extent(width)?;
    let height = exact_surface_extent(height)?;
    Ok(width / height)
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
